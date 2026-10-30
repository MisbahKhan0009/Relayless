//! Small helpers: time, ids, positioned file I/O, filename safety, rate limiting.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand::Rng;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn random_id() -> String {
    let mut b = [0u8; 12];
    rand::rng().fill_bytes(&mut b);
    data_encoding::HEXLOWER.encode(&b)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    rand::rng().fill_bytes(&mut b);
    b
}

pub fn short_peer(id: &str) -> String {
    id.chars().take(8).collect()
}

pub fn system_time_ms(t: io::Result<SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ------------------------------------------------------------------ positioned I/O

#[cfg(windows)]
pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

#[cfg(unix)]
pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buf, offset)
}

#[cfg(windows)]
fn write_at(file: &File, buf: &[u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_write(buf, offset)
}

#[cfg(unix)]
fn write_at(file: &File, buf: &[u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.write_at(buf, offset)
}

pub fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        match read_at(file, buf, offset) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "file shrank while reading")),
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub fn write_all_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        match write_at(file, buf, offset) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "write returned zero")),
            Ok(n) => {
                buf = &buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Opens a shared source file for reading. On Windows we deny concurrent
/// writers while a chunk is being read (falling back to shared access if a
/// writer already holds the file, in which case metadata checks still apply).
pub fn open_source(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;
        const FILE_SHARE_DELETE: u32 = 0x4;
        match std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(path)
        {
            Ok(f) => Ok(f),
            Err(e) if e.raw_os_error() == Some(32) => std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .open(path),
            Err(e) => Err(e),
        }
    }
    #[cfg(not(windows))]
    {
        File::open(path)
    }
}

// ------------------------------------------------------------------ filenames

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Makes an untrusted remote filename safe to create locally on Windows.
pub fn sanitize_filename(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    let s = s.trim_start().to_string();
    let mut s = if s.is_empty() || s == "." || s == ".." { "file".to_string() } else { s };
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        s = format!("_{s}");
    }
    if s.chars().count() > 200 {
        let ext = Path::new(&s)
            .extension()
            .map(|e| e.to_string_lossy().chars().take(20).collect::<String>());
        let mut base: String = s.chars().take(180).collect();
        if let Some(ext) = ext {
            base.push('.');
            base.push_str(&ext);
        }
        s = base;
    }
    s
}

