use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    AppHandle, Manager,
};

use crate::models::ApplicationKind;
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
    let mut accounts = controller.accounts(ApplicationKind::Cursor);
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&title];
    let separator = PredefinedMenuItem::separator(app)?;
    items.push(&separator);
    let switches: Vec<MenuItem<_>> = accounts
        .drain(..)
        .filter(|account| account.import_type.supports_desktop_switch())
        .map(|account| {
            MenuItem::with_id(
                app,
                format!("switch:{}", account.id),
                account.label,
                true,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    for item in &switches {
        items.push(item);
    }
    let after_switches = PredefinedMenuItem::separator(app)?;
    if !switches.is_empty() {
        items.push(&after_switches);
    }
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
