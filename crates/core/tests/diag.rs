//! Diagnostics: end-to-end checks of the per-transfer summaries, plus
//! opt-in benchmarks (`--ignored`) for repeatable measurements.
//!
//!   cargo test -p relayless-core --release --test diag
//!   cargo test -p relayless-core --release --test diag -- --ignored --nocapture bench_loopback
//!   cargo test -p relayless-core --release --test diag -- --ignored --nocapture bench_disk_hash

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use relayless_core::diag::read_summaries;
use relayless_core::*;
use serde_json::Value;

fn opts(dir: &Path, name: &str, diagnostics: bool) -> CoreOptions {
    CoreOptions {
        data_dir: dir.join("data"),
        default_device_name: name.into(),
        default_download_dir: dir.join("downloads"),
        local_only: true,
        diagnostics,
        endpoint_hook: None,
    }
}

fn make_file(path: &Path, size: usize, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(path).unwrap();
    let mut block = vec![0u8; 1 << 20];
    let mut x: u32 = (seed as u32).wrapping_mul(2654435761).wrapping_add(1);
    let mut left = size;
    while left > 0 {
        for b in block.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        let n = left.min(block.len());
        f.write_all(&block[..n]).unwrap();
        left -= n;
    }
}

fn hash(path: &Path) -> String {
    blake3::hash(&std::fs::read(path).unwrap()).to_hex().to_string()
}

fn auto_approve(sender: &Core) {
    let s = sender.clone();
    let mut rx = s.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let CoreEvent::AccessRequested(r) = ev {
                s.respond_access(&r.id, true).unwrap();
            }
        }
    });
}

async fn pair(sender: &Core, receiver: &Core, share: &ShareDto) -> String {
    receiver.add_peer_addr(sender.endpoint_addr());
    let inv = sender.create_invitation(&share.id, 3600, Some(1), true, false).await.unwrap();
    receiver.join(&inv.invite).await.unwrap().peer_id
}

