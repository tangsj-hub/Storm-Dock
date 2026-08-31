use std::path::Path;

use super::adapter::{adapter, NativeCleanup, ToolAdapter};
use super::{
    anchored_npm_command, brew_formula_from_path, infer_install_source, parent_dir,
    quote_path_if_spaced, shell_single_quote, sibling_bin, slash_path,
};

#[cfg(target_os = "windows")]
use super::{sibling_bin_with_ext, win_quote_path_for_batch};

#[derive(Debug, Clone)]
pub(crate) enum UninstallAction {
    OfficialCli {
        args: &'static str,
        also_remove_launcher: bool,
    },
    PackageManager {
        sibling: &'static str,
        args: String,
    },
    RemovePaths {
        extra_dirs: Vec<String>,
    },
    Winget {
        id: &'static str,
    },
}

pub(crate) fn plan_uninstall(tool: &str, bin: &str, real: &str) -> Option<UninstallAction> {
    let a = adapter(tool)?;
    if let Some(native) = a.native_uninstall(bin, real) {
        return action_from_native(a, native);
    }
    if let Some(args) = a.official_uninstall_args() {
        return Some(UninstallAction::OfficialCli {
            args,
            also_remove_launcher: false,
        });
    }
    if let Some(formula) = brew_formula_from_path(real) {
        return Some(UninstallAction::PackageManager {
            sibling: "brew",
            args: format!("uninstall {formula}"),
        });
    }
    if let Some(cask) = brew_cask_from_path(real) {
        return Some(UninstallAction::PackageManager {
            sibling: "brew",
            args: format!("uninstall --cask {cask}"),
        });
    }
    package_manager_uninstall(a, bin)
}

fn action_from_native(a: &dyn ToolAdapter, native: NativeCleanup) -> Option<UninstallAction> {
    if let Some(id) = native.winget_id {
        return Some(UninstallAction::Winget { id });
    }
    if let Some(args) = native.official_args.or_else(|| a.official_uninstall_args()) {
        return Some(UninstallAction::OfficialCli {
            args,
            also_remove_launcher: native.also_remove_launcher,
        });
    }
    Some(UninstallAction::RemovePaths {
        extra_dirs: native.extra_dirs,
    })
}

fn package_manager_uninstall(a: &dyn ToolAdapter, bin: &str) -> Option<UninstallAction> {
    let pkg = a.npm_package()?;
    match infer_install_source(Path::new(bin)) {
        "volta" => Some(UninstallAction::PackageManager {
            sibling: "volta",
            args: format!("uninstall {pkg}"),
        }),
        "bun" => Some(UninstallAction::PackageManager {
            sibling: "bun",
            args: format!("remove -g {pkg}"),
        }),
        "pnpm" => Some(UninstallAction::PackageManager {
            sibling: "pnpm",
            args: format!("remove -g {pkg}"),
        }),
        "nvm" | "fnm" | "mise" | "homebrew" => Some(UninstallAction::PackageManager {
            sibling: "npm",
            args: format!("uninstall -g {pkg}"),
        }),
        #[cfg(target_os = "windows")]
        _ => Some(UninstallAction::PackageManager {
            sibling: "npm",
            args: format!("uninstall -g {pkg}"),
        }),
        #[cfg(not(target_os = "windows"))]
        _ => None,
    }
}

pub(crate) fn brew_cask_from_path(real: &str) -> Option<String> {
    let n = slash_path(real);
    let mut segs = n.split('/');
    while let Some(seg) = segs.next() {
        if seg.eq_ignore_ascii_case("Caskroom") {
            return segs.next().filter(|s| !s.is_empty()).map(|s| s.to_string());
        }
    }
    None
}

