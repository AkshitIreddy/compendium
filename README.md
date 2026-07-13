# Compendium

A desktop **problem → method advisor** for technical practitioners. Describe the problem
you're facing in plain English — a one-line symptom or a detailed overview — and
Compendium's advisor pipeline reasons over curated, offline-prepared knowledge packs to
produce a **cited knowledge dossier**: best-fit techniques with per-problem justification
and confidence, honest tradeoffs, supporting excerpts viewable in-app with span-accurate
citation highlighting, and a one-click export built to be handed to another AI as
grounding for implementing the fix.

## How it works

- **Curated over open**: knowledge is prepared, vetted, and embedded offline by the
  build pipeline, then shipped as versioned read-only **packs** (single SQLite files
  with prebuilt usearch HNSW indexes and FTS5). The app never embeds corpus content on
  your machine — at runtime it only processes your queries via your own Cohere key
  (free trial tier works; stored in the Windows Credential Manager).
- **The advisor pipeline** is a 10-stage state machine composed from the best method
  per stage (surveyed in [research/advisor-pipeline.md](research/advisor-pipeline.md)):
  ontology-guided intake with clarifying questions → query planning → multi-arm hybrid
  retrieval with typed-graph expansion → rerank + diversity selection → sufficiency
  grading with an honest-gap path → grounded synthesis with native span citations →
  claim-level verification → dossier assembly. Three depth tiers (Quick/Balanced/Deep)
  share the architecture; every API failure degrades to a local advisory rather than
  losing the turn. The app implements many of the very techniques it recommends.
- **Chat with memory**: pinned problem statement, sliding window, async-folded
  summaries, and evidence reuse make follow-ups cheap and coherent.

## v1 knowledge packs (bundled)

| Pack | Contents |
|---|---|
| **RAG Techniques** | 39 techniques + 5 evaluation methodologies from [NirDiamant/RAG_Techniques](https://github.com/NirDiamant/RAG_Techniques), analyzed into structured cards with a 25-failure-mode ontology and typed relation graph |
| **Framework Docs** | 155 pages of current LangChain, LangGraph, and LangSmith documentation scoped to retrieval, agentic RAG, memory, evaluation, observability, and prompt engineering |

New subject areas are added as new packs — see [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md).

## Repository layout

| Path | Contents |
|---|---|
| `app/` | Tauri 2 app — React 19 UI (`src/`), Rust engine (`src-tauri/`: pack loading, hybrid search, advisor pipeline, key storage) |
| `pipeline/` | Offline pack build pipeline (Python): source-type processors, embedding, index build, validation |
| `docs/` | [Architecture plan](docs/PLAN.md) · [Pack format spec](docs/PACK_FORMAT.md) · [Contributor guide](docs/CONTRIBUTING.md) |
| `research/` | Phase-1 research: technique cards, ontology, and evidence-backed platform reports |

## Development

```
# packs (offline; needs COHERE_API_KEY_PRODUCTION in .env)
cd pipeline
python -m venv .venv && .venv/Scripts/pip install -r requirements.txt
.venv/Scripts/python -m compendium_pack build packs/rag-techniques --source <RAG_Techniques clone>
.venv/Scripts/python -m compendium_pack build packs/framework-docs --source .cache/webdocs

# app (dev)
cd app && npm install
COMPENDIUM_PACKS_DIR=../packs-out npm run tauri dev

# tests
cd app/src-tauri && cargo test          # engine (needs built packs)
cd app && npx vitest run                # UI + WCAG contrast matrix
npm run tauri build                     # NSIS installer (bundles packs-out/)
```

## Attribution & license posture

The RAG Techniques pack is derived (with modifications) from
[NirDiamant/RAG_Techniques](https://github.com/NirDiamant/RAG_Techniques) by
**Nir Diamant**, used under its custom license for **non-commercial** purposes with
attribution; Compendium is therefore free and non-commercial, and renders this
attribution in-app and in exported dossiers. Framework documentation ©
[LangChain](https://docs.langchain.com), MIT License. Compendium is an independent
project — no endorsement by or affiliation with either source is implied.
