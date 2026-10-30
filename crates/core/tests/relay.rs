//! Transfers over a local, rate-limited relay with direct paths disabled.
//!
//! This reproduces the slow cross-network case from field data: hole punching
//! failed, all data went through n0's public relay, which throttles each
//! client ("The relay is rate-limiting this endpoint"). The relay server
//! stops reading from the sender's TCP connection when its per-client token
//! bucket is empty, so packets queue behind it.
//!
//!   cargo test -p relayless-core --release --test relay -- --ignored --nocapture relay_throughput
//!
//! Environment: RELAY_KIBS (relay rate, KiB/s, 1024), RELAY_BURST_KIB (512),
//! RELAY_SIZE_MIB (48), RELAY_RUNS (1), RELAYLESS_TEST_CONGESTION (bbr|cubic),
//! RELAYLESS_ADAPTIVE_WINDOW (1|0).

use std::io::Write;
use std::num::NonZeroU32;
use std::path::Path;
use std::time::{Duration, Instant};

use iroh_relay::server::{ClientRateLimit, Limits, RelayConfig, Server, ServerConfig};
use iroh_relay::tls::CaTlsConfig;
use relayless_core::diag::read_summaries;
use relayless_core::*;
use serde_json::Value;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

async fn spawn_relay(bytes_per_sec: u32, burst: u32) -> (Server, iroh::RelayMap) {
    let mut relay = RelayConfig::new((std::net::Ipv4Addr::LOCALHOST, 0));
    relay.tls = Some(iroh_relay::server::testing::tls_config());
    relay.key_cache_capacity = Some(1024);
    let mut limit = ClientRateLimit::new(NonZeroU32::new(bytes_per_sec).unwrap());
    limit.max_burst_bytes = NonZeroU32::new(burst);
    let mut limits = Limits::default();
    limits.client_rx = Some(limit);
    relay.limits = limits;
    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    let server = Server::spawn(config).await.expect("relay server");
    let url: iroh::RelayUrl = format!("https://{}", server.https_addr().unwrap()).parse().unwrap();
    let map: iroh::RelayMap = iroh::RelayConfig::new(url, None).into();
    (server, map)
}

fn opts(dir: &Path, name: &str, map: iroh::RelayMap) -> CoreOptions {
    CoreOptions {
        data_dir: dir.join("data"),
        default_device_name: name.into(),
        default_download_dir: dir.join("downloads"),
        local_only: false,
        diagnostics: true,
        endpoint_hook: Some(Box::new(move |b: iroh::endpoint::Builder| {
            b.relay_mode(iroh::RelayMode::Custom(map))
                .ca_tls_config(CaTlsConfig::insecure_skip_verify())
                .clear_ip_transports()
        })),
    }
}

