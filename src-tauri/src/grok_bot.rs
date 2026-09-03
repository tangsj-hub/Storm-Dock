use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use aes::Aes128;
use base64::{engine::general_purpose::STANDARD, Engine};
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac;
use serde::Serialize;
use serde_json::Value;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::{
    cursor::session::jwt_claims,
    error::{AppError, Result},
    models::{Session, ACCESS_TOKEN_KEY},
};

const ACCOUNTS_KEY: &str = "cursor-accounts";
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(150);
static OPERATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchPreparation {
    pub(crate) status: &'static str,
}

enum SessionTarget<'a> {
    Same,
    Different {
        access: &'a str,
        refresh: &'a str,
        sub: String,
        email: String,
        slot: String,
    },
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::{path::Path, process::Command};
    const APP_PATH: &str = "/Applications/Grok Bot.app";
    const APP_NAME: &str = "Grok Bot";
    const SAFE_STORAGE_SERVICE: &str = "Grok Bot Safe Storage";

    pub(crate) fn data_path() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|home| home.join("Library/Application Support/Grok Bot/sand-secrets.json"))
            .ok_or_else(|| AppError::Message("无法读取用户目录。".into()))
    }
    pub(crate) fn ensure_installed() -> Result<()> {
        if Path::new(APP_PATH).is_dir() {
            Ok(())
        } else {
            Err(AppError::Message("未安装 Grok Bot。".into()))
        }
    }
    pub(crate) fn keychain_password() -> Result<String> {
        let output = Command::new("security")
            .args(["find-generic-password", "-s", SAFE_STORAGE_SERVICE, "-w"])
            .output()
            .map_err(|e| AppError::Message(format!("无法访问 Grok Bot 钥匙串: {e}")))?;
        if !output.status.success() {
            return Err(AppError::Message(
                "未授权访问 Grok Bot Safe Storage。".into(),
            ));
        }
        String::from_utf8(output.stdout)
            .map(|v| v.trim().to_owned())
            .map_err(|_| AppError::Message("Grok Bot 钥匙串格式无效。".into()))
    }
    pub(crate) fn is_running() -> bool {
        Command::new("pgrep")
            .args(["-x", APP_NAME])
            .status()
            .is_ok_and(|s| s.success())
    }
    pub(crate) fn launch() -> Result<()> {
        ensure_installed()?;
        Command::new("open")
            .args(["-a", APP_NAME])
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::Message(format!("无法启动 Grok Bot: {e}")))
    }
    pub(crate) fn quit_and_wait() -> Result<()> {
        let status = Command::new("osascript")
            .args(["-e", "tell application \"Grok Bot\" to quit"])
            .status()
            .map_err(|e| AppError::Message(format!("无法正常退出 Grok Bot: {e}")))?;
        if !status.success() {
            return Err(AppError::Message("Grok Bot 未能接受正常退出请求。".into()));
        }
        let deadline = Instant::now() + QUIT_TIMEOUT;
        while Instant::now() < deadline {
            if !is_running() {
                return Ok(());
            }
            thread::sleep(POLL_INTERVAL);
        }
        Err(AppError::Message(
            "Grok Bot 未在等待时间内退出，未修改登录会话。".into(),
        ))
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    pub(crate) fn unsupported<T>() -> Result<T> {
        Err(AppError::Message(
            "Windows Grok Bot 适配尚未实现，未修改任何会话数据。".into(),
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;
    pub(crate) fn unsupported<T>() -> Result<T> {
        Err(AppError::Message("当前平台暂不支持 Grok Bot。".into()))
    }
}

fn account_slot(sub: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"sand-account-slot\0");
    digest.update(sub.as_bytes());
    format!("{:x}", digest.finalize())
}
fn encrypt(value: &str, password: &str) -> String {
    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), b"saltysalt", 1003, &mut key);
    let mut bytes = value.as_bytes().to_vec();
    let len = bytes.len();
    bytes.resize(len + 16, 0);
    let encrypted = cbc::Encryptor::<Aes128>::new((&key).into(), (&[b' '; 16]).into())
        .encrypt_padded_mut::<Pkcs7>(&mut bytes, len)
        .expect("padding has capacity");
    let mut payload = b"v10".to_vec();
    payload.extend_from_slice(encrypted);
    STANDARD.encode(payload)
}

