//! Background check + popup window for the account switcher.
//!
//! Network budget: the active account is refreshed every `ACTIVE_REFRESH_SECS`,
//! other accounts every `OTHERS_REFRESH_SECS`; evaluation itself only reads the
//! local store.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter, Manager};

use super::{
    evaluate, preview_offer, AccountProvider, Cooldown, ProviderSettings, SwitchOffer, SwitcherSettings,
    DEFAULT_COOLDOWN_SECS,
};
use crate::models::now;

pub(crate) const POPUP_LABEL: &str = "popup-switcher";
/// Safety net if the popup page never answers (its own countdown is 5 s).
const POPUP_FORCE_CLOSE_SECS: u64 = 12;
const STARTUP_DELAY_SECS: u64 = 90;
const TICK_SECS: u64 = 60;
const ACTIVE_REFRESH_SECS: u64 = 5 * 60;
const OTHERS_REFRESH_SECS: u64 = 15 * 60;
const SETTINGS_FILE: &str = "account_switcher.json";

#[derive(Default)]
struct Runtime {
    cooldown: Cooldown,
    pending: Option<SwitchOffer>,
    /// A native confirm dialog is open: the watchdog must not close the popup.
    confirming: bool,
    switching: bool,
    last_active_refresh: HashMap<&'static str, u64>,
    last_others_refresh: HashMap<&'static str, u64>,
}

static RUNTIME: Lazy<Mutex<Runtime>> = Lazy::new(|| Mutex::new(Runtime::default()));
static SETTINGS: Lazy<Mutex<Option<SwitcherSettings>>> = Lazy::new(|| Mutex::new(None));

/// One provider per app; switching only happens within a provider's own accounts.
/// ChatGPT / Codex is intentionally never registered.
fn providers() -> Vec<Arc<dyn AccountProvider>> {
    vec![Arc::new(super::grok_bot::GrokBotProvider)]
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderInfo {
    id: &'static str,
    name: &'static str,
}

#[tauri::command]
pub(crate) fn list_account_switcher_providers() -> Vec<ProviderInfo> {
    providers()
        .iter()
        .map(|p| ProviderInfo { id: p.id(), name: p.display_name() })
        .collect()
}

fn provider(id: &str) -> Option<Arc<dyn AccountProvider>> {
    providers().into_iter().find(|p| p.id() == id)
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join(SETTINGS_FILE))
}

pub(crate) fn settings(app: &AppHandle) -> SwitcherSettings {
    let mut cached = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(settings) = cached.as_ref() {
        return settings.clone();
    }
    let loaded = settings_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|raw| SwitcherSettings::parse(&raw))
        .unwrap_or_default();
    *cached = Some(loaded);
    cached.clone().unwrap_or_default()
}

fn sanitize(mut value: ProviderSettings) -> ProviderSettings {
    value.min_remaining_percent = value.min_remaining_percent.clamp(1.0, 90.0);
    value.expiry_window_hours = value.expiry_window_hours.clamp(1, 168);
    value
}

pub(crate) fn spawn(app: AppHandle) {
    #[cfg(debug_assertions)]
    {
        spawn_dev_preview_trigger(app.clone());
        crate::native_popup::spawn_dev_import_preview_trigger(app.clone());
    }
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(STARTUP_DELAY_SECS));
        loop {
            tick(&app);
            thread::sleep(Duration::from_secs(TICK_SECS));
        }
    });
}

/// Debug builds only: `touch /tmp/storm-dock-preview-switcher` opens the preview popup.
#[cfg(debug_assertions)]
fn spawn_dev_preview_trigger(app: AppHandle) {
    thread::spawn(move || loop {
        let flag = std::path::Path::new("/tmp/storm-dock-preview-switcher");
        if flag.exists() {
            let _ = std::fs::remove_file(flag);
            let _ = preview_switch_offer(app.clone(), super::grok_bot::PROVIDER_ID.into());
        }
        thread::sleep(Duration::from_secs(1));
    });
}

