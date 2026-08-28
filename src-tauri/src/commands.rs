use std::{
    fs,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex as StdMutex},
    thread,
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::apps::{launch_cursor, terminate_cursor, wait_for_cursor_stop};
use crate::codex_sessions::{CodexSession, CodexSessionMessage};
use crate::cursor::api::{
    cursor_marketplace_plugins, dashboard_cookie, dashboard_request, fetch_cursor_subscription,
    fetch_cursor_subscription_fast, set_cursor_marketplace_plugin_enabled,
    uninstall_cursor_marketplace_plugin,
};
use crate::cursor::oauth::{complete_cursor_oauth, open_browser, OauthLoginState};
use crate::cursor::usage::{fetch_cursor_usage, usage_pools};
use crate::error::AppError;
use crate::models::{
    import_type, Account, AccountSummary, ApplicationKind, ApplicationStatus, CursorUsageDetails,
    McpServer, Plugin, PluginCapability, Session, SwitchOutcome, SwitchProgress,
    MEMBERSHIP_TYPE_KEY,
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
pub(crate) async fn list_cursor_plugins(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<Plugin>, String> {
    // Capture the session while holding the store lock, then release it before
    // scanning plugin directories or making the Marketplace request.
    let session = state
        .0
        .lock()
        .map_err(|_| "应用状态不可用".to_string())?
        .current_cursor_session()
        .ok();
    tauri::async_runtime::spawn_blocking(move || collect_cursor_plugins(session))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn list_codex_plugins() -> Vec<Plugin> {
    tauri::async_runtime::spawn_blocking(collect_codex_plugins)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn list_codex_sessions() -> Vec<CodexSession> {
    tauri::async_runtime::spawn_blocking(crate::codex_sessions::list_sessions)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn get_codex_session_messages(id: String) -> Vec<CodexSessionMessage> {
    tauri::async_runtime::spawn_blocking(move || crate::codex_sessions::load_messages(&id))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn delete_codex_session(id: String) -> std::result::Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::codex_sessions::delete_session(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionDeleteBatchResult {
    deleted_ids: Vec<String>,
    failed_ids: Vec<String>,
}

#[tauri::command]
pub(crate) async fn delete_codex_sessions(ids: Vec<String>) -> SessionDeleteBatchResult {
    let fallback_ids = ids.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (deleted_ids, failed_ids) = crate::codex_sessions::delete_sessions(&ids);
        SessionDeleteBatchResult {
            deleted_ids,
            failed_ids,
        }
    })
    .await
    .unwrap_or(SessionDeleteBatchResult {
        deleted_ids: Vec::new(),
        failed_ids: fallback_ids,
    })
}

#[tauri::command]
pub(crate) async fn launch_codex_session(id: String) -> std::result::Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::codex_sessions::launch_session(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn list_cursor_sessions() -> Vec<CodexSession> {
    tauri::async_runtime::spawn_blocking(crate::cursor_sessions::list_sessions)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn get_cursor_session_messages(id: String) -> Vec<CodexSessionMessage> {
    tauri::async_runtime::spawn_blocking(move || crate::cursor_sessions::load_messages(&id))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn delete_cursor_session(id: String) -> std::result::Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::cursor_sessions::delete_session(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn delete_cursor_sessions(ids: Vec<String>) -> SessionDeleteBatchResult {
    let fallback_ids = ids.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (deleted_ids, failed_ids) = crate::cursor_sessions::delete_sessions(&ids);
        SessionDeleteBatchResult {
            deleted_ids,
            failed_ids,
        }
    })
    .await
    .unwrap_or(SessionDeleteBatchResult {
        deleted_ids: Vec::new(),
        failed_ids: fallback_ids,
    })
}

#[tauri::command]
pub(crate) fn set_codex_plugin_enabled(
    id: String,
    enabled: bool,
) -> std::result::Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "无法读取用户目录。".to_string())?;
    update_codex_plugin_enabled(
        &PathBuf::from(home).join(".codex/config.toml"),
        &id,
        enabled,
    )
}

#[tauri::command]
pub(crate) fn set_codex_plugin_capability_enabled(
    plugin_id: String,
    capability_id: String,
    kind: String,
    enabled: bool,
) -> std::result::Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "无法读取用户目录。".to_string())?;
    let path = PathBuf::from(home).join(".codex/config.toml");
    match kind.as_str() {
        "mcp" => update_codex_plugin_mcp_enabled(&path, &plugin_id, &capability_id, enabled),
        "skill" => update_codex_skill_enabled(&path, &capability_id, enabled),
        _ => Err("不支持此插件能力类型。".into()),
    }
}

#[tauri::command]
pub(crate) async fn delete_codex_plugin(id: String) -> std::result::Result<(), String> {
    if id.is_empty() || id.contains('\0') {
        return Err("无效的 ChatGPT 插件标识。".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new("codex")
            .args(["plugin", "remove", &id, "--json"])
            .output()
            .map_err(|error| format!("无法启动 Codex 插件管理命令：{error}"))?;
        if output.status.success() {
            return Ok(());
        }
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if message.is_empty() {
            "Codex 插件删除失败。".into()
        } else {
            message
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn list_mcp_servers(kind: ApplicationKind) -> Vec<McpServer> {
    tauri::async_runtime::spawn_blocking(move || collect_mcp_servers(kind))
        .await
        .unwrap_or_default()
}

fn collect_codex_plugins() -> Vec<Plugin> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let codex_root = PathBuf::from(home).join(".codex");
    let config = fs::read_to_string(codex_root.join("config.toml"))
        .ok()
        .and_then(|content| content.parse::<toml::Value>().ok())
        .unwrap_or(toml::Value::Table(toml::map::Map::new()));
    let enabled = config
        .get("plugins")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default();
    let root = codex_root.join("plugins/cache");
    let mut plugins = std::collections::BTreeMap::new();
    for marketplace in fs::read_dir(root).into_iter().flatten().flatten() {
        for plugin in fs::read_dir(marketplace.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let Some(version) = fs::read_dir(plugin.path())
                .into_iter()
                .flatten()
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .max_by_key(|entry| entry.file_name())
            else {
                continue;
            };
            let id = format!(
                "{}@{}",
                plugin.file_name().to_string_lossy(),
                marketplace.file_name().to_string_lossy()
            );
            // Absence from config means that the desktop app's default applies,
            // which is enabled for an installed plugin.
            let is_enabled = enabled
                .get(&id)
                .and_then(|plugin| plugin.get("enabled"))
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            add_codex_plugin_metadata(
                &mut plugins,
                id,
                &version.path(),
                is_enabled,
                &enabled,
                &config,
            );
        }
    }
    plugins
        .into_iter()
        .map(
            |(id, (name, description, icon, source, enabled, team_required, capabilities))| {
                Plugin {
                    id,
                    name,
                    description,
                    icon,
                    source,
                    enabled,
                    team_required,
                    capabilities,
                }
            },
        )
        .collect()
}

fn update_codex_plugin_enabled(
    path: &PathBuf,
    id: &str,
    enabled: bool,
) -> std::result::Result<(), String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let header = format!("[plugins.\"{id}\"]");
    let Some(start) = content.lines().position(|line| line.trim() == header) else {
        let separator = if content.is_empty() || content.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        return fs::write(
            path,
            format!("{content}{separator}\n{header}\nenabled = {enabled}\n"),
        )
        .map_err(|error| error.to_string());
    };
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| line.trim_start().starts_with('[').then_some(index))
        .unwrap_or(lines.len());
    if let Some(index) =
        (start + 1..end).find(|index| lines[*index].trim_start().starts_with("enabled"))
    {
        lines[index] = format!("enabled = {enabled}");
    } else {
        lines.insert(end, format!("enabled = {enabled}"));
    }
    fs::write(path, format!("{}\n", lines.join("\n"))).map_err(|error| error.to_string())
}

fn update_codex_plugin_mcp_enabled(
    path: &PathBuf,
    plugin_id: &str,
    server_id: &str,
    enabled: bool,
) -> std::result::Result<(), String> {
    let header = format!(
        "[plugins.{}.mcp_servers.{}]",
        toml_key(plugin_id),
        toml_key(server_id)
    );
    update_toml_enabled(path, &header, enabled)
}

fn update_codex_skill_enabled(
    path: &PathBuf,
    skill_path: &str,
    enabled: bool,
) -> std::result::Result<(), String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    let quoted_path = toml_value(skill_path);
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() == "[[skills.config]]" {
            let end = lines
                .iter()
                .enumerate()
                .skip(index + 1)
                .find_map(|(position, line)| line.trim_start().starts_with('[').then_some(position))
                .unwrap_or(lines.len());
            if lines[index + 1..end].iter().any(|line| {
                line.trim_start().starts_with("path")
                    && line
                        .split_once('=')
                        .is_some_and(|(_, value)| value.trim() == quoted_path)
            }) {
                if let Some(position) = (index + 1..end)
                    .find(|position| lines[*position].trim_start().starts_with("enabled"))
                {
                    lines[position] = format!("enabled = {enabled}");
                } else {
                    lines.insert(end, format!("enabled = {enabled}"));
                }
                return fs::write(path, format!("{}\n", lines.join("\n")))
                    .map_err(|error| error.to_string());
            }
            index = end;
        } else {
            index += 1;
        }
    }
    let separator = if content.is_empty() || content.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::write(
        path,
        format!(
            "{content}{separator}\n[[skills.config]]\npath = {quoted_path}\nenabled = {enabled}\n"
        ),
    )
    .map_err(|error| error.to_string())
}

fn update_toml_enabled(
    path: &PathBuf,
    header: &str,
    enabled: bool,
) -> std::result::Result<(), String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let Some(start) = content.lines().position(|line| line.trim() == header) else {
        let separator = if content.is_empty() || content.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        return fs::write(
            path,
            format!("{content}{separator}\n{header}\nenabled = {enabled}\n"),
        )
        .map_err(|error| error.to_string());
    };
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| line.trim_start().starts_with('[').then_some(index))
        .unwrap_or(lines.len());
    if let Some(index) =
        (start + 1..end).find(|index| lines[*index].trim_start().starts_with("enabled"))
    {
        lines[index] = format!("enabled = {enabled}");
    } else {
        lines.insert(end, format!("enabled = {enabled}"));
    }
    fs::write(path, format!("{}\n", lines.join("\n"))).map_err(|error| error.to_string())
}

