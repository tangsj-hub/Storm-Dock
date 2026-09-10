import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";

export type UpdateCheckResult =
  | { status: "up-to-date" }
  | { status: "available"; version: string; notes?: string; date?: string };

export async function getCurrentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "";
  }
}

export async function checkForAppUpdate(): Promise<UpdateCheckResult> {
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check({ timeout: 30_000 });
  if (!update) {
    return { status: "up-to-date" };
  }
  return {
    status: "available",
    version: update.version,
    notes: update.body ?? undefined,
    date: update.date ?? undefined
  };
}

/** Download, install, and restart via the Rust command (safer on macOS). */
export function installUpdateAndRestart(): Promise<boolean> {
  return invoke<boolean>("install_update_and_restart");
}
