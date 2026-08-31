export type ApplicationKind = "cursor" | "codex";

export function applicationKindFromQuery(search = window.location.search): ApplicationKind {
  return new URLSearchParams(search).get("kind") === "codex" ? "codex" : "cursor";
}

export function syncDocumentAppKind(kind?: ApplicationKind) {
  if (kind) document.documentElement.dataset.app = kind;
  else delete document.documentElement.dataset.app;
}

export function homePath(kind: ApplicationKind, notice?: string) {
  const params = new URLSearchParams({ kind });
  if (notice) params.set("notice", notice);
  return `/?${params}`;
}

export function editAccountPath(kind: ApplicationKind, id: string) {
  return `/edit.html?kind=${kind}&id=${encodeURIComponent(id)}`;
}

/** Host-neutral plugin contract. New integrations map their native format here. */
export type PluginCapability = { id: string; name: string; description?: string; kind: "skill" | "mcp" | "hook"; enabled: boolean };
export type PluginSource = "local" | "marketplace" | "claude" | "codex" | "other";
export type Plugin = { id: string; name: string; description?: string; icon?: string; source: PluginSource; enabled: boolean; teamRequired: boolean; capabilities: PluginCapability[] };
export type McpServer = { id: string; name: string };

export type LocalSession = { id: string; title: string; projectDir?: string; sourcePath: string; updatedAt: number };
export type LocalSessionMessage = { role: "user" | "assistant"; content: string; timestamp?: number };
export type SessionDeleteBatchResult = { deletedIds: string[]; failedIds: string[] };
export type CodexSession = LocalSession;
export type CodexSessionMessage = LocalSessionMessage;

export type ApplicationStatus = {
  kind: ApplicationKind;
  label: string;
  available: boolean;
  reason?: string;
};

export type Account = {
  id: string;
  label: string;
  email?: string;
  importType: "oauth" | "token" | "jwt" | "native" | "api_key";
  subscription: { plan?: string; expiresAt?: number; billingCycleEnd?: string; checkedAt?: number };
  usage?: { kind: "currency" | "percent" | "requests"; used: number; limit?: number; percent: number };
  daysRemaining?: number;
  isCurrent: boolean;
  status?: "invalid" | "missing";
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
