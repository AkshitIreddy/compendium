"""Notebook processor (the reference source-type implementation).

Genuinely notebook-aware, per the design principle:
- markdown cells are split at headers into a section tree; code cells attach to
  their enclosing section and are never split mid-block;
- install/API-key boilerplate is excluded from chunks (but kept in the document
  so the in-app source view stays faithful);
- outputs are whitelisted and size-capped for the shipped document JSON;
- every chunk records the exact cell range it came from, so the app can
  deep-link a citation to the cells that back it;
- each chunk's embedded text gets a contextual header (technique + section) —
  the corpus's own contextual_chunk_headers technique, applied to itself.

Input: a curation dir of technique cards + ontology.json, and a local clone of
the source repo. Output: ProcessedPack (documents, techniques, chunks, graph).
"""
from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path

import nbformat

# ---------------------------------------------------------------- data model


@dataclass
class Chunk:
    technique_slug: str | None
    heading_path: str
    kind: str  # markdown | code | mixed
    body: str
    location: dict  # {"cells": [first, last]} for notebooks, {"anchor": "#..."} for webdocs


@dataclass
class ProcessedDoc:
    slug: str
    title: str
    source_url: str
    content: dict  # sanitized nbformat-lite JSON
    chunks: list[Chunk] = field(default_factory=list)


@dataclass
class ProcessedPack:
    stages: list[dict]
    failure_modes: list[dict]
    cards: dict[str, dict]         # slug -> curation card JSON
    graph: dict[str, dict]         # slug -> ontology entry {stage, failure_mode_ids, related}
    docs: list[ProcessedDoc]
    source_ref: str


# ------------------------------------------------------------ noise heuristics

INSTALL_RE = re.compile(r"^\s*[%!]?\s*pip3?\s+install|^\s*%%capture|^\s*!apt", re.M)
KEY_SETUP_RE = re.compile(r"getpass|load_dotenv|os\.environ\[[\"']\w*API_KEY", re.I)
# Marketing banner repeated at the top of every notebook in this repo — kept in
# the document view (faithful), excluded from chunks (not knowledge).
PROMO_RE = re.compile(
    r"diamant-ai\.com|The RAG Techniques Book|linktr\.ee/nirdiamant|"
    r"colab\.research\.google\.com/assets/colab-badge", re.I,
)
MAX_OUTPUT_TEXT = 2000
MAX_OUTPUT_HTML = 50_000
MAX_OUTPUT_PNG_B64 = 200_000


def _is_noise_code(src: str) -> bool:
    if INSTALL_RE.search(src):
        return True
    lines = [l for l in src.splitlines() if l.strip() and not l.strip().startswith("#")]
    if len(lines) <= 12 and KEY_SETUP_RE.search(src):
        return True
    return False


def _sanitize_outputs(cell) -> list[dict]:
    out = []
    for o in cell.get("outputs", []):
        otype = o.get("output_type")
        if otype == "stream":
            text = "".join(o.get("text", []))
            if text.strip():
                out.append({"mime": "text/plain", "data": text[:MAX_OUTPUT_TEXT]})
        elif otype in ("execute_result", "display_data"):
            data = o.get("data", {})
            if "image/png" in data:
                png = data["image/png"]
                if isinstance(png, list):
                    png = "".join(png)
                if len(png) <= MAX_OUTPUT_PNG_B64:
                    out.append({"mime": "image/png", "data": png})
                    continue
            if "text/html" in data:
                html = data["text/html"]
                if isinstance(html, list):
                    html = "".join(html)
                if len(html) <= MAX_OUTPUT_HTML:
                    out.append({"mime": "text/html", "data": html})
                    continue
            if "text/plain" in data:
                text = data["text/plain"]
                if isinstance(text, list):
                    text = "".join(text)
                out.append({"mime": "text/plain", "data": text[:MAX_OUTPUT_TEXT]})
        elif otype == "error":
            tb = "\n".join(o.get("traceback", []))
            out.append({"mime": "application/x-traceback", "data": tb[:MAX_OUTPUT_TEXT]})
    return out


# ------------------------------------------------------------------- chunking

