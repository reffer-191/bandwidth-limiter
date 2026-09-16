//! In-app updates through GitHub releases (`latest.json` produced by CI and
//! signed with the project's minisign key; see `plugins.updater` in
//! tauri.conf.json).

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::engine::Engine;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

/// Checks the release feed. `Ok(None)` means we are up to date.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| friendly(e.to_string()))?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    }))
}

/// Downloads and installs the pending update. On Windows this launches the
/// signed NSIS installer in passive mode and exits the application.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| friendly(e.to_string()))? else {
        return Err("No hay ninguna actualización pendiente".into());
    };
    // Release the capture driver first so the installer can replace the files.
    if let Some(engine) = app.try_state::<Engine>() {
        engine.stop();
    }
    let handle = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = handle.emit("update-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| friendly(e.to_string()))?;
    Ok(())
}

/// Background check a few seconds after startup; the UI shows a banner.
pub fn check_on_startup(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(8).await;
        if let Ok(Some(info)) = check(&app).await {
            let _ = app.emit("update-available", info);
        }
    });
}

async fn tokio_sleep(secs: u64) {
    tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_secs(secs)))
        .await
        .ok();
}

fn friendly(e: String) -> String {
    if e.contains("Could not fetch a valid release JSON") || e.contains("404") {
        "No se pudo leer la lista de versiones (¿sin conexión?)".into()
    } else if e.contains("signature") {
        "La firma de la actualización no es válida".into()
    } else {
        e
    }
}
