//! Multi-connection (MMCFTP-style) transfers over an emulated long, lossy
//! path: an in-process custom transport adds one-way delay, random loss, a
//! rate cap and a drop-tail queue. IP and relay transports are off, so all
//! traffic takes this path.
//!
//!   cargo test -p relayless-core --release --test multiconn -- --nocapture
//!   cargo test -p relayless-core --release --test multiconn -- --ignored --nocapture compare_lanes
//!
//! `compare_lanes` environment: LOSSY_RTT_MS (100), LOSSY_LOSS (0.01),
//! LOSSY_MBIT (100), LOSSY_SIZE_MIB (48), LOSSY_LANES ("1,8").

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use iroh::endpoint::transports::{CustomEndpoint, CustomSender, CustomTransport, RecvInfo, Transmit};
use iroh::{EndpointAddr, EndpointId, SecretKey, TransportAddr};
use iroh_base::CustomAddr;
use parking_lot::Mutex;
use relayless_core::diag::read_summaries;
use relayless_core::*;
use serde_json::Value;
use tokio::sync::mpsc;

const LOSSY_ID: u64 = 0x77;

#[derive(Debug, Clone, Copy)]
struct LinkCfg {
    one_way: Duration,
    loss: f64,
    bytes_per_sec: f64,
    max_queue: Duration,
}

type Packet = (Bytes, CustomAddr);

#[derive(Debug, Default)]
struct NetInner {
    inboxes: HashMap<[u8; 32], mpsc::UnboundedSender<Packet>>,
    /// Per direction: when the link is free again, and the delay line.
    links: HashMap<([u8; 32], [u8; 32]), (Instant, mpsc::UnboundedSender<(Instant, Packet)>)>,
}

#[derive(Debug, Clone)]
struct LossyNet {
    cfg: LinkCfg,
    inner: Arc<Mutex<NetInner>>,
}

impl LossyNet {
    fn new(cfg: LinkCfg) -> Self {
        LossyNet { cfg, inner: Default::default() }
    }

    fn transport(&self, id: EndpointId) -> Arc<LossyTransport> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner.lock().inboxes.insert(*id.as_bytes(), tx);
        Arc::new(LossyTransport {
            net: self.clone(),
            id: *id.as_bytes(),
            addrs: n0_watcher::Watchable::new(vec![addr_of(id.as_bytes())]),
            rx: Mutex::new(Some(rx)),
        })
    }

    fn send(&self, from: [u8; 32], to: [u8; 32], data: Bytes) {
        if rand::random::<f64>() < self.cfg.loss {
            return;
        }
        let now = Instant::now();
        let mut g = self.inner.lock();
        let Some(inbox) = g.inboxes.get(&to).cloned() else { return };
        let cfg = self.cfg;
        let link = g.links.entry((from, to)).or_insert_with(|| {
            // Delay line: delivers packets in order at their due time.
            let (tx, mut rx) = mpsc::unbounded_channel::<(Instant, Packet)>();
            tokio::spawn(async move {
                while let Some((due, p)) = rx.recv().await {
                    tokio::time::sleep_until(due.into()).await;
                    let _ = inbox.send(p);
                }
            });
            (now, tx)
        });
        let start = link.0.max(now);
        if start.duration_since(now) > cfg.max_queue {
            return; // drop-tail
        }
        let done = start + Duration::from_secs_f64(data.len() as f64 / cfg.bytes_per_sec);
        link.0 = done;
        let _ = link.1.send((done + cfg.one_way, (data, addr_of(&from))));
    }
}

fn addr_of(id: &[u8; 32]) -> CustomAddr {
    CustomAddr::from_parts(LOSSY_ID, id)
}

#[derive(Debug)]
struct LossyTransport {
    net: LossyNet,
    id: [u8; 32],
    addrs: n0_watcher::Watchable<Vec<CustomAddr>>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Packet>>>,
}

#[derive(Debug)]
struct LossyEndpoint {
    net: LossyNet,
    id: [u8; 32],
    addrs: n0_watcher::Watchable<Vec<CustomAddr>>,
    rx: mpsc::UnboundedReceiver<Packet>,
}

