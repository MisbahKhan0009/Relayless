//! Shared state behind the public `Core` handle.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointId};
use parking_lot::{Mutex, RwLock};
use tokio::sync::{broadcast, oneshot, Notify};
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::diag::{DiagConfig, TransferDiag};
use crate::receiver::{JobHandle, PeerConn};
use crate::types::{AccessRequest, CoreEvent, Settings};
use crate::util::RateLimiter;

pub struct PendingAccess {
    pub request: AccessRequest,
    pub respond: oneshot::Sender<bool>,
}

#[derive(Default)]
pub struct UploadStats {
    pub sent: AtomicU64,
    active: Mutex<HashMap<String, u32>>,
    pub rate: Mutex<(u64, f64)>,
}

impl UploadStats {
    pub fn begin(&self, name: &str) {
        *self.active.lock().entry(name.to_string()).or_default() += 1;
    }
    pub fn end(&self, name: &str) {
        let mut m = self.active.lock();
        if let Some(c) = m.get_mut(name) {
            *c -= 1;
            if *c == 0 {
                m.remove(name);
            }
        }
    }
    pub fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = self.active.lock().keys().cloned().collect();
        v.sort();
        v
    }
}

/// One file this device is sending to one receiver.
pub struct OutgoingFile {
    pub id: String,
    pub peer: EndpointId,
    pub share_id: String,
    pub path: Vec<String>,
    pub size: AtomicU64,
    /// Receiver's own count of bytes it has, from its last request.
    pub base: AtomicU64,
    /// Bytes sent since that count.
    pub since: AtomicU64,
    pub active: AtomicU32,
    pub last_ms: AtomicI64,
    pub started_ms: i64,
    pub verified: AtomicBool,
    pub rate: Mutex<(u64, f64)>,
}

impl OutgoingFile {
    pub fn progress(&self) -> u64 {
        let size = self.size.load(Ordering::Relaxed);
        (self.base.load(Ordering::Relaxed) + self.since.load(Ordering::Relaxed)).min(size)
    }
}

pub struct ShortCode {
    pub endpoint: Endpoint,
}

/// Limits how many files download at the same time.
pub struct JobSlots {
    max: AtomicUsize,
    active: Mutex<usize>,
    notify: Notify,
}

pub struct SlotGuard<'a>(&'a JobSlots);

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        *self.0.active.lock() -= 1;
        self.0.notify.notify_waiters();
    }
}

impl JobSlots {
    pub fn new(max: usize) -> Self {
        JobSlots { max: AtomicUsize::new(max), active: Mutex::new(0), notify: Notify::new() }
    }
    pub fn set_max(&self, max: usize) {
        self.max.store(max.max(1), Ordering::Relaxed);
        self.notify.notify_waiters();
    }
    pub async fn acquire(&self) -> SlotGuard<'_> {
        loop {
            {
                let mut a = self.active.lock();
                if *a < self.max.load(Ordering::Relaxed) {
                    *a += 1;
                    return SlotGuard(self);
                }
            }
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
    }
}

pub struct Inner {
    pub db: Db,
    pub endpoint: Endpoint,
    pub data_dir: PathBuf,
    pub settings: RwLock<Settings>,
    pub events: broadcast::Sender<CoreEvent>,
    pub shutdown: CancellationToken,
    pub local_mode: bool,
    // sender side
    pub pending: Mutex<HashMap<String, PendingAccess>>,
    pub incoming: Mutex<HashMap<EndpointId, Vec<(String, Connection)>>>,
    pub failed_hellos: Mutex<VecDeque<Instant>>,
    pub upload_limiter: RateLimiter,
    pub uploads: Mutex<HashMap<EndpointId, Arc<UploadStats>>>,
    pub short_codes: Mutex<HashMap<String, ShortCode>>,
    pub outgoing: Mutex<HashMap<String, Arc<OutgoingFile>>>,
    // receiver side
    pub download_limiter: RateLimiter,
    pub peers: Mutex<HashMap<EndpointId, Arc<PeerConn>>>,
    pub addr_hints: Mutex<HashMap<EndpointId, iroh::EndpointAddr>>,
    pub jobs: Mutex<HashMap<String, Arc<JobHandle>>>,
    pub slots: JobSlots,
    // diagnostics
    pub diag: DiagConfig,
    /// Congestion controller the endpoint was bound with (settings apply at restart).
    pub congestion_active: &'static str,
    /// Sender-side diagnostics per (receiver, file), keyed like `outgoing`.
    pub send_diags: Mutex<HashMap<String, Arc<TransferDiag>>>,
}

impl Inner {
    pub fn emit(&self, ev: CoreEvent) {
        let _ = self.events.send(ev);
    }

    pub fn log(&self, level: &str, message: &str, job_id: Option<&str>) {
        tracing::info!(level, "{message}");
        if let Ok(a) = self.db.add_activity(level, message, job_id) {
            self.emit(CoreEvent::Activity(a));
        }
    }

    pub fn notify(&self, title: &str, body: &str) {
        if self.settings.read().notifications {
            self.emit(CoreEvent::Notify { title: title.into(), body: body.into() });
        }
    }

    pub fn upload_stats_for(&self, peer: EndpointId) -> Arc<UploadStats> {
        self.uploads.lock().entry(peer).or_default().clone()
    }

    /// Finds or starts tracking a file being sent to `peer`.
    pub fn outgoing_for(&self, peer: EndpointId, share_id: &str, path: &[String], size: u64) -> Arc<OutgoingFile> {
        let key = format!("{peer}|{share_id}|{}", path.join("/"));
        let id = blake3::hash(key.as_bytes()).to_hex()[..16].to_string();
        let mut map = self.outgoing.lock();
        let of = map
            .entry(id.clone())
            .or_insert_with(|| {
                Arc::new(OutgoingFile {
                    id: id.clone(),
                    peer,
                    share_id: share_id.to_string(),
                    path: path.to_vec(),
                    size: AtomicU64::new(size),
                    base: AtomicU64::new(0),
                    since: AtomicU64::new(0),
                    active: AtomicU32::new(0),
                    last_ms: AtomicI64::new(crate::util::now_ms()),
                    started_ms: crate::util::now_ms(),
                    verified: AtomicBool::new(false),
                    rate: Mutex::new((0, 0.0)),
                })
            })
            .clone();
        if of.size.load(Ordering::Relaxed) != size {
            // The file changed on this side; start counting again.
            of.size.store(size, Ordering::Relaxed);
            of.base.store(0, Ordering::Relaxed);
            of.since.store(0, Ordering::Relaxed);
            of.verified.store(false, Ordering::Relaxed);
        }
        // Keep the list bounded: drop the oldest idle entries.
        if map.len() > 200 {
            let mut idle: Vec<(i64, String)> = map
                .values()
                .filter(|o| o.active.load(Ordering::Relaxed) == 0)
                .map(|o| (o.last_ms.load(Ordering::Relaxed), o.id.clone()))
                .collect();
            idle.sort();
            for (_, k) in idle.into_iter().take(map.len() - 200) {
                map.remove(&k);
            }
        }
        of
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }
}
