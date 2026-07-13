// Typed wrappers over every Tauri command — the single place the string
// command names live.
import { invoke } from "@tauri-apps/api/core";
import type {
  AdvisorTurn,
  ConversationDetail,
  ConversationMeta,
  KeyStatus,
  PackDocument,
  PackInfo,
  Quota,
  Technique,
  Tier,
} from "./types";

export const ipc = {
  packsList: () => invoke<PackInfo[]>("packs_list"),

  keyStatus: () => invoke<KeyStatus>("key_status"),
  keySet: (key: string) => invoke<KeyStatus>("key_set", { key }),
  keyDelete: () => invoke<void>("key_delete"),

  advisorAsk: (message: string, conversationId: number | null, tier: Tier | null) =>
    invoke<AdvisorTurn>("advisor_ask", {
      message,
      conversationId,
      tier,
    }),

  conversationList: () => invoke<ConversationMeta[]>("conversation_list"),
  conversationGet: (conversationId: number) =>
    invoke<ConversationDetail>("conversation_get", { conversationId }),
  conversationRename: (conversationId: number, title: string) =>
    invoke<void>("conversation_rename", { conversationId, title }),
  conversationDelete: (conversationId: number) =>
    invoke<void>("conversation_delete", { conversationId }),
  conversationSearch: (query: string) =>
    invoke<{ conversation_id: number; title: string; snippet: string }[]>(
      "conversation_search",
      { query },
    ),

  exportDossier: (turnId: number) => invoke<string>("export_dossier", { turnId }),

  techniqueGet: (packId: string, slug: string) =>
    invoke<Technique>("technique_get", { packId, slug }),
  documentGet: (packId: string, documentId: number) =>
    invoke<PackDocument>("document_get", { packId, documentId }),

  settingsGetAll: () => invoke<Record<string, unknown>>("settings_get_all"),
  settingsSet: (key: string, value: unknown) => invoke<void>("settings_set", { key, value }),

  quotaGet: () => invoke<Quota>("quota_get"),
};
