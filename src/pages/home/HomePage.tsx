import { listen } from "@tauri-apps/api/event";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Progress from "@radix-ui/react-progress";
import * as Tabs from "@radix-ui/react-tabs";
import {
  Check,
  ChartNoAxesCombined,
  CheckSquare,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Clock3,
  Download,
  FileOutput,
  FolderOpen,
  GripVertical,
  KeyRound,
  LogIn,
  MessageSquareText,
  Play,
  Plus,
  Puzzle,
  RefreshCw,
  Search,
  Settings,
  Trash2,
  UserRound,
  Waypoints,
  X,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import {
  type ComponentType,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import { ExportDialog } from "../../components/ExportDialog";
import { CopyIconButton } from "../../components/CopyIconButton";
import { Tooltip } from "../../components/Tooltip";
import {
  cachedPluginCount,
  PluginCatalog,
  type PluginHost,
} from "../../components/plugins/PluginCatalog";
import logo from "../../assets/logo.svg";
import codexIcon from "../../assets/codex.svg";
import cursorIcon from "../../assets/cursor.svg";
import grokIcon from "../../assets/tools/grok.svg";
import "../../i18n";
import {
  deleteCodexPlugin,
  deleteCodexSession,
  deleteCodexSessions,
  deleteCursorSession,
  deleteCursorSessions,
  deleteCursorPlugin,
  deleteGrokPlugin,
  deleteGrokSession,
  deleteGrokSessions,
  getCodexSessionMessages,
  getCursorSessionMessages,
  getGrokSessionMessages,
  launchCodexSession,
  launchGrokSession,
  listAccounts,
  listApplications,
  listCodexPlugins,
  listCodexSessions,
  listCursorSessions,
  listCursorPlugins,
  listGrokPlugins,
  listGrokSessions,
  listMcpServers,
  setCodexPluginCapabilityEnabled,
  setCodexPluginEnabled,
  setCursorPluginEnabled,
  setGrokPluginEnabled,
  setMcpServerEnabled,
} from "../../lib/api";
import {
  APPLICATION_KINDS,
  applicationKindFromQuery,
  canSwitchToDesktop,
  homePath,
  syncDocumentAppKind,
  type Account,
  type ApplicationKind,
  type ApplicationStatus,
  type CodexSession,
  type CodexSessionMessage,
  type McpServer,
} from "../../lib/types";
import "../../styles/global.css";
import styles from "./page.module.css";
import {
  subscriptionLabel,
  usageLabel,
} from "./lib/accountPresentation";
import {
  filterSessions,
  formatRelativeSessionTime,
  groupSessions,
  removeSelectedIds,
  toggleSelectedIds,
} from "./lib/sessionPresentation";
import { useLatestRequest } from "./hooks/useLatestRequest";
import { WorkspaceToolbar } from "./components/WorkspaceToolbar";
import { AccountList } from "./components/AccountList";
import { SessionWorkspace, type SessionProvider } from "./components/SessionWorkspace";
import type { WorkspaceSection, SwitchProgress } from "./types";

type SwitchOutcome = { restartRequired: boolean };
const APP_ICONS: Record<ApplicationKind, string> = {
  cursor: cursorIcon,
  codex: codexIcon,
  grok: grokIcon,
};

const workspaceSections: Array<{
  id: WorkspaceSection;
  icon: ComponentType<{
    "aria-hidden"?: boolean | "true" | "false";
    size?: number;
  }>;
  labelKey: string;
}> = [
  { id: "accounts", icon: UserRound, labelKey: "accounts" },
  { id: "sessions", icon: MessageSquareText, labelKey: "sessions" },
  { id: "plugins", icon: Puzzle, labelKey: "plugins" },
  { id: "mcp", icon: Waypoints, labelKey: "mcp" },
];

function legacySubscriptionLabel(
  account: Account,
  t: (key: string, options?: Record<string, unknown>) => string,
) {
  const plan = account.subscription.plan;
  if (!plan) return undefined;
  const name = t(`subscriptionPlans.${plan.toLowerCase()}`, {
    defaultValue: plan,
  });
  if (!account.subscription.expiresAt)
    return {
      name,
      expiry: t("subscriptionUnknownExpiry"),
      plan: plan.toLowerCase(),
    };
  const days = account.daysRemaining;
  if (days === undefined)
    return {
      name,
      expiry: t("subscriptionUnknownExpiry"),
      plan: plan.toLowerCase(),
    };
  if (days > 0)
    return {
      name,
      expiry: t("subscriptionDays", { count: days }),
      plan: plan.toLowerCase(),
    };
  if (days === 0)
    return { name, expiry: t("subscriptionToday"), plan: plan.toLowerCase() };
  return { name, expiry: t("subscriptionExpired"), plan: plan.toLowerCase() };
}

function legacyUsageLabel(
  account: Account,
  t: (key: string, options?: Record<string, unknown>) => string,
) {
  const usage = account.usage;
  if (!usage)
    return account.subscription.plan?.toLowerCase() === "free"
      ? t("usageFree")
      : undefined;
  if (usage.kind === "currency") {
    return t("usageSpent", { amount: `$${(usage.used / 100).toFixed(2)}` });
  }
  return account.subscription.plan?.toLowerCase() === "free"
    ? t("usageFree")
    : undefined;
}

function legacyFormatRelativeSessionTime(
  timestamp: number,
  t: (key: string, options?: Record<string, unknown>) => string,
) {
  const elapsed = Math.max(0, Date.now() - timestamp);
  const minutes = Math.floor(elapsed / 60_000);
  const hours = Math.floor(elapsed / 3_600_000);
  const days = Math.floor(elapsed / 86_400_000);
  if (minutes < 1) return t("sessionsJustNow");
  if (minutes < 60) return t("sessionsMinutesAgo", { count: minutes });
  if (hours < 24) return t("sessionsHoursAgo", { count: hours });
  if (days < 7) return t("sessionsDaysAgo", { count: days });
  return new Date(timestamp).toLocaleDateString();
}

function LegacySortableAccount({
  account,
  busy,
  onExport,
  onRemove,
  onSwitch,
  progress,
}: {
  account: Account;
  busy: boolean;
  onExport: (account: Account) => void;
  onRemove: (account: Account) => void;
  onSwitch: (account: Account) => void;
  progress?: SwitchProgress;
}) {
  const { t } = useTranslation();
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ disabled: busy, id: account.id });
  return (
    <article
      className={`${styles.accountCard} ${account.isCurrent ? styles.current : ""} ${isDragging ? styles.dragging : ""}`}
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
    >
      <GripVertical
        className={styles.dragHandle}
        aria-label={t("drag", { account: account.label })}
        size={24}
        {...attributes}
        {...listeners}
      />
      <div className={styles.accountCopy}>
        <strong>{account.label}</strong>
        <div className={styles.accountMeta}>
          {(() => {
            const subscription = subscriptionLabel(account, t);
            return (
              subscription && (
                <span
                  className={`${styles.metaBadge} ${styles[`plan-${subscription.plan}`] ?? styles.planDefault}`}
                >
                  {subscription.name} · {subscription.expiry}
                </span>
              )
            );
          })()}
          {(() => {
            const usage = usageLabel(account, t);
            return usage && <span className={styles.metaBadge}>{usage}</span>;
          })()}
          {account.status === "invalid" && (
            <span className={styles.invalidBadge}>
              {t("tokenInvalid", { defaultValue: "Token已失效" })}
            </span>
          )}
          {account.status === "missing" && (
            <span className={styles.missingBadge}>
              {t("credentialMissing", { defaultValue: "凭证缺失" })}
            </span>
          )}
        </div>
      </div>
      <div className={styles.accountActions}>
        {progress ? (
          <div className={styles.progress}>
            <span>{t(`switchStages.${progress.stage}`)}</span>
            <Progress.Root
              aria-label={t("switchProgress")}
              className={styles.progressRoot}
              value={progress.percent}
            >
              <Progress.Indicator
                className={
                  progress.status === "error"
                    ? styles.progressError
                    : styles.progressIndicator
                }
                style={{ transform: `translateX(-${100 - progress.percent}%)` }}
              />
            </Progress.Root>
          </div>
        ) : account.isCurrent ? (
          <span className={styles.currentBadge}>
            <Check aria-hidden="true" size={16} />
            {t("current")}
          </span>
        ) : canSwitchToDesktop(account) ? (
          <button
            className={styles.activate}
            disabled={busy}
            onClick={() => onSwitch(account)}
            type="button"
          >
            <LogIn aria-hidden="true" size={17} />
            {t("switch")}
          </button>
        ) : null}
        {progress?.status === "error" && canSwitchToDesktop(account) && (
          <button
            className={styles.activate}
            onClick={() => onSwitch(account)}
            type="button"
          >
            <RefreshCw aria-hidden="true" size={16} />
            {t("retry")}
          </button>
        )}
        <Tooltip content={t("usage")}>
          <a
            aria-label={t("viewUsage", { account: account.label })}
            className={styles.iconButton}
            href={`/usage.html?accountId=${encodeURIComponent(account.id)}`}
          >
            <ChartNoAxesCombined aria-hidden="true" size={18} />
          </a>
        </Tooltip>
        <Tooltip content={t("export")}>
          <button
            aria-label={t("exportAccount", { account: account.label })}
            className={styles.iconButton}
            disabled={busy}
            onClick={() => onExport(account)}
            type="button"
          >
            <FileOutput aria-hidden="true" size={18} />
          </button>
        </Tooltip>
        <Tooltip content={t("delete")}>
          <button
            aria-label={t("remove", { account: account.label })}
            className={styles.iconButton}
            disabled={busy}
            onClick={() => onRemove(account)}
            type="button"
          >
            <Trash2 aria-hidden="true" size={19} />
          </button>
        </Tooltip>
      </div>
    </article>
  );
}

