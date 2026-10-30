//! Receiver side: connections to senders, browsing, and the resumable
//! download engine.
//!
//! Each file is split into fixed-size chunks. Several chunks are fetched in
//! parallel on independent QUIC streams to fill long, high-latency links.
//! Each chunk arrives in 1 MiB blocks, each followed by the sender's BLAKE3
//! hash of that block. A block is checked, written to its offset in a
//! `.rlpart` file and reported to a committer, which flushes the file to
//! disk and only then records the progress in SQLite (whole chunks, and the
//! verified prefix of unfinished chunks). After a pause, crash, power loss
//! or reboot, every chunk resumes from its last saved block.
//! When every chunk is present the whole file is re-read and re-verified
//! before it is renamed to its final name.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::endpoint::{Connection, VarInt};
use iroh::{EndpointAddr, EndpointId, RelayUrl};
use parking_lot::Mutex;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::db::{HistoryRow, JobRow, JobStats};
use crate::diag;
use crate::inner::Inner;
use crate::net::route_of;
use crate::proto::*;
use crate::types::{CoreEvent, JobSnapshot, JobStatus, PeerStatus, RouteInfo};
use crate::util::{
    available_space, backoff, human_bytes, now_ms, read_exact_at, short_peer, unique_path,
    write_all_at,
};

const BLOCK: usize = crate::proto::BLOCK as usize;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// A chunk stream is only considered stalled when no bytes at all arrive for
/// this long. Slow links (a busy mobile uplink, a relay) can take minutes per
/// 1 MiB block when it is shared by many streams; that is slow, not stuck.
const STALL_TIMEOUT: Duration = Duration::from_secs(90);
/// How long to wait for a direct path before using the encrypted relay
/// (relay fallback mode).
const DIRECT_GRACE: Duration = Duration::from_secs(15);
const COMMIT_INTERVAL: Duration = Duration::from_secs(2);

// ====================================================================== errors

#[derive(Debug, Clone)]
pub enum JobError {
    /// Transient: connection lost, timeouts. Retry with backoff.
    Network(String),
    /// Sender busy/locked file; retry later.
    Busy(String),
    SourceChanged,
    Missing(String),
    Denied(String),
    RelayBlocked,
    Disk(String),
    Fatal(String),
    Stopped,
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::Network(m) | JobError::Busy(m) | JobError::Missing(m) | JobError::Denied(m)
            | JobError::Disk(m) | JobError::Fatal(m) => write!(f, "{m}"),
            JobError::SourceChanged => write!(f, "The file changed on the sender's computer."),
            JobError::RelayBlocked => write!(f, "Waiting for a direct connection."),
            JobError::Stopped => write!(f, "Stopped"),
        }
    }
}

impl JobError {
    fn severity(&self) -> u8 {
        match self {
            JobError::Stopped => 0,
            JobError::Network(_) => 1,
            JobError::Busy(_) => 2,
            JobError::RelayBlocked => 3,
            _ => 9,
        }
    }
}

fn net<E: std::fmt::Display>(e: E) -> JobError {
    JobError::Network(format!("{e}"))
}

fn remote_err(code: ErrCode, message: String) -> JobError {
    match code {
        ErrCode::Denied => JobError::Denied(message),
        ErrCode::NotFound => JobError::Missing("The file is no longer available on the sender's computer.".into()),
        ErrCode::SourceChanged => JobError::SourceChanged,
        ErrCode::RelayBlocked => JobError::RelayBlocked,
        ErrCode::InUse => JobError::Busy(message),
        ErrCode::Io => JobError::Busy(message),
        ErrCode::BadRequest => JobError::Fatal(message),
    }
}

fn disk_err(e: io::Error, what: &str) -> JobError {
    let raw = e.raw_os_error();
    // ERROR_DISK_FULL (112), ERROR_HANDLE_DISK_FULL (39), ENOSPC (28)
    if matches!(raw, Some(112) | Some(39)) || (cfg!(unix) && raw == Some(28)) || e.kind() == io::ErrorKind::StorageFull {
        return JobError::Disk("The destination disk is full. Free up space and resume.".into());
    }
    if e.kind() == io::ErrorKind::PermissionDenied {
        return JobError::Disk(format!("Windows denied access while {what}. Choose another folder or check permissions."));
    }
    if e.kind() == io::ErrorKind::NotFound {
        return JobError::Disk(format!("The destination folder is missing ({what}). Reconnect the drive and resume."));
    }
    JobError::Disk(format!("Disk error while {what}: {e}"))
}

// ====================================================================== peers

pub struct PeerConn {
    conn: tokio::sync::Mutex<Option<Connection>>,
    pub route: watch::Sender<RouteInfo>,
    pub status: Mutex<PeerStatus>,
    /// Chunk streams open to this sender across all jobs (diagnostics).
    pub streams: diag::Gauge,
}

impl PeerConn {
    fn new(id: EndpointId) -> Self {
        PeerConn {
            conn: tokio::sync::Mutex::new(None),
            streams: diag::Gauge::new(),
            route: watch::channel(RouteInfo::none()).0,
            status: Mutex::new(PeerStatus {
                peer_id: id.to_string(),
                state: "disconnected".into(),
                route: RouteInfo::none(),
                message: None,
            }),
        }
    }

}

pub fn peer_conn(inner: &Inner, id: EndpointId) -> Arc<PeerConn> {
    inner.peers.lock().entry(id).or_insert_with(|| Arc::new(PeerConn::new(id))).clone()
}

fn set_peer_state(inner: &Inner, pc: &PeerConn, state: &str, message: Option<String>) {
    let st = {
        let mut s = pc.status.lock();
        s.state = state.into();
        s.message = message;
        s.route = pc.route.borrow().clone();
        s.clone()
    };
    inner.emit(CoreEvent::PeerStatus(st));
}

pub fn peer_addr(inner: &Inner, peer: EndpointId) -> EndpointAddr {
    if let Some(a) = inner.addr_hints.lock().get(&peer) {
        return a.clone();
    }
    let mut addr = EndpointAddr::new(peer);
    if let Ok(Some(r)) = inner.db.get_remote(&peer.to_string()) {
        if let Some(url) = r.relay_url.and_then(|u| u.parse::<RelayUrl>().ok()) {
            addr = addr.with_relay_url(url);
        }
    }
    addr
}

/// Installs a fresh connection: watches its route and marks the peer connected.
fn install_conn(inner: &Arc<Inner>, pc: &Arc<PeerConn>, conn: &Connection) {
    pc.route.send_replace(route_of(conn));
    set_peer_state(inner, pc, "connected", None);
    let inner = inner.clone();
    let pc = pc.clone();
    let conn = conn.clone();
    tokio::spawn(async move {
        use n0_future::StreamExt;
        let mut events = conn.path_events();
        loop {
            tokio::select! {
                ev = events.next() => {
                    if ev.is_none() { break; }
                    let r = route_of(&conn);
                    if *pc.route.borrow() != r {
                        pc.route.send_replace(r);
                        set_peer_state(&inner, &pc, "connected", None);
                    }
                }
                _ = conn.closed() => break,
            }
        }
        pc.route.send_replace(RouteInfo::none());
        set_peer_state(&inner, &pc, "disconnected", None);
    });
}

async fn dial(inner: &Arc<Inner>, peer: EndpointId) -> Result<Connection, JobError> {
    let addr = peer_addr(inner, peer);
    match tokio::time::timeout(CONNECT_TIMEOUT, inner.endpoint.connect(addr, ALPN)).await {
        Ok(Ok(c)) => Ok(c),
        Ok(Err(e)) => Err(JobError::Network(format!(
            "Can't reach the sender right now. Relayless must be open on their computer. ({e})"
        ))),
        Err(_) => Err(JobError::Network(
            "Timed out reaching the sender. They may be offline, or a firewall is blocking the connection.".into(),
        )),
    }
}

/// Returns a live, authorized connection to the sender, dialing if needed.
pub async fn get_conn(inner: &Arc<Inner>, peer: EndpointId) -> Result<Connection, JobError> {
    get_conn_traced(inner, peer, None).await
}

