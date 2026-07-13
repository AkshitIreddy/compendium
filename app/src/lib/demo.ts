// Demo-mode IPC: serves a real advisory (captured from a live pipeline run)
// with staged progress events, so the README GIF shows the true product with
// zero API calls and perfect determinism. Active only under ?demo=1.
import type {
  AdvisorTurn,
  ConversationDetail,
  ConversationMeta,
  KeyStatus,
  PackDocument,
  PackInfo,
  ProgressEvent,
  Quota,
  Technique,
  TurnRecord,
} from "./types";
import { emitDemoEvent } from "./events";

interface Fixture {
  packs: PackInfo[];
  advisory_turn: AdvisorTurn;
  user_message: string;
  techniques: Record<string, Technique>; // "pack:slug"
  documents: Record<string, PackDocument>; // "pack:id"
}

let fixturePromise: Promise<Fixture> | null = null;
function fixture(): Promise<Fixture> {
  fixturePromise ??= import("../demo/fixture.json").then((m) => m.default as unknown as Fixture);
  return fixturePromise;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const STAGES: ProgressEvent["stage"][] = [
  "analyzing",
  "planning",
  "retrieving",
  "ranking",
  "grading",
  "writing",
  "verifying",
];

let turns: TurnRecord[] = [];

export const demoIpc = {
  packsList: async () => (await fixture()).packs,
  keyStatus: async (): Promise<KeyStatus> => ({ present: true, last4: "demo" }),
  keySet: async (): Promise<KeyStatus> => ({ present: true, last4: "demo" }),
  keyDelete: async () => {},

  advisorAsk: async (message: string): Promise<AdvisorTurn> => {
    const fx = await fixture();
    for (const stage of STAGES) {
      emitDemoEvent<ProgressEvent>("advisor-progress", { conversation_id: 1, stage });
      // writing gets a longer beat — it's where the real pipeline spends time
      await sleep(stage === "writing" ? 1600 : stage === "retrieving" ? 900 : 650);
    }
    turns = [
      {
        id: 1,
        role: "user",
        content_md: message,
        advisory: null,
        citations: null,
        created_at: new Date().toISOString(),
      },
      {
        id: 2,
        role: "advisor",
        content_md: fx.advisory_turn.advisory.answer_md,
        advisory: fx.advisory_turn.advisory,
        citations: fx.advisory_turn.advisory.citations,
        created_at: new Date().toISOString(),
      },
    ];
    emitDemoEvent<ProgressEvent>("advisor-progress", { conversation_id: 1, stage: "done" });
    return { ...fx.advisory_turn, conversation_id: 1, user_turn_id: 1, advisor_turn_id: 2 };
  },

  // Always empty: the demo GIF loops back to the empty state, and a sidebar
  // entry appearing mid-scene would break the seamless anchor seam.
  conversationList: async (): Promise<ConversationMeta[]> => [],
  conversationGet: async (): Promise<ConversationDetail> => ({
    id: 1,
    title: "Legal-contract RAG design",
    turns,
  }),
  conversationRename: async () => {},
  conversationDelete: async () => {
    turns = [];
  },
  conversationSearch: async () => [],

  exportDossier: async () => "# Compendium dossier (demo)",

  techniqueGet: async (packId: string, slug: string): Promise<Technique> => {
    const fx = await fixture();
    const t = fx.techniques[`${packId}:${slug}`];
    if (!t) throw new Error(`demo fixture has no technique ${packId}:${slug}`);
    return t;
  },
  documentGet: async (packId: string, documentId: number): Promise<PackDocument> => {
    const fx = await fixture();
    const d = fx.documents[`${packId}:${documentId}`];
    if (!d) throw new Error(`demo fixture has no document ${packId}:${documentId}`);
    return d;
  },

  settingsGetAll: async () => ({}) as Record<string, unknown>,
  settingsSet: async () => {},
  quotaGet: async (): Promise<Quota> => ({
    month: "demo",
    embed_calls: 12,
    chat_calls: 38,
    rerank_calls: 9,
  }),
};
