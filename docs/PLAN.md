# Plan — "Modus": a problem → method advisor (Phase 1 deliverable)

> Status: **awaiting sign-off**. Nothing below is implemented yet; this is the proposal.
> Research grounding: 44 technique notebooks analyzed into structured cards ([research/corpus/](../research/corpus/)),
> ontology + catalog synthesized ([ontology.json](../research/corpus/ontology.json), [catalog.md](../research/corpus/catalog.md)),
> and four web-verified research reports ([docs-acquisition](../research/docs-acquisition.md),
> [cohere-platform](../research/cohere-platform.md), [desktop-stack](../research/desktop-stack.md),
> [ui-stack](../research/ui-stack.md)).

---

## 1. Product definition

A Windows desktop app for technical practitioners. The user describes a problem in plain
English ("my retrieved chunks are technically on-topic but the answers keep missing the
point"); the app reasons over **curated, offline-prepared knowledge packs** and returns
the best-fit techniques with:

- a short, specific justification per recommendation ("why this fits *your* symptom"),
- honest tradeoffs and what it costs to adopt,
- alternatives and composition advice ("pair this with a reranker"),
- links to the exact source material, viewable **in-app** (notebook cells, docs sections).

v1 ships two packs: **RAG Techniques** (from NirDiamant/RAG_Techniques) and
**LangChain/LangGraph docs** (fresh framework guidance). The core is pack-agnostic;
RAG is content, not architecture.

### Name (proposal)

**Modus** — Latin for *method, manner* ("modus operandi"). Short, premium, memorable, and
not RAG-locked: future packs (agent memory, prompt engineering) fit naturally under
"the method advisor." Alternatives if you dislike it: **Praxis** (theory applied to
practice), **Vademecum** (a handbook carried for ready reference), **Fieldbook**,
**Counsel**. The plan uses "Modus" throughout; swapping is a rename.

---

## 2. What the research established (the load-bearing facts)

### 2.1 The corpus (all 44 notebooks read and analyzed)

Each notebook now has a structured **technique card** in `research/corpus/<slug>.json`:
problem solved, failure modes addressed, when to use / when not, tradeoffs, dependencies,
key code signals (imports/calls as ground truth), stage, relationships, complexity,
keywords, and an embedding-ready 120–180-word summary. A synthesis pass produced:

- **7 stages** (the RAG lifecycle): chunking → indexing & enrichment → query
  transformation → retrieval → post-retrieval → orchestration → evaluation.
- **25 deduplicated failure modes** with plain-English "user phrasings" (e.g. *"Retrieved
  chunk lacks surrounding context"*, *"Right passage retrieved but ranked too low"*,
  *"Answers live in figures, tables, and charts"*), each mapped to the techniques that
  address it.
- A **relationship graph** with typed edges: `composes_with`, `alternative_to`,
  `prerequisite_of`, `refines`, `evaluated_by`.

Insights that directly shape the recommendation engine:

1. **The opposite-remedy trap.** "Context starved" (fragments, missing surroundings) wants
   *expansion* (context windows, relevant segment extraction); "context polluted" wants
   *shrinking* (compression, reranking, propositions). Users phrase both as "my answers
   are bad/incomplete." The advisor must disambiguate — sometimes by asking one targeted
   clarifying question — before recommending, or it will confidently recommend the
   opposite of the right fix.
2. **Index-time vs query-time mirror pairs** (HyDE ↔ HyPE, query transformations ↔
   contextual chunk headers). The tiebreakers are "can you re-index?" and "what's your
   per-query latency budget?" — the advisor should surface these as deciding questions.
3. **A self-correction escalation ladder** — reranking → reliable RAG → CRAG → Self-RAG →
   Agentic RAG — these are escalation steps, not a stack; recommending several together
   is wrong.
4. **Reranking is the composition hub**: recall-boosting recommendations (hybrid search,
   query expansion) should co-recommend a reranker; RSE has a hard prerequisite edge on
   reranker scores.
5. **Framework twins** (5 techniques have LangChain + LlamaIndex variants) collapse into
   one technique with an implementation choice.
6. Some failure modes have essentially **one** answer (feedback loops, exact-term misses,
   judge validation, visual content) — the advisor should say so plainly instead of
   padding to N recommendations.
7. Two notebooks are **sponsor-affiliated** (Contextual AI, Zilliz/Milvus) — cards carry a
   vendor-lock-in disclosure the UI must render.

### 2.2 License (constraint on the whole product)

RAG_Techniques uses a **custom non-commercial license**: redistribution of modified/derived
content is permitted for non-commercial purposes with attribution — credit *Nir Diamant*,
link the repo, and mark content as modified; no implied endorsement. **The app must remain
free and non-commercial** unless written permission is obtained (nirdiamant21@gmail.com).
Consequences baked into the design:

- Pack manifests carry **required license + attribution fields**; the UI renders
  attribution on every source view and in About.
- Processed notebook content is labeled "adapted from" with a link to the original.
- LangChain docs are MIT (verified in github.com/langchain-ai/docs LICENSE) — clean, with
  attribution bundled in the pack.

### 2.3 Cohere platform (verified against docs.cohere.com, 2026-07-13)

- **Embeddings: `embed-v4.0`** — Matryoshka dims 256/512/1024/1536, 128k-token input,
  `input_type=search_document` at build time / `search_query` at runtime (required
  pairing), float/int8/binary output. Best deprecation outlook (v2.0 family was shut down
  April 2026; v3.0 is aging). We ship raw card text alongside vectors so the index can
  always be rebuilt.
- **Rerank: `rerank-v4.0-fast`** — our top-K fits one call = 1 billed search unit.
- **Generation: `command-r7b-12-2024`** (cheapest, 128k context) with `json_schema`
  structured output — *needs one live verification call*; fallback `command-r-08-2024`.
- **Trial key verdict: sufficient.** Binding constraint is 1,000 API calls/month
  (per-minute limits are irrelevant for one human). At 3 calls/query that's ~333
  queries/month (~11/day); with rerank off, ~500. **The app accepts a trial key by
  default** and treats quota exhaustion gracefully (clear error + "add production key"
  path + local usage meter). Production cost if upgraded: **~$0.002–0.003/query**.
- **No Rust SDK** — plain REST via `reqwest` against `api.cohere.com/v2/*` from the Rust
  backend. CORS is currently open but we do not architect webview-direct calls (key never
  enters the webview).
- ToS: shipping precomputed embeddings of our own corpus is fine; BYO-key in a
  third-party app is fine; trial keys are non-commercial (consistent with our license
  posture anyway).

### 2.4 LangChain/LangGraph docs acquisition (verified 2026-07-13)

- Everything now lives on **docs.langchain.com** (Mintlify); `python.langchain.com`
  308-redirects there. LangChain Python at `/oss/python/langchain/*`, LangGraph at
  `/oss/python/langgraph/*`.
- **Append `.md` to any page URL → clean per-page markdown** (Python variant
  pre-resolved, headings/code fences intact). `sitemap.xml` (1,441 URLs, per-page
  `lastmod`) is the only complete machine-readable index.
- `llms.txt` is a trap: it omits **all** OSS (LangChain/LangGraph) pages. `llms-full.txt`
  (13.6 MB, 1,491 pages) does include them — usable as fallback + completeness
  cross-check. Git clone of the docs repo is *not* viable as primary (raw MDX with
  unresolved includes; rendered output not committed).
- **Chosen method:** sitemap-scoped fetch of ~40–80 retrieval-relevant pages as `.md`
  (URL allowlist: retrieval, retrievers/vectorstores/splitters/embeddings integrations,
  agentic-rag, memory/persistence), monthly refresh, `lastmod` diff confirmed by content
  hash, pack versioned `langchain-docs-YYYY.MM.N`, loud failure (human review) if
  allowlisted URLs 404 or page count drops >10% (the docs have reorganized before and
  will again). MIT license + `ai-train=yes` robots signal = clean.

### 2.5 Desktop stack (verified 2026-07)

**Tauri 2.11.x is the right call** — and the numbers back your instinct: 5–15 MB
installers, ~45–95 MB idle RAM, <0.5 s cold start (Electron: ~96 MB installers, 2× RAM;
Flutter: forfeits the web-tech UI craft we need). Details that matter:

- **Vector search: no vector DB.** At ≤10k vectors × 1024-dim f32, exact brute-force
  cosine over pre-normalized embeddings in contiguous memory is ~1–5 ms with SIMD
  (`simsimd` + `rayon`), 100% recall, zero dependencies. usearch/LanceDB are overkill;
  sqlite-vec is still pre-v1. Vectors load at startup (overlapping WebView2 spawn).
- **BM25: SQLite FTS5** — rusqlite's bundled SQLite enables FTS5; the FTS index is just
  shadow tables, so it's **prebuilt into the shipped pack .db** (external-content mode +
  `optimize`). Fusion via **Reciprocal Rank Fusion, k=60**.
