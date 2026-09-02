use std::collections::BTreeMap;

use crate::cursor::session::jwt_claims;
use crate::error::{AppError, Result};
use crate::models::{json_text, ImportType, Session};

pub(crate) const CODEX_AUTH_KEY: &str = "codex/auth.json";
pub(crate) const CODEX_EMAIL_KEY: &str = "codex/email";
pub(crate) const CODEX_ACCOUNT_ID_KEY: &str = "codex/account_id";
pub(crate) const CODEX_BASE_URL_KEY: &str = "codex/base_url";

pub(crate) fn auth_value(session: &Session) -> Result<serde_json::Value> {
    let raw = session
        .values
        .get(CODEX_AUTH_KEY)
        .ok_or(AppError::SecretMissing)?;
    Ok(serde_json::from_str(raw)?)
}

pub(crate) const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

pub(crate) fn base_url(session: &Session) -> Option<String> {
    session
        .values
        .get(CODEX_BASE_URL_KEY)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn effective_base_url(session: &Session) -> String {
    base_url(session).unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.into())
}

pub(crate) fn api_key(auth: &serde_json::Value) -> Option<String> {
    auth.get("OPENAI_API_KEY")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn chatgpt_account_id(auth: &serde_json::Value) -> Option<String> {
    auth.pointer("/tokens/account_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn has_login_material(auth: &serde_json::Value) -> bool {
    api_key(auth).is_some()
        || auth
            .pointer("/tokens/access_token")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|token| !token.trim().is_empty())
}

pub(crate) fn is_chatgpt_login(auth: &serde_json::Value) -> bool {
    auth.get("auth_mode").and_then(serde_json::Value::as_str) == Some("chatgpt")
        && chatgpt_account_id(auth).is_some()
}

pub(crate) fn import_type(session: &Session) -> ImportType {
    auth_value(session)
        .ok()
        .filter(is_chatgpt_login)
        .map(|_| ImportType::OAuth)
        .unwrap_or(ImportType::ApiKey)
}

pub(crate) fn display_label(session: &Session) -> Option<String> {
    session
        .values
        .get(CODEX_EMAIL_KEY)
        .cloned()
        .filter(|value| !value.is_empty())
        .or_else(|| session.values.get(CODEX_ACCOUNT_ID_KEY).cloned())
        .or_else(|| base_url(session))
}

pub(crate) fn oauth_auth_json(
    account_id: &str,
    access_token: &str,
    id_token: Option<&str>,
    refresh_token: &str,
    last_refresh: &str,
) -> serde_json::Value {
    let mut tokens = serde_json::Map::new();
    if let Some(id_token) = id_token.filter(|token| !token.is_empty()) {
        tokens.insert(
            "id_token".into(),
            serde_json::Value::String(id_token.into()),
        );
    }
    tokens.insert(
        "access_token".into(),
        serde_json::Value::String(access_token.into()),
    );
    tokens.insert(
        "refresh_token".into(),
        serde_json::Value::String(refresh_token.into()),
    );
    tokens.insert(
        "account_id".into(),
        serde_json::Value::String(account_id.into()),
    );
    serde_json::json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": serde_json::Value::Null,
        "tokens": tokens,
        "last_refresh": last_refresh,
    })
}

pub(crate) fn api_key_auth_json(api_key: &str) -> serde_json::Value {
    serde_json::json!({ "OPENAI_API_KEY": api_key })
}

pub(crate) fn session_from_auth(
    auth: serde_json::Value,
    base_url: Option<String>,
) -> Result<Session> {
    if !has_login_material(&auth) {
        return Err(AppError::SecretMissing);
    }
    let email = auth
        .pointer("/tokens/id_token")
        .and_then(serde_json::Value::as_str)
        .and_then(email_from_jwt);
    let account_id = chatgpt_account_id(&auth);
    let mut values = BTreeMap::from([(CODEX_AUTH_KEY.into(), serde_json::to_string(&auth)?)]);
    if let Some(email) = &email {
        values.insert(CODEX_EMAIL_KEY.into(), email.clone());
    }
    if let Some(account_id) = &account_id {
        values.insert(CODEX_ACCOUNT_ID_KEY.into(), account_id.clone());
    }
    if let Some(base_url) = base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        values.insert(CODEX_BASE_URL_KEY.into(), base_url.to_owned());
    }
    let mut record = serde_json::Map::new();
    record.insert("auth".into(), auth);
    if let Some(base_url) = values.get(CODEX_BASE_URL_KEY) {
        record.insert(
            "base_url".into(),
            serde_json::Value::String(base_url.clone()),
        );
    }
    if let Some(email) = email {
        record.insert("email".into(), serde_json::Value::String(email));
    }
    Ok(Session {
        values,
        raw_export: Some(serde_json::Value::Object(record)),
    })
}