/// Returns `path` if free, otherwise "name (1).ext", "name (2).ext", ...
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy()));
    for i in 1..10_000 {
        let candidate = parent.join(format!("{stem} ({i}){}", ext.clone().unwrap_or_default()));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

pub fn available_space(dir: &Path) -> Option<u64> {
    let mut p = dir.to_path_buf();
    loop {
        if p.exists() {
            return fs4::available_space(&p).ok();
        }
        if !p.pop() {
            return None;
        }
    }
}

// ------------------------------------------------------------------ rate limiting

/// Converts a limit from settings (KiB/s, 0 = unlimited) to bytes/s.
/// Saturates instead of wrapping: release builds have overflow checks off,
/// so a plain `kib * 1024` turns a huge limit into a tiny one.
pub fn kib_to_bytes_per_sec(kib_per_sec: u64) -> u64 {
    kib_per_sec.saturating_mul(1024)
}

/// Process-wide limiter counters (all transfers and peers in one direction).
#[derive(Default, Debug)]
pub struct LimiterStats {
    /// Calls to `acquire`.
    pub acquires: AtomicU64,
    /// Calls that had to sleep at least once.
    pub wait_events: AtomicU64,
    /// Total time spent sleeping, in nanoseconds.
    pub wait_ns: AtomicU64,
    /// Bytes charged to the bucket while a limit was set.
    pub charged_bytes: AtomicU64,
    /// Longest single `acquire` wait, in nanoseconds.
    pub max_wait_ns: AtomicU64,
}

/// Token bucket shared by all streams in one direction. 0 = unlimited.
pub struct RateLimiter {
    bytes_per_sec: AtomicU64,
    state: parking_lot::Mutex<(f64, Instant)>,
    pub stats: LimiterStats,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        RateLimiter {
            bytes_per_sec: AtomicU64::new(bytes_per_sec),
            state: parking_lot::Mutex::new((0.0, Instant::now())),
            stats: LimiterStats::default(),
        }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.bytes_per_sec.store(bytes_per_sec, Ordering::Relaxed);
    }

    /// Effective limit in bytes/s; 0 = unlimited.
    pub fn rate(&self) -> u64 {
        self.bytes_per_sec.load(Ordering::Relaxed)
    }

    /// Bucket capacity for a rate: a quarter second of data, at least 256 KiB.
    pub fn burst_for(rate: u64) -> f64 {
        (rate as f64 * 0.25).max(256.0 * 1024.0)
    }

    pub fn stats_json(&self) -> serde_json::Value {
        let s = &self.stats;
        let rate = self.rate();
        serde_json::json!({
            "scope": "process-wide, shared by all transfers and peers in this direction",
            "effective_bytes_per_sec": if rate == 0 { serde_json::Value::Null } else { serde_json::json!(rate) },
            "unlimited": rate == 0,
            "burst_bytes": if rate == 0 { serde_json::Value::Null } else { serde_json::json!(Self::burst_for(rate) as u64) },
            "acquires": s.acquires.load(Ordering::Relaxed),
            "wait_events": s.wait_events.load(Ordering::Relaxed),
            "wait_ms_total": s.wait_ns.load(Ordering::Relaxed) as f64 / 1e6,
            "max_wait_ms": s.max_wait_ns.load(Ordering::Relaxed) as f64 / 1e6,
            "charged_bytes": s.charged_bytes.load(Ordering::Relaxed),
        })
    }

    /// Takes `n` bytes of budget, sleeping as needed. Returns the time spent
    /// waiting (zero when unlimited or when tokens were available).
    pub async fn acquire(&self, n: usize) -> Duration {
        self.stats.acquires.fetch_add(1, Ordering::Relaxed);
        let start = Instant::now();
        let mut waited = false;
        self.acquire_inner(n, &mut waited).await;
        if !waited {
            return Duration::ZERO;
        }
        let d = start.elapsed();
        let ns = d.as_nanos().min(u64::MAX as u128) as u64;
        self.stats.wait_events.fetch_add(1, Ordering::Relaxed);
        self.stats.wait_ns.fetch_add(ns, Ordering::Relaxed);
        self.stats.max_wait_ns.fetch_max(ns, Ordering::Relaxed);
        d
    }

    async fn acquire_inner(&self, n: usize, waited: &mut bool) {
        loop {
            let rate = self.bytes_per_sec.load(Ordering::Relaxed);
            if rate == 0 {
                return;
            }
            let wait = {
                let mut st = self.state.lock();
                let now = Instant::now();
                let burst = Self::burst_for(rate);
                let elapsed = now.duration_since(st.1).as_secs_f64();
                st.0 = (st.0 + elapsed * rate as f64).min(burst);
                st.1 = now;
                if st.0 >= n as f64 || st.0 >= burst {
                    st.0 -= n as f64;
                    None
                } else {
                    let deficit = n as f64 - st.0;
                    Some(Duration::from_secs_f64((deficit / rate as f64).min(1.0)))
                }
            };
            match wait {
                None => {
                    self.stats.charged_bytes.fetch_add(n as u64, Ordering::Relaxed);
                    return;
                }
                Some(d) => {
                    *waited = true;
                    tokio::time::sleep(d).await
                }
            }
        }
    }
}

/// Exponential backoff with full jitter, capped.
pub fn backoff(attempt: u32) -> Duration {
    let base = 2u64.saturating_pow(attempt.min(6)).saturating_mul(1000);
    let capped = base.min(60_000);
    let jitter = rand::random::<u64>() % (capped / 2 + 1);
    Duration::from_millis(capped / 2 + jitter)
}