fn toml_key(value: &str) -> String {
    toml_value(value)
}
fn toml_value(value: &str) -> String {
    toml::Value::String(value.into()).to_string()
}

fn add_codex_plugin_metadata(
    plugins: &mut std::collections::BTreeMap<
        String,
        (
            String,
            Option<String>,
            Option<String>,
            String,
            bool,
            bool,
            Vec<PluginCapability>,
        ),
    >,
    id: String,
    path: &PathBuf,
    enabled: bool,
    config_plugins: &toml::map::Map<String, toml::Value>,
    config: &toml::Value,
) {
    let (name, description, icon) = read_plugin_metadata(path, &id);
    let manifest = codex_plugin_manifest(path);
    let plugin_config = config_plugins.get(&id).and_then(toml::Value::as_table);
    let mut capabilities = collect_codex_plugin_skills(path, config);
    capabilities.extend(collect_codex_plugin_mcp(path, plugin_config));
    capabilities.extend(collect_codex_plugin_hooks(path, manifest.as_ref()));
    plugins.insert(
        id,
        (
            name,
            description,
            icon,
            "codex".into(),
            enabled,
            false,
            capabilities,
        ),
    );
}

fn collect_codex_plugin_skills(path: &PathBuf, config: &toml::Value) -> Vec<PluginCapability> {
    let overrides = config
        .get("skills")
        .and_then(toml::Value::as_table)
        .and_then(|skills| skills.get("config"))
        .and_then(toml::Value::as_array);
    let skills = path.join("skills");
    fs::read_dir(skills)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let skill = entry.path();
            let id = skill.to_string_lossy().into_owned();
            skill.join("SKILL.md").is_file().then(|| PluginCapability {
                name: entry.file_name().to_string_lossy().into_owned(),
                description: skill_description(&skill.join("SKILL.md")),
                enabled: overrides
                    .and_then(|items| {
                        items.iter().find(|item| {
                            item.get("path").and_then(toml::Value::as_str) == Some(&id)
                        })
                    })
                    .and_then(|item| item.get("enabled"))
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(true),
                id,
                kind: "skill".into(),
            })
        })
        .collect()
}

