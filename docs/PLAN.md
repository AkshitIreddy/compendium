# Plan v2 — Compendium: a problem → method advisor

> Status: **v2, awaiting final sign-off before Phase 2 (implementation).**
> v1 sign-off feedback incorporated: name **Compendium**; deep-reasoning agentic engine
> (quality over call count); chat with follow-up memory; real local vector store;
> LangSmith docs added to the v1 pack; no code signing; both packs bundled.
> Research grounding: 44 technique cards + ontology ([research/corpus/](../research/corpus/))
> and nine web-verified reports in [research/](../research/) — stack/platform
> (docs-acquisition, cohere-platform, desktop-stack, ui-stack) and engine redesign
> (advisor-pipeline, cohere-agentic, context-management, vector-store, langsmith-docs-scope).

---

## 1. Product definition

**Compendium** is a Windows desktop app for technical practitioners. In a chat interface,
the user describes a problem — a one-line symptom *or* a detailed multi-paragraph
overview of their system and where it hurts — and Compendium's advisor agent works
through curated, offline-prepared knowledge packs to produce a **cited knowledge
dossier**:

- the best-fit techniques with per-recommendation justification, tradeoffs, and a
  confidence indicator,
- multiple supporting data points: excerpts from the exact notebook sections and docs
  pages, viewable in-app with span-accurate highlighting,
- composition and escalation advice drawn from the technique-relation graph,
- an honest "the corpus doesn't cover this" path when that's the truth.

The dossier is explicitly designed to be **handed to another AI** as reference material:
one click exports a self-contained markdown bundle (structured header + cited prose +
verbatim evidence appendix with provenance anchors) that the user can paste into any
assistant that will implement the fix. Compendium's job is knowledge and evidence, not
the final code.

**Follow-ups are first-class**: the conversation has memory — the advisor remembers the
user's problem statement and constraints, what it already recommended, and the evidence
it already retrieved, so "what about the second option?" or "we can't re-index, does
that change things?" work naturally.

v1 ships two packs: **RAG Techniques** (NirDiamant/RAG_Techniques, 39 techniques + 5
evaluation notebooks) and **Framework Docs** (LangChain + LangGraph + LangSmith). The
core is pack-agnostic; RAG is content, not architecture.

---

## 2. Established facts (updated where v2 research superseded v1)

### 2.1 Corpus, ontology, and license — unchanged from v1

44 notebooks analyzed into structured technique cards; 7-stage lifecycle; 25 failure
modes with user-phrasing variants; typed relation graph (`composes_with`,
`alternative_to`, `prerequisite_of`, `refines`, `evaluated_by`). Key engine-shaping
insights: the opposite-remedy trap (context *starved* vs *polluted* — same user
phrasing, opposite fixes → disambiguate before recommending), index-time vs query-time
mirror pairs, the self-correction escalation ladder (recommend as steps, never stacked),
reranking as composition hub, framework twins collapse to one technique, sponsor
disclosures on two cards.

**License (load-bearing)**: RAG_Techniques is custom **non-commercial** — the app stays
free; attribution to Nir Diamant + repo link + modified-content marking is structural
(required manifest fields, rendered in-app, embedded in every exported dossier).
LangChain/LangGraph/LangSmith docs: MIT (verified — `src/langsmith/` lives in the same
MIT-licensed `langchain-ai/docs` repo), robots signal `ai-train=yes`.

### 2.2 Cohere platform (re-verified for the agentic design, 2026-07-13)

- **Model lineup changed May 2026**: `command-a-plus-05-2026` (218B sparse-MoE, Apache
  2.0, native citations, 128k ctx / **64k output**, big agentic gains) exists but hosted
  access is sales-gated. The flagship **self-serve** model remains
  **`command-a-03-2025`** (256k ctx, $2.50/$10 per 1M, 500 req/min prod).
  `command-a-reasoning-08-2025` (controllable thinking budget) is trial-usable,
  sales-gated for production. Per-role model ids stay **configurable** — never
  hard-depend on gated models.
