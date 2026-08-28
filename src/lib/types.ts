export type ApplicationKind = "cursor" | "codex";

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
  importType: "oauth" | "token" | "jwt" | "native";
  subscription: { plan?: string; expiresAt?: number; billingCycleEnd?: string; checkedAt?: number };
  usage?: { kind: "currency" | "percent" | "requests"; used: number; limit?: number; percent: number };
  daysRemaining?: number;
  isCurrent: boolean;
  status?: "invalid" | "missing";
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
