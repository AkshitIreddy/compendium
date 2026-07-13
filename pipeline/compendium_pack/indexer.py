"""usearch index construction + recall validation.

Builds one f16 HNSW index per tier (cards, chunks) keyed by card_key / chunk id,
serializes it to bytes for storage as a BLOB inside the pack, and measures
recall@10 against exact brute-force cosine over the float32 vectors. The build
fails if recall drops below the recipe's gate — a shipped index must never be
silently worse than exact search plus the runtime's exact re-scoring assumes
the true vectors are the source of truth.
"""
from __future__ import annotations

import hashlib

import numpy as np
import usearch
from usearch.index import Index

from .recipe import IndexSpec


def build_index(keys: np.ndarray, vectors: np.ndarray, spec: IndexSpec) -> tuple[bytes, float]:
    """Returns (serialized_index_bytes, recall_at_10)."""
    assert vectors.dtype == np.float32 and vectors.ndim == 2
    index = Index(
        ndim=vectors.shape[1],
        metric="cos",
        dtype=spec.quantization,
        connectivity=spec.connectivity,
        expansion_add=spec.expansion_add,
        expansion_search=spec.expansion_search,
    )
    index.add(keys.astype(np.uint64), vectors)

    recall = _recall_at_10(index, keys, vectors)
    blob = bytes(index.save(None))
    return blob, recall


def _recall_at_10(index: Index, keys: np.ndarray, vectors: np.ndarray, sample: int = 256) -> float:
    n = len(keys)
    k = min(10, n)
    rng = np.random.default_rng(7)
    q_idx = rng.choice(n, size=min(sample, n), replace=False)
    queries = vectors[q_idx]

    sims = queries @ vectors.T  # vectors are L2-normalized -> cosine
    exact = [set(keys[np.argsort(-row)[:k]].tolist()) for row in sims]

    matches = index.search(queries, k)
    hits = 0
    for i in range(len(q_idx)):
        got = set(int(x) for x in matches[i].keys)
        hits += len(got & exact[i])
    return hits / (len(q_idx) * k)


def index_row(tier: str, blob: bytes, recall: float, dims: int, count: int, spec: IndexSpec) -> dict:
    return {
        "tier": tier,
        "usearch_version": usearch.__version__,
        "metric": "cos",
        "quantization": spec.quantization,
        "dims": dims,
        "connectivity": spec.connectivity,
        "expansion_add": spec.expansion_add,
        "expansion_search": spec.expansion_search,
        "count": count,
        "recall_at_10": recall,
        "sha256": hashlib.sha256(blob).hexdigest(),
        "blob": blob,
    }
