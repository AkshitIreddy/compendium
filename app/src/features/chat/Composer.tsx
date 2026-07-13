import { useRef, useState } from "react";
import type { Tier } from "../../lib/types";
import { useSettings } from "../../lib/settings";

const TIER_INFO: Record<Tier, { label: string; calls: string }> = {
  quick: { label: "Quick", calls: "~3 API calls" },
  balanced: { label: "Balanced", calls: "~7 API calls" },
  deep: { label: "Deep", calls: "~12+ API calls" },
};

export function Composer({
  busy,
  onSend,
  placeholder,
}: {
  busy: boolean;
  onSend: (message: string, tier: Tier) => void;
  placeholder?: string;
}) {
  const { settings, update } = useSettings();
  const [text, setText] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  function send() {
    const message = text.trim();
    if (!message || busy) return;
    onSend(message, settings.tier);
    setText("");
    if (textareaRef.current) textareaRef.current.style.height = "auto";
  }

  return (
    <div className="border-t border-edge bg-surface p-[length:var(--sp-3)]">
      <div
        className="mx-auto flex max-w-3xl flex-col gap-2 rounded-[length:var(--radius-lg)]
                   border border-edge-input bg-raised p-2 shadow-[var(--shadow-raised)]
                   focus-within:border-edge-strong transition-token"
      >
        <textarea
          ref={textareaRef}
          value={text}
          rows={2}
          disabled={busy}
          placeholder={placeholder ?? "Describe the problem you're facing — a symptom or a detailed overview…"}
          aria-label="Describe your problem"
          className="max-h-48 w-full resize-none bg-transparent px-2 py-1 text-[length:var(--text-base)]
                     text-primary placeholder:text-muted focus:outline-none"
          onChange={(e) => {
            setText(e.target.value);
            e.target.style.height = "auto";
            e.target.style.height = `${Math.min(e.target.scrollHeight, 192)}px`;
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        />
        <div className="flex items-center justify-between px-1">
          <div
            className="flex items-center gap-0.5 rounded-full bg-inset p-0.5"
            role="radiogroup"
            aria-label="Advisor depth"
          >
            {(Object.keys(TIER_INFO) as Tier[]).map((tier) => (
              <button
                key={tier}
                type="button"
                role="radio"
                aria-checked={settings.tier === tier}
                title={TIER_INFO[tier].calls}
                onClick={() => update("tier", tier)}
                className={`rounded-full px-2.5 py-1 text-[length:var(--text-xs)] font-medium transition-token ${
                  settings.tier === tier
                    ? "bg-accent text-accent-fg"
                    : "text-secondary hover:text-primary"
                }`}
              >
                {TIER_INFO[tier].label}
              </button>
            ))}
          </div>
          <button
            type="button"
            onClick={send}
            disabled={busy || !text.trim()}
            className="rounded-[length:var(--radius-md)] bg-accent px-4 py-1.5 text-[length:var(--text-sm)]
                       font-semibold text-accent-fg transition-token hover:opacity-90
                       disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {busy ? "Working…" : "Advise"}
          </button>
        </div>
      </div>
    </div>
  );
}
