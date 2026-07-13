"""Cohere embedding with a content-hash cache.

Every text is cached in .cache/embed-cache.sqlite keyed by
sha256(model|dims|input_type|text), so iterating on pack metadata or re-running
builds costs zero API calls for unchanged content. Batches up to 96 texts per
call (the embed-v4.0 limit) with exponential backoff on 429/5xx.
"""
from __future__ import annotations

import hashlib
import sqlite3
import time
from pathlib import Path

import numpy as np
import requests

EMBED_URL = "https://api.cohere.com/v2/embed"
BATCH = 96
RETRY_STATUSES = {429, 500, 502, 503, 504}


class Embedder:
    def __init__(self, api_key: str, model: str, dims: int, input_type: str, cache_dir: Path):
        self.api_key = api_key
        self.model = model
        self.dims = dims
        self.input_type = input_type
        cache_dir.mkdir(parents=True, exist_ok=True)
        self.cache = sqlite3.connect(cache_dir / "embed-cache.sqlite")
        self.cache.execute(
            "CREATE TABLE IF NOT EXISTS embeddings (key TEXT PRIMARY KEY, vector BLOB NOT NULL)"
        )
        self.api_calls = 0

    def _key(self, text: str) -> str:
        h = hashlib.sha256()
        h.update(f"{self.model}|{self.dims}|{self.input_type}|".encode())
        h.update(text.encode("utf-8"))
        return h.hexdigest()

    def embed(self, texts: list[str]) -> np.ndarray:
        """Return an (n, dims) float32 array of L2-normalized embeddings."""
        keys = [self._key(t) for t in texts]
        out: dict[int, np.ndarray] = {}
        missing: list[int] = []
        for i, key in enumerate(keys):
            row = self.cache.execute(
                "SELECT vector FROM embeddings WHERE key = ?", (key,)
            ).fetchone()
            if row:
                out[i] = np.frombuffer(row[0], dtype="<f4")
            else:
                missing.append(i)

        for start in range(0, len(missing), BATCH):
            idxs = missing[start : start + BATCH]
            vectors = self._call([texts[i] for i in idxs])
            with self.cache:
                for i, vec in zip(idxs, vectors):
                    out[i] = vec
                    self.cache.execute(
                        "INSERT OR REPLACE INTO embeddings (key, vector) VALUES (?, ?)",
                        (keys[i], vec.astype("<f4").tobytes()),
                    )
            done = min(start + BATCH, len(missing))
            print(f"  embedded {done}/{len(missing)} new texts ({self.api_calls} API calls)")

        matrix = np.stack([out[i] for i in range(len(texts))]).astype(np.float32)
        # embed-v4.0 returns unit vectors already; normalize defensively so the
        # cosine/IP equivalence the runtime relies on can never silently break.
        norms = np.linalg.norm(matrix, axis=1, keepdims=True)
        matrix /= np.clip(norms, 1e-12, None)
        return matrix

    def _call(self, texts: list[str]) -> list[np.ndarray]:
        body = {
            "model": self.model,
            "texts": texts,
            "input_type": self.input_type,
            "embedding_types": ["float"],
            "output_dimension": self.dims,
        }
        last_failure = "no attempt made"
        for attempt in range(6):
            try:
                r = requests.post(
                    EMBED_URL,
                    headers={"Authorization": f"Bearer {self.api_key}"},
                    json=body,
                    timeout=300,
                )
            except requests.exceptions.RequestException as e:
                last_failure = type(e).__name__
                print(f"  embed attempt {attempt + 1}: {last_failure}; retrying")
                time.sleep(2**attempt)
                continue
            if r.status_code in RETRY_STATUSES:
                last_failure = f"HTTP {r.status_code}"
                print(f"  embed attempt {attempt + 1}: {last_failure}; retrying")
                time.sleep(2**attempt)
                continue
            r.raise_for_status()
            self.api_calls += 1
            floats = r.json()["embeddings"]["float"]
            if len(floats) != len(texts) or any(len(v) != self.dims for v in floats):
                raise RuntimeError("embedding count/dims mismatch from API")
            return [np.asarray(v, dtype=np.float32) for v in floats]
        raise RuntimeError(f"embed failed after retries (last failure: {last_failure})")
