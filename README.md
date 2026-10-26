# Relayless

Send very large files (tens or hundreds of GB) directly from one Windows PC to another over the internet. No cloud upload, no size limit, and a transfer survives dropped connections, app restarts and reboots without starting over.

> Your files stay on your devices. Your transfer survives interruptions.
> (In the default direct-only mode, file data never passes through any server. If you enable relay fallback, encrypted data may pass through a public relay when routers block a direct path; the app always shows which route is in use.)

## Install

Download `Relayless-Setup-<version>-x64.exe` from the [Releases](../../releases) page and run it. It installs per user, no administrator rights needed. The installer isn't code-signed yet, so SmartScreen may warn: choose **More info → Run anyway**.

## Use it

**Sender** (the computer that has the files)
1. **Share a folder** and pick the folder. Only that folder is reachable; whole drives can't be shared.
2. Click **Invite**. Send the invitation text (or read out the short code, valid 30 minutes) to the receiver.
3. When their device connects, approve it. Keep Relayless running (it can sit in the tray) while they download.

**Receiver**
1. Click **Connect**, paste the invitation or type the code.
2. Browse the shared folder, select files or folders, **Download selected**.
3. Watch progress in **Transfers**. Pause, resume or retry any time.

If either side goes offline, the transfer shows *Waiting for sender* or *Reconnecting* and continues from the last verified piece on its own. After a reboot it resumes when Relayless starts again (turn on *Start Relayless when I sign in* to make that automatic).

## Speed without hogging the computer

- **Direct QUIC connection with NAT hole punching** (via [iroh](https://iroh.computer)), so data takes the shortest path between the two PCs.
- **BBRv3 congestion control** keeps throughput high on long, lossy intercontinental links where classic algorithms back off too much.
- **12 parallel streams per file** (configurable up to 32) keep a high-latency link full. 12 streams sustain roughly 480 Mbit/s at 200 ms round-trip.
- **Streaming I/O in 1 MiB blocks**: files are never loaded into memory. Measured peak memory for a 1 GiB loopback transfer was about 110 MB for sender and receiver together.
- **BLAKE3 hashing** (several times faster than SHA-256) and UDP segmentation offload keep CPU per byte low.
- **Below-normal process priority** by default, so other programs stay responsive. Network speed is unaffected.
- **Closing the window frees the UI's memory**; transfers keep running in the tray.
- Optional download and upload limits if you need to leave bandwidth for others.

Real-world speed is capped by the slowest link: usually the sender's upload speed.

## How transfers stay correct

- Files are split into pieces (16 MB by default). Each piece is hashed by the sender and checked by the receiver as it arrives.
- A piece is recorded as done only after its bytes are flushed to disk, so the saved progress never claims more than what's really on disk, even after a crash or power cut.
- Downloads are written to `name.rlpart`. When every piece is present, the whole file is re-read and re-verified, then renamed. Corrupted pieces are fetched again.
- If the source file changes on the sender's PC mid-transfer, the download stops instead of mixing two versions, and offers to restart with the new version.

## Security

- Every connection is end-to-end encrypted (QUIC/TLS 1.3) and authenticated by each device's Ed25519 key. The receiver verifies it reached the exact sender named in the invitation.
- Invitations hold a random 128-bit key; only its hash is stored. They expire, can be limited to N devices, and can be revoked. New devices need the sender's approval by default.
- Short codes are stretched with Argon2id and only unlock an encrypted record; the lookup service never sees the invitation in plain text.
- The sender enforces access on every request: path traversal, symlinks and junctions that escape the shared folder are rejected; receivers can't modify or delete anything.
- Remote file names are sanitized before anything is written locally.

## Build from source

Requirements: Rust (stable), Node.js 22, and on Windows the WebView2 runtime (preinstalled on Windows 10/11).

```sh
npm ci
npm run tauri dev                       # run the app
npx tauri build --bundles nsis          # build the installer
cargo test -p relayless-core --release  # engine tests: real transfers, pause/resume, crash recovery, corruption, source changes, path traversal
```

CI (`.github/workflows/build.yml`) runs the engine tests on Linux and Windows, builds the installer, then installs and launches it on a clean Windows runner as a smoke test. Every run uploads the installer as a build artifact.

To publish a release: on GitHub, **Releases → Draft a new release**, create a tag such as `v0.1.0`, and publish. CI builds the installer and attaches it to that release (pushing a `v*` tag does the same). Bump `version` in `src-tauri/tauri.conf.json` and `Cargo.toml` for each new release.

## Layout

```
crates/core   Rust engine: protocol, sharing and permissions, invitations, resumable download engine, SQLite state
src-tauri     Desktop shell: commands, events, tray, notifications
src           React + TypeScript UI
```

Data and logs live in `%APPDATA%\app.relayless.desktop`.
