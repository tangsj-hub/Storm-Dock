use std::{fs, path::PathBuf};

use crate::error::{AppError, Result};

pub(crate) fn default_auth_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".grok").join("auth.json"))
}

pub(crate) fn read_auth(path: &PathBuf) -> Result<serde_json::Value> {
    if !path.exists() {
        return Err(AppError::GrokNotDetected);
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub(crate) fn write_auth(path: &PathBuf, auth: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(auth)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, &bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

pub(crate) fn restore_bytes(path: &PathBuf, previous: Option<&[u8]>) -> Result<()> {
    match previous {
        Some(bytes) => {
            fs::write(path, bytes)?;
            Ok(())
        }
        None if path.exists() => {
            fs::remove_file(path)?;
            Ok(())
        }
        None => Ok(()),
    }
}

pub(crate) fn read_bytes(path: &PathBuf) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    #[test]
    fn writes_and_restores_auth_json() {
        let path = env::temp_dir().join(format!(
            "storm-dock-grok-auth-{}.json",
            uuid::Uuid::new_v4()
        ));
        let _ = fs::remove_file(&path);
        let auth = serde_json::json!({ "https://auth.x.ai::client": { "key": "token" } });
        write_auth(&path, &auth).unwrap();
        assert_eq!(
            read_auth(&path).unwrap()["https://auth.x.ai::client"]["key"],
            "token"
        );
        restore_bytes(&path, None).unwrap();
        assert!(!path.exists());
    }
}
