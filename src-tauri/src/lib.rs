mod apps;
mod codex;
mod codex_sessions;
mod commands;
mod cursor;
mod cursor_sessions;
mod error;
mod grok;
mod grok_sessions;
mod grok_bot;
mod grok_bot_sessions;
mod models;
mod sql_backup;
mod store;
mod tools;
mod tray;
mod window_chrome;
mod updater;

use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Mutex,
};

use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tauri_plugin_dialog::DialogExt;

use crate::cursor::oauth::OauthLoginState;
use crate::error::AppError;
use crate::store::{AppState, Controller};
use crate::tray::{build_tray_menu, show_main_window};

const CLOSE_ASK: u8 = 0;
const CLOSE_TRAY: u8 = 1;
const CLOSE_QUIT: u8 = 2;

static CLOSE_BEHAVIOR: AtomicU8 = AtomicU8::new(CLOSE_ASK);
static CLOSE_ASK_OPEN: AtomicBool = AtomicBool::new(false);

fn parse_close_behavior(value: &str) -> u8 {
    match value.trim().to_ascii_lowercase().as_str() {
        "tray" => CLOSE_TRAY,
        "quit" => CLOSE_QUIT,
        _ => CLOSE_ASK,
    }
}

fn close_behavior_name(code: u8) -> &'static str {
    match code {
        CLOSE_TRAY => "tray",
        CLOSE_QUIT => "quit",
        _ => "ask",
    }
}

fn close_behavior_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("close_behavior"))
}

fn load_close_behavior(app: &tauri::AppHandle) {
    if let Some(path) = close_behavior_path(app) {
        if let Ok(raw) = std::fs::read_to_string(path) {
            CLOSE_BEHAVIOR.store(parse_close_behavior(&raw), Ordering::Relaxed);
        }
    }
}

fn persist_close_behavior(app: &tauri::AppHandle, behavior: &str) {
    if let Some(path) = close_behavior_path(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, behavior);
    }
}

fn store_close_behavior(app: &tauri::AppHandle, behavior: &str) {
    let code = parse_close_behavior(behavior);
    CLOSE_BEHAVIOR.store(code, Ordering::Relaxed);
    persist_close_behavior(app, close_behavior_name(code));
}

#[tauri::command]
fn set_close_behavior(app: tauri::AppHandle, behavior: String) {
    store_close_behavior(&app, &behavior);
}

#[tauri::command]
fn set_close_to_tray(app: tauri::AppHandle, enabled: bool) {
    store_close_behavior(&app, if enabled { "tray" } else { "quit" });
}

#[tauri::command]
fn confirm_close_action(app: tauri::AppHandle, action: String) {
    CLOSE_ASK_OPEN.store(false, Ordering::Relaxed);
    match action.trim().to_ascii_lowercase().as_str() {
        "tray" => crate::tray::hide_main_window_to_tray(&app),
        "quit" => app.exit(0),
        _ => {}
    }
}

#[tauri::command]
fn sync_window_chrome(window: tauri::Window) {
    crate::window_chrome::apply(&window);
}

struct DialogLabels {
    title: &'static str,
    message: &'static str,
    tray: &'static str,
    quit: &'static str,
    cancel: &'static str,
}