pub fn human_bytes(b: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_hostile_names() {
        assert_eq!(sanitize_filename("..\\..\\evil.exe"), ".._.._evil.exe");
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("con.txt"), "_con.txt");
        assert_eq!(sanitize_filename("a:b*c?.txt "), "a_b_c_.txt");
        assert_eq!(sanitize_filename(".."), "file");
        assert_eq!(sanitize_filename(""), "file");
        assert!(sanitize_filename(&"x".repeat(500)).chars().count() <= 201);
    }

    #[test]
    fn limit_conversion_is_kib_and_saturates() {
        assert_eq!(kib_to_bytes_per_sec(0), 0, "0 stays unlimited");
        assert_eq!(kib_to_bytes_per_sec(1), 1024);
        assert_eq!(kib_to_bytes_per_sec(25 * 1024), 25 * 1024 * 1024);
        // Regression: the old `kib * 1024` wraps in release builds (overflow
        // checks off), turning a huge "limit" into a tiny one.
        // 2^54 + 4 KiB/s is exactly representable as a JS number (the UI sends numbers).
        let huge = (1u64 << 54) + 4;
        assert_eq!(huge.wrapping_mul(1024), 4096, "old behaviour: wraps to 4 KiB/s");
        assert_eq!(kib_to_bytes_per_sec(huge), u64::MAX, "new behaviour: saturates (effectively unlimited)");
        assert_eq!((1u64 << 54).wrapping_mul(1024), 0, "old behaviour: wraps to 0, i.e. unlimited by accident");
    }

    #[tokio::test]
    async fn unlimited_limiter_never_waits_or_charges() {
        let l = RateLimiter::new(0);
        for _ in 0..100 {
            assert_eq!(l.acquire(1 << 20).await, Duration::ZERO);
        }
        assert_eq!(l.stats.acquires.load(Ordering::Relaxed), 100);
        assert_eq!(l.stats.wait_events.load(Ordering::Relaxed), 0);
        assert_eq!(l.stats.charged_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(l.stats_json()["unlimited"], true);
        assert!(l.stats_json()["effective_bytes_per_sec"].is_null());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn limiter_counts_waits_and_charged_bytes() {
        let rate = 4 * 1024 * 1024;
        let l = RateLimiter::new(rate);
        let t = Instant::now();
        let mut waited = Duration::ZERO;
        for _ in 0..4 {
            waited += l.acquire(1 << 20).await;
        }
        let elapsed = t.elapsed();
        // 4 MiB at 4 MiB/s with a 1 MiB burst and an empty start: ~0.75-1 s.
        assert!(elapsed >= Duration::from_millis(600), "too fast: {elapsed:?}");
        assert!(elapsed < Duration::from_secs(3), "too slow: {elapsed:?}");
        assert_eq!(l.stats.charged_bytes.load(Ordering::Relaxed), 4 << 20);
        assert!(l.stats.wait_events.load(Ordering::Relaxed) >= 1);
        let recorded = Duration::from_nanos(l.stats.wait_ns.load(Ordering::Relaxed));
        assert_eq!(recorded, waited, "returned waits and counters agree");
        assert!(recorded <= elapsed);
        assert_eq!(l.stats_json()["burst_bytes"], 1024 * 1024);
    }

    /// With S streams sharing one bucket, each stream's next 1 MiB waits about
    /// S x 1 MiB / rate. At low limits that exceeds the receiver's 90 s stall
    /// timeout (e.g. 12 streams at 100 KiB/s: ~123 s per block per stream).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn limiter_wait_grows_with_concurrent_streams() {
        let rate = 8 * 1024 * 1024;
        let l = std::sync::Arc::new(RateLimiter::new(rate));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let l = l.clone();
            set.spawn(async move { l.acquire(1 << 20).await });
        }
        let mut waits = Vec::new();
        while let Some(w) = set.join_next().await {
            waits.push(w.unwrap());
        }
        waits.sort();
        let max = *waits.last().unwrap();
        // 8 MiB requested, 2 MiB burst, 8 MiB/s: the last stream waits ~0.75 s.
        assert!(max >= Duration::from_millis(600), "last waiter {max:?}");
        assert!(max < Duration::from_secs(3));
        assert_eq!(l.stats.charged_bytes.load(Ordering::Relaxed), 8 << 20);
        let max_recorded = Duration::from_nanos(l.stats.max_wait_ns.load(Ordering::Relaxed));
        assert_eq!(max_recorded, max);
    }
}
