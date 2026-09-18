//! Persistent user configuration (rules + UI preferences), stored as JSON in
//! `%APPDATA%\Bandwidth Limiter\config.json`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Lowest rates the engine accepts (bytes/s). Mirrored in RuleEditor.tsx.
pub const MIN_RATE_GENERAL: u64 = 16_000;
pub const MIN_RATE_APP: u64 = 1_000;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub enabled: bool,
    /// bytes per second
    pub rate: u64,
}

/// Time window (local time) in which a rule is enforced. `from > to` spans
/// midnight (22:00 → 06:00). Days are Monday-first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Schedule {
    pub enabled: bool,
    pub days: [bool; 7],
    /// minutes since midnight
    pub from: u16,
    pub to: u16,
}

impl Default for Schedule {
    fn default() -> Self {
        Self { enabled: false, days: [true; 7], from: 0, to: 0 }
    }
}

impl Schedule {
    /// `weekday` is Monday-first (0..7), `minute` minutes since midnight.
    pub fn is_active(&self, weekday: usize, minute: u16) -> bool {
        if !self.enabled {
            return true;
        }
        let (from, to) = (self.from, self.to);
        if from == to {
            // Whole selected days.
            return self.days[weekday % 7];
        }
        if from < to {
            self.days[weekday % 7] && minute >= from && minute < to
        } else {
            // Overnight: the part after `from` belongs to today, the part
            // before `to` to the previous day's selection.
            (self.days[weekday % 7] && minute >= from) || (self.days[(weekday + 6) % 7] && minute < to)
        }
    }
}

/// Data allowance; when it runs out the rule either blocks the traffic or
/// only raises a notification.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Quota {
    pub enabled: bool,
    pub bytes: u64,
    /// "day" | "week" | "month"
    pub period: String,
    /// "block" | "notify"
    pub action: String,
}

impl Default for Quota {
    fn default() -> Self {
        Self { enabled: false, bytes: 1 << 30, period: "day".into(), action: "block".into() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    pub dl: Limit,
    pub ul: Limit,
    pub block_dl: bool,
    pub block_ul: bool,
    /// "high" | "normal" | "low" — only meaningful for apps/devices.
    pub priority: String,
    pub schedule: Schedule,
    pub quota: Quota,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            dl: Limit::default(),
            ul: Limit::default(),
            block_dl: false,
            block_ul: false,
            priority: "normal".into(),
            schedule: Schedule::default(),
            quota: Quota::default(),
        }
    }
}

impl Rule {
    /// True when the rule does something: a limit/block, a quota or a
    /// non-default priority. A schedule alone is nothing to enforce.
    pub fn is_active(&self) -> bool {
        self.dl.enabled || self.ul.enabled || self.block_dl || self.block_ul || self.quota.enabled || self.priority != "normal"
    }

    pub fn priority_level(&self) -> u8 {
        match self.priority.as_str() {
            "high" => 2,
            "low" => 0,
            _ => 1,
        }
    }