fn make_file(path: &Path, size: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(path).unwrap();
    let mut block = vec![0u8; 1 << 20];
    let mut x: u32 = 0x1234_5678;
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

async fn wait_online(c: &Core) {
    let t = Instant::now();
    while c.endpoint_addr().relay_urls().next().is_none() {
        assert!(t.elapsed() < Duration::from_secs(20), "endpoint never connected to the test relay");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

/// Interval samples after data started flowing (skips the hole-punch wait
/// and the first two samples of start-up), excluding the final partial one.
fn steady(samples: &[Value]) -> Vec<Value> {
    let start = samples.iter().position(|s| s["bytes"].as_u64().unwrap_or(0) > 0).unwrap_or(samples.len());
    let v: Vec<Value> = samples.iter().skip(start + 2).cloned().collect();
    let n = v.len().saturating_sub(1);
    v.into_iter().take(n).collect()
}

/// Mean, coefficient of variation and median of one sample field.
fn steadiness(samples: &[Value], key: &str) -> (f64, f64, f64) {
    let mut v: Vec<f64> = steady(samples).iter().filter_map(|s| s[key].as_f64()).collect();
    if v.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    let p50 = pct(&mut v, 0.5);
    (mean, if mean > 0.0 { sd / mean } else { f64::NAN }, p50)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn relay_throughput() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("TLOG").unwrap_or("relayless_core=warn,relayless_core::diag=info".into()))
        .try_init();
    let rate = env_u64("RELAY_KIBS", 1024) as u32 * 1024;
    let burst = env_u64("RELAY_BURST_KIB", 512) as u32 * 1024;
    let size = env_u64("RELAY_SIZE_MIB", 48) as usize * 1024 * 1024;
    let runs = env_u64("RELAY_RUNS", 1);
    for run in 0..runs {
        let (_relay, map) = spawn_relay(rate, burst).await;
        let tmp = tempfile::tempdir().unwrap();
        make_file(&tmp.path().join("shared/file.bin"), size);
        let sender = Core::start(opts(&tmp.path().join("s"), "Sender", map.clone())).await.unwrap();
        let receiver = Core::start(opts(&tmp.path().join("r"), "Receiver", map)).await.unwrap();
        wait_online(&sender).await;
        wait_online(&receiver).await;
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
        receiver.add_peer_addr(sender.endpoint_addr());
        let inv = sender.create_invitation(&share.id, 3600, Some(1), true, false).await.unwrap();
        let peer = receiver.join(&inv.invite).await.unwrap().peer_id;
        let t = Instant::now();
        let id = receiver
            .start_download(&peer, &share.id, vec![DownloadItem { path: vec!["file.bin".into()], is_dir: false }], None)
            .await
            .unwrap()[0]
            .clone();
        let limit = Duration::from_secs((size as u64 / rate as u64) * 4 + 120);
        let done = loop {
            let j = receiver.list_jobs().unwrap().into_iter().find(|j| j.id == id).unwrap();
            if j.status == JobStatus::Completed {
                break true;
            }
            if t.elapsed() > limit {
                break false;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let secs = t.elapsed().as_secs_f64();
        if !done {
            receiver.pause_job(&id).await.unwrap();
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        let (_, rdir) = receiver.diagnostics();
        let r = read_summaries(&rdir).into_iter().filter(|s| s["transfer_id"] == id.as_str()).last().unwrap();
        receiver.shutdown().await;
        sender.shutdown().await;
        let (_, sdir) = sender.diagnostics();
        let s = read_summaries(&sdir).into_iter().filter(|s| s["role"] == "sender").last().unwrap_or(Value::Null);
        let rs = r["samples"].as_array().cloned().unwrap_or_default();
        let ss = s["samples"].as_array().cloned().unwrap_or_default();
        let (mean, cv, _) = steadiness(&rs, "interval_avg_mib_per_sec");
        let (_, _, rtt_p50) = steadiness(&ss, "rtt_ms");
        let mut rtts: Vec<f64> = ss.iter().filter_map(|x| x["rtt_ms"].as_f64()).collect();
        let rtt_p95 = pct(&mut rtts, 0.95);
        let zero = steady(&rs).iter().filter(|x| x["interval_avg_bytes_per_sec"].as_f64().unwrap_or(0.0) < (rate as f64) * 0.1).count();
        let lost = s.pointer("/connection/at_end/totals/lost_packets").cloned().unwrap_or_default();
        let sent = s.pointer("/connection/at_end/totals/sent_packets").cloned().unwrap_or_default();
        let window = r.pointer("/transfer/flow_window").cloned().unwrap_or_default();
        println!(
            "relay_throughput run={run} congestion={} adaptive={} relay_kib_s={} done={done} secs={secs:.1} avg_mib_s={:.3} interval_mean_mib_s={mean:.3} interval_cv={cv:.2} zero_intervals={zero} sender_rtt_p50_ms={rtt_p50:.0} sender_rtt_p95_ms={rtt_p95:.0} sender_lost={lost} sender_sent={sent} stalls={} reconnects={} window={}",
            r.pointer("/configuration/transport/congestion_control_active").and_then(|v| v.as_str()).unwrap_or("?"),
            std::env::var("RELAYLESS_ADAPTIVE_WINDOW").unwrap_or("1".into()),
            rate / 1024,
            r["transfer"]["bytes_transferred"].as_f64().unwrap_or(0.0) / 1048576.0 / secs,
            r["errors"]["stall_timeout"].as_u64().unwrap_or(0),
            r["transfer"]["passes"].as_u64().unwrap_or(0).saturating_sub(1),
            window,
        );
        let series: Vec<String> = steady(&rs).iter().map(|x| format!("{:.2}", x["interval_avg_mib_per_sec"].as_f64().unwrap_or(0.0))).collect();
        let rtt_series: Vec<String> = ss.iter().map(|x| format!("{}", x["rtt_ms"].as_u64().unwrap_or(0))).collect();
        println!("relay_series run={run} receiver_mib_s=[{}] sender_rtt_ms=[{}]", series.join(" "), rtt_series.join(" "));
    }
}
