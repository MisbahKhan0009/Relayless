//! Endpoint setup and route inspection.
//!
//! Throughput tuning: BBRv3 congestion control copes far better than loss-based
//! algorithms with long, lossy intercontinental paths. GSO/GRO segmentation
//! offload is on by default and keeps CPU per byte low.
//!
//! Window sizing: the QUIC stack aborts a connection if one stream's reassembly
//! buffer holds more than 1024 separate fragments, which heavy scattered loss
//! can cause with large per-stream windows. A stream can never have more holes
//! than packets in flight, so each stream's window is kept below 1024
//! minimum-size packets (~1 MB). Throughput on long links comes from running
//! many chunk streams in parallel instead (12 by default: ~1 MB x 12 in flight
//! sustains ~480 Mbit/s at 200 ms RTT, more with extra streams).

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use iroh::endpoint::{presets, Connection, IdleTimeout, QuicTransportConfig, VarInt};
use iroh::{Endpoint, RelayMap, RelayMode, RelayUrl, SecretKey, TransportAddr};
use tracing::warn;

use crate::proto::ALPN;
use crate::types::RouteInfo;

pub const STREAM_WINDOW: u32 = 1_000_000;
pub const CONN_WINDOW: u32 = 48 * 1024 * 1024;
pub const SEND_WINDOW: u64 = 48 * 1024 * 1024;
pub const MAX_BIDI_STREAMS: u32 = 64;
pub const KEEP_ALIVE: Duration = Duration::from_secs(5);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(40);

/// Connection receive window advertised in the handshake.
pub fn initial_receive_window() -> u32 {
    if crate::flow::enabled() {
        crate::flow::INITIAL_WINDOW as u32
    } else {
        CONN_WINDOW
    }
}

pub fn transport_config(congestion: &str) -> QuicTransportConfig {
    let mut cfg = QuicTransportConfig::builder()
        .stream_receive_window(VarInt::from_u32(STREAM_WINDOW))
        // With adaptive flow control the receiver starts small and grows the
        // window per connection (flow.rs); the initial MAX_DATA can't be taken back.
        .receive_window(VarInt::from_u32(initial_receive_window()))
        .send_window(SEND_WINDOW)
        .max_concurrent_bidi_streams(VarInt::from_u32(MAX_BIDI_STREAMS))
        .max_concurrent_uni_streams(VarInt::from_u32(0))
        .keep_alive_interval(KEEP_ALIVE)
        .max_idle_timeout(IdleTimeout::try_from(IDLE_TIMEOUT).ok());
    cfg = if congestion != "bbr" {
        cfg.congestion_controller_factory(Arc::new(noq_proto::congestion::CubicConfig::default()))
    } else {
        cfg.congestion_controller_factory(Arc::new(noq_proto::congestion::Bbr3Config::default()))
    };
    cfg.build()
}

/// Parses the user's relay list (one URL per line or comma separated).
pub fn parse_relays(list: &str) -> Vec<RelayUrl> {
    list.split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| s.contains('.') || s.contains("localhost"))
        .filter_map(|s| {
            let s = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
            match s.parse::<RelayUrl>() {
                Ok(u) => Some(u),
                Err(e) => {
                    warn!("ignoring relay {s}: {e}");
                    None
                }
            }
        })
        .collect()
}

/// Main endpoint: DNS/pkarr lookup, and either n0's public relays or the
/// user's own relays for hole-punching coordination and fallback.
pub async fn bind_main(secret: SecretKey, congestion: &str, relays: &str) -> Result<Endpoint> {
    let custom = parse_relays(relays);
    let mut b = Endpoint::builder(presets::N0)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .transport_config(transport_config(congestion));
    if !custom.is_empty() {
        b = b.relay_mode(RelayMode::Custom(RelayMap::from_iter(custom)));
    }
    Ok(b.bind().await?)
}

/// Test endpoint: minimal preset plus caller adjustments (see `CoreOptions::endpoint_hook`).
pub async fn bind_custom(
    secret: SecretKey,
    congestion: &str,
    hook: Box<dyn FnOnce(iroh::endpoint::Builder) -> iroh::endpoint::Builder + Send>,
) -> Result<Endpoint> {
    let b = Endpoint::builder(presets::Minimal)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .transport_config(transport_config(congestion));
    Ok(hook(b).bind().await?)
}

