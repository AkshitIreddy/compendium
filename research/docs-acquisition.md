# Acquiring LangChain & LangGraph docs for an offline RAG knowledge pack

Research date: 2026-07-13. All URLs below were fetched and verified live on this date.

## 1. Current canonical docs layout (verified)

The docs have consolidated. As of July 2026 there is **one unified docs site** for the whole ecosystem:

| Property | Site | Status (verified) |
|---|---|---|
| **Unified docs hub** | https://docs.langchain.com | Canonical. Mintlify-built. Covers LangChain, LangGraph, Deep Agents, integrations, LangSmith. |
| LangChain (Python) guides | https://docs.langchain.com/oss/python/langchain/* | Current home |
| LangGraph (Python) guides | https://docs.langchain.com/oss/python/langgraph/* | Current home |
| Integrations (Python) | https://docs.langchain.com/oss/python/integrations/* | 23 pages (consolidated category/provider pages) |
| API reference | https://reference.langchain.com/python/ | Generated reference (separate host) |
| python.langchain.com | — | **308 Permanent Redirect** to docs.langchain.com (verified: `/llms.txt` redirects with `HTTP/1.1 308`) |
| langchain-ai.github.io/langgraph | — | Stub. Its `llms.txt` (1,962 bytes) says "LangGraph documentation has moved to docs.langchain.com" and lists redirect targets under `/oss/python/langgraph/` |
| api.python.langchain.com | — | Legacy (<v0.3) API reference only |

Docs source repo: **https://github.com/langchain-ai/docs** ("Unified LangChain documentation"). The old in-repo docs are gone: `langchain-ai/langchain/docs` returns 404 on the GitHub contents API, and `langchain-ai/langgraph/docs` now contains only redirect machinery (`generate_redirects.py`, `redirects.json`, a pointer `llms.txt`).

## 2. llms.txt / llms-full.txt (verified byte sizes)

| URL | Result | Size | Notes |
|---|---|---|---|
| https://docs.langchain.com/llms.txt | **200 OK** | 100 KB, 958 links | Every link is a per-page `.md` URL (e.g. `.../langsmith/evaluate-rag-tutorial.md`). **Critical gap: it contains ZERO `/oss/` links** — it indexes LangSmith + platform API reference only, not LangChain/LangGraph OSS docs. Do not use it as the discovery index. |
| https://docs.langchain.com/llms-full.txt | **200 OK** | **13.58 MB**, 1,491 pages | Full site flattened to one markdown file. Pages delimited by `# <Title>` followed by `Source: <URL>` lines (1,491 `Source:` markers). **Does include** OSS content (`Source: https://docs.langchain.com/oss/javascript/langchain/retrieval`, `# Graph API overview`, `# Agentic RAG`, integration pages). Includes both Python and JavaScript variants plus all LangSmith content. Headings and code blocks preserved; Mintlify MDX components appear as literal tags (`<Note>`, `<Callout>`). |
| https://python.langchain.com/llms.txt | 308 → docs.langchain.com/llms.txt | — | Legacy host redirects |
| https://langchain-ai.github.io/langgraph/llms.txt | 200 OK | 1,962 B | Pointer file only: a curated list of ~15 links into `docs.langchain.com/oss/python/langgraph/*` plus `reference.langchain.com/python/langgraph/` |
| https://langchain-ai.github.io/langgraph/llms-full.txt | **404** | — | Removed |
| https://reference.langchain.com/python/llms.txt | 503 on probe | — | Not reliably available |

Both `llms.txt` and `llms-full.txt` carried `Last-Modified: Mon, 13 Jul 2026 17:0x GMT` — regenerated on deploy, i.e. effectively daily.

## 3. GitHub source repo and license

**Repo:** https://github.com/langchain-ai/docs — the build pipeline + source for docs.langchain.com.

