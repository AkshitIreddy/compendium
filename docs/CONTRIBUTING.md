# Contributing to Compendium

Compendium is deliberately **not** a sealed plugin system. Extending it — a new
knowledge pack, a new source type, a new advisor capability — is a normal feature PR
that may touch any part of the codebase when the work genuinely requires it. The
architecture optimizes for that: clear seams, low coupling, and this guide.

Ground rules:

- The app stays **free and non-commercial** (a pack license requires it — see
  [PACK_FORMAT.md](PACK_FORMAT.md)); every pack must carry license + attribution fields
  or it will not build and will not load.
- Conventional Commits, atomic commits, working states only.
- Quality gates are not optional: pack builds must pass the validator (including the
  recall@10 gate), `cargo test` and `npx vitest run` must stay green, and UI changes
  must keep the contrast matrix passing.

## The seams (where things plug in)

| You want to… | You touch… |
|---|---|
| Add a pack from an **existing** source type | `pipeline/packs/<id>/` only (recipe + curation) |
| Add a **new source type** | `pipeline/compendium_pack/processors/<type>.py` + registry; possibly `documents.content` shapes + a renderer |
| Render a new document kind in-app | `app/src/features/sources/` (renderer dispatch on `content.format`) |
| Change retrieval behavior | `app/src-tauri/src/engine/search.rs` (local) or `advisor/mod.rs` (pipeline stages) |
| Add advisor stages / models | `engine/advisor/` (stages are plain Rust in one state machine) |
| UI features | `app/src/features/<feature>/` on the token system (`design/tokens.css`) |

## Path A — new pack, existing source type

1. Create `pipeline/packs/<pack-id>/recipe.toml`. Copy an existing recipe; every field
   is documented by example. License fields (`license.id/text/attribution_html/
   document_note`) are **mandatory** — builds fail without them.
2. For notebook packs: put curation data in `curation/` — technique cards (one JSON per
   technique) + `ontology.json` (stages, failure modes with `example_phrasings`, typed
   relations). Curation is the vetted knowledge layer; write it with care, it is what
   the advisor reasons over. For webdocs packs: define the allowlist in the recipe
   (`url_prefixes` for hierarchical namespaces, `exact_slugs` for flat ones) and set
   `expected_pages` — the ±10% guardrail catches upstream reorganizations.
3. Build and validate:
   ```
   cd pipeline
   .venv/Scripts/python -m compendium_pack build packs/<pack-id> --source <path>
   ```
   The validator runs automatically; fix what it reports. Embeddings are cached by
   content hash, so iterating is cheap after the first run.
4. Drop the resulting `packs-out/<pack-id>.pack` next to the others — the app loads
   every `*.pack` in its packs directory. Add it to the bundle by rebuilding the
   installer (it ships everything in `packs-out/`).
5. PR checklist: recipe + curation committed; `sources.lock` updated; a sentence in the
   README's pack list; license provenance explained in the PR description.

## Path B — new source type (e.g. PDF papers)

The source type owns the "raw sources → vetted documents + chunks" logic. The notebook
processor is the reference implementation; the webdocs processor shows a second shape.

1. Add `pipeline/compendium_pack/processors/<type>.py` exposing
   `process(recipe, source_dir) -> ProcessedPack`. Reuse the `Chunk` / `ProcessedDoc` /
   `ProcessedPack` dataclasses. Design decisions that matter:
   - **Chunking must fit the medium** (PDFs: layout-aware parsing, section-aware
     chunks, table/figure handling — not generic text splitting).
   - Every chunk needs a `location` dict that lets the app deep-link a citation back
     into the document (`{"cells": [a, b]}`, `{"anchor": "#…"}`, `{"page": 7}`…).
   - Prepend a contextual header to each chunk's embedded text (title + section) —
     it measurably improves retrieval and every existing processor does it.
   - Keep documents faithful for the in-app source view; filter noise from *chunks*,
     not from documents.
2. Register it in `processors/__init__.py`.
3. If the new type needs a new document `content` shape, document it in
   [PACK_FORMAT.md](PACK_FORMAT.md) and add a renderer branch in
   `app/src/features/sources/SourcePanel.tsx` (dispatch on `content.format`). This is
   the expected kind of core change — make it, don't work around it.
4. If the pack has technique cards + ontology, the advisor uses them automatically
   (S0 matching, graph expansion). Card-less packs contribute evidence chunks only —
   also fully supported.
5. Add an integration test in `app/src-tauri/tests/` that loads a built pack of the new
   type and exercises search (see `docs_pack_test.rs`).

## Refreshing the framework-docs pack

Monthly, or after a significant LangChain/LangGraph/LangSmith release:

```
cd pipeline
.venv/Scripts/python -m compendium_pack build packs/framework-docs --source .cache/webdocs
```

- Content-hash caching means only changed pages re-embed.
- The build **fails loudly** when allowlisted pages 404 or the page count swings ±10% —
  that means the docs reorganized again (they have before: python.langchain.com →
  docs.langchain.com, the llms.txt truncation). Re-verify against the sitemap and the
  MIT `langchain-ai/docs` repo listing — never against llms.txt.
- Bump `pack.version` (`YYYY.MM.N`) in the recipe as part of the refresh PR.

## Working on the engine

- `cargo test` in `app/src-tauri` runs offline against the built packs (build
  `rag-techniques.pack` first). Live-API tests are `#[ignore]`d; run them explicitly
  with a trial key in `.env` (`cargo test -- --ignored`).
- The advisor pipeline (`engine/advisor/mod.rs`) is a fixed state machine — stages are
  plain functions, tiers are configuration. New capability = new stage function + a
  tier gate + a trace field. Keep the invariants: every Cohere failure degrades to a
  local advisory (never lose a turn), `json_schema` never combines with tools/documents
  (the client enforces this), and any LLM-judge stage needs a validation plan before
  its scores surface in the UI.
- usearch stays **pinned in lockstep** between `pipeline/requirements.txt` and
  `app/src-tauri/Cargo.toml`. If you bump it, bump both, rebuild every pack, and run
  the round-trip test (`tests/engine_test.rs`).

## Working on the UI

- Everything themable flows through `app/src/design/tokens.css` → the `@theme inline`
  mapping. Never hard-code a color; add a semantic token and it works in all themes.
- The contrast matrix (`design/contrast.test.ts`) parses `tokens.css` directly — new
  tokens used for text or affordances belong in its pair list. It fails the build below
  WCAG AA, including for every user-selectable accent hue.
- Interaction affordances follow the existing grammar: focus rings via
  `:focus-visible`, motion through `--dur-*`/`--motion-scale` tokens (respecting
  reduced-motion), meaning never conveyed by color alone.
