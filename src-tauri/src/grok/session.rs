use std::collections::BTreeMap;

use crate::cursor::session::jwt_claims;
use crate::error::{AppError, Result};
use crate::models::{json_text, ImportType, Session};

pub(crate) const GROK_AUTH_KEY: &str = "grok/auth.json";
pub(crate) const GROK_EMAIL_KEY: &str = "grok/email";
pub(crate) const GROK_USER_ID_KEY: &str = "grok/user_id";
pub(crate) const GROK_BASE_URL_KEY: &str = "grok/base_url";

pub(crate) const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
pub(crate) const XAI_ISSUER: &str = "https://auth.x.ai";
pub(crate) const OIDC_SCOPE_PREFIX: &str = "https://auth.x.ai::";
const LEGACY_SESSION_SCOPE: &str = "https://accounts.x.ai/sign-in";

pub(crate) fn oidc_scope_key() -> String {
    format!("{OIDC_SCOPE_PREFIX}{XAI_CLIENT_ID}")
}

pub(crate) fn auth_value(session: &Session) -> Result<serde_json::Value> {
    let raw = session
        .values
        .get(GROK_AUTH_KEY)
        .ok_or(AppError::SecretMissing)?;
    Ok(serde_json::from_str(raw)?)
}

pub(crate) fn base_url(session: &Session) -> Option<String> {
    session
        .values
        .get(GROK_BASE_URL_KEY)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn effective_base_url(session: &Session) -> String {
    base_url(session).unwrap_or_else(|| crate::grok::config::DEFAULT_BASE_URL.into())
}

fn string_field(entry: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    entry
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn preferred_entry(
    auth: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    let root = auth.as_object()?;
    let mut oidc_candidate = None;
    let mut legacy_candidate = None;
    for (scope, value) in root {
        let Some(entry) = value.as_object() else {
            continue;
        };
        if string_field(entry, "key").is_none() {
            continue;
        }
        if scope.starts_with(OIDC_SCOPE_PREFIX) {
            oidc_candidate = Some(entry);
        } else if scope == LEGACY_SESSION_SCOPE || scope.contains("/sign-in") {
            legacy_candidate = Some(entry);
        }
    }
    oidc_candidate.or(legacy_candidate)
}

pub(crate) fn access_token(auth: &serde_json::Value) -> Option<String> {
    preferred_entry(auth).and_then(|entry| string_field(entry, "key"))
}

pub(crate) fn user_id(auth: &serde_json::Value) -> Option<String> {
    preferred_entry(auth).and_then(|entry| {
        string_field(entry, "user_id").or_else(|| string_field(entry, "principal_id"))
    })
}

pub(crate) fn email(auth: &serde_json::Value) -> Option<String> {
    preferred_entry(auth)
        .and_then(|entry| string_field(entry, "email"))
        .filter(|value| value.contains('@'))
}

pub(crate) fn api_key(auth: &serde_json::Value) -> Option<String> {
    if preferred_entry(auth).is_some() {
        return None;
    }
    json_text(auth, &["api_key", "API_KEY", "OPENAI_API_KEY", "key"])
}

pub(crate) fn has_login_material(auth: &serde_json::Value) -> bool {
    access_token(auth).is_some() || api_key(auth).is_some()
}

pub(crate) fn is_official_login(auth: &serde_json::Value) -> bool {
    preferred_entry(auth).is_some()
}

pub(crate) fn import_type(session: &Session) -> ImportType {
    auth_value(session)
        .ok()
        .filter(is_official_login)
        .map(|_| ImportType::OAuth)
        .unwrap_or(ImportType::ApiKey)
}

pub(crate) fn display_label(session: &Session) -> Option<String> {
    session
        .values
        .get(GROK_EMAIL_KEY)
        .cloned()
        .filter(|value| !value.is_empty())
        .or_else(|| session.values.get(GROK_USER_ID_KEY).cloned())
        .or_else(|| base_url(session))
}

pub(crate) fn api_key_auth_json(api_key: &str) -> serde_json::Value {
    serde_json::json!({ "api_key": api_key })
}

pub(crate) fn oauth_auth_json(
    access_token: &str,
    refresh_token: &str,
    user_id: Option<&str>,
    email: Option<&str>,
    expires_at: &str,
) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    entry.insert("key".into(), serde_json::Value::String(access_token.into()));
    entry.insert("auth_mode".into(), serde_json::Value::String("oidc".into()));
    entry.insert(
        "refresh_token".into(),
        serde_json::Value::String(refresh_token.into()),
    );
    entry.insert(
        "expires_at".into(),
        serde_json::Value::String(expires_at.into()),
    );
    entry.insert(
        "oidc_issuer".into(),
        serde_json::Value::String(XAI_ISSUER.into()),
    );
    entry.insert(
        "oidc_client_id".into(),
        serde_json::Value::String(XAI_CLIENT_ID.into()),
    );
    if let Some(user_id) = user_id.filter(|value| !value.is_empty()) {
        entry.insert(
            "user_id".into(),
            serde_json::Value::String(user_id.to_owned()),
        );
        entry.insert(
            "principal_id".into(),
            serde_json::Value::String(user_id.to_owned()),
        );
        entry.insert(
            "principal_type".into(),
            serde_json::Value::String("User".into()),
        );
    }
    if let Some(email) = email.filter(|value| !value.is_empty()) {
        entry.insert("email".into(), serde_json::Value::String(email.to_owned()));
    }
    serde_json::json!({ oidc_scope_key(): entry })
}

pub(crate) fn session_from_auth(
    auth: serde_json::Value,
    base_url: Option<String>,
) -> Result<Session> {
    if !has_login_material(&auth) {
        return Err(AppError::SecretMissing);
    }
    let email = email(&auth);
    let user_id = user_id(&auth);
    let mut values = BTreeMap::from([(GROK_AUTH_KEY.into(), serde_json::to_string(&auth)?)]);
    if let Some(email) = &email {
        values.insert(GROK_EMAIL_KEY.into(), email.clone());
    }
    if let Some(user_id) = &user_id {
        values.insert(GROK_USER_ID_KEY.into(), user_id.clone());
    }
    if !is_official_login(&auth) {
        if let Some(base_url) = base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            values.insert(GROK_BASE_URL_KEY.into(), base_url.to_owned());
        }
    }
    let mut record = serde_json::Map::new();
    record.insert("auth".into(), auth);
    if let Some(base_url) = values.get(GROK_BASE_URL_KEY) {
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
    if preferred_entry(&value).is_some() {
        return session_from_auth(value, None);
    }
    if object.get("auth").is_some_and(serde_json::Value::is_object) {
        let auth = object.get("auth").cloned().unwrap();
        return session_from_auth(auth, base_url);
    }
    let key = json_text(
        &value,
        &["api_key", "API_KEY", "OPENAI_API_KEY", "apiKey", "key"],
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
    if is_official_login(&left_auth) && is_official_login(&right_auth) {
        return user_id(&left_auth) == user_id(&right_auth) && user_id(&left_auth).is_some();
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
    if is_official_login(&saved_auth) {
        return is_official_login(&live_auth) && user_id(&saved_auth) == user_id(&live_auth);
    }
    !is_official_login(&live_auth)
        && api_key(&saved_auth) == api_key(&live_auth)
        && effective_base_url(saved) == effective_base_url(live)
}

pub(crate) fn email_from_jwt(token: &str) -> Option<String> {
    let claims = jwt_claims(token)?;
    json_text(&claims, &["email", "email_address", "preferred_username"])
        .filter(|value| value.contains('@'))
}

pub(crate) fn user_id_from_jwt(token: &str) -> Option<String> {
    let claims = jwt_claims(token)?;
    json_text(&claims, &["sub", "user_id", "principal_id"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_official_login_and_api_key() {
        let oauth = session_from_auth(
            oauth_auth_json("access", "refresh", Some("user-1"), Some("me@x.ai"), "now"),
            None,
        )
        .unwrap();
        assert_eq!(import_type(&oauth), ImportType::OAuth);
        assert_eq!(
            oauth.values.get(GROK_USER_ID_KEY).map(String::as_str),
            Some("user-1")
        );
        assert_eq!(
            oauth.values.get(GROK_EMAIL_KEY).map(String::as_str),
            Some("me@x.ai")
        );

        let key = from_import("sk-test-key").unwrap();
        assert_eq!(import_type(&key), ImportType::ApiKey);
        let custom =
            from_import(r#"{"api_key":"sk-custom","base_url":"https://api.example.com/v1"}"#)
                .unwrap();
        assert_eq!(
            base_url(&custom).as_deref(),
            Some("https://api.example.com/v1")
        );
        assert!(!same_identity(&key, &custom));
        assert!(same_identity(&key, &from_import("sk-test-key").unwrap()));
        assert_eq!(
            effective_base_url(&key),
            crate::grok::config::DEFAULT_BASE_URL
        );

        let live_default = session_from_auth(
            api_key_auth_json("sk-test-key"),
            Some(crate::grok::config::DEFAULT_BASE_URL.into()),
        )
        .unwrap();
        assert!(matches_live(&key, &live_default));
        assert!(!matches_live(&oauth, &live_default));
        assert!(matches_live(&oauth, &oauth));
        assert!(!matches_live(&custom, &live_default));
    }

    #[test]
    fn prefers_oidc_entry_over_legacy() {
        let auth = serde_json::json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy" },
            "https://auth.x.ai::client": { "key": "oidc", "user_id": "u1" }
        });
        assert_eq!(access_token(&auth).as_deref(), Some("oidc"));
        assert_eq!(user_id(&auth).as_deref(), Some("u1"));
    }
}
