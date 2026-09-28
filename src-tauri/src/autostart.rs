//! Launch-at-login registration.
//!
//! On Windows, `tauri-plugin-autostart` / `auto-launch` write the executable
//! path into `HKCU\...\Run` **without quotes**. Storm Dock installs to paths
//! with spaces (`...\Storm Dock\Storm Dock.exe`), so Windows truncates the
//! command at the first space and never starts the app — while the Settings
//! toggle still appears enabled because the registry value exists.
//!
//! This module keeps macOS/Linux on `auto-launch` (same as the old plugin)
//! and registers a properly quoted command on Windows.

#[cfg(not(windows))]
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

/// Registry / LaunchAgent entry name shown in OS startup UIs.
const APP_NAME: &str = "Storm Dock";

/// Names previously written by `tauri-plugin-autostart` (Cargo package name).
#[cfg(windows)]
const LEGACY_APP_NAMES: &[&str] = &["storm-dock"];

#[cfg(windows)]
const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
const STARTUP_APPROVED_KEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// Task Manager "enabled" blob (12 bytes; last 8 are zeros).
#[cfg(windows)]
const STARTUP_APPROVED_ENABLED: [u8; 12] = [
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Quote a Windows executable path for use in `HKCU\...\Run`.
#[cfg(any(windows, test))]
pub(crate) fn quote_windows_exe_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('"') {
        return trimmed.to_string();
    }
    if trimmed.chars().any(|c| c.is_whitespace()) {
        format!("\"{trimmed}\"")
    } else {
        trimmed.to_string()
    }
}

#[cfg(windows)]
fn windows_exe_path() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
    Ok(exe.display().to_string())
}

#[cfg(windows)]
fn windows_run_command() -> Result<String, String> {
    Ok(quote_windows_exe_path(&windows_exe_path()?))
}

#[cfg(windows)]
fn delete_run_value(name: &str) {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
        let _ = key.delete_value(name);
    }
    if let Ok(key) = hkcu.open_subkey_with_flags(STARTUP_APPROVED_KEY, KEY_SET_VALUE) {
        let _ = key.delete_value(name);
    }
}

#[cfg(windows)]
fn cleanup_legacy_entries() {
    for name in LEGACY_APP_NAMES {
        delete_run_value(name);
    }
}

#[cfg(not(windows))]
fn cleanup_legacy_entries() {}

#[cfg(windows)]
fn enable_windows() -> Result<(), String> {
    use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::{RegKey, RegValue};

    cleanup_legacy_entries();
    let cmd = windows_run_command()?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .map_err(|e| format!("open Run key: {e}"))?
        .set_value(APP_NAME, &cmd)
        .map_err(|e| format!("set Run value: {e}"))?;

    if let Ok(key) = hkcu.open_subkey_with_flags(STARTUP_APPROVED_KEY, KEY_SET_VALUE) {
        let _ = key.set_raw_value(
            APP_NAME,
            &RegValue {
                vtype: RegType::REG_BINARY,
                bytes: STARTUP_APPROVED_ENABLED.to_vec(),
            },
        );
    }
    Ok(())
}

#[cfg(windows)]
fn disable_windows() -> Result<(), String> {
    cleanup_legacy_entries();
    delete_run_value(APP_NAME);
    Ok(())
}

#[cfg(windows)]
fn is_enabled_windows() -> Result<bool, String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_enabled = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .map_err(|e| format!("open Run key: {e}"))?
        .get_value::<String, _>(APP_NAME)
        .is_ok();

    if !run_enabled {
        return Ok(false);
    }

    // Mirror auto-launch: honor Task Manager disable via StartupApproved.
    let approved = hkcu
        .open_subkey_with_flags(STARTUP_APPROVED_KEY, KEY_READ)
        .ok()
        .and_then(|key| key.get_raw_value(APP_NAME).ok());
    if let Some(value) = approved {
        if value.bytes.len() >= 8 {
            let enabled = value.bytes.iter().rev().take(8).all(|b| *b == 0);
            return Ok(enabled);
        }
    }
    Ok(true)
}

