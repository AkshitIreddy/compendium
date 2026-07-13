//! Multi-pack engine test: the card-less framework-docs pack loads alongside
//! rag-techniques and contributes chunks to hybrid search.

use std::path::PathBuf;

use compendium_lib::engine::pack::load_pack;
use compendium_lib::engine::search::{search, SearchOptions};

fn packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packs-out")
}

#[test]
fn docs_pack_loads_without_cards() {
    let pack = load_pack(&packs_dir().join("framework-docs.pack")).expect("docs pack loads");
    assert_eq!(pack.manifest.pack_id, "framework-docs");
    assert_eq!(pack.card_slugs.len(), 0);
    assert_eq!(pack.cards_index.size(), 0);
    assert!(pack.chunks_index.size() > 1500);
    assert!(!pack.healed);
    assert!(pack.phrasings.phrasings.is_empty());
}

#[test]
fn multi_pack_bm25_search_reaches_docs_chunks() {
    let rag = load_pack(&packs_dir().join("rag-techniques.pack")).unwrap();
    let docs = load_pack(&packs_dir().join("framework-docs.pack")).unwrap();

    let resp = search(
        &[rag, docs],
        "how do I add persistent memory to a langgraph agent with checkpointers",
        None,
        SearchOptions::default(),
    )
    .unwrap();

    assert!(
        resp.chunks.iter().any(|c| c.pack_id == "framework-docs"),
        "expected framework-docs chunks in results, got packs: {:?}",
        resp.chunks.iter().map(|c| &c.pack_id).take(8).collect::<Vec<_>>()
    );
    // technique cards still come from the rag pack only, and nothing crashes
    assert!(resp.cards.iter().all(|c| c.pack_id == "rag-techniques"));
}
