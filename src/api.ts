import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type JobStatus =
  | "queued"
  | "connecting"
  | "downloading"
  | "waiting_peer"
  | "waiting_direct"
  | "reconnecting"
  | "paused"
  | "verifying"
  | "completed"
  | "failed"
  | "canceled"
  | "needs_attention";

export interface RouteInfo {
  kind: "direct" | "relay" | "none" | string;
  rttMs: number | null;
  remote: string | null;
}

export interface Job {
  id: string;
  peerId: string;
  peerName: string;
  shareId: string;
  remotePath: string[];
  displayName: string;
  destPath: string;
  size: number;
  verifiedBytes: number;
  doneBytes: number;
  status: JobStatus;
  error: string | null;
  speedBps: number;
  etaSecs: number | null;
  totalChunks: number;
  verifiedChunks: number;
  createdAt: number;
  updatedAt: number;
  completedAt: number | null;
  route: string;
}

export interface Invitation {
  id: string;
  shareId: string;
  expiresAt: number;
  maxUses: number | null;
  useCount: number;
  requireApproval: boolean;
  createdAt: number;
  state: "active" | "expired" | "used_up" | "revoked";
  hasShortCode: boolean;
}

export interface CreatedInvitation {
  invitation: Invitation;
  invite: string;
  shortCode: string | null;
  shortCodeExpiresAt: number | null;
  qrSvg: string;
}

export interface Share {
  id: string;
  name: string;
  rootPath: string;
  active: boolean;
  createdAt: number;
  invitations: Invitation[];
  deviceCount: number;
}

export interface Grant {
  peerId: string;
  peerShort: string;
  deviceName: string;
  shareId: string;
  shareName: string;
  createdAt: number;
  lastSeenAt: number;
  connected: boolean;
  route: RouteInfo;
}

export interface PeerStatus {
  peerId: string;
  state: "disconnected" | "connecting" | "connected" | "unreachable" | "denied" | string;
  route: RouteInfo;
  message: string | null;
}

export interface Remote {
  peerId: string;
  peerShort: string;
  name: string;
  shares: { id: string; name: string }[];
  createdAt: number;
  lastSeenAt: number;
  status: PeerStatus;
}

export interface RemoteEntry {
  name: string;
  isDir: boolean;
  size: number;
  modifiedMs: number;
}

export interface ListResult {
  entries: RemoteEntry[];
  total: number;
  offset: number;
}

export interface AccessRequest {
  id: string;
  peerId: string;
  peerShort: string;
  deviceName: string;
  shareId: string;
  shareName: string;
  createdAt: number;
}

export interface Activity {
  id: number;
  ts: number;
  level: "info" | "success" | "warning" | "error";
  message: string;
  jobId: string | null;
}

export interface NetworkStatus {
  endpointId: string;
  endpointShort: string;
  online: boolean;
  relayUrl: string | null;
  directAddrs: number;
}

export interface Upload {
  peerId: string;
  deviceName: string;
  bps: number;
  totalSent: number;
  files: string[];
}

export interface Settings {
  deviceName: string;
  downloadDir: string;
  parallelStreams: number;
  maxConcurrentJobs: number;
  downloadLimitKbps: number;
  uploadLimitKbps: number;
  connectionMode: "direct_only" | "allow_relay";
  congestion: "bbr" | "cubic";
  chunkSizeMib: number;
  lowPriority: boolean;
  notifications: boolean;
  closeToTray: boolean;
  onboarded: boolean;
}

export interface Bootstrap {
  settings: Settings;
  network: NetworkStatus;
  version: string;
  dataDir: string;
}

export interface JoinResult {
  peerId: string;
  senderName: string;
  shares: { id: string; name: string }[];
}

export interface DownloadItem {
  path: string[];
  isDir: boolean;
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  networkStatus: () => invoke<NetworkStatus>("network_status"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  listShares: () => invoke<Share[]>("list_shares"),
  createShare: (path: string, name?: string) => invoke<Share>("create_share", { path, name: name ?? null }),
  setShareActive: (id: string, active: boolean) => invoke<void>("set_share_active", { id, active }),
  deleteShare: (id: string) => invoke<void>("delete_share", { id }),
  createInvitation: (shareId: string, expiresInSecs: number, maxUses: number | null, requireApproval: boolean, withCode: boolean) =>
    invoke<CreatedInvitation>("create_invitation", { shareId, expiresInSecs, maxUses, requireApproval, withCode }),
  revokeInvitation: (id: string) => invoke<void>("revoke_invitation", { id }),
  listGrants: () => invoke<Grant[]>("list_grants"),
  revokeDevice: (peerId: string) => invoke<void>("revoke_device", { peerId }),
  pendingRequests: () => invoke<AccessRequest[]>("pending_requests"),
  respondAccess: (id: string, approve: boolean) => invoke<void>("respond_access", { id, approve }),
  join: (input: string) => invoke<JoinResult>("join", { input }),
  listRemotes: () => invoke<Remote[]>("list_remotes"),
  removeRemote: (peerId: string) => invoke<void>("remove_remote", { peerId }),
  connectRemote: (peerId: string) => invoke<PeerStatus>("connect_remote", { peerId }),
  listRemoteDir: (peerId: string, shareId: string, path: string[], offset: number, limit: number) =>
    invoke<ListResult>("list_remote_dir", { peerId, shareId, path, offset, limit }),
  startDownload: (peerId: string, shareId: string, items: DownloadItem[], destDir: string | null) =>
    invoke<string[]>("start_download", { peerId, shareId, items, destDir }),
  listJobs: () => invoke<Job[]>("list_jobs"),
  pauseJob: (id: string) => invoke<void>("pause_job", { id }),
  resumeJob: (id: string) => invoke<void>("resume_job", { id }),
  restartJob: (id: string) => invoke<void>("restart_job", { id }),
  cancelJob: (id: string, deletePartial: boolean) => invoke<void>("cancel_job", { id, deletePartial }),
  removeJob: (id: string) => invoke<void>("remove_job", { id }),
  listActivity: (limit: number) => invoke<Activity[]>("list_activity", { limit }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
};

export function on<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
