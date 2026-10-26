use std::collections::BTreeMap;

use crate::error::{AppError, Result};
use crate::models::{json_text, ImportType, Session};

pub(crate) const WORKBUDDY_PROFILE_KEY: &str = "workbuddy/profile.json";
pub(crate) const WORKBUDDY_EMAIL_KEY: &str = "workbuddy/email";
pub(crate) const WORKBUDDY_UID_KEY: &str = "workbuddy/uid";
pub(crate) const DEFAULT_DOMAIN: &str = "www.workbuddy.cn";

pub(crate) fn profile_value(session: &Session) -> Result<serde_json::Value> {
    let raw = session
        .values
        .get(WORKBUDDY_PROFILE_KEY)
        .ok_or(AppError::SecretMissing)?;
    Ok(serde_json::from_str(raw)?)
}

pub(crate) fn uid(session: &Session) -> Option<String> {
    session
        .values
        .get(WORKBUDDY_UID_KEY)
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            profile_value(session)
                .ok()
                .and_then(|profile| account_uid(&profile))
        })
}

pub(crate) fn display_label(session: &Session) -> Option<String> {
    session
        .values
        .get(WORKBUDDY_EMAIL_KEY)
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| uid(session))
}

pub(crate) fn import_type(session: &Session) -> ImportType {
    let Ok(profile) = profile_value(session) else {
        return ImportType::OAuth;
    };
    match profile
        .pointer("/auth/source")
        .and_then(serde_json::Value::as_str)
    {
        // Unlike Cursor token imports, WorkBuddy tokens can be written into the
        // desktop auth file, so keep them switchable via OAuth/Native.
        Some("native") | Some("desktop") => ImportType::Native,
        _ => ImportType::OAuth,
    }
}

