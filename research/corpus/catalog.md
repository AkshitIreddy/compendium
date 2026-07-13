# RAG Techniques Catalog

Derived from the NirDiamant/RAG_Techniques repository (44 analyzed notebooks). Content modified from the original; non-commercial use with attribution required — see `_repo-overview.json` for license details.

Techniques are grouped by their stage in the RAG lifecycle: **chunking → indexing → query transformation → retrieval → post-retrieval → orchestration → evaluation**. Failure-mode ids reference `ontology.json`.

---

## 1. Chunking

How documents are split into retrieval units before anything is embedded.

| Slug | Title | One-liner | Complexity | Top failure modes addressed |
|---|---|---|---|---|
| `choose_chunk_size` | Optimizing Chunk Sizes | Benchmark faithfulness, relevancy, and latency across candidate fixed chunk sizes on auto-generated eval questions. | low | chunk size/boundaries wrong; fragmented context; cost/latency budget |
| `semantic_chunking` | Semantic Chunking | Split at embedding-detected topic boundaries instead of fixed character counts, so each chunk is semantically coherent. | low | chunk boundaries wrong; fragmented context; context noise |
| `proposition_chunking` | Proposition Chunking | LLM decomposes documents into atomic, self-contained factual propositions, quality-grades them, and indexes the survivors. | medium | chunk boundaries wrong; context noise; lost document identity (pronouns) |
| `contextual_chunk_headers` | Contextual Chunk Headers (CCH) | Prepend an LLM-generated document title/section header to every chunk before embedding so chunks keep document-level context. | low | lost document identity; vocabulary mismatch; hallucination from misread chunks |

## 2. Indexing & Index Enrichment

How the searchable index is built and enriched beyond plain chunk embeddings.

| Slug | Title | One-liner | Complexity | Top failure modes addressed |
|---|---|---|---|---|
| `hype_hypothetical_prompt_embeddings` | HyPE (Hypothetical Prompt Embeddings) | Generate hypothetical questions per chunk at index time and embed those, turning retrieval into question-question matching with zero query-time cost. | medium | vocabulary mismatch; right chunk ranked low; per-query cost |
| `document_augmentation` | Document Augmentation via Question Generation | Enrich the index with LLM-generated synthetic questions per fragment so question-shaped queries match question-shaped vectors. | medium | vocabulary mismatch; right chunk ranked low |
| `hierarchical_indices` | Hierarchical Indices | Two linked vector stores — page summaries plus detailed chunks — retrieved coarse-to-fine with metadata drill-down. | medium | lost document identity; context noise; fragmented context |
| `raptor` | RAPTOR | Recursively cluster and summarize chunks into a multi-level summary tree so queries match at the right level of abstraction. | high | corpus-level questions; multi-hop; fragmented context |
| `microsoft_graphrag` | Microsoft GraphRAG | LLM-extracted knowledge graph with Leiden community summaries enabling local (entity) and global (whole-corpus) search. | high | corpus-level questions; multi-hop |
| `multi_model_rag_with_captioning` | Multi-modal RAG with Captioning | Caption PDF images with a vision LLM and index captions alongside page text in one vector store. | medium | answers live in figures/tables/charts |
| `simple_csv_rag` | Simple RAG over CSV | Each CSV row becomes a "column: value" document embedded in FAISS; natural-language Q&A over tabular records. | low | structured/tabular data; cold start |
| `simple_csv_rag_with_llamaindex` | Simple CSV RAG (LlamaIndex) | Same row-per-document CSV RAG via LlamaIndex's PagedCSVReader. | low | structured/tabular data; cold start |
| `json_rag` | Simple RAG with JSON | Field-selective jsonpath flattening of JSON records into embeddable strings with a mapping back to the full record. | low | structured/tabular data; cold start |

## 3. Query Transformation

Reshaping the user's query before search.

