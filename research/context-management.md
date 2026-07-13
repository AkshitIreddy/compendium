# Context Management Research — Conversation & Follow-up Design for the Advisor Chat

Researched: 2026-07-13. Web-verified patterns (2025–2026 production practice) synthesized against the
Phase-1 plan ([docs/PLAN.md](../docs/PLAN.md)): Tauri 2 + Rust engine, packs in read-only SQLite,
Cohere-only API (`embed-v4.0` @1024d, `rerank-v4.0-fast`, `command-r7b-12-2024` 128k ctx / 4k out,
fallback `command-r-08-2024`; `command-a-03-2025` 256k if the user upgrades). Corpus 2–10k chunks
now, 50k+ later. Directive: optimize for retrieval+reasoning quality, not call count.

---

## TL;DR recommendation

- **Pin the problem, window the middle, summarize the tail.** The first user problem statement +
  extracted hard constraints are kept **verbatim forever**; the last 3 exchanges are kept verbatim;
  everything older is folded into a **running summary** regenerated incrementally when the history
  block exceeds its token budget. Assistant advisories are *never* replayed as prose — they are
  replayed as a compact structured **advisor state** (slugs + verdicts), because the app already
  stores every answer as validated JSON.
- **One extra cheap chat call per turn** — a combined **router + query-rewriter** (`command-r7b`,
  `json_schema`) that classifies the message (`new_problem | followup_retrieve | followup_reuse |
  clarify_answer | meta`) and, when retrieval is needed, emits a standalone search query. A
  deterministic pre-filter catches greetings/commands so the router never over-rewrites them.
  This makes a retrieving turn 4 Cohere calls instead of 3 — accepted per the quality directive.
- **Local retrieval is free; only embed + rerank cost API calls.** So "reuse vs re-retrieve" is
  decided for *coherence*, not cost: reuse the conversation's cached candidate set when the user
  is asking *about what was already recommended*; re-retrieve whenever new symptom/constraint
  information enters the conversation.
- **Store structured traces, not raw prompts.** Turns hold rendered content; a 1:1 trace row holds
  route, rewritten query, per-stage retrieval ids+scores, prompt token accounting, model ids,
  latencies, and the validated advisory JSON. Chunk *ids* only — texts live in the immutable packs
  and are re-joined at render time. Raw request/response bodies only behind a debug setting.
- **Budget the prompt at ~28k tokens even though the model takes 128k.** Small models degrade well
  before nominal capacity ("context rot"); production guidance converges on staying under ~50% of
  the effective window and compacting at fixed thresholds. 28k also keeps `command-r7b` sharp and
  leaves the design unchanged if the user later switches to a bigger Command model.

---

## 1. Proven context-management patterns for multi-turn RAG chat (2025–2026)

### 1.1 The consensus architecture: pinned anchor + sliding window + running summary

Every serious production write-up of 2025–2026 (Google ADK's compaction module, Anthropic's
context-engineering guidance, agent-harness surveys) lands on the same three-layer shape:

1. **Pinned verbatim anchor** — the material that must never be paraphrased. For a chat assistant
   that's the system prompt; for an *advisor* it is additionally the **user's original problem
   statement** and any **hard constraints** they've stated ("can't re-index", "latency budget
   200 ms", "we're on LangChain + pgvector"). Summarization is lossy precisely on the details that
   make a recommendation fit *this* user — the opposite-remedy trap in PLAN §2.2.1 (context-starved
   vs context-polluted) can flip on a single adjective, so the words the user chose must survive.
2. **Sliding window of raw recent turns** — the last N exchanges verbatim, because follow-ups
   overwhelmingly reference the immediately preceding answer, and paraphrase there breaks
   coreference ("the second one you mentioned").
3. **Running summary of older turns** — one maintained summary block, updated by *folding* evicted
   turns into the previous summary (incremental, not re-summarizing the whole transcript), so cost
   is bounded and the summary can't silently drop the anchor (the anchor isn't in it — it's pinned).

### 1.2 When to summarize (thresholds)

Numbers seen in production guidance: Google ADK triggers token-based compaction at a configured
threshold and otherwise compacts on a sliding window with overlap; harness surveys report
compaction triggers around 70% of *effective* window and proactive management "well under 50% of
nominal capacity"; eviction typically starts with the oldest ~30% of messages. Two subtleties that
matter for us:

- **Summarize early, not at the cliff.** If you wait until the context is degraded, the model
  writing the summary is already impaired and the summary bakes in the degradation. Compact when
  the *history block* exceeds its own small budget (thousands of tokens), not when the whole
  prompt nears the window.
- **Summarize on turn-commit, asynchronously.** The fold-in runs after an answer is delivered
  (like title generation), so it never adds user-visible latency, and a failed fold just retries
  next turn (the raw turns are still in app.db — the summary is a cache, never the source of
  truth).

### 1.3 What to preserve verbatim vs compress

| Preserve verbatim (pin) | Compress into running summary | Drop entirely |
|---|---|---|
| Original problem statement (first substantive user message) | Older follow-up questions and the gist of answers | Greetings, thanks, chitchat |
| Hard constraints & environment facts, as a maintained bullet list extracted per turn | Reasoning digressions, rejected directions ("user decided against fine-tuning") | Streaming artifacts, retries |
| The user's answers to clarifying questions (they *refine the problem*) | Prose renderings of old advisories (state carries the essentials) | Full retrieved chunk texts from past turns (ids are in traces) |
| Current advisor state (structured, see §3) | | Raw tool/API payloads |

A structured-advisory app has an advantage over generic chat here: the assistant's past outputs are
already JSON, so "what was said" compresses losslessly to slugs + verdicts instead of an LLM
paraphrase. Use the LLM summary only for *user-side* content.

## 2. Follow-up handling: rewriting and routing

### 2.1 Contextualized query rewriting — the standard pattern

The standard (LangChain "history-aware retriever", ChatQA, SemEval-2026 Task 8 systems, TREC iKAT):
given chat history + the new message, generate **one standalone query** that resolves pronouns,
ellipsis, and implicit references, then run the normal retriever on it. Verified best practices:

- Rewrite **only the final user turn**; use history solely to resolve references — never to answer,
  add facts, or speculate.
- Prefer **explicit noun phrases over pronouns**; carry forward key domain terms from earlier turns
  ("it" → "the reranker you recommended for ranking-order failures").
- **Keyword-dense output** helps our hybrid setup specifically: the rewrite feeds both `embed-v4.0`
  (semantic) and FTS5 BM25 (lexical), so the rewriter prompt should instruct "include the concrete
  technique/failure-mode vocabulary the user is implying" — this is where the ontology's
  `user_phrasings` earn double duty as few-shot vocabulary.
- Two-stage variants (resolve coreference first, then rewrite) score higher in 2025–2026 evals but
  one good prompt with 3–5 in-context examples is the right complexity for us; examples should be
  drawn from our own domain (RAG symptoms), not generic trivia.
- If the rewrite fails or times out, **fall back to `summary-of-topic + raw message`** concatenation
  as the search string — never block the turn on the rewriter.

### 2.2 Failure cases (all reported repeatedly in production postmortems)

1. **Over-rewriting meta/chitchat.** "thanks!", "can you explain that more simply?", "what should I
   tell my team?" get rewritten into keyword soup and retrieve garbage which then *pollutes the
   answer*. Fix: route *before* rewrite (§2.3) and give the router an explicit `meta` class; also a
   deterministic pre-filter (short message + greeting/ack lexicon, no domain tokens) that skips the
   router entirely.
2. **Topic bleed.** After a topic switch, the rewriter drags old-topic entities into the new query
   ("chunking strategy for my *reranker* problem"). Research finding: when the conversation is
   topic-consistent, using full history helps retrieval; on topic switches it actively hurts. Fix:
   the router's `new_problem` class instructs the rewriter to ignore prior topical content (keep
   only environment constraints, which usually still apply).
3. **Clarifying-question answers treated as queries.** "the second one — we can re-index" is not a
   query; it's a *problem refinement*. It must be merged with the pinned problem statement into a
   refined statement, then retrieved as a whole. This is a first-class route for us because the
   advisor deliberately asks disambiguation questions (PLAN §6.4).
4. **Rewriting away the user's own words.** The rewrite is for *retrieval only*. The generation
   prompt always includes the user's verbatim message; the standalone query never replaces it.

