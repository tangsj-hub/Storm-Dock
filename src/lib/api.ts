import { invoke } from "@tauri-apps/api/core";
import type { Account, ApplicationKind, ApplicationStatus, CursorPlugin, McpServer } from "./types";

export const listApplications = () => invoke<ApplicationStatus[]>("list_applications");
export const listAccounts = (kind: ApplicationKind) => invoke<Account[]>("list_accounts", { kind });
export const getDatabasePath = () => invoke<string>("get_database_path");
export const moveDatabase = (directory: string) => invoke<string>("move_database", { directory });
export const listCursorPlugins = () => invoke<CursorPlugin[]>("list_cursor_plugins");
export const listCodexPlugins = () => invoke<CursorPlugin[]>("list_codex_plugins");
export const setCodexPluginEnabled = (id: string, enabled: boolean) => invoke("set_codex_plugin_enabled", { id, enabled });
export const setCodexPluginCapabilityEnabled = (pluginId: string, capabilityId: string, kind: "skill" | "mcp", enabled: boolean) => invoke("set_codex_plugin_capability_enabled", { pluginId, capabilityId, kind, enabled });
export const listMcpServers = (kind: ApplicationKind) => invoke<McpServer[]>("list_mcp_servers", { kind });
export const setCursorPluginEnabled = (id: string, source: CursorPlugin["source"], enabled: boolean) => invoke("set_cursor_plugin_enabled", { id, source, enabled });
export const deleteCursorPlugin = (id: string, source: CursorPlugin["source"]) => invoke("delete_cursor_plugin", { id, source });
