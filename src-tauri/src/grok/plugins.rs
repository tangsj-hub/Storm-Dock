use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{AppError, Result};
use crate::grok::config::{grok_home, read_text, write_text};
use crate::models::{Plugin, PluginCapability};

pub(crate) fn list_plugins() -> Vec<Plugin> {
    let Some(home) = grok_home() else {
        return Vec::new();
    };
    list_plugins_from(
        &home,
        &read_text(&home.join("config.toml")).unwrap_or_default(),
    )
}

pub(crate) fn set_plugin_enabled(id: &str, enabled: bool) -> std::result::Result<(), String> {
    if !is_valid_id(id) {
        return Err("无效的插件标识。".into());
    }
    let home = grok_home().ok_or_else(|| "无法读取用户目录。".to_string())?;
    let path = home.join("config.toml");
    let next = apply_enabled(&read_text(&path).unwrap_or_default(), id, enabled)
        .map_err(|error| error.to_string())?;
    write_text(&path, &next).map_err(|error| error.to_string())
}

pub(crate) fn delete_plugin(id: &str) -> std::result::Result<(), String> {
    if !is_valid_id(id) {
        return Err("无效的插件标识。".into());
    }
    let home = grok_home().ok_or_else(|| "无法读取用户目录。".to_string())?;
    let config_path = home.join("config.toml");
    let config = read_text(&config_path).unwrap_or_default();
    let path = plugin_dir(&home, &config, id).ok_or_else(|| "插件不存在。".to_string())?;
    fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
    let next = apply_removed(&config, id).map_err(|error| error.to_string())?;
    write_text(&config_path, &next).map_err(|error| error.to_string())
}

fn list_plugins_from(home: &Path, config_text: &str) -> Vec<Plugin> {
    let enabled = string_list(config_text, "enabled");
    let disabled = string_list(config_text, "disabled");
    let mut plugins = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for dir in plugin_roots(home, config_text) {
        for path in plugin_dirs(&dir) {
            let id = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned();
            if id.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            plugins.push(plugin_from_dir(&path, &id, &enabled, &disabled));
        }
    }
    plugins
}

fn plugin_roots(home: &Path, config_text: &str) -> Vec<PathBuf> {
    let mut roots = vec![home.join("plugins")];
    for path in string_list(config_text, "paths") {
        let expanded = expand_path(home, &path);
        if !roots.iter().any(|root| root == &expanded) {
            roots.push(expanded);
        }
    }
    roots
}

fn plugin_dirs(root: &Path) -> Vec<PathBuf> {
    if looks_like_plugin(root) {
        return vec![root.to_path_buf()];
    }
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !name.starts_with('.'))
                && looks_like_plugin(path)
        })
        .collect()
}

fn plugin_dir(home: &Path, config_text: &str, id: &str) -> Option<PathBuf> {
    plugin_roots(home, config_text)
        .into_iter()
        .flat_map(|root| plugin_dirs(&root))
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(id))
}

fn looks_like_plugin(path: &Path) -> bool {
    path.join("plugin.json").is_file()
        || path.join(".grok-plugin/plugin.json").is_file()
        || path.join("skills").is_dir()
        || path.join("commands").is_dir()
        || path.join("agents").is_dir()
        || path.join(".mcp.json").is_file()
        || path.join("hooks/hooks.json").is_file()
}

fn plugin_from_dir(path: &Path, id: &str, enabled: &[String], disabled: &[String]) -> Plugin {
    let (name, description, icon) = metadata(path, id);
    Plugin {
        id: id.into(),
        name,
        description,
        icon,
        source: "grok".into(),
        enabled: is_enabled(id, enabled, disabled),
        team_required: false,
        capabilities: capabilities(path),
    }
}

fn is_enabled(id: &str, enabled: &[String], disabled: &[String]) -> bool {
    !matches_name(id, disabled) && matches_name(id, enabled)
}

fn matches_name(id: &str, values: &[String]) -> bool {
    values.iter().any(|value| {
        let value = value.trim();
        value == id || value.ends_with(&format!("/{id}"))
    })
}

