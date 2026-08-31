use super::{
    chain_update_commands, claude, codex, gemini, grok, hermes, opencode, openclaw, pi, slash_path,
    LifecycleCommandShell, ToolLifecycleAction,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct NativeCleanup {
    pub extra_dirs: Vec<String>,
    pub official_args: Option<&'static str>,
    pub also_remove_launcher: bool,
    pub winget_id: Option<&'static str>,
}

pub(crate) trait ToolAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn npm_package(&self) -> Option<&'static str> {
        None
    }
    fn posix_installer(&self) -> Option<&'static str> {
        None
    }
    fn windows_install_command(&self) -> Option<String> {
        None
    }
    fn official_uninstall_args(&self) -> Option<&'static str> {
        None
    }
    fn native_uninstall(&self, _bin: &str, _real: &str) -> Option<NativeCleanup> {
        None
    }
    fn source_label(&self, _bin: &str, _real: &str) -> Option<String> {
        None
    }
    fn is_native_layout(&self, bin: &str, real: &str) -> bool {
        self.native_uninstall(bin, real).is_some()
    }
    fn static_update_command(&self, _shell: LifecycleCommandShell) -> Option<String> {
        None
    }

    fn npm_install_command(&self) -> Option<String> {
        self.npm_package()
            .map(|pkg| format!("npm i -g {pkg}@latest"))
    }

    fn install_command(&self, shell: LifecycleCommandShell) -> String {
        match shell {
            LifecycleCommandShell::Posix => {
                match (self.posix_installer(), self.npm_install_command()) {
                    (Some(installer), Some(npm)) => {
                        chain_update_commands(installer.to_string(), npm, shell)
                    }
                    (Some(installer), None) => installer.to_string(),
                    (None, Some(npm)) => npm,
                    (None, None) => String::new(),
                }
            }
            LifecycleCommandShell::WindowsBatch => {
                match (self.windows_install_command(), self.npm_install_command()) {
                    (Some(installer), Some(npm)) => chain_update_commands(installer, npm, shell),
                    (Some(installer), None) => installer,
                    (None, Some(npm)) => npm,
                    (None, None) => String::new(),
                }
            }
        }
    }
}

pub(crate) fn adapter(name: &str) -> Option<&'static dyn ToolAdapter> {
    Some(match name {
        "claude" => &claude::ADAPTER,
        "codex" => &codex::ADAPTER,
        "gemini" => &gemini::ADAPTER,
        "grok" => &grok::ADAPTER,
        "opencode" => &opencode::ADAPTER,
        "openclaw" => &openclaw::ADAPTER,
        "hermes" => &hermes::ADAPTER,
        "pi" => &pi::ADAPTER,
        _ => return None,
    })
}

pub(crate) fn npm_package_for(tool: &str) -> Option<&'static str> {
    adapter(tool).and_then(|a| a.npm_package())
}

pub(crate) fn display_name(tool: &str) -> &'static str {
    adapter(tool).map(|a| a.display_name()).unwrap_or("Unknown")
}

pub(crate) fn install_command(tool: &str, shell: LifecycleCommandShell) -> String {
    adapter(tool)
        .map(|a| a.install_command(shell))
        .unwrap_or_default()
}

pub(crate) fn static_action_command(
    tool: &str,
    action: ToolLifecycleAction,
    shell: LifecycleCommandShell,
) -> Option<String> {
    let a = adapter(tool)?;
    match action {
        ToolLifecycleAction::Install => {
            let cmd = a.install_command(shell);
            (!cmd.is_empty()).then_some(cmd)
        }
        ToolLifecycleAction::Update => a.static_update_command(shell),
        ToolLifecycleAction::Uninstall => None,
    }
}

pub(crate) fn exe_matches(path: &str, tool: &str) -> bool {
    let file = slash_path(path)
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    file == tool || file == format!("{tool}.exe") || file == format!("{tool}.cmd")
}

pub(crate) fn path_has(path: &str, needle: &str) -> bool {
    slash_path(path)
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}