- **Packs = SQLite files** bundled as Tauri resources, ATTACHed read-only
  (`file:...?mode=ro&immutable=1`); one writable `app.db` (WAL) in `app_data_dir()` for
  history/settings. `rusqlite` in Tauri commands; skip tauri-plugin-sql/-store/Stronghold
  (Stronghold is deprecated for Tauri v3).
- **API key: `keyring` crate → Windows Credential Manager** (2560-byte blob limit is
  ample). Graceful fallback to per-session prompt if a domain policy disables it.
- **Installer: NSIS, `currentUser`** (perMachine breaks silent updates), WebView2
  `downloadBootstrapper`, updater plugin. Embedding blobs are high-entropy (won't
  compress — size estimate below is realistic). Code signing is the one open money
  question (SmartScreen friction without it).

### 2.6 UI stack (verified 2026-07)

- **React 19 + Vite + TypeScript.** Decisive because the craft-critical ecosystems are
  React-only: **Base UI v1.0** (ex-Radix team; Radix has slowed post-acquisition) via
  shadcn-style components, React Aria hooks for the hardest widgets, **Motion v12**,
  and all markdown/notebook rendering prior art. Framework runtime size is irrelevant in
  a local-asset webview.
- **Tokens: 3-layer OKLCH CSS custom properties** (primitive → semantic → component) with
  Tailwind v4 CSS-first `@theme inline`; themes/density/motion as `data-*` root
  attributes. **WCAG AA enforced in CI**: a Vitest matrix over every theme × token pair
  (culori, hard fail at 4.5:1 / 3:1) + axe-core Playwright on the rendered app. The
  user-accent ramp generator shares the same clamp function, so no accent choice can
  break AA.
