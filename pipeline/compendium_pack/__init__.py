"""Compendium pack build pipeline.

Turns curated sources into a shipped knowledge pack: a single SQLite file
containing technique cards, chunks, documents, embeddings, prebuilt FTS5
indexes, and serialized usearch vector indexes.

Entry point: python -m compendium_pack --help
"""

SCHEMA_VERSION = 1
PACK_APPLICATION_ID = 0x434D5044  # 'CMPD'
