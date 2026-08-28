mod apps;
mod codex_sessions;
mod commands;
mod cursor;
mod cursor_sessions;
mod error;
mod models;
mod store;
mod tray;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

use tauri::{menu::Menu, tray::TrayIconBuilder, Manager};

use crate::cursor::oauth::OauthLoginState;
use crate::error::AppError;
use crate::store::{AppState, Controller};
use crate::tray::build_tray_menu;

static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(true);

#[tauri::command]
fn set_close_to_tray(enabled: bool) {
    CLOSE_TO_TRAY.store(enabled, Ordering::Relaxed);
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if CLOSE_TO_TRAY.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .menu(Menu::default)
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| AppError::Message(error.to_string()))?;
            app.manage(AppState(Mutex::new(Controller::new(data_dir)?)));
            app.manage(OauthLoginState::default());
            let menu = build_tray_menu(app.handle())?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(true)
                .menu(&menu)
                .on_menu_event(|app, event| {
                    let id = event.id().as_ref();
                    if id == "open" {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    } else if id == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_applications,
            commands::list_cursor_plugins,
            commands::list_codex_plugins,
            commands::list_codex_sessions,
            commands::get_codex_session_messages,
            commands::delete_codex_session,
            commands::delete_codex_sessions,
            commands::launch_codex_session,
            commands::list_cursor_sessions,
            commands::get_cursor_session_messages,
            commands::delete_cursor_session,
            commands::delete_cursor_sessions,
            commands::set_codex_plugin_enabled,
            commands::set_codex_plugin_capability_enabled,
            commands::delete_codex_plugin,
            commands::list_mcp_servers,
            commands::set_cursor_plugin_enabled,
            commands::delete_cursor_plugin,
            commands::list_accounts,
            commands::get_database_path,
            commands::move_database,
            commands::export_cursor_accounts,
            commands::get_cursor_export_record,
            commands::refresh_account_subscription,
            commands::refresh_all_cursor_accounts,
            commands::get_cursor_usage,
            commands::get_saved_cursor_usage,
            commands::reorder_accounts,
            commands::import_current_account,
            commands::import_token_or_json,
            commands::start_official_login,
            commands::cancel_official_login,
            commands::open_official_login_url,
            commands::delete_account,
            commands::switch_account,
            commands::force_restart_cursor,
            set_close_to_tray
        ])
        .run(tauri::generate_context!())
        .expect("error while running storm-dock");
}
