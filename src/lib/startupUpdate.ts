import type { UpdateCheckResult } from "./updater";

export const STARTUP_UPDATE_DISMISSED_KEY = "storm-dock.startupUpdateDismissedVersion";

type MemoryStorage = {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
};

export function shouldPromptStartupUpdate({
  result,
  dismissedVersion
}: {
  result: UpdateCheckResult;
  dismissedVersion?: string | null;
}) {
  if (result.status !== "available") return false;
  const dismissed = dismissedVersion?.trim();
  return !dismissed || dismissed !== result.version;
}

export function readDismissedStartupUpdateVersion(storage: MemoryStorage = localStorage) {
  const value = storage.getItem(STARTUP_UPDATE_DISMISSED_KEY)?.trim();
  return value || undefined;
}

export function rememberDismissedStartupUpdate(
  version: string,
  storage: MemoryStorage = localStorage
) {
  const trimmed = version.trim();
  if (!trimmed) return;
  storage.setItem(STARTUP_UPDATE_DISMISSED_KEY, trimmed);
}
