use std::time::Duration;

use crate::cursor::session::{jwt_claims, session_user_id};
use crate::error::{AppError, Result};
use crate::models::{now, parse_timestamp, Session, SubscriptionSummary, ACCESS_TOKEN_KEY};

const CURSOR_SUBSCRIPTION_URL: &str = "https://api2.cursor.sh/auth/full_stripe_profile";
const CURSOR_DASHBOARD_URL: &str = "https://cursor.com/api";

fn plan_from_response(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => ["membershipType", "stripeMembershipType"]
            .iter()
            .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| object.values().find_map(plan_from_response)),
        serde_json::Value::Array(items) => items.iter().find_map(plan_from_response),
        _ => None,
    }
}

pub(crate) fn subscription_from_response(value: &serde_json::Value) -> SubscriptionSummary {
    let billing_cycle_end = value
        .get("billingCycleEnd")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    SubscriptionSummary {
        plan: plan_from_response(value),
        expires_at: billing_cycle_end
            .as_deref()
            .and_then(crate::models::parse_iso_timestamp)
            .or_else(|| value.get("billingCycleEnd").and_then(parse_timestamp)),
        billing_cycle_end,
        checked_at: Some(now()),
    }
}

pub(crate) fn merge_subscription(
    primary: Option<SubscriptionSummary>,
    fallback: Option<SubscriptionSummary>,
) -> Option<SubscriptionSummary> {
    match (primary, fallback) {
        (None, None) => None,
        (Some(primary), None) => Some(primary),
        (None, Some(fallback)) => Some(fallback),
        (Some(primary), Some(fallback)) => Some(SubscriptionSummary {
            plan: primary.plan.or(fallback.plan),
            expires_at: primary.expires_at,
            billing_cycle_end: primary.billing_cycle_end,
            checked_at: primary.checked_at.or(fallback.checked_at),
        }),
    }
}

pub(crate) fn fetch_stripe_profile(session: &Session) -> Result<serde_json::Value> {
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

pub(crate) fn fetch_cursor_subscription(session: &Session) -> Result<SubscriptionSummary> {
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

pub(crate) fn dashboard_cookie(session: &Session) -> Result<String> {
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

pub(crate) fn dashboard_request(
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::cursor::session::session_from_access_token;
    use crate::models::{subscription_from_session, Session, MEMBERSHIP_TYPE_KEY};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use std::collections::BTreeMap;
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

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
        assert_eq!(summary.expires_at, None);
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
            usage_summary.billing_cycle_end.as_deref(),
            Some("2026-08-27T00:00:00.000Z")
        );
        assert_eq!(
            usage_summary.expires_at,
            Some(
                OffsetDateTime::parse("2026-08-27T00:00:00.000Z", &Rfc3339)
                    .unwrap()
                    .unix_timestamp() as u64
            )
        );
        let merged = merge_subscription(Some(usage_summary), Some(summary.clone()));
        assert_eq!(
            merged.unwrap().expires_at,
            Some(
                OffsetDateTime::parse("2026-08-27T00:00:00.000Z", &Rfc3339)
                    .unwrap()
                    .unix_timestamp() as u64
            )
        );
        let stripe_only_date = subscription_from_response(&serde_json::json!({
            "membershipType": "pro",
            "nested": { "billingCycleEnd": "2026-12-01T00:00:00.000Z" }
        }));
        assert_eq!(stripe_only_date.expires_at, None);
        let merged_without_usage_date = merge_subscription(
            Some(subscription_from_response(&serde_json::json!({ "membershipType": "pro" }))),
            Some(summary),
        );
        assert_eq!(merged_without_usage_date.unwrap().expires_at, None);
        let fractional = subscription_from_response(&serde_json::json!({
            "membershipType": "pro",
            "billingCycleEnd": "2026-08-15T03:23:58.561Z"
        }));
        assert_eq!(
            fractional.expires_at,
            Some(
                OffsetDateTime::parse("2026-08-15T03:23:58.561Z", &Rfc3339)
                    .unwrap()
                    .unix_timestamp() as u64
            )
        );
    }

    #[test]
    fn missing_subscription_response_values_do_not_fabricate_an_expiry() {
        let summary = subscription_from_response(&serde_json::json!({ "status": "active" }));
        assert_eq!(summary.plan, None);
        assert_eq!(summary.expires_at, None);
    }

}
