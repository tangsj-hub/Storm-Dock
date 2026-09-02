use std::path::PathBuf;

use crate::apps::ApplicationAdapter;
use crate::codex::{auth, config, session};
use crate::error::{AppError, Result};
use crate::models::{ApplicationKind, ApplicationStatus, Session};

pub(crate) struct CodexAdapter {
    pub(crate) auth_path: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) preserve_official_auth: bool,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self {
            auth_path: auth::default_auth_path(),
            config_path: config::default_config_path(),
            preserve_official_auth: true,
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

    fn config_text(&self) -> String {
        self.config_path
            .as_ref()
            .and_then(|path| config::read_text(path).ok())
            .unwrap_or_default()
    }

    pub(crate) fn live_match_session(&self) -> Result<Session> {
        let config_text = self.config_text();
        if let Some(key) = config::experimental_bearer_token(&config_text) {
            return session::session_from_auth(
                session::api_key_auth_json(&key),
                config::active_base_url(&config_text),
            );
        }
        self.import_current()
    }

    fn live_matches_applied(&self, session: &Session, next_auth: &serde_json::Value) -> bool {
        let config_text = self.config_text();
        if session::is_chatgpt_login(next_auth) {
            let Ok(path) = self.auth_path() else {
                return false;
            };
            let Ok(current) = auth::read_auth(path) else {
                return false;
            };
            return session::chatgpt_account_id(&current) == session::chatgpt_account_id(next_auth)
                && config::experimental_bearer_token(&config_text).is_none();
        }
        config::experimental_bearer_token(&config_text) == session::api_key(next_auth)
            && config::active_base_url(&config_text).as_deref()
                == Some(session::effective_base_url(session).as_str())
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
        let base_url = if session::is_chatgpt_login(&auth) {
            None
        } else {
            config::active_base_url(&self.config_text())
        };
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
        let live_auth = auth::read_auth(auth_path).ok();
        let is_oauth = session::is_chatgpt_login(&next_auth);
        let (base_url, bearer) = if is_oauth {
            (None, None)
        } else {
            (
                Some(session::effective_base_url(session)),
                session::api_key(&next_auth),
            )
        };
        let config_text = match config::apply_provider_route(
            &config::read_text(config_path).unwrap_or_default(),
            base_url.as_deref(),
            bearer.as_deref(),
        ) {
            Ok(text) => text,
            Err(error) => return Err(error),
        };
        let skip_auth = if is_oauth {
            live_auth.as_ref().is_some_and(|live| {
                session::is_chatgpt_login(live)
                    && session::chatgpt_account_id(live) == session::chatgpt_account_id(&next_auth)
            })
        } else {
            self.preserve_official_auth && live_auth.as_ref().is_some_and(session::is_chatgpt_login)
        };
        if !skip_auth {
            auth::write_auth(auth_path, &next_auth)?;
        }
        if let Err(error) = config::write_text(config_path, &config_text) {
            let _ = auth::restore_bytes(auth_path, previous_auth.as_deref());
            return Err(error);
        }
        if self.live_matches_applied(session, &next_auth) {
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
                preserve_official_auth: true,
            },
            auth_path,
            config_path,
        )
    }

    fn oauth_session() -> Session {
        session::session_from_auth(
            session::oauth_auth_json("acct", "access-live", Some("id"), "refresh-live", "now"),
            None,
        )
        .unwrap()
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
        assert_eq!(
            session::import_type(&imported),
            crate::models::ImportType::ApiKey
        );

        let next = session::from_import(
            r#"{"OPENAI_API_KEY":"sk-next","base_url":"https://api.example.com/v1"}"#,
        )
        .unwrap();
        adapter.apply(&next).unwrap();
        assert_eq!(
            adapter
                .import_current()
                .unwrap()
                .values
                .get(session::CODEX_AUTH_KEY),
            next.values.get(session::CODEX_AUTH_KEY)
        );
        let config_text = fs::read_to_string(&config_path).unwrap();
        assert!(config_text.contains("https://api.example.com/v1"));
        assert!(config_text.contains("sk-next"));
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

    #[test]
    fn preserve_keeps_chatgpt_auth_when_switching_api_key() {
        let (adapter, auth_path, config_path) = test_adapter();
        let oauth = oauth_session();
        adapter.apply(&oauth).unwrap();
        let original_auth = fs::read(&auth_path).unwrap();

        let key = session::from_import(
            r#"{"OPENAI_API_KEY":"sk-next","base_url":"https://api.example.com/v1"}"#,
        )
        .unwrap();
        adapter.apply(&key).unwrap();
        assert_eq!(fs::read(&auth_path).unwrap(), original_auth);
        let config_text = fs::read_to_string(&config_path).unwrap();
        assert_eq!(
            crate::codex::config::experimental_bearer_token(&config_text).as_deref(),
            Some("sk-next")
        );
        assert_eq!(
            crate::codex::config::active_base_url(&config_text).as_deref(),
            Some("https://api.example.com/v1")
        );

        let imported = adapter.import_current().unwrap();
        assert_eq!(
            session::import_type(&imported),
            crate::models::ImportType::OAuth
        );
        assert!(!imported.values.contains_key(session::CODEX_BASE_URL_KEY));

        let live = adapter.live_match_session().unwrap();
        assert!(session::matches_live(&key, &live));
        assert!(!session::matches_live(&oauth, &live));

        adapter.apply(&oauth).unwrap();
        assert_eq!(fs::read(&auth_path).unwrap(), original_auth);
        let restored = fs::read_to_string(&config_path).unwrap();
        assert_eq!(
            crate::codex::config::experimental_bearer_token(&restored),
            None
        );
        assert_eq!(crate::codex::config::active_base_url(&restored), None);
        let live = adapter.live_match_session().unwrap();
        assert!(session::matches_live(&oauth, &live));
        assert!(!session::matches_live(&key, &live));

        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn preserve_writes_default_openai_url_for_key_without_base_url() {
        let (adapter, auth_path, config_path) = test_adapter();
        adapter.apply(&oauth_session()).unwrap();
        adapter
            .apply(&session::from_import("sk-default").unwrap())
            .unwrap();
        let config_text = fs::read_to_string(&config_path).unwrap();
        assert_eq!(
            crate::codex::config::experimental_bearer_token(&config_text).as_deref(),
            Some("sk-default")
        );
        assert_eq!(
            crate::codex::config::active_base_url(&config_text).as_deref(),
            Some(session::DEFAULT_OPENAI_BASE_URL)
        );
        assert!(session::is_chatgpt_login(
            &crate::codex::auth::read_auth(&auth_path).unwrap()
        ));
        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn preserve_off_overwrites_auth_json_for_api_key() {
        let (mut adapter, auth_path, config_path) = test_adapter();
        adapter.preserve_official_auth = false;
        adapter.apply(&oauth_session()).unwrap();
        adapter
            .apply(
                &session::from_import(
                    r#"{"OPENAI_API_KEY":"sk-overwrite","base_url":"https://api.example.com/v1"}"#,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            crate::codex::auth::read_auth(&auth_path).unwrap()["OPENAI_API_KEY"],
            "sk-overwrite"
        );
        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }
}
