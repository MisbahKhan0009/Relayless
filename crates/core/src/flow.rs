//! Receiver-side adaptive flow control.
//!
//! Field data (cross-network transfer over n0's public relay) showed the
//! sender's congestion controller filling a deep queue in front of the
//! relay's per-client rate limiter: up to 12 MB in flight (12 streams x 1 MB
//! stream windows), RTT of 2-33 s, 10-30 % loss, bursts of 2-3 MiB/s
//! followed by tens of seconds at zero, stall timeouts and an idle-timeout
//! disconnect. The queue sits on the sender's side of the relay, so the
//! sender's congestion controller only sees it through inflated RTT, and
//! Cubic does not react to delay at all.
//!
//! The receiver bounds the queue itself through QUIC connection-level flow
//! control (`MAX_DATA`): it lets the sender have at most
//! `delivery_rate x (base_rtt + QUEUE_TARGET)` bytes outstanding. That is
//! enough to keep the link busy (one bandwidth-delay product) plus a small,
//! fixed amount of queue, so throughput stays at the bottleneck rate and RTT
//! stays near `base_rtt + QUEUE_TARGET` whatever the sender's congestion
//! controller does. It needs no protocol change and works with old senders.
//!
//! Rules, evaluated every `TICK`:
//! - delivery rate = bytes read in the last ~1 s; the controller uses the
//!   maximum over the last 2 s (`r_max`), so a short dip does not shrink it.
//! - base RTT = lowest smoothed RTT seen on the current path.
//! - target = r_max x (base + QUEUE_TARGET).
//! - If the RTT shows a queue above the target, the window moves down to the
//!   target. Otherwise it may only grow (an idle or app-limited connection
//!   keeps its window). When the window is the limit, the rate follows the
//!   window and the target exceeds it, so the window grows by about
//!   `(base + QUEUE_TARGET) / rtt` per step until the queue appears.
//! - A drain probe re-measures the base RTT: the window drops to ~50 ms of
//!   data until the RTT stops falling (at least 4 base RTTs, at most 8 s).
//!   It runs after 30 s of continuous queueing, and at most every 60 s while
//!   data flows (in case the first RTT sample already included a queue). A
//!   real path change can raise the base this way, and a stale value can't
//!   inflate the queue. On a healthy path a probe costs about 1 % of
//!   throughput.
//! - A path change (relay to direct, new address) resets the estimates.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How often the controller runs.
pub const TICK: Duration = Duration::from_millis(200);
/// Queueing delay the receiver allows on top of the base RTT.
pub const QUEUE_TARGET: Duration = Duration::from_millis(100);
/// Smallest normal window. At 0.5 MB/s it allows ~0.25 s of queue.
pub const MIN_WINDOW: u64 = 128 * 1024;
/// Window during a drain probe can go below `MIN_WINDOW` (it must be under
/// one bandwidth-delay product for the queue to empty).
const PROBE_MIN_WINDOW: u64 = 32 * 1024;
/// A probe ends once the RTT has stopped falling for this long.
const PROBE_SETTLE: Duration = Duration::from_millis(600);
/// Window advertised before any measurement (also the transport default).
pub const INITIAL_WINDOW: u64 = 4 * 1024 * 1024;
/// Largest window (the previous static connection window).
pub const MAX_WINDOW: u64 = crate::net::CONN_WINDOW as u64;
const RATE_SPAN: Duration = Duration::from_secs(1);
const RATE_MAX_SPAN: Duration = Duration::from_secs(2);
/// Probe after this long with a queue above the target.
const PROBE_INTERVAL: Duration = Duration::from_secs(30);
/// Probe at least this often while data flows, in case the base RTT was
/// first measured with a queue already present (then no queue is visible).
const PROBE_MAX_INTERVAL: Duration = Duration::from_secs(60);
const PROBE_MAX_DURATION: Duration = Duration::from_secs(8);

