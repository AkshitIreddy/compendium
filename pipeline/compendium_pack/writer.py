"""Pack SQLite writer: owns the pack schema (single source of truth for the
DDL — docs/PACK_FORMAT.md mirrors this file) and all inserts.

A pack is one SQLite file. It is read-only at runtime; the app ATTACHes it
with mode=ro&immutable=1. PRAGMA application_id / user_version let the app
reject non-pack files and incompatible schema versions cheaply.
"""
from __future__ import annotations

import json
import sqlite3
from pathlib import Path

from . import PACK_APPLICATION_ID, SCHEMA_VERSION

DDL = """
CREATE TABLE manifest (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE stages (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  description TEXT NOT NULL,
  position    INTEGER NOT NULL UNIQUE
);

CREATE TABLE failure_modes (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  description TEXT NOT NULL,
  phrasings   TEXT NOT NULL  -- JSON array of example user phrasings
);

CREATE TABLE documents (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL CHECK (kind IN ('notebook', 'webdoc')),
  title        TEXT NOT NULL,
  source_url   TEXT NOT NULL,
  license_note TEXT NOT NULL,
  content      TEXT NOT NULL  -- JSON; shape depends on kind (see PACK_FORMAT.md)
);

CREATE TABLE techniques (
  slug              TEXT PRIMARY KEY,
  card_key          INTEGER NOT NULL UNIQUE,  -- usearch key for the cards index
  title             TEXT NOT NULL,
  one_liner         TEXT NOT NULL,
  stage_id          TEXT NOT NULL REFERENCES stages(id),
  complexity        TEXT NOT NULL CHECK (complexity IN ('low', 'medium', 'high')),
  problem_solved    TEXT NOT NULL,
  how_it_works      TEXT NOT NULL,
  when_to_use       TEXT NOT NULL,  -- JSON array
  tradeoffs         TEXT NOT NULL,  -- JSON array
  key_dependencies  TEXT NOT NULL,  -- JSON array
  keywords          TEXT NOT NULL,  -- JSON array
  summary           TEXT NOT NULL,
  vendor_disclosure TEXT,
  document_id       INTEGER REFERENCES documents(id)
);

CREATE TABLE technique_failure_modes (
  technique_slug  TEXT NOT NULL REFERENCES techniques(slug),
  failure_mode_id TEXT NOT NULL REFERENCES failure_modes(id),
  PRIMARY KEY (technique_slug, failure_mode_id)
) WITHOUT ROWID;

CREATE TABLE technique_relations (
  from_slug TEXT NOT NULL REFERENCES techniques(slug),
  to_slug   TEXT NOT NULL REFERENCES techniques(slug),
  relation  TEXT NOT NULL CHECK (relation IN
    ('composes_with', 'alternative_to', 'prerequisite_of', 'refines', 'evaluated_by')),
  PRIMARY KEY (from_slug, to_slug, relation)
) WITHOUT ROWID;

CREATE TABLE chunks (
  id             INTEGER PRIMARY KEY,  -- usearch key for the chunks index
  document_id    INTEGER NOT NULL REFERENCES documents(id),
  technique_slug TEXT REFERENCES techniques(slug),
  heading_path   TEXT NOT NULL,
  kind           TEXT NOT NULL CHECK (kind IN ('markdown', 'code', 'mixed')),
  text           TEXT NOT NULL,   -- embedded text (contextual header included)
  display_text   TEXT NOT NULL,   -- rendered text (no header)
  token_count    INTEGER NOT NULL,
  location       TEXT NOT NULL    -- JSON: {"cells":[first,last]} or {"anchor":"#..."}
);

CREATE TABLE card_embeddings (
  technique_slug TEXT PRIMARY KEY REFERENCES techniques(slug),
  vector         BLOB NOT NULL    -- float32 LE, L2-normalized, manifest embedding_dims
) WITHOUT ROWID;

CREATE TABLE chunk_embeddings (
  chunk_id INTEGER PRIMARY KEY REFERENCES chunks(id),
  vector   BLOB NOT NULL
);

CREATE TABLE phrasing_embeddings (
  id              INTEGER PRIMARY KEY,
  failure_mode_id TEXT NOT NULL REFERENCES failure_modes(id),
  phrasing        TEXT NOT NULL,
  vector          BLOB NOT NULL
);

CREATE TABLE vector_indexes (
  tier             TEXT PRIMARY KEY CHECK (tier IN ('cards', 'chunks')),
  usearch_version  TEXT NOT NULL,
  metric           TEXT NOT NULL,
  quantization     TEXT NOT NULL,
  dims             INTEGER NOT NULL,
  connectivity     INTEGER NOT NULL,
  expansion_add    INTEGER NOT NULL,
  expansion_search INTEGER NOT NULL,
  count            INTEGER NOT NULL,
  recall_at_10     REAL NOT NULL,
  sha256           TEXT NOT NULL,
  blob             BLOB NOT NULL
) WITHOUT ROWID;

CREATE VIRTUAL TABLE chunks_fts USING fts5(
  text, heading_path,
  content='chunks', content_rowid='id',
  tokenize='porter unicode61'
);

-- Small self-contained FTS tables (44 cards / ~80 phrasings; duplication is trivial).
CREATE VIRTUAL TABLE cards_fts USING fts5(
  slug UNINDEXED, title, one_liner, summary, keywords_text, problem_solved,
  tokenize='porter unicode61'
);

CREATE VIRTUAL TABLE phrasings_fts USING fts5(
  failure_mode_id UNINDEXED, phrasing,
  tokenize='porter unicode61'
);

CREATE INDEX idx_chunks_technique ON chunks(technique_slug);
CREATE INDEX idx_chunks_document ON chunks(document_id);
CREATE INDEX idx_tfm_fm ON technique_failure_modes(failure_mode_id);
CREATE INDEX idx_relations_to ON technique_relations(to_slug);
"""

