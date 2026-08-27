import { listen } from "@tauri-apps/api/event";
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Progress from "@radix-ui/react-progress";
import * as Tabs from "@radix-ui/react-tabs";
import { Check, ChartNoAxesCombined, Download, FileOutput, GripVertical, KeyRound, LogIn, Plus, Puzzle, RefreshCw, Settings, Trash2, UserRound, Waypoints, Zap } from "lucide-react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { type ComponentType, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { ExportDialog } from "../../components/ExportDialog";
import { Tooltip } from "../../components/Tooltip";
import logo from "../../assets/logo.svg";
import codexIcon from "../../assets/codex.svg";
import cursorIcon from "../../assets/cursor.svg";
import "../../i18n";
import { deleteCursorPlugin, listAccounts, listApplications, listCodexPlugins, listCursorPlugins, listMcpServers, setCodexPluginCapabilityEnabled, setCodexPluginEnabled, setCursorPluginEnabled } from "../../lib/api";
import { canSwitchToDesktop, type Account, type ApplicationKind, type ApplicationStatus, type CursorPlugin, type McpServer, type PluginCapability } from "../../lib/types";
import "../../styles/global.css";
import styles from "./page.module.css";

type SwitchProgress = {
  operationId: string;
  accountId: string;
  stage: string;
  percent: number;
  status: "running" | "waiting" | "success" | "error";
};

type SwitchOutcome = { restartRequired: boolean };
type WorkspaceSection = "accounts" | "plugins" | "mcp";
type PluginCache = { application: ApplicationKind; accountId?: string; plugins: CursorPlugin[] };
const workspaceSections: Array<{ id: WorkspaceSection; icon: ComponentType<{ "aria-hidden"?: boolean | "true" | "false"; size?: number }>; labelKey: string }> = [
  { id: "accounts", icon: UserRound, labelKey: "accounts" }, { id: "plugins", icon: Puzzle, labelKey: "plugins" }, { id: "mcp", icon: Waypoints, labelKey: "mcp" },
];

const PLUGIN_CACHE_KEY = "cursor-plugin-catalog-v8";

function readPluginCache(application: ApplicationKind, accountId?: string) {
  try {
    const cached = JSON.parse(localStorage.getItem(PLUGIN_CACHE_KEY) ?? "null") as PluginCache | null;
    return cached && cached.application === application && cached.accountId === accountId && Array.isArray(cached.plugins) ? cached.plugins : undefined;
  } catch { return undefined; }
}

function writePluginCache(application: ApplicationKind, accountId: string | undefined, plugins: CursorPlugin[]) {
  localStorage.setItem(PLUGIN_CACHE_KEY, JSON.stringify({ application, accountId, plugins } satisfies PluginCache));
}

function PluginIcon({ icon }: Pick<CursorPlugin, "icon">) {
  const [failed, setFailed] = useState(false);
  const src = icon ? pluginIconSource(icon) : undefined;
  return <span className={styles.pluginIcon}>{src && !failed ? <img alt="" decoding="async" loading="lazy" onError={() => setFailed(true)} src={src} /> : <Puzzle aria-hidden="true" size={20} />}</span>;
}

function pluginIconSource(value: string) {
  try {
    const url = new URL(value);
    if (url.protocol === "https:") return value;
  } catch { /* A local icon path is handled below. */ }
  return /^(?:\/|[A-Za-z]:[\\/])/.test(value) ? convertFileSrc(value) : undefined;
}

function subscriptionLabel(account: Account, t: (key: string, options?: Record<string, unknown>) => string) {
  const plan = account.subscription.plan;
  if (!plan) return undefined;
  const name = t(`subscriptionPlans.${plan.toLowerCase()}`, { defaultValue: plan });
  if (!account.subscription.expiresAt) return { name, expiry: t("subscriptionUnknownExpiry"), plan: plan.toLowerCase() };
  const days = account.daysRemaining;
  if (days === undefined) return { name, expiry: t("subscriptionUnknownExpiry"), plan: plan.toLowerCase() };
  if (days > 0) return { name, expiry: t("subscriptionDays", { count: days }), plan: plan.toLowerCase() };
  if (days === 0) return { name, expiry: t("subscriptionToday"), plan: plan.toLowerCase() };
  return { name, expiry: t("subscriptionExpired"), plan: plan.toLowerCase() };
}

