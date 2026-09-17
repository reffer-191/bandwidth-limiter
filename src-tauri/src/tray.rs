//! System tray icon: show/hide the window, toggle the limiter, switch
//! profiles, quit.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::config::Config;
use crate::engine::Engine;
use crate::i18n::{tr, Lang};

pub const TRAY_ID: &str = "main";

pub struct Tray {
    pub icon: tauri::image::Image<'static>,
    pub icon_active: tauri::image::Image<'static>,
}

fn build_menu(app: &AppHandle, cfg: &Config) -> tauri::Result<Menu<Wry>> {
    let lang: Lang = cfg.lang();
    let show = MenuItem::with_id(app, "show", tr(lang, "tray.show"), true, None::<&str>)?;
    let master = CheckMenuItem::with_id(app, "master", tr(lang, "tray.master"), true, cfg.master, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr(lang, "tray.quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &master])?;
    if !cfg.profiles.is_empty() {
        let mut items: Vec<CheckMenuItem<Wry>> = Vec::new();
        for p in &cfg.profiles {
            items.push(CheckMenuItem::with_id(app, format!("profile:{}", p.name), &p.name, true, p.name == cfg.active_profile, None::<&str>)?);
        }
        let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>).collect();
        let sub = Submenu::with_items(app, tr(lang, "tray.profile"), true, &refs)?;
        menu.append(&sub)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit)?;
    Ok(menu)
}

pub fn setup(app: &AppHandle, cfg: &Config) -> tauri::Result<()> {
    let menu = build_menu(app, cfg)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Bandwidth Limiter")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "master" => toggle_master(app),
            "quit" => quit_app(app),
            id => {
                if let Some(name) = id.strip_prefix("profile:") {
                    switch_profile(app, name);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    // Explicit 32px PNGs so the tray never ends up without an icon; the
    // "active" variant carries a green dot while rules are being applied.
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
        .map_err(|e| tauri::Error::Io(std::io::Error::other(e.to_string())))?
        .to_owned();
    let icon_active = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-active.png"))
        .map_err(|e| tauri::Error::Io(std::io::Error::other(e.to_string())))?
        .to_owned();
    builder = builder.icon(icon.clone());
    builder.build(app)?;
    app.manage(Tray { icon, icon_active });
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn hide_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
}

fn toggle_master(app: &AppHandle) {
    let engine = app.state::<Engine>();
    let mut cfg = engine.state.config.read().clone();
    cfg.master = !cfg.master;
    let _ = engine.set_config(cfg);
}

fn switch_profile(app: &AppHandle, name: &str) {
    let engine = app.state::<Engine>();
    let mut cfg = engine.state.config.read().clone();
    if cfg.switch_profile(name) {
        let _ = engine.set_config(cfg);
    } else {
        // Re-sync the check marks (the click toggled one visually).
        sync(app, &engine.state.config.read());
    }
}

pub fn quit_app(app: &AppHandle) {
    if let Some(engine) = app.try_state::<Engine>() {
        engine.stop();
    }
    app.exit(0);
}

/// Rebuilds the tray menu from the configuration (called after every save:
/// language, limiter state and profiles all live there).
pub fn sync(app: &AppHandle, cfg: &Config) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = build_menu(app, cfg) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// Swaps the tray icon to the badged variant while limits are enforced.
pub fn set_active(app: &AppHandle, active: bool) {
    if let (Some(tray), Some(icon)) = (app.try_state::<Tray>(), app.tray_by_id(TRAY_ID)) {
        let img = if active { tray.icon_active.clone() } else { tray.icon.clone() };
        let _ = icon.set_icon(Some(img));
    }
}

pub fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}
