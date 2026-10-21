use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_SESSIONS: usize = 1_000;
const MAX_MESSAGES: usize = 1_000;
const TITLE_MAX_CHARS: usize = 120;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexSession {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) project_dir: Option<String>,
    pub(crate) source_path: String,
    pub(crate) updated_at: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionAttachment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexSessionMessage {
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) timestamp: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) attachments: Option<Vec<SessionAttachment>>,
}

#[derive(Deserialize)]
struct SessionIndexEntry {
    id: String,
    thread_name: String,
}

pub(crate) fn list_sessions() -> Vec<CodexSession> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let root = PathBuf::from(home).join(".codex");
    let titles = load_thread_titles(&root.join("session_index.jsonl"));
    let mut files = Vec::new();
    for directory in [root.join("sessions"), root.join("archived_sessions")] {
        collect_session_files(&directory, &mut files);
    }
    let mut files = dedupe_siblings(files);
    files.sort_by_key(|path| std::cmp::Reverse(modified_at(path)));
    files.truncate(MAX_SESSIONS);

    let mut sessions = files
        .into_iter()
        .filter_map(|path| parse_session(&path, &titles))
        .collect::<Vec<_>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}

pub(crate) fn load_messages(id: &str) -> Vec<CodexSessionMessage> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
    {
        return Vec::new();
    }
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let root = PathBuf::from(home).join(".codex");
    let mut files = Vec::new();
    for directory in [root.join("sessions"), root.join("archived_sessions")] {
        collect_session_files(&directory, &mut files);
    }
    let Some(path) = files
        .into_iter()
        .filter(|path| parse_session(path, &HashMap::new()).is_some_and(|session| session.id == id))
        .max_by_key(|path| modified_at(path))
    else {
        return Vec::new();
    };
    let Ok(reader) = open_reader(&path) else {
        return Vec::new();
    };
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| parse_message(&line))
        .take(MAX_MESSAGES)
        .collect()
}

pub(crate) fn delete_session(id: &str) -> Result<(), String> {
    let path = find_session_path(id).ok_or_else(|| "会话不存在。".to_string())?;
    fs::remove_file(&path).map_err(|error| error.to_string())?;
    // 同一会话可能短暂同时存在 .jsonl 与 .jsonl.zst 两种形态，一并删除。
    if let Some(other) = sibling(&path) {
        let _ = fs::remove_file(other);
    }
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
    #[cfg(target_os = "macos")]
    {
        crate::macos_native::launch_terminal_command(&format!("codex resume {id}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Err("当前平台不支持从此处启动终端会话。".into())
    }
}

fn find_session_path(id: &str) -> Option<PathBuf> {
    if !is_valid_id(id) {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    let root = PathBuf::from(home).join(".codex");
    let mut files = Vec::new();
    for directory in [root.join("sessions"), root.join("archived_sessions")] {
        collect_session_files(&directory, &mut files);
    }
    files
        .into_iter()
        .filter(|path| parse_session(path, &HashMap::new()).is_some_and(|session| session.id == id))
        .max_by_key(|path| modified_at(path))
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
}

fn collect_session_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_session_files(&path, files);
        } else if is_rollout_file(&path) {
            files.push(path);
        }
    }
}

fn load_thread_titles(path: &Path) -> HashMap<String, String> {
    let Ok(file) = File::open(path) else {
        return HashMap::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<SessionIndexEntry>(&line).ok())
        .filter_map(|entry| safe_title(&entry.thread_name).map(|title| (entry.id, title)))
        .collect()
}

fn parse_session(path: &Path, titles: &HashMap<String, String>) -> Option<CodexSession> {
    let reader = open_reader(path).ok()?;
    let mut id = None;
    let mut project_dir = None;
    for line in reader.lines().take(24).map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }
        let payload = value.get("payload")?;
        if payload
            .get("source")
            .and_then(Value::as_object)
            .is_some_and(|source| source.contains_key("subagent"))
        {
            return None;
        }
        id = payload.get("id").and_then(Value::as_str).map(str::to_owned);
        project_dir = payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_owned);
        break;
    }
    let id = id.or_else(|| session_id_from_filename(path))?;
    let title = titles.get(&id).cloned().unwrap_or_else(|| {
        project_dir
            .as_deref()
            .and_then(path_basename)
            .unwrap_or_else(|| short_id(&id))
            .to_owned()
    });
    Some(CodexSession {
        id,
        title,
        project_dir,
        source_path: path.display().to_string(),
        updated_at: modified_at(path),
    })
}

