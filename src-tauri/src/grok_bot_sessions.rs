use serde::Deserialize;
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::codex_sessions::{CodexSession, CodexSessionMessage};
use crate::grok_bot;

const MAX_SESSIONS: usize = 1_000;
const MAX_MESSAGES: usize = 1_000;
const TITLE_MAX_CHARS: usize = 120;

#[derive(Debug, Deserialize)]
struct GrokSessionInfo {
    id: String,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GrokSessionSummary {
    info: GrokSessionInfo,
    #[serde(default)]
    session_summary: Option<String>,
    #[serde(default)]
    generated_title: Option<String>,
    #[serde(default)]
    updated_at: Option<Value>,
    #[serde(default)]
    last_active_at: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct BotRecord {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    project_dir: Option<String>,
    #[serde(default)]
    updated_at: Option<Value>,
}

pub(crate) fn list_sessions() -> Vec<CodexSession> {
    let Some(root) = grok_bot::user_data_dir() else {
        return Vec::new();
    };
    list_sessions_from(&root)
}

pub(crate) fn load_messages(id: &str) -> Vec<CodexSessionMessage> {
    if !is_valid_id(id) {
        return Vec::new();
    }
    let Some(root) = grok_bot::user_data_dir() else {
        return Vec::new();
    };
    load_messages_from_root(&root, id)
}

pub(crate) fn delete_session(id: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err("无效的会话标识。".into());
    }
    let root = grok_bot::user_data_dir().ok_or_else(|| "未检测到 Grok Bot 数据目录。".to_string())?;
    delete_session_from(&root, id)
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

pub(crate) fn rename_session(id: &str, title: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err("无效的会话标识。".into());
    }
    let title = safe_title(title).ok_or_else(|| "会话标题无效。".to_string())?;
    let root = grok_bot::user_data_dir().ok_or_else(|| "未检测到 Grok Bot 数据目录。".to_string())?;
    rename_session_from(&root, id, &title)
}

fn list_sessions_from(root: &Path) -> Vec<CodexSession> {
    let mut sessions = Vec::new();
    for folder in ["sessions", "archived_sessions"] {
        collect_summary_sessions(&root.join(folder), &mut sessions);
    }
    collect_summary_sessions(root, &mut sessions);
    collect_bot_catalog(root, &mut sessions);
    collect_bot_directories(&root.join("bots"), &mut sessions);
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    let mut seen = std::collections::HashSet::new();
    sessions.retain(|session| seen.insert(session.id.clone()));
    sessions.truncate(MAX_SESSIONS);
    sessions
}

fn collect_summary_sessions(root: &Path, sessions: &mut Vec<CodexSession>) {
    let mut files = Vec::new();
    collect_named_files(root, "summary.json", &mut files);
    for path in files {
        if let Some(session) = parse_summary(&path) {
            sessions.push(session);
        }
    }
}

fn collect_bot_catalog(root: &Path, sessions: &mut Vec<CodexSession>) {
    for name in ["bots.json", "agents.json"] {
        let path = root.join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for bot in parse_bot_records(&text) {
            if !is_valid_id(&bot.id) || sessions.iter().any(|session| session.id == bot.id) {
                continue;
            }
            let title = bot
                .name
                .as_deref()
                .and_then(safe_title)
                .or_else(|| bot.title.as_deref().and_then(safe_title))
                .unwrap_or_else(|| short_id(&bot.id).to_owned());
            let project_dir = bot
                .project_dir
                .or(bot.cwd)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            sessions.push(CodexSession {
                id: bot.id,
                title,
                project_dir,
                source_path: path.display().to_string(),
                updated_at: bot
                    .updated_at
                    .as_ref()
                    .and_then(timestamp_ms)
                    .unwrap_or_else(|| modified_at(&path)),
            });
        }
    }
}

fn collect_bot_directories(root: &Path, sessions: &mut Vec<CodexSession>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_valid_id(id) {
            continue;
        }
        if sessions.iter().any(|session| session.id == id) {
            continue;
        }
        let title = read_title_override(&path)
            .or_else(|| read_conversation_title(&path))
            .unwrap_or_else(|| short_id(id).to_owned());
        sessions.push(CodexSession {
            id: id.to_owned(),
            title,
            project_dir: None,
            source_path: path.display().to_string(),
            updated_at: modified_at(&path),
        });
    }
}

