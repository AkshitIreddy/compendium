# Cohere Agentic Loop — API Research Report (July 2026)

**Scope:** Upgrading Compendium from single-call generation to an agentic loop (5–30 Cohere calls per deep query), driven from Rust via REST (`reqwest`). Verified against docs.cohere.com as of 2026-07-13. All endpoint shapes below are the **v2 API** (`https://api.cohere.com/v2/...`).

---

## Executive summary

- **The model landscape changed in May 2026.** Cohere shipped **Command A+** (`command-a-plus-05-2026`) on 2026-05-20 — a 218B sparse-MoE (25B active) Apache-2.0 model that consolidates Command A, Command A Reasoning, Command A Vision, and Command A Translate into one set of weights, with controllable `thinking`, native citation generation, and large agentic-benchmark gains over Command A Reasoning. Catch: 128k context (vs 256k for Command A / A Reasoning), and on the hosted API its production rate limits and pricing are "contact sales"; on trial keys it works at the standard trial limits.
- **Native documents-mode citations are exactly what we want.** v2 Chat returns character-span citations (`start`, `end`, `text`, `sources[]` with document ids) for both the `documents` parameter and for tool results typed as documents — including in multi-step tool loops, streaming and non-streaming. Use `citation_options: {"mode": "accurate"}`. This should power the in-app source highlighting; do not prompt-engineer citations.
- **Hard constraint discovered:** `response_format` with `json_schema` is **not supported in combination with `tools` or `documents`**. You cannot force a schema-validated JSON final answer inside the tool/RAG loop. The dossier finalizer must be either (a) a separate no-tools, no-documents call with `json_schema` (losing native citations), or (b) — recommended — a cited documents-mode synthesis call whose citations you consume structurally from the API response instead of from the model's text.
- **Trial-key reality:** 20 chat req/min and **1,000 API calls/month**. A deep query costing ~15–30 calls means roughly **30–60 deep queries per month** on a trial key, and each deep query takes ≥1 minute of wall-clock pacing if it exceeds 20 chat calls. Fine for evaluation; a production key (500 req/min for Command A/R7B, no monthly cap) removes all friction.
- **Recommended assignment:** planner + synthesizer = `command-a-03-2025` (256k context, proven, $2.50/$10 per 1M, 500 req/min prod) with `command-a-reasoning-08-2025` (thinking budget) as an optional "deep mode"; searcher/grader/rewriter = `command-r7b-12-2024` ($0.0375/$0.15 per 1M — ~65x cheaper than Command A). Re-evaluate Command A+ for the synthesizer once hosted pricing/production limits are published.

---

## 1. v2 Chat tool use — multi-step, parallel, tool_choice, streaming, structured output

Source: docs.cohere.com/v2/docs/tool-use-overview, /v2/docs/tool-use-usage-patterns, /v2/reference/chat.

### Request/response shapes

Tool definitions use JSON Schema, OpenAI-style:

```json
{
  "type": "function",
  "function": {
    "name": "search_chunks",
    "description": "Semantic search over the knowledge pack",
    "parameters": {
      "type": "object",
      "properties": { "query": { "type": "string", "description": "..." } },
      "required": ["query"]
    }
  }
}
```

