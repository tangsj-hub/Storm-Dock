use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::cursor::oauth::{emit_official_login_status, OauthLoginState};
use crate::error::{AppError, Result};
use crate::http::{Body, Budget, Call, Client, Retry, USAGE_BUDGET};
use crate::models::{Account, ApplicationKind, ImportType};
use crate::store::AppState;
use crate::workbuddy::session::{
    profile_from_tokens, session_from_profile, DEFAULT_DOMAIN,
};

const USER_AGENT: &str = "Storm-Dock/workbuddy-oauth";
const POLL_ATTEMPTS: u32 = 120;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

const STATE_ENDPOINTS: &[&str] = &[
    "https://www.workbuddy.cn/v2/plugin/auth/state",
    "https://copilot.tencent.com/v2/plugin/auth/state",
];

const TOKEN_ENDPOINTS: &[&str] = &[
    "https://www.workbuddy.cn/v2/plugin/auth/token",
    "https://copilot.tencent.com/v2/plugin/auth/token",
];

const ACCOUNT_ENDPOINTS: &[&str] = &[
    "https://www.workbuddy.cn/v2/plugin/account",
    "https://copilot.tencent.com/v2/plugin/account",
];

#[derive(Clone)]
struct DeviceState {
    state: String,
    auth_url: String,
    state_base: String,
}

#[derive(Deserialize)]
struct PluginEnvelope<T> {
    code: i64,
    #[serde(default)]
    msg: Option<String>,
    data: Option<T>,
}

#[derive(Deserialize)]
struct StateData {
    state: String,
    #[serde(default, alias = "url")]
    auth_url: Option<String>,
    #[serde(default, rename = "authUrl")]
    auth_url_camel: Option<String>,
}

#[derive(Deserialize)]
struct TokenData {
    #[serde(default, alias = "access_token")]
    access_token: Option<String>,
    #[serde(default, rename = "accessToken")]
    access_token_camel: Option<String>,
    #[serde(default, alias = "refresh_token")]
    refresh_token: Option<String>,
    #[serde(default, rename = "refreshToken")]
    refresh_token_camel: Option<String>,
    #[serde(default, alias = "token_type")]
    token_type: Option<String>,
    #[serde(default, rename = "tokenType")]
    token_type_camel: Option<String>,
    #[serde(default, alias = "expires_in")]
    expires_in: Option<u64>,
    #[serde(default, rename = "expiresIn")]
    expires_in_camel: Option<u64>,
    #[serde(default, alias = "refresh_expires_in")]
    refresh_expires_in: Option<u64>,
    #[serde(default, rename = "refreshExpiresIn")]
    refresh_expires_in_camel: Option<u64>,
    #[serde(default, alias = "user_id")]
    user_id: Option<String>,
    #[serde(default, rename = "userId")]
    user_id_camel: Option<String>,
}

