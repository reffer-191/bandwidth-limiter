//! Persistent per-application usage: which apps used the network in the last
//! 30 days and how much, in hourly buckets, stored in `usage.db` (SQLite)
//! next to the configuration. Keeps the Activity list stable across restarts
//! and feeds the Statistics view and the data quotas.
//!
//! Everything is kept in memory (a few hundred KB); the database only receives
//! the deltas accumulated since the last flush, inside one transaction.

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::clock::{DAY_MS, HOUR_MS};
use crate::config::Config;

pub const RETENTION_DAYS: u32 = 30;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HourUsage {
    /// Hours since the Unix epoch (UTC).
    pub hour: u32,
    pub dl: u64,
    pub ul: u64,
}

#[derive(Clone, Debug, Default)]
pub struct AppUsage {
    pub name: String,
    pub description: String,
    pub exe: String,
    pub is_device: bool,
    pub first_seen: u64,
    pub last_seen: u64,
    /// Ascending by hour.
    pub hours: Vec<HourUsage>,
}

#[derive(Default)]
pub struct UsageStore {
    pub apps: HashMap<String, AppUsage>,
    db: Option<Connection>,
    /// (key, hour) → bytes not yet written.
    pending: HashMap<(String, u32), (u64, u64)>,
    /// (key, hour, dl, ul) to subtract on the next flush (re-attribution).
    corrections: Vec<(String, u32, i64, i64)>,
    meta_dirty: Vec<String>,
}

/// The pre-0.7 JSON layout, read once for migration.
#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LegacyStore {
    apps: HashMap<String, LegacyApp>,
}
#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LegacyApp {
    name: String,
    description: String,
    exe: String,
    is_device: bool,
    first_seen: u64,
    last_seen: u64,
    days: Vec<LegacyDay>,
}
#[derive(Deserialize, Serialize, Default, Clone, Copy)]
struct LegacyDay {
    day: u32,
    dl: u64,
    ul: u64,
}

pub fn hour_of(ms: u64) -> u32 {
    (ms / HOUR_MS) as u32
}

impl UsageStore {
    pub fn path() -> PathBuf {
        Config::path().with_file_name("usage.db")
    }

    fn legacy_path() -> PathBuf {
        Config::path().with_file_name("usage.json")
    }

    pub fn load() -> UsageStore {
        let mut store = UsageStore::default();
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match Self::open(&path) {
            Ok(db) => {
                store.db = Some(db);
                store.migrate_legacy();
                store.drop_bogus_devices();
                if let Err(e) = store.read_all() {
                    log::warn!("usage read: {e}");
                }
            }
            Err(e) => log::warn!("usage db {}: {e}", path.display()),
        }
        store
    }

    /// In-memory store (tests).
    #[cfg(test)]
    pub fn in_memory() -> UsageStore {
        UsageStore { db: Self::open_in_memory().ok(), ..Default::default() }
    }

    fn open(path: &PathBuf) -> rusqlite::Result<Connection> {
        let db = Connection::open(path)?;
        Self::init(&db)?;
        Ok(db)
    }

    #[cfg(test)]
    fn open_in_memory() -> rusqlite::Result<Connection> {
        let db = Connection::open_in_memory()?;
        Self::init(&db)?;
        Ok(db)
    }

