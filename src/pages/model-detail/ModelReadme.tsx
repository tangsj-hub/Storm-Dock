import { useMemo } from "react";
import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { prepareReadmeMarkdown, readmeAssetBase, resolveReadmeUrl } from "../../lib/modelReadme";
import type { ModelSource } from "../../lib/types";
import extra from "../add-model/detail.module.css";

type ModelReadmeProps = {
  markdown: string;
  source: ModelSource;
  repo: string;
  revision: string;
  titleHint?: string;
};

/** Allow HF/ModelScope HTML tables (e.g. vl-table) while blocking scripts/styles. */
const readmeSchema = {
  ...defaultSchema,
  tagNames: [
    ...(defaultSchema.tagNames ?? []),
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
  ],
  attributes: {
    ...defaultSchema.attributes,
    table: ["className", "class"],
    thead: ["className", "class"],
    tbody: ["className", "class"],
    tr: ["className", "class"],
    th: ["className", "class", "colSpan", "colspan", "rowSpan", "rowspan", "align"],
    td: ["className", "class", "colSpan", "colspan", "rowSpan", "rowspan", "align"],
    img: [...(defaultSchema.attributes?.img ?? []), "className", "class", "loading", "decoding"],
    a: [...(defaultSchema.attributes?.a ?? []), "className", "class", "rel", "target"],
  },
};

function childText(children: unknown): string {
  if (typeof children === "string" || typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(childText).join("");
  return "";
}

/** Renders source-native README as readable prose with resolved relative assets. */
export function ModelReadme({ markdown, source, repo, revision, titleHint }: ModelReadmeProps) {
  const base = useMemo(() => readmeAssetBase(source, repo, revision), [source, repo, revision]);
  const normalized = useMemo(
    () => prepareReadmeMarkdown(markdown, titleHint ?? repo),
    [markdown, titleHint, repo],
  );

  if (!normalized) return null;

  return (
    <div className={extra.readmeProse}>
      <ReactMarkdown
        components={{
          h1: ({ children }) => <h2 className={extra.readmeH1}>{children}</h2>,
          h2: ({ children }) => <h3 className={extra.readmeH2}>{children}</h3>,
          h3: ({ children }) => <h4 className={extra.readmeH3}>{children}</h4>,
          h4: ({ children }) => <h4 className={extra.readmeH3}>{children}</h4>,
          p: ({ children }) => <p className={extra.readmeP}>{children}</p>,
          ul: ({ children }) => <ul className={extra.readmeList}>{children}</ul>,
          ol: ({ children }) => <ol className={extra.readmeList}>{children}</ol>,
          blockquote: ({ children }) => <blockquote className={extra.readmeQuote}>{children}</blockquote>,
          hr: () => <hr className={extra.readmeHr} />,
          a: ({ href, children }) => {
            const resolved = resolveReadmeUrl(base, href);
            return (
              <a className={extra.readmeLink} href={resolved || undefined} rel="noreferrer" target="_blank">
                {children}
              </a>
            );
          },
          img: ({ src, alt }) => {
            const resolved = resolveReadmeUrl(base, src);
            if (!resolved) return null;
            return (
              <img
                alt={alt ?? ""}
                className={extra.readmeImg}
                decoding="async"
                loading="lazy"
                src={resolved}
              />
            );
          },
          code: ({ className, children, ...props }) => {
            const text = childText(children);
            // Fenced blocks may omit language class — treat multiline as block code.
            const block = Boolean(className) || text.includes(String.fromCharCode(10));
            if (!block) {
              return (
                <code className={extra.readmeInlineCode} {...props}>
                  {children}
                </code>
              );
            }
            return (
              <code className={className ? `${extra.readmeCodeInner} ${className}` : extra.readmeCodeInner} {...props}>
                {children}
              </code>
            );
          },
          pre: ({ children }) => <pre className={extra.readmeCode}>{children}</pre>,
          table: ({ children, className }) => (
            <div className={extra.readmeTableWrap}>
              <table className={className ? `${extra.readmeTable} ${className}` : extra.readmeTable}>{children}</table>
            </div>
          ),
          th: ({ children, className, ...props }) => (
            <th className={className ? `${extra.readmeTh} ${className}` : extra.readmeTh} {...props}>
              {children}
            </th>
          ),
          td: ({ children, className, ...props }) => (
            <td className={className ? `${extra.readmeTd} ${className}` : extra.readmeTd} {...props}>
              {children}
            </td>
          ),
        }}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, readmeSchema]]}
        remarkPlugins={[remarkGfm]}
        urlTransform={(url) => resolveReadmeUrl(base, url) || url}
      >
        {normalized}
      </ReactMarkdown>
    </div>
  );
}