/// `RELAYLESS_ADAPTIVE_WINDOW=0` turns the controller off (static 48 MiB
/// connection window, the old behaviour) for A/B measurements.
pub fn enabled() -> bool {
    !matches!(std::env::var("RELAYLESS_ADAPTIVE_WINDOW").ok().as_deref().map(str::trim), Some("0" | "off" | "false" | "no"))
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FlowSnapshot {
    pub enabled: bool,
    pub window_bytes: u64,
    pub window_min_bytes: u64,
    pub window_max_bytes: u64,
    pub base_rtt_ms: Option<f64>,
    pub srtt_ms: Option<f64>,
    pub rate_max_bytes_per_sec: Option<f64>,
    pub increases: u64,
    pub decreases: u64,
    pub probes: u64,
    pub path_resets: u64,
}

#[derive(Debug)]
pub struct FlowController {
    window: u64,
    rx: VecDeque<(Instant, u64)>,
    rates: VecDeque<(Instant, f64)>,
    base: Option<Duration>,
    srtt: Option<Duration>,
    queued_since: Option<Instant>,
    last_probe: Instant,
    probe: Option<Probe>,
    pub snap: FlowSnapshot,
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    start: Instant,
    /// Last time the RTT fell by more than 3 %.
    improved: Instant,
    low: Duration,
    min_duration: Duration,
}

impl FlowController {
    pub fn new(now: Instant) -> Self {
        FlowController {
            window: INITIAL_WINDOW,
            rx: VecDeque::new(),
            rates: VecDeque::new(),
            base: None,
            srtt: None,
            queued_since: None,
            last_probe: now,
            probe: None,
            snap: FlowSnapshot {
                enabled: true,
                window_bytes: INITIAL_WINDOW,
                window_min_bytes: INITIAL_WINDOW,
                window_max_bytes: INITIAL_WINDOW,
                ..Default::default()
            },
        }
    }

    pub fn window(&self) -> u64 {
        self.window
    }

    /// The selected path changed: forget RTT and rate estimates.
    pub fn reset_path(&mut self, now: Instant) {
        self.rx.clear();
        self.rates.clear();
        self.base = None;
        self.queued_since = None;
        self.probe = None;
        self.last_probe = now;
        self.snap.path_resets += 1;
        // Start from the initial window again (never above the current one if
        // that is smaller, so a fresh relay path isn't flooded).
        let w = self.window.min(INITIAL_WINDOW).max(MIN_WINDOW);
        self.set(w);
    }

    /// Delivery rate over the last ~1 s, if at least 0.5 s of history exists.
    fn rate(&mut self, now: Instant, rx_total: u64) -> Option<f64> {
        self.rx.push_back((now, rx_total));
        while self.rx.len() > 2 && now.duration_since(self.rx[1].0) >= RATE_SPAN {
            self.rx.pop_front();
        }
        let (t0, b0) = *self.rx.front()?;
        let dt = now.duration_since(t0).as_secs_f64();
        if dt < 0.5 {
            return None;
        }
        Some(rx_total.saturating_sub(b0) as f64 / dt)
    }

    /// One controller step. `rx_total` is the cumulative payload bytes read
    /// from this connection, `srtt` the selected path's smoothed RTT.
    /// Returns the new window when it should be applied.
    pub fn tick(&mut self, now: Instant, rx_total: u64, srtt: Option<Duration>) -> Option<u64> {
        if let Some(r) = self.rate(now, rx_total) {
            self.rates.push_back((now, r));
        }
        while self.rates.front().is_some_and(|(t, _)| now.duration_since(*t) > RATE_MAX_SPAN) {
            self.rates.pop_front();
        }
        let r_max = self.rates.iter().map(|(_, r)| *r).fold(None, |m: Option<f64>, r| Some(m.map_or(r, |m| m.max(r))));
        self.srtt = srtt;
        self.snap.srtt_ms = srtt.map(ms);
        self.snap.rate_max_bytes_per_sec = r_max.map(|r| r.round());
        let srtt = srtt?;
        if srtt.is_zero() {
            return None;
        }
        self.base = Some(self.base.map_or(srtt, |b| b.min(srtt)));

        // Drain probe in progress: hold the small window until the RTT stops
        // falling (the queue has drained), then take the lowest RTT seen.
        if let Some(mut p) = self.probe.take() {
            if srtt.as_secs_f64() < p.low.as_secs_f64() * 0.97 {
                p.improved = now;
            }
            p.low = p.low.min(srtt);
            let elapsed = now.duration_since(p.start);
            let settled = elapsed >= p.min_duration && now.duration_since(p.improved) >= PROBE_SETTLE;
            if !settled && elapsed < PROBE_MAX_DURATION {
                self.probe = Some(p);
                return None;
            }
            self.last_probe = now;
            self.queued_since = None;
            // Re-baseline: may go up (real path change) or down.
            self.base = Some(p.low);
        }
        let base = self.base?;
        self.snap.base_rtt_ms = Some(ms(base));
        let Some(r_max) = r_max else { return None };

        let target = (r_max * (base + QUEUE_TARGET).as_secs_f64()) as u64;
        let queued = srtt > base + QUEUE_TARGET;
        if queued {
            self.queued_since.get_or_insert(now);
        } else {
            self.queued_since = None;
        }

        let since_probe = now.duration_since(self.last_probe);
        // Small enough to drain any queue even if `base` is overestimated:
        // 50 ms of data at the current rate.
        let probe_window = ((r_max * 0.05) as u64).max(PROBE_MIN_WINDOW);
        let long_queue = self.queued_since.is_some_and(|t| now.duration_since(t) >= PROBE_INTERVAL) && since_probe >= PROBE_INTERVAL;
        let periodic = since_probe >= PROBE_MAX_INTERVAL && r_max > 0.0 && self.window > probe_window * 2;
        if long_queue || periodic {
            self.probe = Some(Probe {
                start: now,
                improved: now,
                low: srtt,
                min_duration: (base * 4).max(Duration::from_millis(400)),
            });
            self.snap.probes += 1;
            return self.set(probe_window);
        }

        let new = if queued { target } else { target.max(self.window) };
        let new = new.clamp(MIN_WINDOW, MAX_WINDOW);
        // Ignore changes under 1/16 of the window to avoid churn.
        let diff = new.abs_diff(self.window);
        if diff == 0 || (diff < self.window / 16 && new != MIN_WINDOW && new != MAX_WINDOW) {
            return None;
        }
        self.set(new)
    }

    fn set(&mut self, w: u64) -> Option<u64> {
        if w == self.window {
            return None;
        }
        if w > self.window {
            self.snap.increases += 1;
        } else {
            self.snap.decreases += 1;
        }
        self.window = w;
        self.snap.window_bytes = w;
        self.snap.window_min_bytes = self.snap.window_min_bytes.min(w);
        self.snap.window_max_bytes = self.snap.window_max_bytes.max(w);
        Some(w)
    }
}

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 10_000.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulates a bottleneck of `cap` bytes/s with base RTT `base`: the
    /// sender keeps the receiver's window full, so the queue holds
    /// `window - cap x base` bytes and RTT = base + queue / cap.
    fn simulate(cap: f64, base: Duration, secs: u64) -> (FlowController, Vec<(f64, f64)>) {
        let t0 = Instant::now();
        let mut fc = FlowController::new(t0);
        // The connection exists before data flows: the handshake RTT is clean.
        fc.tick(t0, 0, Some(base));
        let mut rx = 0u64;
        let mut trace = Vec::new();
        let steps = secs * 1000 / TICK.as_millis() as u64;
        for i in 1..=steps {
            let now = t0 + TICK * i as u32;
            let w = fc.window() as f64;
            let bdp = cap * base.as_secs_f64();
            // Window-limited below the BDP, link-limited above it.
            let rate = if w <= bdp { w / base.as_secs_f64() } else { cap };
            let queue = (w - bdp).max(0.0);
            let rtt = base + Duration::from_secs_f64(queue / cap);
            rx += (rate * TICK.as_secs_f64()) as u64;
            fc.tick(now, rx, Some(rtt));
            trace.push((rate, rtt.as_secs_f64()));
        }
        (fc, trace)
    }

    #[test]
    fn bounds_queue_on_a_slow_relay() {
        // 0.5 MB/s relay, 150 ms base RTT: the old static setup let 12 MB
        // queue (24 s of delay). The controller must keep RTT near
        // base + target and still use the whole link.
        let (fc, trace) = simulate(500_000.0, Duration::from_millis(150), 120);
        let tail = &trace[trace.len() - 300..];
        let max_rtt = tail.iter().map(|t| t.1).fold(0.0, f64::max);
        let avg_rate = tail.iter().map(|t| t.0).sum::<f64>() / tail.len() as f64;
        assert!(max_rtt < 0.5, "queue not bounded: rtt {max_rtt:.2} s, window {}", fc.window());
        // Probes cost a little; the link must stay essentially full.
        assert!(avg_rate >= 0.97 * 500_000.0, "link underused: {avg_rate:.0}");
    }

    #[test]
    fn recovers_when_started_behind_an_existing_queue() {
        // No clean RTT sample: the first one already includes a deep queue
        // (e.g. a controller started mid-transfer). Periodic probes must
        // still find the real base and bound the queue within a few minutes.
        let cap = 500_000.0;
        let base = Duration::from_millis(150);
        let t0 = Instant::now();
        let mut fc = FlowController::new(t0);
        let mut rx = 0u64;
        let mut last_rtt = 0.0;
        // A queue drains at `cap`, so model it explicitly here.
        let mut queue = 4.0 * 1024.0 * 1024.0;
        for i in 1..=(300_000 / TICK.as_millis() as u32) {
            let now = t0 + TICK * i;
            let w = fc.window() as f64;
            let bdp = cap * base.as_secs_f64();
            let want = (w - bdp).max(0.0);
            let dt = TICK.as_secs_f64();
            queue = if want < queue { (queue - cap * dt).max(want) } else { want };
            let rate = if w < bdp && queue == 0.0 { w / base.as_secs_f64() } else { cap };
            let rtt = base + Duration::from_secs_f64(queue / cap);
            rx += (rate * dt) as u64;
            fc.tick(now, rx, Some(rtt));
            last_rtt = rtt.as_secs_f64();
        }
        assert!(fc.snap.probes >= 2);
        assert!(last_rtt < 1.0, "queue still {last_rtt:.2} s after 5 min, base {:?}", fc.snap.base_rtt_ms);
    }

    #[test]
    fn grows_to_fill_a_fast_long_path() {
        // 50 MB/s, 200 ms: BDP 10 MB, above the initial 4 MiB window.
        let (fc, trace) = simulate(50_000_000.0, Duration::from_millis(200), 30);
        let tail = &trace[trace.len() - 25..];
        let min_rate = tail.iter().map(|t| t.0).fold(f64::MAX, f64::min);
        let max_rtt = tail.iter().map(|t| t.1).fold(0.0, f64::max);
        assert!(min_rate >= 49_000_000.0, "did not fill the link: {min_rate:.0} B/s, window {}", fc.window());
        assert!(max_rtt < 0.2 + 0.25, "queue too deep: {max_rtt:.3} s");
        assert!(fc.window() <= MAX_WINDOW);
    }

    #[test]
    fn lan_and_loopback_are_not_limited() {
        // Includes a periodic probe (at 60 s): on a short path the probe
        // window is still above one BDP, so nothing is lost.
        let (_, trace) = simulate(110_000_000.0, Duration::from_millis(2), 90);
        let tail = &trace[trace.len() - 400..];
        assert!(tail.iter().all(|t| t.0 >= 109_000_000.0));
    }

    #[test]
    fn periodic_probe_costs_little_on_a_long_fast_path() {
        let (fc, trace) = simulate(50_000_000.0, Duration::from_millis(200), 180);
        let tail = &trace[50..];
        let avg = tail.iter().map(|t| t.0).sum::<f64>() / tail.len() as f64;
        assert!(fc.snap.probes >= 2);
        assert!(avg >= 0.97 * 50_000_000.0, "average {avg:.0} B/s");
    }

    #[test]
    fn idle_connection_keeps_its_window() {
        let t0 = Instant::now();
        let mut fc = FlowController::new(t0);
        fc.window = 20 * 1024 * 1024;
        for i in 1..100u32 {
            // No bytes, RTT at base: nothing is queued, the window must not shrink.
            assert_eq!(fc.tick(t0 + TICK * i, 0, Some(Duration::from_millis(50))), None);
        }
        assert_eq!(fc.window(), 20 * 1024 * 1024);
    }

    #[test]
    fn shrinks_quickly_when_a_queue_appears() {
        let t0 = Instant::now();
        let mut fc = FlowController::new(t0);
        fc.window = 12 * 1024 * 1024;
        let mut rx = 0u64;
        // Establish the base RTT with no queue.
        for i in 1..=10u32 {
            rx += 100_000;
            fc.tick(t0 + TICK * i, rx, Some(Duration::from_millis(150)));
        }
        // Then 0.5 MB/s with a 20 s queue, as in the field data.
        for i in 11..=20u32 {
            rx += 100_000;
            fc.tick(t0 + TICK * i, rx, Some(Duration::from_secs(20)));
        }
        assert!(fc.window() <= 1024 * 1024, "window {}", fc.window());
        assert!(fc.snap.decreases >= 1);
    }

    #[test]
    fn probe_rebaselines_after_long_queueing() {
        // Base RTT jumps from 50 ms to 400 ms (e.g. a reroute): the probe
        // must raise the estimate so the window doesn't collapse to the floor.
        let t0 = Instant::now();
        let mut fc = FlowController::new(t0);
        let mut rx = 0u64;
        for i in 1..=20u32 {
            rx += 1_000_000;
            fc.tick(t0 + TICK * i, rx, Some(Duration::from_millis(50)));
        }
        for i in 21..=400u32 {
            rx += 1_000_000;
            fc.tick(t0 + TICK * i, rx, Some(Duration::from_millis(400)));
        }
        assert!(fc.snap.probes >= 1);
        assert_eq!(fc.snap.base_rtt_ms, Some(400.0));
        // 5 MB/s x (0.4 + 0.1) s.
        assert!(fc.window() >= 2_000_000, "window {}", fc.window());
    }

    #[test]
    fn path_reset_forgets_estimates() {
        let t0 = Instant::now();
        let mut fc = FlowController::new(t0);
        let mut rx = 0;
        for i in 1..=10u32 {
            rx += 1_000_000;
            fc.tick(t0 + TICK * i, rx, Some(Duration::from_millis(150)));
        }
        fc.reset_path(t0 + TICK * 11);
        assert!(fc.snap.base_rtt_ms.is_some(), "snapshot keeps the last value for diagnostics");
        assert!(fc.window() <= INITIAL_WINDOW);
        assert_eq!(fc.snap.path_resets, 1);
    }
}
