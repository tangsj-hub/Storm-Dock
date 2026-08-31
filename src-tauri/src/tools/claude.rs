use super::adapter::{NativeCleanup, ToolAdapter, exe_matches, path_has};
use super::slash_path;

pub(crate) const ADAPTER: ClaudeAdapter = ClaudeAdapter;

pub(crate) struct ClaudeAdapter;

const INSTALL_UNIX: &str = "bash -c 'tmp=$(mktemp) && curl -fsSL https://claude.ai/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

impl ToolAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "claude"
    }
    fn display_name(&self) -> &'static str {
        "Claude Code"
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@anthropic-ai/claude-code")
    }
    fn posix_installer(&self) -> Option<&'static str> {
        Some(INSTALL_UNIX)
    }

    fn native_uninstall(&self, bin: &str, real: &str) -> Option<NativeCleanup> {
        if is_windows_apps(bin) || is_windows_apps(real) {
            return Some(NativeCleanup {
                winget_id: Some("Anthropic.ClaudeCode"),
                ..NativeCleanup::default()
            });
        }
        if !is_native(bin, real) {
            return None;
        }
        let mut extra_dirs = Vec::new();
        if let Some(payload) = payload_dir(real).or_else(|| payload_dir(bin)) {
            extra_dirs.push(payload);
        }
        Some(NativeCleanup {
            extra_dirs,
            ..NativeCleanup::default()
        })
    }

    fn source_label(&self, bin: &str, real: &str) -> Option<String> {
        if self.is_native_layout(bin, real) {
            return Some("native".into());
        }
        if path_has(real, "/.claude/local/") {
            return Some("local".into());
        }
        let path = dirs::home_dir()?.join(".claude.json");
        install_method_from_json(&std::fs::read_to_string(path).ok()?)
    }
}

fn is_windows_apps(path: &str) -> bool {
    path_has(path, "/windowsapps/") && path_has(path, "claudecode")
}

fn is_native(bin: &str, real: &str) -> bool {
    [bin, real].iter().any(|path| {
        path_has(path, "/.local/share/claude/")
            || path_has(path, "/claude/versions/")
            || path_has(path, "/programs/claude")
            || (exe_matches(path, "claude") && path_has(path, "/.local/bin/"))
    })
}

pub(crate) fn payload_dir(real_target: &str) -> Option<String> {
    let n = slash_path(real_target);
    let lower = n.to_ascii_lowercase();
    const SHARE: &str = "/.local/share/claude";
    if let Some(idx) = lower.find(SHARE) {
        let end = idx + SHARE.len();
        if end == n.len() || n.as_bytes().get(end) == Some(&b'/') {
            return Some(n[..end].to_string());
        }
    }
    const VERSIONS: &str = "/claude/versions/";
    if let Some(idx) = lower.find(VERSIONS) {
        return Some(n[..idx + "/claude".len()].to_string());
    }
    const PROGRAMS: &str = "/programs/claude";
    if let Some(idx) = lower.find(PROGRAMS) {
        let end = idx + PROGRAMS.len();
        if end == n.len() || n.as_bytes().get(end) == Some(&b'/') {
            return Some(n[..end].to_string());
        }
    }
    None
}

pub(crate) fn install_method_from_json(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let method = v.get("installMethod")?.as_str()?.trim();
    if method.is_empty() {
        None
    } else {
        Some(method.to_string())
    }
}