pub(crate) fn account_uid(profile: &serde_json::Value) -> Option<String> {
    profile
        .pointer("/account/uid")
        .or_else(|| profile.pointer("/account/userId"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn account_label(profile: &serde_json::Value) -> Option<String> {
    let account = profile.get("account")?;
    if let Some(nickname) = account.get("nickname").and_then(serde_json::Value::as_str) {
        let nickname = nickname.trim();
        // Encrypted nickname blobs are objects after parse; string form may look like JSON.
        if !nickname.is_empty() && !nickname.starts_with('{') {
            return Some(nickname.to_owned());
        }
    }
    for key in ["email", "mail", "userName", "username", "uin"] {
        if let Some(value) = account
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(value.to_owned());
        }
    }
    account_uid(profile)
}

fn auth_access_present(auth: &serde_json::Value) -> bool {
    match auth.get("accessToken").or_else(|| auth.get("access_token")) {
        Some(serde_json::Value::String(token)) => !token.trim().is_empty(),
        Some(serde_json::Value::Object(map)) => {
            map.get("$wbEncrypted")
                .and_then(serde_json::Value::as_u64)
                == Some(1)
                || map.contains_key("envelope")
        }
        _ => false,
    }
}

pub(crate) fn has_login_material(profile: &serde_json::Value) -> bool {
    profile.get("auth").is_some_and(auth_access_present)
}

pub(crate) fn access_token_plaintext(profile: &serde_json::Value) -> Option<String> {
    profile
        .pointer("/auth/accessToken")
        .or_else(|| profile.pointer("/auth/access_token"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}


/// WorkBuddy encrypts nickname in workbuddy-desktop.info; the desktop app still
/// keeps a plaintext snapshot under ~/.workbuddy for the signed-in profile.
pub(crate) fn local_account_snapshot_nickname(uid: &str) -> Option<String> {
    let home = dirs::home_dir()?;
    let path = home.join(".workbuddy/storage/skeleton/account-snapshot.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let primary = value.get("primary")?;
    let snap_uid = primary
        .get("uid")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if snap_uid != uid {
        return None;
    }
    primary
        .get("nickname")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.starts_with('{'))
        .map(str::to_owned)
}

pub(crate) fn enrich_profile_from_local_snapshot(mut profile: serde_json::Value) -> serde_json::Value {
    let Some(uid) = account_uid(&profile) else {
        return profile;
    };
    let live_nick_bad = match profile.pointer("/account/nickname") {
        Some(serde_json::Value::String(s)) => s.trim().is_empty() || s.trim().starts_with('{'),
        Some(serde_json::Value::Object(_)) => true,
        None => true,
        _ => true,
    };
    if !live_nick_bad {
        return profile;
    }
    let Some(nickname) = local_account_snapshot_nickname(&uid) else {
        return profile;
    };
    if let Some(account) = profile.get_mut("account").and_then(|v| v.as_object_mut()) {
        account.insert("nickname".into(), serde_json::Value::String(nickname));
    }
    profile
}

pub(crate) fn token_is_encrypted(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.get("$wbEncrypted")
                .and_then(serde_json::Value::as_u64)
                == Some(1)
                || map.contains_key("envelope")
        }
        _ => false,
    }
}

/// When the live desktop auth file has encrypted tokens, keep Storm Dock's
/// matching plaintext tokens so re-import does not wipe usable credentials.
pub(crate) fn prefer_saved_plaintext(live: Session, saved: &Session) -> Session {
    let Ok(mut live_profile) = profile_value(&live) else {
        return live;
    };
    if access_token_plaintext(&live_profile).is_some() {
        return live;
    }
    let Ok(saved_profile) = profile_value(saved) else {
        return live;
    };
    let Some(saved_access) = access_token_plaintext(&saved_profile) else {
        return live;
    };
    let Some(auth) = live_profile
        .get_mut("auth")
        .and_then(|value| value.as_object_mut())
    else {
        return live;
    };
    let access_key = if auth.contains_key("accessToken") {
        "accessToken"
    } else if auth.contains_key("access_token") {
        "access_token"
    } else {
        "accessToken"
    };
    let live_access = auth.get(access_key);
    if live_access.is_some_and(token_is_encrypted) || live_access.is_none() {
        auth.insert(
            access_key.into(),
            serde_json::Value::String(saved_access),
        );
    }
    let refresh_key = if auth.contains_key("refreshToken") {
        Some("refreshToken")
    } else if auth.contains_key("refresh_token") {
        Some("refresh_token")
    } else {
        None
    };
    if let Some(key) = refresh_key {
        let live_refresh = auth.get(key);
        if live_refresh.is_some_and(token_is_encrypted) || live_refresh.is_none() {
            if let Some(saved_refresh) = saved_profile
                .pointer(&format!("/auth/{key}"))
                .or_else(|| {
                    if key == "refreshToken" {
                        saved_profile.pointer("/auth/refresh_token")
                    } else {
                        saved_profile.pointer("/auth/refreshToken")
                    }
                })
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                auth.insert(key.into(), serde_json::Value::String(saved_refresh.to_owned()));
            }
        }
    }
    // Prefer a human nickname from the saved profile when live nickname is encrypted.
    if let (Some(live_account), Some(saved_account)) = (
        live_profile.get_mut("account").and_then(|v| v.as_object_mut()),
        saved_profile.get("account").and_then(|v| v.as_object()),
    ) {
        let live_nick_bad = match live_account.get("nickname") {
            Some(serde_json::Value::String(s)) => s.trim().is_empty() || s.trim().starts_with('{'),
            Some(serde_json::Value::Object(_)) => true,
            None => true,
            _ => true,
        };
        if live_nick_bad {
            if let Some(nick) = saved_account
                .get("nickname")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.starts_with('{'))
            {
                live_account.insert(
                    "nickname".into(),
                    serde_json::Value::String(nick.to_owned()),
                );
            }
        }
    }
    let source = live_profile
        .pointer("/auth/source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("native")
        .to_owned();
    session_from_profile(live_profile, &source).unwrap_or(live)
}

pub(crate) fn matches_live(saved: &Session, live: &Session) -> bool {
    match (uid(saved), uid(live)) {
        (Some(left), Some(right)) if !left.is_empty() && left == right => true,
        _ => {
            let Ok(saved_profile) = profile_value(saved) else {
                return false;
            };
            let Ok(live_profile) = profile_value(live) else {
                return false;
            };
            access_token_plaintext(&saved_profile)
                .zip(access_token_plaintext(&live_profile))
                .is_some_and(|(left, right)| left == right)
        }
    }
}

pub(crate) fn session_from_profile(
    mut profile: serde_json::Value,
    source: &str,
) -> Result<Session> {
    if !has_login_material(&profile) {
        return Err(AppError::SecretMissing);
    }
    profile = enrich_profile_from_local_snapshot(profile);
    if let Some(auth) = profile.get_mut("auth").and_then(|value| value.as_object_mut()) {
        auth.entry("tokenType".to_string())
            .or_insert_with(|| serde_json::Value::String("Bearer".into()));
        auth.entry("domain".to_string())
            .or_insert_with(|| serde_json::Value::String(DEFAULT_DOMAIN.into()));
        auth.insert(
            "source".into(),
            serde_json::Value::String(source.to_owned()),
        );
    }
    if let Some(account) = profile.get("account").cloned() {
        if profile.get("accounts").is_none() {
            profile
                .as_object_mut()
                .ok_or(AppError::InvalidImport)?
                .insert("accounts".into(), serde_json::json!([account]));
        }
    }
    let label = account_label(&profile);
    let user_id = account_uid(&profile);
    let mut values = BTreeMap::from([(
        WORKBUDDY_PROFILE_KEY.into(),
        serde_json::to_string(&profile)?,
    )]);
    if let Some(label) = &label {
        values.insert(WORKBUDDY_EMAIL_KEY.into(), label.clone());
    }
    if let Some(user_id) = &user_id {
        values.insert(WORKBUDDY_UID_KEY.into(), user_id.clone());
    }
    Ok(Session {
        values,
        raw_export: Some(profile),
    })
}

pub(crate) fn profile_from_tokens(
    access_token: &str,
    refresh_token: Option<&str>,
    email: Option<&str>,
    user_id: Option<&str>,
    domain: Option<&str>,
    source: &str,
) -> serde_json::Value {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let uid = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_owned();
    let nickname = email
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(uid.as_str())
        .to_owned();
    let account = serde_json::json!({
        "uid": uid,
        "userId": uid,
        "nickname": nickname,
        "email": email.unwrap_or(""),
    });
    let mut auth = serde_json::Map::new();
    auth.insert(
        "accessToken".into(),
        serde_json::Value::String(access_token.to_owned()),
    );
    if let Some(refresh) = refresh_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        auth.insert(
            "refreshToken".into(),
            serde_json::Value::String(refresh.to_owned()),
        );
    }
    auth.insert(
        "tokenType".into(),
        serde_json::Value::String("Bearer".into()),
    );
    auth.insert(
        "domain".into(),
        serde_json::Value::String(domain.unwrap_or(DEFAULT_DOMAIN).to_owned()),
    );
    auth.insert(
        "source".into(),
        serde_json::Value::String(source.to_owned()),
    );
    auth.insert("lastRefreshTime".into(), serde_json::Value::from(now_ms));
    serde_json::json!({
        "account": account,
        "auth": auth,
        "accounts": [account],
        "allAccounts": [account],
    })
}

