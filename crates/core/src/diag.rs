//! Transfer diagnostics: low-overhead counters, stage timers, route tracking
//! and one JSON summary per transfer.
//!
//! Design rules:
//! - Durations use `Instant` (monotonic). Wall-clock time is only used for the
//!   summary's `started_at_ms` / `finished_at_ms` labels.
//! - Counters are atomics updated per 1 MiB block or per chunk, never per byte
//!   or per packet. They are always on (cost: a few `Instant::now()` calls and
//!   atomic adds per MiB). "Disabled" mode turns off everything that produces
//!   output or extra work: the per-transfer sampler task, interval and summary
//!   log lines, and the JSON summary file.
//! - Unknown values are `null`, never `0`.
//! - Stage totals are summed across parallel workers, so the sum of all stages
//!   can exceed wall-clock time. Compare a stage with `worker_time_ms`, not with
//!   `elapsed_ms`.
//! - Nothing here logs file contents, names, keys, tokens or IP addresses.
//!   Files are identified by `file_key` (a hash of share id and path, the same
//!   on sender and receiver) and peers by an 8-character public-key prefix.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Value};

pub const SCHEMA_VERSION: u32 = 1;
pub const SUMMARY_FILE: &str = "transfers.jsonl";
const ROTATED_FILE: &str = "transfers.old.jsonl";
const MAX_SUMMARY_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
const MAX_SAMPLES: usize = 240;
const MAX_CHUNK_RECORDS: usize = 1000;
const MAX_ROUTE_CHANGES: usize = 100;
const MAX_CONN_ATTEMPTS: usize = 50;
/// An `open_bi` slower than this is counted as a stream-open wait (the
/// sender's concurrent stream limit is the usual cause).
pub const SLOW_STREAM_OPEN: Duration = Duration::from_millis(100);

/// Environment variable that overrides `CoreOptions::diagnostics`.
pub const ENV_VAR: &str = "RELAYLESS_DIAG";

// ====================================================================== config

#[derive(Debug, Clone)]
pub struct DiagConfig {
    pub enabled: bool,
    pub dir: PathBuf,
}

impl DiagConfig {
    /// `RELAYLESS_DIAG=0|off|false|no` disables, `1|on|true|yes` enables;
    /// anything else (or unset) keeps the option passed by the app.
    pub fn resolve(option: bool, env: Option<&str>, data_dir: &Path) -> Self {
        DiagConfig { enabled: env_override(env).unwrap_or(option), dir: data_dir.join("diagnostics") }
    }

    pub fn summary_path(&self) -> PathBuf {
        self.dir.join(SUMMARY_FILE)
    }
}

pub fn env_override(v: Option<&str>) -> Option<bool> {
    match v.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("0" | "off" | "false" | "no") => Some(false),
        Some("1" | "on" | "true" | "yes") => Some(true),
        _ => None,
    }
}

// ====================================================================== primitives

/// Milliseconds with microsecond resolution, for JSON.
pub fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1_000_000.0).round() / 1000.0
}

fn ms_u64(ns: u64) -> f64 {
    ms(Duration::from_nanos(ns))
}

/// Bytes per second over `d`, or `None` for an empty or invalid interval.
pub fn rate(bytes: u64, d: Duration) -> Option<f64> {
    let s = d.as_secs_f64();
    if s <= 0.0 || !s.is_finite() {
        return None;
    }
    Some(bytes as f64 / s)
}

pub fn to_mib(bps: Option<f64>) -> Option<f64> {
    bps.map(|b| (b / (1024.0 * 1024.0) * 1000.0).round() / 1000.0)
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn dur_ns(d: Duration) -> u64 {
    d.as_nanos().min(u64::MAX as u128) as u64
}

/// Accumulated time spent in one stage, with event count, bytes and max.
#[derive(Default, Debug)]
pub struct Stage {
    ns: AtomicU64,
    count: AtomicU64,
    bytes: AtomicU64,
    max_ns: AtomicU64,
}

impl Stage {
    pub fn record(&self, d: Duration) {
        self.record_bytes(d, 0);
    }

    pub fn record_bytes(&self, d: Duration, bytes: u64) {
        let ns = dur_ns(d);
        self.ns.fetch_add(ns, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
    }

    pub fn total(&self) -> Duration {
        Duration::from_nanos(self.ns.load(Ordering::Relaxed))
    }

    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    pub fn json(&self) -> Value {
        let count = self.count();
        let total = self.total();
        let bytes = self.bytes();
        let mut v = json!({
            "total_ms": ms(total),
            "count": count,
            "max_ms": if count > 0 { json!(ms_u64(self.max_ns.load(Ordering::Relaxed))) } else { Value::Null },
            "avg_ms": if count > 0 { json!(ms(total / count.min(u32::MAX as u64) as u32)) } else { Value::Null },
        });
        if bytes > 0 {
            v["bytes"] = json!(bytes);
            // Bytes divided by the time spent in this stage only.
            v["stage_mib_per_sec"] = json!(to_mib(rate(bytes, total)));
        }
        v
    }
}

/// Times a section and records it into a stage when dropped or finished.
pub fn timed<T>(stage: &Stage, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    let d = t.elapsed();
    stage.record(d);
    (r, d)
}

/// Current, peak and time-weighted average of a count (active streams).
#[derive(Debug)]
pub struct Gauge {
    st: Mutex<GaugeState>,
}

#[derive(Debug)]
struct GaugeState {
    cur: u64,
    peak: u64,
    /// Integral of `cur` over time, in count x seconds.
    area: f64,
    /// Seconds with `cur > 0`.
    busy: f64,
    last: Instant,
    start: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GaugeSnapshot {
    pub current: u64,
    pub peak: u64,
    /// Time-weighted average since the gauge was created; `None` before any time passed.
    pub average: Option<f64>,
    /// Integral of the count over time (count x seconds), e.g. stream-seconds.
    pub count_seconds: f64,
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

impl Gauge {
    pub fn new() -> Self {
        let now = Instant::now();
        Gauge { st: Mutex::new(GaugeState { cur: 0, peak: 0, area: 0.0, busy: 0.0, last: now, start: now }) }
    }

    fn update(&self, delta: i64) -> u64 {
        let mut s = self.st.lock();
        let now = Instant::now();
        let dt = now.duration_since(s.last).as_secs_f64();
        s.area += s.cur as f64 * dt;
        if s.cur > 0 {
            s.busy += dt;
        }
        s.last = now;
        s.cur = if delta >= 0 { s.cur + delta as u64 } else { s.cur.saturating_sub((-delta) as u64) };
        s.peak = s.peak.max(s.cur);
        s.cur
    }

    /// Increments and returns a guard that decrements on drop.
    pub fn enter(&self) -> GaugeGuard<'_> {
        self.update(1);
        GaugeGuard(self)
    }

    pub fn current(&self) -> u64 {
        self.st.lock().cur
    }

    /// Total time with at least one active entry.
    pub fn busy(&self) -> Duration {
        let s = self.st.lock();
        let extra = if s.cur > 0 { s.last.elapsed().as_secs_f64() } else { 0.0 };
        Duration::from_secs_f64(s.busy + extra)
    }

    pub fn snapshot(&self) -> GaugeSnapshot {
        let s = self.st.lock();
        let now = Instant::now();
        let area = s.area + s.cur as f64 * now.duration_since(s.last).as_secs_f64();
        let span = now.duration_since(s.start).as_secs_f64();
        GaugeSnapshot {
            current: s.cur,
            peak: s.peak,
            average: if span > 0.0 { Some((area / span * 100.0).round() / 100.0) } else { None },
            count_seconds: (area * 1000.0).round() / 1000.0,
        }
    }
}

pub struct GaugeGuard<'a>(&'a Gauge);

impl Drop for GaugeGuard<'_> {
    fn drop(&mut self) {
        self.0.update(-1);
    }
}

// ====================================================================== labels

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    Direct,
    Relay,
    Unknown,
}