    fn init(db: &Connection) -> rusqlite::Result<()> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS apps (
               key TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
               exe TEXT NOT NULL DEFAULT '', is_device INTEGER NOT NULL DEFAULT 0,
               first_seen INTEGER NOT NULL DEFAULT 0, last_seen INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS usage (
               key TEXT NOT NULL, hour INTEGER NOT NULL, dl INTEGER NOT NULL DEFAULT 0, ul INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (key, hour)) WITHOUT ROWID;",
        )
    }

    /// One-off import of `usage.json` (daily buckets) into an *empty*
    /// database; a json file written later by an older build is ignored.
    fn migrate_legacy(&mut self) {
        let legacy = Self::legacy_path();
        let Ok(bytes) = std::fs::read(&legacy) else { return };
        let Ok(old) = serde_json::from_slice::<LegacyStore>(&bytes) else { return };
        if let Some(db) = self.db.as_mut() {
            let rows: i64 = db.query_row("SELECT COUNT(*) FROM usage", [], |r| r.get(0)).unwrap_or(0);
            if rows > 0 {
                return;
            }
            let r: rusqlite::Result<()> = (|| {
                let tx = db.transaction()?;
                for (key, a) in &old.apps {
                    tx.execute(
                        "INSERT INTO apps (key, name, description, exe, is_device, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                         ON CONFLICT(key) DO UPDATE SET name = excluded.name, description = excluded.description, exe = excluded.exe,
                           is_device = excluded.is_device, last_seen = MAX(last_seen, excluded.last_seen)",
                        params![key, a.name, a.description, a.exe, a.is_device as i64, a.first_seen as i64, a.last_seen as i64],
                    )?;
                    for d in &a.days {
                        // Daily totals land at midday of their (UTC) day.
                        let hour = d.day * 24 + 12;
                        tx.execute(
                            "INSERT INTO usage (key, hour, dl, ul) VALUES (?1, ?2, ?3, ?4)
                             ON CONFLICT(key, hour) DO UPDATE SET dl = dl + excluded.dl, ul = ul + excluded.ul",
                            params![key, hour as i64, d.dl as i64, d.ul as i64],
                        )?;
                    }
                }
                tx.commit()
            })();
            match r {
                Ok(()) => {
                    if let Err(e) = std::fs::rename(&legacy, legacy.with_extension("json.migrated")) {
                        log::warn!("usage: could not rename usage.json: {e}");
                    }
                    log::info!("usage: migrated {} apps from usage.json", old.apps.len());
                }
                Err(e) => log::warn!("usage migration: {e}"),
            }
        }
    }

    /// Versions before 0.8.1 could record public addresses as hotspot
    /// "devices" (direction guessed wrong); remove them for good.
    fn drop_bogus_devices(&mut self) {
        let Some(db) = self.db.as_ref() else { return };
        let keys: Vec<String> = db
            .prepare("SELECT key FROM apps WHERE key LIKE 'hotspot:%'")
            .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).map(|it| it.flatten().collect()))
            .unwrap_or_default();
        let bogus: Vec<String> = keys
            .into_iter()
            .filter(|k| {
                let ip = &k["hotspot:".len()..];
                match ip.parse::<std::net::IpAddr>() {
                    Ok(std::net::IpAddr::V4(v4)) => !super::packet::is_local(&super::packet::map_ipv4(&v4.octets())),
                    Ok(std::net::IpAddr::V6(v6)) => !super::packet::is_local(&v6.octets()),
                    Err(_) => false,
                }
            })
            .collect();
        for k in &bogus {
            for r in [db.execute("DELETE FROM usage WHERE key = ?1", params![k]), db.execute("DELETE FROM apps WHERE key = ?1", params![k])] {
                if let Err(e) = r {
                    log::warn!("usage cleanup: {e}");
                }
            }
        }
        if !bogus.is_empty() {
            log::info!("usage: removed {} public addresses recorded as hotspot devices", bogus.len());
        }
    }

    fn read_all(&mut self) -> rusqlite::Result<()> {
        let Some(db) = self.db.as_ref() else { return Ok(()) };
        let mut apps = HashMap::new();
        {
            let mut st = db.prepare("SELECT key, name, description, exe, is_device, first_seen, last_seen FROM apps")?;
            let rows = st.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    AppUsage {
                        name: r.get(1)?,
                        description: r.get(2)?,
                        exe: r.get(3)?,
                        is_device: r.get::<_, i64>(4)? != 0,
                        first_seen: r.get::<_, i64>(5)? as u64,
                        last_seen: r.get::<_, i64>(6)? as u64,
                        hours: Vec::new(),
                    },
                ))
            })?;
            for row in rows {
                let (k, a) = row?;
                apps.insert(k, a);
            }
        }
        let mut st = db.prepare("SELECT key, hour, dl, ul FROM usage ORDER BY key, hour")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, HourUsage { hour: r.get::<_, i64>(1)? as u32, dl: r.get::<_, i64>(2)? as u64, ul: r.get::<_, i64>(3)? as u64 }))
        })?;
        for row in rows {
            let (k, h) = row?;
            apps.entry(k).or_default().hours.push(h);
        }
        self.apps = apps;
        Ok(())
    }

    /// Writes the pending deltas. Cheap when nothing changed.
    pub fn save(&mut self) -> Result<(), String> {
        if self.pending.is_empty() && self.meta_dirty.is_empty() && self.corrections.is_empty() {
            return Ok(());
        }
        let Some(db) = self.db.as_mut() else { return Ok(()) };
        let tx = db.transaction().map_err(|e| e.to_string())?;
        for key in self.meta_dirty.drain(..) {
            if let Some(a) = self.apps.get(&key) {
                tx.execute(
                    "INSERT INTO apps (key, name, description, exe, is_device, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(key) DO UPDATE SET name = excluded.name, description = excluded.description, exe = excluded.exe,
                       is_device = excluded.is_device, last_seen = excluded.last_seen",
                    params![key, a.name, a.description, a.exe, a.is_device as i64, a.first_seen as i64, a.last_seen as i64],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        for ((key, hour), (dl, ul)) in self.pending.drain() {
            tx.execute(
                "INSERT INTO usage (key, hour, dl, ul) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key, hour) DO UPDATE SET dl = dl + excluded.dl, ul = ul + excluded.ul",
                params![key, hour as i64, dl as i64, ul as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        for (key, hour, dl, ul) in self.corrections.drain(..) {
            tx.execute(
                "UPDATE usage SET dl = MAX(0, dl - ?3), ul = MAX(0, ul - ?4) WHERE key = ?1 AND hour = ?2",
                params![key, hour as i64, dl, ul],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// Records traffic for an app; metadata is refreshed so renamed/moved
    /// executables keep a sensible label.
    pub fn add(&mut self, key: &str, meta: &super::AppMeta, dl: u64, ul: u64, now_ms: u64) {
        if dl == 0 && ul == 0 {
            return;
        }
        let entry = self.apps.entry(key.to_string()).or_insert_with(|| AppUsage { first_seen: now_ms, ..Default::default() });
        entry.name = meta.name.clone();
        entry.description = meta.description.clone();
        entry.exe = meta.exe.clone();
        entry.is_device = meta.is_device;
        entry.last_seen = now_ms;
        let hour = hour_of(now_ms);
        match entry.hours.last_mut() {
            Some(h) if h.hour == hour => {
                h.dl += dl;
                h.ul += ul;
            }
            _ => entry.hours.push(HourUsage { hour, dl, ul }),
        }
        let p = self.pending.entry((key.to_string(), hour)).or_default();
        p.0 += dl;
        p.1 += ul;
        if !self.meta_dirty.iter().any(|k| k == key) {
            self.meta_dirty.push(key.to_string());
        }
    }

    /// Moves bytes already stored for `from` (this hour) to `to`; used when
    /// a flow's owner is learnt after its first packets were counted as
    /// "Unknown". Whatever is not in this hour's bucket stays where it is.
    pub fn transfer(&mut self, from: &str, to_meta: &super::AppMeta, dl: u64, ul: u64, now_ms: u64) {
        if dl + ul == 0 {
            return;
        }
        let hour = hour_of(now_ms);
        let (mut mdl, mut mul) = (0, 0);
        if let Some(a) = self.apps.get_mut(from) {
            if let Some(h) = a.hours.last_mut() {
                if h.hour == hour {
                    mdl = dl.min(h.dl);
                    mul = ul.min(h.ul);
                    h.dl -= mdl;
                    h.ul -= mul;
                }
            }
        }
        if mdl + mul == 0 {
            return;
        }
        // The database is updated with deltas: queue the subtraction.
        self.corrections.push((from.to_string(), hour, mdl as i64, mul as i64));
        self.add(&to_meta.key, to_meta, mdl, mul, now_ms);
    }

    /// Drops hour buckets and apps outside the retention window.
    pub fn prune(&mut self, now_ms: u64) {
        let cutoff_hour = hour_of(now_ms).saturating_sub(RETENTION_DAYS * 24);
        let cutoff_ms = now_ms.saturating_sub(RETENTION_DAYS as u64 * DAY_MS);
        for a in self.apps.values_mut() {
            a.hours.retain(|h| h.hour >= cutoff_hour);
        }
        self.apps.retain(|_, a| a.last_seen >= cutoff_ms);
        if let Some(db) = self.db.as_ref() {
            for r in [
                db.execute("DELETE FROM usage WHERE hour < ?1", params![cutoff_hour as i64]),
                db.execute("DELETE FROM apps WHERE last_seen < ?1", params![cutoff_ms as i64]),
            ] {
                if let Err(e) = r {
                    log::warn!("usage prune: {e}");
                }
            }
        }
    }

    /// (download, upload) bytes within the retention window.
    pub fn totals(&self, key: &str, now_ms: u64) -> (u64, u64) {
        self.sum_since(key, now_ms.saturating_sub(RETENTION_DAYS as u64 * DAY_MS))
    }

    /// (download, upload) bytes in hour buckets starting at or after `from_ms`.
    pub fn sum_since(&self, key: &str, from_ms: u64) -> (u64, u64) {
        let from = hour_of(from_ms);
        self.apps
            .get(key)
            .map(|a| {
                let start = a.hours.partition_point(|h| h.hour < from);
                a.hours[start..].iter().fold((0, 0), |(dl, ul), h| (dl + h.dl, ul + h.ul))
            })
            .unwrap_or((0, 0))
    }

    /// Sum over every app matching `filter` since `from_ms`.
    pub fn sum_all_since(&self, from_ms: u64, filter: impl Fn(&str, &AppUsage) -> bool) -> (u64, u64) {
        let from = hour_of(from_ms);
        let mut t = (0, 0);
        for (k, a) in &self.apps {
            if !filter(k, a) {
                continue;
            }
            let start = a.hours.partition_point(|h| h.hour < from);
            for h in &a.hours[start..] {
                t.0 += h.dl;
                t.1 += h.ul;
            }
        }
        t
    }

    pub fn last_seen(&self, key: &str) -> u64 {
        self.apps.get(key).map(|a| a.last_seen).unwrap_or(0)
    }

    /// Time series and per-app totals between `from_ms` and `to_ms`, in
    /// buckets of `bucket_ms` aligned to `from_ms`. `key` restricts the series
    /// to one app (the per-app totals always cover everything).
    pub fn stats(&self, from_ms: u64, to_ms: u64, bucket_ms: u64, key: Option<&str>) -> Stats {
        let n = ((to_ms.saturating_sub(from_ms)) + bucket_ms - 1) / bucket_ms;
        let mut buckets: Vec<StatBucket> = (0..n).map(|i| StatBucket { t: from_ms + i * bucket_ms, dl: 0, ul: 0 }).collect();
        let from_hour = hour_of(from_ms);
        let to_hour = hour_of(to_ms.saturating_sub(1));
        let mut apps: Vec<StatApp> = Vec::new();
        let mut total = (0u64, 0u64);
        for (k, a) in &self.apps {
            let start = a.hours.partition_point(|h| h.hour < from_hour);
            let mut sum = (0u64, 0u64);
            for h in &a.hours[start..] {
                if h.hour > to_hour {
                    break;
                }
                sum.0 += h.dl;
                sum.1 += h.ul;
                if key.map(|kk| kk == k).unwrap_or(true) {
                    let t = h.hour as u64 * HOUR_MS;
                    let i = t.saturating_sub(from_ms) / bucket_ms;
                    if let Some(b) = buckets.get_mut(i as usize) {
                        b.dl += h.dl;
                        b.ul += h.ul;
                    }
                }
            }
            if sum.0 + sum.1 > 0 {
                total.0 += sum.0;
                total.1 += sum.1;
                apps.push(StatApp {
                    key: k.clone(),
                    name: a.name.clone(),
                    description: a.description.clone(),
                    exe: a.exe.clone(),
                    is_device: a.is_device,
                    dl: sum.0,
                    ul: sum.1,
                });
            }
        }
        apps.sort_by(|a, b| (b.dl + b.ul).cmp(&(a.dl + a.ul)).then_with(|| a.name.cmp(&b.name)));
        Stats { buckets, apps, total: StatBucket { t: from_ms, dl: total.0, ul: total.1 } }
    }
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct StatBucket {
    /// Bucket start, Unix ms.
    pub t: u64,
    pub dl: u64,
    pub ul: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StatApp {
    pub key: String,
    pub name: String,
    pub description: String,
    pub exe: String,
    pub is_device: bool,
    pub dl: u64,
    pub ul: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Stats {
    pub buckets: Vec<StatBucket>,
    pub apps: Vec<StatApp>,
    pub total: StatBucket,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str) -> crate::engine::AppMeta {
        crate::engine::AppMeta { id: 1, key: name.into(), name: name.into(), description: String::new(), exe: String::new(), pids: vec![], is_device: false }
    }

    #[test]
    fn rolling_window_and_prune() {
        let mut s = UsageStore::in_memory();
        let day = DAY_MS;
        let now = 100 * day + 5000;
        s.add("a", &meta("a"), 10, 1, now - 40 * day); // too old
        s.add("a", &meta("a"), 20, 2, now - 10 * day);
        s.add("a", &meta("a"), 30, 3, now);
        s.add("a", &meta("a"), 5, 5, now + 1000); // same hour bucket
        s.add("old", &meta("old"), 99, 99, now - 45 * day);
        assert_eq!(s.totals("a", now), (55, 10));
        assert_eq!(s.apps["a"].hours.len(), 3);
        assert_eq!(s.sum_since("a", now - day), (35, 8));
        s.save().unwrap();
        s.prune(now);
        assert!(s.apps.get("old").is_none(), "apps unseen for 30 days are dropped");
        assert_eq!(s.apps["a"].hours.len(), 2, "old hour buckets are dropped");
        assert_eq!(s.totals("a", now), (55, 10));
        // Reload from the database: the pending deltas were written.
        let mut back = UsageStore { db: s.db.take(), ..Default::default() };
        back.read_all().unwrap();
        assert_eq!(back.totals("a", now), (55, 10));
        assert!(back.apps.get("old").is_none());
    }

    #[test]
    fn stats_buckets() {
        let mut s = UsageStore::in_memory();
        let from = 200 * DAY_MS;
        s.add("a", &meta("a"), 100, 10, from + HOUR_MS / 2);
        s.add("a", &meta("a"), 100, 10, from + 3 * HOUR_MS);
        s.add("b", &meta("b"), 5, 5, from + 3 * HOUR_MS);
        let st = s.stats(from, from + DAY_MS, HOUR_MS, None);
        assert_eq!(st.buckets.len(), 24);
        assert_eq!(st.buckets[0].dl, 100);
        assert_eq!(st.buckets[3].dl, 105);
        assert_eq!(st.total.dl, 205);
        assert_eq!(st.apps[0].key, "a");
        let only_b = s.stats(from, from + DAY_MS, HOUR_MS, Some("b"));
        assert_eq!(only_b.buckets[3].dl, 5);
        assert_eq!(only_b.apps.len(), 2, "app totals are not filtered");
    }
}
