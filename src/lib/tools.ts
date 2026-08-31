import { invoke } from "@tauri-apps/api/core";

export const TOOL_NAMES = [
  "claude",
  "codex",
  "gemini",
  "grok",
  "opencode",
  "openclaw",
  "hermes",
  "pi"
] as const;

export type ToolName = (typeof TOOL_NAMES)[number];
export type ToolLifecycleAction = "install" | "update" | "uninstall";

export type WslShellPreference = {
  wslShell?: string | null;
  wslShellFlag?: string | null;
};

export type ToolVersion = {
  name: string;
  version: string | null;
  latest_version: string | null;
  error: string | null;
  installed_but_broken: boolean;
  env_type: "windows" | "wsl" | "macos" | "linux" | "unknown";
  wsl_distro: string | null;
  update_available: boolean;
  latest_pending: boolean;
};

export type ToolInstallation = {
  path: string;
  version: string | null;
  runnable: boolean;
  error: string | null;
  source: string;
  is_path_default: boolean;
};

export type ToolInstallationReport = {
  tool: string;
  installs: ToolInstallation[];
  is_conflict: boolean;
  needs_confirmation: boolean;
  command: string;
  anchored: boolean;
  uninstall_command: string | null;
};

export const TOOL_DISPLAY_NAMES: Record<ToolName, string> = {
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  grok: "Grok Build",
  opencode: "OpenCode",
  openclaw: "OpenClaw",
  hermes: "Hermes",
  pi: "Pi"
};

export function getToolVersions(tools?: string[], wslShellByTool?: Record<string, WslShellPreference>, force?: boolean) {
  return invoke<ToolVersion[]>("get_tool_versions", { tools, wslShellByTool, force });
}

export function runToolLifecycleAction(tools: string[], action: ToolLifecycleAction, wslShellByTool?: Record<string, WslShellPreference>) {
  return invoke<void>("run_tool_lifecycle_action", { tools, action, wslShellByTool });
}

export function probeToolInstallations(tools: string[]) {
  return invoke<ToolInstallationReport[]>("probe_tool_installations", { tools });
}

export function pathDefaultSource(installs: ToolInstallation[]): string | undefined {
  return (installs.find((inst) => inst.is_path_default) ?? installs[0])?.source;
}

export function mergeToolVersions(prev: ToolVersion[], updated: ToolVersion[]) {
  if (prev.length === 0) return updated;
  const byName = new Map(prev.map((tool) => [tool.name, tool]));
  for (const next of updated) {
    const prevTool = byName.get(next.name);
    if (!prevTool) {
      byName.set(next.name, next);
      continue;
    }
    byName.set(next.name, {
      ...prevTool,
      ...next,
      latest_version: next.latest_pending ? prevTool.latest_version : next.latest_version ?? prevTool.latest_version,
      latest_pending: next.latest_pending && !prevTool.latest_version,
      update_available: next.latest_pending ? prevTool.update_available : next.update_available
    });
  }
  const merged = prev.map((tool) => byName.get(tool.name) ?? tool);
  for (const tool of updated) if (!prev.some((item) => item.name === tool.name)) merged.push(byName.get(tool.name)!);
  return merged;
}
