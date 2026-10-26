//! Decrypt WorkBuddy `$wbEncrypted` auth blobs via the local WorkBuddy Electron
//! runtime (`ELECTRON_RUN_AS_NODE` + `electron_browser_workbuddy_storage`).
//!
//! Plaintext tokens/keys never touch disk; the helper returns decrypted strings
//! over stdout only. Failures are soft on import — identity still imports.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use serde_json::Value;

use crate::desktop::{self, DesktopApp};
use crate::error::{AppError, Result};

/// Embedded Node helper. Mirrors the community WorkBuddy toolkit protocol:
/// stdin JSON `{version:1,operation:"decrypt"|"decrypt_many",...}` → stdout JSON.
const HELPER_JS: &str = r#"
'use strict';
const crypto = require('crypto');
const failure = (reason) => { throw { reason }; };
const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
function base64(value, length) {
  if (typeof value !== 'string') failure('INVALID_FORMAT');
  let bytes;
  try { bytes = Buffer.from(value, 'base64'); } catch (_) { failure('INVALID_FORMAT'); }
  if (length !== undefined && bytes.length !== length) failure('INVALID_FORMAT');
  return bytes;
}
function utf8(bytes) {
  const text = bytes.toString('utf8');
  if (!Buffer.from(text, 'utf8').equals(bytes)) failure('INVALID_FORMAT');
  return text;
}
function decode(value) {
  if (!object(value) || value.$wbEncrypted !== 1 || typeof value.envelope !== 'string') {
    failure('UNSUPPORTED_ENVELOPE');
  }
  let envelope;
  try { envelope = JSON.parse(utf8(base64(value.envelope))); }
  catch (error) { failure(error && error.reason ? error.reason : 'INVALID_FORMAT'); }
  if (!object(envelope) || envelope.suite !== 1 || typeof envelope.keyId !== 'string') {
    failure('UNSUPPORTED_ENVELOPE');
  }
  return {
    keyId: envelope.keyId,
    nonce: base64(envelope.nonce, 12),
    tag: base64(envelope.authTag, 16),
    ciphertext: base64(envelope.ciphertext),
  };
}
function nativeStorage() {
  try {
    const storage = process._linkedBinding('electron_browser_workbuddy_storage');
    if (!storage || typeof storage.loggerGet !== 'function') failure('RUNTIME_UNAVAILABLE');
    return storage;
  } catch (_) {
    failure('RUNTIME_UNAVAILABLE');
  }
}
function getAtRestKey() {
  let payload;
  try { payload = JSON.parse(nativeStorage().loggerGet()); }
  catch (_) { failure('RUNTIME_UNAVAILABLE'); }
  if (!object(payload) || payload.version !== 1 || typeof payload.atRestSecretKey !== 'string') {
    failure('RUNTIME_UNAVAILABLE');
  }
  const key = crypto.createHash('sha256').update(payload.atRestSecretKey, 'utf8').digest();
  const keyId = crypto.createHash('sha256').update(key).digest('hex').slice(0, 16);
  return { key, keyId };
}
function framedAad(keyId) {
  const lengthPrefixed = (text) => {
    const bytes = Buffer.from(text, 'utf8');
    const length = Buffer.alloc(4);
    length.writeUInt32BE(bytes.length, 0);
    return Buffer.concat([length, bytes]);
  };
  return Buffer.concat([
    Buffer.from('WB-AAD\0', 'ascii'),
    Buffer.from([1]),
    lengthPrefixed('WBEV1'),
    lengthPrefixed('sym-v1'),
    Buffer.from([0, 0, 0, 1]),
    lengthPrefixed(keyId),
    Buffer.from([2, 0, 0]),
  ]);
}
function decrypt(value) {
  const envelope = decode(value);
  const keyState = getAtRestKey();
  if (keyState.keyId !== envelope.keyId) failure('KEY_MISMATCH');
  const cipher = crypto.createDecipheriv('aes-256-gcm', keyState.key, envelope.nonce, {
    authTagLength: 16,
  });
  cipher.setAAD(framedAad(envelope.keyId), { plaintextLength: envelope.ciphertext.length });
  cipher.setAuthTag(envelope.tag);
  const plaintext = Buffer.concat([cipher.update(envelope.ciphertext), cipher.final()]);
  return utf8(plaintext);
}
const chunks = [];
function reply(payload) {
  process.stdout.write(JSON.stringify({ version: 1, ...payload }), () => {
    process.exit(payload.ok ? 0 : 1);
  });
}
process.stdin.on('data', (chunk) => chunks.push(chunk));
process.stdin.on('end', () => {
  try {
    const request = JSON.parse(Buffer.concat(chunks).toString('utf8'));
    if (!object(request) || request.version !== 1) failure('HELPER_PROTOCOL');
    if (request.operation === 'decrypt') {
      reply({ ok: true, value: decrypt(request.value) });
      return;
    }
    if (request.operation === 'decrypt_many') {
      if (!Array.isArray(request.values)) failure('HELPER_PROTOCOL');
      reply({ ok: true, values: request.values.map(decrypt) });
      return;
    }
    failure('HELPER_PROTOCOL');
  } catch (error) {
    reply({ ok: false, reason: error && error.reason ? error.reason : 'HELPER_PROTOCOL' });
  }
});
"#;