/// Storm Dock / workbuddy-desktop.info shaped JSON for export (no usage cache).
pub(crate) fn export_record(session: &Session) -> Result<serde_json::Value> {
    let mut profile = profile_value(session)?;
    sanitize_export_profile(&mut profile);
    if !has_login_material(&profile) {
        return Err(AppError::SecretMissing);
    }
    Ok(profile)
}

fn sanitize_export_profile(profile: &mut serde_json::Value) {
    let Some(object) = profile.as_object_mut() else {
        return;
    };
    object.remove("workbuddy_billing_raw");
    object.remove("workos_token");
    object.remove("cursor_usage_raw");
    object.remove("cursor_auth_raw");
    object.remove("telemetry_machine_ids");
}

/// Import one or many WorkBuddy credentials (token, profile JSON, or Storm Dock export array).
pub(crate) fn from_import_many(raw: &str) -> Result<Vec<Session>> {
    let raw = raw.trim().trim_matches(['"', '\'']).trim();
    let raw = raw.strip_prefix("Bearer ").unwrap_or(raw).trim();
    if raw.is_empty() {
        return Err(AppError::InvalidToken);
    }
    if !raw.starts_with('[') {
        return Ok(vec![from_import(raw)?]);
    }
    let values: Vec<serde_json::Value> = serde_json::from_str(raw)?;
    if values.is_empty() {
        return Err(AppError::InvalidImport);
    }
    values.into_iter().map(session_from_import_value).collect()
}

