use rusqlite::Connection;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::codex_sessions::{CodexSession, CodexSessionMessage};

const MAX_SESSIONS: usize = 1_000;
const MAX_MESSAGES: usize = 1_000;

#[derive(Default)]
struct CursorSessionMetadata {
    titles: HashMap<String, String>,
    archived_ids: HashSet<String>,
}

pub(crate) fn list_sessions() -> Vec<CodexSession> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let home = PathBuf::from(home);
    let root = home.join(".cursor").join("projects");
    let metadata = load_cursor_metadata(
        &home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb"),
    );
    let mut files = Vec::new();
    collect_transcripts(&root, &mut files);
    files.sort_by_key(|path| std::cmp::Reverse(modified_at(path)));
    files.truncate(MAX_SESSIONS);
    let mut sessions = files
        .into_iter()
        .filter_map(|path| parse_session(&path, &root, &metadata))
        .collect::<Vec<_>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}

pub(crate) fn load_messages(id: &str) -> Vec<CodexSessionMessage> {
    if !is_valid_id(id) {
        return Vec::new();
    }
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let root = PathBuf::from(home).join(".cursor").join("projects");
    let path = find_transcript(&root, id);
    path.map(|path| read_messages(&path)).unwrap_or_default()
}

pub(crate) fn delete_session(id: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err("无效的会话标识。".into());
    }
    let Some(home) = std::env::var_os("HOME") else {
        return Err("无法读取用户目录。".into());
    };
    let home = PathBuf::from(home);
    let root = home.join(".cursor").join("projects");
    delete_session_at(
        &root,
        &home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb"),
        &home.join("Library/Application Support/Cursor/User/globalStorage/conversation-search.db"),
        id,
    )
}

pub(crate) fn delete_sessions(ids: &[String]) -> (Vec<String>, Vec<String>) {
    let mut deleted = Vec::with_capacity(ids.len());
    let mut failed = Vec::new();
    for id in ids {
        match delete_session(id) {
            Ok(()) => deleted.push(id.clone()),
            Err(_) => failed.push(id.clone()),
        }
    }
    (deleted, failed)
}

fn delete_session_at(
    root: &Path,
    state_db: &Path,
    search_db: &Path,
    id: &str,
) -> Result<(), String> {
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let canonical_dir = find_transcript(root, id)
        .map(|transcript| {
            transcript
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "无效的会话目录。".to_string())
        })
        .transpose()?
        .map(|session_dir| {
            session_dir
                .canonicalize()
                .map_err(|error| error.to_string())
        })
        .transpose()?;
    if let Some(directory) = &canonical_dir {
        if !directory.starts_with(&canonical_root)
            || directory.file_name().and_then(|value| value.to_str()) != Some(id)
            || directory
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                != Some("agent-transcripts")
        {
            return Err("无效的会话目录。".into());
        }
    }
    purge_cursor_metadata(state_db, search_db, id)?;
    if let Some(directory) = canonical_dir {
        fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn purge_cursor_metadata(state_db: &Path, search_db: &Path, id: &str) -> Result<(), String> {
    let state = Connection::open(state_db).map_err(|error| error.to_string())?;
    state
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|error| error.to_string())?;
    let transaction = state
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM composerHeaders WHERE composerId = ?1", [id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM cursorDiskKV WHERE key LIKE ?1",
            [format!("%{id}%")],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM ItemTable WHERE key LIKE ?1",
            [format!("%{id}%")],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;

    let search = Connection::open(search_db).map_err(|error| error.to_string())?;
    search
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|error| error.to_string())?;
    let transaction = search
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction.execute("DELETE FROM conversation_fts WHERE rowid IN (SELECT fts_rowid FROM conversations WHERE id = ?1)", [id]).map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM conversations WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

fn collect_transcripts(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_transcripts(&path, files);
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(parent) = path.parent() else {
            continue;
        };
        if parent.file_name().and_then(|value| value.to_str()) == Some("subagents") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if parent.file_name().and_then(|value| value.to_str()) == Some(stem) && is_valid_id(stem) {
            files.push(path);
        }
    }
}

