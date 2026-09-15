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
pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    crate::autostart::set_enabled(enabled)
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
