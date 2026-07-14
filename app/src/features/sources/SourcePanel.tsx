import { useEffect, useRef, useState } from "react";
import { motion } from "motion/react";
import { ipc } from "../../lib/ipc";
import type { NotebookCell, PackDocument, Technique } from "../../lib/types";
import type { SourceRequest } from "../chat/AdvisoryView";
import { Markdown } from "../../components/Markdown";
import { SafeHtml } from "../../components/SafeHtml";
import { NotebookViewer } from "./NotebookViewer";

function parseJsonArray(s: string): string[] {
  try {
    const v = JSON.parse(s);
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function TechniqueDetail({
  technique,
  onOpenNotebook,
}: {
  technique: Technique;
  onOpenNotebook: () => void;
}) {
  const whenToUse = parseJsonArray(technique.when_to_use);
  const tradeoffs = parseJsonArray(technique.tradeoffs);
  return (
    <div className="grid gap-[length:var(--sp-4)]">
      <header>
        <h2 className="text-[length:var(--text-lg)] font-semibold">{technique.title}</h2>
        <p className="mt-1 text-[length:var(--text-sm)] text-secondary">{technique.one_liner}</p>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <span className="rounded-full bg-accent-subtle px-2 py-0.5 text-[length:var(--text-xs)] font-medium text-accent-subtle-fg">
            {technique.stage_id}
          </span>
          <span className="rounded-full bg-inset px-2 py-0.5 text-[length:var(--text-xs)] text-secondary">
            {technique.complexity} complexity
          </span>
        </div>
      </header>

      {technique.vendor_disclosure && (
        <p className="rounded-[length:var(--radius-sm)] bg-inset px-3 py-2 text-[length:var(--text-xs)] text-warning">
          ⚠ {technique.vendor_disclosure}
        </p>
      )}

      <section>
        <h3 className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wide text-muted">
          Problem it solves
        </h3>
        <p className="mt-1 text-[length:var(--text-sm)]">{technique.problem_solved}</p>
      </section>

      <section>
        <h3 className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wide text-muted">
          How it works
        </h3>
        <div className="mt-1 text-[length:var(--text-sm)]">
          <Markdown text={technique.how_it_works} />
        </div>
      </section>

      {whenToUse.length > 0 && (
        <section>
          <h3 className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wide text-muted">
            When to use
          </h3>
          <ul className="mt-1 grid gap-1 pl-4 list-disc text-[length:var(--text-sm)]">
            {whenToUse.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </section>
      )}

      {tradeoffs.length > 0 && (
        <section>
          <h3 className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wide text-muted">
            Tradeoffs
          </h3>
          <ul className="mt-1 grid gap-1 pl-4 list-disc text-[length:var(--text-sm)]">
            {tradeoffs.map((t, i) => (
              <li key={i}>{t}</li>
            ))}
          </ul>
        </section>
      )}

      {technique.relations.length > 0 && (
        <section>
          <h3 className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wide text-muted">
            Related techniques
          </h3>
          <ul className="mt-1 grid gap-1 text-[length:var(--text-sm)]">
            {technique.relations.map((r) => (
              <li key={`${r.slug}:${r.relation}`} className="text-secondary">
                <span className="font-medium text-primary">{r.title}</span>{" "}
                <span className="text-muted">({r.relation.replace(/_/g, " ")})</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      <button
        type="button"
        onClick={onOpenNotebook}
        className="w-fit rounded-[length:var(--radius-sm)] border border-edge bg-surface px-3 py-1.5
                   text-[length:var(--text-sm)] font-medium transition-token hover:border-edge-strong hover:bg-raised"
      >
        Open source notebook →
      </button>
    </div>
  );
}

export function SourcePanel({
  request,
  onClose,
  fullscreen = false,
  onToggleFullscreen,
}: {
  request: SourceRequest;
  onClose: () => void;
  fullscreen?: boolean;
  onToggleFullscreen?: () => void;
}) {
  const [technique, setTechnique] = useState<Technique | null>(null);
  const [doc, setDoc] = useState<PackDocument | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"card" | "document">("card");
  const scrollRef = useRef<HTMLDivElement>(null);

  // A new source or view starts reading from the top — never inherit the
  // previous document's scroll offset. (Cited-cell focus then scrolls itself.)
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 });
  }, [request, view]);

  // Esc restores from fullscreen first, then closes the panel.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      if (fullscreen) onToggleFullscreen?.();
      else onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fullscreen, onToggleFullscreen, onClose]);

  useEffect(() => {
    let cancelled = false;
    setTechnique(null);
    setDoc(null);
    setError(null);
    setView(request.slug ? "card" : "document");

    (async () => {
      try {
        if (request.slug) {
          const t = await ipc.techniqueGet(request.packId, request.slug);
          if (cancelled) return;
          setTechnique(t);
          const d = await ipc.documentGet(request.packId, t.document_id);
          if (!cancelled) setDoc(d);
        } else if (request.documentId != null) {
          const d = await ipc.documentGet(request.packId, request.documentId);
          if (!cancelled) setDoc(d);
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : JSON.stringify(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [request]);

  const { cells, markdownText } = (() => {
    if (!doc) return { cells: [] as NotebookCell[], markdownText: null as string | null };
    try {
      const content = JSON.parse(doc.content);
      if (content.format === "markdown") {
        return { cells: [] as NotebookCell[], markdownText: (content.text as string) ?? "" };
      }
      return { cells: (content.cells ?? []) as NotebookCell[], markdownText: null };
    } catch {
      return { cells: [] as NotebookCell[], markdownText: null };
    }
  })();

  return (
    <motion.aside
      initial={{ x: 32, opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
      className="flex h-full w-full min-w-0 flex-col border-l border-edge bg-surface"
      aria-label="Source panel"
    >
      <header className="flex items-center justify-between gap-2 border-b border-edge px-[length:var(--sp-3)] py-[length:var(--sp-2)]">
        <div className="flex min-w-0 items-center gap-2">
          {technique && (
            <div
              className="flex items-center gap-0.5 rounded-full bg-inset p-0.5"
              role="tablist"
              aria-label="Source view"
            >
              {(["card", "document"] as const).map((v) => (
                <button
                  key={v}
                  role="tab"
                  aria-selected={view === v}
                  onClick={() => setView(v)}
                  className={`rounded-full px-2.5 py-0.5 text-[length:var(--text-xs)] font-medium transition-token ${
                    view === v ? "bg-accent text-accent-fg" : "text-secondary hover:text-primary"
                  }`}
                >
                  {v === "card" ? "Card" : "Notebook"}
                </button>
              ))}
            </div>
          )}
          <span className="truncate text-[length:var(--text-sm)] font-medium text-secondary">
            {doc?.title ?? technique?.title ?? "Source"}
          </span>
        </div>
        <div className="flex items-center gap-0.5">
          {onToggleFullscreen && (
            <button
              type="button"
              onClick={onToggleFullscreen}
              aria-label={fullscreen ? "Restore source panel" : "Expand source panel"}
              title={fullscreen ? "Restore (Esc)" : "Expand to full window"}
              className="rounded-[length:var(--radius-sm)] px-2 py-1 text-secondary transition-token hover:bg-inset hover:text-primary"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                {fullscreen ? (
                  <>
                    <path d="M9 4v5H4" /><path d="M15 4v5h5" /><path d="M9 20v-5H4" /><path d="M15 20v-5h5" />
                  </>
                ) : (
                  <>
                    <path d="M4 9V4h5" /><path d="M20 9V4h-5" /><path d="M4 15v5h5" /><path d="M20 15v5h-5" />
                  </>
                )}
              </svg>
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close source panel"
            className="rounded-[length:var(--radius-sm)] px-2 py-1 text-secondary transition-token hover:bg-inset hover:text-primary"
          >
            ✕
          </button>
        </div>
      </header>

      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto p-[length:var(--sp-4)]">
        {error && <p className="text-[length:var(--text-sm)] text-danger">{error}</p>}
        {!error && view === "card" && technique && (
          <TechniqueDetail technique={technique} onOpenNotebook={() => setView("document")} />
        )}
        {!error && view === "document" && doc && markdownText != null && (
          <Markdown text={markdownText} />
        )}
        {!error && view === "document" && doc && markdownText == null && (
          <NotebookViewer
            cells={cells}
            focusCells={request.focusCells ?? null}
            highlightText={request.highlightText}
          />
        )}
        {!error && !technique && !doc && (
          <p className="text-[length:var(--text-sm)] text-muted">Loading source…</p>
        )}
      </div>

      {doc && (
        <footer className="border-t border-edge px-[length:var(--sp-3)] py-[length:var(--sp-2)] text-[length:var(--text-xs)] text-muted">
          <p><SafeHtml html={doc.attribution_html} /></p>
          <a
            href={doc.source_url}
            target="_blank"
            rel="noreferrer noopener"
            className="text-accent underline underline-offset-2"
          >
            View original source ↗
          </a>
        </footer>
      )}
    </motion.aside>
  );
}
