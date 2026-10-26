// Dev-only stand-in for the Tauri bridge, used to preview the UI in a browser (RL_MOCK=1).
const now = Date.now();
const GB = 1024 ** 3;
const st = (route: string, rtt: number | null) => ({ kind: route, rttMs: rtt, remote: null });
const job = (o: any) => ({
  peerId: "a1b2c3d4e5", peerName: "Misbah-Desktop (Dhaka)", shareId: "s1", remotePath: [o.displayName], destPath: "C:\\Users\\Jonas\\Downloads\\Relayless\\" + o.displayName,
  verifiedBytes: o.doneBytes, error: null, speedBps: 0, etaSecs: null, totalChunks: Math.ceil(o.size / (16 * 1024 * 1024)), verifiedChunks: Math.floor(o.doneBytes / (16 * 1024 * 1024)),
  createdAt: now - 3600e3, updatedAt: now, completedAt: null, route: "direct", ...o,
});
const data: Record<string, any> = {
  bootstrap: {
    settings: { deviceName: "Jonas-Workstation", downloadDir: "C:\\Users\\Jonas\\Downloads\\Relayless", parallelStreams: 12, maxConcurrentJobs: 2, downloadLimitKbps: 0, uploadLimitKbps: 0, connectionMode: "direct_only", congestion: "bbr", chunkSizeMib: 16, lowPriority: true, notifications: true, closeToTray: true, onboarded: (window as any).__onboarded ?? true },
    network: { endpointId: "9f3c1e7a2b4d6f8091a3c5e7f9b1d3f5a7c9e1b3d5f7a9c1e3b5d7f9a1c3e5b7", endpointShort: "9f3c1e7a", online: true, relayUrl: "https://euc1-1.relay.n0.iroh-canary.iroh.link./", directAddrs: 3 },
    version: "0.1.0", dataDir: "C:\\Users\\Jonas\\AppData\\Roaming\\app.relayless.desktop",
  },
  list_shares: [
    { id: "s1", name: "Lab Datasets", rootPath: "D:\\Research\\Lab Datasets", active: true, createdAt: now - 86400e3, deviceCount: 1,
      invitations: [{ id: "i1", shareId: "s1", expiresAt: now + 20 * 3600e3, maxUses: 1, useCount: 0, requireApproval: true, createdAt: now - 3600e3, state: "active", hasShortCode: true }] },
    { id: "s2", name: "Video Archive", rootPath: "E:\\Archive\\Video", active: false, createdAt: now - 5 * 86400e3, deviceCount: 0, invitations: [] },
  ],
  list_grants: [
    { peerId: "77aa88bb99", peerShort: "77aa88bb", deviceName: "Jonas-Laptop (Berlin)", shareId: "s1", shareName: "Lab Datasets", createdAt: now - 86400e3, lastSeenAt: now, connected: true, route: st("direct", 184) },
  ],
  list_remotes: [
    { peerId: "a1b2c3d4e5", peerShort: "a1b2c3d4", name: "Misbah-Desktop (Dhaka)", shares: [{ id: "s1", name: "CT Scans 2026" }, { id: "s2", name: "Backups" }], createdAt: now - 2 * 86400e3, lastSeenAt: now, status: { peerId: "a1b2c3d4e5", state: "connected", route: st("direct", 176), message: null } },
    { peerId: "ffee", peerShort: "ffee0011", name: "Office-NAS", shares: [{ id: "s9", name: "Shared" }], createdAt: now - 9 * 86400e3, lastSeenAt: now - 3 * 86400e3, status: { peerId: "ffee", state: "disconnected", route: st("none", null), message: null } },
  ],
  list_jobs: [
    job({ id: "j1", displayName: "uls23_full_dataset.tar", size: 64 * GB, doneBytes: 27.4 * GB, status: "downloading", speedBps: 11.6 * 1024 * 1024, etaSecs: 3270 }),
    job({ id: "j2", displayName: "ct_volumes_part2.zip", size: 18.2 * GB, doneBytes: 6.1 * GB, status: "reconnecting", error: "Connection interrupted. Your progress is safe. The transfer stalled." }),
    job({ id: "j3", displayName: "notes_2026.pdf", size: 4.2 * 1024 * 1024, doneBytes: 4.2 * 1024 * 1024, status: "completed", completedAt: now - 600e3 }),
    job({ id: "j4", displayName: "segmentation_weights.ckpt", size: 2.4 * GB, doneBytes: 0.8 * GB, status: "needs_attention", error: "The file changed on the sender's computer." }),
    job({ id: "j5", displayName: "raw_scans_batch7.7z", size: 9.8 * GB, doneBytes: 3.3 * GB, status: "paused" }),
  ],
  pending_requests: (window as any).__pending ? [{ id: "r1", peerId: "x", peerShort: "4be1c09a", deviceName: "Jonas-Laptop (Berlin)", shareId: "s1", shareName: "Lab Datasets", createdAt: now }] : [],
  list_activity: [
    { id: 5, ts: now - 60e3, level: "warning", message: "Waiting for sender: ct_volumes_part2.zip", jobId: "j2" },
    { id: 4, ts: now - 600e3, level: "success", message: "Downloaded and verified notes_2026.pdf (4.2 MB)", jobId: "j3" },
    { id: 3, ts: now - 3600e3, level: "info", message: "Queued 4 file(s), 94.4 GB from Misbah-Desktop (Dhaka)", jobId: null },
    { id: 2, ts: now - 7200e3, level: "success", message: "Paired with Misbah-Desktop (Dhaka)", jobId: null },
    { id: 1, ts: now - 7300e3, level: "error", message: "segmentation_weights.ckpt: The file changed on the sender's computer.", jobId: "j4" },
  ],
  list_remote_dir: {
    entries: [
      { name: "2024", isDir: true, size: 0, modifiedMs: now - 300 * 86400e3 },
      { name: "2025", isDir: true, size: 0, modifiedMs: now - 100 * 86400e3 },
      { name: "annotations", isDir: true, size: 0, modifiedMs: now - 10 * 86400e3 },
      ...Array.from({ length: 60 }, (_, i) => ({ name: `patient_${String(i + 1).padStart(4, "0")}_ct_volume.nii.gz`, isDir: false, size: (300 + i * 37) * 1024 * 1024, modifiedMs: now - i * 86400e3 })),
      { name: "uls23_full_dataset.tar", isDir: false, size: 64 * GB, modifiedMs: now - 2 * 86400e3 },
      { name: "README.md", isDir: false, size: 4200, modifiedMs: now - 2 * 86400e3 },
    ], total: 65, offset: 0,
  },
  create_invitation: {
    invitation: { id: "i2", shareId: "s1", expiresAt: now + 86400e3, maxUses: 1, useCount: 0, requireApproval: true, createdAt: now, state: "active", hasShortCode: true },
    invite: "RL1-T7QBZK2M4XHV9RCN3EWP5Y8AFJDG6SLQ2TMVK7XB4NHC9REW3PY5AF8JDG2KLQ6TMV7XB4NHC9REW3PY5AF8JDG2KLQ6TMVBZK2M4XHV9RCN3EWP5Y8AFJDG",
    shortCode: "K7QM-9XRT-4HWD", shortCodeExpiresAt: now + 30 * 60e3,
    qrSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 29 29"><rect width="29" height="29" fill="#fff"/><path d="M1 1h7v7H1zM21 1h7v7h-7zM1 21h7v7H1z" fill="#0e0f11"/><path d="M10 2h1v1h-1zM12 4h2v2h-2zM10 10h9v9h-9zM22 12h3v3h-3zM12 22h4v1h-4zM20 20h6v6h-6z" fill="#0e0f11"/></svg>',
  },
};
export async function invoke<T>(cmd: string, _args?: any): Promise<T> {
  await new Promise((r) => setTimeout(r, 30));
  if (cmd in data) return JSON.parse(JSON.stringify(data[cmd]));
  return undefined as T;
}