HEADER_RE = re.compile(r"^(#{1,6})\s+(.*)$")
TARGET_MAX_CHARS = 2200   # ~600 tokens with header; sections split at paragraphs beyond this
MIN_MERGE_CHARS = 180     # chunks smaller than this merge into their predecessor
MAX_CODE_CHARS = 4000     # giant code cells split at top-level def/class boundaries
TOPLEVEL_RE = re.compile(r"^(?:def |class |# |@|if __name__)", re.M)


METHOD_RE = re.compile(r"^\s{1,8}(?:def |async def |@)")
CLASS_RE = re.compile(r"^class\s+(\w+)", re.M)


def _split_lines_at(src: str, boundary: re.Pattern, min_block: int = 400) -> list[str]:
    blocks: list[list[str]] = [[]]
    for line in src.splitlines():
        if (
            boundary.match(line)
            and blocks[-1]
            and sum(len(l) + 1 for l in blocks[-1]) > min_block
        ):
            blocks.append([])
        blocks[-1].append(line)
    # coalesce neighbors so each piece stays under the cap where possible
    pieces: list[str] = []
    cur: list[str] = []
    for block in blocks:
        text = "\n".join(block)
        if cur and sum(len(c) + 1 for c in cur) + len(text) > MAX_CODE_CHARS:
            pieces.append("\n".join(cur))
            cur = []
        cur.append(text)
    if cur:
        pieces.append("\n".join(cur))
    return pieces


def _split_code(src: str) -> list[str]:
    """Split an oversized code cell, first at top-level def/class boundaries,
    then — for giant single classes — at method boundaries, prefixing
    continuation pieces with the class name so each piece stays interpretable."""
    if len(src) <= MAX_CODE_CHARS:
        return [src]
    out: list[str] = []
    for piece in _split_lines_at(src, TOPLEVEL_RE):
        if len(piece) <= MAX_CODE_CHARS:
            out.append(piece)
            continue
        cls = CLASS_RE.search(piece)
        subpieces = _split_lines_at(piece, METHOD_RE)
        for i, sp in enumerate(subpieces):
            if i > 0 and cls:
                sp = f"# class {cls.group(1)} (continued)\n{sp}"
            out.append(sp)
    return out


@dataclass
class _Segment:
    heading_path: str
    text: str
    cell_idx: int
    is_code: bool


def _walk_segments(nb, title: str) -> list[_Segment]:
    """Flatten the notebook into (heading_path, text) segments in order."""
    heading_stack: list[tuple[int, str]] = []  # (level, title)
    segments: list[_Segment] = []

    def path() -> str:
        return " > ".join(t for _, t in heading_stack) or title

    for idx, cell in enumerate(nb.cells):
        src = cell.source if isinstance(cell.source, str) else "".join(cell.source)
        if not src.strip():
            continue
        if cell.cell_type == "markdown":
            if PROMO_RE.search(src):
                continue
            buf: list[str] = []
            for line in src.splitlines():
                m = HEADER_RE.match(line)
                if m:
                    if buf and "".join(buf).strip():
                        segments.append(_Segment(path(), "\n".join(buf).strip(), idx, False))
                    buf = []
                    level, htext = len(m.group(1)), m.group(2).strip()
                    while heading_stack and heading_stack[-1][0] >= level:
                        heading_stack.pop()
                    heading_stack.append((level, htext))
                else:
                    buf.append(line)
            if buf and "\n".join(buf).strip():
                segments.append(_Segment(path(), "\n".join(buf).strip(), idx, False))
        elif cell.cell_type == "code":
            if _is_noise_code(src):
                continue
            for piece in _split_code(src.strip()):
                segments.append(_Segment(path(), f"```python\n{piece}\n```", idx, True))
    return segments


