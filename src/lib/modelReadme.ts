import type { ModelSource } from "./types";

const README_CHAR_LIMIT = 120_000;

/** Asset base for resolving relative README image/link paths. */
export function readmeAssetBase(source: ModelSource, repo: string, revision: string) {
  const rev = revision.trim() || (source === "huggingface" ? "main" : "master");
  if (source === "huggingface") {
    return `https://huggingface.co/${repo}/resolve/${rev}/`;
  }
  return `https://www.modelscope.cn/models/${repo}/resolve/${rev}/`;
}

export function resolveReadmeUrl(base: string, raw?: string | null) {
  const href = (raw ?? "").trim();
  if (!href || href.startsWith("#") || href.startsWith("mailto:") || href.startsWith("data:")) {
    return href;
  }
  if (/^(https?:)?\/\//i.test(href)) {
    return href.startsWith("//") ? `https:${href}` : href;
  }
  try {
    return new URL(href.replace(/^\.\//, ""), base).toString();
  } catch {
    return href;
  }
}

function attr(tag: string, name: string) {
  const match = new RegExp(`\\b${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)')`, "i").exec(tag);
  return (match?.[1] ?? match?.[2] ?? "").trim();
}

/**
 * Policy: do NOT chase Hub README dialects.
 * Normalize to core Markdown/GFM + a tiny HTML allowlist (tables/images).
 * Unsupported dialects are stripped to readable plain text, not specially rendered.
 */

/** Essential only: HTML anchors/images → markdown so rehype does not fight leftovers. */
function htmlAnchorsAndImagesToMarkdown(input: string) {
  let body = input.replace(/<a\b([^>]*)>([\s\S]*?)<\/a>/gi, (full, attrs: string, inner: string) => {
    const href = attr(`a ${attrs}`, "href");
    if (!href) return inner.replace(/<[^>]+>/g, "");
    const images = [...inner.matchAll(/<img\b[^>]*>/gi)].map((item) => item[0]);
    if (images.length > 0) {
      const first = images[0];
      const src = attr(first, "src");
      const alt = attr(first, "alt");
      if (!src) return inner.replace(/<[^>]+>/g, "");
      return `[![${alt}](${src})](${href})`;
    }
    const text = inner.replace(/<[^>]+>/g, "").trim();
    return text ? `[${text}](${href})` : "";
  });

  body = body.replace(/<img\b[^>]*>/gi, (tag) => {
    const src = attr(tag, "src");
    if (!src) return "";
    const alt = attr(tag, "alt");
    return `![${alt}](${src})`;
  });

  return body;
}

/** Strip GitHub alert / admonition markers; keep the body text only. */
export function stripReadmeDialects(input: string) {
  const lines = input.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // > [!NOTE] blockquotes → plain paragraphs (drop the marker / quote chrome)
    const quoteAlert = /^\s*>\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*(.*)$/i.exec(line);
    if (quoteAlert) {
      if (quoteAlert[2].trim()) out.push(quoteAlert[2].trim());
      i += 1;
      while (i < lines.length && /^\s*>/.test(lines[i])) {
        const text = lines[i].replace(/^\s*>\s?/, "").trim();
        if (text) out.push(text);
        i += 1;
      }
      out.push("");
      continue;
    }

    // [!Note] / [!Tip] inline markers → plain text body
    const inlineAlert = /^\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*(.*)$/i.exec(line);
    if (inlineAlert) {
      if (inlineAlert[2].trim()) out.push(inlineAlert[2].trim());
      i += 1;
      continue;
    }

    // Drop common non-content HTML chrome tags but keep table structure for GFM/HTML tables.
    out.push(line);
    i += 1;
  }

  return out.join("\n");
}

function stripNonContentHtml(input: string) {
  return input
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, "")
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "")
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<\/?(?:div|span|section|center|font|picture|figure|figcaption|video|audio|source)\b[^>]*>/gi, "")
    .replace(/<\/?p\b[^>]*>/gi, "\n")
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/&nbsp;/gi, " ");
}

/** Prepare Hub README into a small, stable subset — strip dialects, don't emulate them. */
export function prepareReadmeMarkdown(raw: string, titleHint?: string) {
  let body = raw.replace(/\r\n/g, "\n");
  if (body.startsWith("---")) {
    const end = body.indexOf("\n---", 3);
    if (end >= 0) {
      body = body.slice(end + 4).replace(/^\n+/, "");
    }
  }

  body = htmlAnchorsAndImagesToMarkdown(body);
  body = stripReadmeDialects(body);
  body = stripNonContentHtml(body)
    .replace(/\n{3,}/g, "\n\n")
    .trim();

  const lines = body.split("\n");
  if (lines[0]) {
    const heading = /^#\s+(.+?)\s*$/.exec(lines[0].trim());
    if (heading) {
      const title = heading[1].replace(/[#*_`]/g, "").trim().toLowerCase();
      const hint = (titleHint ?? "").trim().toLowerCase();
      const hintTail = hint.includes("/") ? hint.split("/").pop()! : hint;
      if (!hint || title === hint || title === hintTail || hint.endsWith(title) || title.endsWith(hintTail)) {
        body = lines.slice(1).join("\n").replace(/^\n+/, "");
      }
    }
  }

  if (body.length > README_CHAR_LIMIT) {
    body = `${body.slice(0, README_CHAR_LIMIT).trimEnd()}\n\n---\n\nREADME truncated for performance.`;
  }
  return body.trim();
}
