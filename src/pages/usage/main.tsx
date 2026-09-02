import { ArrowLeft, ChartNoAxesCombined, FileOutput, LoaderCircle, RefreshCw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { ExportDialog } from "../../components/ExportDialog";
import { DownloadDock } from "../../components/DownloadDock";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import toastStyles from "../../components/ToastMessage.module.css";
import { Tooltip } from "../../components/Tooltip";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import "../../i18n";
import { applicationKindFromQuery, homePath, syncDocumentAppKind, type CursorUsageDetails } from "../../lib/types";
import "../../styles/global.css";
import { daysUntil, hasLimit, isOverLimit, metric, spendCents } from "./format";
import { EventLedger } from "./EventLedger";
import { ModelBars } from "./ModelBars";
import { WeeklyChart } from "./WeeklyChart";
import styles from "./page.module.css";

function membershipLabel(type?: string) {
  if (!type) return;
  return type.charAt(0).toUpperCase() + type.slice(1);
}

function usageUsedCopy(
  label: string,
  value: CursorUsageDetails["primary"] | NonNullable<CursorUsageDetails["onDemand"]>,
  t: (key: string, options?: Record<string, unknown>) => string,
) {
  const amount = metric(value);
  if (value.kind === "currency" && hasLimit(value)) {
    return t("usageUsedWithLimit", { label, amount, percent: Math.round(Math.max(value.percent, 0)) });
  }
  if (value.kind === "currency" || value.kind === "percent") {
    return t("usageUsed", { label, amount });
  }
  return `${label} ${amount}`;
}

function resetCopy(resetAt: string | undefined, t: (key: string, options?: Record<string, unknown>) => string) {
  const days = daysUntil(resetAt);
  if (days === undefined) return t("usageUnknown");
  if (days > 0) return t("usageResetsIn", { count: days });
  if (days === 0) return t("usageResetsToday");
  return t("usageResetPassed");
}

function UsagePage() {
  const { t } = useTranslation();
  const accountId = new URLSearchParams(window.location.search).get("accountId") ?? "";
  const [data, setData] = useState<CursorUsageDetails>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [noticeStatus, setNoticeStatus] = useState<"loading" | "success" | "error">("success");
  const [justUpdated, setJustUpdated] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportData, setExportData] = useState<unknown>();
  const flashTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(flashTimer.current), []);
  useEffect(() => {
    let cancelled = false;
    setData(undefined);
    setError(undefined);
    if (!accountId) return () => { cancelled = true; };
    void invoke<CursorUsageDetails | null>("get_saved_cursor_usage", { id: accountId })
      .then((usage) => {
        if (!cancelled && usage?.accountId === accountId) setData(usage);
      })
      .catch((error) => {
        if (!cancelled) setError(error instanceof Error ? error.message : String(error));
      });
    return () => { cancelled = true; };
  }, [accountId]);
  const refresh = async () => {
    if (!accountId || busy) { if (!accountId) setError(t("usageUnknown")); return; }
    setBusy(true);
    setError(undefined);
    setNoticeStatus("loading");
    setNotice(t("usageLoading"));
    try {
      const usage = await invoke<CursorUsageDetails>("get_cursor_usage", { id: accountId });
      if (usage.accountId !== accountId) throw new Error(t("usageAccountMismatch"));
      setData(usage);
      setNoticeStatus("success");
      setNotice(t("usageRefreshed"));
      setJustUpdated(true);
      window.clearTimeout(flashTimer.current);
      flashTimer.current = window.setTimeout(() => setJustUpdated(false), 1400);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setError(message);
      setNoticeStatus("error");
      setNotice(message);
    } finally {
      setBusy(false);
    }
  };
  const openExport = async () => {
    if (!accountId) return;
    try { setExportData(await invoke<unknown>("get_cursor_export_record", { id: accountId })); setExportOpen(true); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); }
  };
  const membership = membershipLabel(data?.membershipType);
  const resetAt = data?.resetAt ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(data.resetAt)) : undefined;
  const checkedAt = data ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(data.checkedAt * 1000)) : undefined;
  return <Toast.Provider>
    <main className={styles.shell}>
    <WindowDragSurface />
    <header className={styles.header}>
      <a aria-label={t("back")} className={styles.back} href={homePath(applicationKindFromQuery())}><ArrowLeft aria-hidden="true" size={20} /></a>
      <h1>{t("usageTitle")}</h1>
      <div className={styles.actions}>
        <Tooltip content={t("export")}><button aria-label={t("export")} className={styles.export} disabled={busy} onClick={() => void openExport()} type="button"><FileOutput aria-hidden="true" size={18} /></button></Tooltip>
        <button aria-busy={busy} aria-label={busy ? t("usageLoading") : t("usageRefresh")} className={`${styles.refresh} ${busy ? styles.refreshBusy : ""}`} onClick={() => void refresh()} type="button">
          {busy ? <LoaderCircle aria-hidden="true" className={styles.spinning} size={17} /> : <RefreshCw aria-hidden="true" size={17} />}
          {t("usageRefresh")}
        </button>
      </div>
    </header>
    {error && !(notice && noticeStatus === "error") && <p className={styles.error}>{error}</p>}
    {!data && !busy && !error && <section className={styles.empty}><ChartNoAxesCombined aria-hidden="true" size={48} /><h2>{t("usageEmptyTitle")}</h2><p>{t("usageEmptyDescription")}</p></section>}
    {!data && busy && <section className={styles.empty}><LoaderCircle aria-hidden="true" className={styles.spinning} size={28} /><h2>{t("usageLoading")}</h2></section>}
    {data && <section className={styles.workspace}>
      {busy && <div aria-hidden="true" className={styles.indeterminate}><span /></div>}
      <div className={styles.identity}>
        <strong>{data.email ?? data.label}</strong>
        {membership && <span className={styles.badge}>{membership}</span>}
      </div>
      <div className={styles.usageLines}>
        <p className={isOverLimit(data.primary) ? `${styles.usageLine} ${styles.overLimit}` : styles.usageLine}>{usageUsedCopy(t("usagePrimary"), data.primary, t)}</p>
        {data.onDemand && <p className={isOverLimit(data.onDemand) ? `${styles.usageLine} ${styles.overLimit}` : styles.usageLine}>{usageUsedCopy(t("usageOnDemand"), data.onDemand, t)}</p>}
      </div>
      <div className={styles.charts}>
        <section><h2>{t("usageWeekly")}</h2>{data.weeklyAvailable ? <WeeklyChart days={data.weekly} events={data.events ?? []} /> : <p className={styles.muted}>{data.weeklyError ?? t("usageWeeklyUnavailable")}</p>}</section>
        <section><h2>{t("usageModels")}</h2>{(data.events ?? []).some((event) => spendCents(event) !== undefined) ? <ModelBars events={data.events ?? []} models={data.models} /> : <p className={styles.muted}>{t("usageNoModels")}</p>}</section>
      </div>
      <EventLedger events={data.events ?? []} unavailable={data.weeklyError} />
      <div className={styles.meta}>
        {resetAt ? <Tooltip content={resetAt}><button className={styles.reset} type="button">{resetCopy(data.resetAt, t)}</button></Tooltip> : <span>{t("usageUnknown")}</span>}
        <span className={justUpdated ? styles.justUpdated : undefined}>{t("usageCheckedAt", { time: checkedAt })}</span>
      </div>
    </section>}
    {exportData !== undefined && <ExportDialog data={[exportData]} filename={`cursor-account-${accountId}.json`} onOpenChange={setExportOpen} open={exportOpen} />}
  </main>
  <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={noticeStatus} />
  <Toast.Viewport className={toastStyles.viewport} />
  <DownloadDock />
  </Toast.Provider>;
}

syncDocumentAppKind(applicationKindFromQuery());
createRoot(document.getElementById("root")!).render(<UsagePage />);
