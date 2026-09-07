import { describe, expect, it } from "vitest";
import { prepareReadmeMarkdown, stripReadmeDialects } from "./modelReadme";

describe("prepareReadmeMarkdown", () => {
  it("normalizes linked HTML badges to markdown without leaving raw anchors", () => {
    const raw = `
<p align="center">
<a href="https://www.deepseek.com/"><img alt="logo" src="https://example.com/badge.svg"/></a>
</p>
`;
    const out = prepareReadmeMarkdown(raw);
    expect(out).toContain("[![logo](https://example.com/badge.svg)](https://www.deepseek.com/)");
    expect(out).not.toContain("<a href");
    expect(out).not.toContain("<p");
  });

  it("converts plain HTML anchors into markdown links", () => {
    const out = prepareReadmeMarkdown('<a href="https://www.qwencloud.com/">Qwen Cloud</a>');
    expect(out).toContain("[Qwen Cloud](https://www.qwencloud.com/)");
  });
});

describe("stripReadmeDialects", () => {
  it("removes alert markers and keeps body text only", () => {
    const out = stripReadmeDialects(`[!Note] This repository contains weights.

[!Tip] Use Qwen Cloud for managed inference.
`);
    expect(out).toContain("This repository contains weights.");
    expect(out).toContain("Use Qwen Cloud for managed inference.");
    expect(out).not.toContain("[!Note]");
    expect(out).not.toContain("[!Tip]");
    expect(out).not.toContain("data-alert");
    expect(out).not.toContain("readmeAlert");
  });

  it("flattens blockquote GitHub alerts to plain text", () => {
    const out = stripReadmeDialects(`> [!WARNING]
> Be careful
> with this
`);
    expect(out).toContain("Be careful");
    expect(out).toContain("with this");
    expect(out).not.toContain("[!WARNING]");
    expect(out).not.toMatch(/^\s*>/m);
  });
});
