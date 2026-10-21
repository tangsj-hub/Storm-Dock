import { describe, expect, it } from "vitest";
import { confirmLabels, hoursLeft, offerMessage, percentText, providerSettings, withProviderSettings, type SwitchOffer } from "./accountSwitcher";

const t = (key: string, options?: Record<string, unknown>) => `${key}${options ? JSON.stringify(options) : ""}`;
const NOW = 1_800_000_000_000;

const offer = (patch: Partial<SwitchOffer> = {}): SwitchOffer => ({
  provider: "grokBot",
  providerName: "Grok Bot",
  reason: "lowQuota",
  severity: 0,
  current: { id: "a", label: "a@x.com", current: true, remainingPercent: 8.4, expiresAt: null },
  candidate: { id: "b", label: "b@x.com", current: false, remainingPercent: 70, expiresAt: null },
  ...patch,
});

describe("accountSwitcher", () => {
  it("formats percents", () => {
    expect(percentText(8.4)).toBe("8%");
    expect(percentText(-3)).toBe("0%");
    expect(percentText(undefined)).toBe("--");
  });

  it("builds the low-quota message", () => {
    expect(offerMessage(offer(), t, NOW)).toBe('switcherLowQuota{"percent":"8%"}switcherJoinswitcherTarget{"account":"b@x.com","percent":"70%"}');
  });

  it("builds the expiring and expired messages", () => {
    const expiring = offer({ reason: "expiring", current: { ...offer().current, expiresAt: NOW / 1000 + 2.5 * 3600 } });
    expect(offerMessage(expiring, t, NOW)).toContain('switcherExpiring{"count":3}');
    const expired = offer({ reason: "expiring", current: { ...offer().current, expiresAt: NOW / 1000 - 10 } });
    expect(offerMessage(expired, t, NOW)).toContain("switcherExpired");
    expect(hoursLeft(null)).toBeUndefined();
  });

  it("reuses the manual launch warning for the native confirm", () => {
    const labels = confirmLabels(offer(), t);
    expect(labels.message).toBe('grokBotSwitchDescription{"account":"b@x.com"}\ngrokBotSwitchWarning');
    expect(labels.ok).toBe("grokBotSwitchConfirm");
    expect(labels.cancel).toBe("cancel");
  });

  it("merges and clamps provider settings", () => {
    expect(providerSettings(undefined, "grokBot")).toEqual({ enabled: true, minRemainingPercent: 10, expiryWindowHours: 24 });
    const next = withProviderSettings({ providers: {} }, "grokBot", { minRemainingPercent: 150, expiryWindowHours: Number.NaN });
    expect(next.providers.grokBot).toEqual({ enabled: true, minRemainingPercent: 90, expiryWindowHours: 24 });
  });
});