| Slug | Title | One-liner | Complexity | Top failure modes addressed |
|---|---|---|---|---|
| `query_transformations` | Query Transformations | LLM rewriting (more specific), step-back prompting (more general), and sub-query decomposition before retrieval. | low | vague queries; vocabulary mismatch; multi-part questions |
| `hyde_hypothetical_document_embedding` | HyDE | LLM writes a fake answer-document for the question; that document (not the query) is embedded and searched. | low | vocabulary mismatch; right chunk ranked low; multi-part questions |
| `memorag` | MemoRAG | Build an LLM "memory" (topic→details KV pairs) over the corpus at index time; mine it for clue spans and surrogate queries at query time. | medium | vague queries; corpus-level questions; vocabulary mismatch |

## 4. Retrieval

The search execution itself.

| Slug | Title | One-liner | Complexity | Top failure modes addressed |
|---|---|---|---|---|
| `simple_rag` | Simple RAG (baseline) | The canonical baseline: chunk a PDF with overlap, embed with OpenAI, index in FAISS, retrieve top-k. | low | cold start; hallucination on private docs; no source grounding |
| `simple_rag_with_llamaindex` | Simple RAG (LlamaIndex) | Same baseline in LlamaIndex with sentence-aware token chunking and a deepeval harness. | low | cold start; hallucination; no source grounding |
| `fusion_retrieval` | Fusion Retrieval (Hybrid Dense + BM25) | Run vector and BM25 search over the same chunks, normalize scores, merge with a tunable weighted sum. | low | exact terms/IDs missed; vocabulary mismatch; mixed query workload |
| `fusion_retrieval_with_llamaindex` | Fusion Retrieval (LlamaIndex) | Hybrid BM25 + vector via QueryFusionRetriever; num_queries>1 turns it into RAG-Fusion. | medium | exact terms/IDs missed; vocabulary mismatch; right chunk ranked low |
| `dartboard` | Dartboard Retrieval | Greedy selection over an oversampled pool jointly optimizing relevance and diversity, so top-k stops repeating itself. | medium | redundant/one-sided results; multi-part questions; context noise |
| `adaptive_retrieval` | Adaptive Retrieval | LLM classifies each query (Factual/Analytical/Opinion/Contextual) and routes it to a specialized retrieval strategy. | medium | mixed query workload; multi-part questions; redundancy |
| `graph_rag` | Graph RAG (LangChain chunk-graph) | NetworkX graph over chunks (similarity + shared concepts) traversed Dijkstra-style with per-step completeness checks. | high | multi-hop; fragmented context; retrieval opacity |
| `graph_rag_local_attribution` | Local Graph RAG with Verifiable Attribution | Fully-local (Ollama) graph RAG citing every claim back to its source sentence and graph path. | high | multi-hop; no attribution; data cannot leave infrastructure |
| `graphrag_with_milvus_vectordb` | Graph RAG with Milvus | Entities and relationship triplets in separate Milvus collections; subgraph expansion via adjacency-matrix multiplication plus LLM reranking. | medium | multi-hop; vocabulary mismatch |
| `multi_model_rag_with_colpali` | Multi-modal RAG with ColPali | Skip PDF parsing: index every page as an image with a vision late-interaction retriever; a VLM reads the retrieved page. | medium | answers live in figures/tables; brittle parsing/chunking |

## 5. Post-Retrieval Processing

What happens to candidates between retriever and generator.

