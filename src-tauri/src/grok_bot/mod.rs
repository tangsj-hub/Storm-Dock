use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(target_os = "macos")]
use aes::Aes128;
use base64::{engine::general_purpose::STANDARD, Engine};
#[cfg(target_os = "macos")]
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
#[cfg(target_os = "macos")]
use pbkdf2::pbkdf2_hmac;
use serde::Serialize;
use serde_json::Value;
#[cfg(target_os = "macos")]
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::{
    cursor::session::jwt_claims,
    error::{AppError, Result},
    models::{Session, ACCESS_TOKEN_KEY, AUTH_ID_KEY},
};

pub(crate) mod attachments;

const ACCOUNTS_KEY: &str = "cursor-accounts";
static OPERATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchPreparation {
    pub(crate) status: &'static str,
}

/// Plain Grok Bot client login state (what sand-secrets stores after encrypt).
/// Distinct from Storm-Dock Cursor account export.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokBotClientExport {
    pub(crate) kind: &'static str,
    pub(crate) version: u32,
    pub(crate) slot: String,
    pub(crate) auth_id: String,
    pub(crate) email: String,
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) profile: Value,
    /// Whether this slot is currently active in the local Grok Bot client.
    pub(crate) active_in_client: bool,
}

enum SessionTarget {
    Same,
    Different,
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    const SAFE_STORAGE_SERVICE: &str = "Grok Bot Safe Storage";

    pub(crate) fn data_path() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|home| home.join("Library/Application Support/Grok Bot/sand-secrets.json"))
            .ok_or_else(|| AppError::Message("无法读取用户目录。".into()))
    }
    pub(crate) fn ensure_installed() -> Result<()> {
        crate::desktop::ensure_installed(crate::desktop::DesktopApp::GrokBot)
    }
    fn keychain_password() -> Result<String> {
        crate::macos_native::generic_password_for_service(SAFE_STORAGE_SERVICE).ok_or_else(|| {
            AppError::Message("未授权访问 Grok Bot Safe Storage。".into())
        })
    }
    pub(crate) fn encrypt_account_fields(
        access: &str,
        refresh: &str,
        profile: &str,
    ) -> Result<(String, String, String)> {
        let password = keychain_password()?;
        Ok((
            encrypt_macos(access, &password),
            encrypt_macos(refresh, &password),
            encrypt_macos(profile, &password),
        ))
    }
    pub(crate) fn is_running() -> bool {
        crate::desktop::is_running(crate::desktop::DesktopApp::GrokBot)
    }
    pub(crate) fn launch() -> Result<()> {
        crate::desktop::launch(crate::desktop::DesktopApp::GrokBot)
    }
    pub(crate) fn quit_and_wait() -> Result<()> {
        crate::desktop::quit_and_wait(crate::desktop::DesktopApp::GrokBot)
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

fn active_slot_from_root(root: &Value) -> Option<String> {
    root.get(ACCOUNTS_KEY)
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|accounts| {
            accounts
                .get("active")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|slot| !slot.is_empty())
}

fn session_auth_id(session: &Session) -> Option<String> {
    if let Some(auth_id) = session
        .values
        .get(AUTH_ID_KEY)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
    {
        return Some(auth_id.to_owned());
    }
    let access = session.values.get(ACCESS_TOKEN_KEY)?;
    jwt_claims(access)?
        .get("sub")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Active Grok Bot account slot from the local client store, if any.
pub(crate) fn active_slot() -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let path = platform::data_path().ok()?;
        let root = read_store_root(&path).ok()?;
        active_slot_from_root(&root)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// How often the local Grok Bot client store is checked for account changes.
/// Only a file `stat` runs per tick; the file is parsed only after it changes.
#[cfg(any(target_os = "macos", target_os = "windows"))]
const ACTIVE_SLOT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn store_stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// Watch the local Grok Bot client store and emit `accounts-changed` when its
/// active account slot changes (sign-in / sign-out / account switch done in the
/// Grok Bot client itself or by Storm Dock), so the Grok Bot tab updates
/// without a manual refresh.
pub(crate) fn spawn_active_slot_watcher(app: tauri::AppHandle) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        use tauri::Emitter;
        let Ok(path) = platform::data_path() else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("grok-bot-slot-watcher".into())
            .spawn(move || {
                let mut last_stamp = store_stamp(&path);
                let mut last_active = read_store_root(&path)
                    .ok()
                    .and_then(|root| active_slot_from_root(&root));
                loop {
                    std::thread::sleep(ACTIVE_SLOT_POLL_INTERVAL);
                    let stamp = store_stamp(&path);
                    if stamp == last_stamp {
                        continue;
                    }
                    // A partially written file fails to parse: keep the old
                    // stamp so the next tick reads it again.
                    let Ok(root) = read_store_root(&path) else {
                        continue;
                    };
                    last_stamp = stamp;
                    let active = active_slot_from_root(&root);
                    if active != last_active {
                        last_active = active;
                        let _ = app.emit("accounts-changed", ());
                    }
                }
            });
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
    }
}