pub(crate) fn from_import(raw: &str) -> Result<Session> {
    let raw = raw.trim().trim_matches(['"', '\'']).trim();
    let raw = raw.strip_prefix("Bearer ").unwrap_or(raw).trim();
    if raw.is_empty() {
        return Err(AppError::InvalidToken);
    }
    if !raw.starts_with('{') && !raw.starts_with('[') {
        let profile = profile_from_tokens(raw, None, None, None, None, "token");
        return session_from_profile(profile, "token");
    }
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let value = match value {
        serde_json::Value::Array(values) => {
            values.into_iter().next().ok_or(AppError::InvalidImport)?
        }
        value => value,
    };
    session_from_import_value(value)
}

fn session_from_import_value(mut value: serde_json::Value) -> Result<Session> {
    // Allow `{ "profile": { account, auth } }` wrappers if present.
    if value.get("auth").is_none() {
        if let Some(profile) = value.get("profile").cloned() {
            if profile.get("auth").is_some() {
                value = profile;
            }
        }
    }
    sanitize_export_profile(&mut value);
    if value.get("auth").is_some()
        && (value.get("account").is_some() || has_login_material(&value))
    {
        let source = match value
            .pointer("/auth/source")
            .and_then(serde_json::Value::as_str)
        {
            Some("native") | Some("desktop") => "native",
            Some("token") | Some("jwt") => "token",
            _ => "oauth",
        };
        return session_from_profile(value, source);
    }
    // Nested auth object with tokens only.
    if let Some(auth) = value.get("auth").cloned() {
        if let Some(access) = json_text(
            &auth,
            &[
                "accessToken",
                "access_token",
                "token",
                "Authorization",
                "authorization",
            ],
        ) {
            let refresh = json_text(&auth, &["refreshToken", "refresh_token"]);
            let domain = json_text(&auth, &["domain"]).or_else(|| json_text(&value, &["domain"]));
            let email = json_text(&value, &["email", "nickname", "mail", "userName"]);
            let user_id = json_text(&value, &["uid", "userId", "user_id", "id"]);
            let profile = profile_from_tokens(
                &access,
                refresh.as_deref(),
                email.as_deref(),
                user_id.as_deref(),
                domain.as_deref(),
                "token",
            );
            return session_from_profile(profile, "token");
        }
    }
    let access = json_text(
        &value,
        &[
            "accessToken",
            "access_token",
            "token",
            "Authorization",
            "authorization",
        ],
    )
    .ok_or(AppError::InvalidImport)?;
    let refresh = json_text(&value, &["refreshToken", "refresh_token"]);
    let email = json_text(&value, &["email", "nickname", "mail", "userName"]);
    let user_id = json_text(&value, &["uid", "userId", "user_id", "id"]);
    let domain = json_text(&value, &["domain"]);
    let profile = profile_from_tokens(
        &access,
        refresh.as_deref(),
        email.as_deref(),
        user_id.as_deref(),
        domain.as_deref(),
        "token",
    );
    session_from_profile(profile, "token")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_access_token_and_profile_json() {
        let token = from_import("wb-access-token-example").unwrap();
        assert_eq!(import_type(&token), ImportType::OAuth);
        assert!(has_login_material(&profile_value(&token).unwrap()));

        let json = r#"{
          "account": {"uid":"u-1","nickname":"Ada"},
          "auth": {"accessToken":"tok-1","refreshToken":"ref-1","domain":"www.workbuddy.cn"}
        }"#;
        let session = from_import(json).unwrap();
        assert_eq!(uid(&session).as_deref(), Some("u-1"));
        assert_eq!(display_label(&session).as_deref(), Some("Ada"));
        assert!(matches_live(&session, &session));
    }

    #[test]
    fn export_record_strips_usage_cache_and_round_trips() {
        let json = r#"{
          "account": {"uid":"u-2","nickname":"Bea"},
          "auth": {"accessToken":"tok-2","refreshToken":"ref-2","domain":"www.workbuddy.cn","source":"oauth"},
          "workbuddy_billing_raw": {"TotalCredits": "2000"}
        }"#;
        let imported = from_import(json).unwrap();
        let mut profile = profile_value(&imported).unwrap();
        profile.as_object_mut().unwrap().insert(
            "workbuddy_billing_raw".into(),
            serde_json::json!({"TotalCredits": "2000"}),
        );
        let live = Session {
            values: std::collections::BTreeMap::from([(
                WORKBUDDY_PROFILE_KEY.into(),
                serde_json::to_string(&profile).unwrap(),
            )]),
            raw_export: Some(profile),
        };
        let exported = export_record(&live).unwrap();
        assert!(exported.get("workbuddy_billing_raw").is_none());
        assert_eq!(
            exported.pointer("/auth/accessToken").and_then(|v| v.as_str()),
            Some("tok-2")
        );
        let again = from_import(&serde_json::to_string(&exported).unwrap()).unwrap();
        assert_eq!(uid(&again).as_deref(), Some("u-2"));
        assert_eq!(display_label(&again).as_deref(), Some("Bea"));
    }

    #[test]
    fn imports_storm_dock_export_array() {
        let json = r#"[
          {
            "account": {"uid":"u-a","nickname":"Ann"},
            "auth": {"accessToken":"tok-a","refreshToken":"ref-a","domain":"www.workbuddy.cn"}
          },
          {
            "account": {"uid":"u-b","nickname":"Bob"},
            "auth": {"accessToken":"tok-b","refreshToken":"ref-b","domain":"www.workbuddy.cn"}
          }
        ]"#;
        let sessions = from_import_many(json).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(uid(&sessions[0]).as_deref(), Some("u-a"));
        assert_eq!(uid(&sessions[1]).as_deref(), Some("u-b"));
    }

    #[test]
    fn prefer_saved_plaintext_keeps_tokens_when_live_encrypted() {
        let saved = from_import(
            r#"{"account":{"uid":"u-1","nickname":"Jay"},"auth":{"accessToken":"plain-access","refreshToken":"plain-refresh"}}"#,
        )
        .unwrap();
        let live = from_import(
            r#"{"account":{"uid":"u-1","nickname":{"$wbEncrypted":1}},"auth":{"accessToken":{"$wbEncrypted":1,"envelope":"x"},"refreshToken":{"$wbEncrypted":1,"envelope":"y"},"domain":"www.workbuddy.cn"}}"#,
        )
        .unwrap();
        let merged = prefer_saved_plaintext(live, &saved);
        let profile = profile_value(&merged).unwrap();
        assert_eq!(access_token_plaintext(&profile).as_deref(), Some("plain-access"));
        assert_eq!(account_label(&profile).as_deref(), Some("Jay"));
        assert_eq!(
            profile.pointer("/auth/refreshToken").and_then(|v| v.as_str()),
            Some("plain-refresh")
        );
    }

}
