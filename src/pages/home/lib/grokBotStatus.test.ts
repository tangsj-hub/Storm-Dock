import { describe, expect, it } from "vitest";
import type { GrokBotStatus } from "../../../lib/types";
import { grokBotAvailabilityLabel, grokBotSignInLabel } from "./grokBotStatus";

const t = (key: string, options?: Record<string, unknown>) =>
  `${key}${options?.account ? `:${options.account}` : ""}`;

const status = (overrides: Partial<GrokBotStatus> = {}): GrokBotStatus => ({
  installed: true,
  signedIn: true,
  running: false,
  available: true,
  ...overrides,
});

describe("grok bot status copy", () => {
  it("prefers a ready label when the local client is signed in", () => {
    expect(grokBotAvailabilityLabel(status(), t)).toBe("grokBotReady");
    expect(grokBotAvailabilityLabel(status({ available: false, reason: "未安装 Grok Bot。" }), t)).toBe("未安装 Grok Bot。");
    expect(grokBotAvailabilityLabel(status({ available: false, reason: "  " }), t)).toBe("grokBotUnavailable");
  });

  it("names the matching Cursor account when Grok Bot is using it", () => {
    expect(grokBotSignInLabel(status({ currentAccountLabel: "Pro desk" }), t)).toBe("grokBotSignedInAs:Pro desk");
    expect(grokBotSignInLabel(status({ signedIn: true }), t)).toBe("grokBotSignedIn");
    expect(grokBotSignInLabel(status({ signedIn: false, currentAccountLabel: undefined }), t)).toBe("grokBotNotSignedIn");
  });
});
