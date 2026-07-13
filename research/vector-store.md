# Local Vector Store for Compendium Packs — Evaluation & Recommendation

*Research date: 2026-07-13. Versions verified against crates.io / docs.rs / GitHub releases as of this date. Facts that could not be fully verified are flagged inline with confidence levels.*

**Decision context.** Indexes are prebuilt offline by the Python pipeline and shipped read-only inside a Windows installer. Runtime is Rust (Tauri 2 backend). Packs are single SQLite files (`<pack-id>.pack`, per `docs/PLAN.md` §4). Scale: 2–10k chunks now, possibly 50k–500k vectors later. Quality-first: exact re-scoring and generous candidate depths are in-budget. Only external API is Cohere (embed-v4.0 @ 1024-dim f32, pre-normalized at build time).

---

## TL;DR recommendation

**Use usearch (unum-cloud), pinned to the same version in the Python pipeline and the Rust crate.** Ship one HNSW index per pack **as a BLOB inside the pack's SQLite file** (single-file packaging preserved), extract it once to an app-cache file on first load, and **`view()` (mmap) it from the cache thereafter** — near-zero startup cost at any scale. Store the index in **f16** scalar kind (half the size, negligible recall loss) and keep the shipped **f32 BLOBs in SQLite as the vectors-of-record** for exact cosine re-scoring and for full index rebuild if the index blob is ever corrupt or version-incompatible. Query per-pack indexes independently and merge — cosine scores are directly comparable across packs because every pack uses the same embedding model and normalization. Hybrid retrieval stays exactly as planned: FTS5 BM25 + ANN candidates → RRF → exact f32 cosine re-score of the fused top ~50.

---

## 1. usearch (unum-cloud) — the winner

### Maturity & activity (2026)

