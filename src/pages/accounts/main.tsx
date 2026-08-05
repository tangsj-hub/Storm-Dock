import { listen } from "@tauri-apps/api/event";
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Progress from "@radix-ui/react-progress";
import * as Tabs from "@radix-ui/react-tabs";
import { Check, GripVertical, KeyRound, LogIn, Plus, RefreshCw, Settings, ShieldCheck, Trash2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import "../../i18n";
import { listAccounts, listApplications } from "../../lib/api";
import { type Account, type ApplicationKind, type ApplicationStatus } from "../../lib/types";
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

function SortableAccount({ account, busy, onRemove, onSwitch, progress }: { account: Account; busy: boolean; onRemove: (account: Account) => void; onSwitch: (account: Account) => void; progress?: SwitchProgress }) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: account.id });
  return <article className={`${styles.accountCard} ${account.isCurrent ? styles.current : ""} ${isDragging ? styles.dragging : ""}`} ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }}>
    <GripVertical className={styles.dragHandle} aria-label={t("drag", { account: account.label })} size={24} {...attributes} {...listeners} />
    <div className={styles.accountCopy}><strong>{account.label}</strong><span>{t(`importTypes.${account.importType}`)}</span></div>
    <div className={styles.accountActions}>
      {progress ? <div className={styles.progress}><span>{t(`switchStages.${progress.stage}`)}</span><Progress.Root aria-label={t("switchProgress")} className={styles.progressRoot} value={progress.percent}><Progress.Indicator className={progress.status === "error" ? styles.progressError : styles.progressIndicator} style={{ transform: `translateX(-${100 - progress.percent}%)` }} /></Progress.Root></div> : account.isCurrent ? <span className={styles.currentBadge}><Check aria-hidden="true" size={16} />{t("current")}</span> : <button className={styles.activate} disabled={busy} onClick={() => onSwitch(account)} type="button"><LogIn aria-hidden="true" size={17} />{t("switch")}</button>}
      {progress?.status === "error" && <button className={styles.activate} onClick={() => onSwitch(account)} type="button"><RefreshCw aria-hidden="true" size={16} />{t("retry")}</button>}
      <button className={styles.iconButton} disabled={busy} onClick={() => onRemove(account)} title={t("remove", { account: account.label })} type="button"><Trash2 aria-hidden="true" size={19} /></button>
    </div>
  </article>;
}

function AccountsPage() {
  const { t } = useTranslation();
  const [applications, setApplications] = useState<ApplicationStatus[]>([]);
  const [selected, setSelected] = useState<ApplicationKind>("cursor");
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState(() => new URLSearchParams(window.location.search).get("notice") ?? undefined);
  const [switchProgress, setSwitchProgress] = useState<SwitchProgress>();
  const [restartDialog, setRestartDialog] = useState<{ account: Account; operationId: string }>();
  const [countdown, setCountdown] = useState(10);
  const activeOperationId = useRef<string | undefined>(undefined);
  const sensors = useSensors(useSensor(PointerSensor), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  const isCursor = selected === "cursor";

  const showError = (error: unknown) => setNotice(error instanceof Error ? error.message : String(error));
  const load = async () => {
    const [nextApplications, nextAccounts] = await Promise.all([listApplications(), listAccounts(selected)]);
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
    const operationId = crypto.randomUUID();
    activeOperationId.current = operationId;
    setBusy(true);
    setNotice(undefined);
    setSwitchProgress({ operationId, accountId: account.id, stage: "loading", percent: 0, status: "running" });
    try {
      const outcome = await invoke<SwitchOutcome>("switch_account", { id: account.id, operationId });
      if (outcome.restartRequired) {
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
    setNotice(t("restartRequired"));
    clearSwitch();
  };
  const forceRestart = async () => {
    const dialog = restartDialog;
    if (!dialog) return;
    setRestartDialog(undefined);
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

  return <Toast.Provider><main className={styles.shell}>
    <header className={styles.header}>
      <div className={styles.brand}><ShieldCheck aria-hidden="true" size={19} /><span>{t("appName")}</span><a className={styles.settingsButton} href="/settings.html" title={t("settings")}><Settings aria-hidden="true" size={16} /></a></div>
      <Tabs.Root className={styles.switcher} onValueChange={(value) => setSelected(value as ApplicationKind)} value={selected}><Tabs.List aria-label={t("applications")}>
        {(["cursor", "codex"] as const).map((kind) => <Tabs.Trigger className={styles.appTab} key={kind} value={kind}>{applications.find((app) => app.kind === kind)?.label ?? t(kind)}</Tabs.Trigger>)}
      </Tabs.List></Tabs.Root>
      <div className={styles.toolbar}><button className={styles.iconButton} disabled={busy} onClick={() => void load().catch(showError)} title={t("refresh")} type="button"><RefreshCw aria-hidden="true" size={19} /></button><a aria-disabled={busy || !isCursor} className={styles.addButton} href={busy || !isCursor ? undefined : "/add.html"}><Plus aria-hidden="true" size={18} />{t("addAccount")}</a></div>
    </header>
    <section className={styles.workspace}>
      {selected === "codex" ? <div className={styles.empty}><h2>{t("unsupportedTitle")}</h2><p>{t("unsupportedDescription")}</p></div> : accounts.length === 0 ? <div className={styles.empty}><KeyRound aria-hidden="true" size={32} /><h2>{t("emptyTitle")}</h2><p>{t("emptyDescription")}</p></div> :
        <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => void reorder(String(active.id), over ? String(over.id) : undefined)} sensors={sensors}><SortableContext items={accounts.map((account) => account.id)} strategy={verticalListSortingStrategy}><div className={styles.accountList}>{accounts.map((account) => <SortableAccount account={account} busy={busy} key={account.id} onRemove={remove} onSwitch={switchTo} progress={switchProgress?.accountId === account.id ? switchProgress : undefined} />)}</div></SortableContext></DndContext>}
    </section>
  </main>
  <AlertDialog.Root onOpenChange={(open) => { if (!open && restartDialog) cancelRestart(); }} open={Boolean(restartDialog)}><AlertDialog.Portal><AlertDialog.Overlay className={styles.dialogOverlay} /><AlertDialog.Content className={styles.dialogContent}><AlertDialog.Title>{t("restartDialogTitle")}</AlertDialog.Title><AlertDialog.Description>{t("restartDialogDescription")}</AlertDialog.Description><p className={styles.dialogWarning}>{t("restartDialogWarning")}</p><div className={styles.dialogActions}><AlertDialog.Cancel asChild><button autoFocus className={styles.dialogCancel} onClick={cancelRestart} type="button">{t("cancelCountdown", { seconds: countdown })}</button></AlertDialog.Cancel><button className={styles.dialogConfirm} onClick={() => void forceRestart()} type="button">{t("forceRestart")}</button></div></AlertDialog.Content></AlertDialog.Portal></AlertDialog.Root>
  <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<AccountsPage />);