- **v2 Chat tool use** fits our agent loop natively: `tool_plan` + `tool_calls[]`,
  parallel calls in one turn, `strict_tools: true` for schema-valid calls, streamed
  tool/citation events.
- **Native span citations are the backbone of source rendering**: documents-mode and
  document-typed tool results return character-span citations
  `{start, end, text, sources[]}` (streaming + non-streaming; `citation_options:
  accurate`). We persist these beside the dossier for character-offset highlighting.
  **Hard constraint**: `json_schema` response format cannot combine with tools/documents
  — final synthesis is cited prose whose structure is assembled in Rust; `json_schema`
  only on intermediate no-tool calls. Enforced at the type level in the Rust client.
- **Rate limits**: trial = 20 chat/min, 10 rerank/min, 1,000 calls/month → **~30–50
  Deep-tier queries/month on a trial key** (or hundreds of Quick/Balanced). Production
  self-serve = 500 chat/min, no monthly cap, **~$0.20–0.45 per Deep query** (Command A +
  R7B assignment). The app paces calls (token bucket: 20 chat / 10 rerank per minute)
  and meters monthly usage.
- Embed v4.0 batches up to 96 texts per call — all of a turn's query reformulations cost
  one embed call. Rerank `v4.0-pro`; rerank once per loop iteration over merged
  candidates (not per sub-query) to respect trial's 10/min.

### 2.3 Docs acquisition (extended to LangSmith, verified 2026-07-13)

Unchanged mechanics: sitemap-scoped per-page `.md` fetch from docs.langchain.com,
content-hash change gating, monthly refresh, loud failure guardrails (404s / ±10% page
swing). New:

- **LangSmith scope**: 127-page **exact-slug allowlist** (the `/langsmith/` namespace is
  flat — no prefix matching): 72 evaluation + 6 online-eval + 13 prompt engineering +
  2 context engineering + 31 observability/tracing + 3 monitoring. All 127 verified
  fetchable as `.md` (1.68 MB total, ~400–600 chunks). Excluded: ~880 URLs of API
  reference, deploy/self-host, admin/billing, vendor-specific tracing pages.
- **Correction to v1**: `llms.txt` is explicitly truncated (omits 527 pages) — invalid
  even as a completeness cross-check. Reconcile against the GitHub `src/langsmith/`
  listing or `llms-full.txt` `Source:` markers instead.
- LangSmith pages churn fast and `lastmod` is deploy noise (241/409 pages re-stamped in
  one second) — **content hash is the only re-embed trigger**.
- Overlap with corpus eval notebooks is complementary, not duplicative (zero RAGAS/GroUSE
  mentions in LangSmith pages); the pack build cross-links LangSmith eval pages to the
  eval technique cards as alternatives.

### 2.4 Desktop + UI stack — unchanged from v1

Tauri 2.11 / NSIS `currentUser` / updater / WebView2 bootstrapper; rusqlite (bundled,
FTS5) + keyring→Windows Credential Manager; React 19 + Vite + TS, Base UI + React Aria,
Tailwind v4 CSS-first with 3-layer OKLCH tokens and **CI-enforced WCAG AA**,
react-markdown + Streamdown + Shiki v4 (JS regex engine — the WASM default breaks strict
CSP), custom sanitized nbformat viewer (DOMPurify mandatory), Motion v12 + View
Transitions with a global `--motion-scale`, raw Web Audio + CC0 sound sets (off by
default), Inter Variable + JetBrains Mono Variable, Mica vibrancy with opaque fallbacks,
Snap-Layouts-capable custom titlebar, forced-colors audit. Reference apps: Yaak, Cap, Jan.

### 2.5 Vector store decision (v2 — replaces v1's brute-force-only design)

**usearch 2.26.0** (actively maintained; single C++ core with lockstep Python + Rust
SDKs sharing one serialized format — build-in-Python/load-in-Rust is the documented
canonical workflow):