### 2.3 Detecting new-topic vs follow-up vs meta

Three signals, cheapest first:

1. **Deterministic pre-filter** (Rust, no API): empty/greeting/ack lexicon match with no
   domain-vocabulary hit (check against pack keywords + failure-mode phrasings via FTS) → `meta`;
   message while a clarifying question is pending and message plausibly answers it (short, or
   references the offered options) → `clarify_answer` candidate (still confirmed by router).
2. **Embedding similarity** (optional, zero extra API cost only if we already embed): cosine
   between the new message embedding and (a) the pinned problem embedding, (b) the previous
   standalone query embedding. High-vs-both → follow-up; low-vs-both with domain terms → new topic.
   We cache `query_embedding` per turn anyway (schema §7), so this is available for telemetry and
   as a tiebreaker, but it costs an embed call before the route is known — so use it as a *check*,
   not the decider.
3. **LLM router** (the decider): one `command-r7b` call with `json_schema`, combined with the
   rewriter so routing+rewriting is a single call. Schema:

```json
{
  "route": "new_problem | followup_retrieve | followup_reuse | clarify_answer | meta",
  "standalone_query": "string|null      // required for new_problem/followup_retrieve",
  "refined_problem": "string|null       // required for clarify_answer: merged restatement",
  "referenced_slugs": ["..."],          // techniques from advisor state the user refers to
  "new_constraints": ["..."]            // constraint facts to append to the pinned list
}
```

Route semantics:
- `new_problem` — a different symptom/goal; retrieval from scratch; prior topical context excluded
  from the rewrite (constraints kept). Starts a new *segment* within the same conversation.
- `followup_retrieve` — same topic but introduces new information or asks something the cached
  candidates don't cover ("what about when the answers cite the wrong table?").
- `followup_reuse` — asks about/among the things already presented ("compare the first two",
  "what's the migration cost of HyPE?", "why not just rerank?"). No embed/rerank; answer from the
  conversation's cached cards + advisor state; the engine may pull *additional evidence chunks for
  already-known slugs* locally (SQL by slug, free, no API).
- `clarify_answer` — merges into `refined_problem`; full retrieve on the refined statement.
- `meta` — chitchat/meta/formatting requests ("make that shorter", "export this"); no retrieval,
  no advisory schema — plain chat answer grounded in history (or a pure-UI action when it matches
  an app command).

Router misroute safety: if the generation step finds it lacks grounding (model returns the schema's
`insufficient_context` flag), the engine falls back to `followup_retrieve` with the fallback search
string and retries once. Log both attempts in the trace.

## 3. Reusing prior work across turns

### 3.1 What to cache per conversation

Because packs are local and immutable, the *cheap* thing to cache is ids; the *valuable* thing to
cache is decisions:

- **Candidate cache** — per retrieving turn: the post-RRF candidate list and post-rerank top-K
  (chunk/card ids + scores). Union across turns = "the conversation's evidence pool". On
  `followup_reuse`, context is built from this pool (joined fresh from the pack by id).
- **Advisor state** (the compact "what I already told the user") — maintained structured object,
  updated from each advisory's validated JSON, rendered into every generation prompt:

```json
{
  "problem": "verbatim pinned statement",
  "refinements": ["can re-index", "latency budget tight"],
  "constraints": ["LangChain", "pgvector", "no fine-tuning"],
  "recommended": [
    {"slug": "reranking", "fit": "strong", "one_liner": "precision fix for ranking-order failures", "status": "presented"},
    {"slug": "contextual-chunk-headers", "fit": "moderate", "status": "user_rejected: cannot re-chunk yet"}
  ],
  "ruled_out": [{"slug": "self-rag", "why": "escalation step beyond current need"}],
  "open_question": null
}
```

  This is the anti-repetition mechanism: the generation prompt says *"these were already presented;
  do not re-pitch them — reference, compare, or extend"*. It is ~10× smaller than replaying the
  advisories as prose and it can't drift, because it's derived from validated JSON, not paraphrase.

- **Turn-level query embeddings** — cached (already in the Phase-1 sketch) for similarity checks
  and future "related conversations" features. Never re-billed.

### 3.2 When to re-retrieve vs reuse

