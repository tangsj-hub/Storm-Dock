import {
  calendarDaysUntil,
  calendarDaysUntilUnix,
  formatDatedRelative,
  resolveInstant,
  type DatedRelativeLabel,
} from "../../../lib/calendar";
import type { Account } from "../../../lib/types";

export type Translate = (key: string, options?: Record<string, unknown>) => string;

function appLocale() {
  try {
    return globalThis.localStorage?.getItem?.("language") ?? "zh";
  } catch {
    return "zh";
  }
}

export function subscriptionPlanName(plan: string, t: Translate) {
  const normalized = plan.toLowerCase().replace(/[\s-]+/g, "_");
  return t(`subscriptionPlans.${normalized}`, { defaultValue: plan });
}

export function subscriptionPlanBadge(account: Account, t: Translate) {
  const plan = account.subscription.plan?.toLowerCase();
  if (!plan) return { name: t("subscriptionUnknownPlan"), plan: "unknown" };
  return {
    name: subscriptionPlanName(plan, t),
    plan,
  };
}

/** Prefer billingCycleEnd ISO, then expiresAt unix seconds. */
export function subscriptionInstant(account: Account) {
  return resolveInstant({
    iso: account.subscription.billingCycleEnd,
    unixSec: account.subscription.billingCycleEnd ? undefined : account.subscription.expiresAt,
  });
}

/** Live local-calendar days until subscription end for any provider. */
export function subscriptionRemainingDays(account: Account, nowMs: number = Date.now()): number | undefined {
  if (account.subscription.billingCycleEnd) {
    return calendarDaysUntil(account.subscription.billingCycleEnd, nowMs);
  }
  if (account.subscription.expiresAt != null) {
    return calendarDaysUntilUnix(account.subscription.expiresAt, nowMs);
  }
  return account.daysRemaining;
}

/** Codex stores rate-limit reset_at as expiresAt — label as quota reset, not subscription end. */
export function accountDateKind(account: Account): "subscription" | "quota" {
  return account.application === "codex" ? "quota" : "subscription";
}

export function subscriptionDatedLabel(
  account: Account,
  t: Translate,
  nowMs: number = Date.now(),
): DatedRelativeLabel {
  return formatDatedRelative({
    instant: subscriptionInstant(account),
    daysFallback: account.subscription.billingCycleEnd || account.subscription.expiresAt != null
      ? undefined
      : account.daysRemaining,
    nowMs,
    locale: appLocale(),
    t,
    kind: accountDateKind(account),
  });
}

export function subscriptionExpiryLabel(days: number | undefined, t: Translate) {
  if (days === undefined) return t("subscriptionUnknownExpiry");
  if (days === 1) return t("subscriptionTomorrow");
  if (days > 0) return t("subscriptionDays", { count: days });
  if (days === 0) return t("subscriptionToday");
  return t("subscriptionExpired");
}

export function subscriptionLabel(account: Account, t: Translate, nowMs?: number) {
  const plan = account.subscription.plan;
  if (!plan) {
    return {
      name: t("subscriptionUnknownPlan"),
      expiry: t("subscriptionUnknownExpiry"),
      expiryFull: t("subscriptionUnknownExpiry"),
      plan: "unknown" as const,
    };
  }
  const normalizedPlan = plan.toLowerCase();
  const name = subscriptionPlanName(plan, t);
  const dated = subscriptionDatedLabel(account, t, nowMs);
  return {
    name,
    expiry: dated.short,
    expiryFull: dated.full,
    expiryTitle: dated.absolute,
    plan: normalizedPlan,
  };
}

export function usageLabel(account: Account, t: Translate, nowMs?: number) {
  const resetPassed = account.application === "grok"
    && (grokBotResetDaysRemaining(account.resetAt, nowMs) ?? 0) < 0;
  let label: string | undefined;
  if (account.usage?.kind === "currency")
    label = t("usageSpent", { amount: `$${(account.usage.used / 100).toFixed(2)}` });
  else if (account.usage?.kind === "percent")
    label = t("usagePercent", { percent: resetPassed ? 0 : Math.round(account.usage.percent) });
  else if (account.subscription.plan?.toLowerCase() === "free")
    label = t("usageFree");
  const reset = account.application === "grok" ? grokBotResetLabel(account.resetAt, t, nowMs) : undefined;
  const text = label && reset ? `${label} · ${reset}` : label;
  const title = account.application === "grok"
    ? grokBotResetDated(account.resetAt, t, nowMs)?.absolute
    : undefined;
  return text ? { text, title } : undefined;
}

export function endpointHost(url?: string) {
  if (!url) return undefined;
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}

export function accountKindKey(account: Account) {
  return account.importType === "api_key" ? "accountKind.apiKey" : "accountKind.account";
}

/** Local calendar days until quota reset; negative means the reset time has already passed. */
export function grokBotResetDaysRemaining(resetAt: string | undefined, nowMs?: number) {
  return calendarDaysUntil(resetAt, nowMs);
}

export function grokBotResetDated(resetAt: string | undefined, t: Translate, nowMs?: number) {
  if (!resetAt) return undefined;
  const instant = resolveInstant({ iso: resetAt });
  if (!instant) return undefined;
  const dated = formatDatedRelative({
    instant,
    nowMs,
    locale: appLocale(),
    t,
    kind: "reset",
  });
  // Compact badge copy (今天 / 明天 / Nd) — full/absolute still carry the clock time.
  const days = dated.days;
  let short = dated.short;
  if (days === 0) short = t("grokBotResetToday");
  else if (days === 1) short = t("grokBotResetTomorrow");
  else if (days !== undefined && days > 1) short = t("grokBotResetDays", { count: days });
  else if (days !== undefined && days < 0) short = t("grokBotResetPassed");
  return { ...dated, short };
}

export function grokBotResetLabel(resetAt: string | undefined, t: Translate, nowMs?: number) {
  return grokBotResetDated(resetAt, t, nowMs)?.short;
}

export function isGrokBotFreePlan(account: Account) {
  return account.subscription.plan?.toLowerCase() === "free";
}

/** Exclude unknown subscription, free plan, expired token, banned, and missing-credential accounts from Grok Bot lists. */
export function isGrokBotListEligible(account: Account) {
  if (!account.subscription.plan) return false;
  if (isGrokBotFreePlan(account)) return false;
  if (account.status === "blocked" || account.status === "invalid" || account.status === "missing") return false;
  return true;
}

/** Cursor paid + any Grok Build account may launch Grok Bot. */
export function canLaunchGrokBot(account: Account) {
  if (account.application === "grok") return true;
  // After the grok early-return, application is narrowed to cursor | codex | undefined.
  if (account.application && account.application !== "cursor") return false;
  if (isGrokBotFreePlan(account)) return false;
  return true;
}

export function grokBotSourceKey(account: Account) {
  return account.application === "grok" ? "grok" : "cursor";
}

export function grokBotUsageLabel(account: Account, t: Translate, nowMs?: number) {
  const usage = account.grokBotUsage;
  if (!usage) return undefined;
  const dated = grokBotResetDated(account.grokBotResetAt, t, nowMs);
  const resetPassed = (dated?.days ?? 0) < 0;
  const percent = resetPassed ? 0 : Math.round(usage.percent);
  const reset = dated?.short;
  const text = reset ? t("grokBotUsageBadge", { percent, reset }) : t("grokBotUsagePercent", { percent });
  return { text, title: dated?.absolute };
}
