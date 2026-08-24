import { listen } from "@tauri-apps/api/event";
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Progress from "@radix-ui/react-progress";
import * as Tabs from "@radix-ui/react-tabs";
import { Check, ChartNoAxesCombined, Download, FileOutput, GripVertical, KeyRound, LogIn, Plus, RefreshCw, Settings, Trash2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { ExportDialog } from "../../components/ExportDialog";
import { Tooltip } from "../../components/Tooltip";
import logo from "../../assets/logo.svg";
import codexIcon from "../../assets/codex.svg";
import cursorIcon from "../../assets/cursor.svg";
import "../../i18n";
import { listAccounts, listApplications } from "../../lib/api";
import { canSwitchToDesktop, type Account, type ApplicationKind, type ApplicationStatus } from "../../lib/types";
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

function SortableAccount({ account, busy, onExport, onRemove, onSwitch, progress }: { account: Account; busy: boolean; onExport: (account: Account) => void; onRemove: (account: Account) => void; onSwitch: (account: Account) => void; progress?: SwitchProgress }) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ disabled: busy, id: account.id });
  return <article className={`${styles.accountCard} ${account.isCurrent ? styles.current : ""} ${isDragging ? styles.dragging : ""}`} ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }}>
    <GripVertical className={styles.dragHandle} aria-label={t("drag", { account: account.label })} size={24} {...attributes} {...listeners} />
    <div className={styles.accountCopy}><strong>{account.label}</strong>{(() => { const subscription = subscriptionLabel(account, t); return subscription && <span className={styles.subscription}><span className={`${styles.planBadge} ${styles[`plan-${subscription.plan}`] ?? styles.planDefault}`}>{subscription.name}</span><span className={styles.expiry}>{subscription.expiry}</span></span>; })()}{account.status === "invalid" && <span className={styles.invalidBadge}>{t("tokenInvalid", { defaultValue: "Token已失效" })}</span>}{account.status === "missing" && <span className={styles.missingBadge}>{t("credentialMissing", { defaultValue: "凭证缺失" })}</span>}</div>
    <div className={styles.accountActions}>
      {progress ? <div className={styles.progress}><span>{t(`switchStages.${progress.stage}`)}</span><Progress.Root aria-label={t("switchProgress")} className={styles.progressRoot} value={progress.percent}><Progress.Indicator className={progress.status === "error" ? styles.progressError : styles.progressIndicator} style={{ transform: `translateX(-${100 - progress.percent}%)` }} /></Progress.Root></div> : account.isCurrent ? <span className={styles.currentBadge}><Check aria-hidden="true" size={16} />{t("current")}</span> : canSwitchToDesktop(account) ? <button className={styles.activate} disabled={busy} onClick={() => onSwitch(account)} type="button"><LogIn aria-hidden="true" size={17} />{t("switch")}</button> : null}
      {progress?.status === "error" && canSwitchToDesktop(account) && <button className={styles.activate} onClick={() => onSwitch(account)} type="button"><RefreshCw aria-hidden="true" size={16} />{t("retry")}</button>}
      <Tooltip content={t("usage")}><a aria-label={t("viewUsage", { account: account.label })} className={styles.iconButton} href={`/usage.html?accountId=${encodeURIComponent(account.id)}`}><ChartNoAxesCombined aria-hidden="true" size={18} /></a></Tooltip>
      <Tooltip content={t("export")}><button aria-label={t("exportAccount", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onExport(account)} type="button"><FileOutput aria-hidden="true" size={18} /></button></Tooltip>
      <Tooltip content={t("delete")}><button aria-label={t("remove", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onRemove(account)} type="button"><Trash2 aria-hidden="true" size={19} /></button></Tooltip>
    </div>
  </article>;
}

function AccountsPage() {
  const { t } = useTranslation();
  const [applications, setApplications] = useState<ApplicationStatus[]>([]);
  const [selected, setSelected] = useState<ApplicationKind>("cursor");
  const [accounts, setAccounts] = useState<Account[]>([]);
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
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  const isCursor = selected === "cursor";
  const showError = (error: unknown) => setNotice(error instanceof Error ? error.message : String(error));
  const load = async () => {
    const request = ++latestLoad.current;
    const [nextApplications, nextAccounts] = await Promise.all([listApplications(), listAccounts(selected)]);
    if (request !== latestLoad.current) return;
    setApplications(nextApplications);
    setAccounts(nextAccounts);
  };
  useEffect(() => { void load().catch(showError); }, [selected]);
  useEffect(() => {
    if (window.location.search) window.history.replaceState({}, "", "/");
    let unlisten: () => void = () => {};
    void listen("accounts-changed", () => void load().catch(showError)).then((stop) => { unlisten = stop; });
    return () => unlisten();
  }, [selected]);
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
    try { await work(); await load(); return true; } catch (error) { showError(error); return false; } finally { setBusy(false); }
  };
  const clearSwitch = () => {
    activeOperationId.current = undefined;
    setSwitchProgress(undefined);
    setBusy(false);
  };
  const finishSwitch = async (noticeKey: string) => {
    await load();
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
      await load();
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

  return <Toast.Provider><main className={styles.shell}>
    <header className={styles.header}>
      <div className={styles.brand}><img alt="" src={logo} /><span>{t("appName")}</span><Tooltip content={t("settings")}><a aria-label={t("settings")} className={styles.settingsButton} href="/settings.html"><Settings aria-hidden="true" size={16} /></a></Tooltip></div>
      <Tabs.Root className={styles.switcher} onValueChange={(value) => setSelected(value as ApplicationKind)} value={selected}><Tabs.List aria-label={t("applications")}>
        {(["cursor", "codex"] as const).map((kind) => <Tabs.Trigger className={styles.appTab} key={kind} value={kind}><img alt="" src={kind === "cursor" ? cursorIcon : codexIcon} />{applications.find((app) => app.kind === kind)?.label ?? t(kind)}</Tabs.Trigger>)}
      </Tabs.List></Tabs.Root>
      <div className={styles.toolbar}><Tooltip content={t("export")}><button aria-label={t("export")} className={styles.iconButton} disabled={busy || !isCursor || !accounts.length} onClick={() => void exportAccounts()} type="button"><Download aria-hidden="true" size={19} /></button></Tooltip><Tooltip content={t("refresh")}><button aria-label={t("refresh")} className={styles.iconButton} disabled={busy} onClick={() => void refresh()} type="button"><RefreshCw aria-hidden="true" className={refreshing ? styles.spinning : undefined} size={19} /></button></Tooltip><a aria-disabled={busy || !isCursor} className={styles.addButton} href={busy || !isCursor ? undefined : "/add.html"}><Plus aria-hidden="true" size={18} />{t("addAccount")}</a></div>
    </header>
    <section className={styles.workspace}>
      {selected === "codex" ? <div className={styles.empty}><h2>{t("unsupportedTitle")}</h2><p>{t("unsupportedDescription")}</p></div> : accounts.length === 0 ? <div className={styles.empty}><KeyRound aria-hidden="true" size={32} /><h2>{t("emptyTitle")}</h2><p>{t("emptyDescription")}</p></div> :
        <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => void reorder(String(active.id), over ? String(over.id) : undefined)} sensors={sensors}><SortableContext items={accounts.map((account) => account.id)} strategy={verticalListSortingStrategy}><div className={styles.accountList}>{accounts.map((account) => <SortableAccount account={account} busy={busy} key={account.id} onExport={(account) => void openAccountExport(account)} onRemove={remove} onSwitch={switchTo} progress={switchProgress?.accountId === account.id ? switchProgress : undefined} />)}</div></SortableContext></DndContext>}
    </section>
  </main>
  <AlertDialog.Root onOpenChange={(open) => { if (!open && restartDialog) cancelRestart(); }} open={Boolean(restartDialog)}><AlertDialog.Portal><AlertDialog.Overlay className={styles.dialogOverlay} /><AlertDialog.Content className={styles.dialogContent}><AlertDialog.Title>{t("restartDialogTitle")}</AlertDialog.Title><AlertDialog.Description>{t("restartDialogDescription")}</AlertDialog.Description><p className={styles.dialogWarning}>{t("restartDialogWarning")}</p><div className={styles.dialogActions}><AlertDialog.Cancel asChild><button autoFocus className={styles.dialogCancel} onClick={cancelRestart} type="button">{t("cancelCountdown", { seconds: countdown })}</button></AlertDialog.Cancel><button className={styles.dialogConfirm} onClick={() => void forceRestart()} type="button">{t("forceRestart")}</button></div></AlertDialog.Content></AlertDialog.Portal></AlertDialog.Root>
  {exportTarget && exportData !== undefined && <ExportDialog data={[exportData]} filename={`cursor-account-${exportTarget.id}.json`} onOpenChange={(open) => { if (!open) setExportTarget(undefined); }} open />}
  <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={refreshing ? "loading" : refreshFailed ? "error" : "success"} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<AccountsPage />);
