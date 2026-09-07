import { describe, expect, it } from "vitest";
import { resolveProviderLogo, splitModelRepo } from "./providerLogos";

describe("resolveProviderLogo", () => {
  it("prefers specific nvidia prefixes over meta Llama", () => {
    const logo = resolveProviderLogo("unsloth", "Llama-3.1-Nemotron-70B-Instruct");
    expect(logo?.id).toBe("nvidia");
  });

  it("prefers DeepSeek-R1-Distill over qwen family", () => {
    const logo = resolveProviderLogo("unsloth", "DeepSeek-R1-Distill-Qwen-32B");
    expect(logo?.id).toBe("deepseek");
  });

  it("matches owner when prefixes miss", () => {
    expect(resolveProviderLogo("meta-llama", "custom-finetune")?.id).toBe("meta");
    expect(resolveProviderLogo("Qwen", "something-else")?.id).toBe("qwen");
    expect(resolveProviderLogo("moonshotai", "custom")?.id).toBe("moonshot");
  });

  it("matches google gemma stem", () => {
    expect(resolveProviderLogo("someone", "my-gemma-ft")?.id).toBe("google");
  });

  it("returns null for unknown orgs", () => {
    expect(resolveProviderLogo("random-org", "cool-model-v1")).toBeNull();
  });
});

describe("splitModelRepo", () => {
  it("splits owner/name", () => {
    expect(splitModelRepo("Qwen/Qwen2.5-7B")).toEqual({ owner: "Qwen", repoName: "Qwen2.5-7B" });
  });
});
