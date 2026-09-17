//! Windows toast notifications (new app seen, quota reached, rule
//! activated). Unpackaged apps need an AppUserModelID registered under HKCU
//! for the toast to show with our name and icon; that is done once at start.

use std::path::PathBuf;

use tauri::AppHandle;
use tauri_winrt_notification::{Duration, IconCrop, Sound, Toast};

pub const AUMID: &str = "com.fer.bandwidthlimiter";

fn icon_path() -> PathBuf {
    crate::config::Config::path().with_file_name("notification-icon.png")
}

/// Writes the icon next to the configuration and registers the AUMID
/// (display name + icon) so toasts are attributed to "Bandwidth Limiter".
pub fn register() {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ};

    let icon = icon_path();
    if !icon.exists() {
        if let Some(dir) = icon.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&icon, include_bytes!("../icons/128x128.png"));
    }
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let sub = wide(&format!("Software\\Classes\\AppUserModelId\\{AUMID}"));
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let r = RegCreateKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, std::ptr::null(), &mut key, std::ptr::null_mut());
        if r != 0 {
            log::warn!("notify: could not register AUMID ({r})");
            return;
        }
        for (name, value) in [("DisplayName", "Bandwidth Limiter".to_string()), ("IconUri", icon.display().to_string()), ("IconBackgroundColor", "FF0A84FF".to_string())] {
            let n = wide(name);
            let v = wide(&value);
            RegSetValueExW(key, n.as_ptr(), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32);
        }
        RegCloseKey(key);
    }
}

/// Shows a toast; clicking it brings the main window back. Runs on its own
/// thread because WinRT calls are not cheap enough for the ticker.
pub fn show(app: &AppHandle, title: &str, body: &str) {
    let app = app.clone();
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::Builder::new()
        .name("bwl-toast".into())
        .spawn(move || {
            if let Err(e) = show_blocking(&app, &title, &body) {
                log::warn!("notify: {e}");
            }
        })
        .ok();
}

pub fn show_blocking(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    let app = app.clone();
    let icon = icon_path();
    let mut toast = Toast::new(AUMID).title(title).text1(body).duration(Duration::Short).sound(Some(Sound::Default));
    if icon.exists() {
        toast = toast.icon(&icon, IconCrop::Square, "Bandwidth Limiter");
    }
    toast
        .on_activated(move |_| {
            crate::tray::show_main(&app);
            Ok(())
        })
        .show()
        .map_err(|e| format!("{e:?}"))
}
