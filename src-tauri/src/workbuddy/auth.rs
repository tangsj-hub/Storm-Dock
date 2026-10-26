use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::{AppError, Result};

pub(crate) const AUTH_FILE_NAME: &str = "workbuddy-desktop.info";
/// Legacy mistaken name from an earlier Storm Dock build; still cleared on write.
pub(crate) const LEGACY_LOGOUT_MARKER_NAME: &str = "workbuddy-desktop.logout-marker";

/// Relative auth directory under Roaming / Local AppData (Windows) or OS data dirs.
#[cfg_attr(target_os = "macos", allow(dead_code))]
const AUTH_DIR_REL: &str = "CodeBuddyExtension/Data/Public/auth";

/// Pick Windows WorkBuddy auth dir.
///
/// Probe order (toolkit-style): **Roaming (`APPDATA`) then Local (`LOCALAPPDATA`)**.
/// Prefer a path that already has `workbuddy-desktop.info`, else an existing auth
/// directory. If neither exists yet, fall back to Local (community default), then
/// Roaming if Local is unavailable.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn pick_windows_auth_dir(
    roaming: Option<PathBuf>,
    local: Option<PathBuf>,
) -> Option<PathBuf> {
    let ordered = [roaming.clone(), local.clone()];
    for dir in ordered.iter().flatten() {
        if dir.join(AUTH_FILE_NAME).is_file() {
            return Some(dir.clone());
        }
    }
    for dir in ordered.iter().flatten() {
        if dir.is_dir() {
            return Some(dir.clone());
        }
    }
    local.or(roaming)
}

pub(crate) fn default_auth_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| {
            home.join("Library/Application Support/CodeBuddyExtension/Data/Public/auth")
        })
    }
    #[cfg(target_os = "windows")]
    {
        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(dirs::data_dir)
            .map(|base| base.join(AUTH_DIR_REL));
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(dirs::data_local_dir)
            .or_else(|| {
                dirs::home_dir().map(|home| home.join("AppData/Local"))
            })
            .map(|base| base.join(AUTH_DIR_REL));
        pick_windows_auth_dir(roaming, local)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        dirs::data_local_dir()
            .map(|base| base.join(AUTH_DIR_REL))
            .or_else(|| {
                dirs::home_dir()
                    .map(|home| home.join(".local/share/CodeBuddyExtension/Data/Public/auth"))
            })
    }
}

pub(crate) fn default_auth_path() -> Option<PathBuf> {
    default_auth_dir().map(|dir| dir.join(AUTH_FILE_NAME))
}

/// WorkBuddy's real logout marker is `${authFile}.logged-out`
/// (`getLogoutMarkerPath` in the desktop app). While that file exists the client
/// ignores a valid `workbuddy-desktop.info` and stays signed out.
pub(crate) fn logout_marker_path(auth_path: &Path) -> PathBuf {
    let mut marker = auth_path.as_os_str().to_owned();
    marker.push(".logged-out");
    PathBuf::from(marker)
}

fn legacy_logout_marker_path(auth_path: &Path) -> PathBuf {
    auth_path
        .parent()
        .map(|parent| parent.join(LEGACY_LOGOUT_MARKER_NAME))
        .unwrap_or_else(|| auth_path.with_file_name(LEGACY_LOGOUT_MARKER_NAME))
}

pub(crate) fn is_logout_marked(auth_path: &Path) -> bool {
    logout_marker_path(auth_path).exists()
}

pub(crate) fn clear_logout_marker(auth_path: &Path) {
    for marker in [logout_marker_path(auth_path), legacy_logout_marker_path(auth_path)] {
        if marker.exists() {
            let _ = fs::remove_file(marker);
        }
    }
}

pub(crate) fn read_profile(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Err(AppError::WorkbuddyNotDetected);
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    if !value.is_object() {
        return Err(AppError::Message("WorkBuddy 登录文件格式无效。".into()));
    }
    Ok(value)
}

pub(crate) fn write_profile(path: &Path, profile: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(profile)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("info.tmp.{stamp}"));
    fs::write(&temporary, &bytes)?;
    fs::rename(&temporary, path)?;
    clear_logout_marker(path);
    Ok(())
}

