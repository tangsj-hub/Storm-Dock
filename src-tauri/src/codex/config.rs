use std::{fs, path::PathBuf};

use toml::Value;

use crate::error::{AppError, Result};

const CUSTOM_PROVIDER: &str = "custom";

pub(crate) fn default_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex").join("config.toml"))
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

pub(crate) fn apply_base_url(config_text: &str, base_url: Option<&str>) -> Result<String> {
    let mut table = if config_text.trim().is_empty() {
        toml::map::Map::new()
    } else {
        config_text
            .parse::<Value>()
            .map_err(|error| AppError::Message(format!("config.toml 无效: {error}")))?
            .as_table()
            .cloned()
            .ok_or_else(|| AppError::Message("config.toml 无效".into()))?
    };
    match base_url.map(str::trim).filter(|value| !value.is_empty()) {
        Some(base_url) => {
            table.insert(
                "model_provider".into(),
                Value::String(CUSTOM_PROVIDER.into()),
            );
            let mut providers = table
                .remove("model_providers")
                .and_then(|value| value.as_table().cloned())
                .unwrap_or_default();
            let mut custom = toml::map::Map::new();
            custom.insert("name".into(), Value::String(CUSTOM_PROVIDER.into()));
            custom.insert("base_url".into(), Value::String(base_url.into()));
            custom.insert("wire_api".into(), Value::String("responses".into()));
            custom.insert("requires_openai_auth".into(), Value::Boolean(true));
            providers.insert(CUSTOM_PROVIDER.into(), Value::Table(custom));
            table.insert("model_providers".into(), Value::Table(providers));
        }
        None => {
            if table.get("model_provider").and_then(Value::as_str) == Some(CUSTOM_PROVIDER) {
                table.remove("model_provider");
            }
            if let Some(providers) = table
                .get_mut("model_providers")
                .and_then(Value::as_table_mut)
            {
                providers.remove(CUSTOM_PROVIDER);
                if providers.is_empty() {
                    table.remove("model_providers");
                }
            }
        }
    }
    Ok(toml::to_string(&Value::Table(table))
        .map_err(|error| AppError::Message(format!("无法写入 config.toml: {error}")))?)
}

pub(crate) fn active_base_url(config_text: &str) -> Option<String> {
    let table = config_text.parse::<Value>().ok()?;
    let provider = table.get("model_provider").and_then(Value::as_str)?;
    table
        .get("model_providers")?
        .get(provider)?
        .get("base_url")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_and_clears_custom_provider_without_dropping_plugins() {
        let original = "[plugins.demo]\nenabled = true\n";
        let with_url = apply_base_url(original, Some("https://api.example.com/v1")).unwrap();
        assert!(with_url.contains("model_provider"));
        assert!(with_url.contains("https://api.example.com/v1"));
        assert!(with_url.contains("demo"));
        let cleared = apply_base_url(&with_url, None).unwrap();
        assert!(!cleared.contains("model_provider"));
        assert!(cleared.contains("demo"));
        assert_eq!(active_base_url(&with_url).as_deref(), Some("https://api.example.com/v1"));
        assert_eq!(active_base_url(&cleared), None);
    }
}