Re-retrieval in this app costs 2 API calls (embed + rerank) and ~1–5 ms of local search. Per the
quality directive, **default to re-retrieving whenever it could help** — the reuse route exists for
*coherence* (answering about the same objects the user is looking at), not thrift:

| Situation | Action |
|---|---|
| New symptom, new failure mode, new constraint | Re-retrieve (rewritten standalone query) |
| Clarifying answer received | Re-retrieve on refined problem (the answer often flips the remedy direction) |
| Compare / explain / tradeoffs / "how do I implement X" for already-presented slugs | Reuse pool + local chunk fetch by slug (deeper evidence: implementation sections, code chunks) |
| "Any alternatives to X?" | Hybrid: local graph hop (`alternative_to` edges in the pack) seeds candidates + re-rerank locally cached scores; only re-retrieve if graph yields <3 candidates |
| Topic switch | Re-retrieve from scratch, constraints preserved |
| RAGBoost-style finding to respect | Keep reused documents in a **stable order** across turns and deduplicate — don't reshuffle the evidence pool between prompts; models track repeated context better when its position is stable (also future-proofs prompt caching if Cohere ships it) |

## 4. Agent-trace persistence

### 4.1 What production observability practice says

Per-step logging consensus (Braintrust, MLflow, agent-observability guides, 2026): per turn store a
trace id; per step store operation type, model/tool identity + version, inputs (redacted/ids),
output hash or structured output, token counts, latency, status/retries, cost. The full trace is
"the real execution record — if you don't store it you can't reconstruct why the agent decided".
For a desktop app the same idea, minus the telemetry stack: **one structured trace row per turn**
in app.db.

### 4.2 Structured trace vs raw transcript — store both, asymmetrically

- **Turns table (transcript)** = what the user saw: role, rendered markdown, and for advisories the
  validated recommendation JSON. This is the source for offline history re-rendering — no
  re-querying, no LLM. (Already a Phase-1 principle; kept.)
- **Trace table (mechanism)** = why: route + rewritten query, per-stage retrieval results as
  **ids + scores only** (`{dense: [...], bm25: [...], rrf: [...], rerank: [...]}`), context-builder
  accounting (which blocks were included, token count per block, summary version used), model ids,
  API latencies/status, validation outcomes (dropped hallucinated slugs!), error info.
- **Anti-bloat rules:** never store chunk *texts* in traces (join packs by id at view time; store
  `pack_id`+`pack_version` so a replaced pack renders an honest "source updated" notice); never
  store raw prompt/response bodies by default — the prompt is *reconstructible* from trace +
  summary version + packs. A `debug_traces` setting (default off) additionally stores gzipped raw
  request/response for engine debugging, with a "clear debug data" button.
- Expected size: a trace row is a few KB of JSON; 1,000 heavy turns ≈ single-digit MB. Fine in WAL
  SQLite; add a retention sweep alongside the existing history-retention setting.

## 5. Token budgeting (128k–256k model, quality-first)

Nominal window ≠ usable window. 2025–2026 production guidance: plan well under 50% of nominal;
compact at fixed block thresholds; allocate fixed overhead first, then give the flexible remainder
to retrieved evidence. `command-r7b` also caps output at 4k tokens, and long-context degradation
hits small models earliest. **Design the prompt to a ~28k target** (soft ceiling 40k):

| Block | Budget (tokens) | Notes |
|---|---|---|
| 1. System prompt + advisor rules + output schema | 2,000 | Static; includes disambiguation rule, escalation-ladder rule, anti-repetition rule, citation format |
| 2. Ontology hints (dynamic) | 1,500 | Only edges/failure modes touching current candidates — never the whole ontology |
| 3. Pinned problem + constraints + refinements | 800 | Verbatim; hard-capped by truncating middle of very long problem statements (keep head+tail), never paraphrased |
| 4. Advisor state (structured) | 800 | §3.1 object, serialized compactly |
| 5. Running summary of older turns | 1,000 | Regenerated when it would exceed this |
| 6. Recent raw turns (sliding window) | 4,000 | Last 3 user+assistant exchanges; assistant advisories rendered as their `one_liner`-level digest + prose intro, not full JSON |
| 7. Retrieved evidence | 14,000 | 8 technique cards (~600–800 tok each incl. tradeoffs) + top evidence chunks (~250–500 tok each); flexible: unused history budget flows here |
| 8. Current user message (verbatim) + rewritten query shown as "search intent" | 400 | |
| 9. Generation headroom | 4,000 | = R7B max output; with 256k `command-a-03-2025` keep prompt discipline identical, allow evidence to grow to ~30k, headroom 8k |
| **Total prompt** | **~24.5k** | ~19% of 128k — deliberately conservative |