- **Rendering: react-markdown + remark-gfm + rehype-sanitize; Streamdown for streaming
  answers; Shiki v4 with the JS regex engine** (no WASM — the default WASM engine
  violates a strict Tauri CSP), lazy language loading, dual-theme CSS variables.
- **Notebook viewer: custom, ~500 lines** over nbformat JSON, reusing the markdown+Shiki
  stack; mime-bundle priority dispatch; **DOMPurify mandatory** on `text/html` outputs
  (pandas tables are an XSS vector); `anser` for ANSI tracebacks; TanStack Virtual for
  long notebooks. (nteract renderers are stale — pattern references only.)
- **Motion:** Motion v12 + same-document View Transitions (Baseline in WebView2),
  transform/opacity only, global `--motion-scale` token (0 / 0.5 / 1) honored by both CSS
  and Motion, `prefers-reduced-motion` respected.
- **Sound:** ~40 lines of raw Web Audio (AudioContext + decoded buffer pool + master
  gain); Kenney / ObsydianX **CC0** UI sound sets; **off by default**.
- **Fonts:** Inter Variable (UI) + JetBrains Mono Variable (code), both OFL 1.1 (bundling
  explicitly permitted), subset woff2.
- **Windows polish:** window-vibrancy **Mica** (not Acrylic — documented resize lag),
  opaque fallback for Windows 10 / reduced-transparency; Snap-Layouts-capable custom
  titlebar (tauri-plugin-decorum pattern) with real aria-labelled window controls;
  system accent via CSS `AccentColor`; `forced-colors` (Contrast Themes) audit.
- Reference apps to study: **Yaak**, **Cap**, **Jan**.

---

## 3. Architecture

```
┌────────────────────────────── Modus (Tauri 2) ──────────────────────────────┐
│                                                                             │
│  WebView (React 19)                    Rust core (src-tauri)                │
│  ┌───────────────────────┐             ┌──────────────────────────────────┐ │
│  │ advisor chat + results│  IPC (typed │ engine/                          │ │
│  │ source viewer (ipynb, │  commands + │   packs.rs    load/attach packs, │ │
│  │  markdown docs)       │  events for │               manifest registry  │ │
│  │ history sidebar       │  streaming) │   search.rs   dense SIMD cosine  │ │
│  │ settings panel        │◄───────────►│               + FTS5 BM25 + RRF  │ │
│  │ design system (tokens,│             │   advisor.rs  Cohere v2 REST:    │ │
│  │  motion, sound)       │             │               embed/rerank/chat  │ │
│  └───────────────────────┘             │   keys.rs     keyring (WinCred)  │ │
│                                        │   history.rs  app.db (WAL)       │ │
│                                        │   quota.rs    local usage ledger │ │
│                                        └──────────────────────────────────┘ │
│  resources/packs/*.pack (read-only SQLite)      %APPDATA%/modus/app.db      │
└─────────────────────────────────────────────────────────────────────────────┘

Offline (never on user machines):
pipeline/ (Python)  — sources → processor (by source_type) → curation merge →
                      embed (Cohere, production key) → pack.db build → validate
```