/// Whether this Cursor session matches the local Grok Bot client's active slot.
pub(crate) fn session_matches_active_slot(session: &Session, active: &str) -> bool {
    session_auth_id(session)
        .map(|sub| account_slot(&sub) == active)
        .unwrap_or(false)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalStatus {
    pub(crate) installed: bool,
    pub(crate) signed_in: bool,
    pub(crate) running: bool,
    pub(crate) available: bool,
    pub(crate) reason: Option<String>,
}

pub(crate) fn user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| home.join("Library/Application Support/Grok Bot"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_dir().map(|dir| dir.join("Grok Bot"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Installed / signed-in / running state for the local Grok Bot client.
pub(crate) fn local_status() -> LocalStatus {
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        return LocalStatus {
            installed: false,
            signed_in: false,
            running: false,
            available: false,
            reason: Some("当前平台暂不支持 Grok Bot。".into()),
        };
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let installed = platform::ensure_installed().is_ok();
        let signed_in = active_slot().is_some();
        let running = platform::is_running();
        let available = installed && signed_in;
        let reason = if !installed {
            Some("未安装 Grok Bot。".into())
        } else if !signed_in {
            Some("未检测到 Grok Bot 登录。".into())
        } else {
            None
        };
        LocalStatus {
            installed,
            signed_in,
            running,
            available,
            reason,
        }
    }
}

#[cfg(target_os = "macos")]
fn encrypt_macos(value: &str, password: &str) -> String {
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

#[cfg(any(target_os = "windows", test))]
fn encrypt_os_crypt_windows(value: &str, key: &[u8; 32]) -> String {
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    use ring::rand::{SecureRandom, SystemRandom};

    let unbound = UnboundKey::new(&AES_256_GCM, key).expect("32-byte OSCrypt key");
    let sealing = LessSafeKey::new(unbound);
    let mut nonce_bytes = [0u8; 12];
    SystemRandom::new().fill(&mut nonce_bytes).expect("nonce");
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = value.as_bytes().to_vec();
    sealing
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .expect("encrypt");
    let mut payload = Vec::with_capacity(3 + 12 + in_out.len());
    payload.extend_from_slice(b"v10");
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&in_out);
    STANDARD.encode(payload)
}

#[cfg(test)]
fn decrypt_os_crypt_windows(encoded: &str, key: &[u8; 32]) -> Result<String> {
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};

    let payload = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::Message("Grok Bot 密文格式无效。".into()))?;
    if payload.len() < 3 + 12 + 16 || &payload[..3] != b"v10" {
        return Err(AppError::Message("Grok Bot 密文前缀无效。".into()));
    }
    let nonce = Nonce::try_assume_unique_for_key(&payload[3..15])
        .map_err(|_| AppError::Message("Grok Bot 密文 nonce 无效。".into()))?;
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| AppError::Message("Grok Bot OSCrypt 密钥无效。".into()))?;
    let opening = LessSafeKey::new(unbound);
    let mut in_out = payload[15..].to_vec();
    let plaintext = opening
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| AppError::Message("Grok Bot 密文无法解密。".into()))?;
    String::from_utf8(plaintext.to_vec())
        .map_err(|_| AppError::Message("Grok Bot 密文不是有效文本。".into()))
}