fn parse_bot_records(text: &str) -> Vec<BotRecord> {
    if let Ok(bots) = serde_json::from_str::<Vec<BotRecord>>(text) {
        return bots;
    }
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let items = value
        .get("bots")
        .or_else(|| value.get("agents"))
        .cloned()
        .unwrap_or(value);
    serde_json::from_value(items).unwrap_or_default()
}

fn parse_summary(path: &Path) -> Option<CodexSession> {
    let text = fs::read_to_string(path).ok()?;
    let summary = serde_json::from_str::<GrokSessionSummary>(&text).ok()?;
    let id = summary.info.id;
    if !is_valid_id(&id) {
        return None;
    }
    let title = read_title_override(path.parent()?)
        .or_else(|| summary.generated_title.as_deref().and_then(safe_title))
        .or_else(|| summary.session_summary.as_deref().and_then(safe_title))
        .unwrap_or_else(|| short_id(&id).to_owned());
    let updated_at = summary
        .last_active_at
        .as_ref()
        .or(summary.updated_at.as_ref())
        .and_then(timestamp_ms)
        .unwrap_or_else(|| modified_at(path));
    Some(CodexSession {
        id,
        title,
        project_dir: summary
            .info
            .cwd
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        source_path: path.display().to_string(),
        updated_at,
    })
}

fn load_messages_from_root(root: &Path, id: &str) -> Vec<CodexSessionMessage> {
    if let Some(path) = find_history_path(root, id) {
        return load_jsonl_messages(&path);
    }
    Vec::new()
}

fn find_history_path(root: &Path, id: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    collect_named_files(root, "chat_history.jsonl", &mut candidates);
    collect_named_files(root, "messages.jsonl", &mut candidates);
    candidates.into_iter().find(|path| {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            == Some(id)
            || parse_summary(&path.with_file_name("summary.json"))
                .is_some_and(|session| session.id == id)
    })
}

fn load_jsonl_messages(path: &Path) -> Vec<CodexSessionMessage> {
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
    let value = serde_json::from_str::<Value>(line).ok()?;
    let role = value
        .get("type")
        .or_else(|| value.get("role"))
        .and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }
    let content = extract_text(
        value
            .get("content")
            .or_else(|| value.get("message").and_then(|message| message.get("content")))
            .unwrap_or(&Value::Null),
    )
    .trim()
    .to_owned();
    if content.is_empty() {
        return None;
    }
    Some(CodexSessionMessage {
        role: role.into(),
        content,
        timestamp: value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .and_then(timestamp_ms),
    })
}

fn delete_session_from(root: &Path, id: &str) -> Result<(), String> {
    let mut removed = false;
    if let Some(summary) = find_summary_path(root, id) {
        let session_dir = summary.parent().ok_or_else(|| "会话路径无效。".to_string())?;
        if session_dir == root || !session_dir.starts_with(root) {
            return Err("会话路径无效。".into());
        }
        if session_dir.file_name().and_then(|name| name.to_str()) != Some(id)
            && parse_summary(&summary).is_some_and(|session| session.id != id)
        {
            return Err("会话标识不匹配。".into());
        }
        fs::remove_dir_all(session_dir).map_err(|error| error.to_string())?;
        removed = true;
    }
    let bot_dir = root.join("bots").join(id);
    if bot_dir.is_dir() {
        if !bot_dir.starts_with(root) {
            return Err("会话路径无效。".into());
        }
        fs::remove_dir_all(&bot_dir).map_err(|error| error.to_string())?;
        removed = true;
    }
    if remove_bot_catalog_entry(root, id)? {
        removed = true;
    }
    if removed {
        Ok(())
    } else {
        Err("会话不存在。".into())
    }
}