fn find_transcript(root: &Path, id: &str) -> Option<PathBuf> {
    let mut files = Vec::new();
    collect_transcripts(root, &mut files);
    files
        .into_iter()
        .find(|path| path.file_stem().and_then(|value| value.to_str()) == Some(id))
}

fn parse_session(
    path: &Path,
    root: &Path,
    metadata: &CursorSessionMetadata,
) -> Option<CodexSession> {
    let id = path.file_stem()?.to_str()?.to_owned();
    if metadata.archived_ids.contains(&id) {
        return None;
    }
    let messages = read_messages(path);
    let fallback_title = messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| session_title(&message.content))
        .filter(|value| !value.is_empty() && !contains_sensitive_value(value))?;
    let title = metadata.titles.get(&id).cloned().unwrap_or(fallback_title);
    let project_dir = path
        .strip_prefix(root)
        .ok()?
        .components()
        .next()?
        .as_os_str()
        .to_str()
        .map(ToOwned::to_owned);
    Some(CodexSession {
        id,
        title,
        project_dir,
        source_path: path.to_string_lossy().into_owned(),
        updated_at: modified_at(path),
    })
}

fn load_cursor_metadata(path: &Path) -> CursorSessionMetadata {
    let Ok(database) =
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return CursorSessionMetadata::default();
    };
    let Ok(mut statement) = database
        .prepare("SELECT composerId, isArchived, value FROM composerHeaders WHERE isSubagent = 0")
    else {
        return CursorSessionMetadata::default();
    };
    let mut metadata = CursorSessionMetadata::default();
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .ok()
        .into_iter()
        .flatten()
        .flatten();
    for (id, archived, value) in rows {
        if archived {
            metadata.archived_ids.insert(id);
            continue;
        }
        if let Some(title) = serde_json::from_str::<Value>(&value)
            .ok()
            .and_then(|value| {
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty() && !contains_sensitive_value(name))
                    .map(compact_title)
            })
        {
            metadata.titles.insert(id, title);
        }
    }
    metadata
}

fn read_messages(path: &Path) -> Vec<CodexSessionMessage> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| parse_message(&line))
        .take(MAX_MESSAGES)
        .collect()
}

fn parse_message(line: &str) -> Option<CodexSessionMessage> {
    let value: Value = serde_json::from_str(line).ok()?;
    let role = match value.get("role")?.as_str()? {
        "user" => "user",
        "assistant" => "assistant",
        _ => return None,
    }
    .to_owned();
    let content = extract_text(value.get("message")?.get("content")?)
        .trim()
        .to_owned();
    (!content.is_empty()).then_some(CodexSessionMessage {
        role,
        content: if contains_sensitive_value(&content) {
            "[Sensitive content hidden]".into()
        } else {
            content
        },
        timestamp: None,
    })
}

