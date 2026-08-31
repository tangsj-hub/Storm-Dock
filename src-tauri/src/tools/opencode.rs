use super::adapter::{NativeCleanup, ToolAdapter, exe_matches, path_has};

pub(crate) const ADAPTER: OpenCodeAdapter = OpenCodeAdapter;

pub(crate) struct OpenCodeAdapter;

const INSTALL_UNIX: &str = "bash -c 'tmp=$(mktemp) && curl -fsSL https://opencode.ai/install -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";
const UNINSTALL_ARGS: &str = "uninstall --force --keep-config --keep-data";

impl ToolAdapter for OpenCodeAdapter {
    fn name(&self) -> &'static str {
        "opencode"
    }
    fn display_name(&self) -> &'static str {
        "OpenCode"
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("opencode-ai")
    }
    fn posix_installer(&self) -> Option<&'static str> {
        Some(INSTALL_UNIX)
    }
    fn official_uninstall_args(&self) -> Option<&'static str> {
        Some(UNINSTALL_ARGS)
    }

    fn native_uninstall(&self, bin: &str, real: &str) -> Option<NativeCleanup> {
        if !is_curl_layout(bin, real) {
            return None;
        }
        Some(NativeCleanup {
            official_args: Some(UNINSTALL_ARGS),
            also_remove_launcher: true,
            ..NativeCleanup::default()
        })
    }

    fn source_label(&self, bin: &str, real: &str) -> Option<String> {
        is_curl_layout(bin, real).then(|| "curl".into())
    }
}

fn is_curl_layout(bin: &str, real: &str) -> bool {
    if path_has(bin, "/node_modules/") || path_has(real, "/node_modules/") {
        return false;
    }
    if path_has(bin, "/cellar/") || path_has(real, "/cellar/") {
        return false;
    }
    if path_has(bin, "/caskroom/") || path_has(real, "/caskroom/") {
        return false;
    }
    [bin, real].iter().any(|path| {
        path_has(path, "/.opencode/bin/")
            || (exe_matches(path, "opencode") && is_user_bin(path))
    })
}

fn is_user_bin(path: &str) -> bool {
    let n = super::slash_path(path).to_ascii_lowercase();
    let parent = super::parent_dir(&n);
    parent.ends_with("/bin")
        && !parent.ends_with("/usr/bin")
        && !parent.ends_with("/usr/local/bin")
        && !parent.contains("/homebrew/")
        && !parent.contains("/cellar/")
        && !parent.contains("/caskroom/")
}