- Per pack, **two f16 HNSW indexes** (technique cards; chunks) stored as **BLOBs in a
  `vector_indexes` table inside the pack SQLite** — single-file packaging preserved.
  Manifest records `usearch_version`, build params, and SHA-256.
- Runtime: verify hash → extract once to app cache → **`view()` mmap** → queryable in
  single-digit ms at any scale. Small packs may `load_from_buffer` directly.
- **f32 vectors remain in the pack as vectors-of-record**: exact cosine re-scoring of
  the fused top ~50, and **self-healing** — any corruption/version mismatch triggers an
  automatic index rebuild from stored vectors (1–2 s at 10k vectors).
- usearch publishes no cross-version format guarantee → version pinned identically in
  pipeline lockfile and Cargo.toml; mismatch = rebuild, never a crash.
- Quality-first HNSW params (connectivity 16–32, expansion_add 256–512, expansion_search
  128–512); pipeline build gate: **recall@10 ≥ 0.98 vs exact brute force**.
- Rejected: LanceDB (Arrow/DataFusion dependency avalanche, directory datasets break
  single-file packs), sqlite-vec (still pre-v1, brute-force core), FAISS (Windows
  packaging pain).
- A **spike precedes engine code**: Python-save → Rust-load round-trip on Windows,
  `view()` latency, binary-size delta.

---

## 3. The advisor engine (v2 core — a composed, best-of-each-part pipeline)

Survey conclusion (full comparisons per part, with runners-up, in
[advisor-pipeline.md](../research/advisor-pipeline.md)): with a **local** corpus where
retrieval costs ~10 ms, the **deep-research pattern** (plan → parallel deterministic
retrieval fan-out → batched grading → per-section synthesis → critic) dominates
interleaved ReAct/IRCoT loops, which exist to ration *expensive* retrieval. Searchers
are concurrent Rust functions, not LLMs — we get multi-agent-style coverage without
multi-agent token multiplication. The pipeline is a **fixed Rust state machine** (S0–S9);
the LLM plans and judges inside stages but does not invent the pipeline per query.

### The stages

| # | Stage | What happens | LLM? |
|---|---|---|---|
| S0 | **Ontology matcher** | User text matched against pre-embedded failure-mode phrasings + BM25 → candidate failure modes, before any API call | no |
| S1 | **Intake analyzer** | One structured call: query type (factual/overview/comparison/follow-up/meta), constraints extraction (can re-index? latency budget? local-only?), failure-mode confirmation, **starved-vs-polluted disambiguation**, tier routing, standalone query rewrite; may return ≤1 clarifying question instead | 1× R7B |
| S2 | **Query planner** | Dossier outline + sub-questions + diverse query rewrites (DMQR-style); ontology-guided expansion into corpus vocabulary added locally for free; HyDE (Deep tier) | 1× A |
| S3 | **Hierarchical hybrid retrieval** | Per sub-query, cards-then-chunks: usearch dense + FTS5 BM25 → RRF (k=60) → exact f32 re-score → **1-hop typed-graph expansion** (pull alternatives, prerequisites, composition partners that similarity search ranks low — the only reliable source of tradeoff content) → constraint filters *demote, don't delete* | no |
| S4 | **Rank & select** | rerank-v4.0-pro over merged candidates (once per iteration) → adaptive-k truncation (score-gap, not fixed k) → dartboard-style diversity across cards/failure modes → RSE segment reconstruction | rerank |
| S5 | **Sufficiency gate** | One batched CRAG-style grading call: per-sub-question SUFFICIENT/INSUFFICIENT verdicts (models hallucinate rather than abstain on insufficient context — ICLR 2025); corrective local re-query loops (cheap!); stop on coverage/convergence/budget; first-class **honest-gap** output when the corpus lacks the answer | 1× R7B |
| S6 | **Evidence assembly** | Stable anchors (`pack/technique/section`), per-card token budgets, sandwich ordering (lost-in-the-middle), dedup across sub-queries | no |
| S7 | **Grounded synthesis** | Per-section generation bound to that section's evidence (outline-then-write), Cohere **native span citations** (`accurate`), streamed per section | 1–N× A |
| S8 | **Verify & score** | Local citation-integrity string check (zero LLM) → one batched claim-level critic (RAGAS-faithfulness formulation) → composite per-recommendation confidence; Deep tier: targeted section repair | 1× R7B (+A) |
| S9 | **Dossier & cache** | Dual-format output: structured header + cited prose + verbatim evidence appendix + license attribution; session evidence cache retained for follow-ups | no |