impl Route {
    pub fn as_str(&self) -> &'static str {
        match self {
            Route::Direct => "direct",
            Route::Relay => "relay",
            Route::Unknown => "unknown",
        }
    }
}

/// Maps `RouteInfo::kind` ("direct", "relay", "none", "other") to a route.
/// Anything that is not positively direct or relay is unknown.
pub fn route_label(kind: &str) -> Route {
    match kind {
        "direct" => Route::Direct,
        "relay" => Route::Relay,
        _ => Route::Unknown,
    }
}

/// Diagnostic error categories. These do not change user-facing errors.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ErrCat {
    ConnectFailed,
    ConnectionLost,
    Denied,
    RouteUnknown,
    RelayPathActive,
    StreamOpenFailed,
    RequestWriteFailed,
    ResponseTimeout,
    StallTimeout,
    StreamReset,
    StreamClosedEarly,
    PeerStopped,
    SenderDiskRead,
    ReceiverDiskWrite,
    HashMismatch,
    SourceChanged,
    RemoteError,
    BadRequest,
    Cancelled,
    Unknown,
}

impl ErrCat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrCat::ConnectFailed => "connect_failed",
            ErrCat::ConnectionLost => "connection_lost",
            ErrCat::Denied => "denied",
            ErrCat::RouteUnknown => "route_unknown",
            ErrCat::RelayPathActive => "relay_path_active",
            ErrCat::StreamOpenFailed => "stream_open_failed",
            ErrCat::RequestWriteFailed => "request_write_failed",
            ErrCat::ResponseTimeout => "response_timeout",
            ErrCat::StallTimeout => "stall_timeout",
            ErrCat::StreamReset => "stream_reset",
            ErrCat::StreamClosedEarly => "stream_closed_early",
            ErrCat::PeerStopped => "peer_stopped",
            ErrCat::SenderDiskRead => "sender_disk_read",
            ErrCat::ReceiverDiskWrite => "receiver_disk_write",
            ErrCat::HashMismatch => "hash_mismatch",
            ErrCat::SourceChanged => "source_changed",
            ErrCat::RemoteError => "remote_error",
            ErrCat::BadRequest => "bad_request",
            ErrCat::Cancelled => "cancelled",
            ErrCat::Unknown => "unknown",
        }
    }
}

/// Classifies a QUIC read error.
pub fn classify_read(e: &iroh::endpoint::ReadError) -> ErrCat {
    use iroh::endpoint::ReadError as R;
    match e {
        R::Reset(_) => ErrCat::StreamReset,
        R::ConnectionLost(_) => ErrCat::ConnectionLost,
        R::ClosedStream => ErrCat::StreamClosedEarly,
        _ => ErrCat::Unknown,
    }
}

pub fn classify_read_exact(e: &iroh::endpoint::ReadExactError) -> ErrCat {
    use iroh::endpoint::ReadExactError as R;
    match e {
        R::FinishedEarly(_) => ErrCat::StreamClosedEarly,
        R::ReadError(r) => classify_read(r),
    }
}

pub fn classify_write(e: &iroh::endpoint::WriteError) -> ErrCat {
    use iroh::endpoint::WriteError as W;
    match e {
        W::Stopped(_) => ErrCat::PeerStopped,
        W::ConnectionLost(_) => ErrCat::ConnectionLost,
        W::ClosedStream => ErrCat::StreamClosedEarly,
        _ => ErrCat::Unknown,
    }
}

/// Classifies an error chain produced by `anyhow` around QUIC calls.
pub fn classify_anyhow(e: &anyhow::Error) -> ErrCat {
    for c in e.chain() {
        if let Some(w) = c.downcast_ref::<iroh::endpoint::WriteError>() {
            return classify_write(w);
        }
        if let Some(r) = c.downcast_ref::<iroh::endpoint::ReadExactError>() {
            return classify_read_exact(r);
        }
        if let Some(r) = c.downcast_ref::<iroh::endpoint::ReadError>() {
            return classify_read(r);
        }
        if c.downcast_ref::<iroh::endpoint::ConnectionError>().is_some() {
            return ErrCat::ConnectionLost;
        }
    }
    ErrCat::Unknown
}

/// Stable, privacy-safe identifier of a shared file, computed the same way on
/// sender and receiver so their summaries can be matched.
pub fn file_key(share_id: &str, path: &[String]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"relayless-diag-file-v1\0");
    h.update(share_id.as_bytes());
    for c in path {
        h.update(b"\0");
        h.update(c.as_bytes());
    }
    h.finalize().to_hex()[..16].to_string()
}

pub fn file_ext(path: &[String]) -> String {
    path.last().map(|n| crate::receiver::ext_of(n)).unwrap_or_default()
}

/// Rate limit as the user configured it and as the engine applies it.
pub fn limit_json(configured_kib_per_sec: u64, effective_bytes_per_sec: u64) -> Value {
    let unlimited = effective_bytes_per_sec == 0;
    json!({
        "configured_value": configured_kib_per_sec,
        "configured_unit": "KiB/s",
        "effective_bytes_per_sec": if unlimited { Value::Null } else { json!(effective_bytes_per_sec) },
        "effective_mib_per_sec": if unlimited { Value::Null } else { json!(to_mib(Some(effective_bytes_per_sec as f64))) },
        "unlimited": unlimited,
        "zero_means_unlimited": true,
    })
}

// ====================================================================== per-transfer state

#[derive(Serialize, Clone, Debug)]
pub struct RouteChange {
    pub at_ms: f64,
    pub from: Route,
    pub to: Route,
    pub rtt_ms: Option<u64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConnAttempt {
    pub at_ms: f64,
    pub reused: bool,
    /// Time waiting for the per-peer connection lock (another job dialing).
    pub lock_wait_ms: f64,
    pub dial_ms: Option<f64>,
    pub hello_ms: Option<f64>,
    pub total_ms: f64,
    pub ok: bool,
    pub error: Option<ErrCat>,
}

#[derive(Serialize, Clone, Debug)]
pub struct HolePunch {
    pub wait_ms: f64,
    /// "already_direct", "direct_found", "timeout", "cancelled", "not_applicable_direct_only"
    pub outcome: &'static str,
}

/// One chunk request as seen by one side.
#[derive(Serialize, Clone, Debug, Default)]
pub struct ChunkRecord {
    pub index: u64,
    pub worker: Option<u32>,
    pub start_ms: f64,
    pub from_bytes: u64,
    pub bytes: u64,
    pub blocks: u32,
    pub route: Option<Route>,
    pub stream_open_ms: Option<f64>,
    pub request_write_ms: Option<f64>,
    pub header_wait_ms: Option<f64>,
    pub limiter_wait_ms: f64,
    /// Receiver: waiting in `recv.read` (network and sender pace, plus task scheduling).
    pub read_wait_ms: Option<f64>,
    /// Sender: `read_exact_at` on the source file.
    pub disk_read_ms: Option<f64>,
    pub hash_ms: f64,
    /// Receiver: `write_all_at` on the partial file.
    pub disk_write_ms: Option<f64>,
    /// Sender: blocked in `write_all` (QUIC flow control, congestion and buffer backpressure).
    pub quic_write_ms: Option<f64>,
    /// Waiting for a blocking-pool thread (spawn_blocking overhead).
    pub blocking_queue_ms: f64,
    pub total_ms: f64,
    pub hash_failures: u32,
    pub outcome: &'static str,
    pub error: Option<ErrCat>,
}

/// Per-chunk accumulator kept on the worker's stack.
pub struct ChunkTrace {
    pub start: Instant,
    pub rec: ChunkRecord,
    limiter: Duration,
    read_wait: Duration,
    disk_read: Duration,
    hash: Duration,
    disk_write: Duration,
    quic_write: Duration,
    blocking_queue: Duration,
}

impl ChunkTrace {
    pub fn new(origin: Instant, index: u64, worker: Option<u32>, from: u64, route: Option<Route>) -> Self {
        let start = Instant::now();
        ChunkTrace {
            start,
            rec: ChunkRecord {
                index,
                worker,
                start_ms: ms(start.duration_since(origin)),
                from_bytes: from,
                route,
                outcome: "running",
                ..Default::default()
            },
            limiter: Duration::ZERO,
            read_wait: Duration::ZERO,
            disk_read: Duration::ZERO,
            hash: Duration::ZERO,
            disk_write: Duration::ZERO,
            quic_write: Duration::ZERO,
            blocking_queue: Duration::ZERO,
        }
    }
    pub fn limiter(&mut self, d: Duration) {
        self.limiter += d;
    }
    pub fn read_wait(&mut self, d: Duration) {
        self.read_wait += d;
    }
    pub fn disk_read(&mut self, d: Duration) {
        self.disk_read += d;
    }
    pub fn hash(&mut self, d: Duration) {
        self.hash += d;
    }
    pub fn disk_write(&mut self, d: Duration) {
        self.disk_write += d;
    }
    pub fn quic_write(&mut self, d: Duration) {
        self.quic_write += d;
    }
    pub fn blocking_queue(&mut self, d: Duration) {
        self.blocking_queue += d;
    }
    pub fn block(&mut self, bytes: u64) {
        self.rec.bytes += bytes;
        self.rec.blocks += 1;
    }