/// `get_conn`, recording reuse, lock wait, dial and Hello timings into `d`.
async fn get_conn_traced(inner: &Arc<Inner>, peer: EndpointId, d: Option<&diag::TransferDiag>) -> Result<Connection, JobError> {
    let start = Instant::now();
    let at_ms = d.map(|d| d.at_ms()).unwrap_or(0.0);
    let mut attempt = diag::ConnAttempt {
        at_ms,
        reused: false,
        lock_wait_ms: 0.0,
        dial_ms: None,
        hello_ms: None,
        total_ms: 0.0,
        ok: false,
        error: None,
    };
    let finish = |mut a: diag::ConnAttempt, ok: bool, err: Option<diag::ErrCat>| {
        if let Some(d) = d {
            a.total_ms = diag::ms(start.elapsed());
            a.ok = ok;
            a.error = err;
            d.stages.conn_setup.record(start.elapsed());
            if let Some(e) = err {
                d.error(e);
            }
            d.conn_attempt(a);
        }
    };
    let pc = peer_conn(inner, peer);
    let mut guard = pc.conn.lock().await;
    attempt.lock_wait_ms = diag::ms(start.elapsed());
    if let Some(c) = guard.as_ref() {
        if c.close_reason().is_none() {
            attempt.reused = true;
            finish(attempt, true, None);
            return Ok(c.clone());
        }
    }
    set_peer_state(inner, &pc, "connecting", None);
    let t = Instant::now();
    let dialed = dial(inner, peer).await;
    attempt.dial_ms = Some(diag::ms(t.elapsed()));
    let conn = match dialed {
        Ok(c) => c,
        Err(e) => {
            set_peer_state(inner, &pc, "unreachable", Some(e.to_string()));
            finish(attempt, false, Some(diag::ErrCat::ConnectFailed));
            return Err(e);
        }
    };
    let device = inner.settings.read().device_name.clone();
    let t = Instant::now();
    let hello_res = hello(&conn, None, &device, |_| {}).await;
    attempt.hello_ms = Some(diag::ms(t.elapsed()));
    let hello = match hello_res {
        Ok(h) => h,
        Err(e) => {
            set_peer_state(inner, &pc, "unreachable", Some(e.to_string()));
            finish(attempt, false, Some(diag::ErrCat::ConnectFailed));
            return Err(e);
        }
    };
    if hello.status != HelloStatus::Approved {
        let msg = hello.message.unwrap_or_else(|| "The sender has removed this device's access.".into());
        set_peer_state(inner, &pc, "denied", Some(msg.clone()));
        conn.close(VarInt::from_u32(0), b"denied");
        finish(attempt, false, Some(diag::ErrCat::Denied));
        return Err(JobError::Denied(msg));
    }
    finish(attempt, true, None);
    let peer_s = peer.to_string();
    let _ = inner.db.upsert_remote(&peer_s, &hello.sender_name, None);
    let _ = inner.db.set_remote_shares(
        &peer_s,
        &hello.shares.iter().map(|s| (s.id.clone(), s.name.clone())).collect::<Vec<_>>(),
    );
    inner.emit(CoreEvent::RemotesChanged);
    install_conn(inner, &pc, &conn);
    *guard = Some(conn.clone());
    Ok(conn)
}

pub fn adopt_conn(inner: &Arc<Inner>, peer: EndpointId, conn: Connection) {
    let pc = peer_conn(inner, peer);
    if let Ok(mut g) = pc.conn.try_lock() {
        *g = Some(conn.clone());
    }
    install_conn(inner, &pc, &conn);
}

pub async fn drop_conn(inner: &Arc<Inner>, peer: EndpointId) {
    let pc = peer_conn(inner, peer);
    let taken = pc.conn.lock().await.take();
    if let Some(c) = taken {
        c.close(VarInt::from_u32(0), b"reset");
    }
}

/// Sends a Hello and waits for a final answer (approval may take a while).
pub async fn hello(
    conn: &Connection,
    token: Option<String>,
    device_name: &str,
    mut on_pending: impl FnMut(&HelloResp),
) -> Result<HelloResp, JobError> {
    let (mut send, mut recv) = conn.open_bi().await.map_err(net)?;
    write_frame(&mut send, &Request::Hello { v: PROTOCOL_VERSION, token, device_name: device_name.to_string() })
        .await
        .map_err(net)?;
    send.finish().map_err(net)?;
    loop {
        let resp: Response = tokio::time::timeout(Duration::from_secs(20 * 60), read_frame(&mut recv))
            .await
            .map_err(|_| JobError::Network("No answer from the sender.".into()))?
            .map_err(net)?;
        match resp {
            Response::Hello(h) if h.status == HelloStatus::Pending => on_pending(&h),
            Response::Hello(h) => return Ok(h),
            Response::Error { code, message } => return Err(remote_err(code, message)),
            _ => return Err(JobError::Network("Unexpected reply from sender.".into())),
        }
    }
}

/// One request, one JSON response.
pub async fn request(conn: &Connection, req: &Request) -> Result<Response, JobError> {
    let (mut send, mut recv) = conn.open_bi().await.map_err(net)?;
    write_frame(&mut send, req).await.map_err(net)?;
    send.finish().map_err(net)?;
    let resp: Response = tokio::time::timeout(READ_TIMEOUT, read_frame(&mut recv))
        .await
        .map_err(|_| JobError::Network("The sender stopped responding.".into()))?
        .map_err(net)?;
    if let Response::Error { code, message } = resp {
        return Err(remote_err(code, message));
    }
    Ok(resp)
}

// ====================================================================== jobs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Pause,
    Cancel,
    Shutdown,
}

pub struct SpeedState {
    last_bytes: u64,
    last_at: Instant,
    ema: f64,
    samples: u32,
}

pub struct JobHandle {
    pub id: String,
    pub cancel: CancellationToken,
    pub stop: Mutex<Option<StopReason>>,
    pub row: Mutex<JobRow>,
    pub peer_name: String,
    pub verified: AtomicU64,
    pub inflight: AtomicU64,
    /// Bytes received for blocks that are still arriving (not yet verified).
    pub arriving: AtomicU64,
    pub verified_chunks: AtomicU64,
    speed: Mutex<SpeedState>,
    /// Bytes received over a direct path and through a relay.
    pub direct_bytes: AtomicU64,
    pub relay_bytes: AtomicU64,
    /// Time spent actually downloading, excluding the current stretch.
    active_ms: AtomicU64,
    active_since: Mutex<Option<Instant>>,
    peak_bps: Mutex<f64>,
    /// Diagnostics for this run of the job (spawn to exit).
    pub diag: Arc<diag::TransferDiag>,
}

impl JobHandle {
    pub fn done_bytes(&self) -> u64 {
        self.verified.load(Ordering::Relaxed) + self.inflight.load(Ordering::Relaxed) + self.arriving.load(Ordering::Relaxed)
    }

    /// Updates the smoothed speed estimate; returns (bytes/s, eta).
    pub fn sample_speed(&self) -> (f64, Option<u64>) {
        let done = self.done_bytes();
        let size = self.row.lock().size;
        let mut s = self.speed.lock();
        let dt = s.last_at.elapsed().as_secs_f64();
        if dt >= 0.4 {
            let delta = done.saturating_sub(s.last_bytes) as f64;
            let inst = delta / dt;
            s.ema = if s.samples == 0 { inst } else { s.ema * 0.75 + inst * 0.25 };
            s.samples += 1;
            s.last_bytes = done;
            s.last_at = Instant::now();
            if s.samples >= 3 {
                let mut p = self.peak_bps.lock();
                if s.ema > *p {
                    *p = s.ema;
                }
            }
        }
        let eta = if s.samples >= 6 && s.ema > 4096.0 {
            Some((size.saturating_sub(done) as f64 / s.ema) as u64)
        } else {
            None
        };
        (s.ema, eta)
    }

    fn reset_speed(&self) {
        let mut s = self.speed.lock();
        s.last_bytes = self.done_bytes();
        s.last_at = Instant::now();
        s.ema = 0.0;
        s.samples = 0;
    }

    pub fn status(&self) -> JobStatus {
        self.row.lock().status
    }

    fn begin_active(&self) {
        let mut a = self.active_since.lock();
        if a.is_none() {
            *a = Some(Instant::now());
        }
    }

    fn end_active(&self) {
        if let Some(t) = self.active_since.lock().take() {
            self.active_ms.fetch_add(t.elapsed().as_millis() as u64, Ordering::Relaxed);
        }
    }

    pub fn stats(&self) -> JobStats {
        let running = self.active_since.lock().map(|t| t.elapsed().as_millis() as u64).unwrap_or(0);
        JobStats {
            direct_bytes: self.direct_bytes.load(Ordering::Relaxed),
            relay_bytes: self.relay_bytes.load(Ordering::Relaxed),
            active_ms: self.active_ms.load(Ordering::Relaxed) + running,
            peak_bps: *self.peak_bps.lock(),
        }
    }
}

