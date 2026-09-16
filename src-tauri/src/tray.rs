//! System tray icon: show/hide the window, toggle the limiter, quit.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::engine::Engine;
use crate::i18n::{tr, Lang};

pub const TRAY_ID: &str = "main";

/// Menu items we need to update later (kept in Tauri's managed state).
pub struct Tray {
    pub show: MenuItem<Wry>,
    pub master: CheckMenuItem<Wry>,
    pub quit: MenuItem<Wry>,
    pub icon: tauri::image::Image<'static>,
    pub icon_active: tauri::image::Image<'static>,
}

pub fn setup(app: &AppHandle, master: bool, lang: Lang) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", tr(lang, "tray.show"), true, None::<&str>)?;
    let master_item = CheckMenuItem::with_id(app, "master", tr(lang, "tray.master"), true, master, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr(lang, "tray.quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &master_item, &PredefinedMenuItem::separator(app)?, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Bandwidth Limiter")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "master" => toggle_master(app),
            "quit" => quit_app(app),
            _ => {}
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
    app.manage(Tray { show, master: master_item, quit, icon, icon_active });
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

pub fn quit_app(app: &AppHandle) {
    if let Some(engine) = app.try_state::<Engine>() {
        engine.stop();
    }
    app.exit(0);
}

/// Keeps the tray in sync with the configuration (called after every save).
pub fn sync(app: &AppHandle, master: bool, lang: Lang) {
    if let Some(tray) = app.try_state::<Tray>() {
        let _ = tray.master.set_checked(master);
        let _ = tray.show.set_text(tr(lang, "tray.show"));
        let _ = tray.master.set_text(tr(lang, "tray.master"));
        let _ = tray.quit.set_text(tr(lang, "tray.quit"));
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