    /// Closes the record. `receiver` selects which stage fields apply.
    pub fn finish(mut self, outcome: &'static str, error: Option<ErrCat>, receiver: bool) -> ChunkRecord {
        self.rec.total_ms = ms(self.start.elapsed());
        self.rec.limiter_wait_ms = ms(self.limiter);
        self.rec.hash_ms = ms(self.hash);
        self.rec.blocking_queue_ms = ms(self.blocking_queue);
        if receiver {
            self.rec.read_wait_ms = Some(ms(self.read_wait));
            self.rec.disk_write_ms = Some(ms(self.disk_write));
        } else {
            self.rec.disk_read_ms = Some(ms(self.disk_read));
            self.rec.quic_write_ms = Some(ms(self.quic_write));
        }
        self.rec.outcome = outcome;
        self.rec.error = error;
        self.rec
    }
}

/// Stage timers shared by both roles.
#[derive(Default, Debug)]
pub struct Stages {
    // receiver
    pub slot_wait: Stage,
    pub conn_setup: Stage,
    pub hole_punch_wait: Stage,
    pub direct_only_wait: Stage,
    pub backoff_wait: Stage,
    pub stat_request: Stage,
    pub prepare_partial: Stage,
    pub stream_open: Stage,
    pub request_write: Stage,
    pub header_wait: Stage,
    pub read_wait: Stage,
    pub disk_write: Stage,
    pub fsync: Stage,
    pub db_commit: Stage,
    pub final_read: Stage,
    pub final_hash: Stage,
    pub final_verification: Stage,
    pub rename: Stage,
    // sender
    pub request_read: Stage,
    pub file_open: Stage,
    pub disk_read: Stage,
    pub metadata_check: Stage,
    pub quic_write: Stage,
    pub stream_finish_wait: Stage,
    // both
    pub limiter_wait: Stage,
    pub hash: Stage,
    pub blocking_queue: Stage,
    /// Time each worker (receiver) or chunk handler (sender) was alive.
    pub worker_time: Stage,
}

impl Stages {
    fn json(&self, receiver: bool) -> Value {
        if receiver {
            json!({
                "slot_wait": self.slot_wait.json(),
                "connection_setup": self.conn_setup.json(),
                "hole_punch_wait": self.hole_punch_wait.json(),
                "direct_only_wait": self.direct_only_wait.json(),
                "backoff_wait": self.backoff_wait.json(),
                "stat_request": self.stat_request.json(),
                "prepare_partial": self.prepare_partial.json(),
                "stream_open": self.stream_open.json(),
                "request_write": self.request_write.json(),
                "response_header_wait": self.header_wait.json(),
                "rate_limiter_wait": self.limiter_wait.json(),
                "quic_read_wait": self.read_wait.json(),
                "hash_verify": self.hash.json(),
                "disk_write": self.disk_write.json(),
                "blocking_pool_queue": self.blocking_queue.json(),
                "sync_flush": self.fsync.json(),
                "db_commit": self.db_commit.json(),
                "final_verification": self.final_verification.json(),
                "final_verification_read": self.final_read.json(),
                "final_verification_hash": self.final_hash.json(),
                "rename": self.rename.json(),
                "worker_time": self.worker_time.json(),
            })
        } else {
            json!({
                "request_read": self.request_read.json(),
                "file_open": self.file_open.json(),
                "rate_limiter_wait": self.limiter_wait.json(),
                "disk_read": self.disk_read.json(),
                "metadata_check": self.metadata_check.json(),
                "hash": self.hash.json(),
                "blocking_pool_queue": self.blocking_queue.json(),
                "quic_write_blocked": self.quic_write.json(),
                "stream_finish_wait": self.stream_finish_wait.json(),
                "worker_time": self.worker_time.json(),
            })
        }
    }

    /// Compact view for log lines and interval samples.
    fn totals_ms(&self) -> BTreeMap<&'static str, f64> {
        let mut m = BTreeMap::new();
        for (k, s) in [
            ("limiter", &self.limiter_wait),
            ("read_wait", &self.read_wait),
            ("disk_read", &self.disk_read),
            ("disk_write", &self.disk_write),
            ("hash", &self.hash),
            ("quic_write", &self.quic_write),
            ("fsync", &self.fsync),
        ] {
            m.insert(k, ms(s.total()));
        }
        m
    }
}

#[derive(Default, Debug)]
pub struct Counters {
    /// Verified block bytes this run (receiver) or block bytes written to streams (sender).
    pub bytes: AtomicU64,
    /// Payload bytes read from streams, including blocks later discarded (receiver).
    pub net_bytes: AtomicU64,
    pub direct_bytes: AtomicU64,
    pub relay_bytes: AtomicU64,
    pub unknown_route_bytes: AtomicU64,
    pub blocks: AtomicU64,
    pub chunks_started: AtomicU64,
    pub chunks_completed: AtomicU64,
    pub chunks_failed: AtomicU64,
    pub chunks_stopped: AtomicU64,
    pub chunks_resumed: AtomicU64,
    pub chunk_retries: AtomicU64,
    pub hash_failures: AtomicU64,
    pub passes: AtomicU64,
    pub workers_started: AtomicU64,
    pub slow_stream_opens: AtomicU64,
    pub requests: AtomicU64,
    pub limiter_waits: AtomicU64,
    /// Sender: most requests seen in flight on the connection when a chunk started.
    pub conn_streams_peak: AtomicU64,
}

impl Counters {
    pub fn add_route_bytes(&self, route: Route, n: u64) {
        match route {
            Route::Direct => self.direct_bytes.fetch_add(n, Ordering::Relaxed),
            Route::Relay => self.relay_bytes.fetch_add(n, Ordering::Relaxed),
            Route::Unknown => self.unknown_route_bytes.fetch_add(n, Ordering::Relaxed),
        };
    }
}