fn metadata(path: &Path, id: &str) -> (String, Option<String>, Option<String>) {
    let manifest = [
        path.join(".grok-plugin/plugin.json"),
        path.join("plugin.json"),
    ]
    .into_iter()
    .find(|file| file.is_file())
    .and_then(|file| fs::read(file).ok())
    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let name = manifest
        .as_ref()
        .and_then(|value| {
            value
                .get("displayName")
                .or_else(|| value.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| id.to_owned());
    let description = manifest.as_ref().and_then(|value| {
        value
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    });
    let icon = manifest.as_ref().and_then(|value| {
        value
            .get("icon")
            .and_then(serde_json::Value::as_str)
            .map(|icon| {
                let candidate = path.join(icon);
                if candidate.is_file() {
                    candidate.display().to_string()
                } else {
                    icon.to_owned()
                }
            })
    });
    (name, description, icon)
}

fn capabilities(path: &Path) -> Vec<PluginCapability> {
    let mut items = Vec::new();
    items.extend(skills(path));
    items.extend(mcp(path));
    items.extend(hooks(path));
    items
}

fn skills(path: &Path) -> Vec<PluginCapability> {
    fs::read_dir(path.join("skills"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let skill = entry.path();
            skill.join("SKILL.md").is_file().then(|| PluginCapability {
                id: skill.display().to_string(),
                name: entry.file_name().to_string_lossy().into_owned(),
                kind: "skill".into(),
                enabled: true,
                description: skill_description(&skill.join("SKILL.md")),
            })
        })
        .collect()
}

fn mcp(path: &Path) -> Vec<PluginCapability> {
    let Some(value) = fs::read(path.join(".mcp.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return Vec::new();
    };
    let Some(servers) = value
        .get("mcpServers")
        .or_else(|| value.get("mcp_servers"))
        .or(Some(&value))
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    servers
        .iter()
        .filter(|(id, server)| *id != "mcpServers" && server.is_object())
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

fn hooks(path: &Path) -> Vec<PluginCapability> {
    let Some(value) = fs::read(path.join("hooks/hooks.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return Vec::new();
    };
    value
        .get("hooks")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flatten()
        .map(|(event, definitions)| PluginCapability {
            id: format!("hooks.json:{event}"),
            name: event.clone(),
            kind: "hook".into(),
            enabled: true,
            description: definitions.as_array().map(|items| {
                format!(
                    "hooks.json - {} handler{}",
                    items.len(),
                    if items.len() == 1 { "" } else { "s" }
                )
            }),
        })
        .collect()
}

fn skill_description(path: &Path) -> Option<String> {
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
    (!value.is_empty()).then_some(value.trim_matches('"').to_owned())
}

fn apply_enabled(config_text: &str, id: &str, enabled: bool) -> Result<String> {
    let mut table = parse_table(config_text)?;
    let plugins = table
        .entry("plugins".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let plugins = plugins
        .as_table_mut()
        .ok_or_else(|| AppError::Message("config.toml 无效".into()))?;
    let mut enabled_list = array_strings(plugins.get("enabled"));
    let mut disabled_list = array_strings(plugins.get("disabled"));
    if enabled {
        push_unique(&mut enabled_list, id);
        retain_not(&mut disabled_list, id);
    } else {
        push_unique(&mut disabled_list, id);
        retain_not(&mut enabled_list, id);
    }
    set_array(plugins, "enabled", enabled_list);
    set_array(plugins, "disabled", disabled_list);
    encode_table(table)
}

fn apply_removed(config_text: &str, id: &str) -> Result<String> {
    let mut table = parse_table(config_text)?;
    if let Some(plugins) = table.get_mut("plugins").and_then(toml::Value::as_table_mut) {
        let mut enabled_list = array_strings(plugins.get("enabled"));
        let mut disabled_list = array_strings(plugins.get("disabled"));
        retain_not(&mut enabled_list, id);
        retain_not(&mut disabled_list, id);
        set_array(plugins, "enabled", enabled_list);
        set_array(plugins, "disabled", disabled_list);
    }
    encode_table(table)
}

fn parse_table(config_text: &str) -> Result<toml::map::Map<String, toml::Value>> {
    if config_text.trim().is_empty() {
        return Ok(toml::map::Map::new());
    }
    config_text
        .parse::<toml::Value>()
        .map_err(|error| AppError::Message(format!("config.toml 无效: {error}")))?
        .as_table()
        .cloned()
        .ok_or_else(|| AppError::Message("config.toml 无效".into()))
}

fn encode_table(table: toml::map::Map<String, toml::Value>) -> Result<String> {
    if table.is_empty() {
        return Ok(String::new());
    }
    toml::to_string(&toml::Value::Table(table))
        .map_err(|error| AppError::Message(format!("无法写入 config.toml: {error}")))
}

fn string_list(config_text: &str, key: &str) -> Vec<String> {
    parse_table(config_text)
        .ok()
        .and_then(|table| {
            table
                .get("plugins")
                .and_then(toml::Value::as_table)
                .cloned()
        })
        .map(|plugins| array_strings(plugins.get(key)))
        .unwrap_or_default()
}

fn array_strings(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn set_array(table: &mut toml::map::Map<String, toml::Value>, key: &str, values: Vec<String>) {
    if values.is_empty() {
        table.remove(key);
        return;
    }
    table.insert(
        key.into(),
        toml::Value::Array(values.into_iter().map(toml::Value::String).collect()),
    );
}

fn push_unique(values: &mut Vec<String>, id: &str) {
    if !matches_name(id, values) {
        values.push(id.to_owned());
    }
}

fn retain_not(values: &mut Vec<String>, id: &str) {
    values.retain(|value| value.trim() != id && !value.trim().ends_with(&format!("/{id}")));
}

fn expand_path(home: &Path, path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| home.to_path_buf())
            .join(rest);
    }
    PathBuf::from(path)
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.contains(['\\', '\0']) && !id.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_plugin(root: &Path, name: &str) {
        let path = root.join(name);
        fs::create_dir_all(path.join("skills/demo")).unwrap();
        fs::write(
            path.join("plugin.json"),
            format!(r#"{{"name":"{name}","description":"{name} plugin"}}"#),
        )
        .unwrap();
        fs::write(
            path.join("skills/demo/SKILL.md"),
            "---\ndescription: Demo skill\n---\n",
        )
        .unwrap();
    }

    #[test]
    fn lists_enabled_plugins_and_skips_disabled_default() {
        let home =
            std::env::temp_dir().join(format!("storm-dock-grok-plugins-{}", uuid::Uuid::new_v4()));
        write_plugin(&home.join("plugins"), "demo");
        write_plugin(&home.join("plugins"), "other");
        let config = "[plugins]\nenabled = [\"demo\"]\n";
        let plugins = list_plugins_from(&home, config);
        let _ = fs::remove_dir_all(&home);
        assert_eq!(plugins.len(), 2);
        let demo = plugins.iter().find(|plugin| plugin.id == "demo").unwrap();
        let other = plugins.iter().find(|plugin| plugin.id == "other").unwrap();
        assert!(demo.enabled);
        assert!(!other.enabled);
        assert_eq!(demo.capabilities.len(), 1);
        assert_eq!(demo.capabilities[0].kind, "skill");
    }

    #[test]
    fn toggling_writes_enabled_and_disabled_arrays() {
        let next = apply_enabled("[cli]\ninstaller = \"internal\"\n", "demo", true).unwrap();
        assert!(next.contains("installer"));
        assert!(next.contains("demo"));
        let disabled = apply_enabled(&next, "demo", false).unwrap();
        assert!(disabled.contains("disabled"));
        assert!(!is_enabled(
            "demo",
            &string_list(&disabled, "enabled"),
            &string_list(&disabled, "disabled")
        ));
    }

    #[test]
    fn delete_removes_directory_and_config_entries() {
        let home = std::env::temp_dir().join(format!(
            "storm-dock-grok-plugin-del-{}",
            uuid::Uuid::new_v4()
        ));
        write_plugin(&home.join("plugins"), "demo");
        let config = "[plugins]\nenabled = [\"demo\"]\n";
        let path = plugin_dir(&home, config, "demo").unwrap();
        fs::remove_dir_all(&path).unwrap();
        let next = apply_removed(config, "demo").unwrap();
        assert!(!home.join("plugins/demo").exists());
        assert!(!string_list(&next, "enabled")
            .iter()
            .any(|value| value == "demo"));
        let _ = fs::remove_dir_all(&home);
    }
}
