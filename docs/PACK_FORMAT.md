# Compendium knowledge pack format — schema v1

A knowledge pack is **one SQLite file** (`<pack-id>.pack`) containing everything the
app needs to recommend from a subject area: curated technique cards, a failure-mode
ontology, a typed relation graph, source documents for in-app viewing, chunk text,
embeddings, prebuilt FTS5 keyword indexes, and serialized usearch vector indexes.

Packs are built **offline** by `pipeline/` (never on user machines) and are **read-only
forever** at runtime — the app ATTACHes them with `mode=ro&immutable=1`. User data never
writes into a pack; upgrading a pack means replacing the file.

The DDL below mirrors [`pipeline/compendium_pack/writer.py`](../pipeline/compendium_pack/writer.py),
which is the single source of truth.

## File identification

| PRAGMA | Value | Purpose |
|---|---|---|
| `application_id` | `0x434D5044` (`CMPD`) | reject non-pack SQLite files cheaply |
| `user_version` | `1` | schema version gate; the app refuses packs newer than it understands |

## Tables

### `manifest` — key/value strings

Required keys (build fails without them):

| Key | Meaning |
|---|---|
| `schema_version` | pack schema version (`1`) |
| `pack_id`, `pack_version` | identity; version is `YYYY.MM.N` |
| `name`, `description` | display strings |
| `source_type` | processor that built it (`notebook`, `webdoc`, …) |
| `embedding_model` | `embed-v4.0` — **runtime queries must embed with the same model** |
| `embedding_dims` | `1024` — vector blob length is `dims × 4` bytes |
| `embedding_input_type` | `search_document` (runtime uses `search_query` — the required pairing) |
| `license_id`, `license_text` | license of the pack content |
| `attribution_html` | rendered in-app on source views and About — **the app refuses to load a pack without it** |
| `built_at`, `source_ref` | provenance (UTC ISO time; e.g. `<repo-url>@<git-sha>`) |

Additional informational keys: `usearch_version`, `counts` (JSON).

### Ontology: `stages`, `failure_modes`

- `stages(id, name, description, position)` — the subject's lifecycle stages. These are
  **pack data, not app code**: a future pack defines its own stages.
- `failure_modes(id, name, description, phrasings)` — the problem taxonomy;
  `phrasings` is a JSON array of example user wordings. Pre-embedded phrasings (below)
  power the zero-LLM symptom matcher (advisor stage S0).

### `techniques` — the recommendation targets

```
slug PK · card_key (INTEGER UNIQUE — usearch key) · title · one_liner ·
stage_id → stages · complexity (low|medium|high) · problem_solved · how_it_works ·
when_to_use (JSON[]) · tradeoffs (JSON[]) · key_dependencies (JSON[]) ·
keywords (JSON[]) · summary (embedding-ready ~150 words) ·
vendor_disclosure (nullable — rendered as a disclosure chip) · document_id → documents
```

### Graph: `technique_failure_modes`, `technique_relations`

Typed directed edges: `composes_with`, `alternative_to`, `prerequisite_of`, `refines`,
`evaluated_by`. The advisor's retrieval does 1-hop expansion along these edges, and the
synthesis prompt receives them so it can present alternatives/prerequisites honestly.

### `documents` — in-app source viewing

`content` is JSON, shaped by `kind`:

- `notebook` → `{"format": "nbformat-lite", "v": 1, "cells": [{"t": "md"|"code",
  "src": str, "outputs?": [{"mime", "data"}]}]}`. Outputs are whitelisted at build time
  (`text/plain` ≤ 2 KB, `text/html` ≤ 50 KB, `image/png` ≤ 200 KB base64,
  `application/x-traceback` ≤ 2 KB); the renderer must still sanitize `text/html`
  (DOMPurify) — defense in depth.
- `webdoc` → `{"format": "markdown", "v": 1, "text": str, "headings": [...]}` (Phase 7).

`source_url` links to the exact upstream source (pinned to the built git sha);
`license_note` is shown alongside the document.

### `chunks` — retrieval units

```
id PK (usearch key) · document_id · technique_slug (nullable) · heading_path ·
kind (markdown|code|mixed) · text (embedded text, contextual header included) ·
display_text (rendered text, no header) · token_count · location (JSON)
```