#[derive(Default)]
struct DiagState {
    route_initial: Option<Route>,
    route_current: Option<Route>,
    route_changes: Vec<RouteChange>,
    route_changes_dropped: u64,
    conn_attempts: Vec<ConnAttempt>,
    hole_punch: Option<HolePunch>,
    errors: BTreeMap<ErrCat, u64>,
    last_error: Option<ErrCat>,
    samples: VecDeque<Value>,
    samples_dropped: u64,
    peak_interval_bps: Option<f64>,
    chunks: Vec<ChunkRecord>,
    chunks_dropped: u64,
    status: Option<(&'static str, Instant)>,
    status_ms: BTreeMap<&'static str, f64>,
    notes: Vec<String>,
    conn: Option<iroh::endpoint::Connection>,
    conn_start: Option<Value>,
    configuration: Option<Value>,
    finished: bool,
}

/// Diagnostics for one transfer: a receiver job run, or a sender serving one
/// file to one receiver.
pub struct TransferDiag {
    pub id: String,
    pub role: &'static str,
    pub file_key: String,
    pub file_ext: String,
    pub peer: String,
    pub size: u64,
    pub origin: Instant,
    pub started_at_ms: i64,
    pub stages: Stages,
    pub c: Counters,
    /// Streams active for this transfer.
    pub streams: Gauge,
    last_activity: Mutex<Instant>,
    st: Mutex<DiagState>,
    /// Counters at the previous `sample_now` call.
    base: Mutex<Option<SampleBase>>,
}

impl TransferDiag {
    pub fn new(id: String, role: &'static str, file_key: String, file_ext: String, peer: String, size: u64) -> Self {
        let now = Instant::now();
        TransferDiag {
            id,
            role,
            file_key,
            file_ext,
            peer,
            size,
            origin: now,
            started_at_ms: crate::util::now_ms(),
            stages: Stages::default(),
            c: Counters::default(),
            streams: Gauge::new(),
            last_activity: Mutex::new(now),
            st: Mutex::new(DiagState::default()),
            base: Mutex::new(None),
        }
    }

    /// Interval sample against the previous call (the first call covers the
    /// time since the transfer started).
    pub fn sample_now(&self, extra: Value) -> Value {
        let mut b = self.base.lock();
        let base = b.get_or_insert_with(|| SampleBase {
            at: self.origin,
            net_bytes: 0,
            limiter: Duration::ZERO,
            read_wait: Duration::ZERO,
            quic_write: Duration::ZERO,
        });
        self.sample(base, extra)
    }

    pub fn is_receiver(&self) -> bool {
        self.role == "receiver"
    }

    pub fn elapsed(&self) -> Duration {
        self.origin.elapsed()
    }

    pub fn at_ms(&self) -> f64 {
        ms(self.origin.elapsed())
    }

    pub fn touch(&self) {
        *self.last_activity.lock() = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        self.last_activity.lock().elapsed()
    }

    pub fn error(&self, cat: ErrCat) {
        let mut s = self.st.lock();
        *s.errors.entry(cat).or_default() += 1;
        s.last_error = Some(cat);
    }

    pub fn last_error(&self) -> Option<ErrCat> {
        self.st.lock().last_error
    }

    pub fn error_count(&self, cat: ErrCat) -> u64 {
        self.st.lock().errors.get(&cat).copied().unwrap_or(0)
    }

    pub fn note(&self, n: impl Into<String>) {
        let mut s = self.st.lock();
        if s.notes.len() < 50 {
            s.notes.push(n.into());
        }
    }

    /// Records the route; returns true if it changed.
    pub fn observe_route(&self, route: Route, rtt_ms: Option<u64>) -> bool {
        let mut s = self.st.lock();
        if s.route_initial.is_none() {
            s.route_initial = Some(route);
            s.route_current = Some(route);
            return false;
        }
        let from = s.route_current.unwrap_or(Route::Unknown);
        if from == route {
            return false;
        }
        s.route_current = Some(route);
        let at_ms = ms(self.origin.elapsed());
        if s.route_changes.len() < MAX_ROUTE_CHANGES {
            s.route_changes.push(RouteChange { at_ms, from, to: route, rtt_ms });
        } else {
            s.route_changes_dropped += 1;
        }
        true
    }

    pub fn route_current(&self) -> Option<Route> {
        self.st.lock().route_current
    }

    pub fn conn_attempt(&self, a: ConnAttempt) {
        let mut s = self.st.lock();
        if s.conn_attempts.len() < MAX_CONN_ATTEMPTS {
            s.conn_attempts.push(a);
        }
    }

    pub fn hole_punch(&self, h: HolePunch) {
        let mut s = self.st.lock();
        if s.hole_punch.is_none() {
            s.hole_punch = Some(h);
        }
    }

    /// Tracks time per job status (receiver).
    pub fn on_status(&self, status: &'static str) {
        let mut s = self.st.lock();
        let now = Instant::now();
        if let Some((prev, since)) = s.status.take() {
            *s.status_ms.entry(prev).or_default() += ms(now.duration_since(since));
        }
        s.status = Some((status, now));
    }

    pub fn status_time(&self, status: &str) -> Duration {
        let s = self.st.lock();
        let mut total = s.status_ms.get(status).copied().unwrap_or(0.0);
        if let Some((cur, since)) = s.status {
            if cur == status {
                total += ms(since.elapsed());
            }
        }
        Duration::from_secs_f64(total / 1000.0)
    }

    pub fn set_connection(&self, conn: &iroh::endpoint::Connection) {
        let mut s = self.st.lock();
        if s.conn_start.is_none() {
            s.conn_start = Some(crate::net::path_report(conn));
        }
        s.conn = Some(conn.clone());
    }

    pub fn connection(&self) -> Option<iroh::endpoint::Connection> {
        self.st.lock().conn.clone()
    }

    pub fn set_configuration(&self, v: Value) {
        self.st.lock().configuration = Some(v);
    }

    pub fn has_configuration(&self) -> bool {
        self.st.lock().configuration.is_some()
    }

    pub fn finish_chunk(&self, rec: ChunkRecord) {
        tracing::debug!(
            target: "relayless_core::diag",
            "chunk role={} id={} index={} outcome={} bytes={} blocks={} total_ms={:.1} header_ms={:?} limiter_ms={:.1} read_wait_ms={:?} disk_read_ms={:?} hash_ms={:.1} disk_write_ms={:?} quic_write_ms={:?} error={:?}",
            self.role, self.id, rec.index, rec.outcome, rec.bytes, rec.blocks, rec.total_ms, rec.header_wait_ms,
            rec.limiter_wait_ms, rec.read_wait_ms, rec.disk_read_ms, rec.hash_ms, rec.disk_write_ms, rec.quic_write_ms,
            rec.error.map(|e| e.as_str())
        );
        if !self.is_receiver() {
            // The sender has no long-lived workers: a chunk handler's lifetime is its worker time.
            self.stages.worker_time.record(Duration::from_secs_f64(rec.total_ms / 1000.0));
        }
        match rec.outcome {
            "completed" => self.c.chunks_completed.fetch_add(1, Ordering::Relaxed),
            "stopped" => self.c.chunks_stopped.fetch_add(1, Ordering::Relaxed),
            _ => self.c.chunks_failed.fetch_add(1, Ordering::Relaxed),
        };
        let mut s = self.st.lock();
        if s.chunks.len() < MAX_CHUNK_RECORDS {
            s.chunks.push(rec);
        } else {
            s.chunks_dropped += 1;
        }
    }

    /// Takes an interval sample. `prev` holds the previous sample's counters.
    pub fn sample(&self, prev: &mut SampleBase, extra: Value) -> Value {
        let now = Instant::now();
        let dt = now.duration_since(prev.at);
        let net = if self.is_receiver() { self.c.net_bytes.load(Ordering::Relaxed) } else { self.c.bytes.load(Ordering::Relaxed) };
        let limiter = self.stages.limiter_wait.total();
        let read_wait = self.stages.read_wait.total();
        let quic_write = self.stages.quic_write.total();
        let d_bytes = net.saturating_sub(prev.net_bytes);
        let bps = rate(d_bytes, dt);
        let streams = self.streams.snapshot();
        let mut v = json!({
            "t_ms": ms(now.duration_since(self.origin)),
            "interval_ms": ms(dt),
            "bytes": d_bytes,
            "interval_avg_bytes_per_sec": bps.map(round1),
            "interval_avg_mib_per_sec": to_mib(bps),
            "active_streams": streams.current,
            "route": self.route_current().unwrap_or(Route::Unknown),
            "limiter_wait_ms": ms(limiter.saturating_sub(prev.limiter)),
            "read_wait_ms": ms(read_wait.saturating_sub(prev.read_wait)),
            "quic_write_ms": ms(quic_write.saturating_sub(prev.quic_write)),
        });
        if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) {
            m.extend(e);
        }
        prev.at = now;
        prev.net_bytes = net;
        prev.limiter = limiter;
        prev.read_wait = read_wait;
        prev.quic_write = quic_write;
        let mut s = self.st.lock();
        if dt >= Duration::from_millis(500) {
            if let Some(b) = bps {
                if s.peak_interval_bps.is_none_or(|p| b > p) {
                    s.peak_interval_bps = Some(b);
                }
            }
        }
        if s.samples.len() >= MAX_SAMPLES {
            s.samples.pop_front();
            s.samples_dropped += 1;
        }
        s.samples.push_back(v.clone());
        v
    }

