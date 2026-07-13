// Mirrors of the Rust IPC types (src-tauri/src/engine/advisor/types.rs et al).

export type Tier = "quick" | "balanced" | "deep";
export type Route =
  | "new_problem"
  | "followup_retrieve"
  | "followup_reuse"
  | "clarify_answer"
  | "meta";

export interface PackInfo {
  pack_id: string;
  pack_version: string;
  name: string;
  description: string;
  source_type: string;
  embedding_model: string;
  attribution_html: string;
  counts: Record<string, number>;
  healed: boolean;
  path: string;
}

export interface KeyStatus {
  present: boolean;
  last4: string | null;
}

export interface Evidence {
  doc_key: string;
  pack_id: string;
  chunk_id: number;
  document_id: number;
  technique_slug: string | null;
  heading_path: string;
  text: string;
  location: string;
  rerank_score: number | null;
}

export interface Recommendation {
  slug: string;
  pack_id: string;
  title: string;
  stage_id: string;
  complexity: string;
  fit: string;
  tradeoffs: string;
  pair_with: string[];
  vendor_disclosure: string | null;
  confidence: number;
  confidence_label: string;
}

export interface SpanCitation {
  start: number;
  end: number;
  text: string;
  doc_keys: string[];
}

export interface AdvisoryFailureMode {
  id: string;
  name: string;
  score: number;
}

export interface SufficiencyVerdict {
  sub_question: string;
  sufficient: boolean;
  missing: string | null;
}

export interface Advisory {
  tier: string;
  route: Route;
  clarifying_question: string | null;
  diagnosis_md: string;
  failure_modes: AdvisoryFailureMode[];
  recommendations: Recommendation[];
  answer_md: string;
  citations: SpanCitation[];
  evidence: Evidence[];
  gaps: string | null;
  sufficiency: SufficiencyVerdict[];
  degraded: boolean;
  attribution_html: string[];
}

export interface AdvisorTurn {
  conversation_id: number;
  user_turn_id: number;
  advisor_turn_id: number;
  advisory: Advisory;
}

export interface ConversationMeta {
  id: number;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface TurnRecord {
  id: number;
  role: "user" | "advisor";
  content_md: string;
  advisory: Advisory | null;
  citations: SpanCitation[] | null;
  created_at: string;
}

export interface ConversationDetail {
  id: number;
  title: string;
  turns: TurnRecord[];
}

export interface Technique {
  slug: string;
  title: string;
  one_liner: string;
  stage_id: string;
  complexity: string;
  problem_solved: string;
  how_it_works: string;
  when_to_use: string; // JSON array string
  tradeoffs: string; // JSON array string
  key_dependencies: string;
  keywords: string;
  summary: string;
  vendor_disclosure: string | null;
  document_id: number;
  relations: { slug: string; relation: string; title: string }[];
  failure_modes: { id: string; name: string }[];
}

export interface PackDocument {
  kind: "notebook" | "webdoc";
  title: string;
  source_url: string;
  license_note: string;
  content: string; // JSON
  attribution_html: string;
}

export interface NotebookCell {
  t: "md" | "code";
  src: string;
  outputs?: { mime: string; data: string }[];
}

export interface Quota {
  month: string;
  embed_calls: number;
  chat_calls: number;
  rerank_calls: number;
}

export interface IpcError {
  kind: string;
  message: string;
  status?: number;
}

export interface ProgressEvent {
  conversation_id: number;
  stage:
    | "analyzing"
    | "planning"
    | "retrieving"
    | "ranking"
    | "grading"
    | "writing"
    | "verifying"
    | "done";
}