### Quality tiers (one architecture, configuration not code paths)

| Tier | Enables | LLM calls | Rerank | Trial-key reality |
|---|---|---|---|---|
| Quick | S1 intake + single-pass S3/S4 + one-shot S7 | ~2–3 | 1 | hundreds/month |
| **Balanced** (default) | + S2 planning, S5 gate, S8 critic | ~7–9 | 1–2 | ~100+/month |
| Deep | + HyDE, per-sub-question grading, per-section synthesis + repair | ~12–20 | 3–5 | ~30–50/month |

Model assignment (all configurable in settings): planner + synthesizer =
`command-a-03-2025`; router/grader/critic = `command-r7b-12-2024` with `json_schema`
(**live-verify R7B schema support in the Phase 3 spike**; fallback
`command-r-08-2024`); optional Deep reasoning = `command-a-reasoning-08-2025`.

**The app implements what it recommends** — ≥14 of its own corpus techniques run inside
this pipeline (fusion retrieval, RAG-fusion, HyDE, dartboard, adaptive retrieval,
reranking, RSE, contextual compression, reliable-RAG grading, CRAG, Self-RAG-style
critique, explainable retrieval, graph RAG, hierarchical indices, GroUSE-style inline
judging), and honors graph-encoded constraints (e.g. reranking `prerequisite_of` RSE).

**Judge validation**: S5/S8 are LLM judges — the corpus's own "unvalidated judge"
failure mode. Before confidence scores are shown as trustworthy, they get validated
against a small human-labeled set (GroUSE method) during Phase 4.

### Degraded modes (designed, not accidental)

No key / offline / quota exhausted → S0 + BM25 + graph expansion still produce ranked
technique results with full source browsing (packs are local); no LLM prose, labeled as
"local match". Per-minute throttles → queued pacing with progress UI, never silent
degradation. Quota meter warns before the monthly wall and explains tier costs.

---

## 4. Conversation & context management (v2)

Full design in [context-management.md](../research/context-management.md). Three-layer
context, asymmetric storage, routing before rewriting:

- **Three layers**: (1) **pinned anchor** — the user's original problem statement + hard
  constraints, verbatim, never truncated or paraphrased; (2) sliding window of the last
  ~3 raw exchanges; (3) a running summary, updated by **asynchronously folding evicted
  turns** after each turn commits (never summarize at the context cliff).
- **Routing before rewriting**: deterministic pre-filter (greetings, commands), then one
  combined router+rewriter `json_schema` call with 5 routes: `new_problem` /
  `followup_retrieve` / `followup_reuse` / `clarify_answer` / `meta`. Re-retrieve on any
  new symptom/constraint (a clarify answer can flip the remedy direction — the
  opposite-remedy trap); **reuse the conversation's cached candidate pool** when the
  user drills into already-presented techniques (local chunk fetches by slug are free).
- **Anti-repetition**: advisories are validated JSON already, so "what the advisor
  already said" compresses losslessly into a structured advisor-state object
  (recommended slugs + verdicts + user reactions) — ~10× smaller than prose replay.
- **Token budget** ~28k prompt tokens for R7B-facing calls (small models degrade well
  before nominal capacity): system 2k · ontology hints 1.5k · pinned problem 0.8k ·
  advisor state 0.8k · summary 1k · recent turns 4k · **evidence 14k** (absorbs unused
  history budget, never the reverse) · message 0.4k · 4k headroom. Synthesis calls on
  Command A get proportionally larger evidence budgets.