**Core is pack-agnostic.** The Rust engine knows only the pack schema (manifest,
techniques, chunks, embeddings, failure modes, relations, documents). Everything
RAG-specific lives in pack *content* — including the ontology (stages/failure modes are
pack data, not code). A future "agent memory" pack reuses the engine untouched.

**Contribution model (per your spec): open code-level extensibility, not a sealed plugin
API.** Adding a pack that fits existing machinery = add a recipe + curation data under
`pipeline/packs/`. Adding a new *source type* = add a processor module (normal PR, may
touch core seams if the type demands it — e.g. a PDF pack adding figure rendering to the
source viewer). The seams are kept clean (processor interface, pack schema version,
renderer registry keyed by document kind) so deep changes are possible without fighting
the design. `CONTRIBUTING.md` documents both paths with the notebook processor as the
worked reference example.

---

## 4. Knowledge pack format (v1 spec sketch)

One pack = **one SQLite file** `<pack-id>.pack` — single-file, mmap-friendly,
transactional, prebuilt FTS, trivially ATTACHed. Schema version gates compatibility.

```
manifest (key/value)                    techniques
  schema_version   = 1                    slug PK, title, one_liner, stage,
  pack_id          = rag-techniques       problem_solved, how_it_works,
  pack_version     = 2026.07.0            when_to_use JSON, tradeoffs JSON,
  name, description                       complexity, keywords JSON,
  source_type      = notebook             summary, vendor_disclosure NULL,
  embedding_model  = embed-v4.0           doc_id FK → documents
  embedding_dims   = 1024
  embedding_input_type = search_document
  license_id, license_text,
  attribution_html (rendered in-app)
  built_at, source_ref (git sha / docs snapshot date)

failure_modes: id, name, description,   technique_failure_modes: technique, fm_id, weight
  user_phrasings JSON                   technique_relations: from, to, relation
stages: id, name, description, position

chunks: id, technique_slug NULL, doc_id, heading_path, kind (markdown|code|mixed),
        text, token_count, location JSON (cell indexes / anchor)
embeddings: chunk/card id, vector BLOB (f32 LE, pre-normalized)
chunks_fts: FTS5 external-content over chunks.text + technique summaries (PREBUILT)

documents: id, kind (notebook|webdoc), title, source_url, license_note, content JSON
  - notebook: sanitized nbformat cells (markdown, code, whitelisted outputs)
  - webdoc:   markdown + heading tree
```

Design points:

- **Two embedding tiers**: technique-card summaries (the primary recommendation targets)
  and section chunks (evidence + docs-pack content). Card vectors give clean
  technique-level matching; chunk vectors give citations and depth.
- **Documents ship in the pack** so sources render in-app offline, with provenance
  (`location`) linking every chunk to its exact cells/section for deep-linking.
- **License is structural**: `attribution_html` is required; the app refuses to load a
  pack without it.
- Packs are read-only forever; user data never writes into a pack. New pack = new file;
  upgrade = replace file. `pack_id` + `pack_version` registered in app.db on first load.
- Full spec with DDL + authoring guide lands as `docs/PACK_FORMAT.md` in Phase 2.

---

## 5. Build pipeline (offline, Python)

`pipeline/` — Python 3.12, `uv`-managed, CLI: `modus-pack build packs/rag-techniques/`.

```
pipeline/
  core/            recipe loader, pack writer (SQLite), embedder (Cohere, PROD key,
                   batched, cached by content-hash), validator
  processors/
    notebook/      ★ reference implementation (see below)
    webdocs/       sitemap-scoped .md fetcher + heading-aware chunker
  packs/
    rag-techniques/  recipe.toml, curation/ (the 44 reviewed technique cards),
                     sources.lock (git sha)
    langchain-docs/  recipe.toml, allowlist.toml, snapshots/
```

Each recipe declares its `source_type`; the pipeline dispatches to that processor.
**Processors own the "raw → vetted knowledge" logic for their type** — nothing
one-size-fits-all in core, exactly per your design principle.

