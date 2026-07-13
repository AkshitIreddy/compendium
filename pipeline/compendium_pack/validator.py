"""Pack validation: every check a shipped pack must pass. Run automatically at
the end of build and available standalone (python -m compendium_pack validate).
"""
from __future__ import annotations

import hashlib
import json
import sqlite3
from pathlib import Path

import numpy as np
from usearch.index import Index

from . import PACK_APPLICATION_ID, SCHEMA_VERSION
from .writer import REQUIRED_MANIFEST_KEYS


class ValidationError(Exception):
    pass


def validate_pack(path: Path) -> dict:
    errors: list[str] = []
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)

    def check(cond: bool, msg: str):
        if not cond:
            errors.append(msg)

    app_id = db.execute("PRAGMA application_id").fetchone()[0]
    user_version = db.execute("PRAGMA user_version").fetchone()[0]
    check(app_id == PACK_APPLICATION_ID, f"application_id {app_id:#x} != CMPD magic")
    check(user_version == SCHEMA_VERSION, f"schema version {user_version} != {SCHEMA_VERSION}")

    integrity = db.execute("PRAGMA integrity_check").fetchone()[0]
    check(integrity == "ok", f"integrity_check: {integrity}")
    fk_violations = db.execute("PRAGMA foreign_key_check").fetchall()
    check(not fk_violations, f"foreign key violations: {fk_violations[:5]}")

    manifest = dict(db.execute("SELECT key, value FROM manifest").fetchall())
    for key in REQUIRED_MANIFEST_KEYS:
        check(bool(manifest.get(key, "").strip()), f"manifest key missing/empty: {key}")
    dims = int(manifest.get("embedding_dims", 0))

    counts = {
        t: db.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0]
        for t in (
            "techniques", "chunks", "documents", "failure_modes",
            "card_embeddings", "chunk_embeddings", "phrasing_embeddings",
            "technique_relations", "technique_failure_modes",
        )
    }
    check(counts["techniques"] > 0, "no techniques")
    check(counts["chunks"] > 0, "no chunks")
    check(counts["card_embeddings"] == counts["techniques"], "card embedding count mismatch")
    check(counts["chunk_embeddings"] == counts["chunks"], "chunk embedding count mismatch")

    orphan_techniques = db.execute(
        "SELECT COUNT(*) FROM techniques t WHERE NOT EXISTS "
        "(SELECT 1 FROM chunks c WHERE c.technique_slug = t.slug)"
    ).fetchone()[0]
    check(orphan_techniques == 0, f"{orphan_techniques} techniques have zero chunks")

    for table, id_col in (("card_embeddings", "technique_slug"), ("chunk_embeddings", "chunk_id")):
        bad = db.execute(
            f"SELECT COUNT(*) FROM {table} WHERE length(vector) != ?", (dims * 4,)
        ).fetchone()[0]
        check(bad == 0, f"{table}: {bad} vectors with wrong byte length")

    fts_chunks = db.execute("SELECT COUNT(*) FROM chunks_fts").fetchone()[0]
    check(fts_chunks == counts["chunks"], "chunks_fts row count mismatch")
    fts_cards = db.execute("SELECT COUNT(*) FROM cards_fts").fetchone()[0]
    check(fts_cards == counts["techniques"], "cards_fts row count mismatch")

    sample = db.execute(
        "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH 'retrieval' LIMIT 1"
    ).fetchall()
    check(bool(sample), "FTS smoke query for 'retrieval' returned nothing")

    # vector indexes: hash, loadability, self-query sanity
    for tier, id_query in (
        ("cards", "SELECT t.card_key, e.vector FROM techniques t "
                  "JOIN card_embeddings e ON e.technique_slug = t.slug ORDER BY t.card_key"),
        ("chunks", "SELECT chunk_id, vector FROM chunk_embeddings ORDER BY chunk_id"),
    ):
        row = db.execute(
            "SELECT blob, sha256, count, dims, quantization FROM vector_indexes WHERE tier = ?",
            (tier,),
        ).fetchone()
        if row is None:
            errors.append(f"vector index missing for tier {tier}")
            continue
        blob, sha, count, idims, quant = row
        check(hashlib.sha256(blob).hexdigest() == sha, f"{tier} index sha256 mismatch")
        check(idims == dims, f"{tier} index dims != manifest dims")
        index = Index.restore(bytes(blob))
        check(len(index) == count, f"{tier} index size {len(index)} != recorded {count}")

        pairs = db.execute(id_query).fetchall()
        keys = np.array([p[0] for p in pairs], dtype=np.uint64)
        vecs = np.stack([np.frombuffer(p[1], dtype="<f4") for p in pairs])
        rng = np.random.default_rng(3)
        for qi in rng.choice(len(keys), size=min(20, len(keys)), replace=False):
            got = [int(k) for k in index.search(vecs[qi], 3).keys]
            check(
                int(keys[qi]) in got,
                f"{tier} self-query: key {int(keys[qi])} not in top-3 {got}",
            )

    db.close()
    if errors:
        raise ValidationError("\n".join(f"- {e}" for e in errors))
    return {"manifest": {k: v for k, v in manifest.items() if k != "license_text"}, "counts": counts}