- **Turn cost**: retrieving turns cost 3–4 calls + occasional async summary/title calls
  (trial throughput ~230–250 Balanced queries/month — reflected in the quota meter).
- Conversation titles: one async R7B call after the first exchange; user rename wins;
  conversation search via local FTS5 over titles + turns + slugs (works offline).

---

## 5. Architecture

```
┌──────────────────────────── Compendium (Tauri 2) ───────────────────────────┐
│  WebView (React 19)                    Rust core (src-tauri)                │
│  ┌───────────────────────┐             ┌──────────────────────────────────┐ │
│  │ chat thread + dossier │  IPC typed  │ engine/                          │ │
│  │  (streamed sections,  │  commands + │   packs.rs    load/attach packs, │ │
│  │  span-highlighted     │  events     │               usearch view/heal  │ │
│  │  citations)           │◄───────────►│   search.rs   hybrid+RRF+graph   │ │
│  │ source viewer (ipynb, │             │   pipeline.rs S0–S9 state machine│ │
│  │  docs, highlight-to-  │             │   router.rs   pre-filter + routes│ │
│  │  citation)            │             │   context.rs  3-layer builder    │ │
│  │ history sidebar       │             │   cohere.rs   v2 REST client     │ │
│  │ settings panel        │             │               (typed: no schema  │ │
│  │ export dossier        │             │                +tools mixing),   │ │
│  └───────────────────────┘             │               pacing, meter      │ │
│                                        │   keys.rs     keyring (WinCred)  │ │
│                                        │   history.rs  app.db (WAL)       │ │
│                                        └──────────────────────────────────┘ │
│  resources/packs/*.pack (read-only)             %APPDATA%/compendium/app.db │
└──────────────────────────────────────────────────────────────────────────────┘

Offline (never on user machines):
pipeline/ (Python) — sources → processor (by source_type) → curation merge →
                     embed (Cohere, prod key) → usearch index build →
                     pack.db assemble → validate (incl. recall@10 gate)
```

Contribution model unchanged: open code-level extensibility with clean seams (processor
interface, pack schema version, renderer registry), documented in `CONTRIBUTING.md`
with the notebook processor as the worked example.

---

## 6. Knowledge pack format (v2 additions)

v1 schema (manifest / techniques / failure_modes / stages / relations / chunks /
embeddings / chunks_fts / documents) **plus**:

```
vector_indexes: tier (cards|chunks), usearch_version, connectivity, expansion_add,
                dims, metric, sha256, blob (serialized f16 HNSW index)
failure_mode phrasing embeddings  (powers the zero-LLM S0 matcher)
manifest additions: usearch_version, index_params, recall_at_10,
                    exact_slug_allowlist_ref (webdocs packs)
```

Pack-build enrichments recommended by the pipeline research: contextual chunk headers
on every chunk (eating our own dog food), card-summary embeddings as the primary
recommendation tier, pre-embedded failure-mode phrasings, optional HyPE-style
hypothetical questions (evaluate in Phase 2).

---

## 7. App data model (v2 — replaces the v1 `messages` sketch)

```
conversations:      id, title, created_at, updated_at, archived
turns:              id, conversation_id, role, content_md, advisory JSON,
                    citations JSON (span offsets → anchors), created_at
turn_traces:        turn_id 1:1, route, standalone_query, tier,
                    retrieval ids+scores per stage (never chunk texts — join by id),
                    token accounting per block, model ids, latencies, validation
                    outcomes; raw bodies only behind a default-off debug setting
summaries:          conversation_id, seq, content (append-only fold log)
conversation_state: conversation_id 1:1, pinned_problem, constraints JSON,
                    advisor_state JSON (slugs+verdicts+reactions),
                    candidate_pool JSON (ids+scores, stable order),
                    open_clarifying_question
turns_fts:          FTS5 over titles + turn content + recommended slugs
settings, pack_registry, quota_ledger (chat/rerank/embed calls per month)
```