const HELPER_TIMEOUT: Duration = Duration::from_secs(8);

#[cfg(test)]
thread_local! {
    static EXECUTABLE_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn is_encrypted_token(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("$wbEncrypted").and_then(Value::as_u64) == Some(1)
                || map.contains_key("envelope")
        }
        _ => false,
    }
}

pub(crate) fn profile_needs_token_decrypt(profile: &Value) -> bool {
    let Some(auth) = profile.get("auth") else {
        return false;
    };
    ["accessToken", "access_token", "refreshToken", "refresh_token"]
        .iter()
        .any(|key| auth.get(*key).is_some_and(is_encrypted_token))
}

/// When access/refresh tokens are `$wbEncrypted`, decrypt them in-place via the
/// local WorkBuddy Electron binary. Soft-fails: returns Ok even when the runtime
/// is missing so identity import can continue with encrypted material.
pub(crate) fn decrypt_profile_tokens_best_effort(profile: &mut Value) -> Result<()> {
    if !profile_needs_token_decrypt(profile) {
        return Ok(());
    }
    match decrypt_profile_tokens(profile) {
        Ok(()) => Ok(()),
        Err(error) => {
            // Soft-fail: keep encrypted blobs; prefer_saved_plaintext / usage path
            // already treat missing plaintext separately from an invalid login.
            let _ = error;
            Ok(())
        }
    }
}

pub(crate) fn decrypt_profile_tokens(profile: &mut Value) -> Result<()> {
    let Some(auth) = profile.get_mut("auth").and_then(Value::as_object_mut) else {
        return Ok(());
    };

    let mut jobs: Vec<(String, Value)> = Vec::new();
    for key in ["accessToken", "access_token", "refreshToken", "refresh_token"] {
        if let Some(value) = auth.get(key).filter(|value| is_encrypted_token(value)) {
            jobs.push((key.to_owned(), value.clone()));
        }
    }
    if jobs.is_empty() {
        return Ok(());
    }

    let values: Vec<Value> = jobs.iter().map(|(_, value)| value.clone()).collect();
    let plaintexts = decrypt_many(&values)?;
    if plaintexts.len() != jobs.len() {
        return Err(AppError::Message(
            "WorkBuddy 凭证解密结果数量不匹配。".into(),
        ));
    }
    for ((key, _), plaintext) in jobs.into_iter().zip(plaintexts) {
        let trimmed = plaintext.trim();
        if trimmed.is_empty() {
            return Err(AppError::Message("WorkBuddy 凭证解密结果为空。".into()));
        }
        auth.insert(key, Value::String(trimmed.to_owned()));
    }
    Ok(())
}