async fn wait_status(core: &Core, id: &str, want: JobStatus, secs: u64) -> JobSnapshot {
    let start = Instant::now();
    loop {
        let j = core.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        if j.status == want {
            return j;
        }
        if start.elapsed() > Duration::from_secs(secs) {
            panic!("timed out waiting for {want:?}; job is {:?} ({:?})", j.status, j.error);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Waits until `pred` holds for some summary in `dir`.
async fn wait_summary(dir: &Path, secs: u64, pred: impl Fn(&Value) -> bool) -> Value {
    let start = Instant::now();
    loop {
        if let Some(s) = read_summaries(dir).into_iter().find(|s| pred(s)) {
            return s;
        }
        assert!(start.elapsed() < Duration::from_secs(secs), "no matching summary in {}", dir.display());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

struct Pair {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    sender: Core,
    receiver: Core,
    share: ShareDto,
    peer: String,
}

async fn setup(files: &[(&str, usize)], diagnostics: bool) -> Pair {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let shared = root.join("shared");
    for (i, (name, size)) in files.iter().enumerate() {
        make_file(&shared.join(name), *size, i as u8 + 1);
    }
    let sender = Core::start(opts(&root.join("s"), "Sender", diagnostics)).await.unwrap();
    let receiver = Core::start(opts(&root.join("r"), "Receiver", diagnostics)).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(shared.to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    Pair { _tmp: tmp, root, sender, receiver, share, peer }
}

async fn download(p: &Pair, name: &str) -> (String, JobSnapshot) {
    let id = p
        .receiver
        .start_download(&p.peer, &p.share.id, vec![DownloadItem { path: vec![name.into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    let done = wait_status(&p.receiver, &id, JobStatus::Completed, 120).await;
    assert_eq!(hash(Path::new(&done.dest_path)), hash(&p.root.join("shared").join(name)));
    (id, done)
}

fn num(v: &Value, ptr: &str) -> f64 {
    v.pointer(ptr).and_then(|x| x.as_f64()).unwrap_or_else(|| panic!("missing number at {ptr}: {}", v.pointer(ptr).unwrap_or(&Value::Null)))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn summaries_describe_a_complete_transfer() {
    let size = 40 * 1024 * 1024 + 4321;
    let p = setup(&[("video.mkv", size)], true).await;
    let mut s = p.receiver.settings();
    s.chunk_size_mib = 8;
    s.parallel_streams = 3;
    p.receiver.update_settings(s).unwrap();
    let (id, _) = download(&p, "video.mkv").await;

    let (on, rdir) = p.receiver.diagnostics();
    assert!(on);
    let r = wait_summary(&rdir, 10, |s| s["transfer_id"] == id.as_str()).await;
    assert_eq!(r["schema_version"], 1);
    assert_eq!(r["role"], "receiver");
    assert_eq!(r["transfer"]["outcome"], "completed");
    assert_eq!(r["transfer"]["file_size_bytes"], size as u64);
    assert_eq!(r["transfer"]["bytes_transferred"], size as u64, "every byte was received in this run");
    assert_eq!(r["transfer"]["chunks_completed"], 6);
    assert_eq!(r["transfer"]["chunks_failed"], 0);
    assert_eq!(r["transfer"]["blocks"], 41);
    assert_eq!(r["transfer"]["hash_failures"], 0);
    // Loopback is a direct path, measured from the connection's path API.
    assert_eq!(r["route"]["initial"], "direct");
    assert_eq!(r["route"]["final"], "direct");
    assert_eq!(r["route"]["bytes_by_route"]["direct"], size as u64);
    assert_eq!(r["route"]["bytes_by_route"]["relay"], 0);
    assert_eq!(r["route"]["hole_punch"]["outcome"], "already_direct");
    assert_eq!(r["connection"]["at_end"]["selected_kind"], "direct");
    assert_eq!(r["connection"]["at_end"]["paths"][0]["ip_scope"], "loopback");
    assert!(r["connection"]["attempts"].as_array().unwrap().iter().any(|a| a["ok"] == true));
    // Configuration as applied.
    assert_eq!(r["configuration"]["chunk_size_bytes"], 8 * 1024 * 1024);
    assert_eq!(r["configuration"]["block_size_bytes"], 1024 * 1024);
    assert_eq!(r["configuration"]["parallel_streams_setting"], 3);
    assert_eq!(r["configuration"]["workers_first_pass"], 3);
    assert_eq!(r["configuration"]["transport"]["stream_window_bytes"], 1_000_000);
    let expected_cc = if std::env::var("RELAYLESS_TEST_CONGESTION").as_deref() == Ok("bbr") { "bbr3" } else { "cubic" };
    assert_eq!(r["configuration"]["transport"]["congestion_control_active"], expected_cc);
    assert_eq!(r["configuration"]["transport"]["adaptive_flow_window"], true);
    assert_eq!(r["transfer"]["flow_window"]["enabled"], true);
    assert_eq!(r["configuration"]["download_limit"]["unlimited"], true);
    // Concurrency: never more than the configured streams.
    let peak = num(&r, "/transfer/active_streams/peak");
    assert!((1.0..=3.0).contains(&peak), "peak streams {peak}");
    let avg = num(&r, "/transfer/average_active_streams_in_active_window");
    assert!(avg > 0.0 && avg <= peak + 0.01, "average streams while downloading {avg}");
    assert_eq!(num(&r, "/transfer/connection_streams_peak"), peak, "one job: connection peak equals job peak");
    assert_eq!(num(&r, "/transfer/other_jobs_running_at_end"), 0.0);
    assert!(r["transfer"]["error_category"].is_null());
    assert_eq!(num(&r, "/transfer/passes"), 1.0);
    // Stage timings were measured for real work.
    assert!(num(&r, "/timings_ms/hash_verify/count") >= 41.0);
    assert!(num(&r, "/timings_ms/disk_write/count") >= 41.0);
    assert_eq!(num(&r, "/timings_ms/disk_write/bytes"), size as f64);
    assert!(num(&r, "/timings_ms/quic_read_wait/count") > 0.0);
    assert!(num(&r, "/timings_ms/response_header_wait/count") >= 6.0);
    assert!(num(&r, "/timings_ms/sync_flush/count") >= 1.0);
    assert_eq!(num(&r, "/timings_ms/final_verification/count"), 1.0);
    assert_eq!(num(&r, "/timings_ms/final_verification_read/bytes"), size as f64);
    assert_eq!(num(&r, "/timings_ms/rate_limiter_wait/count"), 0.0, "unlimited never waits");
    assert!(num(&r, "/transfer/rates/whole_run_average_bytes_per_sec") > 0.0);
    assert!(num(&r, "/transfer/elapsed_ms") > 0.0);
    // Per-chunk records.
    let chunks = r["chunks"].as_array().unwrap();
    assert_eq!(chunks.len(), 6);
    assert!(chunks.iter().all(|c| c["outcome"] == "completed" && c["header_wait_ms"].is_number()));
    assert_eq!(chunks.iter().map(|c| c["bytes"].as_u64().unwrap()).sum::<u64>(), size as u64);
    // Unavailable metrics are labelled, not zero.
    assert!(r["connection"]["at_end"]["totals"]["stream_data_blocked_frames"].is_null());
    assert!(r["unavailable"]["stream_flow_control_blocked"].is_string());
    // No file names anywhere in the summary.
    assert!(!serde_json::to_string(&r).unwrap().contains("video.mkv"));

    // The sender writes its own summary when the receipt arrives, with the same file_key.
    let (_, sdir) = p.sender.diagnostics();
    let s = wait_summary(&sdir, 10, |s| s["role"] == "sender" && s["file_key"] == r["file_key"]).await;
    assert_eq!(s["transfer"]["outcome"], "receipt_received");
    assert_eq!(s["transfer"]["bytes_transferred"], size as u64);
    assert_eq!(s["transfer"]["chunks_completed"], 6);
    assert_eq!(s["route"]["final"], "direct");
    assert!(num(&s, "/timings_ms/disk_read/count") >= 41.0);
    assert_eq!(num(&s, "/timings_ms/disk_read/bytes"), size as f64);
    assert!(num(&s, "/timings_ms/hash/count") >= 41.0);
    assert!(num(&s, "/timings_ms/quic_write_blocked/count") >= 41.0);
    assert!(num(&s, "/transfer/connection_requests_in_flight_peak") <= 64.0);
    assert_eq!(s["configuration"]["upload_limit"]["unlimited"], true);

    p.receiver.shutdown().await;
    p.sender.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn limiter_waits_are_measured_on_both_sides() {
    let size = 6 * 1024 * 1024;
    let p = setup(&[("a.bin", size)], true).await;
    let mut s = p.receiver.settings();
    s.chunk_size_mib = 8;
    s.download_limit_kbps = 4 * 1024; // 4 MiB/s
    p.receiver.update_settings(s).unwrap();
    let mut s = p.sender.settings();
    s.upload_limit_kbps = 3 * 1024; // 3 MiB/s: the lower limit wins
    p.sender.update_settings(s).unwrap();
    let t = Instant::now();
    let (id, _) = download(&p, "a.bin").await;
    let secs = t.elapsed().as_secs_f64();
    // 6 MiB at 3 MiB/s from an empty bucket: about 2 s.
    assert!(secs >= 1.5, "limits were not applied: {secs:.2}s");

    let (_, rdir) = p.receiver.diagnostics();
    let r = wait_summary(&rdir, 10, |s| s["transfer_id"] == id.as_str()).await;
    assert_eq!(r["configuration"]["download_limit"]["configured_value"], 4096);
    assert_eq!(r["configuration"]["download_limit"]["effective_bytes_per_sec"], 4 * 1024 * 1024);
    assert_eq!(r["configuration"]["download_limit"]["unlimited"], false);
    assert!(num(&r, "/transfer/download_limiter_process/charged_bytes") >= size as f64);

    let (_, sdir) = p.sender.diagnostics();
    let s = wait_summary(&sdir, 10, |s| s["role"] == "sender" && s["file_key"] == r["file_key"]).await;
    assert_eq!(s["configuration"]["upload_limit"]["effective_bytes_per_sec"], 3 * 1024 * 1024);
    let sender_wait = num(&s, "/timings_ms/rate_limiter_wait/total_ms");
    assert!(sender_wait >= 1000.0, "sender limiter wait {sender_wait} ms");
    assert!(num(&s, "/timings_ms/rate_limiter_wait/count") >= 1.0);
    assert!(s["diagnostic_notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("upload limit is active")));

    p.receiver.shutdown().await;
    p.sender.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_diagnostics_write_nothing_and_transfers_still_work() {
    let p = setup(&[("b.bin", 3 * 1024 * 1024)], false).await;
    let (id, _) = download(&p, "b.bin").await;
    assert!(!id.is_empty());
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (on, rdir) = p.receiver.diagnostics();
    assert!(!on);
    assert!(read_summaries(&rdir).is_empty());
    p.receiver.shutdown().await;
    p.sender.shutdown().await;
    let (_, sdir) = p.sender.diagnostics();
    assert!(read_summaries(&sdir).is_empty(), "sender flushes on shutdown, but only when enabled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paused_transfer_gets_a_paused_summary_and_resume_gets_its_own() {
    let size = 48 * 1024 * 1024;
    let p = setup(&[("c.bin", size)], true).await;
    let mut s = p.receiver.settings();
    s.chunk_size_mib = 8;
    s.download_limit_kbps = 8 * 1024;
    p.receiver.update_settings(s).unwrap();
    let id = p
        .receiver
        .start_download(&p.peer, &p.share.id, vec![DownloadItem { path: vec!["c.bin".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    tokio::time::sleep(Duration::from_secs(2)).await;
    p.receiver.pause_job(&id).await.unwrap();
    let (_, rdir) = p.receiver.diagnostics();
    let first = wait_summary(&rdir, 10, |s| s["transfer_id"] == id.as_str()).await;
    assert_eq!(first["transfer"]["outcome"], "paused");
    let got = num(&first, "/transfer/bytes_transferred") as u64;
    assert!(got > 0 && got < size as u64, "partial bytes {got}");
    // A pause stops chunks; that is not an error and must not hide real ones.
    assert!(first["errors"].get("cancelled").is_none(), "errors: {}", first["errors"]);
    assert!(first["transfer"]["error_category"].is_null(), "{}", first["transfer"]["error_category"]);
    assert!(num(&first, "/transfer/chunks_stopped") >= 1.0);

    let mut s = p.receiver.settings();
    s.download_limit_kbps = 0;
    p.receiver.update_settings(s).unwrap();
    p.receiver.resume_job(&id).await.unwrap();
    wait_status(&p.receiver, &id, JobStatus::Completed, 60).await;
    let second = wait_summary(&rdir, 10, |s| s["transfer_id"] == id.as_str() && s["transfer"]["outcome"] == "completed").await;
    let resumed_from = num(&second, "/transfer/verified_bytes_at_start") as u64;
    assert!(resumed_from > 0, "the second run started from saved progress");
    // Bytes received in the second run plus what was saved before cover the file.
    assert!(num(&second, "/transfer/bytes_transferred") as u64 + resumed_from >= size as u64);
    assert!(second["transfer"]["error_category"].is_null());
    // The sender saw the pause as streams stopped by the receiver, not failures.
    let (_, sdir) = p.sender.diagnostics();
    let s = wait_summary(&sdir, 10, |s| s["role"] == "sender" && s["file_key"] == second["file_key"]).await;
    assert_eq!(s["transfer"]["outcome"], "receipt_received");
    assert_eq!(s["transfer"]["chunks_failed"], 0, "sender summary: {}", s["transfer"]);
    assert!(s["transfer"]["error_category"].is_null());
    let started = num(&s, "/transfer/chunks_started");
    let closed = num(&s, "/transfer/chunks_completed") + num(&s, "/transfer/chunks_failed") + num(&s, "/transfer/chunks_stopped");
    assert_eq!(started, closed, "every started sender chunk is closed");
    p.receiver.shutdown().await;
    p.sender.shutdown().await;
}

/// A paused receiver leaves the sender idle; after 15 s the sender closes its
/// diagnostics with outcome "idle".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sender_summary_closes_after_idle() {
    let p = setup(&[("d.bin", 64 * 1024 * 1024)], true).await;
    let mut s = p.receiver.settings();
    s.download_limit_kbps = 8 * 1024;
    s.chunk_size_mib = 8;
    p.receiver.update_settings(s).unwrap();
    let id = p
        .receiver
        .start_download(&p.peer, &p.share.id, vec![DownloadItem { path: vec!["d.bin".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    tokio::time::sleep(Duration::from_secs(2)).await;
    p.receiver.pause_job(&id).await.unwrap();
    let (_, sdir) = p.sender.diagnostics();
    let s = wait_summary(&sdir, 30, |s| s["role"] == "sender").await;
    assert_eq!(s["transfer"]["outcome"], "idle");
    assert!(num(&s, "/transfer/bytes_transferred") > 0.0);
    assert!(s["samples"].as_array().unwrap().len() >= 1, "sender interval samples are kept");
    p.receiver.shutdown().await;
    p.sender.shutdown().await;
}

/// Reproduces a slow-transfer mechanism: with a low *sender* upload limit,
/// each of the receiver's streams waits about streams x 1 MiB / limit for its
/// next block. Above 90 s the receiver's stall timeout fires, the pass is
/// cancelled and the connection dropped, then the job reconnects with backoff.
/// Here: 12 streams at 100 KiB/s = ~123 s per block per stream.
/// Takes ~2 minutes:
///   cargo test -p relayless-core --release --test diag -- --ignored --nocapture repro_low_upload_limit_stalls
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn repro_low_upload_limit_stalls() {
    let p = setup(&[("slow.bin", 96 * 1024 * 1024)], true).await;
    let mut s = p.receiver.settings();
    s.chunk_size_mib = 8;
    s.parallel_streams = 12;
    p.receiver.update_settings(s).unwrap();
    let mut s = p.sender.settings();
    s.upload_limit_kbps = 100;
    p.sender.update_settings(s).unwrap();
    let id = p
        .receiver
        .start_download(&p.peer, &p.share.id, vec![DownloadItem { path: vec!["slow.bin".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    let start = Instant::now();
    let interrupted = loop {
        let j = p.receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        if j.status == JobStatus::Reconnecting {
            break j;
        }
        assert!(start.elapsed() < Duration::from_secs(150), "no interruption: {:?} {:?}", j.status, j.error);
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    println!("interrupted after {:.1}s: {:?}", start.elapsed().as_secs_f64(), interrupted.error);
    p.receiver.pause_job(&id).await.unwrap();
    let (_, rdir) = p.receiver.diagnostics();
    let r = wait_summary(&rdir, 10, |s| s["transfer_id"] == id.as_str()).await;
    println!("receiver errors: {}", r["errors"]);
    println!("receiver bytes: {} in {} ms", r["transfer"]["bytes_transferred"], r["transfer"]["elapsed_ms"]);
    assert!(r["errors"]["stall_timeout"].as_u64().unwrap_or(0) >= 1, "expected a stall timeout: {}", r["errors"]);
    p.receiver.shutdown().await;
    p.sender.shutdown().await;
    let (_, sdir) = p.sender.diagnostics();
    let s = wait_summary(&sdir, 10, |s| s["role"] == "sender").await;
    println!(
        "sender limiter wait: {} ms over {} waits, max single wait {} ms",
        s["timings_ms"]["rate_limiter_wait"]["total_ms"], s["timings_ms"]["rate_limiter_wait"]["count"], s["timings_ms"]["rate_limiter_wait"]["max_ms"]
    );
    assert!(s["timings_ms"]["rate_limiter_wait"]["max_ms"].as_f64().unwrap() >= 60_000.0);
}

/// Starts a sender/receiver pair with a congestion controller applied at
/// startup (the setting only takes effect on restart).
async fn setup_with_congestion(files: &[(&str, usize)], congestion: &str) -> Pair {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let shared = root.join("shared");
    for (i, (name, size)) in files.iter().enumerate() {
        make_file(&shared.join(name), *size, i as u8 + 1);
    }
    for side in ["s", "r"] {
        let c = Core::start(opts(&root.join(side), side, true)).await.unwrap();
        let mut s = c.settings();
        s.congestion = congestion.into();
        c.update_settings(s).unwrap();
        c.shutdown().await;
    }
    let sender = Core::start(opts(&root.join("s"), "Sender", true)).await.unwrap();
    let receiver = Core::start(opts(&root.join("r"), "Receiver", true)).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(shared.to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    Pair { _tmp: tmp, root, sender, receiver, share, peer }
}

/// One pair: rate-limited download, pause, resume without a limit, then
/// measure the unlimited phase. Returns a result line and whether it finished.
async fn limited_then_unlimited(congestion: &str, label: String) -> (bool, f64, String) {
    let size = 120 * 1024 * 1024;
    let p = setup_with_congestion(&[("movie.mkv", size)], congestion).await;
    let mut s = p.receiver.settings();
    s.download_limit_kbps = 25 * 1024;
    s.chunk_size_mib = 8;
    p.receiver.update_settings(s).unwrap();
    let id = p
        .receiver
        .start_download(&p.peer, &p.share.id, vec![DownloadItem { path: vec!["movie.mkv".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    // Pause once 16 MiB are saved, like the e2e pause/resume test.
    let t = Instant::now();
    loop {
        let j = p.receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        if j.verified_bytes >= 16 * 1024 * 1024 || t.elapsed() > Duration::from_secs(20) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    p.receiver.pause_job(&id).await.unwrap();
    let mut s = p.receiver.settings();
    s.download_limit_kbps = 0;
    p.receiver.update_settings(s).unwrap();
    p.receiver.resume_job(&id).await.unwrap();
    let t = Instant::now();
    let completed = loop {
        let j = p.receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        if j.status == JobStatus::Completed {
            break true;
        }
        if t.elapsed() > Duration::from_secs(40) {
            break false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let secs = t.elapsed().as_secs_f64();
    if !completed {
        p.receiver.pause_job(&id).await.unwrap();
    }
    let (_, rdir) = p.receiver.diagnostics();
    // The second run of the job (started by resume) is the unlimited phase.
    let r = wait_summary(&rdir, 10, |s| {
        s["transfer_id"] == id.as_str() && s["transfer"]["verified_bytes_at_start"].as_u64().unwrap_or(0) > 0
    })
    .await;
    let bytes = r["transfer"]["bytes_transferred"].as_u64().unwrap_or(0);
    p.receiver.shutdown().await;
    p.sender.shutdown().await;
    let (_, sdir) = p.sender.diagnostics();
    let s = read_summaries(&sdir).into_iter().filter(|s| s["role"] == "sender").last().unwrap_or(Value::Null);
    let path = s.pointer("/connection/at_end/paths/0").cloned().unwrap_or(Value::Null);
    let mib_s = bytes as f64 / 1048576.0 / secs;
    let line = format!(
        "{label} congestion={congestion} completed={completed} resumed_phase_mib_s={mib_s:.2} bytes={bytes} secs={secs:.1} sender_cwnd={} sender_bw_est_bps={} sender_lost={} sender_congestion_events={} rtt_ms={}",
        path["cwnd_bytes"], path["bandwidth_estimate_bits_per_sec"], path["lost_packets"], path["congestion_events"], path["rtt_ms"],
    );
    (completed, mib_s, line)
}

/// Throughput after a rate-limited phase and a pause/resume, with several
/// pairs running at once (mirrors the e2e suite, where 4 tests run in
/// parallel and a resumed transfer sometimes crawls at ~40 KB/s while the
/// sender's cwnd sits at 4 packets with no loss).
/// Environment: REPRO_CONGESTION=bbr|cubic (bbr), REPRO_PAIRS (4), REPRO_ROUNDS (3).
///   cargo test -p relayless-core --release --test diag -- --ignored --nocapture repro_throughput_after_limited_phase
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn repro_throughput_after_limited_phase() {
    let congestion = std::env::var("REPRO_CONGESTION").unwrap_or_else(|_| "bbr".into());
    let pairs = env_u64("REPRO_PAIRS", 4);
    let rounds = env_u64("REPRO_ROUNDS", 3);
    // Background load: an unlimited transfer loop on its own pair, like the
    // e2e suite's large folder test running alongside.
    let load = env_u64("REPRO_LOAD", 1) > 0;
    let stop = tokio_util_stop();
    let loader = if load {
        let stop = stop.clone();
        Some(tokio::spawn(async move {
            let p = setup(&[("load.bin", 160 * 1024 * 1024)], false).await;
            let mut n = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let dest = p.root.join(format!("load{n}"));
                let id = p
                    .receiver
                    .start_download(&p.peer, &p.share.id, vec![DownloadItem { path: vec!["load.bin".into()], is_dir: false }], Some(dest.to_string_lossy().to_string()))
                    .await
                    .unwrap()[0]
                    .clone();
                let _ = tokio::time::timeout(Duration::from_secs(60), wait_status(&p.receiver, &id, JobStatus::Completed, 60)).await;
                let _ = p.receiver.remove_job(&id).await;
                let _ = std::fs::remove_dir_all(&dest);
                n += 1;
            }
            p.receiver.shutdown().await;
            p.sender.shutdown().await;
        }))
    } else {
        None
    };
    let mut all = Vec::new();
    for round in 0..rounds {
        let mut set = tokio::task::JoinSet::new();
        for pair in 0..pairs {
            let c = congestion.clone();
            set.spawn(async move { limited_then_unlimited(&c, format!("round={round} pair={pair}")).await });
        }
        while let Some(r) = set.join_next().await {
            let r = r.unwrap();
            println!("{}", r.2);
            all.push(r);
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(l) = loader {
        let _ = tokio::time::timeout(Duration::from_secs(70), l).await;
    }
    let slow = all.iter().filter(|(done, mib, _)| !done || *mib < 1.0).count();
    println!("summary: congestion={congestion} load={load} runs={} below_1_MiB_s_or_unfinished={slow}", all.len());
}

fn tokio_util_stop() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))
}

// ====================================================================== benchmarks

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Loopback transfer with configurable settings; prints each run's summary
/// line and a compact table. Environment:
///   BENCH_SIZE_MIB (256), BENCH_STREAMS (12), BENCH_CHUNK_MIB (16),
///   BENCH_DOWN_KIBS (0), BENCH_UP_KIBS (0), BENCH_RUNS (3), BENCH_DIR (temp)
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn bench_loopback() {
    let size = env_u64("BENCH_SIZE_MIB", 256) as usize * 1024 * 1024;
    let runs = env_u64("BENCH_RUNS", 3);
    let tmp = tempfile::tempdir_in(std::env::var("BENCH_DIR").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string())).unwrap();
    let root = tmp.path();
    make_file(&root.join("shared/bench.bin"), size, 42);
    let sender = Core::start(opts(&root.join("s"), "S", true)).await.unwrap();
    let receiver = Core::start(opts(&root.join("r"), "R", true)).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(root.join("shared").to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    let mut s = receiver.settings();
    s.parallel_streams = env_u64("BENCH_STREAMS", 12) as u32;
    s.chunk_size_mib = env_u64("BENCH_CHUNK_MIB", 16) as u32;
    s.download_limit_kbps = env_u64("BENCH_DOWN_KIBS", 0);
    receiver.update_settings(s.clone()).unwrap();
    let mut ss = sender.settings();
    ss.upload_limit_kbps = env_u64("BENCH_UP_KIBS", 0);
    sender.update_settings(ss).unwrap();
    println!(
        "bench: size={} MiB streams={} chunk={} MiB down_limit={} KiB/s up_limit={} KiB/s runs={runs}",
        size >> 20,
        s.parallel_streams,
        s.chunk_size_mib,
        s.download_limit_kbps,
        env_u64("BENCH_UP_KIBS", 0)
    );
    let mut rows = Vec::new();
    for run in 0..runs {
        let dest = root.join(format!("dl{run}"));
        let t = Instant::now();
        let id = receiver
            .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["bench.bin".into()], is_dir: false }], Some(dest.to_string_lossy().to_string()))
            .await
            .unwrap()[0]
            .clone();
        wait_status(&receiver, &id, JobStatus::Completed, 1800).await;
        let wall = t.elapsed();
        let (_, rdir) = receiver.diagnostics();
        let r = wait_summary(&rdir, 10, |s| s["transfer_id"] == id.as_str()).await;
        let g = |p: &str| r.pointer(p).cloned().unwrap_or(Value::Null);
        rows.push(format!(
            "run={run} wall_s={:.2} avg_mib_s={} active_mib_s={} streams_peak={} streams_avg={} read_wait_ms={} hash_ms={} disk_write_ms={} fsync_ms={} verify_ms={} limiter_ms={}",
            wall.as_secs_f64(),
            g("/transfer/rates/whole_run_average_mib_per_sec"),
            g("/transfer/rates/active_average_mib_per_sec"),
            g("/transfer/active_streams/peak"),
            g("/transfer/average_active_streams_in_active_window"),
            g("/timings_ms/quic_read_wait/total_ms"),
            g("/timings_ms/hash_verify/total_ms"),
            g("/timings_ms/disk_write/total_ms"),
            g("/timings_ms/sync_flush/total_ms"),
            g("/timings_ms/final_verification/total_ms"),
            g("/timings_ms/rate_limiter_wait/total_ms"),
        ));
        if std::env::var("BENCH_JSON").is_ok() {
            println!("{}", serde_json::to_string(&r).unwrap());
        }
        let _ = std::fs::remove_dir_all(&dest);
    }
    for r in &rows {
        println!("{r}");
    }
    receiver.shutdown().await;
    sender.shutdown().await;
}

/// Disk and hashing alone, with the engine's block size and positioned I/O.
/// BENCH_DIR selects the disk (default: temp); BENCH_SIZE_MIB (512).
#[test]
#[ignore]
fn bench_disk_hash() {
    use relayless_core::util::{read_exact_at, write_all_at};
    const BLOCK: usize = 1024 * 1024;
    let size = env_u64("BENCH_SIZE_MIB", 512) as usize * 1024 * 1024;
    let tmp = tempfile::tempdir_in(std::env::var("BENCH_DIR").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string())).unwrap();
    let path = tmp.path().join("bench.bin");
    let mut buf = vec![0u8; BLOCK];
    for (i, b) in buf.iter_mut().enumerate() {
        *b = (i * 31 % 251) as u8;
    }
    let mib = |d: Duration| size as f64 / 1048576.0 / d.as_secs_f64();

    // Write (like the receiver: positioned writes into a pre-sized file, then fsync).
    let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).unwrap();
    f.set_len(size as u64).unwrap();
    let t = Instant::now();
    for off in (0..size).step_by(BLOCK) {
        write_all_at(&f, &buf, off as u64).unwrap();
    }
    let write = t.elapsed();
    let t = Instant::now();
    f.sync_data().unwrap();
    let sync = t.elapsed();
    drop(f);

    // Read (like the sender and the final check). The OS cache may serve this;
    // use a file larger than RAM, or reboot, for a cold-cache number.
    let f = std::fs::File::open(&path).unwrap();
    let t = Instant::now();
    for off in (0..size).step_by(BLOCK) {
        read_exact_at(&f, &mut buf, off as u64).unwrap();
    }
    let read = t.elapsed();

    // BLAKE3 over the same amount of data in 1 MiB blocks.
    let t = Instant::now();
    let mut acc = 0u8;
    for _ in (0..size).step_by(BLOCK) {
        acc ^= blake3::hash(&buf).as_bytes()[0];
    }
    let hash = t.elapsed();
    println!("bench_disk_hash dir={} size_mib={} (acc {acc})", tmp.path().display(), size >> 20);
    println!("write_mib_s={:.0} (sync {:.0} ms) read_mib_s={:.0} (may be cached) blake3_mib_s={:.0}", mib(write), sync.as_secs_f64() * 1000.0, mib(read), mib(hash));
}