    fn sanitize(&mut self, min_rate: u64) {
        // A limit of 0 B/s would be a block in disguise; treat it as 1 Mbit/s.
        // Below a few KB/s TCP acknowledgements starve and the connection is
        // effectively dead, so rates are floored (blocking is an explicit
        // switch instead).
        for l in [&mut self.dl, &mut self.ul] {
            if l.enabled && l.rate == 0 {
                l.rate = 125_000;
            }
            if l.enabled && l.rate < min_rate {
                l.rate = min_rate;
            }
        }
        if !matches!(self.priority.as_str(), "high" | "low") {
            self.priority = "normal".into();
        }
        self.schedule.from = self.schedule.from.min(24 * 60 - 1);
        self.schedule.to = self.schedule.to.min(24 * 60 - 1);
        if !matches!(self.quota.period.as_str(), "week" | "month") {
            self.quota.period = "day".into();
        }
        if self.quota.action != "notify" {
            self.quota.action = "block".into();
        }
        if self.quota.enabled && self.quota.bytes == 0 {
            self.quota.enabled = false;
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppRule {
    #[serde(flatten)]
    pub rule: Rule,
    /// Display metadata so the rule list is meaningful while the app is offline.
    pub name: String,
    pub description: String,
    pub exe: String,
}

/// Limit or block traffic to a remote host / port, optionally for one app.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// IP, CIDR (10.0.0.0/8, 2a00::/16) or host name; empty = any host.
    pub host: String,
    /// "443", "6881-6889", "80,443"; empty = any port.
    pub ports: String,
    /// "any" | "tcp" | "udp"
    pub protocol: String,
    /// App key the rule is restricted to; empty = every app.
    pub app: String,
    #[serde(flatten)]
    pub rule: Rule,
}

/// Replaces the whole-PC limit for traffic on a given adapter.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AdapterRule {
    pub id: String,
    pub enabled: bool,
    /// "wifi" | "ethernet" | "metered" | "name:<adapter friendly name>"
    pub adapter: String,
    #[serde(flatten)]
    pub rule: Rule,
}

/// Everything a profile stores: the rules, not the UI preferences.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleSet {
    pub global: Rule,
    pub global_internet_only: bool,
    pub hotspot: Rule,
    pub apps: HashMap<String, AppRule>,
    pub connections: Vec<ConnRule>,
    pub adapters: Vec<AdapterRule>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    pub rules: RuleSet,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub master: bool,
    pub global: Rule,
    pub global_internet_only: bool,
    pub hotspot: Rule,
    /// Keyed by lower-cased executable path (or "system" / "unknown").
    pub apps: HashMap<String, AppRule>,
    pub connections: Vec<ConnRule>,
    pub adapters: Vec<AdapterRule>,
    pub profiles: Vec<Profile>,
    /// Name of the profile the live rules were loaded from ("" = none).
    pub active_profile: String,
    pub history_minutes: u32,
    pub units: String,
    pub theme: String,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub check_updates: bool,
    /// "system" | "es" | "en"
    pub language: String,
    pub onboarding_done: bool,
    pub notify_new_app: bool,
    pub notify_quota: bool,
    pub notify_schedule: bool,
    /// Charge the IP/TCP headers to the limits (wire speed) instead of only
    /// the payload (what download managers display).
    pub count_headers: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            master: true,
            global: Rule::default(),
            global_internet_only: true,
            hotspot: Rule::default(),
            apps: HashMap::new(),
            connections: Vec::new(),
            adapters: Vec::new(),
            profiles: Vec::new(),
            active_profile: String::new(),
            history_minutes: 10,
            units: "bits".into(),
            theme: "system".into(),
            start_minimized: false,
            minimize_to_tray: true,
            close_to_tray: false,
            check_updates: true,
            language: "system".into(),
            onboarding_done: false,
            notify_new_app: false,
            notify_quota: true,
            notify_schedule: true,
            count_headers: false,
        }
    }
}

impl Config {
    pub fn lang(&self) -> crate::i18n::Lang {
        crate::i18n::resolve(&self.language)
    }

    /// Portable mode: a file named `portable` next to the executable keeps the
    /// configuration in that same folder instead of %APPDATA%.
    pub fn portable_dir() -> Option<PathBuf> {
        let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        dir.join("portable").exists().then_some(dir)
    }

    pub fn path() -> PathBuf {
        if let Some(dir) = Self::portable_dir() {
            return dir.join("config.json");
        }
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Bandwidth Limiter").join("config.json")
    }

    pub fn load() -> Config {
        let p = Self::path();
        std::fs::read(&p)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap())?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    /// The live rules as a profile-shaped value.
    pub fn rule_set(&self) -> RuleSet {
        RuleSet {
            global: self.global.clone(),
            global_internet_only: self.global_internet_only,
            hotspot: self.hotspot.clone(),
            apps: self.apps.clone(),
            connections: self.connections.clone(),
            adapters: self.adapters.clone(),
        }
    }

    pub fn set_rule_set(&mut self, r: RuleSet) {
        self.global = r.global;
        self.global_internet_only = r.global_internet_only;
        self.hotspot = r.hotspot;
        self.apps = r.apps;
        self.connections = r.connections;
        self.adapters = r.adapters;
    }

    /// Stores the live rules into the active profile (if any), then loads
    /// `name`. Unknown names leave everything untouched.
    pub fn switch_profile(&mut self, name: &str) -> bool {
        if name == self.active_profile {
            return true;
        }
        let Some(target) = self.profiles.iter().position(|p| p.name == name) else { return false };
        let live = self.rule_set();
        if let Some(p) = self.profiles.iter_mut().find(|p| p.name == self.active_profile) {
            p.rules = live;
        }
        let rules = self.profiles[target].rules.clone();
        self.set_rule_set(rules);
        self.active_profile = name.to_string();
        true
    }

    pub fn sanitize(&mut self) {
        self.history_minutes = self.history_minutes.clamp(1, 60);
        if self.units != "bytes" {
            self.units = "bits".into();
        }
        if !matches!(self.theme.as_str(), "light" | "dark") {
            self.theme = "system".into();
        }
        if !matches!(self.language.as_str(), "es" | "en") {
            self.language = "system".into();
        }
        sanitize_rules(
            &mut self.global,
            &mut self.hotspot,
            &mut self.apps,
            &mut self.connections,
            &mut self.adapters,
        );
        for p in &mut self.profiles {
            let r = &mut p.rules;
            sanitize_rules(&mut r.global, &mut r.hotspot, &mut r.apps, &mut r.connections, &mut r.adapters);
        }
        // Profile names are unique, non-empty and the active one must exist.
        let mut seen = std::collections::HashSet::new();
        self.profiles.retain(|p| !p.name.trim().is_empty() && seen.insert(p.name.trim().to_lowercase()));
        for p in &mut self.profiles {
            p.name = p.name.trim().to_string();
        }
        if !self.profiles.iter().any(|p| p.name == self.active_profile) {
            self.active_profile.clear();
        }
    }
}

