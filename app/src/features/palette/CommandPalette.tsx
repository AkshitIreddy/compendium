import { useEffect, useMemo, useRef, useState } from "react";
import { Dialog } from "@base-ui/react/dialog";
import { ipc } from "../../lib/ipc";
import type { ConversationMeta } from "../../lib/types";

interface Command {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

export function CommandPalette({
  open,
  onClose,
  conversations,
  actions,
}: {
  open: boolean;
  onClose: () => void;
  conversations: ConversationMeta[];
  actions: {
    newConversation: () => void;
    openConversation: (id: number) => void;
    openSettings: () => void;
    cycleTheme: () => void;
  };
}) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [searchHits, setSearchHits] = useState<{ conversation_id: number; title: string; snippet: string }[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelected(0);
      setTimeout(() => inputRef.current?.focus(), 30);
    }
  }, [open]);

  // full-text conversation search (debounced, local FTS — free)
  useEffect(() => {
    if (!query.trim() || query.length < 3) {
      setSearchHits([]);
      return;
    }
    const t = setTimeout(() => {
      ipc.conversationSearch(query).then(setSearchHits).catch(() => setSearchHits([]));
    }, 150);
    return () => clearTimeout(t);
  }, [query]);

  const commands = useMemo<Command[]>(() => {
    const base: Command[] = [
      { id: "new", label: "New conversation", hint: "Ctrl+N", run: actions.newConversation },
      { id: "settings", label: "Open settings", hint: "Ctrl+,", run: actions.openSettings },
      { id: "theme", label: "Cycle theme", run: actions.cycleTheme },
    ];
    const convs: Command[] = conversations.slice(0, 8).map((c) => ({
      id: `conv-${c.id}`,
      label: c.title,
      hint: "conversation",
      run: () => actions.openConversation(c.id),
    }));
    const hits: Command[] = searchHits.map((h) => ({
      id: `hit-${h.conversation_id}-${h.snippet.slice(0, 8)}`,
      label: h.title,
      hint: h.snippet.replace(/<\/?b>/g, ""),
      run: () => actions.openConversation(h.conversation_id),
    }));

    const q = query.trim().toLowerCase();
    const pool = q
      ? [...base.filter((c) => c.label.toLowerCase().includes(q)), ...hits]
      : [...base, ...convs];
    return pool.slice(0, 12);
  }, [query, conversations, searchHits, actions]);

  useEffect(() => {
    setSelected((s) => Math.min(s, Math.max(commands.length - 1, 0)));
  }, [commands.length]);

  function runSelected() {
    const cmd = commands[selected];
    if (cmd) {
      onClose();
      cmd.run();
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 bg-black/30" />
        <Dialog.Popup
          className="fixed left-1/2 top-[18%] w-[min(560px,92vw)] -translate-x-1/2 overflow-hidden
                     rounded-[length:var(--radius-lg)] border border-edge bg-overlay shadow-[var(--shadow-overlay)]"
          aria-label="Command palette"
        >
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search commands and conversations…"
            aria-label="Search commands and conversations"
            aria-activedescendant={commands[selected] ? `cmd-${commands[selected].id}` : undefined}
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            className="w-full border-b border-edge bg-transparent px-4 py-3 text-[length:var(--text-md)] focus:outline-none"
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelected((s) => Math.min(s + 1, commands.length - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelected((s) => Math.max(s - 1, 0));
              } else if (e.key === "Enter") {
                e.preventDefault();
                runSelected();
              }
            }}
          />
          <ul ref={listRef} id="palette-list" role="listbox" className="max-h-72 overflow-y-auto p-1.5">
            {commands.length === 0 && (
              <li className="px-3 py-4 text-center text-[length:var(--text-sm)] text-muted">No matches</li>
            )}
            {commands.map((cmd, i) => (
              <li key={cmd.id} role="option" id={`cmd-${cmd.id}`} aria-selected={i === selected}>
                <button
                  type="button"
                  onClick={runSelected}
                  onMouseEnter={() => setSelected(i)}
                  className={`flex w-full items-center justify-between gap-3 rounded-[length:var(--radius-sm)]
                              px-3 py-2 text-left text-[length:var(--text-sm)] transition-token ${
                                i === selected ? "bg-accent-subtle text-accent-subtle-fg" : "text-primary"
                              }`}
                >
                  <span className="truncate font-medium">{cmd.label}</span>
                  {cmd.hint && <span className="truncate text-[length:var(--text-xs)] text-muted">{cmd.hint}</span>}
                </button>
              </li>
            ))}
          </ul>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