History re-renders fully offline from stored advisory JSON + citations. If a pack is
upgraded, old traces that reference replaced chunks render with a "source updated"
notice (byte-level replay across pack upgrades is explicitly out of scope).

---

## 8. UX blueprint (v2 deltas)

Everything from v1 (three-pane layout, command palette, designed empty/error/loading
states, settings inventory, accessibility bar) plus:

- **Chat-first flow**: the thread is a conversation; each advisory renders as streamed
  dossier sections (per-section synthesis maps to per-section reveal). Clarifying
  questions render as a distinct, answerable inline prompt.
- **Span-accurate citations**: hovering/activating a citation highlights the exact
  character range in the source panel (Cohere span offsets → document anchors). Every
  recommendation card shows its confidence indicator (from S8) — with text + icon, not
  color alone.
- **Export dossier**: one click copies/saves the dual-format bundle (structured header,
  cited prose, verbatim evidence appendix with pack/technique/section anchors, license
  attribution block) — the hand-to-another-AI artifact.
- **Tier control**: Quick/Balanced/Deep selector on the composer (default Balanced),
  with per-query call estimate and the monthly meter; Deep tier streams stage progress
  ("planning → retrieving → grading → writing §2/4 → verifying") so tens-of-seconds
  latency feels intentional.
- **Honest-gap rendering**: a designed "the corpus doesn't cover this well" state with
  what *was* found and what's missing — never padded recommendations.
- Settings additions: advisor tier default, per-role model ids (advanced), clarifying
  questions on/off, debug traces toggle.

---

## 9. Build pipeline (v2 deltas)

- Notebook + webdocs processors as in v1; webdocs recipe now covers **three doc sets**
  (LangChain, LangGraph, LangSmith) with the 127-slug LangSmith allowlist checked into
  the recipe; reconciliation against GitHub `src/` listings (not llms.txt).
- New build steps: usearch index build (Python usearch, pinned version) + **recall@10 ≥
  0.98 validation gate**; failure-mode phrasing embeddings; contextual chunk headers.
- Refresh: monthly shared run, content-hash gated, loud-failure guardrails; pack
  versions `rag-techniques-YYYY.MM.N` / `framework-docs-YYYY.MM.N`.

---

## 10. Implementation phases (reviewable increments, sign-off gates kept)

| Phase | Deliverable | Review gate |
|---|---|---|
| 2 | `PACK_FORMAT.md` + pipeline core + notebook processor + **usearch spike** (Python-save → Rust-load on Windows, view latency, R7B json_schema live check) + built `rag-techniques.pack` | validator green incl. recall gate; spike numbers reviewed |
| 3 | Tauri skeleton + engine core: pack loading/healing, hybrid search + graph expansion (S0, S3, S4-local), keyring, typed Cohere client with pacing/meter | debug pane: sane ranked results; cold start < 1 s |
| 4 | Advisor pipeline S1–S9 + router/context manager + judge validation vs labeled set | end-to-end dossier on real queries; honest-gap path demonstrated |
| 5 | Design system + chat/dossier UI + source viewer with span highlighting + export | the product moment |
| 6 | Settings, themes, motion, sound, full a11y audit | a11y checklist signed off |
| 7 | `framework-docs.pack` (LC+LG+LS) + refresh workflow + `CONTRIBUTING.md` | second pack proves extensibility |
| 8 | NSIS installer (unsigned, per decision) + updater + QA/verify pass on clean Windows | installable build |

---

## 11. Decisions log

Resolved at v1 sign-off: name **Compendium** · no code signing · both packs bundled in
the installer · LangChain/LangGraph (now + LangSmith) pack in v1 · engine quality over
call count (tiers expose the tradeoff; trial keys remain supported with pacing + meter)
· real local vector store (usearch).

Defaults I'll proceed with unless you say otherwise: Balanced as the default tier ·
Deep tier available on trial keys (metered) · rerank is pipeline-internal (tier-driven),
not a standalone toggle.