function LegacyWorkspaceToolbar({
  section,
  busy,
  canManageAccounts,
  hasAccounts,
  pluginsExpanded,
  refreshing,
  sessionsRefreshing,
  onExport,
  onPluginsExpandedChange,
  onRefresh,
  onSessionsRefresh,
}: {
  section: WorkspaceSection;
  busy: boolean;
  canManageAccounts: boolean;
  hasAccounts: boolean;
  pluginsExpanded: boolean;
  refreshing: boolean;
  sessionsRefreshing: boolean;
  onExport: () => void;
  onPluginsExpandedChange: (expanded: boolean) => void;
  onRefresh: () => void;
  onSessionsRefresh: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      aria-label={t("sectionActions")}
      className={styles.contextToolbar}
      data-section={section}
    >
      {section === "accounts" && (
        <>
          <Tooltip content={t("export")}>
            <button
              aria-label={t("export")}
              className={styles.iconButton}
              disabled={busy || !canManageAccounts || !hasAccounts}
              onClick={onExport}
              type="button"
            >
              <Download aria-hidden="true" size={19} />
            </button>
          </Tooltip>
          <Tooltip content={t("refresh")}>
            <button
              aria-label={t("refresh")}
              className={styles.iconButton}
              disabled={busy || !canManageAccounts}
              onClick={onRefresh}
              type="button"
            >
              <RefreshCw
                aria-hidden="true"
                className={refreshing ? styles.spinning : undefined}
                size={19}
              />
            </button>
          </Tooltip>
          <a
            aria-disabled={busy || !canManageAccounts}
            className={styles.addButton}
            href={busy || !canManageAccounts ? undefined : "/add.html"}
          >
            <Plus aria-hidden="true" size={18} />
            {t("addAccount")}
          </a>
        </>
      )}
      {section === "plugins" && (
        <Tooltip
          content={
            pluginsExpanded
              ? t("collapsePluginChildren")
              : t("expandPluginChildren")
          }
        >
          <button
            aria-expanded={pluginsExpanded}
            aria-label={
              pluginsExpanded
                ? t("collapsePluginChildren")
                : t("expandPluginChildren")
            }
            className={styles.iconButton}
            onClick={() => onPluginsExpandedChange(!pluginsExpanded)}
            type="button"
          >
            {pluginsExpanded ? (
              <ChevronsDownUp aria-hidden="true" size={19} />
            ) : (
              <ChevronsUpDown aria-hidden="true" size={19} />
            )}
          </button>
        </Tooltip>
      )}
      {section === "sessions" && (
        <Tooltip content={t("refreshSessions")}>
          <button
            aria-label={t("refreshSessions")}
            className={styles.iconButton}
            disabled={sessionsRefreshing}
            onClick={onSessionsRefresh}
            type="button"
          >
            <RefreshCw
              aria-hidden="true"
              className={sessionsRefreshing ? styles.spinning : undefined}
              size={19}
            />
          </button>
        </Tooltip>
      )}
    </div>
  );
}