fn rename_session_from(root: &Path, id: &str, title: &str) -> Result<(), String> {
    let mut renamed = false;
    if let Some(summary) = find_summary_path(root, id) {
        let text = fs::read_to_string(&summary).map_err(|error| error.to_string())?;
        let mut value = serde_json::from_str::<Value>(&text).map_err(|error| error.to_string())?;
        value["generated_title"] = Value::String(title.to_owned());
        fs::write(&summary, serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        if let Some(parent) = summary.parent() {
            write_title_override(parent, title)?;
        }
        renamed = true;
    }
    if update_bot_catalog_title(root, id, title)? {
        renamed = true;
    }
    let bot_dir = root.join("bots").join(id);
    if bot_dir.is_dir() {
        write_title_override(&bot_dir, title)?;
        renamed = true;
    }
    if renamed {
        Ok(())
    } else {
        Err("会话不存在。".into())
    }
}

fn find_summary_path(root: &Path, id: &str) -> Option<PathBuf> {
    let mut files = Vec::new();
    collect_named_files(root, "summary.json", &mut files);
    files
        .into_iter()
        .filter(|path| parse_summary(path).is_some_and(|session| session.id == id))
        .max_by_key(|path| parse_summary(path).map(|session| session.updated_at).unwrap_or(0))
}

fn remove_bot_catalog_entry(root: &Path, id: &str) -> Result<bool, String> {
    let mut changed = false;
    for name in ["bots.json", "agents.json"] {
        let path = root.join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(next) = filter_bot_catalog(&text, id) {
            fs::write(&path, next).map_err(|error| error.to_string())?;
            changed = true;
        }
    }
    Ok(changed)
}

fn update_bot_catalog_title(root: &Path, id: &str, title: &str) -> Result<bool, String> {
    let mut changed = false;
    for name in ["bots.json", "agents.json"] {
        let path = root.join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(next) = rename_bot_catalog(&text, id, title) {
            fs::write(&path, next).map_err(|error| error.to_string())?;
            changed = true;
        }
    }
    Ok(changed)
}

fn filter_bot_catalog(text: &str, id: &str) -> Option<String> {
    rewrite_bot_catalog(text, |bots| {
        let before = bots.len();
        bots.retain(|bot| bot.get("id").and_then(Value::as_str) != Some(id));
        bots.len() != before
    })
}

fn rename_bot_catalog(text: &str, id: &str, title: &str) -> Option<String> {
    rewrite_bot_catalog(text, |bots| {
        let mut changed = false;
        for bot in bots {
            if bot.get("id").and_then(Value::as_str) == Some(id) {
                bot["name"] = Value::String(title.to_owned());
                bot["title"] = Value::String(title.to_owned());
                changed = true;
            }
        }
        changed
    })
}

fn rewrite_bot_catalog(text: &str, mut update: impl FnMut(&mut Vec<Value>) -> bool) -> Option<String> {
    if let Ok(mut bots) = serde_json::from_str::<Vec<Value>>(text) {
        return update(&mut bots).then(|| serde_json::to_string_pretty(&bots).ok())?;
    }
    let mut value = serde_json::from_str::<Value>(text).ok()?;
    let key = if value.get("bots").is_some() {
        "bots"
    } else if value.get("agents").is_some() {
        "agents"
    } else {
        return None;
    };
    let mut bots = value.get(key)?.as_array()?.clone();
    if !update(&mut bots) {
        return None;
    }
    value[key] = Value::Array(bots);
    serde_json::to_string_pretty(&value).ok()
}

fn collect_named_files(root: &Path, file_name: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_named_files(&path, file_name, files);
        } else if path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
            files.push(path);
        }
    }
}

fn read_title_override(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("title.json")).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    value
        .get("title")
        .and_then(Value::as_str)
        .and_then(safe_title)
}

