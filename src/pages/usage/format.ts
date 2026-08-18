import type { CursorUsageDetails } from "../../lib/types";

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
  return number.format(value.used);
}

export function localDateKey(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
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

export function daysUntil(iso?: string) {
  if (!iso) return;
  const reset = new Date(iso);
  if (Number.isNaN(reset.getTime())) return;
  const target = new Date(reset.getFullYear(), reset.getMonth(), reset.getDate()).getTime();
  const today = new Date();
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  return Math.round((target - start) / 86_400_000);
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
