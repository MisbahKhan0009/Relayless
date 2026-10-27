# Architecture

Relayless has three layers:

| Layer | Location | Responsibility |
|---|---|---|
| Transfer engine | `crates/core` | Networking, sharing and permissions, invitations, the resumable download engine, durable state. Plain Rust library with no UI dependencies. |
| App shell | `src-tauri`, `plugins/relayless-android` | Starts the engine, exposes it to the interface as typed commands and events, and integrates with the platform (Windows tray and autostart; Android storage access, foreground service, notifications). |
| Interface | `src` | React + TypeScript. Holds no transfer logic: it calls commands and renders events. |

## Connectivity

Each installation has a persistent Ed25519 key (`identity.key` in the app data folder); its public key is the device ID. Connections use [iroh](https://github.com/n0-computer/iroh), which provides QUIC (TLS 1.3) between public keys, NAT traversal (hole punching), address lookup over DNS/pkarr, and relay fallback.

- **Route visibility.** The engine watches the connection's selected path and reports `direct` or `relay` to the UI.
- **Direct-only mode (default).** The receiver doesn't request file data and the sender refuses to serve it while the selected path is a relay. Both sides re-check during a transfer.
- **Tuning.** BBRv3 congestion control; per-stream receive windows below 1 MB (the QUIC stack aborts a stream whose reassembly buffer exceeds 1024 fragments, which a large window allows under heavy loss); throughput comes from many parallel chunk streams instead.

## Invitations and pairing

An invitation (`RL1-…`) encodes the sender's public key, a random 128-bit token and the sender's home relay URL. The sender stores only a BLAKE3 hash of the token, with expiry, a use limit and a revocation flag.

A short code (`K7QM-9XRT-4HWD`, about 59 bits) is optional. Argon2id derives two keys from it: an Ed25519 lookup key and a ChaCha20-Poly1305 key. The sender publishes the encrypted invitation under the lookup key for up to 30 minutes; the receiver derives the same keys, fetches and decrypts it. The lookup service sees only ciphertext.

Both devices show **safety words** during pairing: BLAKE3 over a domain tag and the two Ed25519 public keys, sorted so both sides agree, mapped to five words from a fixed 256-word list (40 bits). A device in the middle would hold a different key and so produce different words; comparing them out loud closes the gap if an invitation was intercepted and swapped.

On first contact the receiver sends `Hello { token }`. The sender validates the token and, unless the invitation was created without approval, asks the user to approve the device. Approved devices are stored as grants `(device, folder)`; later connections authenticate by device key alone.

## Wire protocol (v2)

Every request opens a new bidirectional QUIC stream. The client writes one length-prefixed JSON request and finishes its side; the server replies with length-prefixed JSON frames.

| Request | Reply |
|---|---|
| `Hello { v, token?, device_name }` | `Hello { status, sender_name, shares }`; may first send `Pending` while waiting for approval |
| `List { share, path, offset, limit }` | One page of a directory (at most 1000 entries) |
| `Walk { share, path }` | All files below a folder (at most 50,000) |
| `Stat { share, path }` | Size, modification time, folder flag |
| `Chunk { share, path, size, mtime_ms, chunk_size, index, from }` | `Chunk { len }`, then the data |
| `Receipt { share, path, size, job, fingerprint, started_at, finished_at, active_ms, direct_bytes, relay_bytes, peak_bps, blocks }` | `Ack`. Sent once by the receiver after a file is verified, so the sender can keep a history entry. The sender only records it for a file that exists in the share at that size. Older senders reject the unknown request, which is harmless. |

Chunk data is sent in 1 MiB blocks, each followed by the 32-byte BLAKE3 hash of that block. `from` is a block-aligned offset inside the chunk, used to resume a partly received chunk. The sender re-checks the file's size and modification time after reading every block and resets the stream if the file changed, so a block hash never covers bytes from two versions.

Every request is authorized on the sender against the stored grants. Paths are resolved by canonicalization and must stay inside the shared folder; symlinks and junctions that leave it are refused. All lengths, offsets and indices from the network are bounds-checked.

## Resumable downloads

A download job records the remote file's identity (path, size, modification time), the chunk size (8–32 MiB, 16 MiB by default) and the destination. Data is written to `<name>.rlpart`, pre-allocated to the full size.

1. Workers (12 by default) take chunks from a queue, finishing part-done chunks first.
2. For each block: read data and hash, verify, write at the block's offset, report progress.
3. A committer collects progress, calls `fsync` on the file, then records it in SQLite in one transaction: finished chunks (with a chunk hash derived from their block hashes) and the verified prefix of unfinished chunks.
4. Progress is committed every 2 seconds and whenever workers stop (pause, disconnect, shutdown), so the database never claims more than what is on disk.
5. When every chunk is recorded, the whole file is re-read and each chunk is verified again. Bad chunks are dropped and fetched again; otherwise the file is renamed to its final name (with ` (1)` etc. if the name is taken).

Network errors retry with exponential backoff and jitter; a missing sender shows *Waiting for sender*. Jobs survive restarts: on launch, unfinished jobs restart and reconcile with the partial file (a missing or resized partial file resets that job's progress).

## Receipts and history

Each job counts bytes received over a direct path and through a relay (sampled from the connection's current path for every block), active transfer time and peak speed, and saves them with its progress. When the file is verified, the receiver writes a `history` row: direction, device, file name (unless the user turned names off) and extension, size, times, route bytes, block count, and a fingerprint, BLAKE3 over the file's chunk hashes in order. It then sends `Receipt` to the sender, which stores the matching `sent` row with the same fingerprint.

History never leaves the device. A retention setting (1 year by default; 90 or 30 days, forever, or off) prunes old rows on start, every hour and whenever it changes; *off* deletes everything. The History page computes the calendar, device-pair totals and file-type breakdown from these rows in the interface.

## State

SQLite in WAL mode (`relayless.db` in the app data folder):

| Table | Contents |
|---|---|
| `settings` | User settings (JSON) |
| `shares`, `invitations`, `grants` | Sender side: shared folders, invitation hashes and limits, approved devices |
| `remotes`, `remote_shares` | Receiver side: paired senders and their folders |
| `jobs`, `chunks`, `partials` | Downloads, finished chunks, verified prefixes of unfinished chunks |
| `history` | Finished transfers and their receipts, kept for the retention period |
| `activity` | Recent activity log (no file contents or secrets) |

App data lives in `%APPDATA%\app.relayless.desktop` on Windows and in the app's private storage on Android.

## Platform integration

**Windows.** Single instance; closing the window keeps transfers running in the tray and destroys the WebView to free its memory; optional start at sign-in (minimized); below-normal process priority; source files are opened deny-write while read.

**Android.** A foreground service with a live notification, a partial wake lock and a high-performance Wi-Fi lock keeps transfers running with the screen off; it stops two minutes after activity ends. *All files access* lets users share and save to real folders (downloads default to `Download/Relayless`).

## Android signing

Android installs an update over an existing app only if both are signed with the same key. CI signs release APKs with:

- the repository secrets `ANDROID_KEYSTORE_BASE64` (a keystore with key alias `relayless`, base64-encoded) and `ANDROID_KEYSTORE_PASSWORD`, if set; otherwise
- a key generated once and kept in the Actions cache (`relayless-signing-key-v1`).

For long-term releases, create a keystore, back it up, and add it as secrets:

```sh
keytool -genkeypair -keystore relayless.jks -storetype PKCS12 -alias relayless \
  -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Relayless"
base64 -w0 relayless.jks   # value for ANDROID_KEYSTORE_BASE64
```

Switching keys means existing users must uninstall once before updating.

## Testing

`cargo test -p relayless-core --release` runs unit tests and end-to-end tests with two in-process peers on loopback:

- full transfer of large files and folders, including empty files
- path traversal and symlink escapes, single-use invitations, device revocation
- pause and resume, simulated crash and restart, corrupted partial data
- pausing before any chunk finishes (mid-chunk resume), across an app restart
- sender going offline and returning, source file modified mid-transfer

CI also installs and launches the Windows installer and the Android APK (Android 13 emulator) on every push.
