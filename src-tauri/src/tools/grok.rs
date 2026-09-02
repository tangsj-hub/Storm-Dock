use super::adapter::{path_has, NativeCleanup, ToolAdapter};
use super::slash_path;

pub(crate) const ADAPTER: GrokAdapter = GrokAdapter;

pub(crate) struct GrokAdapter;

const INSTALL_UNIX: &str = "bash -c 'tmp=$(mktemp) && curl -fsSL https://x.ai/cli/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";
const INSTALL_WINDOWS_SCRIPT: &str = "irm https://x.ai/cli/install.ps1 | iex";

impl ToolAdapter for GrokAdapter {
    fn name(&self) -> &'static str {
        "grok"
    }
    fn display_name(&self) -> &'static str {
        "Grok Build"
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@xai-official/grok")
    }
    fn posix_installer(&self) -> Option<&'static str> {
        Some(INSTALL_UNIX)
    }
    fn windows_install_command(&self) -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            Some(super::powershell_irm_install(INSTALL_WINDOWS_SCRIPT))
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }

    fn native_uninstall(&self, bin: &str, real: &str) -> Option<NativeCleanup> {
        if !is_native(bin, real) {
            return None;
        }
        let mut extra_dirs = Vec::new();
        if let Some(downloads) = downloads_dir(bin, real) {
            extra_dirs.push(downloads);
        }
        Some(NativeCleanup {
            extra_dirs,
            ..NativeCleanup::default()
        })
    }

    fn source_label(&self, bin: &str, real: &str) -> Option<String> {
        if !is_native(bin, real) {
            return None;
        }
        let path = std::env::var_os("GROK_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".grok")))?
            .join("config.toml");
        installer_from_toml(&std::fs::read_to_string(path).ok()?)
    }
}

pub(crate) fn is_native(bin_path: &str, real_target: &str) -> bool {
    [bin_path, real_target]
        .iter()
        .any(|path| path_has(path, "/.grok/bin/") || path_has(path, "/.grok/downloads/grok-"))
}

pub(crate) fn downloads_dir(bin_path: &str, real_target: &str) -> Option<String> {
    for path in [bin_path, real_target] {
        let n = slash_path(path);
        let lower = n.to_ascii_lowercase();
        const MARK: &str = "/.grok/";
        if let Some(idx) = lower.find(MARK) {
            return Some(format!("{}/downloads", &n[..idx + "/.grok".len()]));
        }
    }
    None
}

pub(crate) fn installer_from_toml(text: &str) -> Option<String> {
    let v: toml::Value = text.parse().ok()?;
    let installer = v.get("cli")?.get("installer")?.as_str()?.trim();
    if installer.is_empty() {
        None
    } else {
        Some(installer.to_string())
    }
}
