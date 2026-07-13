# Compendium Advisor Pipeline — Retrieval + Reasoning Design Survey and Recommendation

Date: 2026-07-13
Scope: full-landscape survey of retrieval+reasoning method families per pipeline part (2025–2026 literature + production systems + the app's own 39-technique corpus), followed by ONE composed pipeline recommendation with Quick / Balanced / Deep tiers.
Method: deep-research process — parallel search fan-out per pipeline part, primary-source fetches for load-bearing claims (Cohere docs, Google Research, Perplexity research blog, arXiv/ICLR), corpus read of `research/corpus/catalog.md` and `research/corpus/ontology.json`. Claims verified against at least one primary source are unmarked; single-source or secondary-source claims are flagged "(single source)".

---

## Executive summary

- **The naive pipeline loses in every part.** For each of the 11 parts surveyed there is a 2025–2026 method with documented gains over the naive step, and almost all of the winning methods are cheap for Compendium because retrieval is local (~10 ms) and only LLM/rerank calls cost anything.
- **The single biggest structural decision:** use the **deep-research / plan-and-execute pattern** (planner → parallel sub-question retrieval → coverage check loop → per-section synthesis → critic), not an interleaved ReAct/IRCoT token-loop. Interleaved loops exist to amortize *expensive* retrieval inside reasoning; with a 10 ms local corpus the economics invert — fan out retrieval deterministically and spend LLM calls only on planning, grading, synthesis, and verification.
- **The unusual asset changes three parts outright.** The failure-mode ontology (24 failure modes with `example_phrasings`) and typed relation graph (`composes_with` / `alternative_to` / `refines` / `prerequisite_of` / `evaluated_by`) give Compendium (a) zero-LLM ontology-guided query expansion, (b) zero-LLM graph expansion of candidates that similarity search would never surface (especially *alternatives* — mandatory for tradeoff sections), and (c) a deterministic "starved vs polluted" ambiguity detector that drives the clarifying-question policy. No off-the-shelf pipeline has these; they should be wired into intake, expansion, retrieval, and quality control.
- **Cohere's stack covers more than embedding + rerank.** The current rerank model is `rerank-v4.0-pro` (Cohere docs, July 2026); the Chat endpoint with Command-family models emits **native span-level citations** (`citation_options: accurate`), which directly implements the grounded-synthesis and citation-faithfulness parts with zero extra calls.
- **Honesty is a first-class output.** Google's ICLR 2025 "sufficient context" result shows strong models *hallucinate rather than abstain* even when context is insufficient; the pipeline therefore includes an explicit sufficiency gate per dossier section and an honest "the corpus lacks this" path (the GroUSE negative-rejection dimension — already in the corpus).
- **Recommended composition (Balanced tier): ~7–9 LLM calls + 1–2 rerank calls + 1 embed call per question**, all sharing one architecture with Quick (~2–3 LLM calls) and Deep (~12–20 LLM calls) as configuration, not separate code paths.
- **The app implements what it recommends.** The composed pipeline operationalizes ≥14 of the corpus's own 39 techniques (fusion retrieval, RAG-fusion, HyDE, dartboard, adaptive retrieval, reranking, RSE, contextual compression, reliable RAG, CRAG, Self-RAG, explainable retrieval, graph RAG, hierarchical indices, GroUSE-style inline judging) — a mapping table is included at the end.

---

## 0. Assets, constraints, and what they imply

| Fact | Design implication |
|---|---|
| Local SQLite corpus, 2–10k chunks (→50k later) | Brute-force or sqlite-vec dense search is milliseconds; **retrieval is effectively free**. Over-fetch aggressively (k=50–150 per query variant), run many query variants, filter later. |
| Curated technique cards + failure-mode ontology + typed relation graph | The corpus is not a bag of chunks; it is a **knowledge base with schema**. Retrieval should be hierarchical (card → chunk) and graph-informed, and expansion should be ontology-informed. |
| Only external API: Cohere (embed-v4.0 @1024d, rerank-v4.0-pro, Command A chat with native citations) | Every LLM step must map to Cohere Chat; rerank is a separate cheap call; native `citations` field replaces hand-rolled citation prompting. |
| Optimize for quality, not call count | Prefer batched LLM calls over skipped ones; but batching (one call grading 30 strips) is still better than 30 calls — quality per token matters, not asceticism. |
| Output is a dossier handed to another AI | Synthesis format must be dual-audience: human-readable prose + machine-stable IDs, verbatim excerpt appendix, and a structured recommendation header. |
| 24 failure modes with `example_phrasings` | Embed these at build time → a **local symptom→failure-mode matcher** that costs zero LLM calls and runs before any LLM sees the query. |

---

## Part 1 — Input understanding & routing

### Candidate landscape

| Method | Description | Evidence |
|---|---|---|
| **Single structured-output LLM triage call** (classify + extract + route in one) | One call returns query type, constraints, candidate failure modes, ambiguity flags, depth route | Production agentic-RAG guidance converges on an LLM router node as the entry state (LangGraph agentic RAG templates; TDS "Agentic RAG vs Classic RAG: from a pipeline to a control loop") |
| Trained lightweight classifier (Adaptive-RAG) | T5-Large (or TF-IDF+SVM) routes queries to no-retrieval / single-step / multi-step by predicted complexity | Adaptive-RAG (Jeong et al., NAACL 2024); RAGRouter-Bench (2026) found TF-IDF+SVM hits 93.2% routing accuracy with 28.1% token savings, and lexical features *beat* embeddings by 3.1 macro-F1 |
| Embedding-similarity routing (semantic router) | Route by nearest labeled intent centroid; zero LLM | Common in production (semantic-router libraries); cheap but coarse |
| Corpus: `adaptive_retrieval` | LLM classifies query as Factual/Analytical/Opinion/Contextual and routes to per-class retrieval strategies | catalog.md §4 |
| Self-RAG-style "retrieve or not" token | Model decides retrieval necessity during generation | Asai et al. 2023; corpus `self_rag` |
| Clarify-vs-proceed policies | Ask a clarifying question when a parsed field is below confidence threshold / when ambiguity spans divergent remedies; "clarify once, learn the default" | AWS ML blog on user-interaction RAG; TDS "When RAG Users Ask Vague Questions"; RAC (ECIR 2026) |

### Analysis for Compendium

- A trained router (Adaptive-RAG style) is the literature's cost-optimizer, but Compendium is not cost-bound at this step, has no training data, and its routing label space is bespoke (symptom / overview / comparison / follow-up / meta + starved-vs-polluted ambiguity). RAGRouter-Bench's own caveat applies: routers trained on query features alone ignore query–corpus compatibility, which is exactly what Compendium's ontology matcher provides for free.
- The killer feature the corpus enables: **run the local symptom→failure-mode matcher first** (embed the query, score against pre-embedded failure-mode descriptions + example phrasings), then hand the LLM triage call the top-scoring failure modes as *hypotheses to confirm/deny*. The LLM is grounded in the ontology's vocabulary instead of inventing its own taxonomy.
- Clarifying-question policy: the corpus itself documents that **context-starved and context-polluted symptoms are adjacent but take opposite remedies** (catalog.md "Expand vs. shrink"). That is a precise, checkable trigger: ask exactly one clarifying question when (a) the failure-mode posterior is split across opposite-remedy modes (fm-fragmented-context vs fm-context-noise), or (b) a constraint that flips recommendations (can re-index? latency budget? data locality?) is missing AND the depth tier is Balanced/Deep. Never ask more than one question per turn; long problem descriptions usually contain the answer — extract, don't ask (AWS guidance; underspecified-question detection work, arXiv 2602.11938).

### Winner
**One structured-output LLM "Intake Analyzer" call, primed with the locally-computed failure-mode shortlist**, returning: query class, extracted constraints (re-index feasibility, latency, privacy, data type, framework), confirmed failure modes, starved-vs-polluted flag, clarify-vs-proceed decision (with the single question if clarify), and depth route. One call does all of it — the ontology (24 modes with one-liners) fits comfortably in the prompt.

**Runner-up:** Adaptive-RAG-style trained classifier — revisit if per-question LLM latency at intake ever matters; rejected now for maintenance cost, no training data, and inability to do constraint extraction.
**Rejected:** embedding-only routing (too coarse for constraint extraction); Self-RAG retrieve-or-not (Compendium should essentially always retrieve — it is an advisor over a corpus; only `meta` queries skip retrieval).

---

## Part 2 — Query transformation & expansion

### Candidate landscape

| Method | Mechanism | Evidence / when it wins |
|---|---|---|
| Multi-query / RAG-Fusion | N paraphrases → parallel retrieval → RRF merge | Corpus `fusion_retrieval_with_llamaindex` (num_queries>1); DMQR-RAG (arXiv 2411.13154) shows *diverse* rewrites beat naive paraphrases; MDP-optimized multi-query beat HyDE by ~7% on HotPotQA (ACM CNML 2025) |
| Sub-question decomposition | Split compound problems into atomic sub-questions | Corpus `query_transformations`; exploration–exploitation decomposition work (arXiv 2510.18633); essential for comparison-type queries |
| HyDE | Embed a hypothetical answer document instead of the query | Corpus `hyde_hypothetical_document_embedding`; strongest on vocabulary mismatch; practitioner consensus (2025 comparisons) is that gains are corpus-dependent and it can *hurt* when the LLM's hypothetical drifts off-domain |
| Step-back prompting | Retrieve with a more abstract version of the question | Corpus `query_transformations`; good for symptom→principle mapping |
| Keyword/BM25-oriented reformulation | Extract exact terms, error codes, technique names for the sparse leg | DMQR-RAG includes keyword rewriting as one of its diverse strategies; needed because dense embeddings smooth over rare tokens (corpus fm-exact-term-miss) |
| **Ontology-guided expansion** | Map symptom language to failure-mode vocabulary; fan out to the mode's `example_phrasings` and member technique slugs as additional queries | OntologyRAG / KG-guided RAG literature (NAACL 2025 KG²RAG: seed chunks + graph-guided expansion; ontology-guided evidence-path inference, arXiv 2606.28076) — Compendium has this graph curated, no extraction needed |
| Index-time mirrors (HyPE, document augmentation, contextual chunk headers) | Pay the expansion cost at build time | Corpus "index-time vs query-time mirror pairs" — Compendium controls pack builds, so these are complements, not query-time competitors |

### Analysis for Compendium

- The user's queries are *symptom descriptions*; the corpus is *technique prose*. This is the maximal vocabulary-mismatch regime — expansion is not optional.
- The ontology gives an expansion channel nobody else has: matched failure mode `fm-fragmented-context` deterministically yields queries like "retrieved chunks are cut off mid-sentence…" (its example phrasings) plus targeted card lookups for its 8 member techniques. **Zero LLM calls, high precision, and it speaks the corpus's exact vocabulary** — the same trick HyPE plays at index time, but curated instead of generated.
- LLM expansion still earns its call for what the ontology can't do: paraphrase diversity, decomposition of long multi-part problem descriptions, and step-back abstraction ("my top-k results repeat themselves" → "diversity-aware retrieval selection").
- HyDE's known failure mode (hypothetical drift) is mitigated here because the Intake call can instruct HyDE generation to *sound like a technique card* — a domain-anchored hypothetical. Still, keep it Deep-tier only; multi-query + ontology fan-out covers most of its value at this corpus scale.

### Winner
**Ontology-first hybrid expansion**: (1) deterministic fan-out from matched failure modes (phrasings + member-technique queries, zero LLM); (2) one LLM "Query Planner" call producing 3–5 diverse rewrites (paraphrase, step-back, keyword-oriented) and sub-questions for compound problems, DMQR-RAG-style (diversity-constrained, not N near-duplicates). All variants → parallel local retrieval → RRF (Part 3).

**Runner-up:** pure RAG-Fusion multi-query (works everywhere, ignores the ontology asset).
**Deep-tier addition:** domain-anchored HyDE (one extra call).
**Rejected as query-time steps:** HyPE/document augmentation (adopt at pack build time instead); MemoRAG (its value — corpus-level memory — is subsumed by the curated catalog itself).

---

## Part 3 — Retrieval

### Candidate landscape

| Method | Notes |
|---|---|
| Dense-only (embed-v4.0 cosine) | Baseline; misses exact terms (corpus fm-exact-term-miss) |
| Sparse-only (SQLite FTS5 BM25) | Misses paraphrase; free in SQLite |
| **Hybrid dense+sparse, RRF fusion** | 2025–2026 production consensus: RRF over rank positions avoids score-scale calibration entirely; tuned hybrid gave +7.4% NDCG over either leg alone on WANDS; "single score-weighted formula combining BM25 and cosine is the architectural mistake most failed implementations share" (multiple 2025–2026 practitioner benchmarks) |
| Weighted/learned score fusion | Corpus `fusion_retrieval` uses tunable weighted sum; can beat RRF *when tuned per-corpus*, but needs a tuning harness and recalibration per pack |
| Metadata/stage-filtered retrieval | Filter/boost by ontology stage, complexity, failure-mode tags — trivial in SQLite WHERE clauses |
| **Hierarchical (card → chunk)** | Corpus `hierarchical_indices`; retrieve over card-level summary embeddings first, then chunks within top cards, union with global chunk search |
| **Graph-informed expansion** | Corpus `graph_rag` family; KG²RAG (NAACL 2025) pattern: retrieved seeds → expand via graph edges → rerank. Compendium's graph is *curated and typed*, so expansion is a SQL join, not LLM extraction |
| Multi-vector / late interaction (ColBERT-style) | Better fine-grained matching; heavy for a Tauri-shipped app, and rerank-v4.0-pro recovers most of the precision |

### Analysis for Compendium

- **RRF wins the fusion question** for a multi-query pipeline specifically: with 5–10 query variants each producing dense+sparse lists, RRF merges *all* lists uniformly with one hyperparameter (k≈60), which is exactly the RAG-Fusion formulation. Weighted fusion would need per-leg calibration that breaks every time a new pack ships. (Weighted fusion remains the corpus's `fusion_retrieval` default; document the divergence and the reason.)
- **Hierarchical retrieval matters because the deliverable is technique-centric.** The dossier recommends *techniques*, then cites *sections*. Retrieving cards first (39 now, hundreds later) and chunks second matches the output structure and prevents one verbose notebook from flooding the candidate pool.
- **Graph expansion is the highest-leverage nonstandard step.** For every candidate technique above threshold, pull 1-hop neighbors with edge-type-aware intent: `alternative_to` neighbors feed the dossier's alternatives/tradeoffs section; `composes_with`/`prerequisite_of` neighbors feed the composition-pipeline section (e.g., RSE requires reranking — the graph encodes this as `reranking prerequisite_of relevant_segment_extraction`); `refines` finds upgrades; `evaluated_by` attaches the right eval harness to every recommendation. Similarity search alone would rank alternatives *low* precisely because they solve the problem differently — the graph is the only reliable source of them.
- Constraint filters from intake should **demote, not delete** (e.g., "cannot re-index" demotes index-time techniques but keeps them visible as "if you can rebuild the index…" options) — hard filtering fights the advisor's job of explaining tradeoffs.

### Winner
**Hierarchical hybrid retrieval with RRF fusion and typed-graph expansion**: per query variant, dense (sqlite-vec / brute-force) + FTS5 BM25 at both card and chunk level, RRF-merge all lists (k=60), over-fetch ~100–150 unique chunks, then 1-hop typed-graph expansion of above-threshold cards, then soft metadata boosts/demotions from constraints. All local, all sub-50 ms at 10k chunks.

**Runner-up:** flat hybrid + rerank (simpler; loses the alternatives/prerequisites channel and card-level structure).
**Rejected:** learned fusion (tuning burden per pack), late interaction (weight/complexity for marginal gain under a strong reranker).

---

## Part 4 — Ranking & selection

### Candidate landscape

| Method | Notes |
|---|---|
| Cross-encoder rerank (Cohere `rerank-v4.0-pro`) | Current latest Cohere rerank model (Cohere docs, 2026); handles semi-structured JSON docs — useful for card metadata; typical uplift NDCG@10 +0.05–0.08, up to +0.10–0.15 on domain documents; known weak spot: identifier-heavy queries (practitioner evals, 2026) |
| MMR | Classic relevance–diversity tradeoff at selection |
| **Dartboard** | Greedy joint relevance+diversity over an oversampled pool; corpus `dartboard`; strictly better formulation than MMR for "stop repeating yourself" |
| **Adaptive-k / dynamic selection** | Adaptive-k (Taguchi et al. 2025, arXiv 2505.07233): choose k from the similarity-score distribution; DPS (arXiv 2508.09497) selects minimal sufficient subset; DynamicRAG (2025) RL-trained; score-adaptive truncation τ = α·max-score replaces per-dataset k tuning (2025–2026) |
| Relevant Segment Extraction (RSE) | Corpus `relevant_segment_extraction`: rebuild contiguous multi-chunk segments around clusters of relevant chunks; consumes reranker scores (graph: reranking is its prerequisite) |
| Cross-sub-query dedup | Chunk-ID dedup + near-dup suppression before rerank; RRF already handles list-level merging |
| LLM listwise reranking | Highest quality ceiling, expensive; subsumed by the Part-5 evidence grading call |

### Analysis for Compendium

- Rerank the ~100–150-chunk pool against the **canonical rewritten problem statement** (one rerank call). In Deep tier, additionally rerank per sub-question (2–4 more rerank calls) so each dossier section gets its own precision ordering. Rerank calls are cheap relative to generation; the identifier-weakness caveat is minor here (queries are symptom prose, not function names).
- Fixed k is the documented failure (corpus fm-low-rank / fm-context-noise pull in opposite directions). Use **score-adaptive truncation** (keep chunks with rerank score ≥ α·max, plus a hard floor/ceiling) — one universal α instead of per-query k tuning, per 2025–2026 dynamic-selection results.
- Then **dartboard-style greedy selection** locally (embeddings are already on disk) over the truncated pool, with a twist: diversify across *technique cards and failure modes*, not just embedding space, so the evidence set covers multiple candidate remedies instead of six excerpts from the same notebook.
- Finally **RSE**: within each surviving document, merge adjacent relevant chunks into contiguous segments (scores already available from the reranker). This directly implements the corpus's own canonical upgrade pipeline (`fusion_retrieval → reranking → relevant_segment_extraction`).

### Winner
**Rerank-v4.0-pro → score-adaptive truncation → card/failure-mode-aware dartboard selection → RSE segment reconstruction.** One rerank API call (Balanced), all other steps local.

**Runner-up:** rerank + fixed k=10 + MMR (the 2024 default; measurably worse on redundancy and adaptivity).
**Rejected:** RL-trained DynamicRAG (training burden), LLM listwise rerank as a separate step (folded into Part-5 grading instead).

---

## Part 5 — Retrieval quality control

### Candidate landscape

| Method | Notes |
|---|---|
| **CRAG-style retrieval evaluator** | Grade evidence Correct/Incorrect/Ambiguous; decompose-then-recompose into knowledge strips, score strips, drop irrelevant parts (Yan et al., arXiv 2401.15884; up to +36.6% on factual benchmarks). Original uses fine-tuned T5; production versions use LLM grading (LangGraph CRAG template) |
| LLM relevance grading (Reliable-RAG) | Corpus `reliable_rag`: per-doc relevance gate + groundedness check + verbatim support extraction |
| **Sufficient-context gate** | ICLR 2025 (Google, arXiv 2411.06037): classify whether retrieved context *suffices to answer*, not merely whether it's relevant; models hallucinate rather than abstain without it; selective generation with this signal improves correct-answer fraction 2–10% |
| Confidence thresholds on rerank scores | Zero-LLM heuristic gate; Quick-tier substitute |
| Re-query loops on low coverage | CRAG's corrective action; in Compendium the "fallback" is not web search (offline corpus) but reformulation + graph-neighbor expansion + card-level backoff |
| Honest insufficiency ("corpus lacks this") | GroUSE (COLING 2025; corpus `evaluation_grouse`) defines positive acceptance / **negative rejection** as first-class metrics — the advisor must be able to say "no pack covers this" |

### Analysis for Compendium

- Batch the grading: **one LLM call grades all selected segments against all sub-questions simultaneously** (structured output: per-segment relevance, per-sub-question sufficiency verdict, missing-aspect notes). This is CRAG's evaluator + knowledge-strip scoring + the sufficient-context signal in a single call, at 2025 model quality (no T5 fine-tune needed).
- On insufficiency: run **one corrective loop** (Balanced) or up to two (Deep): take the grader's missing-aspect notes → generate targeted reformulations (this can reuse the Query Planner call format) → re-retrieve locally → re-grade only the new material. Retrieval being free makes corrective loops nearly pure-quality upside; only the grading call costs.
- If still insufficient after the loop budget: **do not force an answer.** Write the gap into the dossier explicitly ("The corpus does not cover streaming ingestion; nearest covered topics are X, Y"), which is both honest (sufficient-context finding) and useful to the downstream AI consuming the dossier. This is CRAG's "Incorrect" branch with honesty replacing web-search fallback.

### Winner
**Batched CRAG-style evidence grading with an explicit sufficient-context verdict per sub-question, one corrective re-query loop, and a first-class honest-gap output path.** Quick tier replaces the LLM grade with rerank-score thresholds only.

**Runner-up:** Reliable-RAG per-doc gating (the same idea, un-batched and without the sufficiency lens).
**Rejected:** fine-tuned evaluator models (maintenance, no training data), web-search fallback (violates the offline/Cohere-only constraint).

---

## Part 6 — Iterative & agentic orchestration

### Candidate landscape

| Pattern | Mechanism | Fit for a 10 ms local corpus |
|---|---|---|
| ReAct tool loop | LLM interleaves thought/act/observe, retrieval as a tool | Flexible; but every retrieval costs a full LLM turn — pays the LLM tax to trigger a free operation |
| IRCoT | Interleave CoT steps with retrieval | Same economics as ReAct; designed for expensive/multi-hop web retrieval |
| Self-ask | Decompose into follow-up questions sequentially | Sequential; subsumed by up-front decomposition when sub-questions are independent |
| FLARE / DRAGIN | Retrieval triggered by token-level uncertainty during generation | Needs logprob access mid-generation; awkward over a REST chat API |
| **Plan-and-execute / deep-research** | Planner → parallel searchers → synthesizer → critic; explicit gap-check loop | Anthropic's multi-agent research system (2025): orchestrator + parallel subagents beat single-agent Opus 4 by 90.2% on research evals (at ~15× tokens); LangChain Open Deep Research is built exactly this way; the pattern spends LLM calls on *planning and synthesis*, retrieval fanned out in parallel — matching Compendium's cost structure |
| Adaptive stopping | Stop-RAG (value-based, arXiv 2510.14337), TASR (training-free adaptive stopping, 2026): fixed iteration counts waste compute on easy queries and stop early on hard ones |
| SoK taxonomy | Agentic-RAG SoK (arXiv 2603.07379) — 2026 systematization confirming the "predefined-workflow vs autonomous-agent" split and that component ablations matter more than agent autonomy for local-corpus QA (component-ablation study, arXiv 2606.21553) |

### Analysis for Compendium

- Interleaved loops (ReAct/IRCoT/FLARE) solve a problem Compendium doesn't have: deciding *whether* the next expensive retrieval is worth it. Compendium's retrieval is free; its scarce resource is LLM calls. The deep-research pattern concentrates LLM spend where it changes the answer: one plan, parallel free retrieval, one grade, sectioned synthesis, one critique.
- Because sub-questions run against the same local SQLite file, "parallel searchers" are just concurrent Rust functions, not subagent LLMs — Compendium gets the multi-agent fan-out benefit **without** multi-agent token multiplication. (Anthropic's 15× token cost was for web research subagents that each reason; local search needs no reasoning to execute.)
- Stopping criteria (per Stop-RAG/TASR framing, adapted): stop the corrective loop when (a) **coverage**: every planned dossier section has sufficient graded evidence; (b) **convergence**: an iteration produced no new unique chunks; (c) **budget**: tier-defined max iterations (Quick 0, Balanced 1, Deep 2–3). All three are cheap local checks plus the existing grading call.

### Winner
**Deep-research-shaped orchestration with deterministic parallel retrieval**: Intake → Plan (outline + sub-questions) → parallel local retrieval fan-out → rerank/select → batched grade → corrective loop (coverage/convergence/budget stopping) → per-section synthesis → critic. A fixed LangGraph-style state machine (implemented in Rust), not an autonomous agent — the 2026 SoK and ablation literature supports structured workflows over free-form agency for closed-corpus QA.

**Runner-up:** ReAct loop with retrieval tools (choose it only if Compendium later adds *expensive* tools, e.g., optional web search).
**Rejected:** FLARE/DRAGIN (logprob plumbing over REST), sequential self-ask (loses parallelism), autonomous multi-agent (token multiplication without local benefit).

---

## Part 7 — Context assembly

### Candidate landscape & evidence

- **Lost-in-the-middle is real and persistent**: U-shaped attention (Liu et al., TACL 2024) reconfirmed through 2025 ("Principled Context Engineering for RAG", arXiv 2511.17908; ICLR 2025 long-context-meets-RAG: more retrieved passages ≠ better answers); mitigations: retrieval reordering / sandwich placement (strongest evidence at start and end), position engineering, compression.
- **Compression options**: LLM extractive compression (corpus `contextual_compression` — one LLM call per chunk, expensive); Provence (ICLR 2025, arXiv 2501.16214) — a DeBERTa sentence-labeling pruner unified with reranking, threshold transferable across datasets; LLMLingua-style token pruning (needs a compression-ratio knob, harder to tune). Local ONNX Provence-style pruning is feasible in Rust but adds a model dependency.
- **Structured evidence formatting**: 2025–2026 context-engineering guidance converges on structured, labeled, per-source blocks with stable IDs over concatenated prose; Cohere's RAG API takes structured `documents` natively and cites by document/span.
- **Per-source budgeting**: cap tokens per technique card so one verbose source can't crowd out alternatives (the dartboard selection already fights this at selection time; the budget enforces it at assembly time).

### Analysis for Compendium

- Per-section synthesis (Part 8) is itself the strongest lost-in-the-middle mitigation: each generation call sees only ~5–15 segments, inside the high-attention regime. Ordering within a section: highest-graded evidence first, second-best last (sandwich), neighbors in the middle.
- **Prefer RSE + adaptive-k truncation over aggressive compression.** The dossier's export use case wants faithful, contiguous excerpts (another AI will re-read them); compressing them lossy-ly damages the artifact. Compress only on budget overflow, extractively (drop lowest-graded sentences per the grading call's strip scores — no extra LLM call).
- Evidence block format (assembled locally, zero LLM):

```
[E17] card=relevant_segment_extraction (stage=post-retrieval, complexity=medium)
      failure_modes=[fm-fragmented-context, fm-cost-budget]
      relations: prerequisite reranking; alternative_to contextual_compression
      source=notebook §"Segment scoring"  anchor=pack1/rse/sec4  scores: rerank=0.91 grade=SUPPORTS(q2)
      text: "..."
```

### Winner
**Structured per-card evidence blocks with stable anchors + per-source token budgets + sandwich ordering inside per-section prompts; RSE-first, compression only on overflow (extractive, reusing grading-call strip scores).**

**Runner-up:** Provence-style local pruner (adopt later if packs grow verbose); **Rejected:** per-chunk LLM compression calls (cost without quality gain given RSE), whole-context single-prompt assembly (lost-in-the-middle).

---

## Part 8 — Grounded synthesis for a long-form dossier

### Candidate landscape

| Method | Evidence |
|---|---|
| Single-shot long generation | Baseline; degrades with context length and section count; citation drift documented in long-form RAG (FACTUM, arXiv 2601.05866: report generation is significantly harder than single-passage QA) |
| **Outline-then-write / plan-then-write, per-section generation** | The dominant 2025–2026 long-form pattern: oRAG, RaPID (arXiv 2503.00751), WebWeaver (arXiv 2509.13312 — dynamic outlines + per-section evidence binding), ScaffoldAgent (2026, utility-guided outline optimization), Deep-Reporter (2026) |
| Extract-then-abstract | Quote verbatim support, then abstract over it; corpus `reliable_rag` extracts verbatim segments; pairs with the export use case |
| Self-RAG-style support/critique tokens | Reflection tokens grading each generated segment (Asai et al. 2023); modern equivalent: separate critic pass (Part 9) rather than special tokens, since Compendium uses a hosted model |
| **Native grounded generation (Cohere Chat `documents` + citations)** | Command-family models emit fine-grained span-level citations out of the box; `citation_options={"mode":"accurate"}` for precise alignment (Cohere docs). Removes the least-reliable part of hand-rolled citation prompting |
| Structured/machine-readable output header | No single canonical citation; 2026 deep-research implementations and the hand-to-another-AI requirement both push toward a dual-format artifact |

### Analysis for Compendium

- The dossier has a natural fixed skeleton the planner instantiates: **Problem restatement & diagnosed failure modes → Recommended techniques (ranked, each: what/why/tradeoffs/how-it-composes/evidence) → Alternatives considered (from `alternative_to` edges) → Composition pipeline (from `composes_with`/`prerequisite_of` edges) → How to evaluate (from `evaluated_by` edges) → Coverage gaps & confidence → Evidence appendix.** Outline-then-write with per-section evidence binding (WebWeaver's core finding: bind evidence to outline nodes, don't pool it) maps 1:1 onto this.
- Per-section generation calls use Cohere Chat with the section's evidence blocks as `documents`, native citations on. Section calls can run concurrently (independent evidence sets); a final cheap stitch is local string assembly plus one optional coherence pass in Deep tier.
- **Export format (deliberate design for the hand-to-another-AI case):**
  1. A fenced YAML header: ranked recommendations with technique slugs, failure modes addressed, confidence scores, constraint caveats, citation keys.
  2. Prose sections with inline `[E17]`-style citation keys (rendered from Cohere's citation spans).
  3. A verbatim **evidence appendix**: every cited excerpt in full with its stable anchor (`pack/technique/section`) so the downstream AI can verify claims without corpus access, plus deep links for the human to open the exact notebook section.
  4. Attribution footer (NirDiamant/RAG_Techniques, modified content, non-commercial license — a hard license requirement).

### Winner
**Outline-then-write with per-section grounded generation via Cohere native citations, extract-then-abstract evidence appendix, dual human/machine format.**

**Runner-up:** single-call generation with Self-RAG-style self-critique inside the same prompt (Quick tier actually uses this: one synthesis call, fixed short outline).
**Rejected:** free-form single-shot long generation (citation drift, lost-in-the-middle), fine-tuned citation models (unavailable, unnecessary given Cohere's native support).

---

## Part 9 — Verification & quality scoring

### Candidate landscape

- **RAGAS-style faithfulness, run inline**: decompose the answer into atomic claims, verify each against retrieved context, score = supported/total (RAGAS docs & 2026 guides). Designed for offline eval but directly executable as an answer-time critic call.
- **GroUSE dimensions** (COLING 2025; corpus `evaluation_grouse`): positive acceptance, negative rejection, faithfulness, usefulness — the negative-rejection lens is what makes the honest-gap path testable.
- **Citation-faithfulness detection**: CiteCheck (arXiv 2502.10881), eTracer claim-level grounding (2026), FACTUM (mechanistic citation-hallucination detection in long-form RAG, 2026) — all confirm that a *substantial fraction* of cited statements are unsupported in long-form RAG, i.e., a verification pass is not optional for a citation-centric product.
- **Self-critique/refinement loops**: critic → targeted repair of failing sections only (not full regeneration); standard in 2026 deep-research implementations (LangChain Open Deep Research includes a reflection/critic node).
- **Per-recommendation confidence**: composite local score — retrieval coverage (graded sufficiency), rerank score mass, evidence diversity (number of independent sources), critic support rate. Surfacing calibrated-ish confidence per recommendation follows the sufficient-context selective-generation result (accuracy/coverage tradeoff made explicit).
- Structural advantage: Cohere's native citations make claim→span alignment *machine-checkable locally* — every cited span either exists in the evidence or doesn't (string-level check, zero LLM); the LLM critic then only judges *semantic* support, entailment-style.

### Winner
**Two-layer verification: (1) local citation integrity check (every citation resolves to a real span — zero LLM); (2) one batched critic call scoring claim-level support (RAGAS-faithfulness formulation) + GroUSE-style negative-rejection check on gap statements; targeted repair regeneration only for sections with unsupported claims (Deep tier; Balanced flags them in the confidence block instead).** Per-recommendation confidence = weighted composite of coverage, rerank mass, source diversity, critic support — displayed in the dossier header and UI.

**Runner-up:** Self-RAG-style in-generation reflection (cheaper, weaker — the generator grading itself in the same call is the known judge-validity trap the corpus's `evaluation_grouse` exists to catch).
**Rejected at answer time:** full RAGAS/DeepEval suites (offline regression harness instead — corpus `end-2-end_rag_evaluation` pattern), mechanistic detectors like FACTUM (need model internals).

---

## Part 10 — Multi-turn conditioning

### Candidate landscape & evidence

- **Contextualized standalone rewriting** remains the 2025–2026 consensus mechanism (ChatQA; SemEval-2026 Task 8 systems: hybrid retrieval + controlled query rewriting + cross-encoder reranking ranked top-3 of 38 teams; production data: >60% of follow-ups contain unresolved coreference — Amazon Science 2025).
- **Retrieval reuse**: SemEval-2026 winners explicitly *reused documents retrieved for earlier turns* for follow-up answer generation; H-RAG (2026) uses hierarchical parent-child retrieval across turns.
- Follow-up typology for an advisor chat: **drill-down** ("how do I implement #2 in Rust?") → reuse cached evidence, retrieve deeper within the same cards; **pivot** ("what about latency instead?") → fresh plan, fresh retrieval, carry constraints forward; **meta** ("shorten that", "export the dossier") → no retrieval.

### Winner
**Fold standalone rewriting into the Intake Analyzer call (it already sees history) + a session evidence cache keyed by technique card**: intake classifies drill-down/pivot/meta; drill-downs skip Parts 2–5 for cached cards and only run incremental retrieval within them; constraints extracted in earlier turns persist in session state and keep conditioning ranking. Zero additional LLM calls versus single-turn.

**Runner-up:** always-fresh full pipeline per turn (simpler, wastes the cache, slower answers on drill-downs).

---

## Part 11 — What production systems actually do in 2026

- **Anthropic multi-agent research** (2025, engineering blog; verified via secondary write-ups): orchestrator–worker pattern, parallel subagents with separate context windows, explicit synthesis stage; 90.2% improvement over single-agent Opus 4 on internal research evals at ~15× token cost; their stated lesson — "agent architecture is a token-spending strategy." Compendium borrows the *shape* but replaces LLM subagents with free local searches.
- **LangChain Open Deep Research + LangGraph templates**: open-source deep research is a LangGraph graph (scope → plan → parallel research → synthesize → reflect); LangGraph also ships CRAG, Self-RAG, and agentic-RAG reference graphs — the same corpus techniques Compendium recommends, as production templates. LangSmith methodology: separate retrieval-quality from generation-quality evaluators; trace per-node scores (chunk scores pre/post rerank, iteration counts, token budgets) — adopt this telemetry schema in Compendium's local traces.
- **Perplexity** (research.perplexity.ai, 2025–2026): multi-stage progressive refinement — hybrid lexical+semantic candidate generation prioritizing *comprehensiveness*, prefilters, progressively heavier rankers ending in cross-encoders, scoring at document *and sub-document* granularity ("most atomic units possible"); 2026's "Search as Code": a model assembles retrieval primitives into a per-request pipeline. Compendium's intake-routed, per-query-shaped pipeline is the small-scale analog of that per-request assembly idea.
- **Cohere's own RAG guidance**: two-stage retrieval (embed → rerank) + Chat-with-documents + native fine-grained citations as the reference architecture; `rerank-v4.0-pro` is the current top rerank model; Command A is the current flagship for RAG/agentic use (Command A+ reported May 2026 — single source).
- **Google research**: sufficient-context selective generation (ICLR 2025) — production-relevant honesty gating, adopted in Part 5.
- Convergent production pattern across all of the above: **hybrid retrieval → cross-encoder rerank → LLM orchestration as an explicit graph → grounded generation with citations → separate evaluation of retrieval vs generation.** The composed pipeline below is that consensus plus Compendium's ontology/graph asset.

---

# THE RECOMMENDATION — Compendium Advisor Pipeline v1

One architecture, three tiers. Stages marked **[LLM]** are Cohere Chat calls, **[RERANK]** Cohere Rerank calls, **[EMB]** Cohere embed calls, **[local]** pure Rust/SQLite.

```
                         ┌──────────────────────────────────────────────┐
 user turn ─► S0 [local] │ symptom→failure-mode match (pre-embedded     │
                         │ ontology) + session cache lookup             │
                         └──────────────┬───────────────────────────────┘
                                        ▼
              S1 [LLM] INTAKE ANALYZER (1 structured call)
              class · constraints · failure modes confirmed · starved-vs-polluted
              · clarify? (≤1 question) · route: Quick/Balanced/Deep · standalone rewrite
                       │ (clarify → ask & wait)          │ meta → answer, no retrieval
                       ▼
              S2 [LLM] QUERY PLANNER (1 call; merged into S1 in Quick)
              dossier outline · sub-questions · 3–5 diverse rewrites
              (+ ontology fan-out queries added locally, zero LLM; + HyDE in Deep)
                       ▼
              S3 [EMB][local] RETRIEVAL FAN-OUT (parallel, ~ms)
              1 batched embed call for all variants → per-variant hybrid search:
              card-level + chunk-level, dense (sqlite-vec) + FTS5 BM25 → RRF(k=60)
              → 1-hop typed-graph expansion (alternative_to / composes_with /
              prerequisite_of / refines / evaluated_by) → constraint boosts/demotions
                       ▼
              S4 [RERANK][local] RANK & SELECT
              rerank-v4.0-pro vs canonical problem (1 call; +per-sub-question in Deep)
              → score-adaptive truncation (α·max) → dartboard selection diversified
              across cards & failure modes → RSE segment reconstruction
                       ▼
              S5 [LLM] EVIDENCE GRADER (1 batched call; skipped in Quick → thresholds)
              per-segment relevance + per-sub-question SUFFICIENT/INSUFFICIENT
              + missing-aspect notes (CRAG evaluator + sufficient-context gate)
                       │ insufficient & loop budget left
                       ├────────► corrective loop: targeted reformulation → S3 → S5
                       │          stop on coverage ∨ convergence ∨ budget
                       ▼ still insufficient → mark section as HONEST GAP
              S6 [local] CONTEXT ASSEMBLY
              structured evidence blocks w/ stable anchors · per-card token budget
              · sandwich ordering · extractive trim only on overflow
                       ▼
              S7 [LLM×n] SECTIONED SYNTHESIS (outline-then-write, concurrent)
              per-section Chat call with `documents` + native accurate citations;
              Quick = 1 combined call; Balanced ≈ 2–4 merged sections; Deep = every section
                       ▼
              S8 [local]+[LLM] VERIFY & SCORE
              local citation-integrity check (0 LLM) → 1 batched critic call
              (claim-level support, RAGAS-faithfulness; GroUSE negative-rejection on gaps)
              → Deep only: targeted repair regeneration of failing sections
                       ▼
              S9 [local] DOSSIER EMIT
              YAML recommendation header · cited prose · verbatim evidence appendix
              w/ pack/technique/section anchors · per-recommendation confidence
              · license attribution · cache evidence for follow-ups
```

### Per-stage: winner, justification, runner-up

| Stage | Chosen method | Why (one line) | Runner-up |
|---|---|---|---|
| S0 | Local ontology matcher (pre-embedded failure modes) | Zero-cost corpus-vocabulary prior grounding everything downstream | LLM-only classification |
| S1 | Single structured intake call, ontology-primed; ≤1 clarifying question on opposite-remedy ambiguity or recommendation-flipping missing constraint | One call covers class+constraints+route; ontology makes the label space checkable | Adaptive-RAG trained router |
| S2 | Ontology fan-out (free) + DMQR-style diverse multi-query + decomposition; HyDE Deep-only | Max recall against symptom↔technique vocabulary gap; diversity beats N paraphrases | Pure RAG-Fusion |
| S3 | Hierarchical hybrid (card→chunk) + RRF + typed-graph 1-hop expansion; demote-don't-delete constraint filters | RRF is the calibration-free production consensus; the graph is the only source of alternatives/prerequisites | Flat hybrid + rerank |
| S4 | rerank-v4.0-pro → adaptive-k (score-adaptive τ) → card-aware dartboard → RSE | Fixed k is the documented failure; diversity across remedies is the advisor's job; RSE rebuilds readable sections | Fixed k=10 + MMR |
| S5 | Batched CRAG-grade + sufficient-context verdict + 1–3 corrective loops + honest-gap path | Models hallucinate instead of abstaining without a sufficiency gate (ICLR 2025); local re-query is free | Reliable-RAG per-doc gates |
| S6 | Structured evidence blocks, per-card budgets, sandwich order, RSE-first / trim-on-overflow | Lost-in-the-middle mitigation without damaging the export artifact | Provence-style local pruner |
| S7 | Outline-then-write, per-section Cohere native-citation generation, extract-then-abstract appendix | The 2025–2026 long-form consensus; native citations remove the flakiest prompt engineering | Single call + self-critique |
| S8 | Local citation-integrity check + batched claim-level critic + Deep-tier targeted repair; composite per-recommendation confidence | Long-form RAG citation drift is measured and real; string-level checks are free | In-generation Self-RAG reflection |
| S9 | Dual-format dossier (YAML header + cited prose + verbatim appendix + anchors) | Designed artifact for the hand-to-another-AI use case | Prose-only report |
| Multi-turn | Rewrite inside S1 + session evidence cache; drill-down/pivot/meta routing | Zero extra calls; SemEval-2026 winners reuse prior-turn retrievals | Fresh pipeline every turn |

### Decision points (explicit branches)

1. **S1: clarify vs proceed** — ask ≤1 question only if (starved-vs-polluted split) ∨ (missing recommendation-flipping constraint ∧ tier ≥ Balanced); otherwise proceed with stated assumptions listed in the dossier header.
2. **S1: meta/no-retrieval** — meta turns (formatting, export, "explain your last answer") bypass S2–S8.
3. **S5: corrective loop** — loop while (any section INSUFFICIENT) ∧ (new chunks last iteration) ∧ (iterations < tier budget).
4. **S5→S9: honest gap** — sections still insufficient are written as explicit coverage gaps with nearest-covered-topic pointers, never padded.
5. **S8: repair** — Deep only: regenerate a section iff critic finds unsupported claims; Balanced surfaces them as low-confidence flags.
6. **Multi-turn: drill-down** — reuse cached evidence, run S3 only *within* cached cards; **pivot** — full pipeline with carried constraints.

### Call budget per user question

| Tier | LLM (Chat) calls | Rerank | Embed | Loops | Typical use |
|---|---|---|---|---|---|
| **Quick** | 2–3 (intake+plan merged; 1 synthesis w/ citations; optional micro-critic) | 1 | 1 (batched) | 0 | short symptom, fast answer |
| **Balanced** (default) | 7–9 (intake 1 · plan 1 · grade 1–2 · synthesis 2–4 · critic 1) | 1–2 | 1–2 | ≤1 | normal advisory turn |
| **Deep** | 12–20 (+HyDE 1 · per-sub-question grading · per-section synthesis 4–8 · critic+repair 2–3) | 3–5 | 2–3 | ≤3 | long problem dossier, export-grade |

Tiers share every stage; deeper tiers only *enable more branches* (HyDE, per-sub-question rerank, LLM grading vs thresholds, per-section vs merged synthesis, repair loop). Router picks the tier; user can override.

### "Implements what it recommends" — corpus technique map

| Pipeline stage | Corpus techniques operationalized |
|---|---|
| S0–S1 routing | `adaptive_retrieval` (query-type routing) |
| S2 expansion | `query_transformations` (rewrite/step-back/decompose), `hyde_hypothetical_document_embedding` (Deep), RAG-Fusion via `fusion_retrieval_with_llamaindex` num_queries pattern |
| S3 retrieval | `fusion_retrieval` (hybrid dense+BM25), `hierarchical_indices` (card→chunk), `graph_rag` / `graphrag_with_milvus_vectordb` spirit (typed-graph expansion over the technique-relation graph) |
| S4 ranking | `reranking`, `dartboard`, `relevant_segment_extraction` (with reranking as its graph-encoded prerequisite, honored) |
| S5 quality control | `crag` (evaluator + corrective action; honest gap instead of web fallback), `reliable_rag` (relevance gate + grounding), `self_rag` (retrieve-or-not for meta turns; answer self-grading via critic) |
| S6 assembly | `contextual_compression` (overflow-only, extractive), `context_enrichment_window_around_chunk` spirit (RSE neighbor stitching) |
| S7 synthesis | `reliable_rag` verbatim support extraction (evidence appendix), `explainable_retrieval` (each evidence block carries why-relevant notes from the grader) |
| S8 verification | `define_evaluation_metrics` / `evaluation_grouse` dimensions run inline (faithfulness, negative rejection); `open-rag-eval-example` citation-checking spirit (local integrity check) |
| Offline harness (build-time, recommended) | `end-2-end_rag_evaluation` + `evaluation_deep_eval` as the regression suite; `choose_chunk_size` + `semantic_chunking` + `contextual_chunk_headers` + `hype_hypothetical_prompt_embeddings` at pack-build time; `retrieval_with_feedback_loop` as a future thumbs-up/down re-ranking signal |

Pack-build recommendations that fell out of the survey (index-time mirrors are free at query time): contextual chunk headers on every chunk, card-level summary embeddings for the hierarchical tier, pre-embedded failure-mode phrasings, optional HyPE-style hypothetical questions per section.

---

## Risks and open questions

1. **Cohere single-vendor dependence** — rerank-v4.0-pro / Command A deprecations or trial-key rate limits would stall Deep tier (user accepts trial-key limits, but 12–20 calls/turn may hit per-minute caps; implement queue+retry, not degradation).
2. **LLM-judge validity** — the S5 grader and S8 critic are unvalidated judges (the corpus's own fm-judge-unvalidated); mitigate with a small human-labeled calibration set and the GroUSE unit-test method before trusting confidence scores.
3. **Ontology staleness/coverage** — expansion and honesty gates inherit ontology quality; a symptom outside the 24 failure modes degrades to plain multi-query (acceptable), but the gap should be logged for pack curation.
4. **Latency of Deep tier** — 12–20 sequential-ish LLM calls ≈ tens of seconds; per-section synthesis must run concurrently and the UI must stream per-section.
5. **Single-source claims** — Command A+ (May 2026) and some 2026 benchmark numbers (RAGRouter-Bench figures, rerank latency percentiles) come from single or secondary sources; re-verify before citing them in-app.
6. **License** — dossiers embed modified NirDiamant/RAG_Techniques content: attribution footer and non-commercial constraint are load-bearing product requirements, including in exported dossiers.
7. **Scale shift at 50k+ chunks** — brute-force dense search and dartboard-over-pool remain fine, but revisit RRF pool sizes and consider ANN (sqlite-vec already supports it) and a local pruner (Provence-style) when packs multiply.

---

## Sources

Corpus (local): `research/corpus/catalog.md`, `research/corpus/ontology.json`.

Routing & adaptive RAG: [Adaptive-RAG / RAGRouter-Bench baseline study (arXiv 2604.03455)](https://arxiv.org/abs/2604.03455) · [RAGRouter-Bench (arXiv 2602.00296)](https://arxiv.org/html/2602.00296) · [LLM routing survey (arXiv 2502.00409)](https://arxiv.org/pdf/2502.00409) · [Tier-based hybrid routing (arXiv 2604.14222)](https://arxiv.org/html/2604.14222v1)
Query transformation: [DMQR-RAG (arXiv 2411.13154)](https://arxiv.org/html/2411.13154v1) · [Query decomposition, exploration–exploitation (arXiv 2510.18633)](https://arxiv.org/pdf/2510.18633) · [MDP multi-query rewriting (ACM CNML 2025)](https://dl.acm.org/doi/10.1145/3728199.3728221) · [Practitioner comparison of query transformations](https://alexchernysh.com/blog/query-transformation-for-rag)
Retrieval & fusion: [Hybrid search BM25+dense interactive analysis](https://mbrenndoerfer.com/writing/hybrid-search-bm25-dense-retrieval-fusion) · [Hybrid search reference 2026](https://www.digitalapplied.com/blog/hybrid-search-bm25-vector-reranking-reference-2026) · [Weaviate hybrid search explained](https://weaviate.io/blog/hybrid-search-explained) · [KG-guided RAG (NAACL 2025)](https://aclanthology.org/2025.naacl-long.449.pdf) · [Ontology-guided evidence path inference (arXiv 2606.28076)](https://arxiv.org/pdf/2606.28076) · [TrustGraph ontology RAG](https://trustgraph.ai/guides/key-concepts/ontology-rag/) · [Awesome-GraphRAG survey list](https://github.com/DEEP-PolyU/Awesome-GraphRAG)
Ranking & selection: [Cohere rerank docs (rerank-v4.0-pro)](https://docs.cohere.com/docs/rerank) · [Cohere reranking best practices](https://docs.cohere.com/docs/reranking-best-practices) · [Rerank evaluation 2026](https://futureagi.com/blog/evaluating-cohere-rerank-rag-2026/) · [Adaptive-k (arXiv 2505.07233)](https://arxiv.org/pdf/2505.07233) · [Dynamic Passage Selector (arXiv 2508.09497)](https://arxiv.org/html/2508.09497) · [Dynamic context selection (arXiv 2512.14313)](https://arxiv.org/html/2512.14313v1)
Quality control: [CRAG (arXiv 2401.15884)](https://arxiv.org/abs/2401.15884) · [Sufficient Context (ICLR 2025, arXiv 2411.06037)](https://arxiv.org/abs/2411.06037) · [Google Research blog: sufficient context](https://research.google/blog/deeper-insights-into-retrieval-augmented-generation-the-role-of-sufficient-context/) · [LangGraph CRAG tutorial](https://www.datacamp.com/tutorial/corrective-rag-crag)
Orchestration: [Anthropic multi-agent research architecture (analysis)](https://theaiengineer.substack.com/p/how-anthropic-built-multi-agent-deep) · [LangChain Open Deep Research](https://www.langchain.com/blog/open-deep-research) · [Agentic RAG SoK (arXiv 2603.07379)](https://arxiv.org/html/2603.07379v1) · [Agentic RAG component ablation (arXiv 2606.21553)](https://arxiv.org/pdf/2606.21553) · [Stop-RAG (arXiv 2510.14337)](https://arxiv.org/html/2510.14337v1) · [TASR training-free adaptive stopping (arXiv 2606.13814)](https://arxiv.org/pdf/2606.13814) · [One-shot vs iterative retrieval (arXiv 2509.04820)](https://arxiv.org/pdf/2509.04820)
Context assembly: [Lost in the Middle (Liu et al.)](https://arxiv.org/abs/2307.03172) · [Principled context engineering for RAG (arXiv 2511.17908)](https://www.arxiv.org/pdf/2511.17908) · [Long-context LLMs meet RAG (ICLR 2025)](https://proceedings.iclr.cc/paper_files/paper/2025/file/5df5b1f121c915d8bdd00db6aac20827-Paper-Conference.pdf) · [Provence (ICLR 2025, arXiv 2501.16214)](https://arxiv.org/abs/2501.16214) · [RAG context placement & citations](https://mbrenndoerfer.com/writing/rag-prompt-engineering-context-citations)
Synthesis: [WebWeaver dynamic outlines (arXiv 2509.13312)](https://arxiv.org/pdf/2509.13312) · [RaPID plan-then-write (arXiv 2503.00751)](https://arxiv.org/html/2503.00751v1) · [ScaffoldAgent (arXiv 2606.20122)](https://arxiv.org/pdf/2606.20122) · [Deep-Reporter (arXiv 2604.10741)](https://arxiv.org/html/2604.10741) · [Cohere RAG + citations docs](https://docs.cohere.com/docs/retrieval-augmented-generation-rag) · [Cohere RAG citations (accurate mode)](https://docs.cohere.com/docs/rag-citations)
Verification: [RAGAS faithfulness guide 2026](https://qaskills.sh/blog/ragas-faithfulness-answer-relevancy-guide) · [CiteCheck (arXiv 2502.10881)](https://arxiv.org/pdf/2502.10881) · [eTracer claim-level grounding (arXiv 2601.03669)](https://arxiv.org/pdf/2601.03669) · [FACTUM citation hallucination in long-form RAG (arXiv 2601.05866)](https://arxiv.org/pdf/2601.05866) · [GroUSE (COLING 2025)](https://aclanthology.org/2025.coling-main.304.pdf)
Multi-turn: [ChatQA (arXiv 2401.10225)](https://arxiv.org/pdf/2401.10225) · [SemEval-2026 Task 8 hybrid retrieval + rewriting (arXiv 2606.28352)](https://arxiv.org/pdf/2606.28352) · [H-RAG hierarchical multi-turn (arXiv 2605.00631)](https://arxiv.org/pdf/2605.00631) · [CORAL benchmark (arXiv 2410.23090)](https://arxiv.org/pdf/2410.23090) · [Amazon Science: multi-turn RAG for support](https://assets.amazon.science/30/1b/6aca1b504a588cc204adbe49d34f/building-multi-turn-rag-for-customer-support-with-llm-labeling.pdf)
Clarification: [AWS: improving RAG via user interaction](https://aws.amazon.com/blogs/machine-learning/improve-llm-responses-in-rag-use-cases-by-interacting-with-the-user/) · [TDS: clarify once, learn the default](https://towardsdatascience.com/when-rag-users-ask-vague-questions-clarify-once-learn-the-default/) · [Underspecified question rewriting (arXiv 2602.11938)](https://arxiv.org/pdf/2602.11938)
Production systems: [Perplexity: architecting an AI-first search API](https://research.perplexity.ai/articles/architecting-and-evaluating-an-ai-first-search-api) · [Perplexity: search as code generation](https://research.perplexity.ai/articles/rethinking-search-as-code-generation) · [LangSmith evaluation](https://www.langchain.com/langsmith/evaluation) · [Cohere Command A review 2026 (single source for Command A+)](https://aiagentsquare.com/agents/cohere)