#[derive(Debug)]
struct LossySender {
    net: LossyNet,
    id: [u8; 32],
}

impl CustomTransport for LossyTransport {
    fn bind(&self) -> io::Result<Box<dyn CustomEndpoint>> {
        let rx = self.rx.lock().take().ok_or_else(|| io::Error::other("already bound"))?;
        Ok(Box::new(LossyEndpoint { net: self.net.clone(), id: self.id, addrs: self.addrs.clone(), rx }))
    }
}

impl CustomEndpoint for LossyEndpoint {
    fn watch_local_addrs(&self) -> n0_watcher::Direct<Vec<CustomAddr>> {
        self.addrs.watch()
    }

    fn create_sender(&self) -> Arc<dyn CustomSender> {
        Arc::new(LossySender { net: self.net.clone(), id: self.id })
    }

    fn poll_recv(
        &mut self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        metas: &mut [noq_udp::RecvMeta],
        recv_infos: &mut [RecvInfo],
    ) -> Poll<io::Result<usize>> {
        let n = bufs.len();
        if n == 0 {
            return Poll::Ready(Ok(0));
        }
        let mut packets = Vec::new();
        match self.rx.poll_recv_many(cx, &mut packets, n) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(0) => return Poll::Ready(Err(io::Error::other("closed"))),
            Poll::Ready(_) => {}
        }
        let mut count = 0;
        for (i, (data, from)) in packets.into_iter().enumerate() {
            if bufs[i].len() < data.len() {
                break;
            }
            bufs[i][..data.len()].copy_from_slice(&data);
            recv_infos[i] = RecvInfo::new(from, Some(addr_of(&self.id)));
            metas[i].len = data.len();
            metas[i].stride = data.len();
            count += 1;
        }
        Poll::Ready(Ok(count))
    }
}

impl CustomSender for LossySender {
    fn is_valid_send_addr(&self, addr: &CustomAddr) -> bool {
        addr.id() == LOSSY_ID
    }

    fn poll_send(&self, _cx: &mut Context, dst: &CustomAddr, _src: Option<&CustomAddr>, t: &Transmit<'_>) -> Poll<io::Result<()>> {
        let to: [u8; 32] = dst.data().try_into().map_err(|_| io::Error::other("bad addr"))?;
        let seg = t.segment_size.unwrap_or(t.contents.len()).max(1);
        for chunk in t.contents.chunks(seg) {
            self.net.send(self.id, to, Bytes::copy_from_slice(chunk));
        }
        Poll::Ready(Ok(()))
    }
}

// ====================================================================== harness

