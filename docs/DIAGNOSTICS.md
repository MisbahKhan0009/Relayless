# Transfer diagnostics

Relayless writes one JSON summary per transfer and readable log lines, so a slow transfer can be explained with measurements instead of guesses. This page covers what is recorded, where it goes, how to switch it off, and a repeatable test procedure.

Diagnostics never change the wire protocol (`relayless/1`, protocol version 2) or any default setting.

## Switching it on and off

| Where | How |
|---|---|
| Desktop and Android app | On by default (`CoreOptions::diagnostics = true` in `src-tauri/src/lib.rs`) |
| Any build | Environment variable `RELAYLESS_DIAG=0` (or `off`, `false`, `no`) turns it off; `1`/`on` forces it on |
| Tests | `CoreOptions { diagnostics: true/false, .. }` |
| Adaptive flow window (not diagnostics, but useful for A/B tests) | `RELAYLESS_ADAPTIVE_WINDOW=0` restores the old static 48 MiB connection window on the receiver |

"Off" stops the per-job sampler task, interval and summary log lines, and the JSON file. The counters themselves stay on: they are a few `Instant::now()` calls and atomic adds per 1 MiB block. On loopback, 4 runs each of the 160 MB e2e transfer gave 173–215 MB/s before the change, 188–210 MB/s with diagnostics on and 202–216 MB/s with them off, so the overhead is below the noise.

## Where output goes

