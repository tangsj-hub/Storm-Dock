use super::adapter::{NativeCleanup, ToolAdapter, exe_matches, path_has};
use super::parent_dir;
use super::slash_path;

pub(crate) const ADAPTER: CodexAdapter = CodexAdapter;

pub(crate) struct CodexAdapter;

impl ToolAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }
    fn display_name(&self) -> &'static str {
        "Codex"
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@openai/codex")
    }

    fn native_uninstall(&self, bin: &str, real: &str) -> Option<NativeCleanup> {
        if !is_standalone(bin, real) {
            return None;
        }
        let mut extra_dirs = Vec::new();
        if let Some(dir) = standalone_payload_dir(bin, real) {
            extra_dirs.push(dir);
        }
        Some(NativeCleanup {
            extra_dirs,
            ..NativeCleanup::default()
        })
    }
}

fn is_standalone(bin: &str, real: &str) -> bool {
    if path_has(bin, "/node_modules/") || path_has(real, "/node_modules/") {
        return false;
    }
    [bin, real].iter().any(|path| {
        path_has(path, "/.codex/packages/standalone")
            || path_has(path, "/programs/openai/codex")
            || (exe_matches(path, "codex") && path_has(path, "/.local/bin/"))
    })
}

fn standalone_payload_dir(bin: &str, real: &str) -> Option<String> {
    for path in [bin, real] {
        let n = slash_path(path);
        let lower = n.to_ascii_lowercase();
        const STANDALONE: &str = "/.codex/packages/standalone";
        if let Some(idx) = lower.find(STANDALONE) {
            return Some(n[..idx + STANDALONE.len()].to_string());
        }
        const PROGRAMS: &str = "/programs/openai/codex";
        if let Some(idx) = lower.find(PROGRAMS) {
            return Some(n[..idx + PROGRAMS.len()].to_string());
        }
    }
    if exe_matches(bin, "codex") && path_has(bin, "/.local/bin/") {
        let parent = parent_dir(bin);
        if !parent.is_empty() {
            return None;
        }
    }
    None
}