fn decrypt_many(values: &[Value]) -> Result<Vec<String>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let executable = resolve_electron_executable().ok_or_else(|| {
        AppError::Message(
            "未找到 WorkBuddy 客户端，无法解密本地加密凭证；请安装 WorkBuddy 或改用官方登录。".into(),
        )
    })?;
    let request = serde_json::json!({
        "version": 1,
        "operation": "decrypt_many",
        "values": values,
    });
    let response = run_helper(&executable, &request)?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let reason = response
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWN");
        return Err(AppError::Message(format!(
            "WorkBuddy 本地凭证解密失败（{reason}）。"
        )));
    }
    let list = response
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Message("WorkBuddy 凭证解密响应无效。".into()))?;
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let Some(text) = item.as_str() else {
            return Err(AppError::Message(
                "WorkBuddy 凭证解密响应包含非字符串结果。".into(),
            ));
        };
        out.push(text.to_owned());
    }
    Ok(out)
}

fn resolve_electron_executable() -> Option<PathBuf> {
    #[cfg(test)]
    {
        if let Some(path) = EXECUTABLE_OVERRIDE.with(|cell| cell.borrow().clone()) {
            return Some(path);
        }
    }
    let installed = desktop::app_path(DesktopApp::Workbuddy)?;
    electron_binary_from_install(&installed)
}

fn electron_binary_from_install(installed: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let candidates = [
            installed.join("Contents/MacOS/Electron"),
            installed.join("Contents/MacOS/WorkBuddy"),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        if installed.is_file() {
            return Some(installed.to_path_buf());
        }
        let named = installed.join("WorkBuddy.exe");
        if named.is_file() {
            return Some(named);
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = installed;
        None
    }
}

fn run_helper(executable: &Path, request: &Value) -> Result<Value> {
    let payload = serde_json::to_vec(request)?;
    let mut command = Command::new(executable);
    command
        .arg("-e")
        .arg(HELPER_JS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    scrub_helper_env(&mut command);
    command.env("ELECTRON_RUN_AS_NODE", "1");

    let mut child = command.spawn().map_err(|error| {
        AppError::Message(format!("无法启动 WorkBuddy 解密进程: {error}"))
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&payload).map_err(|error| {
            AppError::Message(format!("写入 WorkBuddy 解密请求失败: {error}"))
        })?;
        // Drop stdin to signal EOF.
    }

    let output = wait_with_timeout(child, HELPER_TIMEOUT)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            AppError::Message(format!(
                "WorkBuddy 解密进程失败（exit={}）。",
                output.status.code().unwrap_or(-1)
            ))
        })?;
    let response: Value = serde_json::from_str(line).map_err(|_| {
        AppError::Message("WorkBuddy 解密进程返回了无效 JSON。".into())
    })?;
    Ok(response)
}

