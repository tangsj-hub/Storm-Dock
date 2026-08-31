import { describe, expect, it } from "vitest";
import type { Account } from "../../../lib/types";
import { subscriptionLabel, usageLabel, endpointHost, accountKindKey } from "./accountPresentation";

const t = (key: string, options?: Record<string, unknown>) =>
  `${key}:${options?.count ?? options?.amount ?? options?.percent ?? ""}`;
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

  it("formats percent usage and endpoint hosts", () => {
    expect(usageLabel({ ...account, usage: { kind: "percent", used: 12, percent: 12 } }, t)).toBe("usagePercent:12");
    expect(endpointHost("https://api.example.com/v1")).toBe("api.example.com");
  });

  it("maps ChatGPT import types to sign-in vs API Key", () => {
    expect(accountKindKey(account)).toBe("accountKind.account");
    expect(accountKindKey({ ...account, importType: "api_key" })).toBe("accountKind.apiKey");
  });
});
