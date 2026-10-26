import type { CursorUsageDetails } from "../../lib/types";
import {
  calendarDaysUntil,
  formatDatedRelative,
  localDateKey,
  resolveInstant,
  type DatedRelativeLabel,
  type TranslateFn,
} from "../../lib/calendar";

export { localDateKey } from "../../lib/calendar";

export const money = (cents: number) => `$${(cents / 100).toFixed(2)}`;
export const number = new Intl.NumberFormat();

export function displayModel(name: string) {
  return name
    .split(/[-_]/g)
    .filter(Boolean)
    .map((part) => (/^[a-z]+$/.test(part) ? part[0].toUpperCase() + part.slice(1) : part))
    .join(" ");
}

export function hasLimit(value: { limit?: number | null }) {
  return value.limit != null && value.limit > 0;
}

export function metric(value: CursorUsageDetails["primary"] | NonNullable<CursorUsageDetails["onDemand"]>) {
  if (value.kind === "currency") {
    const limit = value.limit;
    return hasLimit(value) && limit != null ? `${money(value.used)} / ${money(limit)}` : money(value.used);
  }
  if (value.kind === "percent") return `${Math.round(value.percent)}%`;
  if (value.kind === "credits") {
    const limit = value.limit;
    if (hasLimit(value) && limit != null) {
      const remaining = Math.max(0, Math.round(limit - value.used));
      return `${number.format(remaining)} / ${number.format(Math.round(limit))}`;
    }
    return number.format(Math.round(value.used));
  }
  return number.format(value.used);
}

export function productParts(products?: { name: string; percent: number }[]) {
  if (!products?.length) return;
  return products.map((item) => `${item.name} ${Math.round(item.percent)}%`).join(" + ");
}

export function localHourKey(date = new Date()) {
  return `${localDateKey(date)}T${String(date.getHours()).padStart(2, "0")}:00`;
}

export function hourStartMs(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date.setMinutes(0, 0, 0);
}

export function hourEndMs(value: string) {
  const start = hourStartMs(value);
  return start === undefined ? undefined : start + 3_599_999;
}

/** Local calendar days until reset (not elapsed 24h buckets). */
export function daysUntil(iso?: string, nowMs?: number) {
  return calendarDaysUntil(iso, nowMs);
}

export function isOverLimit(value: { used: number; limit?: number | null; percent: number }) {
  const limit = value.limit;
  return hasLimit(value) && limit != null && value.used > limit;
}

export function formatTokens(input?: number, output?: number) {
  if (input === undefined && output === undefined) return;
  return `${number.format(input ?? 0)} / ${number.format(output ?? 0)}`;
}

export function spendCents(event: { chargedCents?: number; costUsd?: number }) {
  if (event.chargedCents !== undefined) return event.chargedCents;
  if (event.costUsd !== undefined) return event.costUsd * 100;
}

function appLocale() {
  try {
    return globalThis.localStorage?.getItem?.("language") ?? "zh";
  } catch {
    return "zh";
  }
}

/** Quota / billing reset label with absolute local date-time when available. */
export function resetDatedLabel(
  resetAt: string | undefined,
  t: TranslateFn,
  nowMs?: number,
): DatedRelativeLabel {
  return formatDatedRelative({
    instant: resolveInstant({ iso: resetAt }),
    nowMs,
    locale: appLocale(),
    t,
    kind: "reset",
  });
}