- **Formats:** `.mdx` primarily (plus `.md`); snippets in `src/snippets/`; `.ipynb` supported but "not recommended for new content" per the README.
- **Structure:** `/src` (source, with `src/oss/{langchain,langgraph,integrations,concepts,python,javascript,...}`), `/pipeline` (build code), `src/docs.json` (navigation). **`/build` (generated output) is NOT committed** — GitHub contents API returns 404 for it. The published per-language pages are produced by the pipeline at deploy time.
- **License:** MIT, "Copyright (c) 2025 LangChain" (verified at https://raw.githubusercontent.com/langchain-ai/docs/main/LICENSE). MIT's grant covers the "software and associated documentation files", so redistribution/derivative use of the docs content is permitted with attribution + license notice. `langchain-ai/langchain` and `langchain-ai/langgraph` are also MIT.
- **Velocity:** ~3,534 commits; 5 commits landed on 2026-07-13 alone (multiple commits per day is normal). Docs change continuously in small increments.

**Crawling permission signals (verified):** https://docs.langchain.com/robots.txt is explicitly permissive:

> "Content-Signal: ai-train=yes, search=yes, ai-input=yes"

Only `/cdn-cgi/` and `/_next/` are disallowed, and it advertises `Sitemap: https://docs.langchain.com/sitemap.xml`. Combined with the MIT-licensed source repo, license/ToS risk for offline ingestion is about as low as it gets — just ship the MIT license text and attribution in the pack.

## 4. Per-page markdown and sitemap (verified)

- **Per-page markdown works:** append `.md` to any docs URL. `https://docs.langchain.com/oss/python/langchain/retrieval.md` → `200`, `Content-Type: text/markdown`, 15,271 bytes of clean markdown (headings, fenced code, `<Note>` MDX tags intact), already **resolved to the Python variant** (the site's language toggle is pre-applied per URL path). Each `.md` page carries a small injected preamble pointing at the llms.txt index.
- **Sitemap:** https://docs.langchain.com/sitemap.xml → 217 KB, **1,441 `<loc>` URLs, each with `<lastmod>`** (e.g. `2026-07-13T15:44:33.069Z`). 384 URLs are under `/oss/`. This is the only complete machine-readable index of the OSS docs (remember: llms.txt omits them).
- `.md` responses send `Last-Modified` but it appears to be deploy-time, so per-page change detection should use sitemap `<lastmod>` plus content hashing, not HTTP conditional GET alone.

## 5. Method comparison for a repeatable refresh pipeline

| Criterion | A. Sitemap-scoped `.md` fetch | B. llms-full.txt download | C. git clone langchain-ai/docs |
|---|---|---|---|
| Completeness | Full (sitemap = 1,441 pages incl. all `/oss/`) | Full site (1,491 pages) but monolithic | Full source, but raw |
| Structure preservation | Excellent: per-page markdown, headings/code fences intact, Python variant pre-resolved | Good: same markdown, but one 13.6 MB blob; needs splitting on `Source:` lines | Raw `.mdx` with unresolved snippet includes and combined Python/JS conditional content; requires running their pipeline to get what the site shows |
| Scoping to retrieval topics | Trivial: URL-prefix allowlist before fetching | Post-hoc: split then filter by `Source:` URL — you still download all 13.6 MB | Path allowlist in `src/oss/`, but content isn't render-ready |
| Change detection | sitemap `<lastmod>` + SHA-256 of page bodies | File-level only (whole blob changes daily) | Best-in-class: git SHAs and diffs |
| Stability of method | High: Mintlify-standard `.md` endpoints + sitemap; survived the python.langchain.com → docs.langchain.com move via 308 redirects | Medium-high: llms-full.txt is standard Mintlify output, but the old LangGraph site's llms-full.txt already 404s — these files get dropped when sites reorganize | High for the repo's existence; low for format stability (pipeline/source layout is an internal implementation detail) |
| License/ToS | MIT source + robots `ai-train=yes`; polite scoped fetch of ~40-80 pages | Same | MIT, cleanest provenance |

## 6. Scoping to retrieval-relevant content

Filter sitemap URLs with an **allowlist of prefixes/exact paths** (all verified present in sitemap):

**Core retrieval concepts (LangChain Python):**
- `https://docs.langchain.com/oss/python/langchain/retrieval` — the central RAG/retrieval conceptual doc (covers 2-step RAG, agentic RAG, knowledge bases)
- `https://docs.langchain.com/oss/python/langchain/knowledge-base`
- Supporting: `/oss/python/langchain/{overview,quickstart,structured-output,streaming}` as context pages (optional)

**Integration category pages (Python):**
- `/oss/python/integrations/retrievers`
- `/oss/python/integrations/vectorstores`
- `/oss/python/integrations/splitters`
- `/oss/python/integrations/embeddings`
- `/oss/python/integrations/document_loaders`
- Optionally `/oss/python/integrations/providers/{openai,anthropic,google,aws,huggingface,ollama,...}` (17 provider pages exist)

Note: unlike the pre-2026 docs, Python integrations are now **consolidated category pages** (only 23 Python integration URLs total), not hundreds of per-vendor pages — deep per-class detail lives in reference.langchain.com. This makes full integration coverage cheap.

**LangGraph (Python) — RAG-relevant subset:**
- `/oss/python/langgraph/agentic-rag` (the agentic RAG tutorial)
- `/oss/python/langgraph/{overview,graph-api,persistence,add-memory,streaming,workflows-agents,use-subgraphs}`
- `/oss/python/concepts/memory`

**Exclude:** everything under `/langsmith/`, `/api-reference/` (platform APIs, ~60% of the site), `/oss/javascript/` (JS duplicates of every page), `/oss/python/deepagents/` unless wanted, `/oss/python/contributing/`.

A first-cut allowlist yields roughly **40-80 pages (~1-3 MB markdown)** versus 1,441 pages / 13.6 MB for everything — a ~90%+ noise reduction before chunking.

**API-level detail (optional tier 2):** class/method docstrings (e.g. `VectorStore.as_retriever`, `RecursiveCharacterTextSplitter`) live at reference.langchain.com, which had no working llms.txt on probe. If needed, generate that tier from the MIT-licensed Python source docstrings (`pip download langchain-core langchain-text-splitters` or clone `langchain-ai/langchain`) rather than scraping the reference site.

## 7. Recommendation

**Primary: sitemap-scoped per-page `.md` fetch from docs.langchain.com.**
Pipeline per refresh: (1) fetch `sitemap.xml`; (2) filter `<loc>` against the allowlist above; (3) fetch each URL + `.md` (rate-limited, custom User-Agent); (4) strip the injected llms.txt preamble; (5) store one file per page with front matter (source URL, sitemap lastmod, fetch date, SHA-256). This gives per-page, Python-variant-resolved markdown with perfect heading/code structure, trivially scopable, license-clean (MIT + `ai-train=yes` robots signal), and the mechanism (Mintlify `.md` endpoints + sitemap) has proven durable across the site migration (old hosts 308-redirect into it).

**Fallback: llms-full.txt (https://docs.langchain.com/llms-full.txt, 13.6 MB).**
One request, zero crawl logic. Split on `Source: <url>` delimiters into 1,491 page records, filter by the same URL allowlist, discard `/oss/javascript/` and `/langsmith/`. Use this if `.md` endpoints or the sitemap ever break, or as a cross-check that the sitemap crawl didn't miss pages (page-count reconciliation between the two sources is a good CI assertion).

**Do not** build the primary path on: `llms.txt` (missing all `/oss/` pages), git clone of `langchain-ai/docs` (raw MDX needs their pipeline; `/build` is not committed), or the retired `python.langchain.com` / `langchain-ai.github.io/langgraph` hosts (keep them only as redirect-following aliases).

### Refresh strategy

- **Cadence:** monthly scheduled rebuild, plus an ad-hoc rebuild on LangChain/LangGraph minor releases (watch GitHub releases for `langchain-ai/langchain` and `langchain-ai/langgraph`). The docs repo commits daily, but retrieval-concept pages churn far slower than the LangSmith/platform sections; monthly is a good freshness/cost balance.
- **Change detection:** diff sitemap `<lastmod>` values against the stored manifest to pick candidate pages, then confirm with SHA-256 of the normalized page body (lastmod can be deploy-noise). Only re-embed chunks from pages whose content hash changed. Cheap tripwires between runs: watch `Last-Modified` on `llms-full.txt`, and watch commits touching `src/oss/langchain/retrieval.mdx` / `src/oss/langgraph/` in `langchain-ai/docs` via the GitHub API.
- **Pack versioning:** name packs `langchain-docs-YYYY.MM.N` with a `manifest.json` (per-page: URL, lastmod, hash, fetch timestamp; plus pipeline version and library versions documented at fetch time, e.g. from the changelog pages). Keep the previous pack until the new one passes smoke checks (page count within tolerance, key pages present, no truncated fetches). Include the MIT LICENSE text and an attribution note ("Documentation (c) LangChain, MIT License, from docs.langchain.com") inside every pack.
- **Structural drift guardrails:** fail the pipeline loudly if the allowlist matches < 90% of last run's page count, if any allowlisted URL 404s, or if the sitemap URL scheme changes (e.g. `/oss/python/` prefix disappears) — that is the signal the docs moved again and the allowlist needs a human review, not a silent partial pack.

## Sources

- https://docs.langchain.com/ and https://docs.langchain.com/robots.txt (robots + Content-Signal, verified 2026-07-13)
- https://docs.langchain.com/llms.txt (100 KB, 958 .md links, no /oss/)
- https://docs.langchain.com/llms-full.txt (13,580,473 bytes, 1,491 `Source:` page markers)
- https://docs.langchain.com/sitemap.xml (1,441 URLs with lastmod, 384 under /oss/)
- https://docs.langchain.com/oss/python/langchain/retrieval.md (200, text/markdown, 15,271 B)
- https://github.com/langchain-ai/docs + https://raw.githubusercontent.com/langchain-ai/docs/main/LICENSE (MIT) + GitHub contents/commits API (structure, /build 404, commit dates)
- https://python.langchain.com/llms.txt (308 → docs.langchain.com)
- https://langchain-ai.github.io/langgraph/llms.txt (moved notice); .../llms-full.txt (404)
- https://github.com/langchain-ai/langgraph (docs dir reduced to redirects); langchain-ai/langchain contents API (/docs 404)
- https://reference.langchain.com/python/ (API reference host; llms.txt probe 503)
