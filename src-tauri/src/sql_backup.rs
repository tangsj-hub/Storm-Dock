use rusqlite::{types::ValueRef, Connection};
use std::{fs, path::Path};

use crate::codex::session::{has_login_material, session_from_auth};
use crate::error::{AppError, Result};
use crate::models::Session;

pub(crate) const STORM_DOCK_SQL_HEADER: &str = "-- Storm Dock SQLite 导出";
pub(crate) const CC_SWITCH_SQL_HEADER: &str = "-- CC Switch SQLite 导出";

const DUMP_TABLES: &[&str] = &[
    "accounts",
    "sessions",
    "application_state",
    "app_kv",
    "local_models",
];

pub(crate) enum BackupKind {
    StormDockSql,
    CcSwitchSql,
}

pub(crate) fn sniff(path: &Path) -> Result<BackupKind> {
    let text = fs::read_to_string(path).map_err(|_| {
        AppError::Message("仅支持 Storm Dock 或 CC Switch 导出的 SQL 备份。".into())
    })?;
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with(STORM_DOCK_SQL_HEADER) {
        return Ok(BackupKind::StormDockSql);
    }
    if trimmed.starts_with(CC_SWITCH_SQL_HEADER) {
        return Ok(BackupKind::CcSwitchSql);
    }
    Err(AppError::Message(
        "仅支持 Storm Dock 或 CC Switch 导出的 SQL 备份。".into(),
    ))
}

pub(crate) fn dump_sql(conn: &Connection) -> Result<String> {
    let mut output = String::from(STORM_DOCK_SQL_HEADER);
    output.push_str("\nPRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\n");
    for table in DUMP_TABLES {
        dump_table(conn, table, &mut output)?;
    }
    output.push_str("COMMIT;\nPRAGMA foreign_keys=ON;\n");
    Ok(output)
}

pub(crate) fn load_sql(conn: &Connection, sql: &str) -> Result<()> {
    let sql = sql.trim_start_matches('\u{feff}');
    conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
    conn.execute_batch(sql)
        .map_err(|error| AppError::Message(format!("执行 SQL 导入失败: {error}")))?;
    if !conn.is_autocommit() {
        let _ = conn.execute_batch("ROLLBACK;");
        return Err(AppError::Message(
            "SQL 备份事务未完成，文件可能已截断。".into(),
        ));
    }
    Ok(())
}

pub(crate) fn cc_switch_codex_sessions(conn: &Connection) -> Result<Vec<(String, Session)>> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='providers'",
        [],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(AppError::Message("不是有效的 CC Switch SQL 备份。".into()));
    }
    let mut statement =
        conn.prepare("SELECT name, settings_config FROM providers WHERE app_type='codex'")?;
    let mut rows = statement.query([])?;
    let mut sessions = Vec::new();
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        let settings: String = row.get(1)?;
        if let Some(session) = session_from_cc_provider(&settings)? {
            sessions.push((name, session));
        }
    }
    Ok(sessions)
}

fn session_from_cc_provider(settings: &str) -> Result<Option<Session>> {
    let value: serde_json::Value = serde_json::from_str(settings)?;
    let auth = value.get("auth").cloned().filter(|item| item.is_object());
    let Some(auth) = auth else {
        return Ok(None);
    };
    if !has_login_material(&auth) {
        return Ok(None);
    }
    Ok(Some(session_from_auth(
        auth,
        base_url_from_cc_config(&value),
    )?))
}

fn base_url_from_cc_config(value: &serde_json::Value) -> Option<String> {
    let text = value.get("config")?.as_str()?;
    let parsed: toml::Value = toml::from_str(text).ok()?;
    parsed
        .get("model_providers")?
        .get("custom")?
        .get("base_url")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn dump_table(conn: &Connection, table: &str, output: &mut String) -> Result<()> {
    let sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |row| row.get(0),
    )?;
    output.push_str(&sql);
    output.push_str(";\n");
    let columns = table_columns(conn, table)?;
    if columns.is_empty() {
        return Ok(());
    }
    let quoted_columns = columns
        .iter()
        .map(|column| quote_ident(column))
        .collect::<Vec<_>>()
        .join(", ");
    let mut statement = conn.prepare(&format!(
        "SELECT {quoted_columns} FROM {}",
        quote_ident(table)
    ))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            values.push(sql_literal(row.get_ref(index)?)?);
        }
        output.push_str(&format!(
            "INSERT INTO {} ({quoted_columns}) VALUES ({});\n",
            quote_ident(table),
            values.join(", ")
        ));
    }
    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({})", quote_ident(table)))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(columns)
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sql_literal(value: ValueRef<'_>) -> Result<String> {
    match value {
        ValueRef::Null => Ok("NULL".into()),
        ValueRef::Integer(value) => Ok(value.to_string()),
        ValueRef::Real(value) => Ok(value.to_string()),
        ValueRef::Text(bytes) => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| AppError::Message("SQL 导出遇到非 UTF-8 文本".into()))?;
            Ok(format!("'{}'", text.replace('\'', "''")))
        }
        ValueRef::Blob(_) => Err(AppError::Message("SQL 导出不支持二进制字段".into())),
    }
}
