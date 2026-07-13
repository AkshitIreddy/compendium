# Cohere Platform Research — Runtime Key Requirements for a Shipped RAG Desktop App

Researched: 2026-07-13. Verified against docs.cohere.com, cohere.com, api.cohere.com (live CORS probe), and GitHub. Architecture under evaluation: prebuilt embedded index shipped in the installer (embeddings computed at build time with the developer's production key); at runtime the END USER's own Cohere key performs 1 query embed + 1 rerank (~30–50 short docs) + 1 chat call per query.

---

## TL;DR verdict

**A free trial key IS sufficient at runtime for typical single-user desktop usage — with a hard ceiling.** The binding constraint is the trial key's overall cap of **1,000 API calls per month**. At 3 calls per query (embed + rerank + chat), that is **~333 queries/month (~11/day)** — comfortable for a personal recommendation tool, fatal for heavy use. Per-minute limits are not the problem (the tightest is rerank at 10 req/min, i.e. ~10 queries/min). The app should accept a trial key by default, handle HTTP 429 gracefully with a "monthly trial quota reached — upgrade to a production key (pay-as-you-go, ~$0.002–0.003/query)" message, and treat the production key as an optional upgrade, not a requirement. If rerank is made optional (skippable), trial headroom rises to 500 queries/month.

Caveat: Cohere's pricing page states trial keys are free but rate-limited and "cannot be used commercially." An end user running the app for their own purposes on their own key is their own usage, but if this app is sold commercially you should surface that language and recommend production keys in your docs rather than promising trial-key operation.

---

## 1. Embedding models

Current lineup (docs.cohere.com/docs/models, docs.cohere.com/docs/embeddings):

| Model | Dimensions | Max input | Notes |
|---|---|---|---|
| `embed-v4.0` | 256 / 512 / 1024 / **1536 (default)** — Matryoshka | 128k tokens | Multimodal (text+image), current flagship |
| `embed-english-v3.0` | 1024 | 512 tokens | English-only, previous generation |
| `embed-english-light-v3.0` | 384 | 512 tokens | Small/fast |
| `embed-multilingual-v3.0` | 1024 | 512 tokens | |
| `embed-multilingual-light-v3.0` | 384 | 512 tokens | |

**Recommendation for the shipped index: `embed-v4.0`.**

- **Matryoshka dimensions**: embed-v4.0 supports output dimensions [256, 512, 1024, 1536] from a single training ("coarse-to-fine representation within a single vector"). For a curated corpus of short technique cards, **512 or 1024 dims** is a good size/quality tradeoff and shrinks the installer payload; 1536 is default.
- **Max input**: 128k tokens per document vs only 512 for the v3.0 family — no chunking gymnastics at build time.
- **input_type consistency (critical)**: index documents with `input_type: "search_document"` at build time; embed runtime queries with `input_type: "search_query"`. Cohere explicitly requires this pairing for semantic search quality. Bake this into both the build pipeline and the runtime client.
- **embedding_types**: `float` (highest quality, largest), `int8`/`uint8` (~4x smaller, minor quality loss), `binary`/`ubinary` (~32x smaller — a 1024-dim vector becomes 128 bytes — noticeable quality loss). For a small curated corpus (hundreds–thousands of cards), size is trivial: ship **float**, or **int8** if you want a smaller installer. Whatever type you index with, request the same type for the runtime query embedding.

**Deprecation risk**: This is the existential risk for a shipped index — query embeddings MUST come from the same model as the index; if the model is shut down, the shipped index is dead weight until you re-embed the corpus and push an app update. Cohere's lifecycle (docs.cohere.com/docs/deprecations): Active → Legacy → Deprecated → Shutdown, with email/docs/blog notice ranging from several months to over a year. The entire **v2.0 embed family was shut down April 4, 2026**; the v3.0 family is now two generations old and the obvious next candidate. **`embed-v4.0` has the best longevity outlook** (newest, flagship, multimodal, and the model Cohere's own docs recommend migrating to). Mitigation: keep the build-time embedding pipeline scripted so a corpus re-embed + index rebuild is a one-command release, and store raw card text (not just vectors) in the app so a future migration needs no source-corpus access.

## 2. Rerank

Current models (docs.cohere.com/docs/rerank, docs.cohere.com/reference/rerank):

- **`rerank-v4.0-pro`** — 32k context, best quality, handles semi-structured JSON docs
- **`rerank-v4.0-fast`** — 32k context, "low latency and high throughput"
- `rerank-v3.5` (4k), `rerank-english-v3.0` (4k), `rerank-multilingual-v3.0` (4k)

