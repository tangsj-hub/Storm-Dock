use serde::Deserialize;
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::codex_sessions::{CodexSession, CodexSessionMessage};
use crate::grok::config::grok_home;

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

pub(crate) fn list_sessions() -> Vec<CodexSession> {
    list_sessions_from(&session_roots())
}

pub(crate) fn load_messages(id: &str) -> Vec<CodexSessionMessage> {
    if !is_valid_id(id) {
        return Vec::new();
    }
    let Some(path) = find_summary_path(id) else {
        return Vec::new();
    };
    load_messages_from(&path)
}

pub(crate) fn delete_session(id: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err("无效的会话标识。".into());
    }
    let path = find_summary_path(id).ok_or_else(|| "会话不存在。".to_string())?;
    let root = session_roots()
        .into_iter()
        .find(|root| path.starts_with(root))
        .ok_or_else(|| "会话不存在。".to_string())?;
    delete_session_dir(&root, &path, id)?;
    Ok(())
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

pub(crate) fn launch_session(id: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err("无效的会话标识。".into());
    }
    std::process::Command::new("osascript")
        .args([
            "-e",
            &format!("tell application \"Terminal\" to do script \"grok --resume {id}\""),
        ])
        .status()
        .map_err(|error| error.to_string())
        .and_then(|status| {
            status
                .success()
                .then_some(())
                .ok_or_else(|| "无法启动终端会话。".into())
        })
}

fn session_roots() -> Vec<PathBuf> {
    let Some(home) = grok_home() else {
        return Vec::new();
    };
    vec![home.join("sessions"), home.join("archived_sessions")]
}

fn list_sessions_from(roots: &[PathBuf]) -> Vec<CodexSession> {
    let mut files = Vec::new();
    for root in roots {
        collect_summary_files(root, &mut files);
    }
    let mut sessions = files
        .into_iter()
        .filter_map(|path| parse_summary(&path))
        .collect::<Vec<_>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions.truncate(MAX_SESSIONS);
    sessions
}

fn find_summary_path(id: &str) -> Option<PathBuf> {
    let mut files = Vec::new();
    for root in session_roots() {
        collect_summary_files(&root, &mut files);
    }
    files
        .into_iter()
        .filter(|path| parse_summary(path).is_some_and(|session| session.id == id))
        .max_by_key(|path| parse_summary(path).map(|session| session.updated_at).unwrap_or(0))
}

fn collect_summary_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_summary_files(&path, files);
        } else if path.file_name().and_then(|name| name.to_str()) == Some("summary.json") {
            files.push(path);
        }
    }
}

fn read_summary(path: &Path) -> Result<GrokSessionSummary, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

fn parse_summary(path: &Path) -> Option<CodexSession> {
    let summary = read_summary(path).ok()?;
    let id = summary.info.id;
    if !is_valid_id(&id) {
        return None;
    }
    let title = summary
        .generated_title
        .as_deref()
        .and_then(safe_title)
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

fn load_messages_from(summary_path: &Path) -> Vec<CodexSessionMessage> {
    let Some(session_dir) = summary_path.parent() else {
        return Vec::new();
    };
    let Ok(file) = File::open(session_dir.join("chat_history.jsonl")) else {
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
    let role = value.get("type").and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }
    let content = extract_text(value.get("content")?).trim().to_owned();
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

fn delete_session_dir(root: &Path, path: &Path, session_id: &str) -> Result<(), String> {
    if !path.starts_with(root) {
        return Err("会话路径无效。".into());
    }
    if path.file_name().and_then(|name| name.to_str()) != Some("summary.json") {
        return Err("会话路径无效。".into());
    }
    let summary = read_summary(path)?;
    if summary.info.id != session_id {
        return Err("会话标识不匹配。".into());
    }
    let session_dir = path.parent().ok_or_else(|| "会话路径无效。".to_string())?;
    if session_dir == root || !session_dir.starts_with(root) {
        return Err("会话路径无效。".into());
    }
    if session_dir.file_name().and_then(|name| name.to_str()) != Some(session_id) {
        return Err("会话路径无效。".into());
    }
    fs::remove_dir_all(session_dir).map_err(|error| error.to_string())
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
    !id.is_empty()
        && id.len() <= 128
        && !id.contains(['/', '\\', '\0'])
        && !id.contains("..")
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

    fn write_session(root: &Path, project: &str, id: &str, summary: &str, history: &str) -> PathBuf {
        let session_dir = root.join(project).join(id);
        fs::create_dir_all(&session_dir).unwrap();
        let summary_path = session_dir.join("summary.json");
        fs::write(&summary_path, summary).unwrap();
        fs::write(session_dir.join("chat_history.jsonl"), history).unwrap();
        summary_path
    }

    #[test]
    fn scans_native_layout_and_prefers_generated_title() {
        let root = std::env::temp_dir().join(format!("storm-dock-grok-sessions-{}", uuid::Uuid::new_v4()));
        let id = "019f6af2-18b0-7673-958e-d25be650e172";
        write_session(
            &root,
            "encoded-project",
            id,
            &format!(
                r#"{{"info":{{"id":"{id}","cwd":"/work"}},"session_summary":"hello grok","generated_title":"Grok session","last_active_at":"2026-07-16T12:00:01Z"}}"#
            ),
            "",
        );
        let sessions = list_sessions_from(&[root.clone()]);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert_eq!(sessions[0].title, "Grok session");
        assert_eq!(sessions[0].project_dir.as_deref(), Some("/work"));
    }

    #[test]
    fn loads_chat_history_and_skips_reasoning() {
        let root = std::env::temp_dir().join(format!("storm-dock-grok-messages-{}", uuid::Uuid::new_v4()));
        let summary_path = write_session(
            &root,
            "project",
            "session-1",
            r#"{"info":{"id":"session-1"}}"#,
            concat!(
                r#"{"type":"user","content":[{"type":"text","text":"hello"}]}"#,
                "\n",
                r#"{"type":"reasoning","summary":[{"type":"summary_text","text":"private"}]}"#,
                "\n",
                r#"{"type":"tool","content":"ignored"}"#,
                "\n",
                r#"{"type":"assistant","content":"Hi there"}"#,
                "\n"
            ),
        );
        let messages = load_messages_from(&summary_path);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "hello");
        assert_eq!(messages[1].content, "Hi there");
    }

    #[test]
    fn delete_session_removes_only_the_matching_directory() {
        let root = std::env::temp_dir().join(format!("storm-dock-grok-delete-{}", uuid::Uuid::new_v4()));
        let id = "session-to-delete";
        let summary_path = write_session(&root, "project", id, &format!(r#"{{"info":{{"id":"{id}"}}}}"#), "");
        let sibling = root.join("project").join("session-to-keep");
        fs::create_dir_all(&sibling).unwrap();
        fs::write(sibling.join("keep.txt"), "keep").unwrap();

        delete_session_dir(&root, &summary_path, id).unwrap();
        assert!(!root.join("project").join(id).exists());
        assert!(sibling.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_session_rejects_paths_outside_root() {
        let root = std::env::temp_dir().join(format!("storm-dock-grok-outside-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let outside = std::env::temp_dir().join(format!("storm-dock-grok-outside-dir-{}", uuid::Uuid::new_v4()));
        let id = "session-outside";
        let summary_path = write_session(&outside, "project", id, &format!(r#"{{"info":{{"id":"{id}"}}}}"#), "");
        assert!(delete_session_dir(&root, &summary_path, id).is_err());
        assert!(outside.join("project").join(id).exists());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }
}
