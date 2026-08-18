use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, Date, Duration as TimeDuration, OffsetDateTime};

const DATABASE_NAME: &str = "storm-dock.db";
const DATABASE_PATH_FILE: &str = "database-path.txt";
const CURSOR_KEYS: [&str; 7] = [
    "cursorAuth/accessToken",
    "cursorAuth/refreshToken",
    "cursorAuth/cachedEmail",
    "cursorAuth/cachedScopedProfile",
    "cursorAuth/cachedSignUpType",
    "cursorAuth/stripeMembershipType",
    "glass.lastSignedInAuthId",
];
const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
const REFRESH_TOKEN_KEY: &str = "cursorAuth/refreshToken";
const EMAIL_KEY: &str = "cursorAuth/cachedEmail";
const AUTH_ID_KEY: &str = "glass.lastSignedInAuthId";
const MEMBERSHIP_TYPE_KEY: &str = "cursorAuth/stripeMembershipType";
const CURSOR_SUBSCRIPTION_URL: &str = "https://api2.cursor.sh/auth/full_stripe_profile";
const CURSOR_DASHBOARD_URL: &str = "https://cursor.com/api";
const USAGE_EVENTS_PAGE_SIZE: u32 = 100;
const USAGE_EVENTS_MAX_PAGES: u32 = 5;
const CURSOR_OAUTH_LOGIN_URL: &str = "https://cursor.com/loginDeepControl";
const CURSOR_OAUTH_POLL_URL: &str = "https://api2.cursor.sh/auth/poll";
const CURSOR_OAUTH_POLL_ATTEMPTS: u32 = 150;
const CURSOR_OAUTH_MAX_ERRORS: u32 = 3;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
enum ApplicationKind {
    Cursor,
    Codex,
}

