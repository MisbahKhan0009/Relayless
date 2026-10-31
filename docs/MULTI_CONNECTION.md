# Multi-connection transfers (MMCFTP-style)

Relayless can download from one sender over several QUIC connections at once and adjusts how many it uses, following the idea of MMCFTP (Massively Multi-Connection File Transfer Protocol, NII Japan): split the data, send it over many connections, balance the load, and change the number of connections to the network conditions.

Code: `crates/core/src/mmc.rs` (pool and controller), wired into `crates/core/src/receiver.rs`. Tests: unit tests in `mmc.rs`, `crates/core/tests/multiconn.rs`.

## Why it helps

Each QUIC connection has its own congestion controller. On a long path with random loss, one loss-based controller is limited to about `1.22 × MSS / (RTT × √loss)`, whatever the link capacity (MSS ≈ 1200 bytes for QUIC):

| Path | One connection | 8 connections |
|---|---|---|
| 200 ms, 1 % loss | ~73 KB/s | ~590 KB/s |
| 100 ms, 1 % loss | ~146 KB/s | ~1.2 MB/s |

N connections each get that share, and a loss slows only 1/N of the transfer. Parallel streams inside one connection (the 12 chunk workers) don't help here, because they share one congestion controller.

It does **not** help on a relay (relays limit per device, so extra connections only add queueing) or on a clean link that one connection already fills. In both cases Relayless stays at one connection.

## How it works

Receiver side only. Senders need no change; older senders work, because every request is authorized by device key, not per connection.

- **Pool ("lanes").** Per sender, lane 0 is the normal connection (Hello, Stat, receipts, chunks). Extra lanes are plain QUIC connections to the same sender and carry chunk streams only. Up to `max_connections` lanes (setting, default 8, 1 = off).
- **Balancing.** Each new chunk stream goes to the least-loaded lane. Every extra lane also brings 4 extra chunk workers (`WORKERS_PER_EXTRA_LANE`), so it carries traffic at once instead of waiting for running chunks to finish; they leave again when the lane is removed. Each lane has its own adaptive flow window (`flow.rs`), fed by that lane's own byte counter.
- **Rebalancing.** After every verified 1 MiB block, a stream moves if its lane was removed or moved to a relay, or if its lane has at least 2 more streams than the least-loaded one. It stops and the same chunk resumes on the other lane from that block (`ChunkError::Moved`). No data is fetched twice.
- **Limit.** Streams can't exceed the chunks left, so a small file (few chunks) gains little.
- **Failures.** If an extra lane fails, it is dropped and its chunk is retried on the remaining lanes; the job keeps running. A primary failure behaves as before (reconnect with backoff). Header and stall timeouts fire only when nothing arrived from the sender on any lane for the period (`receiver::wait_live`), so a stream queued behind busy siblings, or a lane carrying a single slow stream, isn't mistaken for a dead connection. Dead connections are caught by QUIC's idle timeout (40 s).
- **Eligibility.** Extra lanes are used only when the primary path is not a relay and no download limit is set. An extra lane whose own path is a relay gets no new work.
- **Cleanup.** Removed lanes finish their current streams, then close. Extra lanes close after 30 s without downloads and when the primary connection closes.

## Controller