#[cfg(not(windows))]
fn current_exe_path() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;

    #[cfg(target_os = "macos")]
    {
        // Match tauri-plugin-autostart LaunchAgent behavior: register the
        // Mach-O path (not the .app bundle).
        return Ok(exe.canonicalize().unwrap_or(exe).display().to_string());
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(appimage) = std::env::var("APPIMAGE") {
            if !appimage.is_empty() {
                return Ok(appimage);
            }
        }
        return Ok(exe.display().to_string());
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = exe;
        Err("launch at login is not supported on this platform".into())
    }
}

#[cfg(not(windows))]
fn build_auto_launch() -> Result<AutoLaunch, String> {
    let app_path = current_exe_path()?;
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name(APP_NAME).set_app_path(&app_path);

    #[cfg(target_os = "macos")]
    builder.set_use_launch_agent(true);

    builder
        .build()
        .map_err(|e| format!("create AutoLaunch: {e}"))
}


/// Repair broken / legacy Windows Run entries written by tauri-plugin-autostart
/// (unquoted paths, or the old `storm-dock` value name).
#[cfg(windows)]
pub(crate) fn migrate_windows_autostart() {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ) else {
        return;
    };

    let has_legacy = LEGACY_APP_NAMES
        .iter()
        .any(|name| key.get_value::<String, _>(name).is_ok());
    let needs_repair = key
        .get_value::<String, _>(APP_NAME)
        .ok()
        .map(|cmd| {
            let trimmed = cmd.trim();
            !trimmed.starts_with('"') && trimmed.chars().any(char::is_whitespace)
        })
        .unwrap_or(false);

    if has_legacy || needs_repair {
        let _ = enable_windows();
    }
}

#[cfg(not(windows))]
pub(crate) fn migrate_windows_autostart() {}

#[tauri::command]
pub(crate) fn enable_launch_at_login() -> Result<(), String> {
    #[cfg(windows)]
    {
        return enable_windows();
    }
    #[cfg(not(windows))]
    {
        cleanup_legacy_entries();
        build_auto_launch()?
            .enable()
            .map_err(|e| format!("enable launch at login: {e}"))
    }
}

#[tauri::command]
pub(crate) fn disable_launch_at_login() -> Result<(), String> {
    #[cfg(windows)]
    {
        return disable_windows();
    }
    #[cfg(not(windows))]
    {
        cleanup_legacy_entries();
        let auto = build_auto_launch()?;
        match auto.is_enabled() {
            Ok(true) => auto
                .disable()
                .map_err(|e| format!("disable launch at login: {e}")),
            Ok(false) => Ok(()),
            Err(_) => {
                let _ = auto.disable();
                Ok(())
            }
        }
    }
}

#[tauri::command]
pub(crate) fn is_launch_at_login_enabled() -> Result<bool, String> {
    #[cfg(windows)]
    {
        return is_enabled_windows();
    }
    #[cfg(not(windows))]
    {
        build_auto_launch()?
            .is_enabled()
            .map_err(|e| format!("check launch at login: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::quote_windows_exe_path;

    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(
            quote_windows_exe_path(r"C:\Users\a\AppData\Local\Programs\Storm Dock\Storm Dock.exe"),
            r#""C:\Users\a\AppData\Local\Programs\Storm Dock\Storm Dock.exe""#
        );
    }

    #[test]
    fn leaves_paths_without_spaces_alone() {
        assert_eq!(
            quote_windows_exe_path(r"C:\Apps\storm-dock.exe"),
            r"C:\Apps\storm-dock.exe"
        );
    }

    #[test]
    fn does_not_double_quote() {
        assert_eq!(
            quote_windows_exe_path(r#""C:\Storm Dock\Storm Dock.exe""#),
            r#""C:\Storm Dock\Storm Dock.exe""#
        );
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            quote_windows_exe_path("  C:\\Storm Dock\\app.exe  "),
            r#""C:\Storm Dock\app.exe""#
        );
    }
}
