import { describe, expect, it } from "vitest";
import type { UpdateCheckResult } from "./updater";
import {
  rememberDismissedStartupUpdate,
  readDismissedStartupUpdateVersion,
  shouldPromptStartupUpdate
} from "./startupUpdate";

const available = (version: string): UpdateCheckResult => ({ status: "available", version });

describe("shouldPromptStartupUpdate", () => {
  it("does not prompt when the app is already up to date", () => {
    expect(shouldPromptStartupUpdate({ result: { status: "up-to-date" } })).toBe(false);
  });

  it("prompts when a newer version is available", () => {
    expect(shouldPromptStartupUpdate({ result: available("1.5.2") })).toBe(true);
  });

  it("does not prompt again for a version the user already dismissed", () => {
    expect(
      shouldPromptStartupUpdate({ result: available("1.5.2"), dismissedVersion: "1.5.2" })
    ).toBe(false);
  });

  it("prompts again when a newer version arrives after a dismissal", () => {
    expect(
      shouldPromptStartupUpdate({ result: available("1.6.0"), dismissedVersion: "1.5.2" })
    ).toBe(true);
  });
});

describe("dismissed startup update version", () => {
  it("round-trips the skipped version through storage", () => {
    const store = new Map<string, string>();
    const storage = {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => {
        store.set(key, value);
      }
    };

    expect(readDismissedStartupUpdateVersion(storage)).toBeUndefined();
    rememberDismissedStartupUpdate("1.5.2", storage);
    expect(readDismissedStartupUpdateVersion(storage)).toBe("1.5.2");
  });
});