fn write_json_file(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(value)?;
    let temp = path.with_file_name(format!(".storm-dock-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn session_target(session: &Session, root: &Value) -> Result<SessionTarget> {
    let account = client_account_from_session(session)?;
    let active = active_slot_from_root(root);
    if active.as_deref() == Some(account.slot.as_str()) {
        Ok(SessionTarget::Same)
    } else {
        Ok(SessionTarget::Different)
    }
}

/// Build the Grok Bot client login payload from a Storm-Dock session.
/// Launch writes this (encrypted) into sand-secrets; export returns it in plain form.
pub(crate) fn client_account_from_session(session: &Session) -> Result<GrokBotClientExport> {
    let access = session
        .values
        .get(ACCESS_TOKEN_KEY)
        .ok_or(AppError::SecretMissing)?
        .clone();
    let refresh = session
        .values
        .get("cursorAuth/refreshToken")
        .ok_or(AppError::SecretMissing)?
        .clone();
    let claims =
        jwt_claims(&access).ok_or_else(|| AppError::Message("Grok Bot 登录凭据无效。".into()))?;
    let auth_id = claims
        .get("sub")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::Message("Grok Bot 登录凭据缺少账号标识。".into()))?
        .to_owned();
    let email = claims
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let slot = account_slot(&auth_id);
    let active_in_client = active_slot().as_deref() == Some(slot.as_str());
    Ok(GrokBotClientExport {
        kind: "grok-bot-client",
        version: 1,
        slot,
        auth_id: auth_id.clone(),
        email: email.clone(),
        access_token: access,
        refresh_token: refresh,
        profile: serde_json::json!({ "authId": auth_id, "email": email }),
        active_in_client,
    })
}

pub(crate) fn export_client_account(session: &Session) -> Result<Value> {
    Ok(serde_json::to_value(client_account_from_session(session)?)?)
}

/// Write / activate a Grok Bot client account in sand-secrets, then launch.
pub(crate) fn apply_client_account_and_launch(account: &GrokBotClientExport) -> Result<()> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let _guard = OPERATION_LOCK
            .lock()
            .map_err(|_| AppError::Message("Grok Bot 操作锁不可用。".into()))?;
        platform::ensure_installed()?;
        let path = platform::data_path()?;
        let original = fs::read(&path).ok();
        let mut root = read_store_root(&path)?;
        if active_slot_from_root(&root).as_deref() == Some(account.slot.as_str()) {
            platform::launch()?;
            return Ok(());
        }
        if platform::is_running() {
            platform::quit_and_wait()?;
        }
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
        let profile = account.profile.to_string();
        let (access_enc, refresh_enc, profile_enc) = platform::encrypt_account_fields(
            &account.access_token,
            &account.refresh_token,
            &profile,
        )?;
        account_map.insert(
            account.slot.clone(),
            serde_json::json!({
                "cursor-access-token": access_enc,
                "cursor-refresh-token": refresh_enc,
                "cursor-account-profile": profile_enc,
            }),
        );
        root[ACCOUNTS_KEY] = Value::String(
            serde_json::json!({"active": account.slot, "accounts": account_map}).to_string(),
        );
        let encoded = serde_json::to_vec_pretty(&root)?;
        let _: Value = serde_json::from_slice(&encoded)
            .map_err(|_| AppError::Message("Grok Bot 登录数据验证失败。".into()))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_file_name(format!(".storm-dock-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&temp, &path)?;
            platform::launch()
        })();
        if result.is_err() {
            match &original {
                Some(bytes) => {
                    let _ = fs::write(&path, bytes);
                }
                None => {
                    let _ = fs::remove_file(&path);
                }
            }
            let _ = fs::remove_file(&temp);
        }
        result
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = account;
        platform::unsupported()
    }
}