| Output | Location |
|---|---|
| JSON summaries, one per line | `<app data>/diagnostics/transfers.jsonl` (rotated to `transfers.old.jsonl` at 16 MiB). Windows: `%APPDATA%\app.relayless.desktop\diagnostics\` |
| Readable lines | The normal log, `<app data>/logs/relayless.log`, target `relayless_core::diag` |
| Per-chunk lines | Same log at debug level: `RELAYLESS_LOG=relayless_core::diag=debug` |

Log line kinds:

- `engine_config ...` at start: diagnostics switch, active and configured congestion controller, streams, jobs, chunk size, both limits (KiB/s), QUIC windows.
- `settings_changed ...` when a limit, stream count, route mode or congestion setting changes, with the effective bytes/s.
- `transfer_interval role=receiver|sender ...` every 5 s per active transfer: bytes and MiB/s for the interval, route from the path API, active streams, RTT, cwnd, loss, limiter wait and read or write wait. The sender line also has pacing rate and bandwidth estimate.
- `route_change role=receiver ... from= to=` whenever the selected path changes between direct, relay and unknown.
- `transfer_summary role=... outcome=...` once per transfer, a one-line digest of the JSON summary.
- `connection_closed role=sender ... requests_in_flight_peak= limit=64` when a receiver disconnects.

## What a "transfer" is

- **Receiver:** one run of a download job, from start (or resume) to exit (completed, paused, canceled, shutdown, failed, needs attention). A paused and resumed job produces two summaries with the same `transfer_id` (the job id); the second has `verified_bytes_at_start > 0`.
- **Sender:** one file served to one receiver, closed when the receipt arrives (`receipt_received`), after 15 s without requests (`idle`), on shutdown (`shutdown`) or when the file's size changes (`source_changed`).
- **Matching the two sides:** both summaries carry the same `file_key` (a BLAKE3 hash of share id and path, 16 hex characters), and each names the other side in `peer` (first 8 characters of its public key). File names, paths, IP addresses, keys and tokens are never written.

## Summary fields (schema_version 1)

Durations are milliseconds (`*_ms`), rates are bytes/s or MiB/s as named, sizes are bytes. A value that could not be measured is `null`, never `0`.

| Section | Contents |
|---|---|
| `route` | `initial`, `final` (`direct`, `relay`, `unknown`), `changes` and `change_log` (time, from, to, RTT), `direct_only`, `hole_punch` (`wait_ms`, outcome: `already_direct`, `direct_found`, `timeout`, `cancelled`, `not_applicable_direct_only`), `bytes_by_route` (bytes counted while the selected path was direct, relay or unknown) |
| `configuration` | Chunk size (from the job), block size, total chunks, configured streams and workers actually started, max jobs, route mode, transport values passed to QUIC, active congestion controller, both limits (`configured_value` in KiB/s, `effective_bytes_per_sec`, `unlimited`), timeouts |
| `connection` | `attempts` (reused or dialed, lock wait, dial and Hello time, error), `at_start` and `at_end` path reports (per path: kind, selected, IP scope, relay URL, RTT, cwnd, pacing rate, bandwidth estimate, sent and lost packets, congestion events, MTU; connection totals including `STREAMS_BLOCKED` frames), `close_reason` |
| `transfer` | File size, `bytes_transferred` in this run, `elapsed_ms`, `rates` (whole run, active window, peak 5 s interval), blocks, chunks started, completed, failed, stopped and resumed, retries, hash failures, `active_streams` (current, peak, time-weighted average, stream-seconds), `average_active_streams_in_active_window`, outcome, `error_category`. Receiver: `passes` (download passes that started workers), `workers_started`, slow stream opens, `status_time_ms` per job state, `connection_streams_peak` (streams open to this sender across all jobs, seen when each chunk started), `other_jobs_running_at_end`, process-wide limiter counters. `error_category` is the last error and is `null` when the transfer completed; chunks stopped by a pause or by a sibling worker's failure count in `chunks_stopped`, not in `errors`. Sender: chunk requests, peak requests in flight on the connection versus the limit of 64 |
| `timings_ms` | One entry per stage: `total_ms`, `count`, `max_ms`, `avg_ms`, and for byte stages `bytes` and `stage_mib_per_sec` (bytes divided by the time spent in that stage alone) |
| `flow_control_ceiling` | `STREAM_WINDOW / RTT` per stream and for the average stream count. Labelled as a theoretical ceiling, not a prediction |
| `errors` | Count per category (below) |
| `samples` | The 5 s interval samples (last 240) |
| `chunks` | Per chunk (first 1000): index, worker, start time, resume offset, bytes, blocks, route, stream open, request write, header wait, limiter, read wait or disk read, hash, disk write or QUIC write, blocking-pool queue, total, outcome, error |
| `unavailable` | Metrics this build cannot measure, with the reason |
| `diagnostic_notes` | Automatic observations, prefixed `observation:` or `note:`. They describe measurements and do not claim causes |

### Stages

Stage totals are summed over parallel workers, so they can add up to more than the wall-clock time. Compare a stage with `worker_time`.

| Receiver stage | Measures |
|---|---|
| `slot_wait` | Waiting for a free job slot (`max_concurrent_jobs`) |
| `connection_setup` | `get_conn`: lock wait plus dial and Hello when not reused |
| `hole_punch_wait` | Waiting up to 5 s for a direct path on the first attempt |
| `direct_only_wait` | Waiting for a direct path in direct-only mode |
| `backoff_wait` | Sleeping between attempts |
| `stat_request`, `prepare_partial` | Start-of-pass `Stat` round trip; opening and pre-allocating the `.rlpart` file |
| `stream_open` | `open_bi`. Slow opens (≥100 ms) are counted separately; the sender's 64-stream limit is the usual cause |
| `request_write`, `response_header_wait` | Writing the `Chunk` request; waiting for the `Chunk { len }` header (sender's file open plus one RTT plus scheduling) |
| `rate_limiter_wait` | Time the download limiter made a block wait (only non-zero waits are counted) |
| `quic_read_wait` | Time awaiting `recv.read` for block bytes and hashes, recorded once per block, or every second of waiting on slow links so interval samples stay live (`count` is the number of records, not reads). Includes network time, the sender's pace and local task scheduling; it cannot separate them |
| `hash_verify`, `disk_write`, `blocking_pool_queue` | BLAKE3 of each block, `write_all_at`, and the wait for a blocking thread, each measured separately inside the same blocking task |
| `sync_flush`, `db_commit` | `fsync` of the partial file and the SQLite progress transaction |
| `final_verification` (+`_read`, `_hash`), `rename` | Full re-read and re-hash after all chunks; final rename |

| Sender stage | Measures |
|---|---|
| `request_read` | Reading the request frame |
| `file_open` | Resolving, opening and checking the source file |
| `rate_limiter_wait` | Time the upload limiter made a block wait |
| `disk_read`, `metadata_check`, `hash`, `blocking_pool_queue` | `read_exact_at`, the size and mtime re-check, BLAKE3, and blocking-thread wait |
| `quic_write_blocked` | Time in `write_all` for block plus hash. Covers flow control, congestion control and send-buffer backpressure together; noq does not report which one. Recorded when the write returns, so an interval sample shows `quic_write_ms=0` while a write is still stuck and a large value in the interval it completes |
| `stream_finish_wait` | `send.stopped()` after `finish()`: waiting for the receiver to acknowledge the stream's last bytes |

### Error categories

`connect_failed`, `connection_lost`, `denied`, `route_unknown`, `relay_path_active`, `stream_open_failed`, `request_write_failed`, `response_timeout`, `stall_timeout`, `stream_reset`, `stream_closed_early`, `peer_stopped`, `sender_disk_read`, `receiver_disk_write`, `hash_mismatch`, `source_changed`, `remote_error`, `bad_request`, `cancelled`, `unknown`. They are diagnostic only; user-facing errors are unchanged.

### Direction matters for path statistics

`cwnd`, pacing rate, bandwidth estimate, `sent_*` and `lost_*` describe packets the reporting side sends. File data flows from sender to receiver, so the data direction's congestion window, pacing and loss are in the **sender's** summary and `transfer_interval role=sender` lines. The receiver's values cover its own packets: ACKs, requests and flow-control credit (`MAX_STREAM_DATA`) for the sender. A receiver controller that paces very slowly can therefore still hold the sender back; see the receiver's `pacing_bytes_s` in its interval lines.

## Example (real loopback run, trimmed)

From `bench_loopback` with a 64 MiB file, default settings (16 MiB chunks, so 4 workers):

```json
{
  "schema_version": 1, "kind": "transfer_summary", "role": "receiver",
  "transfer_id": "bdc3423a3cf47636e348089f", "file_key": "cc880726b33998aa", "peer": "04bcc366",
  "route": { "initial": "direct", "final": "direct", "changes": 0, "direct_only": false,
             "hole_punch": { "outcome": "already_direct", "wait_ms": 0.0 },
             "bytes_by_route": { "direct": 67108864, "relay": 0, "unknown": 0 } },
  "configuration": { "chunk_size_bytes": 16777216, "block_size_bytes": 1048576, "parallel_streams_setting": 12,
                     "workers_first_pass": 4, "max_concurrent_jobs": 2,
                     "transport": { "stream_window_bytes": 1000000, "connection_window_bytes": 50331648,
                                    "send_window_bytes": 50331648, "max_concurrent_bidi_streams": 64,
                                    "congestion_control_active": "bbr3" },   // older build; now "cubic" by default
                     "download_limit": { "configured_value": 0, "configured_unit": "KiB/s",
                                         "effective_bytes_per_sec": null, "unlimited": true } },
  "transfer": { "file_size_bytes": 67108864, "bytes_transferred": 67108864, "elapsed_ms": 414.343,
                "rates": { "whole_run_average_mib_per_sec": 154.461, "active_average_mib_per_sec": 168.412 },
                "chunks_completed": 4, "chunks_failed": 0, "hash_failures": 0,
                "active_streams": { "peak": 4 }, "outcome": "completed",
                "status_time_ms": { "connecting": 1.851, "downloading": 380.02, "verifying": 32.44 } },
  "timings_ms": { "quic_read_wait": { "total_ms": 1372.182, "count": 16413 },
                  "hash_verify": { "total_ms": 17.736, "stage_mib_per_sec": 3608.46 },
                  "disk_write": { "total_ms": 42.943, "stage_mib_per_sec": 1490.348 },
                  "sync_flush": { "total_ms": 19.26, "count": 1 },
                  "final_verification": { "total_ms": 31.667 },
                  "rate_limiter_wait": { "total_ms": 0.0, "count": 0 },
                  "worker_time": { "total_ms": 1439.8, "count": 4 } },
  "diagnostic_notes": [
    "observation: workers spent over 80% of their time waiting for incoming bytes; local disk and hashing were not the main time sink on this side"
  ]
}
```

## Test procedure

Keep the two computers, the test file, the destination disk and the direction fixed while changing one setting at a time. Repeat each configuration at least three times. Use a file of at least 1 GiB for real networks so start-up time doesn't dominate. Make one with:

```powershell
$f = [IO.File]::Create("D:\relayless-test\test-1g.bin"); $f.SetLength(1GB); $f.Close()   # sparse zeros, fine for network tests
# Random content (slower to create, avoids any compression or caching effects):
$b = New-Object byte[] (1MB); $r = [Random]::new(1); $s = [IO.File]::OpenWrite("D:\relayless-test\test-1g-rand.bin"); 1..1024 | % { $r.NextBytes($b); $s.Write($b,0,$b.Length) }; $s.Close()
```

Before each series, note the app version and both devices' Settings (streams, files at once, piece size, both limits, connection mode, relay servers). After each run, collect `transfers.jsonl` from **both** devices and match lines by `file_key`. `scripts/diag-report.ps1` prints a table:

```powershell
powershell -File scripts\diag-report.ps1 "$env:APPDATA\app.relayless.desktop\diagnostics\transfers.jsonl"
```

### Test A: baseline

Default settings, three runs. Record from the receiver: `transfer.rates.active_average_mib_per_sec`, `elapsed_ms`, `route.final`, `route.bytes_by_route`, both limits. From the sender: `route.final`, upload limit, `timings_ms.quic_write_blocked`, `connection.at_end.paths[selected]` (`rtt_ms`, `cwnd_bytes`, `pacing_rate_bytes_per_sec`, `lost_packets`).

### Test B: direct versus relay

1. Run with the default route mode (direct first, relay fallback). Read `route.initial`, `route.final`, `route.change_log`, `route.hole_punch` and `bytes_by_route`.
2. Settings → Connection → Direct only, run again. If the job shows *Waiting for a direct connection*, hole punching did not find a path between these networks; record that. It does not prove the relay is at fault.
3. Compare throughput only between runs whose `bytes_by_route` shows the route you think it shows. n0's public relays are rate-limited; a relay route on them is expected to be slow.

### Test C: rate limits

1. Confirm both `configuration.download_limit` (receiver) and `configuration.upload_limit` (sender) say `"unlimited": true`. The Settings page now shows limits with three decimals; before this change a limit under 0.5 MB/s was displayed as "0".
2. Optional validation: set the sender's upload limit to 2 MB/s, run, and check that `rates.active_average_mib_per_sec` is about 1.9 and `timings_ms.rate_limiter_wait.total_ms` on the sender is large. Set it back to empty (unlimited) afterwards.
3. Keep upload limits above about `parallel_streams × 1 MiB / 90 s` (≈ 140 KiB/s at 12 streams); below that, see confirmed issue 1 in the report.

### Test D: stream count

Same file and direction with Settings → Parallel streams at 1, 4, 8, 12, 16 and 24. Leave the QUIC windows alone. For each run record `active_average_mib_per_sec`, `average_active_streams_in_active_window`, `slow_stream_opens` and, on the sender, `connection_requests_in_flight_peak` and `STREAMS_BLOCKED` frames. A file needs at least as many chunks as streams for all of them to run (`workers_first_pass`).

### Test E: congestion control

Settings → Connection → congestion control Cubic (the default), then BBR. The setting applies after restarting the app **on the sender**, which is the side whose controller paces file data (restart both to be safe). Confirm the change in each summary's `configuration.transport.congestion_control_active` or the `engine_config` log line, not just in Settings. Compare sender `pacing_rate_bytes_per_sec`, `cwnd_bytes` and loss.

On loopback, the same comparison runs with the unchanged e2e suite:

```powershell
$env:RELAYLESS_TEST_CONGESTION="cubic"; cargo test -p relayless-core --release --test e2e; Remove-Item Env:RELAYLESS_TEST_CONGESTION
```

`RELAYLESS_TEST_CONGESTION` only applies to loopback test endpoints (`local_only`).

### Test F: disk and hashing alone

On each machine, against the source folder's disk (sender) and the download folder's disk (receiver):

```powershell
$env:BENCH_DIR="D:\Downloads"; $env:BENCH_SIZE_MIB="2048"
cargo test -p relayless-core --release --test diag -- --ignored --nocapture bench_disk_hash
```

It prints positioned-write, fsync, read (likely served from cache; use a file larger than RAM for a cold number) and BLAKE3 MiB/s with the engine's 1 MiB blocks. Compare with `stage_mib_per_sec` of `disk_write`, `disk_read`, `hash_verify` and `final_verification` in the summaries.

### Test G: repeatability and direction

Repeat Test A in the opposite direction (swap sender and receiver), three runs each. Note whether slowness follows the sending machine, the receiving machine, the route, or a setting.

### Loopback benchmark

Measures the engine without a network, with configurable settings:

```powershell
$env:BENCH_SIZE_MIB="512"; $env:BENCH_STREAMS="12"; $env:BENCH_CHUNK_MIB="16"; $env:BENCH_RUNS="3"
$env:BENCH_DOWN_KIBS="0"; $env:BENCH_UP_KIBS="0"     # optional limits in KiB/s
$env:BENCH_JSON="1"                                   # also print each full summary
cargo test -p relayless-core --release --test diag -- --ignored --nocapture bench_loopback
```

### Reproductions

| Command | Shows |
|---|---|
| `cargo test -p relayless-core --release --test diag -- --ignored --nocapture repro_low_upload_limit_stalls` | A 100 KiB/s sender upload limit with 12 streams triggers the 90 s stall timeout and a reconnect (~2 min) |
| `cargo test -p relayless-core --release --test relay -- --ignored --nocapture relay_throughput` | A transfer through a local relay with a per-client rate limit and no direct path, like n0's public relays. `RELAY_KIBS`, `RELAY_BURST_KIB`, `RELAY_SIZE_MIB`, `RELAY_RUNS`, `RELAYLESS_TEST_CONGESTION`, and `RELAYLESS_ADAPTIVE_WINDOW=0` to compare against the old static window. Prints average rate, interval CV, sender RTT p50/p95, loss and the per-interval series |
| `cargo test -p relayless-core --release --test diag -- --ignored --nocapture repro_throughput_after_limited_phase` | Pause and resume after a rate-limited phase, several pairs at once (`REPRO_PAIRS`, `REPRO_ROUNDS`, `REPRO_CONGESTION`) |

### Optional network checks

`iperf3` between the two machines (if one can accept a connection) shows general capacity, but it uses TCP and does not measure the iroh QUIC path. `ping` gives RTT and loss when ICMP is allowed; blocked ping proves nothing about the connection.

## Reading a slow transfer

Work through these in order and write down what each summary shows:

1. **Route.** `route.final` and `bytes_by_route` on both sides. If most bytes were `relay`, the relay's capacity is the first suspect (n0's public relays are rate-limited).
2. **Limits.** Both sides' `configuration.*_limit.unlimited` and `timings_ms.rate_limiter_wait`. A sender upload limit is only visible in the sender's summary.
3. **Direction and machine.** Does the slow case follow one machine or one direction (Test G)?
4. **Where the time goes.** Receiver: `quic_read_wait` against `disk_write`, `hash_verify`, `blocking_pool_queue`. Sender: `quic_write_blocked` against `disk_read`, `hash`. Large `quic_read_wait` with small sender `quic_write_blocked` means the sender is slow to produce data (disk, limiter); large `quic_write_blocked` on the sender means the transport is not draining.
5. **Concurrency.** `average_active_streams_in_active_window` against `workers_first_pass`; `slow_stream_opens`; sender `connection_requests_in_flight_peak` against 64.
6. **Transport.** Sender path report and interval lines: `rtt_ms`, `cwnd_bytes`, `pacing_rate_bytes_per_sec`, `lost_packets`, `congestion_events`. A pacing rate in the tens of KB/s with zero loss points at the congestion controller rather than the network (the sender summary adds a note when it sees this).
7. **Settings.** Repeat with different stream counts (Test D) and congestion control (Test E).
8. **Instability.** `route.change_log`, `errors`, `passes`, `connection.attempts`, and samples where `bytes` drops to 0.

Treat every finding as a correlation until a controlled change (one setting, same machines, repeated) moves the result.

## Limitations

- noq-proto 1.4 never sends `STREAM_DATA_BLOCKED` or `DATA_BLOCKED` frames, so per-stream and connection flow-control stalls cannot be observed. Those fields are `null`.
- BBR's internal phase (Startup, Drain, ProbeBW, ProbeRTT) and app-limited flag are private. Only cwnd, pacing rate, send quantum and bandwidth estimate are exposed.
- The applied transport configuration cannot be read back; summaries report the values passed to the QUIC builder.
- `quic_read_wait` includes local task scheduling, and `quic_write_blocked` mixes flow control, congestion control and buffer backpressure.
- Route bytes are attributed by the selected path when each 1 MiB block completes, from the route value updated on iroh path events. The receiver's interval samples cross-check it against the path API (`route_cache_matches`).
- Path statistics are per connection, shared by all transfers with that peer.
- The receiver cannot see the sender's limits, disk or CPU; it needs the sender's summary.
- Destination disk type is not probed.
