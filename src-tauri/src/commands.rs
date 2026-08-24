use std::{path::PathBuf, sync::{mpsc, Arc, Mutex as StdMutex}, thread};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::apps::{launch_cursor, terminate_cursor, wait_for_cursor_stop};
use crate::cursor::api::{fetch_cursor_subscription, fetch_cursor_subscription_fast};
use crate::cursor::oauth::{
    complete_cursor_oauth, enrich_cursor_session, open_browser, OauthLoginState,
};
use crate::cursor::usage::fetch_cursor_usage;
use crate::error::AppError;
use crate::models::{
    import_type, Account, AccountSummary, ApplicationKind, ApplicationStatus, CursorUsageDetails,
    Session, SwitchOutcome, SwitchProgress,
};
use crate::store::AppState;
use crate::tray::refresh_tray;

pub(crate) fn emit_switch_progress(
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

#[tauri::command]
pub(crate) fn list_applications(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<ApplicationStatus>, String> {
    state
        .0
        .lock()
        .map_err(|_| "应用状态不可用".to_string())
        .map(|controller| controller.statuses())
}

#[tauri::command]
pub(crate) fn list_accounts(
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
pub(crate) fn get_database_path(state: State<'_, AppState>) -> std::result::Result<String, String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())
        .map(|controller| controller.database_path())
}

#[tauri::command]
pub(crate) fn move_database(
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
pub(crate) fn export_cursor_accounts(
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
pub(crate) fn get_cursor_export_record(
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
pub(crate) fn refresh_account_subscription(
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
    let summary = match fetch_cursor_subscription(&session) {
        Ok(summary) => summary,
        Err(error) => {
            if error.to_string().contains("失效") || error.to_string().contains("过期") {
                let _ = state.0.lock().ok().and_then(|mut controller| controller.mark_token_invalid(&id).ok());
                let _ = app.emit("accounts-changed", ());
            }
            return Err(error_text(error));
        }
    };
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .save_subscription(&id, summary)
        .map_err(error_text)?;
    let _ = app.emit("accounts-changed", ());
    Ok(())
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RefreshAccountsResult {
    pub total: usize,
    pub failed: usize,
    pub invalid: usize,
    pub missing: usize,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshAccountsProgress { completed: usize, total: usize }

#[tauri::command]
pub(crate) async fn refresh_all_cursor_accounts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<RefreshAccountsResult, String> {
    let (sessions, missing_credentials) = {
        let mut controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
        let mut sessions = Vec::new();
        let mut missing = Vec::new();
        for account in controller.accounts(ApplicationKind::Cursor) {
            match controller.subscription_session(&account.id) {
                Ok(session) => sessions.push((account.id, session)),
                Err(_) => missing.push(account.id),
            }
        }
        (sessions, missing)
    };
    let total = sessions.len() + missing_credentials.len();
    let progress_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let session_count = sessions.len();
        let (job_sender, job_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        for session in sessions { let _ = job_sender.send(session); }
        drop(job_sender);
        let job_receiver = Arc::new(StdMutex::new(job_receiver));
        let workers = usize::min(3, session_count);
        let jobs = (0..workers).map(|_| {
            let jobs = Arc::clone(&job_receiver);
            let results = result_sender.clone();
            thread::spawn(move || loop {
                let Some((id, session)) = jobs.lock().ok().and_then(|receiver| receiver.recv().ok()) else { break; };
                let _ = results.send((id, fetch_cursor_subscription_fast(&session)));
            })
        }).collect::<Vec<_>>();
        drop(result_sender);
        let mut updates = Vec::new();
        let mut failures = Vec::new();
        for completed in 1..=session_count {
            match result_receiver.recv() {
                Ok((id, Ok(summary))) => updates.push((id, summary)),
                Ok((id, Err(error))) => failures.push((id, error.to_string())),
                Err(_) => { failures.push((String::new(), "刷新线程异常退出".into())); break; }
            }
            let _ = progress_app.emit("account-refresh-progress", RefreshAccountsProgress { completed, total });
        }
        for job in jobs { let _ = job.join(); }
        (updates, failures)
    }).await.map_err(|error| error.to_string())?;
    let (updates, failures) = result;
    let invalid = failures.iter().filter(|(_, message)| message.contains("失效") || message.contains("过期")).count();
    let missing = missing_credentials.len();
    let failed = failures.len() + missing;
    let mut controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
    for id in missing_credentials { let _ = controller.mark_credential_missing(&id); }
    for (id, message) in failures { if !id.is_empty() && (message.contains("失效") || message.contains("过期")) { let _ = controller.mark_token_invalid(&id); } }
    for (id, summary) in updates { controller.save_subscription(&id, summary).map_err(error_text)?; }
    let _ = app.emit("accounts-changed", ());
    Ok(RefreshAccountsResult { total, failed, invalid, missing })
}

#[tauri::command]
pub(crate) async fn get_cursor_usage(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<CursorUsageDetails, String> {
    let (account, session) = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .cursor_usage_session(&id)
        .map_err(error_text)?;
    let usage_result = tauri::async_runtime::spawn_blocking(move || fetch_cursor_usage(&account, &session))
        .await
        .map_err(|error| error.to_string())?;
    let (usage, raw) = match usage_result {
        Ok(value) => value,
        Err(error) => {
            let message = error.to_string();
            if message.contains("失效") || message.contains("过期") {
                if let Ok(mut controller) = state.0.lock() { let _ = controller.mark_token_invalid(&id); }
                let _ = app.emit("accounts-changed", ());
            }
            return Err(error_text(error));
        }
    };
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .save_cursor_usage(&id, usage.clone(), raw)
        .map_err(error_text)?;
    Ok(usage)
}

#[tauri::command]
pub(crate) fn get_saved_cursor_usage(
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
pub(crate) fn reorder_accounts(
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
pub(crate) fn import_current_account(
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
pub(crate) fn import_token_or_json(
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
pub(crate) async fn start_official_login(
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
pub(crate) fn cancel_official_login(state: State<'_, OauthLoginState>) {
    state.cancel();
}

#[tauri::command]
pub(crate) fn open_official_login_url(state: State<'_, OauthLoginState>) -> std::result::Result<(), String> {
    let url = state
        .url()
        .ok_or_else(|| "没有进行中的官方登录。".to_string())?;
    open_browser(&url).map_err(error_text)
}

#[tauri::command]
pub(crate) fn delete_account(
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
pub(crate) fn switch_account(
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
pub(crate) fn force_restart_cursor(
    id: String,
    operation_id: String,
    app: AppHandle,
) -> std::result::Result<(), String> {
    emit_switch_progress(&app, &operation_id, &id, "terminating", 25, "running");
    if let Err(error) = terminate_cursor().and_then(|_| wait_for_cursor_stop()) {
        emit_switch_progress(&app, &operation_id, &id, "error", 100, "error");
        return Err(error_text(error));
    }
    emit_switch_progress(&app, &operation_id, &id, "applying", 60, "running");
    let apply_result = app
        .state::<AppState>()
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())
        .and_then(|mut controller| controller.apply_account(&id).map_err(error_text));
    if let Err(error) = apply_result {
        emit_switch_progress(&app, &operation_id, &id, "error", 100, "error");
        return Err(error);
    }
    emit_switch_progress(&app, &operation_id, &id, "launching", 80, "running");
    if let Err(error) = launch_cursor() {
        emit_switch_progress(&app, &operation_id, &id, "error", 100, "error");
        return Err(error_text(error));
    }
    emit_switch_progress(&app, &operation_id, &id, "complete", 100, "success");
    Ok(())
}

pub(crate) fn error_text(error: AppError) -> String {
    error.to_string()
}