Rules:
- **Evidence gets leftovers, history never does.** If history under-spends, add evidence chunks (in
  rerank order); if evidence under-spends, do *not* pad with more history.
- **Per-block truncation, oldest-first within a block**; block 3 and block 8 are never truncated to
  make room for anything else.
- **Summarization trigger:** when blocks 5+6 together would exceed 5,000 tokens, fold the oldest
  exchange(s) beyond the 3-exchange window into the running summary (async, after the turn
  commits) until they fit. Token counting: approximate with a local tokenizer heuristic
  (chars/3.6 for English + code); Cohere returns exact billed counts in responses — record them in
  the trace and use the running ratio to self-correct the estimator.

## 6. Title generation & conversation search

- **Title**: generate once, after the first substantive user+assistant exchange, via one small
  `command-r7b` call ("3–8 word noun-phrase title; no quotes"), fired async after the answer
  streams (never blocks; the DeerFlow/hermes-agent middleware pattern). Fallback (no key / offline
  / 429 / failure): first 6 significant words of the problem statement. Store `title_source`
  (`llm|fallback|user`); a user rename wins forever. Offer "regenerate title" in the sidebar
  context menu. If the first message routed `meta`, defer titling until the first `new_problem`
  turn.
- **Search**: FTS5 table in app.db over (title, turn content, running summary, recommended slugs),
  external-content over `turns`, porter tokenizer + `snippet()` for the sidebar results; also match
  technique slugs so "the conversation where it suggested HyPE" works. Trigger-maintained on
  insert/update/delete. Zero API cost, works offline — consistent with degraded-mode design.
- Nice extra (cheap, local): sidebar filter chips derived from advisor state (techniques
  recommended, stage) — pure SQL over stored JSON, no ML.

## 7. RECOMMENDED DESIGN

### 7.1 app.db schema additions (replaces the Phase-1 `messages` sketch)

```sql
-- conversations (extended)
CREATE TABLE conversations (
  id            INTEGER PRIMARY KEY,
  title         TEXT NOT NULL DEFAULT '',
  title_source  TEXT NOT NULL DEFAULT 'fallback',    -- llm | fallback | user
  created_at    INTEGER NOT NULL,                    -- unixepoch ms
  updated_at    INTEGER NOT NULL,
  archived      INTEGER NOT NULL DEFAULT 0
);

-- one row per user or assistant message (the transcript; offline re-render source)
CREATE TABLE turns (
  id               INTEGER PRIMARY KEY,
  conversation_id  INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  idx              INTEGER NOT NULL,                 -- 0,1,2… within conversation
  segment          INTEGER NOT NULL DEFAULT 0,       -- increments on new_problem route
  role             TEXT NOT NULL,                    -- user | assistant
  kind             TEXT NOT NULL,                    -- problem | followup | clarify_answer | meta
                                                     -- | advisory | clarifying_question | prose
  content_md       TEXT NOT NULL,                    -- what was rendered
  advisory_json    TEXT,                             -- validated recommendation JSON (assistant advisories)
  created_at       INTEGER NOT NULL,
  UNIQUE (conversation_id, idx)
);

-- one row per assistant turn: the mechanism (debugging + next-turn context feeding)
CREATE TABLE turn_traces (
  turn_id            INTEGER PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE,
  route              TEXT NOT NULL,                  -- router decision
  standalone_query   TEXT,                           -- rewritten search query (null for meta/reuse)
  query_embedding    BLOB,                           -- f32 LE, cached, never re-billed
  retrieval_json     TEXT,                           -- {dense:[{id,score}],bm25:[…],rrf:[…],rerank:[…],pack_versions:{…}}
  context_json       TEXT,                           -- blocks included + token count per block + summary_id used
  api_json           TEXT,                           -- per call: endpoint, model, latency_ms, status, billed tokens
  validation_json    TEXT,                           -- dropped slugs, schema retries, fallback route taken
  error              TEXT,
  debug_blob         BLOB                            -- gzipped raw req/resp, only if settings.debug_traces
);

-- versioned running summaries (append-only; latest is live, older kept for trace fidelity)
CREATE TABLE summaries (
  id               INTEGER PRIMARY KEY,
  conversation_id  INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  thru_idx         INTEGER NOT NULL,                 -- summarizes turns [0..thru_idx] minus pinned
  content          TEXT NOT NULL,
  token_estimate   INTEGER NOT NULL,
  created_at       INTEGER NOT NULL
);

-- one live row per conversation: pinned anchor + advisor state + candidate pool
CREATE TABLE conversation_state (
  conversation_id  INTEGER PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
  problem_md       TEXT,                             -- verbatim pinned problem statement
  constraints_json TEXT NOT NULL DEFAULT '[]',       -- accumulated hard constraints/refinements
  advisor_json     TEXT NOT NULL DEFAULT '{}',       -- §3.1 advisor state object
  pool_json        TEXT NOT NULL DEFAULT '[]',       -- union of candidate ids+scores+first_seen_idx
  open_question    TEXT,                             -- pending clarifying question, null if none
  updated_at       INTEGER NOT NULL
);

-- conversation search
CREATE VIRTUAL TABLE turns_fts USING fts5(
  content_md, title, slugs,
  content='turns', content_rowid='id', tokenize='porter unicode61'
);
-- + INSERT/UPDATE/DELETE triggers on turns; title+slugs backfilled from
--   conversations/advisor_json at trigger time.
```

