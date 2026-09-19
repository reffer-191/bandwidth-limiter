//! Traffic shaping: deficit token buckets + per-class FIFO queues.
//!
//! Every packet must pass a *chain* of buckets (app → connection rule →
//! hotspot → PC/adapter). A bucket is "ready" when its token balance is
//! non-negative; sending a packet subtracts its size and may drive the balance
//! negative, which delays the next packet by exactly the time needed to pay
//! the debt. This gives an accurate average rate regardless of packet size
//! (64 KiB LSO segments included) while keeping bursts small.
//!
//! When several queues wait on the same bucket, the scheduler picks the next
//! packet with start-time fair queuing weighted by the app's priority, so a
//! "high" app gets most of a saturated PC limit and a "low" one the leftovers.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::windivert::Address;

use super::packet::Addr16;
use super::AppId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Down,
    Up,
}

pub struct Bucket {
    /// bytes per second
    pub rate: f64,
    tokens: f64,
    burst: f64,
    last: Instant,
}

impl Bucket {
    pub fn new(rate: u64) -> Self {
        let rate = rate.max(1) as f64;
        Self { rate, tokens: 0.0, burst: (rate * 0.05).max(3000.0), last: Instant::now() }
    }

    #[inline]
    fn refill(&mut self, now: Instant) {
        let dt = now.saturating_duration_since(self.last).as_secs_f64();
        if dt > 0.0 {
            self.tokens = (self.tokens + self.rate * dt).min(self.burst);
            self.last = now;
        }
    }

    #[inline]
    fn ready(&self) -> bool {
        self.tokens >= 0.0
    }

    #[inline]
    fn take(&mut self, n: usize) {
        self.tokens -= n as f64;
    }

    #[inline]
    fn wait(&self) -> Duration {
        if self.tokens >= 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64((-self.tokens) / self.rate)
        }
    }
}

/// Byte weights of the three priority levels (low, normal, high).
pub const PRIORITY_WEIGHT: [f64; 3] = [1.0, 4.0, 16.0];

pub struct Limits {
    pub down: Option<Bucket>,
    pub up: Option<Bucket>,
    pub block_down: bool,
    pub block_up: bool,
    /// 0 low, 1 normal, 2 high (apps only).
    pub priority: u8,
}

impl Default for Limits {
    fn default() -> Self {
        Self { down: None, up: None, block_down: false, block_up: false, priority: 1 }
    }
}

impl Limits {
    pub fn is_empty(&self) -> bool {
        self.down.is_none() && self.up.is_none() && !self.block_down && !self.block_up
    }
    fn bucket(&mut self, dir: Dir) -> Option<&mut Bucket> {
        match dir {
            Dir::Down => self.down.as_mut(),
            Dir::Up => self.up.as_mut(),
        }
    }
    fn bucket_ref(&self, dir: Dir) -> Option<&Bucket> {
        match dir {
            Dir::Down => self.down.as_ref(),
            Dir::Up => self.up.as_ref(),
        }
    }
    fn blocked(&self, dir: Dir) -> bool {
        match dir {
            Dir::Down => self.block_down,
            Dir::Up => self.block_up,
        }
    }
    fn refill(&mut self, now: Instant) {
        if let Some(b) = self.down.as_mut() { b.refill(now); }
        if let Some(b) = self.up.as_mut() { b.refill(now); }
    }
}

/// Compiled connection rule: which packets it applies to.
pub struct ConnMatcher {
    pub app: Option<AppId>,
    pub any_host: bool,
    /// Resolved networks (IPv4 mapped), prefix length.
    pub nets: Vec<(Addr16, u8)>,
    /// Inclusive port ranges; empty = any.
    pub ports: Vec<(u16, u16)>,
    /// 0 = any protocol.
    pub protocol: u8,
}

