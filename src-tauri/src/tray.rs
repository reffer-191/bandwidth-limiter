//! System tray icon: show/hide the window, toggle the limiter, quit.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::engine::Engine;

pub const TRAY_ID: &str = "main";

/// Menu items we need to update later (kept in Tauri's managed state).
pub struct Tray {
    pub master: CheckMenuItem<Wry>,
}

pub fn setup(app: &AppHandle, master: bool) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Mostrar Bandwidth Limiter", true, None::<&str>)?;
    let master_item = CheckMenuItem::with_id(app, "master", "Limitador activo", true, master, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
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
    // Explicit 32px PNG so the tray never ends up without an icon.
    match tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")) {
        Ok(icon) => builder = builder.icon(icon),
        Err(_) => {
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }
        }
    }
    builder.build(app)?;
    app.manage(Tray { master: master_item });
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
pub fn sync(app: &AppHandle, master: bool) {
    if let Some(tray) = app.try_state::<Tray>() {
        let _ = tray.master.set_checked(master);
    }
}

pub fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}