pub fn snapshot(inner: &Inner, row: &JobRow, h: Option<&JobHandle>, peer_name: Option<&str>) -> JobSnapshot {
    let (verified, done, speed, eta, chunks) = match h {
        Some(h) => {
            let (speed, eta) = if h.status() == JobStatus::Downloading { h.sample_speed() } else { (0.0, None) };
            (
                h.verified.load(Ordering::Relaxed),
                h.done_bytes(),
                speed,
                eta,
                h.verified_chunks.load(Ordering::Relaxed),
            )
        }
        None => {
            let chunks = if row.status == JobStatus::Completed { row.total_chunks } else { 0 };
            (row.verified_bytes, row.verified_bytes, 0.0, None, chunks)
        }
    };
    let peer_name = peer_name.map(|s| s.to_string()).unwrap_or_else(|| {
        inner
            .db
            .get_remote(&row.peer_id)
            .ok()
            .flatten()
            .map(|r| r.name)
            .unwrap_or_else(|| short_peer(&row.peer_id))
    });
    let route = row
        .peer_id
        .parse::<EndpointId>()
        .ok()
        .and_then(|id| inner.peers.lock().get(&id).map(|p| p.route.borrow().kind.clone()))
        .unwrap_or_else(|| "none".into());
    JobSnapshot {
        id: row.id.clone(),
        peer_id: row.peer_id.clone(),
        peer_name,
        share_id: row.share_id.clone(),
        remote_path: row.remote_path.clone(),
        display_name: row.display_name.clone(),
        dest_path: row.dest_path.clone(),
        size: row.size,
        verified_bytes: verified,
        done_bytes: done.min(row.size),
        status: row.status,
        error: row.error.clone(),
        speed_bps: speed,
        eta_secs: eta,
        total_chunks: row.total_chunks,
        verified_chunks: chunks,
        created_at: row.created_at,
        updated_at: row.updated_at,
        completed_at: row.completed_at,
        route,
    }
}

pub fn emit_job(inner: &Inner, h: &JobHandle) {
    let row = h.row.lock().clone();
    inner.emit(CoreEvent::JobUpdated(snapshot(inner, &row, Some(h), Some(&h.peer_name))));
}

fn set_status(inner: &Inner, h: &JobHandle, status: JobStatus, error: Option<String>) {
    if h.cancel.is_cancelled() {
        return;
    }
    {
        let mut r = h.row.lock();
        if r.status == status && r.error == error {
            return;
        }
        r.status = status;
        r.error = error.clone();
        r.updated_at = now_ms();
    }
    h.diag.on_status(status.as_str());
    if status == JobStatus::Downloading {
        h.reset_speed();
    }
    let _ = inner.db.set_job_status(&h.id, status, error.as_deref());
    emit_job(inner, h);
}

/// Starts the job runner if it isn't already running.
pub fn spawn_job(inner: &Arc<Inner>, job_id: &str) {
    let Ok(Some(row)) = inner.db.get_job(job_id) else { return };
    let mut jobs = inner.jobs.lock();
    if jobs.contains_key(job_id) {
        return;
    }
    let peer_name = inner
        .db
        .get_remote(&row.peer_id)
        .ok()
        .flatten()
        .map(|r| r.name)
        .unwrap_or_else(|| short_peer(&row.peer_id));
    let verified_chunks = inner.db.verified_chunks(job_id).map(|v| v.len() as u64).unwrap_or(0);
    let diag = Arc::new(diag::TransferDiag::new(
        row.id.clone(),
        "receiver",
        diag::file_key(&row.share_id, &row.remote_path),
        diag::file_ext(&row.remote_path),
        short_peer(&row.peer_id),
        row.size,
    ));
    diag.on_status(row.status.as_str());
    let h = Arc::new(JobHandle {
        diag,
        id: row.id.clone(),
        cancel: inner.shutdown.child_token(),
        stop: Mutex::new(None),
        verified: AtomicU64::new(row.verified_bytes),
        inflight: AtomicU64::new(0),
        arriving: AtomicU64::new(0),
        verified_chunks: AtomicU64::new(verified_chunks),
        speed: Mutex::new(SpeedState { last_bytes: row.verified_bytes, last_at: Instant::now(), ema: 0.0, samples: 0 }),
        direct_bytes: AtomicU64::new(row.stats.direct_bytes),
        relay_bytes: AtomicU64::new(row.stats.relay_bytes),
        active_ms: AtomicU64::new(row.stats.active_ms),
        active_since: Mutex::new(None),
        peak_bps: Mutex::new(row.stats.peak_bps),
        row: Mutex::new(row),
        peer_name,
    });
    jobs.insert(job_id.to_string(), h.clone());
    drop(jobs);
    let inner = inner.clone();
    tokio::spawn(async move {
        let sampler = if inner.diag.enabled { Some(tokio::spawn(sample_loop(inner.clone(), h.clone()))) } else { None };
        let resumed_from = h.verified.load(Ordering::Relaxed);
        run_job(&inner, &h).await;
        if let Some(s) = sampler {
            s.abort();
        }
        finish_diag(&inner, &h, resumed_from);
        inner.jobs.lock().remove(&h.id);
        if let Ok(Some(row)) = inner.db.get_job(&h.id) {
            inner.emit(CoreEvent::JobUpdated(snapshot(&inner, &row, None, Some(&h.peer_name))));
        }
    });
}

/// Interval samples and route-change tracking for one job run (diagnostics on).
async fn sample_loop(inner: Arc<Inner>, h: Arc<JobHandle>) {
    let Ok(peer) = h.row.lock().peer_id.parse::<EndpointId>() else { return };
    let pc = peer_conn(&inner, peer);
    let mut rx = pc.route.subscribe();
    let d = h.diag.clone();
    let mut base = diag::SampleBase::new(&d);
    let mut lost_prev: Option<u64> = None;
    let mut tick = tokio::time::interval(diag::SAMPLE_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick.tick().await;
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            r = rx.changed() => {
                if r.is_err() {
                    return;
                }
                let ri = rx.borrow_and_update().clone();
                let to = diag::route_label(&ri.kind);
                let from = d.route_current();
                if d.observe_route(to, ri.rtt_ms) {
                    info!(
                        target: "relayless_core::diag",
                        "route_change role=receiver id={} from={} to={} at_s={:.1} rtt_ms={:?}",
                        d.id, from.unwrap_or(diag::Route::Unknown).as_str(), to.as_str(), d.at_ms() / 1000.0, ri.rtt_ms
                    );
                }
                continue;
            }
        }
        if !matches!(h.status(), JobStatus::Downloading | JobStatus::Verifying) {
            base = diag::SampleBase::new(&d);
            continue;
        }
        let mut extra = serde_json::json!({
            "status": h.status().as_str(),
            "connection_streams": pc.streams.current(),
        });
        if let Some(conn) = d.connection() {
            // Route from the path API directly, compared with the cached value used for byte counting.
            let actual = route_of(&conn);
            let cached = pc.route.borrow().kind.clone();
            let report = crate::net::path_report(&conn);
            let lost = report.pointer("/totals/lost_packets").and_then(|v| v.as_u64());
            let sel = report["paths"].as_array().and_then(|ps| ps.iter().find(|p| p["selected"] == true)).cloned();
            let cwnd = sel.as_ref().and_then(|p| p["cwnd_bytes"].as_u64());
            // This side's controller paces ACK-eliciting packets, including
            // flow-control credit (MAX_STREAM_DATA) for the sender.
            extra["pacing_rate_bytes_per_sec"] = sel.as_ref().map(|p| p["pacing_rate_bytes_per_sec"].clone()).unwrap_or_default();
            extra["paths_open"] = serde_json::json!(report["paths"].as_array().map(|p| p.len()));
            extra["route_api"] = serde_json::json!(diag::route_label(&actual.kind));
            extra["route_cache_matches"] = serde_json::json!(actual.kind == cached);
            extra["rtt_ms"] = serde_json::json!(actual.rtt_ms);
            extra["cwnd_bytes"] = serde_json::json!(cwnd);
            extra["lost_packets_delta"] = match (lost, lost_prev) {
                (Some(l), Some(p)) => serde_json::json!(l.saturating_sub(p)),
                _ => serde_json::Value::Null,
            };
            lost_prev = lost;
        } else {
            extra["route_api"] = serde_json::json!(diag::Route::Unknown);
        }
        let s = d.sample(&mut base, extra);
        info!(
            target: "relayless_core::diag",
            "transfer_interval role=receiver id={} t_s={:.1} bytes={} interval_mib_s={} route={} streams={} conn_streams={} rtt_ms={} cwnd={} pacing_bytes_s={} lost_delta={} limiter_wait_ms={} read_wait_ms={}",
            d.id,
            s["t_ms"].as_f64().unwrap_or(0.0) / 1000.0,
            s["bytes"],
            s["interval_avg_mib_per_sec"],
            s["route_api"].as_str().unwrap_or("unknown"),
            s["active_streams"],
            s["connection_streams"],
            s["rtt_ms"],
            s["cwnd_bytes"],
            s["pacing_rate_bytes_per_sec"],
            s["lost_packets_delta"],
            s["limiter_wait_ms"],
            s["read_wait_ms"],
        );
    }
}

