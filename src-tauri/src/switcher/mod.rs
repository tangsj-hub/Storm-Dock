//! Generic account switcher: providers expose accounts / quota / expiry / switch,
//! this module owns trigger rules, candidate ranking, and cooldown (pure logic),
//! while `runtime` drives the background check and the popup window.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub(crate) mod grok_bot;
pub(crate) mod runtime;

/// One account as the shared layer sees it. Units: percent 0..=100, unix seconds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountStatus {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) current: bool,
    /// Remaining quota percent; `None` when unknown (never a candidate).
    pub(crate) remaining_percent: Option<f64>,
    /// Subscription / login expiry; `None` when unknown (treated as not expiring).
    pub(crate) expires_at: Option<u64>,
}

/// Grok-Bot-only (or other provider-specific) follow-up after a successful switch.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum PostSwitchStep {
    /// Hook for copying assistants via saved Bot模版 links (not built yet).
    GrokBotAssistantMigration {
        from_account_id: String,
        to_account_id: String,
    },
}

pub(crate) trait AccountProvider: Send + Sync {
    /// Stable id, also the settings key (e.g. "grokBot").
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    /// Cheap: cached quota / expiry from the local store, no network.
    fn list_accounts(&self, app: &tauri::AppHandle) -> Vec<AccountStatus>;
    /// Network refresh of quota / expiry for the given accounts (persisted by the provider).
    fn refresh(&self, app: &tauri::AppHandle, account_ids: &[String]);
    /// Make `account_id` the active account (may quit and relaunch the app).
    fn switch_to(&self, app: &tauri::AppHandle, account_id: &str) -> Result<(), String>;
    /// Whether switching now needs a native confirm first (e.g. the app is running
    /// and will be quit). Default: no confirm.
    fn needs_confirm(&self) -> bool {
        false
    }
    /// Optional provider-specific step after switching. Default: none.
    fn post_switch_step(&self, _from: &str, _to: &str) -> Option<PostSwitchStep> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ProviderSettings {
    pub(crate) enabled: bool,
    /// Trigger when remaining quota is strictly below this percent.
    pub(crate) min_remaining_percent: f64,
    /// Trigger when expiry is within this many hours.
    pub(crate) expiry_window_hours: u64,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            min_remaining_percent: 10.0,
            expiry_window_hours: 24,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SwitcherSettings {
    pub(crate) providers: HashMap<String, ProviderSettings>,
}

impl SwitcherSettings {
    /// Per-app settings file; a legacy flat `{enabled, minRemainingPercent, ...}`
    /// object is migrated into the Grok Bot group.
    pub(crate) fn parse(raw: &str) -> Self {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
            return Self::default();
        };
        if value.get("providers").is_some() {
            return serde_json::from_value(value).unwrap_or_default();
        }
        let legacy = ["enabled", "minRemainingPercent", "expiryWindowHours"]
            .iter()
            .any(|key| value.get(*key).is_some());
        if !legacy {
            return Self::default();
        }
        let mut settings = Self::default();
        if let Ok(provider) = serde_json::from_value::<ProviderSettings>(value) {
            settings
                .providers
                .insert(grok_bot::PROVIDER_ID.to_string(), provider);
        }
        settings
    }

    pub(crate) fn for_provider(&self, id: &str) -> ProviderSettings {
        self.providers.get(id).copied().unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TriggerReason {
    LowQuota,
    Expiring,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SwitchOffer {
    pub(crate) provider: String,
    pub(crate) provider_name: String,
    pub(crate) reason: TriggerReason,
    /// 0 = just crossed, higher = worse; used by cooldown.
    pub(crate) severity: u8,
    pub(crate) current: AccountStatus,
    pub(crate) candidate: AccountStatus,
    /// Settings-page preview: never switches, never touches cooldown.
    #[serde(default)]
    pub(crate) preview: bool,
}

fn expiring(account: &AccountStatus, settings: &ProviderSettings, now: u64) -> bool {
    account
        .expires_at
        .is_some_and(|at| at <= now + settings.expiry_window_hours * 3600)
}

/// Why (and how badly) the current account needs a switch.
pub(crate) fn trigger(
    current: &AccountStatus,
    settings: &ProviderSettings,
    now: u64,
) -> Option<(TriggerReason, u8)> {
    if let Some(at) = current.expires_at {
        if expiring(current, settings, now) {
            let left = at.saturating_sub(now);
            let severity = if left == 0 {
                3
            } else if left <= 3600 {
                2
            } else if left <= 6 * 3600 {
                1
            } else {
                0
            };
            return Some((TriggerReason::Expiring, severity));
        }
    }
    let remaining = current.remaining_percent?;
    if remaining < settings.min_remaining_percent {
        let severity = if remaining <= 0.0 {
            2
        } else if remaining <= settings.min_remaining_percent / 2.0 {
            1
        } else {
            0
        };
        return Some((TriggerReason::LowQuota, severity));
    }
    None
}

/// Best other account: not expiring, known quota above the threshold (and, for low
/// quota, above the current account), most remaining quota first, later expiry as tie-break.
pub(crate) fn best_candidate<'a>(
    accounts: &'a [AccountStatus],
    current: &AccountStatus,
    reason: TriggerReason,
    settings: &ProviderSettings,
    now: u64,
) -> Option<&'a AccountStatus> {
    let floor = match reason {
        TriggerReason::LowQuota => current
            .remaining_percent
            .unwrap_or(0.0)
            .max(settings.min_remaining_percent),
        TriggerReason::Expiring => settings.min_remaining_percent,
    };
    let mut candidates: Vec<&AccountStatus> = accounts
        .iter()
        .filter(|a| !a.current && a.id != current.id)
        .filter(|a| !expiring(a, settings, now))
        .filter(|a| a.remaining_percent.is_some_and(|r| r > floor))
        .collect();
    candidates.sort_by(|a, b| {
        b.remaining_percent
            .partial_cmp(&a.remaining_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.expires_at.unwrap_or(u64::MAX).cmp(&a.expires_at.unwrap_or(u64::MAX)))
    });
    candidates.into_iter().next()
}

pub(crate) fn evaluate(
    provider: &str,
    provider_name: &str,
    accounts: &[AccountStatus],
    settings: &ProviderSettings,
    now: u64,
) -> Option<SwitchOffer> {
    if !settings.enabled {
        return None;
    }
    let current = accounts.iter().find(|a| a.current)?;
    let (reason, severity) = trigger(current, settings, now)?;
    let candidate = best_candidate(accounts, current, reason, settings, now)?;
    Some(SwitchOffer {
        provider: provider.into(),
        provider_name: provider_name.into(),
        reason,
        severity,
        current: current.clone(),
        candidate: candidate.clone(),
        preview: false,
    })
}

/// Sample offer for the settings preview: the real current account and the best
/// real candidate when available, otherwise placeholders.
pub(crate) fn preview_offer(
    provider: &str,
    provider_name: &str,
    accounts: &[AccountStatus],
    settings: &ProviderSettings,
    now: u64,
) -> SwitchOffer {
    let placeholder = |id: &str, label: &str, current: bool, remaining: f64| AccountStatus {
        id: id.into(),
        label: label.into(),
        current,
        remaining_percent: Some(remaining),
        expires_at: None,
    };
    let mut current = accounts
        .iter()
        .find(|a| a.current)
        .cloned()
        .unwrap_or_else(|| placeholder("preview-current", "current@example.com", true, 8.0));
    // Show the low-quota wording with a below-threshold sample value.
    current.remaining_percent = Some((settings.min_remaining_percent - 2.0).max(0.0).round());
    current.expires_at = None;
    let candidate = accounts
        .iter()
        .filter(|a| !a.current && a.id != current.id && !expiring(a, settings, now))
        .filter(|a| a.remaining_percent.is_some())
        .max_by(|a, b| {
            a.remaining_percent
                .partial_cmp(&b.remaining_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
        .unwrap_or_else(|| placeholder("preview-candidate", "other@example.com", false, 70.0));
    SwitchOffer {
        provider: provider.into(),
        provider_name: provider_name.into(),
        reason: TriggerReason::LowQuota,
        severity: 0,
        current,
        candidate,
        preview: true,
    }
}

pub(crate) const DEFAULT_COOLDOWN_SECS: u64 = 3 * 3600;

/// Same provider + account + reason is shown once per cooldown unless it got worse.
#[derive(Default)]
pub(crate) struct Cooldown {
    shown: HashMap<(String, String, TriggerReason), (u64, u8)>,
}

impl Cooldown {
    pub(crate) fn allows(&self, offer: &SwitchOffer, now: u64, cooldown_secs: u64) -> bool {
        let key = (offer.provider.clone(), offer.current.id.clone(), offer.reason);
        match self.shown.get(&key) {
            None => true,
            Some(&(at, severity)) => {
                now.saturating_sub(at) >= cooldown_secs || offer.severity > severity
            }
        }
    }

    pub(crate) fn record(&mut self, offer: &SwitchOffer, now: u64) {
        let key = (offer.provider.clone(), offer.current.id.clone(), offer.reason);
        self.shown.insert(key, (now, offer.severity));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn acct(id: &str, current: bool, remaining: Option<f64>, expires_at: Option<u64>) -> AccountStatus {
        AccountStatus {
            id: id.into(),
            label: id.into(),
            current,
            remaining_percent: remaining,
            expires_at,
        }
    }

    fn eval(accounts: &[AccountStatus]) -> Option<SwitchOffer> {
        evaluate("grokBot", "Grok Bot", accounts, &ProviderSettings::default(), NOW)
    }

    #[test]
    fn healthy_current_account_never_triggers() {
        let accounts = [acct("a", true, Some(50.0), None), acct("b", false, Some(90.0), None)];
        assert!(eval(&accounts).is_none());
    }

    #[test]
    fn low_quota_offers_best_candidate() {
        let accounts = [
            acct("a", true, Some(8.0), None),
            acct("b", false, Some(40.0), None),
            acct("c", false, Some(70.0), None),
            acct("d", false, None, None),
        ];
        let offer = eval(&accounts).unwrap();
        assert_eq!(offer.reason, TriggerReason::LowQuota);
        assert_eq!(offer.candidate.id, "c");
        assert_eq!(offer.severity, 0);
    }

    #[test]
    fn no_better_candidate_means_no_offer() {
        let accounts = [
            acct("a", true, Some(5.0), None),
            acct("b", false, Some(9.0), None),
            acct("c", false, None, None),
        ];
        assert!(eval(&accounts).is_none());
        assert!(eval(&[acct("a", true, Some(1.0), None)]).is_none());
    }

    #[test]
    fn expiring_or_expired_candidates_are_skipped() {
        let accounts = [
            acct("a", true, Some(5.0), None),
            acct("b", false, Some(99.0), Some(NOW + 3600)),
            acct("c", false, Some(95.0), Some(NOW - 10)),
            acct("d", false, Some(30.0), Some(NOW + 48 * 3600)),
        ];
        assert_eq!(eval(&accounts).unwrap().candidate.id, "d");
    }

    #[test]
    fn expiry_within_window_triggers_even_with_quota() {
        let accounts = [
            acct("a", true, Some(80.0), Some(NOW + 2 * 3600)),
            acct("b", false, Some(60.0), None),
        ];
        let offer = eval(&accounts).unwrap();
        assert_eq!(offer.reason, TriggerReason::Expiring);
        assert_eq!(offer.severity, 1);
        assert_eq!(offer.candidate.id, "b");
    }

    #[test]
    fn ranking_tie_breaks_on_later_expiry() {
        let accounts = [
            acct("a", true, Some(5.0), None),
            acct("b", false, Some(70.0), Some(NOW + 30 * 86400)),
            acct("c", false, Some(70.0), Some(NOW + 60 * 86400)),
        ];
        assert_eq!(eval(&accounts).unwrap().candidate.id, "c");
    }

    #[test]
    fn disabled_or_custom_threshold() {
        let accounts = [acct("a", true, Some(15.0), None), acct("b", false, Some(70.0), None)];
        let mut settings = ProviderSettings::default();
        assert!(evaluate("p", "P", &accounts, &settings, NOW).is_none());
        settings.min_remaining_percent = 20.0;
        assert!(evaluate("p", "P", &accounts, &settings, NOW).is_some());
        settings.enabled = false;
        assert!(evaluate("p", "P", &accounts, &settings, NOW).is_none());
    }

    #[test]
    fn severity_grows_as_quota_drops() {
        let s = ProviderSettings::default();
        assert_eq!(trigger(&acct("a", true, Some(9.0), None), &s, NOW), Some((TriggerReason::LowQuota, 0)));
        assert_eq!(trigger(&acct("a", true, Some(4.0), None), &s, NOW), Some((TriggerReason::LowQuota, 1)));
        assert_eq!(trigger(&acct("a", true, Some(0.0), None), &s, NOW), Some((TriggerReason::LowQuota, 2)));
        assert_eq!(trigger(&acct("a", true, Some(50.0), Some(NOW)), &s, NOW), Some((TriggerReason::Expiring, 3)));
    }

    #[test]
    fn cooldown_suppresses_repeat_until_expiry_or_worse() {
        let accounts = [acct("a", true, Some(8.0), None), acct("b", false, Some(70.0), None)];
        let offer = eval(&accounts).unwrap();
        let mut cooldown = Cooldown::default();
        assert!(cooldown.allows(&offer, NOW, DEFAULT_COOLDOWN_SECS));
        cooldown.record(&offer, NOW);
        assert!(!cooldown.allows(&offer, NOW + 60, DEFAULT_COOLDOWN_SECS));
        assert!(cooldown.allows(&offer, NOW + DEFAULT_COOLDOWN_SECS, DEFAULT_COOLDOWN_SECS));

        let worse = eval(&[acct("a", true, Some(2.0), None), acct("b", false, Some(70.0), None)]).unwrap();
        assert!(cooldown.allows(&worse, NOW + 60, DEFAULT_COOLDOWN_SECS));

        let other_reason = eval(&[acct("a", true, Some(50.0), Some(NOW + 3 * 3600)), acct("b", false, Some(70.0), None)]).unwrap();
        assert!(cooldown.allows(&other_reason, NOW + 60, DEFAULT_COOLDOWN_SECS));
    }

    #[test]
    fn legacy_flat_settings_migrate_into_grok_bot_group() {
        let parsed = SwitcherSettings::parse(r#"{"enabled":false,"minRemainingPercent":20}"#);
        let p = parsed.for_provider("grokBot");
        assert!(!p.enabled);
        assert_eq!(p.min_remaining_percent, 20.0);
        assert_eq!(p.expiry_window_hours, 24);
        let grouped = SwitcherSettings::parse(r#"{"providers":{"grokBot":{"enabled":false}}}"#);
        assert!(!grouped.for_provider("grokBot").enabled);
        assert_eq!(SwitcherSettings::parse("garbage"), SwitcherSettings::default());
    }

    #[test]
    fn preview_uses_real_accounts_or_placeholders() {
        let s = ProviderSettings::default();
        let accounts = [acct("a", true, Some(60.0), None), acct("b", false, Some(40.0), None), acct("c", false, Some(80.0), None)];
        let offer = preview_offer("grokBot", "Grok Bot", &accounts, &s, NOW);
        assert!(offer.preview);
        assert_eq!(offer.current.id, "a");
        assert_eq!(offer.current.remaining_percent, Some(8.0));
        assert_eq!(offer.candidate.id, "c");
        let empty = preview_offer("grokBot", "Grok Bot", &[], &s, NOW);
        assert_eq!(empty.current.id, "preview-current");
        assert_eq!(empty.candidate.id, "preview-candidate");
        assert!(!evaluate("grokBot", "Grok Bot", &accounts, &s, NOW).is_some_and(|o| o.preview));
    }

    #[test]
    fn settings_fall_back_to_defaults() {
        let parsed: SwitcherSettings =
            serde_json::from_str(r#"{"providers":{"grokBot":{"minRemainingPercent":25}}}"#).unwrap();
        let p = parsed.for_provider("grokBot");
        assert_eq!(p.min_remaining_percent, 25.0);
        assert!(p.enabled);
        assert_eq!(p.expiry_window_hours, 24);
        assert_eq!(parsed.for_provider("cursor"), ProviderSettings::default());
    }
}
