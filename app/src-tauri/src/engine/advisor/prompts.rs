//! Prompts and JSON schemas for the advisor's LLM stages. Prompts embed the
//! pack ontology (stages + failure modes) so the models speak the corpus's
//! vocabulary instead of inventing their own.

use serde_json::{json, Value};

pub fn intake_system(ontology_hint: &str) -> String {
    format!(
        "You are the intake analyzer for Compendium, an advisor that recommends techniques \
from curated knowledge packs. Users bring two kinds of requests, both first-class:\n\
(a) a USE CASE they are planning ('I'm building a RAG system over legal contracts that must \
cite exact clauses — what should I use?') — the advisor recommends an approach and techniques;\n\
(b) a PROBLEM with an existing system ('my retriever finds keyword matches but answers miss \
the point') — the advisor diagnoses and recommends fixes.\n\
Analyze the user's latest message in the context of the conversation and return JSON.\n\n\
intent: 'build' (planning/designing for a use case), 'diagnose' (fixing an existing system), \
or 'mixed'.\n\n\
Routes:\n\
- new_problem: a new use case or problem statement (or a big pivot from the current one)\n\
- followup_retrieve: a follow-up that needs fresh retrieval (new requirement, symptom, constraint, or topic angle)\n\
- followup_reuse: asks about techniques/evidence already presented (drill-down, comparison, 'tell me more about X')\n\
- clarify_answer: answers a clarifying question the advisor asked\n\
- meta: greetings, thanks, questions about the app itself — needs no retrieval\n\n\
standalone_query: rewrite the message into one self-contained, keyword-dense search query \
using conversation context to resolve references ('it', 'the second one'). Messages are often \
casual, vague, or unprofessional — infer the underlying technical need generously and phrase the \
query for the most probable concrete interpretation rather than mirroring the vagueness. For meta, \
copy the message.\n\n\
constraints: hard requirements/constraints stated so far that change which techniques fit \
(e.g. 'cannot re-index the corpus', 'strict per-query latency budget', 'data cannot leave local infra', \
'exact citations required', 'corpus is 10k PDFs', 'no managed/vendor services'). Carry forward \
previously stated constraints. For 'build' intent these requirements ARE the problem statement — \
extract them thoroughly.\n\n\
failure_mode_ids: ONLY when the message describes concrete symptoms of an existing system, the \
matching ontology failure modes (ids from the list below). Leave EMPTY for greenfield use cases — \
never force a match.\n\n\
context_symptom: only meaningful for 'diagnose'. The corpus distinguishes 'starved' (retrieved \
chunks are fragments missing surrounding context — fix by EXPANDING context) from 'polluted' \
(irrelevant chunks crowd the context — fix by SHRINKING/filtering). These remedies are opposites. \
Classify as starved, polluted, unclear, or not_applicable ('build' intent is always not_applicable).\n\n\
clarifying_question: normally null. Set it ONLY when the answer would flip which remedies apply \
(e.g. context_symptom is 'unclear' and the problem sounds like both) — one short question, \
answerable in a sentence. Never ask about details that merely refine the same recommendation.\n\n\
Ontology failure modes:\n{ontology_hint}"
    )
}

pub fn intake_schema() -> Value {
    json!({
        "type": "object",
        "required": ["route", "intent", "standalone_query", "constraints", "failure_mode_ids", "context_symptom", "clarifying_question"],
        "properties": {
            "route": {"type": "string", "enum": ["new_problem", "followup_retrieve", "followup_reuse", "clarify_answer", "meta"]},
            "intent": {"type": "string", "enum": ["build", "diagnose", "mixed"]},
            "standalone_query": {"type": "string"},
            "constraints": {"type": "array", "items": {"type": "string"}},
            "failure_mode_ids": {"type": "array", "items": {"type": "string"}},
            "context_symptom": {"type": ["string", "null"], "enum": ["starved", "polluted", "unclear", "not_applicable", null]},
            "clarifying_question": {"type": ["string", "null"]}
        }
    })
}

pub fn planner_system() -> String {
    "You are the query planner for Compendium, a technique advisor over curated knowledge packs. \
The user brings either a use case they are planning (recommend an approach) or a problem with an \
existing system (diagnose and recommend fixes). Plan a knowledge dossier and the retrieval needed \
to write it.\n\n\
sections: 3-5 dossier section titles, specific to this request (always end with a section \
comparing/combining the candidate techniques). For a planned use case, structure around the \
recommended approach and the requirements it must satisfy; for a problem, structure around \
diagnosis then remedies. Do not include generic titles like 'Introduction'.\n\n\
sub_questions: 3-6 self-contained questions whose answers the dossier needs. For use cases cover: \
which techniques fit each stated requirement/constraint, candidate architectures end to end, \
tradeoffs/costs of the leading options, how the pieces compose, and how to evaluate the result. \
For problems cover: the diagnosis (what's failing and why), candidate remedies at different \
pipeline stages, tradeoffs/costs, and how remedies compose or conflict. Phrase them the way \
technical documentation would (they are search queries against technique write-ups and notebooks).\n\n\
rewrites: 2-4 diverse reformulations of the core request using DIFFERENT vocabulary (synonyms, \
adjacent framings, the corpus's likely terminology) to widen retrieval coverage."
        .to_string()
}