    /// Builds the JSON summary. Can be called once; later calls return `None`.
    pub fn summary(&self, outcome: &str, extra: SummaryExtra) -> Option<Value> {
        let receiver = self.is_receiver();
        let elapsed = self.origin.elapsed();
        let (route_block, chunks, samples, errors, notes_in, conn_attempts, hole_punch, status_ms, conn, conn_start, cfg, peak_interval, last_error) = {
            let mut s = self.st.lock();
            if s.finished {
                return None;
            }
            s.finished = true;
            if let Some((prev, since)) = s.status.take() {
                *s.status_ms.entry(prev).or_default() += ms(since.elapsed());
            }
            let route_block = json!({
                "initial": s.route_initial.unwrap_or(Route::Unknown),
                "final": s.route_current.unwrap_or(Route::Unknown),
                "changes": s.route_changes.len() as u64 + s.route_changes_dropped,
                "change_log": s.route_changes,
            });
            (
                route_block,
                std::mem::take(&mut s.chunks),
                std::mem::take(&mut s.samples),
                s.errors.iter().map(|(k, v)| (k.as_str(), *v)).collect::<BTreeMap<_, _>>(),
                std::mem::take(&mut s.notes),
                std::mem::take(&mut s.conn_attempts),
                s.hole_punch.clone(),
                s.status_ms.clone(),
                s.conn.take(),
                s.conn_start.take(),
                s.configuration.take(),
                s.peak_interval_bps,
                s.last_error,
            )
        };
        let chunks_dropped = self.st.lock().chunks_dropped;
        let bytes = self.c.bytes.load(Ordering::Relaxed);
        let streams = self.streams.snapshot();
        let downloading = Duration::from_secs_f64(status_ms.get("downloading").copied().unwrap_or(0.0) / 1000.0);
        let avg_elapsed = rate(bytes, elapsed);
        let active_window = if receiver { downloading } else { extra.active_time.unwrap_or(Duration::ZERO) };
        let avg_active = rate(bytes, active_window);

        let mut route = route_block;
        route["direct_only"] = json!(extra.direct_only);
        route["hole_punch"] = json!(hole_punch);
        route["bytes_by_route"] = json!({
            "direct": self.c.direct_bytes.load(Ordering::Relaxed),
            "relay": self.c.relay_bytes.load(Ordering::Relaxed),
            "unknown": self.c.unknown_route_bytes.load(Ordering::Relaxed),
            "sampling": if receiver {
                "cached route (updated from the path API on every iroh path event) when each 1 MiB block was verified; samples[].route_cache_matches cross-checks it"
            } else {
                "selected path from the path API when each 1 MiB block was written"
            },
        });

        let (conn_end, conn_close) = match &conn {
            Some(c) => (Some(crate::net::path_report(c)), c.close_reason().map(|r| r.to_string().chars().take(160).collect::<String>())),
            None => (None, None),
        };
        let rtt_ms = conn_end
            .as_ref()
            .and_then(|r| r.get("selected_rtt_ms"))
            .and_then(|v| v.as_f64());
        let ceilings = rtt_ms.filter(|r| *r > 0.0).map(|r| {
            let rtt = r / 1000.0;
            let per_stream = crate::net::STREAM_WINDOW as f64 / rtt;
            let avg_streams = streams.average.unwrap_or(0.0).max(0.0);
            let aggregate = (avg_streams * crate::net::STREAM_WINDOW as f64).min(crate::net::CONN_WINDOW as f64) / rtt;
            json!({
                "kind": "theoretical flow-control ceiling (window / RTT), not a throughput prediction",
                "rtt_ms_used": r,
                "per_stream_bytes_per_sec": round1(per_stream),
                "aggregate_bytes_per_sec_at_avg_streams": round1(aggregate),
                "aggregate_mib_per_sec_at_avg_streams": to_mib(Some(aggregate)),
            })
        });

        let mut notes = auto_notes(self, &extra, elapsed, downloading, &streams, avg_active);
        if let Some(n) = conn_end.as_ref().and_then(|r| congestion_note(self.is_receiver(), r)) {
            notes.push(n);
        }
        notes.extend(notes_in);

        let mut transfer = json!({
            "file_size_bytes": self.size,
            "bytes_transferred": bytes,
            "elapsed_ms": ms(elapsed),
            "rates": {
                "whole_run_average_bytes_per_sec": avg_elapsed.map(round1),
                "whole_run_average_mib_per_sec": to_mib(avg_elapsed),
                "active_average_bytes_per_sec": avg_active.map(round1),
                "active_average_mib_per_sec": to_mib(avg_active),
                "active_window_ms": ms(active_window),
                "active_window": if receiver { "time in the downloading state" } else { "time with at least one chunk stream being served" },
                "peak_interval_average_bytes_per_sec": peak_interval.map(round1),
                "peak_interval_average_mib_per_sec": to_mib(peak_interval),
                "interval_ms": ms(SAMPLE_INTERVAL),
            },
            "blocks": self.c.blocks.load(Ordering::Relaxed),
            "chunks_started": self.c.chunks_started.load(Ordering::Relaxed),
            "chunks_completed": self.c.chunks_completed.load(Ordering::Relaxed),
            "chunks_failed": self.c.chunks_failed.load(Ordering::Relaxed),
            "chunks_stopped": self.c.chunks_stopped.load(Ordering::Relaxed),
            "chunks_resumed_mid_chunk": self.c.chunks_resumed.load(Ordering::Relaxed),
            "chunk_retries": self.c.chunk_retries.load(Ordering::Relaxed),
            "hash_failures": self.c.hash_failures.load(Ordering::Relaxed),
            "active_streams": streams,
            // Stream-seconds divided by the active window (downloading time on
            // the receiver, time with any stream on the sender).
            "average_active_streams_in_active_window": if active_window > Duration::ZERO {
                json!(((streams.count_seconds / active_window.as_secs_f64()) * 100.0).round() / 100.0)
            } else {
                Value::Null
            },
            "outcome": outcome,
            // Most recent error category; null when the transfer succeeded
            // (errors that were retried are still listed in `errors`).
            "error_category": if matches!(outcome, "completed" | "receipt_received") { None } else { last_error },
        });
        if receiver {
            transfer["network_payload_bytes"] = json!(self.c.net_bytes.load(Ordering::Relaxed));
            transfer["passes"] = json!(self.c.passes.load(Ordering::Relaxed));
            transfer["workers_started"] = json!(self.c.workers_started.load(Ordering::Relaxed));
            transfer["slow_stream_opens"] = json!(self.c.slow_stream_opens.load(Ordering::Relaxed));
            transfer["verified_bytes_at_start"] = json!(extra.resumed_from);
            transfer["status_time_ms"] = json!(status_ms);
        } else {
            transfer["chunk_requests"] = json!(self.c.requests.load(Ordering::Relaxed));
            transfer["connection_requests_in_flight_peak"] = json!(self.c.conn_streams_peak.load(Ordering::Relaxed));
            transfer["connection_stream_limit"] = json!(crate::net::MAX_BIDI_STREAMS);
        }
        for (k, v) in extra.transfer_extra {
            transfer[k] = v;
        }

        let summary = json!({
            "schema_version": SCHEMA_VERSION,
            "kind": "transfer_summary",
            "transfer_id": self.id,
            "role": self.role,
            "file_key": self.file_key,
            "file_ext": self.file_ext,
            "peer": self.peer,
            "started_at_ms": self.started_at_ms,
            "finished_at_ms": crate::util::now_ms(),
            "engine_version": env!("CARGO_PKG_VERSION"),
            "route": route,
            "configuration": cfg,
            "connection": {
                "attempts": conn_attempts,
                "at_start": conn_start,
                "at_end": conn_end,
                "close_reason": conn_close,
                "scope_note": "path and connection statistics cover the whole QUIC connection, which all transfers with this peer share",
                "direction_note": "cwnd, bandwidth_estimate, sent_* and lost_* describe packets THIS side sends. File data flows sender to receiver, so data-packet loss and the data path's congestion window are in the sender's summary; the receiver's values cover ACKs and requests",
            },
            "transfer": transfer,
            "timings_ms": self.stages.json(receiver),
            "flow_control_ceiling": ceilings,
            "errors": errors,
            "samples": samples,
            "chunks": chunks,
            "chunk_records_dropped": chunks_dropped,
            "unavailable": unavailable_metrics(),
            "diagnostic_notes": notes,
        });
        Some(summary)
    }