impl TokenData {
    fn access_token(&self) -> Option<&str> {
        self.access_token_camel
            .as_deref()
            .or(self.access_token.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn refresh_token(&self) -> Option<&str> {
        self.refresh_token_camel
            .as_deref()
            .or(self.refresh_token.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn user_id(&self) -> Option<&str> {
        self.user_id_camel
            .as_deref()
            .or(self.user_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn expires_in(&self) -> Option<u64> {
        self.expires_in_camel.or(self.expires_in)
    }

    fn refresh_expires_in(&self) -> Option<u64> {
        self.refresh_expires_in_camel.or(self.refresh_expires_in)
    }
}

pub(crate) fn complete_workbuddy_oauth(
    label: Option<String>,
    login_id: u64,
    app: AppHandle,
) -> Result<Account> {
    let oauth = app.state::<OauthLoginState>();
    let device = start_device_flow()?;
    oauth.set_url(login_id, device.auth_url.clone())?;
    emit_workbuddy_login_status(&app, "started", Some(device.auth_url.clone()));
    let _ = crate::browser::open(&device.auth_url, &oauth.browser());
    emit_workbuddy_login_status(&app, "waiting", Some(device.auth_url.clone()));
    let tokens = poll_device_flow(&device, login_id, &app)?;
    if !oauth.is_active(login_id) {
        return Err(AppError::LoginCancelled);
    }
    emit_official_login_status(&app, "importing", None);
    let session = session_from_tokens(&tokens, &device)?;
    let state = app.state::<AppState>();
    let mut controller = state
        .0
        .lock()
        .map_err(|_| AppError::Message("账户存储不可用".into()))?;
    let account = controller.save_imported_session(
        ApplicationKind::Workbuddy,
        label,
        session,
        ImportType::OAuth,
    )?;
    // Official WorkBuddy login should land in the desktop app as well — unlike
    // Cursor (which keeps desktop apply as an explicit switch), the browser
    // flow is the same path WorkBuddy itself uses to write workbuddy-desktop.info.
    let account_id = account.id.clone();
    if let Err(error) = controller.apply_account(&account_id) {
        drop(controller);
        oauth.finish(login_id);
        return Err(AppError::Message(format!(
            "账号已导入 Storm Dock，但写入 WorkBuddy 桌面端失败：{error}"
        )));
    }
    let account = controller.account(&account_id).unwrap_or(account);
    drop(controller);
    // Inline refresh so the home list shows 体验版 / credits right after login,
    // matching Cursor/Codex post-import behavior (no empty-plan race).
    let account = crate::workbuddy::usage::refresh_imported_account_now(&app, account);
    oauth.finish(login_id);
    Ok(account)
}

fn plugin_headers() -> Vec<(String, String)> {
    vec![
        ("Content-Type".into(), "application/json".into()),
        ("Accept".into(), "application/json".into()),
        ("User-Agent".into(), USER_AGENT.into()),
        ("X-Product".into(), "SaaS".into()),
        ("X-No-Authorization".into(), "true".into()),
        ("X-No-User-Id".into(), "true".into()),
        ("X-No-Enterprise-Id".into(), "true".into()),
        ("X-No-Department-Info".into(), "true".into()),
    ]
}

fn start_device_flow() -> Result<DeviceState> {
    let mut last_error = AppError::Message("WorkBuddy 登录请求失败".into());
    for endpoint in STATE_ENDPOINTS {
        for platform in ["workbuddy", "CLI"] {
            match request_state(endpoint, platform) {
                Ok(device) => return Ok(device),
                Err(error) => last_error = error,
            }
        }
    }
    Err(last_error)
}

fn request_state(endpoint: &str, platform: &str) -> Result<DeviceState> {
    let response = Client::shared().send(
        &Call {
            method: reqwest::Method::POST,
            url: format!("{endpoint}?platform={platform}"),
            headers: plugin_headers(),
            query: Vec::new(),
            body: Body::Json(serde_json::json!({ "platform": platform })),
        },
        &Budget::new(USAGE_BUDGET),
        Retry::Transient,
    )?;
    if !(200..300).contains(&response.status) {
        return Err(AppError::Message(format!(
            "WorkBuddy 登录请求失败: {} - {}",
            response.status,
            response.text()
        )));
    }
    let envelope: PluginEnvelope<StateData> = response.json()?;
    if envelope.code != 0 {
        return Err(AppError::Message(format!(
            "WorkBuddy 登录请求失败: {}",
            envelope.msg.unwrap_or_else(|| format!("code {}", envelope.code))
        )));
    }
    let data = envelope
        .data
        .ok_or_else(|| AppError::Message("WorkBuddy 登录响应缺少 data".into()))?;
    let auth_url = data
        .auth_url_camel
        .or(data.auth_url)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Message("WorkBuddy 登录响应缺少授权链接".into()))?;
    Ok(DeviceState {
        state: data.state,
        auth_url,
        state_base: endpoint.trim_end_matches("/state").to_owned(),
    })
}

fn poll_device_flow(
    device: &DeviceState,
    login_id: u64,
    app: &AppHandle,
) -> Result<TokenData> {
    for _ in 0..POLL_ATTEMPTS {
        if !app.state::<OauthLoginState>().is_active(login_id) {
            return Err(AppError::LoginCancelled);
        }
        match poll_once(device) {
            Ok(Some(tokens)) => return Ok(tokens),
            Ok(None) => crate::cursor::oauth::wait_if_active(app, login_id, POLL_INTERVAL)?,
            Err(AppError::LoginCancelled) => return Err(AppError::LoginCancelled),
            Err(_) => crate::cursor::oauth::wait_if_active(app, login_id, POLL_INTERVAL)?,
        }
    }
    Err(AppError::Message("WorkBuddy 官方登录超时，请重试".into()))
}

fn poll_once(device: &DeviceState) -> Result<Option<TokenData>> {
    let mut endpoints = vec![format!("{}/token", device.state_base)];
    for endpoint in TOKEN_ENDPOINTS {
        if !endpoints.iter().any(|item| item == endpoint) {
            endpoints.push((*endpoint).to_owned());
        }
    }
    let mut pending = false;
    let mut last_error = None;
    for endpoint in endpoints {
        let response = Client::shared().send(
            &Call {
                method: reqwest::Method::GET,
                url: format!("{endpoint}?state={}", urlencoding_lite(&device.state)),
                headers: plugin_headers(),
                query: Vec::new(),
                body: Body::Empty,
            },
            &Budget::new(USAGE_BUDGET),
            Retry::None,
        );
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(error.into());
                continue;
            }
        };
        if !(200..300).contains(&response.status) {
            last_error = Some(AppError::Message(format!(
                "WorkBuddy 登录轮询失败: {} - {}",
                response.status,
                response.text()
            )));
            continue;
        }
        let envelope: PluginEnvelope<TokenData> = match response.json() {
            Ok(value) => value,
            Err(error) => {
                last_error = Some(error.into());
                continue;
            }
        };
        // 11217 = still waiting for browser authorization (one-shot token).
        if envelope.code == 11217 {
            pending = true;
            continue;
        }
        if envelope.code == 0 {
            if let Some(data) = envelope.data {
                if data.access_token().is_some() {
                    return Ok(Some(data));
                }
            }
            return Err(AppError::Message(
                "WorkBuddy 登录成功但未返回 accessToken".into(),
            ));
        }
        last_error = Some(AppError::Message(format!(
            "WorkBuddy 登录失败: {}",
            envelope
                .msg
                .unwrap_or_else(|| format!("code {}", envelope.code))
        )));
    }
    if pending {
        return Ok(None);
    }
    Err(last_error.unwrap_or_else(|| AppError::Message("WorkBuddy 登录轮询失败".into())))
}

fn session_from_tokens(tokens: &TokenData, device: &DeviceState) -> Result<crate::models::Session> {
    let access = tokens
        .access_token()
        .ok_or_else(|| AppError::Message("WorkBuddy accessToken 缺失".into()))?;
    let refresh = tokens.refresh_token();
    let user_id = tokens.user_id().map(str::to_owned);
    let account = fetch_account(access, user_id.as_deref());
    let email = account
        .as_ref()
        .and_then(|value| {
            value
                .get("nickname")
                .or_else(|| value.get("email"))
                .or_else(|| value.get("userName"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
    let resolved_uid = account
        .as_ref()
        .and_then(|value| {
            value
                .get("uid")
                .or_else(|| value.get("userId"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
        .or(user_id);
    let domain = device
        .auth_url
        .split('/')
        .nth(2)
        .unwrap_or(DEFAULT_DOMAIN);
    let mut profile = profile_from_tokens(
        access,
        refresh,
        email.as_deref(),
        resolved_uid.as_deref(),
        Some(domain),
        "oauth",
    );
    if let Some(account) = account {
        if let Some(root) = profile.as_object_mut() {
            root.insert("account".into(), account.clone());
            root.insert("accounts".into(), serde_json::json!([account]));
            root.insert("allAccounts".into(), serde_json::json!([account]));
        }
    }
    if let Some(auth) = profile.get_mut("auth").and_then(|value| value.as_object_mut()) {
        if let Some(expires_in) = tokens.expires_in() {
            auth.insert("expiresIn".into(), serde_json::Value::from(expires_in));
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            auth.insert(
                "expiresAt".into(),
                serde_json::Value::from(now_ms + expires_in.saturating_mul(1000)),
            );
        }
        if let Some(refresh_expires_in) = tokens.refresh_expires_in() {
            auth.insert(
                "refreshExpiresIn".into(),
                serde_json::Value::from(refresh_expires_in),
            );
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            auth.insert(
                "refreshExpiresAt".into(),
                serde_json::Value::from(now_ms + refresh_expires_in.saturating_mul(1000)),
            );
        }
        if let Some(token_type) = tokens
            .token_type_camel
            .as_deref()
            .or(tokens.token_type.as_deref())
        {
            auth.insert(
                "tokenType".into(),
                serde_json::Value::String(token_type.to_owned()),
            );
        }
        auth.insert(
            "sessionState".into(),
            serde_json::Value::String(device.state.clone()),
        );
    }
    session_from_profile(profile, "oauth")
}

fn fetch_account(access_token: &str, fallback_uid: Option<&str>) -> Option<serde_json::Value> {
    for endpoint in ACCOUNT_ENDPOINTS {
        let response = Client::shared().send(
            &Call {
                method: reqwest::Method::GET,
                url: (*endpoint).into(),
                headers: vec![
                    ("Accept".into(), "application/json".into()),
                    ("User-Agent".into(), USER_AGENT.into()),
                    ("X-Product".into(), "SaaS".into()),
                    (
                        "Authorization".into(),
                        format!("Bearer {access_token}"),
                    ),
                ],
                query: Vec::new(),
                body: Body::Empty,
            },
            &Budget::new(USAGE_BUDGET),
            Retry::Transient,
        );
        let Ok(response) = response else {
            continue;
        };
        if !(200..300).contains(&response.status) {
            continue;
        }
        let Ok(envelope) = response.json::<PluginEnvelope<serde_json::Value>>() else {
            continue;
        };
        if envelope.code != 0 {
            continue;
        }
        match envelope.data {
            Some(serde_json::Value::Object(map)) => {
                return Some(serde_json::Value::Object(map));
            }
            Some(serde_json::Value::Array(mut values)) => {
                if let Some(first) = values
                    .iter()
                    .find(|value| value.is_object())
                    .cloned()
                    .or_else(|| values.pop())
                {
                    return Some(first);
                }
            }
            _ => {}
        }
    }
    fallback_uid.map(|uid| {
        serde_json::json!({
            "uid": uid,
            "userId": uid,
            "nickname": uid,
        })
    })
}

fn emit_workbuddy_login_status(
    app: &AppHandle,
    stage: &'static str,
    login_url: Option<String>,
) {
    emit_official_login_status(app, stage, login_url);
}

fn urlencoding_lite(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
