use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use std::collections::BTreeMap;

use crate::error::{AppError, Result};
use crate::models::{
    json_text, Session, ACCESS_TOKEN_KEY, AUTH_ID_KEY, EMAIL_KEY, MEMBERSHIP_TYPE_KEY,
};

pub(crate) fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let payload = payload.replace('+', "-").replace('/', "_");
    let bytes = URL_SAFE_NO_PAD.decode(&payload).ok().or_else(|| {
        let mut padded = payload;
        while padded.len() % 4 != 0 {
            padded.push('=');
        }
        URL_SAFE.decode(padded).ok()
    })?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn jwt_claim_text(claims: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| claims.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn session_user_id(session: &Session) -> Option<String> {
    if let Some(token) = session.values.get(ACCESS_TOKEN_KEY) {
        if let Some(claims) = jwt_claims(token) {
            if let Some(sub) = jwt_claim_text(&claims, &["sub"]) {
                if let Some(user_id) = sub.rsplit('|').next().filter(|id| !id.is_empty()) {
                    return Some(user_id.to_owned());
                }
            }
        }
    }
    session
        .values
        .get("glass.lastSignedInAuthId")
        .and_then(|value| value.rsplit('|').next())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

pub(crate) fn session_display_label(session: &Session) -> Option<String> {
    if let Some(email) = session
        .values
        .get(EMAIL_KEY)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(email.to_owned());
    }
    if let Some(profile) = session
        .values
        .get("cursorAuth/cachedScopedProfile")
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
    {
        if let Some(name) = json_text(&profile, &["displayName", "name"]) {
            return Some(name);
        }
    }
    session_user_id(session)
}

pub(crate) fn is_cursor_user_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("user_") else {
        return false;
    };
    rest.len() >= 6
        && rest
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

pub(crate) fn parse_cursor_session_token(raw: &str) -> Option<(String, String)> {
    let raw = raw
        .strip_prefix("WorkosCursorSessionToken=")
        .unwrap_or(raw)
        .trim();
    let decoded = raw.replace("%3A%3A", "::");
    let (user_id, token) = decoded.split_once("::")?;
    let user_id = user_id.trim();
    let token = token.trim();
    if is_cursor_user_id(user_id) && token.len() >= 40 {
        Some((user_id.to_owned(), token.to_owned()))
    } else {
        None
    }
}

pub(crate) fn split_cursor_credential(raw: &str) -> Result<(Option<String>, String)> {
    if let Some((user_id, token)) = parse_cursor_session_token(raw) {
        return Ok((Some(user_id), token));
    }
    let token = raw.trim();
    if token.len() < 40 {
        return Err(AppError::InvalidToken);
    }
    Ok((None, token.to_owned()))
}

pub(crate) fn apply_jwt_profile(values: &mut BTreeMap<String, String>, token: &str) {
    let Some(claims) = jwt_claims(token) else {
        return;
    };
    if !values.contains_key(EMAIL_KEY) {
        if let Some(email) = ["email", "email_address", "preferred_username"]
            .iter()
            .find_map(|key| jwt_claim_text(&claims, &[key]))
            .filter(|value| value.contains('@'))
        {
            values.insert(EMAIL_KEY.into(), email.clone());
            values.insert(
                "cursorAuth/cachedScopedProfile".into(),
                serde_json::json!({ "displayName": email }).to_string(),
            );
        }
    }
    if !values.contains_key(AUTH_ID_KEY) {
        if let Some(sub) = jwt_claim_text(&claims, &["sub"]) {
            values.insert(AUTH_ID_KEY.into(), sub);
        }
    }
}

pub(crate) fn session_from_access_token(token: &str, user_id: Option<String>) -> Session {
    let mut values = BTreeMap::from([(ACCESS_TOKEN_KEY.into(), token.to_owned())]);
    apply_jwt_profile(&mut values, token);
    if !values.contains_key(AUTH_ID_KEY) {
        if let Some(user_id) = user_id.filter(|value| !value.is_empty()) {
            values.insert(AUTH_ID_KEY.into(), user_id);
        }
    }
    Session {
        values,
        raw_export: None,
    }
}

impl Session {
    pub(crate) fn from_import(raw: &str) -> Result<Self> {
        let raw = raw.trim().trim_matches(['"', '\'']).trim();
        let raw = raw.strip_prefix("Bearer ").unwrap_or(raw).trim();
        if raw.is_empty() {
            return Err(AppError::InvalidToken);
        }
        if !raw.starts_with('{') && !raw.starts_with('[') {
            let (user_id, token) = split_cursor_credential(raw)?;
            return Ok(session_from_access_token(&token, user_id));
        }

        let value: serde_json::Value = serde_json::from_str(raw)?;
        let value = match value {
            serde_json::Value::Array(values) => {
                values.into_iter().next().ok_or(AppError::InvalidImport)?
            }
            value => value,
        };
        let object = value.as_object().ok_or(AppError::InvalidImport)?;
        let text = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
                .map(str::to_owned)
        };
        let credential = text(&[
            "access_token",
            "accessToken",
            ACCESS_TOKEN_KEY,
            "sessionToken",
            "session_token",
            "WorkosCursorSessionToken",
            "token",
        ])
        .ok_or(AppError::InvalidImport)?;
        let (user_id, access_token) = split_cursor_credential(&credential)?;
        let mut session = session_from_access_token(&access_token, user_id);
        if let Some(refresh) = text(&["refresh_token", "refreshToken", "cursorAuth/refreshToken"]) {
            session
                .values
                .insert("cursorAuth/refreshToken".into(), refresh);
        }
        if let Some(email) = text(&["email", "cursorAuth/cachedEmail"]) {
            session.values.insert(EMAIL_KEY.into(), email.clone());
            session.values.insert(
                "cursorAuth/cachedScopedProfile".into(),
                serde_json::json!({ "displayName": email }).to_string(),
            );
        }
        for field in ["cursor_auth_raw", "cursorAuthRaw"] {
            let Some(cache) = object.get(field).and_then(serde_json::Value::as_object) else {
                continue;
            };
            for (source, target) in [
                ("authId", "glass.lastSignedInAuthId"),
                ("cachedSignUpType", "cursorAuth/cachedSignUpType"),
                ("stripeMembershipType", "cursorAuth/stripeMembershipType"),
            ] {
                if let Some(value) = cache.get(source).and_then(serde_json::Value::as_str) {
                    session.values.insert(target.into(), value.into());
                }
            }
        }
        session.raw_export = Some(value);
        Ok(session)
    }
}

