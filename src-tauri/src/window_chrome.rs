use tauri::{window::Color, Theme, WebviewWindow, Window};

const LIGHT_BG: Color = Color(255, 255, 255, 255);
const DARK_BG: Color = Color(32, 32, 32, 255);

pub(crate) fn apply(window: &Window) {
    apply_theme(window, window.theme().unwrap_or(Theme::Light));
}

pub(crate) fn apply_webview(window: &WebviewWindow) {
    let theme = window.theme().unwrap_or(Theme::Light);
    let dark = matches!(theme, Theme::Dark);
    let _ = window.set_background_color(Some(if dark { DARK_BG } else { LIGHT_BG }));
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        paint_caption(hwnd.0, dark);
    }
}

pub(crate) fn apply_theme(window: &Window, theme: Theme) {
    let dark = matches!(theme, Theme::Dark);
    let _ = window.set_background_color(Some(if dark { DARK_BG } else { LIGHT_BG }));
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        paint_caption(hwnd.0, dark);
    }
}

fn colorref(red: u8, green: u8, blue: u8) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

#[cfg(windows)]
fn paint_caption(hwnd: *mut core::ffi::c_void, dark: bool) {
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    let caption = if dark { colorref(32, 32, 32) } else { colorref(255, 255, 255) };
    let text = if dark { colorref(232, 234, 238) } else { colorref(32, 36, 44) };
    let immersive: i32 = i32::from(dark);
    unsafe {
        dwm_set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &immersive, 4);
        dwm_set(hwnd, DWMWA_CAPTION_COLOR, &caption, 4);
        dwm_set(hwnd, DWMWA_BORDER_COLOR, &caption, 4);
        dwm_set(hwnd, DWMWA_TEXT_COLOR, &text, 4);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(windows)]
unsafe fn dwm_set<T>(hwnd: *mut core::ffi::c_void, attr: u32, value: &T, size: u32) {
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut core::ffi::c_void,
            attr: u32,
            value: *const core::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    let _ = DwmSetWindowAttribute(hwnd, attr, (value as *const T).cast(), size);
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn SetWindowPos(
        hwnd: *mut core::ffi::c_void,
        insert_after: *mut core::ffi::c_void,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::colorref;

    #[test]
    fn page_colors_encode_as_win32_colorref() {
        assert_eq!(colorref(255, 255, 255), 0x00FF_FFFF);
        assert_eq!(colorref(32, 32, 32), 0x0020_2020);
    }
}
