import { useEffect, useRef } from "react";
import { AnimatePresence, motion } from "motion/react";
import type { ProgressEvent, TurnRecord } from "../../lib/types";
import { AdvisoryView, type SourceRequest } from "./AdvisoryView";
import { Markdown } from "../../components/Markdown";

const STAGE_LABELS: Record<ProgressEvent["stage"], string> = {
  analyzing: "Understanding the problem",
  planning: "Planning the dossier",
  retrieving: "Searching the knowledge packs",
  ranking: "Ranking evidence",
  grading: "Grading sufficiency",
  writing: "Writing the advisory",
  verifying: "Verifying citations",
  done: "Done",
};

const EXAMPLE_PROMPTS = [
  "I'm building a RAG assistant over 10,000 legal PDFs where every answer must cite exact clauses — how should I design retrieval?",
  "Planning a support chatbot over our product docs that must run fully on-prem, with follow-up questions — what techniques fit?",
  "My retriever finds chunks with the right keywords, but the answers keep missing the point",
  "I have no way to tell if my RAG changes make quality better or worse",
];

function ProgressIndicator({ stage }: { stage: ProgressEvent["stage"] | null }) {
  if (!stage || stage === "done") return null;
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.18 }}
      className="flex items-center gap-2 text-[length:var(--text-sm)] text-secondary"
      role="status"
      aria-live="polite"
    >
      <span className="relative flex h-2 w-2">
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent opacity-60" />
        <span className="relative inline-flex h-2 w-2 rounded-full bg-accent" />
      </span>
      {STAGE_LABELS[stage]}…
    </motion.div>
  );
}

export function Thread({
  turns,
  stage,
  busy,
  onOpenSource,
  onExample,
}: {
  turns: TurnRecord[];
  stage: ProgressEvent["stage"] | null;
  busy: boolean;
  onOpenSource: (req: SourceRequest) => void;
  onExample: (prompt: string) => void;
}) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [turns.length, stage]);

  if (turns.length === 0 && !busy) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-[length:var(--sp-5)] p-[length:var(--sp-6)]">
        <div className="text-center">
          <h2 className="text-[length:var(--text-xl)] font-semibold tracking-tight">
            What are you building — or what's going wrong?
          </h2>
          <p className="mt-1 max-w-md text-[length:var(--text-sm)] text-secondary">
            Describe the system you're planning (use case + constraints) or a problem with an
            existing one. Compendium reasons over its curated packs and recommends the techniques
            to use, with cited sources.
          </p>
        </div>
        <div className="grid w-full max-w-xl gap-2">
          {EXAMPLE_PROMPTS.map((p) => (
            <button
              key={p}
              type="button"
              onClick={() => onExample(p)}
              className="rounded-[length:var(--radius-md)] border border-edge bg-surface px-4 py-2.5
                         text-left text-[length:var(--text-sm)] text-secondary transition-token
                         hover:border-edge-strong hover:bg-raised hover:text-primary"
            >
              “{p}”
            </button>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto" role="log" aria-label="Conversation">
      <div className="mx-auto grid max-w-3xl gap-[length:var(--sp-5)] p-[length:var(--sp-4)] pb-[length:var(--sp-6)]">
        <AnimatePresence initial={false}>
          {turns.map((turn) => (
            <motion.article
              key={turn.id}
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
              aria-label={turn.role === "user" ? "You" : "Compendium"}
            >
              {turn.role === "user" ? (
                <div className="ml-auto max-w-[85%] w-fit rounded-[length:var(--radius-lg)] rounded-br-[4px] bg-accent-subtle px-4 py-2.5 text-accent-subtle-fg">
                  <p className="whitespace-pre-wrap text-[length:var(--text-base)]">{turn.content_md}</p>
                </div>
              ) : turn.advisory ? (
                <AdvisoryView advisory={turn.advisory} turnId={turn.id} onOpenSource={onOpenSource} />
              ) : (
                <Markdown text={turn.content_md} />
              )}
            </motion.article>
          ))}
        </AnimatePresence>
        <ProgressIndicator stage={busy ? stage : null} />
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
