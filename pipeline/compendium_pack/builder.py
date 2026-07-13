"""Build orchestration: recipe -> processor -> embed -> index -> pack file."""
from __future__ import annotations

import datetime
import json
from pathlib import Path

import numpy as np

from . import SCHEMA_VERSION
from .embedder import Embedder
from .indexer import build_index, index_row
from .processors import get_processor
from .recipe import Recipe, load_recipe
from .writer import PackWriter


def _token_estimate(text: str) -> int:
    return max(1, round(len(text) / 3.6))


def _card_embed_text(card: dict) -> str:
    return f"{card['title']}. {card['one_liner']}\n\n{card['summary']}"


def build_pack(pack_dir: Path, source_dir: Path, out_dir: Path, api_key: str) -> Path:
    recipe = load_recipe(pack_dir)
    print(f"building pack '{recipe.id}' v{recipe.version} (source_type={recipe.source_type})")

    processor = get_processor(recipe.source_type)
    processed = processor(recipe, source_dir)
    n_chunks = sum(len(d.chunks) for d in processed.docs)
    print(f"processed {len(processed.docs)} documents -> {n_chunks} chunks")

    embedder = Embedder(
        api_key,
        recipe.embedding.model,
        recipe.embedding.dims,
        recipe.embedding.input_type,
        cache_dir=pack_dir.parent.parent / ".cache",
    )

    out_path = out_dir / f"{recipe.id}.pack"
    writer = PackWriter(out_path)

    writer.write_stages(processed.stages)
    writer.write_failure_modes(processed.failure_modes)

    # documents + techniques + chunks. Packs without technique cards (e.g.
    # webdocs) skip the card layer entirely — chunks are the knowledge.
    vendor_disclosures = recipe.processor_options.get("vendor_disclosures", {})
    slugs = sorted(processed.cards)
    card_keys = {slug: i + 1 for i, slug in enumerate(slugs)}
    chunk_rows: list[tuple[int, str]] = []  # (chunk_id, embed_text)
    doc_kind = "notebook" if recipe.source_type == "notebook" else "webdoc"

    for doc in processed.docs:
        card = processed.cards.get(doc.slug)
        doc_id = writer.write_document(
            kind=doc_kind,
            title=card["title"] if card else doc.title,
            source_url=doc.source_url,
            license_note=recipe.document_note,
            content=doc.content,
        )
        if card:
            graph_entry = processed.graph[doc.slug]
            writer.write_technique(
                {
                    "slug": doc.slug,
                    "card_key": card_keys[doc.slug],
                    "title": card["title"],
                    "one_liner": card["one_liner"],
                    "stage_id": graph_entry["stage"],
                    "complexity": card["complexity"],
                    "problem_solved": card["problem_solved"],
                    "how_it_works": card["how_it_works"],
                    "when_to_use": json.dumps(card["when_to_use"]),
                    "tradeoffs": json.dumps(card["tradeoffs"]),
                    "key_dependencies": json.dumps(card.get("key_dependencies", [])),
                    "keywords": json.dumps(card.get("keywords", [])),
                    "summary": card["summary"],
                    "vendor_disclosure": vendor_disclosures.get(doc.slug),
                    "document_id": doc_id,
                }
            )
            writer.write_technique_failure_modes(
                [(doc.slug, fm_id) for fm_id in graph_entry["failure_mode_ids"]]
            )
        for chunk in doc.chunks:
            context = card["title"] if card else doc.title
            prefix = "Technique" if card else "Doc"
            embed_text = (
                f"{prefix}: {context} — Section: {chunk.heading_path}\n\n{chunk.body}"
            )
            chunk_id = writer.write_chunk(
                {
                    "document_id": doc_id,
                    "technique_slug": chunk.technique_slug,
                    "heading_path": chunk.heading_path,
                    "kind": chunk.kind,
                    "text": embed_text,
                    "display_text": chunk.body,
                    "token_count": _token_estimate(embed_text),
                    "location": json.dumps(chunk.location),
                }
            )
            chunk_rows.append((chunk_id, embed_text))

    # relations (drop dangling targets defensively; the ontology was already cleaned)
    triples, dropped = [], []
    for slug in slugs:
        for rel in processed.graph[slug].get("related", []):
            if rel["target_slug"] in card_keys:
                triples.append((slug, rel["target_slug"], rel["relation"]))
            else:
                dropped.append((slug, rel["target_slug"]))
    writer.write_relations(triples)
    if dropped:
        print(f"  dropped {len(dropped)} dangling relations: {dropped}")

    # ---- embeddings
    card_vecs = None
    if slugs:
        print("embedding technique cards...")
        card_texts = [_card_embed_text(processed.cards[s]) for s in slugs]
        card_vecs = embedder.embed(card_texts)
        writer.write_card_embeddings(
            [(s, card_vecs[i].astype("<f4").tobytes()) for i, s in enumerate(slugs)]
        )

    print("embedding chunks...")
    chunk_vecs = embedder.embed([t for _, t in chunk_rows])
    writer.write_chunk_embeddings(
        [(cid, chunk_vecs[i].astype("<f4").tobytes()) for i, (cid, _) in enumerate(chunk_rows)]
    )

    phrasing_rows = [
        (fm["id"], p)
        for fm in processed.failure_modes
        for p in fm["example_phrasings"]
    ]
    if phrasing_rows:
        print("embedding failure-mode phrasings...")
        phrasing_vecs = embedder.embed([p for _, p in phrasing_rows])
        writer.write_phrasing_embeddings(
            [
                (fm_id, p, phrasing_vecs[i].astype("<f4").tobytes())
                for i, (fm_id, p) in enumerate(phrasing_rows)
            ]
        )

    writer.finish_chunks_fts()

    # ---- vector indexes
    print("building usearch indexes...")
    gates: list[tuple[str, float]] = []
    if card_vecs is not None:
        card_blob, card_recall = build_index(
            np.array([card_keys[s] for s in slugs], dtype=np.uint64), card_vecs, recipe.index
        )
        gates.append(("cards", card_recall))
        writer.write_vector_index(
            index_row("cards", card_blob, card_recall, recipe.embedding.dims, len(slugs), recipe.index)
        )
    chunk_blob, chunk_recall = build_index(
        np.array([cid for cid, _ in chunk_rows], dtype=np.uint64), chunk_vecs, recipe.index
    )
    gates.append(("chunks", chunk_recall))
    writer.write_vector_index(
        index_row("chunks", chunk_blob, chunk_recall, recipe.embedding.dims, len(chunk_rows), recipe.index)
    )
    print(
        "  recall@10 "
        + " ".join(f"{t}={r:.4f}" for t, r in gates)
        + f" (gate {recipe.index.recall_gate})"
    )
    for tier, recall in gates:
        if recall < recipe.index.recall_gate:
            raise RuntimeError(
                f"{tier} index recall@10 {recall:.4f} below gate {recipe.index.recall_gate} — "
                "raise expansion_search/expansion_add in the recipe and rebuild"
            )

    # ---- manifest + finalize
    import usearch as _usearch

    writer.write_manifest(
        {
            "schema_version": SCHEMA_VERSION,
            "pack_id": recipe.id,
            "pack_version": recipe.version,
            "name": recipe.name,
            "description": recipe.description,
            "source_type": recipe.source_type,
            "embedding_model": recipe.embedding.model,
            "embedding_dims": recipe.embedding.dims,
            "embedding_input_type": recipe.embedding.input_type,
            "license_id": recipe.license_id,
            "license_text": recipe.license_text,
            "attribution_html": recipe.attribution_html,
            "built_at": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
            "source_ref": processed.source_ref,
            "usearch_version": _usearch.__version__,
            "counts": json.dumps(
                {
                    "techniques": len(slugs),
                    "documents": len(processed.docs),
                    "chunks": len(chunk_rows),
                    "failure_modes": len(processed.failure_modes),
                    "relations": len(triples),
                    "phrasings": len(phrasing_rows),
                }
            ),
        }
    )
    writer.finalize()

    lock = {
        "source_ref": processed.source_ref,
        "built_at": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
        "embed_api_calls": embedder.api_calls,
    }
    (pack_dir / "sources.lock").write_text(json.dumps(lock, indent=2))
    size_mb = out_path.stat().st_size / 1e6
    print(f"pack written: {out_path} ({size_mb:.1f} MB, {embedder.api_calls} embed API calls)")
    return out_path
