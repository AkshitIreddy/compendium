"""Webdocs processor: sitemap-scoped per-page markdown acquisition.

Built for docs.langchain.com (Mintlify) per the verified research:
- sitemap.xml is the only complete machine-readable index (llms.txt is
  truncated AND omits all /oss/ pages — never rely on it);
- appending .md to any page URL returns clean markdown with the Python
  variant pre-resolved;
- scoping is a prefix allowlist for the hierarchical /oss/ namespace plus an
  exact-slug allowlist for the flat /langsmith/ namespace;
- pages are cached by content hash so refresh runs only re-embed what
  actually changed (sitemap lastmod is deploy noise);
- guardrails fail the build loudly: any allowlisted page 404s, or the fetched
  page count swings ±10% from the recipe's expected count.

Produces a card-less ProcessedPack: chunks are the knowledge, documents are
the in-app source views.
"""
from __future__ import annotations

import hashlib
import json
import re
import time
import xml.etree.ElementTree as ET
from pathlib import Path

import requests

from .notebook import Chunk, ProcessedDoc, ProcessedPack

UA = "compendium-pack-pipeline (+https://github.com/AkshitIreddy/compendium)"
FETCH_DELAY_S = 0.25  # polite: ~4 pages/second
HEADER_RE = re.compile(r"^(#{1,6})\s+(.*)$")
TARGET_MAX_CHARS = 2400
MIN_MERGE_CHARS = 200
# Mintlify injects a short "Documentation Index" preamble into .md exports.
PREAMBLE_RE = re.compile(
    r"\A(?:>\s*.*documentation index.*\n|\s*<!--.*?-->\s*\n)*", re.I
)


def _fetch(url: str, session: requests.Session, tries: int = 4) -> requests.Response:
    last: requests.Response | None = None
    for attempt in range(tries):
        try:
            last = session.get(url, timeout=60, headers={"User-Agent": UA})
        except requests.exceptions.RequestException:
            time.sleep(2**attempt)
            continue
        if last.status_code in (429, 500, 502, 503, 504):
            time.sleep(2**attempt)
            continue
        return last
    if last is None:
        raise RuntimeError(f"network failure after {tries} attempts: {url}")
    return last


def _sitemap_urls(base: str, session: requests.Session) -> list[str]:
    r = _fetch(f"{base}/sitemap.xml", session)
    r.raise_for_status()
    root = ET.fromstring(r.content)
    ns = {"sm": "http://www.sitemaps.org/schemas/sitemap/0.9"}
    return [loc.text.strip() for loc in root.findall(".//sm:loc", ns) if loc.text]


