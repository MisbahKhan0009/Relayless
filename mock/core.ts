// Demo backend for `npm run dev:ui`: lets the interface run in a browser with sample data.
const now = Date.now();
const GB = 1024 ** 3;
const st = (route: string, rtt: number | null) => ({ kind: route, rttMs: rtt, remote: null });
const job = (o: any) => ({
  peerId: "a1b2c3d4e5", peerName: "Dhaka-Edit-Bay", shareId: "s1", remotePath: [o.displayName], destPath: "C:\\Users\\Lena\\Downloads\\Relayless\\" + o.displayName,
  verifiedBytes: o.doneBytes, error: null, speedBps: 0, etaSecs: null, totalChunks: Math.ceil(o.size / (16 * 1024 * 1024)), verifiedChunks: Math.floor(o.doneBytes / (16 * 1024 * 1024)),
  createdAt: now - 3600e3, updatedAt: now, completedAt: null, route: "direct", ...o,
});
export const data: Record<string, any> = {
  bootstrap: {
    settings: { deviceName: "Berlin-Studio", downloadDir: "C:\\Users\\Lena\\Downloads\\Relayless", parallelStreams: 12, maxConcurrentJobs: 2, downloadLimitKbps: 0, uploadLimitKbps: 0, connectionMode: "direct_only", congestion: "bbr", chunkSizeMib: 16, lowPriority: true, notifications: true, closeToTray: true, onboarded: (window as any).__onboarded ?? true, historyRetention: "365", historyNames: true },
    network: { endpointId: "9f3c1e7a2b4d6f8091a3c5e7f9b1d3f5a7c9e1b3d5f7a9c1e3b5d7f9a1c3e5b7", endpointShort: "9f3c1e7a", online: true, relayUrl: "https://euc1-1.relay.n0.iroh-canary.iroh.link./", directAddrs: 3 },
    platform: (window as any).__platform ?? "windows",
    version: "0.1.0", dataDir: "C:\\Users\\Lena\\AppData\\Roaming\\app.relayless.desktop",
  },
  list_shares: [
    { id: "s1", name: "Client deliveries", rootPath: "D:\\Research\\Client deliveries", active: true, createdAt: now - 86400e3, deviceCount: 1,
      invitations: [{ id: "i1", shareId: "s1", expiresAt: now + 20 * 3600e3, maxUses: 1, useCount: 0, requireApproval: true, createdAt: now - 3600e3, state: "active", hasShortCode: true }] },
    { id: "s2", name: "Video Archive", rootPath: "E:\\Archive\\Video", active: false, createdAt: now - 5 * 86400e3, deviceCount: 0, invitations: [] },
  ],
  list_grants: [
    { peerId: "77aa88bb99", peerShort: "77aa88bb", deviceName: "Lena's laptop", shareId: "s1", shareName: "Client deliveries", createdAt: now - 86400e3, lastSeenAt: now, connected: true, route: st("direct", 184), safetyWords: ["harbor", "quill", "maple", "orbit", "tulip"] },
  ],
  list_remotes: [
    { peerId: "a1b2c3d4e5", peerShort: "a1b2c3d4", name: "Dhaka-Edit-Bay", shares: [{ id: "s1", name: "Film masters" }, { id: "s2", name: "Archive" }], createdAt: now - 2 * 86400e3, lastSeenAt: now, status: { peerId: "a1b2c3d4e5", state: "connected", route: st("direct", 176), message: null }, safetyWords: ["cedar", "lantern", "otter", "quartz", "violet"] },
    { peerId: "ffee", peerShort: "ffee0011", name: "Home server", shares: [{ id: "s9", name: "Shared" }], createdAt: now - 9 * 86400e3, lastSeenAt: now - 3 * 86400e3, status: { peerId: "ffee", state: "disconnected", route: st("none", null), message: null }, safetyWords: ["bison", "ember", "kettle", "plum", "summit"] },
  ],
  list_jobs: [
    job({ id: "j1", displayName: "feature-film-master-4k.mov", size: 64 * GB, doneBytes: 27.4 * GB, status: "downloading", speedBps: 11.6 * 1024 * 1024, etaSecs: 3270 }),
    job({ id: "j2", displayName: "raw-footage-day-2.zip", size: 18.2 * GB, doneBytes: 6.1 * GB, status: "reconnecting", error: "Connection interrupted. Your progress is safe. The transfer stalled." }),
    job({ id: "j3", displayName: "color-grade-notes.pdf", size: 4.2 * 1024 * 1024, doneBytes: 4.2 * 1024 * 1024, status: "completed", completedAt: now - 600e3 }),
    job({ id: "j4", displayName: "sound-mix-stems.wav", size: 2.4 * GB, doneBytes: 0.8 * GB, status: "needs_attention", error: "The file changed on the sender's computer." }),
    job({ id: "j5", displayName: "vfx-plates-batch-7.7z", size: 9.8 * GB, doneBytes: 3.3 * GB, status: "paused" }),
  ],
  pending_requests: (window as any).__pending ? [{ id: "r1", peerId: "x", peerShort: "4be1c09a", deviceName: "Lena's laptop", shareId: "s1", shareName: "Client deliveries", createdAt: now, safetyWords: ["harbor", "quill", "maple", "orbit", "tulip"] }] : [],
  list_activity: [
    { id: 5, ts: now - 60e3, level: "warning", message: "Waiting for sender: raw-footage-day-2.zip", jobId: "j2" },
    { id: 4, ts: now - 600e3, level: "success", message: "Downloaded and verified color-grade-notes.pdf (4.2 MB)", jobId: "j3" },
    { id: 3, ts: now - 3600e3, level: "info", message: "Queued 4 file(s), 94.4 GB from Dhaka-Edit-Bay", jobId: null },
    { id: 2, ts: now - 7200e3, level: "success", message: "Paired with Dhaka-Edit-Bay", jobId: null },
    { id: 1, ts: now - 7300e3, level: "error", message: "sound-mix-stems.wav: The file changed on the sender's computer.", jobId: "j4" },
  ],
  list_remote_dir: {
    entries: [
      { name: "2024", isDir: true, size: 0, modifiedMs: now - 300 * 86400e3 },
      { name: "2025", isDir: true, size: 0, modifiedMs: now - 100 * 86400e3 },
      { name: "subtitles", isDir: true, size: 0, modifiedMs: now - 10 * 86400e3 },
      ...Array.from({ length: 60 }, (_, i) => ({ name: `scene-${String(i + 1).padStart(3, "0")}-take-${(i % 4) + 1}.mov`, isDir: false, size: (300 + i * 37) * 1024 * 1024, modifiedMs: now - i * 86400e3 })),
      { name: "feature-film-master-4k.mov", isDir: false, size: 64 * GB, modifiedMs: now - 2 * 86400e3 },
      { name: "README.md", isDir: false, size: 4200, modifiedMs: now - 2 * 86400e3 },
    ], total: 65, offset: 0,
  },
  create_invitation: {
    invitation: { id: "i2", shareId: "s1", expiresAt: now + 86400e3, maxUses: 1, useCount: 0, requireApproval: true, createdAt: now, state: "active", hasShortCode: true },
    invite: "RL1-T7QBZK2M4XHV9RCN3EWP5Y8AFJDG6SLQ2TMVK7XB4NHC9REW3PY5AF8JDG2KLQ6TMV7XB4NHC9REW3PY5AF8JDG2KLQ6TMVBZK2M4XHV9RCN3EWP5Y8AFJDG",
    shortCode: "K7QM-9XRT-4HWD", shortCodeExpiresAt: now + 30 * 60e3,
    qrSvg: '<svg width="41mm" height="41mm" version="1.1" viewBox="0 0 41 41" xmlns="http://www.w3.org/2000/svg"><path d="M2,2H3V3H2zM3,2H4V3H3zM4,2H5V3H4zM5,2H6V3H5zM6,2H7V3H6zM7,2H8V3H7zM8,2H9V3H8zM13,2H14V3H13zM14,2H15V3H14zM16,2H17V3H16zM17,2H18V3H17zM18,2H19V3H18zM19,2H20V3H19zM20,2H21V3H20zM21,2H22V3H21zM23,2H24V3H23zM25,2H26V3H25zM26,2H27V3H26zM27,2H28V3H27zM29,2H30V3H29zM30,2H31V3H30zM32,2H33V3H32zM33,2H34V3H33zM34,2H35V3H34zM35,2H36V3H35zM36,2H37V3H36zM37,2H38V3H37zM38,2H39V3H38zM2,3H3V4H2zM8,3H9V4H8zM10,3H11V4H10zM11,3H12V4H11zM12,3H13V4H12zM14,3H15V4H14zM15,3H16V4H15zM16,3H17V4H16zM20,3H21V4H20zM21,3H22V4H21zM29,3H30V4H29zM32,3H33V4H32zM38,3H39V4H38zM2,4H3V5H2zM4,4H5V5H4zM5,4H6V5H5zM6,4H7V5H6zM8,4H9V5H8zM10,4H11V5H10zM11,4H12V5H11zM12,4H13V5H12zM15,4H16V5H15zM17,4H18V5H17zM20,4H21V5H20zM21,4H22V5H21zM22,4H23V5H22zM23,4H24V5H23zM26,4H27V5H26zM28,4H29V5H28zM30,4H31V5H30zM32,4H33V5H32zM34,4H35V5H34zM35,4H36V5H35zM36,4H37V5H36zM38,4H39V5H38zM2,5H3V6H2zM4,5H5V6H4zM5,5H6V6H5zM6,5H7V6H6zM8,5H9V6H8zM10,5H11V6H10zM11,5H12V6H11zM13,5H14V6H13zM19,5H20V6H19zM20,5H21V6H20zM21,5H22V6H21zM23,5H24V6H23zM24,5H25V6H24zM26,5H27V6H26zM27,5H28V6H27zM28,5H29V6H28zM30,5H31V6H30zM32,5H33V6H32zM34,5H35V6H34zM35,5H36V6H35zM36,5H37V6H36zM38,5H39V6H38zM2,6H3V7H2zM4,6H5V7H4zM5,6H6V7H5zM6,6H7V7H6zM8,6H9V7H8zM12,6H13V7H12zM15,6H16V7H15zM17,6H18V7H17zM24,6H25V7H24zM25,6H26V7H25zM28,6H29V7H28zM32,6H33V7H32zM34,6H35V7H34zM35,6H36V7H35zM36,6H37V7H36zM38,6H39V7H38zM2,7H3V8H2zM8,7H9V8H8zM14,7H15V8H14zM15,7H16V8H15zM16,7H17V8H16zM17,7H18V8H17zM20,7H21V8H20zM21,7H22V8H21zM23,7H24V8H23zM24,7H25V8H24zM25,7H26V8H25zM26,7H27V8H26zM27,7H28V8H27zM28,7H29V8H28zM30,7H31V8H30zM32,7H33V8H32zM38,7H39V8H38zM2,8H3V9H2zM3,8H4V9H3zM4,8H5V9H4zM5,8H6V9H5zM6,8H7V9H6zM7,8H8V9H7zM8,8H9V9H8zM10,8H11V9H10zM12,8H13V9H12zM14,8H15V9H14zM16,8H17V9H16zM18,8H19V9H18zM20,8H21V9H20zM22,8H23V9H22zM24,8H25V9H24zM26,8H27V9H26zM28,8H29V9H28zM30,8H31V9H30zM32,8H33V9H32zM33,8H34V9H33zM34,8H35V9H34zM35,8H36V9H35zM36,8H37V9H36zM37,8H38V9H37zM38,8H39V9H38zM10,9H11V10H10zM11,9H12V10H11zM13,9H14V10H13zM16,9H17V10H16zM18,9H19V10H18zM22,9H23V10H22zM23,9H24V10H23zM26,9H27V10H26zM2,10H3V11H2zM8,10H9V11H8zM10,10H11V11H10zM11,10H12V11H11zM14,10H15V11H14zM20,10H21V11H20zM21,10H22V11H21zM22,10H23V11H22zM23,10H24V11H23zM24,10H25V11H24zM26,10H27V11H26zM28,10H29V11H28zM29,10H30V11H29zM30,10H31V11H30zM31,10H32V11H31zM32,10H33V11H32zM35,10H36V11H35zM36,10H37V11H36zM37,10H38V11H37zM5,11H6V12H5zM7,11H8V12H7zM9,11H10V12H9zM13,11H14V12H13zM14,11H15V12H14zM15,11H16V12H15zM16,11H17V12H16zM22,11H23V12H22zM24,11H25V12H24zM27,11H28V12H27zM28,11H29V12H28zM29,11H30V12H29zM31,11H32V12H31zM32,11H33V12H32zM33,11H34V12H33zM34,11H35V12H34zM3,12H4V13H3zM5,12H6V13H5zM7,12H8V13H7zM8,12H9V13H8zM11,12H12V13H11zM12,12H13V13H12zM13,12H14V13H13zM19,12H20V13H19zM20,12H21V13H20zM22,12H23V13H22zM23,12H24V13H23zM24,12H25V13H24zM25,12H26V13H25zM26,12H27V13H26zM27,12H28V13H27zM31,12H32V13H31zM32,12H33V13H32zM35,12H36V13H35zM2,13H3V14H2zM4,13H5V14H4zM5,13H6V14H5zM6,13H7V14H6zM9,13H10V14H9zM11,13H12V14H11zM12,13H13V14H12zM14,13H15V14H14zM15,13H16V14H15zM17,13H18V14H17zM18,13H19V14H18zM19,13H20V14H19zM20,13H21V14H20zM24,13H25V14H24zM25,13H26V14H25zM28,13H29V14H28zM29,13H30V14H29zM32,13H33V14H32zM36,13H37V14H36zM38,13H39V14H38zM3,14H4V15H3zM8,14H9V15H8zM9,14H10V15H9zM10,14H11V15H10zM11,14H12V15H11zM14,14H15V15H14zM15,14H16V15H15zM16,14H17V15H16zM18,14H19V15H18zM21,14H22V15H21zM23,14H24V15H23zM27,14H28V15H27zM28,14H29V15H28zM29,14H30V15H29zM30,14H31V15H30zM31,14H32V15H31zM37,14H38V15H37zM38,14H39V15H38zM2,15H3V16H2zM3,15H4V16H3zM7,15H8V16H7zM11,15H12V16H11zM14,15H15V16H14zM16,15H17V16H16zM17,15H18V16H17zM18,15H19V16H18zM19,15H20V16H19zM20,15H21V16H20zM22,15H23V16H22zM24,15H25V16H24zM28,15H29V16H28zM29,15H30V16H29zM31,15H32V16H31zM32,15H33V16H32zM34,15H35V16H34zM35,15H36V16H35zM36,15H37V16H36zM37,15H38V16H37zM2,16H3V17H2zM3,16H4V17H3zM4,16H5V17H4zM6,16H7V17H6zM8,16H9V17H8zM9,16H10V17H9zM10,16H11V17H10zM11,16H12V17H11zM12,16H13V17H12zM14,16H15V17H14zM15,16H16V17H15zM16,16H17V17H16zM17,16H18V17H17zM18,16H19V17H18zM19,16H20V17H19zM21,16H22V17H21zM23,16H24V17H23zM27,16H28V17H27zM32,16H33V17H32zM35,16H36V17H35zM3,17H4V18H3zM7,17H8V18H7zM11,17H12V18H11zM13,17H14V18H13zM14,17H15V18H14zM15,17H16V18H15zM18,17H19V18H18zM19,17H20V18H19zM20,17H21V18H20zM21,17H22V18H21zM29,17H30V18H29zM34,17H35V18H34zM4,18H5V19H4zM5,18H6V19H5zM6,18H7V19H6zM7,18H8V19H7zM8,18H9V19H8zM9,18H10V19H9zM11,18H12V19H11zM12,18H13V19H12zM16,18H17V19H16zM18,18H19V19H18zM23,18H24V19H23zM26,18H27V19H26zM27,18H28V19H27zM28,18H29V19H28zM30,18H31V19H30zM31,18H32V19H31zM33,18H34V19H33zM34,18H35V19H34zM37,18H38V19H37zM2,19H3V20H2zM3,19H4V20H3zM10,19H11V20H10zM13,19H14V20H13zM14,19H15V20H14zM15,19H16V20H15zM16,19H17V20H16zM20,19H21V20H20zM22,19H23V20H22zM23,19H24V20H23zM24,19H25V20H24zM26,19H27V20H26zM28,19H29V20H28zM32,19H33V20H32zM34,19H35V20H34zM36,19H37V20H36zM38,19H39V20H38zM3,20H4V21H3zM4,20H5V21H4zM6,20H7V21H6zM7,20H8V21H7zM8,20H9V21H8zM10,20H11V21H10zM11,20H12V21H11zM12,20H13V21H12zM16,20H17V21H16zM17,20H18V21H17zM19,20H20V21H19zM22,20H23V21H22zM23,20H24V21H23zM24,20H25V21H24zM25,20H26V21H25zM26,20H27V21H26zM27,20H28V21H27zM29,20H30V21H29zM34,20H35V21H34zM38,20H39V21H38zM2,21H3V22H2zM4,21H5V22H4zM5,21H6V22H5zM7,21H8V22H7zM10,21H11V22H10zM11,21H12V22H11zM14,21H15V22H14zM15,21H16V22H15zM16,21H17V22H16zM19,21H20V22H19zM20,21H21V22H20zM21,21H22V22H21zM22,21H23V22H22zM23,21H24V22H23zM27,21H28V22H27zM28,21H29V22H28zM30,21H31V22H30zM31,21H32V22H31zM32,21H33V22H32zM34,21H35V22H34zM36,21H37V22H36zM37,21H38V22H37zM38,21H39V22H38zM2,22H3V23H2zM3,22H4V23H3zM4,22H5V23H4zM6,22H7V23H6zM8,22H9V23H8zM9,22H10V23H9zM10,22H11V23H10zM15,22H16V23H15zM17,22H18V23H17zM18,22H19V23H18zM19,22H20V23H19zM20,22H21V23H20zM21,22H22V23H21zM22,22H23V23H22zM24,22H25V23H24zM27,22H28V23H27zM31,22H32V23H31zM32,22H33V23H32zM33,22H34V23H33zM34,22H35V23H34zM36,22H37V23H36zM37,22H38V23H37zM2,23H3V24H2zM5,23H6V24H5zM9,23H10V24H9zM13,23H14V24H13zM14,23H15V24H14zM16,23H17V24H16zM17,23H18V24H17zM20,23H21V24H20zM23,23H24V24H23zM24,23H25V24H24zM26,23H27V24H26zM28,23H29V24H28zM29,23H30V24H29zM31,23H32V24H31zM32,23H33V24H32zM35,23H36V24H35zM36,23H37V24H36zM37,23H38V24H37zM2,24H3V25H2zM3,24H4V25H3zM8,24H9V25H8zM11,24H12V25H11zM12,24H13V25H12zM14,24H15V25H14zM15,24H16V25H15zM16,24H17V25H16zM17,24H18V25H17zM20,24H21V25H20zM24,24H25V25H24zM25,24H26V25H25zM27,24H28V25H27zM29,24H30V25H29zM32,24H33V25H32zM33,24H34V25H33zM37,24H38V25H37zM3,25H4V26H3zM5,25H6V26H5zM7,25H8V26H7zM12,25H13V26H12zM13,25H14V26H13zM14,25H15V26H14zM16,25H17V26H16zM17,25H18V26H17zM21,25H22V26H21zM23,25H24V26H23zM26,25H27V26H26zM27,25H28V26H27zM28,25H29V26H28zM30,25H31V26H30zM35,25H36V26H35zM36,25H37V26H36zM37,25H38V26H37zM38,25H39V26H38zM2,26H3V27H2zM4,26H5V27H4zM5,26H6V27H5zM6,26H7V27H6zM7,26H8V27H7zM8,26H9V27H8zM9,26H10V27H9zM14,26H15V27H14zM15,26H16V27H15zM18,26H19V27H18zM19,26H20V27H19zM20,26H21V27H20zM21,26H22V27H21zM23,26H24V27H23zM27,26H28V27H27zM29,26H30V27H29zM31,26H32V27H31zM33,26H34V27H33zM34,26H35V27H34zM36,26H37V27H36zM2,27H3V28H2zM5,27H6V28H5zM9,27H10V28H9zM11,27H12V28H11zM12,27H13V28H12zM13,27H14V28H13zM14,27H15V28H14zM16,27H17V28H16zM18,27H19V28H18zM19,27H20V28H19zM25,27H26V28H25zM32,27H33V28H32zM33,27H34V28H33zM38,27H39V28H38zM2,28H3V29H2zM6,28H7V29H6zM8,28H9V29H8zM10,28H11V29H10zM12,28H13V29H12zM13,28H14V29H13zM15,28H16V29H15zM22,28H23V29H22zM23,28H24V29H23zM25,28H26V29H25zM27,28H28V29H27zM28,28H29V29H28zM30,28H31V29H30zM31,28H32V29H31zM35,28H36V29H35zM37,28H38V29H37zM38,28H39V29H38zM2,29H3V30H2zM4,29H5V30H4zM6,29H7V30H6zM7,29H8V30H7zM11,29H12V30H11zM13,29H14V30H13zM16,29H17V30H16zM17,29H18V30H17zM19,29H20V30H19zM22,29H23V30H22zM23,29H24V30H23zM25,29H26V30H25zM27,29H28V30H27zM28,29H29V30H28zM29,29H30V30H29zM31,29H32V30H31zM32,29H33V30H32zM36,29H37V30H36zM37,29H38V30H37zM2,30H3V31H2zM5,30H6V31H5zM8,30H9V31H8zM15,30H16V31H15zM16,30H17V31H16zM20,30H21V31H20zM25,30H26V31H25zM26,30H27V31H26zM27,30H28V31H27zM30,30H31V31H30zM31,30H32V31H31zM32,30H33V31H32zM33,30H34V31H33zM34,30H35V31H34zM36,30H37V31H36zM37,30H38V31H37zM10,31H11V32H10zM11,31H12V32H11zM13,31H14V32H13zM17,31H18V32H17zM19,31H20V32H19zM21,31H22V32H21zM22,31H23V32H22zM24,31H25V32H24zM25,31H26V32H25zM27,31H28V32H27zM28,31H29V32H28zM29,31H30V32H29zM30,31H31V32H30zM34,31H35V32H34zM35,31H36V32H35zM36,31H37V32H36zM37,31H38V32H37zM38,31H39V32H38zM2,32H3V33H2zM3,32H4V33H3zM4,32H5V33H4zM5,32H6V33H5zM6,32H7V33H6zM7,32H8V33H7zM8,32H9V33H8zM11,32H12V33H11zM13,32H14V33H13zM15,32H16V33H15zM16,32H17V33H16zM17,32H18V33H17zM21,32H22V33H21zM22,32H23V33H22zM23,32H24V33H23zM24,32H25V33H24zM28,32H29V33H28zM30,32H31V33H30zM32,32H33V33H32zM34,32H35V33H34zM2,33H3V34H2zM8,33H9V34H8zM12,33H13V34H12zM13,33H14V34H13zM14,33H15V34H14zM15,33H16V34H15zM16,33H17V34H16zM19,33H20V34H19zM20,33H21V34H20zM21,33H22V34H21zM22,33H23V34H22zM24,33H25V34H24zM28,33H29V34H28zM30,33H31V34H30zM34,33H35V34H34zM36,33H37V34H36zM37,33H38V34H37zM38,33H39V34H38zM2,34H3V35H2zM4,34H5V35H4zM5,34H6V35H5zM6,34H7V35H6zM8,34H9V35H8zM11,34H12V35H11zM15,34H16V35H15zM16,34H17V35H16zM18,34H19V35H18zM23,34H24V35H23zM25,34H26V35H25zM26,34H27V35H26zM27,34H28V35H27zM30,34H31V35H30zM31,34H32V35H31zM32,34H33V35H32zM33,34H34V35H33zM34,34H35V35H34zM36,34H37V35H36zM38,34H39V35H38zM2,35H3V36H2zM4,35H5V36H4zM5,35H6V36H5zM6,35H7V36H6zM8,35H9V36H8zM11,35H12V36H11zM14,35H15V36H14zM15,35H16V36H15zM16,35H17V36H16zM18,35H19V36H18zM20,35H21V36H20zM25,35H26V36H25zM28,35H29V36H28zM29,35H30V36H29zM30,35H31V36H30zM31,35H32V36H31zM34,35H35V36H34zM36,35H37V36H36zM2,36H3V37H2zM4,36H5V37H4zM5,36H6V37H5zM6,36H7V37H6zM8,36H9V37H8zM15,36H16V37H15zM16,36H17V37H16zM17,36H18V37H17zM18,36H19V37H18zM19,36H20V37H19zM20,36H21V37H20zM21,36H22V37H21zM23,36H24V37H23zM30,36H31V37H30zM32,36H33V37H32zM33,36H34V37H33zM35,36H36V37H35zM36,36H37V37H36zM38,36H39V37H38zM2,37H3V38H2zM8,37H9V38H8zM15,37H16V38H15zM19,37H20V38H19zM20,37H21V38H20zM21,37H22V38H21zM22,37H23V38H22zM28,37H29V38H28zM29,37H30V38H29zM30,37H31V38H30zM32,37H33V38H32zM33,37H34V38H33zM34,37H35V38H34zM35,37H36V38H35zM36,37H37V38H36zM2,38H3V39H2zM3,38H4V39H3zM4,38H5V39H4zM5,38H6V39H5zM6,38H7V39H6zM7,38H8V39H7zM8,38H9V39H8zM10,38H11V39H10zM13,38H14V39H13zM15,38H16V39H15zM17,38H18V39H17zM18,38H19V39H18zM19,38H20V39H19zM22,38H23V39H22zM23,38H24V39H23zM25,38H26V39H25zM28,38H29V39H28zM31,38H32V39H31zM33,38H34V39H33zM34,38H35V39H34zM36,38H37V39H36zM37,38H38V39H37zM38,38H39V39H38z" id="qr-path" fill="#000000" fill-opacity="1" fill-rule="nonzero" stroke="none"/></svg>',
  },
};
// Six months of finished transfers for the History page.
{
  let seed = 7;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  const peers = [
    { peerId: "a1b2c3d4e5", peerName: "Dhaka-Edit-Bay", words: ["cedar", "lantern", "otter", "quartz", "violet"], dir: 0.9 },
    { peerId: "77aa88bb99", peerName: "Lena's laptop", words: ["harbor", "quill", "maple", "orbit", "tulip"], dir: 0.15 },
    { peerId: "ffee", peerName: "Home server", words: ["bison", "ember", "kettle", "plum", "summit"], dir: 0.6 },
  ];
  const kinds: [string, string, number, number][] = [
    ["scene-%03d-take-%d", "mov", 1.2, 14],
    ["A%03d_C%03d_RAW", "braw", 4, 38],
    ["plate-%03d", "exr", 0.05, 0.4],
    ["still-%04d", "jpg", 0.006, 0.02],
    ["stems-reel-%d", "wav", 0.3, 2.4],
    ["edit-notes-%d", "pdf", 0.001, 0.01],
    ["dailies-%03d", "zip", 2, 22],
    ["grade-lut-pack-%d", "7z", 0.02, 0.3],
    ["render-node-%d", "iso", 3, 6],
    ["dataset-shard-%03d", "tar", 1, 9],
  ];
  const fmt = (p: string) => p.replace(/%0?(\d)?d/g, () => String(Math.floor(rnd() * 240) + 1).padStart(3, "0"));
  const out: any[] = [];
  let id = 0;
  const start = new Date(new Date(now).getFullYear(), new Date(now).getMonth() - 5, 1).getTime();
  for (let t = start; t < now; t += 86400e3) {
    const d = new Date(t);
    const weekend = d.getDay() === 0 || d.getDay() === 6;
    if (rnd() < (weekend ? 0.75 : 0.35)) continue;
    const n = 1 + Math.floor(rnd() * rnd() * 7);
    for (let i = 0; i < n; i++) {
      const p = peers[Math.floor(rnd() * rnd() * peers.length)];
      const k = kinds[Math.floor(rnd() * kinds.length)];
      const size = Math.round((k[2] + rnd() * rnd() * (k[3] - k[2])) * GB);
      const relay = rnd() < 0.06 ? Math.round(size * (0.1 + rnd() * 0.4)) : 0;
      const finishedAt = t + (9 + rnd() * 12) * 3600e3;
      if (finishedAt > now) continue;
      const secs = size / ((6 + rnd() * 9) * 1024 * 1024);
      out.push({
        id: `h${id++}`, direction: rnd() < p.dir ? "received" : "sent", peerId: p.peerId, peerName: p.peerName,
        name: `${fmt(k[0])}.${k[1]}`, ext: k[1], size, startedAt: finishedAt - secs * 1000, finishedAt,
        activeMs: Math.round(secs * 1000), directBytes: size - relay, relayBytes: relay, peakBps: size / secs * (1.2 + rnd() * 0.5),
        blocks: Math.ceil(size / (1024 * 1024)),
        fingerprint: Array.from({ length: 64 }, () => "0123456789abcdef"[Math.floor(rnd() * 16)]).join(""),
        safetyWords: p.words,
      });
    }
  }
  data.list_history = out.sort((a, b) => b.finishedAt - a.finishedAt);
}
data.list_outgoing = [
  { id: "o1", peerId: "77aa88bb99", deviceName: "Lena's laptop", shareName: "Client deliveries", name: "scene-014-take-2.mov", path: ["scene-014-take-2.mov"], size: 6.4 * GB, sentBytes: 2.71 * GB, bps: 3.4 * 1024 * 1024, status: "sending", startedAt: now - 900e3, updatedAt: now },
  { id: "o2", peerId: "77aa88bb99", deviceName: "Lena's laptop", shareName: "Client deliveries", name: "color-grade-notes.pdf", path: ["color-grade-notes.pdf"], size: 4.2 * 1024 * 1024, sentBytes: 4.2 * 1024 * 1024, bps: 0, status: "verified", startedAt: now - 1200e3, updatedAt: now - 600e3 },
];
data["plugin:relayless-android|has_all_files_access"] = { granted: false };
data["plugin:relayless-android|battery_status"] = { unrestricted: false };
data["list_local_dir"] = { path: "/storage/emulated/0", parent: null, root: "/storage/emulated/0", dirs: ["Alarms", "DCIM", "Documents", "Download", "Movies", "Music", "Pictures"] };
export async function invoke<T>(cmd: string, _args?: any): Promise<T> {
  await new Promise((r) => setTimeout(r, 30));
  if (cmd in data) return JSON.parse(JSON.stringify(data[cmd]));
  return undefined as T;
}