fn read_store_root(path: &Path) -> Result<Value> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(Value::Object(Default::default()))
        }
        Err(_) => Err(AppError::Message("未检测到 Grok Bot 登录数据。".into())),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn prepare_for_session(session: &Session) -> Result<LaunchPreparation> {
    platform::ensure_installed()?;
    let root = read_store_root(&platform::data_path()?)?;
    match session_target(session, &root)? {
        SessionTarget::Same => {
            platform::launch()?;
            Ok(LaunchPreparation { status: "same" })
        }
        SessionTarget::Different if platform::is_running() => Ok(LaunchPreparation {
            status: "requiresConfirmation",
        }),
        SessionTarget::Different => Ok(LaunchPreparation { status: "ready" }),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn prepare_for_session(_: &Session) -> Result<LaunchPreparation> {
    platform::unsupported()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn confirm_for_session(session: &Session) -> Result<()> {
    let account = client_account_from_session(session)?;
    apply_client_account_and_launch(&account)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn confirm_for_session(_: &Session) -> Result<()> {
    platform::unsupported()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Session;
    use std::collections::BTreeMap;

    #[test]
    fn account_slot_is_stable() {
        assert_eq!(
            account_slot("cursor-user-123"),
            "e52340e7310f5a441a1f8f2710dacffd1f6af8167f868bd19c9b61214f67ca22"
        );
    }

    #[test]
    fn local_status_reports_machine_readiness() {
        let status = local_status();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            assert!(!status.available);
            assert!(!status.installed);
            assert_eq!(
                status.reason.as_deref(),
                Some("当前平台暂不支持 Grok Bot。")
            );
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            assert_eq!(status.available, status.installed && status.signed_in);
            if !status.installed {
                assert_eq!(status.reason.as_deref(), Some("未安装 Grok Bot。"));
            }
        }
    }

    #[test]
    fn windows_oscrypt_roundtrip() {
        let key = [7u8; 32];
        let encoded = encrypt_os_crypt_windows("cursor-access-token-value", &key);
        let raw = STANDARD.decode(&encoded).expect("base64");
        assert!(raw.starts_with(b"v10"));
        assert!(raw.len() > 3 + 12 + 16);
        assert_eq!(
            decrypt_os_crypt_windows(&encoded, &key).expect("decrypt"),
            "cursor-access-token-value"
        );
    }

    #[test]
    fn session_target_treats_empty_store_as_different() {
        let token = test_access_token("auth0|user-1", "one@example.com");
        let session = Session {
            values: BTreeMap::from([
                (ACCESS_TOKEN_KEY.into(), token.clone()),
                ("cursorAuth/refreshToken".into(), "refresh-1".into()),
            ]),
            raw_export: None,
        };
        let export = client_account_from_session(&session).expect("client export");
        assert_eq!(export.slot, account_slot("auth0|user-1"));
        assert_eq!(export.email, "one@example.com");
        assert_eq!(export.kind, "grok-bot-client");
        assert!(matches!(
            session_target(&session, &serde_json::json!({})).expect("target"),
            SessionTarget::Different
        ));
    }

    #[test]
    fn session_target_detects_active_slot() {
        let token = test_access_token("auth0|user-1", "one@example.com");
        let slot = account_slot("auth0|user-1");
        let session = Session {
            values: BTreeMap::from([
                (ACCESS_TOKEN_KEY.into(), token),
                ("cursorAuth/refreshToken".into(), "refresh-1".into()),
            ]),
            raw_export: None,
        };
        let root = serde_json::json!({
            "cursor-accounts": serde_json::json!({
                "active": slot,
                "accounts": {}
            }).to_string()
        });
        assert!(matches!(
            session_target(&session, &root).expect("target"),
            SessionTarget::Same
        ));
        assert_eq!(active_slot_from_root(&root).as_deref(), Some(slot.as_str()));
        assert!(session_matches_active_slot(&session, &slot));
    }

    #[test]
    fn session_matches_active_slot_uses_auth_id() {
        let slot = account_slot("auth0|user-2");
        let session = Session {
            values: BTreeMap::from([(AUTH_ID_KEY.into(), "auth0|user-2".into())]),
            raw_export: None,
        };
        assert!(session_matches_active_slot(&session, &slot));
        assert!(!session_matches_active_slot(&session, "other-slot"));
    }

    #[test]
    fn parse_lnk_reads_ansi_local_base_path() {
        let target = r"D:\GrokBot\Grok Bot\Grok Bot.exe";
        let bytes = minimal_lnk(target);
        assert_eq!(
            crate::desktop::parse_lnk_target(&bytes).as_deref(),
            Some(Path::new(target))
        );
    }

    #[test]
    fn process_name_matches_grok_bot_exe() {
        let mut name: Vec<u16> = "Grok Bot.exe".encode_utf16().collect();
        name.push(0);
        assert!(process_name_matches_for_test(&name, "Grok Bot.exe"));
        assert!(process_name_matches_for_test(&name, "grok bot.exe"));
        let mut other: Vec<u16> = "Cursor.exe".encode_utf16().collect();
        other.push(0);
        assert!(!process_name_matches_for_test(&other, "Grok Bot.exe"));
    }

    fn process_name_matches_for_test(wide: &[u16], expected: &str) -> bool {
        let end = wide
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..end]).eq_ignore_ascii_case(expected)
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_installed_grok_bot_shortcut_if_present() {
        let Some(appdata) = std::env::var_os("APPDATA") else {
            return;
        };
        let path = PathBuf::from(appdata)
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join("Grok Bot.lnk");
        if !path.is_file() {
            return;
        }
        let target = crate::desktop::parse_lnk_target(&fs::read(&path).expect("read shortcut"))
            .expect("parse Grok Bot shortcut");
        assert_eq!(
            target.file_name().and_then(|name| name.to_str()),
            Some("Grok Bot.exe"),
            "{target:?}"
        );
        assert!(target.is_file(), "{target:?}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn toolhelp_process_scan_does_not_panic() {
        let _ = crate::desktop::is_running(crate::desktop::DesktopApp::GrokBot);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn dpapi_protect_roundtrip() {
        let raw = b"grok-bot-key-material-32-bytes!!";
        let protected = windows::dpapi_protect_for_test(raw).expect("protect");
        assert_ne!(protected.as_slice(), raw);
        assert_eq!(
            windows::dpapi_unprotect_for_test(&protected).expect("unprotect"),
            raw
        );
    }

    fn test_access_token(sub: &str, email: &str) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"sub":"{sub}","email":"{email}"}}"#));
        format!("header.{payload}.sig")
    }

    fn minimal_lnk(target: &str) -> Vec<u8> {
        let mut header = vec![0u8; 0x4C];
        header[0..4].copy_from_slice(&0x4Cu32.to_le_bytes());
        header[0x14..0x18].copy_from_slice(&0x2u32.to_le_bytes());
        let mut path = target.as_bytes().to_vec();
        path.push(0);
        let volume = vec![0x10, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0x10, 0, 0, 0];
        let mut info = vec![0u8; 0x1C];
        let info_size = 0x1C + volume.len() + path.len() + 1;
        info[0..4].copy_from_slice(&(info_size as u32).to_le_bytes());
        info[4..8].copy_from_slice(&0x1Cu32.to_le_bytes());
        info[8..12].copy_from_slice(&0x1u32.to_le_bytes());
        info[12..16].copy_from_slice(&0x1Cu32.to_le_bytes());
        info[16..20].copy_from_slice(&((0x1C + volume.len()) as u32).to_le_bytes());
        info[24..28].copy_from_slice(&((0x1C + volume.len() + path.len()) as u32).to_le_bytes());
        let mut bytes = header;
        bytes.extend(info);
        bytes.extend(volume);
        bytes.extend(path);
        bytes.push(0);
        bytes
    }
}
