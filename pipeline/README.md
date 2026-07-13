# Compendium pack pipeline

Offline build pipeline that turns curated sources into shipped knowledge packs.
See [docs/PACK_FORMAT.md](../docs/PACK_FORMAT.md) for the pack schema and authoring guide.

## Setup

```
python -m venv .venv
.venv/Scripts/pip install -r requirements.txt
```

Put your Cohere keys in the repo-root `.env` (see `.env.example`). Builds use
`COHERE_API_KEY_PRODUCTION`; the trial key is for runtime/dev testing only.

## Commands

```
# build a pack (embeddings are content-hash cached in .cache/)
.venv/Scripts/python -m compendium_pack build packs/rag-techniques --source <RAG_Techniques clone>

# validate any built pack
.venv/Scripts/python -m compendium_pack validate ../packs-out/rag-techniques.pack
```

## Layout

```
compendium_pack/
  recipe.py       recipe.toml loader (license fields are mandatory)
  builder.py      orchestration: process -> embed -> index -> write -> validate
  embedder.py     Cohere embed v4 with sha256 content cache + backoff
  indexer.py      usearch f16 HNSW build + recall@10 gate vs exact search
  writer.py       pack SQLite DDL (the schema's source of truth) + inserts
  validator.py    everything a shipped pack must pass
  processors/
    notebook.py   reference source-type processor (section-aware, code-intact)
packs/
  rag-techniques/ recipe.toml + curation/ (technique cards + ontology) + sources.lock
```

## Invariants worth knowing

- usearch is **pinned** here and in `app/src-tauri/Cargo.toml` in lockstep; the
  serialized index format has no cross-version guarantee.
- Vectors of record are the f32 BLOBs in the pack; usearch indexes are derived
  artifacts the runtime can rebuild from them.
- A build that loses recall (< recipe gate), drops license fields, or breaks FK/FTS
  integrity fails loudly — never ship a silently degraded pack.
