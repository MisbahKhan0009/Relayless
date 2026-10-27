use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use relayless_core::*;

fn opts(dir: &Path, name: &str) -> CoreOptions {
    CoreOptions {
        data_dir: dir.join("data"),
        default_device_name: name.into(),
        default_download_dir: dir.join("downloads"),
        local_only: true,
    }
}

fn make_file(path: &Path, size: usize, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(path).unwrap();
    let mut block = vec![0u8; 1 << 20];
    let mut x: u32 = seed as u32 * 2654435761 + 1;
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

/// Approves every access request automatically (stand-in for the sender clicking Approve).
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
    let joined = receiver.join(&inv.invite).await.unwrap();
    assert_eq!(joined.shares.len(), 1);
    joined.peer_id
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

async fn wait_progress(core: &Core, id: &str, min_bytes: u64) -> JobSnapshot {
    let start = Instant::now();
    loop {
        let j = core.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        if j.verified_bytes >= min_bytes {
            return j;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "no progress: {:?} {:?}", j.status, j.error);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn set_limit(core: &Core, kbps: u64) {
    let mut s = core.settings();
    s.download_limit_kbps = kbps;
    s.chunk_size_mib = 8;
    core.update_settings(s).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_transfer_folder_and_security() {
    let _ = tracing_subscriber::fmt().with_env_filter(std::env::var("TLOG").unwrap_or("relayless_core=info".into())).try_init();
    let tmp = tempfile::tempdir().unwrap();
    let shared = tmp.path().join("shared");
    make_file(&shared.join("big.bin"), 150 * 1024 * 1024 + 12345, 1);
    make_file(&shared.join("nested/a.txt"), 1000, 2);
    make_file(&shared.join("nested/deeper/b.dat"), 9 * 1024 * 1024, 3);
    make_file(&shared.join("nested/empty.txt"), 0, 4);
    make_file(&tmp.path().join("secret.txt"), 100, 5);
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("secret.txt"), shared.join("escape.txt")).unwrap();

    let sender = Core::start(opts(&tmp.path().join("s"), "Sender PC")).await.unwrap();
    let receiver = Core::start(opts(&tmp.path().join("r"), "Receiver PC")).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(shared.to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    assert_eq!(sender.list_grants().unwrap().len(), 1);

    // Browsing: symlinks are hidden, traversal is rejected.
    let list = receiver.list_remote_dir(&peer, &share.id, vec![], 0, 100).await.unwrap();
    let names: Vec<_> = list.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, vec!["nested", "big.bin"]);
    assert!(receiver.list_remote_dir(&peer, &share.id, vec!["..".into()], 0, 100).await.is_err());
    assert!(receiver
        .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["escape.txt".into()], is_dir: false }], None)
        .await
        .is_err());

    // A single invitation (max_uses = 1) can't be reused by another device.
    let third = Core::start(opts(&tmp.path().join("t"), "Third")).await.unwrap();
    third.add_peer_addr(sender.endpoint_addr());
    let inv = sender.create_invitation(&share.id, 3600, Some(1), false, false).await.unwrap();
    third.join(&inv.invite).await.unwrap();
    let err = Core::start(opts(&tmp.path().join("u"), "Fourth")).await.unwrap();
    err.add_peer_addr(sender.endpoint_addr());
    assert!(err.join(&inv.invite).await.is_err());

    // Download a large file and a whole folder.
    let dest = tmp.path().join("dl");
    let started = Instant::now();
    let ids = receiver
        .start_download(
            &peer,
            &share.id,
            vec![
                DownloadItem { path: vec!["big.bin".into()], is_dir: false },
                DownloadItem { path: vec!["nested".into()], is_dir: true },
            ],
            Some(dest.to_string_lossy().to_string()),
        )
        .await
        .unwrap();
    assert_eq!(ids.len(), 4);
    for id in &ids {
        wait_status(&receiver, id, JobStatus::Completed, 120).await;
    }
    let secs = started.elapsed().as_secs_f64();
    println!("transferred ~160 MB in {secs:.2}s ({:.0} MB/s on loopback)", 160.0 / secs);
    assert_eq!(hash(&dest.join("big.bin")), hash(&shared.join("big.bin")));
    assert_eq!(hash(&dest.join("nested/a.txt")), hash(&shared.join("nested/a.txt")));
    assert_eq!(hash(&dest.join("nested/deeper/b.dat")), hash(&shared.join("nested/deeper/b.dat")));
    assert_eq!(std::fs::metadata(dest.join("nested/empty.txt")).unwrap().len(), 0);
    assert!(!dest.join("big.bin.rlpart").exists());

    // Revoking the device blocks further access.
    sender.revoke_device(&peer_id_of(&receiver)).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(receiver.list_remote_dir(&peer, &share.id, vec![], 0, 10).await.is_err());

    receiver.shutdown().await;
    sender.shutdown().await;
    third.shutdown().await;
}