/// Writes the job run's summary (diagnostics on) when the runner exits.
fn finish_diag(inner: &Arc<Inner>, h: &Arc<JobHandle>, resumed_from: u64) {
    let d = &h.diag;
    if let Some(conn) = d.connection() {
        let r = route_of(&conn);
        d.observe_route(diag::route_label(&r.kind), r.rtt_ms);
    }
    let status = h.status();
    let outcome = if status == JobStatus::Completed {
        "completed"
    } else {
        match *h.stop.lock() {
            Some(StopReason::Pause) => "paused",
            Some(StopReason::Cancel) => "canceled",
            Some(StopReason::Shutdown) => "shutdown",
            None => status.as_str(),
        }
    };
    if !inner.diag.enabled {
        return;
    }
    let settings = inner.settings();
    let pc = h.row.lock().peer_id.parse::<EndpointId>().ok().map(|p| peer_conn(inner, p));
    // Workers per pass actually started (the setting, capped by missing chunks).
    let passes = d.c.passes.load(Ordering::Relaxed);
    let workers = d.c.workers_started.load(Ordering::Relaxed);
    let extra = diag::SummaryExtra {
        direct_only: settings.direct_only(),
        resumed_from: Some(resumed_from),
        download_limit_bps: Some(inner.download_limiter.rate()),
        upload_limit_bps: Some(inner.upload_limiter.rate()),
        parallel_streams: if passes > 0 && workers > 0 { Some(workers / passes) } else { None },
        active_time: None,
        transfer_extra: vec![
            ("connection_streams_peak", serde_json::json!(d.c.conn_streams_peak.load(Ordering::Relaxed))),
            ("connection_streams_now", pc.map(|p| serde_json::json!(p.streams.current())).unwrap_or_default()),
            ("download_limiter_process", inner.download_limiter.stats_json()),
            // This job is still registered while its summary is written.
            ("other_jobs_running_at_end", serde_json::json!(inner.jobs.lock().len().saturating_sub(1))),
        ],
    };
    if let Some(s) = d.summary(outcome, extra) {
        if outcome == "shutdown" {
            diag::emit_summary_sync(&inner.diag, d, &s);
        } else {
            diag::emit_summary(&inner.diag, d, &s);
        }
    }
}

/// Sleeps between attempts; the time counts as `backoff_wait` in diagnostics.
async fn sleep_or_stop(h: &JobHandle, d: Duration) -> bool {
    let t = Instant::now();
    let stopped = tokio::select! {
        _ = tokio::time::sleep(d) => false,
        _ = h.cancel.cancelled() => true,
    };
    h.diag.stages.backoff_wait.record(t.elapsed());
    stopped
}

async fn run_job(inner: &Arc<Inner>, h: &Arc<JobHandle>) {
    let mut attempt: u32 = 0;
    let peer: EndpointId = match h.row.lock().peer_id.parse() {
        Ok(p) => p,
        Err(_) => {
            set_status(inner, h, JobStatus::Failed, Some("Invalid sender id.".into()));
            return;
        }
    };
    let mut announced_wait = false;
    loop {
        if h.cancel.is_cancelled() {
            return;
        }
        if !matches!(h.status(), JobStatus::Downloading | JobStatus::Verifying) {
            set_status(inner, h, JobStatus::Queued, None);
        }
        let t = Instant::now();
        let _slot = tokio::select! {
            g = inner.slots.acquire() => g,
            _ = h.cancel.cancelled() => return,
        };
        h.diag.stages.slot_wait.record(t.elapsed());
        set_status(inner, h, if attempt == 0 { JobStatus::Connecting } else { JobStatus::Reconnecting }, None);
        let conn = tokio::select! {
            c = get_conn_traced(inner, peer, Some(&h.diag)) => c,
            _ = h.cancel.cancelled() => return,
        };
        let conn = match conn {
            Ok(c) => c,
            Err(JobError::Denied(m)) => {
                inner.log("error", &format!("{}: {m}", h.row.lock().display_name), Some(&h.id));
                set_status(inner, h, JobStatus::NeedsAttention, Some(m));
                return;
            }
            Err(e) => {
                set_status(inner, h, JobStatus::WaitingPeer, Some(e.to_string()));
                if !announced_wait {
                    announced_wait = true;
                    inner.log("warning", &format!("Waiting for sender: {}", h.row.lock().display_name), Some(&h.id));
                }
                drop(_slot);
                if sleep_or_stop(h, backoff(attempt)).await {
                    return;
                }
                attempt = attempt.saturating_add(1);
                continue;
            }
        };

        // Relay fallback: give hole punching a moment to find a direct path
        // first; after that, carry on over the encrypted relay.
        if !inner.settings.read().direct_only() && attempt == 0 {
            let pc = peer_conn(inner, peer);
            let mut rx = pc.route.subscribe();
            let t = Instant::now();
            if !rx.borrow().is_direct() {
                let wait = tokio::time::timeout(DIRECT_GRACE, async {
                    loop {
                        if rx.borrow().is_direct() || rx.changed().await.is_err() {
                            return;
                        }
                    }
                });
                let outcome = tokio::select! {
                    r = wait => if r.is_err() { "timeout" } else if pc.route.borrow().is_direct() { "direct_found" } else { "route_watch_closed" },
                    _ = h.cancel.cancelled() => "cancelled",
                };
                h.diag.stages.hole_punch_wait.record(t.elapsed());
                h.diag.hole_punch(diag::HolePunch { wait_ms: diag::ms(t.elapsed()), outcome });
                if outcome == "cancelled" {
                    return;
                }
            } else {
                h.diag.hole_punch(diag::HolePunch { wait_ms: 0.0, outcome: "already_direct" });
            }
        } else if attempt == 0 {
            h.diag.hole_punch(diag::HolePunch { wait_ms: 0.0, outcome: "not_applicable_direct_only" });
        }

        // Honour direct-only mode before sending any file bytes.
        if inner.settings.read().direct_only() {
            let pc = peer_conn(inner, peer);
            let mut rx = pc.route.subscribe();
            if !rx.borrow().is_direct() {
                set_status(inner, h, JobStatus::WaitingDirect, Some(
                    "Connected through a relay. Direct-only mode is on, so no file data is sent until a direct path opens.".into(),
                ));
                let t = Instant::now();
                let waited = tokio::time::timeout(Duration::from_secs(20), async {
                    loop {
                        if rx.borrow().is_direct() {
                            return;
                        }
                        if rx.changed().await.is_err() {
                            return;
                        }
                    }
                });
                tokio::select! {
                    _ = waited => {}
                    _ = h.cancel.cancelled() => return,
                }
                h.diag.stages.direct_only_wait.record(t.elapsed());
                if !pc.route.borrow().is_direct() {
                    h.diag.error(diag::ErrCat::RelayPathActive);
                    drop(_slot);
                    if sleep_or_stop(h, Duration::from_secs(5)).await {
                        return;
                    }
                    continue;
                }
            }
        }

        let before = h.verified.load(Ordering::Relaxed);
        let res = transfer_pass(inner, h, &conn, peer).await;
        h.end_active();
        let _ = inner.db.save_job_stats(&h.id, &h.stats());
        if h.verified.load(Ordering::Relaxed) > before {
            attempt = 0;
            announced_wait = false;
        }
        match res {
            Ok(()) => {
                if h.cancel.is_cancelled() {
                    return;
                }
                set_status(inner, h, JobStatus::Verifying, None);
                match verify_and_finalize(inner, h).await {
                    Ok(true) => return,
                    Ok(false) => continue,
                    Err(JobError::Stopped) => return,
                    Err(e) => {
                        inner.log("error", &format!("{}: {e}", h.row.lock().display_name), Some(&h.id));
                        set_status(inner, h, JobStatus::NeedsAttention, Some(e.to_string()));
                        notify_attention(inner, h);
                        return;
                    }
                }
            }
            Err(JobError::Stopped) => return,
            Err(e @ JobError::Network(_)) | Err(e @ JobError::Busy(_)) => {
                if h.cancel.is_cancelled() {
                    return;
                }
                if matches!(e, JobError::Network(_)) {
                    drop_conn(inner, peer).await;
                }
                info!("job {} interrupted: {e}", h.id);
                set_status(inner, h, JobStatus::Reconnecting, Some(format!("Connection interrupted. Your progress is safe. {e}")));
                drop(_slot);
                if sleep_or_stop(h, backoff(attempt)).await {
                    return;
                }
                attempt = attempt.saturating_add(1);
            }
            Err(JobError::RelayBlocked) => {
                // Already counted per stream in fetch_chunk.
                set_status(inner, h, JobStatus::WaitingDirect, Some(
                    "The connection switched to a relay. Waiting for a direct path (direct-only mode).".into(),
                ));
                drop(_slot);
                if sleep_or_stop(h, Duration::from_secs(5)).await {
                    return;
                }
            }
            Err(e) => {
                let msg = e.to_string();
                inner.log("error", &format!("{}: {msg}", h.row.lock().display_name), Some(&h.id));
                let status = if matches!(e, JobError::Fatal(_)) { JobStatus::Failed } else { JobStatus::NeedsAttention };
                set_status(inner, h, status, Some(msg));
                notify_attention(inner, h);
                return;
            }
        }
    }
}

