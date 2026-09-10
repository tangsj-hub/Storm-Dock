use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(crate) const CURSOR_KEYS: [&str; 7] = [
    "cursorAuth/accessToken",
    "cursorAuth/refreshToken",
    "cursorAuth/cachedEmail",
    "cursorAuth/cachedScopedProfile",
    "cursorAuth/cachedSignUpType",
    "cursorAuth/stripeMembershipType",
    "glass.lastSignedInAuthId",
];
pub(crate) const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
pub(crate) const REFRESH_TOKEN_KEY: &str = "cursorAuth/refreshToken";
pub(crate) const EMAIL_KEY: &str = "cursorAuth/cachedEmail";
pub(crate) const AUTH_ID_KEY: &str = "glass.lastSignedInAuthId";
pub(crate) const MEMBERSHIP_TYPE_KEY: &str = "cursorAuth/stripeMembershipType";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ApplicationKind {
    Cursor,
    Codex,
    Grok,
}

impl ApplicationKind {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor",
            Self::Codex => "Codex",
            Self::Grok => "Grok Build",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApplicationStatus {
    pub(crate) kind: ApplicationKind,
    pub(crate) label: String,
    pub(crate) available: bool,
    pub(crate) reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Plugin {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) source: String,
    pub(crate) enabled: bool,
    pub(crate) team_required: bool,
    pub(crate) capabilities: Vec<PluginCapability>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCapability {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) enabled: bool,
    pub(crate) description: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpServer {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Account {
    pub(crate) id: String,
    pub(crate) application: ApplicationKind,
    pub(crate) label: String,
    pub(crate) email: Option<String>,
    #[serde(default)]
    pub(crate) import_type: ImportType,
    #[serde(default)]
    pub(crate) subscription: SubscriptionSummary,
    #[serde(default)]
    pub(crate) raw_export: serde_json::Value,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) last_used_at: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountSummary {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) email: Option<String>,
    pub(crate) import_type: ImportType,
    pub(crate) subscription: SubscriptionSummary,
    pub(crate) usage: Option<UsageMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) grok_bot_usage: Option<UsageMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) grok_bot_reset_at: Option<String>,
    pub(crate) days_remaining: Option<i64>,
    pub(crate) is_current: bool,
    #[serde(default)]
    pub(crate) is_grok_bot_current: bool,
    pub(crate) status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubscriptionSummary {
    pub(crate) plan: Option<String>,
    pub(crate) expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) billing_cycle_end: Option<String>,
    pub(crate) checked_at: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CursorUsageDetails {
    pub(crate) account_id: String,
    pub(crate) label: String,
    pub(crate) email: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) membership_type: Option<String>,
    pub(crate) primary: UsageMetric,
    pub(crate) reset_at: Option<String>,
    pub(crate) on_demand: Option<UsageMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) grok_bot: Option<UsageMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) grok_bot_reset_at: Option<String>,
    pub(crate) models: Vec<ModelUsageSummary>,
    pub(crate) weekly: Vec<WeeklyUsageSummary>,
    pub(crate) weekly_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) weekly_error: Option<String>,
    #[serde(default)]
    pub(crate) events: Vec<UsageEvent>,
    pub(crate) checked_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageEvent {
    pub(crate) timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    pub(crate) requests: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) input_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) output_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) charged_cents: Option<f64>,
    pub(crate) on_demand: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageMetric {
    pub(crate) kind: String,
    pub(crate) used: f64,
    pub(crate) limit: Option<f64>,
    pub(crate) percent: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelUsageSummary {
    pub(crate) name: String,
    pub(crate) requests: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeeklyUsageSummary {
    pub(crate) date: String,
    pub(crate) requests: f64,
    pub(crate) on_demand_cents: f64,
    pub(crate) is_on_demand: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwitchProgress {
    pub(crate) operation_id: String,
    pub(crate) account_id: String,
    pub(crate) stage: &'static str,
    pub(crate) percent: u8,
    pub(crate) status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwitchOutcome {
    pub(crate) restart_required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ImportType {
    OAuth,
    Token,
    Jwt,
    Native,
    #[serde(rename = "api_key")]
    ApiKey,
}

impl Default for ImportType {
    fn default() -> Self {
        Self::Native
    }
}

impl ImportType {
    pub(crate) fn supports_desktop_switch(&self) -> bool {
        matches!(self, Self::OAuth | Self::Native | Self::ApiKey)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Session {
    pub(crate) values: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) raw_export: Option<serde_json::Value>,
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn days_remaining(expires_at: u64, current_time: u64) -> i64 {
    if expires_at <= current_time {
        return -1;
    }
    ((expires_at - current_time) / 86_400) as i64
}

pub(crate) fn parse_iso_timestamp(text: &str) -> Option<u64> {
    OffsetDateTime::parse(text.trim(), &Rfc3339)
        .ok()
        .map(|time| time.unix_timestamp() as u64)
}

pub(crate) fn parse_timestamp(value: &serde_json::Value) -> Option<u64> {
    if let Some(number) = value
        .as_u64()
        .or_else(|| value.as_f64().map(|number| number as u64))
    {
        return Some(if number > 10_000_000_000 {
            number / 1_000
        } else {
            number
        });
    }
    parse_iso_timestamp(value.as_str()?)
}

impl SubscriptionSummary {
    pub(crate) fn merge_from(&mut self, summary: SubscriptionSummary) {
        self.plan = summary.plan.or_else(|| self.plan.take());
        self.expires_at = summary.expires_at.or(self.expires_at);
        self.billing_cycle_end = summary
            .billing_cycle_end
            .or_else(|| self.billing_cycle_end.take());
        self.checked_at = summary.checked_at.or(self.checked_at);
        if self.expires_at.is_none() {
            self.expires_at = self
                .billing_cycle_end
                .as_deref()
                .and_then(parse_iso_timestamp);
        }
    }

    pub(crate) fn reset_timestamp(&self, raw_export: &serde_json::Value) -> Option<u64> {
        self.expires_at
            .or_else(|| {
                self.billing_cycle_end
                    .as_deref()
                    .and_then(parse_iso_timestamp)
            })
            .or_else(|| parse_timestamp(raw_export.get("billingCycleEnd")?))
            .or_else(|| parse_timestamp(raw_export.pointer("/cursor_usage_raw/billingCycleEnd")?))
    }
}

pub(crate) fn import_type(session: &Session) -> ImportType {
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

pub(crate) fn subscription_from_session(session: &Session) -> SubscriptionSummary {
    SubscriptionSummary {
        plan: session
            .values
            .get(MEMBERSHIP_TYPE_KEY)
            .cloned()
            .filter(|value| !value.is_empty()),
        ..Default::default()
    }
}

pub(crate) fn matching_account_index(
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

pub(crate) fn json_text(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_days_floor_elapsed_seconds() {
        assert_eq!(days_remaining(6 * 86_400 + 1, 1 * 86_400 + 86_399), 4);
        assert_eq!(days_remaining(1 * 86_400 + 86_399, 1 * 86_400), 0);
        assert_eq!(days_remaining(100 + 86_400, 100), 1);
        assert_eq!(days_remaining(100, 100), -1);
        assert_eq!(days_remaining(99, 100), -1);
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
}
