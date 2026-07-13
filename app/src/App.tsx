import { useCallback, useEffect, useRef, useState } from "react";
import { listenAppEvent } from "./lib/events";
import { openUrl } from "@tauri-apps/plugin-opener";
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
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { ResizeHandle } from "./components/ResizeHandle";
import { CommandPalette } from "./features/palette/CommandPalette";
import { play } from "./lib/sound";
import { useSettings } from "./lib/settings";

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
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const { settings, update } = useSettings();
  // last-request-wins guard: navigation during an in-flight advisor call or
  // conversation load must not be clobbered by the older response
  const navSeq = useRef(0);
  // live panel widths during a drag; settings persist on commit
  const [liveSidebarW, setLiveSidebarW] = useState<number | null>(null);
  const [liveSourceW, setLiveSourceW] = useState<number | null>(null);
  const sidebarW = Math.min(Math.max(liveSidebarW ?? settings.sidebarWidth, 180), 420);
  const sourceW = Math.min(Math.max(liveSourceW ?? settings.sourcePanelWidth, 320), 760);

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

    const unlistenProgress = listenAppEvent<ProgressEvent>("advisor-progress", (p) => {
      setStage(p.stage);
    });
    const unlistenTitle = listenAppEvent("conversation-titled", () => refreshConversations());
    return () => {
      void unlistenProgress.then((f) => f());
      void unlistenTitle.then((f) => f());
    };
  }, [refreshConversations]);

  // All anchor clicks route through the OS browser via the opener plugin.
  // Without this, WebView2 resolves hrefs against tauri.localhost and hands
  // the browser a dead URL. Relative links from docs markdown are inert.
  useEffect(() => {
    function onClick(e: MouseEvent) {
      const anchor = (e.target as HTMLElement).closest?.("a[href]");
      if (!anchor) return;
      const href = anchor.getAttribute("href") ?? "";
      if (/^https?:\/\//i.test(href)) {
        e.preventDefault();
        void openUrl(href);
      } else if (!href.startsWith("#")) {
        e.preventDefault();
      }
    }
    document.addEventListener("click", onClick, true);
    return () => document.removeEventListener("click", onClick, true);
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      } else if (e.ctrlKey && e.key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
      } else if (e.ctrlKey && e.key.toLowerCase() === "n") {
        e.preventDefault();
        newConversation();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const openConversation = useCallback(async (id: number) => {
    const seq = ++navSeq.current;
    setActiveId(id);
    setError(null);
    try {
      const detail = await ipc.conversationGet(id);
      if (navSeq.current === seq) setTurns(detail.turns);
    } catch (e) {
      if (navSeq.current === seq) {
        setError((e as { message?: string }).message ?? "Could not load conversation.");
        setTurns([]);
      }
    }
  }, []);

  function newConversation() {
    navSeq.current++;
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
    play("send");
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
    const seq = navSeq.current;
    try {
      const result = await ipc.advisorAsk(message, activeId, tier);
      refreshConversations();
      play("result");
      // apply only if the user hasn't navigated away mid-request
      if (navSeq.current === seq) {
        setActiveId(result.conversation_id);
        const detail = await ipc.conversationGet(result.conversation_id);
        if (navSeq.current === seq) setTurns(detail.turns);
      }
    } catch (e) {
      const err = e as IpcError;
      setError(
        err.kind === "quota_exhausted"
          ? "Your Cohere key's monthly quota is exhausted — the advisor will answer in local match mode until it resets, or add a production key in Settings."
          : err.kind === "no_api_key"
            ? "No API key configured — add one to get full advisories."
            : (err.message ?? "Something went wrong."),
      );
      play("error");
      // roll back the optimistic turn on hard failure (unless user navigated)
      if (navSeq.current === seq) setTurns((prev) => prev.filter((t) => t.id > 0));
    } finally {
      setBusy(false);
      setStage(null);
    }
  }

  return (
    <div className="flex h-full bg-app text-primary">
      <div style={sidebarCollapsed ? undefined : { width: sidebarW }} className="flex shrink-0">
        <Sidebar
          conversations={conversations}
          activeId={activeId}
          collapsed={sidebarCollapsed}
          onSelect={openConversation}
          onNew={newConversation}
          onDelete={deleteConversation}
          onToggle={() => setSidebarCollapsed((c) => !c)}
        />
      </div>
      {!sidebarCollapsed && (
        <ResizeHandle
          ariaLabel="Resize history sidebar"
          onDrag={(dx) => setLiveSidebarW((w) => (w ?? settings.sidebarWidth) + dx)}
          onCommit={() => {
            if (liveSidebarW != null) {
              update("sidebarWidth", Math.min(Math.max(liveSidebarW, 180), 420));
              setLiveSidebarW(null);
            }
          }}
        />
      )}

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
        <>
          <ResizeHandle
            ariaLabel="Resize source panel"
            onDrag={(dx) => setLiveSourceW((w) => (w ?? settings.sourcePanelWidth) - dx)}
            onCommit={() => {
              if (liveSourceW != null) {
                update("sourcePanelWidth", Math.min(Math.max(liveSourceW, 320), 760));
                setLiveSourceW(null);
              }
            }}
          />
          <div style={{ width: sourceW }} className="flex shrink-0">
            <SourcePanel request={source} onClose={() => setSource(null)} />
          </div>
        </>
      )}

      <button
        type="button"
        onClick={() => setSettingsOpen(true)}
        aria-label="Open settings (Ctrl+,)"
        title="Settings (Ctrl+,)"
        className="fixed bottom-3 left-3 z-10 rounded-full border border-edge bg-surface p-2
                   text-secondary shadow-[var(--shadow-raised)] transition-token
                   hover:text-primary hover:border-edge-strong"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      </button>

      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        keyStatus={keyStatus}
        onKeyChange={setKeyStatus}
      />
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        conversations={conversations}
        actions={{
          newConversation,
          openConversation,
          openSettings: () => setSettingsOpen(true),
          cycleTheme: () => {
            const order = ["system", "porcelain", "graphite", "midnight", "contrast"] as const;
            const next = order[(order.indexOf(settings.theme) + 1) % order.length];
            update("theme", next);
          },
        }}
      />
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
  const { settings } = useSettings();
  useEffect(() => {
    if (seed && !busy) {
      onSeedUsed();
      onSend(seed, settings.tier);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seed]);
  return <Composer busy={busy} onSend={onSend} />;
}