API shape:
- No hard max documents, but Cohere "recommend[s] against sending more than 1,000 documents in a single request." Your 30–50 docs is trivially fine in **one call**.
- Per-doc limit: `max_tokens_per_doc` (default **4,096**); longer docs are truncated/chunked automatically. Query tokens count against the combined per-doc budget.
- `top_n` limits returned results (e.g., top_n=10 to feed the chat step).
- Billing "search unit" = 1 query + up to 100 documents; any doc exceeding 500 tokens (including query length) splits into chunks that each count as a document. Short technique cards → **your rerank call = exactly 1 search unit**.

**Recommendation**: `rerank-v4.0-fast` for 30–50 short docs (latency-optimized, 32k context is ample).

## 3. Generation (Command family)

Current live models (docs.cohere.com/docs/models):

| Model | Context | Max output |
|---|---|---|
| `command-a-plus-05-2026` | 128k | 64k (flagship; also released open-weights Apache 2.0; hosted rate limits "contact sales") |
| `command-a-03-2025` | 256k | 8k |
| `command-a-reasoning-08-2025` | 256k | 32k |
| `command-r-plus-08-2024` | 128k | 4k |
| `command-r-08-2024` | 128k | 4k |
| `command-r7b-12-2024` | 128k | 4k |

**Structured output** (docs.cohere.com/docs/structured-outputs): the Chat API supports `response_format: {"type": "json_object"}` and full **`json_schema`** mode with strict adherence (required fields, types, nesting). Constraints: top-level must be a JSON object; every object needs ≥1 `required` field; not supported in RAG mode (pass your retrieved cards as plain message content, not via the RAG/documents parameter, when using json_schema); first request with a new schema has extra latency. The docs page lists Command A+, Command A, Command R+ (08-2024), and Command R (08-2024) as supported; **`command-r7b-12-2024` is not explicitly listed on that page** — verify with a live call before committing to R7B for json_schema.

**Recommendation for a short structured recommendation from ~10 cards**: `command-r7b-12-2024` — smallest/fastest, 128k context (far more than needed for ~10 short cards), positioned by Cohere for "high throughput, latency-sensitive" RAG, and by far the cheapest ($0.0375/$0.15 per 1M tokens). If json_schema turns out unsupported on R7B, fall back to `command-r-08-2024`, or use R7B with `json_object` + prompt-side schema + client-side validation.

## 4. Rate limits — the deciding factor

Source: **https://docs.cohere.com/docs/rate-limits** (verified 2026-07-13).

| Endpoint | Trial key | Production key |
|---|---|---|
| Chat | 20 req/min (all models) | 500 req/min (Command A, R+, R, R7B); A+ family: contact sales |
| Embed (text) | 2,000 inputs/min | 2,000 inputs/min |
| Embed (images) | 5 inputs/min | 400 inputs/min |
| Rerank | **10 req/min** | 1,000 req/min |
| EmbedJob | 5 req/min | 50 req/min |
| Default (other) | 500 req/min | 500 req/min |
| **Overall monthly cap** | **1,000 API calls/month** | none (pay-as-you-go) |

Applied to the app's 3-calls-per-query pattern:
- **Per minute**: bottleneck is rerank at 10/min → ~10 queries/min on trial. A human clicking around a desktop app will never hit this.
- **Per month**: 1,000 calls ÷ 3 = **~333 queries/month (~11/day)** on a trial key. This is the real limit.
- Production key: effectively unlimited for a single user (500 chat/min, 1,000 rerank/min).

## 5. Production pricing and expected per-query user cost

Canonical page is cohere.com/pricing (prices render client-side; figures below cross-checked across the pricing page, docs, and multiple trackers as of July 2026):

- **Embed v4**: **$0.12 per 1M input tokens** (text; image tokens $0.47/1M)
- **Rerank**: per search unit (1 query + up to 100 docs, 500-token chunking): **Rerank 4 Pro $2.50 / 1k searches**, **Rerank 4 Fast ~$2.00 / 1k**, Rerank 3.5 $2.00 / 1k (some trackers list $1.00 — confirm on cohere.com/pricing before publishing user-facing numbers)
- **Chat**: `command-r7b` **$0.0375 in / $0.15 out per 1M tokens**; `command-a-03-2025` $2.50 in / $10.00 out; `command-r-plus-08-2024` $2.50/$10.00; Command A+ has no public per-token hosted rate (open-weights Apache 2.0 release)