fn notify_attention(inner: &Inner, h: &JobHandle) {
    let name = h.row.lock().display_name.clone();
    inner.notify("Transfer needs attention", &format!("{name} stopped. Open Relayless for details."));
}

/// Opens (or creates) the partial file and reconciles it with stored progress.
fn prepare_partial(inner: &Inner, row: &JobRow) -> Result<File, JobError> {
    let temp = PathBuf::from(&row.temp_path);
    if let Some(parent) = temp.parent() {
        std::fs::create_dir_all(parent).map_err(|e| disk_err(e, "creating the destination folder"))?;
    }
    let existed = temp.exists();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&temp)
        .map_err(|e| disk_err(e, "opening the partial file"))?;
    let len = file.metadata().map_err(|e| disk_err(e, "reading the partial file"))?.len();
    if !existed || len != row.size {
        if row.verified_bytes > 0 || existed {
            warn!("partial file for {} missing or resized; restarting its progress", row.id);
            inner.db.clear_chunks(&row.id).map_err(|e| JobError::Fatal(e.to_string()))?;
        }
        let needed = row.size.saturating_sub(len);
        if let Some(free) = available_space(&temp) {
            if free < needed + 64 * 1024 * 1024 {
                return Err(JobError::Disk(format!(
                    "Not enough free space: this file needs {}, but only {} is free.",
                    human_bytes(needed),
                    human_bytes(free)
                )));
            }
        }
        file.set_len(row.size).map_err(|e| disk_err(e, "reserving space"))?;
    }
    Ok(file)
}

async fn transfer_pass(inner: &Arc<Inner>, h: &Arc<JobHandle>, conn: &Connection, peer: EndpointId) -> Result<(), JobError> {
    let row = h.row.lock().clone();
    let d = &h.diag;
    d.set_connection(conn);
    // Route straight from the connection's path API, not the cached watch.
    let r = route_of(conn);
    d.observe_route(diag::route_label(&r.kind), r.rtt_ms);

    // The sender's file must still be the exact version this job started with.
    let t = Instant::now();
    let resp = request(conn, &Request::Stat { share: row.share_id.clone(), path: row.remote_path.clone() }).await;
    d.stages.stat_request.record(t.elapsed());
    let resp = resp.inspect_err(|e| d.error(job_err_cat(e)))?;
    match resp {
        Response::Stat { size, mtime_ms, dir } if !dir && size == row.size && mtime_ms == row.mtime_ms => {}
        Response::Stat { .. } => {
            d.error(diag::ErrCat::SourceChanged);
            return Err(JobError::SourceChanged);
        }
        _ => return Err(JobError::Network("Unexpected reply.".into())),
    }

    let r2 = row.clone();
    let inner2 = inner.clone();
    let t = Instant::now();
    let file = tokio::task::spawn_blocking(move || prepare_partial(&inner2, &r2))
        .await
        .map_err(|e| JobError::Fatal(e.to_string()))?;
    d.stages.prepare_partial.record(t.elapsed());
    let file = file.inspect_err(|e| d.error(job_err_cat(e)))?;
    let file = Arc::new(file);

    let verified: HashSet<u64> = inner
        .db
        .verified_chunks(&row.id)
        .map_err(|e| JobError::Fatal(e.to_string()))?
        .into_iter()
        .map(|(i, _, _)| i)
        .collect();
    let saved: Vec<crate::db::Partial> = inner
        .db
        .partials(&row.id)
        .map_err(|e| JobError::Fatal(e.to_string()))?
        .into_iter()
        .filter(|(i, d, hs)| !verified.contains(i) && *i < row.total_chunks && *d < chunk_len(&row, *i) && hs.len() as u64 == d.div_ceil(BLOCK as u64))
        .collect();
    let verified_bytes: u64 = verified.iter().map(|&i| chunk_len(&row, i)).sum::<u64>() + saved.iter().map(|(_, d, _)| *d).sum::<u64>();
    h.verified.store(verified_bytes, Ordering::Relaxed);
    h.verified_chunks.store(verified.len() as u64, Ordering::Relaxed);
    h.inflight.store(0, Ordering::Relaxed);
    h.arriving.store(0, Ordering::Relaxed);
    // Finish part-done chunks first, then the rest in order.
    let mut missing: VecDeque<u64> = saved.iter().map(|(i, _, _)| *i).collect();
    missing.extend((0..row.total_chunks).filter(|i| !verified.contains(i) && !saved.iter().any(|(p, _, _)| p == i)));
    if missing.is_empty() {
        return Ok(());
    }
    set_status(inner, h, JobStatus::Downloading, None);
    h.begin_active();

    let committed: HashMap<u64, u64> = saved.iter().map(|(i, d, _)| (*i, *d)).collect();
    let partials: Arc<Mutex<HashMap<u64, (u64, Vec<[u8; 32]>)>>> =
        Arc::new(Mutex::new(saved.into_iter().map(|(i, d, hs)| (i, (d, hs))).collect()));

    let settings = inner.settings();
    let workers = (settings.parallel_streams as usize).min(missing.len()).max(1);
    if !d.has_configuration() {
        d.set_configuration(receiver_config_json(inner, &settings, &row, workers, missing.len()));
    }
    // Passes that start workers (Stat failures and empty passes don't count).
    d.c.passes.fetch_add(1, Ordering::Relaxed);
    d.c.workers_started.fetch_add(workers as u64, Ordering::Relaxed);
    let queue = Arc::new(Mutex::new(missing));
    let pass_cancel = h.cancel.child_token();
    let (done_tx, done_rx) = mpsc::unbounded_channel::<Progress>();
    let committer = tokio::spawn(commit_loop(inner.clone(), h.clone(), file.clone(), done_rx, committed));
    let pc = peer_conn(inner, peer);

    let mut set = tokio::task::JoinSet::new();
    for worker in 0..workers {
        let inner = inner.clone();
        let h = h.clone();
        let conn = conn.clone();
        let file = file.clone();
        let queue = queue.clone();
        let cancel = pass_cancel.clone();
        let done_tx = done_tx.clone();
        let row = row.clone();
        let pc = pc.clone();
        let partials = partials.clone();
        set.spawn(async move {
            let born = Instant::now();
            let r = async {
            let mut corrupt_retries = 0u32;
            loop {
                if cancel.is_cancelled() {
                    return Err(JobError::Stopped);
                }
                let Some(idx) = queue.lock().pop_front() else { return Ok(()) };
                match fetch_chunk(&inner, &conn, &file, &row, idx, &cancel, &pc, &partials, &done_tx, &h, worker as u32).await {
                    Ok(()) => {}
                    Err(ChunkError::Corrupt) if corrupt_retries < 5 => {
                        corrupt_retries += 1;
                        h.diag.c.chunk_retries.fetch_add(1, Ordering::Relaxed);
                        inner.log("warning", &format!("A block of {} failed verification; fetching it again", row.display_name), Some(&row.id));
                        queue.lock().push_back(idx);
                    }
                    Err(e) => {
                        queue.lock().push_front(idx);
                        let e = match e {
                            ChunkError::Corrupt => JobError::Fatal("Data kept failing verification.".into()),
                            ChunkError::Job(e) => e,
                        };
                        if !matches!(e, JobError::Stopped) {
                            cancel.cancel();
                        }
                        return Err(e);
                    }
                }
            }
            }
            .await;
            h.diag.stages.worker_time.record(born.elapsed());
            r
        });
    }
    drop(done_tx);

    let mut first_err: Option<JobError> = None;
    while let Some(r) = set.join_next().await {
        let r = r.unwrap_or_else(|e| Err(JobError::Fatal(format!("worker crashed: {e}"))));
        if let Err(e) = r {
            if first_err.as_ref().is_none_or(|f| e.severity() > f.severity()) {
                first_err = Some(e);
            }
        }
    }
    // The committer saves everything received so far, including on pause.
    let commit_res = committer.await.unwrap_or_else(|e| Err(JobError::Fatal(e.to_string())));
    if let Err(e) = commit_res {
        return Err(e);
    }
    if h.cancel.is_cancelled() {
        return Err(JobError::Stopped);
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    if !queue.lock().is_empty() {
        return Err(JobError::Network("Transfer interrupted.".into()));
    }
    Ok(())
}

fn chunk_len(row: &JobRow, idx: u64) -> u64 {
    let off = idx * row.chunk_size;
    row.chunk_size.min(row.size.saturating_sub(off))
}

enum ChunkError {
    Corrupt,
    Job(JobError),
}

impl From<JobError> for ChunkError {
    fn from(e: JobError) -> Self {
        ChunkError::Job(e)
    }
}

/// Progress reported by download workers to the committer.
enum Progress {
    /// Verified prefix of a chunk (bytes done, hashes of the blocks so far).
    Partial { idx: u64, done: u64, hashes: Vec<[u8; 32]> },
    /// A complete chunk and its recorded hash.
    Done { idx: u64, hash: [u8; 32] },
}

/// Read-wait accumulator: one stage record per block (or per second of
/// waiting on slow links) instead of one per `recv.read` call.
struct ReadAcc<'a> {
    d: &'a diag::TransferDiag,
    wait: Duration,
}

