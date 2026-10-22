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
pub(crate) const POPUP_PAGE: &str = "switcher.html";
/// Reminder hides itself (= 忽略) this long after each show; paused while hovered
/// or while the native confirm is open. No visible countdown.
const POPUP_AUTO_HIDE: Duration = Duration::from_secs(5);
const STARTUP_DELAY_SECS: u64 = 90;
const TICK_SECS: u64 = 60;
const ACTIVE_REFRESH_SECS: u64 = 5 * 60;
const OTHERS_REFRESH_SECS: u64 = 15 * 60;
const SETTINGS_FILE: &str = "account_switcher.json";

struct Pending {
    generation: u64,
    offer: SwitchOffer,
}

impl Runtime {
    fn offer_for(&self, generation: u64) -> Option<&SwitchOffer> {
        self.pending.as_ref().filter(|p| p.generation == generation).map(|p| &p.offer)
    }
}

#[derive(Default)]
struct Runtime {
    cooldown: Cooldown,
    /// The offer on screen, tied to the popup generation that shows it.
    pending: Option<Pending>,
    /// A native confirm dialog is open.
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
        crate::native_popup::spawn_dev_import_preview_trigger();
    }
    crate::native_popup::on_expire(POPUP_LABEL, on_popup_expired);
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
        let mut runtime = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
        if runtime.pending.is_some()
            || runtime.switching
            || !runtime.cooldown.allows(&offer, now(), DEFAULT_COOLDOWN_SECS)
        {
            continue;
        }
        runtime.cooldown.record(&offer, now());
        show_offer(&mut runtime, offer);
        return; // one popup at a time
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

/// Caller holds the lock, so pending and the popup generation always agree.
fn show_offer(runtime: &mut Runtime, offer: SwitchOffer) {
    let generation = crate::native_popup::show_with(POPUP_LABEL, POPUP_PAGE, &offer, Some(POPUP_AUTO_HIDE));
    runtime.pending = Some(Pending { generation, offer });
}

/// Auto-hide fired (actor thread): same as 忽略 for that generation only.
fn on_popup_expired(generation: u64) {
    let mut runtime = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
    if runtime.offer_for(generation).is_some() && !runtime.confirming && !runtime.switching {
        runtime.pending = None;
    }
}

/// Clears the offer and hides the popup only if `generation` is still current.
fn dismiss(generation: u64, animate: bool) {
    let mut runtime = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
    if runtime.offer_for(generation).is_some() {
        runtime.pending = None;
    }
    crate::native_popup::hide(POPUP_LABEL, Some(generation), animate);
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
    RUNTIME.lock().ok().and_then(|runtime| runtime.pending.as_ref().map(|p| p.offer.clone()))
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

/// Answer from the popup showing `generation`; answers from a superseded popup
/// are ignored. `accept = false` (忽略 / swipe) keeps the current account;
/// `accept = true` first shows a native confirm, cancel = no switch.
/// `instant` hides without the slide-out (the page already swiped it away).
#[tauri::command]
pub(crate) async fn respond_switch_offer(
    app: AppHandle,
    generation: u64,
    accept: bool,
    confirm: Option<ConfirmLabels>,
    instant: Option<bool>,
) -> Result<(), String> {
    let animate = !instant.unwrap_or(false);
    let Some(offer) = RUNTIME
        .lock()
        .map_err(|_| "切换器不可用".to_string())?
        .offer_for(generation)
        .cloned()
    else {
        return Ok(());
    };
    // Preview shows the same native confirm so the flow is visible, then just hides.
    let confirm_needed =
        accept && (offer.preview || provider(&offer.provider).is_some_and(|p| p.needs_confirm()));
    if let Some(labels) = confirm.filter(|_| confirm_needed) {
        {
            let mut runtime = RUNTIME.lock().map_err(|_| "切换器不可用".to_string())?;
            if runtime.confirming || runtime.offer_for(generation).is_none() {
                return Ok(());
            }
            runtime.confirming = true;
        }
        let hold = |on| crate::native_popup::pause(POPUP_LABEL, generation, crate::native_popup::PauseReason::Hold, on);
        hold(true);
        let dialog_app = app.clone();
        let confirmed = tauri::async_runtime::spawn_blocking(move || confirm_natively(&dialog_app, &labels))
            .await
            .unwrap_or(false);
        RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).confirming = false;
        hold(false);
        if !confirmed {
            dismiss(generation, true);
            return Ok(());
        }
    }
    if offer.preview || !accept {
        dismiss(generation, animate);
        return Ok(());
    }
    let offer = {
        let mut runtime = RUNTIME.lock().map_err(|_| "切换器不可用".to_string())?;
        if runtime.offer_for(generation).is_none() {
            return Ok(());
        }
        runtime.switching = true;
        runtime.pending.take().map(|p| p.offer).unwrap_or(offer)
    };
    crate::native_popup::hide(POPUP_LABEL, Some(generation), animate);
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
/// refused only while a real reminder or switch is in progress. Repeated clicks
/// just replace the preview offer and update the popup already on screen.
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
        if runtime.pending.as_ref().is_some_and(|p| !p.offer.preview) {
            return Err("已有提醒正在显示。".into());
        }
        show_offer(&mut runtime, offer);
    }
    Ok(())
}

/// Run one check now (refresh intervals and cooldown still apply).
#[tauri::command]
pub(crate) fn run_account_switcher_check(app: AppHandle) {
    thread::spawn(move || tick(&app));
}
