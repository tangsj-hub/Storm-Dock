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

export function metric(value: CursorUsageDetails["primary"] | NonNullable<CursorUsageDetails["onDemand"]>) {
  if (value.kind === "currency") return value.limit === undefined ? money(value.used) : `${money(value.used)} / ${money(value.limit)}`;
  if (value.kind === "percent") return `${Math.round(value.percent)}%`;
  return number.format(value.used);
}

export function localDateKey(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
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

export function isOverLimit(value: { used: number; limit?: number; percent: number }) {
  return value.limit !== undefined && value.used > value.limit;
}