fn collect_codex_plugin_mcp(
    path: &PathBuf,
    plugin_config: Option<&toml::map::Map<String, toml::Value>>,
) -> Vec<PluginCapability> {
    let Some(mcp) = fs::read(path.join(".mcp.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return Vec::new();
    };
    let Some(servers) = mcp
        .get("mcp_servers")
        .or(Some(&mcp))
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    servers
        .iter()
        .map(|(id, server)| PluginCapability {
            id: id.clone(),
            name: id.clone(),
            description: server
                .get("description")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            kind: "mcp".into(),
            enabled: plugin_config
                .and_then(|config| config.get("mcp_servers"))
                .and_then(toml::Value::as_table)
                .and_then(|servers| servers.get(id))
                .and_then(toml::Value::as_table)
                .and_then(|server| server.get("enabled"))
                .and_then(toml::Value::as_bool)
                .unwrap_or(true),
        })
        .collect()
}

fn collect_codex_plugin_hooks(
    path: &PathBuf,
    manifest: Option<&serde_json::Value>,
) -> Vec<PluginCapability> {
    let hook_files = manifest
        .and_then(|value| value.get("hooks"))
        .map(|hooks| match hooks {
            serde_json::Value::String(value) => vec![value.as_str()],
            serde_json::Value::Array(values) => values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect(),
            _ => Vec::new(),
        })
        .unwrap_or_else(|| vec!["./hooks/hooks.json"]);
    let root = path.canonicalize().ok();
    hook_files
        .into_iter()
        .flat_map(|hook_file| {
            let candidate = path.join(hook_file).canonicalize().ok();
            let Some(candidate) = candidate.filter(|candidate| {
                root.as_ref()
                    .is_some_and(|root| candidate.starts_with(root))
            }) else {
                return Vec::new();
            };
            let Some(value) = fs::read(&candidate)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            else {
                return Vec::new();
            };
            let label = candidate
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("hooks.json")
                .to_owned();
            value
                .get("hooks")
                .and_then(serde_json::Value::as_object)
                .into_iter()
                .flat_map(|hooks| hooks.iter())
                .map(|(event, definitions)| PluginCapability {
                    id: format!("{label}:{event}"),
                    name: event.clone(),
                    kind: "hook".into(),
                    enabled: true,
                    description: definitions.as_array().map(|items| {
                        format!(
                            "{label} - {} handler{}",
                            items.len(),
                            if items.len() == 1 { "" } else { "s" }
                        )
                    }),
                })
                .collect()
        })
        .collect()
}

fn codex_plugin_manifest(path: &PathBuf) -> Option<serde_json::Value> {
    plugin_manifest(path)
}

fn plugin_manifest(path: &PathBuf) -> Option<serde_json::Value> {
    [
        ".codex-plugin/plugin.json",
        ".cursor-plugin/plugin.json",
        ".claude-plugin/plugin.json",
        "plugin.json",
    ]
    .into_iter()
    .map(|file| path.join(file))
    .find(|file| file.is_file())
    .and_then(|file| fs::read(file).ok())
    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn skill_description(path: &PathBuf) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let mut lines = content.lines();
    (lines.next()?.trim() == "---").then_some(())?;
    let frontmatter: Vec<&str> = lines.take_while(|line| line.trim() != "---").collect();
    let description_index = frontmatter
        .iter()
        .position(|line| line.starts_with("description:"))?;
    let value = frontmatter[description_index]
        .strip_prefix("description:")?
        .trim();
    if value == ">" || value == "|" {
        let description = frontmatter[description_index + 1..]
            .iter()
            .take_while(|line| line.starts_with(' ') || line.starts_with('\t'))
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" ");
        return (!description.is_empty()).then_some(description);
    }
    (!value.is_empty()).then_some(value.trim_matches('"').to_owned())
}