fn make_file(path: &Path, size: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(path).unwrap();
    let mut block = vec![0u8; 1 << 20];
    let mut x: u32 = 0x9e37_79b9;
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

/// Starts a Core on the lossy network with a known identity.
async fn start(net: &LossyNet, dir: &Path, name: &str) -> (Core, EndpointId) {
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let key: [u8; 32] = rand::random();
    std::fs::write(data.join("identity.key"), key).unwrap();
    let id = SecretKey::from_bytes(&key).public();
    let transport = net.transport(id);
    let core = Core::start(CoreOptions {
        data_dir: data,
        default_device_name: name.into(),
        default_download_dir: dir.join("downloads"),
        local_only: false,
        diagnostics: true,
        endpoint_hook: Some(Box::new(move |b: iroh::endpoint::Builder| {
            b.relay_mode(iroh::RelayMode::Disabled).clear_ip_transports().add_custom_transport(transport)
        })),
    })
    .await
    .unwrap();
    (core, id)
}

struct Run {
    secs: f64,
    mib_s: f64,
    summary: Value,
}

async fn transfer(cfg: LinkCfg, size: usize, max_connections: u32) -> Run {
    let net = LossyNet::new(cfg);
    let tmp = tempfile::tempdir().unwrap();
    make_file(&tmp.path().join("shared/file.bin"), size);
    let (sender, sid) = start(&net, &tmp.path().join("s"), "Sender").await;
    let (receiver, _) = start(&net, &tmp.path().join("r"), "Receiver").await;
    let s2 = sender.clone();
    let mut rx = s2.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let CoreEvent::AccessRequested(r) = ev {
                s2.respond_access(&r.id, true).unwrap();
            }
        }
    });
    let share = sender.create_share(tmp.path().join("shared").to_str().unwrap(), None).unwrap();
    receiver.add_peer_addr(EndpointAddr::from_parts(sid, [TransportAddr::Custom(addr_of(sid.as_bytes()))]));
    let inv = sender.create_invitation(&share.id, 3600, Some(1), true, false).await.unwrap();
    let peer = receiver.join(&inv.invite).await.unwrap().peer_id;
    let mut s = receiver.settings();
    s.max_connections = max_connections;
    s.chunk_size_mib = 8;
    receiver.update_settings(s).unwrap();
    let t = Instant::now();
    let id = receiver
        .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["file.bin".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    let done = loop {
        let j = receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
        if j.status == JobStatus::Completed {
            break j;
        }
        assert!(t.elapsed() < Duration::from_secs(900), "transfer did not finish: {:?} {:?}", j.status, j.error);
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(hash(Path::new(&done.dest_path)), hash(&tmp.path().join("shared/file.bin")), "file intact");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, rdir) = receiver.diagnostics();
    let summary = read_summaries(&rdir).into_iter().filter(|s| s["transfer_id"] == id.as_str()).last().unwrap();
    receiver.shutdown().await;
    sender.shutdown().await;
    Run { secs, mib_s: size as f64 / 1048576.0 / secs, summary }
}

fn set_fast() {
    // Short controller timings so tests see several decisions.
    std::env::set_var("RELAYLESS_MMC_FAST", "1");
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("TLOG").unwrap_or("relayless_core=warn,relayless_core::diag=info".into()))
        .try_init();
}

/// On a lossy path the receiver opens extra connections, keeps them because
/// they raise goodput, and the file still arrives intact.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn adds_lanes_on_a_lossy_path() {
    set_fast();
    let cfg = LinkCfg {
        one_way: Duration::from_millis(10),
        loss: 0.02,
        bytes_per_sec: 25e6,
        max_queue: Duration::from_millis(100),
    };
    // 6 chunks of 8 MiB, so up to 6 streams can spread over lanes.
    let r = transfer(cfg, 48 * 1024 * 1024, 8).await;
    let mc = &r.summary["transfer"]["multi_connection"];
    println!("lossy path: {:.2} MiB/s in {:.1} s, multi_connection={mc}", r.mib_s, r.secs);
    assert!(mc["lanes_peak"].as_u64().unwrap() >= 2, "no extra lane: {mc}");
    assert!(mc["adds"].as_u64().unwrap() >= 1);
}

/// With the setting at 1, no extra connection is ever opened.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn one_connection_setting_is_respected() {
    set_fast();
    let cfg = LinkCfg {
        one_way: Duration::from_millis(15),
        loss: 0.02,
        bytes_per_sec: 25e6,
        max_queue: Duration::from_millis(100),
    };
    let r = transfer(cfg, 8 * 1024 * 1024, 1).await;
    let mc = &r.summary["transfer"]["multi_connection"];
    assert_eq!(mc["adds"], 0, "{mc}");
    assert_eq!(mc["lanes_peak"].as_u64().unwrap_or(1), 1);
}

