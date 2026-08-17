import * as Progress from "@radix-ui/react-progress";
import { ArrowLeft, ChartNoAxesCombined, FileOutput, LoaderCircle, RefreshCw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { ExportDialog } from "../../components/ExportDialog";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import toastStyles from "../../components/ToastMessage.module.css";
import { Tooltip } from "../../components/Tooltip";
import "../../i18n";
import type { CursorUsageDetails } from "../../lib/types";
import "../../styles/global.css";
import { daysUntil, isOverLimit, metric } from "./format";
import { planHeat } from "./heat";
import { ModelBars } from "./ModelBars";
import { WeeklyChart } from "./WeeklyChart";
import styles from "./page.module.css";

function membershipLabel(type?: string) {
  if (!type) return;
  return type.charAt(0).toUpperCase() + type.slice(1);
}

function UsageMeter({ label, value }: { label: string; value: CursorUsageDetails["primary"] }) {
  const percent = Math.min(Math.max(value.percent, 0), 100);
  return <div className={styles.meter}>
    <div className={styles.meterRow}><span>{label}</span><strong>{metric(value)}</strong></div>
    <div className={styles.progressRow}>
      <Progress.Root aria-label={label} className={styles.progressRoot} value={percent}>
        <Progress.Indicator className={`${styles.progressIndicator} ${styles[planHeat(value.percent)]}`} style={{ transform: `translateX(-${100 - percent}%)` }} />
      </Progress.Root>
      <small>{Math.round(value.percent)}%</small>
    </div>
  </div>;
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
  useEffect(() => { if (accountId) void invoke<CursorUsageDetails | null>("get_saved_cursor_usage", { id: accountId }).then((usage) => { if (usage) setData(usage); }).catch((error) => setError(error instanceof Error ? error.message : String(error))); }, [accountId]);
  const refresh = async () => {
    if (!accountId || busy) { if (!accountId) setError(t("usageUnknown")); return; }
    setBusy(true);
    setError(undefined);
    setNoticeStatus("loading");
    setNotice(t("usageLoading"));
    try {
      setData(await invoke<CursorUsageDetails>("get_cursor_usage", { id: accountId }));
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
    <header className={styles.header}>
      <a aria-label={t("back")} className={styles.back} href="/"><ArrowLeft aria-hidden="true" size={20} /></a>
      <div><p>{data?.label ?? t("cursor")}</p><h1>{t("usageTitle")}</h1></div>
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
        <strong>{data.name ?? data.label}</strong>
        {membership && <span className={styles.badge}>{membership}</span>}
        {data.email && <span className={styles.email}>{data.email}</span>}
      </div>
      <UsageMeter label={t("usagePrimary")} value={data.primary} />
      {data.onDemand && <div className={styles.secondary}><span>{t("usageOnDemand")}</span><strong className={isOverLimit(data.onDemand) ? styles.overLimit : undefined}>{metric(data.onDemand)}</strong></div>}
      <div className={styles.charts}>
        <section><h2>{t("usageWeekly")}</h2>{data.weeklyAvailable ? <WeeklyChart days={data.weekly} unitsLabel={t("usageUnits")} /> : <p className={styles.muted}>{data.weeklyError ?? t("usageWeeklyUnavailable")}</p>}</section>
        <section><h2>{t("usageModels")}</h2>{data.models.length ? <ModelBars models={data.models} /> : <p className={styles.muted}>{t("usageNoModels")}</p>}</section>
      </div>
      <div className={styles.meta}>
        {resetAt ? <Tooltip content={resetAt}><button className={styles.reset} type="button">{resetCopy(data.resetAt, t)}</button></Tooltip> : <span>{t("usageUnknown")}</span>}
        <span className={justUpdated ? styles.justUpdated : undefined}>{t("usageCheckedAt", { time: checkedAt })}</span>
      </div>
    </section>}
    {exportData !== undefined && <ExportDialog data={[exportData]} filename={`cursor-account-${accountId}.json`} onOpenChange={setExportOpen} open={exportOpen} />}
  </main>
  <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={noticeStatus} />
  <Toast.Viewport className={toastStyles.viewport} />
  </Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<UsagePage />);