fn write_title_override(dir: &Path, title: &str) -> Result<(), String> {
    fs::write(
        dir.join("title.json"),
        serde_json::to_vec_pretty(&serde_json::json!({ "title": title }))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn read_conversation_title(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("conversation.json")).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    value
        .get("title")
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .and_then(safe_title)
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

fn timestamp_ms(value: &Value) -> Option<u64> {
    crate::models::parse_timestamp(value).map(|seconds| seconds.saturating_mul(1_000))
}

fn modified_at(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.contains(['/', '\\', '\0']) && !id.contains("..")
}

fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn safe_title(value: &str) -> Option<String> {
    let title = value.trim();
    if title.is_empty() {
        return None;
    }
    Some(if title.chars().count() > TITLE_MAX_CHARS {
        title.chars().take(TITLE_MAX_CHARS).collect()
    } else {
        title.to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("storm-dock-grok-bot-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_session(root: &Path, id: &str, title: &str, history: &str) -> PathBuf {
        let dir = root.join("sessions").join("project").join(id);
        fs::create_dir_all(&dir).unwrap();
        let summary = dir.join("summary.json");
        fs::write(
            &summary,
            format!(
                r#"{{"info":{{"id":"{id}","cwd":"/work"}},"generated_title":"{title}","last_active_at":"2026-07-16T12:00:01Z"}}"#
            ),
        )
        .unwrap();
        fs::write(dir.join("chat_history.jsonl"), history).unwrap();
        summary
    }

    #[test]
    fn lists_summary_and_catalog_sessions_without_duplicates() {
        let root = temp_root("list");
        write_session(&root, "session-1", "Summary title", "");
        fs::write(
            root.join("bots.json"),
            r#"{"bots":[{"id":"session-1","name":"Catalog title"},{"id":"bot-2","name":"Inbox bot","cwd":"/inbox"}]}"#,
        )
        .unwrap();
        let sessions = list_sessions_from(&root);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions.iter().find(|session| session.id == "session-1").unwrap().title, "Summary title");
        assert_eq!(
            sessions.iter().find(|session| session.id == "bot-2").unwrap().project_dir.as_deref(),
            Some("/inbox")
        );
    }

    #[test]
    fn loads_chat_history_and_skips_non_messages() {
        let root = temp_root("messages");
        write_session(
            &root,
            "session-1",
            "Chat",
            concat!(
                r#"{"type":"user","content":[{"type":"text","text":"hello"}]}"#,
                "\n",
                r#"{"type":"reasoning","content":"private"}"#,
                "\n",
                r#"{"role":"assistant","content":"Hi there"}"#,
                "\n"
            ),
        );
        let messages = load_messages_from_root(&root, "session-1");
        let _ = fs::remove_dir_all(&root);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "hello");
        assert_eq!(messages[1].content, "Hi there");
    }

    #[test]
    fn rename_updates_summary_and_catalog() {
        let root = temp_root("rename");
        write_session(&root, "session-1", "Old", "");
        fs::write(
            root.join("bots.json"),
            r#"{"bots":[{"id":"session-1","name":"Old"}]}"#,
        )
        .unwrap();
        rename_session_from(&root, "session-1", "  New name  ").unwrap();
        let sessions = list_sessions_from(&root);
        let catalog = fs::read_to_string(root.join("bots.json")).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(sessions[0].title, "New name");
        assert!(catalog.contains("New name"));
    }

    #[test]
    fn delete_removes_session_dir_and_catalog_entry() {
        let root = temp_root("delete");
        write_session(&root, "session-1", "Gone", "");
        fs::write(
            root.join("bots.json"),
            r#"{"bots":[{"id":"session-1","name":"Gone"},{"id":"keep","name":"Keep"}]}"#,
        )
        .unwrap();
        delete_session_from(&root, "session-1").unwrap();
        let sessions = list_sessions_from(&root);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "keep");
    }

    #[test]
    fn rejects_unsafe_ids() {
        assert!(rename_session("../oops", "x").is_err());
        assert!(delete_session("..\\oops").is_err());
        assert!(load_messages("../oops").is_empty());
    }
}
