use rusqlite::{params, Connection};
use std::{
    collections::BTreeMap,
    env, fs,
    path::PathBuf,
    time::Duration,
};

use crate::apps::ApplicationAdapter;
use crate::error::{AppError, Result};
use crate::models::{
    ApplicationKind, ApplicationStatus, Session, ACCESS_TOKEN_KEY, CURSOR_KEYS,
};

pub(crate) struct CursorAdapter {
    pub(crate) database: Option<PathBuf>,
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

    fn read_session_unchecked(&self, db: &Connection) -> Result<Session> {
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
        Ok(Session {
            values,
            raw_export: None,
        })
    }

    fn read_session(&self, db: &Connection) -> Result<Session> {
        let session = self.read_session_unchecked(db)?;
        if !session.values.contains_key(ACCESS_TOKEN_KEY) {
            return Err(AppError::UnsupportedCursor(
                "access token is missing".into(),
            ));
        }
        Ok(session)
    }

    fn write_session(&self, db: &mut Connection, session: &Session) -> Result<()> {
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

    pub(crate) fn telemetry(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut values = serde_json::Map::new();
        let Some(database_path) = &self.database else {
            return values;
        };
        let storage = database_path.parent().map(|path| path.join("storage.json"));
        if let Some(storage) = storage {
            if let Ok(storage) =
                serde_json::from_slice::<serde_json::Value>(&fs::read(storage).unwrap_or_default())
            {
                for (source, target) in [
                    ("telemetry.devDeviceId", "devDeviceId"),
                    ("telemetry.macMachineId", "macMachineId"),
                    ("telemetry.machineId", "machineId"),
                    ("telemetry.sqmId", "sqmId"),
                ] {
                    if let Some(value) = storage.get(source) {
                        values.insert(target.into(), value.clone());
                    }
                }
            }
        }
        if let Ok(database) = self.open() {
            for (source, target) in [
                ("storage.serviceMachineId", "serviceMachineId"),
                ("telemetry.firstSessionDate", "firstSessionDate"),
            ] {
                if let Ok(value) = database.query_row(
                    "SELECT value FROM ItemTable WHERE key=?1",
                    [source],
                    |row| row.get::<_, String>(0),
                ) {
                    values.insert(target.into(), serde_json::Value::String(value));
                }
            }
        }
        values
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
        if !session.values.contains_key(ACCESS_TOKEN_KEY) {
            return Err(AppError::SecretMissing);
        }
        let mut db = self.open()?;
        let before = self.read_session_unchecked(&db)?;
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

pub(crate) fn launch_cursor() -> Result<()> {
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

pub(crate) fn terminate_cursor() -> Result<()> {
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

pub(crate) fn wait_for_cursor_stop() -> Result<()> {
    for _ in 0..50 {
        if !CursorAdapter::default().is_running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(AppError::Message("Cursor 未在 5 秒内退出。".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EMAIL_KEY;
    use rusqlite::params;
    use std::{env, fs};

    fn test_cursor_db() -> PathBuf {
        let path = env::temp_dir().join(format!("storm-dock-test-{}.vscdb", uuid::Uuid::new_v4()));
        let _ = fs::remove_file(&path);
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
            raw_export: None,
        };
        assert!(!session.values.contains_key(ACCESS_TOKEN_KEY));
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
            raw_export: None,
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

    #[test]
    fn cursor_adapter_switches_when_current_session_is_signed_out() {
        let path = test_cursor_db();
        let db = Connection::open(&path).unwrap();
        db.execute("DELETE FROM ItemTable WHERE key=?1", [ACCESS_TOKEN_KEY])
            .unwrap();
        drop(db);
        let adapter = CursorAdapter {
            database: Some(path.clone()),
        };
        let replacement = Session {
            values: BTreeMap::from([(ACCESS_TOKEN_KEY.into(), "replacement-token".into())]),
            raw_export: None,
        };

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
}
