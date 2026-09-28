import { listen } from "@tauri-apps/api/event";
import { arrayMove } from "@dnd-kit/sortable";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Tabs from "@radix-ui/react-tabs";
import {
  CheckSquare,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Clock3,
  Download,
  FolderOpen,
  KeyRound,
  MessageSquareText,
  Play,
  Plus,
  Puzzle,
  RefreshCw,
  Search,
  Settings,
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
import { StartupUpdateDialog } from "../../components/StartupUpdateDialog";
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
import grokBotIcon from "../../assets/tools/grok-bot.png";
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
  getCursorUsage,
  getGrokBotStatus,
  getGrokSessionMessages,
  launchChatgpt,
  launchCodexSession,
  launchCursor,
  launchGrokBot,
  launchGrokSession,
  listAccounts,
  getGrokBotExportRecord,
  listGrokBotAccounts,
  refreshGrokBotAccounts,
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
import { requestedHomeTab, resolveHomeView, visibleHomeTabs, type HomeTabId } from "../../lib/homeTabs";
import {
  canSwitchToDesktop,
  homePath,
  syncDocumentAppKind,
  type Account,
  type ApplicationKind,
  type ApplicationStatus,
  type CodexSession,
  type GrokBotStatus,
  type CodexSessionMessage,
  type McpServer,
} from "../../lib/types";
import "../../styles/global.css";
import styles from "./page.module.css";
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
import { GrokBotAccountList } from "./components/GrokBotAccountList";
import { GrokBotStatusCard } from "./components/GrokBotStatusCard";
import { SessionWorkspace, type SessionProvider } from "./components/SessionWorkspace";
import { GrokBotSessionWorkspace } from "./components/GrokBotSessionWorkspace";
import { shouldApplySwitchProgress } from "./lib/switchProgress";
import type { WorkspaceSection, SwitchProgress } from "./types";

type SwitchOutcome = { restartRequired: boolean };
const APP_ICONS: Record<HomeTabId, string> = {
  cursor: cursorIcon,
  codex: codexIcon,
  grok: grokIcon,
  grokBot: grokBotIcon,
};

const workspaceSections: Array<{
  id: WorkspaceSection;
  icon?: ComponentType<{
    "aria-hidden"?: boolean | "true" | "false";
    size?: number;
  }>;
  image?: string;
  labelKey: string;
  cursorOnly?: boolean;
}> = [
  { id: "accounts", icon: UserRound, labelKey: "accounts" },
  { id: "sessions", icon: MessageSquareText, labelKey: "sessions" },
  { id: "plugins", icon: Puzzle, labelKey: "plugins" },
  { id: "mcp", icon: Waypoints, labelKey: "mcp" },
];


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
  const [selected, setSelected] = useState<ApplicationKind>(() => resolveHomeView().selected);
  const [grokBotMode, setGrokBotMode] = useState(() => resolveHomeView().grokBotMode);
  const homeTabs = visibleHomeTabs();
  const [workspaceSection, setWorkspaceSection] =
    useState<WorkspaceSection>("accounts");
  const [pluginsExpanded, setPluginsExpanded] = useState(false);
  const [pluginCount, setPluginCount] = useState<number | undefined>(() =>
    cachedPluginCount(resolveHomeView().selected),
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
  const [grokBotStatus, setGrokBotStatus] = useState<GrokBotStatus>();
  const [grokBotStatusLoading, setGrokBotStatusLoading] = useState(false);
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
  const [exportKind, setExportKind] = useState<"account" | "grokBot">("account");
  const [testingId, setTestingId] = useState<string>();
  const [countdown, setCountdown] = useState(10);
  const activeOperationId = useRef<string | undefined>(undefined);
  const beginAccountsRequest = useLatestRequest();
  const knownSessionProjects = useRef(new Set<string>());
  const beginSessionMessageRequest = useLatestRequest();
  const isCursor = selected === "cursor";
  useLayoutEffect(() => {
    syncDocumentAppKind(grokBotMode ? "grokBot" : selected);
  }, [grokBotMode, selected]);
  useLayoutEffect(() => {
    const view = resolveHomeView();
    if (requestedHomeTab() === view.tab) return;
    window.history.replaceState({}, "", homePath(view.tab));
  }, []);
  const showError = useCallback(
    (error: unknown) =>
      setNotice(error instanceof Error ? error.message : String(error)),
    [],
  );
  const loadAccounts = useCallback(async () => {
    const isCurrent = beginAccountsRequest();
    const [nextApplications, nextAccounts] = await Promise.all([
      listApplications(),
      grokBotMode ? listGrokBotAccounts() : listAccounts(selected),
    ]);
    if (!isCurrent()) return;
    setApplications(nextApplications);
    setAccounts(nextAccounts);
  }, [beginAccountsRequest, grokBotMode, selected]);
  const selectApplication = (next: ApplicationKind) => {
    // Grok Bot keeps selected="cursor". Leaving it must not clear accounts or
    // skip reload — selected does not change, so loadAccounts would not re-run.
    if (next === selected) {
      setGrokBotMode(false);
      if (workspaceSection === "grokBot") setWorkspaceSection("accounts");
      window.history.replaceState({}, "", homePath(next));
      return;
    }
    setGrokBotMode(false);
    setMcpServers([]);
    setMcpPending(new Set());
    setAccounts([]);
    setSelected(next);
    if (workspaceSection === "grokBot") setWorkspaceSection("accounts");
    window.history.replaceState({}, "", homePath(next));
  };
  const openGrokBotMode = () => {
    setGrokBotMode(true);
    setSelected("cursor");
    setWorkspaceSection("accounts");
    window.history.replaceState({}, "", homePath("grokBot"));
  };
  useEffect(() => {
    void loadAccounts().catch(showError);
  }, [loadAccounts, showError]);
  useEffect(() => {
    if (workspaceSection === "mcp")
      void listMcpServers(selected).then(setMcpServers).catch(showError);
  }, [selected, workspaceSection]);
  useEffect(() => {
    if (grokBotMode) {
      if (workspaceSection !== "accounts" && workspaceSection !== "sessions")
        setWorkspaceSection("accounts");
      return;
    }
    if (selected !== "cursor" && workspaceSection === "grokBot")
      setWorkspaceSection("accounts");
  }, [grokBotMode, selected, workspaceSection]);
  const loadGrokBotStatus = useCallback(async () => {
    if (selected !== "cursor") {
      setGrokBotStatus(undefined);
      return;
    }
    setGrokBotStatusLoading(true);
    try {
      setGrokBotStatus(await getGrokBotStatus());
    } catch (error) {
      showError(error);
    } finally {
      setGrokBotStatusLoading(false);
    }
  }, [selected, showError]);
  useEffect(() => {
    void loadGrokBotStatus();
  }, [loadGrokBotStatus, sessionRefreshKey]);
  const refreshGrokBotPage = useCallback(async () => {
    setGrokBotStatusLoading(true);
    try {
      const currentId =
        grokBotStatus?.currentAccountId ??
        accounts.find((account) => account.isGrokBotCurrent)?.id;
      if (currentId) {
        setNotice(t("refreshGrokBotUsage"));
        await getCursorUsage(currentId);
        await loadAccounts();
        setNotice(t("refreshGrokBotDone"));
      } else {
        setNotice(t("refreshGrokBotNoAccount"));
      }
      setSessionRefreshKey((current) => current + 1);
    } catch (error) {
      showError(error);
    } finally {
      setGrokBotStatusLoading(false);
    }
  }, [accounts, grokBotStatus?.currentAccountId, loadAccounts, showError, t]);
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
      if (
        payload.operationId === activeOperationId.current &&
        shouldApplySwitchProgress(payload)
      )
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
    setNotice(t(noticeKey, { application: t(selected) }));
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
      await finishSwitch(isCursor ? "cursorLaunched" : selected === "codex" ? "chatgptLaunched" : "accountSwitched");
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
  const canHoverLaunchTab = (kind: HomeTabId) => kind === "cursor" || kind === "codex" || kind === "grokBot";
  const launchTabApp = async (kind: HomeTabId) => {
    if (busy || !canHoverLaunchTab(kind)) return;
    setBusy(true);
    try {
      if (kind === "codex") await launchChatgpt();
      else if (kind === "grokBot") await launchGrokBot();
      else await launchCursor();
      setNotice(t("appLaunched"));
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  };
  const launchCurrentApp = async () => {
    if (selected === "cursor" || selected === "codex") await launchTabApp(selected);
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
      await loadAccounts();
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
      await loadAccounts();
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
      await invoke("force_restart", {
        id: dialog.account.id,
        operationId: dialog.operationId,
      });
      await finishSwitch("appRestarted");
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
      if (!selected) {
        await loadAccounts();
        setNotice(t("refreshed"));
        return;
      }
      const result = await invoke<{
        total: number;
        failed: number;
        invalid: number;
        missing: number;
        blocked?: number;
      }>(
        selected === "cursor"
          ? "refresh_all_cursor_accounts"
          : selected === "grok"
            ? "refresh_all_grok_accounts"
            : "refresh_all_codex_accounts",
      );
      const failed = result.failed;
      await loadAccounts();
      const other = failed - result.invalid - result.missing;
      const reasons = [
        result.blocked && t("refreshAccountBlocked", { count: result.blocked }),
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
  const refreshGrokBotAccountQuotas = async () => {
    setBusy(true);
    setRefreshing(true);
    setRefreshFailed(false);
    setNotice(t("refreshing"));
    try {
      const result = await refreshGrokBotAccounts();
      const failed = result.failed;
      await loadAccounts();
      const other = failed - result.invalid - result.missing;
      const reasons = [
        result.blocked && t("refreshAccountBlocked", { count: result.blocked }),
        result.invalid && t("refreshTokenInvalid", { count: result.invalid }),
        result.missing && t("refreshCredentialMissing", { count: result.missing }),
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
      setExportKind("account");
    } catch (error) {
      showError(error);
    }
  };
  const openGrokBotExport = async (account: Account) => {
    try {
      setExportData(await getGrokBotExportRecord(account.id));
      setExportTarget(account);
      setExportKind("grokBot");
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
      if (workspaceSection === "sessions") {
        if (grokBotMode) {
          return (
            <GrokBotSessionWorkspace
              key="grokBot-sessions"
              onError={showError}
              onNotice={setNotice}
              onRefreshingChange={setSessionsRefreshing}
              refreshKey={sessionRefreshKey}
            />
          );
        }
        return <SessionWorkspace key={sessionProvider.id} onError={showError} onNotice={setNotice} onRefreshingChange={setSessionsRefreshing} provider={sessionProvider} refreshKey={sessionRefreshKey} />;
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
      grokBotMode ? (
        <>
          <GrokBotStatusCard
            accounts={accounts}
            busy={busy}
            loading={grokBotStatusLoading}
            onError={showError}
            onRefresh={() => void refreshGrokBotPage()}
            onSwitchAccount={(account) => void launchBot(account)}
            refreshing={grokBotStatusLoading || sessionsRefreshing}
            status={grokBotStatus}
          />
          <GrokBotAccountList
            accounts={accounts}
            busy={busy}
            key="grokBot-accounts"
            onExport={(account) => void openGrokBotExport(account)}
            onLaunchBot={(account) => void launchBot(account)}
            onRemove={remove}
            onReorder={(activeId, targetId) => void reorder(activeId, targetId)}
          />
        </>
      ) : (
        <AccountList
          accounts={accounts}
          busy={busy}
          kind={selected}
          key={selected}
          onDuplicate={(account) => void duplicateAccount(account)}
          onExport={(account) => void openAccountExport(account)}
          onLaunchBot={(account) => void launchBot(account)}
          onLaunchCurrent={() => void launchCurrentApp()}
          onRemove={remove}
          onReorder={(activeId, targetId) => void reorder(activeId, targetId)}
          onSwitch={switchTo}
          onTest={(account) => void testApiKey(account)}
          progress={switchProgress}
          testingId={testingId}
        />
      )
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
                href={`/settings.html?kind=${grokBotMode ? "grokBot" : selected}`}
              >
                <Settings aria-hidden="true" size={16} />
              </a>
            </Tooltip>
          </div>
          <div className={styles.headerModes}>
          <Tabs.Root
            className={styles.switcher}
            onValueChange={(value) => {
              if (value === "grokBot") openGrokBotMode();
              else selectApplication(value as ApplicationKind);
            }}
            value={grokBotMode ? "grokBot" : selected}
          >
            <Tabs.List aria-label={t("applications")}>
              {homeTabs.map((kind) => {
                const label =
                  kind === "grokBot"
                    ? t("grokBot")
                    : kind === "codex"
                      ? t("codex")
                      : (applications.find((app) => app.kind === kind)?.label ??
                        t(kind));
                const hoverLaunch = canHoverLaunchTab(kind);
                const isActiveTab = (grokBotMode ? "grokBot" : selected) === kind;
                return (
                  <Tabs.Trigger
                    className={`${styles.appTab}${hoverLaunch ? ` ${styles.appTabLaunchable}` : ""}`}
                    key={kind}
                    value={kind}
                  >
                    {hoverLaunch ? (
                      <span className={styles.appTabIcon}>
                        <img alt="" className={`${styles.appTabIconGlyph} ${styles.appTabIdle} ink`} src={APP_ICONS[kind]} />
                        <span
                          aria-label={t("launchApp")}
                          className={styles.appTabLaunchHit}
                          onClick={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                            if (!isActiveTab || busy) return;
                            void launchTabApp(kind);
                          }}
                          onKeyDown={(event) => {
                            if (event.key !== "Enter" && event.key !== " ") return;
                            event.preventDefault();
                            event.stopPropagation();
                            if (!isActiveTab || busy) return;
                            void launchTabApp(kind);
                          }}
                          onPointerDown={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                          }}
                          role="button"
                          tabIndex={isActiveTab ? 0 : -1}
                          title={t("launchApp")}
                        >
                          <Play aria-hidden="true" className={styles.appTabIconGlyph} size={15} strokeWidth={2.4} />
                        </span>
                      </span>
                    ) : (
                      <img alt="" className="ink" src={APP_ICONS[kind]} />
                    )}
                    {label}
                  </Tabs.Trigger>
                );
              })}
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
            onRefresh={() => {
              if (grokBotMode) void refreshGrokBotAccountQuotas();
              else void refresh();
            }}
            onSessionsRefresh={() => {
              if (grokBotMode) {
                setSessionRefreshKey((current) => current + 1);
                void loadGrokBotStatus();
              } else {
                setSessionRefreshKey((current) => current + 1);
              }
            }}
            pluginsExpanded={pluginsExpanded}
            refreshing={refreshing}
            sessionsRefreshing={sessionsRefreshing}
            section={workspaceSection}
            sessionsRefreshLabel={grokBotMode ? t("refreshSessions") : undefined}
            showAddAccount={!grokBotMode}
          />
        </header>
        <section className={styles.workspace}>
          <aside aria-label={t("accountSections")} className={styles.sidebar}>
            <nav className={styles.sidebarNav}>
              {workspaceSections.filter((section) => {
                if (grokBotMode) return section.id === "accounts" || section.id === "sessions";
                return !section.cursorOnly || isCursor;
              }).map(({ id, icon: Icon, image, labelKey }) => {
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
                      {image ? <img alt="" className={id === "grokBot" ? styles.sidebarGrokBotIcon : styles.sidebarAppIcon} src={image} /> : Icon && <Icon aria-hidden="true" size={18} />}
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
            <AlertDialog.Title>{t("restartDialogTitle", { application: t(selected) })}</AlertDialog.Title>
            <AlertDialog.Description>
              {t("restartDialogDescription", { application: t(selected) })}
            </AlertDialog.Description>
            <p className={styles.dialogWarning}>{t("restartDialogWarning", { application: t(selected) })}</p>
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
          description={exportKind === "grokBot" ? t("exportGrokBotDescription") : undefined}
          filename={
            exportKind === "grokBot"
              ? `grok-bot-account-${exportTarget.id}.json`
              : `${selected}-account-${exportTarget.id}.json`
          }
          onOpenChange={(open) => {
            if (!open) {
              setExportTarget(undefined);
              setExportKind("account");
            }
          }}
          open
          title={exportKind === "grokBot" ? t("exportGrokBot") : undefined}
        />
      )}
      <StartupUpdateDialog />
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
