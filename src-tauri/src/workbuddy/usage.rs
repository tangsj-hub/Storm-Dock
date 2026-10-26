use serde_json::Value;
use crate::error::{AppError, Result};
use crate::http::{Body, Budget, Call, Client, HttpError, Retry, USAGE_BUDGET};
use crate::models::{
    now, Account, CursorUsageDetails, ProductUsageShare, Session, SubscriptionSummary, UsageMetric,
};
use crate::store::AppState;
use crate::tray::refresh_tray;
use crate::workbuddy::session::{access_token_plaintext, profile_value, DEFAULT_DOMAIN};
use tauri::{AppHandle, Emitter, Manager};

const USER_AGENT: &str = "Storm-Dock/workbuddy-usage";
const SUMMARY_PATH: &str = "/billing/meter/get-user-resource-summary";

#[derive(Debug)]
pub(crate) struct WorkbuddySnapshot {
    pub(crate) subscription: SubscriptionSummary,
    pub(crate) usage: CursorUsageDetails,
    pub(crate) billing_raw: Value,
}

pub(crate) fn fetch_workbuddy_snapshot(
    account: &Account,
    session: &Session,
) -> Result<(WorkbuddySnapshot, Option<Session>)> {
    let profile = profile_value(session)?;
    let token = access_token_plaintext(&profile).ok_or(AppError::SecretMissing)?;
    let host = api_host(&profile);
    let summary = fetch_resource_summary(&host, &token)?;
    let snapshot = snapshot_from_summary(account, &summary)?;
    Ok((snapshot, None))
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
    let products = groups
        .iter()
        .filter(|group| group.total > 0.0 || group.used > 0.0)
        .map(|group| ProductUsageShare {
            name: group.name.clone(),
            percent: if group.total > 0.0 {
                (group.used / group.total) * 100.0
            } else {
                0.0
            },
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


/// Run billing refresh inline after local import / OAuth so the returned account
/// already carries plan + credits (avoids racing the home page reload).
pub(crate) fn refresh_imported_account_now(app: &AppHandle, account: Account) -> Account {
    if account.application != crate::models::ApplicationKind::Workbuddy {
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
        let _ = app.emit("accounts-changed", ());
        return account;
    };
    match fetch_workbuddy_snapshot(&account, &session) {
        Ok((snapshot, refreshed)) => {
            if let Ok(mut controller) = app.state::<AppState>().0.lock() {
                if let Some(refreshed) = refreshed {
                    let _ = controller.replace_account_session(&account.id, &refreshed);
                }
                let _ = controller.save_workbuddy_snapshot(&account.id, snapshot);
                if let Ok(updated) = controller.account(&account.id) {
                    refresh_tray(app);
                    let _ = app.emit("accounts-changed", ());
                    return updated;
                }
            }
        }
        Err(error) => {
            // Encrypted desktop auth has no plaintext token — do not mark the
            // account invalid; peers still show the imported identity row.
            let message = error.to_string();
            if !message.contains("凭证缺失") {
                if let Ok(mut controller) = app.state::<AppState>().0.lock() {
                    let _ = controller.mark_token_issue_from_message(&account.id, &message);
                }
            }
        }
    }
    refresh_tray(app);
    let _ = app.emit("accounts-changed", ());
    app.state::<AppState>()
        .0
        .lock()
        .ok()
        .and_then(|controller| controller.account(&account.id).ok())
        .unwrap_or(account)
}

/// Background snapshot after import/OAuth so the list is not stuck on an unknown plan.
pub(crate) fn spawn_imported_refresh(app: AppHandle, account: Account) {
    if account.application != crate::models::ApplicationKind::Workbuddy {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let session = app
            .state::<AppState>()
            .0
            .lock()
            .ok()
            .and_then(|mut controller| controller.subscription_session(&account.id).ok());
        let Some(session) = session else {
            return;
        };
        match fetch_workbuddy_snapshot(&account, &session) {
            Ok((snapshot, refreshed)) => {
                if let Ok(mut controller) = app.state::<AppState>().0.lock() {
                    if let Some(refreshed) = refreshed {
                        let _ = controller.replace_account_session(&account.id, &refreshed);
                    }
                    let _ = controller.save_workbuddy_snapshot(&account.id, snapshot);
                }
            }
            Err(error) => {
                let message = error.to_string();
                // Desktop .info often stores $wbEncrypted tokens; missing plaintext
                // is not the same as an expired login.
                if !message.contains("凭证缺失") {
                    if let Ok(mut controller) = app.state::<AppState>().0.lock() {
                        let _ = controller.mark_token_issue_from_message(&account.id, &message);
                    }
                }
            }
        }
        refresh_tray(&app);
        let _ = app.emit("accounts-changed", ());
    });
}

fn fetch_resource_summary(host: &str, token: &str) -> Result<Value> {
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
        &Budget::new(USAGE_BUDGET),
        Retry::Transient,
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
        assert_eq!(snap.usage.products.len(), 2);
        assert_eq!(snap.usage.products[0].name, "套餐基础积分");
        assert_eq!(snap.usage.products[0].percent, 0.0);
        assert_eq!(snap.usage.products[1].name, "平台奖励积分");
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
        assert!((snap.usage.products[1].percent - 10.0).abs() < 0.01);
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
}