pub(crate) fn read_bytes(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

pub(crate) fn restore_bytes(path: &Path, previous: Option<&[u8]>) -> Result<()> {
    match previous {
        Some(bytes) => {
            fs::write(path, bytes)?;
            Ok(())
        }
        None if path.exists() => {
            fs::remove_file(path)?;
            Ok(())
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn logout_marker_matches_workbuddy_suffix() {
        let path = PathBuf::from("/tmp/workbuddy-desktop.info");
        assert_eq!(
            logout_marker_path(&path),
            PathBuf::from("/tmp/workbuddy-desktop.info.logged-out")
        );
    }

    #[test]
    fn writes_and_clears_logout_marker() {
        let dir = env::temp_dir().join(format!("storm-dock-wb-auth-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(AUTH_FILE_NAME);
        let marker = logout_marker_path(&path);
        let legacy = legacy_logout_marker_path(&path);
        fs::write(&marker, "627e5113-3fdc-45e4-ab30-fa17978271e7").unwrap();
        fs::write(&legacy, "legacy").unwrap();
        assert!(is_logout_marked(&path));
        let profile = serde_json::json!({
            "account": { "uid": "u1", "nickname": "Neo" },
            "auth": { "accessToken": "tok", "tokenType": "Bearer", "domain": "www.workbuddy.cn" }
        });
        write_profile(&path, &profile).unwrap();
        assert!(!marker.exists(), "WorkBuddy .logged-out marker must be removed");
        assert!(!legacy.exists(), "legacy logout-marker must also be removed");
        assert!(!is_logout_marked(&path));
        assert_eq!(read_profile(&path).unwrap()["account"]["uid"], "u1");
        let _ = fs::remove_dir_all(&dir);
    }

    fn temp_auth_root(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "storm-dock-wb-win-{}-{}",
            label,
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn windows_probe_prefers_roaming_auth_file() {
        let root = temp_auth_root("file");
        let roaming = root.join("Roaming").join(AUTH_DIR_REL);
        let local = root.join("Local").join(AUTH_DIR_REL);
        fs::create_dir_all(&roaming).unwrap();
        fs::create_dir_all(&local).unwrap();
        fs::write(roaming.join(AUTH_FILE_NAME), "{}").unwrap();
        fs::write(local.join(AUTH_FILE_NAME), "{}").unwrap();
        assert_eq!(
            pick_windows_auth_dir(Some(roaming.clone()), Some(local.clone())),
            Some(roaming)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_probe_uses_local_file_when_roaming_has_only_dir() {
        let root = temp_auth_root("local-file");
        let roaming = root.join("Roaming").join(AUTH_DIR_REL);
        let local = root.join("Local").join(AUTH_DIR_REL);
        fs::create_dir_all(&roaming).unwrap();
        fs::create_dir_all(&local).unwrap();
        fs::write(local.join(AUTH_FILE_NAME), "{}").unwrap();
        assert_eq!(
            pick_windows_auth_dir(Some(roaming), Some(local.clone())),
            Some(local)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_probe_prefers_existing_roaming_dir_over_missing_local() {
        let root = temp_auth_root("roaming-dir");
        let roaming = root.join("Roaming").join(AUTH_DIR_REL);
        let local = root.join("Local").join(AUTH_DIR_REL);
        fs::create_dir_all(&roaming).unwrap();
        // local parent may exist but auth dir itself must not
        fs::create_dir_all(local.parent().unwrap()).unwrap();
        assert_eq!(
            pick_windows_auth_dir(Some(roaming.clone()), Some(local)),
            Some(roaming)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_probe_falls_back_to_local_when_neither_exists() {
        let root = temp_auth_root("fallback");
        let roaming = root.join("Roaming").join(AUTH_DIR_REL);
        let local = root.join("Local").join(AUTH_DIR_REL);
        assert_eq!(
            pick_windows_auth_dir(Some(roaming), Some(local.clone())),
            Some(local)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_probe_falls_back_to_roaming_if_local_unavailable() {
        let root = temp_auth_root("roaming-only");
        let roaming = root.join("Roaming").join(AUTH_DIR_REL);
        assert_eq!(
            pick_windows_auth_dir(Some(roaming.clone()), None),
            Some(roaming)
        );
        let _ = fs::remove_dir_all(&root);
    }
}