| Slug | Title | One-liner | Complexity | Top failure modes addressed |
|---|---|---|---|---|
| `reranking` | Reranking (LLM / Cross-Encoder) | Re-score an oversized candidate set with a stronger relevance model; keep only the top few. | low | right chunk ranked low; context noise; redundancy |
| `reranking_with_llamaindex` | Reranking (LlamaIndex) | Same technique via LLM or cross-encoder node postprocessors. | low | right chunk ranked low; context noise |
| `contextual_compression` | Contextual Compression | LLM compressor extracts only the query-relevant sentences from each retrieved chunk before generation. | low | context noise; token/context budget |
| `context_enrichment_window_around_chunk` | Context Enrichment Window | Fetch N neighboring chunks around each hit and stitch them into one deduplicated passage. | low | fragmented context; chunk-size tradeoff |
| `context_enrichment_window_around_chunk_with_llamaindex` | Sentence-Window Retrieval (LlamaIndex) | Retrieve at sentence granularity, then swap each hit for its stored window of surrounding sentences. | low | fragmented context; chunk-size tradeoff |
| `relevant_segment_extraction` | Relevant Segment Extraction (RSE) | Reconstruct contiguous multi-chunk segments around clusters of relevant chunks so the LLM sees complete in-order sections. | medium | fragmented context; chunk-size tradeoff; context budget |
| `reliable_rag` | Reliable RAG | Three LLM gates: grade each doc's relevance, verify the answer is grounded, extract the verbatim supporting segments. | low | context noise; hallucination; no attribution |
| `explainable_retrieval` | Explainable Retrieval | Attach an LLM-generated natural-language explanation to every retrieved chunk stating why it is relevant. | low | retrieval opacity; no attribution |
| `retrieval_with_feedback_loop` | Retrieval with Feedback Loop | Collect user ratings, use them to re-rank future retrievals, and fold highly-rated Q&A pairs back into the index. | medium | system never learns from feedback; context noise |

## 6. Orchestration & Architecture

Pipeline-level control flow and end-to-end architectures.

| Slug | Title | One-liner | Complexity | Top failure modes addressed |
|---|---|---|---|---|
| `self_rag` | Self-RAG | Control loop deciding whether to retrieve, filtering chunks for relevance, and self-grading candidate answers for support and utility. | medium | hallucination; context noise; unnecessary retrieval; out-of-scope queries |
| `crag` | Corrective RAG (CRAG) | Score retrieved chunks; when local retrieval is weak, rewrite the query and fall back to live web search. | medium | hallucination on retrieval miss; out-of-scope/stale knowledge |
| `agentic_rag` | Agentic RAG with Contextual AI | Managed platform: autonomous query reformulation, vision parser, instruction-following reranker, grounded LM, LMUnit testing. (Sponsor-affiliated.) | medium | vague/multi-turn queries; multi-hop; visual content; conflicting sources |
| `local_rag_huggingface_faiss` | Fully Local RAG (HuggingFace + FAISS) | End-to-end pipeline from free open-source components (BGE embeddings, FAISS, Zephyr-7b); no data leaves the machine. | low | data cannot leave infrastructure; API cost; cold start |

## 7. Evaluation

Measuring RAG quality offline.

| Slug | Title | One-liner | Complexity | Top failure modes addressed |
|---|---|---|---|---|
| `define_evaluation_metrics` | Custom LLM-as-Judge Metrics | Hand-rolled correctness, faithfulness, and retrieval-relevancy judges with transparent prompts and structured output. | low | no eval signal; undetected hallucination |
| `evaluation_deep_eval` | DeepEval Evaluation | GEval correctness, faithfulness, and contextual relevancy over batchable test cases. | low | no eval signal; undetected hallucination |
| `evaluation_grouse` | GroUSE | Six grounded-QA metrics (incl. positive acceptance / negative rejection) plus meta-evaluation of cheaper judges via unit tests. | medium | no eval signal; unvalidated judge; refusal calibration |
| `end-2-end_rag_evaluation` | End-to-End RAG Evaluation | Criteria derived from human-annotated failures, judge prompts benchmarked against those labels, RAGAS faithfulness, per-criterion gates. | medium | no eval signal; unvalidated judge; missed incompleteness |
| `open-rag-eval-example` | Open-RAG-Eval | Reference-free scoring (UMBRELA, AutoNuggetizer, citation/hallucination detection) — no golden answers needed. | low | no eval signal without ground truth; citation accuracy |

---

## Notable relationships and composition patterns

### The canonical upgrade pipeline

Most techniques are designed to stack onto `simple_rag` in lifecycle order. The most commonly implied full pipeline across analyses:

```
semantic_chunking (or choose_chunk_size)
  → contextual_chunk_headers            (index-time context)
  → fusion_retrieval                    (dense + BM25, over-fetch high k)
  → reranking                           (precision cut to top-n)
  → relevant_segment_extraction         (rebuild coherent segments; RSE consumes reranker scores — reranking is its prerequisite)
  → reliable_rag                        (relevance gate + groundedness check + source highlighting)
  → evaluation_deep_eval / end-2-end_rag_evaluation   (offline regression harness)
```

### Reranking is the hub

`reranking` composes with more techniques than anything else (fusion retrieval, query transformations, hierarchical indices, HyPE, HyDE, document augmentation, graph RAG variants, multimodal captioning, feedback loops). Any recommendation that increases recall (hybrid search, query expansion, wider k) should co-recommend a reranker to restore precision.

### Expand vs. shrink — opposite remedies for adjacent symptoms

- **Context starved** (fragments, cut-off answers) → expand: `context_enrichment_window_around_chunk`, `relevant_segment_extraction`.
- **Context polluted** (off-topic text, token bloat) → shrink: `contextual_compression`, `reranking`, `proposition_chunking`.
- They chain: expand-then-compress is explicitly suggested (window enrichment → contextual compression). An advisor must diagnose which symptom the user actually has, since the fixes pull in opposite directions.

### Index-time vs. query-time mirror pairs

The same fix often exists on both sides of the index; the choice hinges on whether re-indexing is possible and on the per-query latency budget:

| Query-time (no re-index, adds latency) | Index-time (re-index, zero query cost) |
|---|---|
| `hyde_hypothetical_document_embedding` (fake answer per query) | `hype_hypothetical_prompt_embeddings` / `document_augmentation` (fake questions per chunk) |
| `query_transformations` (rewrite the query) | `contextual_chunk_headers` (rewrite the chunk) |
| `context_enrichment_window_around_chunk` (expand after retrieval) | `semantic_chunking` / `choose_chunk_size` (get boundaries right up front) |

### The self-correction ladder

Increasing sophistication for "retrieval returns junk and the model answers anyway": `reranking` (reorder) → `reliable_rag` (detect and filter) → `crag` (detect and correct via web-search fallback) → `self_rag` (decide whether to retrieve at all, self-grade answers) → `agentic_rag` (fully autonomous managed pipeline). These are mostly alternatives to each other, not stackable.

### Four graph RAG variants, differentiated by ops constraints

`graph_rag` (hand-rolled, LangChain), `microsoft_graphrag` (heavyweight, community summaries, best for global questions), `graphrag_with_milvus_vectordb` (no graph DB needed — reuses the vector store), `graph_rag_local_attribution` (fully local + sentence-level citations). All target multi-hop failure; recommend by deployment constraint (managed vs. vector-DB-only vs. air-gapped) rather than capability.

### Evaluation composes with everything

The five evaluation notebooks are alternatives to each other and universal companions to every other technique (A/B harness for any change). `evaluation_grouse` and `end-2-end_rag_evaluation` uniquely address the meta-problem of validating the LLM judge itself.

### Framework twins

Five techniques ship LangChain and LlamaIndex implementations (`simple_rag`, `simple_csv_rag`, `fusion_retrieval`, `reranking`, `context_enrichment_window_around_chunk`). A recommendation engine should present these as one technique with an implementation choice, not two recommendations.

---

## Caveats for the recommendation engine

- **Sponsor-affiliated content**: `agentic_rag` (Contextual AI managed platform) and `graphrag_with_milvus_vectordb` (Zilliz/Milvus) are vendor showcases; recommendations should disclose platform lock-in.
- **README ghosts**: "Multi-faceted Filtering" and the "Sophisticated Controllable Agent" appear in the repo README but have no notebooks; they are excluded here, and seven analyses' references to `multi_faceted_filtering` were dropped as dangling.
- **License**: custom non-commercial license; the knowledge-base app must stay free, attribute Nir Diamant with a repo link, and mark content as modified.
