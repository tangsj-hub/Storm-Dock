use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};
use thiserror::Error;

const SECRET_SERVICE: &str = "app.cc-login.desktop";
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
const EMAIL_KEY: &str = "cursorAuth/cachedEmail";

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
    is_current: bool,
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

#[derive(Serialize)]
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

fn import_type(session: &Session) -> ImportType {
    if session.values.contains_key("cursorAuth/refreshToken") {
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

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountsFile {
    version: u8,
    accounts: Vec<Account>,
    current_account_ids: BTreeMap<ApplicationKind, String>,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("account not found")]
    AccountNotFound,
    #[error("the saved session is missing")]
    SecretMissing,
    #[error("Token is empty or too short")]
    InvalidToken,
    #[error("JSON does not contain a supported Cursor session")]
    InvalidImport,
    #[error("this application is not supported yet")]
    ComingSoon,
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
    #[error(transparent)]
    Keyring(#[from] keyring::Error),
}

type Result<T> = std::result::Result<T, AppError>;

#[derive(Clone, Deserialize, Serialize)]
struct Session {
    values: BTreeMap<String, String>,
}

impl Session {
    fn from_import(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw.len() < 40 {
            return Err(AppError::InvalidToken);
        }
        if !raw.starts_with('{') && !raw.starts_with('[') {
            return Ok(Self {
                values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), raw.into())]),
            });
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
        let access_token = text(&["access_token", "accessToken", ACCESS_TOKEN_KEY])
            .filter(|value| value.len() >= 40)
            .ok_or(AppError::InvalidImport)?;
        let mut values = BTreeMap::from([(ACCESS_TOKEN_KEY.into(), access_token)]);
        if let Some(refresh) = text(&["refresh_token", "refreshToken", "cursorAuth/refreshToken"]) {
            values.insert("cursorAuth/refreshToken".into(), refresh);
        }
        if let Some(email) = text(&["email", "cursorAuth/cachedEmail"]) {
            values.insert(EMAIL_KEY.into(), email.clone());
            values.insert(
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
                    values.insert(target.into(), value.into());
                }
            }
        }
        Ok(Self { values })
    }
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

    fn read_session(&self, db: &Connection) -> Result<Session> {
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
        if !values.contains_key(ACCESS_TOKEN_KEY) {
            return Err(AppError::UnsupportedCursor(
                "access token is missing".into(),
            ));
        }
        Ok(Session { values })
    }

    fn write_session(&self, db: &mut Connection, session: &Session) -> Result<()> {
        if !session.values.contains_key(ACCESS_TOKEN_KEY) {
            return Err(AppError::SecretMissing);
        }
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
        let mut db = self.open()?;
        let before = self.read_session(&db)?;
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
    file: PathBuf,
    data: AccountsFile,
    cursor: CursorAdapter,
    codex: CodexAdapter,
}

