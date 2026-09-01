use std::path::PathBuf;

use crate::apps::ApplicationAdapter;
use crate::error::{AppError, Result};
use crate::grok::{auth, config, session};
use crate::models::{ApplicationKind, ApplicationStatus, Session};

pub(crate) struct GrokAdapter {
    pub(crate) auth_path: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
}

impl Default for GrokAdapter {
    fn default() -> Self {
        Self {
            auth_path: auth::default_auth_path(),
            config_path: config::default_config_path(),
        }
    }
}

impl GrokAdapter {
    fn auth_path(&self) -> Result<&PathBuf> {
        self.auth_path.as_ref().ok_or(AppError::GrokNotDetected)
    }

    fn config_path(&self) -> Result<&PathBuf> {
        self.config_path.as_ref().ok_or(AppError::GrokNotDetected)
    }

    fn config_text(&self) -> String {
        self.config_path
            .as_ref()
            .and_then(|path| config::read_text(path).ok())
            .unwrap_or_default()
    }

    pub(crate) fn live_match_session(&self) -> Result<Session> {
        self.import_current()
    }

    fn live_matches_applied(&self, session: &Session, next_auth: &serde_json::Value) -> bool {
        let config_text = self.config_text();
        if session::is_official_login(next_auth) {
            let Ok(path) = self.auth_path() else {
                return false;
            };
            let Ok(current) = auth::read_auth(path) else {
                return false;
            };
            return session::user_id(&current) == session::user_id(next_auth)
                && session::user_id(next_auth).is_some()
                && config::is_official_live_config(&config_text);
        }
        config::extract_api_key(&config_text) == session::api_key(next_auth)
            && config::extract_base_url(&config_text).as_deref()
                == Some(session::effective_base_url(session).as_str())
    }
}

impl ApplicationAdapter for GrokAdapter {
    fn kind(&self) -> ApplicationKind {
        ApplicationKind::Grok
    }

    fn detect(&self) -> ApplicationStatus {
        let has_key = config::extract_api_key(&self.config_text()).is_some();
        let has_oauth = self
            .auth_path
            .as_ref()
            .and_then(|path| auth::read_auth(path).ok())
            .is_some_and(|auth| session::has_login_material(&auth));
        let available = has_key || has_oauth;
        ApplicationStatus {
            kind: self.kind(),
            label: self.kind().display_name().into(),
            available,
            reason: (!available).then(|| "未检测到 Grok 登录。".into()),
        }
    }

