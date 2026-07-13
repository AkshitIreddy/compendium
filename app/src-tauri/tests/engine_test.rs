//! Engine integration tests against the real built pack (packs-out/).
//! No network: dense search is exercised with a stored chunk vector as the
//! query, which must retrieve its own chunk first.

use std::path::PathBuf;

use compendium_lib::engine::pack::{blob_to_f32, load_pack};
use compendium_lib::engine::search::{fts_query, search, SearchOptions};

fn pack_path() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs-out/rag-techniques.pack")
        .canonicalize()
        .expect("packs-out/rag-techniques.pack missing — run the pipeline build first");
    p
}

#[test]
fn pack_loads_fast_with_valid_manifest() {
    let start = std::time::Instant::now();
    let pack = load_pack(&pack_path()).expect("pack should load");
    let elapsed = start.elapsed();

    assert_eq!(pack.manifest.pack_id, "rag-techniques");
    assert_eq!(pack.manifest.embedding_dims, 1024);
    assert!(!pack.manifest.attribution_html.is_empty());
    assert!(!pack.healed, "shipped index should load without a heal rebuild");
    assert_eq!(pack.card_slugs.len(), 44);
    assert_eq!(pack.cards_index.size(), 44);
    assert!(pack.chunks_index.size() > 500);
    assert!(!pack.phrasings.phrasings.is_empty());
    // Phase 3 gate: pack load well under a second (cold start budget).
    assert!(elapsed.as_millis() < 1000, "pack load took {elapsed:?}");
}

#[test]
fn bm25_only_search_finds_reranking() {
    let pack = load_pack(&pack_path()).unwrap();
    let resp = search(
        &[pack],
        "cross encoder reranking relevance scoring after retrieval",
        None,
        SearchOptions::default(),
    )
    .unwrap();
    assert!(!resp.dense_used);
    assert!(!resp.cards.is_empty(), "BM25 should return cards");
    let top_slugs: Vec<&str> = resp.cards.iter().take(6).map(|c| c.slug.as_str()).collect();
    assert!(
        top_slugs.iter().any(|s| s.contains("reranking")),
        "expected a reranking technique in the top cards, got {top_slugs:?}"
    );
}

#[test]
fn dense_search_self_retrieves_and_fuses() {
    let pack = load_pack(&pack_path()).unwrap();

    // Use a real stored chunk vector as the query: that chunk must come first.
    let (chunk_id, technique, query_vec) = {
        let conn = pack.conn.lock();
        let (id, slug, blob): (i64, Option<String>, Vec<u8>) = conn
            .query_row(
                "SELECT c.id, c.technique_slug, e.vector FROM chunks c
                 JOIN chunk_embeddings e ON e.chunk_id = c.id
                 WHERE c.technique_slug = 'fusion_retrieval' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        (id, slug.unwrap(), blob_to_f32(&blob, 1024).unwrap())
    };

    let resp = search(
        &[pack],
        "combine bm25 keyword search with vector similarity",
        Some(&query_vec),
        SearchOptions::default(),
    )
    .unwrap();
    assert!(resp.dense_used);
    assert_eq!(
        resp.chunks.first().map(|c| c.chunk_id),
        Some(chunk_id),
        "self-query must rank its own chunk first"
    );
    assert_eq!(resp.chunks[0].technique_slug.as_deref(), Some(technique.as_str()));
    assert!(
        resp.chunks[0].exact_cosine.unwrap() > 0.999,
        "exact re-score of the identical vector must be ~1.0"
    );
    // Graph expansion should annotate at least one related technique.
    assert!(
        resp.cards.iter().any(|c| c.expanded_from.is_some()),
        "expected 1-hop graph-expanded cards"
    );
}

#[test]
fn fts_query_sanitizes_natural_language() {
    let q = fts_query("Why won't my retriever find the right chunks?!").unwrap();
    assert!(q.contains("\"retriever\""));
    assert!(!q.contains("why"));
    assert!(fts_query("a an of").is_none());
    // punctuation-heavy input must not produce FTS syntax errors
    assert!(fts_query("c++ \"quoted\" AND (weird) NEAR/3").is_some());
}

#[test]
fn search_runs_fast_locally() {
    let pack = load_pack(&pack_path()).unwrap();
    let conn_vec = {
        let conn = pack.conn.lock();
        let blob: Vec<u8> = conn
            .query_row("SELECT vector FROM chunk_embeddings WHERE chunk_id = 100", [], |r| r.get(0))
            .unwrap();
        blob_to_f32(&blob, 1024).unwrap()
    };
    let start = std::time::Instant::now();
    for _ in 0..10 {
        search(
            &[pack.clone()],
            "my answers are incomplete and missing context",
            Some(&conn_vec),
            SearchOptions::default(),
        )
        .unwrap();
    }
    let per_query = start.elapsed() / 10;
    assert!(
        per_query.as_millis() < 100,
        "hybrid search should be well under 100ms locally, took {per_query:?}"
    );
}
