#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod config;
mod elevate;
mod engine;
mod tray;
mod update;
mod windivert;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Release builds carry a requireAdministrator manifest; debug builds
    // re-launch themselves through UAC so `cargo tauri dev` just works.
    let skip_elevation = cfg!(debug_assertions) && std::env::var_os("BWL_NO_ELEVATE").is_some();
    if !skip_elevation && !elevate::is_elevated() && elevate::relaunch_elevated() {
        return;
    }

    let builder = tauri::Builder::default();
    // A second launch just brings the running instance back. Release only, so a
    // dev build can run side by side with an installed/portable copy.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        tray::show_main(app);
    }));
    builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let engine = engine::Engine::start(app.handle().clone());
            app.manage(engine);

            let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Bandwidth Limiter")
                .inner_size(1180.0, 760.0)
                .min_inner_size(900.0, 560.0)
                .decorations(false)
                .transparent(true)
                .shadow(true)
                .center()
                .visible(false);
            // Dev builds keep their own WebView2 profile so they never fight with
            // an installed/portable copy over the same user-data folder.
            if cfg!(debug_assertions) {
                if let Ok(dir) = app.path().app_local_data_dir() {
                    builder = builder.data_directory(dir.join("EBWebView-dev"));
                }
            }
            // Debug builds can expose the WebView2 devtools protocol for
            // automated UI testing: BWL_DEVTOOLS_PORT=9223.
            if cfg!(debug_assertions) {
                let port_file = std::env::current_exe()
                    .ok()
                    .and_then(|e| std::fs::read_to_string(e.with_file_name("devtools-port.txt")).ok())
                    .map(|s| s.trim().to_string());
                if let Some(port) = std::env::var("BWL_DEVTOOLS_PORT").ok().or(port_file) {
                    builder = builder.additional_browser_args(&format!(
                        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
                    ));
                }
            }
            let win = builder.build()?;
            #[cfg(target_os = "windows")]
            {
                // Mica backdrop (Windows 11); acrylic as a fallback.
                if window_vibrancy::apply_mica(&win, None).is_err() {
                    let _ = window_vibrancy::apply_acrylic(&win, Some((18, 18, 18, 125)));
                }
            }
            let _ = win.show();
            // Start minimized when the user asked for it or when the logon task
            // launched us (nobody wants a window popping up at login).
            let (start_minimized, minimize_to_tray, master) = {
                let engine = app.state::<engine::Engine>();
                let cfg = engine.state.config.read();
                (cfg.start_minimized, cfg.minimize_to_tray, cfg.master)
            };
            tray::setup(app.handle(), master)?;
            if app.state::<engine::Engine>().state.config.read().check_updates {
                update::check_on_startup(app.handle().clone());
            }
            if autostart::launched_by_task() || start_minimized {
                if minimize_to_tray {
                    let _ = win.hide();
                } else {
                    let _ = win.minimize();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let close_to_tray = app
                    .try_state::<engine::Engine>()
                    .map(|e| e.state.config.read().close_to_tray)
                    .unwrap_or(false);
                if close_to_tray {
                    api.prevent_close();
                    tray::hide_main(app);
                } else {
                    tray::quit_app(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_config,
            commands::set_config,
            commands::get_snapshot,
            commands::get_history,
            commands::get_adapters,
            commands::get_app_icon,
            commands::get_app_flows,
            commands::get_autostart,
            commands::set_autostart,
            commands::minimize_window,
            commands::show_window,
            commands::check_update,
            commands::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                if let Some(engine) = app.try_state::<engine::Engine>() {
                    engine.stop();
                }
            }
        });
}
