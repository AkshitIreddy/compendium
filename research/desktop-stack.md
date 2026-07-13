# Desktop Stack Research: Windows RAG Knowledge-Base App

**Date:** 2026-07-13
**Context:** Windows desktop app shipping a prebuilt vector index (2k–10k chunks, 1024/1536-dim f32 ≈ 10–60 MB) + metadata in the installer. Runtime: hybrid local search (dense cosine + BM25), SQLite persistence, one Cohere API key stored securely, HTTPS calls to Cohere. Extensible via additional bundled "knowledge packs". Priorities: fast cold start, small footprint, snappy search, premium web-tech UI.

---

## 1. Framework: Tauri 2.x on Windows (vs Electron, vs Flutter)

### Tauri 2.x status (July 2026)

- **Maturity:** Tauri 2.0 went stable in October 2024; the 2.x line is now at **v2.11.x** (2.11.5 released early July 2026) with a steady patch cadence — recent patches are small fixes (dependency pins, drag-region behavior, async protocol-handler startup optimization). The 2.x line is production-stable and widely deployed. ([GitHub releases](https://github.com/tauri-apps/tauri/releases))
- **Bundle size:** A minimal Tauri app is ~3 MB; realistic apps ship 5–15 MB installers before payload data. Electron ships Chromium with every app: ~96 MB compressed on Windows, commonly >150–200 MB installed. ([Tauri vs Electron 2026](https://tech-insider.org/tauri-vs-electron-2026/), [PkgPulse guide](https://www.pkgpulse.com/guides/electron-vs-tauri-2026))
- **Memory:** 2026 benchmarks consistently report Tauri at roughly **half the RAM of an equivalent Electron app** (no second Chromium tree, no Node main process); idle figures cluster around 45–95 MB on Windows depending on UI complexity. ([PkgPulse](https://www.pkgpulse.com/guides/electron-vs-tauri-2026), [Tauri vs Flutter benchmarks](https://johal.in/comparison-tauri-20-vs-flutter-40-desktop-cross-platform-comparison/))
- **Cold start & WebView2:** Tauri on Windows renders through **WebView2** (Edge/Chromium, evergreen). WebView2 is **preinstalled on Windows 11** and near-universally present on updated Windows 10; the Tauri NSIS installer detects a missing runtime and can download/embed the Evergreen Bootstrapper. First paint is typically **<200 ms–~500 ms** on modern hardware; the dominant cold-start cost is WebView2 process spawn (~150–300 ms), not your binary. If you must embed a *fixed* WebView2 runtime, the installer grows by ~180 MB — do not do this; use the bootstrapper (`webviewInstallMode: downloadBootstrapper`, adds ~1.8 MB). ([Tauri webview versions](https://v2.tauri.app/reference/webview-versions/), [Windows installer docs](https://v2.tauri.app/distribute/windows-installer/))
- **Installers:** First-class **NSIS (.exe)** and **MSI (WiX)** bundlers. NSIS is the recommended default: supports per-user install (no UAC), `installMode: currentUser|perMachine|both`, custom compression, and is the format the updater re-uses. MSI is there for enterprise/GPO deployment. ([Windows installer docs](https://v2.tauri.app/distribute/windows-installer/))
- **Updater:** `tauri-plugin-updater` is stable and production-ready for NSIS and MSI. Signed updates (minisign keys) with a static JSON or dynamic endpoint. Known caveats: NSIS `installMode: "both"`/perMachine requires elevation during update (use **currentUser** install to avoid); occasional CLI/NSIS-component regressions get reported and fixed quickly (e.g., a tauri-cli 2.10 NSIS issue in Feb 2026). ([Updater plugin](https://v2.tauri.app/plugin/updater/), [issue #7184](https://github.com/tauri-apps/tauri/issues/7184))
- **Windows pain points (known, manageable):**
  - WebView2 absent on unpatched/enterprise-locked Windows 10 → bootstrapper flow adds a one-time download on first install.
  - Renderer differences vs Chrome-on-Electron are effectively nil (same Chromium), but WebView2 version varies by machine (evergreen), so pin your CSS/JS to broadly supported features.
  - Antivirus/SmartScreen: unsigned NSIS exes get flagged; **code-sign** (OV/EV cert) is effectively mandatory for a premium feel.
  - Elevated-updater edge cases as above; occasional reports of AV false-positives on `nsis_tauri_utils.dll` ([issue #14882](https://github.com/tauri-apps/tauri/issues/14882)).

### Comparison against OUR constraints

| Constraint | Tauri 2.x | Electron | Flutter (Windows) |
|---|---|---|---|
| Prebuilt-index shipping | `bundle.resources` first-class; NSIS compression control | Fine (extraResources) but installer already huge | Assets bundle fine |
| Local similarity search | In-process Rust — ideal (SIMD, rusqlite, zero IPC copies of vectors) | Native Node addon or WASM; N-API friction, larger memory | Dart FFI to C/Rust; workable but more plumbing |
| Cold start | <0.5 s typical (WebView2 spawn dominated) | 1–2 s+ (Chromium boot) | Fastest raw start (~200 ms) but see below |
| Binary size | 5–15 MB + data | 90–200 MB + data | ~25 MB+ + data |
| Premium web-tech UI | Full HTML/CSS/JS (React/Svelte/Tailwind) on Chromium-class engine | Same, at 10x the weight | **Not web-tech** — Flutter widgets; WinUI 3 likewise rules out web UI |
| Memory | ~45–95 MB idle | ~150–300 MB | ~90–280 MB (Skia + Dart VM) |

([Fyrosoft 2026 comparison](https://fyrosofttech.com/blog/cross-platform-desktop-apps-2026/), [Tauri 2.0 vs Flutter 4.0 benchmarks](https://johal.in/comparison-tauri-20-vs-flutter-40-desktop-cross-platform-comparison/), [PkgPulse frameworks guide](https://www.pkgpulse.com/guides/best-desktop-app-frameworks-2026))

**Verdict:** Electron fails the size/memory bar for no benefit (same web UI, worse everything else). Flutter starts fast but ships a heavier runtime, uses more RAM, and forfeits the web-tech UI requirement (as would WinUI 3, which also drops cross-platform optionality and has a weaker ecosystem for "premium" UI polish). **Tauri 2.x is the only option that satisfies all constraints simultaneously**, and the Rust backend is precisely where the vector search wants to live.

---

## 2. Local vector search in Rust at ≤10k vectors

### The math first: brute force is simply correct at this scale

Full exact scan of 10,000 × 1536-dim f32 vectors = 61 MB of reads and ~31 MFLOPs (fused mul-add) per query. This is **memory-bandwidth bound**: at a conservative 10–20 GB/s effective single-thread bandwidth, a full scan is **~3–6 ms single-threaded; ~1–2 ms with SIMD + a few threads (rayon)**. Pre-normalize embeddings at build time so cosine reduces to a dot product. Published sqlite-vec numbers corroborate the order of magnitude (brute force at 45k vectors ≈ 27 QPS *with SQL overhead*; raw in-memory scans are far faster). At ≤10k, **exact search with 100% recall at interactive latency** — ANN indexes solve a problem you do not have. ([State of Vector Search in SQLite](https://marcobambini.substack.com/p/the-state-of-vector-search-in-sqlite))

### Option comparison

| Option | Latency @10k×1536 | Binary size impact | Maturity / Windows | Shipping a prebuilt index |
|---|---|---|---|---|
| **(a) In-memory brute force + SIMD (simsimd or hand-rolled)** | ~1–5 ms exact | Negligible (simsimd is a small C lib; hand-rolled = zero) | simsimd: mature, 350+ kernels, AVX2/AVX-512, C core builds fine on MSVC; hand-rolled f32 dot with `f32::mul_add` chunks + rayon is ~40 lines | Trivial: ship raw f32 little-endian blob (or Arrow/flat file), `mmap` or read into `Vec<f32>` at startup (61 MB reads in ~50–200 ms from SSD) |
| **(b) sqlite-vec in rusqlite** | ~5–40 ms (SQL + vtab overhead, still fine) | Small (~few hundred KB); statically linked via `cc`, official Rust crate, works with rusqlite `bundled` via `sqlite3_auto_extension` | **Pre-v1: v0.1.10-alpha.4**, brute-force only, actively maintained but alpha-labeled with occasional memory bugs caught by fuzzing | Excellent: the vec0 virtual table lives *inside the shipped .db* — the index IS the database file |
| **(c) usearch Rust crate** | <1 ms (HNSW, approximate) | Moderate (C++ core, adds C++ runtime linkage on MSVC) | Mature (v2.x), cross-platform incl. Windows; single-file index with `view()` = mmap serving | Good: serialize index at build, `view()` mmaps at runtime — but HNSW adds recall uncertainty and build complexity for zero speed benefit at 10k |
| **(d) LanceDB embedded** | ~1–10 ms | **Heavy**: pulls Arrow, DataFusion, object-store, tokio — tens of MB of binary and long compile times | Mature and moving fast (0.31+), Windows OK | Good format story (Lance columnar, versioned), but it is a full analytical engine — massive overkill |

([sqlite-vec releases](https://github.com/asg017/sqlite-vec/releases), [sqlite-vec Rust docs](https://alexgarcia.xyz/sqlite-vec/rust.html), [SimSIMD](https://github.com/ashvardanian/SimSIMD), [usearch](https://crates.io/crates/usearch), [lancedb crate](https://crates.io/crates/lancedb))

### Recommendation for #2

**Primary: (a) in-memory brute-force with SIMD.** Store embeddings in the pack's SQLite file as BLOBs (one row per chunk, `zerocopy`/`bytemuck` to reinterpret as `&[f32]`), load once into a contiguous `Vec<f32>` (or an `Arc<[f32]>`) at startup on a background thread, and scan with simsimd (or a hand-rolled `mul_add` kernel + rayon if you want zero extra deps). This is the smallest, fastest, most debuggable option, has no index-format versioning risk, and "load-at-start" of ≤60 MB costs well under typical cold-start budgets (and can overlap WebView2 spawn). mmap buys nothing at this size and complicates Windows file-locking during updates.

**Avoid** usearch/LanceDB at this scale; **sqlite-vec is the fallback** if you later want vectors queried purely in SQL, accepting its pre-v1 status.

---

## 3. Hybrid search: FTS5 BM25 + RRF

- **rusqlite `bundled` includes FTS5 on Windows — confirmed.** The `libsqlite3-sys` bundled build compiles SQLite with `-DSQLITE_ENABLE_FTS5` (also FTS3, JSON1, R*Tree) and `-DSQLITE_USE_URI`, so a Tauri app using `rusqlite = { features = ["bundled"] }` gets FTS5 with zero linking hassle on MSVC — that's the exact scenario the bundled feature exists for. ([libsqlite3-sys build.rs](https://github.com/rusqlite/rusqlite/blob/master/libsqlite3-sys/build.rs), [crates.io](https://crates.io/crates/libsqlite3-sys))
- **Prebuilt FTS index in a shipped .db — yes, unconditionally.** An FTS5 index is nothing but ordinary shadow tables (`%_data`, `%_idx`, …) inside the database file; build it on your build machine, ship the .db, open read-only, and `MATCH ... ORDER BY bm25(...)` works immediately. Use `content=` external-content mode against your chunks table to avoid storing text twice, and run `INSERT INTO fts(fts) VALUES('optimize')` at pack-build time to merge b-trees for peak query speed. BM25 ranking is built into FTS5 (`bm25()` / default `rank`). ([SQLite FTS5 docs](https://www.sqlite.org/fts5.html))
- **Fusion — standard practice is Reciprocal Rank Fusion (RRF):** take top-K (e.g., 50) from dense and from BM25, score each doc `Σ 1/(k + rank_i)` with **k = 60** (the constant from Cormack/Clarke/Büttcher and what Elasticsearch/Azure AI Search/Weaviate all default to), sum across the two lists, re-sort. It needs no score normalization, which is exactly why it's the default for fusing incomparable BM25 and cosine scores. Implement it in ~15 lines of Rust; optionally follow with Cohere Rerank over the fused top-20 since you already call Cohere.

---

## 4. Secure storage of one API key on Windows

| Approach | Assessment |
|---|---|
| **keyring crate (→ Windows Credential Manager)** | **Recommended.** Mature (now v4.x, actively maintained, 96%+ coverage), one cross-platform API, `windows-native` store uses the Win32 credential API. Secrets are encrypted by Windows under the user's logon session — the same guarantee git-credential-manager relies on. |
| tauri-plugin-stronghold | **Do not use.** Officially discouraged: Tauri docs state the plugin will be **deprecated and removed in v3**. It's also the wrong shape — a password-unlocked vault with key derivation (argon2 salt file) for one API key, adding a UX obligation (vault password) or a stashed-password anti-pattern. ([Tauri Stronghold docs](https://v2.tauri.app/plugin/stronghold/), [discussion #7846](https://github.com/orgs/tauri-apps/discussions/7846)) |
| Raw DPAPI (`CryptProtectData`) | Fine security-wise (same underlying protection class), but you then own ciphertext storage/rotation yourself and lose cross-platform portability. Only worth it if you need blobs >2.5 KB. |

**Failure modes & limits to handle:**
- **Size limit:** `CRED_MAX_CREDENTIAL_BLOB_SIZE` = **2560 bytes** (5×512) — a Cohere key (~40–100 chars) fits with two orders of magnitude to spare. ([Microsoft CREDENTIALA docs](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentiala), [jaraco/keyring #540](https://github.com/jaraco/keyring/issues/540))
- Credential Manager is **per-Windows-user**; roaming profiles/domain policies can disable it → catch `keyring::Error` and fall back to prompting per-session (never to plaintext on disk).
- Entries survive app uninstall — delete the credential in an uninstall hook or on explicit sign-out.
- Users can see/delete the entry in the Credential Manager control panel; treat "not found" as first-run.

Wire it as a pair of Tauri commands (`set_api_key`, `has_api_key`) — the key never crosses into the WebView; the Rust side injects it into HTTPS calls to Cohere (use `reqwest` with rustls).

---

## 5. Data arrangement: read-only packs + writable user DB

**Recommended arrangement — rusqlite inside Tauri commands, one writable DB, packs ATTACHed read-only:**

```
<install dir>/resources/packs/core.db        ← read-only pack (chunks, metadata, FTS5 tables, embedding BLOBs)
<install dir>/resources/packs/<extra>.db     ← additional knowledge packs, same schema
%APPDATA%/<bundle-id>/app.db                 ← writable: chat/query history, settings (WAL mode)
```

- **Why rusqlite in commands, not tauri-plugin-sql:** the plugin (sqlx-based) exposes SQL to the *frontend*, which is the wrong trust boundary for a search engine — you want ranking, RRF, and vector scans in Rust, returning typed results over IPC. rusqlite gives synchronous, zero-async-overhead access, `bundled` FTS5, extension registration, and full control of open flags. Desktop-only apps are exactly where rusqlite is the community recommendation; the plugin's value (mobile support, frontend-driven SQL, sqlx migrations) buys you nothing here. ([Tauri SQL plugin](https://v2.tauri.app/plugin/sql/), [Tauri + SQLite writeup](https://dev.to/randomengy/tauri-sqlite-p3o))
- **tauri-plugin-store:** fine for trivial UI prefs (window size, theme) as a JSON file, but since you already have a writable SQLite DB, keep settings in a `settings` table — one persistence story, transactional, queryable. Don't add the store plugin just for this. ([Store plugin](https://v2.tauri.app/plugin/store/))
- **Declaring bundled resources:** in `tauri.conf.json`: `"bundle": { "resources": { "packs/": "packs/" } }` (map form controls destination). Resolve at runtime with `app.path().resolve("packs/core.db", BaseDirectory::Resource)` — never hardcode paths; NSIS/MSI place resources next to the exe under `resources/` preserving structure. ([Embedding additional files](https://v2.tauri.app/develop/resources/))
- **ATTACH multiple read-only databases — yes, confirmed.** SQLite supports up to 10 attached DBs by default (raisable to 125 via `SQLITE_LIMIT_ATTACHED`). Because rusqlite's bundled build defines `SQLITE_USE_URI`, you can attach read-only with URI filenames:
  ```sql
  ATTACH DATABASE 'file:C:/.../resources/packs/core.db?mode=ro&immutable=1' AS core;
  ```
  `mode=ro` enforces read-only; `immutable=1` additionally tells SQLite the file cannot change (true for files under Program Files), which skips locking/journal checks — faster and avoids any write attempt to a read-only install dir. Cross-pack queries and `UNION ALL` over identical schemas then work in one connection alongside the writable main DB. New knowledge packs = drop in another .db + a row in an installed-packs registry table.

**Open-flags detail:** open the user DB normally (`WAL`), and if you open packs as standalone connections instead of ATTACH, pass `OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`. Keep one long-lived connection (or a small pool) owned by a `tauri::State` struct behind a `Mutex`.

---

## 6. Gotchas: bundling multi-MB binary resources in Tauri installers

1. **NSIS compression vs float data:** NSIS default LZMA compresses well generally, but **f32 embedding blobs are high-entropy and barely compress** (expect ~5–15%). Compression *time* on a 60 MB payload is noticeable at build; `bundle > windows > nsis > compression` accepts `lzma` (default), `zlib`, `bzip2`, `none`. Keep LZMA (your text/metadata compresses well) unless CI build time hurts. ([Tauri config reference](https://v2.tauri.app/reference/config/), [issue #7685](https://github.com/tauri-apps/tauri/issues/7685))
2. **Historical slow-install bug with many resources** (NSIS extraction) was fixed by precomputing directory structure and estimated size at build time (merged in tauri-bundler; [PR #8233](https://github.com/tauri-apps/tauri/pull/8233)) — ship few large files (one .db per pack), not thousands of small ones, and you're on the happy path anyway.
3. **Resource path resolution:** always `BaseDirectory::Resource` / `PathResolver` — the layout differs between `tauri dev` (target dir) and installed app, and between NSIS per-user (`%LOCALAPPDATA%\Programs\<app>`) and per-machine (`Program Files`). Hardcoded relative paths break after install.
4. **Read-only install dir:** with per-machine installs, `resources/` is not writable by the app; with per-user NSIS installs it technically is, but treat packs as immutable regardless (hence `immutable=1`). Anything writable goes to `app_data_dir()`.
5. **Updater bandwidth:** the updater downloads the **entire new installer** — your 60 MB payload rides along on every app update (no delta updates in the stock updater). If packs change rarely, consider decoupling: keep the core pack in the installer but fetch/update additional packs into `app_data_dir()` via your own signed download flow.
6. **MSI quirks:** WiX is stricter (upgrade codes, no `both` install mode granularity); the updater's NSIS path is the better-trodden one. Prefer NSIS `currentUser` unless enterprise MSI is a hard requirement.
7. **Sign everything:** the exe, the installer, and (if AV flags it) be ready for `nsis_tauri_utils.dll` false positives ([issue #14882](https://github.com/tauri-apps/tauri/issues/14882)). An EV certificate also tames SmartScreen for a "premium" first-run.

---

## Definitive stack recommendation

- **Framework:** Tauri 2.11.x, NSIS installer, `installMode: currentUser`, WebView2 `downloadBootstrapper`, `tauri-plugin-updater`, code-signed.
- **Vector search:** exact brute-force cosine in Rust — embeddings pre-normalized at pack-build time, stored as BLOBs in the pack .db, loaded at startup into contiguous memory, scanned with simsimd (or hand-rolled `mul_add` + rayon). Expected query cost ~1–5 ms; no ANN library.
- **Keyword search:** SQLite FTS5 (rusqlite `bundled`), FTS index prebuilt and `optimize`d inside each shipped pack .db, BM25 via `bm25()`.
- **Fusion:** RRF with k=60 over top-50 dense + top-50 BM25; optional Cohere Rerank on the fused top-20.
- **Persistence:** rusqlite inside Tauri commands; writable `app.db` in `app_data_dir()` (WAL) for history/settings; pack DBs bundled under `resources/packs/`, ATTACHed with `file:...?mode=ro&immutable=1`. No tauri-plugin-sql, no tauri-plugin-store, no Stronghold.
- **Secrets:** `keyring` crate v4 (`windows-native`) → Windows Credential Manager; 2560-byte blob limit is ample for one Cohere key; graceful fallback to session-prompt on store failure; key never enters the WebView.

### Sources

- https://github.com/tauri-apps/tauri/releases
- https://v2.tauri.app/reference/webview-versions/
- https://v2.tauri.app/distribute/windows-installer/
- https://v2.tauri.app/plugin/updater/
- https://v2.tauri.app/develop/resources/
- https://v2.tauri.app/reference/config/
- https://v2.tauri.app/plugin/stronghold/
- https://v2.tauri.app/plugin/sql/
- https://v2.tauri.app/plugin/store/
- https://www.pkgpulse.com/guides/electron-vs-tauri-2026
- https://tech-insider.org/tauri-vs-electron-2026/
- https://fyrosofttech.com/blog/cross-platform-desktop-apps-2026/
- https://johal.in/comparison-tauri-20-vs-flutter-40-desktop-cross-platform-comparison/
- https://www.pkgpulse.com/guides/best-desktop-app-frameworks-2026
- https://marcobambini.substack.com/p/the-state-of-vector-search-in-sqlite
- https://github.com/asg017/sqlite-vec/releases
- https://alexgarcia.xyz/sqlite-vec/rust.html
- https://github.com/ashvardanian/SimSIMD
- https://crates.io/crates/usearch
- https://crates.io/crates/lancedb
- https://www.sqlite.org/fts5.html
- https://github.com/rusqlite/rusqlite/blob/master/libsqlite3-sys/build.rs
- https://crates.io/crates/libsqlite3-sys
- https://docs.rs/keyring/latest/keyring/
- https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentiala
- https://github.com/jaraco/keyring/issues/540
- https://github.com/orgs/tauri-apps/discussions/7846
- https://dev.to/randomengy/tauri-sqlite-p3o
- https://github.com/tauri-apps/tauri/pull/8233
- https://github.com/tauri-apps/tauri/issues/7685
- https://github.com/tauri-apps/tauri/issues/7184
- https://github.com/tauri-apps/tauri/issues/14882
