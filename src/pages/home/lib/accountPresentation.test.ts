import { describe, expect, it } from "vitest";
import type { Account } from "../../../lib/types";
import { subscriptionLabel, usageLabel } from "./accountPresentation";

const t = (key: string, options?: Record<string, unknown>) => `${key}:${options?.count ?? options?.amount ?? ""}`;
const account: Account = { id: "a", label: "A", importType: "oauth", subscription: { plan: "Pro", expiresAt: 1 }, daysRemaining: 2, isCurrent: false };

describe("account presentation", () => {
  it("formats subscription status from account data", () => {
    expect(subscriptionLabel(account, t)).toEqual({ name: "subscriptionPlans.pro:", expiry: "subscriptionDays:2", plan: "pro" });
    expect(subscriptionLabel({ ...account, daysRemaining: -1 }, t)?.expiry).toBe("subscriptionExpired:");
  });

  it("formats currency usage and free accounts", () => {
    expect(usageLabel({ ...account, usage: { kind: "currency", used: 123, percent: 1 } }, t)).toBe("usageSpent:$1.23");
    expect(usageLabel({ ...account, subscription: { plan: "Free" } }, t)).toBe("usageFree:");
  });
});