fn collect_mcp_servers(kind: ApplicationKind) -> Vec<McpServer> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    match kind {
        ApplicationKind::Codex => {
            fs::read_to_string(PathBuf::from(home).join(".codex/config.toml"))
                .ok()
                .and_then(|content| content.parse::<toml::Value>().ok())
                .and_then(|value| {
                    value
                        .get("mcp_servers")
                        .and_then(toml::Value::as_table)
                        .cloned()
                })
                .map(|servers| {
                    servers
                        .keys()
                        .cloned()
                        .map(|id| McpServer {
                            name: id.clone(),
                            id,
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        ApplicationKind::Cursor => Vec::new(),
    }
}

fn collect_cursor_plugins(session: Option<Session>) -> Vec<Plugin> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let home = PathBuf::from(home);
    let mut plugins = std::collections::BTreeMap::new();
    if let Some(session) = session {
        if let Ok(marketplace_plugins) = cursor_marketplace_plugins(&session) {
            for plugin in marketplace_plugins {
                let cached = marketplace_plugin_cache(&home, &plugin.slug);
                let metadata = cached
                    .as_ref()
                    .map(|path| read_plugin_metadata(path, &plugin.name));
                plugins.insert(
                    plugin.id.to_string(),
                    (
                        plugin.name,
                        metadata
                            .as_ref()
                            .and_then(|(_, description, _)| description.clone()),
                        plugin
                            .icon
                            .or_else(|| metadata.as_ref().and_then(|(_, _, icon)| icon.clone())),
                        "marketplace".into(),
                        plugin.enabled,
                        plugin.team_required,
                        cached
                            .as_ref()
                            .map(collect_plugin_capabilities)
                            .unwrap_or_default(),
                    ),
                );
            }
        }
    }
    let local = home.join(".cursor/plugins/local");
    if let Ok(entries) = fs::read_dir(local) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                add_plugin_metadata(
                    &mut plugins,
                    entry.file_name().to_string_lossy().into_owned(),
                    &entry.path(),
                    "local",
                    true,
                );
            }
        }
    }
    let disabled_local = home.join(".cursor/plugins/disabled");
    if let Ok(entries) = fs::read_dir(disabled_local) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                add_plugin_metadata(
                    &mut plugins,
                    entry.file_name().to_string_lossy().into_owned(),
                    &entry.path(),
                    "local",
                    false,
                );
            }
        }
    }
    let claude_root = home.join(".claude");
    let claude_settings = read_json(claude_root.join("settings.json")).unwrap_or_default();
    if let Some(installed) =
        read_json(claude_root.join("plugins/installed_plugins.json")).and_then(|value| {
            value
                .get("plugins")
                .and_then(serde_json::Value::as_object)
                .cloned()
        })
    {
        for (id, installs) in installed {
            let Some(items) = installs.as_array() else {
                continue;
            };
            let Some(install) = items
                .iter()
                .find(|item| item.get("scope").and_then(serde_json::Value::as_str) == Some("user"))
            else {
                continue;
            };
            let Some(path) = install
                .get("installPath")
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
            else {
                continue;
            };
            if path.is_dir() {
                let enabled = claude_settings
                    .get("enabledPlugins")
                    .and_then(|value| value.get(&id))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                add_plugin_metadata(&mut plugins, id, &path, "claude", enabled);
            }
        }
    }
    plugins
        .into_iter()
        .map(
            |(id, (name, description, icon, source, enabled, team_required, capabilities))| {
                Plugin {
                    id,
                    name,
                    description,
                    icon,
                    source,
                    enabled,
                    team_required,
                    capabilities,
                }
            },
        )
        .collect()
}

