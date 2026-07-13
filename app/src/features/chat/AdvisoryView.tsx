import { memo, useState } from "react";
import type { Advisory, Recommendation, SpanCitation } from "../../lib/types";
import { ipc } from "../../lib/ipc";
import { CitedMarkdown } from "./CitedMarkdown";
import { RecommendationCard } from "./RecommendationCard";

export interface SourceRequest {
  packId: string;
  documentId?: number;
  slug?: string;
  highlightText?: string;
  /** exact notebook cell range from the evidence's location JSON */
  focusCells?: [number, number] | null;
}

export const AdvisoryView = memo(function AdvisoryView({
  advisory,
  turnId,
  onOpenSource,
}: {
  advisory: Advisory;
  turnId: number;
  onOpenSource: (req: SourceRequest) => void;
}) {
  const [activeCite, setActiveCite] = useState<SpanCitation | null>(null);
  const [copied, setCopied] = useState<"idle" | "copied" | "failed">("idle");

  function handleCite(citation: SpanCitation) {
    setActiveCite(citation);
    const key = citation.doc_keys[0];
    const evidence = advisory.evidence.find((e) => e.doc_key === key);
    if (evidence) {
      let focusCells: [number, number] | null = null;
      try {
        const loc = JSON.parse(evidence.location);
        if (Array.isArray(loc.cells)) focusCells = [loc.cells[0], loc.cells[1]];
      } catch {
        // location stays null — panel opens without cell focus
      }
      onOpenSource({
        packId: evidence.pack_id,
        documentId: evidence.document_id,
        highlightText: evidence.text.slice(0, 120),
        focusCells,
      });
      return;
    }
    // card citation: pack:card:slug
    const parts = key.split(":card:");
    if (parts.length === 2) {
      onOpenSource({ packId: parts[0], slug: parts[1] });
    }
  }

  function handleOpenRec(rec: Recommendation) {
    onOpenSource({ packId: rec.pack_id, slug: rec.slug });
  }

  async function copyDossier() {
    try {
      const md = await ipc.exportDossier(turnId);
      await navigator.clipboard.writeText(md);
      setCopied("copied");
    } catch {
      setCopied("failed");
    }
    setTimeout(() => setCopied("idle"), 2500);
  }

  if (advisory.clarifying_question) {
    return (
      <div className="rounded-[length:var(--radius-md)] border border-edge bg-accent-subtle/40 p-[length:var(--sp-4)]">
        <p className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wide text-accent-subtle-fg">
          One question first
        </p>
        <p className="mt-1 text-[length:var(--text-md)]">{advisory.clarifying_question}</p>
        <p className="mt-2 text-[length:var(--text-sm)] text-secondary">
          The answer changes which remedies apply — reply below and I'll continue.
        </p>
      </div>
    );
  }

  return (
    <div className="grid gap-[length:var(--sp-4)]">
      {advisory.degraded && (
        <div
          role="status"
          className="rounded-[length:var(--radius-md)] border border-edge bg-inset px-3 py-2 text-[length:var(--text-sm)] text-secondary"
        >
          ◦ Local match mode — results ranked from the on-device index only (no API access).
          Add or check your Cohere key in Settings for full advisories.
        </div>
      )}

      {(advisory.diagnosis_md || advisory.failure_modes.length > 0) && (
        <div className="flex flex-wrap items-center gap-1.5">
          {advisory.failure_modes.map((fm) => (
            <span
              key={fm.id}
              className="rounded-full border border-edge bg-surface px-2 py-0.5 text-[length:var(--text-xs)] text-secondary"
              title={fm.id}
            >
              ◈ {fm.name}
            </span>
          ))}
        </div>
      )}

      {advisory.answer_md && (
        <CitedMarkdown
          text={advisory.answer_md}
          citations={advisory.citations}
          onCite={handleCite}
          activeCite={activeCite}
        />
      )}

      {advisory.recommendations.length > 0 && (
        <section aria-label="Recommended techniques" className="grid gap-2">
          <h3 className="text-[length:var(--text-xs)] font-semibold uppercase tracking-wide text-muted">
            Recommended techniques
          </h3>
          <div className="grid gap-2 md:grid-cols-2">
            {advisory.recommendations.map((rec) => (
              <RecommendationCard key={`${rec.pack_id}:${rec.slug}`} rec={rec} onOpen={handleOpenRec} />
            ))}
          </div>
        </section>
      )}

      {advisory.gaps && (
        <div
          role="note"
          className="rounded-[length:var(--radius-md)] border border-warning/40 bg-inset p-3 text-[length:var(--text-sm)]"
        >
          <p className="font-semibold text-warning">Honest gaps</p>
          <p className="mt-1 text-secondary">{advisory.gaps}</p>
        </div>
      )}

      {advisory.answer_md && (
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={copyDossier}
            className="rounded-[length:var(--radius-sm)] border border-edge bg-surface px-3 py-1.5
                       text-[length:var(--text-sm)] font-medium transition-token
                       hover:border-edge-strong hover:bg-raised"
          >
            {copied === "copied"
              ? "✓ Copied"
              : copied === "failed"
                ? "Copy failed — try again"
                : "Copy dossier for another AI"}
          </button>
          <span className="text-[length:var(--text-xs)] text-muted">
            {advisory.evidence.length} evidence excerpts · {advisory.citations.length} citations ·{" "}
            {advisory.tier} tier
          </span>
        </div>
      )}

    </div>
  );
});