`settings` gains `debug_traces` (bool, default false) and `history_retention_days`; the existing
`pack_registry` and `quota_ledger` are unchanged. Retention sweep deletes `debug_blob` first, then
whole conversations past retention.

### 7.2 Context-builder algorithm (per user message)

```
INPUT: conversation_id, user_message
STATE: conversation_state row, latest summary, last 3 exchanges, turn_traces of this conversation

STEP 0  Persist the user turn (kind provisional).

STEP 1  PRE-FILTER (Rust, 0 API calls)
        - greeting/ack lexicon + no FTS hit against pack keywords/failure phrasings → route=meta
        - app-command patterns ("export", "copy", "shorter") → meta (or UI action)
        - else if open_question != null → candidate clarify_answer (router confirms)

STEP 2  ROUTER+REWRITER (1 chat call, json_schema; skipped if pre-filter decided)
        prompt = router system prompt + few-shot (domain examples)
               + pinned problem/constraints + advisor state (slugs list)
               + last 3 exchanges + open_question + user_message
        output = {route, standalone_query, refined_problem, referenced_slugs, new_constraints}
        - on failure/timeout: route=followup_retrieve,
          standalone_query = summary_topic_line + " " + user_message
        - append new_constraints to conversation_state.constraints_json
        - route=clarify_answer → problem_md stays pinned; refined_problem becomes the
          retrieval text and is appended to constraints as a refinement
        - route=new_problem → segment += 1; problem_md := user_message (new pin);
          advisor_json topical fields reset (constraints kept); pool kept but flagged stale

STEP 3  RETRIEVE (routes new_problem / followup_retrieve / clarify_answer)
        a. embed standalone_query|refined_problem (search_query, 1024d)   [1 API call]
        b. hybrid local search: SIMD dense + FTS5 BM25 → RRF k=60 → top 30
           (cards weighted over chunks; failure-mode phrasings indexed)
        c. merge with pool_json: union, stable order = first_seen then rerank score;
           dedupe by id
        d. rerank merged top ~40 → top 8 cards + top evidence chunks     [1 API call]
        e. update pool_json (ids + scores + first_seen_idx)
        RETRIEVE-LOCAL (route followup_reuse):
        a. resolve referenced_slugs against advisor state
        b. fetch those cards + deeper chunks by slug from packs (SQL, free)
        c. "alternatives" asks: hop alternative_to/composes_with edges for seeds
        d. no embed/rerank calls
        (route meta: skip entirely)

STEP 4  ASSEMBLE PROMPT (budgets from §5; per-block truncation only)
        [2k]  system + rules + json_schema
        [1.5k] ontology edges/failure modes touching current candidates only
        [0.8k] pinned problem_md + constraints_json (never truncated vs other blocks)
        [0.8k] advisor state (anti-repetition contract: "already presented — do not re-pitch")
        [1k]  latest summary (if any)
        [4k]  last 3 exchanges (advisory turns as digest, not full JSON)
        [14k] evidence: 8 cards + chunks in stable pool order; leftover history budget flows here
        [0.4k] user_message verbatim + "search intent: <standalone_query>"
        headroom 4k output

STEP 5  GENERATE (1 chat call, json_schema advisory | plain chat for meta)
        - validate slugs against candidates; drop hallucinations (log in validation_json)
        - insufficient_context flag → fallback to followup_retrieve once (STEP 3), retry
        - stream to UI

STEP 6  COMMIT (single transaction)
        - assistant turn (content_md + advisory_json) + turn_trace (route, retrieval ids+scores,
          context accounting w/ billed-token actuals, api stats, validation)
        - update advisor_json (new recommendations → status presented; user reactions from this
          turn → user_rejected/adopted), open_question, pool_json, updated_at

STEP 7  ASYNC POST-TURN (never blocks; failures retry next turn)
        - if history block (summary + raw turns beyond window) est. > 5k tokens:
          fold oldest evicted exchange(s) into a NEW summaries row (1 small chat call)
        - if conversation has no llm title and this was the first substantive exchange:
          title call (1 small chat call); fallback = truncated problem_md
        - FTS triggers have already indexed the turns

Cohere calls per turn: meta 1 (or 0 if pre-filtered to a canned/local reply … recommend still 1 for
quality), followup_reuse 2 (router + generate), retrieving turn 4 (router + embed + rerank +
generate), +occasional async summary/title. Quota ledger counts all of them; the §6-of-PLAN usage
meter copy updates from "3 calls/query" to "3–4".
```

