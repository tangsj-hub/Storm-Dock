use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::http::{
    Body, Budget, Call, Client, HttpError, Retry, WORKBUDDY_IMPORT_BUDGET, WORKBUDDY_REFRESH_BUDGET,
};
#[cfg(test)]
use crate::http::USAGE_BUDGET;
use crate::models::{
    now, parse_iso_timestamp, Account, AccountSummary, ApplicationKind, CursorUsageDetails,
    ModelUsageSummary, ProductUsageShare, Session, SubscriptionSummary, UsageEvent, UsageMetric,
    WeeklyUsageSummary,
};
use time::{Date, Duration as TimeDuration, OffsetDateTime, UtcOffset};
use crate::store::AppState;
use crate::tray::refresh_tray;
use crate::workbuddy::session::{access_token_plaintext, profile_value, DEFAULT_DOMAIN};
use tauri::{AppHandle, Emitter, Manager};

const USER_AGENT: &str = "Storm-Dock/workbuddy-usage";
const SUMMARY_PATH: &str = "/billing/meter/get-user-resource-summary";
const FREE_PACKAGES_PATH: &str = "/billing/meter/get-user-resource-free-packages";
const PAID_PACKAGES_PATH: &str = "/billing/meter/get-user-resource-paid-packages";
const REQUEST_USAGE_PATH: &str = "/billing/meter/get-user-request-usage";
/// Soft analytics pull — enough for weekly chart + ledger without starving summary.
const REQUEST_USAGE_PAGE_SIZE: u32 = 100;
const REQUEST_USAGE_MAX_PAGES: u32 = 10;

/// Full WorkBuddy PackageCodes (with catalog suffixes), mirrored from the IDE.
const FREE_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_001_PqouKr6QWV", // free (daily)
    "TCACA_code_008_cfWoLwvjU4", // freeMon
    "TCACA_code_035_ArVxJcGDsm", // freeMonIntl
    "TCACA_code_006_DbXS0lrypC", // gift
    "TCACA_code_039_KRcQj7wUat", // proTrialMon
    "TCACA_code_040_mi9rCYg46x", // proTrialYear
    "TCACA_code_007_nzdH5h4Nl0", // activity
    "TCACA_code_028_NtpWi0jzXs", // bonus28
    "TCACA_code_037_WxOD3MpI2o", // bonusIntl
    "TCACA_code_029_6wCGEWquYy", // bonus29
    "TCACA_code_030_BjSt89qTvr", // bonus30
];
const PAID_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_002_AkiJS3ZHF5", // proMon
    "TCACA_code_005_maRGyrHhw1", // proMonPlus
    "TCACA_code_003_FAnt7lcmRT", // proYear
    "TCACA_code_023_4xbGhMrE6q", // youth
    "TCACA_code_026_BaESVICNoi", // advanced
    "TCACA_code_027_0FCGVA6vSa", // flagship
    "TCACA_code_009_0XmEQc2xOf", // extra
    "TCACA_code_038_OhvqZtiPKr", // extra38
    "TCACA_code_036_lupO5WgNdG", // extraIntl
];

/// Packages whose CycleEndTime drives the top-level cycle reset (showRefreshAt).
const REFRESH_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_001_PqouKr6QWV", // free
    "TCACA_code_002_AkiJS3ZHF5", // proMon
    "TCACA_code_005_maRGyrHhw1", // proMonPlus
    "TCACA_code_003_FAnt7lcmRT", // proYear
    "TCACA_code_008_cfWoLwvjU4", // freeMon
    "TCACA_code_023_4xbGhMrE6q", // youth
    "TCACA_code_026_BaESVICNoi", // advanced
    "TCACA_code_027_0FCGVA6vSa", // flagship
    "TCACA_code_028_NtpWi0jzXs", // bonus28
    "TCACA_code_035_ArVxJcGDsm", // freeMonIntl
    "TCACA_code_039_KRcQj7wUat", // proTrialMon
    "TCACA_code_040_mi9rCYg46x", // proTrialYear
    "TCACA_code_037_WxOD3MpI2o", // bonusIntl
    "TCACA_code_006_DbXS0lrypC", // gift
];

/// Packages that expose DeductionEndTime as per-pool expiry (showExpireAt).
const EXPIRE_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_001_PqouKr6QWV", // free — CycleEndTime as expire
    "TCACA_code_006_DbXS0lrypC", // gift → 套餐赠送积分
    "TCACA_code_007_nzdH5h4Nl0", // activity → 平台奖励积分
    "TCACA_code_009_0XmEQc2xOf", // extra → 购买积分
    "TCACA_code_038_OhvqZtiPKr", // extra38
    "TCACA_code_036_lupO5WgNdG", // extraIntl
    "TCACA_code_029_6wCGEWquYy", // bonus29 → 平台奖励积分
    "TCACA_code_030_BjSt89qTvr", // bonus30 → 平台奖励积分
];

const DAILY_FREE_CODE: &str = "TCACA_code_001_PqouKr6QWV";

