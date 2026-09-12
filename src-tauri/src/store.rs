use rusqlite::{params, Connection};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::apps::{ApplicationAdapter, CodexAdapter, CursorAdapter, GrokAdapter};
use crate::cursor::session::{raw_export_from_session, session_display_label};
use crate::cursor::usage::{cursor_usage_from_snapshot, update_export_usage, usage_pools};
use crate::error::{AppError, Result};
use crate::models::{
    days_remaining, matching_account_index, now, subscription_from_session, Account,
    AccountSummary, ApplicationKind, ApplicationStatus, CursorUsageDetails, ImportType, Session,
    SubscriptionSummary, SwitchOutcome, ACCESS_TOKEN_KEY, AUTH_ID_KEY, EMAIL_KEY,
};

#[cfg(test)]
use crate::models::import_type;

pub(crate) const DATABASE_NAME: &str = "storm-dock.db";
const DATABASE_PATH_FILE: &str = "database-path.txt";

pub(crate) struct AppState(pub(crate) Mutex<Controller>);

pub(crate) struct Controller {
    pointer_file: PathBuf,
    database_path: PathBuf,
    database: Connection,
    cursor: CursorAdapter,
    codex: CodexAdapter,
    grok: GrokAdapter,
}

impl Controller {
    pub(crate) fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir)?;
        let pointer_file = data_dir.join(DATABASE_PATH_FILE);
        let database_path = fs::read_to_string(&pointer_file)
            .ok()
            .map(|value| PathBuf::from(value.trim()))
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| data_dir.join(DATABASE_NAME));
        let database = Self::open_database(&database_path)?;
        let mut controller = Self {
            pointer_file,
            database_path,
            database,
            cursor: CursorAdapter::default(),
            codex: CodexAdapter::default(),
            grok: GrokAdapter::default(),
        };
        controller.migrate_raw_exports()?;
        Ok(controller)
    }

    pub(crate) fn open_database(path: &std::path::Path) -> Result<Connection> {
        let parent = path
            .parent()
            .ok_or_else(|| AppError::Message("数据库路径无效".into()))?;
        fs::create_dir_all(parent)?;
        let database = Connection::open(path)?;
        database.busy_timeout(Duration::from_secs(5))?;
        database.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS accounts (
               id TEXT PRIMARY KEY, application TEXT NOT NULL, label TEXT NOT NULL,
               email TEXT, import_type TEXT NOT NULL, subscription_json TEXT NOT NULL,
               token_status TEXT,
               usage_json TEXT, usage_raw_json TEXT, raw_export_json TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
               last_used_at INTEGER NOT NULL, sort_order INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS sessions (
               account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
               session_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS application_state (
               application TEXT PRIMARY KEY, current_account_id TEXT
             );
             CREATE TABLE IF NOT EXISTS app_kv (
               key TEXT PRIMARY KEY, value TEXT NOT NULL
             );
             PRAGMA user_version=1;",
        )?;
        let columns = database
            .prepare("PRAGMA table_info(accounts)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !columns.iter().any(|name| name == "usage_raw_json") {
            database.execute("ALTER TABLE accounts ADD COLUMN usage_raw_json TEXT", [])?;
        }
        if !columns.iter().any(|name| name == "raw_export_json") {
            database.execute("ALTER TABLE accounts ADD COLUMN raw_export_json TEXT", [])?;
        }
        if !columns.iter().any(|name| name == "token_status") {
            database.execute("ALTER TABLE accounts ADD COLUMN token_status TEXT", [])?;
        }
        if !columns.iter().any(|name| name == "usage_summary_json") {
            database.execute(
                "ALTER TABLE accounts ADD COLUMN usage_summary_json TEXT",
                [],
            )?;
        }
        Ok(database)
    }

    pub(crate) fn kind_value(kind: ApplicationKind) -> &'static str {
        match kind {
            ApplicationKind::Cursor => "cursor",
            ApplicationKind::Codex => "codex",
            ApplicationKind::Grok => "grok",
        }
    }

    pub(crate) fn import_type_value(import_type: &ImportType) -> &'static str {
        match import_type {
            ImportType::OAuth => "oauth",
            ImportType::Token => "token",
            ImportType::Jwt => "jwt",
            ImportType::Native => "native",
            ImportType::ApiKey => "api_key",
        }
    }

    pub(crate) fn import_type_from(value: &str) -> Result<ImportType> {
        match value {
            "oauth" => Ok(ImportType::OAuth),
            "token" => Ok(ImportType::Token),
            "jwt" => Ok(ImportType::Jwt),
            "native" => Ok(ImportType::Native),
            "api_key" => Ok(ImportType::ApiKey),
            _ => Err(AppError::Message("数据库中的导入类型无效".into())),
        }
    }

    pub(crate) fn all_accounts(&self) -> Result<Vec<Account>> {
        let mut statement = self.database.prepare("SELECT id, application, label, email, import_type, subscription_json, usage_json, usage_raw_json, raw_export_json, created_at, updated_at, last_used_at FROM accounts ORDER BY application, sort_order")?;
        let mut rows = statement.query([])?;
        let mut accounts = Vec::new();
        while let Some(row) = rows.next()? {
            let application = match row.get::<_, String>(1)?.as_str() {
                "cursor" => ApplicationKind::Cursor,
                "codex" => ApplicationKind::Codex,
                "grok" => ApplicationKind::Grok,
                _ => return Err(AppError::Message("数据库中的应用类型无效".into())),
            };
            accounts.push(Account {
                id: row.get(0)?,
                application,
                label: row.get(2)?,
                email: row.get(3)?,
                import_type: Self::import_type_from(&row.get::<_, String>(4)?)?,
                subscription: serde_json::from_str(&row.get::<_, String>(5)?)?,
                raw_export: row
                    .get::<_, Option<String>>(8)?
                    .map(|json| serde_json::from_str(&json))
                    .transpose()?
                    .unwrap_or(serde_json::Value::Null),
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                last_used_at: row.get(11)?,
            });
        }
        Ok(accounts)
    }

    pub(crate) fn account(&self, id: &str) -> Result<Account> {
        self.all_accounts()?
            .into_iter()
            .find(|account| account.id == id)
            .ok_or(AppError::AccountNotFound)
    }

    pub(crate) fn load_session(&self, id: &str) -> Result<Session> {
        let mut statement = self
            .database
            .prepare("SELECT session_json FROM sessions WHERE account_id = ?1")?;
        let mut rows = statement.query(params![id])?;
        let Some(row) = rows.next()? else {
            return Err(AppError::SecretMissing);
        };
        Ok(serde_json::from_str(&row.get::<_, String>(0)?)?)
    }

    pub(crate) fn legacy_usage_raw(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let json: Option<String> = self.database.query_row(
            "SELECT usage_raw_json FROM accounts WHERE id=?1",
            params![id],
            |row| row.get(0),
        )?;
        json.map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(Into::into)
    }

    pub(crate) fn migrate_raw_exports(&mut self) -> Result<()> {
        for account in self.all_accounts()? {
            let mut raw = account.raw_export;
            let mut changed = false;
            if raw.is_null() {
                raw = raw_export_from_session(
                    &self.load_session(&account.id)?,
                    &account.id,
                    account.created_at,
                    account.updated_at,
                    account.last_used_at,
                    self.cursor.telemetry(),
                );
                changed = true;
            }
            if raw.get("cursor_usage_raw").is_none() {
                if let Some(usage) = self.legacy_usage_raw(&account.id)? {
                    update_export_usage(&mut raw, usage, account.updated_at);
                    changed = true;
                }
            }
            if changed {
                self.database.execute(
                    "UPDATE accounts SET raw_export_json=?1 WHERE id=?2",
                    params![serde_json::to_string(&raw)?, account.id],
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn adapter(&self, kind: ApplicationKind) -> &dyn ApplicationAdapter {
        match kind {
            ApplicationKind::Cursor => &self.cursor,
            ApplicationKind::Codex => &self.codex,
            ApplicationKind::Grok => &self.grok,
        }
    }

    fn prepare_codex_apply(&mut self) {
        self.codex.preserve_official_auth = self.preserve_codex_official_auth();
    }

    pub(crate) fn preserve_codex_official_auth(&self) -> bool {
        self.database
            .query_row(
                "SELECT value FROM app_kv WHERE key=?1",
                params!["preserve_codex_official_auth"],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .map(|value| value != "0")
            .unwrap_or(true)
    }

    pub(crate) fn set_preserve_codex_official_auth(&mut self, enabled: bool) -> Result<()> {
        self.database.execute(
            "INSERT INTO app_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params!["preserve_codex_official_auth", if enabled { "1" } else { "0" }],
        )?;
        self.codex.preserve_official_auth = enabled;
        Ok(())
    }

    pub(crate) fn statuses(&self) -> Vec<ApplicationStatus> {
        vec![
            self.cursor.detect(),
            self.codex.detect(),
            self.grok.detect(),
        ]
    }

    pub(crate) fn current_cursor_session(&self) -> Result<Session> {
        self.cursor.import_current()
    }

    pub(crate) fn accounts(&self, kind: ApplicationKind) -> Vec<AccountSummary> {
        let current_session = match kind {
            ApplicationKind::Codex => self.codex.live_match_session().ok(),
            ApplicationKind::Grok => self.grok.live_match_session().ok(),
            ApplicationKind::Cursor => self.cursor.import_current().ok(),
        };
        let grok_bot_active = if kind == ApplicationKind::Cursor {
            crate::grok_bot::active_slot()
        } else {
            None
        };
        self.all_accounts()
            .unwrap_or_default()
            .into_iter()
            .filter(|account| account.application == kind)
            .map(|account| {
                let account_session = self.load_session(&account.id).ok();
                let is_current = current_session
                    .as_ref()
                    .zip(account_session.as_ref())
                    .is_some_and(|(current, saved)| match kind {
                        ApplicationKind::Codex => {
                            crate::codex::session::matches_live(saved, current)
                        }
                        ApplicationKind::Grok => crate::grok::session::matches_live(saved, current),
                        ApplicationKind::Cursor => [AUTH_ID_KEY, EMAIL_KEY, ACCESS_TOKEN_KEY]
                            .iter()
                            .any(|key| {
                                current.values.get(*key).is_some_and(|value| {
                                    !value.is_empty() && saved.values.get(*key) == Some(value)
                                })
                            }),
                    });
                let is_grok_bot_current = grok_bot_active
                    .as_deref()
                    .zip(account_session.as_ref())
                    .is_some_and(|(active, saved)| {
                        crate::grok_bot::session_matches_active_slot(saved, active)
                    });
                let (grok_bot_usage, grok_bot_reset_at) = if kind == ApplicationKind::Cursor {
                    self.cursor_grok_bot_summary(&account)
                } else {
                    (None, None)
                };
                AccountSummary {
                    is_current,
                    is_grok_bot_current,
                    id: account.id.clone(),
                    label: account.label.clone(),
                    email: account.email.clone(),
                    import_type: account.import_type.clone(),
                    subscription: account.subscription.clone(),
                    usage: self
                        .database
                        .query_row(
                            "SELECT usage_summary_json FROM accounts WHERE id=?1",
                            params![account.id],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                        .and_then(|json| serde_json::from_str(&json).ok()),
                    grok_bot_usage,
                    grok_bot_reset_at,
                    status: self
                        .database
                        .query_row(
                            "SELECT token_status FROM accounts WHERE id=?1",
                            params![account.id],
                            |row| row.get(0),
                        )
                        .ok()
                        .flatten(),
                    days_remaining: account
                        .subscription
                        .reset_timestamp(&account.raw_export)
                        .map(|expires_at| days_remaining(expires_at, now())),
                    base_url: match kind {
                        ApplicationKind::Codex => account_session
                            .as_ref()
                            .and_then(crate::codex::session::base_url),
                        ApplicationKind::Grok => account_session
                            .as_ref()
                            .and_then(crate::grok::session::base_url),
                        ApplicationKind::Cursor => None,
                    },
                }
            })
            .collect()
    }

    fn cursor_grok_bot_summary(
        &self,
        account: &Account,
    ) -> (Option<crate::models::UsageMetric>, Option<String>) {
        let json: Option<String> = self
            .database
            .query_row(
                "SELECT usage_json FROM accounts WHERE id=?1",
                params![account.id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        if let Some(json) = json {
            if let Ok(details) = serde_json::from_str::<CursorUsageDetails>(&json) {
                if details.account_id == account.id {
                    return (details.grok_bot, details.grok_bot_reset_at);
                }
            }
        }
        account
            .raw_export
            .get("cursor_usage_raw")
            .and_then(|raw| cursor_usage_from_snapshot(account, raw))
            .map(|details| (details.grok_bot, details.grok_bot_reset_at))
            .unwrap_or((None, None))
    }

    pub(crate) fn reorder_accounts(
        &mut self,
        kind: ApplicationKind,
        ids: Vec<String>,
    ) -> Result<()> {
        let accounts = self.all_accounts()?;
        let existing: Vec<_> = accounts
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
        let transaction = self.database.transaction()?;
        for (position, id) in ids.iter().enumerate() {
            transaction.execute(
                "UPDATE accounts SET sort_order = ?1 WHERE id = ?2",
                params![position as i64, id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn save_imported_session(
        &mut self,
        kind: ApplicationKind,
        label: Option<String>,
        session: Session,
        import_type: ImportType,
    ) -> Result<Account> {
        let now = now();
        let email = session
            .values
            .get(match kind {
                ApplicationKind::Codex => crate::codex::CODEX_EMAIL_KEY,
                ApplicationKind::Grok => crate::grok::GROK_EMAIL_KEY,
                ApplicationKind::Cursor => EMAIL_KEY,
            })
            .cloned()
            .filter(|value| !value.is_empty());
        let display_label = match kind {
            ApplicationKind::Codex => crate::codex::session::display_label(&session),
            ApplicationKind::Grok => crate::grok::session::display_label(&session),
            ApplicationKind::Cursor => session_display_label(&session),
        };
        let existing_id = match kind {
            ApplicationKind::Codex => self.matching_codex_account_id(&session)?,
            ApplicationKind::Grok => self.matching_grok_account_id(&session)?,
            ApplicationKind::Cursor => {
                if let Some(email) = email.as_deref() {
                    matching_account_index(&self.all_accounts()?, kind, email)
                        .map(|index| self.all_accounts().unwrap()[index].id.clone())
                } else {
                    None
                }
            }
        };
        if let Some(existing_id) = existing_id {
            let mut account = self.account(&existing_id)?;
            if let Some(label) = label.filter(|value| !value.trim().is_empty()) {
                account.label = label;
            } else if let Some(display_label) = display_label.clone() {
                account.label = display_label;
            }
            account.email = email.clone();
            account.import_type = import_type;
            let imported_subscription = subscription_from_session(&session);
            if imported_subscription.plan.is_some() {
                account.subscription.merge_from(imported_subscription);
            }
            account.updated_at = now;
            account.last_used_at = now;
            account.raw_export = raw_export_from_session(
                &session,
                &account.id,
                account.created_at,
                now,
                now,
                self.cursor.telemetry(),
            );
            let transaction = self.database.transaction()?;
            transaction.execute("UPDATE accounts SET label=?1, email=?2, import_type=?3, subscription_json=?4, raw_export_json=?5, updated_at=?6, last_used_at=?7 WHERE id=?8", params![account.label, account.email, Self::import_type_value(&account.import_type), serde_json::to_string(&account.subscription)?, serde_json::to_string(&account.raw_export)?, account.updated_at as i64, account.last_used_at as i64, account.id])?;
            transaction.execute("INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2) ON CONFLICT(account_id) DO UPDATE SET session_json=excluded.session_json", params![account.id, serde_json::to_string(&session)?])?;
            transaction.commit()?;
            return Ok(account);
        }
        let id = format!(
            "acc_{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let label = label
            .filter(|value| !value.trim().is_empty())
            .or(display_label)
            .unwrap_or_else(|| format!("{} Account", kind.display_name()));
        let account = Account {
            id: id.clone(),
            application: kind,
            label,
            email,
            import_type,
            subscription: subscription_from_session(&session),
            raw_export: raw_export_from_session(
                &session,
                &id,
                now,
                now,
                now,
                self.cursor.telemetry(),
            ),
            created_at: now,
            updated_at: now,
            last_used_at: now,
        };
        let transaction = self.database.transaction()?;
        let sort_order: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM accounts WHERE application=?1",
            params![Self::kind_value(kind)],
            |row| row.get(0),
        )?;
        transaction.execute("INSERT INTO accounts (id, application, label, email, import_type, subscription_json, usage_json, usage_raw_json, raw_export_json, created_at, updated_at, last_used_at, sort_order) VALUES (?1,?2,?3,?4,?5,?6,NULL,NULL,?7,?8,?9,?10,?11)", params![account.id, Self::kind_value(kind), account.label, account.email, Self::import_type_value(&account.import_type), serde_json::to_string(&account.subscription)?, serde_json::to_string(&account.raw_export)?, account.created_at as i64, account.updated_at as i64, account.last_used_at as i64, sort_order])?;
        transaction.execute(
            "INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2)",
            params![account.id, serde_json::to_string(&session)?],
        )?;
        transaction.commit()?;
        Ok(account)
    }

    pub(crate) fn import_current(
        &mut self,
        kind: ApplicationKind,
        label: Option<String>,
    ) -> Result<Account> {
        let session = self.adapter(kind).import_current()?;
        let import_type = match kind {
            ApplicationKind::Codex => crate::codex::session::import_type(&session),
            ApplicationKind::Grok => crate::grok::session::import_type(&session),
            ApplicationKind::Cursor => ImportType::Native,
        };
        self.save_imported_session(kind, label, session, import_type)
    }

    fn matching_codex_account_id(&self, session: &Session) -> Result<Option<String>> {
        for account in self.all_accounts()? {
            if account.application != ApplicationKind::Codex {
                continue;
            }
            if let Ok(saved) = self.load_session(&account.id) {
                if crate::codex::session::same_identity(&saved, session) {
                    return Ok(Some(account.id));
                }
            }
        }
        Ok(None)
    }

    fn matching_grok_account_id(&self, session: &Session) -> Result<Option<String>> {
        for account in self.all_accounts()? {
            if account.application != ApplicationKind::Grok {
                continue;
            }
            if let Ok(saved) = self.load_session(&account.id) {
                if crate::grok::session::same_identity(&saved, session) {
                    return Ok(Some(account.id));
                }
            }
        }
        Ok(None)
    }

    #[cfg(test)]
    pub(crate) fn import_payload(
        &mut self,
        kind: ApplicationKind,
        label: Option<String>,
        payload: &str,
    ) -> Result<Account> {
        if kind == ApplicationKind::Codex {
            let session = crate::codex::session::from_import(payload)?;
            let import_type = crate::codex::session::import_type(&session);
            return self.save_imported_session(kind, label, session, import_type);
        }
        if kind == ApplicationKind::Grok {
            let session = crate::grok::session::from_import(payload)?;
            let import_type = crate::grok::session::import_type(&session);
            return self.save_imported_session(kind, label, session, import_type);
        }
        if kind != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        let session = Session::from_import(payload)?;
        let import_type = import_type(&session);
        self.save_imported_session(kind, label, session, import_type)
    }

    pub(crate) fn delete_account(&mut self, id: &str) -> Result<()> {
        self.account(id)?;
        let transaction = self.database.transaction()?;
        transaction.execute("DELETE FROM accounts WHERE id=?1", params![id])?;
        transaction.execute(
            "DELETE FROM application_state WHERE current_account_id=?1",
            params![id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn subscription_session(&mut self, id: &str) -> Result<Session> {
        self.load_session(&self.account(id)?.id)
    }

    pub(crate) fn save_subscription(
        &mut self,
        id: &str,
        summary: SubscriptionSummary,
    ) -> Result<()> {
        let mut account = self.account(id)?;
        account.subscription.merge_from(summary);
        account.updated_at = now();
        self.database.execute("UPDATE accounts SET subscription_json=?1, token_status=NULL, updated_at=?2 WHERE id=?3", params![serde_json::to_string(&account.subscription)?, account.updated_at as i64, id])?;
        Ok(())
    }

    pub(crate) fn mark_token_invalid(&mut self, id: &str) -> Result<()> {
        self.database.execute(
            "UPDATE accounts SET token_status='invalid', updated_at=?1 WHERE id=?2",
            params![now() as i64, id],
        )?;
        Ok(())
    }

    pub(crate) fn mark_credential_missing(&mut self, id: &str) -> Result<()> {
        self.database.execute(
            "UPDATE accounts SET token_status='missing', updated_at=?1 WHERE id=?2",
            params![now() as i64, id],
        )?;
        Ok(())
    }

    pub(crate) fn saved_cursor_usage(&self, id: &str) -> Result<Option<CursorUsageDetails>> {
        let account = self.account(id)?;
        if account.application != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        let json: Option<String> = self.database.query_row(
            "SELECT usage_json FROM accounts WHERE id=?1",
            params![id],
            |row| row.get(0),
        )?;
        if let Some(json) = json {
            if let Ok(mut details) = serde_json::from_str::<CursorUsageDetails>(&json) {
                if details.account_id != id {
                    return Ok(None);
                }
                if let Some(raw) = account.raw_export.get("cursor_usage_raw") {
                    let (primary, on_demand) = usage_pools(raw, raw.get("hard_limit"));
                    if !(primary.kind == "requests" && details.primary.kind != "requests") {
                        details.primary = primary;
                        details.on_demand = on_demand;
                    }
                }
                return Ok(Some(details));
            }
        }
        let raw = account.raw_export.get("cursor_usage_raw").cloned();
        Ok(raw.and_then(|raw| cursor_usage_from_snapshot(&account, &raw)))
    }

    pub(crate) fn save_cursor_usage(
        &mut self,
        id: &str,
        usage: CursorUsageDetails,
        raw: serde_json::Value,
    ) -> Result<()> {
        if usage.account_id != id {
            return Err(AppError::Message("用量数据与账号不匹配。".into()));
        }
        let mut account = self.account(id)?;
        if let Some(summary) = raw.get("usage_summary") {
            account
                .subscription
                .merge_from(crate::cursor::api::subscription_from_response(summary));
        } else if let Some(reset_at) = usage.reset_at.clone() {
            account.subscription.merge_from(SubscriptionSummary {
                billing_cycle_end: Some(reset_at.clone()),
                expires_at: crate::models::parse_iso_timestamp(&reset_at),
                plan: usage.membership_type.clone(),
                checked_at: Some(usage.checked_at),
            });
        }
        update_export_usage(&mut account.raw_export, raw, usage.checked_at);
        self.database.execute(
            "UPDATE accounts SET usage_json=?1, usage_summary_json=?2, raw_export_json=?3, subscription_json=?4, updated_at=?5 WHERE id=?6",
            params![
                serde_json::to_string(&usage)?,
                serde_json::to_string(&usage.primary)?,
                serde_json::to_string(&account.raw_export)?,
                serde_json::to_string(&account.subscription)?,
                now() as i64,
                id
            ],
        )?;
        Ok(())
    }

    pub(crate) fn save_cursor_usage_summary(
        &mut self,
        id: &str,
        usage: crate::models::UsageMetric,
    ) -> Result<()> {
        self.database.execute(
            "UPDATE accounts SET usage_summary_json=?1 WHERE id=?2",
            params![serde_json::to_string(&usage)?, id],
        )?;
        Ok(())
    }

    pub(crate) fn cursor_usage_session(&mut self, id: &str) -> Result<(Account, Session)> {
        let account = self.account(id)?;
        if account.application != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        let session = self.subscription_session(id)?;
        Ok((account, session))
    }

    pub(crate) fn switch_account<F>(&mut self, id: &str, mut progress: F) -> Result<SwitchOutcome>
    where
        F: FnMut(&'static str, u8),
    {
        let account = self.account(id)?;
        if !account.import_type.supports_desktop_switch() {
            return Err(AppError::Message(
                "Token / JWT 账户只能查询用量，不能切换登录。".into(),
            ));
        }
        let session = self.load_session(&account.id)?;
        let running = self.adapter(account.application).is_running();
        // A running Cursor process can flush its old in-memory state back to
        // state.vscdb. Defer the write and the progress UI until the user
        // confirms a force restart.
        if running && account.application == ApplicationKind::Cursor {
            let transaction = self.database.transaction()?;
            transaction.execute(
                "UPDATE accounts SET last_used_at=?1 WHERE id=?2",
                params![now() as i64, id],
            )?;
            transaction.commit()?;
            return Ok(SwitchOutcome {
                restart_required: true,
            });
        }
        progress("loading", 15);
        progress("applying", 45);
        if !running {
            self.prepare_codex_apply();
            self.adapter(account.application).apply(&session)?;
        }
        progress("persisting", 75);
        let transaction = self.database.transaction()?;
        if !running {
            transaction.execute("INSERT INTO application_state (application, current_account_id) VALUES (?1, ?2) ON CONFLICT(application) DO UPDATE SET current_account_id=excluded.current_account_id", params![Self::kind_value(account.application), account.id])?;
        }
        transaction.execute(
            "UPDATE accounts SET last_used_at=?1 WHERE id=?2",
            params![now() as i64, id],
        )?;
        transaction.commit()?;
        Ok(SwitchOutcome {
            restart_required: running,
        })
    }

    pub(crate) fn apply_account(&mut self, id: &str) -> Result<()> {
        let account = self.account(id)?;
        let session = self.load_session(id)?;
        self.prepare_codex_apply();
        self.adapter(account.application).apply(&session)?;
        self.database.execute("INSERT INTO application_state (application, current_account_id) VALUES (?1, ?2) ON CONFLICT(application) DO UPDATE SET current_account_id=excluded.current_account_id", params![Self::kind_value(account.application), account.id])?;
        Ok(())
    }

    pub(crate) fn current_label(&self) -> String {
        self.accounts(ApplicationKind::Cursor)
            .into_iter()
            .find(|account| account.is_current)
            .map(|account| account.label)
            .unwrap_or_else(|| "未选择账户".into())
    }

    pub(crate) fn database_path(&self) -> String {
        self.database_path.display().to_string()
    }

    pub(crate) fn move_database(&mut self, directory: PathBuf) -> Result<String> {
        if !directory.is_dir() {
            return Err(AppError::Message("请选择有效的同步目录".into()));
        }
        let target = directory.join(DATABASE_NAME);
        if target == self.database_path {
            return Ok(self.database_path());
        }
        if target.exists() {
            return Err(AppError::Message("目标目录已包含 storm-dock.db".into()));
        }
        let temporary = directory.join(format!(".{DATABASE_NAME}.tmp"));
        if temporary.exists() {
            fs::remove_file(&temporary)?;
        }
        self.database.execute_batch("PRAGMA optimize;")?;
        let source = self.database_path.clone();
        fs::copy(&source, &temporary)?;
        let check = Connection::open(&temporary)?;
        let integrity: String = check.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            let _ = fs::remove_file(&temporary);
            return Err(AppError::Message("迁移后的数据库校验失败".into()));
        }
        drop(check);
        fs::rename(&temporary, &target)?;
        fs::write(&self.pointer_file, target.to_string_lossy().as_bytes())?;
        let database = Self::open_database(&target)?;
        self.database = database;
        self.database_path = target;
        let _ = fs::remove_file(source);
        Ok(self.database_path())
    }

    fn same_file(left: &Path, right: &Path) -> bool {
        match (fs::canonicalize(left), fs::canonicalize(right)) {
            (Ok(a), Ok(b)) => a == b,
            _ => left == right,
        }
    }

    fn assert_importable_database(path: &Path) -> Result<()> {
        let database =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let integrity: String =
            database.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(AppError::Message("数据库校验失败".into()));
        }
        let has_accounts: i64 = database.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='accounts'",
            [],
            |row| row.get(0),
        )?;
        if has_accounts == 0 {
            return Err(AppError::Message("不是 Storm Dock 数据库".into()));
        }
        Ok(())
    }

    pub(crate) fn export_database(&self, file: PathBuf) -> Result<()> {
        if Self::same_file(&file, &self.database_path) {
            return Err(AppError::Message(
                "不能导出到当前正在使用的数据库文件".into(),
            ));
        }
        if let Some(parent) = file.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let dump = crate::sql_backup::dump_sql(&self.database)?;
        let temporary = file.with_file_name(format!(
            ".{}.tmp",
            file.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("storm-dock.sql")
        ));
        fs::write(&temporary, dump.as_bytes())?;
        if file.exists() {
            fs::remove_file(&file)?;
        }
        fs::rename(&temporary, &file)?;
        Ok(())
    }

    pub(crate) fn import_database(&mut self, file: PathBuf) -> Result<String> {
        if Self::same_file(&file, &self.database_path) {
            return Err(AppError::Message("不能导入当前正在使用的数据库".into()));
        }
        if !file.is_file() {
            return Err(AppError::Message("请选择有效的数据库文件".into()));
        }
        match crate::sql_backup::sniff(&file)? {
            crate::sql_backup::BackupKind::StormDockSql => {
                let sql = fs::read_to_string(&file)?;
                let staging = self.staging_import_path("sql")?;
                let result = (|| {
                    let staging_db = Connection::open(&staging)?;
                    crate::sql_backup::load_sql(&staging_db, &sql)?;
                    drop(staging_db);
                    self.replace_with_database_file(staging.clone())
                })();
                let _ = fs::remove_file(&staging);
                result
            }
            crate::sql_backup::BackupKind::CcSwitchSql => self.import_cc_switch_sql(&file),
        }
    }

    fn staging_import_path(&self, suffix: &str) -> Result<PathBuf> {
        let parent = self
            .database_path
            .parent()
            .ok_or_else(|| AppError::Message("数据库路径无效".into()))?;
        let path = parent.join(format!(".{DATABASE_NAME}.{suffix}.import"));
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(path)
    }

    fn import_cc_switch_sql(&mut self, file: &Path) -> Result<String> {
        let sql = fs::read_to_string(file)?;
        let staging = self.staging_import_path("cc")?;
        let imported = (|| {
            let staging_db = Connection::open(&staging)?;
            crate::sql_backup::load_sql(&staging_db, &sql)?;
            crate::sql_backup::cc_switch_codex_sessions(&staging_db)
        })();
        let _ = fs::remove_file(&staging);
        let sessions = imported?;
        if sessions.is_empty() {
            return Err(AppError::Message(
                "未找到可导入的 ChatGPT 账号。官方登录凭证不在 CC Switch 的 SQL 备份中。".into(),
            ));
        }
        for (name, session) in sessions {
            let import_type = crate::codex::session::import_type(&session);
            self.save_imported_session(ApplicationKind::Codex, Some(name), session, import_type)?;
        }
        Ok(self.database_path())
    }

    fn replace_with_database_file(&mut self, file: PathBuf) -> Result<String> {
        Self::assert_importable_database(&file)?;
        let parent = self
            .database_path
            .parent()
            .ok_or_else(|| AppError::Message("数据库路径无效".into()))?;
        let temporary = parent.join(format!(".{DATABASE_NAME}.import"));
        let backup = parent.join(format!(".{DATABASE_NAME}.bak"));
        if temporary.exists() {
            fs::remove_file(&temporary)?;
        }
        fs::copy(&file, &temporary)?;
        Self::assert_importable_database(&temporary)?;
        let live = self.database_path.clone();
        self.database = Connection::open_in_memory()?;
        if backup.exists() {
            let _ = fs::remove_file(&backup);
        }
        if let Err(error) = fs::rename(&live, &backup) {
            self.database = Self::open_database(&live)?;
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        if let Err(error) = fs::rename(&temporary, &live) {
            let _ = fs::rename(&backup, &live);
            self.database = Self::open_database(&live)?;
            return Err(error.into());
        }
        match Self::open_database(&live) {
            Ok(database) => {
                self.database = database;
                let _ = fs::remove_file(&backup);
                self.migrate_raw_exports()?;
                Ok(self.database_path())
            }
            Err(error) => {
                let _ = fs::remove_file(&live);
                let _ = fs::rename(&backup, &live);
                self.database = Self::open_database(&live)?;
                Err(error)
            }
        }
    }

    pub(crate) fn export_cursor_account(&self, account: &Account) -> Result<serde_json::Value> {
        if !account.raw_export.is_object() {
            return Err(AppError::Message("账户的原始导出数据无效".into()));
        }
        let mut record = account.raw_export.clone();
        if let Some(raw) = record.get("cursor_usage_sources").cloned() {
            let checked_at = record
                .get("usage_updated_at")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(account.updated_at);
            update_export_usage(&mut record, raw, checked_at);
        }
        if record.get("cursor_usage_raw").is_none() {
            if let Some(raw) = self.legacy_usage_raw(&account.id)? {
                update_export_usage(&mut record, raw, account.updated_at);
            }
        }
        Ok(record)
    }

    pub(crate) fn require_codex_api_key(&self, id: &str) -> Result<(Account, Session)> {
        let account = self.account(id)?;
        if !matches!(
            account.application,
            ApplicationKind::Codex | ApplicationKind::Grok
        ) || account.import_type != ImportType::ApiKey
        {
            return Err(AppError::Message("不是 API Key 账号".into()));
        }
        let session = self.load_session(id)?;
        Ok((account, session))
    }

    pub(crate) fn codex_api_key_account(
        &self,
        id: &str,
    ) -> Result<(String, String, Option<String>)> {
        let (account, session) = self.require_codex_api_key(id)?;
        let key = match account.application {
            ApplicationKind::Grok => {
                crate::grok::session::api_key(&crate::grok::session::auth_value(&session)?)
            }
            _ => crate::codex::session::api_key(&crate::codex::session::auth_value(&session)?),
        }
        .ok_or(AppError::SecretMissing)?;
        let base_url = match account.application {
            ApplicationKind::Grok => crate::grok::session::base_url(&session),
            _ => crate::codex::session::base_url(&session),
        };
        Ok((account.label, key, base_url))
    }

    pub(crate) fn update_codex_api_key(
        &mut self,
        id: &str,
        api_key: &str,
        base_url: Option<&str>,
        label: Option<&str>,
    ) -> Result<Account> {
        let (mut account, _) = self.require_codex_api_key(id)?;
        let session = match account.application {
            ApplicationKind::Grok => crate::grok::session::session_from_auth(
                crate::grok::session::api_key_auth_json(api_key.trim()),
                base_url
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            )?,
            _ => crate::codex::session::session_from_auth(
                crate::codex::session::api_key_auth_json(api_key.trim()),
                base_url
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            )?,
        };
        let is_current = self
            .accounts(account.application)
            .iter()
            .any(|item| item.id == id && item.is_current);
        let now = now();
        if let Some(label) = label.map(str::trim).filter(|value| !value.is_empty()) {
            account.label = label.to_owned();
        }
        account.updated_at = now;
        account.raw_export = session
            .raw_export
            .clone()
            .unwrap_or(serde_json::Value::Null);
        let transaction = self.database.transaction()?;
        transaction.execute(
            "UPDATE accounts SET label=?1, import_type=?2, raw_export_json=?3, updated_at=?4 WHERE id=?5",
            params![
                account.label,
                Self::import_type_value(&ImportType::ApiKey),
                serde_json::to_string(&account.raw_export)?,
                now as i64,
                id
            ],
        )?;
        transaction.execute(
            "INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2) ON CONFLICT(account_id) DO UPDATE SET session_json=excluded.session_json",
            params![id, serde_json::to_string(&session)?],
        )?;
        transaction.commit()?;
        if is_current {
            self.apply_account(id)?;
        }
        self.account(id)
    }

    pub(crate) fn duplicate_codex_api_key(&mut self, id: &str) -> Result<Account> {
        let (source, session) = self.require_codex_api_key(id)?;
        let now = now();
        let new_id = format!(
            "acc_{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let account = Account {
            id: new_id.clone(),
            application: source.application,
            label: format!("{} copy", source.label),
            email: source.email.clone(),
            import_type: ImportType::ApiKey,
            subscription: source.subscription.clone(),
            raw_export: raw_export_from_session(
                &session,
                &new_id,
                now,
                now,
                now,
                self.cursor.telemetry(),
            ),
            created_at: now,
            updated_at: now,
            last_used_at: now,
        };
        let transaction = self.database.transaction()?;
        let sort_order: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM accounts WHERE application=?1",
            params![Self::kind_value(source.application)],
            |row| row.get(0),
        )?;
        transaction.execute("INSERT INTO accounts (id, application, label, email, import_type, subscription_json, usage_json, usage_raw_json, raw_export_json, created_at, updated_at, last_used_at, sort_order) VALUES (?1,?2,?3,?4,?5,?6,NULL,NULL,?7,?8,?9,?10,?11)", params![account.id, Self::kind_value(account.application), account.label, account.email, Self::import_type_value(&account.import_type), serde_json::to_string(&account.subscription)?, serde_json::to_string(&account.raw_export)?, account.created_at as i64, account.updated_at as i64, account.last_used_at as i64, sort_order])?;
        transaction.execute(
            "INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2)",
            params![account.id, serde_json::to_string(&session)?],
        )?;
        transaction.commit()?;
        Ok(account)
    }

    pub(crate) fn export_account(&self, account: &Account) -> Result<serde_json::Value> {
        if account.import_type == ImportType::ApiKey {
            return Err(AppError::Message("API Key 账号不支持导出".into()));
        }
        if account.application == ApplicationKind::Codex {
            if account.raw_export.is_object() {
                return Ok(account.raw_export.clone());
            }
            let session = self.load_session(&account.id)?;
            if let Some(raw) = session.raw_export.clone() {
                return Ok(raw);
            }
            return Ok(serde_json::json!({
                "auth": crate::codex::session::auth_value(&session)?,
                "base_url": crate::codex::session::base_url(&session),
            }));
        }
        if account.application == ApplicationKind::Grok {
            if account.raw_export.is_object() {
                return Ok(account.raw_export.clone());
            }
            let session = self.load_session(&account.id)?;
            if let Some(raw) = session.raw_export.clone() {
                return Ok(raw);
            }
            return Ok(serde_json::json!({
                "auth": crate::grok::session::auth_value(&session)?,
                "base_url": crate::grok::session::base_url(&session),
            }));
        }
        self.export_cursor_account(account)
    }

    pub(crate) fn export_accounts(&self, kind: ApplicationKind, file: PathBuf) -> Result<()> {
        let parent = file
            .parent()
            .ok_or_else(|| AppError::Message("导出路径无效".into()))?;
        fs::create_dir_all(parent)?;
        let accounts = self
            .all_accounts()?
            .into_iter()
            .filter(|account| {
                account.application == kind && account.import_type != ImportType::ApiKey
            })
            .map(|account| self.export_account(&account))
            .collect::<Result<Vec<_>>>()?;
        let temporary = file.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&accounts)?)?;
        fs::rename(temporary, file)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn export_cursor_accounts(&self, file: PathBuf) -> Result<()> {
        self.export_accounts(ApplicationKind::Cursor, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::CursorAdapter;
    use crate::models::{now, ACCESS_TOKEN_KEY, EMAIL_KEY};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rusqlite::params;
    use std::{
        collections::BTreeMap,
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn token_import_labels_account_from_email_or_user_id() {
        let data_dir =
            env::temp_dir().join(format!("storm-dock-token-label-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let user_id = "user_01ABCDEFGHJKMNPQRSTVWXYZ";
        let token = "a".repeat(40);
        let account = controller
            .import_payload(
                ApplicationKind::Cursor,
                None,
                &format!("{user_id}::{token}"),
            )
            .unwrap();
        assert_eq!(account.label, user_id);
        assert_eq!(account.email, None);

        let claims = URL_SAFE_NO_PAD.encode(
            r#"{"sub":"auth0|user_01ABCDEFGHJKMNPQRSTVWXYZ","email":"me@example.com","exp":4102444800}"#,
        );
        let jwt = format!("header.{claims}.signature-padding-for-length");
        let account = controller
            .import_payload(ApplicationKind::Cursor, None, &format!("{user_id}::{jwt}"))
            .unwrap();
        assert_eq!(account.label, "me@example.com");
        assert_eq!(account.email.as_deref(), Some("me@example.com"));
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn reimport_without_a_plan_keeps_the_verified_subscription() {
        let data_dir =
            env::temp_dir().join(format!("storm-dock-reimport-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let account = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                None,
                Session {
                    values: BTreeMap::from([
                        (ACCESS_TOKEN_KEY.into(), "a".repeat(40)),
                        (EMAIL_KEY.into(), "me@example.com".into()),
                        (crate::models::MEMBERSHIP_TYPE_KEY.into(), "pro".into()),
                    ]),
                    raw_export: None,
                },
                ImportType::Jwt,
            )
            .unwrap();
        controller
            .save_imported_session(
                ApplicationKind::Cursor,
                None,
                Session {
                    values: BTreeMap::from([
                        (ACCESS_TOKEN_KEY.into(), "b".repeat(40)),
                        (EMAIL_KEY.into(), "me@example.com".into()),
                    ]),
                    raw_export: None,
                },
                ImportType::Jwt,
            )
            .unwrap();
        assert_eq!(
            controller
                .account(&account.id)
                .unwrap()
                .subscription
                .plan
                .as_deref(),
            Some("pro")
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn account_summaries_do_not_include_session_values() {
        let data_dir = env::temp_dir().join(format!("storm-dock-summary-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let session = Session {
            values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "secret-token".into())]),
            raw_export: None,
        };
        controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Test".into()),
                session,
                ImportType::Token,
            )
            .unwrap();
        let json = serde_json::to_string(&controller.accounts(ApplicationKind::Cursor)).unwrap();
        assert!(!json.contains("secret-token"));
        let _ = fs::remove_dir_all(data_dir);
    }
    #[test]
    fn account_order_is_preserved() {
        let data_dir = env::temp_dir().join(format!("storm-dock-order-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let first = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("First".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "first-token".into())]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let second = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Second".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "second-token".into())]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let second_id = second.id.clone();
        controller
            .reorder_accounts(ApplicationKind::Cursor, vec![second_id.clone(), first.id])
            .unwrap();
        assert_eq!(
            controller.accounts(ApplicationKind::Cursor)[0].id,
            second_id
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn database_move_preserves_accounts_and_sessions() {
        let source = env::temp_dir().join(format!(
            "storm-dock-source-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let destination = env::temp_dir().join(format!(
            "storm-dock-destination-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&destination).unwrap();
        let mut controller = Controller::new(source.clone()).unwrap();
        let account = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Test".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "secret-token".into())]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let path = controller.move_database(destination.clone()).unwrap();
        assert_eq!(PathBuf::from(path), destination.join(DATABASE_NAME));
        assert_eq!(
            controller
                .load_session(&account.id)
                .unwrap()
                .values
                .get(ACCESS_TOKEN_KEY),
            Some(&"secret-token".into())
        );
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn database_export_import_roundtrip_replaces_accounts() {
        let source_dir = env::temp_dir().join(format!(
            "storm-dock-export-src-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dest_dir = env::temp_dir().join(format!(
            "storm-dock-export-dst-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut source = Controller::new(source_dir.clone()).unwrap();
        let account = source
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Backup".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "backup-token".into())]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let dump = dest_dir.join("backup.sql");
        source.export_database(dump.clone()).unwrap();
        let sql = fs::read_to_string(&dump).unwrap();
        assert!(sql.starts_with("-- Storm Dock SQLite 导出"));
        assert!(sql.contains("INSERT INTO \"accounts\""));
        let mut dest = Controller::new(dest_dir.clone()).unwrap();
        dest.import_database(dump).unwrap();
        assert_eq!(dest.accounts(ApplicationKind::Cursor)[0].id, account.id);
        assert_eq!(
            dest.load_session(&account.id)
                .unwrap()
                .values
                .get(ACCESS_TOKEN_KEY),
            Some(&"backup-token".into())
        );
        let _ = fs::remove_dir_all(source_dir);
        let _ = fs::remove_dir_all(dest_dir);
    }

    #[test]
    fn database_import_rejects_non_storm_dock_file() {
        let data_dir = env::temp_dir().join(format!(
            "storm-dock-import-reject-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let junk = data_dir.join("not-a-db.txt");
        fs::write(&junk, "hello").unwrap();
        assert!(controller.import_database(junk).is_err());
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn cc_switch_sql_import_adds_codex_and_keeps_cursor() {
        let data_dir = env::temp_dir().join(format!(
            "storm-dock-cc-switch-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let cursor = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Keep Me".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "cursor-token".into())]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let dump = data_dir.join("cc-switch.sql");
        fs::write(
            &dump,
            r#"-- CC Switch SQLite 导出
CREATE TABLE providers (
  id TEXT NOT NULL,
  app_type TEXT NOT NULL,
  name TEXT NOT NULL,
  settings_config TEXT NOT NULL,
  meta TEXT NOT NULL DEFAULT '{}',
  is_current BOOLEAN NOT NULL DEFAULT 0,
  in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
  PRIMARY KEY (id, app_type)
);
INSERT INTO providers (id, app_type, name, settings_config, meta, is_current, in_failover_queue) VALUES
('official', 'codex', 'OpenAI Official', '{"auth":{},"config":""}', '{}', 0, 0),
('key', 'codex', 'Third Party', '{"auth":{"OPENAI_API_KEY":"sk-imported"},"config":"[model_providers.custom]\nbase_url = \"https://example.com/v1\""}', '{}', 1, 0),
('claude', 'claude', 'Claude', '{"env":{"ANTHROPIC_API_KEY":"sk-ant"}}', '{}', 0, 0);
"#,
        )
        .unwrap();
        controller.import_database(dump).unwrap();
        assert_eq!(
            controller.accounts(ApplicationKind::Cursor)[0].id,
            cursor.id
        );
        let codex = controller.accounts(ApplicationKind::Codex);
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].label, "Third Party");
        let session = controller.load_session(&codex[0].id).unwrap();
        assert_eq!(
            crate::codex::session::api_key(&crate::codex::session::auth_value(&session).unwrap())
                .as_deref(),
            Some("sk-imported")
        );
        assert_eq!(
            crate::codex::session::base_url(&session).as_deref(),
            Some("https://example.com/v1")
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn export_preserves_cursor_raw_record_without_frontend_serialization() {
        let data_dir = env::temp_dir().join(format!(
            "storm-dock-export-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        controller.cursor = CursorAdapter { database: None };
        let token = "a".repeat(40);
        let source = format!(
            r#"[{{"id":"cursor_source","access_token":"{token}","auth_id":"auth0|user_1","email":"me@example.com","cursor_auth_raw":{{"accessToken":"{token}","custom":true}},"cursor_usage_raw":{{"total_input_tokens":123}},"telemetry_machine_ids":{{"machineId":"source-machine"}}}}]"#
        );
        controller
            .import_payload(ApplicationKind::Cursor, None, &source)
            .unwrap();
        let file = data_dir.join("cursor-accounts.json");
        controller.export_cursor_accounts(file.clone()).unwrap();
        let exported: serde_json::Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        let account = &exported[0];
        assert_eq!(account["id"], "cursor_source");
        assert_eq!(account["access_token"], token);
        assert_eq!(account["cursor_auth_raw"]["custom"], true);
        assert_eq!(account["cursor_usage_raw"]["total_input_tokens"], 123);
        assert_eq!(
            account["telemetry_machine_ids"]["machineId"],
            "source-machine"
        );
        let _ = fs::remove_dir_all(data_dir);
    }
    #[test]
    fn database_migration_creates_raw_export_from_saved_session() {
        let data_dir = env::temp_dir().join(format!("storm-dock-raw-migration-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let account = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                None,
                Session {
                    values: BTreeMap::from([
                        (ACCESS_TOKEN_KEY.into(), "a".repeat(40)),
                        (EMAIL_KEY.into(), "me@example.com".into()),
                    ]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        controller.database.execute("UPDATE accounts SET usage_raw_json=?1 WHERE id=?2", params![r#"{"membershipType":"enterprise","billingCycleEnd":"2026-08-27T00:00:00.000Z"}"#, account.id]).unwrap();
        controller
            .database
            .execute(
                "UPDATE accounts SET raw_export_json=NULL WHERE id=?1",
                params![account.id],
            )
            .unwrap();
        drop(controller);
        let controller = Controller::new(data_dir.clone()).unwrap();
        let exported = controller
            .export_cursor_account(&controller.account(&account.id).unwrap())
            .unwrap();
        assert_eq!(exported["access_token"], "a".repeat(40));
        assert_eq!(exported["cursor_auth_raw"]["cachedEmail"], "me@example.com");
        assert_eq!(exported["cursor_usage_raw"]["membershipType"], "enterprise");
        let _ = fs::remove_dir_all(data_dir);
    }
    #[test]
    fn token_and_jwt_accounts_cannot_switch_desktop() {
        let data_dir =
            env::temp_dir().join(format!("storm-dock-token-switch-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let token = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Token".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a".repeat(40))]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let jwt = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Jwt".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a.b.c".into())]),
                    raw_export: None,
                },
                ImportType::Jwt,
            )
            .unwrap();

        for account in [&token, &jwt] {
            let error = controller
                .switch_account(&account.id, |_, _| {})
                .unwrap_err();
            assert!(error.to_string().contains("只能查询用量"), "{error}");
        }
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn cursor_switch_skips_progress_when_restart_is_required() {
        let data_dir = env::temp_dir().join(format!(
            "storm-dock-switch-progress-{}",
            uuid::Uuid::new_v4()
        ));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        controller.cursor = CursorAdapter { database: None };
        let account = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Native".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a".repeat(40))]),
                    raw_export: None,
                },
                ImportType::Native,
            )
            .unwrap();

        let mut stages = Vec::new();
        let result = controller.switch_account(&account.id, |stage, _| {
            stages.push(stage);
        });
        match result {
            Ok(outcome) => {
                assert!(outcome.restart_required);
                assert!(stages.is_empty(), "{stages:?}");
            }
            Err(_) => {
                assert_eq!(stages, ["loading", "applying"]);
            }
        }
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn usage_refresh_persists_billing_cycle_end_on_the_account() {
        let data_dir = env::temp_dir().join(format!("storm-dock-billing-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let account = controller
            .save_imported_session(
                ApplicationKind::Cursor,
                Some("Test".into()),
                Session {
                    values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a".repeat(40))]),
                    raw_export: None,
                },
                ImportType::Token,
            )
            .unwrap();
        let usage = CursorUsageDetails {
            account_id: account.id.clone(),
            label: account.label.clone(),
            email: account.email.clone(),
            name: None,
            membership_type: Some("pro".into()),
            primary: crate::cursor::usage::usage_metric("percent", 10.0, None),
            reset_at: Some("2026-08-27T00:00:00.000Z".into()),
            on_demand: None,
            grok_bot: None,
            grok_bot_reset_at: None,
            models: vec![],
            weekly: vec![],
            weekly_available: false,
            weekly_error: None,
            events: vec![],
            checked_at: 1,
        };
        controller
            .save_cursor_usage(
                &account.id,
                usage,
                serde_json::json!({
                    "usage_summary": {
                        "membershipType": "pro",
                        "billingCycleEnd": "2026-08-27T00:00:00.000Z"
                    }
                }),
            )
            .unwrap();
        let saved = controller.account(&account.id).unwrap();
        assert_eq!(
            saved.subscription.billing_cycle_end.as_deref(),
            Some("2026-08-27T00:00:00.000Z")
        );
        assert!(saved.subscription.expires_at.is_some());
        assert_eq!(saved.subscription.plan.as_deref(), Some("pro"));
        assert!(controller.accounts(ApplicationKind::Cursor)[0]
            .days_remaining
            .is_some());

        controller
            .save_subscription(
                &account.id,
                SubscriptionSummary {
                    plan: Some("pro".into()),
                    checked_at: Some(2),
                    ..Default::default()
                },
            )
            .unwrap();
        let kept = controller.account(&account.id).unwrap();
        assert_eq!(
            kept.subscription.billing_cycle_end.as_deref(),
            Some("2026-08-27T00:00:00.000Z")
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn codex_api_key_import_dedupes_by_key_and_base_url() {
        let data_dir =
            env::temp_dir().join(format!("storm-dock-codex-key-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let first = controller
            .import_payload(ApplicationKind::Codex, Some("One".into()), "sk-shared")
            .unwrap();
        let again = controller
            .import_payload(ApplicationKind::Codex, None, "sk-shared")
            .unwrap();
        assert_eq!(first.id, again.id);
        let custom = controller
            .import_payload(
                ApplicationKind::Codex,
                None,
                r#"{"OPENAI_API_KEY":"sk-shared","base_url":"https://api.example.com/v1"}"#,
            )
            .unwrap();
        assert_ne!(first.id, custom.id);
        assert_eq!(custom.import_type, ImportType::ApiKey);
        assert_eq!(controller.accounts(ApplicationKind::Codex).len(), 2);
        controller
            .update_codex_api_key(
                &custom.id,
                "sk-edited",
                Some("https://api.example.com/v1"),
                Some("Renamed"),
            )
            .unwrap();
        let (label, key, base_url) = controller.codex_api_key_account(&custom.id).unwrap();
        assert_eq!(label, "Renamed");
        assert_eq!(key, "sk-edited");
        assert_eq!(base_url.as_deref(), Some("https://api.example.com/v1"));
        assert!(controller
            .export_account(&controller.account(&custom.id).unwrap())
            .is_err());
        let copy = controller.duplicate_codex_api_key(&custom.id).unwrap();
        assert_ne!(copy.id, custom.id);
        assert_eq!(copy.label, "Renamed copy");
        let (_, key, base_url) = controller.codex_api_key_account(&copy.id).unwrap();
        assert_eq!(key, "sk-edited");
        assert_eq!(base_url.as_deref(), Some("https://api.example.com/v1"));
        assert_eq!(controller.accounts(ApplicationKind::Codex).len(), 3);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn preserve_codex_official_auth_defaults_on_and_persists() {
        let data_dir =
            env::temp_dir().join(format!("storm-dock-preserve-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        assert!(controller.preserve_codex_official_auth());
        controller.set_preserve_codex_official_auth(false).unwrap();
        assert!(!controller.preserve_codex_official_auth());
        drop(controller);
        let controller = Controller::new(data_dir.clone()).unwrap();
        assert!(!controller.preserve_codex_official_auth());
        let _ = fs::remove_dir_all(data_dir);
    }

}