fn session_target<'a>(session: &'a Session, root: &Value) -> Result<SessionTarget<'a>> {
    let access = session
        .values
        .get(ACCESS_TOKEN_KEY)
        .ok_or(AppError::SecretMissing)?;
    let refresh = session
        .values
        .get("cursorAuth/refreshToken")
        .ok_or(AppError::SecretMissing)?;
    let claims =
        jwt_claims(access).ok_or_else(|| AppError::Message("Cursor 登录凭据无效。".into()))?;
    let sub = claims
        .get("sub")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::Message("Cursor 登录凭据缺少账号标识。".into()))?
        .to_owned();
    let email = claims
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let slot = account_slot(&sub);
    let active = root
        .get(ACCOUNTS_KEY)
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|accounts| {
            accounts
                .get("active")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    if active.as_deref() == Some(&slot) {
        Ok(SessionTarget::Same)
    } else {
        Ok(SessionTarget::Different {
            access,
            refresh,
            sub,
            email,
            slot,
        })
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn prepare_for_session(session: &Session) -> Result<LaunchPreparation> {
    platform::ensure_installed()?;
    let path = platform::data_path()?;
    let root: Value = serde_json::from_slice(
        &fs::read(path).map_err(|_| AppError::Message("未检测到 Grok Bot 登录数据。".into()))?,
    )?;
    match session_target(session, &root)? {
        SessionTarget::Same => {
            platform::launch()?;
            Ok(LaunchPreparation { status: "same" })
        }
        SessionTarget::Different { .. } if platform::is_running() => Ok(LaunchPreparation {
            status: "requiresConfirmation",
        }),
        SessionTarget::Different { .. } => Ok(LaunchPreparation { status: "ready" }),
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn prepare_for_session(_: &Session) -> Result<LaunchPreparation> {
    platform::unsupported()
}

#[cfg(target_os = "macos")]
pub(crate) fn confirm_for_session(session: &Session) -> Result<()> {
    let _guard = OPERATION_LOCK
        .lock()
        .map_err(|_| AppError::Message("Grok Bot 操作锁不可用。".into()))?;
    platform::ensure_installed()?;
    let path = platform::data_path()?;
    let original =
        fs::read(&path).map_err(|_| AppError::Message("未检测到 Grok Bot 登录数据。".into()))?;
    let mut root: Value = serde_json::from_slice(&original)?;
    let target = session_target(session, &root)?;
    if matches!(target, SessionTarget::Same) {
        platform::launch()?;
        return Ok(());
    }
    if platform::is_running() {
        platform::quit_and_wait()?;
    }
    let SessionTarget::Different {
        access,
        refresh,
        sub,
        email,
        slot,
    } = target
    else {
        unreachable!()
    };
    let password = platform::keychain_password()?;
    let accounts: Value = root
        .get(ACCOUNTS_KEY)
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_else(|| serde_json::json!({"accounts": {}}));
    let mut account_map: BTreeMap<String, Value> = accounts
        .get("accounts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    account_map.insert(slot.clone(), serde_json::json!({"cursor-access-token": encrypt(access, &password), "cursor-refresh-token": encrypt(refresh, &password), "cursor-account-profile": encrypt(&serde_json::json!({"authId": sub, "email": email}).to_string(), &password)}));
    root[ACCOUNTS_KEY] =
        Value::String(serde_json::json!({"active": slot, "accounts": account_map}).to_string());
    let encoded = serde_json::to_vec_pretty(&root)?;
    let _: Value = serde_json::from_slice(&encoded)
        .map_err(|_| AppError::Message("Grok Bot 登录数据验证失败。".into()))?;
    let temp = path.with_file_name(format!(".storm-dock-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        fs::rename(&temp, &path)?;
        platform::launch()
    })();
    if result.is_err() {
        let _ = fs::write(&path, &original);
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn confirm_for_session(_: &Session) -> Result<()> {
    platform::unsupported()
}

#[cfg(test)]
mod tests {
    use super::account_slot;
    #[test]
    fn account_slot_is_stable() {
        assert_eq!(
            account_slot("cursor-user-123"),
            "e52340e7310f5a441a1f8f2710dacffd1f6af8167f868bd19c9b61214f67ca22"
        );
    }
}
