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
    let r = if engine.state.config.read().minimize_to_tray { window.hide() } else { window.minimize() };
    if let Err(e) = r {
        log::warn!("minimize: {e}");
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub version: String,
    pub windows: String,
    pub uptime_secs: u64,
    pub status: Status,
    pub queued_bytes: usize,
    pub dropped: u64,
    pub limiting: bool,
    pub apps: usize,
    pub flows_pending: bool,
    pub adapters: usize,
    pub metered: bool,
    pub config_path: String,
    pub usage_path: String,
    pub log_path: String,
    /// Last warnings/errors from the log (oldest first).
    pub problems: Vec<String>,
    /// Plain-text report ready to paste into an issue.
    pub report: String,
}

#[tauri::command]
pub fn get_diagnostics(engine: State<'_, Engine>) -> Diagnostics {
    let s = &engine.state;
    let status = s.status.lock().clone();
    let (queued_bytes, dropped, limiting) = {
        let sh = s.shaper.lock();
        (sh.queued_bytes, sh.dropped, sh.has_any_limit())
    };
    let apps = s.apps.lock().list.len();
    let flows_pending = s.flows.lock().has_pending();
    let (adapters, metered) = {
        let a = s.adapters.read();
        (a.adapters.len(), a.metered)
    };
    let cfg = s.config.read();
    let uptime_secs = s.started.elapsed().as_secs();
    let windows = windows_version();
    let problems = crate::diag::recent_problems(8);
    let tail = crate::diag::tail();
    let report = format!(
        "Bandwidth Limiter {v} — Windows {windows}\nuptime {uptime_secs}s · elevated {el} · driver {drv} ({path}) · forward {fwd}\nthreads {th} · packets {pk} · send errors {se} · restarts {rs} · queued {qb} B · dropped {dr} · limiting {lim}\nlast error: {le}\nadapters {ad} · metered {met} · apps {apps} · rules: global {g} hotspot {h} apps {ar} conn {cr} adapter {adr} · profile \"{prof}\" · count headers {ch}\nconfig {cp}\nlog {lp}\n\n--- log tail ---\n{tail}",
        v = env!("CARGO_PKG_VERSION"),
        el = status.elevated,
        drv = status.driver_ok,
        path = status.windivert_path,
        fwd = status.forward_ok,
        th = status.threads,
        pk = status.packets,
        se = status.send_errors,
        rs = status.restarts,
        qb = queued_bytes,
        dr = dropped,
        lim = limiting,
        le = status.last_error.clone().unwrap_or_else(|| "none".into()),
        ad = adapters,
        met = metered,
        g = cfg.global.is_active(),
        h = cfg.hotspot.is_active(),
        ar = cfg.apps.len(),
        cr = cfg.connections.len(),
        adr = cfg.adapters.len(),
        prof = cfg.active_profile,
        ch = cfg.count_headers,
        cp = status.config_path,
        lp = crate::diag::log_path().display(),
        tail = tail.iter().rev().take(60).cloned().collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"),
    );
    Diagnostics {
        version: env!("CARGO_PKG_VERSION").into(),
        windows,
        uptime_secs,
        queued_bytes,
        dropped,
        limiting,
        apps,
        flows_pending,
        adapters,
        metered,
        config_path: status.config_path.clone(),
        usage_path: crate::engine::usage::UsageStore::path().display().to_string(),
        log_path: crate::diag::log_path().display().to_string(),
        problems,
        report,
        status,
    }
}

#[tauri::command]
pub fn open_log_folder() -> Result<(), String> {
    let dir = crate::diag::log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe").arg(&dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// "10.0.26200" from the registry (RtlGetVersion is not in windows-sys' safe surface).
fn windows_version() -> String {
    use windows_sys::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};
    unsafe {
        let mut v: OSVERSIONINFOW = std::mem::zeroed();
        v.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if GetVersionExW(&mut v) != 0 {
            return format!("{}.{}.{}", v.dwMajorVersion, v.dwMinorVersion, v.dwBuildNumber);
        }
    }
    "?".into()
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
    use crate::clock::{now_ms, period_start, DAY_MS, HOUR_MS};
    let now = now_ms();
    let today = period_start(now, "day");
    let (from, to, bucket) = match range.as_str() {
        "week" => (today.saturating_sub(6 * DAY_MS), today + DAY_MS, DAY_MS),
        "month" => (today.saturating_sub(29 * DAY_MS), today + DAY_MS, DAY_MS),
        _ => (today, today + DAY_MS, HOUR_MS),
    };
    // Daily buckets follow local midnights: `stats` aligns to `from`, which
    // is already a local midnight expressed in UTC.
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