def _chunk_segments(segments: list[_Segment], slug: str) -> list[Chunk]:
    chunks: list[Chunk] = []
    cur: list[_Segment] = []

    def flush():
        if not cur:
            return
        body = "\n\n".join(s.text for s in cur)
        kinds = {s.is_code for s in cur}
        kind = "mixed" if kinds == {True, False} else ("code" if True in kinds else "markdown")
        chunks.append(
            Chunk(
                technique_slug=slug,
                heading_path=cur[0].heading_path,
                kind=kind,
                body=body,
                location={
                    "cells": [min(s.cell_idx for s in cur), max(s.cell_idx for s in cur)]
                },
            )
        )
        cur.clear()

    def top2(p: str) -> str:
        return " > ".join(p.split(" > ")[:2])

    for seg in segments:
        if cur and (
            top2(seg.heading_path) != top2(cur[0].heading_path)
            or sum(len(s.text) for s in cur) + len(seg.text) > TARGET_MAX_CHARS
        ):
            flush()
        cur.append(seg)
    flush()

    # merge undersized chunks into their predecessor (same technique, adjacent)
    merged: list[Chunk] = []
    for c in chunks:
        if merged and len(c.body) < MIN_MERGE_CHARS:
            prev = merged[-1]
            prev.body += "\n\n" + c.body
            prev.last_cell = max(prev.last_cell, c.last_cell)
            if prev.kind != c.kind:
                prev.kind = "mixed"
        else:
            merged.append(c)
    return merged


# ------------------------------------------------------------------ processor


def _resolve_git_sha(clone: Path) -> str:
    head = (clone / ".git" / "HEAD").read_text().strip()
    if head.startswith("ref: "):
        ref = head[5:]
        ref_file = clone / ".git" / ref
        if ref_file.exists():
            return ref_file.read_text().strip()
        packed = (clone / ".git" / "packed-refs").read_text()
        for line in packed.splitlines():
            if line.endswith(ref):
                return line.split()[0]
    return head


def _notebook_inventory(clone: Path) -> dict[str, Path]:
    """slug -> notebook path; slug convention: filename stem lowercased."""
    inventory = {}
    for sub in ("all_rag_techniques", "evaluation"):
        for nb_path in sorted((clone / sub).glob("*.ipynb")):
            inventory[nb_path.stem.lower()] = nb_path
    return inventory


def process(recipe, source_dir: Path) -> ProcessedPack:
    curation = recipe.curation_dir
    ontology = json.loads((curation / "ontology.json").read_text(encoding="utf-8"))
    cards: dict[str, dict] = {}
    for card_file in sorted(curation.glob("*.json")):
        if card_file.name in ("ontology.json", "_repo-overview.json"):
            continue
        card = json.loads(card_file.read_text(encoding="utf-8"))
        cards[card["slug"]] = card

    graph = ontology["techniques"]
    missing = sorted(set(graph) - set(cards)) + sorted(set(cards) - set(graph))
    if missing:
        raise ValueError(f"curation cards and ontology disagree on slugs: {missing}")

    inventory = _notebook_inventory(source_dir)
    repo_url = recipe.source.get("repo", "").rstrip("/")
    sha = _resolve_git_sha(source_dir)

    docs: list[ProcessedDoc] = []
    for slug, card in sorted(cards.items()):
        nb_path = inventory.get(slug)
        if nb_path is None:
            raise FileNotFoundError(f"no notebook found for slug '{slug}'")
        nb = nbformat.read(nb_path, as_version=4)

        rel = nb_path.relative_to(source_dir).as_posix()
        cells = []
        for cell in nb.cells:
            src = cell.source if isinstance(cell.source, str) else "".join(cell.source)
            entry = {"t": "md" if cell.cell_type == "markdown" else "code", "src": src}
            if cell.cell_type == "code":
                outputs = _sanitize_outputs(cell)
                if outputs:
                    entry["outputs"] = outputs
            if cell.cell_type in ("markdown", "code"):
                cells.append(entry)

        segments = _walk_segments(nb, card["title"])
        chunks = _chunk_segments(segments, slug)

        docs.append(
            ProcessedDoc(
                slug=slug,
                title=card["title"],
                source_url=f"{repo_url}/blob/{sha}/{rel}",
                content={"format": "nbformat-lite", "v": 1, "cells": cells},
                chunks=chunks,
            )
        )

    return ProcessedPack(
        stages=ontology["stages"],
        failure_modes=ontology["failure_modes"],
        cards=cards,
        graph=graph,
        docs=docs,
        source_ref=f"{repo_url}@{sha}",
    )
