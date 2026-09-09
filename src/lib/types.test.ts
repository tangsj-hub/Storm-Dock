import { describe, expect, it } from "vitest";
import { APPLICATION_KINDS } from "./types";

describe("application kinds", () => {
  it("keeps app switcher limited to account hosts", () => {
    expect(APPLICATION_KINDS).toEqual(["cursor", "codex", "grok"]);
  });
});
