import { useState } from "react";
import type { ConversationMeta } from "../../lib/types";

export function Sidebar({
  conversations,
  activeId,
  collapsed,
  onSelect,
  onNew,
  onDelete,
  onToggle,
}: {
  conversations: ConversationMeta[];
  activeId: number | null;
  collapsed: boolean;
  onSelect: (id: number) => void;
  onNew: () => void;
  onDelete: (id: number) => void;
  onToggle: () => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState<number | null>(null);

  if (collapsed) {
    return (
      <div className="flex h-full w-12 flex-col items-center gap-2 border-r border-edge bg-surface py-3">
        <button
          type="button"
          onClick={onToggle}
          aria-label="Expand history sidebar"
          className="rounded-[length:var(--radius-sm)] p-2 text-secondary transition-token hover:bg-inset hover:text-primary"
        >
          ☰
        </button>
        <button
          type="button"
          onClick={onNew}
          aria-label="New conversation"
          className="rounded-[length:var(--radius-sm)] p-2 text-secondary transition-token hover:bg-inset hover:text-primary"
        >
          +
        </button>
      </div>
    );
  }

  return (
    <nav
      className="flex h-full w-full flex-col border-r border-edge bg-surface"
      aria-label="Conversation history"
    >
      <div className="flex items-center justify-between gap-2 p-[length:var(--sp-2)]">
        <button
          type="button"
          onClick={onToggle}
          aria-label="Collapse history sidebar"
          className="rounded-[length:var(--radius-sm)] p-1.5 text-secondary transition-token hover:bg-inset hover:text-primary"
        >
          ☰
        </button>
        <button
          type="button"
          onClick={onNew}
          className="flex-1 rounded-[length:var(--radius-md)] border border-edge bg-raised px-3 py-1.5
                     text-[length:var(--text-sm)] font-medium transition-token hover:border-edge-strong"
        >
          + New conversation
        </button>
      </div>

      <ul className="min-h-0 flex-1 overflow-y-auto px-[length:var(--sp-2)] pb-2">
        {conversations.length === 0 && (
          <li className="px-2 py-4 text-[length:var(--text-xs)] text-muted">
            Past conversations appear here.
          </li>
        )}
        {conversations.map((c) => (
          <li key={c.id} className="group relative">
            <button
              type="button"
              onClick={() => onSelect(c.id)}
              aria-current={activeId === c.id ? "page" : undefined}
              className={`w-full truncate rounded-[length:var(--radius-sm)] px-2.5 py-1.5 text-left
                          text-[length:var(--text-sm)] transition-token ${
                            activeId === c.id
                              ? "bg-accent-subtle text-accent-subtle-fg font-medium"
                              : "text-secondary hover:bg-inset hover:text-primary"
                          }`}
            >
              {c.title}
            </button>
            <button
              type="button"
              aria-label={`Delete conversation: ${c.title}`}
              onClick={() =>
                confirmDelete === c.id ? (onDelete(c.id), setConfirmDelete(null)) : setConfirmDelete(c.id)
              }
              onBlur={() => setConfirmDelete(null)}
              className={`absolute right-1 top-1/2 -translate-y-1/2 rounded px-1.5 py-0.5
                          text-[length:var(--text-xs)] opacity-0 transition-token
                          focus-visible:opacity-100 group-hover:opacity-100 ${
                            confirmDelete === c.id
                              ? "bg-danger text-white opacity-100"
                              : "text-muted hover:text-danger"
                          }`}
            >
              {confirmDelete === c.id ? "sure?" : "✕"}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