function usageLabel(account: Account, t: (key: string, options?: Record<string, unknown>) => string) {
  const usage = account.usage;
  if (!usage) return account.subscription.plan?.toLowerCase() === "free" ? t("usageFree") : undefined;
  if (usage.kind === "currency") {
    return t("usageSpent", { amount: `$${(usage.used / 100).toFixed(2)}` });
  }
  return account.subscription.plan?.toLowerCase() === "free" ? t("usageFree") : undefined;
}

function SortableAccount({ account, busy, onExport, onRemove, onSwitch, progress }: { account: Account; busy: boolean; onExport: (account: Account) => void; onRemove: (account: Account) => void; onSwitch: (account: Account) => void; progress?: SwitchProgress }) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ disabled: busy, id: account.id });
  return <article className={`${styles.accountCard} ${account.isCurrent ? styles.current : ""} ${isDragging ? styles.dragging : ""}`} ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }}>
    <GripVertical className={styles.dragHandle} aria-label={t("drag", { account: account.label })} size={24} {...attributes} {...listeners} />
    <div className={styles.accountCopy}><strong>{account.label}</strong><div className={styles.accountMeta}>{(() => { const subscription = subscriptionLabel(account, t); return subscription && <span className={`${styles.metaBadge} ${styles[`plan-${subscription.plan}`] ?? styles.planDefault}`}>{subscription.name} · {subscription.expiry}</span>; })()}{(() => { const usage = usageLabel(account, t); return usage && <span className={styles.metaBadge}>{usage}</span>; })()}{account.status === "invalid" && <span className={styles.invalidBadge}>{t("tokenInvalid", { defaultValue: "Token已失效" })}</span>}{account.status === "missing" && <span className={styles.missingBadge}>{t("credentialMissing", { defaultValue: "凭证缺失" })}</span>}</div></div>
    <div className={styles.accountActions}>
      {progress ? <div className={styles.progress}><span>{t(`switchStages.${progress.stage}`)}</span><Progress.Root aria-label={t("switchProgress")} className={styles.progressRoot} value={progress.percent}><Progress.Indicator className={progress.status === "error" ? styles.progressError : styles.progressIndicator} style={{ transform: `translateX(-${100 - progress.percent}%)` }} /></Progress.Root></div> : account.isCurrent ? <span className={styles.currentBadge}><Check aria-hidden="true" size={16} />{t("current")}</span> : canSwitchToDesktop(account) ? <button className={styles.activate} disabled={busy} onClick={() => onSwitch(account)} type="button"><LogIn aria-hidden="true" size={17} />{t("switch")}</button> : null}
      {progress?.status === "error" && canSwitchToDesktop(account) && <button className={styles.activate} onClick={() => onSwitch(account)} type="button"><RefreshCw aria-hidden="true" size={16} />{t("retry")}</button>}
      <Tooltip content={t("usage")}><a aria-label={t("viewUsage", { account: account.label })} className={styles.iconButton} href={`/usage.html?accountId=${encodeURIComponent(account.id)}`}><ChartNoAxesCombined aria-hidden="true" size={18} /></a></Tooltip>
      <Tooltip content={t("export")}><button aria-label={t("exportAccount", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onExport(account)} type="button"><FileOutput aria-hidden="true" size={18} /></button></Tooltip>
      <Tooltip content={t("delete")}><button aria-label={t("remove", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onRemove(account)} type="button"><Trash2 aria-hidden="true" size={19} /></button></Tooltip>
    </div>
  </article>;
}

function WorkspaceToolbar({ section, busy, canManageAccounts, hasAccounts, refreshing, onExport, onRefresh }: {
  section: WorkspaceSection;
  busy: boolean;
  canManageAccounts: boolean;
  hasAccounts: boolean;
  refreshing: boolean;
  onExport: () => void;
  onRefresh: () => void;
}) {
  const { t } = useTranslation();
  return <div aria-label={t("sectionActions")} className={styles.contextToolbar} data-section={section}>
    {section === "accounts" && <>
      <Tooltip content={t("export")}><button aria-label={t("export")} className={styles.iconButton} disabled={busy || !canManageAccounts || !hasAccounts} onClick={onExport} type="button"><Download aria-hidden="true" size={19} /></button></Tooltip>
      <Tooltip content={t("refresh")}><button aria-label={t("refresh")} className={styles.iconButton} disabled={busy || !canManageAccounts} onClick={onRefresh} type="button"><RefreshCw aria-hidden="true" className={refreshing ? styles.spinning : undefined} size={19} /></button></Tooltip>
      <a aria-disabled={busy || !canManageAccounts} className={styles.addButton} href={busy || !canManageAccounts ? undefined : "/add.html"}><Plus aria-hidden="true" size={18} />{t("addAccount")}</a>
    </>}
  </div>;
}

function AccountsPage() {
  const { t } = useTranslation();
  const [applications, setApplications] = useState<ApplicationStatus[]>([]);
  const [selected, setSelected] = useState<ApplicationKind>("cursor");
  const [workspaceSection, setWorkspaceSection] = useState<WorkspaceSection>("accounts");
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [plugins, setPlugins] = useState<CursorPlugin[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [pluginsLoading, setPluginsLoading] = useState(false);
  const [pendingPlugins, setPendingPlugins] = useState<Set<string>>(() => new Set());
  const [busy, setBusy] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshFailed, setRefreshFailed] = useState(false);
  const [notice, setNotice] = useState(() => new URLSearchParams(window.location.search).get("notice") ?? undefined);
  const [switchProgress, setSwitchProgress] = useState<SwitchProgress>();
  const [restartDialog, setRestartDialog] = useState<{ account: Account; operationId: string }>();
  const [exportData, setExportData] = useState<unknown>();
  const [exportTarget, setExportTarget] = useState<Account>();
  const [countdown, setCountdown] = useState(10);
  const activeOperationId = useRef<string | undefined>(undefined);
  const latestLoad = useRef(0);
  const pluginsLoaded = useRef(false);
  const pluginsLoadingRef = useRef(false);
  const latestPluginLoad = useRef(0);
  const pluginLoadFrame = useRef<number | undefined>(undefined);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  const isCursor = selected === "cursor";
  const currentCursorAccountId = selected === "cursor" ? accounts.find((account) => account.isCurrent)?.id : undefined;
  const showError = (error: unknown) => setNotice(error instanceof Error ? error.message : String(error));
  const loadAccounts = async () => {
    const request = ++latestLoad.current;
    const [nextApplications, nextAccounts] = await Promise.all([listApplications(), listAccounts(selected)]);
    if (request !== latestLoad.current) return;
    setApplications(nextApplications);
    setAccounts(nextAccounts);
  };
  const loadPlugins = async () => {
    if (pluginsLoaded.current || pluginsLoadingRef.current) return;
    const request = ++latestPluginLoad.current;
    const cached = readPluginCache(selected, currentCursorAccountId);
    if (cached) setPlugins(cached);
    else setPluginsLoading(true);
    pluginsLoadingRef.current = true;
    try {
      const nextPlugins = selected === "cursor" ? await listCursorPlugins() : await listCodexPlugins();
      if (request === latestPluginLoad.current) {
        setPlugins(nextPlugins);
        writePluginCache(selected, currentCursorAccountId, nextPlugins);
        pluginsLoaded.current = true;
      }
    } catch (error) { showError(error); }
    finally {
      if (request === latestPluginLoad.current) {
        pluginsLoadingRef.current = false;
        setPluginsLoading(false);
      }
    }
  };
  const schedulePluginLoad = () => {
    if (pluginLoadFrame.current !== undefined) cancelAnimationFrame(pluginLoadFrame.current);
    pluginLoadFrame.current = requestAnimationFrame(() => {
      pluginLoadFrame.current = requestAnimationFrame(() => {
        pluginLoadFrame.current = undefined;
        void loadPlugins();
      });
    });
  };
  const selectApplication = (next: ApplicationKind) => {
    if (next === selected) return;
    // A product switch is a data-boundary change: invalidate in-flight work
    // before exposing the new workspace, so Cursor data cannot bleed into Codex.
    ++latestPluginLoad.current;
    pluginsLoaded.current = false;
    pluginsLoadingRef.current = false;
    if (pluginLoadFrame.current !== undefined) cancelAnimationFrame(pluginLoadFrame.current);
    pluginLoadFrame.current = undefined;
    setPlugins(readPluginCache(next) ?? []);
    setPluginsLoading(false);
    setMcpServers([]);
    setSelected(next);
  };
  useEffect(() => { void loadAccounts().catch(showError); }, [selected]);
  useEffect(() => {
    if (workspaceSection === "plugins") schedulePluginLoad();
    return () => {
      if (pluginLoadFrame.current !== undefined) {
        cancelAnimationFrame(pluginLoadFrame.current);
        pluginLoadFrame.current = undefined;
      }
    };
  }, [selected, workspaceSection]);
  useEffect(() => { if (workspaceSection === "mcp") void listMcpServers(selected).then(setMcpServers).catch(showError); }, [selected, workspaceSection]);
  useEffect(() => {
    if (window.location.search) window.history.replaceState({}, "", "/");
    let unlisten: () => void = () => {};
    void listen("accounts-changed", () => {
      pluginsLoaded.current = false;
      localStorage.removeItem(PLUGIN_CACHE_KEY);
      void loadAccounts().catch(showError);
      if (workspaceSection === "plugins") schedulePluginLoad();
    }).then((stop) => { unlisten = stop; });
    return () => unlisten();
  }, [isCursor, selected, workspaceSection]);
  useEffect(() => {
    let unlisten: () => void = () => {};
    void listen<SwitchProgress>("account-switch-progress", ({ payload }) => {
      if (payload.operationId === activeOperationId.current) setSwitchProgress(payload);
    }).then((stop) => { unlisten = stop; });
    return () => unlisten();
  }, []);
  useEffect(() => {
    let unlisten: () => void = () => {};
    void listen<{ completed: number; total: number }>("account-refresh-progress", ({ payload }) => {
      setNotice(t("refreshProgress", payload));
    }).then((stop) => { unlisten = stop; });
    return () => unlisten();
  }, [t]);

  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    setNotice(undefined);
    try { await work(); await loadAccounts(); return true; } catch (error) { showError(error); return false; } finally { setBusy(false); }
  };
  const clearSwitch = () => {
    activeOperationId.current = undefined;
    setSwitchProgress(undefined);
    setBusy(false);
  };
  const finishSwitch = async (noticeKey: string) => {
    await loadAccounts();
    pluginsLoaded.current = false;
    if (workspaceSection === "plugins") schedulePluginLoad();
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
      const outcome = await invoke<SwitchOutcome>("switch_account", { id: account.id, operationId });
      if (outcome.restartRequired) {
        // The first phase only validates and prepares the switch. Do not show
        // its progress as an active switch while waiting for confirmation.
        setSwitchProgress(undefined);
        setBusy(false);
        setRestartDialog({ account, operationId });
        setCountdown(10);
        return;
      }
      await finishSwitch("cursorLaunched");
    } catch (error) {
      showError(error);
      setSwitchProgress({ operationId, accountId: account.id, stage: "error", percent: 100, status: "error" });
      setBusy(false);
    }
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
    setSwitchProgress({ operationId: dialog.operationId, accountId: dialog.account.id, stage: "terminating", percent: 0, status: "running" });
    try {
      await invoke("force_restart_cursor", { id: dialog.account.id, operationId: dialog.operationId });
      await finishSwitch("cursorRestarted");
    } catch (error) {
      showError(error);
      setSwitchProgress({ operationId: dialog.operationId, accountId: dialog.account.id, stage: "error", percent: 100, status: "error" });
      setBusy(false);
    }
  };
  useEffect(() => {
    if (!restartDialog) return;
    const timer = window.setInterval(() => {
      setCountdown((seconds) => {
        if (seconds <= 1) {
          window.clearInterval(timer);
          cancelRestart();
          return 0;
        }
        return seconds - 1;
      });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [restartDialog?.operationId]);
  const remove = (account: Account) => act(async () => { const { invoke } = await import("@tauri-apps/api/core"); await invoke("delete_account", { id: account.id }); setNotice(t("deleted")); });
  const refresh = async () => {
    setBusy(true);
    setRefreshing(true);
    setRefreshFailed(false);
    setNotice(t("refreshing"));
    try {
      const result = isCursor ? await invoke<{ total: number; failed: number; invalid: number; missing: number }>("refresh_all_cursor_accounts") : { total: 0, failed: 0, invalid: 0, missing: 0 };
      const failed = result.failed;
      await loadAccounts();
      const other = failed - result.invalid - result.missing;
      const reasons = [result.invalid && t("refreshTokenInvalid", { count: result.invalid }), result.missing && t("refreshCredentialMissing", { count: result.missing }), other && t("refreshOtherFailed", { count: other })].filter(Boolean).join("，");
      setNotice(failed ? t("subscriptionsRefreshIncomplete", { reasons }) : t(accounts.length && isCursor ? "subscriptionsRefreshed" : "refreshed"));
    } catch (error) { setRefreshFailed(true); showError(error); }
    finally { setBusy(false); setRefreshing(false); }
  };
  const reorder = async (activeId: string, targetId?: string) => {
    (document.activeElement as HTMLElement | null)?.blur();
    if (!targetId || activeId === targetId) return;
    const from = accounts.findIndex((account) => account.id === activeId);
    const to = accounts.findIndex((account) => account.id === targetId);
    if (from < 0 || to < 0) return;
    const next = arrayMove(accounts, from, to);
    setAccounts(next);
    if (await act(async () => { const { invoke } = await import("@tauri-apps/api/core"); await invoke("reorder_accounts", { kind: selected, ids: next.map((account) => account.id) }); })) setNotice(t("reordered"));
  };
  const exportAccounts = async () => {
    const file = await save({ defaultPath: `${selected}-accounts.json`, filters: [{ name: "JSON", extensions: ["json"] }], title: t("exportTitle") });
    if (!file) return;
    if (await act(async () => { await invoke("export_cursor_accounts", { file }); })) setNotice(t("exported"));
  };
  const openAccountExport = async (account: Account) => {
    try { setExportData(await invoke<unknown>("get_cursor_export_record", { id: account.id })); setExportTarget(account); }
    catch (error) { showError(error); }
  };
  const pluginKey = (plugin: Pick<CursorPlugin, "id" | "source">) => `${plugin.source}:${plugin.id}`;
  const togglePlugin = async (plugin: CursorPlugin) => {
    const key = pluginKey(plugin);
    const enabled = !plugin.enabled;
    setPendingPlugins((current) => new Set(current).add(key));
    try {
      if (selected === "codex") await setCodexPluginEnabled(plugin.id, enabled);
      else await setCursorPluginEnabled(plugin.id, plugin.source, enabled);
      setPlugins((current) => {
        const next = current.map((item) => pluginKey(item) === key ? { ...item, enabled } : item);
        writePluginCache(selected, currentCursorAccountId, next);
        return next;
      });
      setNotice(t("pluginRestartRequired", { application: selected === "codex" ? t("codex") : t("cursor") }));
    } catch (error) { showError(error); }
    finally {
      setPendingPlugins((current) => {
        const next = new Set(current);
        next.delete(key);
        return next;
      });
    }
  };
  const removePlugin = async (plugin: CursorPlugin) => {
    const key = pluginKey(plugin);
    setPendingPlugins((current) => new Set(current).add(key));
    try {
      await deleteCursorPlugin(plugin.id, plugin.source);
      setPlugins((current) => {
        const next = current.filter((item) => pluginKey(item) !== key);
        writePluginCache(selected, currentCursorAccountId, next);
        return next;
      });
      setNotice(t("pluginRestartRequired"));
    } catch (error) { showError(error); }
    finally {
      setPendingPlugins((current) => {
        const next = new Set(current);
        next.delete(key);
        return next;
      });
    }
  };
  const togglePluginCapability = async (plugin: CursorPlugin, capability: PluginCapability) => {
    if (capability.kind === "hook") return;
    const key = `${pluginKey(plugin)}:${capability.kind}:${capability.id}`;
    setPendingPlugins((current) => new Set(current).add(key));
    try {
      const enabled = !capability.enabled;
      await setCodexPluginCapabilityEnabled(plugin.id, capability.id, capability.kind, enabled);
      setPlugins((current) => current.map((item) => pluginKey(item) === pluginKey(plugin) ? { ...item, capabilities: item.capabilities.map((entry) => entry.kind === capability.kind && entry.id === capability.id ? { ...entry, enabled } : entry) } : item));
      setNotice(t("pluginRestartRequired", { application: t("codex") }));
    } catch (error) { showError(error); }
    finally { setPendingPlugins((current) => { const next = new Set(current); next.delete(key); return next; }); }
  };
  const pluginSourceLabel = (source: CursorPlugin["source"]) => t(`pluginSource${source[0].toUpperCase()}${source.slice(1)}`);
  const renderSection = () => {
    const applicationLabel = applications.find((app) => app.kind === selected)?.label ?? t(selected);
    if (workspaceSection !== "accounts") {
      const section = workspaceSections.find(({ id }) => id === workspaceSection)!;
      if (workspaceSection === "plugins") return plugins.length ? <div className={styles.pluginList}>{plugins.map((plugin) => { const groups = (["skill", "mcp", "hook"] as const).map((kind) => ({ kind, capabilities: plugin.capabilities.filter((capability) => capability.kind === kind) })).filter((group) => group.capabilities.length); return <article className={styles.pluginCard} key={pluginKey(plugin)}><header className={styles.pluginHeader}><PluginIcon icon={plugin.icon} /><div className={styles.pluginCopy}><strong>{plugin.name}</strong>{plugin.description && <p>{plugin.description}</p>}</div><span className={styles.pluginSourceBadge} data-source={plugin.source}>{pluginSourceLabel(plugin.source)}</span><div className={styles.pluginActions}><Tooltip content={plugin.teamRequired ? t("pluginManagedByTeam") : plugin.enabled ? t("pluginDisable") : t("pluginEnable")}><button aria-checked={plugin.enabled} aria-label={plugin.teamRequired ? t("pluginManagedByTeam") : plugin.enabled ? t("pluginDisable") : t("pluginEnable")} className={styles.pluginSwitch} disabled={busy || plugin.teamRequired || pendingPlugins.has(pluginKey(plugin))} onClick={() => void togglePlugin(plugin)} role="switch" type="button"><span /></button></Tooltip>{(plugin.source === "local" || plugin.source === "marketplace") && <Tooltip content={plugin.teamRequired ? t("pluginManagedByTeam") : t("pluginDelete")}><button aria-label={plugin.teamRequired ? t("pluginManagedByTeam") : t("pluginDelete")} className={styles.iconButton} disabled={busy || plugin.teamRequired || pendingPlugins.has(pluginKey(plugin))} onClick={() => void removePlugin(plugin)} type="button"><Trash2 aria-hidden="true" size={18} /></button></Tooltip>}</div></header>{groups.map(({ kind, capabilities }) => <section className={styles.capabilityGroup} key={kind}><h2>{kind === "skill" ? t("pluginCapabilitySkills") : kind === "mcp" ? t("pluginCapabilityMcps") : t("pluginCapabilityHooks")} <span>{capabilities.length}</span></h2><div className={styles.capabilityList}>{capabilities.map((capability) => { const key = `${pluginKey(plugin)}:${capability.kind}:${capability.id}`; const manageable = capability.kind !== "hook"; return <div className={styles.capabilityRow} key={`${capability.kind}:${capability.id}`}><span className={styles.capabilityIcon}>{capability.kind === "skill" ? <Puzzle aria-hidden="true" size={20} /> : capability.kind === "mcp" ? <Waypoints aria-hidden="true" size={20} /> : <Zap aria-hidden="true" size={20} />}</span><div className={styles.capabilityCopy}><strong>{capability.name}</strong>{capability.description && <p>{capability.description}</p>}</div>{manageable ? <Tooltip content={capability.enabled ? t("pluginDisable") : t("pluginEnable")}><button aria-checked={capability.enabled} aria-label={capability.enabled ? t("pluginDisable") : t("pluginEnable")} className={styles.pluginSwitch} disabled={busy || !plugin.enabled || pendingPlugins.has(key)} onClick={() => void togglePluginCapability(plugin, capability)} role="switch" type="button"><span /></button></Tooltip> : <span className={styles.hookNotice}>{t("pluginHookTrustRequired")}</span>}</div>; })}</div></section>)}</article>; })}</div> : <div className={styles.empty}><Puzzle aria-hidden="true" size={32} />{pluginsLoading ? <h2>{t("refreshing")}</h2> : <><h2>{t("pluginsEmptyTitle")}</h2><p>{t("pluginsEmptyDescription")}</p></>}</div>;
      if (workspaceSection === "mcp") return mcpServers.length ? <div className={styles.pluginList}>{mcpServers.map((server) => <article className={styles.pluginCard} key={server.id}><span className={styles.pluginIcon}><Waypoints aria-hidden="true" size={20} /></span><strong>{server.name}</strong></article>)}</div> : <div className={styles.empty}><Waypoints aria-hidden="true" size={32} /><h2>{applicationLabel} · {t("mcpTitle")}</h2><p>{t("mcpDescription")}</p></div>;
      return <div className={styles.placeholder}><h2>{applicationLabel} · {t(`${section.labelKey}Title`)}</h2><p>{t(`${section.labelKey}Description`)} </p></div>;
    }
    return accounts.length === 0 ? <div className={styles.empty}><KeyRound aria-hidden="true" size={32} /><h2>{t("emptyTitle")}</h2><p>{t("emptyDescription")}</p></div> : <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => void reorder(String(active.id), over ? String(over.id) : undefined)} sensors={sensors}><SortableContext items={accounts.map((account) => account.id)} strategy={verticalListSortingStrategy}><div className={styles.accountList}>{accounts.map((account) => <SortableAccount account={account} busy={busy} key={account.id} onExport={(account) => void openAccountExport(account)} onRemove={remove} onSwitch={switchTo} progress={switchProgress?.accountId === account.id ? switchProgress : undefined} />)}</div></SortableContext></DndContext>;
  };

  return <Toast.Provider><main className={styles.shell}>
    <header className={styles.header}>
      <div className={styles.brand}><img alt="" src={logo} /><span>{t("appName")}</span><Tooltip content={t("settings")}><a aria-label={t("settings")} className={styles.settingsButton} href="/settings.html"><Settings aria-hidden="true" size={16} /></a></Tooltip></div>
      <Tabs.Root className={styles.switcher} onValueChange={(value) => selectApplication(value as ApplicationKind)} value={selected}><Tabs.List aria-label={t("applications")}>
        {(["cursor", "codex"] as const).map((kind) => <Tabs.Trigger className={styles.appTab} key={kind} value={kind}><img alt="" src={kind === "cursor" ? cursorIcon : codexIcon} />{kind === "codex" ? t("codex") : applications.find((app) => app.kind === kind)?.label ?? t(kind)}</Tabs.Trigger>)}
      </Tabs.List></Tabs.Root>
      <WorkspaceToolbar busy={busy} canManageAccounts={isCursor} hasAccounts={accounts.length > 0} onExport={() => void exportAccounts()} onRefresh={() => void refresh()} refreshing={refreshing} section={workspaceSection} />
    </header>
    <section className={styles.workspace}>
      <aside aria-label={t("accountSections")} className={styles.sidebar}>
        <nav className={styles.sidebarNav}>
          {workspaceSections.map(({ id, icon: Icon, labelKey }) => <Tooltip content={t(labelKey)} key={id}><button aria-label={t(labelKey)} aria-current={workspaceSection === id ? "page" : undefined} className={`${styles.sidebarItem} ${workspaceSection === id ? styles.sidebarItemActive : ""}`} onClick={() => { if (id === workspaceSection) return; setWorkspaceSection(id); }} type="button"><Icon aria-hidden="true" size={18} /></button></Tooltip>)}
        </nav>
      </aside>
      <div className={styles.content}>
        {renderSection()}
      </div>
    </section>
  </main>
  <AlertDialog.Root onOpenChange={(open) => { if (!open && restartDialog) cancelRestart(); }} open={Boolean(restartDialog)}><AlertDialog.Portal><AlertDialog.Overlay className={styles.dialogOverlay} /><AlertDialog.Content className={styles.dialogContent}><AlertDialog.Title>{t("restartDialogTitle")}</AlertDialog.Title><AlertDialog.Description>{t("restartDialogDescription")}</AlertDialog.Description><p className={styles.dialogWarning}>{t("restartDialogWarning")}</p><div className={styles.dialogActions}><AlertDialog.Cancel asChild><button autoFocus className={styles.dialogCancel} onClick={cancelRestart} type="button">{t("cancelCountdown", { seconds: countdown })}</button></AlertDialog.Cancel><button className={styles.dialogConfirm} onClick={() => void forceRestart()} type="button">{t("forceRestart")}</button></div></AlertDialog.Content></AlertDialog.Portal></AlertDialog.Root>
  {exportTarget && exportData !== undefined && <ExportDialog data={[exportData]} filename={`cursor-account-${exportTarget.id}.json`} onOpenChange={(open) => { if (!open) setExportTarget(undefined); }} open />}
  <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={refreshing ? "loading" : refreshFailed ? "error" : "success"} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<AccountsPage />);
