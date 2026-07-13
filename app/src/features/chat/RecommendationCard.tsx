import { memo } from "react";
import type { Recommendation } from "../../lib/types";

const STAGE_LABELS: Record<string, string> = {
  chunking: "Chunking",
  indexing: "Indexing",
  "query-transformation": "Query transformation",
  retrieval: "Retrieval",
  "post-retrieval": "Post-retrieval",
  orchestration: "Orchestration",
  evaluation: "Evaluation",
};

function ConfidenceMeter({ value, label }: { value: number; label: string }) {
  // Icon + text + meter: never color alone.
  const tone =
    label === "high" ? "text-success" : label === "medium" ? "text-warning" : "text-muted";
  return (
    <div
      className="flex items-center gap-2"
      role="meter"
      aria-valuenow={Math.round(value * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={`Confidence: ${label}`}
    >
      <div className="h-1.5 w-16 rounded-full bg-inset overflow-hidden">
        <div
          className="h-full rounded-full bg-accent transition-token"
          style={{ width: `${Math.round(value * 100)}%` }}
        />
      </div>
      <span className={`text-[length:var(--text-xs)] font-medium ${tone}`}>{label}</span>
    </div>
  );
}

export const RecommendationCard = memo(function RecommendationCard({
  rec,
  onOpen,
}: {
  rec: Recommendation;
  onOpen: (rec: Recommendation) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onOpen(rec)}
      className="group w-full text-left rounded-[length:var(--radius-md)] border border-edge
                 bg-surface p-[length:var(--sp-3)] transition-token
                 hover:border-edge-strong hover:bg-raised hover:shadow-[var(--shadow-raised)]"
    >
      <div className="flex items-start justify-between gap-2">
        <h4 className="font-semibold text-[length:var(--text-base)] text-primary group-hover:text-accent transition-token">
          {rec.title}
        </h4>
        <ConfidenceMeter value={rec.confidence} label={rec.confidence_label} />
      </div>
      <p className="mt-1 text-[length:var(--text-sm)] text-secondary">{rec.fit}</p>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        <span className="rounded-full bg-accent-subtle px-2 py-0.5 text-[length:var(--text-xs)] font-medium text-accent-subtle-fg">
          {STAGE_LABELS[rec.stage_id] ?? rec.stage_id}
        </span>
        <span className="rounded-full bg-inset px-2 py-0.5 text-[length:var(--text-xs)] text-secondary">
          {rec.complexity} complexity
        </span>
        {rec.pair_with.length > 0 && (
          <span className="text-[length:var(--text-xs)] text-muted">
            pairs with {rec.pair_with.slice(0, 2).join(", ")}
          </span>
        )}
      </div>
      {rec.vendor_disclosure && (
        <p className="mt-2 rounded-[length:var(--radius-sm)] bg-inset px-2 py-1 text-[length:var(--text-xs)] text-warning">
          ⚠ {rec.vendor_disclosure}
        </p>
      )}
    </button>
  );
});
