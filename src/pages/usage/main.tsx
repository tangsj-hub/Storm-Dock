import { ArrowLeft, ChartNoAxesCombined, FileOutput, LoaderCircle, RefreshCw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useLocalCalendarDay } from "../../lib/useLocalCalendarDay";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { ExportDialog } from "../../components/ExportDialog";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import toastStyles from "../../components/ToastMessage.module.css";
import { Tooltip } from "../../components/Tooltip";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import "../../i18n";
import { listAccounts } from "../../lib/api";
import { homeKindFromUsageQuery, homePath, syncDocumentAppKind, type Account, type CursorUsageBreakdown, type CursorUsageDetails } from "../../lib/types";
import { subscriptionDatedLabel, subscriptionLabel, subscriptionPlanName } from "../home/lib/accountPresentation";
import "../../styles/global.css";
import { creditGroupRows, credits, expirePoolLabel, hasEventSpend, hasLimit, isOverLimit, metric, money, number, productParts, resetDatedLabel } from "./format";
import { EventLedger } from "./EventLedger";
import { ModelBars } from "./ModelBars";
import { WeeklyChart } from "./WeeklyChart";
import styles from "./page.module.css";

function membershipLabel(type?: string, t?: (key: string, options?: Record<string, unknown>) => string) {
  if (!type || !t) return;
  return subscriptionPlanName(type, t);
}

function usageUsedCopy(
  label: string,
  value: CursorUsageDetails["primary"] | NonNullable<CursorUsageDetails["onDemand"]> | NonNullable<CursorUsageDetails["grokBot"]>,
  t: (key: string, options?: Record<string, unknown>) => string,
  parts?: string,
) {
  const amount = metric(value);
  if (parts) return t("usageUsedWithParts", { label, amount, parts });
  if (value.kind === "currency" && hasLimit(value)) {
    return t("usageUsedWithLimit", { label, amount, percent: Math.round(Math.max(value.percent, 0)) });
  }
  if (value.kind === "currency" || value.kind === "percent" || value.kind === "credits") {
    return t("usageUsed", { label, amount });
  }
  return `${label} ${amount}`;
}

/** Stable backend error codes → i18n; unknown errors fall back to the raw message. */
const USAGE_ERROR_CODES = new Set(["usage.cycleMissing", "usage.cycleUnsupported", "usage.cycleBusy"]);
function usageErrorText(message: string, t: (key: string) => string) {
  return USAGE_ERROR_CODES.has(message) ? t(message) : message;
}