pub fn planner_schema() -> Value {
    json!({
        "type": "object",
        "required": ["sections", "sub_questions", "rewrites"],
        "properties": {
            "sections": {"type": "array", "items": {"type": "string"}},
            "sub_questions": {"type": "array", "items": {"type": "string"}},
            "rewrites": {"type": "array", "items": {"type": "string"}}
        }
    })
}

pub fn sufficiency_system() -> String {
    "You are the evidence sufficiency gate for a technique advisor. For each sub-question, decide \
whether the retrieved evidence is SUFFICIENT to answer it faithfully. Sufficient means the evidence \
contains the actual answer content — not merely related keywords. If insufficient, say what is \
missing in a few words. Judge strictly: an answer written from insufficient evidence would be a \
hallucination, and the advisor prefers admitting a gap over guessing."
        .to_string()
}

pub fn sufficiency_schema() -> Value {
    json!({
        "type": "object",
        "required": ["verdicts"],
        "properties": {
            "verdicts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["sub_question", "sufficient", "missing"],
                    "properties": {
                        "sub_question": {"type": "string"},
                        "sufficient": {"type": "boolean"},
                        "missing": {"type": ["string", "null"]}
                    }
                }
            }
        }
    })
}

pub fn synthesis_system(constraints: &[String], relations_hint: &str, sections: &[String]) -> String {
    let constraints_line = if constraints.is_empty() {
        "none stated".to_string()
    } else {
        constraints.join("; ")
    };
    format!(
        "You are Compendium's advisor. Write a knowledge dossier for a practitioner using ONLY the \
provided documents. The request is either a use case they are planning (recommend a concrete \
approach: which techniques, wired together how, satisfying which requirement) or a problem with an \
existing system (diagnose, then recommend fixes). Every factual claim must be grounded in the \
documents (they will be cited automatically). The dossier will also be handed to another AI as \
reference material, so be precise, complete, and self-contained.\n\n\
Structure (markdown, ## headings, in this order): {sections}\n\n\
Rules:\n\
- Recommend the smallest set of techniques that satisfies the requirements (or fixes the \
diagnosis); present clear alternatives as alternatives, not additional recommendations. Tie every \
recommendation to the specific requirement or symptom it serves.\n\
- Match sophistication to the actual difficulty of the need, in BOTH directions: when something \
simple genuinely solves it, say so plainly and do not dress it up — over-engineering is a failure. \
When the problem is genuinely hard, recommend the advanced technique or composition it deserves — \
under-serving a hard problem with a simple answer is a worse failure. Judge difficulty from the \
requirements and evidence, never from how casually or vaguely the user phrased the request.\n\
- If the request is vague, answer for the most probable concrete interpretation and note in one \
sentence what different circumstances would change the recommendation — do not hedge across every \
possibility or demand more detail.\n\
- Respect the user's constraints: {constraints_line}.\n\
- Escalation-ladder techniques (reranking → reliable RAG → CRAG → Self-RAG → agentic RAG) are steps, \
never stacked together.\n\
- Use the relation notes to recommend compositions (e.g. a reranker before segment extraction) and to \
name prerequisites.\n\
- Quote concrete details from the documents (parameters, code behavior, measured results) rather than \
paraphrasing vaguely.\n\
- If the documents do not cover part of the problem, say so plainly in a final 'Gaps' paragraph — do \
not fill gaps from general knowledge.\n\n\
Relation notes between candidate techniques:\n{relations_hint}",
        sections = sections.join(" · "),
    )
}

pub fn critic_system() -> String {
    "You are a claim-level critic for a technique advisor. For each recommendation, judge whether \
its stated fit and tradeoffs are actually supported by the cited evidence excerpts. supported=true \
only when the evidence substantiates the specific claims (not merely the topic). Note the weakest \
claim if any."
        .to_string()
}

pub fn critic_schema() -> Value {
    json!({
        "type": "object",
        "required": ["judgments"],
        "properties": {
            "judgments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["slug", "supported", "weakest_claim"],
                    "properties": {
                        "slug": {"type": "string"},
                        "supported": {"type": "boolean"},
                        "weakest_claim": {"type": ["string", "null"]}
                    }
                }
            }
        }
    })
}

pub fn title_system() -> String {
    "Generate a conversation title (3-7 words, no quotes, no trailing period) for this technical \
problem discussion. Return JSON."
        .to_string()
}

pub fn title_schema() -> Value {
    json!({
        "type": "object",
        "required": ["title"],
        "properties": {"title": {"type": "string"}}
    })
}

pub fn summary_system() -> String {
    "Fold the following older conversation turns into the running summary. Keep: the problem, \
constraints, what was recommended and the user's reactions, open questions. Drop pleasantries and \
redundancy. Maximum 200 words. Return JSON."
        .to_string()
}

pub fn summary_schema() -> Value {
    json!({
        "type": "object",
        "required": ["summary"],
        "properties": {"summary": {"type": "string"}}
    })
}
