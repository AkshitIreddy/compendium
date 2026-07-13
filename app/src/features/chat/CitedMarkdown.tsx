import { memo, useMemo, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import { CodeBlock } from "../../components/CodeBlock";
import type { SpanCitation } from "../../lib/types";

// Allow our cite: protocol through sanitization (used for citation marks).
const schema = {
  ...defaultSchema,
  protocols: {
    ...defaultSchema.protocols,
    href: [...(defaultSchema.protocols?.href ?? []), "cite"],
  },
};

/** Splice citation spans into the source markdown as `[text](cite:i)` links,
 * skipping spans that would break fenced code blocks. Offsets reference the
 * raw markdown string (Cohere cites against the text it generated). */
function spliceCitations(md: string, citations: SpanCitation[]): string {
  if (!citations.length) return md;
  const sorted = [...citations]
    .map((c, i) => ({ ...c, i }))
    .filter((c) => c.start < c.end && c.end <= md.length)
    .sort((a, b) => a.start - b.start);

  let out = "";
  let pos = 0;
  for (const c of sorted) {
    if (c.start < pos) continue; // overlapping span — keep the first
    const inner = md.slice(c.start, c.end);
    if (inner.includes("```") || inner.includes("\n\n")) {
      continue; // would break block structure; skip the mark, keep the text
    }
    out += md.slice(pos, c.start);
    out += `[${inner}](cite:${c.i})`;
    pos = c.end;
  }
  out += md.slice(pos);
  return out;
}

export const CitedMarkdown = memo(function CitedMarkdown({
  text,
  citations,
  onCite,
  activeCite,
}: {
  text: string;
  citations: SpanCitation[];
  onCite: (citation: SpanCitation) => void;
  activeCite: SpanCitation | null;
}) {
  const spliced = useMemo(() => spliceCitations(text, citations), [text, citations]);

  return (
    <div className="prose-compendium">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[[rehypeSanitize, schema]]}
        components={{
          code({ className, children, ...props }) {
            const match = /language-(\w+)/.exec(className ?? "");
            const code = String(children).replace(/\n$/, "");
            if (match && code.includes("\n")) {
              return <CodeBlock code={code} lang={match[1]} />;
            }
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          },
          a({ href, children }) {
            if (href?.startsWith("cite:")) {
              const idx = Number(href.slice(5));
              const citation = citations[idx];
              if (!citation) return <>{children as ReactNode}</>;
              return (
                <button
                  type="button"
                  className="citation-mark"
                  data-active={activeCite === citation}
                  onClick={() => onCite(citation)}
                  aria-label={`Show source for: ${citation.text.slice(0, 60)}`}
                >
                  {children as ReactNode}
                </button>
              );
            }
            return (
              <a href={href} target="_blank" rel="noreferrer noopener">
                {children as ReactNode}
              </a>
            );
          },
        }}
      >
        {spliced}
      </ReactMarkdown>
    </div>
  );
});
