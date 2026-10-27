//! Data transfer objects shared with the UI (serialized camelCase).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Connecting,
    Downloading,
    WaitingPeer,
    WaitingDirect,
    Reconnecting,
    Paused,
    Verifying,
    Completed,
    Failed,
    Canceled,
    NeedsAttention,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Connecting => "connecting",
            JobStatus::Downloading => "downloading",
            JobStatus::WaitingPeer => "waiting_peer",
            JobStatus::WaitingDirect => "waiting_direct",
            JobStatus::Reconnecting => "reconnecting",
            JobStatus::Paused => "paused",
            JobStatus::Verifying => "verifying",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Canceled => "canceled",
            JobStatus::NeedsAttention => "needs_attention",
        }
    }

    pub fn parse(s: &str) -> JobStatus {
        match s {
            "queued" => JobStatus::Queued,
            "connecting" => JobStatus::Connecting,
            "downloading" => JobStatus::Downloading,
            "waiting_peer" => JobStatus::WaitingPeer,
            "waiting_direct" => JobStatus::WaitingDirect,
            "reconnecting" => JobStatus::Reconnecting,
            "paused" => JobStatus::Paused,
            "verifying" => JobStatus::Verifying,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "canceled" => JobStatus::Canceled,
            _ => JobStatus::NeedsAttention,
        }
    }

    /// Jobs in these states should be (re)started automatically on launch.
    pub fn auto_runs(&self) -> bool {
        matches!(
            self,
            JobStatus::Queued
                | JobStatus::Connecting
                | JobStatus::Downloading
                | JobStatus::WaitingPeer
                | JobStatus::WaitingDirect
                | JobStatus::Reconnecting
                | JobStatus::Verifying
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RouteInfo {
    /// "direct", "relay" or "none"
    pub kind: String,
    pub rtt_ms: Option<u64>,
    pub remote: Option<String>,
}

impl RouteInfo {
    pub fn none() -> Self {
        RouteInfo { kind: "none".into(), rtt_ms: None, remote: None }
    }
    pub fn is_direct(&self) -> bool {
        self.kind == "direct"
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub share_id: String,
    pub remote_path: Vec<String>,
    pub display_name: String,
    pub dest_path: String,
    pub size: u64,
    pub verified_bytes: u64,
    pub done_bytes: u64,
    pub status: JobStatus,
    pub error: Option<String>,
    pub speed_bps: f64,
    pub eta_secs: Option<u64>,
    pub total_chunks: u64,
    pub verified_chunks: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub route: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InvitationDto {
    pub id: String,
    pub share_id: String,
    pub expires_at: i64,
    pub max_uses: Option<u32>,
    pub use_count: u32,
    pub require_approval: bool,
    pub created_at: i64,
    /// "active", "expired", "used_up", "revoked"
    pub state: String,
    pub has_short_code: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CreatedInvitation {
    pub invitation: InvitationDto,
    pub invite: String,
    pub short_code: Option<String>,
    pub short_code_expires_at: Option<i64>,
    pub qr_svg: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ShareDto {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub active: bool,
    pub created_at: i64,
    pub invitations: Vec<InvitationDto>,
    pub device_count: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrantDto {
    pub peer_id: String,
    pub peer_short: String,
    pub device_name: String,
    pub share_id: String,
    pub share_name: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub connected: bool,
    pub route: RouteInfo,
    pub safety_words: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteShareDto {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDto {
    pub peer_id: String,
    pub peer_short: String,
    pub name: String,
    pub shares: Vec<RemoteShareDto>,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub status: PeerStatus,
    pub safety_words: Vec<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PeerStatus {
    pub peer_id: String,
    /// "disconnected", "connecting", "connected", "unreachable"
    pub state: String,
    pub route: RouteInfo,
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    pub entries: Vec<RemoteEntry>,
    pub total: u64,
    pub offset: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccessRequest {
    pub id: String,
    pub peer_id: String,
    pub peer_short: String,
    pub device_name: String,
    pub share_id: String,
    pub share_name: String,
    pub created_at: i64,
    pub safety_words: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDto {
    pub id: i64,
    pub ts: i64,
    /// "info", "success", "warning", "error"
    pub level: String,
    pub message: String,
    pub job_id: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub endpoint_id: String,
    pub endpoint_short: String,
    pub online: bool,
    pub relay_url: Option<String>,
    pub direct_addrs: usize,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JoinResult {
    pub peer_id: String,
    pub sender_name: String,
    pub shares: Vec<RemoteShareDto>,
    pub safety_words: Vec<String>,
}

/// A finished transfer as kept in local history, with everything the
/// privacy receipt shows.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDto {
    pub id: String,
    /// "received" or "sent"
    pub direction: String,
    pub peer_id: String,
    pub peer_name: String,
    pub name: String,
    pub ext: String,
    pub size: u64,
    pub started_at: i64,
    pub finished_at: i64,
    pub active_ms: u64,
    pub direct_bytes: u64,
    pub relay_bytes: u64,
    pub peak_bps: f64,
    pub blocks: u64,
    pub fingerprint: String,
    pub safety_words: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub device_name: String,
    pub download_dir: String,
    /// Parallel chunk streams per file (1..=32)
    pub parallel_streams: u32,
    /// Concurrent file transfers (1..=6)
    pub max_concurrent_jobs: u32,
    /// 0 = unlimited. KiB/s.
    pub download_limit_kbps: u64,
    /// 0 = unlimited. KiB/s.
    pub upload_limit_kbps: u64,
    /// "direct_only" or "allow_relay"
    pub connection_mode: String,
    /// "bbr" or "cubic"; applied at next start
    pub congestion: String,
    /// Chunk size in MiB for new transfers (8..=32)
    pub chunk_size_mib: u32,
    pub low_priority: bool,
    pub notifications: bool,
    pub close_to_tray: bool,
    pub onboarded: bool,
    /// How long finished transfers stay in local history:
    /// "forever", "365", "90", "30" (days) or "off".
    pub history_retention: String,
    /// Keep file names in history. When off, only type and size are kept.
    pub history_names: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            device_name: String::new(),
            download_dir: String::new(),
            parallel_streams: 12,
            max_concurrent_jobs: 2,
            download_limit_kbps: 0,
            upload_limit_kbps: 0,
            connection_mode: "direct_only".into(),
            congestion: "bbr".into(),
            chunk_size_mib: 16,
            low_priority: true,
            notifications: true,
            close_to_tray: true,
            onboarded: false,
            history_retention: "365".into(),
            history_names: true,
        }
    }
}

impl Settings {
    pub fn sanitized(mut self) -> Self {
        self.parallel_streams = self.parallel_streams.clamp(1, 32);
        self.max_concurrent_jobs = self.max_concurrent_jobs.clamp(1, 6);
        self.chunk_size_mib = self.chunk_size_mib.clamp(8, 32);
        if self.connection_mode != "allow_relay" {
            self.connection_mode = "direct_only".into();
        }
        if self.congestion != "cubic" {
            self.congestion = "bbr".into();
        }
        if !matches!(self.history_retention.as_str(), "forever" | "365" | "90" | "30" | "off") {
            self.history_retention = "365".into();
        }
        self.device_name = self.device_name.trim().chars().take(64).collect();
        self
    }
    pub fn direct_only(&self) -> bool {
        self.connection_mode == "direct_only"
    }
    pub fn history_on(&self) -> bool {
        self.history_retention != "off"
    }
    /// Oldest finish time to keep, in ms since the epoch.
    pub fn history_cutoff(&self, now_ms: i64) -> i64 {
        match self.history_retention.parse::<i64>() {
            Ok(days) => now_ms - days * 86_400_000,
            Err(_) if self.history_retention == "off" => i64::MAX,
            Err(_) => i64::MIN,
        }
    }
}

/// What this computer is currently sending to one receiver.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UploadDto {
    pub peer_id: String,
    pub device_name: String,
    pub bps: f64,
    pub total_sent: u64,
    pub files: Vec<String>,
}

/// Events pushed to the UI.
#[derive(Clone, Debug)]
pub enum CoreEvent {
    Uploads(Vec<UploadDto>),
    JoinStatus { message: String, safety_words: Option<Vec<String>> },
    HistoryChanged,
    JobUpdated(JobSnapshot),
    JobRemoved(String),
    PeerStatus(PeerStatus),
    AccessRequested(AccessRequest),
    AccessResolved(String),
    SharesChanged,
    RemotesChanged,
    GrantsChanged,
    Activity(ActivityDto),
    Notify { title: String, body: String },
    Network(NetworkStatus),
    IntegrityProgress { job_id: String, checked: u64, total: u64 },
}

impl CoreEvent {
    pub fn name(&self) -> &'static str {
        match self {
            CoreEvent::Uploads(_) => "uploads",
            CoreEvent::JoinStatus { .. } => "join-status",
            CoreEvent::HistoryChanged => "history-changed",
            CoreEvent::JobUpdated(_) => "job-updated",
            CoreEvent::JobRemoved(_) => "job-removed",
            CoreEvent::PeerStatus(_) => "peer-status",
            CoreEvent::AccessRequested(_) => "access-requested",
            CoreEvent::AccessResolved(_) => "access-resolved",
            CoreEvent::SharesChanged => "shares-changed",
            CoreEvent::RemotesChanged => "remotes-changed",
            CoreEvent::GrantsChanged => "grants-changed",
            CoreEvent::Activity(_) => "activity",
            CoreEvent::Notify { .. } => "notify",
            CoreEvent::Network(_) => "network-status",
            CoreEvent::IntegrityProgress { .. } => "integrity-progress",
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            CoreEvent::Uploads(u) => serde_json::to_value(u).unwrap_or_default(),
            CoreEvent::JoinStatus { message, safety_words } => {
                json!({ "message": message, "safetyWords": safety_words })
            }
            CoreEvent::JobUpdated(j) => serde_json::to_value(j).unwrap_or_default(),
            CoreEvent::JobRemoved(id) => json!({ "id": id }),
            CoreEvent::PeerStatus(p) => serde_json::to_value(p).unwrap_or_default(),
            CoreEvent::AccessRequested(r) => serde_json::to_value(r).unwrap_or_default(),
            CoreEvent::AccessResolved(id) => json!({ "id": id }),
            CoreEvent::SharesChanged
            | CoreEvent::RemotesChanged
            | CoreEvent::GrantsChanged
            | CoreEvent::HistoryChanged => {
                json!({})
            }
            CoreEvent::Activity(a) => serde_json::to_value(a).unwrap_or_default(),
            CoreEvent::Notify { title, body } => json!({ "title": title, "body": body }),
            CoreEvent::Network(n) => serde_json::to_value(n).unwrap_or_default(),
            CoreEvent::IntegrityProgress { job_id, checked, total } => {
                json!({ "jobId": job_id, "checked": checked, "total": total })
            }
        }
    }
}