fn marketplace_plugin_cache(home: &PathBuf, slug: &str) -> Option<PathBuf> {
    if slug.is_empty() || slug == "." || slug == ".." || slug.contains(['/', '\\']) {
        return None;
    }
    let root = home.join(".cursor/plugins/cache").join(slug).join(slug);
    fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .max_by_key(|entry| entry.file_name())
        .map(|entry| entry.path())
}

fn read_json(path: PathBuf) -> Option<serde_json::Value> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn add_plugin_metadata(
    plugins: &mut std::collections::BTreeMap<
        String,
        (
            String,
            Option<String>,
            Option<String>,
            String,
            bool,
            bool,
            Vec<PluginCapability>,
        ),
    >,
    id: String,
    path: &PathBuf,
    source: &str,
    enabled: bool,
) {
    let (name, description, icon) = read_plugin_metadata(path, &id);
    plugins.insert(
        id,
        (
            name,
            description,
            icon,
            source.into(),
            enabled,
            false,
            collect_plugin_capabilities(path),
        ),
    );
}

// Cursor and Claude expose plugin bundle metadata but not per-capability
// controls, so their child entries are intentionally read-only in the UI.
fn collect_plugin_capabilities(path: &PathBuf) -> Vec<PluginCapability> {
    let mut capabilities = collect_plugin_skills(path);
    capabilities.extend(collect_plugin_mcp(path));
    capabilities.extend(collect_codex_plugin_hooks(
        path,
        plugin_manifest(path).as_ref(),
    ));
    capabilities
}