def _slugify(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def _chunk_markdown(md: str, page_title: str) -> list[Chunk]:
    """Heading-aware chunking: sections split at h1-h3, oversize sections split
    at paragraph boundaries (never inside a code fence)."""
    heading_stack: list[tuple[int, str]] = []
    segments: list[tuple[str, str, str]] = []  # (heading_path, anchor, text)
    buf: list[str] = []
    in_fence = False
    anchor = ""

    def path() -> str:
        return " > ".join(t for _, t in heading_stack) or page_title

    def flush_buf():
        text = "\n".join(buf).strip()
        if text:
            segments.append((path(), anchor, text))
        buf.clear()

    for line in md.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            buf.append(line)
            continue
        m = None if in_fence else HEADER_RE.match(line)
        if m:
            flush_buf()
            level, title = len(m.group(1)), m.group(2).strip()
            while heading_stack and heading_stack[-1][0] >= level:
                heading_stack.pop()
            heading_stack.append((level, title))
            anchor = f"#{_slugify(title)}"
        else:
            buf.append(line)
    flush_buf()

    chunks: list[Chunk] = []
    for heading_path, anchor, text in segments:
        pieces = [text]
        if len(text) > TARGET_MAX_CHARS:
            pieces = _split_paragraphs(text)
        for piece in pieces:
            kind = "mixed" if "```" in piece else "markdown"
            chunks.append(
                Chunk(
                    technique_slug=None,
                    heading_path=heading_path,
                    kind=kind,
                    body=piece,
                    location={"anchor": anchor},
                )
            )

    # merge undersized chunks forward
    merged: list[Chunk] = []
    for c in chunks:
        if merged and len(c.body) < MIN_MERGE_CHARS and merged[-1].heading_path == c.heading_path:
            merged[-1].body += "\n\n" + c.body
            if merged[-1].kind != c.kind:
                merged[-1].kind = "mixed"
        else:
            merged.append(c)
    return merged


def _split_paragraphs(text: str) -> list[str]:
    """Split at blank lines, keeping code fences intact and coalescing to the cap."""
    blocks: list[str] = []
    cur: list[str] = []
    in_fence = False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
        cur.append(line)
        if not in_fence and not line.strip():
            blocks.append("\n".join(cur))
            cur = []
    if cur:
        blocks.append("\n".join(cur))

    pieces: list[str] = []
    acc: list[str] = []
    for block in blocks:
        if acc and sum(len(a) for a in acc) + len(block) > TARGET_MAX_CHARS:
            pieces.append("\n".join(acc).strip())
            acc = []
        acc.append(block)
    if acc:
        pieces.append("\n".join(acc).strip())
    return [p for p in pieces if p]


def process(recipe, source_dir: Path) -> ProcessedPack:
    """source_dir is the page cache directory (created if missing) — webdocs
    sources come from the network, cached by content hash for refreshes."""
    base = recipe.source["base_url"].rstrip("/")
    prefixes: list[str] = recipe.source.get("url_prefixes", [])
    exact_slugs: list[str] = recipe.source.get("exact_slugs", [])
    exact_prefix: str = recipe.source.get("exact_slug_prefix", "")
    expected = int(recipe.source.get("expected_pages", 0))

    cache_dir = Path(source_dir)
    cache_dir.mkdir(parents=True, exist_ok=True)
    session = requests.Session()

    sitemap = set(_sitemap_urls(base, session))
    print(f"  sitemap: {len(sitemap)} URLs")

    wanted: list[str] = sorted(
        url for url in sitemap if any(url.startswith(f"{base}{p}") for p in prefixes)
    )
    missing_slugs = []
    for slug in exact_slugs:
        url = f"{base}{exact_prefix}{slug}"
        if url in sitemap:
            wanted.append(url)
        else:
            missing_slugs.append(slug)
    if missing_slugs:
        raise RuntimeError(
            f"allowlisted slugs missing from sitemap (docs reorganized?): {missing_slugs[:10]}"
        )
    if expected and abs(len(wanted) - expected) > expected * 0.10:
        raise RuntimeError(
            f"page count {len(wanted)} deviates >10% from expected {expected} — "
            "docs likely reorganized; review the allowlist before shipping"
        )
    print(f"  allowlisted pages: {len(wanted)}")

    docs: list[ProcessedDoc] = []
    fetched_hashes: dict[str, str] = {}
    for i, url in enumerate(wanted):
        cache_key = hashlib.sha256(url.encode()).hexdigest()[:24]
        cache_file = cache_dir / f"{cache_key}.md"

        r = _fetch(f"{url}.md", session)
        if r.status_code == 404:
            raise RuntimeError(f"allowlisted page 404s: {url} — fail loudly, review allowlist")
        r.raise_for_status()
        # A 200 serving the SPA shell / an HTML error page must not be embedded.
        content_type = r.headers.get("Content-Type", "")
        body_head = r.text.lstrip()[:200].lower()
        if "markdown" not in content_type and (
            body_head.startswith("<!doctype") or body_head.startswith("<html")
        ):
            raise RuntimeError(
                f"page returned HTML instead of markdown (Content-Type: {content_type}): {url}"
            )
        md = PREAMBLE_RE.sub("", r.text).strip()
        cache_file.write_text(md, encoding="utf-8")
        fetched_hashes[url] = hashlib.sha256(md.encode()).hexdigest()
        time.sleep(FETCH_DELAY_S)

        title_match = re.search(r"^#\s+(.+)$", md, re.M)
        title = title_match.group(1).strip() if title_match else url.rsplit("/", 1)[-1]
        slug = url[len(base) :].strip("/").replace("/", "-")

        chunks = _chunk_markdown(md, title)
        docs.append(
            ProcessedDoc(
                slug=slug,
                title=title,
                source_url=url,
                content={"format": "markdown", "v": 1, "text": md},
                chunks=chunks,
            )
        )
        if (i + 1) % 25 == 0:
            print(f"  fetched {i + 1}/{len(wanted)} pages")

    (cache_dir / "_hashes.json").write_text(json.dumps(fetched_hashes, indent=1))

    snapshot = time.strftime("%Y-%m-%d", time.gmtime())
    return ProcessedPack(
        stages=[],
        failure_modes=[],
        cards={},
        graph={},
        docs=docs,
        source_ref=f"{base} (snapshot {snapshot}, {len(docs)} pages)",
    )