function cycleEndText(iso?: string | null) {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

const pctText = (value?: number | null) => (value == null ? "—" : `${Math.round(value)}%`);
const centsText = (value?: number | null) => (value == null ? "—" : money(value));
const tokenText = (value?: number | null) => (value == null ? "—" : number.format(Math.round(value)));

function CursorBreakdownView({ b, membership, daysLeft, grokBot, grokBotResetAt }: {
  b: CursorUsageBreakdown;
  grokBot?: CursorUsageDetails["grokBot"];
  grokBotResetAt?: string;
  membership?: string;
  daysLeft?: number;
}) {
  const { t } = useTranslation();
  const planUsed = b.planUsed ?? undefined;
  const planLimit = b.planLimit ?? undefined;
  const planPercent = planUsed !== undefined && planLimit ? (planUsed / planLimit) * 100 : undefined;
  const exceeded = b.overage > 0 || (planPercent ?? 0) >= 100;
  return <div className={styles.breakdown}>
    <div className={styles.statCard}>
      <div className={styles.stat}><span>{t("usageInputTokens")}</span><strong>{tokenText(b.inputTokens)}</strong></div>
      <div className={styles.stat}><span>{t("usageOutputTokens")}</span><strong>{tokenText(b.outputTokens)}</strong></div>
      <div className={styles.stat}><span>{t("usageMembership")}</span><strong className={styles.statPlan}>{membership ?? "—"}</strong></div>
      <div className={styles.stat}><span>{t("usageDaysLeft")}</span><strong className={styles.statDays}>{daysLeft === undefined ? "—" : t("usageDaysCount", { count: Math.max(daysLeft, 0) })}</strong></div>
      {grokBot && <div className={styles.stat} title={grokBotResetAt ? resetDatedLabel(grokBotResetAt, t).absolute : undefined}>
        <span>{t("usageGrokBot")}</span>
        <strong>{metric(grokBot)}</strong>
        {grokBotResetAt && <small className={styles.statSub}>{resetFull(grokBotResetAt, t)}</small>}
      </div>}
    </div>
    {planPercent !== undefined && <>
      <div className={styles.planBar} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(planPercent)}>
        <span className={exceeded ? styles.planBarOver : undefined} style={{ width: `${Math.min(Math.max(planPercent, 0), 100)}%` }} />
      </div>
      <p className={styles.planCaption}>
        {money(planUsed!)} / {money(planLimit!)} ({Math.round(planPercent)}%)
        {b.overage > 0 && <span className={styles.planOverage}> · {t("usageOverage", { amount: money(b.overage) })}</span>}
      </p>
    </>}
    <div className={styles.poolCard}>
      <div className={styles.poolCol}>
        <div className={styles.poolRow}><span>{t("usagePoolAuto")}</span><strong>{pctText(b.autoPercent)}</strong></div>
        <div className={styles.poolRow}><span>{t("usagePoolBonus")}</span><strong>{centsText(b.bonus)}</strong></div>
        <div className={styles.poolRow}><span>{t("usageCycleEnd")}</span><strong>{cycleEndText(b.cycleEnd)}</strong></div>
      </div>
      <div className={styles.poolCol}>
        <div className={styles.poolRow}><span>{t("usagePoolApi")}</span><strong>{pctText(b.apiPercent)}</strong></div>
        <div className={styles.poolRow}><span>{t("usagePoolOnDemand")}</span><strong>{centsText(b.onDemandUsed)} / {centsText(b.onDemandLimit)}</strong></div>
      </div>
      {b.planTotal != null && <div className={styles.poolTotal}><span>{t("usagePlanTotal")}</span><strong>{b.planTotalLimit != null ? t("usagePlanTotalEstimate", { used: money(b.planTotal), limit: money(b.planTotalLimit), percent: Math.round((b.planTotal / b.planTotalLimit) * 100) }) : money(b.planTotal)}</strong></div>}
    </div>
  </div>;
}