fn dialog_labels() -> DialogLabels {
    let lang = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lang.starts_with("en") {
        DialogLabels {
            title: "Close Storm Dock",
            message: "Minimize to the menu bar / system tray, or quit the app?",
            tray: "Minimize to tray",
            quit: "Quit",
            cancel: "Cancel",
        }
    } else {
        DialogLabels {
            title: "关闭 Storm Dock",
            message: "最小化到菜单栏 / 系统托盘，还是退出应用？",
            tray: "最小化到托盘",
            quit: "退出",
            cancel: "取消",
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        // Must be registered first so a second launch notifies this process
        // instead of opening another window (Windows / Linux / macOS CLI).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if window.label() != "main" {
                        return;
                    }
                    match CLOSE_BEHAVIOR.load(Ordering::Relaxed) {
                        CLOSE_QUIT => {}
                        CLOSE_TRAY => {
                            api.prevent_close();
                            crate::tray::hide_main_window_to_tray(window.app_handle());
                        }
                        _ => {
                            api.prevent_close();
                            if CLOSE_ASK_OPEN
                                .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                                .is_err()
                            {
                                return;
                            }
                            let app = window.app_handle().clone();
                            if window.emit("close-requested-ask", ()).is_err() {
                                CLOSE_ASK_OPEN.store(false, Ordering::Relaxed);
                                let labels = dialog_labels();
                                let app_for_dialog = app.clone();
                                let tray_label = labels.tray.to_string();
                                let quit_label = labels.quit.to_string();
                                app.dialog()
                                    .message(labels.message)
                                    .title(labels.title)
                                    .buttons(
                                        tauri_plugin_dialog::MessageDialogButtons::YesNoCancelCustom(
                                            labels.tray.into(),
                                            labels.quit.into(),
                                            labels.cancel.into(),
                                        ),
                                    )
                                    .show_with_result(move |result| {
                                        CLOSE_ASK_OPEN.store(false, Ordering::Relaxed);
                                        let choice = match result {
                                            tauri_plugin_dialog::MessageDialogResult::Yes => "tray",
                                            tauri_plugin_dialog::MessageDialogResult::No => "quit",
                                            tauri_plugin_dialog::MessageDialogResult::Custom(label)
                                                if label == tray_label =>
                                            {
                                                "tray"
                                            }
                                            tauri_plugin_dialog::MessageDialogResult::Custom(label)
                                                if label == quit_label =>
                                            {
                                                "quit"
                                            }
                                            _ => "cancel",
                                        };
                                        match choice {
                                            "tray" => {
                                                crate::tray::hide_main_window_to_tray(
                                                    &app_for_dialog,
                                                );
                                                store_close_behavior(&app_for_dialog, "tray");
                                                let _ = app_for_dialog
                                                    .emit("close-behavior-changed", "tray");
                                            }
                                            "quit" => {
                                                store_close_behavior(&app_for_dialog, "quit");
                                                let _ = app_for_dialog
                                                    .emit("close-behavior-changed", "quit");
                                                app_for_dialog.exit(0);
                                            }
                                            _ => {}
                                        }
                                    });
                            }
                        }
                    }
                }
                tauri::WindowEvent::ThemeChanged(theme) => {
                    crate::window_chrome::apply_theme(window, *theme);
                }
                _ => {}
            }
        })
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| AppError::Message(error.to_string()))?;
            app.manage(AppState(Mutex::new(Controller::new(data_dir)?)));
            app.manage(OauthLoginState::default());
            load_close_behavior(app.handle());
            let menu = build_tray_menu(app.handle())?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let id = event.id().as_ref();
                    if id == "open" {
                        show_main_window(app);
                    } else if id == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                crate::window_chrome::apply_webview(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_applications,
            commands::list_cursor_plugins,
            commands::list_codex_plugins,
            commands::list_grok_plugins,
            commands::list_codex_sessions,
            commands::get_codex_session_messages,
            commands::delete_codex_session,
            commands::delete_codex_sessions,
            commands::launch_codex_session,
            commands::list_grok_sessions,
            commands::get_grok_session_messages,
            commands::delete_grok_session,
            commands::delete_grok_sessions,
            commands::launch_grok_session,
            commands::list_cursor_sessions,
            commands::get_cursor_session_messages,
            commands::delete_cursor_session,
            commands::delete_cursor_sessions,
            commands::set_codex_plugin_enabled,
            commands::set_codex_plugin_capability_enabled,
            commands::delete_codex_plugin,
            commands::set_grok_plugin_enabled,
            commands::delete_grok_plugin,
            commands::list_mcp_servers,
            commands::set_mcp_server_enabled,
            commands::set_cursor_plugin_enabled,
            commands::delete_cursor_plugin,
            commands::list_accounts,
            commands::get_database_path,
            commands::move_database,
            commands::export_database,
            commands::import_database,
            commands::get_preserve_codex_official_auth,
            commands::set_preserve_codex_official_auth,
            commands::export_cursor_accounts,
            commands::get_cursor_export_record,
            commands::refresh_account_subscription,
            commands::refresh_all_cursor_accounts,
            commands::refresh_all_codex_accounts,
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
            commands::prepare_launch_grok_bot,
            commands::confirm_launch_grok_bot,
            commands::get_grok_bot_status,
            commands::list_grok_bot_sessions,
            commands::get_grok_bot_session_messages,
            commands::delete_grok_bot_session,
            commands::delete_grok_bot_sessions,
            commands::rename_grok_bot_session,
            commands::get_codex_api_key_account,
            commands::update_codex_api_key_account,
            commands::duplicate_codex_api_key_account,
            commands::test_codex_api_key_account,
            set_close_behavior,
            set_close_to_tray,
            confirm_close_action,
            sync_window_chrome,
            updater::install_update_and_restart,
            tools::get_tool_versions,
            tools::run_tool_lifecycle_action,
            tools::probe_tool_installations
        ])
        .build(tauri::generate_context!())
        .expect("error while building storm-dock")
        .run(|app, event| {
            // macOS dock clicks reuse the running process and emit Reopen
            // instead of starting a second instance.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (app, event);
            }
        });
}