fn extract_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.to_owned(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .or_else(|| item.get("content"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn compact_title(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

fn session_title(content: &str) -> String {
    let query = content
        .split_once("<user_query>")
        .and_then(|(_, rest)| rest.split_once("</user_query>").map(|(query, _)| query))
        .unwrap_or(content);
    compact_title(query)
}
fn modified_at(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
fn is_valid_id(id: &str) -> bool {
    id.len() <= 128
        && !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
}

fn contains_sensitive_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "bearer ",
        "api_key",
        "api-key",
        "access_token",
        "refresh_token",
        "password=",
        "token=",
        "sk-",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_transcripts_without_a_displayable_user_message() {
        let root = std::env::temp_dir().join(format!("storm-dock-cursor-{}", std::process::id()));
        let session_dir = root
            .join("empty-window")
            .join("agent-transcripts")
            .join("12345678-1234-1234-1234-123456789abc");
        fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("12345678-1234-1234-1234-123456789abc.jsonl");
        fs::write(
            &path,
            r#"{"role":"assistant","message":{"content":"Ready"}}"#,
        )
        .unwrap();
        assert!(parse_session(&path, &root, &CursorSessionMetadata::default()).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extracts_cursor_user_query_as_the_session_title() {
        assert_eq!(
            session_title(
                "<timestamp>Thursday</timestamp>\n<user_query>\nDoes Region2D work?\n</user_query>"
            ),
            "Does Region2D work?"
        );
        assert_eq!(session_title("Plain request"), "Plain request");
    }

    #[test]
    fn prefers_the_cursor_composer_header_title() {
        let root = std::env::temp_dir().join(format!("storm-dock-title-{}", std::process::id()));
        let id = "12345678-1234-1234-1234-123456789abc";
        let session_dir = root.join("project").join("agent-transcripts").join(id);
        fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join(format!("{id}.jsonl"));
        fs::write(
            &path,
            r#"{"role":"user","message":{"content":"<user_query>Raw first request</user_query>"}}"#,
        )
        .unwrap();
        let metadata = CursorSessionMetadata {
            titles: HashMap::from([(id.to_owned(), "Cursor generated title".to_owned())]),
            archived_ids: HashSet::new(),
        };
        assert_eq!(
            parse_session(&path, &root, &metadata).unwrap().title,
            "Cursor generated title"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn omits_archived_cursor_sessions_from_the_list() {
        let root = std::env::temp_dir().join(format!("storm-dock-archived-{}", std::process::id()));
        let id = "12345678-1234-1234-1234-123456789abc";
        let session_dir = root.join("project").join("agent-transcripts").join(id);
        fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join(format!("{id}.jsonl"));
        fs::write(
            &path,
            r#"{"role":"user","message":{"content":"<user_query>Archived prompt</user_query>"}}"#,
        )
        .unwrap();
        let metadata = CursorSessionMetadata {
            titles: HashMap::new(),
            archived_ids: HashSet::from([id.to_owned()]),
        };
        assert!(parse_session(&path, &root, &metadata).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_rejects_invalid_ids() {
        assert!(delete_session("../../oops").is_err());
    }

    #[test]
    fn deletes_the_complete_cursor_session_directory() {
        let root = std::env::temp_dir().join(format!("storm-dock-delete-{}", std::process::id()));
        let id = "12345678-1234-1234-1234-123456789abc";
        fs::create_dir_all(&root).unwrap();
        let state_db = root.join("state.vscdb");
        let search_db = root.join("conversation-search.db");
        let state = Connection::open(&state_db).unwrap();
        state.execute_batch("CREATE TABLE composerHeaders (composerId TEXT PRIMARY KEY); CREATE TABLE cursorDiskKV (key TEXT UNIQUE, value BLOB); CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB);").unwrap();
        state
            .execute("INSERT INTO composerHeaders VALUES (?1)", [id])
            .unwrap();
        state
            .execute(
                "INSERT INTO cursorDiskKV VALUES (?1, '')",
                [format!("composerData:{id}")],
            )
            .unwrap();
        drop(state);
        let search = Connection::open(&search_db).unwrap();
        search.execute_batch("CREATE TABLE conversations (fts_rowid INTEGER PRIMARY KEY, id TEXT); CREATE VIRTUAL TABLE conversation_fts USING fts5(title);").unwrap();
        search
            .execute("INSERT INTO conversations VALUES (1, ?1)", [id])
            .unwrap();
        search
            .execute(
                "INSERT INTO conversation_fts(rowid, title) VALUES (1, 'title')",
                [],
            )
            .unwrap();
        drop(search);
        let session_dir = root.join("project").join("agent-transcripts").join(id);
        fs::create_dir_all(session_dir.join("subagents")).unwrap();
        fs::write(session_dir.join(format!("{id}.jsonl")), "{}").unwrap();
        fs::write(session_dir.join("subagents").join("child.jsonl"), "{}").unwrap();
        delete_session_at(&root, &state_db, &search_db, id).unwrap();
        assert!(!session_dir.exists());
        let state = Connection::open(&state_db).unwrap();
        assert_eq!(
            state
                .query_row("SELECT count(*) FROM composerHeaders", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            state
                .query_row("SELECT count(*) FROM cursorDiskKV", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        let search = Connection::open(&search_db).unwrap();
        assert_eq!(
            search
                .query_row("SELECT count(*) FROM conversations", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        let _ = fs::remove_dir_all(root);
    }
}