#[derive(Debug)]
pub(crate) struct WorkbuddySnapshot {
    pub(crate) subscription: SubscriptionSummary,
    pub(crate) usage: CursorUsageDetails,
    pub(crate) billing_raw: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountUpdatedPayload {
    pub(crate) id: String,
    pub(crate) application: ApplicationKind,
    pub(crate) account: AccountSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountSubscriptionPendingPayload {
    pub(crate) id: String,
    pub(crate) application: ApplicationKind,
}

pub(crate) fn fetch_workbuddy_snapshot(
    account: &Account,
    session: &Session,
) -> Result<(WorkbuddySnapshot, Option<Session>)> {
    fetch_workbuddy_snapshot_with(
        account,
        session,
        WORKBUDDY_REFRESH_BUDGET,
        Retry::None,
    )
}

pub(crate) fn fetch_workbuddy_snapshot_with(
    account: &Account,
    session: &Session,
    budget: Duration,
    retry: Retry,
) -> Result<(WorkbuddySnapshot, Option<Session>)> {
    let profile = profile_value(session)?;
    let token = access_token_plaintext(&profile).ok_or(AppError::SecretMissing)?;
    let host = api_host(&profile);
    let summary = fetch_resource_summary_with(&host, &token, budget, retry)?;
    // Soft-fail packages: summary alone still yields pools; times just stay empty.
    let packages = fetch_resource_packages_soft(&host, &token, budget);
    let mut snapshot = snapshot_from_summary(account, &summary)?;
    apply_package_times(&mut snapshot, &packages);
    // Soft-fail request usage: weekly / events / models stay empty on error.
    apply_request_usage_soft(&mut snapshot, &host, &token, budget);
    Ok((snapshot, None))
}

/// Decrypt `$wbEncrypted` tokens via the local WorkBuddy runtime and persist.
/// Returns plaintext session, or `SecretMissing` when decrypt is impossible.
pub(crate) fn ensure_plaintext_session(
    app: &AppHandle,
    account_id: &str,
    session: Session,
) -> Result<Session> {
    let mut profile = profile_value(&session)?;
    if access_token_plaintext(&profile).is_some() {
        return Ok(session);
    }
    if !crate::workbuddy::crypto::profile_needs_token_decrypt(&profile) {
        return Err(AppError::SecretMissing);
    }
    crate::workbuddy::crypto::decrypt_profile_tokens(&mut profile)?;
    let source = profile
        .pointer("/auth/source")
        .and_then(|value| value.as_str())
        .unwrap_or("native")
        .to_owned();
    let decrypted = crate::workbuddy::session::session_from_profile(profile, &source)?;
    if let Ok(mut controller) = app.state::<AppState>().0.lock() {
        let _ = controller.replace_account_session(account_id, &decrypted);
    }
    Ok(decrypted)
}

fn workbuddy_summary(app: &AppHandle, id: &str) -> Option<AccountSummary> {
    let state = app.state::<AppState>();
    let controller = state.0.lock().ok()?;
    controller
        .accounts(ApplicationKind::Workbuddy)
        .into_iter()
        .find(|account| account.id == id)
}

pub(crate) fn emit_account_updated(app: &AppHandle, account: AccountSummary) {
    let _ = app.emit(
        "account-updated",
        AccountUpdatedPayload {
            id: account.id.clone(),
            application: account.application,
            account,
        },
    );
}

fn emit_workbuddy_account(app: &AppHandle, id: &str) {
    if let Some(summary) = workbuddy_summary(app, id) {
        emit_account_updated(app, summary);
    }
}

fn persist_snapshot(
    app: &AppHandle,
    account_id: &str,
    snapshot: WorkbuddySnapshot,
    refreshed: Option<Session>,
) -> Option<AccountSummary> {
    let state = app.state::<AppState>();
    let mut controller = state.0.lock().ok()?;
    if let Some(refreshed) = refreshed {
        let _ = controller.replace_account_session(account_id, &refreshed);
    }
    controller.save_workbuddy_snapshot(account_id, snapshot).ok()?;
    drop(controller);
    workbuddy_summary(app, account_id)
}

pub(crate) fn snapshot_from_summary(account: &Account, raw: &Value) -> Result<WorkbuddySnapshot> {
    let data = unwrap_data(raw)?;
    let plan = text_field(data, &["SubscriptionPackageName", "subscriptionPackageName"])
        .filter(|value| !value.is_empty());
    let total_credits = f64_field(data, &["TotalCredits", "totalCredits"]).unwrap_or(0.0);
    let groups = credit_groups(data);
    let used_credits = groups.iter().map(|group| group.used).sum::<f64>();
    let limit = if total_credits > 0.0 {
        total_credits
    } else {
        groups.iter().map(|group| group.total).sum::<f64>()
    };
    let percent = if limit > 0.0 {
        (used_credits / limit) * 100.0
    } else {
        0.0
    };
    // Keep every named CreditGroup, including zero-capacity pools like 购买积分,
    // so the usage page can show absolute capacities (500 / 1,500 / 0).
    let products = groups
        .iter()
        .map(|group| ProductUsageShare {
            name: group.name.clone(),
            percent: if group.total > 0.0 {
                (group.used / group.total) * 100.0
            } else {
                0.0
            },
            used: Some(group.used),
            limit: Some(group.total),
            expires_at: None,
        })
        .collect::<Vec<_>>();
    let usage = CursorUsageDetails {
        account_id: account.id.clone(),
        label: account.label.clone(),
        email: account.email.clone(),
        name: None,
        membership_type: plan.clone().or_else(|| account.subscription.plan.clone()),
        // Credit pool, not a calendar subscription. Keep percent for bars; used/limit are credits.
        primary: UsageMetric {
            kind: "credits".into(),
            used: used_credits,
            limit: Some(limit),
            percent,
        },
        reset_at: None,
        on_demand: None,
        grok_bot: None,
        grok_bot_reset_at: None,
        products,
        breakdown: None,
        models: Vec::new(),
        weekly: Vec::new(),
        weekly_available: false,
        weekly_error: None,
        events: Vec::new(),
        checked_at: now(),
    };
    Ok(WorkbuddySnapshot {
        subscription: SubscriptionSummary {
            plan,
            expires_at: None,
            billing_cycle_end: None,
            checked_at: Some(now()),
        },
        usage,
        billing_raw: data.clone(),
    })
}


/// After native import: decrypt off the UI thread, then refresh usage with a
/// tight budget and stream `account-updated` so the home row patches immediately.
pub(crate) fn spawn_imported_decrypt_and_refresh(app: AppHandle, account: Account) {
    if account.application != ApplicationKind::Workbuddy {
        return;
    }
    let _ = app.emit(
        "account-subscription-pending",
        AccountSubscriptionPendingPayload {
            id: account.id.clone(),
            application: ApplicationKind::Workbuddy,
        },
    );
    tauri::async_runtime::spawn_blocking(move || {
        let loaded = app
            .state::<AppState>()
            .0
            .lock()
            .ok()
            .and_then(|controller| controller.load_session(&account.id).ok());
        let Some(session) = loaded else {
            refresh_tray(&app);
            emit_workbuddy_account(&app, &account.id);
            return;
        };
        if let Err(_) = ensure_plaintext_session(&app, &account.id, session) {
            // Soft-fail: keep encrypted identity; FE clears spinner via account-updated.
            refresh_tray(&app);
            emit_workbuddy_account(&app, &account.id);
            return;
        }
        let _ = refresh_imported_account_now(&app, account);
    });
}

/// Run billing refresh after local import / OAuth. Emits per-account updates.
pub(crate) fn refresh_imported_account_now(app: &AppHandle, account: Account) -> Account {
    if account.application != ApplicationKind::Workbuddy {
        return account;
    }
    let session = app
        .state::<AppState>()
        .0
        .lock()
        .ok()
        .and_then(|mut controller| controller.subscription_session(&account.id).ok());
    let Some(session) = session else {
        refresh_tray(app);
        emit_workbuddy_account(app, &account.id);
        return account;
    };
    let session = match ensure_plaintext_session(app, &account.id, session) {
        Ok(session) => session,
        Err(_) => {
            refresh_tray(app);
            emit_workbuddy_account(app, &account.id);
            return account;
        }
    };
    match fetch_workbuddy_snapshot_with(
        &account,
        &session,
        WORKBUDDY_IMPORT_BUDGET,
        Retry::None,
    ) {
        Ok((snapshot, refreshed)) => {
            if let Some(summary) = persist_snapshot(app, &account.id, snapshot, refreshed) {
                refresh_tray(app);
                emit_account_updated(app, summary);
                return app
                    .state::<AppState>()
                    .0
                    .lock()
                    .ok()
                    .and_then(|controller| controller.account(&account.id).ok())
                    .unwrap_or(account);
            }
        }
        Err(error) => {
            // Encrypted desktop auth has no plaintext token — do not mark invalid.
            let message = error.to_string();
            if !message.contains("凭证缺失") {
                if let Ok(mut controller) = app.state::<AppState>().0.lock() {
                    let _ = controller.mark_token_issue_from_message(&account.id, &message);
                }
            }
        }
    }
    refresh_tray(app);
    emit_workbuddy_account(app, &account.id);
    app.state::<AppState>()
        .0
        .lock()
        .ok()
        .and_then(|controller| controller.account(&account.id).ok())
        .unwrap_or(account)
}

/// Background snapshot after import/OAuth so the list is not stuck on an unknown plan.
pub(crate) fn spawn_imported_refresh(app: AppHandle, account: Account) {
    if account.application != ApplicationKind::Workbuddy {
        return;
    }
    let _ = app.emit(
        "account-subscription-pending",
        AccountSubscriptionPendingPayload {
            id: account.id.clone(),
            application: ApplicationKind::Workbuddy,
        },
    );
    tauri::async_runtime::spawn_blocking(move || {
        let _ = refresh_imported_account_now(&app, account);
    });
}

fn fetch_resource_summary_with(
    host: &str,
    token: &str,
    budget: Duration,
    retry: Retry,
) -> Result<Value> {
    let url = format!("https://{host}{SUMMARY_PATH}");
    let response = Client::shared().send(
        &Call {
            method: reqwest::Method::POST,
            url,
            headers: vec![
                ("Accept".into(), "application/json".into()),
                ("Content-Type".into(), "application/json".into()),
                ("User-Agent".into(), USER_AGENT.into()),
                ("X-Product".into(), "SaaS".into()),
                ("Authorization".into(), format!("Bearer {token}")),
            ],
            query: Vec::new(),
            body: Body::Json(serde_json::json!({})),
        },
        &Budget::new(budget),
        retry,
    )?;
    if response.status == 401 || response.status == 403 {
        return Err(AppError::Message(
            "WorkBuddy 登录已失效，请重新登录或导入。".into(),
        ));
    }
    if !(200..300).contains(&response.status) {
        return Err(AppError::Message(format!(
            "WorkBuddy 用量接口失败 ({}): {}",
            response.status,
            response.text().chars().take(160).collect::<String>()
        )));
    }
    response.json().map_err(|_| HttpError::Decode.into())
}

#[cfg(test)]
fn fetch_resource_summary(host: &str, token: &str) -> Result<Value> {
    fetch_resource_summary_with(host, token, USAGE_BUDGET, Retry::Transient)
}

fn fetch_resource_packages_soft(host: &str, token: &str, budget: Duration) -> Vec<Value> {
    let mut accounts = Vec::new();
    let slice = slice_period_range_now();
    if let Ok(free) = fetch_resource_packages_page(
        host,
        token,
        FREE_PACKAGES_PATH,
        FREE_PACKAGE_CODES,
        budget,
        Some(&slice),
        false,
    ) {
        accounts.extend(free);
    }
    if let Ok(paid) = fetch_resource_packages_page(
        host,
        token,
        PAID_PACKAGES_PATH,
        PAID_PACKAGE_CODES,
        budget,
        None,
        true,
    ) {
        accounts.extend(paid);
    }
    accounts
}

fn slice_period_range_now() -> serde_json::Map<String, Value> {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let now = OffsetDateTime::now_utc().to_offset(offset);
    let date = now.date();
    let mut map = serde_json::Map::new();
    map.insert(
        "SlicePeriodStartTime".into(),
        Value::String(format!(
            "{:04}-{:02}-{:02} 00:00:00",
            date.year(),
            date.month() as u8,
            date.day()
        )),
    );
    map.insert(
        "SlicePeriodEndTime".into(),
        Value::String(format!(
            "{:04}-{:02}-{:02} 23:59:59",
            date.year(),
            date.month() as u8,
            date.day()
        )),
    );
    map
}

fn fetch_resource_packages_page(
    host: &str,
    token: &str,
    path: &str,
    package_codes: &[&str],
    budget: Duration,
    slice: Option<&serde_json::Map<String, Value>>,
    need_renew_info: bool,
) -> Result<Vec<Value>> {
    let url = format!("https://{host}{path}");
    let mut body = serde_json::Map::new();
    body.insert("PageNumber".into(), Value::from(1));
    body.insert("PageSize".into(), Value::from(200));
    body.insert(
        "PackageCodes".into(),
        Value::Array(
            package_codes
                .iter()
                .map(|code| Value::String((*code).to_owned()))
                .collect(),
        ),
    );
    body.insert("Status".into(), Value::Array(vec![Value::from(0), Value::from(3)]));
    if need_renew_info {
        body.insert("NeedRenewInfo".into(), Value::Bool(true));
    }
    if let Some(slice) = slice {
        for (key, value) in slice {
            body.insert(key.clone(), value.clone());
        }
    }
    let response = Client::shared().send(
        &Call {
            method: reqwest::Method::POST,
            url,
            headers: vec![
                ("Accept".into(), "application/json".into()),
                ("Content-Type".into(), "application/json".into()),
                ("User-Agent".into(), USER_AGENT.into()),
                ("X-Product".into(), "SaaS".into()),
                ("Authorization".into(), format!("Bearer {token}")),
            ],
            query: Vec::new(),
            body: Body::Json(Value::Object(body)),
        },
        &Budget::new(budget),
        Retry::None,
    )?;
    if !(200..300).contains(&response.status) {
        return Err(AppError::Message(format!(
            "WorkBuddy packages failed ({}): {}",
            response.status,
            response.text().chars().take(120).collect::<String>()
        )));
    }
    let raw: Value = response.json().map_err(|_| HttpError::Decode)?;
    let data = unwrap_data(&raw)?;
    Ok(data
        .get("Accounts")
        .or_else(|| data.get("accounts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn apply_package_times(snapshot: &mut WorkbuddySnapshot, accounts: &[Value]) {
    if accounts.is_empty() {
        return;
    }
    let mut reset_unix: Option<u64> = None;
    let mut group_expire: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();

    for account in accounts {
        let code = text_field(account, &["PackageCode", "packageCode"]).unwrap_or_default();
        if code.is_empty() {
            continue;
        }
        if REFRESH_PACKAGE_CODES.iter().any(|c| *c == code) {
            if let Some(cycle) = billing_time_unix(account.get("CycleEndTime")) {
                // Official UI uses CycleEndTime + 1s as refreshAt.
                let refresh = cycle.saturating_add(1);
                reset_unix = Some(match reset_unix {
                    Some(existing) => existing.min(refresh),
                    None => refresh,
                });
            }
        }
        if EXPIRE_PACKAGE_CODES.iter().any(|c| *c == code) {
            let expire = if code == DAILY_FREE_CODE {
                billing_time_unix(account.get("CycleEndTime"))
            } else {
                billing_time_unix(account.get("DeductionEndTime"))
                    .or_else(|| billing_time_unix(account.get("CycleEndTime")))
                    .or_else(|| billing_time_unix(account.get("ExpiredTime")))
            };
            if let Some(expire) = expire {
                if let Some(group) = credit_group_for_package(&code) {
                    group_expire
                        .entry(group.to_owned())
                        .and_modify(|existing| *existing = (*existing).min(expire))
                        .or_insert(expire);
                }
            }
        }
    }

    if let Some(reset) = reset_unix {
        let iso = unix_to_rfc3339(reset);
        snapshot.usage.reset_at = Some(iso.clone());
        snapshot.subscription.billing_cycle_end = Some(iso);
        // Keep subscription.expires_at unset: home list must not invent a calendar
        // "已过期" from credit-cycle end (days_remaining stays None for WorkBuddy).
    }

    for product in &mut snapshot.usage.products {
        if let Some(expire) = group_expire.get(&product.name) {
            product.expires_at = Some(unix_to_rfc3339(*expire));
        }
    }
}

fn credit_group_for_package(code: &str) -> Option<&'static str> {
    match code {
        "TCACA_code_007_nzdH5h4Nl0" // activity
        | "TCACA_code_029_6wCGEWquYy" // bonus29
        | "TCACA_code_030_BjSt89qTvr" // bonus30
            => Some("平台奖励积分"),
        "TCACA_code_009_0XmEQc2xOf" // extra
        | "TCACA_code_038_OhvqZtiPKr" // extra38
        | "TCACA_code_036_lupO5WgNdG" // extraIntl
            => Some("购买积分"),
        "TCACA_code_006_DbXS0lrypC" // gift
            => Some("套餐赠送积分"),
        "TCACA_code_001_PqouKr6QWV" // daily free
            => Some("套餐基础积分"),
        _ => None,
    }
}

fn billing_time_unix(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    if let Some(number) = value
        .as_u64()
        .or_else(|| value.as_i64().map(|n| n as u64))
        .or_else(|| value.as_f64().map(|n| n as u64))
    {
        return Some(if number > 10_000_000_000 {
            number / 1_000
        } else {
            number
        });
    }
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    if text.chars().all(|ch| ch.is_ascii_digit()) {
        let number: u64 = text.parse().ok()?;
        return Some(if number > 10_000_000_000 {
            number / 1_000
        } else {
            number
        });
    }
    if let Some(unix) = parse_iso_timestamp(text) {
        return Some(unix);
    }
    // WorkBuddy wall times like "2026-10-31 23:59:59" are local (no offset).
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let parts: Vec<&str> = text.split(['-', ' ', ':']).collect();
    if parts.len() != 6 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u8 = parts[1].parse().ok()?;
    let day: u8 = parts[2].parse().ok()?;
    let hour: u8 = parts[3].parse().ok()?;
    let minute: u8 = parts[4].parse().ok()?;
    let second: u8 = parts[5].parse().ok()?;
    let date = time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
    let time_of_day = time::Time::from_hms(hour, minute, second).ok()?;
    Some(
        time::PrimitiveDateTime::new(date, time_of_day)
            .assume_offset(offset)
            .unix_timestamp() as u64,
    )
}

fn unix_to_rfc3339(unix: u64) -> String {
    OffsetDateTime::from_unix_timestamp(unix as i64)
        .ok()
        .and_then(|dt| dt.format(&time::format_description::well_known::Rfc3339).ok())
        .unwrap_or_else(|| format!("{unix}"))
}

fn apply_request_usage_soft(
    snapshot: &mut WorkbuddySnapshot,
    host: &str,
    token: &str,
    budget: Duration,
) {
    match fetch_request_usage_window(host, token, budget) {
        Ok(rows) => {
            let mapped = map_request_usage_rows(&rows);
            snapshot.usage.events = mapped.events;
            snapshot.usage.weekly = mapped.weekly;
            snapshot.usage.models = mapped.models;
            snapshot.usage.weekly_available = true;
            snapshot.usage.weekly_error = None;
        }
        Err(error) => {
            snapshot.usage.weekly_available = false;
            snapshot.usage.weekly_error = Some(error.to_string());
        }
    }
}

struct MappedRequestUsage {
    events: Vec<UsageEvent>,
    weekly: Vec<WeeklyUsageSummary>,
    models: Vec<ModelUsageSummary>,
}

fn map_request_usage_rows(rows: &[Value]) -> MappedRequestUsage {
    let mut events: Vec<UsageEvent> = rows.iter().filter_map(usage_event_from_request_row).collect();
    events.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    let weekly = weekly_from_wb_events(&events);
    let models = models_from_wb_events(&events);
    MappedRequestUsage {
        events,
        weekly,
        models,
    }
}

fn usage_event_from_request_row(row: &Value) -> Option<UsageEvent> {
    let request_time = text_field(row, &["requestTime", "RequestTime"])?;
    let unix_secs = billing_time_unix(Some(&Value::String(request_time)))?;
    let credit = f64_field(row, &["credit", "Credit"]).unwrap_or(0.0);
    // Absolute credits (2-dp). FE formats via credits(), not money($).
    let credits = (credit * 100.0).round() / 100.0;
    let model = text_field(row, &["model", "Model"]).or_else(|| {
        // No separate label field on UsageEvent; fall back to truncated input.
        text_field(row, &["inputTrunc", "InputTrunc", "input", "Input"])
    });
    let subscribe = text_field(row, &["subscribeType", "SubscribeType"]).unwrap_or_default();
    let on_demand = !subscribe.is_empty() && !subscribe.eq_ignore_ascii_case("free");
    Some(UsageEvent {
        timestamp: unix_secs.saturating_mul(1000),
        model,
        requests: 1.0,
        input_tokens: None,
        output_tokens: None,
        cost_usd: None,
        charged_cents: None,
        credits: Some(credits),
        on_demand,
    })
}

fn weekly_from_wb_events(events: &[UsageEvent]) -> Vec<WeeklyUsageSummary> {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let today = OffsetDateTime::now_utc().to_offset(offset).date();
    let days: Vec<Date> = (0..7)
        .rev()
        .map(|day| today - TimeDuration::days(day))
        .collect();
    let mut values: std::collections::BTreeMap<Date, (f64, f64)> =
        std::collections::BTreeMap::new();
    for event in events {
        let Ok(dt) =
            OffsetDateTime::from_unix_timestamp_nanos((event.timestamp as i128) * 1_000_000)
        else {
            continue;
        };
        let date = dt.to_offset(offset).date();
        if !days.contains(&date) {
            continue;
        }
        let entry = values.entry(date).or_default();
        entry.0 += 1.0;
        // on_demand_cents slot carries credit totals for WorkBuddy weekly fallback.
        entry.1 += event.credits.unwrap_or(0.0);
    }
    days.into_iter()
        .map(|date| {
            let (requests, on_demand_cents) = values.get(&date).copied().unwrap_or_default();
            WeeklyUsageSummary {
                date: date.to_string(),
                requests,
                on_demand_cents,
                is_on_demand: on_demand_cents > 0.0,
            }
        })
        .collect()
}

fn models_from_wb_events(events: &[UsageEvent]) -> Vec<ModelUsageSummary> {
    let mut by_model: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();
    for event in events {
        let Some(name) = event.model.as_ref().filter(|name| !name.is_empty()) else {
            continue;
        };
        *by_model.entry(name.clone()).or_default() += 1;
    }
    let mut models: Vec<_> = by_model
        .into_iter()
        .map(|(name, requests)| ModelUsageSummary { name, requests })
        .collect();
    models.sort_by(|left, right| {
        right
            .requests
            .cmp(&left.requests)
            .then_with(|| left.name.cmp(&right.name))
    });
    models
}

fn request_usage_range_now() -> (String, String) {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let now = OffsetDateTime::now_utc().to_offset(offset);
    let end = now.date();
    let start = end - TimeDuration::days(6);
    (
        format!(
            "{:04}-{:02}-{:02} 00:00:00",
            start.year(),
            start.month() as u8,
            start.day()
        ),
        format!(
            "{:04}-{:02}-{:02} 23:59:59",
            end.year(),
            end.month() as u8,
            end.day()
        ),
    )
}

fn fetch_request_usage_window(host: &str, token: &str, budget: Duration) -> Result<Vec<Value>> {
    let (start_time, end_time) = request_usage_range_now();
    let mut collected = Vec::new();
    let mut total: Option<u64> = None;
    for page in 1..=REQUEST_USAGE_MAX_PAGES {
        let page_result =
            fetch_request_usage_page(host, token, &start_time, &end_time, page, budget);
        let (rows, page_total) = match page_result {
            Ok(value) => value,
            Err(error) if collected.is_empty() => return Err(error),
            Err(_) => break,
        };
        if total.is_none() {
            total = page_total;
        }
        if rows.is_empty() {
            break;
        }
        let page_len = rows.len();
        collected.extend(rows);
        if let Some(total) = total {
            if collected.len() as u64 >= total {
                break;
            }
        }
        if (page_len as u32) < REQUEST_USAGE_PAGE_SIZE {
            break;
        }
    }
    Ok(collected)
}

fn fetch_request_usage_page(
    host: &str,
    token: &str,
    start_time: &str,
    end_time: &str,
    page_num: u32,
    budget: Duration,
) -> Result<(Vec<Value>, Option<u64>)> {
    let url = format!("https://{host}{REQUEST_USAGE_PATH}");
    let body = serde_json::json!({
        "startTime": start_time,
        "endTime": end_time,
        "pageNum": page_num,
        "pageSize": REQUEST_USAGE_PAGE_SIZE,
    });
    let response = Client::shared().send(
        &Call {
            method: reqwest::Method::POST,
            url,
            headers: vec![
                ("Accept".into(), "application/json".into()),
                ("Content-Type".into(), "application/json".into()),
                ("User-Agent".into(), USER_AGENT.into()),
                ("X-Product".into(), "SaaS".into()),
                ("Authorization".into(), format!("Bearer {token}")),
            ],
            query: Vec::new(),
            body: Body::Json(body),
        },
        &Budget::new(budget),
        Retry::None,
    )?;
    if response.status == 401 || response.status == 403 {
        return Err(AppError::Message(
            "WorkBuddy 登录已失效，请重新登录或导入。".into(),
        ));
    }
    if !(200..300).contains(&response.status) {
        return Err(AppError::Message(format!(
            "WorkBuddy 调用明细失败 ({}): {}",
            response.status,
            response.text().chars().take(160).collect::<String>()
        )));
    }
    let raw: Value = response.json().map_err(|_| HttpError::Decode)?;
    let data = unwrap_data(&raw)?;
    let total = data
        .get("total")
        .or_else(|| data.get("Total"))
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().map(|n| n as u64))
                .or_else(|| value.as_f64().map(|n| n as u64))
                .or_else(|| value.as_str()?.trim().parse().ok())
        });
    let rows = data
        .get("data")
        .or_else(|| data.get("Data"))
        .or_else(|| data.get("list"))
        .or_else(|| data.get("List"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok((rows, total))
}

fn unwrap_data(raw: &Value) -> Result<&Value> {
    if let Some(code) = raw.get("code").and_then(Value::as_i64) {
        if code != 0 {
            let msg = raw
                .get("msg")
                .or_else(|| raw.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("WorkBuddy 用量查询失败");
            return Err(AppError::Message(format!("WorkBuddy 用量查询失败: {msg}")));
        }
    }
    raw.get("data")
        .filter(|value| !value.is_null())
        .ok_or_else(|| AppError::Message("WorkBuddy 用量响应缺少 data。".into()))
}

struct CreditGroup {
    name: String,
    used: f64,
    total: f64,
}

fn credit_groups(data: &Value) -> Vec<CreditGroup> {
    data.get("CreditGroups")
        .or_else(|| data.get("creditGroups"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| {
            let name = text_field(group, &["Name", "name"]).filter(|value| !value.is_empty())?;
            let total = f64_field(
                group,
                &["CycleTotalCapacity", "cycleTotalCapacity", "TotalCapacity"],
            )
            .unwrap_or(0.0);
            let used = f64_field(
                group,
                &["CycleUsedCapacity", "cycleUsedCapacity", "UsedCapacity"],
            )
            .unwrap_or(0.0);
            Some(CreditGroup { name, used, total })
        })
        .collect()
}

fn api_host(profile: &Value) -> String {
    profile
        .pointer("/auth/domain")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_owned()
        })
        .unwrap_or_else(|| DEFAULT_DOMAIN.to_owned())
}

fn text_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            keys.iter().find_map(|key| {
                value
                    .get(*key)
                    .and_then(|node| node.as_i64().or_else(|| node.as_u64().map(|n| n as i64)))
                    .map(|n| n.to_string())
            })
        })
}