Degraded mode (no key / 429): pre-filter still routes; retrieval falls back to BM25 + phrasing
match on the *raw message + pinned problem* (no rewrite available); no generation — local-match
result cards render with the existing "local match" badge; turns/traces still recorded so the
conversation upgrades gracefully when a key returns.

### 7.3 Follow-up routing decision tree (compact)

```
user message
├─ pre-filter: greeting/ack/app-command, no domain terms ──────────────→ META (no retrieval)
├─ open clarifying question pending? → router biased toward CLARIFY_ANSWER
└─ router (1 cheap json_schema call)
   ├─ META ............... answer from history/state; no retrieval; no advisory schema
   ├─ FOLLOWUP_REUSE ..... about already-presented slugs → local fetch by slug/graph; no embed/rerank
   ├─ FOLLOWUP_RETRIEVE .. new info or uncovered ask → rewrite → embed+RRF+rerank, pool-merged
   ├─ CLARIFY_ANSWER ..... merge into refined problem → full retrieve on refinement
   └─ NEW_PROBLEM ........ new segment; re-pin problem; constraints survive; full retrieve
        └─ any route: generation reports insufficient_context → retry once as FOLLOWUP_RETRIEVE
```

### 7.4 What this changes in PLAN.md (delta summary)

1. `messages` table → `turns` + `turn_traces` + `summaries` + `conversation_state` (§7.1).
2. Engine gains `router.rs` (pre-filter + router/rewriter call) and `context.rs` (block assembly,
   budget enforcement, summary folding); `advisor.rs` consumes assembled context.
3. Advisory `json_schema` gains `insufficient_context: boolean` and per-recommendation
   `already_presented: boolean` (render as "as discussed" instead of a fresh card).
4. Per-query call count in user-facing quota copy: 3 → 3–4 (+async title/summary), with the meter
   already designed to absorb it.
5. History export (Data settings) should export turns + advisor state, optionally traces —
   dovetails with the dossier-for-another-AI goal: "export conversation as dossier" = pinned
   problem + advisor state + cited evidence, which this design keeps maintained at all times.

### 7.5 Risks / open questions

