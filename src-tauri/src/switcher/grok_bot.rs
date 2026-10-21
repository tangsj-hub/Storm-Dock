//! Grok Bot provider: accounts = the Grok Bot list (paid Cursor + paid Grok Build),
//! quota = the separate weekly Grok Bot ("sand") pool, switch = rewrite the
//! Grok Bot login state and relaunch (quits a running Grok Bot first).

use tauri::{AppHandle, Emitter, Manager};

use super::{AccountProvider, AccountStatus, PostSwitchStep};
use crate::models::{AccountSummary, ApplicationKind};
use crate::store::AppState;

pub(crate) struct GrokBotProvider;

pub(crate) const PROVIDER_ID: &str = "grokBot";

/// Only accounts that carry a Cursor / Grok Bot token. Grok Build (xAI login only)
/// accounts are excluded: never candidates and never refreshed by the switcher.
pub(crate) fn switcher_eligible(account: &AccountSummary) -> bool {
    account.application == ApplicationKind::Cursor
}

pub(crate) fn status_from_summary(account: &AccountSummary) -> AccountStatus {
    AccountStatus {
        id: account.id.clone(),
        label: account
            .email
            .clone()
            .filter(|email| !email.is_empty())
            .unwrap_or_else(|| account.label.clone()),
        current: account.is_grok_bot_current,
        // usagePercent is the used share of the weekly pool.
        remaining_percent: account
            .grok_bot_usage
            .as_ref()
            .map(|usage| (100.0 - usage.percent).clamp(0.0, 100.0)),
        expires_at: account.subscription.expires_at,
    }
}

impl AccountProvider for GrokBotProvider {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "Grok Bot"
    }

    fn list_accounts(&self, app: &AppHandle) -> Vec<AccountStatus> {
        let state = app.state::<AppState>();
        let Ok(controller) = state.0.lock() else {
            return Vec::new();
        };
        controller
            .grok_bot_accounts()
            .iter()
            .filter(|account| switcher_eligible(account))
            .map(status_from_summary)
            .collect()
    }

    fn refresh(&self, app: &AppHandle, account_ids: &[String]) {
        let state = app.state::<AppState>();
        let jobs: Vec<_> = {
            let Ok(mut controller) = state.0.lock() else {
                return;
            };
            let accounts = controller.grok_bot_accounts();
            accounts
                .into_iter()
                .filter(switcher_eligible)
                .filter(|account| account_ids.contains(&account.id))
                .filter_map(|account| {
                    controller
                        .subscription_session(&account.id)
                        .ok()
                        .map(|session| (account.id, account.application, session))
                })
                .collect()
        };
        if jobs.is_empty() {
            return;
        }
        // Sequential on the background thread: a handful of accounts every 15 min.
        let mut changed = false;
        for (id, application, session) in jobs {
            let _ = application;
            let result = crate::cursor::usage::fetch_grok_bot_quota_fast(&session);
            let Ok((summary, grok, sand)) = result else {
                continue;
            };
            let Ok(mut controller) = state.0.lock() else {
                return;
            };
            let _ = controller.save_subscription(&id, summary);
            let (metric, reset_at) = match grok {
                Some((metric, reset_at)) => (Some(metric), reset_at),
                None => (None, None),
            };
            let _ = controller.save_grok_bot_usage(&id, metric, reset_at, sand);
            changed = true;
        }
        if changed {
            let _ = app.emit("accounts-changed", ());
        }
    }

    fn switch_to(&self, app: &AppHandle, account_id: &str) -> Result<(), String> {
        let session = {
            let state = app.state::<AppState>();
            let controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
            if !controller
                .grok_bot_accounts()
                .iter()
                .any(|account| account.id == account_id && switcher_eligible(account))
            {
                return Err("该账号不可用于 Grok Bot。".into());
            }
            controller.load_session(account_id).map_err(|e| e.to_string())?
        };
        // Quits a running Grok Bot, writes the new login state, relaunches.
        crate::grok_bot::confirm_for_session(&session).map_err(|e| e.to_string())?;
        let _ = app.emit("accounts-changed", ());
        Ok(())
    }

    fn needs_confirm(&self) -> bool {
        // Match the manual launch button: warn only when Grok Bot will be quit.
        crate::grok_bot::local_status().running
    }

    fn post_switch_step(&self, from: &str, to: &str) -> Option<PostSwitchStep> {
        // Runtime opens auto-import Bot模版 links (run_auto_import) after a real switch.
        // The runtime emits this as `account-switcher-post-switch` to the main window.
        Some(PostSwitchStep::GrokBotAssistantMigration {
            from_account_id: from.into(),
            to_account_id: to.into(),
        })
    }
}

/// Grok-Bot-only post-switch step: once Grok Bot is back up, queue every
/// auto-import Bot模版 (one deep link at a time, each confirmed by the user in
/// Grok Bot). Runs on its own thread so it does not depend on the main window.
/// Only reached after a real switch (preview offers never call `switch_to`).
pub(crate) fn run_auto_import(app: &AppHandle) {
    let items = crate::bot_import_queue::auto_import_items(app);
    if items.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        use std::time::{Duration, Instant};
        // Wait (bounded) for the relaunched Grok Bot, then give it a moment to settle.
        let deadline = Instant::now() + Duration::from_secs(30);
        while !crate::grok_bot::local_status().running && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
        std::thread::sleep(Duration::from_secs(3));
        if let Err(error) = crate::bot_import_queue::start(&app, items) {
            eprintln!("bot template auto-import: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ImportType, SubscriptionSummary, UsageMetric};

    fn summary(application: ApplicationKind) -> AccountSummary {
        AccountSummary {
            id: "x".into(),
            label: "Label".into(),
            email: Some("a@b.c".into()),
            application,
            import_type: ImportType::OAuth,
            subscription: SubscriptionSummary {
                plan: Some("pro".into()),
                expires_at: Some(42),
                billing_cycle_end: None,
                checked_at: None,
            },
            usage: None,
            grok_bot_usage: Some(UsageMetric { kind: "percent".into(), used: 92.0, limit: None, percent: 92.0 }),
            grok_bot_reset_at: None,
            reset_at: None,
            days_remaining: None,
            is_current: false,
            is_grok_bot_current: true,
            status: None,
            base_url: None,
        }
    }

    #[test]
    fn grok_build_accounts_are_excluded() {
        assert!(switcher_eligible(&summary(ApplicationKind::Cursor)));
        assert!(!switcher_eligible(&summary(ApplicationKind::Grok)));
        assert!(!switcher_eligible(&summary(ApplicationKind::Codex)));
    }

    #[test]
    fn maps_used_percent_to_remaining() {
        let status = status_from_summary(&summary(ApplicationKind::Cursor));
        assert!(status.current);
        assert_eq!(status.label, "a@b.c");
        assert!((status.remaining_percent.unwrap() - 8.0).abs() < 1e-9);
        assert_eq!(status.expires_at, Some(42));
    }
}
