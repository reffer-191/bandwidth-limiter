//! Byte counters and the rate history ring buffer that feeds the chart.

use std::collections::{HashMap, VecDeque};

use serde::Serialize;

use super::AppId;

#[derive(Default, Clone, Copy, Debug, Serialize)]
pub struct Bytes {
    pub dl: u64,
    pub ul: u64,
}

impl Bytes {
    #[inline]
    pub fn add(&mut self, outbound: bool, n: u64) {
        if outbound { self.ul += n } else { self.dl += n }
    }
    fn delta(&self, prev: &Bytes) -> Bytes {
        Bytes { dl: self.dl.saturating_sub(prev.dl), ul: self.ul.saturating_sub(prev.ul) }
    }
}

#[derive(Default, Clone, Copy, Debug, Serialize)]
pub struct Rate {
    /// bytes per second
    pub dl: f64,
    pub ul: f64,
}

impl Rate {
    fn from_delta(d: Bytes, secs: f64) -> Rate {
        Rate { dl: d.dl as f64 / secs, ul: d.ul as f64 / secs }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Counters {
    pub total: Bytes,
    pub internet: Bytes,
    pub local: Bytes,
    pub hotspot: Bytes,
    pub by_app: HashMap<AppId, Bytes>,
}

impl Counters {
    #[inline]
    pub fn record(&mut self, app: AppId, outbound: bool, len: usize, internet: bool, forward: bool) {
        let n = len as u64;
        self.total.add(outbound, n);
        if forward {
            self.hotspot.add(outbound, n);
        } else if internet {
            self.internet.add(outbound, n);
        } else {
            self.local.add(outbound, n);
        }
        self.by_app.entry(app).or_default().add(outbound, n);
    }
}

/// Snapshot of per-second rates, produced by the ticker.
#[derive(Default, Clone, Debug, Serialize)]
pub struct Sample {
    pub ts: u64,
    pub total: Rate,
    pub internet: Rate,
    pub local: Rate,
    pub hotspot: Rate,
    /// Top consumers at that instant, (app id, rate).
    pub top: Vec<(AppId, Rate)>,
}

pub struct Stats {
    pub counters: Counters,
    prev: Counters,
    pub history: VecDeque<Sample>,
    pub capacity: usize,
}

impl Default for Stats {
    fn default() -> Self {
        Self { counters: Counters::default(), prev: Counters::default(), history: VecDeque::new(), capacity: 3600 }
    }
}

impl Stats {
    /// Moves bytes counted for `from` (the "Unknown" pseudo-app) to `to`
    /// once the flow's owner became known. Both the counters and the
    /// previous snapshot move, so the per-tick deltas (which feed the usage
    /// store) are not disturbed; the usage store is corrected separately.
    pub fn reattribute(&mut self, from: AppId, to: AppId, dl: u64, ul: u64) {
        for c in [&mut self.counters, &mut self.prev] {
            if let Some(b) = c.by_app.get_mut(&from) {
                b.dl = b.dl.saturating_sub(dl);
                b.ul = b.ul.saturating_sub(ul);
            }
            let b = c.by_app.entry(to).or_default();
            b.dl += dl;
            b.ul += ul;
        }
    }

    /// Computes rates since the previous tick and appends a history sample.
    /// Returns the sample and the per-app rates (all apps, not just top).
    pub fn tick(&mut self, ts: u64, secs: f64) -> (Sample, HashMap<AppId, (Rate, Bytes)>) {
        let c = &self.counters;
        let p = &self.prev;
        let mut per_app: HashMap<AppId, (Rate, Bytes)> = HashMap::with_capacity(c.by_app.len());
        for (id, b) in &c.by_app {
            let d = b.delta(p.by_app.get(id).unwrap_or(&Bytes::default()));
            per_app.insert(*id, (Rate::from_delta(d, secs), d));
        }
        let mut top: Vec<(AppId, Rate)> = per_app
            .iter()
            .filter(|(_, (r, _))| r.dl + r.ul > 0.0)
            .map(|(id, (r, _))| (*id, *r))
            .collect();
        top.sort_by(|a, b| (b.1.dl + b.1.ul).partial_cmp(&(a.1.dl + a.1.ul)).unwrap());
        top.truncate(24);
        let sample = Sample {
            ts,
            total: Rate::from_delta(c.total.delta(&p.total), secs),
            internet: Rate::from_delta(c.internet.delta(&p.internet), secs),
            local: Rate::from_delta(c.local.delta(&p.local), secs),
            hotspot: Rate::from_delta(c.hotspot.delta(&p.hotspot), secs),
            top,
        };
        self.prev = self.counters.clone();
        self.history.push_back(sample.clone());
        while self.history.len() > self.capacity {
            self.history.pop_front();
        }
        (sample, per_app)
    }
}