pub(crate) fn render_posix(bin: &str, action: &UninstallAction) -> Option<String> {
    match action {
        UninstallAction::OfficialCli {
            args,
            also_remove_launcher,
        } => {
            let dir = parent_dir(bin);
            if dir.is_empty() {
                return None;
            }
            let mut cmd = format!(
                "PATH={}:\"$PATH\" {} {args}",
                shell_single_quote(&dir),
                quote_path_if_spaced(bin)
            );
            if *also_remove_launcher {
                cmd.push_str(" || true; rm -f ");
                cmd.push_str(&shell_single_quote(bin));
            }
            Some(cmd)
        }
        UninstallAction::PackageManager { sibling, args } => {
            if *sibling == "npm" {
                return anchored_npm_command(bin, args);
            }
            let exe = sibling_bin(bin, sibling)?;
            Some(format!("{} {args}", quote_path_if_spaced(&exe)))
        }
        UninstallAction::RemovePaths { extra_dirs } => {
            let mut cmd = format!("rm -f {}", shell_single_quote(bin));
            for dir in extra_dirs {
                cmd.push_str(" && rm -rf ");
                cmd.push_str(&shell_single_quote(dir));
            }
            Some(cmd)
        }
        UninstallAction::Winget { id } => Some(format!(
            "winget uninstall --id {id} --silent --disable-interactivity"
        )),
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn render_windows(bin: &str, action: &UninstallAction) -> Option<String> {
    match action {
        UninstallAction::OfficialCli {
            args,
            also_remove_launcher,
        } => {
            let mut cmd = format!("{} {args}", win_quote_path_for_batch(bin));
            if *also_remove_launcher {
                cmd.push_str(" & ");
                cmd.push_str(&powershell_remove_item(bin, false));
            }
            Some(cmd)
        }
        UninstallAction::PackageManager { sibling, args } => {
            let (name, exts): (&str, &[&str]) = match *sibling {
                "volta" => ("volta", &["exe", "cmd"]),
                "pnpm" => ("pnpm", &["cmd", "exe"]),
                "bun" => ("bun", &["exe", "cmd"]),
                _ => ("npm", &["cmd", "exe"]),
            };
            let exe = sibling_bin_with_ext(bin, name, exts)?;
            Some(format!("{} {args}", win_quote_path_for_batch(&exe)))
        }
        UninstallAction::RemovePaths { extra_dirs } => {
            let mut cmd = powershell_remove_item(bin, false);
            for dir in extra_dirs {
                cmd.push_str(" & ");
                cmd.push_str(&powershell_remove_item(dir, true));
            }
            Some(cmd)
        }
        UninstallAction::Winget { id } => Some(format!(
            "winget uninstall --id {id} --silent --disable-interactivity"
        )),
    }
}

#[cfg(target_os = "windows")]
fn powershell_remove_item(path: &str, recurse: bool) -> String {
    let literal = path.replace('\'', "''");
    let recurse_flag = if recurse { " -Recurse" } else { "" };
    format!(
        "powershell.exe -NoProfile -Command \"Remove-Item -LiteralPath '{literal}'{recurse_flag} -Force -ErrorAction SilentlyContinue\""
    )
}

pub(crate) fn command_from_paths(tool: &str, bin: &str, real: &str) -> Option<String> {
    let action = plan_uninstall(tool, bin, real)?;
    #[cfg(target_os = "windows")]
    {
        render_windows(bin, &action)
    }
    #[cfg(not(target_os = "windows"))]
    {
        render_posix(bin, &action)
    }
}

pub(crate) fn posix_command_from_paths(tool: &str, bin: &str, real: &str) -> Option<String> {
    render_posix(bin, &plan_uninstall(tool, bin, real)?)
}

#[cfg(test)]
mod tests {
    use super::posix_command_from_paths;
    use super::brew_cask_from_path;

    #[test]
    fn uninstall_claude_native_rms_launcher_and_payload() {
        let cmd = posix_command_from_paths(
            "claude",
            "/Users/me/.local/bin/claude",
            "/Users/me/.local/share/claude/versions/2.1.0/claude",
        )
        .unwrap();
        assert!(cmd.contains("rm -f '/Users/me/.local/bin/claude'"));
        assert!(cmd.contains("rm -rf '/Users/me/.local/share/claude'"));
        assert!(!cmd.contains("rm -rf '/Users/me/.claude'"));
    }

    #[test]
    fn uninstall_claude_local_bin_only() {
        let cmd = posix_command_from_paths(
            "claude",
            "/Users/me/.local/bin/claude",
            "/Users/me/.local/bin/claude",
        )
        .unwrap();
        assert!(cmd.contains("rm -f '/Users/me/.local/bin/claude'"));
    }

    #[test]
    fn uninstall_claude_cask() {
        let cmd = posix_command_from_paths(
            "claude",
            "/opt/homebrew/bin/claude",
            "/opt/homebrew/Caskroom/claude-code/2.1.0/claude",
        )
        .unwrap();
        assert!(cmd.contains("uninstall --cask claude-code"));
        assert!(!cmd.contains("rm -f"));
    }

    #[test]
    fn cask_name_from_path() {
        assert_eq!(
            brew_cask_from_path("/opt/homebrew/Caskroom/claude-code@latest/1.0/claude")
                .as_deref(),
            Some("claude-code@latest")
        );
    }

    #[test]
    fn uninstall_nvm_claude_uses_npm_not_rm() {
        let cmd = posix_command_from_paths(
            "claude",
            "/Users/me/.nvm/versions/node/v22.14.0/bin/claude",
            "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/@anthropic-ai/claude-code/cli.js",
        )
        .unwrap();
        assert!(cmd.contains("uninstall -g @anthropic-ai/claude-code"));
        assert!(!cmd.contains("rm -f"));
    }

    #[test]
    fn uninstall_grok_native_rms_bin_and_downloads_not_home() {
        let cmd = posix_command_from_paths(
            "grok",
            "/Users/a/.grok/bin/grok",
            "/Users/a/.grok/bin/grok",
        )
        .unwrap();
        assert!(cmd.contains("rm -f '/Users/a/.grok/bin/grok'"));
        assert!(cmd.contains("rm -rf '/Users/a/.grok/downloads'"));
        assert!(!cmd.contains("rm -rf '/Users/a/.grok'"));
    }

    #[test]
    fn uninstall_hermes_uses_yes_not_full() {
        let cmd = posix_command_from_paths(
            "hermes",
            "/Users/me/.local/bin/hermes",
            "/Users/me/.local/bin/hermes",
        )
        .unwrap();
        assert!(cmd.contains("uninstall --yes"));
        assert!(!cmd.contains("--full"));
        assert!(cmd.contains("PATH='/Users/me/.local/bin'"));
    }

    #[test]
    fn uninstall_opencode_curl_force_then_rm() {
        let cmd = posix_command_from_paths(
            "opencode",
            "/Users/me/.opencode/bin/opencode",
            "/Users/me/.opencode/bin/opencode",
        )
        .unwrap();
        assert!(cmd.contains("uninstall --force --keep-config --keep-data"));
        assert!(cmd.contains("rm -f '/Users/me/.opencode/bin/opencode'"));
    }

    #[test]
    fn uninstall_opencode_home_bin() {
        let cmd = posix_command_from_paths(
            "opencode",
            "/Users/me/bin/opencode",
            "/Users/me/bin/opencode",
        )
        .unwrap();
        assert!(cmd.contains("uninstall --force --keep-config --keep-data"));
        assert!(cmd.contains("rm -f '/Users/me/bin/opencode'"));
    }

    #[test]
    fn uninstall_codex_standalone_not_home() {
        let cmd = posix_command_from_paths(
            "codex",
            "/Users/me/.local/bin/codex",
            "/Users/me/.codex/packages/standalone/codex",
        )
        .unwrap();
        assert!(cmd.contains("rm -f '/Users/me/.local/bin/codex'"));
        assert!(cmd.contains("rm -rf '/Users/me/.codex/packages/standalone'"));
        assert!(!cmd.contains("rm -rf '/Users/me/.codex'"));
    }

    #[test]
    fn uninstall_gemini_nvm_uses_npm() {
        let cmd = posix_command_from_paths(
            "gemini",
            "/Users/me/.nvm/versions/node/v22.14.0/bin/gemini",
            "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/@google/gemini-cli/dist/index.js",
        )
        .unwrap();
        assert!(cmd.contains("uninstall -g @google/gemini-cli"));
        assert!(!cmd.contains("rm -f"));
    }

    #[test]
    fn uninstall_openclaw_and_pi_use_npm() {
        let claw = posix_command_from_paths(
            "openclaw",
            "/Users/me/.nvm/versions/node/v22.14.0/bin/openclaw",
            "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/openclaw/index.js",
        )
        .unwrap();
        assert!(claw.contains("uninstall -g openclaw"));
        assert!(!claw.contains("openclaw uninstall"));

        let pi = posix_command_from_paths(
            "pi",
            "/Users/me/.nvm/versions/node/v22.14.0/bin/pi",
            "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/@earendil-works/pi-coding-agent/dist/index.js",
        )
        .unwrap();
        assert!(pi.contains("uninstall -g @earendil-works/pi-coding-agent"));
    }

    #[test]
    fn uninstall_codex_npm_not_rm() {
        let cmd = posix_command_from_paths(
            "codex",
            "/Users/me/.nvm/versions/node/v22.14.0/bin/codex",
            "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/@openai/codex/bin/codex.js",
        )
        .unwrap();
        assert!(cmd.contains("uninstall -g @openai/codex"));
        assert!(!cmd.contains("rm -f"));
    }

    #[test]
    fn uninstall_claude_windows_local_bin_posix_render() {
        let cmd = posix_command_from_paths(
            "claude",
            r"C:\Users\me\.local\bin\claude.exe",
            r"C:\Users\me\.local\share\claude\versions\2.1.0\claude.exe",
        )
        .unwrap();
        assert!(cmd.contains("rm -f 'C:\\Users\\me\\.local\\bin\\claude.exe'") || cmd.contains("claude.exe"));
        assert!(cmd.contains(".local/share/claude") || cmd.contains(".local\\share\\claude"));
    }

    #[test]
    fn uninstall_claude_programs_dir() {
        let cmd = posix_command_from_paths(
            "claude",
            r"C:\Users\me\AppData\Local\Programs\claude\claude.exe",
            r"C:\Users\me\AppData\Local\Programs\claude\claude.exe",
        )
        .unwrap();
        assert!(cmd.contains("Programs/claude") || cmd.contains("Programs\\claude") || cmd.contains("/programs/claude"));
    }

    #[test]
    fn uninstall_codex_windows_programs() {
        let cmd = posix_command_from_paths(
            "codex",
            r"C:\Users\me\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe",
            r"C:\Users\me\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe",
        )
        .unwrap();
        assert!(cmd.contains("OpenAI") || cmd.contains("openai"));
    }

    #[test]
    fn uninstall_claude_windowsapps_uses_winget() {
        let cmd = posix_command_from_paths(
            "claude",
            r"C:\Program Files\WindowsApps\Anthropic.ClaudeCode_1.0\claude.exe",
            r"C:\Program Files\WindowsApps\Anthropic.ClaudeCode_1.0\claude.exe",
        )
        .unwrap();
        assert!(cmd.contains("winget uninstall --id Anthropic.ClaudeCode"));
    }
}
