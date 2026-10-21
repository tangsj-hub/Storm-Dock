use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::ApplicationKind;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum DesktopApp {
    Cursor,
    ChatGPT,
    GrokBot,
}

impl DesktopApp {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor",
            Self::ChatGPT => "ChatGPT",
            Self::GrokBot => "Grok Bot",
        }
    }

    pub(crate) fn after_switch(kind: ApplicationKind) -> Option<Self> {
        match kind {
            ApplicationKind::Cursor => Some(Self::Cursor),
            ApplicationKind::Codex => Some(Self::ChatGPT),
            ApplicationKind::Grok => None,
        }
    }

    fn macos_bundle_id(self) -> Option<&'static str> {
        match self {
            Self::Cursor => Some("com.todesktop.230313mzl4w4u92"),
            Self::ChatGPT => Some("com.openai.chat"),
            Self::GrokBot => None,
        }
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    fn windows_exe(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor.exe",
            Self::ChatGPT => "ChatGPT.exe",
            Self::GrokBot => "Grok Bot.exe",
        }
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    fn windows_dir_names(self) -> &'static [&'static str] {
        match self {
            Self::Cursor => &["Cursor", "cursor"],
            Self::ChatGPT => &["ChatGPT"],
            Self::GrokBot => &["Grok Bot"],
        }
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    fn windows_shortcut(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor.lnk",
            Self::ChatGPT => "ChatGPT.lnk",
            Self::GrokBot => "Grok Bot.lnk",
        }
    }
}

pub(crate) fn ensure_installed(app: DesktopApp) -> Result<()> {
    resolve(app).map(|_| ())
}

pub(crate) fn launch(app: DesktopApp) -> Result<()> {
    ensure_installed(app)?;
    platform::launch(app)
}

pub(crate) fn is_running(app: DesktopApp) -> bool {
    platform::is_running(app)
}

pub(crate) fn terminate(app: DesktopApp) -> Result<()> {
    platform::terminate(app)
}

pub(crate) fn wait_until_stopped(app: DesktopApp) -> Result<()> {
    platform::wait_until_stopped(app)
}

pub(crate) fn quit_and_wait(app: DesktopApp) -> Result<()> {
    platform::quit_and_wait(app)
}

/// Installed app path (macOS: the .app bundle; Windows: the exe).
pub(crate) fn app_path(app: DesktopApp) -> Option<PathBuf> {
    resolve(app).ok()
}

fn resolve(app: DesktopApp) -> Result<PathBuf> {
    platform::resolve(app)
}

fn not_installed(app: DesktopApp) -> AppError {
    AppError::Message(format!("未安装 {}。", app.name()))
}

fn launch_failed(app: DesktopApp) -> AppError {
    AppError::Message(format!("无法启动 {}。", app.name()))
}

fn terminate_failed(app: DesktopApp) -> AppError {
    AppError::Message(format!("无法结束 {} 进程。", app.name()))
}

fn quit_failed(app: DesktopApp) -> AppError {
    AppError::Message(format!("{} 未能接受正常退出请求。", app.name()))
}

fn quit_timeout(app: DesktopApp) -> AppError {
    AppError::Message(format!(
        "{} 未在等待时间内退出，未修改登录会话。",
        app.name()
    ))
}