/// Endpoint without relays or lookup, for local tests. Uses the same
/// congestion setting as the main endpoint so it can be compared in tests.
pub async fn bind_local(secret: SecretKey, congestion: &str) -> Result<Endpoint> {
    let ep = Endpoint::builder(presets::Minimal)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .transport_config(transport_config(congestion))
        .bind_addr("127.0.0.1:0")?
        .bind()
        .await?;
    Ok(ep)
}

/// Kind of the selected path ("direct", "relay", "other" or "none") without
/// reading path statistics; cheap enough to call per block.
pub fn route_kind(conn: &Connection) -> &'static str {
    match conn.paths().iter().find(|p| p.is_selected()) {
        Some(p) if p.is_ip() => "direct",
        Some(p) if p.is_relay() => "relay",
        Some(_) => "other",
        None => "none",
    }
}

pub fn route_of(conn: &Connection) -> RouteInfo {
    let paths = conn.paths();
    let selected = paths.iter().find(|p| p.is_selected());
    match selected {
        Some(p) => {
            let kind = if p.is_ip() {
                "direct"
            } else if p.is_relay() {
                "relay"
            } else {
                "other"
            };
            let remote = match p.remote_addr() {
                TransportAddr::Ip(a) => Some(a.to_string()),
                TransportAddr::Relay(u) => Some(u.to_string()),
                _ => None,
            };
            let rtt = p.rtt();
            RouteInfo {
                kind: kind.into(),
                rtt_ms: Some(rtt.as_millis() as u64),
                remote,
            }
        }
        None => RouteInfo::none(),
    }
}

/// Name of the congestion controller `transport_config` installs for a setting.
pub fn congestion_name(setting: &str) -> &'static str {
    if setting == "bbr" {
        "bbr3"
    } else {
        "cubic"
    }
}

/// Transport values passed to the QUIC builder (noq does not expose the
/// applied configuration, so these come from this file, not from runtime).
pub fn transport_json(active_congestion: &str) -> serde_json::Value {
    serde_json::json!({
        "source": "constants passed to QuicTransportConfig in net.rs",
        "stream_window_bytes": STREAM_WINDOW,
        "connection_window_bytes": CONN_WINDOW,
        "connection_window_initial_bytes": initial_receive_window(),
        "adaptive_flow_window": crate::flow::enabled(),
        "send_window_bytes": SEND_WINDOW,
        "max_concurrent_bidi_streams": MAX_BIDI_STREAMS,
        "keep_alive_ms": KEEP_ALIVE.as_millis() as u64,
        "idle_timeout_ms": IDLE_TIMEOUT.as_millis() as u64,
        "congestion_control_active": active_congestion,
    })
}

/// Coarse scope of an IP address. The address itself is never reported.
pub fn ip_scope(ip: std::net::IpAddr) -> &'static str {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            if v.is_loopback() {
                "loopback"
            } else if v.is_private() {
                "private"
            } else if v.is_link_local() {
                "link_local"
            } else if o[0] == 100 && (o[1] & 0xc0) == 64 {
                "cgnat"
            } else {
                "public"
            }
        }
        IpAddr::V6(v) => {
            if let Some(m) = v.to_ipv4_mapped() {
                return ip_scope(IpAddr::V4(m));
            }
            if v.is_loopback() {
                "loopback"
            } else if (v.segments()[0] & 0xffc0) == 0xfe80 {
                "link_local"
            } else if (v.segments()[0] & 0xfe00) == 0xfc00 {
                "private"
            } else {
                "public"
            }
        }
    }
}

