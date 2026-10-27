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
}

impl PeerConn {
    fn new(id: EndpointId) -> Self {
        PeerConn {
            conn: tokio::sync::Mutex::new(None),
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
    let pc = peer_conn(inner, peer);
    let mut guard = pc.conn.lock().await;
    if let Some(c) = guard.as_ref() {
        if c.close_reason().is_none() {
            return Ok(c.clone());
        }
    }
    set_peer_state(inner, &pc, "connecting", None);
    let conn = match dial(inner, peer).await {
        Ok(c) => c,
        Err(e) => {
            set_peer_state(inner, &pc, "unreachable", Some(e.to_string()));
            return Err(e);
        }
    };
    let device = inner.settings.read().device_name.clone();
    let hello = match hello(&conn, None, &device, |_| {}).await {
        Ok(h) => h,
        Err(e) => {
            set_peer_state(inner, &pc, "unreachable", Some(e.to_string()));
            return Err(e);
        }
    };
    if hello.status != HelloStatus::Approved {
        let msg = hello.message.unwrap_or_else(|| "The sender has removed this device's access.".into());
        set_peer_state(inner, &pc, "denied", Some(msg.clone()));
        conn.close(VarInt::from_u32(0), b"denied");
        return Err(JobError::Denied(msg));
    }
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
    pub verified_chunks: AtomicU64,
    speed: Mutex<SpeedState>,
    /// Bytes received over a direct path and through a relay.
    pub direct_bytes: AtomicU64,
    pub relay_bytes: AtomicU64,
    /// Time spent actually downloading, excluding the current stretch.
    active_ms: AtomicU64,
    active_since: Mutex<Option<Instant>>,
    peak_bps: Mutex<f64>,
}

impl JobHandle {
    pub fn done_bytes(&self) -> u64 {
        self.verified.load(Ordering::Relaxed) + self.inflight.load(Ordering::Relaxed)
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
    let h = Arc::new(JobHandle {
        id: row.id.clone(),
        cancel: inner.shutdown.child_token(),
        stop: Mutex::new(None),
        verified: AtomicU64::new(row.verified_bytes),
        inflight: AtomicU64::new(0),
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
        run_job(&inner, &h).await;
        inner.jobs.lock().remove(&h.id);
        if let Ok(Some(row)) = inner.db.get_job(&h.id) {
            inner.emit(CoreEvent::JobUpdated(snapshot(&inner, &row, None, Some(&h.peer_name))));
        }
    });
}

async fn sleep_or_stop(h: &JobHandle, d: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(d) => false,
        _ = h.cancel.cancelled() => true,
    }
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
        let _slot = tokio::select! {
            g = inner.slots.acquire() => g,
            _ = h.cancel.cancelled() => return,
        };
        set_status(inner, h, if attempt == 0 { JobStatus::Connecting } else { JobStatus::Reconnecting }, None);
        let conn = tokio::select! {
            c = get_conn(inner, peer) => c,
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

        // Honour direct-only mode before sending any file bytes.
        if inner.settings.read().direct_only() {
            let pc = peer_conn(inner, peer);
            let mut rx = pc.route.subscribe();
            if !rx.borrow().is_direct() {
                set_status(inner, h, JobStatus::WaitingDirect, Some(
                    "Connected through a relay. Direct-only mode is on, so no file data is sent until a direct path opens.".into(),
                ));
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
                if !pc.route.borrow().is_direct() {
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

    // The sender's file must still be the exact version this job started with.
    let resp = request(conn, &Request::Stat { share: row.share_id.clone(), path: row.remote_path.clone() }).await?;
    match resp {
        Response::Stat { size, mtime_ms, dir } if !dir && size == row.size && mtime_ms == row.mtime_ms => {}
        Response::Stat { .. } => return Err(JobError::SourceChanged),
        _ => return Err(JobError::Network("Unexpected reply.".into())),
    }

    let r2 = row.clone();
    let inner2 = inner.clone();
    let file = tokio::task::spawn_blocking(move || prepare_partial(&inner2, &r2))
        .await
        .map_err(|e| JobError::Fatal(e.to_string()))??;
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
    let queue = Arc::new(Mutex::new(missing));
    let pass_cancel = h.cancel.child_token();
    let (done_tx, done_rx) = mpsc::unbounded_channel::<Progress>();
    let committer = tokio::spawn(commit_loop(inner.clone(), h.clone(), file.clone(), done_rx, committed));
    let pc = peer_conn(inner, peer);

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..workers {
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
            let mut corrupt_retries = 0u32;
            loop {
                if cancel.is_cancelled() {
                    return Err(JobError::Stopped);
                }
                let Some(idx) = queue.lock().pop_front() else { return Ok(()) };
                match fetch_chunk(&inner, &conn, &file, &row, idx, &cancel, &pc, &partials, &done_tx, &h).await {
                    Ok(()) => {}
                    Err(ChunkError::Corrupt) if corrupt_retries < 5 => {
                        corrupt_retries += 1;
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
) -> Result<(), ChunkError> {
    let direct_only = inner.settings.read().direct_only();
    if direct_only && !pc.route.borrow().is_direct() {
        return Err(JobError::RelayBlocked.into());
    }
    let len = chunk_len(row, idx);
    let (mut done, mut hashes) = partials.lock().get(&idx).cloned().unwrap_or((0, Vec::new()));
    let (mut send, mut recv) = conn.open_bi().await.map_err(net)?;
    write_frame(
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
    .map_err(net)?;
    send.finish().map_err(net)?;
    let resp: Response = tokio::select! {
        r = tokio::time::timeout(READ_TIMEOUT, read_frame(&mut recv)) => r
            .map_err(|_| JobError::Network("The sender stopped responding.".into()))?
            .map_err(net)?,
        _ = cancel.cancelled() => return Err(JobError::Stopped.into()),
    };
    match resp {
        Response::Chunk { len: l } if l == len - done => {}
        Response::Error { code, message } => return Err(remote_err(code, message).into()),
        _ => return Err(JobError::Network("Unexpected chunk reply.".into()).into()),
    }

    let base = idx * row.chunk_size;
    let mut buf = vec![0u8; BLOCK];
    let mut want = [0u8; 32];
    let mut blocks = 0u32;
    while done < len {
        let n = ((len - done) as usize).min(BLOCK);
        inner.download_limiter.acquire(n).await;
        tokio::select! {
            r = tokio::time::timeout(READ_TIMEOUT, async {
                recv.read_exact(&mut buf[..n]).await?;
                recv.read_exact(&mut want).await
            }) => {
                r.map_err(|_| JobError::Network("The transfer stalled.".into()))?.map_err(net)?;
            }
            _ = cancel.cancelled() => {
                let _ = recv.stop(VarInt::from_u32(0));
                return Err(JobError::Stopped.into());
            }
        }
        // Verify, then write; nothing unverified ever counts as progress.
        let f = file.clone();
        let pos = base + done;
        let (b, r) = tokio::task::spawn_blocking(move || {
            let b = buf;
            let got = *blake3::hash(&b[..n]).as_bytes();
            let r = if got != want { Ok(false) } else { write_all_at(&f, &b[..n], pos).map(|_| true) };
            (b, r)
        })
        .await
        .map_err(|e| JobError::Fatal(e.to_string()))?;
        buf = b;
        if !r.map_err(|e| disk_err(e, "writing data"))? {
            let _ = recv.stop(VarInt::from_u32(0));
            return Err(ChunkError::Corrupt);
        }
        done += n as u64;
        hashes.push(want);
        h.inflight.fetch_add(n as u64, Ordering::Relaxed);
        if pc.route.borrow().is_direct() {
            h.direct_bytes.fetch_add(n as u64, Ordering::Relaxed);
        } else {
            h.relay_bytes.fetch_add(n as u64, Ordering::Relaxed);
        }
        partials.lock().insert(idx, (done, hashes.clone()));
        let _ = tx.send(Progress::Partial { idx, done, hashes: hashes.clone() });
        blocks += 1;
        if direct_only && blocks % 8 == 0 && !pc.route.borrow().is_direct() {
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
            tokio::task::spawn_blocking(move || f.sync_data())
                .await
                .map_err(|e| JobError::Fatal(e.to_string()))?
                .map_err(|e| disk_err(e, "saving data to disk"))?;
            let new_verified = h.verified.load(Ordering::Relaxed) + delta;
            let plist: Vec<crate::db::Partial> = parts.drain().map(|(i, (d, hs))| (i, d, hs)).collect();
            inner
                .db
                .commit_progress(&row.id, &dones, &plist, new_verified, &h.stats())
                .map_err(|e| JobError::Fatal(format!("database error: {e}")))?;
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
    let bad: Vec<u64> = tokio::task::spawn_blocking(move || -> Result<Vec<u64>, JobError> {
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
                read_exact_at(&f, &mut buf[..k], pos).map_err(|e| disk_err(e, "verifying"))?;
                if *scheme == crate::db::SCHEME_BLOCKS {
                    blocks.push(*blake3::hash(&buf[..k]).as_bytes());
                } else {
                    whole.update(&buf[..k]);
                }
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
    .map_err(|e| JobError::Fatal(e.to_string()))??;

    if !bad.is_empty() {
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
    let final_path = tokio::task::spawn_blocking(move || -> Result<PathBuf, JobError> {
        let target = unique_path(&dest);
        std::fs::rename(&temp, &target).map_err(|e| disk_err(e, "renaming the finished file"))?;
        Ok(target)
    })
    .await
    .map_err(|e| JobError::Fatal(e.to_string()))??;
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