pub(crate) fn from_import(raw: &str) -> Result<Session> {
    let raw = raw.trim().trim_matches(['"', '\'']).trim();
    let raw = raw.strip_prefix("Bearer ").unwrap_or(raw).trim();
    if raw.is_empty() {
        return Err(AppError::InvalidToken);
    }
    if !raw.starts_with('{') && !raw.starts_with('[') {
        return session_from_auth(api_key_auth_json(raw), None);
    }
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let value = match value {
        serde_json::Value::Array(values) => {
            values.into_iter().next().ok_or(AppError::InvalidImport)?
        }
        value => value,
    };
    let object = value.as_object().ok_or(AppError::InvalidImport)?;
    let base_url = json_text(&value, &["base_url", "baseUrl", "BASE_URL"]);
    if value.get("tokens").is_some() || value.get("auth_mode").is_some() {
        let auth = object
            .get("auth")
            .cloned()
            .filter(|auth| auth.is_object())
            .unwrap_or(value.clone());
        return session_from_auth(auth, base_url);
    }
    let key = json_text(
        &value,
        &[
            "OPENAI_API_KEY",
            "openai_api_key",
            "api_key",
            "apiKey",
            "key",
        ],
    )
    .ok_or(AppError::InvalidImport)?;
    session_from_auth(api_key_auth_json(&key), base_url)
}

pub(crate) fn same_identity(left: &Session, right: &Session) -> bool {
    let Ok(left_auth) = auth_value(left) else {
        return false;
    };
    let Ok(right_auth) = auth_value(right) else {
        return false;
    };
    if is_chatgpt_login(&left_auth) && is_chatgpt_login(&right_auth) {
        return chatgpt_account_id(&left_auth) == chatgpt_account_id(&right_auth);
    }
    api_key(&left_auth) == api_key(&right_auth) && base_url(left) == base_url(right)
}

pub(crate) fn matches_live(saved: &Session, live: &Session) -> bool {
    let Ok(saved_auth) = auth_value(saved) else {
        return false;
    };
    let Ok(live_auth) = auth_value(live) else {
        return false;
    };
    if is_chatgpt_login(&saved_auth) {
        return is_chatgpt_login(&live_auth)
            && chatgpt_account_id(&saved_auth) == chatgpt_account_id(&live_auth);
    }
    !is_chatgpt_login(&live_auth)
        && api_key(&saved_auth) == api_key(&live_auth)
        && effective_base_url(saved) == effective_base_url(live)
}

fn email_from_jwt(token: &str) -> Option<String> {
    let claims = jwt_claims(token)?;
    json_text(&claims, &["email", "email_address", "preferred_username"])
        .filter(|value| value.contains('@'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_chatgpt_login_and_api_key() {
        let oauth = session_from_auth(
            oauth_auth_json("acct", "access", Some("id"), "refresh", "now"),
            None,
        )
        .unwrap();
        assert_eq!(import_type(&oauth), ImportType::OAuth);
        assert_eq!(
            oauth.values.get(CODEX_ACCOUNT_ID_KEY).map(String::as_str),
            Some("acct")
        );

        let key = from_import("sk-test-key").unwrap();
        assert_eq!(import_type(&key), ImportType::ApiKey);
        let custom = from_import(
            r#"{"OPENAI_API_KEY":"sk-custom","base_url":"https://api.example.com/v1"}"#,
        )
        .unwrap();
        assert_eq!(
            base_url(&custom).as_deref(),
            Some("https://api.example.com/v1")
        );
        assert!(!same_identity(&key, &custom));
        assert!(same_identity(&key, &from_import("sk-test-key").unwrap()));
        assert_eq!(effective_base_url(&key), DEFAULT_OPENAI_BASE_URL);
        assert_eq!(effective_base_url(&custom), "https://api.example.com/v1");

        let live_default = session_from_auth(
            api_key_auth_json("sk-test-key"),
            Some(DEFAULT_OPENAI_BASE_URL.into()),
        )
        .unwrap();
        assert!(matches_live(&key, &live_default));
        assert!(!matches_live(&oauth, &live_default));
        assert!(matches_live(&oauth, &oauth));
        assert!(!matches_live(&custom, &live_default));
    }
}
