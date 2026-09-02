import { describe, expect, it } from "vitest";
import { APPLICATION_KINDS, homeModeFromQuery, modelDetailPath, modelsHomePath } from "./types";

describe("application kinds", () => {
  it("keeps Model Center out of the app switcher", () => {
    expect(APPLICATION_KINDS).toEqual(["cursor", "codex", "grok"]);
  });
});

describe("model center paths", () => {
  it("reads models mode from the query string", () => {
    expect(homeModeFromQuery("?kind=cursor")).toBe("apps");
    expect(homeModeFromQuery("?models=1")).toBe("models");
    expect(modelsHomePath("done")).toBe("/?models=1&notice=done");
  });

  it("builds a model detail path", () => {
    expect(modelDetailPath("huggingface", "Qwen/Qwen2.5-7B")).toBe("/model-detail.html?source=huggingface&repo=Qwen%2FQwen2.5-7B");
  });
});