impl ReadAcc<'_> {
    fn flush(&mut self) {
        if !self.wait.is_zero() {
            self.d.stages.read_wait.record(self.wait);
            self.wait = Duration::ZERO;
        }
    }
}

impl Drop for ReadAcc<'_> {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Diagnostic category for a job error (diagnostics only).
fn job_err_cat(e: &JobError) -> diag::ErrCat {
    use diag::ErrCat as C;
    match e {
        JobError::Network(_) => C::ConnectionLost,
        JobError::Busy(_) => C::RemoteError,
        JobError::SourceChanged => C::SourceChanged,
        JobError::Missing(_) => C::RemoteError,
        JobError::Denied(_) => C::Denied,
        JobError::RelayBlocked => C::RelayPathActive,
        JobError::Disk(_) => C::ReceiverDiskWrite,
        JobError::Fatal(_) => C::Unknown,
        JobError::Stopped => C::Cancelled,
    }
}

/// Configuration snapshot for a receiver summary.
fn receiver_config_json(inner: &Inner, s: &crate::types::Settings, row: &JobRow, workers: usize, missing: usize) -> serde_json::Value {
    serde_json::json!({
        "chunk_size_bytes": row.chunk_size,
        "chunk_size_source": "stored with the job when it was created",
        "chunk_size_setting_mib_now": s.chunk_size_mib,
        "block_size_bytes": BLOCK,
        "total_chunks": row.total_chunks,
        "chunks_missing_at_first_pass": missing,
        "parallel_streams_setting": s.parallel_streams,
        "workers_first_pass": workers,
        "max_concurrent_jobs": s.max_concurrent_jobs,
        "connection_mode": s.connection_mode,
        "transport": crate::net::transport_json(inner.congestion_active),
        "congestion_control_setting": s.congestion,
        "download_limit": diag::limit_json(s.download_limit_kbps, inner.download_limiter.rate()),
        "upload_limit_this_device": diag::limit_json(s.upload_limit_kbps, inner.upload_limiter.rate()),
        "timeouts_ms": {
            "connect": CONNECT_TIMEOUT.as_millis() as u64,
            "response_header": READ_TIMEOUT.as_millis() as u64,
            "stall": STALL_TIMEOUT.as_millis() as u64,
            "direct_grace": DIRECT_GRACE.as_millis() as u64,
        },
        "commit_interval_ms": COMMIT_INTERVAL.as_millis() as u64,
        "local_mode": inner.local_mode,
    })
}

#[allow(clippy::too_many_arguments)]
async fn fetch_chunk(
    inner: &Arc<Inner>,
    conn: &Connection,
    file: &Arc<File>,
    row: &JobRow,
    idx: u64,
    cancel: &CancellationToken,
    pc: &PeerConn,
    partials: &Mutex<HashMap<u64, (u64, Vec<[u8; 32]>)>>,
    tx: &mpsc::UnboundedSender<Progress>,
    h: &JobHandle,
    worker: u32,
) -> Result<(), ChunkError> {
    let d = &h.diag;
    let from = partials.lock().get(&idx).map(|(done, _)| *done).unwrap_or(0);
    let route = diag::route_label(&pc.route.borrow().kind);
    let mut trace = diag::ChunkTrace::new(d.origin, idx, Some(worker), from, Some(route));
    let mut cat: Option<diag::ErrCat> = None;
    d.c.chunks_started.fetch_add(1, Ordering::Relaxed);
    if from > 0 {
        d.c.chunks_resumed.fetch_add(1, Ordering::Relaxed);
    }
    let r = fetch_chunk_inner(inner, conn, file, row, idx, cancel, pc, partials, tx, h, &mut trace, &mut cat).await;
    let (outcome, cat) = match &r {
        Ok(()) => ("completed", None),
        // Stopped by pause, shutdown or a sibling worker's failure: counted in
        // chunks_stopped, not as an error, so it can't mask the real cause.
        Err(ChunkError::Job(JobError::Stopped)) => ("stopped", None),
        Err(ChunkError::Corrupt) => ("failed", Some(diag::ErrCat::HashMismatch)),
        Err(ChunkError::Job(e)) => ("failed", Some(cat.unwrap_or_else(|| job_err_cat(e)))),
    };
    if let Some(c) = cat {
        d.error(c);
    }
    d.finish_chunk(trace.finish(outcome, cat, true));
    r
}

