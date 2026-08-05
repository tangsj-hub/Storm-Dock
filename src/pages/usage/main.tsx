import { ArrowLeft, ChartNoAxesCombined, FileOutput, RefreshCw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { ExportDialog } from "../../components/ExportDialog";
import { Tooltip } from "../../components/Tooltip";
import "../../i18n";
import type { CursorUsageDetails } from "../../lib/types";
import "../../styles/global.css";
import styles from "./page.module.css";

const money = (cents: number) => `$${(cents / 100).toFixed(2)}`;
const number = new Intl.NumberFormat();

function metric(value: CursorUsageDetails["primary"]) {
  if (value.kind === "currency") return value.limit === undefined ? money(value.used) : `${money(value.used)} / ${money(value.limit)}`;
  if (value.kind === "percent") return `${Math.round(value.percent)}%`;
  return number.format(value.used);
}

function UsagePage() {
  const { t } = useTranslation();
  const accountId = new URLSearchParams(window.location.search).get("accountId") ?? "";
  const [data, setData] = useState<CursorUsageDetails>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [exportOpen, setExportOpen] = useState(false);
  const [exportData, setExportData] = useState<unknown>();
  useEffect(() => { if (accountId) void invoke<CursorUsageDetails | null>("get_saved_cursor_usage", { id: accountId }).then((usage) => { if (usage) setData(usage); }).catch((error) => setError(error instanceof Error ? error.message : String(error))); }, [accountId]);
  const refresh = async () => {
    if (!accountId) { setError(t("usageUnknown")); return; }
    setBusy(true); setError(undefined);
    try { setData(await invoke<CursorUsageDetails>("get_cursor_usage", { id: accountId })); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };
  const openExport = async () => {
    if (!accountId) return;
    try { setExportData(await invoke<unknown>("get_cursor_export_record", { id: accountId })); setExportOpen(true); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); }
  };
  const max = Math.max(...(data?.weekly.map((day) => day.requests) ?? [1]), 1);
  return <main className={styles.shell}>
    <header className={styles.header}><Tooltip content={t("back")}><a aria-label={t("back")} className={styles.back} href="/"><ArrowLeft aria-hidden="true" size={20} /></a></Tooltip><div><p>{data?.label ?? t("cursor")}</p><h1>{t("usageTitle")}</h1></div><div className={styles.actions}><Tooltip content={t("export")}><button aria-label={t("export")} className={styles.export} disabled={busy} onClick={() => void openExport()} type="button"><FileOutput aria-hidden="true" size={18} /></button></Tooltip><button className={styles.refresh} disabled={busy} onClick={() => void refresh()} type="button"><RefreshCw aria-hidden="true" size={17} />{t("usageRefresh")}</button></div></header>
    {busy && <p className={styles.status}>{t("usageLoading")}</p>}
    {error && <p className={styles.error}>{error}</p>}
    {!data && !busy && !error && <section className={styles.empty}><ChartNoAxesCombined aria-hidden="true" size={48} /><h2>{t("usageEmptyTitle")}</h2><p>{t("usageEmptyDescription")}</p></section>}
    {data && <section className={styles.workspace}>
      <p className={styles.checkedAt}>{t("usageCheckedAt", { time: new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(data.checkedAt * 1000)) })}</p>
      <div className={styles.summary}><section><span>{t("usagePrimary")}</span><strong>{metric(data.primary)}</strong><small>{Math.round(data.primary.percent)}%</small></section>{data.onDemand && <section><span>{t("usageOnDemand")}</span><strong>{metric(data.onDemand)}</strong><small>{Math.round(data.onDemand.percent)}%</small></section>}<section><span>{t("usageReset")}</span><strong>{data.resetAt ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(data.resetAt)) : t("usageUnknown")}</strong><small>{data.membershipType ?? t("usageUnknown")}</small></section></div>
      <section className={styles.section}><h2>{t("usageWeekly")}</h2>{data.weeklyAvailable ? <div className={styles.chart}>{data.weekly.map((day) => <div className={styles.day} key={day.date}><Tooltip content={day.isOnDemand ? money(day.onDemandCents) : `${Math.round(day.requests)} ${t("usageUnits")}`}><div aria-label={day.isOnDemand ? money(day.onDemandCents) : `${Math.round(day.requests)} ${t("usageUnits")}`} className={styles.barWrap}><i className={day.isOnDemand ? styles.onDemand : ""} style={{ height: `${Math.max(3, day.requests / max * 100)}%` }} /></div></Tooltip><span>{new Intl.DateTimeFormat(undefined, { weekday: "short" }).format(new Date(`${day.date}T12:00:00`))}</span></div>)}</div> : <p className={styles.muted}>{t("usageWeeklyUnavailable")}</p>}</section>
      <section className={styles.section}><h2>{t("usageModels")}</h2>{data.models.length ? <div className={styles.models}>{data.models.map((model) => <div key={model.name}><span>{model.name}</span><strong>{number.format(model.requests)}</strong></div>)}</div> : <p className={styles.muted}>{t("usageNoModels")}</p>}</section>
    </section>}
    {exportData !== undefined && <ExportDialog data={[exportData]} filename={`cursor-account-${accountId}.json`} onOpenChange={setExportOpen} open={exportOpen} />}
  </main>;
}

createRoot(document.getElementById("root")!).render(<UsagePage />);