    fn import_current(&self) -> Result<Session> {
        let config_text = self.config_text();
        if let Some(key) = config::extract_api_key(&config_text) {
            return session::session_from_auth(
                session::api_key_auth_json(&key),
                config::extract_base_url(&config_text),
            );
        }
        let auth = auth::read_auth(self.auth_path()?)?;
        if !session::has_login_material(&auth) {
            return Err(AppError::GrokNotDetected);
        }
        session::session_from_auth(auth, None)
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
        let is_oauth = session::is_official_login(&next_auth);
        let config_text = if is_oauth {
            config::apply_model_route(
                &config::read_text(config_path).unwrap_or_default(),
                None,
                None,
                None,
            )?
        } else {
            config::apply_model_route(
                &config::read_text(config_path).unwrap_or_default(),
                session::api_key(&next_auth).as_deref(),
                Some(session::effective_base_url(session).as_str()),
                session::display_label(session).as_deref(),
            )?
        };
        let skip_auth = if is_oauth {
            live_auth.as_ref().is_some_and(|live| {
                session::is_official_login(live)
                    && session::user_id(live) == session::user_id(&next_auth)
                    && session::user_id(&next_auth).is_some()
            })
        } else {
            true
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

    fn test_adapter() -> (GrokAdapter, PathBuf, PathBuf) {
        let id = uuid::Uuid::new_v4();
        let auth_path = env::temp_dir().join(format!("storm-dock-grok-{id}.json"));
        let config_path = env::temp_dir().join(format!("storm-dock-grok-{id}.toml"));
        let _ = fs::remove_file(&auth_path);
        let _ = fs::remove_file(&config_path);
        (
            GrokAdapter {
                auth_path: Some(auth_path.clone()),
                config_path: Some(config_path.clone()),
            },
            auth_path,
            config_path,
        )
    }

    fn oauth_session() -> Session {
        session::session_from_auth(
            session::oauth_auth_json("access-live", "refresh-live", Some("user-1"), Some("me@x.ai"), "now"),
            None,
        )
        .unwrap()
    }

    #[test]
    fn imports_and_switches_api_key_sessions() {
        let (adapter, auth_path, config_path) = test_adapter();
        fs::write(&config_path, "[cli]\ninstaller = \"internal\"\n").unwrap();
        let next = session::from_import(
            r#"{"api_key":"sk-next","base_url":"https://api.example.com/v1"}"#,
        )
        .unwrap();
        adapter.apply(&next).unwrap();
        let imported = adapter.import_current().unwrap();
        assert_eq!(session::import_type(&imported), crate::models::ImportType::ApiKey);
        assert_eq!(
            imported.values.get(session::GROK_AUTH_KEY),
            next.values.get(session::GROK_AUTH_KEY)
        );
        let config_text = fs::read_to_string(&config_path).unwrap();
        assert!(config_text.contains("https://api.example.com/v1"));
        assert!(config_text.contains("sk-next"));
        assert!(config_text.contains("installer"));
        assert!(!auth_path.exists());
        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn rejects_empty_session_without_writing() {
        let (adapter, auth_path, config_path) = test_adapter();
        crate::grok::auth::write_auth(
            &auth_path,
            &session::oauth_auth_json("token", "refresh", Some("user-1"), None, "now"),
        )
        .unwrap();
        let error = adapter.apply(&Session {
            values: Default::default(),
            raw_export: None,
        });
        assert!(matches!(error, Err(AppError::SecretMissing)));
        assert_eq!(
            session::user_id(&crate::grok::auth::read_auth(&auth_path).unwrap()).as_deref(),
            Some("user-1")
        );
        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn switching_api_key_keeps_auth_json_and_cli_tables() {
        let (adapter, auth_path, config_path) = test_adapter();
        fs::write(&config_path, "[cli]\ninstaller = \"internal\"\n\n[marketplace]\nkeep = true\n")
            .unwrap();
        let oauth = oauth_session();
        adapter.apply(&oauth).unwrap();
        let original_auth = fs::read(&auth_path).unwrap();

        let key = session::from_import(
            r#"{"api_key":"sk-next","base_url":"https://api.example.com/v1"}"#,
        )
        .unwrap();
        adapter.apply(&key).unwrap();
        assert_eq!(fs::read(&auth_path).unwrap(), original_auth);
        let config_text = fs::read_to_string(&config_path).unwrap();
        assert_eq!(
            crate::grok::config::extract_api_key(&config_text).as_deref(),
            Some("sk-next")
        );
        assert!(config_text.contains("installer"));
        assert!(config_text.contains("marketplace"));

        let imported = adapter.import_current().unwrap();
        assert_eq!(session::import_type(&imported), crate::models::ImportType::ApiKey);
        let live = adapter.live_match_session().unwrap();
        assert!(session::matches_live(&key, &live));
        assert!(!session::matches_live(&oauth, &live));

        adapter.apply(&oauth).unwrap();
        assert_eq!(fs::read(&auth_path).unwrap(), original_auth);
        let restored = fs::read_to_string(&config_path).unwrap();
        assert!(crate::grok::config::is_official_live_config(&restored));
        assert!(restored.contains("installer"));
        assert!(restored.contains("marketplace"));
        let live = adapter.live_match_session().unwrap();
        assert!(session::matches_live(&oauth, &live));
        assert!(!session::matches_live(&key, &live));

        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn same_user_skips_auth_json_write() {
        let (adapter, auth_path, config_path) = test_adapter();
        adapter.apply(&oauth_session()).unwrap();
        let original = fs::read(&auth_path).unwrap();
        let stale = session::session_from_auth(
            session::oauth_auth_json("stale-access", "stale-refresh", Some("user-1"), Some("me@x.ai"), "later"),
            None,
        )
        .unwrap();
        adapter.apply(&stale).unwrap();
        assert_eq!(fs::read(&auth_path).unwrap(), original);
        let _ = fs::remove_file(auth_path);
        let _ = fs::remove_file(config_path);
    }
}