- **R7B as router**: `json_schema` support on `command-r7b-12-2024` still needs the live
  verification flagged in Phase 1; the router uses the same fallback (`json_object` + validation,
  or `command-r-08-2024`). Router quality on a 7B model is the main quality risk — mitigate with
  domain few-shots, the deterministic pre-filter, and the insufficient-context retry; if misroutes
  show up in traces, promote only the router call to `command-r-08-2024` (still cheap).
- **Token estimator drift** for code-heavy chunks: self-correct from billed counts in traces.
- **Summary quality on trial-key outages**: summary folding is skipped when quota-exhausted; the
  raw-turn window then grows past budget — acceptable temporary degradation; oldest raw turns are
  hard-evicted (listed by title-line only) at 2× budget.
- **50k+ chunk future**: nothing here changes — pool sizes, budgets and schema are corpus-size
  independent; only STEP 3b's brute-force scan has a separate scaling plan (already noted in
  desktop-stack research).

---

## Sources

- https://google.github.io/adk-docs/context/compaction/ — ADK context compression: token-threshold + sliding-window compaction, summary written back as event
- https://arize.com/blog/context-management-in-agent-harnesses/ — memory/files/subagents, proactive management well under 50% of window
- https://tianpan.co/blog/2026-02-26-context-engineering-memory-compaction-tool-clearing — compaction thresholds (~70% effective window), tool-output clearing
- https://zylos.ai/research/2026-03-31-context-window-management-session-lifecycle-long-running-agents/ — session lifecycle, degraded-summary ("context rot") warning
- https://agentmarketcap.ai/blog/2026/04/11/agent-context-engineering-sliding-windows-memory-2026 — sliding-window eviction ratios, hierarchical summarization
- https://arxiv.org/pdf/2502.15009 — in-context learning for conversational query rewriting (prompt design, few-shot curation)
- https://arxiv.org/pdf/2507.04884 — decontextualizing user questions for open-retrieval conversational QA
- https://arxiv.org/pdf/2401.10225 — ChatQA: conversational QA/RAG, rewrite-then-retrieve baselines
- https://dl.acm.org/doi/10.1145/3626772.3657933 — multi-query rewriting for conversational passage retrieval
- https://arxiv.org/pdf/2606.11945 and https://arxiv.org/pdf/2606.28352 — SemEval-2026 Task 8 multi-turn RAG systems (query rewriting + hybrid retrieval + reranking)
- https://arxiv.org/pdf/2509.15588 — TREC iKAT 2025: query reformulation + rank fusion in personalized conversational search
- https://arxiv.org/pdf/2504.20624 — PaRT: retrieval-intent classes (natural transition / explicit / implicit retrieval)
- https://medium.com/@talon8080/mastering-rag-chabots-semantic-router-user-intents-ef3dea01afbc — semantic-router intent gating of retrieval
- https://arxiv.org/pdf/2511.03475 — RAGBoost: cross-turn retrieved-context overlap, stable ordering + dedup for reuse
- https://www.emergentmind.com/topics/multi-turn-retrieval-augmented-generation-rag — topic-switch vs topic-consistent history use in retrieval
- https://arxiv.org/html/2506.11092v1 — dynamic context tuning for multi-turn RAG planning
- https://www.motomtech.com/blog-post/agentic-ai-observability-tool-call-logging/ — per-tool-call logging fields (ids, latency, redacted args, token counts)
- https://www.braintrust.dev/articles/agent-observability-tracing-tool-calls-memory and https://www.braintrust.dev/articles/agent-observability-complete-guide-2026 — trace structure, session JSON as execution record
- https://mlflow.org/llm-tracing — spans/trace model for LLM apps
- https://github.com/NousResearch/hermes-agent/issues/624 — async lightweight title-generation middleware pattern (trigger, fallback, non-blocking)
- https://dev.to/swapnanilsaha/llm-context-window-token-budget-why-your-window-fills-up-fast-4c05 — worked token-budget breakdowns
- https://www.72technologies.com/blog/token-budgets-for-rag-retrieval-bloat — fixed-overhead-first budgeting, retrieval bloat control
- https://apxml.com/courses/getting-started-with-llm-toolkit/chapter-3-context-and-token-management/managing-token-budgets — block-based budget management
- Project grounding: docs/PLAN.md, research/cohere-platform.md, research/desktop-stack.md (this repo)
