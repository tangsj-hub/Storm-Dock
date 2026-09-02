use super::adapter::ToolAdapter;
use super::LifecycleCommandShell;

pub(crate) const ADAPTER: HermesAdapter = HermesAdapter;

pub(crate) struct HermesAdapter;

const INSTALL_UNIX: &str = "bash -c 'tmp=$(mktemp) && curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";
const INSTALL_WINDOWS_SCRIPT: &str =
    "irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex";
const UPDATE_UNIX: &str = "hermes update || bash -c 'tmp=$(mktemp) && curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'";

impl ToolAdapter for HermesAdapter {
    fn name(&self) -> &'static str {
        "hermes"
    }
    fn display_name(&self) -> &'static str {
        "Hermes"
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
    fn official_uninstall_args(&self) -> Option<&'static str> {
        Some("uninstall --yes")
    }
    fn is_native_layout(&self, _bin: &str, _real: &str) -> bool {
        true
    }
    fn static_update_command(&self, shell: LifecycleCommandShell) -> Option<String> {
        Some(match shell {
            LifecycleCommandShell::Posix => UPDATE_UNIX.to_string(),
            LifecycleCommandShell::WindowsBatch => {
                #[cfg(target_os = "windows")]
                {
                    format!(
                        "hermes update || {}",
                        super::powershell_irm_install(INSTALL_WINDOWS_SCRIPT)
                    )
                }
                #[cfg(not(target_os = "windows"))]
                {
                    UPDATE_UNIX.to_string()
                }
            }
        })
    }
}
