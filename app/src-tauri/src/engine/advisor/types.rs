//! Advisor pipeline data types: what flows between stages and what the UI
//! receives. The Advisory JSON stored per turn is the offline re-render source.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Quick,
    Balanced,
    Deep,
}

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Quick => "quick",
            Tier::Balanced => "balanced",
            Tier::Deep => "deep",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    NewProblem,
    FollowupRetrieve,
    FollowupReuse,
    ClarifyAnswer,
    Meta,
}

/// Output of the combined S1 turn analysis (router + rewriter + intake).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnAnalysis {
    pub route: Route,
    pub standalone_query: String,
    /// Extracted hard constraints ("cannot re-index", "local only", ...).
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Confirmed failure-mode ids from the pack ontology.
    #[serde(default)]
    pub failure_mode_ids: Vec<String>,
    /// starved | polluted | unclear | not_applicable — the opposite-remedy trap.
    #[serde(default)]
    pub context_symptom: Option<String>,
    /// At most one clarifying question; asking it short-circuits the pipeline.
    #[serde(default)]
    pub clarifying_question: Option<String>,
}

/// Output of the S2 query planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Dossier outline: section titles in order.
    pub sections: Vec<String>,
    /// Sub-questions to retrieve for (each becomes a retrieval fan-out arm).
    pub sub_questions: Vec<String>,
    /// Diverse query rewrites (DMQR-style) added to the fan-out.
    #[serde(default)]
    pub rewrites: Vec<String>,
}

/// One graded sub-question from the S5 sufficiency gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SufficiencyVerdict {
    pub sub_question: String,
    pub sufficient: bool,
    #[serde(default)]
    pub missing: Option<String>,
}

/// A piece of evidence shipped to synthesis and shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub doc_key: String, // stable citation id, e.g. "rag-techniques:chunk:187"
    pub pack_id: String,
    pub chunk_id: i64,
    pub document_id: i64,
    pub technique_slug: Option<String>,
    pub heading_path: String,
    pub text: String,
    pub location: String,
    pub rerank_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub slug: String,
    pub pack_id: String,
    pub title: String,
    pub stage_id: String,
    pub complexity: String,
    pub fit: String,          // why this fits THIS problem (cited prose)
    pub tradeoffs: String,
    #[serde(default)]
    pub pair_with: Vec<String>,
    #[serde(default)]
    pub vendor_disclosure: Option<String>,
    /// 0..1 composite confidence (retrieval strength × critic verdict).
    pub confidence: f64,
    #[serde(default)]
    pub confidence_label: String, // "high" | "medium" | "low"
}

/// Character-span citation against `answer_md`, mapped back to evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanCitation {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub doc_keys: Vec<String>,
}

/// The complete advisory for one turn — persisted as JSON, re-renderable offline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Advisory {
    pub tier: String,
    pub route: Route,
    /// Set when the advisor chose to ask a clarifying question instead.
    #[serde(default)]
    pub clarifying_question: Option<String>,
    pub diagnosis_md: String,
    #[serde(default)]
    pub failure_modes: Vec<AdvisoryFailureMode>,
    pub recommendations: Vec<Recommendation>,
    pub answer_md: String,
    #[serde(default)]
    pub citations: Vec<SpanCitation>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    /// Honest-gap statement when the corpus lacks coverage; None otherwise.
    #[serde(default)]
    pub gaps: Option<String>,
    #[serde(default)]
    pub sufficiency: Vec<SufficiencyVerdict>,
    /// True when generation degraded to local-only (no key / quota).
    #[serde(default)]
    pub degraded: bool,
    pub attribution_html: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisoryFailureMode {
    pub id: String,
    pub name: String,
    pub score: f32,
}

/// What advisor_ask returns to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorTurn {
    pub conversation_id: i64,
    pub user_turn_id: i64,
    pub advisor_turn_id: i64,
    pub advisory: Advisory,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdvisorStateEntry {
    pub slug: String,
    pub pack_id: String,
    pub verdict: String, // "recommended" | "alternative" | "rejected"
}

/// Live conversation state (conversation_state row).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationState {
    #[serde(default)]
    pub pinned_problem: Option<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub advisor_state: Vec<AdvisorStateEntry>,
    /// Cached candidate pool: evidence doc_keys in stable order.
    #[serde(default)]
    pub candidate_pool: Vec<String>,
    #[serde(default)]
    pub open_question: Option<String>,
}