Hill climbing on goodput measured at the receiver (MMCFTP adjusts on RTT and loss; the receiver can't see data-packet loss directly, and goodput is what both stand in for). Every 500 ms, while at least two chunk streams are open:

1. Wait `settle` (4 s) after any change, then measure goodput for `measure` (8 s). A window in which nothing arrived is measured again and changes nothing.
2. **Probe up:** add a lane. Keep it if goodput rose by at least half a fair share: `0.5 / n` when going from n to n+1 lanes (50 % for the 2nd, 25 % for the 3rd, … 7.1 % for the 8th, minimum 7 %). Then try the next one. A borderline result (at least half the required gain) is measured once more first, because a new connection can still be ramping up.
3. Otherwise remove it and **hold** 60 s. Repeated useless probes back off: 60, 120, 240, 480 s.
4. After a hold, measure again. If the last probe up failed at this level, **probe down:** remove a lane, keep going down while goodput stays within 5 % of the best level above, restore the lane otherwise.
5. Relay path, download limit, or setting = 1: collapse to one lane.

Goodput counts only bytes on lanes that take new work (`Pool::active_rx`). A removed lane keeps draining its streams, and on a slow lane a 1 MiB block can take a minute; counting it would make "one lane fewer" look as fast as before.

`RELAYLESS_MMC_FAST=1` shortens the timings (1.5 s / 3 s / 10 s) for tests.

## Setting and diagnostics

- **Settings → Connections per sender:** `1 (off)`, `Up to 4`, `Up to 8` (default). Stored as `maxConnections`.
- Receiver summary, `transfer.multi_connection`: `max_lanes`, `lanes`, `lanes_peak`, `adds`, `add_failures`, `removes`, `lane_moves`, `lane_failures`, `goodput_by_lanes` (bytes/s per lane count), `last_decision`.
- Interval samples and `transfer_interval` log lines include `lanes`; each decision is logged as `mmc_decision ...`.

## Differences from MMCFTP

| | MMCFTP | Relayless |
|---|---|---|
| Transport | Many TCP connections | Up to 8 QUIC connections (encrypted, NAT-traversing) |
| Scale | Hundreds of connections on 10–100 Gbit/s research links | At most 8, to stay fair to other traffic on home and office links |
| Control signal | RTT and loss | Measured goodput, with relay and rate-limit guards |
| Who decides | Both ends | Receiver only; no wire change |

## Measured results

Emulated path (`tests/multiconn.rs`, `compare_lanes`): a custom iroh transport with fixed delay, random loss in both directions, a 100 Mbit/s rate cap and a 100 ms drop-tail queue. 512 MiB file, 8 MiB chunks, production controller timings, 240 s per run (or until the file is done). Cubic.

| Path | 1 connection | Up to 8 | Steady state with MMC | Lanes used |
|---|---|---|---|---|
| 8 ms, 1 % loss | 427 KB/s | 2,555 KB/s (6.0×) | ~3,500 KB/s (8.2×) | 8 |
| 100 ms, 1 % loss | 101 KB/s | 485–549 KB/s (4.8–5.4×, two runs) | ~500–800 KB/s | 5–7 |
| 200 ms, 1 % loss | 78 KB/s | 373 KB/s (4.8×) | ~400–450 KB/s | 5–6 |
| 20 ms, no loss | 7,662 KB/s | 7,665 KB/s (same) | n/a | 1 (probed 2, dropped) |

"Up to 8" is the average over the whole run, including the 1–2.5 minutes the controller needs to climb. On the clean path the link is the limit; the controller tried a second connection, saw less than the required 50 % gain, and went back to one.

The first benchmark run exposed an older bug: on the lossy path the connection dropped about once a minute ("The sender stopped responding") because a new chunk's reply header could wait more than 60 s behind busy sibling streams. That is fixed for all transfers (see Failures above); the numbers above are after the fix. One connection on the 100 ms path went from 78 to 101–107 KB/s because of it.

Run it yourself:

```
cargo test -p relayless-core --release --test multiconn -- --ignored --nocapture compare_lanes
```

Environment: `LOSSY_RTT_MS` (100), `LOSSY_LOSS` (0.01), `LOSSY_MBIT` (100), `LOSSY_SIZE_MIB` (512), `LOSSY_SECS` (240), `LOSSY_LANES` ("1,8"). The non-ignored tests in the same file check that lanes are added on a lossy path (fast timings) and that the 1-connection setting is respected.

## Limits

- **Relays:** no gain, so it stays off (relays limit per device).
- **Small files and the tail of a transfer:** streams can't exceed the chunks left, so the last chunks run on fewer streams.
- **Slow to react:** each step takes 12–20 s, so reaching 8 lanes takes about 1.5–2.5 minutes. Short transfers finish before it helps.
- **Noise:** goodput on lossy paths varies ±20 % over 8 s, so the controller sometimes stops one lane early or late.
- **Fairness:** 8 connections take up to 8 times the share of a single TCP flow on a shared bottleneck. That's the point on a lossy long path, but it's why the maximum is 8 and why it backs off when extra connections don't help.
- **Sender diagnostics** show path statistics for one connection only.
- **Starvation inside a connection:** when the receive window is the limit, noq wakes blocked writers in LIFO order, so some streams wait minutes while others run. Total speed is unaffected and the timeouts now tolerate it; fixing the order would need a sender-side write scheduler.
