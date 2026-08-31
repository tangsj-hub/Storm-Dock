use std::path::PathBuf;

use crate::apps::ApplicationAdapter;
use crate::codex::{
    auth, config,
    session::{self, CODEX_AUTH_KEY},
};
use crate::error::{AppError, Result};
use crate::models::{ApplicationKind, ApplicationStatus, Session};

pub(crate) struct CodexAdapter {
    pub(crate) auth_path: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self {
            auth_path: auth::default_auth_path(),
            config_path: config::default_config_path(),
        }
    }
}

impl CodexAdapter {
    fn auth_path(&self) -> Result<&PathBuf> {
        self.auth_path.as_ref().ok_or(AppError::CodexNotDetected)
    }

    fn config_path(&self) -> Result<&PathBuf> {
        self.config_path.as_ref().ok_or(AppError::CodexNotDetected)
    }
}

impl ApplicationAdapter for CodexAdapter {
    fn kind(&self) -> ApplicationKind {
        ApplicationKind::Codex
    }

    fn detect(&self) -> ApplicationStatus {
        let available = self
            .auth_path
            .as_ref()
            .and_then(|path| auth::read_auth(path).ok())
            .is_some_and(|auth| session::has_login_material(&auth));
        ApplicationStatus {
            kind: self.kind(),
            label: self.kind().display_name().into(),
            available,
            reason: (!available).then(|| "未检测到 ChatGPT 登录。".into()),
        }
    }

    fn import_current(&self) -> Result<Session> {
        let auth = auth::read_auth(self.auth_path()?)?;
        if !session::has_login_material(&auth) {
            return Err(AppError::CodexNotDetected);
        }
        let base_url = self
            .config_path
            .as_ref()
            .and_then(|path| config::read_text(path).ok())
            .and_then(|text| config::active_base_url(&text));
        session::session_from_auth(auth, base_url)
    }

    fn apply(&self, session: &Session) -> Result<()> {
        let next_auth = session::auth_value(session)?;
        if !session::has_login_material(&next_auth) {
            return Err(AppError::SecretMissing);
        }
        let auth_path = self.auth_path()?;
        let config_path = self.config_path()?;
        let previous_auth = auth::read_bytes(auth_path);
        let previous_config = auth::read_bytes(config_path);
        auth::write_auth(auth_path, &next_auth)?;
        let config_text = config::apply_base_url(
            &config::read_text(config_path).unwrap_or_default(),
            session::base_url(session).as_deref(),
        );
        let config_text = match config_text {
            Ok(text) => text,
            Err(error) => {
                let _ = auth::restore_bytes(auth_path, previous_auth.as_deref());
                return Err(error);
            }
        };
        if let Err(error) = config::write_text(config_path, &config_text) {
            let _ = auth::restore_bytes(auth_path, previous_auth.as_deref());
            return Err(error);
        }
        let verified = auth::read_auth(auth_path)
            .ok()
            .and_then(|current| {
                let saved = session
                    .values
                    .get(CODEX_AUTH_KEY)
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())?;
                Some(
                    session::chatgpt_account_id(&current) == session::chatgpt_account_id(&saved)
                        && session::api_key(&current) == session::api_key(&saved),
                )
            })
            .unwrap_or(false);
        if verified {
            return Ok(());
        }
        let restored_auth = auth::restore_bytes(auth_path, previous_auth.as_deref());
        let restored_config = auth::restore_bytes(config_path, previous_config.as_deref());
        if restored_auth.is_err() || restored_config.is_err() {
            return Err(AppError::RestoreFailed);
        }
        Err(AppError::VerifyFailed)
    }

    fn is_running(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    fn test_adapter() -> (CodexAdapter, PathBuf, PathBuf) {
        let id = uuid::Uuid::new_v4();
        let auth_path = env::temp_dir().join(format!("storm-dock-codex-{id}.json"));
        let config_path = env::temp_dir().join(format!("storm-dock-codex-{id}.toml"));
        let _ = fs::remove_file(&auth_path);
        let _ = fs::remove_file(&config_path);
        (
            CodexAdapter {
                auth_path: Some(auth_path.clone()),
                config_path: Some(config_path.clone()),
            },
            auth_path,
            config_path,
        )
    }

    #[test]
    fn imports_and_switches_api_key_sessions() {
        let (adapter, auth_path, config_path) = test_adapter();
        crate::codex::auth::write_auth(
            &auth_path,
            &serde_json::json!({ "OPENAI_API_KEY": "sk-original" }),
        )
        .unwrap();
        let imported = adapter.import_current().unwrap();
        assert_eq!(session::import_type(&imported), crate::models::ImportType::ApiKey);

        let next = session::from_import(
            r#"{"OPENAI_API_KEY":"sk-next","base_url":"https://api.example.com/v1"}"#,
        )
        .unwrap();
        adapter.apply(&next).unwrap();
        assert_eq!(
            adapter.import_current().unwrap().values.get(CODEX_AUTH_KEY),
            next.values.get(CODEX_AUTH_KEY)
        );
        assert!(fs::read_to_string(&config_path)
            .unwrap()
            .contains("https://api.example.com/v1"));
        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn rejects_empty_session_without_writing() {
        let (adapter, auth_path, config_path) = test_adapter();
        crate::codex::auth::write_auth(
            &auth_path,
            &serde_json::json!({ "OPENAI_API_KEY": "sk-original" }),
        )
        .unwrap();
        let error = adapter.apply(&Session {
            values: Default::default(),
            raw_export: None,
        });
        assert!(matches!(error, Err(AppError::SecretMissing)));
        assert_eq!(
            crate::codex::auth::read_auth(&auth_path).unwrap()["OPENAI_API_KEY"],
            "sk-original"
        );
        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }
}
