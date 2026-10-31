//! Multi-connection transfers, modelled on MMCFTP (Massively Multi-Connection
//! File Transfer Protocol, NII Japan): split the data, send it over several
//! connections at once, balance the load across them, and adjust the number
//! of connections to the measured network conditions.
//!
//! Why it helps: every QUIC connection has its own congestion controller.
//! On a long path with random loss, one loss-based controller (Cubic) is
//! limited to roughly `MSS / (RTT * sqrt(loss))` whatever the link capacity
//! (about 90 KB/s at 200 ms and 1 % loss). N connections each get that
//! share, so N of them carry about N times as much, and a loss slows down
//! only 1/N of the transfer.
//!
//! What Relayless does (receiver side only; senders need no change and
//! older senders work):
//! - A per-sender pool of 1..=`max_connections` QUIC connections ("lanes").
//!   Lane 0 is the normal connection (Hello, Stat, receipts); extra lanes
//!   carry chunk streams only. The sender authorizes every request by device
//!   key, so extra lanes need no handshake beyond QUIC.
//! - Chunk workers are spread over the lanes (`worker % lanes`). When the
//!   lane set changes, a worker finishes its current 1 MiB block, stops the
//!   stream and resumes the same chunk on its new lane from that block.
//! - A hill-climbing controller decides the lane count from measured
//!   goodput (receiver side, where the data arrives): measure the current
//!   level, add a lane, and keep it only if it brought at least half a fair
//!   share of extra goodput (`gain_to_add`); otherwise go back and hold. After a hold it re-measures
//!   and tries one lane fewer, keeping it if goodput stays within
//!   `KEEP_FEWER` of the higher level. MMCFTP uses RTT and loss; the
//!   receiver can't see data-packet loss directly, but goodput is what both
//!   are proxies for.
//! - Extra lanes are used only while the path is not a relay: relays rate
//!   limit per device, so more connections only add queueing there.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::endpoint::{Connection, VarInt};
use parking_lot::Mutex;

/// Largest pool size the setting allows.
pub const MAX_LANES: u32 = 8;
/// Chunk workers added for each extra lane, so a new lane carries traffic
/// right away instead of waiting for running chunks to finish.
pub const WORKERS_PER_EXTRA_LANE: u32 = 4;

/// Chunk workers for a download pass with `lanes` usable lanes.
pub fn target_workers(parallel_streams: u32, lanes: u32) -> u32 {
    parallel_streams.max(1) + WORKERS_PER_EXTRA_LANE * lanes.saturating_sub(1)
}
/// An added lane is kept if it brings at least half of a fair share:
/// going from `n` to `n + 1` lanes must raise goodput by `0.5 / n`
/// (50 % for the second lane, 25 % for the third, ... 7.1 % for the eighth),
/// and never less than `MIN_GAIN`, which stays above measurement noise.
pub fn gain_to_add(n: u32) -> f64 {
    (0.5 / n.max(1) as f64).max(MIN_GAIN)
}
pub const MIN_GAIN: f64 = 0.07;
/// One lane fewer is kept if goodput stays within this fraction.
pub const KEEP_FEWER: f64 = 0.05;
/// How often the controller runs.
pub const TICK: Duration = Duration::from_millis(500);

/// Controller timings; shortened in tests through `RELAYLESS_MMC_FAST=1`.
#[derive(Debug, Clone, Copy)]
pub struct Timings {
    /// Wait after a lane change before measuring (new congestion windows ramp up).
    pub settle: Duration,
    /// Length of one goodput measurement.
    pub measure: Duration,
    /// Time at a level after a failed probe, before probing again.
    pub hold: Duration,
    /// Extra lanes are closed after this long without chunk traffic.
    pub idle_close: Duration,
}

