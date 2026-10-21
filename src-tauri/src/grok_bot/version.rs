//! Installed Grok Bot client version and the Bot模版 import gate.

use std::path::Path;

/// `grokbot://app/v1/bot-template` import needs at least this client version.
pub(crate) const IMPORT_MIN_VERSION: &str = "0.75.0";

/// Leading `major.minor.patch` (missing parts = 0); suffixes like `-beta.1` ignored.
pub(crate) fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw.trim().trim_start_matches(['v', 'V']);
    let core = core.split(|c: char| c == '-' || c == '+' || c == ' ').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    Some((major, minor, patch))
}

pub(crate) fn version_at_least(version: &str, min: &str) -> bool {
    match (parse_version(version), parse_version(min)) {
        (Some(v), Some(m)) => v >= m,
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn read_version(app: &Path) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string()).filter(|v| !v.is_empty())
}

#[cfg(target_os = "windows")]
fn read_version(exe: &Path) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = format!(
        "(Get-Item -LiteralPath '{}').VersionInfo.ProductVersion",
        exe.display().to_string().replace('\'', "''")
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string()).filter(|v| !v.is_empty())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_version(_: &Path) -> Option<String> {
    None
}

/// Version of the installed client, located the same way launching does.
pub(crate) fn client_version() -> Option<String> {
    let path = crate::desktop::app_path(crate::desktop::DesktopApp::GrokBot)?;
    read_version(&path)
}

/// `Ok(version)` when import is supported, else `Err(current version, if known)`.
pub(crate) fn import_gate() -> std::result::Result<String, Option<String>> {
    match client_version() {
        Some(version) if version_at_least(&version, IMPORT_MIN_VERSION) => Ok(version),
        other => Err(other),
    }
}

pub(crate) fn gate_message(current: Option<&str>) -> String {
    format!(
        "导入需要 Grok Bot {IMPORT_MIN_VERSION} 或更高版本（当前 {}）",
        current.unwrap_or("未知")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares_versions() {
        assert_eq!(parse_version("0.75.0"), Some((0, 75, 0)));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("0.76.1-beta.2"), Some((0, 76, 1)));
        assert_eq!(parse_version("0.75.0.1234"), Some((0, 75, 0)));
        assert_eq!(parse_version("abc"), None);
        assert!(version_at_least("0.75.0", IMPORT_MIN_VERSION));
        assert!(version_at_least("0.100.0", IMPORT_MIN_VERSION));
        assert!(version_at_least("1.0", IMPORT_MIN_VERSION));
        assert!(!version_at_least("0.74.9", IMPORT_MIN_VERSION));
        assert!(!version_at_least("0.9.0", IMPORT_MIN_VERSION));
        assert!(!version_at_least("", IMPORT_MIN_VERSION));
    }

    #[test]
    fn gate_message_mentions_current_or_unknown() {
        assert!(gate_message(Some("0.74.0")).contains("当前 0.74.0"));
        assert!(gate_message(None).contains("当前 未知"));
    }
}