/// Path and connection statistics for diagnostics: route kind, RTT,
/// congestion window, loss and stream-limit frames. No IP addresses.
pub fn path_report(conn: &Connection) -> serde_json::Value {
    use serde_json::json;
    let mut paths = Vec::new();
    let mut selected_rtt: Option<f64> = None;
    let mut selected_kind = "none";
    for p in conn.paths().iter() {
        let kind = if p.is_ip() {
            "direct"
        } else if p.is_relay() {
            "relay"
        } else {
            "other"
        };
        let (addr_kind, scope, relay) = match p.remote_addr() {
            TransportAddr::Ip(a) => (if a.is_ipv4() { "ipv4" } else { "ipv6" }, Some(ip_scope(a.ip())), None),
            TransportAddr::Relay(u) => ("relay", None, Some(u.to_string())),
            _ => ("other", None, None),
        };
        let s = p.stats();
        let rtt_ms = (s.rtt.as_secs_f64() * 1000.0 * 10.0).round() / 10.0;
        // Congestion controller's own view (pacing rate is what BBR sends at).
        let cc = conn.congestion_state(p.id()).map(|c| c.metrics());
        if p.is_selected() {
            selected_rtt = Some(rtt_ms);
            selected_kind = kind;
        }
        paths.push(json!({
            "id": format!("{:?}", p.id()),
            "kind": kind,
            "selected": p.is_selected(),
            "addr_kind": addr_kind,
            "ip_scope": scope,
            "relay_url": relay,
            "rtt_ms": rtt_ms,
            "cwnd_bytes": s.cwnd,
            "bandwidth_estimate_bits_per_sec": s.bandwidth_estimate,
            "pacing_rate_bytes_per_sec": cc.as_ref().and_then(|m| m.pacing_rate),
            "send_quantum_bytes": cc.as_ref().and_then(|m| m.send_quantum),
            "ssthresh_bytes": cc.as_ref().and_then(|m| m.ssthresh),
            "congestion_events": s.congestion_events,
            "spurious_congestion_events": s.spurious_congestion_events,
            "sent_packets": s.sent_packets,
            "sent_bytes": s.sent_bytes,
            "lost_packets": s.lost_packets,
            "lost_bytes": s.lost_bytes,
            "loss_ratio": if s.sent_packets > 0 { json!((s.lost_packets as f64 / s.sent_packets as f64 * 1e6).round() / 1e6) } else { serde_json::Value::Null },
            "udp_tx_bytes": s.udp_tx.bytes,
            "udp_rx_bytes": s.udp_rx.bytes,
            "black_holes_detected": s.black_holes_detected,
            "current_mtu": s.current_mtu,
        }));
    }
    let c = conn.stats();
    json!({
        "selected_kind": selected_kind,
        "selected_rtt_ms": selected_rtt,
        "paths": paths,
        "totals": {
            "udp_tx_bytes": c.udp_tx.bytes,
            "udp_rx_bytes": c.udp_rx.bytes,
            "udp_tx_datagrams": c.udp_tx.datagrams,
            "udp_rx_datagrams": c.udp_rx.datagrams,
            "sent_packets": c.sent_packets,
            "lost_packets": c.lost_packets,
            "lost_bytes": c.lost_bytes,
            // Sent when this side wants more streams than the peer allows (64).
            "streams_blocked_bidi_frames_sent": c.frame_tx.streams_blocked_bidi,
            "streams_blocked_bidi_frames_received": c.frame_rx.streams_blocked_bidi,
            "max_streams_bidi_frames_received": c.frame_rx.max_streams_bidi,
            "reset_stream_frames_sent": c.frame_tx.reset_stream,
            "reset_stream_frames_received": c.frame_rx.reset_stream,
            "stop_sending_frames_sent": c.frame_tx.stop_sending,
            "stop_sending_frames_received": c.frame_rx.stop_sending,
            "max_stream_data_frames_received": c.frame_rx.max_stream_data,
            "max_data_frames_received": c.frame_rx.max_data,
            // noq-proto 1.4 never sends these, so they are reported as unavailable.
            "stream_data_blocked_frames": null,
            "data_blocked_frames": null,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_scopes() {
        let s = |a: &str| ip_scope(a.parse().unwrap());
        assert_eq!(s("127.0.0.1"), "loopback");
        assert_eq!(s("192.168.1.4"), "private");
        assert_eq!(s("10.0.0.1"), "private");
        assert_eq!(s("169.254.3.3"), "link_local");
        assert_eq!(s("100.72.0.1"), "cgnat");
        assert_eq!(s("8.8.8.8"), "public");
        assert_eq!(s("::1"), "loopback");
        assert_eq!(s("fe80::1"), "link_local");
        assert_eq!(s("fd00::1"), "private");
        assert_eq!(s("2001:db8::1"), "public");
        assert_eq!(s("::ffff:192.168.0.1"), "private");
    }

    #[test]
    fn congestion_names() {
        assert_eq!(congestion_name("cubic"), "cubic");
        assert_eq!(congestion_name("bbr"), "bbr3");
        assert_eq!(congestion_name("anything"), "cubic");
    }

    #[test]
    fn parses_relay_lists() {
        assert!(parse_relays("").is_empty());
        let r = parse_relays("relay.example.org, https://eu.example.org\n  bad url with spaces");
        let s: Vec<String> = r.iter().map(|u| u.to_string()).collect();
        assert!(s[0].starts_with("https://relay.example.org"));
        assert!(s[1].starts_with("https://eu.example.org"));
        assert_eq!(s.len(), 2, "words that aren't host names are ignored");
    }
}