impl Timings {
    pub fn get() -> Self {
        if std::env::var("RELAYLESS_MMC_FAST").ok().as_deref() == Some("1") {
            Timings {
                settle: Duration::from_millis(1500),
                measure: Duration::from_secs(3),
                hold: Duration::from_secs(10),
                idle_close: Duration::from_secs(10),
            }
        } else {
            Timings {
                settle: Duration::from_secs(4),
                measure: Duration::from_secs(8),
                hold: Duration::from_secs(60),
                idle_close: Duration::from_secs(30),
            }
        }
    }
}

// ====================================================================== pool

/// One connection in a sender's pool.
#[derive(Debug)]
pub struct Lane {
    pub id: u32,
    pub conn: Connection,
    /// Chunk payload bytes read on this lane (drives its flow window).
    pub rx: Arc<AtomicU64>,
    /// Chunk streams currently open on this lane.
    pub streams: AtomicU64,
    /// No new chunks; closed once its streams finish.
    pub retired: AtomicBool,
    /// Set when the lane carries no new work because its path is a relay.
    pub relayed: AtomicBool,
    /// The pool's `active_rx` counter.
    active_rx: Arc<AtomicU64>,
}

impl Lane {
    /// Counts `n` payload bytes read on this lane.
    pub fn count_rx(&self, n: u64) {
        self.rx.fetch_add(n, Ordering::Relaxed);
        if !self.retired.load(Ordering::Relaxed) && (self.is_primary() || !self.relayed.load(Ordering::Relaxed)) {
            self.active_rx.fetch_add(n, Ordering::Relaxed);
        }
    }
    pub fn is_primary(&self) -> bool {
        self.id == 0
    }
    pub fn alive(&self) -> bool {
        self.conn.close_reason().is_none()
    }
    /// Usable for new chunk streams.
    pub fn usable(&self) -> bool {
        self.alive() && !self.retired.load(Ordering::Relaxed) && (self.is_primary() || !self.relayed.load(Ordering::Relaxed))
    }
}

/// Decrements a lane's stream count when a chunk stream ends.
pub struct LaneStream(Arc<Lane>);

impl LaneStream {
    pub fn new(lane: &Arc<Lane>) -> Self {
        lane.streams.fetch_add(1, Ordering::Relaxed);
        LaneStream(lane.clone())
    }
}

