//! Windows toast notifications (new app seen, quota reached, rule
//! activated). An unpackaged app can only show toasts if a Start Menu
//! shortcut carries its AppUserModelID; the registry entry under HKCU adds
//! the display name and icon. Both are set up lazily, the first time a
//! notification is about to be shown.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tauri::AppHandle;
use tauri_winrt_notification::{Duration, IconCrop, Sound, Toast};

pub const AUMID: &str = "com.fer.bandwidthlimiter";

fn icon_path() -> PathBuf {
    crate::config::Config::path().with_file_name("notification-icon.png")
}

/// Tells the shell which AUMID this process belongs to (taskbar grouping and
/// toast attribution). Call before any window is created.
pub fn set_process_aumid() {
    let id: Vec<u16> = AUMID.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
}

/// Start Menu shortcut with the AUMID property. The installer's all-users
/// shortcut is updated in place when it points at this executable; otherwise
/// a per-user one is created (portable / dev builds).
fn ensure_shortcut() -> Result<(), String> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, IPersistFile};
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = "Bandwidth Limiter.lnk";
    let all_users = std::env::var_os("ProgramData").map(|d| Path::new(&d).join("Microsoft").join("Windows").join("Start Menu").join("Programs").join(name));
    let per_user = std::env::var_os("APPDATA").map(|d| Path::new(&d).join("Microsoft").join("Windows").join("Start Menu").join("Programs").join(name));
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let file: IPersistFile = link.cast().map_err(|e| e.to_string())?;
        // Reuse the installer's shortcut when it is ours.
        let mut target = per_user.clone().ok_or("no APPDATA")?;
        if let Some(p) = &all_users {
            if p.exists() && file.Load(&HSTRING::from(p.as_os_str()), windows::Win32::System::Com::STGM_READ).is_ok() {
                let mut buf = [0u16; 1024];
                if link.GetPath(&mut buf, std::ptr::null_mut(), 0).is_ok() {
                    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
                    let path = String::from_utf16_lossy(&buf[..len]);
                    if Path::new(&path) == exe {
                        target = p.clone();
                    }
                }
            }
        }
        link.SetPath(&HSTRING::from(exe.as_os_str())).map_err(|e| e.to_string())?;
        if let Some(dir) = exe.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(|e| e.to_string())?;
        }
        link.SetDescription(&HSTRING::from("Bandwidth Limiter")).map_err(|e| e.to_string())?;
        let store: IPropertyStore = link.cast().map_err(|e| e.to_string())?;
        // PKEY_AppUserModel_ID must be a VT_LPWSTR (CoTaskMem-allocated).
        let wide: Vec<u16> = AUMID.encode_utf16().chain(std::iter::once(0)).collect();
        let mem = CoTaskMemAlloc(wide.len() * 2) as *mut u16;
        if mem.is_null() {
            return Err("out of memory".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), mem, wide.len());
        let pv = PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_LPWSTR,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: PROPVARIANT_0_0_0 { pwszVal: windows::core::PWSTR(mem) },
                }),
            },
        };
        store.SetValue(&PKEY_AppUserModel_ID, &pv).map_err(|e| e.to_string())?;
        store.Commit().map_err(|e| e.to_string())?;
        if let Some(dir) = target.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        file.Save(&HSTRING::from(target.as_os_str()), true).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

static READY: OnceLock<Result<(), String>> = OnceLock::new();

/// Registry entry + shortcut, once per process.
fn prepare() -> Result<(), String> {
    READY
        .get_or_init(|| {
            register();
            ensure_shortcut()
        })
        .clone()
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
    prepare()?;
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