### Notebook processor (source-type-aware, the reference)

1. Parse with `nbformat`; build a section tree from markdown headers.
2. **Separate concerns per cell type**: markdown → prose chunks; code → signal extraction
   (imports, key calls — already validated by Phase 1 analysts) + code chunks kept intact
   (never split mid-function); outputs → whitelist only (small text/plain, selected
   images), strip base64 noise, pip installs, boilerplate.
3. **Curation merge**: the human-reviewable technique cards in `curation/` (seeded from
   `research/corpus/`, editable forever) are authoritative for problem/tradeoffs/failure
   modes; the processor attaches provenance and rendered document content.
4. Chunking: section-aware (header path prepended — eating our own dog food: that's
   contextual chunk headers), markdown+adjacent-code kept together, target ~250–500
   tokens.
5. Associate each notebook with its README entry; carry the repo taxonomy.

### Docs processor (webdocs)

Sitemap fetch → allowlist filter → per-page `.md` download (rate-limited) → strip
injected preamble → front-matter provenance (URL, lastmod, SHA-256) → heading-aware
chunking → embed → pack. Refresh = re-run; only changed hashes re-embed. Assertions fail
the build loudly on 404s/page-count drops.

### Embedding + validation

- `embed-v4.0`, `output_dimension=1024`, `input_type=search_document`, float32,
  L2-normalized at build time. (~4 KB/vector; both v1 packs ≈ 2–4k vectors ≈ **8–16 MB**
  — comfortably inside the premium footprint budget. int8 is a documented later
  optimization.)
- Content-hash embedding cache so iterating on pack metadata costs zero API calls.
- `modus-pack validate`: schema version, required manifest fields (license!), dangling
  relations, FTS integrity, vector count/dims consistency, sample query smoke test.

---

## 6. Recommendation engine (runtime)

Per query (~3 Cohere calls: embed + rerank + chat):

1. **Embed** the user's problem statement (`search_query`, 1024 dims).
2. **Hybrid retrieve** across all enabled packs: dense cosine (SIMD brute force) + FTS5
   BM25 → **RRF (k=60)** → top ~30 candidates (technique cards weighted above raw
   chunks; failure-mode `user_phrasings` are indexed too, so symptom language matches).
3. **Rerank** (optional, default on): `rerank-v4.0-fast` over candidate summaries → top 8.
4. **Advise**: `command-r7b` with a `json_schema`-constrained response:
   `{ recommendations: [{slug, fit, justification, tradeoffs_for_user, pair_with[]}],
      clarifying_question | null }` — prompt includes the retrieved cards, the ontology's
   relationship edges (so it can say "escalation step, don't stack"), and the
   **starved-vs-polluted disambiguation rule**: if the symptom is ambiguous between
   opposite remedies, return exactly one clarifying question instead of guessing.
   Slugs are validated against candidates; hallucinated slugs are dropped.
5. **Render**: recommendation cards (stage badge, failure-mode chips, fit reasoning,
   vendor disclosure where applicable, "escalation ladder" context) with streaming text;
   every card deep-links into the in-app source viewer at the exact notebook
   section/docs heading.

**Degraded modes are designed, not accidental:**

- No key / offline: pure local mode — BM25 + failure-mode phrase matching still rank
  techniques (no dense search without query embedding, no LLM prose); results show a
  "local match" badge and full source browsing works (packs are local).
- Trial quota exhausted (429): local mode + clear explanation + usage meter + "add
  production key" path. A local **quota ledger** (`quota.rs`) counts the month's calls
  so the meter warns *before* the wall.
- Rerank toggle in settings, labeled with its real effect: on = better precision,
  ~333 queries/month on trial; off = ~500.

---

## 7. App data model (writable `app.db`)

```
conversations: id, title, created_at, updated_at
messages:      id, conversation_id, role, content_md, recommendations JSON (as rendered),
               query_embedding_cached BLOB NULL, created_at
settings:      key, value JSON (mirrored to a typed TS/Rust settings schema)
pack_registry: pack_id, version, path, enabled, first_seen, attribution_shown
quota_ledger:  month, embed_calls, rerank_calls, chat_calls
```

History is a first-class citizen: past advisories re-render fully offline from the stored
recommendation JSON (no re-querying), searchable via a small FTS index over messages.

---

## 8. UX blueprint