fn app_bundle_in(name: &str, roots: impl IntoIterator<Item = impl AsRef<Path>>) -> Option<PathBuf> {
    let bundle = format!("{name}.app");
    roots
        .into_iter()
        .map(|root| root.as_ref().join(&bundle))
        .find(|path| path.is_dir())
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn parse_lnk_target(data: &[u8]) -> Option<PathBuf> {
    if data.len() < 0x4C || u32::from_le_bytes(data[0..4].try_into().ok()?) != 0x4C {
        return None;
    }
    let flags = u32::from_le_bytes(data[0x14..0x18].try_into().ok()?);
    let mut offset = 0x4Cusize;
    if flags & 0x01 != 0 {
        if data.len() < offset + 2 {
            return None;
        }
        let idlist_size = u16::from_le_bytes(data[offset..offset + 2].try_into().ok()?) as usize;
        offset = offset.checked_add(2)?.checked_add(idlist_size)?;
    }
    if flags & 0x02 == 0 || data.len() < offset + 0x1C {
        return None;
    }
    let info_size = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
    let header_size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().ok()?) as usize;
    let info_end = offset.checked_add(info_size)?;
    if info_size < 0x1C || data.len() < info_end {
        return None;
    }
    if header_size >= 0x24 {
        let unicode_offset =
            u32::from_le_bytes(data[offset + 0x1C..offset + 0x20].try_into().ok()?) as usize;
        if unicode_offset > 0 {
            if let Some(path) = read_wide_cstring(&data[offset..info_end], unicode_offset) {
                return Some(path);
            }
        }
    }
    let ansi_offset =
        u32::from_le_bytes(data[offset + 0x10..offset + 0x14].try_into().ok()?) as usize;
    read_ansi_cstring(&data[offset..info_end], ansi_offset)
}

#[cfg(any(target_os = "windows", test))]
fn read_ansi_cstring(info: &[u8], start: usize) -> Option<PathBuf> {
    let bytes = info.get(start..)?;
    let end = bytes.iter().position(|&b| b == 0)?;
    if end == 0 {
        return None;
    }
    Some(PathBuf::from(
        String::from_utf8_lossy(&bytes[..end]).into_owned(),
    ))
}

#[cfg(any(target_os = "windows", test))]
fn read_wide_cstring(info: &[u8], start: usize) -> Option<PathBuf> {
    let bytes = info.get(start..)?;
    if bytes.len() < 2 {
        return None;
    }
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|word| *word != 0)
        .collect();
    if words.is_empty() {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&words)))
}

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;

    pub(super) fn resolve(app: DesktopApp) -> Result<PathBuf> {
        Err(not_installed(app))
    }
    pub(super) fn launch(app: DesktopApp) -> Result<()> {
        Err(not_installed(app))
    }
    pub(super) fn is_running(_: DesktopApp) -> bool {
        false
    }
    pub(super) fn terminate(app: DesktopApp) -> Result<()> {
        Err(terminate_failed(app))
    }
    pub(super) fn wait_until_stopped(app: DesktopApp) -> Result<()> {
        Err(AppError::Message(format!(
            "{} 未在 5 秒内退出。",
            app.name()
        )))
    }
    pub(super) fn quit_and_wait(app: DesktopApp) -> Result<()> {
        Err(quit_failed(app))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    #[test]
    fn after_switch_maps_cursor_and_chatgpt() {
        assert_eq!(
            DesktopApp::after_switch(ApplicationKind::Cursor),
            Some(DesktopApp::Cursor)
        );
        assert_eq!(
            DesktopApp::after_switch(ApplicationKind::Codex),
            Some(DesktopApp::ChatGPT)
        );
        assert_eq!(DesktopApp::after_switch(ApplicationKind::Grok), None);
    }

    #[test]
    fn app_bundle_in_requires_a_directory() {
        let root = env::temp_dir().join(format!("storm-dock-app-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("ChatGPT.app")).unwrap();
        fs::write(root.join("Cursor.app"), "").unwrap();
        assert_eq!(
            app_bundle_in("ChatGPT", [&root]).as_deref(),
            Some(root.join("ChatGPT.app").as_path())
        );
        assert_eq!(app_bundle_in("Cursor", [&root]), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parse_lnk_reads_ansi_local_base_path() {
        let target = r"D:\GrokBot\Grok Bot\Grok Bot.exe";
        let bytes = minimal_lnk(target);
        assert_eq!(parse_lnk_target(&bytes).as_deref(), Some(Path::new(target)));
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
