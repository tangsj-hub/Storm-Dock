use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::{AppError, Result};

pub(crate) const AUTH_FILE_NAME: &str = "workbuddy-desktop.info";
pub(crate) const LOGOUT_MARKER_NAME: &str = "workbuddy-desktop.logout-marker";

pub(crate) fn default_auth_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| {
            home.join("Library/Application Support/CodeBuddyExtension/Data/Public/auth")
        })
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .map(|base| base.join("CodeBuddyExtension/Data/Public/auth"))
            .or_else(|| {
                dirs::home_dir().map(|home| {
                    home.join("AppData/Local/CodeBuddyExtension/Data/Public/auth")
                })
            })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        dirs::data_local_dir()
            .map(|base| base.join("CodeBuddyExtension/Data/Public/auth"))
            .or_else(|| {
                dirs::home_dir()
                    .map(|home| home.join(".local/share/CodeBuddyExtension/Data/Public/auth"))
            })
    }
}

pub(crate) fn default_auth_path() -> Option<PathBuf> {
    default_auth_dir().map(|dir| dir.join(AUTH_FILE_NAME))
}

pub(crate) fn logout_marker_path(auth_path: &Path) -> PathBuf {
    auth_path
        .parent()
        .map(|parent| parent.join(LOGOUT_MARKER_NAME))
        .unwrap_or_else(|| auth_path.with_file_name(LOGOUT_MARKER_NAME))
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
    let marker = logout_marker_path(path);
    if marker.exists() {
        let _ = fs::remove_file(marker);
    }
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
    fn writes_and_clears_logout_marker() {
        let dir = env::temp_dir().join(format!("storm-dock-wb-auth-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(AUTH_FILE_NAME);
        let marker = logout_marker_path(&path);
        fs::write(&marker, "logout").unwrap();
        let profile = serde_json::json!({
            "account": { "uid": "u1", "nickname": "Neo" },
            "auth": { "accessToken": "tok", "tokenType": "Bearer", "domain": "www.workbuddy.cn" }
        });
        write_profile(&path, &profile).unwrap();
        assert!(!marker.exists());
        assert_eq!(read_profile(&path).unwrap()["account"]["uid"], "u1");
        let _ = fs::remove_dir_all(&dir);
    }
}