impl Controller {
    fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir)?;
        let file = data_dir.join("accounts.json");
        let data = if file.exists() {
            serde_json::from_slice(&fs::read(&file)?)?
        } else {
            AccountsFile {
                version: 1,
                ..Default::default()
            }
        };
        Ok(Self {
            file,
            data,
            cursor: CursorAdapter::default(),
            codex: CodexAdapter,
        })
    }

    fn adapter(&self, kind: ApplicationKind) -> &dyn ApplicationAdapter {
        match kind {
            ApplicationKind::Cursor => &self.cursor,
            ApplicationKind::Codex => &self.codex,
        }
    }

    fn persist(&self) -> Result<()> {
        fs::write(&self.file, serde_json::to_vec_pretty(&self.data)?)?;
        Ok(())
    }

    fn secret_entry(kind: ApplicationKind, id: &str) -> Result<keyring::Entry> {
        Ok(keyring::Entry::new(
            SECRET_SERVICE,
            &format!("{kind:?}:{id}"),
        )?)
    }

    fn save_session(kind: ApplicationKind, id: &str, session: &Session) -> Result<()> {
        Self::secret_entry(kind, id)?.set_password(&serde_json::to_string(session)?)?;
        Ok(())
    }

    fn load_session(kind: ApplicationKind, id: &str) -> Result<Session> {
        let value = Self::secret_entry(kind, id)?
            .get_password()
            .map_err(|error| match error {
                keyring::Error::NoEntry => AppError::SecretMissing,
                error => AppError::Keyring(error),
            })?;
        Ok(serde_json::from_str(&value)?)
    }

    fn statuses(&self) -> Vec<ApplicationStatus> {
        vec![self.cursor.detect(), self.codex.detect()]
    }

    fn accounts(&self, kind: ApplicationKind) -> Vec<AccountSummary> {
        let current = self.data.current_account_ids.get(&kind);
        self.data
            .accounts
            .iter()
            .filter(|account| account.application == kind)
            .map(|account| AccountSummary {
                is_current: current == Some(&account.id),
                id: account.id.clone(),
                label: account.label.clone(),
                email: account.email.clone(),
                import_type: account.import_type.clone(),
            })
            .collect()
    }

    fn reorder_accounts(&mut self, kind: ApplicationKind, ids: Vec<String>) -> Result<()> {
        let existing: Vec<_> = self
            .data
            .accounts
            .iter()
            .filter(|account| account.application == kind)
            .map(|account| account.id.as_str())
            .collect();
        if ids.len() != existing.len()
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || ids.iter().any(|id| !existing.contains(&id.as_str()))
        {
            return Err(AppError::Message("账户排序无效".into()));
        }
        let ordered: Vec<_> = ids
            .iter()
            .map(|id| {
                self.data
                    .accounts
                    .iter()
                    .find(|account| &account.id == id)
                    .cloned()
                    .ok_or(AppError::AccountNotFound)
            })
            .collect::<Result<_>>()?;
        let mut ordered = ordered.into_iter();
        for account in &mut self.data.accounts {
            if account.application == kind {
                *account = ordered.next().expect("validated account order");
            }
        }
        self.persist()
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
        let id = format!(
            "acc_{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let label = label
            .filter(|value| !value.trim().is_empty())
            .or_else(|| email.clone())
            .unwrap_or_else(|| format!("{} Account", kind.display_name()));
        let account = Account {
            id: id.clone(),
            application: kind,
            label,
            email,
            import_type,
            created_at: now,
            updated_at: now,
            last_used_at: now,
        };
        Self::save_session(kind, &id, &session)?;
        self.data.accounts.push(account.clone());
        self.persist()?;
        Ok(account)
    }

    fn import_current(&mut self, kind: ApplicationKind, label: Option<String>) -> Result<Account> {
        let session = self.adapter(kind).import_current()?;
        self.save_imported_session(kind, label, session, ImportType::Native)
    }

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
        let index = self
            .data
            .accounts
            .iter()
            .position(|account| account.id == id)
            .ok_or(AppError::AccountNotFound)?;
        let account = self.data.accounts.remove(index);
        let _ = Self::secret_entry(account.application, &account.id)?.delete_credential();
        if self.data.current_account_ids.get(&account.application) == Some(&account.id) {
            self.data.current_account_ids.remove(&account.application);
        }
        self.persist()
    }

    fn switch_account<F>(&mut self, id: &str, mut progress: F) -> Result<SwitchOutcome>
    where
        F: FnMut(&'static str, u8),
    {
        progress("loading", 15);
        let account = self
            .data
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
            .ok_or(AppError::AccountNotFound)?;
        let session = Self::load_session(account.application, &account.id)?;
        let running = self.adapter(account.application).is_running();
        progress("applying", 45);
        self.adapter(account.application).apply(&session)?;
        progress("persisting", 75);
        self.data
            .current_account_ids
            .insert(account.application, account.id);
        if let Some(account) = self.data.accounts.iter_mut().find(|item| item.id == id) {
            account.last_used_at = now();
        }
        self.persist()?;
        Ok(SwitchOutcome {
            restart_required: running,
        })
    }

    fn current_label(&self) -> String {
        self.data
            .current_account_ids
            .get(&ApplicationKind::Cursor)
            .and_then(|id| self.data.accounts.iter().find(|account| &account.id == id))
            .map(|account| account.label.clone())
            .unwrap_or_else(|| "未选择账户".into())
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
    let account = state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .import_payload(kind, label, &payload)
        .map_err(error_text)?;
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    Ok(account)
}

#[tauri::command]
fn start_official_login(kind: ApplicationKind) -> std::result::Result<String, String> {
    if kind != ApplicationKind::Cursor {
        return Err(AppError::ComingSoon.to_string());
    }
    launch_cursor()
        .map(|_| "已启动 Cursor。请在 Cursor 中完成官方登录，然后回到这里导入当前账户。".into())
        .map_err(error_text)
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
        .menu(Menu::default)
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| AppError::Message(error.to_string()))?;
            app.manage(AppState(Mutex::new(Controller::new(data_dir)?)));
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
            reorder_accounts,
            import_current_account,
            import_token_or_json,
            start_official_login,
            delete_account,
            switch_account,
            force_restart_cursor
        ])
        .run(tauri::generate_context!())
        .expect("error while running cc-login");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cursor_db() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("cc-login-test-{nonce}.vscdb"));
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
        };
        assert_eq!(import_type(&jwt), ImportType::Jwt);
    }

    #[test]
    fn accounts_file_never_contains_session_values() {
        let account = Account {
            id: "acc_test".into(),
            application: ApplicationKind::Cursor,
            label: "Test".into(),
            email: None,
            import_type: ImportType::Native,
            created_at: 1,
            updated_at: 1,
            last_used_at: 1,
        };
        let json = serde_json::to_string(&AccountsFile {
            version: 1,
            accounts: vec![account],
            current_account_ids: BTreeMap::new(),
        })
        .unwrap();
        assert!(!json.contains("accessToken"));
    }

    #[test]
    fn account_order_is_preserved() {
        let first = Account {
            id: "first".into(),
            application: ApplicationKind::Cursor,
            label: "First".into(),
            email: None,
            import_type: ImportType::Native,
            created_at: 1,
            updated_at: 1,
            last_used_at: 1,
        };
        let second = Account {
            id: "second".into(),
            label: "Second".into(),
            ..first.clone()
        };
        let data_dir = env::temp_dir().join(format!("cc-login-order-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        controller.data.accounts = vec![first, second];
        controller
            .reorder_accounts(
                ApplicationKind::Cursor,
                vec!["second".into(), "first".into()],
            )
            .unwrap();
        assert_eq!(controller.accounts(ApplicationKind::Cursor)[0].id, "second");
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
}
