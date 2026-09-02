use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::cursor::oauth::{
    emit_official_login_status, open_browser, OauthLoginState, OfficialLoginStatus,
};
use crate::error::{AppError, Result};
use crate::grok::session::{
    email_from_jwt, oauth_auth_json, session_from_auth, user_id_from_jwt, XAI_CLIENT_ID, XAI_ISSUER,
};
use crate::models::{now, Account, ApplicationKind, ImportType};
use crate::store::AppState;
use crate::tray::refresh_tray;

const XAI_DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const USER_AGENT: &str = "storm-dock-grok-oauth";
const POLL_ATTEMPTS: u32 = 180;
const DEFAULT_TOKEN_LIFETIME_SECS: u64 = 3_600;

#[derive(Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    token_endpoint: String,
    device_authorization_endpoint: String,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Clone, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

struct DeviceFlow {
    device_code: String,
    user_code: String,
    verification_uri: String,
    token_endpoint: String,
    interval: Duration,
}

pub(crate) fn complete_grok_oauth(
    label: Option<String>,
    login_id: u64,
    app: AppHandle,
) -> Result<Account> {
    let oauth = app.state::<OauthLoginState>();
    let device = start_device_flow()?;
    oauth.set_url(login_id, device.verification_uri.clone())?;
    oauth.set_user_code(login_id, device.user_code.clone())?;
    emit_grok_login_status(
        &app,
        "started",
        Some(device.verification_uri.clone()),
        Some(device.user_code.clone()),
    );
    let _ = open_browser(&device.verification_uri);
    emit_grok_login_status(
        &app,
        "waiting",
        Some(device.verification_uri.clone()),
        Some(device.user_code.clone()),
    );
    let tokens = poll_device_flow(&device, login_id, &app)?;
    if !oauth.is_active(login_id) {
        return Err(AppError::LoginCancelled);
    }
    emit_official_login_status(&app, "importing", None);
    let session = session_from_tokens(&tokens)?;
    let state = app.state::<AppState>();
    let mut controller = state
        .0
        .lock()
        .map_err(|_| AppError::Message("账户存储不可用".into()))?;
    let account = controller.save_imported_session(
        ApplicationKind::Grok,
        label,
        session,
        ImportType::OAuth,
    )?;
    drop(controller);
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    oauth.finish(login_id);
    Ok(account)
}

fn start_device_flow() -> Result<DeviceFlow> {
    let endpoints = discover_endpoints()?;
    let response = http_client()?
        .post(&endpoints.device_authorization_endpoint)
        .header("User-Agent", USER_AGENT)
        .form(&[("client_id", XAI_CLIENT_ID), ("scope", XAI_SCOPE)])
        .send()
        .map_err(network)?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        return Err(AppError::Message(format!(
            "Grok 登录请求失败: {status} - {text}"
        )));
    }
    let device: DeviceCodeResponse = response.json().map_err(network)?;
    Ok(DeviceFlow {
        device_code: device.device_code,
        user_code: device.user_code.clone(),
        verification_uri: device
            .verification_uri_complete
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(device.verification_uri),
        token_endpoint: endpoints.token_endpoint,
        interval: Duration::from_secs(device.interval.unwrap_or(5).clamp(2, 15) + 3),
    })
}

fn discover_endpoints() -> Result<DiscoveryDocument> {
    let response = http_client()?
        .get(XAI_DISCOVERY_URL)
        .header("User-Agent", USER_AGENT)
        .send()
        .map_err(network)?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(AppError::Message(format!(
            "Grok 登录发现失败: HTTP {status}"
        )));
    }
    let document: DiscoveryDocument = response.json().map_err(network)?;
    if document.issuer.trim_end_matches('/') != XAI_ISSUER {
        return Err(AppError::Message("Grok 登录发现 issuer 不匹配".into()));
    }
    Ok(document)
}

fn poll_device_flow(
    device: &DeviceFlow,
    login_id: u64,
    app: &AppHandle,
) -> Result<OAuthTokenResponse> {
    let client = http_client()?;
    for _ in 0..POLL_ATTEMPTS {
        if !app.state::<OauthLoginState>().is_active(login_id) {
            return Err(AppError::LoginCancelled);
        }
        match poll_once(&client, device) {
            Ok(tokens) => return Ok(tokens),
            Err(AppError::LoginTimeout) => return Err(AppError::LoginTimeout),
            Err(AppError::LoginCancelled) => return Err(AppError::LoginCancelled),
            Err(error) if is_denied(&error) => return Err(error),
            Err(_) => std::thread::sleep(device.interval),
        }
    }
    Err(AppError::LoginTimeout)
}

fn poll_once(
    client: &reqwest::blocking::Client,
    device: &DeviceFlow,
) -> Result<OAuthTokenResponse> {
    let response = client
        .post(&device.token_endpoint)
        .header("User-Agent", USER_AGENT)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", XAI_CLIENT_ID),
            ("device_code", device.device_code.as_str()),
        ])
        .send()
        .map_err(network)?;
    let status = response.status();
    let tokens: OAuthTokenResponse = response.json().map_err(network)?;
    match tokens.error.as_deref() {
        Some("authorization_pending" | "slow_down") => {
            return Err(AppError::Message("等待用户授权".into()));
        }
        Some("access_denied") => {
            return Err(AppError::Message("用户拒绝授权".into()));
        }
        Some("expired_token") => return Err(AppError::LoginTimeout),
        Some(error) => {
            return Err(AppError::Message(format!("Grok 登录失败: {error}")));
        }
        None => {}
    }
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "Grok 登录轮询失败: HTTP {status}"
        )));
    }
    Ok(tokens)
}

fn session_from_tokens(tokens: &OAuthTokenResponse) -> Result<crate::models::Session> {
    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Message("登录响应缺少 refresh_token".into()))?;
    let user_id = [
        tokens.id_token.as_deref(),
        Some(tokens.access_token.as_str()),
    ]
    .into_iter()
    .flatten()
    .find_map(user_id_from_jwt);
    let email = [
        tokens.id_token.as_deref(),
        Some(tokens.access_token.as_str()),
    ]
    .into_iter()
    .flatten()
    .find_map(email_from_jwt);
    session_from_auth(
        oauth_auth_json(
            &tokens.access_token,
            refresh_token,
            user_id.as_deref(),
            email.as_deref(),
            &expires_at(tokens.expires_in),
        ),
        None,
    )
}

fn expires_at(expires_in: Option<u64>) -> String {
    let secs = now().saturating_add(expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS));
    OffsetDateTime::from_unix_timestamp(secs as i64)
        .ok()
        .and_then(|time| time.format(&Rfc3339).ok())
        .unwrap_or_else(|| secs.to_string())
}

fn is_denied(error: &AppError) -> bool {
    error.to_string().contains("拒绝")
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(network)
}

fn network(error: impl std::fmt::Display) -> AppError {
    AppError::Message(format!("Grok 登录网络错误: {error}"))
}

fn emit_grok_login_status(
    app: &AppHandle,
    stage: &'static str,
    login_url: Option<String>,
    user_code: Option<String>,
) {
    let _ = app.emit(
        "official-login-status",
        OfficialLoginStatus {
            stage,
            login_url,
            user_code,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_client_matches_grok_cli() {
        assert_eq!(XAI_CLIENT_ID, "b1a00492-073a-47ea-816f-4c329264a828");
        assert_eq!(
            XAI_DISCOVERY_URL,
            "https://auth.x.ai/.well-known/openid-configuration"
        );
        assert!(expires_at(Some(60)).contains('T'));
    }
}