fn tick(app: &AppHandle) {
    // Don't stack a switch offer on top of a running Bot模版 import.
    if crate::bot_import_queue::is_running() {
        return;
    }
    let settings = settings(app);
    for provider in providers() {
        let provider_settings = settings.for_provider(provider.id());
        if !provider_settings.enabled {
            continue;
        }
        refresh_if_due(app, provider.as_ref());
        let accounts = provider.list_accounts(app);
        let Some(offer) = evaluate(
            provider.id(),
            provider.display_name(),
            &accounts,
            &provider_settings,
            now(),
        ) else {
            continue;
        };
        let should_show = {
            let mut runtime = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
            if runtime.pending.is_some()
                || runtime.switching
                || !runtime.cooldown.allows(&offer, now(), DEFAULT_COOLDOWN_SECS)
            {
                false
            } else {
                runtime.cooldown.record(&offer, now());
                runtime.pending = Some(offer);
                true
            }
        };
        if should_show {
            show_popup(app);
            return; // one popup at a time
        }
    }
}

fn refresh_if_due(app: &AppHandle, provider: &dyn AccountProvider) {
    let accounts = provider.list_accounts(app);
    let current_now = now();
    let (active_due, others_due) = {
        let runtime = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
        let last_active = runtime.last_active_refresh.get(provider.id()).copied().unwrap_or(0);
        let last_others = runtime.last_others_refresh.get(provider.id()).copied().unwrap_or(0);
        (
            current_now.saturating_sub(last_active) >= ACTIVE_REFRESH_SECS,
            current_now.saturating_sub(last_others) >= OTHERS_REFRESH_SECS,
        )
    };
    let ids: Vec<String> = accounts
        .iter()
        .filter(|a| if a.current { active_due } else { others_due })
        .map(|a| a.id.clone())
        .collect();
    {
        let mut runtime = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
        if active_due {
            runtime.last_active_refresh.insert(provider.id(), current_now);
        }
        if others_due {
            runtime.last_others_refresh.insert(provider.id(), current_now);
        }
    }
    if !ids.is_empty() {
        provider.refresh(app, &ids);
    }
}

fn show_popup(app: &AppHandle) {
    crate::native_popup::show(app, POPUP_LABEL, "switcher.html");
    watch_popup(app);
}

fn watch_popup(app: &AppHandle) {
    let watchdog = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(POPUP_FORCE_CLOSE_SECS));
        let still_pending = RUNTIME
            .lock()
            .map(|runtime| runtime.pending.is_some() && !runtime.switching && !runtime.confirming)
            .unwrap_or(false);
        if still_pending {
            dismiss(&watchdog);
        }
    });
}

fn close_popup(app: &AppHandle) {
    crate::native_popup::close(app, POPUP_LABEL);
}


fn dismiss(app: &AppHandle) {
    RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).pending = None;
    close_popup(app);
}

#[tauri::command]
pub(crate) fn get_account_switcher_settings(app: AppHandle) -> SwitcherSettings {
    let mut value = settings(&app);
    for provider in providers() {
        value
            .providers
            .entry(provider.id().to_string())
            .or_default();
    }
    value
}

#[tauri::command]
pub(crate) fn set_account_switcher_settings(
    app: AppHandle,
    settings: SwitcherSettings,
) -> Result<SwitcherSettings, String> {
    let mut value = settings;
    let known: Vec<&str> = providers().iter().map(|p| p.id()).collect();
    value.providers.retain(|id, _| known.contains(&id.as_str()));
    for entry in value.providers.values_mut() {
        *entry = sanitize(*entry);
    }
    if let Some(path) = settings_path(&app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let raw = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        std::fs::write(path, raw).map_err(|e| e.to_string())?;
    }
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some(value.clone());
    Ok(value)
}

#[tauri::command]
pub(crate) fn get_switch_offer() -> Option<SwitchOffer> {
    RUNTIME.lock().ok().and_then(|runtime| runtime.pending.clone())
}

/// Labels for the native pre-switch confirm, localized by the popup page
/// (reuses the manual launch warning text).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfirmLabels {
    title: String,
    message: String,
    ok: String,
    cancel: String,
}

/// Native OS ask dialog (tauri-plugin-dialog); blocks the calling worker thread.
fn confirm_natively(app: &AppHandle, labels: &ConfirmLabels) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    app.dialog()
        .message(labels.message.clone())
        .title(labels.title.clone())
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            labels.ok.clone(),
            labels.cancel.clone(),
        ))
        .blocking_show()
}