fn sanitize_rules(
    global: &mut Rule,
    hotspot: &mut Rule,
    apps: &mut HashMap<String, AppRule>,
    connections: &mut Vec<ConnRule>,
    adapters: &mut Vec<AdapterRule>,
) {
    global.sanitize(MIN_RATE_GENERAL);
    global.priority = "normal".into();
    hotspot.sanitize(MIN_RATE_GENERAL);
    hotspot.priority = "normal".into();
    apps.retain(|_, r| r.rule.is_active());
    for r in apps.values_mut() {
        r.rule.sanitize(MIN_RATE_APP);
    }
    for c in connections.iter_mut() {
        c.rule.sanitize(MIN_RATE_APP);
        c.rule.priority = "normal".into();
        c.host = c.host.trim().to_string();
        c.ports = c.ports.trim().to_string();
        if !matches!(c.protocol.as_str(), "tcp" | "udp") {
            c.protocol = "any".into();
        }
        if c.id.is_empty() {
            c.id = new_id();
        }
    }
    connections.truncate(250);
    for a in adapters.iter_mut() {
        a.rule.sanitize(MIN_RATE_GENERAL);
        a.rule.priority = "normal".into();
        if a.id.is_empty() {
            a.id = new_id();
        }
    }
    adapters.truncate(250);
}

/// Short random id for list entries (time + counter, enough for a config file).
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{:x}{:x}", t, N.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_windows() {
        let mut s = Schedule { enabled: true, days: [true; 7], from: 9 * 60, to: 17 * 60 };
        assert!(s.is_active(0, 9 * 60));
        assert!(s.is_active(0, 16 * 60 + 59));
        assert!(!s.is_active(0, 17 * 60));
        assert!(!s.is_active(0, 8 * 60));
        // Overnight window, Friday only: Friday 23:00 and Saturday 03:00 count.
        s.from = 22 * 60;
        s.to = 6 * 60;
        s.days = [false, false, false, false, true, false, false];
        assert!(s.is_active(4, 23 * 60));
        assert!(s.is_active(5, 3 * 60));
        assert!(!s.is_active(5, 23 * 60));
        assert!(!s.is_active(4, 12 * 60));
        // Same from/to = whole day.
        s.from = 0;
        s.to = 0;
        assert!(s.is_active(4, 12 * 60));
        assert!(!s.is_active(0, 12 * 60));
        s.enabled = false;
        assert!(s.is_active(0, 12 * 60));
    }

    #[test]
    fn profiles_round_trip() {
        let mut c = Config::default();
        c.global.dl = Limit { enabled: true, rate: 1_000_000 };
        c.profiles.push(Profile { name: "Home".into(), rules: c.rule_set() });
        c.active_profile = "Home".into();
        let mut work = RuleSet::default();
        work.global.ul = Limit { enabled: true, rate: 50_000 };
        c.profiles.push(Profile { name: "Work".into(), rules: work });
        c.global.dl.rate = 2_000_000; // edited while Home is active
        assert!(c.switch_profile("Work"));
        assert_eq!(c.profiles[0].rules.global.dl.rate, 2_000_000, "edits are stored into the profile we leave");
        assert!(!c.global.dl.enabled);
        assert_eq!(c.global.ul.rate, 50_000);
        assert!(!c.switch_profile("Nope"));
        assert_eq!(c.active_profile, "Work");
        c.sanitize();
        assert_eq!(c.profiles.len(), 2);
    }

    #[test]
    fn old_config_still_loads() {
        let json = r#"{"master":true,"global":{"dl":{"enabled":true,"rate":125000},"ul":{"enabled":false,"rate":0},"blockDl":false,"blockUl":false},"apps":{"x":{"dl":{"enabled":true,"rate":5000},"ul":{"enabled":false,"rate":0},"blockDl":false,"blockUl":false,"name":"x","description":"","exe":""}}}"#;
        let mut c: Config = serde_json::from_str(json).unwrap();
        c.sanitize();
        assert_eq!(c.apps["x"].rule.priority, "normal");
        assert!(c.apps["x"].rule.is_active());
        assert!(!c.global.schedule.enabled);
    }
}