`location` maps a chunk back into its document for citation deep-linking:
`{"cells": [first, last]}` for notebooks, `{"anchor": "#..."}` for webdocs.

### Embeddings: `card_embeddings`, `chunk_embeddings`, `phrasing_embeddings`

Vectors are **float32 little-endian BLOBs, L2-normalized**, `embedding_dims` wide.
These are the **vectors of record**: the runtime uses them for exact cosine re-scoring
of fused candidates, and can rebuild a corrupt/version-mismatched usearch index from
them (self-healing) — which is why they ship alongside the indexes.

### `vector_indexes` — serialized usearch HNSW indexes

One row per tier (`cards`, `chunks`): the serialized index as a BLOB plus everything
needed to load or rebuild it (`usearch_version`, `metric=cos`, `quantization=f16`,
`dims`, HNSW params, `count`, measured `recall_at_10`, `sha256` of the blob).

Runtime contract: verify `sha256` → `Index.restore`/`load_from_buffer` (or extract to
the app cache and `view()` mmap) → if usearch version mismatch or hash/load failure,
**rebuild from the embedding tables instead of failing**. Keys are `card_key` /
`chunks.id`. The pipeline gates builds on `recall@10 ≥ recipe.index.recall_gate`
(default 0.98) measured against exact brute-force over the f32 vectors.

### FTS5: `chunks_fts`, `cards_fts`, `phrasings_fts`

Prebuilt and `optimize`d at build time (`porter unicode61` tokenizer).
`chunks_fts` is external-content over `chunks` (rowid = chunk id); `cards_fts` and
`phrasings_fts` are small self-contained tables. Runtime BM25 comes from these — no
index construction ever happens on user machines.

## Build pipeline

```
cd pipeline
python -m venv .venv && .venv/Scripts/pip install -r requirements.txt
.venv/Scripts/python -m compendium_pack build packs/rag-techniques --source <clone-path>
.venv/Scripts/python -m compendium_pack validate ../packs-out/rag-techniques.pack
```

- The build reads `COHERE_API_KEY_PRODUCTION` from the repo-root `.env`.
- Embeddings are cached in `pipeline/.cache/embed-cache.sqlite` by content hash —
  re-running a build with unchanged content costs **zero** API calls.
- Every build writes `sources.lock` (source ref + build time) into the pack directory
  and runs the full validator; recall gates fail the build loudly.
- **usearch version pinning**: `pipeline/requirements.txt` and the app's
  `Cargo.toml` must pin the identical usearch version — the serialized index format has
  no cross-version guarantee. The runtime treats a mismatch as a rebuild trigger, not
  an error, but shipping matched versions is the intended state.

## Authoring a new pack

A pack directory is `pipeline/packs/<pack-id>/` containing:

- `recipe.toml` — identity, `source_type`, license/attribution (**required — builds
  fail on empty license fields**), embedding + index params, processor options.
- `curation/` — the human-vetted knowledge layer (for notebook packs: technique cards +
  `ontology.json`). Curation is authoritative for problem/tradeoff/relation content;
  processors attach documents, chunks, and provenance.

The `source_type` selects a processor in `pipeline/compendium_pack/processors/`.
Processors own the raw-sources → documents+chunks logic for their type; adding a new
source type is a normal feature PR (see `CONTRIBUTING.md`, Phase 7). The notebook
processor is the reference implementation: section-tree chunking from markdown headers,
code cells kept intact (giant cells split at def/class/method boundaries), install/API-key
boilerplate excluded from chunks but kept in documents, contextual headers prepended to
every chunk's embedded text, and per-chunk cell ranges for citation deep-linking.

## Versioning & compatibility rules

- **Schema v1 is frozen once shipped.** Additive changes (new tables/columns/manifest
  keys) bump `user_version` and must keep the app able to read v1 packs; breaking
  changes require a major app release that migrates or re-ships packs.
- A pack is uniquely identified by `(pack_id, pack_version)`; the app's registry keeps
  the newest version of each `pack_id`.
- Embedding model changes are **breaking** for a pack (query/index must match): they
  require a full re-embed and a new pack version, never an in-place edit.
