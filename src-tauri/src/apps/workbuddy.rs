use std::path::PathBuf;

use crate::apps::ApplicationAdapter;
use crate::error::{AppError, Result};
use crate::models::{ApplicationKind, ApplicationStatus, Session};
use crate::workbuddy::{auth, session};

pub(crate) struct WorkbuddyAdapter {
    pub(crate) auth_path: Option<PathBuf>,
}

impl Default for WorkbuddyAdapter {
    fn default() -> Self {
        Self {
            auth_path: auth::default_auth_path(),
        }
    }
}

impl WorkbuddyAdapter {
    fn auth_path(&self) -> Result<&PathBuf> {
        self.auth_path
            .as_ref()
            .ok_or(AppError::WorkbuddyNotDetected)
    }

    pub(crate) fn live_match_session(&self) -> Result<Session> {
        self.import_current()
    }
}

impl ApplicationAdapter for WorkbuddyAdapter {
    fn kind(&self) -> ApplicationKind {
        ApplicationKind::Workbuddy
    }

    fn detect(&self) -> ApplicationStatus {
        let available = self.auth_path.as_ref().is_some_and(|path| {
            !auth::is_logout_marked(path)
                && auth::read_profile(path)
                    .ok()
                    .is_some_and(|profile| session::has_login_material(&profile))
        });
        ApplicationStatus {
            kind: self.kind(),
            label: self.kind().display_name().into(),
            available,
            reason: (!available).then(|| "未检测到 WorkBuddy 登录。".into()),
        }
    }

    fn import_current(&self) -> Result<Session> {
        // Save desktop auth as-is (may still be $wbEncrypted). Decrypt runs in a
        // background job after import so the UI never blocks on Electron.
        let path = self.auth_path()?;
        if auth::is_logout_marked(path) {
            return Err(AppError::WorkbuddyNotDetected);
        }
        let profile = auth::read_profile(path)?;
        if !session::has_login_material(&profile) {
            return Err(AppError::WorkbuddyNotDetected);
        }
        session::session_from_profile(profile, "native")
    }

    fn apply(&self, next: &Session) -> Result<()> {
        let path = self.auth_path()?;
        let profile = session::profile_value(next)?;
        if !session::has_login_material(&profile) {
            return Err(AppError::SecretMissing);
        }
        let previous = auth::read_bytes(path);
        if let Ok(current) = auth::read_profile(path) {
            if session::account_uid(&current) == session::account_uid(&profile)
                && session::account_uid(&profile).is_some()
                && session::access_token_plaintext(&current)
                    == session::access_token_plaintext(&profile)
            {
                // Same credentials already on disk — still clear the logout
                // marker so WorkBuddy does not stay signed out after a local logout.
                auth::clear_logout_marker(path);
                return Ok(());
            }
        }
        if let Err(error) = auth::write_profile(path, &profile) {
            let _ = auth::restore_bytes(path, previous.as_deref());
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    fn test_adapter() -> (WorkbuddyAdapter, PathBuf) {
        let dir = env::temp_dir().join(format!("storm-dock-wb-adapter-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(auth::AUTH_FILE_NAME);
        (
            WorkbuddyAdapter {
                auth_path: Some(path.clone()),
            },
            path,
        )
    }

    #[test]
    fn apply_after_import_creates_desktop_auth_file() {
        let (adapter, path) = test_adapter();
        let session = session::from_import(
            r#"{"account":{"uid":"u-login","nickname":"Jay"},"auth":{"accessToken":"tok-login","refreshToken":"ref-login","domain":"www.workbuddy.cn"}}"#,
        )
        .unwrap();
        adapter.apply(&session).unwrap();
        assert!(path.exists(), "workbuddy-desktop.info must exist after apply");
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(auth::AUTH_FILE_NAME));
        assert!(!auth::is_logout_marked(&path));
        let live = adapter.import_current().unwrap();
        assert!(session::matches_live(&session, &live));
        let parent = path.parent().unwrap().to_path_buf();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn apply_clears_logged_out_marker_for_same_account() {
        let (adapter, path) = test_adapter();
        let session = session::from_import(
            r#"{"account":{"uid":"u-same","nickname":"Sam"},"auth":{"accessToken":"tok-same","refreshToken":"ref-same"}}"#,
        )
        .unwrap();
        adapter.apply(&session).unwrap();
        let marker = auth::logout_marker_path(&path);
        fs::write(&marker, "logged-out-id").unwrap();
        assert!(auth::is_logout_marked(&path));
        adapter.apply(&session).unwrap();
        assert!(!marker.exists(), "apply must clear workbuddy-desktop.info.logged-out");
        assert!(!auth::is_logout_marked(&path));
        let parent = path.parent().unwrap().to_path_buf();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn detect_respects_logged_out_marker() {
        let (adapter, path) = test_adapter();
        let session = session::from_import(
            r#"{"account":{"uid":"u-out","nickname":"Out"},"auth":{"accessToken":"tok-out"}}"#,
        )
        .unwrap();
        adapter.apply(&session).unwrap();
        assert!(adapter.detect().available);
        fs::write(auth::logout_marker_path(&path), "out").unwrap();
        assert!(!adapter.detect().available);
        assert!(adapter.import_current().is_err());
        let parent = path.parent().unwrap().to_path_buf();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn imports_and_switches_profile() {
        let (adapter, path) = test_adapter();
        assert!(!adapter.detect().available);
        let session = session::from_import(
            r#"{"account":{"uid":"u-9","nickname":"Neo"},"auth":{"accessToken":"tok-9","refreshToken":"ref-9"}}"#,
        )
        .unwrap();
        adapter.apply(&session).unwrap();
        assert!(adapter.detect().available);
        let imported = adapter.import_current().unwrap();
        assert!(session::matches_live(&session, &imported));
        assert_eq!(session::uid(&imported).as_deref(), Some("u-9"));
        let parent = path.parent().unwrap().to_path_buf();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(parent);
    }
}