#[allow(clippy::too_many_arguments)]
async fn fetch_chunk_inner(
    inner: &Arc<Inner>,
    conn: &Connection,
    file: &Arc<File>,
    row: &JobRow,
    idx: u64,
    cancel: &CancellationToken,
    pc: &PeerConn,
    partials: &Mutex<HashMap<u64, (u64, Vec<[u8; 32]>)>>,
    tx: &mpsc::UnboundedSender<Progress>,
    h: &JobHandle,
    trace: &mut diag::ChunkTrace,
    cat: &mut Option<diag::ErrCat>,
) -> Result<(), ChunkError> {
    use diag::ErrCat as C;
    let d = &h.diag;
    let direct_only = inner.settings.read().direct_only();
    if direct_only && !pc.route.borrow().is_direct() {
        *cat = Some(C::RelayPathActive);
        return Err(JobError::RelayBlocked.into());
    }
    let len = chunk_len(row, idx);
    let (mut done, mut hashes) = partials.lock().get(&idx).cloned().unwrap_or((0, Vec::new()));
    let t = Instant::now();
    let opened = conn.open_bi().await;
    let open_d = t.elapsed();
    d.stages.stream_open.record(open_d);
    trace.rec.stream_open_ms = Some(diag::ms(open_d));
    if open_d >= diag::SLOW_STREAM_OPEN {
        d.c.slow_stream_opens.fetch_add(1, Ordering::Relaxed);
    }
    let (mut send, mut recv) = opened.map_err(|e| {
        *cat = Some(C::StreamOpenFailed);
        net(e)
    })?;
    // Active from stream open until this function returns.
    let _job_stream = d.streams.enter();
    let _peer_stream = pc.streams.enter();
    // Streams open to this sender (all jobs) when this one started.
    d.c.conn_streams_peak.fetch_max(pc.streams.current(), Ordering::Relaxed);
    let t = Instant::now();
    let wrote = write_frame(
        &mut send,
        &Request::Chunk {
            share: row.share_id.clone(),
            path: row.remote_path.clone(),
            size: row.size,
            mtime_ms: row.mtime_ms,
            chunk_size: row.chunk_size,
            index: idx,
            from: done,
            have: Some(h.done_bytes().min(row.size)),
        },
    )
    .await
    .and_then(|_| send.finish().map_err(anyhow::Error::from));
    d.stages.request_write.record(t.elapsed());
    trace.rec.request_write_ms = Some(diag::ms(t.elapsed()));
    wrote.map_err(|e| {
        *cat = Some(match diag::classify_anyhow(&e) {
            C::Unknown => C::RequestWriteFailed,
            c => c,
        });
        net(e)
    })?;
    let t = Instant::now();
    let resp = tokio::select! {
        r = tokio::time::timeout(READ_TIMEOUT, read_frame::<Response>(&mut recv)) => Some(r),
        _ = cancel.cancelled() => None,
    };
    d.stages.header_wait.record(t.elapsed());
    trace.rec.header_wait_ms = Some(diag::ms(t.elapsed()));
    let resp: Response = match resp {
        None => return Err(JobError::Stopped.into()),
        Some(Err(_)) => {
            *cat = Some(C::ResponseTimeout);
            return Err(JobError::Network("The sender stopped responding.".into()).into());
        }
        Some(Ok(Err(e))) => {
            *cat = Some(diag::classify_anyhow(&e));
            return Err(net(e).into());
        }
        Some(Ok(Ok(r))) => r,
    };
    match resp {
        Response::Chunk { len: l } if l == len - done => {}
        Response::Error { code, message } => {
            *cat = Some(match code {
                ErrCode::SourceChanged => C::SourceChanged,
                ErrCode::RelayBlocked => C::RelayPathActive,
                ErrCode::Denied => C::Denied,
                ErrCode::BadRequest => C::BadRequest,
                _ => C::RemoteError,
            });
            return Err(remote_err(code, message).into());
        }
        _ => {
            *cat = Some(C::Unknown);
            return Err(JobError::Network("Unexpected chunk reply.".into()).into());
        }
    }

    let base = idx * row.chunk_size;
    let mut buf = vec![0u8; BLOCK];
    let mut want = [0u8; 32];
    let mut blocks = 0u32;
    // Bytes of the current block counted in `h.arriving`; removed again on
    // every exit path so the counter never drifts.
    struct Arriving<'a>(&'a AtomicU64, u64);
    impl Drop for Arriving<'_> {
        fn drop(&mut self) {
            let n = self.1;
            let _ = self.0.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(n)));
        }
    }
    while done < len {
        let n = ((len - done) as usize).min(BLOCK);
        let w = inner.download_limiter.acquire(n).await;
        if !w.is_zero() {
            d.stages.limiter_wait.record_bytes(w, n as u64);
            d.c.limiter_waits.fetch_add(1, Ordering::Relaxed);
            trace.limiter(w);
        }
        let mut arriving = Arriving(&h.arriving, 0);
        // Read wait for this block, recorded once per block or per second of
        // waiting (on drop too, so early returns are counted).
        let mut acc = ReadAcc { d, wait: Duration::ZERO };
        let mut got = 0usize;
        while got < n {
            let t = Instant::now();
            let r = tokio::select! {
                r = tokio::time::timeout(STALL_TIMEOUT, recv.read(&mut buf[got..n])) => r,
                _ = cancel.cancelled() => {
                    let _ = recv.stop(VarInt::from_u32(0));
                    return Err(JobError::Stopped.into());
                }
            };
            let rd = t.elapsed();
            acc.wait += rd;
            trace.read_wait(rd);
            if acc.wait >= Duration::from_secs(1) {
                // Slow link: keep interval samples live instead of waiting
                // minutes for the block to finish.
                acc.flush();
            }
            let r = match r {
                Err(_) => {
                    *cat = Some(C::StallTimeout);
                    return Err(JobError::Network("No data arrived for 90 seconds.".into()).into());
                }
                Ok(r) => r.map_err(|e| {
                    *cat = Some(diag::classify_read(&e));
                    net(e)
                })?,
            };
            match r {
                Some(k) => {
                    got += k;
                    arriving.1 += k as u64;
                    h.arriving.fetch_add(k as u64, Ordering::Relaxed);
                    // One relaxed add per read keeps the interval byte rate live.
                    d.c.net_bytes.fetch_add(k as u64, Ordering::Relaxed);
                }
                None => {
                    *cat = Some(C::StreamClosedEarly);
                    return Err(JobError::Network("The sender closed the stream early.".into()).into());
                }
            }
        }
        let t = Instant::now();
        tokio::select! {
            r = tokio::time::timeout(STALL_TIMEOUT, recv.read_exact(&mut want)) => {
                let rd = t.elapsed();
                acc.wait += rd;
                trace.read_wait(rd);
                match r {
                    Err(_) => {
                        *cat = Some(C::StallTimeout);
                        return Err(JobError::Network("No data arrived for 90 seconds.".into()).into());
                    }
                    Ok(r) => r.map_err(|e| {
                        *cat = Some(diag::classify_read_exact(&e));
                        net(e)
                    })?,
                }
            }
            _ = cancel.cancelled() => {
                let _ = recv.stop(VarInt::from_u32(0));
                return Err(JobError::Stopped.into());
            }
        }
        drop(acc);
        // Verify, then write; nothing unverified ever counts as progress.
        let f = file.clone();
        let pos = base + done;
        let queued = Instant::now();
        let (b, r, hash_d, write_d) = tokio::task::spawn_blocking(move || {
            let b = buf;
            let t = Instant::now();
            let got = *blake3::hash(&b[..n]).as_bytes();
            let hash_d = t.elapsed();
            let t = Instant::now();
            let r = if got != want { Ok(false) } else { write_all_at(&f, &b[..n], pos).map(|_| true) };
            let write_d = t.elapsed();
            (b, r, hash_d, write_d)
        })
        .await
        .map_err(|e| JobError::Fatal(e.to_string()))?;
        let pool_wait = queued.elapsed().saturating_sub(hash_d + write_d);
        d.stages.hash.record_bytes(hash_d, n as u64);
        d.stages.blocking_queue.record(pool_wait);
        trace.hash(hash_d);
        trace.blocking_queue(pool_wait);
        buf = b;
        let ok = match r {
            Ok(ok) => {
                if ok {
                    d.stages.disk_write.record_bytes(write_d, n as u64);
                    trace.disk_write(write_d);
                }
                ok
            }
            Err(e) => {
                *cat = Some(C::ReceiverDiskWrite);
                return Err(disk_err(e, "writing data").into());
            }
        };
        if !ok {
            d.c.hash_failures.fetch_add(1, Ordering::Relaxed);
            trace.rec.hash_failures += 1;
            let _ = recv.stop(VarInt::from_u32(0));
            return Err(ChunkError::Corrupt);
        }
        done += n as u64;
        hashes.push(want);
        // Move the block from "arriving" to "verified, waiting to be saved".
        h.inflight.fetch_add(n as u64, Ordering::Relaxed);
        drop(arriving);
        let route = pc.route.borrow().kind.clone();
        if route == "direct" {
            h.direct_bytes.fetch_add(n as u64, Ordering::Relaxed);
        } else {
            h.relay_bytes.fetch_add(n as u64, Ordering::Relaxed);
        }
        d.c.add_route_bytes(diag::route_label(&route), n as u64);
        d.c.bytes.fetch_add(n as u64, Ordering::Relaxed);
        d.c.blocks.fetch_add(1, Ordering::Relaxed);
        trace.block(n as u64);
        partials.lock().insert(idx, (done, hashes.clone()));
        let _ = tx.send(Progress::Partial { idx, done, hashes: hashes.clone() });
        blocks += 1;
        if direct_only && blocks % 8 == 0 && !pc.route.borrow().is_direct() {
            *cat = Some(C::RelayPathActive);
            let _ = recv.stop(VarInt::from_u32(0));
            return Err(JobError::RelayBlocked.into());
        }
    }
    partials.lock().remove(&idx);
    let _ = tx.send(Progress::Done { idx, hash: chunk_hash_from_blocks(&hashes) });
    Ok(())
}

/// Saves progress: flush the file to disk, then record finished chunks and
/// the verified prefix of unfinished ones. Runs until all workers stop, so a
/// pause or disconnect always saves what was received.
async fn commit_loop(
    inner: Arc<Inner>,
    h: Arc<JobHandle>,
    file: Arc<File>,
    mut rx: mpsc::UnboundedReceiver<Progress>,
    mut committed: HashMap<u64, u64>,
) -> Result<(), JobError> {
    let row = h.row.lock().clone();
    let mut parts: HashMap<u64, (u64, Vec<[u8; 32]>)> = HashMap::new();
    let mut dones: Vec<(u64, [u8; 32])> = Vec::new();
    let mut last = Instant::now();
    let mut open = true;
    while open {
        tokio::select! {
            m = rx.recv() => match m {
                Some(Progress::Partial { idx, done, hashes }) => { parts.insert(idx, (done, hashes)); }
                Some(Progress::Done { idx, hash }) => { parts.remove(&idx); dones.push((idx, hash)); }
                None => open = false,
            },
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
        if dones.is_empty() && parts.is_empty() {
            continue;
        }
        let base = |i: &u64| committed.get(i).copied().unwrap_or(0);
        let delta: u64 = dones.iter().map(|(i, _)| chunk_len(&row, *i).saturating_sub(base(i))).sum::<u64>()
            + parts.iter().map(|(i, (d, _))| d.saturating_sub(base(i))).sum::<u64>();
        if !open || last.elapsed() >= COMMIT_INTERVAL || delta >= 256 * 1024 * 1024 {
            let f = file.clone();
            let t = Instant::now();
            let synced = tokio::task::spawn_blocking(move || {
                let t = Instant::now();
                (f.sync_data(), t.elapsed())
            })
            .await
            .map_err(|e| JobError::Fatal(e.to_string()))?;
            let (synced, sync_d) = synced;
            h.diag.stages.fsync.record(sync_d);
            h.diag.stages.blocking_queue.record(t.elapsed().saturating_sub(sync_d));
            synced.map_err(|e| {
                h.diag.error(diag::ErrCat::ReceiverDiskWrite);
                disk_err(e, "saving data to disk")
            })?;
            let new_verified = h.verified.load(Ordering::Relaxed) + delta;
            let plist: Vec<crate::db::Partial> = parts.drain().map(|(i, (d, hs))| (i, d, hs)).collect();
            let t = Instant::now();
            let committed_db = inner.db.commit_progress(&row.id, &dones, &plist, new_verified, &h.stats());
            h.diag.stages.db_commit.record(t.elapsed());
            committed_db.map_err(|e| JobError::Fatal(format!("database error: {e}")))?;
            for (i, _) in &dones {
                committed.remove(i);
            }
            for (i, d, _) in &plist {
                committed.insert(*i, *d);
            }
            h.verified.store(new_verified, Ordering::Relaxed);
            let _ = h.inflight.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(delta)));
            h.verified_chunks.fetch_add(dones.len() as u64, Ordering::Relaxed);
            h.row.lock().verified_bytes = new_verified;
            dones.clear();
            last = Instant::now();
        }
    }
    Ok(())
}