- Latest release **v2.26.0, 2026-07-10** — three days before this research. Project is highly active: 207 releases, ~4.2k stars, 2,390 commits. ([GitHub](https://github.com/unum-cloud/usearch), [releases](https://github.com/unum-cloud/usearch/releases))
- The 2025–2026 release train shows sustained investment where it matters for us: v2.25.x (Apr–May 2025) was a memory-safety/concurrency hardening series (fixed heap-buffer-overflow in HNSW search, UB in graph refinement, stale Rust build paths); v2.26.0 added `stats()` to the Rust SDK, a Rust compact binding, and tombstone-reclamation fixes. Rust binding fixes appear repeatedly in release notes — the Rust SDK is maintained as a first-class citizen, not an afterthought.
- Rust crate `usearch` v2.26.0 on crates.io, versioned in lockstep with the core. Dependencies are minimal: `cxx` (+`cxx-build` at build time) wrapping the header-only C++ core, optional `numkong` SIMD kernels. ([docs.rs](https://docs.rs/crate/usearch/latest))
- Python binding `usearch` on PyPI, same versioning, wheels for Windows/Linux/macOS.

### Cross-binding index portability (the critical question)

**Verified: portable across bindings.** The serialized `.usearch` format is produced and consumed by the same C++ core in every binding; the project's documented, headline workflow is "reuse the same preconstructed index in various programming languages" — build in one language, `save()`, then `load()`/`view()` in another. Python `index.save(...)` → Rust `index.load(...)` / `index.view(...)` is the canonical example in the docs. ([USearch docs](https://unum-cloud.github.io/USearch/), [Rust SDK docs](https://unum-cloud.github.io/USearch/rust/index.html))

**Cross-version: no formal guarantee.** Neither the README nor the docs publish a file-format stability/versioning policy, and the format has changed across major eras historically. *(Confidence: high that no formal guarantee exists; this is the main engineering risk.)* **Mitigation (mandatory): pin the identical usearch version in `pipeline/pyproject.toml` and `Cargo.toml`, record `usearch_version` + index params in the pack manifest, and validate at load time; the rebuild-from-vectors fallback (§6.5) makes even a worst-case mismatch a self-healing event, not a support incident.** Version bumps become a deliberate, tested act: bump both sides together, rebuild pack indexes in CI.

### mmap "view" on Windows

**Verified: supported.** `view()` serves the index from a memory-mapped file without loading it into RAM. The C++ core (`include/usearch/index.hpp`) contains a dedicated Windows implementation of its memory-mapping layer (separate file + mapping handles, i.e. `CreateFileMapping`-style), alongside the POSIX path — Windows is not a second-class mmap platform here. *(Confidence: high — verified in source; not exercised by us yet, so validate in a spike.)* ([index.hpp](https://github.com/unum-cloud/USearch/blob/main/include/usearch/index.hpp))

### Buffer APIs — index as a BLOB inside pack SQLite

**Verified in the Rust API** ([docs.rs Index](https://docs.rs/usearch/latest/usearch/struct.Index.html)):

- `save_to_buffer(&mut [u8])` / `load_from_buffer(&[u8])` — serialize/deserialize to/from an in-memory byte slice. `serialized_length()` reports the needed size.
- `view_from_buffer(&[u8])` — **unsafe**: zero-copy view over a caller-owned buffer; the buffer must outlive the index (we'd hold the `Box<[u8]>` alongside the `Index` in the same struct).
- `filtered_search(query, count, predicate)` — search with a key-predicate callback.
- `change_expansion_search(n)` — tune ef_search at runtime without rebuild.

So the index genuinely can live as a BLOB in the pack's SQLite: read blob → `load_from_buffer` (owned copy) or `view_from_buffer` (zero-copy over the pinned blob bytes). Note SQLite blobs cannot themselves be mmap'd from disk — for true file-backed `view()` you need a real file, which motivates the extract-to-cache design in §6.

### Cosine on pre-normalized f32

Fully supported: `MetricKind::Cos` and `MetricKind::IP` (inner product). Since our vectors are L2-normalized at build time, IP ≡ cosine; either works. Scalar kinds include f64/f32/f16/bf16/i8/u8/b1 — the index can store **f16** internally (half the footprint) while we keep f32 in SQLite for exact re-scoring. Distance math is SIMD-accelerated (NumKong kernels, 1000+ kernels added in v2.25.0).

### HNSW parameters & expectations (10k–500k vectors)

usearch defaults and its own benchmark config: `connectivity` (M) = 16, `expansion_add` (efConstruction) = 128, `expansion_search` (efSearch) = 64 — benchmarked on 1M-vector Deep1B samples. ([BENCHMARKS.md](https://github.com/unum-cloud/usearch/blob/main/BENCHMARKS.md)) General HNSW tuning guidance (Milvus/OSC references) plus our quality-first posture:

| Scale | connectivity (M) | expansion_add | expansion_search | Expectation |
|---|---|---|---|---|
| ≤50k (today) | 16 | 256 | 128 | recall@10 ≥ ~0.99, well under 1 ms/query |
| 50k–500k (future) | 24–32 | 256–512 | 128–512 (runtime-tunable) | recall@10 ~0.98–0.99+, single-digit ms |

Build cost at these settings is irrelevant (offline, minutes even at 500k). Since `change_expansion_search` is runtime-adjustable, ship generous defaults and expose a "thorough" mode that raises it — we are not latency-constrained. **Measure recall against exact brute-force in the pipeline's validation step** (we have the f32 vectors; exact ground truth is one matrix multiply away) rather than trusting rules of thumb.

### Metadata filtering

usearch has no payload/metadata store — it maps a `u64` key → vector, and `filtered_search` filters by key predicate during traversal. That is the right shape for us: keys are SQLite rowids; anything richer (technique vs chunk tier, doc, stage) is a SQLite lookup. Practical strategy: **per-pack index makes pack filtering free; for tier filtering, either maintain two small indexes per pack (cards / chunks — recommended, they're different retrieval targets anyway) or use `filtered_search` with a rowid-range or bitset predicate. Everything else is post-hoc via SQL joins on the returned keys** — at our candidate depths (top-100), post-hoc filtering costs nothing.

### Binary size

Header-only C++ via `cxx`; no runtime DLLs. Expected addition to the Tauri binary: **roughly 1–3 MB** in release builds. *(Confidence: medium — estimate from the crate's shape; measure in the spike. Compare: LanceDB adds tens of MB and a giant dependency graph.)*

---

## 2. LanceDB embedded — capable, but wrong weight class

- Rust crate `lancedb` **v0.31.0** (2026), in-process, well-documented; OSS "handles millions of vectors on a single node." ([crates.io](https://crates.io/crates/lancedb), [FAQ](https://docs.lancedb.com/faq/faq-oss))
- Index types: IVF_PQ, IVF_SQ, IVF_RQ, and HNSW as sub-index within IVF partitions (IVF_HNSW_FLAT/SQ/PQ). In OSS you build indexes manually via `table.create_index()`. ([Vector Indexes](https://docs.lancedb.com/indexing/vector-index), [docs.rs](https://docs.rs/lancedb/latest/lancedb/index/vector/struct.IvfPqIndexBuilder.html))
- Python-write/Rust-read: both SDKs wrap the same Rust core and Lance columnar format, so cross-language datasets are compatible in principle (pin format versions). *(Confidence: medium — parity is real but format-version pinning across SDK releases is on you.)*

**Why not:** (a) **Dependency weight** — pulls Lance + Arrow + DataFusion + tokio; hundreds of transitive crates, materially larger binary and compile times for a Tauri app that needs exactly one operation: top-K cosine. (b) **Packaging mismatch** — a Lance dataset is a *directory* of fragment/manifest files, which breaks the clean "one pack = one SQLite file" model; we'd ship a directory tree per pack. (c) IVF-style indexes want training and larger corpora; at 2–10k vectors it's all overhead. (d) At 0.31.x it iterates fast with format churn — riskier to pin across a Python-write/Rust-read boundary than usearch's single C++ core. LanceDB becomes attractive only if packs someday carry millions of vectors *and* need columnar scans/SQL-ish filtering; we're nowhere near that.

## 3. sqlite-vec — not there yet for ANN; irrelevant for us even as brute force

- Status: **still pre-v1**. Latest stable **v0.1.9 (2026-03-31)**; v0.1.10-alpha.4 (2026-05-18) is developing **DiskANN** and a rescore index, IVF "experimental, not enabled." Core remains **brute-force** virtual tables. Maintained by Alex Garcia; runs everywhere including Windows. ([releases](https://github.com/asg017/sqlite-vec/releases), [state-of-SQLite-vector-search overview, Sep 2025](https://marcobambini.substack.com/p/the-state-of-vector-search-in-sqlite))
- Rust integration is clean if wanted: `cargo add sqlite-vec` statically links the C source; register via `sqlite3_auto_extension` with rusqlite `bundled`; zero-copy `Vec<f32>` via `zerocopy`. ([Rust docs](https://alexgarcia.xyz/sqlite-vec/rust.html))
- Shipping story is trivially good (indexes are just tables in the .pack), **but**: brute force is fine at 10k and unacceptable as the *strategy* for a store we chose specifically to scale to 500k; its ANN (DiskANN) is alpha-quality mid-2026. And crucially — **we don't need it even for brute force**: our pack schema already stores raw f32 BLOBs, and exact re-scoring over a few hundred candidates is a trivial loop in Rust. sqlite-vec would add a dependency to do something we already do better. Revisit only if its DiskANN ships stable and we want to consolidate everything into SQLite.
- (Aside: `sqlite-vector` from sqlite.ai is faster but license is "free for open-source projects" — a poor fit given our licensing constraints; also brute-force-centric.)

## 4. Other embedded options — brief

- **hnsw_rs v0.3.0** (jean-pierreBoth): pure-Rust HNSW, `hnswio` dump/reload, mmap of the data part (not the graph). Solid library, but **no Python binding** — the index would have to be built by a Rust CLI step inside the Python pipeline, and its dump format is crate-internal with no cross-version promise. More moving parts than usearch for less capability. ([docs.rs](https://docs.rs/hnsw_rs), [GitHub](https://github.com/jean-pierreBoth/hnswlib-rs))
- **instant-distance v0.6.1** (InstantDomainSearch): clean minimal HNSW with serde serialization and a Python binding — the right *shape*, but effectively dormant (0.6.1 is years old), no mmap view, no quantization, serde/bincode format fragility across versions. ([crates.io](https://crates.io/crates/instant-distance/0.6.1))
- **FAISS bindings**: the `faiss` Rust crate wraps the C API and requires shipping/building the FAISS dynamic library — painful on Windows/Tauri, huge footprint, binding maintenance lags upstream. Overkill and wrong deployment shape.
- **hnswlib (C++) via bindings**: the reference HNSW, but Rust bindings are thin/unofficial; usearch gives the same algorithm with an actually-maintained Rust SDK and cross-language format.

None beat usearch on the axis that decides this: **one maintained C++ core with first-class Python and Rust SDKs sharing one serialized format, plus buffer/mmap loading.**

## 5. Hybrid search architecture — sanity check: sound, two refinements

The planned pipeline is the textbook sqlite-vec/FTS5 hybrid pattern and is architecturally correct ([Alex Garcia's hybrid search writeup](https://alexgarcia.xyz/blog/2024/sqlite-vec-hybrid-search/index.html), [Simon Willison](https://simonwillison.net/2024/Oct/4/hybrid-full-text-search-and-vector-search-with-sqlite/)):

1. **BM25**: FTS5 (already prebuilt into the .pack as shadow tables, external-content mode) → top-K per pack, K ≈ 100.
2. **ANN**: usearch per pack → top-K, K ≈ 100. Quality-first: overshoot K; candidates are cheap.
3. **RRF fusion**: `score = Σ 1/(60 + rank_i)` over both lists. Rank-based, so it needs no score normalization between BM25 and cosine — exactly why it's the right fusion here. Optionally weight the vector list slightly higher for long problem descriptions and the BM25 list higher for short symptom queries.
4. **Exact re-score**: fetch f32 BLOBs for the fused top ~50 from SQLite, compute true cosine (pre-normalized ⇒ dot product), re-rank.

Refinements:
- **Exact re-scoring is what makes f16 index storage free**: the ANN stage only needs to get the right candidates *into* the pool; final ordering comes from f32 exact scores (and ultimately Cohere rerank). Recall lost to f16 quantization at candidate depth 100 is negligible.
- **Run the two retrievers truly in parallel** (usearch search is a CPU-bound in-memory call; FTS5 is a rusqlite query) and fuse per-pack *before* cross-pack merge only for BM25 — BM25 scores are corpus-dependent, but RRF works on ranks, so you can also fuse per-pack lists globally by rank. Simplest correct approach: produce one global ANN list (merge per-pack ANN by cosine, which *is* comparable across packs) and one global BM25 list (merge per-pack FTS5 by rank interleaving or by bm25 score within a pack), then RRF the two global lists. Then exact-re-score, then Cohere rerank the top ~30 for the dossier.

## 6. Recommended pack-integration design (usearch)

### 6.1 Pack schema addition

New table in the pack SQLite (spec for `docs/PACK_FORMAT.md`):

```sql
CREATE TABLE vector_indexes (
  name            TEXT PRIMARY KEY,   -- 'cards' | 'chunks'
  usearch_version TEXT NOT NULL,      -- e.g. '2.26.0' (must match Rust crate)
  metric          TEXT NOT NULL,      -- 'cos'
  scalar_kind     TEXT NOT NULL,      -- 'f16'
  dims            INTEGER NOT NULL,   -- 1024
  connectivity    INTEGER NOT NULL,
  expansion_add   INTEGER NOT NULL,
  count           INTEGER NOT NULL,   -- number of vectors
  sha256          TEXT NOT NULL,      -- hash of blob
  blob            BLOB NOT NULL       -- index.save_to_buffer() bytes
);
```

Two indexes per pack: `cards` (technique-card summaries, the recommendation targets) and `chunks` (section chunks, the evidence tier). Keys = SQLite rowids of the corresponding rows. Manifest gains `index_format = usearch/2` and repeats `usearch_version` for fast pre-flight checks.

### 6.2 Python pipeline build step

After embedding + L2-normalization, per tier:

```python
from usearch.index import Index
idx = Index(ndim=1024, metric='cos', dtype='f16',
            connectivity=16, expansion_add=256)   # bump to 24/512 for big packs
idx.add(rowids, vectors)                          # batch add, f32 in; stored f16
buf = idx.save_to_buffer()                        # bytes → sqlite blob + sha256
```

`modus-pack validate` additions: (a) load the blob back and assert `count`/`dims`; (b) **recall check** — exact brute-force top-10 over the f32 matrix for ~200 sampled queries vs index results at the shipped `expansion_search`; fail the build below 0.98 recall@10. Pin `usearch==2.26.0` in the pipeline lockfile, same as `Cargo.toml`.

### 6.3 Shipping: blob-in-SQLite, extract-to-cache for mmap

Single-file `.pack` is preserved — the index rides inside the SQLite. At runtime:

- **First load of a pack version**: read blob, verify sha256, write to `%APPDATA%/modus/index-cache/<pack_id>-<pack_version>-<name>.usearch`, then `index.view(path)` (mmap). One-time cost ≈ one blob read + one file write (tens of ms at today's sizes).
- **Subsequent startups**: stat + hash-check the cache file (hash from the pack manifest), `view()` directly. mmap is lazy — **queryable in single-digit ms regardless of index size**, comfortably under the 100 ms target even at 500k vectors.
- **Small-pack fast path (optional)**: if blob < ~32 MB, skip the cache and `load_from_buffer` straight into RAM — today's packs (2–4k vectors ≈ 8–16 MB f32 in SQLite; f16 index blob ≈ 4–10 MB incl. graph) load in a few ms and search fastest fully in-RAM.
- Cache eviction: on pack upgrade the filename changes with `pack_version`; sweep stale entries at startup.

Why not a sidecar file next to the `.pack`? It works (manifest carries filename + sha256) and skips the extraction step, but it breaks single-file packaging, doubles the installer's file-integrity surface, and the cache design gets you the same mmap benefit. Choose sidecar only if pack indexes someday exceed hundreds of MB and duplicating bytes (blob + cache) becomes objectionable; the schema above ports to a sidecar trivially (blob column → relative path).

### 6.4 Multi-pack querying

**Per-pack indexes, merged at query time** — not one combined index:

- Pack lifecycle stays clean: add/remove/upgrade a pack = attach/detach one file; no global reindex, no cross-pack key collisions (keys are per-pack rowids; the engine tags results with `pack_id`).
- Cosine scores are directly comparable across packs (same model, same normalization, same input_type), so merging per-pack top-K lists by score is mathematically sound.
- At 2–10 packs the fan-out is trivial; searches run in parallel (usearch searches are independent, immutable, thread-safe reads).
- A combined index would only pay off past ~10⁶ total vectors across dozens of packs — and even then, HNSW query time is logarithmic, so K parallel searches over K small graphs ≈ one search over the union.

### 6.5 Corruption / mismatch fallback (self-healing)

Load-time checks, in order: manifest `usearch_version` == compiled crate version → blob sha256 → `view()`/`load()` success → `count` matches. On any failure:

1. Log + telemetry-free user notice ("rebuilding search index for pack X…").
2. **Rebuild from the vectors of record**: stream f32 BLOBs from the pack's `embeddings` table, `reserve(count)` + batch `add()` with the manifest's build params, `save_to_buffer` → write to cache (never into the read-only pack). 10k vectors: ~1–2 s. 500k: a few minutes, once, with a progress event to the UI.
3. If even the vectors table is unreadable, the pack is genuinely corrupt → prompt reinstall of the pack; BM25-only degraded mode in the interim (FTS5 shadow tables are independent of the vector index).

This fallback is also the escape hatch for the cross-version format risk in §1: a usearch upgrade that breaks old blobs degrades to a one-time local rebuild, never a broken app.

### 6.6 Spike checklist (before committing code)

1. Python 2.26.0 `save_to_buffer` → Rust 2.26.0 `load_from_buffer` and `view()` round-trip on Windows, f16 + cos, 10k×1024.
2. Measure: cold extract+view time, warm view time, search latency at expansion_search ∈ {64,128,256}, recall vs exact.
3. Release-build binary size delta from the usearch crate.
4. `filtered_search` predicate overhead (only if we opt for one index per pack instead of two).

---

## Sources

- usearch GitHub (README, releases, BENCHMARKS.md, index.hpp): <https://github.com/unum-cloud/usearch> · <https://github.com/unum-cloud/usearch/releases> · <https://github.com/unum-cloud/usearch/blob/main/BENCHMARKS.md> · <https://github.com/unum-cloud/USearch/blob/main/include/usearch/index.hpp>
- usearch docs & Rust SDK: <https://unum-cloud.github.io/USearch/> · <https://unum-cloud.github.io/USearch/rust/index.html> · <https://docs.rs/usearch/latest/usearch/struct.Index.html> · <https://docs.rs/crate/usearch/latest>
- LanceDB: <https://crates.io/crates/lancedb> · <https://docs.rs/lancedb/latest/lancedb/> · <https://docs.lancedb.com/faq/faq-oss> · <https://docs.lancedb.com/indexing/vector-index>
- sqlite-vec: <https://github.com/asg017/sqlite-vec> · <https://github.com/asg017/sqlite-vec/releases> · <https://alexgarcia.xyz/sqlite-vec/rust.html> · <https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html>
- State of vector search in SQLite (Sep 2025): <https://marcobambini.substack.com/p/the-state-of-vector-search-in-sqlite>
- hnsw_rs / hnswlib-rs: <https://docs.rs/hnsw_rs> · <https://github.com/jean-pierreBoth/hnswlib-rs>
- instant-distance: <https://crates.io/crates/instant-distance/0.6.1>
- HNSW parameter tuning: <https://milvus.io/ai-quick-reference/what-are-the-key-configuration-parameters-for-an-hnsw-index-such-as-m-and-efconstructionefsearch-and-how-does-each-influence-the-tradeoff-between-index-size-build-time-query-speed-and-recall> · <https://opensourceconnections.com/blog/2025/02/27/vector-search-navigating-recall-and-performance/>
- Hybrid FTS5 + vector + RRF: <https://alexgarcia.xyz/blog/2024/sqlite-vec-hybrid-search/index.html> · <https://simonwillison.net/2024/Oct/4/hybrid-full-text-search-and-vector-search-with-sqlite/>
