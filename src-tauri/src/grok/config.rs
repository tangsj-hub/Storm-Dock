use std::{fs, path::PathBuf};

use toml::Value;

use crate::error::{AppError, Result};

pub(crate) const DEFAULT_MODEL: &str = "grok-4.5";
pub(crate) const DEFAULT_API_BACKEND: &str = "responses";
pub(crate) const DEFAULT_CONTEXT_WINDOW: i64 = 500_000;
pub(crate) const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";

pub(crate) fn grok_home() -> Option<PathBuf> {
    std::env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".grok")))
}

pub(crate) fn default_config_path() -> Option<PathBuf> {
    grok_home().map(|home| home.join("config.toml"))
}

pub(crate) fn read_text(path: &PathBuf) -> Result<String> {
    if path.exists() {
        Ok(fs::read_to_string(path)?)
    } else {
        Ok(String::new())
    }
}

pub(crate) fn write_text(path: &PathBuf, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn parse_table(config_text: &str) -> Result<toml::map::Map<String, Value>> {
    if config_text.trim().is_empty() {
        return Ok(toml::map::Map::new());
    }
    config_text
        .parse::<Value>()
        .map_err(|error| AppError::Message(format!("config.toml 无效: {error}")))?
        .as_table()
        .cloned()
        .ok_or_else(|| AppError::Message("config.toml 无效".into()))
}

fn encode_table(table: toml::map::Map<String, Value>) -> Result<String> {
    if table.is_empty() {
        return Ok(String::new());
    }
    toml::to_string(&Value::Table(table))
        .map_err(|error| AppError::Message(format!("无法写入 config.toml: {error}")))
}

pub(crate) fn is_official_live_config(config_toml: &str) -> bool {
    if config_toml.trim().is_empty() {
        return true;
    }
    let Ok(document) = config_toml.parse::<Value>() else {
        return false;
    };
    document
        .as_table()
        .is_some_and(|root| !root.contains_key("models") && !root.contains_key("model"))
}

pub(crate) fn apply_model_route(
    config_text: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    name: Option<&str>,
) -> Result<String> {
    let mut table = parse_table(config_text)?;
    match api_key.map(str::trim).filter(|value| !value.is_empty()) {
        Some(api_key) => {
            let profile = DEFAULT_MODEL.to_string();
            let mut models = table
                .remove("models")
                .and_then(|value| value.as_table().cloned())
                .unwrap_or_default();
            models.insert("default".into(), Value::String(profile.clone()));
            table.insert("models".into(), Value::Table(models));

            let mut model_tables = table
                .remove("model")
                .and_then(|value| value.as_table().cloned())
                .unwrap_or_default();
            model_tables.retain(|key, _| key == &profile);
            let mut selected = model_tables
                .remove(&profile)
                .and_then(|value| value.as_table().cloned())
                .unwrap_or_default();
            selected.insert("model".into(), Value::String(profile.clone()));
            selected.insert(
                "base_url".into(),
                Value::String(
                    base_url
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or(DEFAULT_BASE_URL)
                        .trim_end_matches('/')
                        .to_string(),
                ),
            );
            selected.insert(
                "name".into(),
                Value::String(
                    name.map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("custom")
                        .to_string(),
                ),
            );
            selected.insert("api_key".into(), Value::String(api_key.to_string()));
            selected.insert(
                "api_backend".into(),
                Value::String(DEFAULT_API_BACKEND.into()),
            );
            selected.insert("context_window".into(), Value::Integer(DEFAULT_CONTEXT_WINDOW));
            selected.remove("env_key");
            model_tables.insert(profile, Value::Table(selected));
            table.insert("model".into(), Value::Table(model_tables));
        }
        None => {
            table.remove("models");
            table.remove("model");
        }
    }
    encode_table(table)
}

fn selected_model<'a>(table: &'a toml::map::Map<String, Value>) -> Option<&'a toml::map::Map<String, Value>> {
    let profile = table
        .get("models")?
        .get("default")?
        .as_str()?
        .trim();
    table.get("model")?.get(profile)?.as_table()
}

pub(crate) fn extract_api_key(config_text: &str) -> Option<String> {
    let table = parse_table(config_text).ok()?;
    selected_model(&table)?
        .get("api_key")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn extract_base_url(config_text: &str) -> Option<String> {
    let table = parse_table(config_text).ok()?;
    selected_model(&table)?
        .get("base_url")?
        .as_str()
        .map(str::trim)
        .map(|value| value.trim_end_matches('/'))
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_config_has_no_model_table() {
        assert!(is_official_live_config(""));
        assert!(is_official_live_config("[cli]\ninstaller = \"internal\"\n"));
        assert!(!is_official_live_config("[models]\ndefault = \"grok-4.5\"\n"));
        assert!(!is_official_live_config("not = [valid"));
    }

    #[test]
    fn writes_model_table_without_dropping_cli() {
        let original = "[cli]\ninstaller = \"internal\"\n\n[marketplace]\ndefault_skills_installs_purged = true\n";
        let with_key = apply_model_route(
            original,
            Some("sk-live"),
            Some("https://api.example.com/v1"),
            Some("custom"),
        )
        .unwrap();
        assert!(with_key.contains("installer"));
        assert!(with_key.contains("marketplace"));
        assert_eq!(extract_api_key(&with_key).as_deref(), Some("sk-live"));
        assert_eq!(
            extract_base_url(&with_key).as_deref(),
            Some("https://api.example.com/v1")
        );
        assert!(!is_official_live_config(&with_key));

        let cleared = apply_model_route(&with_key, None, None, None).unwrap();
        assert!(cleared.contains("installer"));
        assert!(cleared.contains("marketplace"));
        assert!(is_official_live_config(&cleared));
        assert_eq!(extract_api_key(&cleared), None);
    }

    #[test]
    fn default_base_url_when_omitted() {
        let text = apply_model_route("", Some("sk-default"), None, None).unwrap();
        assert_eq!(extract_base_url(&text).as_deref(), Some(DEFAULT_BASE_URL));
    }
}
