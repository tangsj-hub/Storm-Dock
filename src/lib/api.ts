import { invoke } from "@tauri-apps/api/core";
import type { Account, ApplicationKind, ApplicationStatus, CodexSession, CodexSessionMessage, McpServer, Plugin } from "./types";

export const listApplications = () => invoke<ApplicationStatus[]>("list_applications");
export const listAccounts = (kind: ApplicationKind) => invoke<Account[]>("list_accounts", { kind });
export const getDatabasePath = () => invoke<string>("get_database_path");
export const moveDatabase = (directory: string) => invoke<string>("move_database", { directory });
export const listCursorPlugins = () => invoke<Plugin[]>("list_cursor_plugins");
export const listCodexPlugins = () => invoke<Plugin[]>("list_codex_plugins");
export const listCodexSessions = () => invoke<CodexSession[]>("list_codex_sessions");
export const getCodexSessionMessages = (id: string) => invoke<CodexSessionMessage[]>("get_codex_session_messages", { id });
export const deleteCodexSession = (id: string) => invoke("delete_codex_session", { id });
export const launchCodexSession = (id: string) => invoke("launch_codex_session", { id });
export const setCodexPluginEnabled = (id: string, enabled: boolean) => invoke("set_codex_plugin_enabled", { id, enabled });
export const setCodexPluginCapabilityEnabled = (pluginId: string, capabilityId: string, kind: "skill" | "mcp", enabled: boolean) => invoke("set_codex_plugin_capability_enabled", { pluginId, capabilityId, kind, enabled });
export const deleteCodexPlugin = (id: string) => invoke("delete_codex_plugin", { id });
export const listMcpServers = (kind: ApplicationKind) => invoke<McpServer[]>("list_mcp_servers", { kind });
export const setCursorPluginEnabled = (id: string, source: Plugin["source"], enabled: boolean) => invoke("set_cursor_plugin_enabled", { id, source, enabled });
export const deleteCursorPlugin = (id: string, source: Plugin["source"]) => invoke("delete_cursor_plugin", { id, source });