**Expected production cost per query** (1 embed of a ~20-token query + 1 rerank search unit + R7B chat with ~5k input / ~300 output tokens):
- Embed: ~$0.0000024 (negligible)
- Rerank (v4-fast): ~$0.0020 — **dominates**
- Chat (R7B): ~$0.00024
- **Total ≈ $0.002–0.003 per query** (~$0.25 for 100 queries). Without rerank: ~$0.0003/query.

## 6. SDKs and CORS

- **TypeScript**: official, actively maintained — `cohere-ai` on npm, repo github.com/cohere-ai/cohere-typescript; supports Cohere platform plus Bedrock/SageMaker/Azure/GCP/OCI. Official SDK languages: Python, TypeScript, Java, Go (docs.cohere.com/reference/about).
- **Rust**: **no official SDK.** Community crate `cohere-rust` (github.com/walterbm/cohere-rust, on crates.io/docs.rs) exists but is community-maintained and may lag the v2 API (embed-v4/Chat v2/json_schema). **Recommendation: plain REST via `reqwest`** against `api.cohere.com/v2/{embed,rerank,chat}` — the three calls are simple JSON POSTs; a thin hand-rolled client avoids dependency rot.
- **CORS**: verified empirically today — `OPTIONS https://api.cohere.com/v2/chat` with a cross-site Origin returns `access-control-allow-origin: *`, `access-control-allow-methods: POST`, `access-control-allow-headers: Authorization, Content-Type`. So the API **can** technically be called directly from a browser/webview. Still route calls through the Rust backend: keeps the user's key out of webview JS context, centralizes retry/429 handling, and avoids depending on a CORS policy Cohere could tighten at any time.

## 7. Terms of service

From cohere.com/terms-of-use and cohere.com/pricing (July 2026):

- **Shipping prebuilt embeddings of your own corpus**: no clause prohibits distributing embedding vectors of your own content to end users. The ToU restricts sharing of *custom (fine-tuned) models*, not API outputs. The relevant restriction is that outputs/Content may not be used "for the purpose of building a similar or competitive product" (i.e., don't train a competing embedding model on the vectors) — shipping vectors as app data for retrieval is not that.
- **Trial keys**: free, rate-limited, and per the pricing page **not for commercial use** — they are evaluation keys. There is no explicit ToU clause forbidding an end user from using their trial key inside a third-party app, but a commercially shipped app should document the production-key path.
- **End users' own keys in a third-party app**: no prohibition found. Standard obligations apply: the key holder is responsible for keeping credentials confidential and for usage under their account — which aligns with the bring-your-own-key design (store the key with Windows DPAPI/credential manager, never bundle your production key in the installer).

## Final recommendation

Ship on **embed-v4.0** (search_document/search_query, 512–1024 dims, float or int8) + **rerank-v4.0-fast** + **command-r7b** (json_schema if a live test confirms support, else json_object + validation). **Support trial keys at runtime** — they work for ~333 queries/month, which fits a personal desktop recommendation tool — but detect 429s and the monthly-cap error and present a one-click path to a production key (~$0.002–0.003/query, rerank-dominated). Keep the corpus re-embedding pipeline automated as insurance against future embed-model deprecation, and route all API calls through the Rust backend even though CORS is currently open.

## Sources

- https://docs.cohere.com/docs/rate-limits — trial vs production limits, 1,000 calls/month trial cap
- https://docs.cohere.com/docs/models — model list, dimensions, context windows
- https://docs.cohere.com/docs/embeddings — input_type, embedding_types, Matryoshka dims
- https://docs.cohere.com/docs/rerank and https://docs.cohere.com/reference/rerank — rerank models, 1,000-doc guidance, max_tokens_per_doc
- https://docs.cohere.com/docs/structured-outputs — json_object / json_schema support and model list
- https://docs.cohere.com/docs/deprecations — lifecycle policy, v2.0 embed shutdown April 4 2026
- https://docs.cohere.com/docs/how-does-cohere-pricing-work and https://cohere.com/pricing — pricing structure, trial "free but limited / not commercial"
- https://docs.cohere.com/docs/command-r7b — R7B positioning
- https://github.com/cohere-ai/cohere-typescript, https://docs.cohere.com/reference/about, https://github.com/walterbm/cohere-rust — SDK status
- https://cohere.com/terms-of-use — output/competition restrictions
- Live CORS probe of https://api.cohere.com/v2/chat (2026-07-13)
- Cross-check pricing trackers: pricepertoken.com/pricing-page/provider/cohere, openrouter.ai/cohere/rerank-4-pro, aipricing.guru/cohere-pricing