impl ConnMatcher {
    fn matches(&self, app: AppId, remote: &Addr16, port: u16, protocol: u8) -> bool {
        if let Some(a) = self.app {
            if a != app {
                return false;
            }
        }
        if self.protocol != 0 && self.protocol != protocol {
            return false;
        }
        if !self.ports.is_empty() && !self.ports.iter().any(|(lo, hi)| port >= *lo && port <= *hi) {
            return false;
        }
        self.any_host || self.nets.iter().any(|(net, plen)| super::adapters::prefix_matches(net, remote, *plen))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QueueKey {
    pub app: AppId,
    pub dir: Dir,
    pub forward: bool,
    pub internet: bool,
    /// Index + 1 into `Shaper::conns`; 0 = no connection rule.
    pub conn: u8,
    /// Index + 1 into `Shaper::adapters`; 0 = the PC-wide limit applies.
    pub adapter: u8,
}

pub struct QueuedPacket {
    pub data: Vec<u8>,
    pub addr: Address,
    pub forward: bool,
    /// Accounting metadata: bytes are counted when the packet is actually
    /// sent, so the chart shows what gets through the limiter.
    pub app: AppId,
    pub outbound: bool,
    pub internet: bool,
    /// Bytes charged to the buckets (payload, or the wire length when
    /// headers are counted).
    pub charge: usize,
    /// Belongs to a hotspot client (counts toward the hotspot totals).
    pub hotspot: bool,
}

#[derive(Default)]
struct Queue {
    packets: VecDeque<QueuedPacket>,
    bytes: usize,
    /// Virtual finish time of the head packet (start-time fair queuing).
    finish: f64,
}

pub enum Decision {
    Pass,
    Drop,
    Queued,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Which {
    App(AppId),
    Conn(u8),
    Hotspot,
    Adapter(u8),
    Global,
}

pub struct Shaper {
    pub master: bool,
    pub global: Limits,
    pub global_internet_only: bool,
    pub hotspot: Limits,
    pub apps: HashMap<AppId, Limits>,
    pub conns: Vec<Limits>,
    pub matchers: Vec<ConnMatcher>,
    pub adapters: Vec<Limits>,
    /// Interface index → adapter rule index + 1.
    pub if_map: HashMap<u32, u8>,
    queues: HashMap<QueueKey, Queue>,
    order: Vec<QueueKey>,
    vtime: f64,
    pub queued_bytes: usize,
    pub dropped: u64,
}

impl Default for Shaper {
    fn default() -> Self {
        Self {
            master: true,
            global: Limits::default(),
            global_internet_only: true,
            hotspot: Limits::default(),
            apps: HashMap::new(),
            conns: Vec::new(),
            matchers: Vec::new(),
            adapters: Vec::new(),
            if_map: HashMap::new(),
            queues: HashMap::new(),
            order: Vec::new(),
            vtime: 0.0,
            queued_bytes: 0,
            dropped: 0,
        }
    }
}

const MIN_QUEUE_BYTES: usize = 64 * 1024;
const MAX_QUEUE_BYTES: usize = 4 * 1024 * 1024;

impl Shaper {
    /// First connection rule matching a packet (index + 1), 0 when none.
    pub fn match_conn(&self, app: AppId, remote: &Addr16, port: u16, protocol: u8) -> u8 {
        for (i, m) in self.matchers.iter().enumerate() {
            if m.matches(app, remote, port, protocol) {
                return (i + 1) as u8;
            }
        }
        0
    }

    /// Adapter rule (index + 1) for an interface index, 0 = PC-wide limit.
    pub fn adapter_for(&self, if_idx: u32) -> u8 {
        self.if_map.get(&if_idx).copied().unwrap_or(0)
    }

    /// Which buckets apply to a class, in evaluation order.
    fn chain(&self, key: &QueueKey, out: &mut [Which; 4]) -> usize {
        let mut n = 0;
        if let Some(l) = self.apps.get(&key.app) {
            if l.bucket_ref(key.dir).is_some() {
                out[n] = Which::App(key.app);
                n += 1;
            }
        }
        if key.conn > 0 {
            if let Some(l) = self.conns.get(key.conn as usize - 1) {
                if l.bucket_ref(key.dir).is_some() {
                    out[n] = Which::Conn(key.conn);
                    n += 1;
                }
            }
        }
        if key.forward && self.hotspot.bucket_ref(key.dir).is_some() {
            out[n] = Which::Hotspot;
            n += 1;
        }
        if !self.global_internet_only || key.internet {
            if key.adapter > 0 {
                if let Some(l) = self.adapters.get(key.adapter as usize - 1) {
                    if l.bucket_ref(key.dir).is_some() {
                        out[n] = Which::Adapter(key.adapter);
                        n += 1;
                    }
                }
            } else if self.global.bucket_ref(key.dir).is_some() {
                out[n] = Which::Global;
                n += 1;
            }
        }
        n
    }

    fn limits_mut(&mut self, w: Which) -> &mut Limits {
        match w {
            Which::App(id) => self.apps.get_mut(&id).expect("app limits vanished"),
            Which::Conn(i) => &mut self.conns[i as usize - 1],
            Which::Hotspot => &mut self.hotspot,
            Which::Adapter(i) => &mut self.adapters[i as usize - 1],
            Which::Global => &mut self.global,
        }
    }

    fn limits_ref(&self, w: Which) -> &Limits {
        match w {
            Which::App(id) => &self.apps[&id],
            Which::Conn(i) => &self.conns[i as usize - 1],
            Which::Hotspot => &self.hotspot,
            Which::Adapter(i) => &self.adapters[i as usize - 1],
            Which::Global => &self.global,
        }
    }

    fn is_blocked(&self, key: &QueueKey) -> bool {
        if let Some(l) = self.apps.get(&key.app) {
            if l.blocked(key.dir) {
                return true;
            }
        }
        if key.conn > 0 && self.conns.get(key.conn as usize - 1).map(|l| l.blocked(key.dir)).unwrap_or(false) {
            return true;
        }
        if key.forward && self.hotspot.blocked(key.dir) {
            return true;
        }
        if !self.global_internet_only || key.internet {
            if key.adapter > 0 {
                return self.adapters.get(key.adapter as usize - 1).map(|l| l.blocked(key.dir)).unwrap_or(false);
            }
            return self.global.blocked(key.dir);
        }
        false
    }

    fn weight(&self, app: AppId) -> f64 {
        let p = self.apps.get(&app).map(|l| l.priority).unwrap_or(1) as usize;
        PRIORITY_WEIGHT[p.min(2)]
    }

    /// Fast path called from the receive threads. Either lets the packet go
    /// right now, drops it, or parks it in its class queue.
    pub fn admit(&mut self, key: QueueKey, len: usize, charge: usize, make: impl FnOnce() -> QueuedPacket) -> Decision {
        if !self.master {
            return Decision::Pass;
        }
        if self.is_blocked(&key) {
            self.dropped += 1;
            return Decision::Drop;
        }
        let mut chain = [Which::Global; 4];
        let n = self.chain(&key, &mut chain);
        if n == 0 {
            return Decision::Pass;
        }
        let chain = &chain[..n];
        let now = Instant::now();
        let queue_empty = self.queues.get(&key).map(|q| q.packets.is_empty()).unwrap_or(true);
        if queue_empty {
            let mut all_ready = true;
            for w in chain {
                let b = self.limits_mut(*w).bucket(key.dir).unwrap();
                b.refill(now);
                if !b.ready() {
                    all_ready = false;
                }
            }
            if all_ready {
                for w in chain {
                    self.limits_mut(*w).bucket(key.dir).unwrap().take(charge);
                }
                return Decision::Pass;
            }
        }
        // Slowest bucket in the chain bounds the queue to ~1 s of traffic.
        let min_rate = chain
            .iter()
            .map(|w| self.limits_ref(*w).bucket_ref(key.dir).unwrap().rate)
            .fold(f64::MAX, f64::min);
        let cap = (min_rate as usize).clamp(MIN_QUEUE_BYTES, MAX_QUEUE_BYTES);
        let weight = self.weight(key.app);
        let vtime = self.vtime;
        let q = self.queues.entry(key).or_default();
        if q.bytes + len > cap {
            self.dropped += 1;
            return Decision::Drop;
        }
        if q.packets.is_empty() {
            // Head packet: its virtual finish time decides its turn against
            // the other queues sharing a bucket.
            q.finish = vtime.max(q.finish) + len as f64 / weight;
            if !self.order.contains(&key) {
                self.order.push(key);
            }
        }
        q.packets.push_back(make());
        q.bytes += len;
        self.queued_bytes += len;
        Decision::Queued
    }

    /// Called by the scheduler thread. Returns packets that may be sent now and
    /// how long to wait before calling again.
    pub fn schedule(&mut self, out: &mut Vec<QueuedPacket>) -> Duration {
        let mut next_wait = Duration::from_millis(100);
        if self.order.is_empty() {
            return next_wait;
        }
        let now = Instant::now();
        for l in self.apps.values_mut() {
            l.refill(now);
        }
        for l in self.conns.iter_mut().chain(self.adapters.iter_mut()) {
            l.refill(now);
        }
        self.global.refill(now);
        self.hotspot.refill(now);

        let mut budget = 512; // packets per pass, keeps latency for the UI thread low
        let mut chain = [Which::Global; 4];
        loop {
            // Pick, among the queues whose whole chain is ready, the one with
            // the earliest virtual finish time.
            let mut best: Option<(usize, f64)> = None;
            let mut min_wait = Duration::MAX;
            for i in 0..self.order.len() {
                let key = self.order[i];
                let Some(q) = self.queues.get(&key) else { continue };
                if q.packets.is_empty() {
                    continue;
                }
                let n = self.chain(&key, &mut chain);
                if n == 0 {
                    // Rule removed while packets were waiting: flush.
                    while let Some(p) = self.pop(&key) {
                        out.push(p);
                    }
                    continue;
                }
                let mut wait = Duration::ZERO;
                for w in &chain[..n] {
                    wait = wait.max(self.limits_ref(*w).bucket_ref(key.dir).unwrap().wait());
                }
                if wait > Duration::ZERO {
                    min_wait = min_wait.min(wait);
                } else if best.map(|(_, f)| q.finish < f).unwrap_or(true) {
                    best = Some((i, q.finish));
                }
            }
            let Some((i, finish)) = best else {
                if min_wait != Duration::MAX {
                    next_wait = next_wait.min(min_wait);
                }
                break;
            };
            let key = self.order[i];
            let n = self.chain(&key, &mut chain);
            let charge = self.queues[&key].packets.front().map(|p| p.charge).unwrap_or(0);
            for w in &chain[..n] {
                self.limits_mut(*w).bucket(key.dir).unwrap().take(charge);
            }
            let weight = self.weight(key.app);
            let pkt = self.pop(&key).unwrap();
            self.vtime = finish;
            if let Some(q) = self.queues.get_mut(&key) {
                if let Some(next) = q.packets.front() {
                    q.finish = finish + next.data.len() as f64 / weight;
                }
            }
            out.push(pkt);
            budget -= 1;
            if budget == 0 {
                self.compact();
                return Duration::ZERO;
            }
        }
        self.compact();
        next_wait.max(Duration::from_micros(500))
    }

    fn compact(&mut self) {
        self.order.retain(|k| self.queues.get(k).map(|q| !q.packets.is_empty()).unwrap_or(false));
        // Idle queues keep their finish tag (bounded by vtime on the next
        // arrival), so dropping them costs nothing.
        self.queues.retain(|_, q| !q.packets.is_empty());
    }

    fn pop(&mut self, key: &QueueKey) -> Option<QueuedPacket> {
        let q = self.queues.get_mut(key)?;
        let p = q.packets.pop_front()?;
        q.bytes -= p.data.len();
        self.queued_bytes -= p.data.len();
        Some(p)
    }

    /// Drains everything (used when the engine stops so nothing is lost).
    pub fn drain_all(&mut self, out: &mut Vec<QueuedPacket>) {
        for (_, mut q) in self.queues.drain() {
            out.extend(q.packets.drain(..));
        }
        self.order.clear();
        self.queued_bytes = 0;
    }

    pub fn has_any_limit(&self) -> bool {
        self.master
            && (!self.global.is_empty()
                || !self.hotspot.is_empty()
                || self.apps.values().any(|l| !l.is_empty())
                || self.conns.iter().any(|l| !l.is_empty())
                || self.adapters.iter().any(|l| !l.is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(app: AppId) -> QueueKey {
        QueueKey { app, dir: Dir::Down, forward: false, internet: true, conn: 0, adapter: 0 }
    }

    fn pkt(len: usize) -> QueuedPacket {
        QueuedPacket { data: vec![0; len], addr: Address::default(), forward: false, app: 0, outbound: false, internet: true, charge: len, hotspot: false }
    }

    fn drain(s: &mut Shaper, out: &mut Vec<QueuedPacket>) {
        // Spin the scheduler as if time were passing.
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(400) {
            let w = s.schedule(out);
            if s.queued_bytes == 0 {
                break;
            }
            std::thread::sleep(w.min(Duration::from_millis(5)));
        }
    }

    #[test]
    fn global_bucket_paces_traffic() {
        let mut s = Shaper { global: Limits { down: Some(Bucket::new(100_000)), ..Default::default() }, ..Default::default() };
        let mut queued = 0;
        for _ in 0..40 {
            if let Decision::Queued = s.admit(key(1), 1500, 1500, || pkt(1500)) {
                queued += 1;
            }
        }
        assert!(queued > 30, "after the burst allowance packets must queue, got {queued}");
        let mut out = Vec::new();
        let t0 = Instant::now();
        drain(&mut s, &mut out);
        // 40 × 1500 B at 100 kB/s ≈ 0.6 s minus the 5 % burst; we only spin
        // for 0.4 s, so not everything is out yet.
        assert!(s.queued_bytes > 0 || t0.elapsed() >= Duration::from_millis(350));
    }

    #[test]
    fn priority_shares_a_saturated_bucket() {
        let mut s = Shaper { global: Limits { down: Some(Bucket::new(200_000)), ..Default::default() }, ..Default::default() };
        s.apps.insert(1, Limits { priority: 2, ..Default::default() }); // high
        s.apps.insert(2, Limits { priority: 0, ..Default::default() }); // low
        // Exhaust the burst allowance first so everything queues.
        s.admit(key(9), 20_000, 20_000, || pkt(20_000));
        for _ in 0..30 {
            s.admit(key(1), 1500, 1500, || pkt(1500));
            s.admit(key(2), 1500, 1500, || pkt(1500));
        }
        let mut out = Vec::new();
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(120) {
            let w = s.schedule(&mut out);
            std::thread::sleep(w.min(Duration::from_millis(2)));
        }
        // Count who got through (app 9's single packet aside). We cannot tell
        // app ids from packets, so check the remaining queue sizes instead.
        let left_high = s.queues.get(&key(1)).map(|q| q.packets.len()).unwrap_or(0);
        let left_low = s.queues.get(&key(2)).map(|q| q.packets.len()).unwrap_or(0);
        assert!(left_high < left_low, "high priority must drain first: high left {left_high}, low left {left_low}");
        assert!(left_high + left_low < 60, "something must have been sent");
    }

    #[test]
    fn blocks_and_connection_rules() {
        let mut s = Shaper::default();
        s.conns.push(Limits { block_down: true, ..Default::default() });
        s.matchers.push(ConnMatcher { app: None, any_host: false, nets: vec![(super::super::packet::map_ipv4(&[10, 0, 0, 0]), 8)], ports: vec![(443, 443)], protocol: 6 });
        let remote = super::super::packet::map_ipv4(&[10, 1, 2, 3]);
        assert_eq!(s.match_conn(1, &remote, 443, 6), 1);
        assert_eq!(s.match_conn(1, &remote, 80, 6), 0);
        assert_eq!(s.match_conn(1, &remote, 443, 17), 0);
        assert_eq!(s.match_conn(1, &super::super::packet::map_ipv4(&[11, 1, 2, 3]), 443, 6), 0);
        let mut k = key(1);
        k.conn = 1;
        assert!(matches!(s.admit(k, 100, 100, || pkt(100)), Decision::Drop));
        k.conn = 0;
        assert!(matches!(s.admit(k, 100, 100, || pkt(100)), Decision::Pass));
        // Adapter override replaces the PC limit (which is a block here).
        s.global.block_down = true;
        s.adapters.push(Limits::default());
        assert!(matches!(s.admit(k, 100, 100, || pkt(100)), Decision::Drop));
        k.adapter = 1;
        assert!(matches!(s.admit(k, 100, 100, || pkt(100)), Decision::Pass));
        // Master off passes everything.
        s.master = false;
        k.adapter = 0;
        assert!(matches!(s.admit(k, 100, 100, || pkt(100)), Decision::Pass));
    }

    #[test]
    fn chain_app_hotspot_global_and_flush() {
        // App bucket + hotspot bucket + global bucket all apply to a forwarded packet.
        let mut s = Shaper {
            global: Limits { down: Some(Bucket::new(1_000_000)), ..Default::default() },
            hotspot: Limits { down: Some(Bucket::new(1_000_000)), ..Default::default() },
            ..Default::default()
        };
        s.apps.insert(5, Limits { down: Some(Bucket::new(1_000)), ..Default::default() });
        let mut k = key(5);
        k.forward = true;
        let mut chain = [Which::Global; 4];
        assert_eq!(s.chain(&k, &mut chain), 3);
        assert_eq!(&chain[..3], &[Which::App(5), Which::Hotspot, Which::Global]);
        // The 1 kB/s app bucket is the bottleneck: the second packet queues.
        assert!(matches!(s.admit(k, 1500, 1500, || pkt(1500)), Decision::Pass));
        assert!(matches!(s.admit(k, 1500, 1500, || pkt(1500)), Decision::Queued));
        assert_eq!(s.queued_bytes, 1500);
        // Upload direction has no buckets at all → passes untouched.
        let mut up = k;
        up.dir = Dir::Up;
        assert!(matches!(s.admit(up, 1500, 1500, || pkt(1500)), Decision::Pass));
        // Removing every rule flushes what was waiting on the next pass.
        s.apps.clear();
        s.global.down = None;
        s.hotspot.down = None;
        let mut out = Vec::new();
        s.schedule(&mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(s.queued_bytes, 0);
        assert!(!s.has_any_limit());
    }

    #[test]
    fn payload_charge_is_what_the_bucket_pays() {
        let mut s = Shaper { global: Limits { down: Some(Bucket::new(10_000)), ..Default::default() }, ..Default::default() };
        // A 1500 B packet whose payload is 1000 B costs the bucket 1000 tokens.
        assert!(matches!(s.admit(key(1), 1500, 1000, || pkt(1500)), Decision::Pass));
        let tokens = s.global.down.as_ref().unwrap().tokens;
        assert!(tokens <= -999.0 && tokens > -1001.0, "tokens {tokens}");
        assert!(matches!(s.admit(key(1), 1500, 1000, || pkt(1500)), Decision::Queued));
    }
}
