import type { Account } from "../../../lib/types";

export type Translate = (key: string, options?: Record<string, unknown>) => string;

export function subscriptionLabel(account: Account, t: Translate) {
  const plan = account.subscription.plan;
  if (!plan) return undefined;
  const normalizedPlan = plan.toLowerCase();
  const name = t(`subscriptionPlans.${normalizedPlan}`, { defaultValue: plan });
  const days = account.daysRemaining;
  const expiry = !account.subscription.expiresAt || days === undefined
    ? t("subscriptionUnknownExpiry")
    : days > 0
      ? t("subscriptionDays", { count: days })
      : days === 0
        ? t("subscriptionToday")
        : t("subscriptionExpired");
  return { name, expiry, plan: normalizedPlan };
}

export function usageLabel(account: Account, t: Translate) {
  if (account.usage?.kind === "currency")
    return t("usageSpent", { amount: `$${(account.usage.used / 100).toFixed(2)}` });
  return account.subscription.plan?.toLowerCase() === "free" ? t("usageFree") : undefined;
}
