export type ApplicationKind = "cursor" | "codex" | "grok";

export const APPLICATION_KINDS = ["cursor", "codex", "grok"] as const;

export function applicationKindFromQuery(search = window.location.search): ApplicationKind {
  const kind = new URLSearchParams(search).get("kind");
  return kind === "codex" || kind === "grok" ? kind : "cursor";
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

export function modelsHomePath(notice?: string) {
  const params = new URLSearchParams({ models: "1" });
  if (notice) params.set("notice", notice);
  return `/?${params}`;
}

export function addModelPath() {
  return "/add-model.html";
}

export function modelDetailPath(source: ModelSource, repo: string) {
  const params = new URLSearchParams({ source, repo });
  return `/model-detail.html?${params}`;
}

export function modelSourceFromQuery(search = window.location.search): ModelSource | undefined {
  const source = new URLSearchParams(search).get("source");
  return source === "huggingface" || source === "modelscope" ? source : undefined;
}

export function modelRepoFromQuery(search = window.location.search) {
  return new URLSearchParams(search).get("repo")?.trim() || "";
}

export function homeModeFromQuery(search = window.location.search): "apps" | "models" {
  return new URLSearchParams(search).get("models") === "1" ? "models" : "apps";
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

export type ModelSource = "huggingface" | "modelscope";
export type ModelFormat = "all" | "gguf" | "safetensors" | "mlx" | "finetune";
export type RemoteModelFile = { path: string; size: number };
export type ModelFit = "fits" | "marginal" | "partial" | "ram" | "oom" | "unknown";
export type RemoteModelVariant = { id: string; label: string; size: number; files: string[]; fit: ModelFit };
export type RemoteModelHit = {
  source: ModelSource;
  repo: string;
  name: string;
  author: string;
  downloads?: number;
  likes?: number;
  library?: string;
  pipeline?: string;
  tags: string[];
  params?: string;
  updatedAt?: string;
};
export type RemoteModelCard = {
  author: string;
  name: string;
  description: string;
  tags: string[];
  license?: string;
  library?: string;
  pipeline?: string;
  baseModel?: string;
  downloads?: number;
  likes?: number;
  params?: string;
  updatedAt?: string;
};
export type RemoteModelProbe = {
  source: ModelSource;
  repo: string;
  revision: string;
  files: RemoteModelFile[];
  variants: RemoteModelVariant[];
  defaultVariantId: string;
  card: RemoteModelCard;
};
export type LocalLlm = {
  id: string;
  source: ModelSource;
  repo: string;
  revision: string;
  path: string;
  size: number;
  files: number;
};
export type DownloadJobStatus = "queued" | "downloading" | "paused" | "retry_wait" | "failed" | "verifying" | "completed" | "cancelled";
export type DownloadJob = {
  jobId: string;
  repo: string;
  source: ModelSource;
  revision: string;
  downloadedBytes: number;
  totalBytes: number;
  speedBps: number;
  currentFile: string;
  percent: number;
  status: DownloadJobStatus;
  error?: string;
  verification: "sha256" | "weak";
  retryCount: number;
  weaklyVerified: boolean;
};

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