impl Drop for LaneStream {
    fn drop(&mut self) {
        self.0.streams.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct MmcSnapshot {
    pub max_lanes: u32,
    pub lanes: u32,
    pub lanes_peak: u32,
    pub adds: u64,
    pub add_failures: u64,
    pub removes: u64,
    pub lane_moves: u64,
    pub lane_failures: u64,
    /// Goodput measured at each lane count (bytes/s), latest value per count.
    pub goodput_by_lanes: Vec<(u32, f64)>,
    pub last_decision: Option<String>,
}

/// The lanes to one sender.
#[derive(Debug, Default)]
pub struct Pool {
    lanes: Mutex<Vec<Arc<Lane>>>,
    next_id: AtomicU32,
    pub snap: Mutex<MmcSnapshot>,
    /// Payload bytes read on lanes that take new work. Retired lanes keep
    /// draining their streams for a while (a 1 MiB block can take minutes on
    /// a slow lane); leaving them out keeps the controller's measurement of
    /// "n lanes" honest after a removal.
    active_rx: Arc<AtomicU64>,
}

impl Pool {
    /// Installs a new primary connection; closes every lane of the old one.
    pub fn set_primary(&self, conn: &Connection) -> Arc<Lane> {
        let lane = Arc::new(Lane {
            id: 0,
            conn: conn.clone(),
            rx: Arc::new(AtomicU64::new(0)),
            streams: AtomicU64::new(0),
            retired: AtomicBool::new(false),
            relayed: AtomicBool::new(false),
            active_rx: self.active_rx.clone(),
        });
        let old: Vec<Arc<Lane>> = std::mem::replace(&mut *self.lanes.lock(), vec![lane.clone()]);
        for l in old {
            if !l.is_primary() {
                l.conn.close(VarInt::from_u32(0), b"lane closed");
            }
        }
        self.next_id.store(1, Ordering::Relaxed);
        self.update_count();
        lane
    }



    pub fn add(&self, conn: Connection) -> Arc<Lane> {
        let lane = Arc::new(Lane {
            id: self.next_id.fetch_add(1, Ordering::Relaxed).max(1),
            conn,
            rx: Arc::new(AtomicU64::new(0)),
            streams: AtomicU64::new(0),
            retired: AtomicBool::new(false),
            relayed: AtomicBool::new(false),
            active_rx: self.active_rx.clone(),
        });
        self.lanes.lock().push(lane.clone());
        self.snap.lock().adds += 1;
        self.update_count();
        lane
    }

    /// Lanes that take new chunk streams, primary first.
    pub fn usable(&self) -> Vec<Arc<Lane>> {
        let mut v: Vec<Arc<Lane>> = self.lanes.lock().iter().filter(|l| l.usable()).cloned().collect();
        v.sort_by_key(|l| l.id);
        v
    }

    /// Extra lanes that are open and not retired (relayed or not).
    pub fn extra_lanes(&self) -> Vec<Arc<Lane>> {
        self.lanes
            .lock()
            .iter()
            .filter(|l| !l.is_primary() && l.alive() && !l.retired.load(Ordering::Relaxed))
            .cloned()
            .collect()
    }

    pub fn primary(&self) -> Option<Arc<Lane>> {
        self.lanes.lock().iter().find(|l| l.is_primary()).cloned()
    }

    /// Lane for the next chunk: the usable lane with the fewest open streams
    /// (lowest id on ties), so a new lane fills up first.
    pub fn pick(&self) -> Option<Arc<Lane>> {
        self.usable().into_iter().min_by_key(|l| (l.streams.load(Ordering::Relaxed), l.id))
    }

    /// A stream on `lane` should move: the lane no longer takes work, or it
    /// has at least two more streams than the least-loaded usable lane.
    pub fn should_move(&self, lane: &Lane) -> bool {
        let Some(best) = self.pick() else { return false };
        if best.id == lane.id {
            return false;
        }
        !lane.usable() || lane.streams.load(Ordering::Relaxed) >= best.streams.load(Ordering::Relaxed) + 2
    }

    /// Number of lanes taking new work.
    pub fn count(&self) -> u32 {
        self.usable().len() as u32
    }

    /// Retires the newest extra lane. Returns false if only the primary is left.
    pub fn retire_one(&self) -> bool {
        let lane = self.usable().into_iter().filter(|l| !l.is_primary()).max_by_key(|l| l.id);
        match lane {
            Some(l) => {
                l.retired.store(true, Ordering::Relaxed);
                self.snap.lock().removes += 1;
                self.update_count();
                true
            }
            None => false,
        }
    }

    pub fn retire_all_extra(&self) {
        while self.retire_one() {}
    }

    /// A lane failed mid-transfer: stop using it.
    pub fn lane_failed(&self, lane: &Lane) {
        if !lane.is_primary() && !lane.retired.swap(true, Ordering::Relaxed) {
            self.snap.lock().lane_failures += 1;
            self.update_count();
        }
    }

    /// Closes retired lanes whose streams have finished and forgets dead ones.
    pub fn reap(&self) {
        let mut lanes = self.lanes.lock();
        lanes.retain(|l| {
            if l.is_primary() {
                return true;
            }
            if !l.alive() {
                return false;
            }
            if l.retired.load(Ordering::Relaxed) && l.streams.load(Ordering::Relaxed) == 0 {
                l.conn.close(VarInt::from_u32(0), b"lane retired");
                return false;
            }
            true
        });
    }

    pub fn close_all_extra(&self) {
        let mut lanes = self.lanes.lock();
        for l in lanes.iter().filter(|l| !l.is_primary()) {
            l.conn.close(VarInt::from_u32(0), b"lane closed");
        }
        lanes.retain(|l| l.is_primary());
        drop(lanes);
        self.update_count();
    }

    /// Cumulative bytes read on lanes that were taking new work at the time.
    pub fn active_rx(&self) -> u64 {
        self.active_rx.load(Ordering::Relaxed)
    }

    pub fn note_move(&self) {
        self.snap.lock().lane_moves += 1;
    }

    fn update_count(&self) {
        let n = self.count();
        let mut s = self.snap.lock();
        s.lanes = n;
        s.lanes_peak = s.lanes_peak.max(n);
    }
}

// ====================================================================== controller

/// What the controller wants the pool to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Add,
    Remove,
    /// Back to one lane (relay path, idle, or the feature is off).
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// First measurement at the current level.
    Base,
    /// Measuring after adding a lane; `prev` is the goodput one level down.
    /// `second`: this is the confirming measurement of a borderline result.
    Up { prev: f64, second: bool },
    /// Measuring after removing a lane; `prev` is the goodput one level up.
    Down { prev: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Settle { until: Instant, kind: Kind },
    Measure { start: Instant, bytes0: u64, kind: Kind },
    Hold { until: Instant },
}

/// Inputs for one controller step.
#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    pub now: Instant,
    /// Cumulative chunk payload bytes read from this sender on lanes that
    /// take new work (`Pool::active_rx`).
    pub rx_total: u64,
    /// Lanes currently taking new work.
    pub lanes: u32,
    /// Setting (1 = off).
    pub max_lanes: u32,
    /// At least two chunk streams are open (a download is running).
    pub busy: bool,
    /// Extra lanes may help: path is not a relay and nothing else (rate
    /// limit) caps the transfer.
    pub eligible: bool,
}

#[derive(Debug)]
pub struct Controller {
    t: Timings,
    phase: Phase,
    idle_since: Option<Instant>,
    /// Level at which the last probe up failed: the next probe from there
    /// goes down instead, so the controller can also shed lanes.
    up_failed_at: Option<u32>,
    /// Consecutive probes up that didn't pay off.
    fail_streak: u32,
    /// Latest goodput per lane count, for diagnostics.
    pub history: Vec<(u32, f64)>,
    pub last_decision: Option<String>,
}

impl Controller {
    pub fn new(now: Instant, t: Timings) -> Self {
        Controller {
            t,
            phase: Phase::Settle { until: now + t.settle, kind: Kind::Base },
            idle_since: None,
            up_failed_at: None,
            fail_streak: 0,
            history: Vec::new(),
            last_decision: None,
        }
    }