    /// One readable line for the log.
    pub fn log_line(&self, summary: &Value) -> String {
        let g = |p: &str| summary.pointer(p).cloned().unwrap_or(Value::Null);
        let secs = |p: &str| g(p).as_f64().map(|v| format!("{:.2}", v / 1000.0)).unwrap_or_else(|| "null".into());
        let t = self.stages.totals_ms();
        let s = |k: &str| t.get(k).map(|v| format!("{:.2}", v / 1000.0)).unwrap_or_default();
        format!(
            "transfer_summary role={} id={} file={} peer={} outcome={} route={}->{} route_changes={} direct_only={} elapsed_s={} bytes={} avg_mib_s={} active_avg_mib_s={} peak_interval_mib_s={} streams_peak={} streams_avg={} chunks_ok={} chunks_failed={} hash_failures={} limiter_wait_s={} quic_read_wait_s={} quic_write_s={} disk_read_s={} disk_write_s={} hash_s={} fsync_s={} final_verify_s={} rtt_ms={} lost_packets={}",
            self.role,
            self.id,
            self.file_key,
            self.peer,
            g("/transfer/outcome").as_str().unwrap_or("?"),
            g("/route/initial").as_str().unwrap_or("unknown"),
            g("/route/final").as_str().unwrap_or("unknown"),
            g("/route/changes"),
            g("/route/direct_only"),
            secs("/transfer/elapsed_ms"),
            g("/transfer/bytes_transferred"),
            g("/transfer/rates/whole_run_average_mib_per_sec"),
            g("/transfer/rates/active_average_mib_per_sec"),
            g("/transfer/rates/peak_interval_average_mib_per_sec"),
            g("/transfer/active_streams/peak"),
            g("/transfer/average_active_streams_in_active_window"),
            g("/transfer/chunks_completed"),
            g("/transfer/chunks_failed"),
            g("/transfer/hash_failures"),
            s("limiter"),
            s("read_wait"),
            s("quic_write"),
            s("disk_read"),
            s("disk_write"),
            s("hash"),
            s("fsync"),
            secs("/timings_ms/final_verification/total_ms"),
            g("/connection/at_end/selected_rtt_ms"),
            g("/connection/at_end/totals/lost_packets"),
        )
    }
}

/// Extra inputs for a summary that live outside the diag struct.
#[derive(Default)]
pub struct SummaryExtra {
    pub direct_only: bool,
    pub resumed_from: Option<u64>,
    pub download_limit_bps: Option<u64>,
    pub upload_limit_bps: Option<u64>,
    pub parallel_streams: Option<u64>,
    pub active_time: Option<Duration>,
    pub transfer_extra: Vec<(&'static str, Value)>,
}

/// Previous counters for interval samples.
pub struct SampleBase {
    pub at: Instant,
    pub net_bytes: u64,
    pub limiter: Duration,
    pub read_wait: Duration,
    pub quic_write: Duration,
}

impl SampleBase {
    pub fn new(d: &TransferDiag) -> Self {
        SampleBase {
            at: Instant::now(),
            net_bytes: if d.is_receiver() { d.c.net_bytes.load(Ordering::Relaxed) } else { d.c.bytes.load(Ordering::Relaxed) },
            limiter: d.stages.limiter_wait.total(),
            read_wait: d.stages.read_wait.total(),
            quic_write: d.stages.quic_write.total(),
        }
    }
}

/// Metrics this build cannot measure, so readers don't mistake absence for zero.
pub fn unavailable_metrics() -> Value {
    json!({
        "stream_flow_control_blocked": "noq-proto 1.4 never sends STREAM_DATA_BLOCKED frames, so per-stream window stalls are not observable; quic_write_blocked on the sender is indirect evidence only",
        "connection_flow_control_blocked": "noq-proto 1.4 never sends DATA_BLOCKED frames, so connection-window stalls are not observable",
        "applied_transport_config": "noq does not expose the negotiated transport parameters; configuration values are the ones passed to the QUIC builder in net.rs",
        "remote_rate_limit": "the other device's rate limit is not sent over the wire; read it from that device's own summary",
        "packet_level_timing": "per-packet timing is not collected; use path statistics (rtt, cwnd, loss) instead",
        "congestion_controller_state": "BBR's phase (Startup, Drain, ProbeBW, ProbeRTT) and app-limited flag are private in noq-proto; only cwnd, pacing rate, send quantum and bandwidth estimate are exposed",
        "destination_disk_type": "not probed",
    })
}

/// Flags a congestion controller that paces slowly without any loss.
/// Only meaningful on the sender (the data direction).
pub fn congestion_note(receiver: bool, report: &Value) -> Option<String> {
    if receiver {
        return None;
    }
    let p = report["paths"].as_array()?.iter().find(|p| p["selected"] == true)?;
    let pacing = p["pacing_rate_bytes_per_sec"].as_u64()?;
    let lost = p["lost_packets"].as_u64().unwrap_or(0);
    let cwnd = p["cwnd_bytes"].as_u64().unwrap_or(0);
    let mtu = p["current_mtu"].as_u64().unwrap_or(1200).max(1);
    if pacing < 256 * 1024 && lost == 0 {
        return Some(format!(
            "observation: at the end, the congestion controller paced the selected path at {pacing} bytes/s with cwnd {cwnd} bytes ({:.1} packets) and 0 lost packets; a rate this low without loss points at the congestion controller's bandwidth estimate, not at the network",
            cwnd as f64 / mtu as f64
        ));
    }
    None
}

fn auto_notes(
    d: &TransferDiag,
    extra: &SummaryExtra,
    elapsed: Duration,
    downloading: Duration,
    streams: &GaugeSnapshot,
    avg_active: Option<f64>,
) -> Vec<String> {
    let mut n = Vec::new();
    let relay = d.c.relay_bytes.load(Ordering::Relaxed);
    let direct = d.c.direct_bytes.load(Ordering::Relaxed);
    let unknown = d.c.unknown_route_bytes.load(Ordering::Relaxed);
    let total = relay + direct + unknown;
    if total > 0 && relay > 0 {
        n.push(format!(
            "observation: {:.0}% of payload bytes were counted while the selected path was a relay",
            relay as f64 * 100.0 / total as f64
        ));
    }
    if total > 0 && unknown > 0 {
        n.push(format!("observation: {unknown} payload bytes arrived while no selected path was reported (route unknown)"));
    }
    let lim = d.stages.limiter_wait.total();
    let worker = d.stages.worker_time.total();
    if lim > Duration::ZERO && worker > Duration::ZERO {
        n.push(format!(
            "observation: rate limiter waits were {:.1}% of worker time ({} waits)",
            lim.as_secs_f64() * 100.0 / worker.as_secs_f64(),
            d.stages.limiter_wait.count()
        ));
    }
    if let Some(l) = if d.is_receiver() { extra.download_limit_bps } else { extra.upload_limit_bps } {
        if l > 0 {
            n.push(format!("observation: this device's {} limit is active at {} bytes/s", if d.is_receiver() { "download" } else { "upload" }, l));
        }
    }
    if d.is_receiver() {
        n.push("note: the sender's upload limit is not visible here; check the sender's summary with the same file_key".into());
        if let Some(p) = extra.parallel_streams {
            if downloading > Duration::ZERO && elapsed > Duration::ZERO && p > 1 {
                let avg = streams.count_seconds / downloading.as_secs_f64();
                if avg < p as f64 * 0.5 {
                    n.push(format!(
                        "observation: average active streams while downloading ({avg:.2}) were under half of the {p} workers started per pass"
                    ));
                }
            }
        }
        let rw = d.stages.read_wait.total();
        if worker > Duration::ZERO && rw.as_secs_f64() > worker.as_secs_f64() * 0.8 {
            n.push("observation: workers spent over 80% of their time waiting for incoming bytes; local disk and hashing were not the main time sink on this side".into());
        }
        let stalls = d.error_count(ErrCat::StallTimeout);
        if stalls > 0 {
            n.push(format!("observation: {stalls} chunk stream(s) hit the 90 s no-data stall timeout"));
        }
    } else {
        let qw = d.stages.quic_write.total();
        if worker > Duration::ZERO && qw.as_secs_f64() > worker.as_secs_f64() * 0.8 {
            n.push("observation: chunk handlers spent over 80% of their time blocked in QUIC writes (flow control, congestion control or send-buffer backpressure; this build cannot tell them apart)".into());
        }
    }
    if avg_active.is_some_and(|r| r < 64.0 * 1024.0) && d.c.bytes.load(Ordering::Relaxed) > 4 * 1024 * 1024 {
        n.push("observation: active average throughput was below 64 KiB/s".into());
    }
    n
}

// ====================================================================== output

static WRITE_LOCK: Mutex<()> = parking_lot::const_mutex(());

/// Logs the readable line and appends the summary to the JSONL file. The
/// file write runs on the blocking pool when called inside a Tokio runtime.
pub fn emit_summary(cfg: &DiagConfig, d: &TransferDiag, summary: &Value) {
    emit(cfg, d, summary, false);
}

/// Like `emit_summary`, but writes before returning (used during shutdown,
/// when the blocking pool may not get to run).
pub fn emit_summary_sync(cfg: &DiagConfig, d: &TransferDiag, summary: &Value) {
    emit(cfg, d, summary, true);
}

fn emit(cfg: &DiagConfig, d: &TransferDiag, summary: &Value, sync: bool) {
    if !cfg.enabled {
        return;
    }
    tracing::info!(target: "relayless_core::diag", "{}", d.log_line(summary));
    let dir = cfg.dir.clone();
    let write = move |v: &Value| {
        if let Err(e) = append_jsonl(&dir, v) {
            tracing::warn!(target: "relayless_core::diag", "could not write diagnostics summary: {e}");
        }
    };
    match tokio::runtime::Handle::try_current() {
        Ok(h) if !sync => {
            let v = summary.clone();
            h.spawn_blocking(move || write(&v));
        }
        _ => write(summary),
    }
}

pub fn append_jsonl(dir: &Path, v: &Value) -> std::io::Result<()> {
    let _g = WRITE_LOCK.lock();
    std::fs::create_dir_all(dir)?;
    let path = dir.join(SUMMARY_FILE);
    if std::fs::metadata(&path).map(|m| m.len() > MAX_SUMMARY_FILE_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join(ROTATED_FILE));
    }
    let mut line = serde_json::to_vec(v).map_err(std::io::Error::other)?;
    line.push(b'\n');
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    f.write_all(&line)
}

