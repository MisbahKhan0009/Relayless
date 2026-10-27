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

/// Token bucket shared by all streams in one direction. 0 = unlimited.
pub struct RateLimiter {
    bytes_per_sec: AtomicU64,
    state: parking_lot::Mutex<(f64, Instant)>,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        RateLimiter {
            bytes_per_sec: AtomicU64::new(bytes_per_sec),
            state: parking_lot::Mutex::new((0.0, Instant::now())),
        }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.bytes_per_sec.store(bytes_per_sec, Ordering::Relaxed);
    }

    pub async fn acquire(&self, n: usize) {
        loop {
            let rate = self.bytes_per_sec.load(Ordering::Relaxed);
            if rate == 0 {
                return;
            }
            let wait = {
                let mut st = self.state.lock();
                let now = Instant::now();
                let burst = (rate as f64 * 0.25).max(256.0 * 1024.0);
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
                None => return,
                Some(d) => tokio::time::sleep(d).await,
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
}
