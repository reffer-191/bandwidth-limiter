//! Tauri commands exposed to the UI.

use serde::Serialize;
use tauri::State;

use crate::config::Config;
use crate::engine::adapters::Adapter;
use crate::engine::packet::{addr_to_string, PROTO_TCP, PROTO_UDP};
use crate::engine::procinfo;
use crate::engine::stats::Sample;
use crate::engine::{Engine, Status, Tick};

#[tauri::command]
pub fn get_status(engine: State<'_, Engine>) -> Status {
    engine.state.status.lock().clone()
}

#[tauri::command]
pub fn get_config(engine: State<'_, Engine>) -> Config {
    engine.state.config.read().clone()
}

#[tauri::command]
pub fn set_config(engine: State<'_, Engine>, config: Config) -> Result<Config, String> {
    engine.set_config(config)
}

#[tauri::command]
pub fn get_snapshot(engine: State<'_, Engine>) -> Option<Tick> {
    engine.state.latest.lock().clone()
}

#[tauri::command]
pub fn get_history(engine: State<'_, Engine>) -> Vec<Sample> {
    engine.state.stats.lock().history.iter().cloned().collect()
}

#[tauri::command]
pub fn get_adapters(engine: State<'_, Engine>) -> Vec<Adapter> {
    engine.state.adapters.read().adapters.clone()
}

/// Icon as a data URL; cached per app key. Runs on a blocking thread because
/// shell icon extraction can take a few milliseconds.
#[tauri::command]
pub async fn get_app_icon(engine: State<'_, Engine>, key: String) -> Result<Option<String>, String> {
    if let Some(cached) = engine.state.icons.lock().get(&key) {
        return Ok(cached.clone());
    }
    let exe = engine.state.app_exe(&key);
    let icon = match exe {
        Some(exe) => tauri::async_runtime::spawn_blocking(move || procinfo::icon_data_url(&exe))
            .await
            .map_err(|e| e.to_string())?,
        None => None,
    };
    engine.state.icons.lock().insert(key, icon.clone());
    Ok(icon)
}

#[tauri::command]
pub fn get_autostart() -> bool {
    crate::autostart::is_enabled()
}

