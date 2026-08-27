export type ApplicationKind = "cursor" | "codex";
export type PluginCapability = { id: string; name: string; description?: string; kind: "skill" | "mcp" | "hook"; enabled: boolean };
export type CursorPlugin = { id: string; name: string; description?: string; icon?: string; source: "local" | "marketplace" | "claude" | "codex"; enabled: boolean; teamRequired: boolean; capabilities: PluginCapability[] };
export type McpServer = { id: string; name: string };

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