function WorkbuddyBreakdownView({
  primary,
  membership,
  products,
  resetAt,
}: {
  primary: CursorUsageDetails["primary"];
  membership?: string;
  products?: CursorUsageDetails["products"];
  resetAt?: string;
}) {
  const { t } = useTranslation();
  const groups = creditGroupRows(products);
  const limit = primary.kind === "credits" && hasLimit(primary) ? primary.limit! : undefined;
  const used = primary.kind === "credits" ? primary.used : 0;
  const percent = limit != null && limit > 0
    ? (used / limit) * 100
    : Math.max(primary.percent, 0);
  const exceeded = isOverLimit(primary);
  const mid = Math.ceil(groups.length / 2);
  const left = groups.slice(0, mid);
  const right = groups.slice(mid);
  const resetLabel = resetAt ? resetDatedLabel(resetAt, t) : undefined;
  return <div className={styles.breakdown}>
    <div className={styles.statCard}>
      <div className={styles.stat}><span>{t("usageMembership")}</span><strong className={styles.statPlan}>{membership ?? "—"}</strong></div>
      <div className={styles.stat}><span>{t("usagePrimaryWorkbuddy")}</span><strong>{metric(primary)}</strong></div>
      {resetLabel && resetLabel.days !== undefined && resetLabel.days >= 0 && <div className={styles.stat} title={resetLabel.absolute}>
        <span>{t("usageDaysLeft")}</span>
        <strong className={styles.statDays}>{t("usageDaysCount", { count: Math.max(resetLabel.days, 0) })}</strong>
        {resetLabel.absolute && <small className={styles.statSub}>{resetLabel.full}</small>}
      </div>}
    </div>
    {limit != null && <>
      <div className={styles.planBar} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}>
        <span className={exceeded ? styles.planBarOver : undefined} style={{ width: `${Math.min(Math.max(percent, 0), 100)}%` }} />
      </div>
      <p className={styles.planCaption}>
        {credits(used)} / {credits(limit)} ({Math.round(percent)}%)
      </p>
    </>}
    {groups.length > 0 && <div className={styles.poolCard}>
      <div className={styles.poolCol}>
        {left.map((row) => {
          const expire = expirePoolLabel(row.expiresAt, t);
          return (
          <div className={styles.poolRow} key={row.labelKey}>
            <span className={styles.poolLabel}>
              {t(row.labelKey)}
              {expire && <small className={styles.poolSub} title={expire.absolute}>{expire.text}</small>}
            </span>
            <strong>{row.used > 0 ? `${credits(row.used)} / ${credits(row.limit)}` : credits(row.limit)}</strong>
          </div>
          );
        })}
      </div>
      <div className={styles.poolCol}>
        {right.map((row) => {
          const expire = expirePoolLabel(row.expiresAt, t);
          return (
          <div className={styles.poolRow} key={row.labelKey}>
            <span className={styles.poolLabel}>
              {t(row.labelKey)}
              {expire && <small className={styles.poolSub} title={expire.absolute}>{expire.text}</small>}
            </span>
            <strong>{row.used > 0 ? `${credits(row.used)} / ${credits(row.limit)}` : credits(row.limit)}</strong>
          </div>
          );
        })}
      </div>
      {limit != null && <div className={styles.poolTotal}>
        <span>{t("usagePlanTotal")}</span>
        <strong>{credits(used)} / {credits(limit)}（{Math.round(percent)}%）</strong>
      </div>}
    </div>}
  </div>;
}

function resetFull(resetAt: string | undefined, t: (key: string, options?: Record<string, unknown>) => string, nowMs?: number) {
  return resetDatedLabel(resetAt, t, nowMs).full;
}