#[tauri::command]
pub fn set_autostart(engine: State<'_, Engine>, enabled: bool) -> Result<bool, String> {
    let lang = engine.state.config.read().lang();
    crate::autostart::set_enabled(enabled, lang)
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<Option<crate::update::UpdateInfo>, String> {
    crate::update::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    crate::update::install(&app).await
}

#[tauri::command]
pub fn show_window(app: tauri::AppHandle) {
    crate::tray::show_main(&app);
}

/// Custom title-bar minimize: to the tray or to the taskbar, per settings.
#[tauri::command]
pub fn minimize_window(window: tauri::WebviewWindow, engine: State<'_, Engine>) {
    if engine.state.config.read().minimize_to_tray {
        let _ = window.hide();
    } else {
        let _ = window.minimize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowView {
    pub protocol: String,
    pub local: String,
    pub remote: String,
    pub pid: u32,
}

#[tauri::command]
pub fn get_app_flows(engine: State<'_, Engine>, key: String) -> Vec<FlowView> {
    let pids: Vec<u32> = {
        let apps = engine.state.apps.lock();
        match apps.by_key.get(&key) {
            Some(&id) => apps.list[id as usize].pids.clone(),
            None => return Vec::new(),
        }
    };
    let mut flows: Vec<FlowView> = engine
        .state
        .flows
        .lock()
        .flows_for(&pids)
        .into_iter()
        .map(|(k, pid)| FlowView {
            protocol: match k.protocol {
                PROTO_TCP => "TCP".into(),
                PROTO_UDP => "UDP".into(),
                p => format!("IP/{p}"),
            },
            local: format!("{}:{}", addr_to_string(&k.local), k.local_port),
            remote: format!("{}:{}", addr_to_string(&k.remote), k.remote_port),
            pid,
        })
        .collect();
    flows.sort_by(|a, b| a.remote.cmp(&b.remote));
    flows.truncate(200);
    flows
}

/// Usage statistics for "day" (today, hourly), "week" (7 days) or "month"
/// (30 days), both daily. `key` narrows the time series to one app.
#[tauri::command]
pub fn get_stats(engine: State<'_, Engine>, range: String, key: Option<String>) -> crate::engine::usage::Stats {
    use crate::clock::{now_ms, period_start, to_local, to_utc, DAY_MS, HOUR_MS};
    let now = now_ms();
    let today = period_start(now, "day");
    let (from, to, bucket) = match range.as_str() {
        "week" => (today.saturating_sub(6 * DAY_MS), today + DAY_MS, DAY_MS),
        "month" => (today.saturating_sub(29 * DAY_MS), today + DAY_MS, DAY_MS),
        _ => (today, today + DAY_MS, HOUR_MS),
    };
    // Daily buckets must follow local midnights, which `stats` aligns to
    // `from`; `from` is already a local midnight expressed in UTC.
    let _ = (to_local, to_utc);
    engine.state.usage.lock().stats(from, to, bucket, key.as_deref())
}

#[tauri::command]
pub fn switch_profile(engine: State<'_, Engine>, name: String) -> Result<Config, String> {
    let mut cfg = engine.state.config.read().clone();
    cfg.switch_profile(&name);
    engine.set_config(cfg)
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RulesFile {
    app: String,
    kind: String,
    version: u32,
    exported_at: String,
    rules: crate::config::RuleSet,
    #[serde(default)]
    profiles: Vec<crate::config::Profile>,
}

/// Save dialog + write the live rules and every profile. Returns the path,
/// or None when cancelled.
#[tauri::command]
pub async fn export_rules(window: tauri::WebviewWindow, engine: State<'_, Engine>) -> Result<Option<String>, String> {
    let cfg = engine.state.config.read().clone();
    let lang = cfg.lang();
    let hwnd = window.hwnd().map(|h| h.0 as _).unwrap_or(std::ptr::null_mut());
    let owner = hwnd as usize;
    let filter = crate::i18n::tr(lang, "dlg.rules").to_string();
    let path = tauri::async_runtime::spawn_blocking(move || crate::dialogs::save_file(owner as _, &filter, "bandwidth-limiter-rules.json"))
        .await
        .map_err(|e| e.to_string())?;
    let Some(path) = path else { return Ok(None) };
    let file = RulesFile {
        app: "Bandwidth Limiter".into(),
        kind: "rules".into(),
        version: 1,
        exported_at: chrono_like_now(),
        rules: cfg.rule_set(),
        profiles: cfg.profiles.clone(),
    };
    std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap())
        .map_err(|e| format!("{}: {e}", crate::i18n::tr(lang, "io.write")))?;
    Ok(Some(path))
}

/// Open dialog + replace the live rules with the file's; profiles in the file
/// are merged by name. Returns the new config, or None when cancelled.
#[tauri::command]
pub async fn import_rules(window: tauri::WebviewWindow, engine: State<'_, Engine>) -> Result<Option<Config>, String> {
    let lang = engine.state.config.read().lang();
    let hwnd = window.hwnd().map(|h| h.0 as _).unwrap_or(std::ptr::null_mut());
    let owner = hwnd as usize;
    let filter = crate::i18n::tr(lang, "dlg.rules").to_string();
    let path = tauri::async_runtime::spawn_blocking(move || crate::dialogs::open_file(owner as _, &filter))
        .await
        .map_err(|e| e.to_string())?;
    let Some(path) = path else { return Ok(None) };
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", crate::i18n::tr(lang, "io.read")))?;
    let file: RulesFile = serde_json::from_slice(&bytes).map_err(|_| crate::i18n::tr(lang, "io.format").to_string())?;
    if file.kind != "rules" {
        return Err(crate::i18n::tr(lang, "io.format").into());
    }
    let mut cfg = engine.state.config.read().clone();
    cfg.set_rule_set(file.rules);
    for p in file.profiles {
        match cfg.profiles.iter_mut().find(|q| q.name.eq_ignore_ascii_case(&p.name)) {
            Some(q) => q.rules = p.rules,
            None => cfg.profiles.push(p),
        }
    }
    // The imported live rules are not "from" any profile any more.
    cfg.active_profile.clear();
    engine.set_config(cfg).map(Some)
}

#[tauri::command]
pub async fn test_notification(app: tauri::AppHandle, engine: State<'_, Engine>) -> Result<(), String> {
    let lang = engine.state.config.read().lang();
    tauri::async_runtime::spawn_blocking(move || crate::notify::show_blocking(&app, "Bandwidth Limiter", crate::i18n::tr(lang, "notif.test")))
        .await
        .map_err(|e| e.to_string())?
}

/// ISO-8601 UTC timestamp without pulling in a date crate.
fn chrono_like_now() -> String {
    let ms = crate::clock::now_ms();
    let days = (ms / crate::clock::DAY_MS) as i64;
    let (y, m, d) = crate::clock::civil_from_days(days);
    let rem = ms % crate::clock::DAY_MS;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3_600_000, (rem / 60_000) % 60, (rem / 1000) % 60)
}
