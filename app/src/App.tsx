import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "./lib/ipc";
import type {
  ConversationMeta,
  IpcError,
  KeyStatus,
  ProgressEvent,
  Tier,
  TurnRecord,
} from "./lib/types";
import { Sidebar } from "./features/history/Sidebar";
import { Thread } from "./features/chat/Thread";
import { Composer } from "./features/chat/Composer";
import { SourcePanel } from "./features/sources/SourcePanel";
import type { SourceRequest } from "./features/chat/AdvisoryView";
import { KeySetup } from "./features/onboarding/KeySetup";

export default function App() {
  const [conversations, setConversations] = useState<ConversationMeta[]>([]);
  const [activeId, setActiveId] = useState<number | null>(null);
  const [turns, setTurns] = useState<TurnRecord[]>([]);
  const [stage, setStage] = useState<ProgressEvent["stage"] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [source, setSource] = useState<SourceRequest | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [keyStatus, setKeyStatus] = useState<KeyStatus | null>(null);
  const [showKeySetup, setShowKeySetup] = useState(false);
  const [composerSeed, setComposerSeed] = useState<string | null>(null);

  const refreshConversations = useCallback(() => {
    ipc.conversationList().then(setConversations).catch(() => {});
  }, []);

  useEffect(() => {
    refreshConversations();
    ipc
      .keyStatus()
      .then((s) => {
        setKeyStatus(s);
        if (!s.present) setShowKeySetup(true);
      })
      .catch(() => {});

    const unlistenProgress = listen<ProgressEvent>("advisor-progress", (e) => {
      setStage(e.payload.stage);
    });
    const unlistenTitle = listen("conversation-titled", () => refreshConversations());
    return () => {
      void unlistenProgress.then((f) => f());
      void unlistenTitle.then((f) => f());
    };
  }, [refreshConversations]);

  const openConversation = useCallback(async (id: number) => {
    setActiveId(id);
    setError(null);
    const detail = await ipc.conversationGet(id);
    setTurns(detail.turns);
  }, []);

  function newConversation() {
    setActiveId(null);
    setTurns([]);
    setError(null);
    setSource(null);
  }

  async function deleteConversation(id: number) {
    await ipc.conversationDelete(id);
    if (id === activeId) newConversation();
    refreshConversations();
  }

  async function send(message: string, tier: Tier) {
    setBusy(true);
    setError(null);
    setStage("analyzing");
    // optimistic user turn
    setTurns((prev) => [
      ...prev,
      {
        id: -Date.now(),
        role: "user",
        content_md: message,
        advisory: null,
        citations: null,
        created_at: new Date().toISOString(),
      },
    ]);
    try {
      const result = await ipc.advisorAsk(message, activeId, tier);
      setActiveId(result.conversation_id);
      const detail = await ipc.conversationGet(result.conversation_id);
      setTurns(detail.turns);
      refreshConversations();
    } catch (e) {
      const err = e as IpcError;
      setError(
        err.kind === "quota_exhausted"
          ? "Your Cohere key's monthly quota is exhausted — the advisor will answer in local match mode until it resets, or add a production key in Settings."
          : err.kind === "no_api_key"
            ? "No API key configured — add one to get full advisories."
            : (err.message ?? "Something went wrong."),
      );
      // roll back the optimistic turn on hard failure
      setTurns((prev) => prev.filter((t) => t.id > 0));
    } finally {
      setBusy(false);
      setStage(null);
    }
  }

  return (
    <div className="flex h-full bg-app text-primary">
      <Sidebar
        conversations={conversations}
        activeId={activeId}
        collapsed={sidebarCollapsed}
        onSelect={openConversation}
        onNew={newConversation}
        onDelete={deleteConversation}
        onToggle={() => setSidebarCollapsed((c) => !c)}
      />

      <main className="flex min-w-0 flex-1 flex-col">
        {showKeySetup && !keyStatus?.present ? (
          <div className="flex h-full items-center justify-center p-[length:var(--sp-4)]">
            <KeySetup
              onDone={(s) => {
                setKeyStatus(s);
                setShowKeySetup(false);
              }}
            />
          </div>
        ) : (
          <>
            <div className="min-h-0 flex-1">
              <Thread
                turns={turns}
                stage={stage}
                busy={busy}
                onOpenSource={setSource}
                onExample={(p) => setComposerSeed(p)}
              />
            </div>
            {error && (
              <div
                role="alert"
                className="mx-auto mb-2 w-fit max-w-2xl rounded-[length:var(--radius-md)] border border-danger/40 bg-inset px-4 py-2 text-[length:var(--text-sm)] text-danger"
              >
                {error}
              </div>
            )}
            <ComposerWithSeed busy={busy} onSend={send} seed={composerSeed} onSeedUsed={() => setComposerSeed(null)} />
          </>
        )}
      </main>

      {source && (
        <div className="w-[42%] min-w-[360px] max-w-[560px]">
          <SourcePanel request={source} onClose={() => setSource(null)} />
        </div>
      )}
    </div>
  );
}

/** Composer wrapper that lets empty-state example prompts prefill and send. */
function ComposerWithSeed({
  busy,
  onSend,
  seed,
  onSeedUsed,
}: {
  busy: boolean;
  onSend: (message: string, tier: Tier) => void;
  seed: string | null;
  onSeedUsed: () => void;
}) {
  useEffect(() => {
    if (seed && !busy) {
      onSeedUsed();
      onSend(seed, "balanced");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seed]);
  return <Composer busy={busy} onSend={onSend} />;
}
