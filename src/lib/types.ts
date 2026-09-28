export type ApplicationKind = "cursor" | "codex" | "grok";

export const APPLICATION_KINDS = ["cursor", "codex", "grok"] as const;

export function isGrokBotHomeView(search = window.location.search) {
  return new URLSearchParams(search).get("kind") === "grokBot";
}

export function applicationKindFromQuery(search = window.location.search): ApplicationKind {
  const kind = new URLSearchParams(search).get("kind");
  if (kind === "grokBot") return "cursor";
  return kind === "codex" || kind === "grok" ? kind : "cursor";
}

export function syncDocumentAppKind(kind?: ApplicationKind | "grokBot") {
  if (kind) document.documentElement.dataset.app = kind;
  else delete document.documentElement.dataset.app;
}

export function homePath(kind: ApplicationKind | "grokBot", notice?: string) {
  const params = new URLSearchParams({ kind });
  if (notice) params.set("notice", notice);
  return `/?${params}`;
}

/** Usage detail needs API kind (cursor|grok); optional `from` restores the home tab on back. */
export function usagePath(accountId: string, kind: "cursor" | "grok", from?: "grokBot") {
  const params = new URLSearchParams({ accountId, kind });
  if (from) params.set("from", from);
  return `/usage.html?${params}`;
}

export function homeKindFromUsageQuery(search = window.location.search): ApplicationKind | "grokBot" {
  const params = new URLSearchParams(search);
  if (params.get("from") === "grokBot") return "grokBot";
  return params.get("kind") === "grok" ? "grok" : "cursor";
}

export function editAccountPath(kind: ApplicationKind, id: string) {
  return `/edit.html?kind=${kind}&id=${encodeURIComponent(id)}`;
}

/** Host-neutral plugin contract. New integrations map their native format here. */
export type PluginCapability = { id: string; name: string; description?: string; kind: "skill" | "mcp" | "hook"; enabled: boolean };
export type PluginSource = "local" | "marketplace" | "claude" | "codex" | "grok" | "other";
export type Plugin = { id: string; name: string; description?: string; icon?: string; source: PluginSource; enabled: boolean; teamRequired: boolean; capabilities: PluginCapability[] };
export type McpServer = { id: string; name: string; enabled: boolean };

export type LocalSession = { id: string; title: string; projectDir?: string; sourcePath: string; updatedAt: number };
export type SessionAttachment = { id?: string; name: string; mime?: string; size?: number; url?: string; path?: string };
export type AttachmentAvailabilityStatus = "available" | "expired" | "unavailable";
export type AttachmentAvailability = { status: AttachmentAvailabilityStatus; reason?: string };
export type LocalSessionMessage = { role: "user" | "assistant"; content: string; timestamp?: number; attachments?: SessionAttachment[] };
export type SessionDeleteBatchResult = { deletedIds: string[]; failedIds: string[] };
export type CodexSession = LocalSession;
export type CodexSessionMessage = LocalSessionMessage;

export type ApplicationStatus = {
  kind: ApplicationKind;
  label: string;
  available: boolean;
  reason?: string;
};

export type GrokBotStatus = {
  installed: boolean;
  signedIn: boolean;
  running: boolean;
  available: boolean;
  reason?: string;
  currentAccountId?: string;
  currentAccountLabel?: string;
};

export type Account = {
  id: string;
  label: string;
  email?: string;
  application?: ApplicationKind;
  importType: "oauth" | "token" | "jwt" | "native" | "api_key";
  subscription: { plan?: string; expiresAt?: number; billingCycleEnd?: string; checkedAt?: number };
  usage?: { kind: "currency" | "percent" | "requests"; used: number; limit?: number; percent: number };
  grokBotUsage?: { kind: "percent"; used: number; percent: number };
  grokBotResetAt?: string;
  resetAt?: string;
  daysRemaining?: number;
  isCurrent: boolean;
  isGrokBotCurrent?: boolean;
  status?: "invalid" | "missing" | "blocked";
  baseUrl?: string;
};

export function canSwitchToDesktop(account: Account) {
  return account.importType !== "token" && account.importType !== "jwt";
}

export type CursorUsageDetails = {
  accountId: string;
  label: string;
  email?: string;
  name?: string;
  membershipType?: string;
  primary: { kind: "currency" | "percent" | "requests"; used: number; limit?: number; percent: number };
  resetAt?: string;
  onDemand?: { kind: "currency"; used: number; limit?: number; percent: number };
  grokBot?: { kind: "percent"; used: number; percent: number };
  grokBotResetAt?: string;
  products?: { name: string; percent: number }[];
  models: { name: string; requests: number }[];
  weekly: { date: string; requests: number; onDemandCents: number; isOnDemand: boolean }[];
  weeklyAvailable: boolean;
  weeklyError?: string;
  events?: UsageEvent[];
  checkedAt: number;
};

export type UsageEvent = {
  timestamp: number;
  model?: string;
  requests: number;
  inputTokens?: number;
  outputTokens?: number;
  costUsd?: number;
  chargedCents?: number;
  onDemand: boolean;
};