fn parse_message(line: &str) -> Option<CodexSessionMessage> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = payload.get("role").and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }
    let content = extract_text(payload.get("content")?).trim().to_owned();
    if content.is_empty() {
        return None;
    }
    let content = if contains_sensitive_value(&content) {
        "[Sensitive content hidden]".into()
    } else {
        content
    };
    Some(CodexSessionMessage {
        role: role.into(),
        content,
        timestamp: value
            .get("timestamp")
            .and_then(crate::models::parse_timestamp)
            .map(|timestamp| timestamp.saturating_mul(1_000)),
                attachments: None,
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

// Codex 新版会把 7 天未活动的 rollout 压缩为同名 `rollout-*.jsonl.zst`（单帧 zstd，
// 保留 mtime）并删除原 `.jsonl`；恢复会话时再解压回 `.jsonl`。参照 cc-switch 的
// `codex_rollout_file.rs`，两种形态都要识别。
const COMPRESSED_SUFFIX: &str = ".zst";
const PLAIN_SUFFIX: &str = ".jsonl";

fn is_compressed(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(COMPRESSED_SUFFIX))
        .is_some_and(|plain| plain.ends_with(PLAIN_SUFFIX))
}

fn is_rollout_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "jsonl") || is_compressed(path)
}

fn logical_path(path: &Path) -> PathBuf {
    match path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|_| is_compressed(path))
        .and_then(|name| name.strip_suffix(COMPRESSED_SUFFIX))
    {
        Some(plain) => path.with_file_name(plain),
        None => path.to_path_buf(),
    }
}

fn sibling(path: &Path) -> Option<PathBuf> {
    if is_compressed(path) {
        return Some(logical_path(path));
    }
    let name = path.file_name()?.to_str()?;
    is_rollout_file(path).then(|| path.with_file_name(format!("{name}{COMPRESSED_SUFFIX}")))
}

fn open_reader(path: &Path) -> std::io::Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    if is_compressed(path) {
        Ok(Box::new(BufReader::new(zstd::stream::read::Decoder::new(file)?)))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// 两种形态同时存在时只保留普通 `.jsonl`（Codex 正在写的那份）。
fn dedupe_siblings(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let plain: std::collections::HashSet<PathBuf> = paths
        .iter()
        .filter(|path| !is_compressed(path))
        .cloned()
        .collect();
    paths
        .into_iter()
        .filter(|path| !is_compressed(path) || !plain.contains(&logical_path(path)))
        .collect()
}

fn modified_at(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn path_basename(path: &str) -> Option<&str> {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
}

fn session_id_from_filename(path: &Path) -> Option<String> {
    let logical = logical_path(path);
    let name = logical.file_stem()?.to_string_lossy();
    name.rsplit('-')
        .next()
        .filter(|id| id.len() >= 8)
        .map(str::to_owned)
}

fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn safe_title(value: &str) -> Option<String> {
    let title = value.trim();
    if title.is_empty()
        || title.len() > TITLE_MAX_CHARS
        || [
            "bearer ",
            "api_key",
            "api-key",
            "access_token",
            "refresh_token",
            "password=",
            "sk-",
        ]
        .iter()
        .any(|marker| title.to_ascii_lowercase().contains(marker))
    {
        return None;
    }
    Some(title.to_owned())
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
    fn omits_sensitive_thread_titles() {
        assert_eq!(
            safe_title("Investigate the login flow"),
            Some("Investigate the login flow".into())
        );
        assert_eq!(safe_title("Bearer abc"), None);
    }

    #[test]
    fn extracts_project_and_id_from_session_metadata() {
        let path =
            std::env::temp_dir().join(format!("storm-dock-session-{}.jsonl", std::process::id()));
        fs::write(&path, r#"{"type":"session_meta","payload":{"id":"12345678-1234-1234-1234-123456789abc","cwd":"/work/example"}}"#).unwrap();
        let session = parse_session(&path, &HashMap::new()).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(session.title, "example");
        assert_eq!(session.project_dir.as_deref(), Some("/work/example"));
    }

    #[test]
    fn reads_compressed_rollouts() {
        let path = std::env::temp_dir().join(format!(
            "rollout-2026-01-01T00-00-00-12345678-1234-1234-1234-123456789abd-{}.jsonl.zst",
            std::process::id()
        ));
        let body = r#"{"type":"session_meta","payload":{"id":"12345678-1234-1234-1234-123456789abd","cwd":"/work/zst"}}"#;
        fs::write(&path, zstd::stream::encode_all(body.as_bytes(), 3).unwrap()).unwrap();
        let session = parse_session(&path, &HashMap::new()).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(session.id, "12345678-1234-1234-1234-123456789abd");
        assert_eq!(session.title, "zst");
        assert!(is_rollout_file(&path));
    }

    #[test]
    fn hides_messages_that_may_contain_credentials() {
        assert!(contains_sensitive_value("Authorization: Bearer secret"));
        assert!(!contains_sensitive_value("Explain API authentication"));
    }
}