impl ApplicationKind {
    fn display_name(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor",
            Self::Codex => "Codex",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationStatus {
    kind: ApplicationKind,
    label: String,
    available: bool,
    reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    id: String,
    application: ApplicationKind,
    label: String,
    email: Option<String>,
    #[serde(default)]
    import_type: ImportType,
    #[serde(default)]
    subscription: SubscriptionSummary,
    #[serde(default)]
    raw_export: serde_json::Value,
    created_at: u64,
    updated_at: u64,
    last_used_at: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountSummary {
    id: String,
    label: String,
    email: Option<String>,
    import_type: ImportType,
    subscription: SubscriptionSummary,
    days_remaining: Option<i64>,
    is_current: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionSummary {
    plan: Option<String>,
    expires_at: Option<u64>,
    checked_at: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CursorUsageDetails {
    account_id: String,
    label: String,
    email: Option<String>,
    name: Option<String>,
    membership_type: Option<String>,
    primary: UsageMetric,
    reset_at: Option<String>,
    on_demand: Option<UsageMetric>,
    models: Vec<ModelUsageSummary>,
    weekly: Vec<WeeklyUsageSummary>,
    weekly_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    weekly_error: Option<String>,
    #[serde(default)]
    events: Vec<UsageEvent>,
    checked_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageEvent {
    timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    requests: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    charged_cents: Option<f64>,
    on_demand: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetric {
    kind: String,
    used: f64,
    limit: Option<f64>,
    percent: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelUsageSummary {
    name: String,
    requests: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WeeklyUsageSummary {
    date: String,
    requests: f64,
    on_demand_cents: f64,
    is_on_demand: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchProgress {
    operation_id: String,
    account_id: String,
    stage: &'static str,
    percent: u8,
    status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchOutcome {
    restart_required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ImportType {
    OAuth,
    Token,
    Jwt,
    Native,
}

impl Default for ImportType {
    fn default() -> Self {
        Self::Native
    }
}

impl ImportType {
    fn supports_desktop_switch(&self) -> bool {
        matches!(self, Self::OAuth | Self::Native)
    }
}

fn import_type(session: &Session) -> ImportType {
    if session.values.contains_key(REFRESH_TOKEN_KEY) {
        ImportType::OAuth
    } else if session
        .values
        .get(ACCESS_TOKEN_KEY)
        .is_some_and(|token| token.matches('.').count() == 2)
    {
        ImportType::Jwt
    } else {
        ImportType::Token
    }
}

fn subscription_from_session(session: &Session) -> SubscriptionSummary {
    SubscriptionSummary {
        plan: session
            .values
            .get(MEMBERSHIP_TYPE_KEY)
            .cloned()
            .filter(|value| !value.is_empty()),
        ..Default::default()
    }
}

fn matching_account_index(
    accounts: &[Account],
    kind: ApplicationKind,
    email: &str,
) -> Option<usize> {
    accounts.iter().position(|account| {
        account.application == kind
            && account
                .email
                .as_deref()
                .is_some_and(|existing| existing.eq_ignore_ascii_case(email))
    })
}

fn parse_timestamp(value: &serde_json::Value) -> Option<u64> {
    if let Some(number) = value.as_u64().or_else(|| value.as_f64().map(|number| number as u64)) {
        return Some(if number > 10_000_000_000 {
            number / 1_000
        } else {
            number
        });
    }
    let text = value.as_str()?.trim();
    OffsetDateTime::parse(text, &Rfc3339)
        .ok()
        .map(|time| time.unix_timestamp() as u64)
}

fn utc_days_remaining(expires_at: u64, current_time: u64) -> i64 {
    (expires_at / 86_400) as i64 - (current_time / 86_400) as i64
}

fn subscription_from_response(value: &serde_json::Value) -> SubscriptionSummary {
    fn find<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a serde_json::Value> {
        match value {
            serde_json::Value::Object(object) => keys
                .iter()
                .find_map(|key| object.get(*key))
                .or_else(|| object.values().find_map(|item| find(item, keys))),
            serde_json::Value::Array(items) => items.iter().find_map(|item| find(item, keys)),
            _ => None,
        }
    }
    SubscriptionSummary {
        plan: find(value, &["stripeMembershipType", "membershipType", "plan"])
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        expires_at: find(
            value,
            &[
                "billingCycleEnd",
                "billing_cycle_end",
                "currentPeriodEnd",
                "current_period_end",
                "expiresAt",
                "expires_at",
                "subscriptionEnd",
            ],
        )
        .and_then(parse_timestamp),
        checked_at: Some(now()),
    }
}

fn merge_subscription(
    primary: Option<SubscriptionSummary>,
    fallback: Option<SubscriptionSummary>,
) -> Option<SubscriptionSummary> {
    match (primary, fallback) {
        (None, None) => None,
        (Some(primary), None) => Some(primary),
        (None, Some(fallback)) => Some(fallback),
        (Some(primary), Some(fallback)) => Some(SubscriptionSummary {
            plan: primary.plan.or(fallback.plan),
            expires_at: primary.expires_at.or(fallback.expires_at),
            checked_at: primary.checked_at.or(fallback.checked_at),
        }),
    }
}

fn fetch_stripe_profile(session: &Session) -> Result<serde_json::Value> {
    let token = session
        .values
        .get(ACCESS_TOKEN_KEY)
        .ok_or(AppError::SecretMissing)?;
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| {
            AppError::Message(format!(
                "could not prepare Cursor subscription refresh: {error}"
            ))
        })?
        .get(CURSOR_SUBSCRIPTION_URL)
        .bearer_auth(token)
        .send()
        .map_err(|error| {
            AppError::Message(format!("could not refresh Cursor subscription: {error}"))
        })?
        .error_for_status()
        .map_err(|error| {
            AppError::Message(format!("could not refresh Cursor subscription: {error}"))
        })?;
    response.json().map_err(|error| {
        AppError::Message(format!("could not read Cursor subscription: {error}"))
    })
}

fn fetch_cursor_subscription(session: &Session) -> Result<SubscriptionSummary> {
    let usage = dashboard_cookie(session)
        .ok()
        .and_then(|cookie| dashboard_request(&cookie, "/usage-summary", None).ok());
    let stripe = fetch_stripe_profile(session);
    merge_subscription(
        usage.as_ref().map(subscription_from_response),
        stripe.as_ref().ok().map(subscription_from_response),
    )
    .ok_or_else(|| {
        stripe.err().unwrap_or_else(|| {
            AppError::Message("could not refresh Cursor subscription".into())
        })
    })
}

fn jwt_claims(token: &str) -> Option<serde_json::Value> {
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

fn jwt_claim_text(claims: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| claims.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn session_user_id(session: &Session) -> Option<String> {
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

fn session_display_label(session: &Session) -> Option<String> {
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

fn dashboard_cookie(session: &Session) -> Result<String> {
    let token = session
        .values
        .get(ACCESS_TOKEN_KEY)
        .ok_or(AppError::SecretMissing)?;
    let user_id = session_user_id(session).ok_or_else(|| {
        AppError::Message("此账户不是可用于 Cursor 用量查询的 JWT，请重新导入账户。".into())
    })?;
    if let Some(claims) = jwt_claims(token) {
        if let Some(exp) = claims.get("exp").and_then(serde_json::Value::as_i64) {
            if exp <= now() as i64 + 60 {
                return Err(AppError::Message(
                    "Cursor 登录已过期，请在 Cursor 中重新登录后重新导入账户。".into(),
                ));
            }
        }
    }
    Ok(format!("WorkosCursorSessionToken={user_id}%3A%3A{token}"))
}

fn dashboard_request(
    cookie: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().host_str() == Some("cursor.com") && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| AppError::Message(format!("无法创建 Cursor 用量请求: {error}")))?;
    let url = format!("{CURSOR_DASHBOARD_URL}{path}");
    let mut request = match &body {
        Some(body) => client.post(url).json(body),
        None => client.get(url),
    }
    .header("Cookie", cookie)
    .header("User-Agent", "Mozilla/5.0")
    .header("Accept", "*/*");
    if body.is_some() || path.contains("dashboard/") {
        request = request
            .header("Origin", "https://cursor.com")
            .header("Referer", "https://cursor.com/dashboard?tab=usage")
            .header("Sec-Fetch-Site", "same-origin")
            .header("Sec-Fetch-Mode", "cors")
            .header("Sec-Fetch-Dest", "empty");
    }
    let response = request
        .send()
        .map_err(|error| AppError::Message(format!("Cursor 用量查询失败: {error}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .map_err(|error| AppError::Message(format!("无法读取 Cursor 用量数据: {error}")))?;
    if status == 204 || bytes.is_empty() {
        return Err(AppError::Message(
            "Cursor 登录已失效，请在 Cursor 中重新登录后重新导入账户。".into(),
        ));
    }
    let parsed = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
    let api_error = parsed.as_ref().and_then(|value| {
        value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    });
    match status {
        401 => {
            return Err(AppError::Message(
                "Cursor 登录已失效，请在 Cursor 中重新登录后重新导入账户。".into(),
            ))
        }
        403 if body.is_none() => {
            return Err(AppError::Message(
                "Cursor 登录已失效，请在 Cursor 中重新登录后重新导入账户。".into(),
            ))
        }
        status if !(200..300).contains(&status) => {
            return Err(AppError::Message(format!(
                "Cursor 用量查询失败（{}）。",
                api_error.unwrap_or_else(|| format!("HTTP {status}"))
            )))
        }
        _ => {}
    }
    if let Some(error) = api_error {
        return Err(AppError::Message(format!("Cursor 用量查询失败（{error}）。")));
    }
    parsed.ok_or_else(|| AppError::Message("无法读取 Cursor 用量数据。".into()))
}

fn number_at(value: &serde_json::Value, path: &[&str]) -> Option<f64> {
    path.iter()
        .try_fold(value, |value, key| value.get(*key))
        .and_then(json_number)
}

fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_u64().map(|number| number as f64))
        .or_else(|| value.as_i64().map(|number| number as f64))
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .filter(|number| number.is_finite())
}

fn json_i64(value: &serde_json::Value) -> Option<i64> {
    json_number(value).and_then(|number| {
        (number.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(&number))
            .then_some(number as i64)
    })
}

fn normalized_email(value: &str) -> Option<String> {
    let email = value.trim().to_ascii_lowercase();
    (!email.is_empty()).then_some(email)
}

fn first_team_id(teams: &serde_json::Value) -> Option<i64> {
    teams
        .get("teams")
        .and_then(serde_json::Value::as_array)
        .or_else(|| teams.as_array())
        .into_iter()
        .flatten()
        .find_map(|team| team.get("id").or_else(|| team.get("teamId")).and_then(json_i64))
}

fn team_member_user_id(spend: &serde_json::Value, emails: &[String]) -> Option<i64> {
    let wanted: Vec<String> = emails.iter().filter_map(|email| normalized_email(email)).collect();
    if wanted.is_empty() {
        return None;
    }
    spend
        .get("teamMemberSpend")
        .or_else(|| spend.pointer("/data/teamMemberSpend"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|member| {
            let email = ["email", "userEmail", "cursorEmail"]
                .iter()
                .find_map(|key| member.get(*key).and_then(serde_json::Value::as_str))
                .and_then(normalized_email)?;
            if !wanted.iter().any(|value| value == &email) {
                return None;
            }
            member
                .get("userId")
                .or_else(|| member.get("id"))
                .and_then(json_i64)
        })
}

fn event_timestamp_ms(event: &serde_json::Value) -> Option<u64> {
    let value = event.get("timestamp")?;
    let number = json_i64(value)
        .map(|number| number as u64)
        .or_else(|| value.as_str()?.parse::<u64>().ok())?;
    Some(if number > 10_000_000_000 {
        number
    } else {
        number * 1000
    })
}

fn event_date(event: &serde_json::Value) -> Option<Date> {
    let ms = event_timestamp_ms(event)? as i128;
    OffsetDateTime::from_unix_timestamp_nanos(ms * 1_000_000)
        .ok()
        .map(|time| time.date())
}

fn events_range_ms() -> (String, String) {
    let end = OffsetDateTime::now_utc();
    let start = end - TimeDuration::days(7);
    (
        (start.unix_timestamp_nanos() / 1_000_000).to_string(),
        (end.unix_timestamp_nanos() / 1_000_000).to_string(),
    )
}

fn auth_numeric_id(me: &serde_json::Value) -> Option<i64> {
    ["id", "userId", "user_id"]
        .iter()
        .find_map(|key| me.get(*key).and_then(json_i64))
}

fn usage_events_from_response(value: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    let item = ["usageEventsDisplay", "usageEvents", "events"]
        .iter()
        .find_map(|key| value.get(*key))
        .or_else(|| value.pointer("/data/usageEventsDisplay"));
    match item {
        None | Some(serde_json::Value::Null) => Some(Vec::new()),
        Some(serde_json::Value::Array(items)) => Some(items.clone()),
        _ => None,
    }
}

fn usage_events_body(team_id: Option<i64>, user_id: Option<i64>, page: u32, dated: bool) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("page".into(), serde_json::Value::from(page));
    body.insert("pageSize".into(), serde_json::Value::from(USAGE_EVENTS_PAGE_SIZE));
    if let Some(team_id) = team_id {
        body.insert("teamId".into(), serde_json::Value::from(team_id));
    }
    if let Some(user_id) = user_id {
        body.insert("userId".into(), serde_json::Value::from(user_id));
    }
    if dated {
        let (start, end) = events_range_ms();
        body.insert("startDate".into(), serde_json::Value::String(start));
        body.insert("endDate".into(), serde_json::Value::String(end));
    }
    serde_json::Value::Object(body)
}

fn collect_usage_event_pages(
    cookie: &str,
    team_id: Option<i64>,
    user_id: Option<i64>,
    dated: bool,
) -> Result<Vec<serde_json::Value>> {
    let cutoff = OffsetDateTime::now_utc().date() - TimeDuration::days(6);
    let mut collected = Vec::new();
    for page in 1..=USAGE_EVENTS_MAX_PAGES {
        let response = dashboard_request(
            cookie,
            "/dashboard/get-filtered-usage-events",
            Some(usage_events_body(team_id, user_id, page, dated)),
        )?;
        let Some(events) = usage_events_from_response(&response) else {
            break;
        };
        if events.is_empty() {
            break;
        }
        let oldest = events.iter().filter_map(event_date).min();
        let page_len = events.len();
        collected.extend(events);
        if oldest.is_some_and(|date| date < cutoff) || page_len < USAGE_EVENTS_PAGE_SIZE as usize {
            break;
        }
    }
    Ok(collected)
}

fn collect_usage_events(
    cookie: &str,
    team_id: Option<i64>,
    user_id: Option<i64>,
) -> Result<serde_json::Value> {
    // Personal accounts: CursorMeter sends only teamId 0 / page / pageSize.
    // Dated bodies are used by the dashboard, but free plans often reply without
    // usageEventsDisplay; treat that as empty and retry the undated shape.
    let personal = team_id == Some(0) && user_id.is_none();
    let shapes = if personal {
        vec![(Some(0), None, false), (None, None, false)]
    } else {
        vec![(team_id, user_id, false), (team_id, user_id, true)]
    };
    let mut collected = Vec::new();
    let mut last_error = None;
    for (next_team_id, next_user_id, dated) in shapes {
        match collect_usage_event_pages(cookie, next_team_id, next_user_id, dated) {
            Ok(events) => {
                collected = events;
                if !collected.is_empty() {
                    break;
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    if collected.is_empty() {
        if let Some(error) = last_error {
            return Err(error);
        }
    }
    Ok(serde_json::json!({ "usageEventsDisplay": collected }))
}

fn text_at(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value, |value, key| value.get(*key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn json_model_name(value: &serde_json::Value) -> Option<String> {
    if let Some(name) = value.as_str().map(str::trim).filter(|name| !name.is_empty()) {
        return Some(name.to_owned());
    }
    ["name", "model", "modelName", "displayName"]
        .iter()
        .find_map(|key| {
            value
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
}

fn event_model_name(event: &serde_json::Value) -> Option<String> {
    ["model", "modelName", "model_name"]
        .iter()
        .find_map(|key| event.get(*key).and_then(json_model_name))
}

fn event_request_weight(event: &serde_json::Value) -> f64 {
    event.get("requestsCosts").and_then(json_number).map(|value| value.max(0.0)).unwrap_or(0.0)
}

fn event_number(event: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| event.get(*key).and_then(json_number))
        .filter(|value| value.is_finite())
}

fn usage_event_from_value(event: &serde_json::Value) -> Option<UsageEvent> {
    Some(UsageEvent {
        timestamp: event_timestamp_ms(event)?,
        model: event_model_name(event),
        requests: event_request_weight(event),
        input_tokens: event_number(event, &["inputTokens", "input_tokens", "inputTokenCount"]),
        output_tokens: event_number(event, &["outputTokens", "output_tokens", "outputTokenCount"]),
        cost_usd: event_number(event, &["costUsd", "cost_usd", "costUSD"]),
        charged_cents: event_number(event, &["chargedCents", "charged_cents"]),
        on_demand: event.get("kind").and_then(serde_json::Value::as_str)
            == Some("USAGE_EVENT_KIND_USAGE_BASED"),
    })
}

fn usage_event_rows(events: &serde_json::Value) -> Vec<UsageEvent> {
    let mut rows: Vec<_> = events
        .get("usageEventsDisplay")
        .or_else(|| events.get("usageEvents"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(usage_event_from_value)
        .collect();
    rows.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    rows
}

fn sort_models(models: &mut [ModelUsageSummary]) {
    models.sort_by(|left, right| {
        right
            .requests
            .cmp(&left.requests)
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn models_from_events(events: &serde_json::Value) -> Vec<ModelUsageSummary> {
    let mut by_model: BTreeMap<String, f64> = BTreeMap::new();
    for event in events.get("usageEventsDisplay").and_then(serde_json::Value::as_array).into_iter().flatten() {
        let Some(name) = event_model_name(event) else { continue; };
        *by_model.entry(name).or_default() += event_request_weight(event);
    }
    let mut models: Vec<_> = by_model
        .into_iter()
        .filter(|(_, requests)| *requests > 0.0)
        .map(|(name, requests)| ModelUsageSummary {
            name,
            requests: requests.round() as u64,
        })
        .collect();
    sort_models(&mut models);
    models
}

fn usage_metric(kind: &str, used: f64, limit: Option<f64>) -> UsageMetric {
    let percent = limit
        .filter(|limit| *limit > 0.0)
        .map(|limit| used / limit * 100.0)
        .unwrap_or(0.0);
    UsageMetric {
        kind: kind.into(),
        used,
        limit,
        percent,
    }
}

fn weekly_usage(events: &serde_json::Value) -> Option<Vec<WeeklyUsageSummary>> {
    let today = OffsetDateTime::now_utc().date();
    let days: Vec<Date> = (0..7)
        .rev()
        .map(|day| today - TimeDuration::days(day))
        .collect();
    let mut values: BTreeMap<Date, (f64, f64, bool)> = BTreeMap::new();
    for event in events.get("usageEventsDisplay")?.as_array()? {
        let Some(date) = event_date(event) else { continue; };
        if !days.contains(&date) {
            continue;
        }
        let item = values.entry(date).or_default();
        item.0 += event.get("requestsCosts").and_then(json_number).unwrap_or(0.0);
        if event.get("kind").and_then(serde_json::Value::as_str) == Some("USAGE_EVENT_KIND_USAGE_BASED") {
            item.1 += event.get("chargedCents").and_then(json_number).unwrap_or(0.0);
            item.2 = true;
        }
    }
    Some(
        days.into_iter()
            .map(|date| {
                let (requests, on_demand_cents, is_on_demand) =
                    values.get(&date).copied().unwrap_or_default();
                WeeklyUsageSummary {
                    date: date.to_string(),
                    requests,
                    on_demand_cents,
                    is_on_demand,
                }
            })
            .collect(),
    )
}

fn percent_from_message(text: Option<&str>) -> Option<f64> {
    let text = text?;
    let end = text.find('%')?;
    let head = &text[..end];
    let start = head
        .rfind(|character: char| !(character.is_ascii_digit() || character == '.'))
        .map(|index| index + 1)
        .unwrap_or(0);
    head.get(start..)?.parse().ok().filter(|value: &f64| value.is_finite())
}

fn percent_metric(percent: f64) -> UsageMetric {
    UsageMetric {
        kind: "percent".into(),
        used: percent,
        limit: None,
        percent,
    }
}

fn with_percent(mut metric: UsageMetric, percent: Option<f64>) -> UsageMetric {
    if let Some(percent) = percent {
        metric.percent = percent;
    }
    metric
}

fn plan_breakdown_total(summary: &serde_json::Value) -> Option<f64> {
    number_at(summary, &["individualUsage", "plan", "breakdown", "total"]).filter(|value| *value > 0.0)
}

fn per_user_limit_cents(summary: &serde_json::Value, hard_limit: Option<&serde_json::Value>) -> Option<f64> {
    hard_limit
        .and_then(|value| number_at(value, &["perUserMonthlyLimitDollars"]))
        .or_else(|| number_at(summary, &["hard_limit", "perUserMonthlyLimitDollars"]))
        .or_else(|| number_at(summary, &["perUserMonthlyLimitDollars"]))
        .filter(|value| *value > 0.0)
        .map(|value| value * 100.0)
}

fn inferred_cursor_limit_cents(summary: &serde_json::Value) -> Option<f64> {
    let total = plan_breakdown_total(summary)?;
    let percent = number_at(summary, &["individualUsage", "plan", "totalPercentUsed"])?;
    if percent <= 0.0 {
        return None;
    }
    if percent < 99.5 {
        return Some((total / (percent / 100.0)).round());
    }
    // The API caps totalPercentUsed at 100 even when spend slightly exceeds the
    // seat cap, so total / 100% would echo the used amount ($181.72 / $181.72).
    let snapped = (total / 1000.0).floor() * 1000.0;
    (snapped > 0.0 && total - snapped < 1000.0).then_some(snapped)
}

fn on_demand_metric(summary: &serde_json::Value) -> Option<UsageMetric> {
    let used = number_at(summary, &["individualUsage", "onDemand", "used"])
        .or_else(|| number_at(summary, &["teamUsage", "onDemand", "used"]))?;
    let limit = number_at(summary, &["individualUsage", "onDemand", "limit"])
        .or_else(|| number_at(summary, &["teamUsage", "onDemand", "limit"]))
        .filter(|limit| *limit > 0.0);
    Some(usage_metric("currency", used, limit))
}

fn is_team_scoped(summary: &serde_json::Value) -> bool {
    let membership = text_at(summary, &["membershipType"]).unwrap_or_default();
    let limit_type = text_at(summary, &["limitType"]).unwrap_or_default();
    ["enterprise", "team", "teams", "business"]
        .iter()
        .any(|name| membership.eq_ignore_ascii_case(name))
        || limit_type.eq_ignore_ascii_case("team")
}

fn usage_pools(summary: &serde_json::Value, hard_limit: Option<&serde_json::Value>) -> (UsageMetric, Option<UsageMetric>) {
    let plan_used = number_at(summary, &["individualUsage", "plan", "used"]);
    let plan_limit = number_at(summary, &["individualUsage", "plan", "limit"]).filter(|limit| *limit > 0.0);
    let overall_used = number_at(summary, &["individualUsage", "overall", "used"]);
    let total_percent = number_at(summary, &["individualUsage", "plan", "totalPercentUsed"])
        .or_else(|| percent_from_message(text_at(summary, &["autoModelSelectedDisplayMessage"]).as_deref()));
    let auto_percent = number_at(summary, &["individualUsage", "plan", "autoPercentUsed"])
        .or_else(|| percent_from_message(text_at(summary, &["autoModelSelectedDisplayMessage"]).as_deref()));
    let api_percent = number_at(summary, &["individualUsage", "plan", "apiPercentUsed"])
        .or_else(|| percent_from_message(text_at(summary, &["namedModelSelectedDisplayMessage"]).as_deref()));
    let seat_limit = per_user_limit_cents(summary, hard_limit)
        .or_else(|| number_at(summary, &["individualUsage", "overall", "limit"]).filter(|limit| *limit > 0.0))
        .or_else(|| {
            let inferred = inferred_cursor_limit_cents(summary)?;
            match plan_limit {
                Some(plan) if inferred > plan + 1.0 => Some(inferred),
                _ => None,
            }
        });
    let cursor_used = plan_breakdown_total(summary)
        .or(overall_used)
        .or_else(|| match (seat_limit, total_percent) {
            (Some(limit), Some(percent)) => Some(limit * percent / 100.0),
            _ => None,
        })
        .or(plan_used);
    let two_pool = auto_percent.is_some() || api_percent.is_some() || matches!((seat_limit, plan_limit), (Some(seat), Some(plan)) if seat > plan + 1.0);
    let primary = if let Some(limit) = seat_limit {
        usage_metric("currency", cursor_used.unwrap_or(0.0), Some(limit))
    } else if two_pool {
        percent_metric(auto_percent.or(total_percent).unwrap_or(0.0))
    } else if let (Some(used), Some(limit)) = (plan_used, plan_limit) {
        usage_metric("currency", used, Some(limit))
    } else if let Some(used) = overall_used {
        usage_metric("currency", used, None)
    } else if let Some(percent) = total_percent {
        percent_metric(percent)
    } else {
        usage_metric("requests", 0.0, None)
    };
    let on_demand = if two_pool {
        if let (Some(used), Some(limit)) = (plan_used, plan_limit) {
            Some(with_percent(usage_metric("currency", used, Some(limit)), api_percent))
        } else if let Some(percent) = api_percent {
            Some(percent_metric(percent))
        } else {
            on_demand_metric(summary)
        }
    } else {
        on_demand_metric(summary)
    };
    (primary, on_demand)
}

fn update_export_usage(record: &mut serde_json::Value, raw: serde_json::Value, checked_at: u64) {
    let Some(record) = record.as_object_mut() else { return; };
    let mut compatibility = raw.get("usage_summary").cloned().unwrap_or_else(|| raw.clone());
    let Some(usage_raw) = compatibility.as_object_mut() else { return; };
    let summary = raw.get("usage_summary").unwrap_or(&raw);
    for key in ["cursor_usage_sources", "total_input_tokens", "total_output_tokens", "used_models", "billing_cycle_start", "billing_cycle_end"] {
        record.remove(key);
    }
    record.insert("usage_updated_at".into(), serde_json::Value::from(checked_at));
    if let Some(value) = text_at(summary, &["membershipType"]) {
        record.insert("membership_type".into(), serde_json::Value::String(value));
    }
    let events = raw.get("usage_events").and_then(|value| value.get("usageEventsDisplay")).and_then(serde_json::Value::as_array);
    let mut input_total = None;
    let mut output_total = None;
    let mut by_model: BTreeMap<String, serde_json::Map<String, serde_json::Value>> = BTreeMap::new();
    for event in events.into_iter().flatten() {
        let number = |keys: &[&str]| keys.iter().find_map(|key| event.get(*key).and_then(serde_json::Value::as_f64));
        if let Some(value) = number(&["inputTokens", "input_tokens", "inputTokenCount"]) {
            input_total = Some(input_total.unwrap_or(0.0) + value);
        }
        if let Some(value) = number(&["outputTokens", "output_tokens", "outputTokenCount"]) {
            output_total = Some(output_total.unwrap_or(0.0) + value);
        }
        let Some(name) = event_model_name(event) else { continue; };
        let model = by_model.entry(name.clone()).or_insert_with(|| {
            let mut model = serde_json::Map::new();
            model.insert("model_name".into(), serde_json::Value::String(name));
            model
        });
        let requests = model.get("num_requests").and_then(json_number).unwrap_or(0.0) + event_request_weight(event);
        model.insert("num_requests".into(), serde_json::Value::from(requests.round() as u64));
        for (target, keys) in [("input_tokens", &["inputTokens", "input_tokens", "inputTokenCount"][..]), ("output_tokens", &["outputTokens", "output_tokens", "outputTokenCount"][..]), ("cost_usd", &["costUsd", "cost_usd", "costUSD"][..])] {
            if let Some(value) = number(keys) {
                let total = model.get(target).and_then(serde_json::Value::as_f64).unwrap_or(0.0) + value;
                model.insert(target.into(), serde_json::Value::from(total));
            }
        }
    }
    if let Some(value) = input_total { usage_raw.insert("total_input_tokens".into(), serde_json::Value::from(value)); }
    if let Some(value) = output_total { usage_raw.insert("total_output_tokens".into(), serde_json::Value::from(value)); }
    if !by_model.is_empty() { usage_raw.insert("used_models".into(), serde_json::Value::Array(by_model.into_values().map(serde_json::Value::Object).collect())); }
    if let Some(value) = raw.get("hard_limit") {
        usage_raw.insert("hard_limit".into(), value.clone());
    }
    record.insert("cursor_usage_raw".into(), compatibility);
}

fn cursor_usage_from_snapshot(account: &Account, raw: &serde_json::Value) -> Option<CursorUsageDetails> {
    let summary = raw;
    let (primary, on_demand) = usage_pools(summary, summary.get("hard_limit"));
    let mut models: Vec<_> = raw.get("used_models").and_then(serde_json::Value::as_array).into_iter().flatten().filter_map(|model| {
        let requests = model.get("num_requests").and_then(json_number).filter(|value| *value > 0.0)?;
        Some(ModelUsageSummary { name: model.get("model_name")?.as_str()?.into(), requests: requests.round() as u64 })
    }).collect();
    sort_models(&mut models);
    Some(CursorUsageDetails {
        account_id: account.id.clone(), label: account.label.clone(), email: account.email.clone(), name: None, membership_type: text_at(summary, &["membershipType"]), primary, reset_at: text_at(summary, &["billingCycleEnd"]), on_demand, models, weekly_available: false, weekly: vec![], weekly_error: None, events: usage_event_rows(raw), checked_at: account.raw_export.get("usage_updated_at").and_then(serde_json::Value::as_u64).unwrap_or(account.updated_at),
    })
}

fn fetch_cursor_usage(account: &Account, session: &Session) -> Result<(CursorUsageDetails, serde_json::Value)> {
    let cookie = dashboard_cookie(session)?;
    let me = dashboard_request(&cookie, "/auth/me", None)?;
    let summary = dashboard_request(&cookie, "/usage-summary", None)?;
    let usage = dashboard_request(&cookie, "/usage", None)?;
    let email = text_at(&me, &["email"]);
    let enterprise = summary
        .get("membershipType")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("enterprise"));
    let team_scoped = is_team_scoped(&summary);
    let teams = team_scoped.then(|| dashboard_request(&cookie, "/dashboard/teams", Some(serde_json::json!({}))).ok()).flatten();
    let team_id = teams.as_ref().and_then(first_team_id);
    let hard_limit = team_id.and_then(|team_id| dashboard_request(&cookie, "/dashboard/get-hard-limit", Some(serde_json::json!({ "teamId": team_id }))).ok());
    let (mut primary, on_demand) = usage_pools(&summary, hard_limit.as_ref());
    if primary.kind == "requests" {
        let requests = usage
            .as_object()
            .into_iter()
            .flat_map(|map| map.values())
            .filter_map(|item| item.get("numRequests").and_then(json_number).map(|value| value.round() as u64))
            .sum::<u64>();
        primary = usage_metric("requests", requests as f64, None);
    }

    let team_spend = team_id.and_then(|team_id| dashboard_request(&cookie, "/dashboard/get-team-spend", Some(serde_json::json!({ "teamId": team_id }))).ok());
    let member_emails: Vec<String> = [email.clone(), account.email.clone()].into_iter().flatten().collect();
    let member_id = team_spend
        .as_ref()
        .and_then(|spend| team_member_user_id(spend, &member_emails))
        .or_else(|| auth_numeric_id(&me));
    let events_team_id = if enterprise { team_id } else { Some(0) };
    let events_user_id = if enterprise { member_id } else { None };
    let events_result = collect_usage_events(&cookie, events_team_id, events_user_id);
    let weekly_error = events_result.as_ref().err().map(|error| error.to_string());
    let usage_events = events_result.ok();
    let weekly = usage_events.as_ref().and_then(weekly_usage);
    let models = models_from_events(usage_events.as_ref().unwrap_or(&serde_json::Value::Null));
    let events = usage_events
        .as_ref()
        .map(usage_event_rows)
        .unwrap_or_default();
    let details = CursorUsageDetails {
        account_id: account.id.clone(),
        label: account.label.clone(),
        email,
        name: text_at(&me, &["name"]),
        membership_type: text_at(&summary, &["membershipType"]),
        primary,
        reset_at: text_at(&summary, &["billingCycleEnd"]),
        on_demand,
        models,
        weekly_available: weekly.is_some(),
        weekly: weekly.unwrap_or_default(),
        weekly_error,
        events,
        checked_at: now(),
    };
    let mut raw = serde_json::Map::new();
    raw.insert("auth_me".into(), me);
    raw.insert("usage_summary".into(), summary);
    raw.insert("usage".into(), usage);
    raw.insert("usage_events".into(), usage_events.unwrap_or(serde_json::Value::Null));
    if team_scoped {
        raw.insert("teams".into(), teams.unwrap_or(serde_json::Value::Null));
        raw.insert("hard_limit".into(), hard_limit.unwrap_or(serde_json::Value::Null));
        raw.insert("team_spend".into(), team_spend.unwrap_or(serde_json::Value::Null));
    }
    Ok((details, serde_json::Value::Object(raw)))
}

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("account not found")]
    AccountNotFound,
    #[error("账户凭证缺失，请重新导入该账户")]
    SecretMissing,
    #[error("Token is empty or too short")]
    InvalidToken,
    #[error("JSON does not contain a supported Cursor session")]
    InvalidImport,
    #[error("this application is not supported yet")]
    ComingSoon,
    #[error("登录已取消")]
    LoginCancelled,
    #[error("Cursor 官方登录超时，请重试")]
    LoginTimeout,
    #[error("unsupported Cursor data: {0}")]
    UnsupportedCursor(String),
    #[error("Cursor is not installed or has not been started")]
    CursorNotDetected,
    #[error("could not verify the Cursor session; the previous state was restored")]
    VerifyFailed,
    #[error("could not restore the previous Cursor session")]
    RestoreFailed,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

type Result<T> = std::result::Result<T, AppError>;

#[derive(Clone, Deserialize, Serialize)]
struct Session {
    values: BTreeMap<String, String>,
    #[serde(default)]
    raw_export: Option<serde_json::Value>,
}

fn is_cursor_user_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("user_") else {
        return false;
    };
    rest.len() >= 6 && rest.chars().all(|character| character.is_ascii_alphanumeric())
}

fn parse_cursor_session_token(raw: &str) -> Option<(String, String)> {
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

fn split_cursor_credential(raw: &str) -> Result<(Option<String>, String)> {
    if let Some((user_id, token)) = parse_cursor_session_token(raw) {
        return Ok((Some(user_id), token));
    }
    let token = raw.trim();
    if token.len() < 40 {
        return Err(AppError::InvalidToken);
    }
    Ok((None, token.to_owned()))
}

fn apply_jwt_profile(values: &mut BTreeMap<String, String>, token: &str) {
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

fn session_from_access_token(token: &str, user_id: Option<String>) -> Session {
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
    fn from_import(raw: &str) -> Result<Self> {
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
            session.values.insert("cursorAuth/refreshToken".into(), refresh);
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

fn raw_export_from_session(
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
    record.insert("telemetry_machine_ids".into(), serde_json::Value::Object(telemetry));
    record.insert("updated_at".into(), serde_json::Value::from(updated_at));
    serde_json::Value::Object(record)
}

trait ApplicationAdapter {
    fn kind(&self) -> ApplicationKind;
    fn detect(&self) -> ApplicationStatus;
    fn import_current(&self) -> Result<Session>;
    fn apply(&self, session: &Session) -> Result<()>;
    fn is_running(&self) -> bool;
}

struct CursorAdapter {
    database: Option<PathBuf>,
}

impl Default for CursorAdapter {
    fn default() -> Self {
        Self {
            database: Self::platform_db_path(),
        }
    }
}

impl CursorAdapter {
    fn platform_db_path() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            env::var_os("HOME").map(PathBuf::from).map(|home| {
                home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
            })
        }
        #[cfg(target_os = "windows")]
        {
            env::var_os("APPDATA")
                .map(PathBuf::from)
                .map(|base| base.join("Cursor/User/globalStorage/state.vscdb"))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            None
        }
    }

    fn open(&self) -> Result<Connection> {
        let Some(path) = &self.database else {
            return Err(AppError::UnsupportedCursor("this operating system".into()));
        };
        if !path.exists() {
            return Err(AppError::CursorNotDetected);
        }
        let db = Connection::open(path)?;
        let has_table: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ItemTable')",
            [],
            |row| row.get(0),
        )?;
        if !has_table {
            return Err(AppError::UnsupportedCursor("ItemTable is missing".into()));
        }
        Ok(db)
    }

    fn read_session_unchecked(&self, db: &Connection) -> Result<Session> {
        let mut values = BTreeMap::new();
        for key in CURSOR_KEYS {
            let value = db.query_row(
                "SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1",
                [key],
                |row| row.get::<_, String>(0),
            );
            match value {
                Ok(value) => {
                    values.insert(key.to_string(), value);
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(Session { values, raw_export: None })
    }

    fn read_session(&self, db: &Connection) -> Result<Session> {
        let session = self.read_session_unchecked(db)?;
        if !session.values.contains_key(ACCESS_TOKEN_KEY) {
            return Err(AppError::UnsupportedCursor(
                "access token is missing".into(),
            ));
        }
        Ok(session)
    }

    fn write_session(&self, db: &mut Connection, session: &Session) -> Result<()> {
        let transaction = db.transaction()?;
        for key in CURSOR_KEYS {
            if let Some(value) = session.values.get(key) {
                transaction.execute(
                    "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
                    params![key, value],
                )?;
            } else {
                transaction.execute("DELETE FROM ItemTable WHERE key = ?1", [key])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    fn telemetry(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut values = serde_json::Map::new();
        let Some(database_path) = &self.database else { return values; };
        let storage = database_path.parent().map(|path| path.join("storage.json"));
        if let Some(storage) = storage {
            if let Ok(storage) = serde_json::from_slice::<serde_json::Value>(&fs::read(storage).unwrap_or_default()) {
                for (source, target) in [("telemetry.devDeviceId", "devDeviceId"), ("telemetry.macMachineId", "macMachineId"), ("telemetry.machineId", "machineId"), ("telemetry.sqmId", "sqmId")] {
                    if let Some(value) = storage.get(source) { values.insert(target.into(), value.clone()); }
                }
            }
        }
        if let Ok(database) = self.open() {
            for (source, target) in [("storage.serviceMachineId", "serviceMachineId"), ("telemetry.firstSessionDate", "firstSessionDate")] {
                if let Ok(value) = database.query_row("SELECT value FROM ItemTable WHERE key=?1", [source], |row| row.get::<_, String>(0)) {
                    values.insert(target.into(), serde_json::Value::String(value));
                }
            }
        }
        values
    }
}

impl ApplicationAdapter for CursorAdapter {
    fn kind(&self) -> ApplicationKind {
        ApplicationKind::Cursor
    }

    fn detect(&self) -> ApplicationStatus {
        match self.open().and_then(|db| self.read_session(&db)) {
            Ok(_) => ApplicationStatus {
                kind: self.kind(),
                label: self.kind().display_name().into(),
                available: true,
                reason: None,
            },
            Err(error) => ApplicationStatus {
                kind: self.kind(),
                label: self.kind().display_name().into(),
                available: false,
                reason: Some(error.to_string()),
            },
        }
    }

    fn import_current(&self) -> Result<Session> {
        let db = self.open()?;
        self.read_session(&db)
    }

    fn apply(&self, session: &Session) -> Result<()> {
        if !session.values.contains_key(ACCESS_TOKEN_KEY) {
            return Err(AppError::SecretMissing);
        }
        let mut db = self.open()?;
        let before = self.read_session_unchecked(&db)?;
        self.write_session(&mut db, session)?;
        let verified = self
            .read_session(&db)
            .map(|current| {
                current.values.get(ACCESS_TOKEN_KEY) == session.values.get(ACCESS_TOKEN_KEY)
            })
            .unwrap_or(false);
        if verified {
            return Ok(());
        }
        self.write_session(&mut db, &before)
            .map_err(|_| AppError::RestoreFailed)?;
        Err(AppError::VerifyFailed)
    }

    fn is_running(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("pgrep")
                .args(["-x", "Cursor"])
                .output()
                .is_ok_and(|output| output.status.success())
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("tasklist")
                .output()
                .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("Cursor.exe"))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    }
}

struct CodexAdapter;

impl ApplicationAdapter for CodexAdapter {
    fn kind(&self) -> ApplicationKind {
        ApplicationKind::Codex
    }

    fn detect(&self) -> ApplicationStatus {
        ApplicationStatus {
            kind: self.kind(),
            label: self.kind().display_name().into(),
            available: false,
            reason: Some("桌面端账号切换待支持".into()),
        }
    }

    fn import_current(&self) -> Result<Session> {
        Err(AppError::ComingSoon)
    }

    fn apply(&self, _: &Session) -> Result<()> {
        Err(AppError::ComingSoon)
    }

    fn is_running(&self) -> bool {
        false
    }
}

struct Controller {
    pointer_file: PathBuf,
    database_path: PathBuf,
    database: Connection,
    cursor: CursorAdapter,
    codex: CodexAdapter,
}

impl Controller {
    fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir)?;
        let pointer_file = data_dir.join(DATABASE_PATH_FILE);
        let database_path = fs::read_to_string(&pointer_file)
            .ok()
            .map(|value| PathBuf::from(value.trim()))
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| data_dir.join(DATABASE_NAME));
        let database = Self::open_database(&database_path)?;
        let mut controller = Self {
            pointer_file,
            database_path,
            database,
            cursor: CursorAdapter::default(),
            codex: CodexAdapter,
        };
        controller.migrate_raw_exports()?;
        Ok(controller)
    }

    fn open_database(path: &std::path::Path) -> Result<Connection> {
        let parent = path.parent().ok_or_else(|| AppError::Message("数据库路径无效".into()))?;
        fs::create_dir_all(parent)?;
        let database = Connection::open(path)?;
        database.busy_timeout(Duration::from_secs(5))?;
        database.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS accounts (
               id TEXT PRIMARY KEY, application TEXT NOT NULL, label TEXT NOT NULL,
               email TEXT, import_type TEXT NOT NULL, subscription_json TEXT NOT NULL,
               usage_json TEXT, usage_raw_json TEXT, raw_export_json TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
               last_used_at INTEGER NOT NULL, sort_order INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS sessions (
               account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
               session_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS application_state (
               application TEXT PRIMARY KEY, current_account_id TEXT
             );
             PRAGMA user_version=1;",
        )?;
        let columns = database
            .prepare("PRAGMA table_info(accounts)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !columns.iter().any(|name| name == "usage_raw_json") {
            database.execute("ALTER TABLE accounts ADD COLUMN usage_raw_json TEXT", [])?;
        }
        if !columns.iter().any(|name| name == "raw_export_json") {
            database.execute("ALTER TABLE accounts ADD COLUMN raw_export_json TEXT", [])?;
        }
        Ok(database)
    }

    fn kind_value(kind: ApplicationKind) -> &'static str {
        match kind { ApplicationKind::Cursor => "cursor", ApplicationKind::Codex => "codex" }
    }

    fn import_type_value(import_type: &ImportType) -> &'static str {
        match import_type { ImportType::OAuth => "oauth", ImportType::Token => "token", ImportType::Jwt => "jwt", ImportType::Native => "native" }
    }

    fn import_type_from(value: &str) -> Result<ImportType> {
        match value { "oauth" => Ok(ImportType::OAuth), "token" => Ok(ImportType::Token), "jwt" => Ok(ImportType::Jwt), "native" => Ok(ImportType::Native), _ => Err(AppError::Message("数据库中的导入类型无效".into())) }
    }

    fn all_accounts(&self) -> Result<Vec<Account>> {
        let mut statement = self.database.prepare("SELECT id, application, label, email, import_type, subscription_json, usage_json, usage_raw_json, raw_export_json, created_at, updated_at, last_used_at FROM accounts ORDER BY application, sort_order")?;
        let mut rows = statement.query([])?;
        let mut accounts = Vec::new();
        while let Some(row) = rows.next()? {
            let application = match row.get::<_, String>(1)?.as_str() { "cursor" => ApplicationKind::Cursor, "codex" => ApplicationKind::Codex, _ => return Err(AppError::Message("数据库中的应用类型无效".into())) };
            accounts.push(Account {
                id: row.get(0)?, application, label: row.get(2)?, email: row.get(3)?,
                import_type: Self::import_type_from(&row.get::<_, String>(4)?)?,
                subscription: serde_json::from_str(&row.get::<_, String>(5)?)?,
                raw_export: row.get::<_, Option<String>>(8)?.map(|json| serde_json::from_str(&json)).transpose()?.unwrap_or(serde_json::Value::Null),
                created_at: row.get(9)?, updated_at: row.get(10)?, last_used_at: row.get(11)?,
            });
        }
        Ok(accounts)
    }

    fn account(&self, id: &str) -> Result<Account> {
        self.all_accounts()?.into_iter().find(|account| account.id == id).ok_or(AppError::AccountNotFound)
    }

    fn load_session(&self, id: &str) -> Result<Session> {
        let mut statement = self.database.prepare("SELECT session_json FROM sessions WHERE account_id = ?1")?;
        let mut rows = statement.query(params![id])?;
        let Some(row) = rows.next()? else { return Err(AppError::SecretMissing); };
        Ok(serde_json::from_str(&row.get::<_, String>(0)?)?)
    }

    fn legacy_usage_raw(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let json: Option<String> = self.database.query_row(
            "SELECT usage_raw_json FROM accounts WHERE id=?1",
            params![id],
            |row| row.get(0),
        )?;
        json.map(|json| serde_json::from_str(&json)).transpose().map_err(Into::into)
    }

    fn migrate_raw_exports(&mut self) -> Result<()> {
        for account in self.all_accounts()? {
            let mut raw = account.raw_export;
            let mut changed = false;
            if raw.is_null() {
                raw = raw_export_from_session(&self.load_session(&account.id)?, &account.id, account.created_at, account.updated_at, account.last_used_at, self.cursor.telemetry());
                changed = true;
            }
            if raw.get("cursor_usage_raw").is_none() {
                if let Some(usage) = self.legacy_usage_raw(&account.id)? {
                    update_export_usage(&mut raw, usage, account.updated_at);
                    changed = true;
                }
            }
            if changed {
                self.database.execute("UPDATE accounts SET raw_export_json=?1 WHERE id=?2", params![serde_json::to_string(&raw)?, account.id])?;
            }
        }
        Ok(())
    }

    fn adapter(&self, kind: ApplicationKind) -> &dyn ApplicationAdapter {
        match kind {
            ApplicationKind::Cursor => &self.cursor,
            ApplicationKind::Codex => &self.codex,
        }
    }

    fn statuses(&self) -> Vec<ApplicationStatus> {
        vec![self.cursor.detect(), self.codex.detect()]
    }

    fn accounts(&self, kind: ApplicationKind) -> Vec<AccountSummary> {
        let current: Option<String> = self.database.query_row("SELECT current_account_id FROM application_state WHERE application = ?1", params![Self::kind_value(kind)], |row| row.get(0)).ok();
        self.all_accounts().unwrap_or_default().into_iter()
            .filter(|account| account.application == kind)
            .map(|account| AccountSummary {
                is_current: current.as_deref() == Some(&account.id),
                id: account.id.clone(),
                label: account.label.clone(),
                email: account.email.clone(),
                import_type: account.import_type.clone(),
                subscription: account.subscription.clone(),
                days_remaining: account
                    .subscription
                    .expires_at
                    .map(|expires_at| utc_days_remaining(expires_at, now())),
            })
            .collect()
    }

    fn reorder_accounts(&mut self, kind: ApplicationKind, ids: Vec<String>) -> Result<()> {
        let accounts = self.all_accounts()?;
        let existing: Vec<_> = accounts.iter()
            .filter(|account| account.application == kind)
            .map(|account| account.id.as_str())
            .collect();
        if ids.len() != existing.len()
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || ids.iter().any(|id| !existing.contains(&id.as_str()))
        {
            return Err(AppError::Message("账户排序无效".into()));
        }
        let transaction = self.database.transaction()?;
        for (position, id) in ids.iter().enumerate() {
            transaction.execute("UPDATE accounts SET sort_order = ?1 WHERE id = ?2", params![position as i64, id])?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn save_imported_session(
        &mut self,
        kind: ApplicationKind,
        label: Option<String>,
        session: Session,
        import_type: ImportType,
    ) -> Result<Account> {
        let now = now();
        let email = session
            .values
            .get(EMAIL_KEY)
            .cloned()
            .filter(|value| !value.is_empty());
        let display_label = session_display_label(&session);
        if let Some(email) = email.as_deref() {
            if let Some(index) = matching_account_index(&self.all_accounts()?, kind, email) {
                let mut account = self.all_accounts()?[index].clone();
                if let Some(label) = label.filter(|value| !value.trim().is_empty()) {
                    account.label = label;
                } else if let Some(display_label) = display_label.clone() {
                    account.label = display_label;
                }
                account.email = Some(email.to_owned());
                account.import_type = import_type;
                account.subscription = subscription_from_session(&session);
                account.updated_at = now;
                account.last_used_at = now;
                account.raw_export = raw_export_from_session(&session, &account.id, account.created_at, now, now, self.cursor.telemetry());
                let transaction = self.database.transaction()?;
                transaction.execute("UPDATE accounts SET label=?1, email=?2, import_type=?3, subscription_json=?4, raw_export_json=?5, updated_at=?6, last_used_at=?7 WHERE id=?8", params![account.label, account.email, Self::import_type_value(&account.import_type), serde_json::to_string(&account.subscription)?, serde_json::to_string(&account.raw_export)?, account.updated_at as i64, account.last_used_at as i64, account.id])?;
                transaction.execute("INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2) ON CONFLICT(account_id) DO UPDATE SET session_json=excluded.session_json", params![account.id, serde_json::to_string(&session)?])?;
                transaction.commit()?;
                return Ok(account);
            }
        }
        let id = format!(
            "acc_{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let label = label
            .filter(|value| !value.trim().is_empty())
            .or(display_label)
            .unwrap_or_else(|| format!("{} Account", kind.display_name()));
        let account = Account {
            id: id.clone(),
            application: kind,
            label,
            email,
            import_type,
            subscription: subscription_from_session(&session),
            raw_export: raw_export_from_session(&session, &id, now, now, now, self.cursor.telemetry()),
            created_at: now,
            updated_at: now,
            last_used_at: now,
        };
        let transaction = self.database.transaction()?;
        let sort_order: i64 = transaction.query_row("SELECT COUNT(*) FROM accounts WHERE application=?1", params![Self::kind_value(kind)], |row| row.get(0))?;
        transaction.execute("INSERT INTO accounts (id, application, label, email, import_type, subscription_json, usage_json, usage_raw_json, raw_export_json, created_at, updated_at, last_used_at, sort_order) VALUES (?1,?2,?3,?4,?5,?6,NULL,NULL,?7,?8,?9,?10,?11)", params![account.id, Self::kind_value(kind), account.label, account.email, Self::import_type_value(&account.import_type), serde_json::to_string(&account.subscription)?, serde_json::to_string(&account.raw_export)?, account.created_at as i64, account.updated_at as i64, account.last_used_at as i64, sort_order])?;
        transaction.execute("INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2)", params![account.id, serde_json::to_string(&session)?])?;
        transaction.commit()?;
        Ok(account)
    }

    fn import_current(&mut self, kind: ApplicationKind, label: Option<String>) -> Result<Account> {
        let session = self.adapter(kind).import_current()?;
        self.save_imported_session(kind, label, session, ImportType::Native)
    }

    #[cfg(test)]
    fn import_payload(
        &mut self,
        kind: ApplicationKind,
        label: Option<String>,
        payload: &str,
    ) -> Result<Account> {
        if kind != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        let session = Session::from_import(payload)?;
        let import_type = import_type(&session);
        self.save_imported_session(kind, label, session, import_type)
    }

    fn delete_account(&mut self, id: &str) -> Result<()> {
        self.account(id)?;
        let transaction = self.database.transaction()?;
        transaction.execute("DELETE FROM accounts WHERE id=?1", params![id])?;
        transaction.execute("DELETE FROM application_state WHERE current_account_id=?1", params![id])?;
        transaction.commit()?;
        Ok(())
    }

    fn subscription_session(&mut self, id: &str) -> Result<Session> {
        let account = self.account(id)?;
        if account.application != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        self.load_session(&account.id)
    }

    fn save_subscription(&mut self, id: &str, summary: SubscriptionSummary) -> Result<()> {
        let mut account = self.account(id)?;
        account.subscription = SubscriptionSummary {
            plan: summary.plan.or_else(|| account.subscription.plan.clone()),
            ..summary
        };
        account.updated_at = now();
        self.database.execute("UPDATE accounts SET subscription_json=?1, updated_at=?2 WHERE id=?3", params![serde_json::to_string(&account.subscription)?, account.updated_at as i64, id])?;
        Ok(())
    }

    fn saved_cursor_usage(&self, id: &str) -> Result<Option<CursorUsageDetails>> {
        let account = self.account(id)?;
        if account.application != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        let json: Option<String> = self.database.query_row(
            "SELECT usage_json FROM accounts WHERE id=?1",
            params![id],
            |row| row.get(0),
        )?;
        if let Some(json) = json {
            if let Ok(mut details) = serde_json::from_str::<CursorUsageDetails>(&json) {
                if let Some(raw) = account.raw_export.get("cursor_usage_raw") {
                    let (primary, on_demand) = usage_pools(raw, raw.get("hard_limit"));
                    if !(primary.kind == "requests" && details.primary.kind != "requests") {
                        details.primary = primary;
                        details.on_demand = on_demand;
                    }
                }
                return Ok(Some(details));
            }
        }
        let raw = account.raw_export.get("cursor_usage_raw").cloned();
        Ok(raw.and_then(|raw| cursor_usage_from_snapshot(&account, &raw)))
    }

    fn save_cursor_usage(&mut self, id: &str, usage: CursorUsageDetails, raw: serde_json::Value) -> Result<()> {
        let mut account = self.account(id)?;
        update_export_usage(&mut account.raw_export, raw, usage.checked_at);
        self.database.execute(
            "UPDATE accounts SET usage_json=?1, raw_export_json=?2, updated_at=?3 WHERE id=?4",
            params![
                serde_json::to_string(&usage)?,
                serde_json::to_string(&account.raw_export)?,
                now() as i64,
                id
            ],
        )?;
        Ok(())
    }

    fn cursor_usage_session(&mut self, id: &str) -> Result<(Account, Session)> {
        let account = self.account(id)?;
        if account.application != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        let session = self.subscription_session(id)?;
        Ok((account, session))
    }

    fn switch_account<F>(&mut self, id: &str, mut progress: F) -> Result<SwitchOutcome>
    where
        F: FnMut(&'static str, u8),
    {
        progress("loading", 15);
        let account = self.account(id)?;
        if !account.import_type.supports_desktop_switch() {
            return Err(AppError::Message(
                "Token / JWT 账户只能查询用量，不能切换登录 Cursor 桌面端。".into(),
            ));
        }
        let session = self.load_session(&account.id)?;
        let running = self.adapter(account.application).is_running();
        progress("applying", 45);
        self.adapter(account.application).apply(&session)?;
        progress("persisting", 75);
        let transaction = self.database.transaction()?;
        transaction.execute("INSERT INTO application_state (application, current_account_id) VALUES (?1, ?2) ON CONFLICT(application) DO UPDATE SET current_account_id=excluded.current_account_id", params![Self::kind_value(account.application), account.id])?;
        transaction.execute("UPDATE accounts SET last_used_at=?1 WHERE id=?2", params![now() as i64, id])?;
        transaction.commit()?;
        Ok(SwitchOutcome {
            restart_required: running,
        })
    }

    fn current_label(&self) -> String {
        self.database.query_row("SELECT a.label FROM accounts a JOIN application_state s ON a.id=s.current_account_id WHERE s.application='cursor'", [], |row| row.get::<_, String>(0)).ok()
            .unwrap_or_else(|| "未选择账户".into())
    }

    fn database_path(&self) -> String { self.database_path.display().to_string() }

    fn move_database(&mut self, directory: PathBuf) -> Result<String> {
        if !directory.is_dir() { return Err(AppError::Message("请选择有效的同步目录".into())); }
        let target = directory.join(DATABASE_NAME);
        if target == self.database_path { return Ok(self.database_path()); }
        if target.exists() { return Err(AppError::Message("目标目录已包含 storm-dock.db".into())); }
        let temporary = directory.join(format!(".{DATABASE_NAME}.tmp"));
        if temporary.exists() { fs::remove_file(&temporary)?; }
        self.database.execute_batch("PRAGMA optimize;")?;
        let source = self.database_path.clone();
        fs::copy(&source, &temporary)?;
        let check = Connection::open(&temporary)?;
        let integrity: String = check.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" { let _ = fs::remove_file(&temporary); return Err(AppError::Message("迁移后的数据库校验失败".into())); }
        drop(check);
        fs::rename(&temporary, &target)?;
        fs::write(&self.pointer_file, target.to_string_lossy().as_bytes())?;
        let database = Self::open_database(&target)?;
        self.database = database;
        self.database_path = target;
        let _ = fs::remove_file(source);
        Ok(self.database_path())
    }

    fn export_cursor_account(&self, account: &Account) -> Result<serde_json::Value> {
        if !account.raw_export.is_object() {
            return Err(AppError::Message("账户的原始导出数据无效".into()));
        }
        let mut record = account.raw_export.clone();
        if let Some(raw) = record.get("cursor_usage_sources").cloned() {
            let checked_at = record.get("usage_updated_at").and_then(serde_json::Value::as_u64).unwrap_or(account.updated_at);
            update_export_usage(&mut record, raw, checked_at);
        }
        if record.get("cursor_usage_raw").is_none() {
            if let Some(raw) = self.legacy_usage_raw(&account.id)? {
                update_export_usage(&mut record, raw, account.updated_at);
            }
        }
        Ok(record)
    }

    fn export_cursor_accounts(&self, file: PathBuf) -> Result<()> {
        let parent = file.parent().ok_or_else(|| AppError::Message("导出路径无效".into()))?;
        fs::create_dir_all(parent)?;
        let accounts = self.all_accounts()?.into_iter().filter(|account| account.application == ApplicationKind::Cursor).map(|account| self.export_cursor_account(&account)).collect::<Result<Vec<_>>>()?;
        let temporary = file.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&accounts)?)?;
        fs::rename(temporary, file)?;
        Ok(())
    }
}

fn emit_switch_progress(
    app: &AppHandle,
    operation_id: &str,
    account_id: &str,
    stage: &'static str,
    percent: u8,
    status: &'static str,
) {
    let _ = app.emit(
        "account-switch-progress",
        SwitchProgress {
            operation_id: operation_id.into(),
            account_id: account_id.into(),
            stage,
            percent,
            status,
        },
    );
}

fn launch_cursor() -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open")
        .args(["-a", "Cursor"])
        .status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", "Cursor"])
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status: std::io::Result<std::process::ExitStatus> =
        Err(std::io::Error::other("unsupported OS"));
    match status {
        Ok(status) if status.success() => Ok(()),
        _ => Err(AppError::Message(
            "无法启动 Cursor，请确认应用已安装。".into(),
        )),
    }
}

fn terminate_cursor() -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("pkill")
        .args(["-x", "Cursor"])
        .status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("taskkill")
        .args(["/IM", "Cursor.exe", "/F"])
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status: std::io::Result<std::process::ExitStatus> =
        Err(std::io::Error::other("unsupported OS"));
    match status {
        Ok(status) if status.success() => Ok(()),
        _ => Err(AppError::Message("无法结束 Cursor 进程。".into())),
    }
}

fn wait_for_cursor_stop() -> Result<()> {
    for _ in 0..50 {
        if !CursorAdapter::default().is_running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(AppError::Message("Cursor 未在 5 秒内退出。".into()))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct AppState(Mutex<Controller>);

#[derive(Default)]
struct OauthLoginState {
    generation: AtomicU64,
    login_url: Mutex<Option<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OfficialLoginStatus {
    stage: &'static str,
    login_url: Option<String>,
}

struct CursorOauthHandshake {
    uuid: String,
    verifier: String,
    login_url: String,
}

impl OauthLoginState {
    fn begin(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn is_active(&self, id: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == id
    }

    fn set_url(&self, id: u64, url: String) -> Result<()> {
        if !self.is_active(id) {
            return Err(AppError::LoginCancelled);
        }
        *self
            .login_url
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(url);
        Ok(())
    }

    fn url(&self) -> Option<String> {
        self.login_url
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        *self
            .login_url
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
    }

    fn finish(&self, id: u64) {
        if self.is_active(id) {
            *self
                .login_url
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
        }
    }
}

impl CursorOauthHandshake {
    fn generate() -> Self {
        let mut random = [0u8; 32];
        random[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        random[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        Self::from_parts(uuid::Uuid::new_v4().to_string(), URL_SAFE_NO_PAD.encode(random))
    }

    fn from_parts(uuid: String, verifier: String) -> Self {
        let challenge = pkce_challenge(&verifier);
        let login_url = format!(
            "{CURSOR_OAUTH_LOGIN_URL}?challenge={challenge}&uuid={uuid}&mode=login&redirectTarget=cli"
        );
        Self {
            uuid,
            verifier,
            login_url,
        }
    }
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn emit_official_login_status(app: &AppHandle, stage: &'static str, login_url: Option<String>) {
    let _ = app.emit(
        "official-login-status",
        OfficialLoginStatus { stage, login_url },
    );
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status: std::io::Result<std::process::ExitStatus> =
        Err(std::io::Error::other("unsupported OS"));
    match status {
        Ok(status) if status.success() => Ok(()),
        _ => Err(AppError::Message("无法打开浏览器，请手动打开登录链接。".into())),
    }
}

fn poll_cursor_oauth(
    handshake: &CursorOauthHandshake,
    login_id: u64,
    app: &AppHandle,
) -> Result<serde_json::Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::Message(format!("无法创建 Cursor 登录请求: {error}")))?;
    let mut delay = Duration::from_secs(1);
    let mut consecutive_errors = 0;
    for _ in 0..CURSOR_OAUTH_POLL_ATTEMPTS {
        if !app.state::<OauthLoginState>().is_active(login_id) {
            return Err(AppError::LoginCancelled);
        }
        match poll_cursor_oauth_once(&client, handshake) {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {
                consecutive_errors = 0;
            }
            Err(AppError::LoginCancelled) => return Err(AppError::LoginCancelled),
            Err(_) => {
                consecutive_errors += 1;
                if consecutive_errors >= CURSOR_OAUTH_MAX_ERRORS {
                    return Err(AppError::Message(
                        "Cursor 官方登录轮询连续失败，请检查网络后重试。".into(),
                    ));
                }
            }
        }
        std::thread::sleep(delay);
        delay = delay.mul_f32(1.2).min(Duration::from_secs(10));
    }
    Err(AppError::LoginTimeout)
}

fn poll_cursor_oauth_once(
    client: &reqwest::blocking::Client,
    handshake: &CursorOauthHandshake,
) -> Result<Option<serde_json::Value>> {
    if let Ok(post) = client
        .post(CURSOR_OAUTH_POLL_URL)
        .header("User-Agent", "Storm Dock")
        .json(&serde_json::json!({
            "uuid": handshake.uuid,
            "verifier": handshake.verifier,
        }))
        .send()
    {
        if post.status().is_success() {
            return read_oauth_poll_response(post);
        }
    }
    let get = client
        .get(CURSOR_OAUTH_POLL_URL)
        .query(&[
            ("uuid", handshake.uuid.as_str()),
            ("verifier", handshake.verifier.as_str()),
        ])
        .header("User-Agent", "Storm Dock")
        .send()
        .map_err(|error| AppError::Message(format!("Cursor 登录轮询失败: {error}")))?;
    if get.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    read_oauth_poll_response(get)
}

fn read_oauth_poll_response(
    response: reqwest::blocking::Response,
) -> Result<Option<serde_json::Value>> {
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let value: serde_json::Value = response
        .json()
        .map_err(|error| AppError::Message(format!("Cursor 登录响应无效: {error}")))?;
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "Cursor 登录轮询失败 (HTTP {status})"
        )));
    }
    Ok(Some(value))
}

fn json_text(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn session_from_oauth_poll(value: &serde_json::Value) -> Result<Session> {
    let access_token = json_text(value, &["accessToken", "access_token"])
        .filter(|token| token.len() >= 40)
        .ok_or(AppError::InvalidImport)?;
    let mut values = BTreeMap::from([(ACCESS_TOKEN_KEY.into(), access_token.clone())]);
    if let Some(refresh) = json_text(value, &["refreshToken", "refresh_token", "cursorAuth/refreshToken"]) {
        values.insert("cursorAuth/refreshToken".into(), refresh);
    }
    if let Some(auth_id) = json_text(value, &["authId", "auth_id"]) {
        values.insert("glass.lastSignedInAuthId".into(), auth_id);
    }
    if let Some(email) = json_text(value, &["email", "cachedEmail", EMAIL_KEY]) {
        values.insert(EMAIL_KEY.into(), email.clone());
        values.insert(
            "cursorAuth/cachedScopedProfile".into(),
            serde_json::json!({ "displayName": email }).to_string(),
        );
    }
    if let Some(claims) = jwt_claims(&access_token) {
        if !values.contains_key(EMAIL_KEY) {
            if let Some(email) = claims.get("email").and_then(serde_json::Value::as_str).filter(|email| !email.is_empty()) {
                values.insert(EMAIL_KEY.into(), email.into());
                values.insert(
                    "cursorAuth/cachedScopedProfile".into(),
                    serde_json::json!({ "displayName": email }).to_string(),
                );
            }
        }
        if !values.contains_key("glass.lastSignedInAuthId") {
            if let Some(sub) = claims.get("sub").and_then(serde_json::Value::as_str).filter(|sub| !sub.is_empty()) {
                values.insert("glass.lastSignedInAuthId".into(), sub.into());
            }
        }
    }
    Ok(Session {
        values,
        raw_export: None,
    })
}

fn enrich_cursor_session(session: &mut Session) -> Option<SubscriptionSummary> {
    if let Ok(cookie) = dashboard_cookie(session) {
        if let Ok(me) = dashboard_request(&cookie, "/auth/me", None) {
            if let Some(email) = json_text(&me, &["email"]) {
                session.values.insert(EMAIL_KEY.into(), email.clone());
                session.values.insert(
                    "cursorAuth/cachedScopedProfile".into(),
                    serde_json::json!({ "displayName": email }).to_string(),
                );
            } else if !session.values.contains_key("cursorAuth/cachedScopedProfile") {
                if let Some(name) = json_text(&me, &["name", "displayName"]) {
                    session.values.insert(
                        "cursorAuth/cachedScopedProfile".into(),
                        serde_json::json!({ "displayName": name }).to_string(),
                    );
                }
            }
        }
    }
    let summary = fetch_cursor_subscription(session).ok();
    if let Some(plan) = summary.as_ref().and_then(|item| item.plan.clone()) {
        session.values.insert(MEMBERSHIP_TYPE_KEY.into(), plan);
    }
    summary
}

fn complete_cursor_oauth(
    label: Option<String>,
    login_id: u64,
    app: AppHandle,
) -> Result<Account> {
    let handshake = CursorOauthHandshake::generate();
    let oauth = app.state::<OauthLoginState>();
    oauth.set_url(login_id, handshake.login_url.clone())?;
    emit_official_login_status(&app, "started", Some(handshake.login_url.clone()));
    let _ = open_browser(&handshake.login_url);
    emit_official_login_status(&app, "waiting", Some(handshake.login_url.clone()));
    let tokens = poll_cursor_oauth(&handshake, login_id, &app)?;
    if !oauth.is_active(login_id) {
        return Err(AppError::LoginCancelled);
    }
    emit_official_login_status(&app, "importing", None);
    let mut session = session_from_oauth_poll(&tokens)?;
    let subscription = enrich_cursor_session(&mut session);
    if !oauth.is_active(login_id) {
        return Err(AppError::LoginCancelled);
    }
    let state = app.state::<AppState>();
    let mut controller = state
        .0
        .lock()
        .map_err(|_| AppError::Message("账户存储不可用".into()))?;
    let account = controller.save_imported_session(
        ApplicationKind::Cursor,
        label,
        session,
        ImportType::OAuth,
    )?;
    let account_id = account.id.clone();
    if let Some(summary) = subscription {
        let _ = controller.save_subscription(&account_id, summary);
    }
    let account = controller.account(&account_id).unwrap_or(account);
    drop(controller);
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    oauth.finish(login_id);
    Ok(account)
}

#[tauri::command]
fn list_applications(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<ApplicationStatus>, String> {
    state
        .0
        .lock()
        .map_err(|_| "应用状态不可用".to_string())
        .map(|controller| controller.statuses())
}

#[tauri::command]
fn list_accounts(
    kind: ApplicationKind,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<AccountSummary>, String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())
        .map(|controller| controller.accounts(kind))
}

#[tauri::command]
fn get_database_path(state: State<'_, AppState>) -> std::result::Result<String, String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())
        .map(|controller| controller.database_path())
}

#[tauri::command]
fn move_database(
    directory: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    let path = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .move_database(PathBuf::from(directory))
        .map_err(error_text)?;
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    Ok(path)
}

#[tauri::command]
fn export_cursor_accounts(
    file: String,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .export_cursor_accounts(PathBuf::from(file))
        .map_err(error_text)
}

#[tauri::command]
fn get_cursor_export_record(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<serde_json::Value, String> {
    let controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
    let account = controller.account(&id).map_err(error_text)?;
    if account.application != ApplicationKind::Cursor {
        return Err(error_text(AppError::ComingSoon));
    }
    controller.export_cursor_account(&account).map_err(error_text)
}

#[tauri::command]
fn refresh_account_subscription(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let session = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .subscription_session(&id)
        .map_err(error_text)?;
    let summary = fetch_cursor_subscription(&session).map_err(error_text)?;
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .save_subscription(&id, summary)
        .map_err(error_text)?;
    let _ = app.emit("accounts-changed", ());
    Ok(())
}

#[tauri::command]
async fn get_cursor_usage(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<CursorUsageDetails, String> {
    let (account, session) = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .cursor_usage_session(&id)
        .map_err(error_text)?;
    let (usage, raw) = tauri::async_runtime::spawn_blocking(move || fetch_cursor_usage(&account, &session))
        .await
        .map_err(|error| error.to_string())?
        .map_err(error_text)?;
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .save_cursor_usage(&id, usage.clone(), raw)
        .map_err(error_text)?;
    Ok(usage)
}

#[tauri::command]
fn get_saved_cursor_usage(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<Option<CursorUsageDetails>, String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .saved_cursor_usage(&id)
        .map_err(error_text)
}

#[tauri::command]
fn reorder_accounts(
    kind: ApplicationKind,
    ids: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .reorder_accounts(kind, ids)
        .map_err(error_text)?;
    let _ = app.emit("accounts-changed", ());
    Ok(())
}

#[tauri::command]
fn import_current_account(
    kind: ApplicationKind,
    label: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<Account, String> {
    let account = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .import_current(kind, label)
        .map_err(error_text)?;
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    Ok(account)
}

#[tauri::command]
fn import_token_or_json(
    kind: ApplicationKind,
    label: Option<String>,
    payload: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<Account, String> {
    if kind != ApplicationKind::Cursor {
        return Err(AppError::ComingSoon.to_string());
    }
    let mut session = Session::from_import(&payload).map_err(error_text)?;
    let subscription = enrich_cursor_session(&mut session);
    let import_type = import_type(&session);
    let mut controller = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?;
    let account = controller
        .save_imported_session(kind, label, session, import_type)
        .map_err(error_text)?;
    let account_id = account.id.clone();
    if let Some(summary) = subscription {
        let _ = controller.save_subscription(&account_id, summary);
    }
    let account = controller.account(&account_id).unwrap_or(account);
    drop(controller);
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    Ok(account)
}

#[tauri::command]
async fn start_official_login(
    kind: ApplicationKind,
    label: Option<String>,
    app: AppHandle,
) -> std::result::Result<Account, String> {
    if kind != ApplicationKind::Cursor {
        return Err(AppError::ComingSoon.to_string());
    }
    let login_id = app.state::<OauthLoginState>().begin();
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || complete_cursor_oauth(label, login_id, worker))
        .await
        .map_err(|error| error.to_string())?
        .map_err(error_text)
}

#[tauri::command]
fn cancel_official_login(state: State<'_, OauthLoginState>) {
    state.cancel();
}

#[tauri::command]
fn open_official_login_url(state: State<'_, OauthLoginState>) -> std::result::Result<(), String> {
    let url = state
        .url()
        .ok_or_else(|| "没有进行中的官方登录。".to_string())?;
    open_browser(&url).map_err(error_text)
}

#[tauri::command]
fn delete_account(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .delete_account(&id)
        .map_err(error_text)?;
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    Ok(())
}

#[tauri::command]
fn switch_account(
    id: String,
    operation_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<SwitchOutcome, String> {
    let outcome = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .switch_account(&id, |stage, percent| {
            emit_switch_progress(&app, &operation_id, &id, stage, percent, "running");
        });
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            emit_switch_progress(&app, &operation_id, &id, "error", 100, "error");
            return Err(error_text(error));
        }
    };
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    if outcome.restart_required {
        emit_switch_progress(&app, &operation_id, &id, "restartRequired", 100, "waiting");
        return Ok(outcome);
    }
    emit_switch_progress(&app, &operation_id, &id, "launching", 90, "running");
    if let Err(error) = launch_cursor() {
        emit_switch_progress(&app, &operation_id, &id, "error", 100, "error");
        return Err(error_text(error));
    }
    emit_switch_progress(&app, &operation_id, &id, "complete", 100, "success");
    Ok(outcome)
}

#[tauri::command]
fn force_restart_cursor(
    id: String,
    operation_id: String,
    app: AppHandle,
) -> std::result::Result<(), String> {
    emit_switch_progress(&app, &operation_id, &id, "terminating", 25, "running");
    if let Err(error) = terminate_cursor().and_then(|_| wait_for_cursor_stop()) {
        emit_switch_progress(&app, &operation_id, &id, "error", 100, "error");
        return Err(error_text(error));
    }
    emit_switch_progress(&app, &operation_id, &id, "launching", 75, "running");
    if let Err(error) = launch_cursor() {
        emit_switch_progress(&app, &operation_id, &id, "error", 100, "error");
        return Err(error_text(error));
    }
    emit_switch_progress(&app, &operation_id, &id, "complete", 100, "success");
    Ok(())
}

fn error_text(error: AppError) -> String {
    error.to_string()
}

fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let controller = state.0.lock().expect("controller mutex");
    let title = MenuItem::with_id(
        app,
        "status",
        format!("Cursor: {}", controller.current_label()),
        false,
        None::<&str>,
    )?;
    let mut accounts = controller.accounts(ApplicationKind::Cursor);
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&title];
    let separator = PredefinedMenuItem::separator(app)?;
    items.push(&separator);
    let switches: Vec<MenuItem<_>> = accounts
        .drain(..)
        .filter(|account| account.import_type.supports_desktop_switch())
        .map(|account| {
            MenuItem::with_id(
                app,
                format!("switch:{}", account.id),
                account.label,
                true,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    for item in &switches {
        items.push(item);
    }
    let after_switches = PredefinedMenuItem::separator(app)?;
    if !switches.is_empty() {
        items.push(&after_switches);
    }
    let open = MenuItem::with_id(app, "open", "打开 Storm Dock", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    items.push(&open);
    items.push(&quit);
    Menu::with_items(app, &items)
}

fn refresh_tray(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build_tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .menu(Menu::default)
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| AppError::Message(error.to_string()))?;
            app.manage(AppState(Mutex::new(Controller::new(data_dir)?)));
            app.manage(OauthLoginState::default());
            let menu = build_tray_menu(app.handle())?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(true)
                .menu(&menu)
                .on_menu_event(|app, event| {
                    let id = event.id().as_ref();
                    if id == "open" {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    } else if id == "quit" {
                        app.exit(0);
                    } else if let Some(account_id) = id.strip_prefix("switch:") {
                        let result =
                            app.state::<AppState>()
                                .0
                                .lock()
                                .ok()
                                .and_then(|mut controller| {
                                    controller.switch_account(account_id, |_, _| {}).ok()
                                });
                        if result.is_some() {
                            refresh_tray(app);
                            let _ = app.emit("accounts-changed", ());
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_applications,
            list_accounts,
            get_database_path,
            move_database,
            export_cursor_accounts,
            get_cursor_export_record,
            refresh_account_subscription,
            get_cursor_usage,
            get_saved_cursor_usage,
            reorder_accounts,
            import_current_account,
            import_token_or_json,
            start_official_login,
            cancel_official_login,
            open_official_login_url,
            delete_account,
            switch_account,
            force_restart_cursor
        ])
        .run(tauri::generate_context!())
        .expect("error while running storm-dock");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cursor_db() -> PathBuf {
        let path = env::temp_dir().join(format!("storm-dock-test-{}.vscdb", uuid::Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        let db = Connection::open(&path).unwrap();
        db.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            params![ACCESS_TOKEN_KEY, "original-token"],
        )
        .unwrap();
        db.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            params![EMAIL_KEY, "original@example.com"],
        )
        .unwrap();
        path
    }

    #[test]
    fn cursor_session_requires_access_token() {
        let session = Session {
            values: BTreeMap::new(),
            raw_export: None,
        };
        assert!(!session.values.contains_key(ACCESS_TOKEN_KEY));
    }

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
            session.values.get("glass.lastSignedInAuthId").map(String::as_str),
            Some("auth0|user_01ABCDEFGHJKMNPQRSTVWXYZ")
        );
        assert_eq!(session.values.get(EMAIL_KEY).map(String::as_str), Some("me@example.com"));
        assert_eq!(import_type(&session), ImportType::Jwt);

        let encoded = Session::from_import(&format!(
            "WorkosCursorSessionToken={user_id}%3A%3A{jwt}"
        ))
        .unwrap();
        assert_eq!(encoded.values.get(ACCESS_TOKEN_KEY), Some(&jwt));

        let token = "a".repeat(40);
        let session_token = Session::from_import(&format!("{user_id}::{token}")).unwrap();
        assert_eq!(session_token.values.get(ACCESS_TOKEN_KEY), Some(&token));
        assert_eq!(
            session_token.values.get("glass.lastSignedInAuthId").map(String::as_str),
            Some(user_id)
        );
        assert_eq!(import_type(&session_token), ImportType::Token);

        let json = Session::from_import(&format!(r#"{{"sessionToken":"{user_id}::{token}"}}"#)).unwrap();
        assert_eq!(json.values.get(ACCESS_TOKEN_KEY), Some(&token));
        assert!(Session::from_import(user_id).is_err());
    }

    #[test]
    fn jwt_claims_accept_padded_payloads() {
        let mut payload = URL_SAFE_NO_PAD.encode(r#"{"email":"me@example.com","sub":"user_01ABCDEFGHJKMNPQRSTVWXYZ"}"#);
        while payload.len() % 4 != 0 {
            payload.push('=');
        }
        assert!(payload.contains('='));
        let claims = jwt_claims(&format!("header.{payload}.signature")).unwrap();
        assert_eq!(claims["email"], "me@example.com");
    }

    #[test]
    fn token_import_labels_account_from_email_or_user_id() {
        let data_dir = env::temp_dir().join(format!("storm-dock-token-label-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let user_id = "user_01ABCDEFGHJKMNPQRSTVWXYZ";
        let token = "a".repeat(40);
        let account = controller
            .import_payload(ApplicationKind::Cursor, None, &format!("{user_id}::{token}"))
            .unwrap();
        assert_eq!(account.label, user_id);
        assert_eq!(account.email, None);

        let claims = URL_SAFE_NO_PAD.encode(
            r#"{"sub":"auth0|user_01ABCDEFGHJKMNPQRSTVWXYZ","email":"me@example.com","exp":4102444800}"#,
        );
        let jwt = format!("header.{claims}.signature-padding-for-length");
        let account = controller
            .import_payload(ApplicationKind::Cursor, None, &format!("{user_id}::{jwt}"))
            .unwrap();
        assert_eq!(account.label, "me@example.com");
        assert_eq!(account.email.as_deref(), Some("me@example.com"));
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn dashboard_cookie_can_use_session_user_id_without_jwt_claims() {
        let token = "a".repeat(40);
        let session = session_from_access_token(&token, Some("user_01ABCDEFGHJKMNPQRSTVWXYZ".into()));
        assert_eq!(
            dashboard_cookie(&session).unwrap(),
            format!("WorkosCursorSessionToken=user_01ABCDEFGHJKMNPQRSTVWXYZ%3A%3A{token}")
        );
    }

    #[test]
    fn cursor_oauth_uses_s256_pkce_and_official_login_url() {
        let challenge = pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let handshake = CursorOauthHandshake::from_parts(
            "11111111-1111-4111-8111-111111111111".into(),
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(),
        );
        assert!(handshake.login_url.starts_with("https://cursor.com/loginDeepControl?"));
        assert!(handshake.login_url.contains("challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
        assert!(handshake.login_url.contains("uuid=11111111-1111-4111-8111-111111111111"));
        assert!(handshake.login_url.contains("mode=login"));
        assert!(handshake.login_url.contains("redirectTarget=cli"));
        assert!(!handshake.login_url.contains("verifier"));
        assert!(!handshake.login_url.contains("dBjftJeZ4CVP"));
    }

    #[test]
    fn oauth_poll_response_builds_a_cursor_session() {
        let claims = URL_SAFE_NO_PAD.encode(r#"{"sub":"auth0|user_123","email":"me@example.com","exp":4102444800}"#);
        let token = format!("header.{claims}.signature-padding-for-length");
        let session = session_from_oauth_poll(&serde_json::json!({
            "accessToken": token,
            "refreshToken": "refresh-token-value",
            "authId": "auth0|user_123"
        }))
        .unwrap();
        assert_eq!(session.values.get(ACCESS_TOKEN_KEY), Some(&token));
        assert_eq!(session.values.get("cursorAuth/refreshToken"), Some(&"refresh-token-value".into()));
        assert_eq!(session.values.get("glass.lastSignedInAuthId"), Some(&"auth0|user_123".into()));
        assert_eq!(session.values.get(EMAIL_KEY), Some(&"me@example.com".into()));
        assert_eq!(import_type(&session), ImportType::OAuth);
    }

    #[test]
    fn dashboard_cookie_requires_a_live_cursor_jwt() {
        let claims = URL_SAFE_NO_PAD.encode(r#"{"sub":"auth0|user_123","exp":4102444800}"#);
        let session = Session {
            values: BTreeMap::from([(
                ACCESS_TOKEN_KEY.into(),
                format!("header.{claims}.signature"),
            )]),
            raw_export: None,
        };
        assert_eq!(
            dashboard_cookie(&session).unwrap(),
            format!("WorkosCursorSessionToken=user_123%3A%3Aheader.{claims}.signature")
        );
        assert!(dashboard_cookie(&Session {
            values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "not-a-jwt".into())]),
            raw_export: None,
        })
        .is_err());
    }

    #[test]
    fn weekly_usage_reads_numeric_timestamps() {
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "timestamp": now_ms, "requestsCosts": "1.5", "model": "composer-2.5-fast" }
        ]});
        let weekly = weekly_usage(&events).unwrap();
        assert_eq!(weekly.last().unwrap().requests, 1.5);
        let models = models_from_events(&events);
        assert_eq!(models[0].name, "composer-2.5-fast");
        assert_eq!(models[0].requests, 2);
    }

    #[test]
    fn weekly_usage_returns_fixed_seven_day_window() {
        let now_ms = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "timestamp": now_ms.to_string(), "requestsCosts": 2.5, "chargedCents": 10.0, "kind": "USAGE_EVENT_KIND_USAGE_BASED" }
        ]});
        let weekly = weekly_usage(&events).unwrap();
        assert_eq!(weekly.len(), 7);
        assert_eq!(weekly.last().unwrap().requests, 2.5);
        assert!(weekly.last().unwrap().is_on_demand);
        assert_eq!(weekly.last().unwrap().on_demand_cents, 10.0);
    }

    #[test]
    fn usage_pools_split_team_hard_limit_from_api_plan() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": {
                    "used": 2000,
                    "limit": 2000,
                    "totalPercentUsed": 38.266666666666666,
                    "breakdown": { "included": 2000, "bonus": 4888, "total": 6888 }
                },
                "onDemand": { "used": 0, "limit": null }
            }
        });
        let hard_limit = serde_json::json!({ "perUserMonthlyLimitDollars": 180 });
        let (primary, on_demand) = usage_pools(&summary, Some(&hard_limit));
        assert_eq!(primary.kind, "currency");
        assert_eq!(primary.used, 6888.0);
        assert_eq!(primary.limit, Some(18000.0));
        assert!((primary.percent - 38.2666).abs() < 0.01);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 2000.0);
        assert_eq!(on_demand.limit, Some(2000.0));
        assert_eq!(on_demand.percent, 100.0);
    }

    #[test]
    fn usage_pools_infer_hard_limit_from_breakdown_percent() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": {
                    "used": 2000,
                    "limit": 2000,
                    "totalPercentUsed": 32.04444444444445,
                    "breakdown": { "included": 2000, "bonus": 3768, "total": 5768 }
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.used, 5768.0);
        assert_eq!(primary.limit, Some(18000.0));
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 2000.0);
        assert_eq!(on_demand.limit, Some(2000.0));
    }

    #[test]
    fn usage_pools_do_not_echo_overage_as_limit_when_percent_is_capped() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": {
                    "used": 2000,
                    "limit": 2000,
                    "totalPercentUsed": 100.0,
                    "autoPercentUsed": 100.0,
                    "apiPercentUsed": 100.0,
                    "breakdown": { "included": 2000, "bonus": 16172, "total": 18172 }
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.used, 18172.0);
        assert_eq!(primary.limit, Some(18000.0));
        assert!((primary.percent - 100.955).abs() < 0.01);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 2000.0);
        assert_eq!(on_demand.limit, Some(2000.0));
    }

    #[test]
    fn usage_pools_pro_uses_auto_percent_and_plan_as_other_models() {
        let summary = serde_json::json!({
            "membershipType": "pro",
            "individualUsage": {
                "plan": {
                    "used": 1234,
                    "limit": 2000,
                    "autoPercentUsed": 12.5,
                    "apiPercentUsed": 61.7,
                    "totalPercentUsed": 40.0
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.kind, "percent");
        assert_eq!(primary.percent, 12.5);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 1234.0);
        assert_eq!(on_demand.limit, Some(2000.0));
        assert_eq!(on_demand.percent, 61.7);
    }

    #[test]
    fn usage_pools_ultra_keeps_included_api_pool_as_other_models() {
        let summary = serde_json::json!({
            "membershipType": "ultra",
            "autoModelSelectedDisplayMessage": "You've used 2% of your included total usage",
            "namedModelSelectedDisplayMessage": "You've used 86% of your included API usage",
            "individualUsage": {
                "plan": {
                    "used": 34400,
                    "limit": 40000,
                    "autoPercentUsed": 2.0,
                    "apiPercentUsed": 86.0,
                    "totalPercentUsed": 86.0
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.kind, "percent");
        assert_eq!(primary.percent, 2.0);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.used, 34400.0);
        assert_eq!(on_demand.limit, Some(40000.0));
        assert_eq!(on_demand.percent, 86.0);
    }

    #[test]
    fn usage_pools_free_uses_auto_and_api_percent() {
        let summary = serde_json::json!({
            "membershipType": "free",
            "individualUsage": {
                "plan": {
                    "used": 0,
                    "limit": 0,
                    "autoPercentUsed": 100.0,
                    "apiPercentUsed": 0.0,
                    "totalPercentUsed": 61.5,
                    "breakdown": { "total": 123 }
                }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.kind, "percent");
        assert_eq!(primary.percent, 100.0);
        let on_demand = on_demand.expect("other models");
        assert_eq!(on_demand.kind, "percent");
        assert_eq!(on_demand.percent, 0.0);
    }

    #[test]
    fn usage_pools_keep_personal_plan_when_no_larger_cap() {
        let summary = serde_json::json!({
            "individualUsage": {
                "plan": { "used": 1200, "limit": 2000, "totalPercentUsed": 60.0, "breakdown": { "total": 1200 } },
                "onDemand": { "used": 400, "limit": 4000 }
            }
        });
        let (primary, on_demand) = usage_pools(&summary, None);
        assert_eq!(primary.used, 1200.0);
        assert_eq!(primary.limit, Some(2000.0));
        let on_demand = on_demand.expect("on demand");
        assert_eq!(on_demand.used, 400.0);
        assert_eq!(on_demand.limit, Some(4000.0));
    }

    #[test]
    fn usage_pools_token_enterprise_uses_overall_and_hard_limit() {
        let summary = serde_json::json!({
            "individualUsage": { "overall": { "used": 17, "limit": null } }
        });
        let hard_limit = serde_json::json!({ "perUserMonthlyLimitDollars": 100 });
        let (primary, on_demand) = usage_pools(&summary, Some(&hard_limit));
        assert_eq!(primary.used, 17.0);
        assert_eq!(primary.limit, Some(10000.0));
        assert!(on_demand.is_none());
    }

    #[test]
    fn models_from_events_sum_weighted_request_costs() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "model": "composer-2.5-fast", "requestsCosts": 2.4 },
            { "modelName": "composer-2.5-fast", "requestsCosts": 1.6 },
            { "model": "gpt-5.5-medium", "requestsCosts": 8 },
            { "model": "ignored", "requestsCosts": 0 }
        ]});
        let models = models_from_events(&events);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "gpt-5.5-medium");
        assert_eq!(models[0].requests, 8);
        assert_eq!(models[1].name, "composer-2.5-fast");
        assert_eq!(models[1].requests, 4);
    }

    #[test]
    fn models_from_events_read_dashboard_model_name_field() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "modelName": "cursor-model", "requestsCosts": 3.2 }
        ]});
        let models = models_from_events(&events);
        assert_eq!(models[0].name, "cursor-model");
        assert_eq!(models[0].requests, 3);
    }

    #[test]
    fn models_from_events_read_nested_model_object() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            { "model": { "name": "composer-2.5-fast" }, "requestsCosts": 4 }
        ]});
        let models = models_from_events(&events);
        assert_eq!(models[0].name, "composer-2.5-fast");
        assert_eq!(models[0].requests, 4);
    }

    #[test]
    fn usage_event_rows_map_dashboard_fields_newest_first() {
        let events = serde_json::json!({ "usageEventsDisplay": [
            {
                "timestamp": 1_700_000_000_000u64,
                "model": "composer-2.5-fast",
                "requestsCosts": 1.5,
                "inputTokens": 12,
                "outputTokens": 4,
                "costUsd": 0.2
            },
            {
                "timestamp": "1700000001000",
                "modelName": "gpt-5.5-medium",
                "requestsCosts": 3,
                "chargedCents": 25,
                "kind": "USAGE_EVENT_KIND_USAGE_BASED"
            }
        ]});
        let rows = usage_event_rows(&events);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].timestamp, 1_700_000_001_000);
        assert_eq!(rows[0].model.as_deref(), Some("gpt-5.5-medium"));
        assert!(rows[0].on_demand);
        assert_eq!(rows[0].charged_cents, Some(25.0));
        assert_eq!(rows[1].model.as_deref(), Some("composer-2.5-fast"));
        assert_eq!(rows[1].input_tokens, Some(12.0));
        assert_eq!(rows[1].output_tokens, Some(4.0));
        assert!(!rows[1].on_demand);
    }

    #[test]
    fn usage_events_body_omits_null_user_id() {
        let personal = usage_events_body(Some(0), None, 1, false);
        assert_eq!(personal["teamId"], 0);
        assert_eq!(personal["page"], 1);
        assert_eq!(personal["pageSize"], 100);
        assert!(personal.get("userId").is_none());
        assert!(personal.get("startDate").is_none());
        let enterprise = usage_events_body(Some(77), Some(42), 2, true);
        assert_eq!(enterprise["teamId"], 77);
        assert_eq!(enterprise["userId"], 42);
        assert_eq!(enterprise["page"], 2);
        assert!(enterprise.get("startDate").and_then(serde_json::Value::as_str).is_some());
        assert!(enterprise.get("endDate").and_then(serde_json::Value::as_str).is_some());
        let unscoped = usage_events_body(None, Some(9), 1, false);
        assert!(unscoped.get("teamId").is_none());
        assert_eq!(unscoped["userId"], 9);
    }

    #[test]
    fn usage_events_from_response_treats_missing_or_null_as_empty() {
        assert_eq!(usage_events_from_response(&serde_json::json!({ "totalUsageEventsCount": 0 })), Some(vec![]));
        assert_eq!(usage_events_from_response(&serde_json::json!({ "usageEventsDisplay": null })), Some(vec![]));
        assert_eq!(
            usage_events_from_response(&serde_json::json!({ "usageEvents": [{ "model": "composer-2.5-fast" }] }))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn auth_numeric_id_reads_dashboard_me_id() {
        assert_eq!(auth_numeric_id(&serde_json::json!({ "id": "232352588", "email": "a@b.c" })), Some(232_352_588));
        assert_eq!(auth_numeric_id(&serde_json::json!({ "userId": 42 })), Some(42));
    }

    #[test]
    fn first_team_id_reads_string_and_numeric_ids() {
        assert_eq!(
            first_team_id(&serde_json::json!({ "teams": [{ "id": "13403082", "name": "acme" }] })),
            Some(13_403_082)
        );
        assert_eq!(
            first_team_id(&serde_json::json!({ "teams": [{ "teamId": 9 }] })),
            Some(9)
        );
    }

    #[test]
    fn team_member_user_id_trims_email_and_accepts_string_ids() {
        let spend = serde_json::json!({
            "teamMemberSpend": [
                { "userId": "232352588", "email": " User@Example.com " },
                { "id": 1, "email": "other@example.com" }
            ]
        });
        assert_eq!(
            team_member_user_id(&spend, &["user@example.com".into()]),
            Some(232_352_588)
        );
        assert_eq!(
            team_member_user_id(&spend, &[" missing@example.com ".into()]),
            None
        );
    }

    #[test]
    fn subscription_summary_uses_local_plan_and_cursor_response_dates() {
        let session = Session {
            values: BTreeMap::from([(MEMBERSHIP_TYPE_KEY.into(), "pro".into())]),
            raw_export: None,
        };
        assert_eq!(
            subscription_from_session(&session).plan.as_deref(),
            Some("pro")
        );
        let summary = subscription_from_response(&serde_json::json!({
            "stripeMembershipType": "free",
            "subscription": { "current_period_end": 1_800_000_000_000u64 }
        }));
        assert_eq!(summary.plan.as_deref(), Some("free"));
        assert_eq!(summary.expires_at, Some(1_800_000_000));
        assert!(summary.checked_at.is_some());

        let live_shape =
            subscription_from_response(&serde_json::json!({ "membershipType": "free" }));
        assert_eq!(live_shape.plan.as_deref(), Some("free"));
        assert_eq!(live_shape.expires_at, None);

        let usage_summary = subscription_from_response(&serde_json::json!({
            "membershipType": "pro",
            "billingCycleEnd": "2026-08-27T00:00:00.000Z"
        }));
        assert_eq!(usage_summary.plan.as_deref(), Some("pro"));
        assert_eq!(
            usage_summary.expires_at,
            Some(
                OffsetDateTime::parse("2026-08-27T00:00:00.000Z", &Rfc3339)
                    .unwrap()
                    .unix_timestamp() as u64
            )
        );
        let merged = merge_subscription(Some(usage_summary), Some(summary));
        assert_eq!(merged.unwrap().expires_at, Some(
            OffsetDateTime::parse("2026-08-27T00:00:00.000Z", &Rfc3339)
                .unwrap()
                .unix_timestamp() as u64
        ));
    }

    #[test]
    fn missing_subscription_response_values_do_not_fabricate_an_expiry() {
        let summary = subscription_from_response(&serde_json::json!({ "status": "active" }));
        assert_eq!(summary.plan, None);
        assert_eq!(summary.expires_at, None);
    }

    #[test]
    fn remaining_days_use_utc_calendar_dates() {
        assert_eq!(utc_days_remaining(6 * 86_400 + 1, 1 * 86_400 + 86_399), 5);
        assert_eq!(utc_days_remaining(1 * 86_400 + 86_399, 1 * 86_400), 0);
    }

    #[test]
    fn finds_same_application_account_by_email_case_insensitively() {
        let account = Account {
            id: "acc_test".into(),
            application: ApplicationKind::Cursor,
            label: "Test".into(),
            email: Some("me@example.com".into()),
            import_type: ImportType::Native,
            subscription: SubscriptionSummary::default(),
            raw_export: serde_json::json!({}),
            created_at: 1,
            updated_at: 1,
            last_used_at: 1,
        };
        assert_eq!(
            matching_account_index(
                &[account.clone()],
                ApplicationKind::Cursor,
                "ME@example.com"
            ),
            Some(0)
        );
        assert_eq!(
            matching_account_index(&[account], ApplicationKind::Codex, "me@example.com"),
            None
        );
    }

    #[test]
    fn account_summaries_do_not_include_session_values() {
        let data_dir = env::temp_dir().join(format!("storm-dock-summary-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let session = Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "secret-token".into())]), raw_export: None };
        controller.save_imported_session(ApplicationKind::Cursor, Some("Test".into()), session, ImportType::Token).unwrap();
        let json = serde_json::to_string(&controller.accounts(ApplicationKind::Cursor)).unwrap();
        assert!(!json.contains("secret-token"));
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn usage_snapshot_is_serializable_without_credentials() {
        let snapshot = CursorUsageDetails {
            account_id: "acc_test".into(),
            label: "Test".into(),
            email: Some("me@example.com".into()),
            name: None,
            membership_type: Some("pro".into()),
            primary: usage_metric("currency", 25.0, Some(100.0)),
            reset_at: None,
            on_demand: None,
            models: vec![],
            weekly: vec![],
            weekly_available: false,
            weekly_error: None,
            events: vec![],
            checked_at: 1,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("checkedAt"));
        assert!(!json.contains("accessToken"));
        assert!(!json.contains("WorkosCursorSessionToken"));
    }

    #[test]
    fn account_order_is_preserved() {
        let data_dir = env::temp_dir().join(format!("storm-dock-order-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let first = controller.save_imported_session(ApplicationKind::Cursor, Some("First".into()), Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "first-token".into())]), raw_export: None }, ImportType::Token).unwrap();
        let second = controller.save_imported_session(ApplicationKind::Cursor, Some("Second".into()), Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "second-token".into())]), raw_export: None }, ImportType::Token).unwrap();
        let second_id = second.id.clone();
        controller
            .reorder_accounts(
                ApplicationKind::Cursor,
                vec![second_id.clone(), first.id],
            )
            .unwrap();
        assert_eq!(controller.accounts(ApplicationKind::Cursor)[0].id, second_id);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn database_move_preserves_accounts_and_sessions() {
        let source = env::temp_dir().join(format!("storm-dock-source-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let destination = env::temp_dir().join(format!("storm-dock-destination-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&destination).unwrap();
        let mut controller = Controller::new(source.clone()).unwrap();
        let account = controller.save_imported_session(ApplicationKind::Cursor, Some("Test".into()), Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "secret-token".into())]), raw_export: None }, ImportType::Token).unwrap();
        let path = controller.move_database(destination.clone()).unwrap();
        assert_eq!(PathBuf::from(path), destination.join(DATABASE_NAME));
        assert_eq!(controller.load_session(&account.id).unwrap().values.get(ACCESS_TOKEN_KEY), Some(&"secret-token".into()));
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn export_preserves_cursor_raw_record_without_frontend_serialization() {
        let data_dir = env::temp_dir().join(format!("storm-dock-export-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        controller.cursor = CursorAdapter { database: None };
        let token = "a".repeat(40);
        let source = format!(r#"[{{"id":"cursor_source","access_token":"{token}","auth_id":"auth0|user_1","email":"me@example.com","cursor_auth_raw":{{"accessToken":"{token}","custom":true}},"cursor_usage_raw":{{"total_input_tokens":123}},"telemetry_machine_ids":{{"machineId":"source-machine"}}}}]"#);
        controller.import_payload(ApplicationKind::Cursor, None, &source).unwrap();
        let file = data_dir.join("cursor-accounts.json");
        controller.export_cursor_accounts(file.clone()).unwrap();
        let exported: serde_json::Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        let account = &exported[0];
        assert_eq!(account["id"], "cursor_source");
        assert_eq!(account["access_token"], token);
        assert_eq!(account["cursor_auth_raw"]["custom"], true);
        assert_eq!(account["cursor_usage_raw"]["total_input_tokens"], 123);
        assert_eq!(account["telemetry_machine_ids"]["machineId"], "source-machine");
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn usage_export_matches_the_requested_shape_without_internal_sources() {
        let mut record = serde_json::json!({ "id": "cursor_source" });
        update_export_usage(&mut record, serde_json::json!({
            "auth_me": { "email": "me@example.com" },
            "usage_summary": { "membershipType": "enterprise", "billingCycleEnd": "2026-08-27T00:00:00.000Z" },
            "usage": { "fallback": { "numRequests": 2 } },
            "usage_events": { "usageEventsDisplay": [{
                "model": "cursor-model", "requestsCosts": 1, "inputTokens": 12, "outputTokens": 5, "costUsd": 1.25
            }, {
                "model": "cursor-model", "requestsCosts": 1, "inputTokens": 3, "outputTokens": 7, "costUsd": 0.75
            }] }
        }), 42);
        assert_eq!(record["cursor_usage_raw"]["membershipType"], "enterprise");
        assert_eq!(record["cursor_usage_raw"]["total_input_tokens"], 15.0);
        assert_eq!(record["cursor_usage_raw"]["total_output_tokens"], 12.0);
        assert_eq!(record["cursor_usage_raw"]["used_models"][0]["model_name"], "cursor-model");
        assert_eq!(record["cursor_usage_raw"]["used_models"][0]["num_requests"], 2);
        assert_eq!(record["cursor_usage_raw"]["used_models"][0]["cost_usd"], 2.0);
        assert!(record.get("cursor_usage_sources").is_none());
        assert!(record.get("total_input_tokens").is_none());
    }

    #[test]
    fn database_migration_creates_raw_export_from_saved_session() {
        let data_dir = env::temp_dir().join(format!("storm-dock-raw-migration-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let account = controller.save_imported_session(ApplicationKind::Cursor, None, Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a".repeat(40)), (EMAIL_KEY.into(), "me@example.com".into())]), raw_export: None }, ImportType::Token).unwrap();
        controller.database.execute("UPDATE accounts SET usage_raw_json=?1 WHERE id=?2", params![r#"{"membershipType":"enterprise","billingCycleEnd":"2026-08-27T00:00:00.000Z"}"#, account.id]).unwrap();
        controller.database.execute("UPDATE accounts SET raw_export_json=NULL WHERE id=?1", params![account.id]).unwrap();
        drop(controller);
        let controller = Controller::new(data_dir.clone()).unwrap();
        let exported = controller.export_cursor_account(&controller.account(&account.id).unwrap()).unwrap();
        assert_eq!(exported["access_token"], "a".repeat(40));
        assert_eq!(exported["cursor_auth_raw"]["cachedEmail"], "me@example.com");
        assert_eq!(exported["cursor_usage_raw"]["membershipType"], "enterprise");
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn cursor_adapter_imports_and_switches_a_valid_session() {
        let path = test_cursor_db();
        let adapter = CursorAdapter {
            database: Some(path.clone()),
        };
        let imported = adapter.import_current().unwrap();
        assert_eq!(
            imported.values.get(ACCESS_TOKEN_KEY).unwrap(),
            "original-token"
        );

        let mut replacement = imported.clone();
        replacement
            .values
            .insert(ACCESS_TOKEN_KEY.into(), "replacement-token".into());
        adapter.apply(&replacement).unwrap();

        assert_eq!(
            adapter
                .import_current()
                .unwrap()
                .values
                .get(ACCESS_TOKEN_KEY)
                .map(String::as_str),
            Some("replacement-token")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn cursor_adapter_rejects_an_incomplete_session_without_writing() {
        let path = test_cursor_db();
        let adapter = CursorAdapter {
            database: Some(path.clone()),
        };
        let error = adapter.apply(&Session {
            values: BTreeMap::new(),
            raw_export: None,
        });
        assert!(matches!(error, Err(AppError::SecretMissing)));
        assert_eq!(
            adapter
                .import_current()
                .unwrap()
                .values
                .get(ACCESS_TOKEN_KEY)
                .map(String::as_str),
            Some("original-token")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn cursor_adapter_switches_when_current_session_is_signed_out() {
        let path = test_cursor_db();
        let db = Connection::open(&path).unwrap();
        db.execute("DELETE FROM ItemTable WHERE key=?1", [ACCESS_TOKEN_KEY])
            .unwrap();
        drop(db);
        let adapter = CursorAdapter {
            database: Some(path.clone()),
        };
        let replacement = Session {
            values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "replacement-token".into())]),
            raw_export: None,
        };

        adapter.apply(&replacement).unwrap();

        assert_eq!(
            adapter
                .import_current()
                .unwrap()
                .values
                .get(ACCESS_TOKEN_KEY)
                .map(String::as_str),
            Some("replacement-token")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn token_and_jwt_accounts_cannot_switch_desktop() {
        let data_dir = env::temp_dir().join(format!("storm-dock-token-switch-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let token = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Token".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a".repeat(40))]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let jwt = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Jwt".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a.b.c".into())]),
                    raw_export: None,
                },
                ImportType::Jwt,
            )
            .unwrap();

        for account in [&token, &jwt] {
            let error = controller.switch_account(&account.id, |_, _| {}).unwrap_err();
            assert!(
                error.to_string().contains("只能查询用量"),
                "{error}"
            );
        }
        let _ = fs::remove_dir_all(data_dir);
    }
}
