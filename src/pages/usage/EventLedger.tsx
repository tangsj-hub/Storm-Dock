import { LoaderCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { UsageEvent } from "../../lib/types";
import { DateTimePicker } from "./DateTimePicker";
import { displayModel, eventSpend, formatSpend, formatTokens, hourEndMs, hourStartMs, localHourKey, number } from "./format";
import styles from "./EventLedger.module.css";

const timeFormat = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

function rangeBounds(events: UsageEvent[]) {
  if (!events.length) return { min: "", max: "" };
  const hours = events.map((event) => localHourKey(new Date(event.timestamp)));
  return { min: hours.reduce((left, right) => (left < right ? left : right)), max: hours.reduce((left, right) => (left > right ? left : right)) };
}

function lastWeekRange(max: string) {
  const end = max ? new Date(max) : new Date();
  const start = new Date(end);
  start.setDate(end.getDate() - 6);
  start.setHours(0, 0, 0, 0);
  return { from: localHourKey(start), to: localHourKey(end) };
}

function eventCost(event: UsageEvent) {
  const spend = eventSpend(event);
  return spend === undefined ? undefined : formatSpend(spend);
}

export type CycleTokensStatus = {
  loading: boolean;
  pulling: boolean;
  complete: boolean;
  fetched: number;
  total?: number | null;
  cachedAt?: number | null;
  onPullFull: () => void;
};

function CycleTokensBar({ status }: { status: CycleTokensStatus }) {
  const { t } = useTranslation();
  if (status.loading) {
    return <div className={styles.cycleTokens}>
      <LoaderCircle aria-hidden="true" className={styles.spinning} size={12} />
      <span>{t("usageTokensLoading")}</span>
    </div>;
  }
  const note = status.complete
    ? status.cachedAt ? t("usageTokensCached", { count: status.fetched }) : undefined
    : status.total != null
      ? t("usageTokensPartialOf", { fetched: status.fetched, total: status.total })
      : t("usageTokensPartial", { fetched: status.fetched });
  if (!note && status.complete) return null;
  return <div className={styles.cycleTokens}>
    {note && <span className={status.complete ? undefined : styles.cycleTokensPartial}>{note}</span>}
    {!status.complete && <button className={styles.cycleTokensPull} disabled={status.pulling} onClick={status.onPullFull} type="button">
      {status.pulling ? <LoaderCircle aria-hidden="true" className={styles.spinning} size={12} /> : null}
      {status.pulling ? t("usagePullingFullCycle") : t("usagePullFullCycle")}
    </button>}
  </div>;
}

export function EventLedger({ events, unavailable, cycleTokens }: {
  events: UsageEvent[];
  unavailable?: string;
  cycleTokens?: CycleTokensStatus;
}) {
  const { t } = useTranslation();
  const bounds = useMemo(() => rangeBounds(events), [events]);
  const [from, setFrom] = useState(bounds.min);
  const [to, setTo] = useState(bounds.max);
  useEffect(() => {
    setFrom(bounds.min);
    setTo(bounds.max);
  }, [bounds.min, bounds.max]);
  const filtered = useMemo(() => {
    const start = hourStartMs(from);
    const end = hourEndMs(to);
    if (start === undefined || end === undefined) return events;
    return events.filter((event) => event.timestamp >= start && event.timestamp <= end);
  }, [events, from, to]);
  const stats = useMemo(() => {
    let input = 0;
    let output = 0;
    let hasTokens = false;
    let amount = 0;
    let unit: "credits" | "currency" | undefined;
    for (const event of filtered) {
      if (event.inputTokens !== undefined || event.outputTokens !== undefined) {
        hasTokens = true;
        input += event.inputTokens ?? 0;
        output += event.outputTokens ?? 0;
      }
      const spend = eventSpend(event);
      if (spend !== undefined) {
        amount += spend.amount;
        unit = spend.unit;
      }
    }
    return {
      tokens: hasTokens ? formatTokens(input, output) : undefined,
      cost: unit ? formatSpend({ amount, unit }) : undefined
    };
  }, [filtered]);
  const week = lastWeekRange(bounds.max);
  const preset = from === bounds.min && to === bounds.max ? "all" : from === week.from && to === week.to ? "week" : "custom";
  const applyRange = (nextFrom: string, nextTo: string) => {
    const start = nextFrom ? localHourKey(new Date(nextFrom)) : nextFrom;
    const end = nextTo ? localHourKey(new Date(nextTo)) : nextTo;
    if (start && end && start > end) {
      setFrom(end);
      setTo(start);
      return;
    }
    setFrom(start);
    setTo(end);
  };
  if (!events.length) {
    return <section className={styles.ledger}>
      <h2>{t("usageEvents")}</h2>
      {cycleTokens && <CycleTokensBar status={cycleTokens} />}
      <p className={styles.empty}>{unavailable ?? t("usageEventsUnavailable")}</p>
    </section>;
  }
  return <section className={styles.ledger}>
    <div className={styles.toolbar}>
      <div>
        <h2>{t("usageEvents")}</h2>
        {cycleTokens && <CycleTokensBar status={cycleTokens} />}
      </div>
      <div className={styles.filters}>
        <div className={styles.presets}>
          <button className={preset === "week" ? styles.presetActive : styles.preset} onClick={() => applyRange(week.from, week.to)} type="button">{t("usageRangeWeek")}</button>
          <button className={preset === "all" ? styles.presetActive : styles.preset} onClick={() => applyRange(bounds.min, bounds.max)} type="button">{t("usageRangeAll")}</button>
        </div>
        <DateTimePicker label={t("usageRangeFrom")} max={bounds.max} min={bounds.min} onChange={(value) => applyRange(value, to)} value={from} />
        <DateTimePicker label={t("usageRangeTo")} max={bounds.max} min={bounds.min} onChange={(value) => applyRange(from, value)} value={to} />
      </div>
    </div>
    <dl className={styles.stats}>
      <div><dt>{t("usageStatCalls")}</dt><dd>{number.format(filtered.length)}</dd></div>
      <div><dt>{t("usageStatTokens")}</dt><dd>{stats.tokens ?? t("usageUnknown")}</dd></div>
      <div><dt>{t("usageStatCost")}</dt><dd>{stats.cost ?? t("usageUnknown")}</dd></div>
    </dl>
    {filtered.length ? <div className={styles.tableWrap}>
      <table className={styles.table}>
        <thead>
          <tr>
            <th>{t("usageEventTime")}</th>
            <th>{t("usageEventModel")}</th>
            <th>{t("usageEventTokens")}</th>
            <th>{t("usageEventCost")}</th>
            <th>{t("usageEventKind")}</th>
          </tr>
        </thead>
        <tbody>
          {filtered.map((event, index) => <tr key={`${event.timestamp}-${event.model ?? "model"}-${index}`}>
            <td>{timeFormat.format(new Date(event.timestamp))}</td>
            <td title={event.model}>{event.model ? displayModel(event.model) : t("usageUnknown")}</td>
            <td>{formatTokens(event.inputTokens, event.outputTokens) ?? t("usageUnknown")}</td>
            <td>{eventCost(event) ?? t("usageUnknown")}</td>
            <td>{event.onDemand ? t("usageEventOnDemand") : t("usageEventIncluded")}</td>
          </tr>)}
        </tbody>
      </table>
    </div> : <p className={styles.empty}>{t("usageEventsEmpty")}</p>}
  </section>;
}