export function HomePage() {
  const { t } = useTranslation();
  const [applications, setApplications] = useState<ApplicationStatus[]>([]);
  const [selected, setSelected] = useState<ApplicationKind>(applicationKindFromQuery);
  const [workspaceSection, setWorkspaceSection] =
    useState<WorkspaceSection>("accounts");
  const [pluginsExpanded, setPluginsExpanded] = useState(false);
  const [pluginCount, setPluginCount] = useState<number | undefined>(() =>
    cachedPluginCount(applicationKindFromQuery()),
  );
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [mcpPending, setMcpPending] = useState<Set<string>>(() => new Set());
  const [codexSessions, setCodexSessions] = useState<CodexSession[]>([]);
  const [sessionsRefreshing, setSessionsRefreshing] = useState(false);
  const [expandedSessionProjects, setExpandedSessionProjects] = useState<
    Set<string>
  >(() => new Set());
  const [selectedCodexSessionId, setSelectedCodexSessionId] =
    useState<string>();
  const [codexSessionMessages, setCodexSessionMessages] = useState<
    CodexSessionMessage[]
  >([]);
  const [sessionMessagesLoading, setSessionMessagesLoading] = useState(false);
  const [sessionSearch, setSessionSearch] = useState("");
  const [sessionSearchOpen, setSessionSearchOpen] = useState(false);
  const [sessionSelectionMode, setSessionSelectionMode] = useState(false);
  const [selectedSessionIds, setSelectedSessionIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [sessionDeleteTargets, setSessionDeleteTargets] = useState<
    string[] | undefined
  >();
  const [sessionsDeleting, setSessionsDeleting] = useState(false);
  const [sessionRefreshKey, setSessionRefreshKey] = useState(0);
  const [busy, setBusy] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshFailed, setRefreshFailed] = useState(false);
  const [notice, setNotice] = useState(
    () =>
      new URLSearchParams(window.location.search).get("notice") ?? undefined,
  );
  const [switchProgress, setSwitchProgress] = useState<SwitchProgress>();
  const [restartDialog, setRestartDialog] = useState<{
    account: Account;
    operationId: string;
  }>();
  const [grokBotDialog, setGrokBotDialog] = useState<Account>();
  const [exportData, setExportData] = useState<unknown>();
  const [exportTarget, setExportTarget] = useState<Account>();
  const [testingId, setTestingId] = useState<string>();
  const [countdown, setCountdown] = useState(10);
  const activeOperationId = useRef<string | undefined>(undefined);
  const beginAccountsRequest = useLatestRequest();
  const knownSessionProjects = useRef(new Set<string>());
  const beginSessionMessageRequest = useLatestRequest();
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );
  const isCursor = selected === "cursor";
  useLayoutEffect(() => {
    syncDocumentAppKind(selected);
  }, [selected]);
  const showError = useCallback(
    (error: unknown) =>
      setNotice(error instanceof Error ? error.message : String(error)),
    [],
  );
  const loadAccounts = useCallback(async () => {
    const isCurrent = beginAccountsRequest();
    const [nextApplications, nextAccounts] = await Promise.all([
      listApplications(),
      listAccounts(selected),
    ]);
    if (!isCurrent()) return;
    setApplications(nextApplications);
    setAccounts(nextAccounts);
  }, [beginAccountsRequest, selected]);
  const selectApplication = (next: ApplicationKind) => {
    if (next === selected) {
      window.history.replaceState({}, "", homePath(next));
      return;
    }
    setMcpServers([]);
    setMcpPending(new Set());
    setAccounts([]);
    setSelected(next);
    window.history.replaceState({}, "", homePath(next));
  };
  useEffect(() => {
    void loadAccounts().catch(showError);
  }, [loadAccounts, showError]);
  useEffect(() => {
    if (workspaceSection === "mcp")
      void listMcpServers(selected).then(setMcpServers).catch(showError);
  }, [selected, workspaceSection]);
  const loadCodexSessions = useCallback(async () => {
    setSessionsRefreshing(true);
    try {
      setCodexSessions(
        selected === "codex" ? await listCodexSessions() : await listCursorSessions(),
      );
    } catch (error) {
      showError(error);
    } finally {
      setSessionsRefreshing(false);
    }
  }, [selected, showError]);
  useEffect(() => {
    const projects = new Set(
      codexSessions.map(
        (session) => session.projectDir?.trim() || "__unknown__",
      ),
    );
    setExpandedSessionProjects((current) => {
      const next = new Set(
        [...current].filter((project) => projects.has(project)),
      );
      knownSessionProjects.current = projects;
      return next;
    });
  }, [codexSessions]);
  useEffect(() => {
    if (
      selectedCodexSessionId &&
      codexSessions.some((session) => session.id === selectedCodexSessionId)
    )
      return;
    setSelectedCodexSessionId(codexSessions[0]?.id);
  }, [codexSessions, selectedCodexSessionId]);
  useEffect(() => {
    if (!selectedCodexSessionId) {
      setCodexSessionMessages([]);
      return;
    }
    const isCurrent = beginSessionMessageRequest();
    setSessionMessagesLoading(true);
    void (selected === "codex"
      ? getCodexSessionMessages(selectedCodexSessionId)
      : getCursorSessionMessages(selectedCodexSessionId))
      .then((messages) => {
        if (isCurrent())
          setCodexSessionMessages(messages);
      })
      .catch(showError)
      .finally(() => {
        if (isCurrent())
          setSessionMessagesLoading(false);
      });
  }, [beginSessionMessageRequest, selected, selectedCodexSessionId, showError]);
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.has("notice")) {
      params.delete("notice");
      window.history.replaceState({}, "", `/?${params}`);
    }
  }, []);
  useEffect(() => {
    let unlisten: () => void = () => {};
    void listen("accounts-changed", () => {
      void loadAccounts().catch(showError);
    }).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten();
  }, [loadAccounts, showError]);
  useEffect(() => {
    let unlisten: () => void = () => {};
    void listen<SwitchProgress>("account-switch-progress", ({ payload }) => {
      if (payload.operationId === activeOperationId.current)
        setSwitchProgress(payload);
    }).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten();
  }, []);
  useEffect(() => {
    let unlisten: () => void = () => {};
    void listen<{ completed: number; total: number }>(
      "account-refresh-progress",
      ({ payload }) => {
        setNotice(t("refreshProgress", payload));
      },
    ).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten();
  }, [t]);

  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    setNotice(undefined);
    try {
      await work();
      await loadAccounts();
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      setBusy(false);
    }
  };
  const clearSwitch = () => {
    activeOperationId.current = undefined;
    setSwitchProgress(undefined);
    setBusy(false);
  };
  const finishSwitch = async (noticeKey: string) => {
    await loadAccounts();
    setNotice(t(noticeKey));
    window.setTimeout(clearSwitch, 500);
  };
  const switchTo = async (account: Account) => {
    if (!canSwitchToDesktop(account)) return;
    const operationId = crypto.randomUUID();
    activeOperationId.current = operationId;
    setBusy(true);
    setNotice(undefined);
    try {
      const outcome = await invoke<SwitchOutcome>("switch_account", {
        id: account.id,
        operationId,
      });
      if (outcome.restartRequired) {
        // The first phase only validates and prepares the switch. Do not show
        // its progress as an active switch while waiting for confirmation.
        setSwitchProgress(undefined);
        setBusy(false);
        setRestartDialog({ account, operationId });
        setCountdown(10);
        return;
      }
      await finishSwitch(isCursor ? "cursorLaunched" : "accountSwitched");
    } catch (error) {
      showError(error);
      setSwitchProgress({
        operationId,
        accountId: account.id,
        stage: "error",
        percent: 100,
        status: "error",
      });
      setBusy(false);
    }
  };
  const launchBot = async (account: Account) => {
    if (busy) return;
    setBusy(true);
    try {
      const result = await invoke<{ status: "same" | "ready" | "requiresConfirmation" }>("prepare_launch_grok_bot", { id: account.id });
      if (result.status === "requiresConfirmation") {
        setGrokBotDialog(account);
        setCountdown(10);
        return;
      }
      if (result.status === "ready") {
        await invoke("confirm_launch_grok_bot", { id: account.id });
      }
      setNotice(result.status === "same" ? t("grokBotAlreadyActive") : t("grokBotLaunched"));
    } catch (error) { setNotice(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };
  const confirmLaunchBot = async () => {
    const account = grokBotDialog;
    if (!account || busy) return;
    setGrokBotDialog(undefined);
    setBusy(true);
    try {
      await invoke("confirm_launch_grok_bot", { id: account.id });
      setNotice(t("grokBotLaunched"));
    } catch (error) { setNotice(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };
  const cancelLaunchBot = () => {
    setGrokBotDialog(undefined);
    setNotice(undefined);
    setBusy(false);
  };
  const cancelRestart = () => {
    setRestartDialog(undefined);
    setNotice(undefined);
    clearSwitch();
  };
  const forceRestart = async () => {
    const dialog = restartDialog;
    if (!dialog) return;
    setRestartDialog(undefined);
    setBusy(true);
    setSwitchProgress({
      operationId: dialog.operationId,
      accountId: dialog.account.id,
      stage: "terminating",
      percent: 0,
      status: "running",
    });
    try {
      await invoke("force_restart_cursor", {
        id: dialog.account.id,
        operationId: dialog.operationId,
      });
      await finishSwitch("cursorRestarted");
    } catch (error) {
      showError(error);
      setSwitchProgress({
        operationId: dialog.operationId,
        accountId: dialog.account.id,
        stage: "error",
        percent: 100,
        status: "error",
      });
      setBusy(false);
    }
  };
  useEffect(() => {
    if (!restartDialog && !grokBotDialog) return;
    const timer = window.setInterval(() => {
      setCountdown((seconds) => {
        if (seconds <= 1) {
          window.clearInterval(timer);
          if (restartDialog) cancelRestart();
          else cancelLaunchBot();
          return 0;
        }
        return seconds - 1;
      });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [restartDialog?.operationId, grokBotDialog?.id]);
  const remove = (account: Account) =>
    act(async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("delete_account", { id: account.id });
      setNotice(t("deleted"));
    });
  const refresh = async () => {
    setBusy(true);
    setRefreshing(true);
    setRefreshFailed(false);
    setNotice(t("refreshing"));
    try {
      if (!selected || selected === "grok") {
        await loadAccounts();
        setNotice(t("refreshed"));
        return;
      }
      const result = await invoke<{
        total: number;
        failed: number;
        invalid: number;
        missing: number;
      }>(
        selected === "cursor"
          ? "refresh_all_cursor_accounts"
          : "refresh_all_codex_accounts",
      );
      const failed = result.failed;
      await loadAccounts();
      const other = failed - result.invalid - result.missing;
      const reasons = [
        result.invalid && t("refreshTokenInvalid", { count: result.invalid }),
        result.missing &&
          t("refreshCredentialMissing", { count: result.missing }),
        other && t("refreshOtherFailed", { count: other }),
      ]
        .filter(Boolean)
        .join("，");
      setNotice(
        failed
          ? t("subscriptionsRefreshIncomplete", { reasons })
          : t(accounts.length ? "subscriptionsRefreshed" : "refreshed"),
      );
    } catch (error) {
      setRefreshFailed(true);
      showError(error);
    } finally {
      setBusy(false);
      setRefreshing(false);
    }
  };
  const reorder = async (activeId: string, targetId?: string) => {
    (document.activeElement as HTMLElement | null)?.blur();
    if (!targetId || activeId === targetId) return;
    const from = accounts.findIndex((account) => account.id === activeId);
    const to = accounts.findIndex((account) => account.id === targetId);
    if (from < 0 || to < 0) return;
    const next = arrayMove(accounts, from, to);
    setAccounts(next);
    if (
      await act(async () => {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("reorder_accounts", {
          kind: selected,
          ids: next.map((account) => account.id),
        });
      })
    )
      setNotice(t("reordered"));
  };
  const exportAccounts = async () => {
    const file = await save({
      defaultPath: `${selected}-accounts.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
      title: t("exportTitle"),
    });
    if (!file) return;
    if (
      await act(async () => {
        await invoke("export_cursor_accounts", { file, kind: selected });
      })
    )
      setNotice(t("exported"));
  };
  const openAccountExport = async (account: Account) => {
    if (account.importType === "api_key") return;
    try {
      setExportData(
        await invoke<unknown>("get_cursor_export_record", { id: account.id }),
      );
      setExportTarget(account);
    } catch (error) {
      showError(error);
    }
  };
  const duplicateAccount = async (account: Account) => {
    if (
      await act(async () => {
        await invoke("duplicate_codex_api_key_account", { id: account.id });
      })
    )
      setNotice(t("accountDuplicated"));
  };
  const testApiKey = async (account: Account) => {
    setTestingId(account.id);
    try {
      const result = await invoke<{
        success: boolean;
        message: string;
        responseTimeMs?: number;
      }>("test_codex_api_key_account", { id: account.id });
      setNotice(
        result.success
          ? t("connectionOk", { ms: result.responseTimeMs ?? 0 })
          : t("connectionFail", { error: result.message }),
      );
    } catch (error) {
      showError(error);
    } finally {
      setTestingId(undefined);
    }
  };
  const currentAccountId = accounts.find((account) => account.isCurrent)?.id;
  useEffect(() => {
    setPluginCount(cachedPluginCount(selected, currentAccountId));
  }, [currentAccountId, selected]);
  const pluginHost = useMemo<PluginHost>(
    () =>
      selected === "codex"
        ? {
            application: selected,
            list: listCodexPlugins,
            setEnabled: async (plugin, enabled) => {
              await setCodexPluginEnabled(plugin.id, enabled);
            },
            setCapabilityEnabled: async (plugin, capability, enabled) => {
              if (capability.kind === "hook") return;
              await setCodexPluginCapabilityEnabled(
                plugin.id,
                capability.id,
                capability.kind,
                enabled,
              );
            },
            remove: async (plugin) => {
              await deleteCodexPlugin(plugin.id);
            },
          }
        : selected === "grok"
          ? {
              application: selected,
              list: listGrokPlugins,
              setEnabled: async (plugin, enabled) => {
                await setGrokPluginEnabled(plugin.id, enabled);
              },
              remove: async (plugin) => {
                await deleteGrokPlugin(plugin.id);
              },
            }
        : {
            application: selected,
            accountId: currentAccountId,
            list: listCursorPlugins,
            setEnabled: async (plugin, enabled) => {
              await setCursorPluginEnabled(plugin.id, plugin.source, enabled);
            },
            remove: async (plugin) => {
              await deleteCursorPlugin(plugin.id, plugin.source);
            },
          },
    [currentAccountId, selected],
  );
  const pluginChanged = useCallback(
    () =>
      setNotice(
        t("pluginRestartRequired", {
          application: t(selected),
        }),
      ),
    [selected, t],
  );
  const canToggleMcp = selected === "cursor" || selected === "codex";
  const toggleMcp = async (server: McpServer) => {
    if (!canToggleMcp || mcpPending.has(server.id)) return;
    setMcpPending((current) => new Set(current).add(server.id));
    try {
      await setMcpServerEnabled(selected, server.id, !server.enabled);
      setMcpServers((current) =>
        current.map((item) =>
          item.id === server.id ? { ...item, enabled: !server.enabled } : item,
        ),
      );
      setNotice(t("mcpRestartRequired", { application: t(selected) }));
    } catch (error) {
      showError(error);
    } finally {
      setMcpPending((current) => {
        const next = new Set(current);
        next.delete(server.id);
        return next;
      });
    }
  };
  const sessionProvider = useMemo<SessionProvider>(
    () => selected === "codex"
      ? { id: "codex", label: t("codex"), icon: codexIcon, list: listCodexSessions, loadMessages: getCodexSessionMessages, remove: deleteCodexSession, removeMany: deleteCodexSessions, launch: launchCodexSession }
      : selected === "grok"
        ? { id: "grok", label: t("grok"), icon: grokIcon, list: listGrokSessions, loadMessages: getGrokSessionMessages, remove: deleteGrokSession, removeMany: deleteGrokSessions, launch: launchGrokSession }
      : { id: "cursor", label: t("cursor"), icon: cursorIcon, list: listCursorSessions, loadMessages: getCursorSessionMessages, remove: deleteCursorSession, removeMany: deleteCursorSessions },
    [selected, t],
  );
  const visibleCodexSessions = useMemo(
    () => filterSessions(codexSessions, sessionSearch),
    [codexSessions, sessionSearch],
  );
  const sessionProjects = useMemo(
    () => groupSessions(visibleCodexSessions),
    [visibleCodexSessions],
  );
  const renderSection = () => {
    const applicationLabel =
      applications.find((app) => app.kind === selected)?.label ?? t(selected);
    if (workspaceSection !== "accounts") {
      const section = workspaceSections.find(
        ({ id }) => id === workspaceSection,
      )!;
      if (workspaceSection === "plugins")
        return (
          <PluginCatalog
            disabled={busy}
            expanded={pluginsExpanded}
            host={pluginHost}
            onChanged={pluginChanged}
            onError={showError}
            onPluginsChange={(plugins) => setPluginCount(plugins.length)}
          />
        );
      if (workspaceSection === "mcp")
        return mcpServers.length ? (
          <div className={styles.pluginList}>
            {mcpServers.map((server) => (
              <article className={styles.pluginCard} key={server.id}>
                <div className={styles.pluginHeader}>
                  <span className={styles.pluginIcon}>
                    <Waypoints aria-hidden="true" size={20} />
                  </span>
                  <strong>{server.name}</strong>
                  {canToggleMcp && (
                    <div className={styles.pluginActions}>
                      <Tooltip
                        content={
                          server.enabled ? t("mcpDisable") : t("mcpEnable")
                        }
                      >
                        <button
                          aria-checked={server.enabled}
                          aria-label={
                            server.enabled ? t("mcpDisable") : t("mcpEnable")
                          }
                          className={styles.pluginSwitch}
                          disabled={busy || mcpPending.has(server.id)}
                          onClick={() => void toggleMcp(server)}
                          role="switch"
                          type="button"
                        >
                          <span />
                        </button>
                      </Tooltip>
                    </div>
                  )}
                </div>
              </article>
            ))}
          </div>
        ) : (
          <div className={styles.empty}>
            <Waypoints aria-hidden="true" size={32} />
            <h2>
              {applicationLabel} · {t("mcpTitle")}
            </h2>
            <p>{t("mcpDescription")}</p>
          </div>
        );
      if (workspaceSection === "sessions")
        return <SessionWorkspace key={sessionProvider.id} onError={showError} onNotice={setNotice} onRefreshingChange={setSessionsRefreshing} provider={sessionProvider} refreshKey={sessionRefreshKey} />;
      if (workspaceSection === "sessions") {
        if (sessionsRefreshing && codexSessions.length === 0)
          return (
            <div className={styles.empty}>
              <RefreshCw
                aria-hidden="true"
                className={styles.spinning}
                size={32}
              />
              <h2>{t("sessionsLoading")}</h2>
            </div>
          );
        if (codexSessions.length === 0)
          return (
            <div className={styles.empty}>
              <MessageSquareText aria-hidden="true" size={32} />
              <h2>{t("sessionsEmptyTitle")}</h2>
              <p>{t("sessionsEmptyDescription")}</p>
            </div>
          );
        const visibleSessions = visibleCodexSessions;
        const projects = sessionProjects;
        const selectedSession = codexSessions.find(
          (session) => session.id === selectedCodexSessionId,
        );
        const toggleAllVisible = () =>
          setSelectedSessionIds((current) =>
            toggleSelectedIds(current, visibleSessions.map((session) => session.id)),
          );
        const deleteSelectedSessions = async () => {
          const targets = sessionDeleteTargets ?? [];
          if (!targets.length || sessionsDeleting) return;
          setSessionsDeleting(true);
          setNotice(undefined);
          const results = await Promise.allSettled(
            targets.map((id) => deleteCodexSession(id)),
          );
          const deletedIds = new Set(
            targets.filter((_, index) => results[index].status === "fulfilled"),
          );
          const failedCount = targets.length - deletedIds.size;
          if (deletedIds.size) {
            setCodexSessions((current) =>
              current.filter((session) => !deletedIds.has(session.id)),
            );
            setSelectedSessionIds((current) => removeSelectedIds(current, deletedIds));
            if (
              selectedCodexSessionId &&
              deletedIds.has(selectedCodexSessionId)
            )
              setSelectedCodexSessionId(undefined);
          }
          setSessionDeleteTargets(undefined);
          setSessionsDeleting(false);
          if (failedCount)
            showError(t("sessionsBatchDeleteFailed", { count: failedCount }));
          else setNotice(t("sessionsBatchDeleted", { count: deletedIds.size }));
        };
        return (
          <>
            <div className={styles.sessionsLayout}>
              <div className={styles.sessionPane}>
                <header className={styles.sessionToolbar}>
                  {sessionSearchOpen ? (
                    <div className={styles.sessionSearch}>
                      <Search aria-hidden="true" size={15} />
                      <input
                        autoFocus
                        onChange={(event) =>
                          setSessionSearch(event.target.value)
                        }
                        onKeyDown={(event) => {
                          if (event.key === "Escape") {
                            setSessionSearch("");
                            setSessionSearchOpen(false);
                          }
                        }}
                        placeholder={t("searchSessions")}
                        value={sessionSearch}
                      />
                      <button
                        aria-label={t("close")}
                        onClick={() => {
                          setSessionSearch("");
                          setSessionSearchOpen(false);
                        }}
                        type="button"
                      >
                        <X aria-hidden="true" size={15} />
                      </button>
                    </div>
                  ) : (
                    <>
                      <div className={styles.sessionToolbarTitle}>
                        <strong>{t("sessionsTitle")}</strong>
                        <span>{visibleSessions.length}</span>
                      </div>
                      <div className={styles.sessionToolbarActions}>
                        <Tooltip content={t("collapseSessionProjects")}>
                          <button
                            aria-label={t("collapseSessionProjects")}
                            onClick={() =>
                              setExpandedSessionProjects(new Set())
                            }
                            type="button"
                          >
                            <ChevronsDownUp aria-hidden="true" size={16} />
                          </button>
                        </Tooltip>
                        {selected === "codex" && <Tooltip
                          content={sessionSelectionMode ? t("exitSessionBatch") : t("manageSessionBatch")}
                        >
                          <button
                            aria-pressed={sessionSelectionMode}
                            className={
                              sessionSelectionMode
                                ? styles.sessionToolbarActive
                                : undefined
                            }
                            onClick={() =>
                              setSessionSelectionMode((active) => !active)
                            }
                            type="button"
                          >
                            <CheckSquare aria-hidden="true" size={16} />
                          </button>
                        </Tooltip>}
                        <Tooltip content={t("searchSessions")}>
                          <button
                            onClick={() => setSessionSearchOpen(true)}
                            type="button"
                          >
                            <Search aria-hidden="true" size={16} />
                          </button>
                        </Tooltip>
                      </div>
                    </>
                  )}
                </header>
                {selected === "codex" && sessionSelectionMode && (
                  <div className={styles.sessionBatchBar}>
                    <span>
                      {t("sessionsSelected", {
                        count: selectedSessionIds.size,
                      })}
                    </span>
                    <button onClick={toggleAllVisible} type="button">
                      {visibleSessions.every((session) =>
                        selectedSessionIds.has(session.id),
                      )
                        ? t("sessionsClearAll")
                        : t("sessionsSelectAll")}
                    </button>
                    <button
                      onClick={() => setSelectedSessionIds(new Set())}
                      type="button"
                    >
                      {t("sessionsClearSelection")}
                    </button>
                    <button
                      className={styles.sessionBatchDelete}
                      disabled={!selectedSessionIds.size || sessionsDeleting}
                      onClick={() =>
                        setSessionDeleteTargets([...selectedSessionIds])
                      }
                      type="button"
                    >
                      <Trash2 aria-hidden="true" size={14} />
                      {sessionsDeleting
                        ? t("sessionsDeleting")
                        : t("sessionsDeleteSelected")}
                    </button>
                  </div>
                )}
                <div className={styles.sessionProjects}>
                  {[...projects].map(([project, sessions]) => {
                    const expanded = expandedSessionProjects.has(project);
                    const allProjectSessionsSelected = sessions.every((session) =>
                      selectedSessionIds.has(session.id),
                    );
                    const label =
                      project === "__unknown__"
                        ? t("sessionsUnknownProject")
                        : (project.split("/").filter(Boolean).at(-1) ??
                          project);
                    return (
                      <section className={styles.sessionProject} key={project}>
                        <div className={styles.sessionProjectHeader}>
                          {selected === "codex" && sessionSelectionMode && (
                            <input
                              aria-label={t("selectSessionProject", { project: label })}
                              checked={allProjectSessionsSelected}
                              onChange={(event) =>
                                setSelectedSessionIds((current) => {
                                  const next = new Set(current);
                                  sessions.forEach((session) => {
                                    if (event.target.checked) next.add(session.id);
                                    else next.delete(session.id);
                                  });
                                  return next;
                                })
                              }
                              type="checkbox"
                            />
                          )}
                          <button
                            aria-expanded={expanded}
                            aria-label={t("toggleSessionProject", {
                              project: label,
                            })}
                            className={styles.sessionProjectTrigger}
                            onClick={() =>
                              setExpandedSessionProjects((current) => {
                                const next = new Set(current);
                                if (next.has(project)) next.delete(project);
                                else next.add(project);
                                return next;
                              })
                            }
                            type="button"
                          >
                            {expanded ? (
                              <ChevronDown aria-hidden="true" size={15} />
                            ) : (
                              <ChevronRight aria-hidden="true" size={15} />
                            )}
                            <FolderOpen aria-hidden="true" size={16} />
                            <span>{label}</span>
                            <small className={styles.sessionProjectCount}>
                              {sessions.length}
                            </small>
                          </button>
                        </div>
                        {expanded && (
                          <div className={styles.sessionList}>
                            {sessions.map((session) => (
                              <div
                                className={`${styles.sessionCard} ${session.id === selectedCodexSessionId ? styles.sessionCardActive : ""}`}
                                key={session.id}
                              >
                                {selected === "codex" && sessionSelectionMode && (
                                  <input
                                    aria-label={t("selectSession", {
                                      session: session.title,
                                    })}
                                    checked={selectedSessionIds.has(session.id)}
                                    onChange={(event) =>
                                      setSelectedSessionIds((current) => {
                                        const next = new Set(current);
                                        if (event.target.checked)
                                          next.add(session.id);
                                        else next.delete(session.id);
                                        return next;
                                      })
                                    }
                                    type="checkbox"
                                  />
                                )}
                                <button
                                  aria-current={
                                    session.id === selectedCodexSessionId
                                      ? "page"
                                      : undefined
                                  }
                                  onClick={() =>
                                    setSelectedCodexSessionId(session.id)
                                  }
                                  type="button"
                                >
                                  <strong>{session.title}</strong>
                                  <span>
                                    {formatRelativeSessionTime(
                                      session.updatedAt,
                                      t,
                                    )}
                                  </span>
                                </button>
                              </div>
                            ))}
                          </div>
                        )}
                      </section>
                    );
                  })}
                </div>
              </div>
              <section className={styles.sessionDetail}>
                {!selectedSession ? (
                  <div className={styles.sessionDetailEmpty}>
                    <MessageSquareText aria-hidden="true" size={32} />
                    <p>{t("sessionsSelect")}</p>
                  </div>
                ) : (
                  <>
                    <header className={styles.sessionDetailHeader}>
                      <div className={styles.sessionDetailTop}>
                        <h2>{selectedSession.title}</h2>
                        <div className={styles.sessionDetailActions}>
                          {selected === "codex" && <Tooltip content={t("launchSession")}>
                            <button
                              aria-label={t("launchSession")}
                              onClick={() =>
                                void launchCodexSession(selectedSession.id)
                              }
                              type="button"
                            >
                              <Play aria-hidden="true" size={17} />
                            </button>
                          </Tooltip>}
                          {selected === "codex" && <Tooltip content={t("deleteSession")}>
                            <button
                              aria-label={t("deleteSession")}
                              onClick={() => {
                                if (window.confirm(t("deleteSessionConfirm")))
                                  void deleteCodexSession(
                                    selectedSession.id,
                                  ).then(() => {
                                    setCodexSessions((current) =>
                                      current.filter(
                                        (session) =>
                                          session.id !== selectedSession.id,
                                      ),
                                    );
                                    setSelectedCodexSessionId(undefined);
                                  });
                              }}
                              type="button"
                            >
                              <Trash2 aria-hidden="true" size={17} />
                            </button>
                          </Tooltip>}
                        </div>
                      </div>
                      <div className={styles.sessionDetailMeta}>
                        <Clock3 aria-hidden="true" size={13} />
                        <span>
                          {new Date(selectedSession.updatedAt).toLocaleString()}
                        </span>
                        {selectedSession.projectDir && (
                          <>
                            <FolderOpen aria-hidden="true" size={13} />
                            <span>
                              {selectedSession.projectDir
                                .split("/")
                                .filter(Boolean)
                                .at(-1)}
                            </span>
                          </>
                        )}
                      </div>
                      <dl className={styles.sessionDetailFields}>
                        <div>
                          <dt>{t("sessionsSourcePath")}</dt>
                          <dd><code>{selectedSession.sourcePath}</code><CopyIconButton onCopied={() => setNotice(t("copied"))} onError={showError} text={selectedSession.sourcePath} /></dd>
                        </div>
                        {selected === "codex" && <div>
                          <dt>{t("sessionsResumeCommand")}</dt>
                          <dd><code>{`codex resume ${selectedSession.id}`}</code><CopyIconButton onCopied={() => setNotice(t("copied"))} onError={showError} text={`codex resume ${selectedSession.id}`} /></dd>
                        </div>}
                      </dl>
                    </header>
                    <div className={styles.sessionMessages}>
                      {sessionMessagesLoading ? (
                        <div className={styles.sessionDetailEmpty}>
                          <RefreshCw
                            aria-hidden="true"
                            className={styles.spinning}
                            size={24}
                          />
                          <p>{t("sessionsMessagesLoading")}</p>
                        </div>
                      ) : codexSessionMessages.length ? (
                        codexSessionMessages.map((message, index) => (
                          <article
                            className={`${styles.sessionMessage} ${message.role === "user" ? styles.sessionMessageUser : styles.sessionMessageAssistant}`}
                            key={`${message.timestamp ?? index}-${index}`}
                          >
                            <header>
                              <strong>
                                {t(
                                  message.role === "user"
                                    ? "sessionsRoleUser"
                                    : "sessionsRoleAssistant",
                                )}
                              </strong>
                              {message.timestamp && (
                                <time>
                                  {new Date(message.timestamp).toLocaleString()}
                                </time>
                              )}
                            </header>
                            <p>{message.content}</p>
                          </article>
                        ))
                      ) : (
                        <div className={styles.sessionDetailEmpty}>
                          <p>{t("sessionsMessagesEmpty")}</p>
                        </div>
                      )}
                    </div>
                  </>
                )}
              </section>
            </div>
            <AlertDialog.Root
              onOpenChange={(open) => {
                if (!open && !sessionsDeleting)
                  setSessionDeleteTargets(undefined);
              }}
              open={Boolean(sessionDeleteTargets)}
            >
              <AlertDialog.Portal>
                <AlertDialog.Overlay className={styles.dialogOverlay} />
                <AlertDialog.Content className={styles.dialogContent}>
                  <AlertDialog.Title>
                    {t("sessionsBatchDeleteTitle")}
                  </AlertDialog.Title>
                  <AlertDialog.Description>
                    {t("sessionsBatchDeleteConfirm", {
                      count: sessionDeleteTargets?.length ?? 0,
                    })}
                  </AlertDialog.Description>
                  <div className={styles.dialogActions}>
                    <AlertDialog.Cancel asChild>
                      <button
                        className={styles.dialogCancel}
                        disabled={sessionsDeleting}
                        type="button"
                      >
                        {t("cancel")}
                      </button>
                    </AlertDialog.Cancel>
                    <AlertDialog.Action asChild>
                      <button
                        autoFocus
                        className={styles.dialogConfirm}
                        disabled={sessionsDeleting}
                        onClick={(event) => {
                          event.preventDefault();
                          void deleteSelectedSessions();
                        }}
                        type="button"
                      >
                        {sessionsDeleting
                          ? t("sessionsDeleting")
                          : t("sessionsDeleteSelected")}
                      </button>
                    </AlertDialog.Action>
                  </div>
                </AlertDialog.Content>
              </AlertDialog.Portal>
            </AlertDialog.Root>
          </>
        );
      }
      return (
        <div className={styles.placeholder}>
          <h2>
            {applicationLabel} · {t(`${section.labelKey}Title`)}
          </h2>
          <p>{t(`${section.labelKey}Description`)} </p>
        </div>
      );
    }
    return accounts.length === 0 ? (
      <div className={styles.empty}>
        <KeyRound aria-hidden="true" size={32} />
        <h2>{t("emptyTitle")}</h2>
        <p>{t("emptyDescription", { application: t(selected) })}</p>
      </div>
    ) : (
      <AccountList accounts={accounts} busy={busy} kind={selected} key={selected} onDuplicate={(account) => void duplicateAccount(account)} onExport={(account) => void openAccountExport(account)} onLaunchBot={(account) => void launchBot(account)} onRemove={remove} onReorder={(activeId, targetId) => void reorder(activeId, targetId)} onSwitch={switchTo} onTest={(account) => void testApiKey(account)} progress={switchProgress} testingId={testingId} />
    );
  };

  return (
    <Toast.Provider>
      <main className={styles.shell}>
        <WindowDragSurface />
        <header className={styles.header}>
          <div className={styles.brand}>
            {!navigator.userAgent.includes("Windows") && <img alt="" src={logo} />}
            <span>{t("appName")}</span>
            <Tooltip content={t("settings")}>
              <a
                aria-label={t("settings")}
                className={styles.settingsButton}
                href={`/settings.html?kind=${selected}`}
              >
                <Settings aria-hidden="true" size={16} />
              </a>
            </Tooltip>
          </div>
          <div className={styles.headerModes}>
          <Tabs.Root
            className={styles.switcher}
            onValueChange={(value) =>
              selectApplication(value as ApplicationKind)
            }
            value={selected}
          >
            <Tabs.List aria-label={t("applications")}>
              {APPLICATION_KINDS.map((kind) => (
                <Tabs.Trigger className={styles.appTab} key={kind} value={kind}>
                  <img alt="" className="ink" src={APP_ICONS[kind]} />
                  {kind === "codex"
                    ? t("codex")
                    : (applications.find((app) => app.kind === kind)?.label ??
                      t(kind))}
                </Tabs.Trigger>
              ))}
            </Tabs.List>
          </Tabs.Root>
          </div>
          <WorkspaceToolbar
            busy={busy}
            canManageAccounts
            hasAccounts={accounts.some((account) => account.importType !== "api_key")}
            kind={selected}
            onExport={() => void exportAccounts()}
            onPluginsExpandedChange={setPluginsExpanded}
            onRefresh={() => void refresh()}
            onSessionsRefresh={() => setSessionRefreshKey((current) => current + 1)}
            pluginsExpanded={pluginsExpanded}
            refreshing={refreshing}
            sessionsRefreshing={sessionsRefreshing}
            section={workspaceSection}
          />
        </header>
        <section className={styles.workspace}>
          <aside aria-label={t("accountSections")} className={styles.sidebar}>
            <nav className={styles.sidebarNav}>
              {workspaceSections.map(({ id, icon: Icon, labelKey }) => {
                const count =
                  id === "accounts"
                    ? accounts.length
                    : id === "plugins"
                      ? pluginCount
                      : undefined;
                const hasCount = count !== undefined;
                const showBadge =
                  hasCount && count > 0 && workspaceSection === id;
                const label = hasCount
                  ? t(
                      id === "accounts"
                        ? "accountsWithCount"
                        : "pluginsWithCount",
                      { count },
                    )
                  : t(labelKey);
                return (
                  <Tooltip content={label} key={id}>
                    <button
                      aria-label={label}
                      aria-current={
                        workspaceSection === id ? "page" : undefined
                      }
                      className={`${styles.sidebarItem} ${workspaceSection === id ? styles.sidebarItemActive : ""}`}
                      onClick={() => {
                        if (id === workspaceSection) return;
                        setWorkspaceSection(id);
                      }}
                      type="button"
                    >
                      <Icon aria-hidden="true" size={18} />
                      {showBadge && (
                        <span
                          aria-hidden="true"
                          className={styles.sidebarBadge}
                        >
                          {count}
                        </span>
                      )}
                    </button>
                  </Tooltip>
                );
              })}
            </nav>
          </aside>
          <div className={styles.content}>{renderSection()}</div>
        </section>
      </main>
      <AlertDialog.Root
        onOpenChange={(open) => {
          if (!open && restartDialog) cancelRestart();
        }}
        open={Boolean(restartDialog)}
      >
        <AlertDialog.Portal>
          <AlertDialog.Overlay className={styles.dialogOverlay} />
          <AlertDialog.Content className={styles.dialogContent}>
            <AlertDialog.Title>{t("restartDialogTitle")}</AlertDialog.Title>
            <AlertDialog.Description>
              {t("restartDialogDescription")}
            </AlertDialog.Description>
            <p className={styles.dialogWarning}>{t("restartDialogWarning")}</p>
            <div className={styles.dialogActions}>
              <AlertDialog.Cancel asChild>
                <button
                  className={styles.dialogCancel}
                  onClick={cancelRestart}
                  type="button"
                >
                  {t("cancelCountdown", { seconds: countdown })}
                </button>
              </AlertDialog.Cancel>
              <button
                autoFocus
                className={styles.dialogConfirm}
                onClick={() => void forceRestart()}
                type="button"
              >
                {t("forceRestart")}
              </button>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
      <AlertDialog.Root
        onOpenChange={(open) => {
          if (!open && grokBotDialog) cancelLaunchBot();
        }}
        open={Boolean(grokBotDialog)}
      >
        <AlertDialog.Portal>
          <AlertDialog.Overlay className={styles.dialogOverlay} />
          <AlertDialog.Content className={styles.dialogContent}>
            <AlertDialog.Title>{t("grokBotSwitchTitle")}</AlertDialog.Title>
            <AlertDialog.Description>
              {t("grokBotSwitchDescription", { account: grokBotDialog?.label ?? "" })}
            </AlertDialog.Description>
            <p className={styles.dialogWarning}>{t("grokBotSwitchWarning")}</p>
            <div className={styles.dialogActions}>
              <AlertDialog.Cancel asChild>
                <button className={styles.dialogCancel} onClick={cancelLaunchBot} type="button">{t("cancelCountdown", { seconds: countdown })}</button>
              </AlertDialog.Cancel>
              <button autoFocus className={styles.dialogConfirm} onClick={() => void confirmLaunchBot()} type="button">
                {t("grokBotSwitchConfirm")}
              </button>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
      {exportTarget && exportData !== undefined && (
        <ExportDialog
          data={[exportData]}
          filename={`${selected}-account-${exportTarget.id}.json`}
          onOpenChange={(open) => {
            if (!open) setExportTarget(undefined);
          }}
          open
        />
      )}
      <ToastMessage
        notice={notice}
        onOpenChange={(open) => {
          if (!open) setNotice(undefined);
        }}
        status={refreshing ? "loading" : refreshFailed ? "error" : "success"}
      />
      <Toast.Viewport className={styles.toastViewport} />
    </Toast.Provider>
  );
}