fn peer_id_of(c: &Core) -> String {
    c.endpoint_id()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_resume_and_crash_recovery() {
    let _ = tracing_subscriber::fmt().with_env_filter(std::env::var("TLOG").unwrap_or("relayless_core=info".into())).try_init();
    let tmp = tempfile::tempdir().unwrap();
    let shared = tmp.path().join("shared");
    let size = 120 * 1024 * 1024 + 777;
    make_file(&shared.join("movie.mkv"), size, 7);

    let sender = Core::start(opts(&tmp.path().join("s"), "Sender")).await.unwrap();
    let rdir = tmp.path().join("r");
    let receiver = Core::start(opts(&rdir, "Receiver")).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(shared.to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    set_limit(&receiver, 25 * 1024); // 25 MB/s so we can interrupt it

    let ids = receiver
        .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["movie.mkv".into()], is_dir: false }], None)
        .await
        .unwrap();
    let id = ids[0].clone();

    // Manual pause and resume.
    wait_progress(&receiver, &id, 16 * 1024 * 1024).await;
    receiver.pause_job(&id).await.unwrap();
    let paused = wait_status(&receiver, &id, JobStatus::Paused, 10).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let still = receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
    assert_eq!(still.verified_bytes, paused.verified_bytes, "no progress while paused");
    receiver.resume_job(&id).await.unwrap();

    // Simulated crash: drop the receiver mid-transfer, start a new instance on the same data.
    wait_progress(&receiver, &id, paused.verified_bytes + 24 * 1024 * 1024).await;
    receiver.shutdown().await;
    drop(receiver);
    let before = {
        let r2 = Core::start(opts(&rdir, "Receiver")).await.unwrap();
        r2.add_peer_addr(sender.endpoint_addr());
        let j = r2.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        assert!(j.verified_bytes >= 24 * 1024 * 1024, "progress survived restart: {}", j.verified_bytes);
        let v = j.verified_bytes;
        let done = wait_status(&r2, &id, JobStatus::Completed, 120).await;
        assert_eq!(hash(Path::new(&done.dest_path)), hash(&shared.join("movie.mkv")));
        r2.shutdown().await;
        v
    };
    println!("resumed from {} MB after restart", before / 1024 / 1024);

    // Corrupted partial data is caught by the final verification and re-fetched.
    let rdir2 = tmp.path().join("r3");
    let r3 = Core::start(opts(&rdir2, "Receiver3")).await.unwrap();
    let peer3 = pair(&sender, &r3, &share).await;
    set_limit(&r3, 30 * 1024);
    let id3 = r3
        .start_download(&peer3, &share.id, vec![DownloadItem { path: vec!["movie.mkv".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    wait_progress(&r3, &id3, 100 * 1024 * 1024).await;
    r3.pause_job(&id3).await.unwrap();
    let j = r3.list_jobs().unwrap().into_iter().find(|j| j.id == id3).unwrap();
    let part = PathBuf::from(format!("{}.rlpart", j.dest_path));
    {
        use std::io::{Seek, SeekFrom};
        let mut f = std::fs::OpenOptions::new().write(true).open(&part).unwrap();
        f.seek(SeekFrom::Start(1024)).unwrap();
        f.write_all(b"CORRUPTION!").unwrap();
        f.sync_all().unwrap();
    }
    r3.resume_job(&id3).await.unwrap();
    let done = wait_status(&r3, &id3, JobStatus::Completed, 120).await;
    assert_eq!(hash(Path::new(&done.dest_path)), hash(&shared.join("movie.mkv")));
    let acts = r3.list_activity(100).unwrap();
    assert!(acts.iter().any(|a| a.message.contains("failed the final check")), "corruption was detected");
    r3.shutdown().await;
    sender.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_change_and_sender_offline() {
    let _ = tracing_subscriber::fmt().with_env_filter(std::env::var("TLOG").unwrap_or("relayless_core=info".into())).try_init();
    let tmp = tempfile::tempdir().unwrap();
    let shared = tmp.path().join("shared");
    make_file(&shared.join("data.bin"), 80 * 1024 * 1024, 9);

    let sdir = tmp.path().join("s");
    let sender = Core::start(opts(&sdir, "Sender")).await.unwrap();
    let receiver = Core::start(opts(&tmp.path().join("r"), "Receiver")).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(shared.to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    set_limit(&receiver, 20 * 1024);

    let id = receiver
        .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["data.bin".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    wait_progress(&receiver, &id, 16 * 1024 * 1024).await;

    // Sender goes offline: receiver keeps progress and waits.
    let addr = sender.endpoint_addr();
    sender.shutdown().await;
    drop(sender);
    let start = Instant::now();
    loop {
        let j = receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        if matches!(j.status, JobStatus::WaitingPeer | JobStatus::Reconnecting) && j.error.is_some() {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "never noticed sender went away: {:?}", j.status);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Sender comes back (same identity, new port).
    // Slow the transfer so the source modification below always lands while
    // most of the file is still outstanding (progress commits lag in-flight data).
    set_limit(&receiver, 3 * 1024);
    let sender = Core::start(opts(&sdir, "Sender")).await.unwrap();
    receiver.add_peer_addr(sender.endpoint_addr());
    let _ = addr;
    let resumed_from = receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap().verified_bytes;
    wait_progress(&receiver, &id, resumed_from + 4 * 1024 * 1024).await;

    // Source is modified mid-transfer: the job must stop rather than mix versions.
    #[cfg(windows)]
    {
        // While pieces are being read, the sender denies other writers, so an
        // edit is refused outright by Windows ("file in use").
        let start = Instant::now();
        let mut blocked = false;
        while start.elapsed() < Duration::from_secs(5) {
            match std::fs::OpenOptions::new().write(true).open(shared.join("data.bin")) {
                Err(e) if e.raw_os_error() == Some(32) => {
                    blocked = true;
                    break;
                }
                _ => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        assert!(blocked, "sender should lock the source against writers while reading");
        // Edit it while the transfer is paused; resuming must detect the change.
        receiver.pause_job(&id).await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        make_file(&shared.join("data.bin"), 80 * 1024 * 1024, 10);
        receiver.resume_job(&id).await.unwrap();
    }
    #[cfg(not(windows))]
    make_file(&shared.join("data.bin"), 80 * 1024 * 1024, 10);
    let j = wait_status(&receiver, &id, JobStatus::NeedsAttention, 60).await;
    println!("source change message: {:?}", j.error);
    // The user chooses to restart against the new version.
    receiver.restart_job(&id).await.unwrap();
    set_limit(&receiver, 0);
    let done = wait_status(&receiver, &id, JobStatus::Completed, 60).await;
    assert_eq!(hash(Path::new(&done.dest_path)), hash(&shared.join("data.bin")));

    receiver.shutdown().await;
    sender.shutdown().await;
}

/// Run with: cargo test --release -- --ignored big_transfer_memory --nocapture
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn big_transfer_memory() {
    let tmp = tempfile::tempdir().unwrap();
    let shared = tmp.path().join("shared");
    make_file(&shared.join("big.iso"), 1024 * 1024 * 1024, 11);
    let sender = Core::start(opts(&tmp.path().join("s"), "S")).await.unwrap();
    let receiver = Core::start(opts(&tmp.path().join("r"), "R")).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(shared.to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    let rss = || {
        std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("VmHWM"))
            .map(|l| l.to_string())
            .unwrap_or_default()
    };
    println!("before: {}", rss());
    let t = Instant::now();
    let id = receiver
        .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["big.iso".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    let done = wait_status(&receiver, &id, JobStatus::Completed, 300).await;
    let secs = t.elapsed().as_secs_f64();
    let hwm = std::fs::read_to_string("/proc/self/status").unwrap();
    println!("1 GiB in {secs:.1}s = {:.0} MB/s (sender+receiver in one process, incl. final re-verify)", 1024.0 / secs);
    for l in hwm.lines().filter(|l| l.starts_with("VmHWM") || l.starts_with("VmRSS")) {
        println!("{l}");
    }
    assert_eq!(hash(Path::new(&done.dest_path)), hash(&shared.join("big.iso")));
}

/// Pausing before any chunk has finished must keep the verified blocks
/// (the case of a slow phone link with many large chunks in flight).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_mid_chunk_keeps_progress() {
    let _ = tracing_subscriber::fmt().with_env_filter(std::env::var("TLOG").unwrap_or("relayless_core=info".into())).try_init();
    let tmp = tempfile::tempdir().unwrap();
    let shared = tmp.path().join("shared");
    let size = 100 * 1024 * 1024 + 4321;
    make_file(&shared.join("episode.mkv"), size, 21);

    let sender = Core::start(opts(&tmp.path().join("s"), "Sender")).await.unwrap();
    let rdir = tmp.path().join("r");
    let receiver = Core::start(opts(&rdir, "Phone")).await.unwrap();
    auto_approve(&sender);
    let share = sender.create_share(shared.to_str().unwrap(), None).unwrap();
    let peer = pair(&sender, &receiver, &share).await;
    let mut s = receiver.settings();
    s.chunk_size_mib = 32;
    s.parallel_streams = 12;
    s.download_limit_kbps = 4 * 1024; // 4 MB/s spread over 12 streams: no 32 MB chunk can finish quickly
    receiver.update_settings(s).unwrap();

    let id = receiver
        .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["episode.mkv".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    tokio::time::sleep(Duration::from_secs(4)).await;
    receiver.pause_job(&id).await.unwrap();
    let paused = wait_status(&receiver, &id, JobStatus::Paused, 10).await;
    println!("paused with {} bytes verified, {} whole chunks", paused.verified_bytes, paused.verified_chunks);
    assert_eq!(paused.verified_chunks, 0, "test setup: no chunk should have completed yet");
    assert!(paused.verified_bytes >= 8 * 1024 * 1024, "partial chunk progress was lost: {}", paused.verified_bytes);

    // Restart the app too: progress must come back from disk.
    receiver.shutdown().await;
    drop(receiver);
    let receiver = Core::start(opts(&rdir, "Phone")).await.unwrap();
    receiver.add_peer_addr(sender.endpoint_addr());
    let j = receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
    assert_eq!(j.verified_bytes, paused.verified_bytes, "progress survived a restart");
    let mut s = receiver.settings();
    s.download_limit_kbps = 0;
    receiver.update_settings(s).unwrap();
    receiver.resume_job(&id).await.unwrap();
    let done = wait_status(&receiver, &id, JobStatus::Completed, 120).await;
    assert_eq!(hash(Path::new(&done.dest_path)), hash(&shared.join("episode.mkv")));
    receiver.shutdown().await;
    sender.shutdown().await;
}