/// Reads all summaries from a diagnostics directory (tests and tools).
pub fn read_summaries(dir: &Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join(SUMMARY_FILE))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

// ====================================================================== tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn rate_handles_zero_and_normal_durations() {
        assert_eq!(rate(1000, Duration::ZERO), None);
        assert_eq!(rate(0, Duration::ZERO), None);
        assert_eq!(rate(1000, Duration::from_secs(2)), Some(500.0));
        assert_eq!(rate(0, Duration::from_secs(1)), Some(0.0));
        assert_eq!(to_mib(None), None);
        assert_eq!(to_mib(Some(1024.0 * 1024.0)), Some(1.0));
    }

    #[test]
    fn env_override_parses_switches() {
        assert_eq!(env_override(None), None);
        assert_eq!(env_override(Some("0")), Some(false));
        assert_eq!(env_override(Some(" OFF ")), Some(false));
        assert_eq!(env_override(Some("1")), Some(true));
        assert_eq!(env_override(Some("maybe")), None);
        let dir = Path::new("x");
        assert!(!DiagConfig::resolve(true, Some("off"), dir).enabled);
        assert!(DiagConfig::resolve(false, Some("on"), dir).enabled);
        assert!(DiagConfig::resolve(true, None, dir).enabled);
        assert!(!DiagConfig::resolve(false, Some("garbage"), dir).enabled);
    }

    #[test]
    fn route_labels() {
        assert_eq!(route_label("direct"), Route::Direct);
        assert_eq!(route_label("relay"), Route::Relay);
        assert_eq!(route_label("none"), Route::Unknown);
        assert_eq!(route_label("other"), Route::Unknown);
        assert_eq!(route_label(""), Route::Unknown);
        assert_eq!(serde_json::to_value(Route::Relay).unwrap(), json!("relay"));
    }

    #[test]
    fn route_changes_are_recorded_once_per_change() {
        let d = TransferDiag::new("j".into(), "receiver", "k".into(), "bin".into(), "p".into(), 10);
        assert!(!d.observe_route(Route::Unknown, None), "first observation sets the initial route");
        assert!(d.observe_route(Route::Relay, Some(80)));
        assert!(!d.observe_route(Route::Relay, Some(81)));
        assert!(d.observe_route(Route::Direct, Some(20)));
        let s = d.summary("completed", SummaryExtra::default()).unwrap();
        assert_eq!(s["route"]["initial"], "unknown");
        assert_eq!(s["route"]["final"], "direct");
        assert_eq!(s["route"]["changes"], 2);
        assert_eq!(s["route"]["change_log"][0]["from"], "unknown");
        assert_eq!(s["route"]["change_log"][0]["to"], "relay");
    }

    #[test]
    fn unknown_route_does_not_break_summary() {
        // No route was ever observed (route inspection unavailable).
        let d = TransferDiag::new("j".into(), "receiver", "k".into(), "".into(), "p".into(), 0);
        d.c.add_route_bytes(Route::Unknown, 1 << 20);
        let s = d.summary("completed", SummaryExtra::default()).unwrap();
        assert_eq!(s["route"]["initial"], "unknown");
        assert_eq!(s["route"]["bytes_by_route"]["unknown"], 1 << 20);
        assert!(s["connection"]["at_end"].is_null());
        assert!(s["flow_control_ceiling"].is_null(), "no RTT means no ceiling, not a zero ceiling");
    }

    #[test]
    fn stage_aggregation_and_json() {
        let s = Stage::default();
        let j = s.json();
        assert_eq!(j["count"], 0);
        assert!(j["avg_ms"].is_null() && j["max_ms"].is_null(), "empty stages report null, not zero");
        s.record_bytes(Duration::from_millis(10), 1024 * 1024);
        s.record_bytes(Duration::from_millis(30), 1024 * 1024);
        let j = s.json();
        assert_eq!(j["count"], 2);
        assert_eq!(j["total_ms"], 40.0);
        assert_eq!(j["max_ms"], 30.0);
        assert_eq!(j["avg_ms"], 20.0);
        assert_eq!(j["bytes"], 2 * 1024 * 1024);
        assert_eq!(j["stage_mib_per_sec"], 50.0);
    }

    #[test]
    fn chunk_trace_does_not_double_count_stage_totals() {
        // Stage totals are updated live; the chunk record keeps its own sums.
        let d = TransferDiag::new("j".into(), "receiver", "k".into(), "".into(), "p".into(), 0);
        let mut t = ChunkTrace::new(d.origin, 3, Some(0), 0, Some(Route::Direct));
        for _ in 0..4 {
            d.stages.hash.record(Duration::from_millis(2));
            t.hash(Duration::from_millis(2));
            d.stages.disk_write.record(Duration::from_millis(1));
            t.disk_write(Duration::from_millis(1));
            t.block(1 << 20);
        }
        let rec = t.finish("completed", None, true);
        d.finish_chunk(rec);
        assert_eq!(d.stages.hash.total(), Duration::from_millis(8));
        assert_eq!(d.stages.hash.count(), 4);
        let s = d.summary("completed", SummaryExtra::default()).unwrap();
        let c = &s["chunks"][0];
        assert_eq!(c["hash_ms"], 8.0);
        assert_eq!(c["disk_write_ms"], 4.0);
        assert_eq!(c["bytes"], 4 << 20);
        assert_eq!(c["blocks"], 4);
        assert!(c["quic_write_ms"].is_null(), "sender-only field is null on the receiver");
        assert_eq!(s["timings_ms"]["hash_verify"]["total_ms"], 8.0);
        assert_eq!(s["transfer"]["chunks_completed"], 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn gauge_tracks_parallel_workers() {
        let g = Arc::new(Gauge::new());
        let barrier = Arc::new(tokio::sync::Barrier::new(8));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let g = g.clone();
            let b = barrier.clone();
            set.spawn(async move {
                let _guard = g.enter();
                b.wait().await;
                tokio::time::sleep(Duration::from_millis(50)).await;
            });
        }
        while set.join_next().await.is_some() {}
        let s = g.snapshot();
        assert_eq!(s.current, 0);
        assert_eq!(s.peak, 8);
        let avg = s.average.unwrap();
        assert!(avg > 0.0 && avg <= 8.0, "time-weighted average {avg}");
    }

    #[test]
    fn summary_schema_and_serialization() {
        let d = TransferDiag::new("job1".into(), "receiver", file_key("share", &["a".into(), "b.mkv".into()]), "mkv".into(), "abcd1234".into(), 5);
        d.set_configuration(json!({"chunk_size_bytes": 16777216}));
        d.c.bytes.fetch_add(5, Ordering::Relaxed);
        d.error(ErrCat::StallTimeout);
        d.on_status("downloading");
        let s = d.summary("needs_attention", SummaryExtra { direct_only: true, ..Default::default() }).unwrap();
        assert_eq!(s["schema_version"], SCHEMA_VERSION);
        assert_eq!(s["kind"], "transfer_summary");
        assert_eq!(s["role"], "receiver");
        assert_eq!(s["transfer"]["bytes_transferred"], 5);
        assert_eq!(s["transfer"]["error_category"], "stall_timeout");
        assert_eq!(s["errors"]["stall_timeout"], 1);
        assert_eq!(s["route"]["direct_only"], true);
        assert_eq!(s["configuration"]["chunk_size_bytes"], 16777216);
        assert!(s["unavailable"]["stream_flow_control_blocked"].is_string());
        // Round-trips through text.
        let text = serde_json::to_string(&s).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["transfer_id"], "job1");
        assert!(d.summary("completed", SummaryExtra::default()).is_none(), "summary is produced once");
        // A completed transfer reports no error category, even after retried errors.
        let ok = TransferDiag::new("job2".into(), "receiver", "k".into(), "".into(), "p".into(), 1);
        ok.error(ErrCat::StallTimeout);
        let s = ok.summary("completed", SummaryExtra::default()).unwrap();
        assert!(s["transfer"]["error_category"].is_null());
        assert_eq!(s["errors"]["stall_timeout"], 1, "retried errors stay listed");
        let line = d.log_line(&s);
        assert!(line.starts_with("transfer_summary role=receiver id=job1 "));
        assert!(!line.contains("b.mkv"), "file names never appear");
    }

    #[test]
    fn congestion_note_flags_slow_lossless_pacing_on_sender_only() {
        let stuck = json!({"paths": [{"selected": true, "pacing_rate_bytes_per_sec": 46_224, "lost_packets": 0, "cwnd_bytes": 5808, "current_mtu": 1452}]});
        let n = congestion_note(false, &stuck).unwrap();
        assert!(n.contains("46224 bytes/s") && n.contains("4.0 packets"), "{n}");
        assert!(congestion_note(true, &stuck).is_none(), "receiver's cwnd is the ACK direction");
        let lossy = json!({"paths": [{"selected": true, "pacing_rate_bytes_per_sec": 46_224, "lost_packets": 9, "cwnd_bytes": 5808}]});
        assert!(congestion_note(false, &lossy).is_none());
        let fast = json!({"paths": [{"selected": true, "pacing_rate_bytes_per_sec": 400_000_000u64, "lost_packets": 0, "cwnd_bytes": 66988}]});
        assert!(congestion_note(false, &fast).is_none());
        let unknown = json!({"paths": [{"selected": true, "pacing_rate_bytes_per_sec": null}]});
        assert!(congestion_note(false, &unknown).is_none(), "no pacing data, no claim");
    }

    #[test]
    fn file_key_matches_on_both_sides_and_hides_names() {
        let a = file_key("s1", &["Videos".into(), "trip.mp4".into()]);
        let b = file_key("s1", &["Videos".into(), "trip.mp4".into()]);
        let c = file_key("s1", &["Videostrip.mp4".into()]);
        assert_eq!(a, b);
        assert_ne!(a, c, "component boundaries matter");
        assert_eq!(a.len(), 16);
        assert!(!a.contains("trip"));
    }

    #[test]
    fn limit_json_states_units_and_unlimited() {
        let u = limit_json(0, 0);
        assert_eq!(u["unlimited"], true);
        assert!(u["effective_bytes_per_sec"].is_null());
        let l = limit_json(50, 50 * 1024);
        assert_eq!(l["unlimited"], false);
        assert_eq!(l["configured_unit"], "KiB/s");
        assert_eq!(l["effective_bytes_per_sec"], 51200);
    }

    #[test]
    fn jsonl_round_trip_and_disabled_mode_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = DiagConfig::resolve(false, None, tmp.path());
        let d = TransferDiag::new("x".into(), "sender", "k".into(), "".into(), "p".into(), 1);
        let s = d.summary("idle", SummaryExtra::default()).unwrap();
        emit_summary(&cfg, &d, &s);
        assert!(!cfg.summary_path().exists(), "disabled diagnostics must not write files");
        let cfg = DiagConfig::resolve(true, None, tmp.path());
        emit_summary(&cfg, &d, &s);
        emit_summary(&cfg, &d, &s);
        let all = read_summaries(&cfg.dir);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0]["role"], "sender");
    }

    #[test]
    fn status_time_accumulates() {
        let d = TransferDiag::new("x".into(), "receiver", "k".into(), "".into(), "p".into(), 1);
        d.on_status("connecting");
        std::thread::sleep(Duration::from_millis(20));
        d.on_status("downloading");
        std::thread::sleep(Duration::from_millis(20));
        d.on_status("connecting");
        assert!(d.status_time("connecting") >= Duration::from_millis(20));
        assert!(d.status_time("downloading") >= Duration::from_millis(20));
        let s = d.summary("paused", SummaryExtra::default()).unwrap();
        assert!(s["transfer"]["status_time_ms"]["downloading"].as_f64().unwrap() >= 20.0);
        assert!(s["transfer"]["rates"]["active_window_ms"].as_f64().unwrap() >= 20.0);
    }
}