/// `accept = false` (button 忽略 or the 5 s countdown) keeps the current account.
/// `accept = true` first shows a native confirm (countdown paused); cancel = no switch.
#[tauri::command]
pub(crate) async fn respond_switch_offer(
    app: AppHandle,
    accept: bool,
    confirm: Option<ConfirmLabels>,
) -> Result<(), String> {
    let pending_preview = RUNTIME
        .lock()
        .ok()
        .and_then(|runtime| runtime.pending.as_ref().map(|offer| offer.preview))
        .unwrap_or(false);
    if pending_preview {
        // Preview: show the same native confirm so the flow is visible, then just close.
        if accept {
            if let Some(labels) = confirm {
                {
                    let mut runtime = RUNTIME.lock().map_err(|_| "切换器不可用".to_string())?;
                    if runtime.confirming {
                        return Ok(());
                    }
                    runtime.confirming = true;
                }
                let dialog_app = app.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    confirm_natively(&dialog_app, &labels)
                })
                .await;
                RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).confirming = false;
            }
        }
        dismiss(&app);
        return Ok(());
    }
    let confirm_needed = accept
        && RUNTIME
            .lock()
            .ok()
            .and_then(|runtime| runtime.pending.as_ref().map(|offer| offer.provider.clone()))
            .and_then(|id| provider(&id))
            .is_some_and(|provider| provider.needs_confirm());
    if confirm_needed {
        if let Some(labels) = confirm {
            {
                let mut runtime = RUNTIME.lock().map_err(|_| "切换器不可用".to_string())?;
                if runtime.pending.is_none() || runtime.confirming {
                    return Ok(());
                }
                runtime.confirming = true;
            }
            let dialog_app = app.clone();
            let confirmed = tauri::async_runtime::spawn_blocking(move || {
                confirm_natively(&dialog_app, &labels)
            })
            .await
            .unwrap_or(false);
            RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).confirming = false;
            if !confirmed {
                dismiss(&app);
                return Ok(());
            }
        }
    }
    let offer = {
        let mut runtime = RUNTIME.lock().map_err(|_| "切换器不可用".to_string())?;
        let offer = runtime.pending.take();
        if accept && offer.is_some() {
            runtime.switching = true;
        }
        offer
    };
    close_popup(&app);
    let Some(offer) = offer.filter(|_| accept) else {
        return Ok(());
    };
    let Some(provider) = provider(&offer.provider) else {
        RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).switching = false;
        return Err("未知的切换来源".into());
    };
    let worker_app = app.clone();
    let from = offer.current.id.clone();
    let to = offer.candidate.id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = provider.switch_to(&worker_app, &to);
        let step = result
            .is_ok()
            .then(|| provider.post_switch_step(&from, &to))
            .flatten();
        (result, step)
    })
    .await
    .map_err(|e| e.to_string());
    RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).switching = false;
    let (result, step) = result?;
    match &result {
        Ok(()) => {
            let _ = app.emit("account-switcher-switched", &offer);
            if let Some(step) = step {
                if matches!(step, super::PostSwitchStep::GrokBotAssistantMigration { .. }) {
                    super::grok_bot::run_auto_import(&app);
                }
                let _ = app.emit("account-switcher-post-switch", &step);
            }
        }
        Err(error) => {
            let _ = app.emit("account-switcher-failed", error);
        }
    }
    result
}

/// Settings-page preview of the real popup. Skips cooldown and never switches;
/// refused only while a real reminder or switch is in progress.
#[tauri::command]
pub(crate) fn preview_switch_offer(app: AppHandle, provider_id: String) -> Result<(), String> {
    let provider = provider(&provider_id).ok_or_else(|| "未知的切换来源".to_string())?;
    let accounts = provider.list_accounts(&app);
    let offer = preview_offer(
        provider.id(),
        provider.display_name(),
        &accounts,
        &settings(&app).for_provider(provider.id()),
        now(),
    );
    {
        let mut runtime = RUNTIME.lock().map_err(|_| "切换器不可用".to_string())?;
        if runtime.switching || runtime.confirming {
            return Err("正在切换账号，请稍后再试。".into());
        }
        if runtime.pending.as_ref().is_some_and(|offer| !offer.preview) {
            return Err("已有提醒正在显示。".into());
        }
        runtime.pending = Some(offer);
    }
    show_popup(&app);
    Ok(())
}

/// Run one check now (refresh intervals and cooldown still apply).
#[tauri::command]
pub(crate) fn run_account_switcher_check(app: AppHandle) {
    thread::spawn(move || tick(&app));
}