    fn settle(&mut self, now: Instant, kind: Kind) {
        self.phase = Phase::Settle { until: now + self.t.settle, kind };
    }

    fn record(&mut self, lanes: u32, rate: f64) {
        match self.history.iter_mut().find(|(n, _)| *n == lanes) {
            Some(e) => e.1 = rate,
            None => self.history.push((lanes, rate)),
        }
        self.history.sort_by_key(|(n, _)| *n);
    }

    fn decide(&mut self, s: String) {
        self.last_decision = Some(s);
    }

    /// One step. The caller applies the returned action and reports the new
    /// lane count on the next call.
    pub fn tick(&mut self, i: Inputs) -> Action {
        let now = i.now;
        if i.max_lanes <= 1 || !i.eligible {
            self.settle(now, Kind::Base);
            if i.lanes > 1 {
                self.decide(format!("collapse to 1 lane ({})", if i.max_lanes <= 1 { "multi-connection off" } else { "relay path or capped transfer" }));
                return Action::Collapse;
            }
            return Action::None;
        }
        if !i.busy {
            let since = *self.idle_since.get_or_insert(now);
            // Measurements need traffic: restart them when data flows again.
            self.settle(now, Kind::Base);
            if i.lanes > 1 && now.duration_since(since) >= self.t.idle_close {
                self.decide("collapse to 1 lane (idle)".into());
                return Action::Collapse;
            }
            return Action::None;
        }
        self.idle_since = None;
        match self.phase {
            Phase::Settle { until, kind } => {
                if now >= until {
                    self.phase = Phase::Measure { start: now, bytes0: i.rx_total, kind };
                }
                Action::None
            }
            Phase::Hold { until } => {
                if now >= until {
                    self.settle(now, Kind::Base);
                }
                Action::None
            }
            Phase::Measure { start, bytes0, kind } => {
                let dt = now.duration_since(start);
                if dt < self.t.measure {
                    return Action::None;
                }
                let rate = i.rx_total.saturating_sub(bytes0) as f64 / dt.as_secs_f64();
                if rate <= 0.0 {
                    // Streams open but nothing arrived (sender-side limit,
                    // stall): no information. Measure again, change nothing.
                    self.phase = Phase::Measure { start: now, bytes0: i.rx_total, kind };
                    return Action::None;
                }
                self.record(i.lanes, rate);
                let n = i.lanes;
                match kind {
                    Kind::Base => {
                        let try_up = n < i.max_lanes && (self.up_failed_at != Some(n) || n == 1);
                        if try_up {
                            self.decide(format!("probe up: {n} lanes at {:.0} KB/s, trying {}", rate / 1e3, n + 1));
                            self.settle(now, Kind::Up { prev: rate, second: false });
                            Action::Add
                        } else if n > 1 {
                            self.decide(format!("probe down: {n} lanes at {:.0} KB/s, trying {}", rate / 1e3, n - 1));
                            self.settle(now, Kind::Down { prev: rate });
                            Action::Remove
                        } else {
                            self.phase = Phase::Hold { until: now + self.t.hold };
                            Action::None
                        }
                    }
                    Kind::Up { prev, second } => {
                        let need = gain_to_add(n.saturating_sub(1));
                        if !second && rate < prev * (1.0 + need) && rate >= prev * (1.0 + need / 2.0) {
                            // Borderline: the new connection may still be
                            // ramping up. Measure once more before deciding.
                            self.decide(format!(
                                "measure {n} lanes again: {:.0} KB/s is {:.0}% more than {:.0} KB/s, need {:.0}%",
                                rate / 1e3,
                                (rate / prev.max(1.0) - 1.0) * 100.0,
                                prev / 1e3,
                                need * 100.0
                            ));
                            self.phase = Phase::Measure { start: now, bytes0: i.rx_total, kind: Kind::Up { prev, second: true } };
                            return Action::None;
                        }
                        if rate >= prev * (1.0 + need) {
                            self.fail_streak = 0;
                            if n < i.max_lanes {
                                self.decide(format!("keep {n} lanes ({:.0} vs {:.0} KB/s), trying {}", rate / 1e3, prev / 1e3, n + 1));
                                self.settle(now, Kind::Up { prev: rate, second: false });
                                Action::Add
                            } else {
                                self.decide(format!("keep {n} lanes ({:.0} vs {:.0} KB/s), at the maximum", rate / 1e3, prev / 1e3));
                                self.phase = Phase::Hold { until: now + self.t.hold };
                                Action::None
                            }
                        } else {
                            self.decide(format!(
                                "drop lane {n}: {:.0} KB/s is under {:.0}% more than {:.0} KB/s",
                                rate / 1e3,
                                need * 100.0,
                                prev / 1e3
                            ));
                            self.up_failed_at = Some(n - 1);
                            // Back off repeated useless probes: hold x1, x2, x4, x8.
                            self.fail_streak = (self.fail_streak + 1).min(4);
                            let hold = self.t.hold * (1 << (self.fail_streak - 1));
                            self.phase = Phase::Hold { until: now + hold };
                            Action::Remove
                        }
                    }
                    Kind::Down { prev } => {
                        self.up_failed_at = None;
                        if rate >= prev * (1.0 - KEEP_FEWER) {
                            if n > 1 {
                                // As good with fewer lanes: keep going down.
                                self.decide(format!("keep {n} lanes ({:.0} vs {:.0} KB/s with one more), trying {}", rate / 1e3, prev / 1e3, n - 1));
                                self.settle(now, Kind::Down { prev: rate.max(prev) });
                                return Action::Remove;
                            }
                            self.decide(format!("keep 1 lane ({:.0} vs {:.0} KB/s with two)", rate / 1e3, prev / 1e3));
                            self.phase = Phase::Hold { until: now + self.t.hold };
                            Action::None
                        } else {
                            self.decide(format!("restore lane {}: {:.0} KB/s fell below {:.0} KB/s", n + 1, rate / 1e3, prev / 1e3));
                            self.phase = Phase::Hold { until: now + self.t.hold };
                            Action::Add
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Timings {
        Timings {
            settle: Duration::from_secs(4),
            measure: Duration::from_secs(8),
            hold: Duration::from_secs(60),
            idle_close: Duration::from_secs(30),
        }
    }

    /// Runs the controller against a model `goodput(lanes)` for `secs`.
    fn run(goodput: impl Fn(u32) -> f64, max: u32, secs: u64, eligible: bool) -> (Controller, u32, Vec<u32>) {
        let t0 = Instant::now();
        let mut c = Controller::new(t0, t());
        let mut lanes = 1u32;
        let mut rx = 0f64;
        let mut trace = Vec::new();
        for k in 1..=(secs * 1000 / TICK.as_millis() as u64) {
            let now = t0 + TICK * k as u32;
            rx += goodput(lanes) * TICK.as_secs_f64();
            let a = c.tick(Inputs { now, rx_total: rx as u64, lanes, max_lanes: max, busy: true, eligible });
            match a {
                Action::Add => lanes += 1,
                Action::Remove => lanes = lanes.saturating_sub(1).max(1),
                Action::Collapse => lanes = 1,
                Action::None => {}
            }
            trace.push(lanes);
        }
        (c, lanes, trace)
    }

    #[test]
    fn lossy_path_grows_to_the_maximum() {
        // Loss-limited: each connection adds ~90 KB/s until the 100 MB/s link.
        let (_, lanes, _) = run(|n| (n as f64 * 90_000.0).min(100e6), 8, 300, true);
        assert_eq!(lanes, 8);
    }

    #[test]
    fn stops_where_the_link_saturates() {
        // Each lane adds 1 MB/s until the 3 MB/s link is full.
        let (c, lanes, _) = run(|n| (n as f64 * 1e6).min(3e6), 8, 400, true);
        assert_eq!(lanes, 3, "history {:?}", c.history);
    }

    #[test]
    fn clean_link_stays_at_one_lane() {
        // A single connection already fills the link.
        let (c, lanes, trace) = run(|_| 50e6, 8, 300, true);
        assert_eq!(lanes, 1);
        // It probed (2 lanes for one measurement) and went back.
        assert!(trace.iter().any(|&n| n == 2), "never probed");
        assert!(c.last_decision.unwrap().contains("drop lane"));
    }

    #[test]
    fn falls_back_when_more_lanes_hurt() {
        // Extra lanes make things worse (e.g. a shaped link that punishes bursts).
        let (_, lanes, _) = run(|n| 2e6 / n as f64, 8, 300, true);
        assert_eq!(lanes, 1);
    }

    #[test]
    fn sheds_lanes_after_conditions_improve() {
        // Starts loss-limited, then (after 200 s) one lane fills the link.
        let t0 = Instant::now();
        let mut c = Controller::new(t0, t());
        let mut lanes = 1u32;
        let mut rx = 0f64;
        for k in 1..=(900 * 2u32) {
            let now = t0 + TICK * k;
            let secs = k as f64 * TICK.as_secs_f64();
            let g = if secs < 200.0 { (lanes as f64 * 90_000.0).min(1e8) } else { 20e6 };
            rx += g * TICK.as_secs_f64();
            match c.tick(Inputs { now, rx_total: rx as u64, lanes, max_lanes: 8, busy: true, eligible: true }) {
                Action::Add => lanes += 1,
                Action::Remove => lanes = lanes.saturating_sub(1).max(1),
                Action::Collapse => lanes = 1,
                Action::None => {}
            }
            if secs.round() as u64 == 199 {
                assert_eq!(lanes, 8, "grew while loss-limited");
            }
        }
        assert!(lanes <= 2, "kept {lanes} lanes on a clean link");
    }

    #[test]
    fn relay_or_off_collapses_to_one() {
        let t0 = Instant::now();
        let mut c = Controller::new(t0, t());
        let base = Inputs { now: t0, rx_total: 0, lanes: 4, max_lanes: 8, busy: true, eligible: false };
        assert_eq!(c.tick(base), Action::Collapse);
        assert_eq!(c.tick(Inputs { lanes: 1, ..base }), Action::None);
        assert_eq!(c.tick(Inputs { eligible: true, max_lanes: 1, ..base }), Action::Collapse);
    }

    #[test]
    fn idle_closes_extra_lanes_after_a_while() {
        let t0 = Instant::now();
        let mut c = Controller::new(t0, t());
        let i = Inputs { now: t0, rx_total: 0, lanes: 3, max_lanes: 8, busy: false, eligible: true };
        assert_eq!(c.tick(i), Action::None);
        assert_eq!(c.tick(Inputs { now: t0 + Duration::from_secs(10), ..i }), Action::None);
        assert_eq!(c.tick(Inputs { now: t0 + Duration::from_secs(31), ..i }), Action::Collapse);
    }

    #[test]
    fn gain_threshold_is_half_a_fair_share() {
        assert_eq!(gain_to_add(1), 0.5);
        assert_eq!(gain_to_add(2), 0.25);
        assert!((gain_to_add(7) - 0.5 / 7.0).abs() < 1e-9);
        assert_eq!(gain_to_add(20), MIN_GAIN);
    }

    #[test]
    fn sublinear_gains_stop_early() {
        // Each extra lane adds less (shared bottleneck partly reached):
        // 1 MB/s, 1.6, 1.9, 2.0, ... The third lane adds 19 % (< 25 %).
        let g = |n: u32| [0.0, 1.0, 1.6, 1.9, 2.0, 2.05, 2.08, 2.1, 2.1][n as usize] * 1e6;
        let (c, lanes, _) = run(g, 8, 300, true);
        assert_eq!(lanes, 2, "history {:?}", c.history);
    }

    #[test]
    fn slow_ramp_is_measured_again() {
        // Loss-limited, but a new connection gives only 30 % of its share for
        // its first 12 s (slow start under loss). The first measurement after
        // an add is borderline; the second one sees the full gain.
        let t0 = Instant::now();
        let mut c = Controller::new(t0, t());
        let mut lanes = 1u32;
        let mut added_at = t0;
        let mut rx = 0f64;
        for k in 1..=(200 * 2u32) {
            let now = t0 + TICK * k;
            let ramping = lanes > 1 && now.duration_since(added_at) < Duration::from_secs(12);
            let g = if ramping { (lanes as f64 - 0.7) * 90_000.0 } else { lanes as f64 * 90_000.0 };
            rx += g * TICK.as_secs_f64();
            match c.tick(Inputs { now, rx_total: rx as u64, lanes, max_lanes: 8, busy: true, eligible: true }) {
                Action::Add => {
                    lanes += 1;
                    added_at = now;
                }
                Action::Remove => lanes = lanes.saturating_sub(1).max(1),
                Action::Collapse => lanes = 1,
                Action::None => {}
            }
        }
        assert_eq!(lanes, 8, "history {:?}, last {:?}", c.history, c.last_decision);
    }

    #[test]
    fn no_data_changes_nothing() {
        // Streams open but nothing arrives (e.g. a sender upload limit that
        // releases a block every few seconds): no lanes are added on 0 vs 0.
        let (_, lanes, trace) = run(|_| 0.0, 8, 300, true);
        assert_eq!(lanes, 1);
        assert!(trace.iter().all(|&n| n == 1));
    }

    #[test]
    fn one_lane_setting_never_adds() {
        let (_, lanes, trace) = run(|n| n as f64 * 90_000.0, 1, 300, true);
        assert_eq!(lanes, 1);
        assert!(trace.iter().all(|&n| n == 1));
    }
}
