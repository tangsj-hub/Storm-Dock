mod apps;
mod commands;
mod cursor;
mod error;
mod models;
mod store;
mod tray;

use std::sync::Mutex;

use tauri::{menu::Menu, tray::TrayIconBuilder, Emitter, Manager};

use crate::cursor::oauth::OauthLoginState;
use crate::error::AppError;
use crate::store::{AppState, Controller};
use crate::tray::{build_tray_menu, refresh_tray};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
                    } else if let Some(account_id) = id.strip_prefix("switch:") {
                        let result =
                            app.state::<AppState>()
                                .0
                                .lock()
                                .ok()
                                .and_then(|mut controller| {
                                    controller.switch_account(account_id, |_, _| {}).ok()
                                });
                        if result.is_some() {
                            refresh_tray(app);
                            let _ = app.emit("accounts-changed", ());
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_applications,
            commands::list_cursor_plugins,
            commands::list_codex_plugins,
            commands::set_codex_plugin_enabled,
            commands::set_codex_plugin_capability_enabled,
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
            commands::force_restart_cursor
        ])
        .run(tauri::generate_context!())
        .expect("error while running storm-dock");
}
