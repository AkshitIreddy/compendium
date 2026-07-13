import { memo, useEffect, useMemo, useRef } from "react";
import DOMPurify from "dompurify";
import Anser from "anser";
import type { NotebookCell } from "../../lib/types";
import { Markdown } from "../../components/Markdown";
import { CodeBlock } from "../../components/CodeBlock";

/** Render one whitelisted output. text/html goes through DOMPurify — packs are
 * trusted-ish (we build them) but defense in depth is non-negotiable for
 * notebook outputs. */
function Output({ mime, data }: { mime: string; data: string }) {
  if (mime === "image/png") {
    return (
      <img
        src={`data:image/png;base64,${data}`}
        alt="Notebook output"
        loading="lazy"
        className="max-w-full rounded-[length:var(--radius-sm)] border border-edge bg-white"
      />
    );
  }
  if (mime === "text/html") {
    const clean = DOMPurify.sanitize(data, {
      FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form"],
      FORBID_ATTR: ["onerror", "onclick", "onload", "style"],
    });
    return (
      <div
        className="notebook-html-output overflow-x-auto text-[length:var(--text-sm)]"
        dangerouslySetInnerHTML={{ __html: clean }}
      />
    );
  }
  if (mime === "application/x-traceback") {
    const html = Anser.ansiToHtml(Anser.escapeForHtml(data));
    return (
      <pre className="overflow-x-auto rounded-[length:var(--radius-sm)] bg-inset p-2 text-[length:var(--text-xs)] text-danger">
        <code dangerouslySetInnerHTML={{ __html: html }} />
      </pre>
    );
  }
  return (
    <pre className="overflow-x-auto rounded-[length:var(--radius-sm)] bg-inset p-2 text-[length:var(--text-xs)] text-secondary">
      <code>{data}</code>
    </pre>
  );
}

export const NotebookViewer = memo(function NotebookViewer({
  cells,
  focusCells,
  highlightText,
}: {
  cells: NotebookCell[];
  /** [first, last] cell range a citation points at */
  focusCells?: [number, number] | null;
  highlightText?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  const focusSet = useMemo(() => {
    if (!focusCells) return new Set<number>();
    const [a, b] = focusCells;
    return new Set(Array.from({ length: b - a + 1 }, (_, i) => a + i));
  }, [focusCells]);

  useEffect(() => {
    if (!focusCells || !containerRef.current) return;
    const el = containerRef.current.querySelector(`[data-cell="${focusCells[0]}"]`);
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [focusCells]);

  // Note: highlightText is used for aria description; visual highlight is the
  // focused-cell ring (span-exact match inside rendered markdown is unreliable).
  return (
    <div ref={containerRef} className="grid gap-[length:var(--sp-3)]">
      {cells.map((cell, i) => (
        <section
          key={i}
          data-cell={i}
          aria-label={cell.t === "md" ? "Markdown cell" : "Code cell"}
          aria-description={
            focusSet.has(i) && highlightText ? `Cited: ${highlightText}` : undefined
          }
          className={`rounded-[length:var(--radius-md)] transition-token ${
            focusSet.has(i)
              ? "ring-2 ring-accent bg-accent-subtle/20 p-2 -m-0.5"
              : ""
          }`}
        >
          {cell.t === "md" ? (
            <Markdown text={cell.src} />
          ) : (
            <div className="grid gap-1.5">
              <CodeBlock code={cell.src} lang="python" />
              {cell.outputs?.map((o, j) => <Output key={j} mime={o.mime} data={o.data} />)}
            </div>
          )}
        </section>
      ))}
    </div>
  );
});
