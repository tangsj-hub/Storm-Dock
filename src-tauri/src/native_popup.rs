//! The one place that creates, positions, shows and closes native banner popups
//! (switcher reminder / preview, Bot模版 import progress).
//!
//! macOS: transparent borderless window + window-vibrancy (Popover, radius 16)
//! with the native shadow following the rounded mask, top-right under the menu bar.
//! Windows: transparent window, the page draws a solid Windows 11 toast card,
//! bottom-right above the taskbar.
//!
//! Every popup label starts with [`LABEL_PREFIX`] so theme syncing (which paints an
//! opaque window background) can skip all of them.

use std::{thread, time::Duration};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub(crate) const LABEL_PREFIX: &str = "popup-";
const WIDTH: f64 = 356.0;
const HEIGHT: f64 = 112.0;
const MARGIN: f64 = 12.0;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const RADIUS: f64 = 16.0;
/// Slide-in distance from the right, like a macOS banner.
const SLIDE: f64 = 48.0;

pub(crate) fn is_popup_label(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

/// Show (or re-create) the popup `label` with `page`. Must use a `popup-` label.
pub(crate) fn show(app: &AppHandle, label: &'static str, page: &'static str) {
    debug_assert!(is_popup_label(label), "popup labels must start with {LABEL_PREFIX}");
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(label) {
            let _ = window.close();
        }
        let mut builder = WebviewWindowBuilder::new(&handle, label, WebviewUrl::App(page.into()))
            .title("Storm Dock")
            .inner_size(WIDTH, HEIGHT)
            .decorations(false)
            .transparent(true)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            // macOS: native shadow follows the vibrancy mask. Windows: an undecorated
            // transparent window's shadow draws a square frame.
            .shadow(cfg!(target_os = "macos"));
        let mut target = None;
        if let Ok(Some(monitor)) = handle.primary_monitor() {
            let scale = monitor.scale_factor();
            let area = monitor.work_area();
            let pos = area.position.to_logical::<f64>(scale);
            let size = area.size.to_logical::<f64>(scale);
            let x = pos.x + size.width - WIDTH - MARGIN;
            let y = if cfg!(target_os = "macos") {
                pos.y + MARGIN
            } else {
                pos.y + size.height - HEIGHT - MARGIN
            };
            target = Some((x, y));
            builder = builder.position(x + SLIDE, y);
        }
        let Ok(window) = builder.build() else {
            return;
        };
        #[cfg(target_os = "macos")]
        {
            use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
            let _ = apply_vibrancy(
                &window,
                NSVisualEffectMaterial::Popover,
                Some(NSVisualEffectState::Active),
                Some(RADIUS),
            );
        }
        if let Some((x, y)) = target {
            thread::spawn(move || {
                // ~180 ms ease-out slide from the right.
                const STEPS: u32 = 12;
                for step in 1..=STEPS {
                    let t = f64::from(step) / f64::from(STEPS);
                    let eased = 1.0 - (1.0 - t).powi(3);
                    let _ = window
                        .set_position(tauri::LogicalPosition::new(x + SLIDE * (1.0 - eased), y));
                    thread::sleep(Duration::from_millis(15));
                }
            });
        }
    });
}

pub(crate) fn close(app: &AppHandle, label: &'static str) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(label) {
            let _ = window.close();
        }
    });
}

/// Debug builds only: `touch /tmp/storm-dock-preview-import` shows the import
/// progress banner with sample data (no import runs); it closes after 8 s.
#[cfg(debug_assertions)]
pub(crate) fn spawn_dev_import_preview_trigger(app: AppHandle) {
    const LABEL: &str = "popup-bot-import-preview";
    thread::spawn(move || loop {
        let flag = std::path::Path::new("/tmp/storm-dock-preview-import");
        if flag.exists() {
            let _ = std::fs::remove_file(flag);
            show(&app, LABEL, "switcher.html?mode=import&preview=1");
            let closer = app.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(8));
                close(&closer, LABEL);
            });
        }
        thread::sleep(Duration::from_secs(1));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_popup_label_shares_the_prefix() {
        assert!(is_popup_label(crate::switcher::runtime::POPUP_LABEL));
        assert!(is_popup_label(crate::bot_import_queue::WINDOW_LABEL));
        assert!(!is_popup_label("main"));
    }
}