/// Starts a download of a large file, lets it run for `secs`, and returns
/// the goodput over that time (bytes/s) and the receiver summary. A large
/// file keeps chunks queued, as in real use (a 60 GB file has ~3800 chunks).
async fn run_for(cfg: LinkCfg, size: usize, max_connections: u32, secs: u64) -> (f64, Vec<(f64, u64)>, Value) {
    let net = LossyNet::new(cfg);
    let tmp = tempfile::tempdir().unwrap();
    make_file(&tmp.path().join("shared/big.bin"), size);
    let (sender, sid) = start(&net, &tmp.path().join("s"), "Sender").await;
    let (receiver, _) = start(&net, &tmp.path().join("r"), "Receiver").await;
    let s2 = sender.clone();
    let mut rx = s2.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let CoreEvent::AccessRequested(r) = ev {
                s2.respond_access(&r.id, true).unwrap();
            }
        }
    });
    let share = sender.create_share(tmp.path().join("shared").to_str().unwrap(), None).unwrap();
    receiver.add_peer_addr(EndpointAddr::from_parts(sid, [TransportAddr::Custom(addr_of(sid.as_bytes()))]));
    let inv = sender.create_invitation(&share.id, 3600, Some(1), true, false).await.unwrap();
    let peer = receiver.join(&inv.invite).await.unwrap().peer_id;
    let mut s = receiver.settings();
    s.max_connections = max_connections;
    s.chunk_size_mib = 8;
    receiver.update_settings(s).unwrap();
    let id = receiver
        .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["big.bin".into()], is_dir: false }], None)
        .await
        .unwrap()[0]
        .clone();
    let done = |r: &Core| r.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap().done_bytes;
    let t = Instant::now();
    let mut series = Vec::new();
    while t.elapsed() < Duration::from_secs(secs) {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let b = done(&receiver);
        series.push((t.elapsed().as_secs_f64(), b));
        // A finished file would dilute the rate with idle time.
        if b >= size as u64 {
            break;
        }
    }
    let bytes = done(&receiver);
    let goodput = bytes as f64 / t.elapsed().as_secs_f64();
    receiver.pause_job(&id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, rdir) = receiver.diagnostics();
    let summary = read_summaries(&rdir).into_iter().filter(|s| s["transfer_id"] == id.as_str()).last().unwrap_or(Value::Null);
    receiver.shutdown().await;
    sender.shutdown().await;
    (goodput, series, summary)
}

/// Goodput with 1 lane against up to N lanes on the same lossy path, using
/// the production controller timings. Environment: LOSSY_RTT_MS (100),
/// LOSSY_LOSS (0.01), LOSSY_MBIT (100), LOSSY_SIZE_MIB (512), LOSSY_SECS
/// (240), LOSSY_LANES ("1,8").
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn compare_lanes() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("TLOG").unwrap_or("relayless_core=warn,relayless_core::diag=info".into()))
        .try_init();
    let env = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let cfg = LinkCfg {
        one_way: Duration::from_secs_f64(env("LOSSY_RTT_MS", 100.0) / 2000.0),
        loss: env("LOSSY_LOSS", 0.01),
        bytes_per_sec: env("LOSSY_MBIT", 100.0) * 1e6 / 8.0,
        max_queue: Duration::from_millis(100),
    };
    let size = env("LOSSY_SIZE_MIB", 512.0) as usize * 1024 * 1024;
    let secs = env("LOSSY_SECS", 240.0) as u64;
    let lanes = std::env::var("LOSSY_LANES").unwrap_or("1,8".into());
    for max in lanes.split(',').filter_map(|s| s.trim().parse::<u32>().ok()) {
        let (goodput, series, summary) = run_for(cfg, size, max, secs).await;
        let mc = &summary["transfer"]["multi_connection"];
        let per10: Vec<String> = series
            .windows(2)
            // Signed: done_bytes drops when an interrupted chunk's partial
            // progress is discarded.
            .map(|w| format!("{:.0}", (w[1].1 as f64 - w[0].1 as f64) / (w[1].0 - w[0].0) / 1e3))
            .collect();
        println!(
            "compare_lanes rtt_ms={:.0} loss={} link_mbit={:.0} max_connections={max} secs={secs} goodput_kb_s={:.0} lanes_now={} lanes_peak={} adds={} removes={} moves={} goodput_by_lanes={} last={}",
            cfg.one_way.as_secs_f64() * 2000.0,
            cfg.loss,
            cfg.bytes_per_sec * 8.0 / 1e6,
            goodput / 1e3,
            mc["lanes"],
            mc["lanes_peak"],
            mc["adds"],
            mc["removes"],
            mc["lane_moves"],
            mc["goodput_by_lanes"],
            mc["last_decision"],
        );
        println!("compare_series max_connections={max} kb_s_per_10s=[{}]", per10.join(" "));
    }
}
