//! macOS helpers that run in-process so system prompts name Storm Dock.
#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};

/// Read a generic-password keychain item by service name via Security.framework.
pub(crate) fn generic_password_for_service(service: &str) -> Option<String> {
    let results = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(service)
        .load_data(true)
        .search()
        .ok()?;
    for item in results {
        let SearchResult::Data(bytes) = item else {
            continue;
        };
        let password = String::from_utf8(bytes).ok()?;
        let password = password.trim().to_owned();
        if !password.is_empty() {
            return Some(password);
        }
    }
    None
}

/// Open Terminal with a shell command without going through `osascript`.
pub(crate) fn launch_terminal_command(command: &str) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("storm-dock-{stamp}.command"));
    let body = format!("#!/bin/zsh\nset -e\n{command}\nexec \"$SHELL\" -l\n");
    fs::write(&path, body).map_err(|e| e.to_string())?;
    let mut perms = fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    let path_str = path.to_str().ok_or_else(|| "路径无效。".to_string())?;
    let status = Command::new("open")
        .args(["-a", "Terminal", path_str])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("无法启动终端会话。".into())
    }
}

/// Locate an .app by bundle id via Spotlight metadata (no Automation prompt).
pub(crate) fn app_path_for_bundle_id(bundle_id: &str) -> Option<PathBuf> {
    let output = Command::new("mdfind")
        .arg(format!("kMDItemCFBundleIdentifier == '{bundle_id}'"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.is_dir() && path.extension().and_then(|e| e.to_str()) == Some("app"))
}

