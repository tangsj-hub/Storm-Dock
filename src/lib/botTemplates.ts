export type BotTemplate = { url: string; name: string; description?: string | null; note?: string | null; autoImport?: boolean; createdAt: number; builtinId?: string | null };

/** Built-in templates ship with the app and are never exported. */
export const isExportable = (item: BotTemplate) => !item.builtinId;
export type BotTemplatePatch = { name: string; description?: string; note?: string; autoImport: boolean };
export type BotTemplateMeta = { name?: string | null; description?: string | null };

/** Client-side pre-check; the Rust side normalizes and validates authoritatively. */
export function isBotTemplateUrl(raw: string) {
  try {
    const url = new URL(raw.trim());
    if (url.protocol !== "https:" && url.protocol !== "http:") return false;
    if (!["x.ai", "www.x.ai"].includes(url.hostname.toLowerCase())) return false;
    const parts = url.pathname.split("/").filter(Boolean);
    return parts.length === 2 && parts[0] === "bot" && /^[A-Za-z0-9_-]+$/.test(parts[1]);
  } catch {
    return false;
  }
}

export const DESCRIPTION_LIMIT = 40;

export function isDescriptionTruncated(text: string, limit = DESCRIPTION_LIMIT) {
  return Array.from(text.trim()).length > limit;
}

/** Cut by code points so emoji / CJK are never split. */
export function truncateDescription(text: string, limit = DESCRIPTION_LIMIT) {
  const chars = Array.from(text.trim());
  return chars.length > limit ? `${chars.slice(0, limit).join("").trimEnd()}…` : chars.join("");
}

export type BotImportProgress = {
  total: number;
  position: number;
  currentName?: string | null;
  done: boolean;
  cancelled: boolean;
  imported: string[];
  skipped: string[];
  /** Set when import was blocked by the client version ("" = unknown). */
  blockedVersion?: string | null;
  requiredVersion: string;
};

export type BotImportGate = { supported: boolean; current?: string | null; required: string };