fn collect_plugin_skills(path: &PathBuf) -> Vec<PluginCapability> {
    fs::read_dir(path.join("skills"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let skill = entry.path();
            let skill_file = skill.join("SKILL.md");
            skill_file.is_file().then(|| PluginCapability {
                id: skill.to_string_lossy().into_owned(),
                name: entry.file_name().to_string_lossy().into_owned(),
                description: skill_description(&skill_file),
                kind: "skill".into(),
                enabled: true,
            })
        })
        .collect()
}

fn collect_plugin_mcp(path: &PathBuf) -> Vec<PluginCapability> {
    let Some(mcp) = fs::read(path.join(".mcp.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return Vec::new();
    };
    let Some(servers) = mcp
        .get("mcp_servers")
        .or(Some(&mcp))
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    servers
        .iter()
        .map(|(id, server)| PluginCapability {
            id: id.clone(),
            name: id.clone(),
            kind: "mcp".into(),
            enabled: true,
            description: server
                .get("description")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        })
        .collect()
}

fn read_plugin_metadata(path: &PathBuf, id: &str) -> (String, Option<String>, Option<String>) {
    let manifest = [
        path.join(".cursor-plugin/plugin.json"),
        path.join(".codex-plugin/plugin.json"),
        path.join(".claude-plugin/plugin.json"),
        path.join("plugin.json"),
        path.join("package.json"),
    ]
    .into_iter()
    .find(|p| p.is_file());
    let metadata = manifest
        .and_then(|manifest| fs::read(manifest).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let name = metadata
        .as_ref()
        .and_then(|value| {
            value
                .get("displayName")
                .or_else(|| value.get("name"))
                .or_else(|| {
                    value
                        .get("interface")
                        .and_then(|interface| interface.get("displayName"))
                })
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| id.to_owned());
    let icon = metadata
        .as_ref()
        .and_then(|value| plugin_manifest_icon(path, value));
    let description = metadata.as_ref().and_then(|value| {
        value
            .get("interface")
            .and_then(|interface| {
                interface
                    .get("shortDescription")
                    .or_else(|| interface.get("longDescription"))
            })
            .or_else(|| value.get("description"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    });
    (name, description, icon)
}

fn plugin_manifest_icon(plugin_root: &PathBuf, manifest: &serde_json::Value) -> Option<String> {
    let icon = manifest
        .get("icon")
        .or_else(|| manifest.get("iconPath"))
        .or_else(|| manifest.get("logo"))
        .or_else(|| {
            manifest
                .get("interface")
                .and_then(|interface| interface.get("logo"))
        })
        .or_else(|| {
            manifest
                .get("interface")
                .and_then(|interface| interface.get("composerIcon"))
        })
        .and_then(serde_json::Value::as_str)?;
    if icon.starts_with("https://") {
        return Some(icon.to_owned());
    }
    let root = plugin_root.canonicalize().ok()?;
    let candidate = root.join(icon).canonicalize().ok()?;
    (candidate.starts_with(&root) && is_plugin_image(&candidate))
        .then(|| candidate.to_string_lossy().into_owned())
}

fn is_plugin_image(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("avif" | "gif" | "ico" | "jpeg" | "jpg" | "png" | "svg" | "webp")
    )
}

#[tauri::command]
pub(crate) fn set_cursor_plugin_enabled(
    id: String,
    source: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    if source == "claude" {
        return set_claude_plugin_enabled(id, enabled);
    }
    if source == "marketplace" {
        let plugin_id = id
            .parse()
            .map_err(|_| "无效的 Marketplace 插件标识。".to_string())?;
        let session = state
            .0
            .lock()
            .map_err(|_| "应用状态不可用".to_string())?
            .current_cursor_session()
            .map_err(error_text)?;
        return set_cursor_marketplace_plugin_enabled(&session, plugin_id, enabled)
            .map_err(error_text);
    }
    if source != "local" {
        return Err("不支持此插件来源。".into());
    }
    if id.is_empty() || id.contains(['/', '\\']) || id == "." || id == ".." {
        return Err("无效的插件标识。".into());
    }
    let Some(home) = std::env::var_os("HOME") else {
        return Err("无法定位用户目录".into());
    };
    let root = PathBuf::from(home).join(".cursor/plugins");
    let active = root.join("local").join(&id);
    let disabled = root.join("disabled").join(&id);
    let (from, to) = if enabled {
        (disabled, active)
    } else {
        (active, disabled)
    };
    if !from.is_dir() {
        return Err("找不到本地插件。".into());
    }
    if to.exists() {
        return Err("插件目标位置已存在。".into());
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(from, to).map_err(|e| e.to_string())
}

fn set_claude_plugin_enabled(id: String, enabled: bool) -> std::result::Result<(), String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Err("无法定位用户目录".into());
    };
    let path = PathBuf::from(home).join(".claude/settings.json");
    let mut settings = match read_json(path.clone()) {
        Some(serde_json::Value::Object(value)) => value,
        Some(_) => return Err("Claude 插件设置格式无效。".into()),
        None => serde_json::Map::new(),
    };
    let plugins = settings
        .entry("enabledPlugins")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| "Claude 插件设置格式无效。".to_string())?;
    plugins.insert(id, serde_json::Value::Bool(enabled));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::Value::Object(settings))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn delete_cursor_plugin(
    id: String,
    source: String,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    if source == "marketplace" {
        let plugin_id = id
            .parse()
            .map_err(|_| "无效的 Marketplace 插件标识。".to_string())?;
        let session = state
            .0
            .lock()
            .map_err(|_| "应用状态不可用".to_string())?
            .current_cursor_session()
            .map_err(error_text)?;
        return uninstall_cursor_marketplace_plugin(&session, plugin_id).map_err(error_text);
    }
    if source != "local" {
        return Err("不支持此插件来源。".into());
    }
    let Some(home) = std::env::var_os("HOME") else {
        return Err("无法定位用户目录".into());
    };
    let root = PathBuf::from(home).join(".cursor/plugins");
    let target = [
        root.join("local").join(&id),
        root.join("disabled").join(&id),
    ]
    .into_iter()
    .find(|path| path.is_dir())
    .ok_or_else(|| "此插件不是可由本机目录删除的本地插件。".to_string())?;
    fs::remove_dir_all(target).map_err(|e| e.to_string())?;
    Ok(())
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
    controller
        .export_cursor_account(&account)
        .map_err(error_text)
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
                let _ = state
                    .0
                    .lock()
                    .ok()
                    .and_then(|mut controller| controller.mark_token_invalid(&id).ok());
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
struct RefreshAccountsProgress {
    completed: usize,
    total: usize,
}

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
        for session in sessions {
            let _ = job_sender.send(session);
        }
        drop(job_sender);
        let job_receiver = Arc::new(StdMutex::new(job_receiver));
        let workers = usize::min(3, session_count);
        let jobs = (0..workers)
            .map(|_| {
                let jobs = Arc::clone(&job_receiver);
                let results = result_sender.clone();
                thread::spawn(move || loop {
                    let Some((id, session)) =
                        jobs.lock().ok().and_then(|receiver| receiver.recv().ok())
                    else {
                        break;
                    };
                    let refreshed = fetch_cursor_subscription_fast(&session).map(|summary| {
                        // The account list needs a compact quota value, not the full usage history.
                        let usage = dashboard_cookie(&session)
                            .ok()
                            .and_then(|cookie| {
                                dashboard_request(&cookie, "/usage-summary", None).ok()
                            })
                            .map(|response| usage_pools(&response, response.get("hard_limit")).0);
                        (summary, usage)
                    });
                    let _ = results.send((id, refreshed));
                })
            })
            .collect::<Vec<_>>();
        drop(result_sender);
        let mut updates = Vec::new();
        let mut failures = Vec::new();
        for completed in 1..=session_count {
            match result_receiver.recv() {
                Ok((id, Ok(summary))) => updates.push((id, summary)),
                Ok((id, Err(error))) => failures.push((id, error.to_string())),
                Err(_) => {
                    failures.push((String::new(), "刷新线程异常退出".into()));
                    break;
                }
            }
            let _ = progress_app.emit(
                "account-refresh-progress",
                RefreshAccountsProgress { completed, total },
            );
        }
        for job in jobs {
            let _ = job.join();
        }
        (updates, failures)
    })
    .await
    .map_err(|error| error.to_string())?;
    let (updates, failures) = result;
    let invalid = failures
        .iter()
        .filter(|(_, message)| message.contains("失效") || message.contains("过期"))
        .count();
    let missing = missing_credentials.len();
    let failed = failures.len() + missing;
    let mut controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
    for id in missing_credentials {
        let _ = controller.mark_credential_missing(&id);
    }
    for (id, message) in failures {
        if !id.is_empty() && (message.contains("失效") || message.contains("过期")) {
            let _ = controller.mark_token_invalid(&id);
        }
    }
    for (id, (summary, usage)) in updates {
        controller
            .save_subscription(&id, summary)
            .map_err(error_text)?;
        if let Some(usage) = usage {
            controller
                .save_cursor_usage_summary(&id, usage)
                .map_err(error_text)?;
        }
    }
    let _ = app.emit("accounts-changed", ());
    Ok(RefreshAccountsResult {
        total,
        failed,
        invalid,
        missing,
    })
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
    let usage_result =
        tauri::async_runtime::spawn_blocking(move || fetch_cursor_usage(&account, &session))
            .await
            .map_err(|error| error.to_string())?;
    let (usage, raw) = match usage_result {
        Ok(value) => value,
        Err(error) => {
            let message = error.to_string();
            if message.contains("失效") || message.contains("过期") {
                if let Ok(mut controller) = state.0.lock() {
                    let _ = controller.mark_token_invalid(&id);
                }
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
pub(crate) async fn import_token_or_json(
    kind: ApplicationKind,
    label: Option<String>,
    payload: String,
    app: AppHandle,
) -> std::result::Result<Account, String> {
    if kind != ApplicationKind::Cursor {
        return Err(AppError::ComingSoon.to_string());
    }
    let import_app = app.clone();
    let (account, usage_session) = tauri::async_runtime::spawn_blocking(move || {
        let mut session = Session::from_import(&payload).map_err(error_text)?;
        // An exported session's membership cache can be stale (commonly
        // "enterprise"). The background usage request is the source of truth.
        session.values.remove(MEMBERSHIP_TYPE_KEY);
        let import_type = import_type(&session);
        let import_state = import_app.state::<AppState>();
        let mut controller = import_state
            .0
            .lock()
            .map_err(|_| "账户存储不可用".to_string())?;
        let account = controller
            .save_imported_session(kind, label, session, import_type)
            .map_err(error_text)?;
        let account_id = account.id.clone();
        let account = controller.account(&account_id).unwrap_or(account);
        let session = controller
            .subscription_session(&account_id)
            .map_err(error_text)?;
        Ok::<(Account, (Account, Session)), String>((account.clone(), (account, session)))
    })
    .await
    .map_err(|error| error.to_string())??;
    refresh_tray(&app);
    let _ = app.emit("accounts-changed", ());
    {
        let usage_app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let (usage_account, usage_session) = usage_session;
            match fetch_cursor_usage(&usage_account, &usage_session) {
                Ok((usage, raw)) => {
                    if let Ok(mut controller) = usage_app.state::<AppState>().0.lock() {
                        let _ = controller.save_cursor_usage(&usage_account.id, usage, raw);
                    }
                }
                Err(error)
                    if error.to_string().contains("失效") || error.to_string().contains("过期") =>
                {
                    if let Ok(mut controller) = usage_app.state::<AppState>().0.lock() {
                        let _ = controller.mark_token_invalid(&usage_account.id);
                    }
                }
                Err(_) => {}
            }
            let _ = usage_app.emit("accounts-changed", ());
        });
    }
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
pub(crate) fn open_official_login_url(
    state: State<'_, OauthLoginState>,
) -> std::result::Result<(), String> {
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