function UsagePage() {
  const { t } = useTranslation();
  const dayKey = useLocalCalendarDay();
  const search = new URLSearchParams(window.location.search);
  const accountId = search.get("accountId") ?? "";
  const rawKind = search.get("kind");
  const usageKind = rawKind === "grok" || rawKind === "workbuddy" ? rawKind : "cursor";
  const returnHome = homeKindFromUsageQuery();
  const isGrok = usageKind === "grok";
  const isWorkbuddy = usageKind === "workbuddy";
  const isPoolUsage = isGrok || isWorkbuddy;
  const usageTitle = t(isWorkbuddy ? "usageTitleWorkbuddy" : isGrok ? "usageTitleGrok" : "usageTitle");
  const usageLoadingKey = isWorkbuddy ? "usageLoadingWorkbuddy" : isGrok ? "usageLoadingGrok" : "usageLoading";
  const primaryLabel = t(isWorkbuddy ? "usagePrimaryWorkbuddy" : isGrok ? "usagePrimaryGrok" : "usagePrimary");
  const onDemandLabel = t(isGrok ? "usageOnDemandGrok" : "usageOnDemand");
  const weeklyTitle = t(isWorkbuddy ? "usageWeeklyWorkbuddy" : isGrok ? "usageWeeklyGrok" : "usageWeekly");
  const modelsTitle = t(isWorkbuddy ? "usageModelsWorkbuddy" : isGrok ? "usageModelsGrok" : "usageModels");
  const noModelsCopy = t(isWorkbuddy ? "usageNoModelsWorkbuddy" : isGrok ? "usageNoModelsGrok" : "usageNoModels");
  const [data, setData] = useState<CursorUsageDetails>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [noticeStatus, setNoticeStatus] = useState<"loading" | "success" | "error">("success");
  const [justUpdated, setJustUpdated] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportData, setExportData] = useState<unknown>();
  const [account, setAccount] = useState<Account>();
  const [pullingFull, setPullingFull] = useState(false);
  const [tokensLoading, setTokensLoading] = useState(false);
  const tokensInFlight = useRef<string | undefined>(undefined);
  const accountStatus = account?.status;
  const flashTimer = useRef<number | undefined>(undefined);
  useEffect(() => { syncDocumentAppKind(returnHome); }, [returnHome]);
  useEffect(() => () => window.clearTimeout(flashTimer.current), []);
  useEffect(() => {
    let cancelled = false;
    setData(undefined);
    setError(undefined);
    setAccount(undefined);
    if (!accountId) return () => { cancelled = true; };
    void invoke<CursorUsageDetails | null>(
      usageKind === "grok"
        ? "get_saved_grok_usage"
        : usageKind === "workbuddy"
          ? "get_saved_workbuddy_usage"
          : "get_saved_cursor_usage",
      { id: accountId },
    )
      .then((usage) => {
        if (!cancelled && usage?.accountId === accountId) setData(usage);
      })
      .catch((error) => {
        if (!cancelled) setError(error instanceof Error ? error.message : String(error));
      });
    void listAccounts(usageKind)
      .then((accounts) => {
        if (cancelled) return;
        setAccount(accounts.find((item) => item.id === accountId));
      })
      .catch(() => {
        /* account badge is optional */
      });
    return () => { cancelled = true; };
  }, [accountId, usageKind]);
  const refresh = async () => {
    if (!accountId || busy) { if (!accountId) setError(t("usageUnknown")); return; }
    setBusy(true);
    setError(undefined);
    setNoticeStatus("loading");
    setNotice(t(usageLoadingKey));
    try {
      const usage = await invoke<CursorUsageDetails>(
        usageKind === "grok"
          ? "get_grok_usage"
          : usageKind === "workbuddy"
            ? "get_workbuddy_usage"
            : "get_cursor_usage",
        { id: accountId },
      );
      if (usage.accountId !== accountId) throw new Error(t("usageAccountMismatch"));
      setData(usage);
      if (usage.breakdown && !(usage.breakdown.tokensComplete && usage.breakdown.tokensCachedAt)) void loadCycleTokens(accountId);
      setAccount(undefined);
      void listAccounts(usageKind).then((accounts) => {
        setAccount(accounts.find((item) => item.id === accountId));
      }).catch(() => undefined);
      setNoticeStatus("success");
      setNotice(t("usageRefreshed"));
      setJustUpdated(true);
      window.clearTimeout(flashTimer.current);
      flashTimer.current = window.setTimeout(() => setJustUpdated(false), 1400);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const lower = message.toLowerCase();
      if (lower.includes("user account is blocked") || message.includes("账号已被封禁") || message.includes("账号已封禁")) {
        setAccount((current) => current ? { ...current, status: "blocked" } : current);
      } else if (message.includes("失效") || message.includes("过期")) {
        setAccount((current) => current && current.status !== "blocked" ? { ...current, status: "invalid" } : current);
      }
      setError(message);
      setNoticeStatus("error");
      setNotice(message);
    } finally {
      setBusy(false);
    }
  };
  // Background default pass (≤5 pages) after a refresh; one per account at a time.
  const loadCycleTokens = async (id: string) => {
    if (tokensInFlight.current === id) return;
    tokensInFlight.current = id;
    setTokensLoading(true);
    try {
      const usage = await invoke<CursorUsageDetails>("get_cursor_cycle_tokens", { id, full: false });
      if (usage.accountId === id) setData((current) => (current?.accountId === id ? usage : current));
    } catch {
      /* token stats are optional; keep the current figures */
    } finally {
      if (tokensInFlight.current === id) tokensInFlight.current = undefined;
      setTokensLoading(false);
    }
  };
  const pullFullCycle = async () => {
    if (!accountId || pullingFull || tokensLoading) return;
    setPullingFull(true);
    setNoticeStatus("loading");
    setNotice(t("usagePullFullCycleLoading"));
    try {
      const usage = await invoke<CursorUsageDetails>("get_cursor_cycle_tokens", { id: accountId, full: true });
      if (usage.accountId !== accountId) throw new Error(t("usageAccountMismatch"));
      setData(usage);
      setNoticeStatus(usage.breakdown?.tokensComplete ? "success" : "error");
      setNotice(t(usage.breakdown?.tokensComplete ? "usagePullFullCycleDone" : "usagePullFullCycleCapped"));
    } catch (error) {
      setNoticeStatus("error");
      setNotice(usageErrorText(error instanceof Error ? error.message : String(error), t));
    } finally {
      setPullingFull(false);
    }
  };
  const openExport = async () => {
    if (!accountId) return;
    try { setExportData(await invoke<unknown>(usageKind === "grok" ? "get_grok_bot_export_record" : "get_cursor_export_record", { id: accountId })); setExportOpen(true); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); }
  };
  const membership = membershipLabel(data?.membershipType, t);
  const resetLabel = data?.resetAt ? resetDatedLabel(data.resetAt, t) : undefined;
  const subscriptionDated = account && !isWorkbuddy ? subscriptionDatedLabel(account, t) : undefined;
  const workbuddyExpiryBadge = (() => {
    if (!account || !isWorkbuddy) return;
    const label = subscriptionLabel(account, t);
    const text = label.expiryFull;
    if (!text || text === t("subscriptionUnknownExpiry")) return;
    // Align with home list: credits remaining (or real calendar end), never fake "已过期".
    return { text, title: label.expiryTitle };
  })();
  void dayKey; // recompute relative labels after local midnight
  const checkedAt = data ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(data.checkedAt * 1000)) : undefined;

  return <Toast.Provider>
    <main className={styles.shell}>
    <WindowDragSurface />
    <header className={styles.header}>
      <a aria-label={t("back")} className={styles.back} href={homePath(returnHome)}><ArrowLeft aria-hidden="true" size={20} /></a>
      <h1>{usageTitle}</h1>
      <div className={styles.actions}>
        <Tooltip content={t("export")}><button aria-label={t("export")} className={styles.export} disabled={busy} onClick={() => void openExport()} type="button"><FileOutput aria-hidden="true" size={18} /></button></Tooltip>
        <button aria-busy={busy} aria-label={busy ? t(usageLoadingKey) : t("usageRefresh")} className={`${styles.refresh} ${busy ? styles.refreshBusy : ""}`} onClick={() => void refresh()} type="button">
          {busy ? <LoaderCircle aria-hidden="true" className={styles.spinning} size={17} /> : <RefreshCw aria-hidden="true" size={17} />}
          {t("usageRefresh")}
        </button>
      </div>
    </header>
    {error && !(notice && noticeStatus === "error") && <p className={styles.error}>{error}</p>}
    {!data && accountStatus === "blocked" && <p className={styles.blockedBanner}>{t("usageAccountBlocked", { defaultValue: "此账号已被封禁，无法刷新用量。请更换账号或联系服务方。" })}</p>}
    {!data && !busy && !error && <section className={styles.empty}><ChartNoAxesCombined aria-hidden="true" size={48} /><h2>{t("usageEmptyTitle")}</h2><p>{t("usageEmptyDescription")}</p></section>}
    {!data && busy && <section className={styles.empty}><LoaderCircle aria-hidden="true" className={styles.spinning} size={28} /><h2>{t(usageLoadingKey)}</h2></section>}
    {data && <section className={styles.workspace}>
      {busy && <div aria-hidden="true" className={styles.indeterminate}><span /></div>}
      <div className={styles.identity}>
        <strong>{data.email ?? data.label}</strong>
        {membership && <span className={styles.badge}>{membership}</span>}
        {workbuddyExpiryBadge && (
          <span className={styles.badge} title={workbuddyExpiryBadge.title}>{workbuddyExpiryBadge.text}</span>
        )}
        {!isWorkbuddy && subscriptionDated && subscriptionDated.days !== undefined && (
          <span className={styles.badge} title={subscriptionDated.absolute}>
            {account?.application === "codex"
              ? subscriptionDated.full
              : subscriptionDated.absolute
                ? t("usageSubscriptionEnd", { label: subscriptionDated.full })
                : subscriptionDated.full}
          </span>
        )}
        {accountStatus === "blocked" && <span className={styles.blockedBadge}>{t("tokenBlocked", { defaultValue: "账号已封禁" })}</span>}
        {accountStatus === "invalid" && <span className={styles.invalidBadge}>{t("tokenInvalid", { defaultValue: "Token已失效" })}</span>}
        {accountStatus === "missing" && <span className={styles.invalidBadge}>{t("credentialMissing", { defaultValue: "凭证缺失" })}</span>}
      </div>
      {accountStatus === "blocked" && <p className={styles.blockedBanner}>{t("usageAccountBlocked", { defaultValue: "此账号已被封禁，无法刷新用量。请更换账号或联系服务方。" })}</p>}
      {!isPoolUsage && data.breakdown && <CursorBreakdownView b={data.breakdown} daysLeft={resetLabel?.days} membership={membership} grokBot={data.grokBot} grokBotResetAt={data.grokBotResetAt} />}
      {isWorkbuddy && <WorkbuddyBreakdownView membership={membership} primary={data.primary} products={data.products} resetAt={data.resetAt} />}
      {!isWorkbuddy && <div className={styles.usageLines}>
        {(isGrok || !data.breakdown) && <><p className={isOverLimit(data.primary) ? `${styles.usageLine} ${styles.overLimit}` : styles.usageLine}>{usageUsedCopy(primaryLabel, data.primary, t, isGrok ? productParts(data.products) : undefined)}{isGrok && data.resetAt ? <span className={styles.poolReset} title={resetLabel?.absolute}>{t("usageResetParen", { reset: resetFull(data.resetAt, t) })}</span> : null}</p>
        {data.onDemand && <p className={isOverLimit(data.onDemand) ? `${styles.usageLine} ${styles.overLimit}` : styles.usageLine}>{usageUsedCopy(onDemandLabel, data.onDemand, t)}</p>}</>}
        {data.grokBot && (isGrok || !data.breakdown) && <p className={styles.usageLine}>{usageUsedCopy(t("usageGrokBot"), data.grokBot, t)}{data.grokBotResetAt && <span className={styles.poolReset} title={resetDatedLabel(data.grokBotResetAt, t).absolute}> · {resetFull(data.grokBotResetAt, t)}</span>}</p>}
      </div>}
      <div className={styles.charts}>
        <section><h2>{weeklyTitle}</h2>{data.weeklyAvailable ? <WeeklyChart days={data.weekly} events={data.events ?? []} /> : <p className={styles.muted}>{data.weeklyError ?? t("usageWeeklyUnavailable")}</p>}</section>
        <section><h2>{modelsTitle}</h2>{(data.events ?? []).some(hasEventSpend) ? <ModelBars events={data.events ?? []} models={data.models} /> : <p className={styles.muted}>{noModelsCopy}</p>}</section>
      </div>
      <EventLedger
        cycleTokens={!isPoolUsage && data.breakdown ? {
          loading: tokensLoading,
          pulling: pullingFull,
          complete: data.breakdown.tokensComplete,
          fetched: data.breakdown.tokensFetched,
          total: data.breakdown.tokensTotal,
          cachedAt: data.breakdown.tokensCachedAt,
          onPullFull: () => void pullFullCycle(),
        } : undefined}
        events={data.events ?? []}
        unavailable={data.weeklyError}
      />
      <div className={styles.meta}>
        {resetLabel && resetLabel.days !== undefined ? <span className={styles.reset} title={resetLabel.absolute}>{resetLabel.full}</span> : <span>{t("usageUnknown")}</span>}
        <span className={justUpdated ? styles.justUpdated : undefined}>{t("usageCheckedAt", { time: checkedAt })}</span>
      </div>
    </section>}
    {exportData !== undefined && <ExportDialog data={[exportData]} filename={`${usageKind}-account-${accountId}.json`} onOpenChange={setExportOpen} open={exportOpen} />}
  </main>
  <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={noticeStatus} />
  <Toast.Viewport className={toastStyles.viewport} />
  
  </Toast.Provider>;
}

syncDocumentAppKind(homeKindFromUsageQuery());
createRoot(document.getElementById("root")!).render(<UsagePage />);