REQUIRED_MANIFEST_KEYS = [
    "schema_version", "pack_id", "pack_version", "name", "description",
    "source_type", "embedding_model", "embedding_dims", "embedding_input_type",
    "license_id", "license_text", "attribution_html", "built_at", "source_ref",
]


class PackWriter:
    def __init__(self, out_path: Path):
        self.path = out_path
        out_path.parent.mkdir(parents=True, exist_ok=True)
        if out_path.exists():
            out_path.unlink()
        self.db = sqlite3.connect(out_path)
        self.db.executescript(
            f"PRAGMA application_id = {PACK_APPLICATION_ID};"
            f"PRAGMA user_version = {SCHEMA_VERSION};"
            "PRAGMA journal_mode = OFF;"
            "PRAGMA synchronous = OFF;"
            "PRAGMA page_size = 4096;"
        )
        self.db.executescript(DDL)

    def write_manifest(self, entries: dict[str, str]) -> None:
        missing = [k for k in REQUIRED_MANIFEST_KEYS if not str(entries.get(k, "")).strip()]
        if missing:
            raise ValueError(f"manifest missing required keys: {missing}")
        with self.db:
            self.db.executemany(
                "INSERT INTO manifest (key, value) VALUES (?, ?)",
                [(k, str(v)) for k, v in entries.items()],
            )

    def write_stages(self, stages: list[dict]) -> None:
        with self.db:
            self.db.executemany(
                "INSERT INTO stages (id, name, description, position) VALUES (?, ?, ?, ?)",
                [(s["id"], s["name"], s["description"], s["position"]) for s in stages],
            )

    def write_failure_modes(self, fms: list[dict]) -> None:
        with self.db:
            self.db.executemany(
                "INSERT INTO failure_modes (id, name, description, phrasings) VALUES (?, ?, ?, ?)",
                [
                    (fm["id"], fm["name"], fm["description"], json.dumps(fm["example_phrasings"]))
                    for fm in fms
                ],
            )

    def write_document(self, kind: str, title: str, source_url: str, license_note: str, content: dict) -> int:
        cur = self.db.execute(
            "INSERT INTO documents (kind, title, source_url, license_note, content) VALUES (?, ?, ?, ?, ?)",
            (kind, title, source_url, license_note, json.dumps(content, ensure_ascii=False)),
        )
        self.db.commit()
        return cur.lastrowid

    def write_technique(self, row: dict) -> None:
        with self.db:
            self.db.execute(
                """INSERT INTO techniques (slug, card_key, title, one_liner, stage_id, complexity,
                     problem_solved, how_it_works, when_to_use, tradeoffs, key_dependencies,
                     keywords, summary, vendor_disclosure, document_id)
                   VALUES (:slug, :card_key, :title, :one_liner, :stage_id, :complexity,
                     :problem_solved, :how_it_works, :when_to_use, :tradeoffs, :key_dependencies,
                     :keywords, :summary, :vendor_disclosure, :document_id)""",
                row,
            )
            self.db.execute(
                "INSERT INTO cards_fts (slug, title, one_liner, summary, keywords_text, problem_solved) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (
                    row["slug"], row["title"], row["one_liner"], row["summary"],
                    " ".join(json.loads(row["keywords"])), row["problem_solved"],
                ),
            )

    def write_technique_failure_modes(self, pairs: list[tuple[str, str]]) -> None:
        with self.db:
            self.db.executemany(
                "INSERT OR IGNORE INTO technique_failure_modes (technique_slug, failure_mode_id) VALUES (?, ?)",
                pairs,
            )

    def write_relations(self, triples: list[tuple[str, str, str]]) -> None:
        with self.db:
            self.db.executemany(
                "INSERT OR IGNORE INTO technique_relations (from_slug, to_slug, relation) VALUES (?, ?, ?)",
                triples,
            )

    def write_chunk(self, row: dict) -> int:
        cur = self.db.execute(
            """INSERT INTO chunks (document_id, technique_slug, heading_path, kind, text,
                 display_text, token_count, location)
               VALUES (:document_id, :technique_slug, :heading_path, :kind, :text,
                 :display_text, :token_count, :location)""",
            row,
        )
        return cur.lastrowid

    def finish_chunks_fts(self) -> None:
        with self.db:
            self.db.execute(
                "INSERT INTO chunks_fts (rowid, text, heading_path) "
                "SELECT id, text, heading_path FROM chunks"
            )
            self.db.execute("INSERT INTO chunks_fts(chunks_fts) VALUES ('optimize')")
            self.db.execute("INSERT INTO cards_fts(cards_fts) VALUES ('optimize')")
            self.db.execute("INSERT INTO phrasings_fts(phrasings_fts) VALUES ('optimize')")

    def write_card_embeddings(self, rows: list[tuple[str, bytes]]) -> None:
        with self.db:
            self.db.executemany(
                "INSERT INTO card_embeddings (technique_slug, vector) VALUES (?, ?)", rows
            )

    def write_chunk_embeddings(self, rows: list[tuple[int, bytes]]) -> None:
        with self.db:
            self.db.executemany(
                "INSERT INTO chunk_embeddings (chunk_id, vector) VALUES (?, ?)", rows
            )

    def write_phrasing_embeddings(self, rows: list[tuple[str, str, bytes]]) -> None:
        with self.db:
            self.db.executemany(
                "INSERT INTO phrasing_embeddings (failure_mode_id, phrasing, vector) VALUES (?, ?, ?)",
                rows,
            )
            self.db.executemany(
                "INSERT INTO phrasings_fts (failure_mode_id, phrasing) VALUES (?, ?)",
                [(fm, p) for fm, p, _ in rows],
            )

    def write_vector_index(self, row: dict) -> None:
        with self.db:
            self.db.execute(
                """INSERT INTO vector_indexes (tier, usearch_version, metric, quantization, dims,
                     connectivity, expansion_add, expansion_search, count, recall_at_10, sha256, blob)
                   VALUES (:tier, :usearch_version, :metric, :quantization, :dims,
                     :connectivity, :expansion_add, :expansion_search, :count, :recall_at_10,
                     :sha256, :blob)""",
                row,
            )

    def finalize(self) -> None:
        self.db.commit()
        self.db.execute("ANALYZE")
        self.db.execute("VACUUM")
        self.db.close()