**Layout**: three-pane — history sidebar (collapsible) | advisor thread (chat-style, but
each answer is a structured advisory, not a wall of prose) | source panel (dockable
right/bottom, opens on citation click, renders notebooks/docs with full fidelity).
Command palette (Ctrl+K): new query, switch conversation, open technique by name, toggle
theme, etc. Global shortcuts, all rebindable later.

**States**: designed empty state (example problems as clickable prompts — drawn from
failure-mode phrasings), skeleton loading with streaming reveal, actionable error states
(no key / bad key / quota / offline each get distinct, helpful screens), zero-result
state that suggests rephrasing with symptom vocabulary.

**Settings** (all live-applied, persisted, one panel with search):

| Group | Controls |
|---|---|
| Appearance | theme (light/dark/system + built-ins incl. high-contrast), accent color (AA-clamped ramp), transparency (Mica) on/off, density (compact/cozy/comfortable) |
| Typography | UI font (Inter/system), code font (JetBrains Mono/system mono), size scale, line height, ligatures |
| Motion | intensity: full / reduced / off (maps `--motion-scale`; system `prefers-reduced-motion` wins by default) |
| Sound | master on/off (default off), volume, per-event toggles (send, result, error, toggle) |
| Advisor | rerank on/off (with quota math shown), recommendations count, clarifying questions on/off |
| API | key entry (trial or production, auto-detected), stored in Windows Credential Manager, usage meter, test-connection |
| Packs | enable/disable per pack, versions, attribution/licenses, import pack file |
| Data | history retention, export (JSON/MD), clear |

**Accessibility = the same bar as premium** (per your framing): Base UI/React Aria
semantics, full keyboard nav with visible focus, `aria-live` announcements for streamed
results, AA contrast enforced in CI across every theme (including user accents), scalable
text without layout breakage, `forced-colors` support, no color-only meaning (stage
badges get icons + text).

---

## 9. Repository layout

```
modus/
  app/
    src/            React: features/ (advisor, sources, history, settings),
                    design-system/ (tokens.css, primitives, motion, sound)
    src-tauri/      Rust: engine/ (packs, search, advisor, keys, history, quota)
  pipeline/         Python pack builder (see §5)
  packs-out/        built .pack artifacts (gitignored; shipped via installer)
  research/         Phase-1 corpus cards + reports (curation seed, kept)
  docs/             PLAN.md (this), ARCHITECTURE.md, PACK_FORMAT.md, CONTRIBUTING.md
  .env / .env.example   Cohere keys (already set up; .env gitignored)
```

Key usage: **production key = pipeline only** (embedding the corpus at build time, per
your guidance). **Trial key = dev/runtime testing**, and end users bring their own trial
key (production optional for heavy use). The developer key is never bundled.

---

## 10. Implementation phases (reviewable increments)

| Phase | Deliverable | Review gate |
|---|---|---|
| 2 | `PACK_FORMAT.md` + pipeline core + notebook processor + built `rag-techniques.pack` (one production-key embedding run) + validator | inspect pack with SQLite browser; validator green |
| 3 | Tauri skeleton + Rust engine (pack load, hybrid search, keyring, Cohere client, quota ledger) + throwaway debug UI | query CLI/debug pane returns sane rankings; cold start < 1 s |
| 4 | Design system (tokens, themes, contrast CI) + advisor flow + recommendation cards + notebook/docs source viewer | the product moment: ask a problem, get advice, open the source |
| 5 | Settings panel (everything in §8), motion + sound polish, full a11y audit (axe + keyboard + screen reader pass) | a11y checklist signed off |
| 6 | `langchain-docs.pack` + webdocs processor + refresh workflow + `CONTRIBUTING.md` (pack authoring + new-source-type guide) | second pack proves extensibility claim |
| 7 | NSIS installer + updater + (signing decision) + history polish + end-to-end verify pass | installable build on a clean Windows machine |

---

## 11. Open questions for sign-off

1. **Name**: Modus? (alternatives: Praxis, Vademecum, Fieldbook, Counsel)
2. **Rerank default**: on (better precision, ~333 trial queries/mo) or off (~500)?
   Recommendation: **on**, with the visible usage meter.
3. **LangChain/LangGraph pack**: in v1 (Phase 6, as planned) — confirm, or defer to v1.1?
4. **Code signing**: an OV cert (~$100–300/yr) avoids SmartScreen warnings. For personal,
   non-commercial use you can skip it and click through SmartScreen once. Skip for now?
5. **Both packs bundled in one installer** (recommended; ~8–16 MB of vectors total) vs
   downloadable pack files + import UI only?
```
