import { useState } from "react";
import { ipc } from "../../lib/ipc";
import type { IpcError, KeyStatus } from "../../lib/types";

/** First-run key onboarding. The app is usable without a key (local match
 * mode) — this explains the tradeoff instead of blocking. */
export function KeySetup({ onDone }: { onDone: (status: KeyStatus) => void }) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    if (!key.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const status = await ipc.keySet(key.trim());
      onDone(status);
    } catch (e) {
      const err = e as IpcError;
      setError(
        err.kind === "api"
          ? "Cohere rejected that key — double-check it and try again."
          : (err.message ?? "Could not store the key."),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto grid w-full max-w-md gap-[length:var(--sp-3)] rounded-[length:var(--radius-lg)] border border-edge bg-surface p-[length:var(--sp-5)] shadow-[var(--shadow-raised)]">
      <div>
        <h2 className="text-[length:var(--text-lg)] font-semibold">Connect Cohere</h2>
        <p className="mt-1 text-[length:var(--text-sm)] text-secondary">
          Compendium uses your own Cohere API key for query understanding and cited answers.
          A <strong>free trial key</strong> covers roughly 100+ Balanced advisories a month.
          The key is stored in the Windows Credential Manager and never leaves this machine
          except to call Cohere.
        </p>
      </div>
      <label className="grid gap-1">
        <span className="text-[length:var(--text-xs)] font-medium text-secondary">API key</span>
        <input
          type="password"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && save()}
          placeholder="Paste a trial or production key"
          className="rounded-[length:var(--radius-md)] border border-edge-input bg-raised px-3 py-2
                     text-[length:var(--text-base)] transition-token focus:border-edge-strong"
        />
      </label>
      {error && (
        <p role="alert" className="text-[length:var(--text-sm)] text-danger">
          {error}
        </p>
      )}
      <div className="flex items-center justify-between">
        <a
          href="https://dashboard.cohere.com/api-keys"
          target="_blank"
          rel="noreferrer noopener"
          className="text-[length:var(--text-sm)] text-accent underline underline-offset-2"
        >
          Get a free key ↗
        </a>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => onDone({ present: false, last4: null })}
            className="rounded-[length:var(--radius-md)] px-3 py-1.5 text-[length:var(--text-sm)]
                       text-secondary transition-token hover:bg-inset"
          >
            Skip for now
          </button>
          <button
            type="button"
            onClick={save}
            disabled={busy || !key.trim()}
            className="rounded-[length:var(--radius-md)] bg-accent px-4 py-1.5 text-[length:var(--text-sm)]
                       font-semibold text-accent-fg transition-token hover:opacity-90 disabled:opacity-40"
          >
            {busy ? "Checking…" : "Save key"}
          </button>
        </div>
      </div>
    </div>
  );
}
