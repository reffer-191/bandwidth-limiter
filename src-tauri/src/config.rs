//! Persistent user configuration (rules + UI preferences), stored as JSON in
//! `%APPDATA%\Bandwidth Limiter\config.json`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub enabled: bool,
    /// bytes per second
    pub rate: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    pub dl: Limit,
    pub ul: Limit,
    pub block_dl: bool,
    pub block_ul: bool,
}

impl Rule {
    pub fn is_active(&self) -> bool {
        self.dl.enabled || self.ul.enabled || self.block_dl || self.block_ul
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub master: bool,
    pub global: Rule,
    pub global_internet_only: bool,
    pub hotspot: Rule,
    /// Keyed by lower-cased executable path (or "system" / "unknown").
    pub apps: HashMap<String, AppRule>,
    pub history_minutes: u32,
    pub units: String,
    pub theme: String,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub check_updates: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            master: true,
            global: Rule::default(),
            global_internet_only: true,
            hotspot: Rule::default(),
            apps: HashMap::new(),
            history_minutes: 10,
            units: "bits".into(),
            theme: "system".into(),
            start_minimized: false,
            minimize_to_tray: true,
            close_to_tray: false,
            check_updates: true,
        }
    }
}

impl Config {
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

    pub fn sanitize(&mut self) {
        self.history_minutes = self.history_minutes.clamp(1, 60);
        if self.units != "bytes" {
            self.units = "bits".into();
        }
        if !matches!(self.theme.as_str(), "light" | "dark") {
            self.theme = "system".into();
        }
        self.apps.retain(|_, r| r.rule.is_active());
        // A limit of 0 B/s would be a block in disguise; treat it as 1 Mbit/s.
        let fix = |l: &mut Limit| {
            if l.enabled && l.rate == 0 {
                l.rate = 125_000;
            }
        };
        fix(&mut self.global.dl);
        fix(&mut self.global.ul);
        fix(&mut self.hotspot.dl);
        fix(&mut self.hotspot.ul);
        for r in self.apps.values_mut() {
            fix(&mut r.rule.dl);
            fix(&mut r.rule.ul);
        }
    }
}

