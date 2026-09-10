use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    AppHandle, Manager, Runtime,
};

use crate::store::AppState;

pub(crate) fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let controller = state.0.lock().expect("controller mutex");
    let title = MenuItem::with_id(
        app,
        "status",
        format!("Cursor: {}", controller.current_label()),
        false,
        None::<&str>,
    )?;
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&title];
    let separator = PredefinedMenuItem::separator(app)?;
    items.push(&separator);
    let open = MenuItem::with_id(app, "open", "打开 Storm Dock", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    items.push(&open);
    items.push(&quit);
    Menu::with_items(app, &items)
}

pub(crate) fn refresh_tray(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build_tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            let _ = app.set_dock_visibility(true);
        }
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_skip_taskbar(false);
        }
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn hide_main_window_to_tray<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        #[cfg(target_os = "macos")]
        {
            let _ = app.set_dock_visibility(false);
        }
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_skip_taskbar(true);
        }
    }
}