fn scrub_helper_env(command: &mut Command) {
    // Avoid inheriting Node/Electron knobs that break ELECTRON_RUN_AS_NODE helpers.
    for (key, _) in std::env::vars() {
        let upper = key.to_ascii_uppercase();
        if upper.starts_with("NODE_")
            || upper.starts_with("ELECTRON_")
            || upper.starts_with("WORKBUDDY_")
            || upper == "NO_COLOR"
            || upper == "FORCE_COLOR"
        {
            command.env_remove(&key);
        }
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output> {
    use std::io::Read;
    // Drain pipes on background threads while waiting so a chatty Electron
    // process cannot fill the OS pipe buffers and deadlock before exit.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_thread = stdout.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_thread = stderr.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.map(|handle| handle.join());
                let _ = stderr_thread.map(|handle| handle.join());
                return Err(AppError::Message("WorkBuddy 凭证解密超时。".into()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = stdout_thread.map(|handle| handle.join());
                let _ = stderr_thread.map(|handle| handle.join());
                return Err(AppError::Message(format!(
                    "等待 WorkBuddy 解密进程失败: {error}"
                )));
            }
        }
    };
    let out = stdout_thread
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    let err = stderr_thread
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout: out,
        stderr: err,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, sync::Mutex};

    // Serialise override mutations across tests in this module.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_executable_override<T>(path: Option<PathBuf>, body: impl FnOnce() -> T) -> T {
        let _guard = TEST_LOCK.lock().unwrap();
        EXECUTABLE_OVERRIDE.with(|cell| {
            *cell.borrow_mut() = path;
        });
        let result = body();
        EXECUTABLE_OVERRIDE.with(|cell| {
            *cell.borrow_mut() = None;
        });
        result
    }

    #[test]
    fn detects_encrypted_token_objects() {
        assert!(is_encrypted_token(&serde_json::json!({
            "$wbEncrypted": 1,
            "envelope": "abc"
        })));
        assert!(!is_encrypted_token(&serde_json::json!("plain-token")));
        assert!(!profile_needs_token_decrypt(&serde_json::json!({
            "auth": { "accessToken": "plain" }
        })));
        assert!(profile_needs_token_decrypt(&serde_json::json!({
            "auth": {
                "accessToken": { "$wbEncrypted": 1, "envelope": "x" }
            }
        })));
    }

    #[test]
    fn plaintext_profile_is_noop() {
        let mut profile = serde_json::json!({
            "auth": {
                "accessToken": "tok",
                "refreshToken": "ref"
            }
        });
        decrypt_profile_tokens(&mut profile).unwrap();
        assert_eq!(profile["auth"]["accessToken"], "tok");
        assert_eq!(profile["auth"]["refreshToken"], "ref");
    }

    #[test]
    fn best_effort_leaves_encrypted_when_runtime_missing() {
        let mut profile = serde_json::json!({
            "account": { "uid": "u-1" },
            "auth": {
                "accessToken": { "$wbEncrypted": 1, "envelope": "e30=" },
                "refreshToken": { "$wbEncrypted": 1, "envelope": "e30=" }
            }
        });
        with_executable_override(Some(PathBuf::from("/nonexistent/WorkBuddyElectron")), || {
            decrypt_profile_tokens_best_effort(&mut profile).unwrap();
        });
        assert!(is_encrypted_token(&profile["auth"]["accessToken"]));
        assert!(is_encrypted_token(&profile["auth"]["refreshToken"]));
    }

    #[test]
    fn mock_helper_decrypts_many_fields() {
        let dir = env::temp_dir().join(format!("storm-dock-wb-crypto-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut profile = serde_json::json!({
            "auth": {
                "accessToken": {
                    "$wbEncrypted": 1,
                    "envelope": "mock-access"
                },
                "refreshToken": {
                    "$wbEncrypted": 1,
                    "envelope": "mock-refresh"
                }
            }
        });

        // Shim speaks decrypt_many over stdin/stdout and ignores Electron -e args.
        let shim = dir.join("shim-electron");
        fs::write(
            &shim,
            r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
  case "$1" in
    -e) shift; [ "$#" -gt 0 ] && shift ;;
    *) shift ;;
  esac
done
exec python3 -c 'import json,sys;req=json.load(sys.stdin);assert req.get("operation")=="decrypt_many";out=[];
for value in req["values"]:
 assert value.get("$wbEncrypted")==1;out.append("plain:"+value.get("envelope",""));
json.dump({"version":1,"ok":True,"values":out}, sys.stdout)'
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&shim).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&shim, perms).unwrap();
        }

        with_executable_override(Some(shim), || {
            decrypt_profile_tokens(&mut profile).unwrap();
        });
        assert_eq!(profile["auth"]["accessToken"], "plain:mock-access");
        assert_eq!(profile["auth"]["refreshToken"], "plain:mock-refresh");
        let _ = fs::remove_dir_all(&dir);
    }
}
