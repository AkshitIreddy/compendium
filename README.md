# Compendium

A desktop **problem → method advisor** for technical practitioners. Describe the problem
you're facing in plain English — short symptom or detailed overview — and Compendium
reasons over curated, offline-prepared knowledge packs to produce a well-cited advisory:
the best-fit techniques, why they fit *your* problem, their tradeoffs, and the exact
source material (notebook sections, framework docs) viewable in-app. Answers are built
as reference dossiers you can hand to another AI as grounding for implementing the fix.

**Status: pre-implementation.** Phase 1 (research + architecture plan) is complete;
see [docs/PLAN.md](docs/PLAN.md).

## How it's built (design summary)

- **Curated over open**: knowledge is prepared, vetted, and embedded offline by a build
  pipeline, then shipped as versioned read-only **knowledge packs** (SQLite). The app
  never embeds corpus content on the user's machine — at runtime it only processes the
  user's queries (Cohere API, user's own key).
- **v1 packs**: RAG techniques (from
  [NirDiamant/RAG_Techniques](https://github.com/NirDiamant/RAG_Techniques)) and
  framework docs (LangChain + LangGraph + LangSmith). The core is pack-agnostic; new
  subject areas are added as new packs without touching the engine.
- **Agentic engine**: a 10-stage advisor pipeline (ontology-guided intake → planned
  multi-query hybrid retrieval with graph expansion → rerank/diversity selection →
  sufficiency grading → per-section grounded synthesis with span citations →
  claim-level verification) in three quality tiers — composed from the best method
  per stage; the app implements many of the very techniques it recommends.
- **Stack**: Tauri 2 (Rust engine: usearch HNSW + SQLite FTS5 hybrid search, the
  advisor state machine, secure key storage) + React 19 (design-token UI with
  first-class accessibility).

## Repository layout

| Path | Contents |
|---|---|
| `docs/` | Architecture plan and (later) pack format spec + contributor guide |
| `research/` | Phase-1 research: 44 structured technique cards, failure-mode ontology, technique catalog, and evidence-backed stack/platform reports |
| `app/` | (Phase 3+) Tauri application |
| `pipeline/` | (Phase 2+) offline pack build pipeline |

## Setup notes

Copy `.env.example` to `.env` and fill in your Cohere keys (never committed). The
production key is used only by the offline pack pipeline; the app itself runs on the
end user's own key.

## Attribution & license posture

The RAG techniques pack is derived (with modifications) from
[NirDiamant/RAG_Techniques](https://github.com/NirDiamant/RAG_Techniques) by
**Nir Diamant**, used under its custom license for **non-commercial** purposes with
attribution. Compendium is therefore free and non-commercial. LangChain/LangGraph
documentation content is MIT-licensed by LangChain.