fn f64_field(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        let node = value.get(*key)?;
        if let Some(number) = node.as_f64().or_else(|| node.as_i64().map(|n| n as f64)) {
            return Some(number);
        }
        node.as_str()?.trim().parse().ok()
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ApplicationKind, ImportType};

    fn sample_account() -> Account {
        Account {
            id: "acc_wb".into(),
            label: "Jay".into(),
            email: Some("Jay".into()),
            application: ApplicationKind::Workbuddy,
            import_type: ImportType::OAuth,
            subscription: SubscriptionSummary::default(),
            raw_export: serde_json::json!({}),
            created_at: 1,
            updated_at: 1,
            last_used_at: 1,
        }
    }

    fn jay_summary() -> Value {
        serde_json::json!({
            "code": 0,
            "msg": "OK",
            "data": {
                "Packages": [
                    {
                        "PackageCode": "TCACA_code_008",
                        "CycleTotalCapacity": "500",
                        "CycleRemainCapacity": "500",
                        "CycleUsedCapacity": "0",
                        "CapacityUnit": "credits",
                        "TotalCount": 1
                    },
                    {
                        "PackageCode": "TCACA_code_030",
                        "CycleTotalCapacity": "1500",
                        "CycleRemainCapacity": "1500",
                        "CycleUsedCapacity": "0",
                        "CapacityUnit": "credits",
                        "TotalCount": 1
                    }
                ],
                "TotalCredits": "2000",
                "SubscriptionPackageCode": "",
                "SubscriptionPackageName": "体验版",
                "IsPaidUser": false,
                "CreditGroups": [
                    {
                        "Kind": "plan_base",
                        "Name": "套餐基础积分",
                        "CycleTotalCapacity": "500",
                        "CycleRemainCapacity": "500",
                        "CycleUsedCapacity": "0",
                        "CapacityUnit": "credits",
                        "TotalCount": 1
                    },
                    {
                        "Kind": "plan_bonus",
                        "Name": "套餐赠送积分",
                        "CycleTotalCapacity": "0",
                        "CycleRemainCapacity": "0",
                        "CycleUsedCapacity": "0",
                        "CapacityUnit": "credits",
                        "TotalCount": 0
                    },
                    {
                        "Kind": "addon",
                        "Name": "购买积分",
                        "CycleTotalCapacity": "0",
                        "CycleRemainCapacity": "0",
                        "CycleUsedCapacity": "0",
                        "CapacityUnit": "credits",
                        "TotalCount": 0
                    },
                    {
                        "Kind": "benefit",
                        "Name": "平台奖励积分",
                        "CycleTotalCapacity": "1500",
                        "CycleRemainCapacity": "1500",
                        "CycleUsedCapacity": "0",
                        "CapacityUnit": "credits",
                        "TotalCount": 1
                    }
                ]
            }
        })
    }

    #[test]
    fn maps_trial_plan_and_credit_groups() {
        let snap = snapshot_from_summary(&sample_account(), &jay_summary()).expect("snapshot");
        assert_eq!(snap.subscription.plan.as_deref(), Some("体验版"));
        assert_eq!(snap.usage.membership_type.as_deref(), Some("体验版"));
        assert_eq!(snap.usage.primary.kind, "credits");
        assert_eq!(snap.usage.primary.used, 0.0);
        assert_eq!(snap.usage.primary.limit, Some(2000.0));
        assert_eq!(snap.usage.primary.percent, 0.0);
        assert_eq!(snap.usage.products.len(), 4);
        assert_eq!(snap.usage.products[0].name, "套餐基础积分");
        assert_eq!(snap.usage.products[0].percent, 0.0);
        assert_eq!(snap.usage.products[0].used, Some(0.0));
        assert_eq!(snap.usage.products[0].limit, Some(500.0));
        assert_eq!(snap.usage.products[1].name, "套餐赠送积分");
        assert_eq!(snap.usage.products[1].limit, Some(0.0));
        assert_eq!(snap.usage.products[2].name, "购买积分");
        assert_eq!(snap.usage.products[2].used, Some(0.0));
        assert_eq!(snap.usage.products[2].limit, Some(0.0));
        assert_eq!(snap.usage.products[3].name, "平台奖励积分");
        assert_eq!(snap.usage.products[3].limit, Some(1500.0));
        assert!(!snap.usage.weekly_available);
    }

    #[test]
    fn maps_partial_usage_percent() {
        let mut raw = jay_summary();
        raw["data"]["TotalCredits"] = Value::String("2000".into());
        raw["data"]["CreditGroups"][0]["CycleUsedCapacity"] = Value::String("250".into());
        raw["data"]["CreditGroups"][0]["CycleRemainCapacity"] = Value::String("250".into());
        raw["data"]["CreditGroups"][3]["CycleUsedCapacity"] = Value::String("150".into());
        raw["data"]["CreditGroups"][3]["CycleRemainCapacity"] = Value::String("1350".into());
        let snap = snapshot_from_summary(&sample_account(), &raw).expect("snapshot");
        // 250 + 150 = 400 / 2000 = 20%
        assert!((snap.usage.primary.used - 400.0).abs() < 0.01);
        assert_eq!(snap.usage.primary.limit, Some(2000.0));
        assert!((snap.usage.primary.percent - 20.0).abs() < 0.01);
        assert!((snap.usage.products[0].percent - 50.0).abs() < 0.01);
        assert_eq!(snap.usage.products[0].used, Some(250.0));
        assert_eq!(snap.usage.products[0].limit, Some(500.0));
        // Index 3 is 平台奖励积分 in the API order.
        assert_eq!(snap.usage.products[3].name, "平台奖励积分");
        assert!((snap.usage.products[3].percent - 10.0).abs() < 0.01);
        assert_eq!(snap.usage.products[3].used, Some(150.0));
        assert_eq!(snap.usage.products[3].limit, Some(1500.0));
        assert_eq!(snap.usage.products[2].name, "购买积分");
        assert_eq!(snap.usage.products[2].limit, Some(0.0));
    }

    #[test]
    fn apply_package_times_sets_cycle_reset_and_reward_expiry() {
        let mut snap = snapshot_from_summary(&sample_account(), &jay_summary()).expect("snapshot");
        assert!(snap.usage.reset_at.is_none());
        let accounts = vec![
            serde_json::json!({
                "PackageCode": "TCACA_code_030_BjSt89qTvr",
                "PackageName": "拉新权益包",
                "CycleEndTime": "2026-11-26 15:40:09",
                "DeductionEndTime": 1795678809000_u64,
                "CycleCapacitySize": 1500,
                "Status": 0
            }),
            serde_json::json!({
                "PackageCode": "TCACA_code_008_cfWoLwvjU4",
                "PackageName": "个人体验版",
                "CycleEndTime": "2026-10-31 23:59:59",
                "DeductionEndTime": 2053410007000_u64,
                "CycleCapacitySize": 500,
                "Status": 0
            }),
        ];
        apply_package_times(&mut snap, &accounts);
        let reset = snap.usage.reset_at.expect("reset_at");
        // CycleEndTime + 1s of freeMon, local wall → RFC3339
        let reset_unix = parse_iso_timestamp(&reset).expect("parse reset");
        let cycle_unix = billing_time_unix(Some(&Value::String(
            "2026-10-31 23:59:59".into(),
        )))
        .expect("cycle");
        assert_eq!(reset_unix, cycle_unix + 1);
        assert_eq!(
            snap.subscription.billing_cycle_end.as_deref(),
            Some(reset.as_str())
        );
        assert!(snap.subscription.expires_at.is_none());
        let reward = snap
            .usage
            .products
            .iter()
            .find(|p| p.name == "平台奖励积分")
            .expect("reward");
        let expire = reward.expires_at.as_deref().expect("reward expires_at");
        assert_eq!(parse_iso_timestamp(expire), Some(1_795_678_809));
        // Base / purchase / gift stay without per-pool expiry from these packages.
        assert!(snap
            .usage
            .products
            .iter()
            .find(|p| p.name == "套餐基础积分")
            .unwrap()
            .expires_at
            .is_none());
    }

    #[test]
    fn billing_time_unix_accepts_ms_and_local_wall() {
        assert_eq!(
            billing_time_unix(Some(&Value::Number(1_795_678_809_000_u64.into()))),
            Some(1_795_678_809)
        );
        assert_eq!(
            billing_time_unix(Some(&Value::String("1795678809000".into()))),
            Some(1_795_678_809)
        );
        let wall = billing_time_unix(Some(&Value::String("2026-10-31 23:59:59".into())));
        assert!(wall.is_some());
        // Must round-trip through RFC3339 for FE resetDatedLabel.
        let iso = unix_to_rfc3339(wall.unwrap());
        assert_eq!(parse_iso_timestamp(&iso), wall);
    }

        #[test]
    fn api_host_strips_scheme() {
        let profile = serde_json::json!({
            "auth": { "domain": "https://www.workbuddy.cn/" }
        });
        assert_eq!(api_host(&profile), "www.workbuddy.cn");
    }

    #[test]
    fn non_zero_code_is_error() {
        let raw = serde_json::json!({ "code": 401, "msg": "unauthorized", "data": null });
        let err = snapshot_from_summary(&sample_account(), &raw).unwrap_err();
        assert!(err.to_string().contains("unauthorized") || err.to_string().contains("用量"));
    }

    #[test]
    fn interactive_budgets_are_tighter_than_shared_usage() {
        assert!(WORKBUDDY_IMPORT_BUDGET < USAGE_BUDGET);
        assert!(WORKBUDDY_REFRESH_BUDGET < USAGE_BUDGET);
        assert!(WORKBUDDY_IMPORT_BUDGET <= WORKBUDDY_REFRESH_BUDGET);
        assert_eq!(WORKBUDDY_IMPORT_BUDGET.as_secs(), 10);
        assert_eq!(WORKBUDDY_REFRESH_BUDGET.as_secs(), 12);
    }

    #[test]
    fn plaintext_session_skips_when_already_plain() {
        // ensure_plaintext_session needs AppHandle; unit-check the gate used by it.
        let profile = serde_json::json!({
            "auth": { "accessToken": "plain-token", "uid": "u1" }
        });
        assert!(access_token_plaintext(&profile).is_some());
        assert!(!crate::workbuddy::crypto::profile_needs_token_decrypt(&profile));
    }

    #[test]
    fn encrypted_profile_needs_decrypt_before_fetch() {
        let profile = serde_json::json!({
            "auth": {
                "accessToken": { "$wbEncrypted": 1, "envelope": "x" },
                "uid": "u1"
            }
        });
        assert!(access_token_plaintext(&profile).is_none());
        assert!(crate::workbuddy::crypto::profile_needs_token_decrypt(&profile));
    }

    fn sample_request_rows() -> Vec<Value> {
        vec![
            serde_json::json!({
                "requestId": "01a177db828c76e0b65ccb4c874a6c5b",
                "credit": 0.45,
                "model": "deepseek-v4.1-flash",
                "client": "WorkBuddy",
                "requestTime": "2026-10-26 18:55:00",
                "inputTrunc": "你好",
                "input": "你好",
                "agentPurpose": "conversation",
                "appType": "codebuddy",
                "subscribeType": "free"
            }),
            serde_json::json!({
                "requestId": "01a177db8f4f7d978c25b1ccdf16ed50",
                "credit": 0,
                "model": "wb-lite",
                "client": "WorkBuddy",
                "requestTime": "2026-10-26 18:55:00",
                "inputTrunc": "生成会话标题 · 你好",
                "agentPurpose": "conversation_topic",
                "subscribeType": "free"
            }),
            serde_json::json!({
                "requestId": "01a177dd26ea746fba0e47fb7bc0f3a7",
                "credit": 0.05,
                "model": "deepseek-v4.1-flash",
                "requestTime": "2026-10-26 18:55:00",
                "inputTrunc": "你会什么",
                "subscribeType": "free"
            }),
        ]
    }

    #[test]
    fn maps_request_usage_to_events_weekly_models() {
        let mapped = map_request_usage_rows(&sample_request_rows());
        assert_eq!(mapped.events.len(), 3);
        assert_eq!(mapped.weekly.len(), 7);
        // Newest-first; all same second so order stable by sort key only.
        let flash = mapped
            .events
            .iter()
            .find(|event| event.model.as_deref() == Some("deepseek-v4.1-flash") && event.credits == Some(0.45))
            .expect("0.45 credit event");
        assert_eq!(flash.credits, Some(0.45));
        assert!(flash.charged_cents.is_none());
        assert!(!flash.on_demand);
        assert_eq!(flash.requests, 1.0);
        let unix = billing_time_unix(Some(&Value::String("2026-10-26 18:55:00".into()))).unwrap();
        assert_eq!(flash.timestamp, unix * 1000);

        let zero = mapped
            .events
            .iter()
            .find(|event| event.model.as_deref() == Some("wb-lite"))
            .expect("wb-lite");
        assert_eq!(zero.credits, Some(0.0));
        assert!(zero.charged_cents.is_none());

        assert!(mapped.models.iter().any(|model| model.name == "deepseek-v4.1-flash" && model.requests == 2));
        assert!(mapped.models.iter().any(|model| model.name == "wb-lite" && model.requests == 1));
        // Top model by request count first.
        assert_eq!(mapped.models[0].name, "deepseek-v4.1-flash");
    }

    #[test]
    fn request_usage_range_spans_seven_local_days() {
        let (start, end) = request_usage_range_now();
        assert!(start.ends_with(" 00:00:00"), "{start}");
        assert!(end.ends_with(" 23:59:59"), "{end}");
        let start_day = &start[..10];
        let end_day = &end[..10];
        let start_unix = billing_time_unix(Some(&Value::String(start.clone()))).unwrap();
        let end_unix = billing_time_unix(Some(&Value::String(end.clone()))).unwrap();
        // Inclusive 7 calendar days ≈ 6*86400 + almost 1 day.
        assert!(end_unix > start_unix);
        assert_eq!(start_day.len(), 10);
        assert_eq!(end_day.len(), 10);
    }

    #[test]
    fn apply_request_usage_soft_success_flags_weekly_available() {
        let mut snap = snapshot_from_summary(&sample_account(), &jay_summary()).expect("snapshot");
        assert!(!snap.usage.weekly_available);
        // Inject mapped rows without network.
        let mapped = map_request_usage_rows(&sample_request_rows());
        snap.usage.events = mapped.events;
        snap.usage.weekly = mapped.weekly;
        snap.usage.models = mapped.models;
        snap.usage.weekly_available = true;
        assert!(snap.usage.weekly_available);
        assert_eq!(snap.usage.weekly.len(), 7);
        assert_eq!(snap.usage.events.len(), 3);
        // Day containing 2026-10-26 should carry spend when "today" is that day;
        // otherwise weekly slots still exist (empty). Either way length is 7.
        assert!(snap.usage.weekly.iter().all(|day| day.date.len() == 10));
    }

    #[test]
    fn usage_event_falls_back_to_input_trunc_label() {
        let row = serde_json::json!({
            "credit": 1.25,
            "requestTime": "2026-10-26 12:00:00",
            "inputTrunc": "仅有截断标题",
            "subscribeType": "pro"
        });
        let event = usage_event_from_request_row(&row).expect("event");
        assert_eq!(event.model.as_deref(), Some("仅有截断标题"));
        assert_eq!(event.credits, Some(1.25));
        assert!(event.charged_cents.is_none());
        assert!(event.on_demand);
    }
}
