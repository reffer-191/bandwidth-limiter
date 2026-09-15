//! Persistent per-application usage: which apps used the network in the last
//! 30 days and how much (daily buckets), stored as `usage.json` next to the
//! configuration. Keeps the Activity list stable across restarts.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;

pub const RETENTION_DAYS: u32 = 30;
const DAY_MS: u64 = 86_400_000;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct DayUsage {
    /// Days since the Unix epoch (UTC).
    pub day: u32,
    pub dl: u64,
    pub ul: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppUsage {
    pub name: String,
    pub description: String,
    pub exe: String,
    pub is_device: bool,
    pub first_seen: u64,
    pub last_seen: u64,
    pub days: Vec<DayUsage>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageStore {
    pub apps: HashMap<String, AppUsage>,
    #[serde(skip)]
    dirty: bool,
}

fn day_of(ms: u64) -> u32 {
    (ms / DAY_MS) as u32
}

impl UsageStore {
    pub fn path() -> PathBuf {
        Config::path().with_file_name("usage.json")
    }

    pub fn load() -> UsageStore {
        std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let p = Self::path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).unwrap())?;
        std::fs::rename(&tmp, &p)?;
        self.dirty = false;
        Ok(())
    }

    /// Records traffic for an app; metadata is refreshed so renamed/moved
    /// executables keep a sensible label.
    pub fn add(&mut self, key: &str, meta: &super::AppMeta, dl: u64, ul: u64, now_ms: u64) {
        if dl == 0 && ul == 0 {
            return;
        }
        let entry = self.apps.entry(key.to_string()).or_insert_with(|| AppUsage {
            first_seen: now_ms,
            ..Default::default()
        });
        entry.name = meta.name.clone();
        entry.description = meta.description.clone();
        entry.exe = meta.exe.clone();
        entry.is_device = meta.is_device;
        entry.last_seen = now_ms;
        let today = day_of(now_ms);
        match entry.days.last_mut() {
            Some(d) if d.day == today => {
                d.dl += dl;
                d.ul += ul;
            }
            _ => entry.days.push(DayUsage { day: today, dl, ul }),
        }
        self.dirty = true;
    }

    /// Drops day buckets and apps outside the retention window.
    pub fn prune(&mut self, now_ms: u64) {
        let cutoff_day = day_of(now_ms).saturating_sub(RETENTION_DAYS);
        let cutoff_ms = now_ms.saturating_sub(RETENTION_DAYS as u64 * DAY_MS);
        let before = self.apps.len();
        for a in self.apps.values_mut() {
            let n = a.days.len();
            a.days.retain(|d| d.day >= cutoff_day);
            if a.days.len() != n {
                self.dirty = true;
            }
        }
        self.apps.retain(|_, a| a.last_seen >= cutoff_ms);
        if self.apps.len() != before {
            self.dirty = true;
        }
    }

    /// (download, upload) bytes within the retention window.
    pub fn totals(&self, key: &str, now_ms: u64) -> (u64, u64) {
        let cutoff_day = day_of(now_ms).saturating_sub(RETENTION_DAYS);
        self.apps
            .get(key)
            .map(|a| {
                a.days
                    .iter()
                    .filter(|d| d.day >= cutoff_day)
                    .fold((0, 0), |(dl, ul), d| (dl + d.dl, ul + d.ul))
            })
            .unwrap_or((0, 0))
    }

    pub fn last_seen(&self, key: &str) -> u64 {
        self.apps.get(key).map(|a| a.last_seen).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str) -> crate::engine::AppMeta {
        crate::engine::AppMeta { id: 1, key: name.into(), name: name.into(), description: String::new(), exe: String::new(), pids: vec![], is_device: false }
    }

    #[test]
    fn rolling_window_and_prune() {
        let mut s = UsageStore::default();
        let day = DAY_MS;
        let now = 100 * day + 5000;
        s.add("a", &meta("a"), 10, 1, now - 40 * day); // too old
        s.add("a", &meta("a"), 20, 2, now - 10 * day);
        s.add("a", &meta("a"), 30, 3, now);
        s.add("a", &meta("a"), 5, 5, now + 1000); // same day bucket
        s.add("old", &meta("old"), 99, 99, now - 45 * day);
        assert_eq!(s.totals("a", now), (55, 10));
        assert_eq!(s.apps["a"].days.len(), 3);
        s.prune(now);
        assert!(s.apps.get("old").is_none(), "apps unseen for 30 days are dropped");
        assert_eq!(s.apps["a"].days.len(), 2, "old day buckets are dropped");
        assert_eq!(s.totals("a", now), (55, 10));
        let json = serde_json::to_string(&s).unwrap();
        let back: UsageStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.totals("a", now), (55, 10));
    }
}