pub(crate) fn raw_export_from_session(
    session: &Session,
    id: &str,
    created_at: u64,
    updated_at: u64,
    last_used_at: u64,
    telemetry: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    if let Some(raw) = &session.raw_export {
        return raw.clone();
    }
    let mut auth = serde_json::Map::new();
    let mut record = serde_json::Map::new();
    record.insert("id".into(), serde_json::Value::String(id.into()));
    record.insert("created_at".into(), serde_json::Value::from(created_at));
    record.insert("last_used".into(), serde_json::Value::from(last_used_at));
    for (target, source) in [
        ("access_token", ACCESS_TOKEN_KEY),
        ("refresh_token", "cursorAuth/refreshToken"),
        ("auth_id", "glass.lastSignedInAuthId"),
        ("email", EMAIL_KEY),
        ("sign_up_type", "cursorAuth/cachedSignUpType"),
        ("membership_type", MEMBERSHIP_TYPE_KEY),
    ] {
        if let Some(value) = session.values.get(source) {
            record.insert(target.into(), serde_json::Value::String(value.clone()));
        }
    }
    for (target, source) in [
        ("accessToken", ACCESS_TOKEN_KEY),
        ("refreshToken", "cursorAuth/refreshToken"),
        ("authId", "glass.lastSignedInAuthId"),
        ("cachedEmail", EMAIL_KEY),
        ("cachedSignUpType", "cursorAuth/cachedSignUpType"),
        ("stripeMembershipType", MEMBERSHIP_TYPE_KEY),
    ] {
        if let Some(value) = session.values.get(source) {
            auth.insert(target.into(), serde_json::Value::String(value.clone()));
        }
    }
    record.insert("cursor_auth_raw".into(), serde_json::Value::Object(auth));
    record.insert(
        "telemetry_machine_ids".into(),
        serde_json::Value::Object(telemetry),
    );
    record.insert("updated_at".into(), serde_json::Value::from(updated_at));
    serde_json::Value::Object(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{import_type, ImportType, ACCESS_TOKEN_KEY, EMAIL_KEY};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use std::collections::BTreeMap;

    #[test]
    fn imports_raw_tokens_and_exported_json() {
        let token = "a".repeat(40);
        assert_eq!(
            Session::from_import(&token)
                .unwrap()
                .values
                .get(ACCESS_TOKEN_KEY),
            Some(&token)
        );
        let json = format!(
            r#"[{{"access_token":"{}","refresh_token":"refresh","email":"me@example.com"}}]"#,
            token
        );
        let session = Session::from_import(&json).unwrap();
        assert_eq!(session.values.get(ACCESS_TOKEN_KEY), Some(&token));
        assert_eq!(
            session.values.get("cursorAuth/refreshToken"),
            Some(&"refresh".into())
        );
        assert_eq!(
            session.values.get(EMAIL_KEY),
            Some(&"me@example.com".into())
        );
        assert_eq!(import_type(&session), ImportType::OAuth);
        let jwt = Session {
            values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a.b.c".into())]),
            raw_export: None,
        };
        assert_eq!(import_type(&jwt), ImportType::Jwt);
    }

    #[test]
    fn imports_user_prefixed_session_tokens() {
        let claims = URL_SAFE_NO_PAD.encode(
            r#"{"sub":"auth0|user_01ABCDEFGHJKMNPQRSTVWXYZ","email":"me@example.com","exp":4102444800}"#,
        );
        let jwt = format!("header.{claims}.signature-padding-for-length");
        let user_id = "user_01ABCDEFGHJKMNPQRSTVWXYZ";
        let session = Session::from_import(&format!("{user_id}::{jwt}")).unwrap();
        assert_eq!(session.values.get(ACCESS_TOKEN_KEY), Some(&jwt));
        assert_eq!(
            session
                .values
                .get("glass.lastSignedInAuthId")
                .map(String::as_str),
            Some("auth0|user_01ABCDEFGHJKMNPQRSTVWXYZ")
        );
        assert_eq!(
            session.values.get(EMAIL_KEY).map(String::as_str),
            Some("me@example.com")
        );
        assert_eq!(import_type(&session), ImportType::Jwt);

        let encoded =
            Session::from_import(&format!("WorkosCursorSessionToken={user_id}%3A%3A{jwt}"))
                .unwrap();
        assert_eq!(encoded.values.get(ACCESS_TOKEN_KEY), Some(&jwt));

        let token = "a".repeat(40);
        let session_token = Session::from_import(&format!("{user_id}::{token}")).unwrap();
        assert_eq!(session_token.values.get(ACCESS_TOKEN_KEY), Some(&token));
        assert_eq!(
            session_token
                .values
                .get("glass.lastSignedInAuthId")
                .map(String::as_str),
            Some(user_id)
        );
        assert_eq!(import_type(&session_token), ImportType::Token);

        let json =
            Session::from_import(&format!(r#"{{"sessionToken":"{user_id}::{token}"}}"#)).unwrap();
        assert_eq!(json.values.get(ACCESS_TOKEN_KEY), Some(&token));
        assert!(Session::from_import(user_id).is_err());
    }

    #[test]
    fn jwt_claims_accept_padded_payloads() {
        let mut payload = URL_SAFE_NO_PAD
            .encode(r#"{"email":"me@example.com","sub":"user_01ABCDEFGHJKMNPQRSTVWXYZ"}"#);
        while payload.len() % 4 != 0 {
            payload.push('=');
        }
        assert!(payload.contains('='));
        let claims = jwt_claims(&format!("header.{payload}.signature")).unwrap();
        assert_eq!(claims["email"], "me@example.com");
    }
}