The model's tool-calling turn returns an assistant message with:
- `tool_plan` — a free-text reflection on what it will do next (log this; it is excellent tracing material for the dossier's "reasoning trail"),
- `tool_calls[]` — each `{ "id": "...", "type": "function", "function": { "name": "...", "arguments": "<JSON string>" } }`.

You execute the tools and append **one `assistant` message containing all the tool calls, then one `tool` message per tool call**:

```json
{
  "role": "tool",
  "tool_call_id": "tc_0",
  "content": [
    { "type": "document",
      "document": { "id": "chunk_412", "data": "{\"title\":\"...\",\"snippet\":\"...\"}" } }
  ]
}
```

The `document.id` is optional but **you should always set it** — it is what citations reference. If omitted, ids are auto-generated as `<tool_call_id>:<auto_generated_id>`.

### Multi-step loop
The documented pattern is exactly the loop we plan: call chat → if the response contains `tool_calls`, execute them, append results, call chat again → repeat until the model responds with plain content. **No documented maximum step count** — termination is model-decided (enforce your own cap, e.g. 12 steps). State is just the growing `messages` array; multi-turn chat is the same array carried across turns.

### Parallel tool calls
Supported natively: the model "can call multiple tools in parallel" in one response — same tool multiple times (e.g., 3 reformulated searches) or different tools. All calls arrive in one `tool_calls[]` array; you respond with one `tool` message per call. This is a big win for fan-out retrieval: one chat call can request N searches.

### tool_choice
- `"REQUIRED"` — model must make tool calls.
- `"NONE"` — model must answer directly, no tools.
- Omitted — model decides.
- Requires **Command R7B or newer** models. There is no per-tool forcing (no "call exactly tool X") — if you need that, present only that tool.

### Streaming tool events
SSE event types (docs.cohere.com/v2/docs/streaming): `message-start`, `content-start`, `content-delta` (`delta.message.content.text`), `content-end`, `citation-start` / `citation-end`, `tool-plan-delta` (tool plan tokens), `tool-call-start` (carries `id`, `type`, `function.name`), `tool-call-delta` (argument tokens), `tool-call-end`, `message-end`. So you can stream the agent's plan and tool arguments live into the UI.

### Structured output + tools: the constraint
- `strict_tools: true` — guarantees generated tool calls match the tool schemas exactly (no hallucinated names/params, correct types, all required params present). Limits: every tool needs ≥1 required parameter; max 200 fields across all tools per request; v2 only. **Turn this on.**
- `response_format: { "type": "json_object", "json_schema": {...} }` — schema-constrained final output. **Explicitly not supported in combination with the `documents` or `tools` parameters** (docs.cohere.com/v2/docs/structured-outputs). JSON mode without schema also documented as "not supported in RAG mode."
- Schema support: string/int/float/bool, arrays, nested objects (unlimited nesting with schema; 5 levels without), enum/const/pattern/format, `$ref`/`$defs`, `anyOf`. Not supported: `allOf`, `oneOf`, `not`, min/max numeric and length constraints, regex anchors.

**Answer to "can the model be forced to a final json_schema response after a tool loop?":** Not in the same request. The workable patterns are:
1. **Separate finalizer call** — after the loop, issue a fresh chat call with no `tools`/`documents`, the gathered material inlined as plain message text, and `response_format.json_schema`. You get guaranteed shape but **lose native citations**.
2. **Cited synthesis + structural envelope (recommended)** — let the final loop turn be a documents-mode call producing cited prose; build the dossier JSON yourself in Rust from `message.content` + `citations[]` (both are already structured API fields). Use small `json_schema` calls only for intermediate steps that need machine-readable output (grading verdicts, query plans) — those steps don't need citations.

## 2. Native RAG mode — documents parameter and fine-grained citations

Source: docs.cohere.com/v2/docs/retrieval-augmented-generation-rag, /v2/docs/tool-use-citations, docs.cohere.com/docs/rag-citations.

### documents parameter formats
Array items, any of:
- plain string: `"Title: ... Content: ..."`,
- `{ "data": "string content" }`,
- `{ "id": "chunk_412", "data": { "title": "...", "snippet": "..." } }` — fields of the `data` object are rendered to the prompt; `id` is echoed in citations (auto-generated as `doc:0`, `doc:1`, ... when omitted).

### Citation format (non-streaming)
Response carries a top-level `citations[]`:

```json
{
  "start": 160, "end": 173,
  "text": "gradient noise",
  "sources": [
    { "type": "document", "id": "chunk_412",
      "document": { "id": "chunk_412", "title": "...", "snippet": "..." } }
  ]
}
```

`start`/`end` are **character offsets into the generated message text**, `text` is the exact cited span, and each span can have multiple sources. This maps 1:1 onto our in-app highlighting: span → set of chunk ids → open exact notebook section. The same citation objects are generated for **tool results** (content type `document`), so citations survive across a full multi-step agent loop, not just single-shot documents mode.

### Streaming citations
Citations arrive as `citation-start` events (payload = the same citation object: start, end, text, sources) followed by `citation-end`. In `accurate` mode they arrive after the text has finished streaming; in `fast` mode they are injected inline as the model uses each source.

### citation_options
`citation_options: { "mode": "accurate" | "fast" }` (the reference also exposes an off/disabled setting to suppress citations entirely).
- **`accurate` (default):** model finishes the response, then emits citations aligned to the final text — slightly higher latency, more precise span indices. **Use this**: our highlighting depends on index precision, and dossier generation is not latency-critical.
- **`fast`:** citations emitted inline during generation — immediate traceability, "slightly less precision in citation relevance."

### Limits
- No hard documented cap on document count or total size; the effective cap is the model's context window.
- Docs repeatedly recommend **keeping each document/snippet under ~300 words** (RAG guide) / chunking to ±400 words and using Rerank to prioritize when near context limits. Our section-chunk granularity fits this well; pass 10–30 reranked chunks per synthesis call rather than everything retrieved.

**Verdict for Compendium: use native citations.** They are out-of-the-box for the whole Command family, precise to the character, structured (no parsing model prose), work in both documents mode and tool loops, and stream. Prompt-engineered citations cannot match this and would burn output tokens on citation markup.

## 3. Model lineup for our tiers (July 2026)

Source: docs.cohere.com/docs/models, /docs/command-a-plus, /docs/command-a-reasoning, cohere.com/blog/command-a-plus, docs.cohere.com/docs/rate-limits, pricing via cohere.com/pricing + corroborating trackers.

| Model | ID | Context / max out | Price (per 1M in/out) | Prod rate limit | Notes |
|---|---|---|---|---|---|
| **Command A+** | `command-a-plus-05-2026` | 128k / 64k | not published (sales); Apache 2.0 open weights | contact sales (trial: 20 req/min) | MoE 218B/25B. Unifies text+reasoning+vision+translate; controllable `thinking`; native citations; vs Command A Reasoning: agentic benchmarks 37%→85%, +110% throughput, −30% latency. Newest, but hosted production terms opaque. |
| **Command A** | `command-a-03-2025` | 256k / 8k | $2.50 / $10.00 | 500 req/min | Workhorse for tool use, agents, RAG, citations. Fully self-serve. 8k max output is the one squeeze for very long dossiers (mitigate: sectioned generation). |
| **Command A Reasoning** | `command-a-reasoning-08-2025` | 256k / 32k | not published (sales) for production; usable on trial | contact sales | First reasoning model; `thinking` on by default with `token_budget` control; 32k output; excels at agents/RAG. |
| **Command R7B** | `command-r7b-12-2024` | 128k / 4k | $0.0375 / $0.15 | 500 req/min | Small/fast; supports tool_choice, strict tools, structured output, RAG+citations. Ideal grader/rewriter. |
| Command R | `command-r-08-2024` | 128k / 4k | $0.15 / $0.60 | 500 req/min | Legacy-ish middle tier; R7B or A dominate it for our roles. |
| Embed v4 | `embed-v4.0` | 128k ctx | $0.12 / 1M tokens | (see §5) | 256/512/1024/1536 dims — our packs are 1024, supported. Matryoshka, multimodal, int8/binary types. |
| Rerank v4 Pro / Fast | `rerank-v4.0-pro`, `rerank-v4.0-fast` | 32k per doc window | ~$2.50 / ~$2.00 per 1k searches | 1,000 req/min | New v4 generation, 32k context, JSON/semi-structured docs. `rerank-v3.5` (4k) still live at ~$1/1k. |

**Reasoning control** (`command-a-reasoning-08-2025`, Command A+): request field `thinking: { "type": "enabled" | "disabled", "token_budget": <int> }`. On reasoning models thinking is **enabled by default**; response contains `content` items of type `"thinking"` followed by `"text"` (streaming: `delta.message.content.thinking`). Docs recommend leaving ≥1k tokens for the response; ~31k budget for maximum reasoning depth.

**Deprecation outlook:** `command-r-03-2024`, `command-r-plus-04-2024`, `command`, `command-light` were deprecated 2025-09-15. `command-a-03-2025`, `command-r7b-12-2024`, `command-a-reasoning-08-2025` are all listed live with no announced sunset. Command A+ is positioned as the consolidation of the four Command A specialist models, so expect Cohere to steer traffic there over time — but Command A remains the flagship *self-serve* API model today. Risk is low near-term; keep model ids in config, not code.

## 4. Rate limits: trial vs production vs an agentic pattern

Source: docs.cohere.com/docs/rate-limits (fetched twice, consistent).

| Endpoint | Trial | Production |
|---|---|---|
| Chat (all listed models) | **20 req/min** | 500 req/min (Command A, R+, R, R7B, North Mini Code); **contact sales** for Command A+/A Reasoning/A Translate/A Vision |
| Embed (text) | 2,000 inputs/min | 2,000 inputs/min (same) |
| Embed (images) | 5 inputs/min | 400 inputs/min |
| Rerank | **10 req/min** | 1,000 req/min |
| Tokenize | 100 req/min | 2,000 req/min |
| Default (other) | 500 req/min | 500 req/min |
| **Monthly cap** | **1,000 API calls / month** (also applies to production keys for the newer sales-gated chat models) | none for self-serve models |

**What this means for a deep query (10–30 chat + ~3–8 embed + ~2–6 rerank calls):**
- **Trial, per-minute:** 20 chat req/min means a 25-chat-call query must be paced over ≥75 seconds; rerank at 10 req/min is the tighter proportional ceiling if you rerank on every retrieval step. Sequential agent loops mostly self-pace (each chat call takes seconds), so in practice a single deep query runs fine with a simple token-bucket + retry-on-429 layer. Two concurrent deep queries will throttle.
- **Trial, monthly (the real UX):** 1,000 calls/month ÷ ~20–35 calls per deep query (chat+embed+rerank) ≈ **~30–50 deep queries/month**, i.e. 1–2 per day. Quick single-shot questions (~3–5 calls) are cheap. The app should show a per-query call meter and a monthly budget indicator when a trial key is detected.
- **Production key (self-serve, Command A + R7B):** 500 chat req/min, no monthly cap — a 30-call deep query is negligible; even 10 concurrent deep queries fit. Cost per deep query at recommended assignment: roughly 5 Command A calls (~60k in / 6k out ≈ $0.21) + 15 R7B calls (~150k in / 8k out ≈ $0.007) + embed/rerank (~$0.02) ≈ **$0.20–0.45 per deep query**.

## 5. Embed and rerank at agentic volume

Source: docs.cohere.com/v2/reference/embed, /v2/reference/rerank.

- **Embed batching:** confirmed — `texts` accepts **max 96 items per call** (the mixed `inputs` array likewise up to 96). Embedding 5–10 reformulated queries per turn is **one** embed call. `input_type: "search_query"` for queries vs `"search_document"` at pack-build time (must match what the packs used). `output_dimension: 1024` matches our prebuilt vectors. Trial and production both allow 2,000 inputs/min — never a bottleneck for query-side embedding; the monthly 1,000-call cap on trial is the only pressure, and batching means ~1 embed call per loop iteration.
- **Rerank:** recommended ≤1,000 documents per request; `max_tokens_per_doc` defaults to 4096 (docs auto-truncate); `top_n` to trim results; scores normalized to [0,1] (useful directly as a relevance-grade signal — a cheap pre-filter before LLM grading). Billing: 1 search unit = 1 query + up to 100 docs; docs >500 tokens count as multiple chunks. Trial 10 req/min: rerank once per retrieval step (candidates from all parallel queries merged into one call ≤1,000 docs), not once per query reformulation.

## 6. Cohere's documented agent-loop guidance

Sources: docs.cohere.com/docs/agentic-rag (6-part tutorial), docs.cohere.com/page/agentic-multi-stage-rag, /page/agentic-rag-mixed-data, /v2/docs/tool-use-usage-patterns.

Cohere's own recommended patterns map cleanly onto Compendium:
1. **Query routing via tools** — expose each knowledge pack / data source as a distinct tool; the model routes.
2. **Parallel query generation** — the model emits several search tool calls in one turn (their "generating parallel queries" pattern); execute concurrently, return one tool message each.
3. **Sequential multi-step** — `while tool_calls: execute; append; re-chat` with no fixed cap; model stops when satisfied. Their multi-stage RAG cookbook adds a `reference_extractor()`-style tool so the agent follows references/links in retrieved docs to fetch more — directly applicable to our "linked notebook sections."
4. **Self-correction** — the agent inspects retrieved evidence and re-queries if insufficient; combine with an explicit `grade_evidence` step on R7B.
5. **Citations in the loop** — return retrieval results as `document`-typed tool content with real chunk ids; the final response then carries span citations automatically (§2).

## Recommendation for Compendium

### Model assignment per role

| Role | Model | Config |
|---|---|---|
| **Planner / agent controller** (decides searches, follows references, decides when done) | `command-a-03-2025` | tools + `strict_tools: true`, `tool_choice` as needed, temp ~0.3. Optional "deep mode": swap to `command-a-reasoning-08-2025` with `thinking: {token_budget: 4000-8000}` for gnarly problems (trial-key friendly; production needs sales). |
| **Query rewriting / expansion** | `command-r7b-12-2024` | `response_format.json_schema` → `{queries: [...]}`. Cheap enough to always fan out 4–6 reformulations. |
| **Relevance grading / evidence sufficiency** | `command-r7b-12-2024` | json_schema verdicts; pre-filter with rerank scores to cut LLM grading volume. |
| **Synthesizer (final dossier)** | `command-a-03-2025` | documents mode with top reranked chunks (each ≤~400 words), `citation_options: {"mode": "accurate"}`. If dossier > 8k tokens, generate per-section. Re-evaluate `command-a-plus-05-2026` (64k output solves sectioning) once hosted pricing/limits are self-serve. |
| **Embedding / rerank** | `embed-v4.0` @ 1024 dims (batch ≤96) / `rerank-v4.0-pro` (fallback `rerank-v3.5`) | one rerank call per loop iteration over merged candidates. |

### Citations: native, unequivocally
Use documents-mode / tool-document citations end to end. Persist `citations[]` (start/end/text/source ids) alongside the dossier markdown; render highlights from character offsets; resolve `sources[].id` → chunk → notebook section anchor. Include the citation map in the exported dossier so downstream AIs receive grounded references. Do **not** ask the model to emit citation markup in prose.

### Structured dossier
Because `json_schema` cannot coexist with `tools`/`documents`: keep intermediate machine-readable steps (rewrites, grades, plan) as small no-tool `json_schema` calls on R7B; produce the final dossier as cited prose (documents mode) and assemble the JSON envelope in Rust from the structured response fields. Avoid a "re-serialize the dossier through a json_schema call" step — it would strip citations and risk drift from the cited text.

### Running the loop from Rust (reqwest, REST)
- `POST https://api.cohere.com/v2/chat`, headers `Authorization: Bearer <key>`, `Content-Type: application/json`. Body: `model`, `messages`, `tools`, `strict_tools`, `documents` (synthesis call), `citation_options`, `temperature`, optionally `stream: true` (SSE).
- Loop: send → if `message.tool_calls` non-empty, push the assistant message verbatim (keep `tool_plan`), execute calls (concurrently for parallel calls), push one `tool` message per call with `content: [{type:"document", document:{id, data}}]` → repeat. Hard cap ~12 iterations; on cap, force `tool_choice: "NONE"` to make it answer with what it has.
- Streaming: parse SSE events; surface `tool-plan-delta` and `tool-call-start` in the UI as live agent status; buffer `content-delta`; apply `citation-start` payloads after `message-end` (accurate mode delivers them post-text).
- Resilience: on HTTP 429 honor `Retry-After` / exponential backoff; token-bucket at 20 chat req/min + 10 rerank req/min when a trial key is configured; count calls per query and per month for the trial-budget UI.

### Risks / watch items
- Command A+ and Command A Reasoning hosted production access is sales-gated (and the 1,000-calls/month cap applies even to production keys for those models) — don't hard-depend on them; keep model choice per-role configurable.
- Command A's 8k max output constrains single-shot long dossiers — design sectioned synthesis from day one.
- The `json_schema` × `tools`/`documents` incompatibility is easy to reintroduce accidentally; encode it as a type-level constraint in the Rust client (request builder forbids the combination).
- Trial monthly math (~30–50 deep queries) should be surfaced in-app to avoid silent mid-month lockout.
- Docs pages evolve; re-verify `citation_options` enum spelling and rate-limit tables against docs.cohere.com/reference/chat and /docs/rate-limits before freezing the client.

---

## Sources

- https://docs.cohere.com/docs/models — model lineup, ids, context windows, deprecations
- https://docs.cohere.com/docs/rate-limits — trial/production limits, 1,000 calls/month trial cap
- https://docs.cohere.com/v2/docs/tool-use-overview — tool use flow, message shapes
- https://docs.cohere.com/v2/docs/tool-use-usage-patterns — multi-step loop, parallel calls, tool_choice
- https://docs.cohere.com/v2/docs/tool-use-citations — citations from tool results, fast/accurate modes
- https://docs.cohere.com/docs/rag-citations — citation modes detail
- https://docs.cohere.com/v2/docs/retrieval-augmented-generation-rag — documents parameter, citation format
- https://docs.cohere.com/v2/docs/structured-outputs — json_schema features, strict_tools, incompatibility with tools/documents
- https://docs.cohere.com/v2/reference/chat — citation_options, response_format, thinking, tool_choice reference
- https://docs.cohere.com/v2/docs/streaming — SSE event catalog
- https://docs.cohere.com/docs/reasoning — thinking parameter, token_budget, thinking content blocks
- https://docs.cohere.com/docs/command-a-plus, https://cohere.com/blog/command-a-plus — Command A+ release
- https://docs.cohere.com/docs/command-a-reasoning — Command A Reasoning specs
- https://docs.cohere.com/v2/reference/embed — 96-text batch limit, output_dimension
- https://docs.cohere.com/v2/reference/rerank — max_tokens_per_doc, 1,000-doc guidance
- https://docs.cohere.com/docs/agentic-rag, https://docs.cohere.com/page/agentic-multi-stage-rag, https://docs.cohere.com/page/agentic-rag-mixed-data — agent patterns/cookbooks
- https://cohere.com/pricing, https://www.eesel.ai/blog/cohere-ai-pricing, https://www.aipricing.guru/cohere-pricing/ — pricing corroboration
- https://venturebeat.com/technology/cohere-cracks-lossless-quantization-and-native-citations-with-first-full-apache-2-0-licensed-open-model-command-a, https://www.marktechpost.com/2026/05/21/cohere-releases-command-a-a-218b-sparse-moe-model-for-agentic-workflows-that-runs-on-as-few-as-two-h100-gpus/ — Command A+ benchmarks/latency