/// Re-reads the whole partial file and checks every chunk hash, then renames.
/// Returns Ok(false) if some chunks were bad and must be fetched again.
async fn verify_and_finalize(inner: &Arc<Inner>, h: &Arc<JobHandle>) -> Result<bool, JobError> {
    let row = h.row.lock().clone();
    let expected = inner.db.verified_chunks(&row.id).map_err(|e| JobError::Fatal(e.to_string()))?;
    if (expected.len() as u64) < row.total_chunks {
        return Ok(false);
    }
    let cancel = h.cancel.clone();
    let inner2 = inner.clone();
    let row2 = row.clone();
    let hd = h.clone();
    let verify_start = Instant::now();
    let bad = tokio::task::spawn_blocking(move || -> Result<Vec<u64>, JobError> {
        let st = &hd.diag.stages;
        let f = File::open(&row2.temp_path).map_err(|e| disk_err(e, "verifying"))?;
        let mut buf = vec![0u8; BLOCK];
        let mut bad = Vec::new();
        let total = expected.len() as u64;
        let mut last_emit = Instant::now();
        for (n, (idx, want, scheme)) in expected.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(JobError::Stopped);
            }
            let mut whole = blake3::Hasher::new();
            let mut blocks: Vec<[u8; 32]> = Vec::new();
            let mut pos = idx * row2.chunk_size;
            let mut remaining = chunk_len(&row2, *idx);
            while remaining > 0 {
                let k = (remaining as usize).min(BLOCK);
                let t = Instant::now();
                read_exact_at(&f, &mut buf[..k], pos).map_err(|e| disk_err(e, "verifying"))?;
                st.final_read.record_bytes(t.elapsed(), k as u64);
                let t = Instant::now();
                if *scheme == crate::db::SCHEME_BLOCKS {
                    blocks.push(*blake3::hash(&buf[..k]).as_bytes());
                } else {
                    whole.update(&buf[..k]);
                }
                st.final_hash.record_bytes(t.elapsed(), k as u64);
                pos += k as u64;
                remaining -= k as u64;
            }
            let got = if *scheme == crate::db::SCHEME_BLOCKS {
                chunk_hash_from_blocks(&blocks)
            } else {
                *whole.finalize().as_bytes()
            };
            if &got != want {
                bad.push(*idx);
            }
            if last_emit.elapsed() > Duration::from_millis(400) || n as u64 + 1 == total {
                last_emit = Instant::now();
                inner2.emit(CoreEvent::IntegrityProgress { job_id: row2.id.clone(), checked: n as u64 + 1, total });
            }
        }
        Ok(bad)
    })
    .await
    .map_err(|e| JobError::Fatal(e.to_string()))?;
    h.diag.stages.final_verification.record_bytes(verify_start.elapsed(), row.size);
    let bad: Vec<u64> = bad?;

    if !bad.is_empty() {
        h.diag.note(format!("observation: {} chunk(s) failed the final re-read check and were queued again", bad.len()));
        h.diag.c.chunk_retries.fetch_add(bad.len() as u64, Ordering::Relaxed);
        let bad_bytes: u64 = bad.iter().map(|i| chunk_len(&row, *i)).sum();
        let new_verified = h.verified.load(Ordering::Relaxed).saturating_sub(bad_bytes);
        inner.db.remove_chunks(&row.id, &bad, new_verified).map_err(|e| JobError::Fatal(e.to_string()))?;
        h.verified.store(new_verified, Ordering::Relaxed);
        inner.log(
            "warning",
            &format!("{}: {} chunk(s) failed the final check and will be downloaded again", row.display_name, bad.len()),
            Some(&row.id),
        );
        return Ok(false);
    }

    let dest = PathBuf::from(&row.dest_path);
    let temp = row.temp_path.clone();
    let t = Instant::now();
    let final_path = tokio::task::spawn_blocking(move || -> Result<PathBuf, JobError> {
        let target = unique_path(&dest);
        std::fs::rename(&temp, &target).map_err(|e| disk_err(e, "renaming the finished file"))?;
        Ok(target)
    })
    .await
    .map_err(|e| JobError::Fatal(e.to_string()))??;
    h.diag.stages.rename.record(t.elapsed());
    let final_s = final_path.to_string_lossy().to_string();
    inner.db.set_job_completed(&row.id, &final_s).map_err(|e| JobError::Fatal(e.to_string()))?;
    {
        let mut r = h.row.lock();
        r.status = JobStatus::Completed;
        r.error = None;
        r.dest_path = final_s.clone();
        r.completed_at = Some(now_ms());
        r.verified_bytes = r.size;
    }
    h.verified.store(row.size, Ordering::Relaxed);
    h.inflight.store(0, Ordering::Relaxed);
    emit_job(inner, h);
    record_received(inner, &row, h);
    inner.log("success", &format!("Downloaded and verified {} ({})", row.display_name, human_bytes(row.size)), Some(&row.id));
    inner.notify("Transfer complete", &format!("{} was downloaded and verified.", row.display_name));
    debug!("job {} complete", row.id);
    Ok(true)
}

/// File extension in lower case, for categories in history.
pub fn ext_of(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase().chars().take(12).collect())
        .unwrap_or_default()
}

/// Adds the finished download to local history and tells the sender, so
/// it can keep its own entry. Both steps are best effort.
fn record_received(inner: &Arc<Inner>, row: &JobRow, h: &Arc<JobHandle>) {
    let settings = inner.settings();
    let mut stats = h.stats();
    // Short transfers can finish before the speed estimate settles; the
    // average is a floor for the peak.
    if stats.active_ms > 0 {
        let avg = (stats.direct_bytes + stats.relay_bytes) as f64 / (stats.active_ms as f64 / 1000.0);
        if avg > stats.peak_bps {
            stats.peak_bps = avg;
        }
    }
    let fingerprint = inner.db.file_fingerprint(&row.id).unwrap_or_default();
    let blocks = row.size.div_ceil(BLOCK as u64);
    let finished_at = now_ms();
    if settings.history_on() {
        let entry = HistoryRow {
            id: format!("r-{}", row.id),
            direction: "received".into(),
            peer_id: row.peer_id.clone(),
            peer_name: h.peer_name.clone(),
            name: if settings.history_names { row.display_name.clone() } else { String::new() },
            ext: ext_of(&row.display_name),
            size: row.size,
            started_at: row.created_at,
            finished_at,
            stats: stats.clone(),
            blocks,
            fingerprint: fingerprint.clone(),
        };
        if inner.db.insert_history(&entry).is_ok() {
            inner.emit(CoreEvent::HistoryChanged);
        }
    }
    let Ok(peer) = row.peer_id.parse::<EndpointId>() else { return };
    let receipt = Request::Receipt {
        share: row.share_id.clone(),
        path: row.remote_path.clone(),
        size: row.size,
        job: row.id.clone(),
        fingerprint,
        started_at: row.created_at,
        finished_at,
        active_ms: stats.active_ms,
        direct_bytes: stats.direct_bytes,
        relay_bytes: stats.relay_bytes,
        peak_bps: stats.peak_bps,
        blocks,
    };
    let inner = inner.clone();
    tokio::spawn(async move {
        let conn = match get_conn(&inner, peer).await {
            Ok(c) => c,
            Err(e) => {
                debug!("receipt not sent: {e}");
                return;
            }
        };
        if let Err(e) = request(&conn, &receipt).await {
            debug!("sender did not take the receipt: {e}");
        }
    });
}
