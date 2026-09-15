//! Traffic shaping: deficit token buckets + per-class FIFO queues.
//!
//! Every packet must pass a *chain* of buckets (app → hotspot → global). A
//! bucket is "ready" when its token balance is non-negative; sending a packet
//! subtracts its size and may drive the balance negative, which delays the
//! next packet by exactly the time needed to pay the debt. This gives an
//! accurate average rate regardless of packet size (64 KiB LSO segments
//! included) while keeping bursts small.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::windivert::Address;

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

#[derive(Default)]
pub struct Limits {
    pub down: Option<Bucket>,
    pub up: Option<Bucket>,
    pub block_down: bool,
    pub block_up: bool,
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
    fn blocked(&self, dir: Dir) -> bool {
        match dir {
            Dir::Down => self.block_down,
            Dir::Up => self.block_up,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QueueKey {
    pub app: AppId,
    pub dir: Dir,
    pub forward: bool,
    pub internet: bool,
}

pub struct QueuedPacket {
    pub data: Vec<u8>,
    pub addr: Address,
    pub forward: bool,
}

#[derive(Default)]
struct Queue {
    packets: VecDeque<QueuedPacket>,
    bytes: usize,
}

pub enum Decision {
    Pass,
    Drop,
    Queued,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Which {
    App(AppId),
    Hotspot,
    Global,
}

pub struct Shaper {
    pub master: bool,
    pub global: Limits,
    pub global_internet_only: bool,
    pub hotspot: Limits,
    pub apps: HashMap<AppId, Limits>,
    queues: HashMap<QueueKey, Queue>,
    order: Vec<QueueKey>,
    rr: usize,
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
            queues: HashMap::new(),
            order: Vec::new(),
            rr: 0,
            queued_bytes: 0,
            dropped: 0,
        }
    }
}

const MIN_QUEUE_BYTES: usize = 64 * 1024;
const MAX_QUEUE_BYTES: usize = 4 * 1024 * 1024;

impl Shaper {
    /// Which buckets apply to a class, in evaluation order.
    fn chain(&self, key: &QueueKey) -> Vec<Which> {
        let mut c = Vec::with_capacity(3);
        if let Some(l) = self.apps.get(&key.app) {
            if l.bucket_ref(key.dir).is_some() {
                c.push(Which::App(key.app));
            }
        }
        if key.forward && self.hotspot.bucket_ref(key.dir).is_some() {
            c.push(Which::Hotspot);
        }
        if (!self.global_internet_only || key.internet) && self.global.bucket_ref(key.dir).is_some() {
            c.push(Which::Global);
        }
        c
    }

    fn limits_mut(&mut self, w: Which) -> &mut Limits {
        match w {
            Which::App(id) => self.apps.get_mut(&id).expect("app limits vanished"),
            Which::Hotspot => &mut self.hotspot,
            Which::Global => &mut self.global,
        }
    }

    fn is_blocked(&self, key: &QueueKey) -> bool {
        if let Some(l) = self.apps.get(&key.app) {
            if l.blocked(key.dir) {
                return true;
            }
        }
        if key.forward && self.hotspot.blocked(key.dir) {
            return true;
        }
        (!self.global_internet_only || key.internet) && self.global.blocked(key.dir)
    }

    /// Fast path called from the receive threads. Either lets the packet go
    /// right now, drops it, or parks it in its class queue.
    pub fn admit(&mut self, key: QueueKey, len: usize, make: impl FnOnce() -> QueuedPacket) -> Decision {
        if !self.master {
            return Decision::Pass;
        }
        if self.is_blocked(&key) {
            self.dropped += 1;
            return Decision::Drop;
        }
        let chain = self.chain(&key);
        if chain.is_empty() {
            return Decision::Pass;
        }
        let now = Instant::now();
        let queue_empty = self.queues.get(&key).map(|q| q.packets.is_empty()).unwrap_or(true);
        if queue_empty {
            let mut all_ready = true;
            for w in &chain {
                let b = self.limits_mut(*w).bucket(key.dir).unwrap();
                b.refill(now);
                if !b.ready() {
                    all_ready = false;
                }
            }
            if all_ready {
                for w in &chain {
                    self.limits_mut(*w).bucket(key.dir).unwrap().take(len);
                }
                return Decision::Pass;
            }
        }
        // Slowest bucket in the chain bounds the queue to ~1 s of traffic.
        let min_rate = chain
            .iter()
            .map(|w| match w {
                Which::App(id) => self.apps[id].bucket_ref(key.dir).unwrap().rate,
                Which::Hotspot => self.hotspot.bucket_ref(key.dir).unwrap().rate,
                Which::Global => self.global.bucket_ref(key.dir).unwrap().rate,
            })
            .fold(f64::MAX, f64::min);
        let cap = (min_rate as usize).clamp(MIN_QUEUE_BYTES, MAX_QUEUE_BYTES);
        let q = self.queues.entry(key).or_default();
        if q.bytes + len > cap {
            self.dropped += 1;
            return Decision::Drop;
        }
        if q.packets.is_empty() && !self.order.contains(&key) {
            self.order.push(key);
        }
        q.packets.push_back(make());
        q.bytes += len;
        self.queued_bytes += len;
        Decision::Queued
    }

    /// Called by the scheduler thread. Returns packets that may be sent now and
    /// how long to wait before calling again.
    pub fn schedule(&mut self, out: &mut Vec<QueuedPacket>) -> Duration {
        let now = Instant::now();
        let mut next_wait = Duration::from_millis(100);
        if self.order.is_empty() {
            return next_wait;
        }
        for l in self.apps.values_mut().chain([&mut self.global, &mut self.hotspot]) {
            if let Some(b) = l.down.as_mut() { b.refill(now); }
            if let Some(b) = l.up.as_mut() { b.refill(now); }
        }
        let n = self.order.len();
        self.rr = (self.rr + 1) % n.max(1);
        let mut budget = 512; // packets per pass, keeps latency for the UI thread low
        for i in 0..n {
            let key = self.order[(self.rr + i) % n];
            let chain = self.chain(&key);
            loop {
                let Some(front_len) = self.queues.get(&key).and_then(|q| q.packets.front()).map(|p| p.data.len()) else { break };
                if chain.is_empty() {
                    // Rule removed while packets were waiting: flush.
                    let pkt = self.pop(&key).unwrap();
                    out.push(pkt);
                    continue;
                }
                let mut wait = Duration::ZERO;
                for w in &chain {
                    let b = self.limits_mut(*w).bucket(key.dir).unwrap();
                    wait = wait.max(b.wait());
                }
                if wait > Duration::ZERO {
                    next_wait = next_wait.min(wait);
                    break;
                }
                for w in &chain {
                    self.limits_mut(*w).bucket(key.dir).unwrap().take(front_len);
                }
                let pkt = self.pop(&key).unwrap();
                out.push(pkt);
                budget -= 1;
                if budget == 0 {
                    return Duration::ZERO;
                }
            }
        }
        self.order.retain(|k| self.queues.get(k).map(|q| !q.packets.is_empty()).unwrap_or(false));
        self.queues.retain(|_, q| !q.packets.is_empty());
        next_wait.max(Duration::from_micros(500))
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
                || self.apps.values().any(|l| !l.is_empty()))
    }
}

impl Limits {
    fn bucket_ref(&self, dir: Dir) -> Option<&Bucket> {
        match dir {
            Dir::Down => self.down.as_ref(),
            Dir::Up => self.up.as_ref(),
        }
    }
}
