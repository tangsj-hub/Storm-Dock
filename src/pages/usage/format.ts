import type { CursorUsageDetails } from "../../lib/types";
import {
  calendarDaysUntil,
  formatDatedRelative,
  formatLocalDateTime,
  localDateKey,
  resolveInstant,
  type DatedRelativeLabel,
  type TranslateFn,
} from "../../lib/calendar";

export { localDateKey } from "../../lib/calendar";

export const money = (cents: number) => `$${(cents / 100).toFixed(2)}`;
export const number = new Intl.NumberFormat();
/**
 * WorkBuddy credit amounts (API is f64).
 * Up to 2 fraction digits; drop trailing zeros so integers show as `2,000` not `2,000.00`.
 */
export const credits = (value: number) =>
  new Intl.NumberFormat(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 2 }).format(value);

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
    // used is consumed credits (CycleUsedCapacity sum); show 已使用 / 总额度 — never remaining.
    if (hasLimit(value) && limit != null) {
      return `${credits(value.used)} / ${credits(limit)}`;
    }
    return credits(value.used);
  }
  return number.format(value.used);
}

export type ProductShare = {
  name: string;
  percent: number;
  used?: number;
  limit?: number;
  expiresAt?: string;
};

/** Grok-style percent shares, or WorkBuddy absolute credit capacities when limit is set. */
export function productParts(products?: ProductShare[]) {
  if (!products?.length) return;
  const absolute = products.some((item) => item.limit != null);
  if (absolute) {
    return products
      .map((item) => `${item.name} ${credits(item.limit ?? 0)}`)
      .join(" + ");
  }
  return products.map((item) => `${item.name} ${Math.round(item.percent)}%`).join(" + ");
}

/** Short i18n keys for WorkBuddy CreditGroup API names (Cursor-style pool rows). */
const WORKBUDDY_POOLS: { names: string[]; labelKey: string; always: boolean }[] = [
  { names: ["套餐基础积分"], labelKey: "usagePoolBase", always: true },
  { names: ["平台奖励积分"], labelKey: "usagePoolReward", always: true },
  { names: ["购买积分"], labelKey: "usagePoolPurchase", always: true },
  { names: ["套餐赠送积分"], labelKey: "usagePoolGift", always: true },
];

export type CreditGroupRow = {
  labelKey: string;
  limit: number;
  used: number;
  expiresAt?: string;
};

/**
 * WorkBuddy CreditGroups as Cursor-style pool rows (limit = CycleTotalCapacity).
 * Always emits 基础积分 / 奖励积分 / 购买积分 / 赠送积分 (zeros when missing).
 * Percent-only Grok shares return [].
 */
export function creditGroupRows(products?: ProductShare[]): CreditGroupRow[] {
  if (products?.length && !products.some((item) => item.limit != null)) return [];
  const byName = new Map((products ?? []).map((item) => [item.name, item]));
  const rows: CreditGroupRow[] = [];
  for (const pool of WORKBUDDY_POOLS) {
    const found = pool.names.map((name) => byName.get(name)).find(Boolean);
    if (!found && !pool.always) continue;
    rows.push({
      labelKey: pool.labelKey,
      limit: found?.limit ?? 0,
      used: found?.used ?? 0,
      expiresAt: found?.expiresAt,
    });
  }
  return rows;
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

export type EventSpend = { amount: number; unit: "credits" | "currency" };

/** WorkBuddy credits take priority; Cursor/Grok stay on chargedCents/costUsd → currency. */
export function eventSpend(event: {
  credits?: number;
  chargedCents?: number;
  costUsd?: number;
}): EventSpend | undefined {
  if (event.credits !== undefined) return { amount: event.credits, unit: "credits" };
  const cents = spendCents(event);
  if (cents !== undefined) return { amount: cents, unit: "currency" };
}

export function formatSpend(spend: EventSpend) {
  return spend.unit === "credits" ? credits(spend.amount) : money(spend.amount);
}

export function hasEventSpend(event: {
  credits?: number;
  chargedCents?: number;
  costUsd?: number;
}) {
  return eventSpend(event) !== undefined;
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

/** Per-pool expiry line for WorkBuddy reward / gift / purchased credits. */
export function expirePoolLabel(
  expiresAt: string | undefined,
  t: TranslateFn,
  nowMs?: number,
): { text: string; absolute?: string } | undefined {
  const instant = resolveInstant({ iso: expiresAt });
  if (!instant) return;
  const absolute = formatLocalDateTime(instant, { locale: appLocale(), nowMs });
  return {
    text: t("usageExpiresAt", { time: absolute }),
    absolute,
  };
}
