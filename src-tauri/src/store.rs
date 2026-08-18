use rusqlite::{params, Connection};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::apps::{ApplicationAdapter, CodexAdapter, CursorAdapter};
use crate::cursor::session::{raw_export_from_session, session_display_label};
use crate::cursor::usage::{cursor_usage_from_snapshot, update_export_usage, usage_pools};
use crate::error::{AppError, Result};
use crate::models::{
    days_remaining, matching_account_index, now, subscription_from_session, Account,
    AccountSummary, ApplicationKind, ApplicationStatus, CursorUsageDetails, ImportType, Session,
    SubscriptionSummary, SwitchOutcome, EMAIL_KEY,
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
            codex: CodexAdapter,
        };
        controller.migrate_raw_exports()?;
        Ok(controller)
    }

    pub(crate) fn open_database(path: &std::path::Path) -> Result<Connection> {
        let parent = path.parent().ok_or_else(|| AppError::Message("数据库路径无效".into()))?;
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
        Ok(database)
    }

    pub(crate) fn kind_value(kind: ApplicationKind) -> &'static str {
        match kind { ApplicationKind::Cursor => "cursor", ApplicationKind::Codex => "codex" }
    }

    pub(crate) fn import_type_value(import_type: &ImportType) -> &'static str {
        match import_type { ImportType::OAuth => "oauth", ImportType::Token => "token", ImportType::Jwt => "jwt", ImportType::Native => "native" }
    }

    pub(crate) fn import_type_from(value: &str) -> Result<ImportType> {
        match value { "oauth" => Ok(ImportType::OAuth), "token" => Ok(ImportType::Token), "jwt" => Ok(ImportType::Jwt), "native" => Ok(ImportType::Native), _ => Err(AppError::Message("数据库中的导入类型无效".into())) }
    }

    pub(crate) fn all_accounts(&self) -> Result<Vec<Account>> {
        let mut statement = self.database.prepare("SELECT id, application, label, email, import_type, subscription_json, usage_json, usage_raw_json, raw_export_json, created_at, updated_at, last_used_at FROM accounts ORDER BY application, sort_order")?;
        let mut rows = statement.query([])?;
        let mut accounts = Vec::new();
        while let Some(row) = rows.next()? {
            let application = match row.get::<_, String>(1)?.as_str() { "cursor" => ApplicationKind::Cursor, "codex" => ApplicationKind::Codex, _ => return Err(AppError::Message("数据库中的应用类型无效".into())) };
            accounts.push(Account {
                id: row.get(0)?, application, label: row.get(2)?, email: row.get(3)?,
                import_type: Self::import_type_from(&row.get::<_, String>(4)?)?,
                subscription: serde_json::from_str(&row.get::<_, String>(5)?)?,
                raw_export: row.get::<_, Option<String>>(8)?.map(|json| serde_json::from_str(&json)).transpose()?.unwrap_or(serde_json::Value::Null),
                created_at: row.get(9)?, updated_at: row.get(10)?, last_used_at: row.get(11)?,
            });
        }
        Ok(accounts)
    }

    pub(crate) fn account(&self, id: &str) -> Result<Account> {
        self.all_accounts()?.into_iter().find(|account| account.id == id).ok_or(AppError::AccountNotFound)
    }

    pub(crate) fn load_session(&self, id: &str) -> Result<Session> {
        let mut statement = self.database.prepare("SELECT session_json FROM sessions WHERE account_id = ?1")?;
        let mut rows = statement.query(params![id])?;
        let Some(row) = rows.next()? else { return Err(AppError::SecretMissing); };
        Ok(serde_json::from_str(&row.get::<_, String>(0)?)?)
    }

    pub(crate) fn legacy_usage_raw(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let json: Option<String> = self.database.query_row(
            "SELECT usage_raw_json FROM accounts WHERE id=?1",
            params![id],
            |row| row.get(0),
        )?;
        json.map(|json| serde_json::from_str(&json)).transpose().map_err(Into::into)
    }

    pub(crate) fn migrate_raw_exports(&mut self) -> Result<()> {
        for account in self.all_accounts()? {
            let mut raw = account.raw_export;
            let mut changed = false;
            if raw.is_null() {
                raw = raw_export_from_session(&self.load_session(&account.id)?, &account.id, account.created_at, account.updated_at, account.last_used_at, self.cursor.telemetry());
                changed = true;
            }
            if raw.get("cursor_usage_raw").is_none() {
                if let Some(usage) = self.legacy_usage_raw(&account.id)? {
                    update_export_usage(&mut raw, usage, account.updated_at);
                    changed = true;
                }
            }
            if changed {
                self.database.execute("UPDATE accounts SET raw_export_json=?1 WHERE id=?2", params![serde_json::to_string(&raw)?, account.id])?;
            }
        }
        Ok(())
    }

    pub(crate) fn adapter(&self, kind: ApplicationKind) -> &dyn ApplicationAdapter {
        match kind {
            ApplicationKind::Cursor => &self.cursor,
            ApplicationKind::Codex => &self.codex,
        }
    }

    pub(crate) fn statuses(&self) -> Vec<ApplicationStatus> {
        vec![self.cursor.detect(), self.codex.detect()]
    }

    pub(crate) fn accounts(&self, kind: ApplicationKind) -> Vec<AccountSummary> {
        let current: Option<String> = self.database.query_row("SELECT current_account_id FROM application_state WHERE application = ?1", params![Self::kind_value(kind)], |row| row.get(0)).ok();
        self.all_accounts().unwrap_or_default().into_iter()
            .filter(|account| account.application == kind)
            .map(|account| AccountSummary {
                is_current: current.as_deref() == Some(&account.id),
                id: account.id.clone(),
                label: account.label.clone(),
                email: account.email.clone(),
                import_type: account.import_type.clone(),
                subscription: account.subscription.clone(),
                days_remaining: account
                    .subscription
                    .reset_timestamp(&account.raw_export)
                    .map(|expires_at| days_remaining(expires_at, now())),
            })
            .collect()
    }

    pub(crate) fn reorder_accounts(&mut self, kind: ApplicationKind, ids: Vec<String>) -> Result<()> {
        let accounts = self.all_accounts()?;
        let existing: Vec<_> = accounts.iter()
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
            transaction.execute("UPDATE accounts SET sort_order = ?1 WHERE id = ?2", params![position as i64, id])?;
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
            .get(EMAIL_KEY)
            .cloned()
            .filter(|value| !value.is_empty());
        let display_label = session_display_label(&session);
        if let Some(email) = email.as_deref() {
            if let Some(index) = matching_account_index(&self.all_accounts()?, kind, email) {
                let mut account = self.all_accounts()?[index].clone();
                if let Some(label) = label.filter(|value| !value.trim().is_empty()) {
                    account.label = label;
                } else if let Some(display_label) = display_label.clone() {
                    account.label = display_label;
                }
                account.email = Some(email.to_owned());
                account.import_type = import_type;
                account.subscription = subscription_from_session(&session);
                account.updated_at = now;
                account.last_used_at = now;
                account.raw_export = raw_export_from_session(&session, &account.id, account.created_at, now, now, self.cursor.telemetry());
                let transaction = self.database.transaction()?;
                transaction.execute("UPDATE accounts SET label=?1, email=?2, import_type=?3, subscription_json=?4, raw_export_json=?5, updated_at=?6, last_used_at=?7 WHERE id=?8", params![account.label, account.email, Self::import_type_value(&account.import_type), serde_json::to_string(&account.subscription)?, serde_json::to_string(&account.raw_export)?, account.updated_at as i64, account.last_used_at as i64, account.id])?;
                transaction.execute("INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2) ON CONFLICT(account_id) DO UPDATE SET session_json=excluded.session_json", params![account.id, serde_json::to_string(&session)?])?;
                transaction.commit()?;
                return Ok(account);
            }
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
            raw_export: raw_export_from_session(&session, &id, now, now, now, self.cursor.telemetry()),
            created_at: now,
            updated_at: now,
            last_used_at: now,
        };
        let transaction = self.database.transaction()?;
        let sort_order: i64 = transaction.query_row("SELECT COUNT(*) FROM accounts WHERE application=?1", params![Self::kind_value(kind)], |row| row.get(0))?;
        transaction.execute("INSERT INTO accounts (id, application, label, email, import_type, subscription_json, usage_json, usage_raw_json, raw_export_json, created_at, updated_at, last_used_at, sort_order) VALUES (?1,?2,?3,?4,?5,?6,NULL,NULL,?7,?8,?9,?10,?11)", params![account.id, Self::kind_value(kind), account.label, account.email, Self::import_type_value(&account.import_type), serde_json::to_string(&account.subscription)?, serde_json::to_string(&account.raw_export)?, account.created_at as i64, account.updated_at as i64, account.last_used_at as i64, sort_order])?;
        transaction.execute("INSERT INTO sessions (account_id, session_json) VALUES (?1, ?2)", params![account.id, serde_json::to_string(&session)?])?;
        transaction.commit()?;
        Ok(account)
    }

    pub(crate) fn import_current(&mut self, kind: ApplicationKind, label: Option<String>) -> Result<Account> {
        let session = self.adapter(kind).import_current()?;
        self.save_imported_session(kind, label, session, ImportType::Native)
    }

    #[cfg(test)]
    pub(crate) fn import_payload(
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

    pub(crate) fn delete_account(&mut self, id: &str) -> Result<()> {
        self.account(id)?;
        let transaction = self.database.transaction()?;
        transaction.execute("DELETE FROM accounts WHERE id=?1", params![id])?;
        transaction.execute("DELETE FROM application_state WHERE current_account_id=?1", params![id])?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn subscription_session(&mut self, id: &str) -> Result<Session> {
        let account = self.account(id)?;
        if account.application != ApplicationKind::Cursor {
            return Err(AppError::ComingSoon);
        }
        self.load_session(&account.id)
    }

    pub(crate) fn save_subscription(&mut self, id: &str, summary: SubscriptionSummary) -> Result<()> {
        let mut account = self.account(id)?;
        account.subscription.merge_from(summary);
        account.updated_at = now();
        self.database.execute("UPDATE accounts SET subscription_json=?1, updated_at=?2 WHERE id=?3", params![serde_json::to_string(&account.subscription)?, account.updated_at as i64, id])?;
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

    pub(crate) fn save_cursor_usage(&mut self, id: &str, usage: CursorUsageDetails, raw: serde_json::Value) -> Result<()> {
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
            "UPDATE accounts SET usage_json=?1, raw_export_json=?2, subscription_json=?3, updated_at=?4 WHERE id=?5",
            params![
                serde_json::to_string(&usage)?,
                serde_json::to_string(&account.raw_export)?,
                serde_json::to_string(&account.subscription)?,
                now() as i64,
                id
            ],
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
        progress("loading", 15);
        let account = self.account(id)?;
        if !account.import_type.supports_desktop_switch() {
            return Err(AppError::Message(
                "Token / JWT 账户只能查询用量，不能切换登录 Cursor 桌面端。".into(),
            ));
        }
        let session = self.load_session(&account.id)?;
        let running = self.adapter(account.application).is_running();
        progress("applying", 45);
        self.adapter(account.application).apply(&session)?;
        progress("persisting", 75);
        let transaction = self.database.transaction()?;
        transaction.execute("INSERT INTO application_state (application, current_account_id) VALUES (?1, ?2) ON CONFLICT(application) DO UPDATE SET current_account_id=excluded.current_account_id", params![Self::kind_value(account.application), account.id])?;
        transaction.execute("UPDATE accounts SET last_used_at=?1 WHERE id=?2", params![now() as i64, id])?;
        transaction.commit()?;
        Ok(SwitchOutcome {
            restart_required: running,
        })
    }

    pub(crate) fn current_label(&self) -> String {
        self.database.query_row("SELECT a.label FROM accounts a JOIN application_state s ON a.id=s.current_account_id WHERE s.application='cursor'", [], |row| row.get::<_, String>(0)).ok()
            .unwrap_or_else(|| "未选择账户".into())
    }

    pub(crate) fn database_path(&self) -> String { self.database_path.display().to_string() }

    pub(crate) fn move_database(&mut self, directory: PathBuf) -> Result<String> {
        if !directory.is_dir() { return Err(AppError::Message("请选择有效的同步目录".into())); }
        let target = directory.join(DATABASE_NAME);
        if target == self.database_path { return Ok(self.database_path()); }
        if target.exists() { return Err(AppError::Message("目标目录已包含 storm-dock.db".into())); }
        let temporary = directory.join(format!(".{DATABASE_NAME}.tmp"));
        if temporary.exists() { fs::remove_file(&temporary)?; }
        self.database.execute_batch("PRAGMA optimize;")?;
        let source = self.database_path.clone();
        fs::copy(&source, &temporary)?;
        let check = Connection::open(&temporary)?;
        let integrity: String = check.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" { let _ = fs::remove_file(&temporary); return Err(AppError::Message("迁移后的数据库校验失败".into())); }
        drop(check);
        fs::rename(&temporary, &target)?;
        fs::write(&self.pointer_file, target.to_string_lossy().as_bytes())?;
        let database = Self::open_database(&target)?;
        self.database = database;
        self.database_path = target;
        let _ = fs::remove_file(source);
        Ok(self.database_path())
    }

    pub(crate) fn export_cursor_account(&self, account: &Account) -> Result<serde_json::Value> {
        if !account.raw_export.is_object() {
            return Err(AppError::Message("账户的原始导出数据无效".into()));
        }
        let mut record = account.raw_export.clone();
        if let Some(raw) = record.get("cursor_usage_sources").cloned() {
            let checked_at = record.get("usage_updated_at").and_then(serde_json::Value::as_u64).unwrap_or(account.updated_at);
            update_export_usage(&mut record, raw, checked_at);
        }
        if record.get("cursor_usage_raw").is_none() {
            if let Some(raw) = self.legacy_usage_raw(&account.id)? {
                update_export_usage(&mut record, raw, account.updated_at);
            }
        }
        Ok(record)
    }

    pub(crate) fn export_cursor_accounts(&self, file: PathBuf) -> Result<()> {
        let parent = file.parent().ok_or_else(|| AppError::Message("导出路径无效".into()))?;
        fs::create_dir_all(parent)?;
        let accounts = self.all_accounts()?.into_iter().filter(|account| account.application == ApplicationKind::Cursor).map(|account| self.export_cursor_account(&account)).collect::<Result<Vec<_>>>()?;
        let temporary = file.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&accounts)?)?;
        fs::rename(temporary, file)?;
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::CursorAdapter;
    use crate::models::{now, ACCESS_TOKEN_KEY, EMAIL_KEY};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rusqlite::params;
    use std::{collections::BTreeMap, env, fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn token_import_labels_account_from_email_or_user_id() {
        let data_dir = env::temp_dir().join(format!("storm-dock-token-label-{}", uuid::Uuid::new_v4()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let user_id = "user_01ABCDEFGHJKMNPQRSTVWXYZ";
        let token = "a".repeat(40);
        let account = controller
            .import_payload(ApplicationKind::Cursor, None, &format!("{user_id}::{token}"))
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
    fn account_summaries_do_not_include_session_values() {
        let data_dir = env::temp_dir().join(format!("storm-dock-summary-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let session = Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "secret-token".into())]), raw_export: None };
        controller.save_imported_session(ApplicationKind::Cursor, Some("Test".into()), session, ImportType::Token).unwrap();
        let json = serde_json::to_string(&controller.accounts(ApplicationKind::Cursor)).unwrap();
        assert!(!json.contains("secret-token"));
        let _ = fs::remove_dir_all(data_dir);
    }
    #[test]
    fn account_order_is_preserved() {
        let data_dir = env::temp_dir().join(format!("storm-dock-order-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let first = controller.save_imported_session(ApplicationKind::Cursor, Some("First".into()), Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "first-token".into())]), raw_export: None }, ImportType::Token).unwrap();
        let second = controller.save_imported_session(ApplicationKind::Cursor, Some("Second".into()), Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "second-token".into())]), raw_export: None }, ImportType::Token).unwrap();
        let second_id = second.id.clone();
        controller
            .reorder_accounts(
                ApplicationKind::Cursor,
                vec![second_id.clone(), first.id],
            )
            .unwrap();
        assert_eq!(controller.accounts(ApplicationKind::Cursor)[0].id, second_id);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn database_move_preserves_accounts_and_sessions() {
        let source = env::temp_dir().join(format!("storm-dock-source-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let destination = env::temp_dir().join(format!("storm-dock-destination-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&destination).unwrap();
        let mut controller = Controller::new(source.clone()).unwrap();
        let account = controller.save_imported_session(ApplicationKind::Cursor, Some("Test".into()), Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "secret-token".into())]), raw_export: None }, ImportType::Token).unwrap();
        let path = controller.move_database(destination.clone()).unwrap();
        assert_eq!(PathBuf::from(path), destination.join(DATABASE_NAME));
        assert_eq!(controller.load_session(&account.id).unwrap().values.get(ACCESS_TOKEN_KEY), Some(&"secret-token".into()));
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn export_preserves_cursor_raw_record_without_frontend_serialization() {
        let data_dir = env::temp_dir().join(format!("storm-dock-export-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        controller.cursor = CursorAdapter { database: None };
        let token = "a".repeat(40);
        let source = format!(r#"[{{"id":"cursor_source","access_token":"{token}","auth_id":"auth0|user_1","email":"me@example.com","cursor_auth_raw":{{"accessToken":"{token}","custom":true}},"cursor_usage_raw":{{"total_input_tokens":123}},"telemetry_machine_ids":{{"machineId":"source-machine"}}}}]"#);
        controller.import_payload(ApplicationKind::Cursor, None, &source).unwrap();
        let file = data_dir.join("cursor-accounts.json");
        controller.export_cursor_accounts(file.clone()).unwrap();
        let exported: serde_json::Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        let account = &exported[0];
        assert_eq!(account["id"], "cursor_source");
        assert_eq!(account["access_token"], token);
        assert_eq!(account["cursor_auth_raw"]["custom"], true);
        assert_eq!(account["cursor_usage_raw"]["total_input_tokens"], 123);
        assert_eq!(account["telemetry_machine_ids"]["machineId"], "source-machine");
        let _ = fs::remove_dir_all(data_dir);
    }
    #[test]
    fn database_migration_creates_raw_export_from_saved_session() {
        let data_dir = env::temp_dir().join(format!("storm-dock-raw-migration-{}", now()));
        let mut controller = Controller::new(data_dir.clone()).unwrap();
        let account = controller.save_imported_session(ApplicationKind::Cursor, None, Session { values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "a".repeat(40)), (EMAIL_KEY.into(), "me@example.com".into())]), raw_export: None }, ImportType::Token).unwrap();
        controller.database.execute("UPDATE accounts SET usage_raw_json=?1 WHERE id=?2", params![r#"{"membershipType":"enterprise","billingCycleEnd":"2026-08-27T00:00:00.000Z"}"#, account.id]).unwrap();
        controller.database.execute("UPDATE accounts SET raw_export_json=NULL WHERE id=?1", params![account.id]).unwrap();
        drop(controller);
        let controller = Controller::new(data_dir.clone()).unwrap();
        let exported = controller.export_cursor_account(&controller.account(&account.id).unwrap()).unwrap();
        assert_eq!(exported["access_token"], "a".repeat(40));
        assert_eq!(exported["cursor_auth_raw"]["cachedEmail"], "me@example.com");
        assert_eq!(exported["cursor_usage_raw"]["membershipType"], "enterprise");
        let _ = fs::remove_dir_all(data_dir);
    }
    #[test]
    fn token_and_jwt_accounts_cannot_switch_desktop() {
        let data_dir = env::temp_dir().join(format!("storm-dock-token-switch-{}", uuid::Uuid::new_v4()));
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
            let error = controller.switch_account(&account.id, |_, _| {}).unwrap_err();
            assert!(
                error.to_string().contains("只能查询用量"),
                "{error}"
            );
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

}
