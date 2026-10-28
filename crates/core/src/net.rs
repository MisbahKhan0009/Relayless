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

pub fn transport_config(congestion: &str) -> QuicTransportConfig {
    let mut cfg = QuicTransportConfig::builder()
        .stream_receive_window(VarInt::from_u32(STREAM_WINDOW))
        .receive_window(VarInt::from_u32(CONN_WINDOW))
        .send_window(SEND_WINDOW)
        .max_concurrent_bidi_streams(VarInt::from_u32(64))
        .max_concurrent_uni_streams(VarInt::from_u32(0))
        .keep_alive_interval(Duration::from_secs(5))
        .max_idle_timeout(IdleTimeout::try_from(Duration::from_secs(40)).ok());
    cfg = if congestion == "cubic" {
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

/// Endpoint without relays or lookup, for local tests.
pub async fn bind_local(secret: SecretKey) -> Result<Endpoint> {
    let ep = Endpoint::builder(presets::Minimal)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .transport_config(transport_config("bbr"))
        .bind_addr("127.0.0.1:0")?
        .bind()
        .await?;
    Ok(ep)
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

#[cfg(test)]
mod tests {
    use super::*;

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
