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

/** Splice citation spans into the source markdown as `[text](cite:i)` links.
 *
 * Cohere reports start/end in Unicode code points against the text it
 * generated, while JS string indices are UTF-16 code units — any astral
 * character before a span shifts the raw offsets. Rather than trusting them,
 * each span is verified against citation.text and, on mismatch, relocated to
 * the nearest occurrence of that text; unlocatable or structure-breaking
 * spans are skipped (the prose stays intact, just unmarked). */
function spliceCitations(md: string, citations: SpanCitation[]): string {
  if (!citations.length) return md;

  const located = citations
    .map((c, i) => {
      if (!c.text || c.text.length > 500) return null;
      let start = c.start;
      if (md.slice(start, start + c.text.length) !== c.text) {
        // offset drifted (code points vs UTF-16) — find nearest occurrence
        const before = md.lastIndexOf(c.text, Math.min(start, md.length));
        const after = md.indexOf(c.text, Math.max(0, start - c.text.length));
        const candidates = [before, after].filter((p) => p >= 0);
        if (!candidates.length) return null;
        start = candidates.reduce((a, b) =>
          Math.abs(a - c.start) <= Math.abs(b - c.start) ? a : b,
        );
      }
      return { i, start, end: start + c.text.length, text: c.text };
    })
    .filter((c): c is { i: number; start: number; end: number; text: string } => c !== null)
    .sort((a, b) => a.start - b.start);

  let out = "";
  let pos = 0;
  for (const c of located) {
    if (c.start < pos) continue; // overlapping span — keep the first
    if (
      c.text.includes("```") ||
      c.text.includes("\n\n") ||
      c.text.includes("[") ||
      c.text.includes("]")
    ) {
      continue; // would break block or link structure; keep text unmarked
    }
    out += md.slice(pos, c.start);
    out += `[${c.text}](cite:${c.i})`;
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
