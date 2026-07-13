//! Advisor pipeline tests. The offline test is free and always runs; the live
//! test hits the Cohere API on the trial key (~10 calls) and is #[ignore]d —
//! run explicitly: cargo test --test advisor_test -- --ignored --nocapture

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;

use compendium_lib::engine::advisor::{self, types::Tier, Deps};
use compendium_lib::engine::cohere::CohereClient;
use compendium_lib::engine::{appdb, pack};

fn pack_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packs-out/rag-techniques.pack")
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "compendium-test-{tag}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_deps(with_key: bool) -> (Deps, Arc<Mutex<Vec<String>>>) {
    let loaded = pack::load_pack(&pack_path()).expect("pack loads");
    let conn = appdb::open(&temp_dir("appdb")).expect("appdb opens");
    let cohere = Arc::new(CohereClient::new());
    if with_key {
        let env = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env"),
        )
        .expect(".env with trial key required for live test");
        let key = env
            .lines()
            .find_map(|l| l.strip_prefix("COHERE_API_KEY_TRIAL="))
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .expect("COHERE_API_KEY_TRIAL set");
        cohere.set_key(Some(key.to_string()));
    }
    let stages: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let stages2 = stages.clone();
    let deps = Deps {
        packs: vec![loaded],
        cohere,
        appdb: Arc::new(Mutex::new(conn)),
        notify: Arc::new(move |event: &str, payload: Value| {
            if event == "advisor-progress" {
                if let Some(s) = payload["stage"].as_str() {
                    stages2.lock().push(s.to_string());
                }
            }
        }),
    };
    (deps, stages)
}

#[tokio::test]
async fn offline_degrades_to_local_advisory_and_persists() {
    let (deps, _stages) = make_deps(false);
    let turn = advisor::ask(
        deps.clone(),
        None,
        "my retriever returns chunks with the right keywords but they don't answer the question".into(),
        Some(Tier::Balanced),
    )
    .await
    .expect("ask succeeds without a key");

    let advisory = &turn.advisory;
    assert!(advisory.degraded, "no key must mean degraded mode");
    assert!(
        !advisory.recommendations.is_empty(),
        "local advisory still ranks techniques"
    );
    assert!(
        !advisory.failure_modes.is_empty(),
        "ontology keyword matching still works offline"
    );

    // turn + state persisted
    let conn = deps.appdb.lock();
    let turns: i64 = conn
        .query_row("SELECT COUNT(*) FROM turns WHERE conversation_id = ?1", [turn.conversation_id], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 2, "user + advisor turns persisted");
    let advisory_json: Option<String> = conn
        .query_row("SELECT advisory FROM turns WHERE id = ?1", [turn.advisor_turn_id], |r| r.get(0))
        .unwrap();
    assert!(advisory_json.is_some(), "advisory JSON stored for offline re-render");
}

#[tokio::test]
async fn smalltalk_never_costs_api_calls() {
    let (deps, _) = make_deps(false);
    let turn = advisor::ask(deps.clone(), None, "hello".into(), None).await.unwrap();
    assert!(!turn.advisory.answer_md.is_empty(), "meta reply present");
    let conn = deps.appdb.lock();
    let quota = appdb::quota_current(&conn).unwrap();
    assert_eq!(quota["chat_calls"], 0);
    assert_eq!(quota["embed_calls"], 0);
}

#[tokio::test]
#[ignore = "live Cohere calls (~10 on the trial key)"]
async fn live_end_to_end_dossier_and_followup() {
    let (deps, stages) = make_deps(true);

    // ---- turn 1: the classic ambiguous-looking symptom, Balanced tier
    let turn = advisor::ask(
        deps.clone(),
        None,
        "My RAG bot's retrieved chunks mention the right keywords but the generated answers \
keep missing the point. Recall at k=20 looks fine when I eyeball it — the good passage is \
usually in there somewhere, just buried around rank 10-15. I can re-index if needed."
            .into(),
        Some(Tier::Balanced),
    )
    .await
    .expect("live ask succeeds");

    let a = &turn.advisory;
    println!("--- tier={} route={:?} degraded={}", a.tier, a.route, a.degraded);
    println!("--- failure modes: {:?}", a.failure_modes.iter().map(|f| &f.id).collect::<Vec<_>>());
    println!(
        "--- recommendations: {:?}",
        a.recommendations
            .iter()
            .map(|r| format!("{} ({:.2})", r.slug, r.confidence))
            .collect::<Vec<_>>()
    );
    println!("--- citations: {} evidence: {}", a.citations.len(), a.evidence.len());
    println!("--- gaps: {:?}", a.gaps);
    println!("--- answer (first 600 chars):\n{}", a.answer_md.chars().take(600).collect::<String>());

    assert!(!a.degraded, "live run must not degrade");
    assert!(a.answer_md.len() > 400, "dossier prose expected");
    assert!(!a.recommendations.is_empty());
    assert!(!a.evidence.is_empty());
    assert!(
        !a.citations.is_empty(),
        "native span citations must come back in documents mode"
    );
    // citation spans must be within the answer and map to shipped evidence/cards
    for c in &a.citations {
        assert!(c.end <= a.answer_md.len() + 1, "span within answer");
        assert!(!c.doc_keys.is_empty());
    }
    // the buried-at-rank symptom should surface reranking or RSE among the top recs
    let top: Vec<&str> = a.recommendations.iter().take(5).map(|r| r.slug.as_str()).collect();
    assert!(
        top.iter().any(|s| s.contains("rerank") || s.contains("relevant_segment")),
        "expected reranking/RSE in top recommendations, got {top:?}"
    );
    let seen = stages.lock().clone();
    assert!(seen.contains(&"writing".to_string()), "progress events fired: {seen:?}");

    // ---- turn 2: follow-up referencing the first answer
    let turn2 = advisor::ask(
        deps.clone(),
        Some(turn.conversation_id),
        "between your top two suggestions, which is cheaper to run per query?".into(),
        Some(Tier::Quick),
    )
    .await
    .expect("follow-up succeeds");
    let a2 = &turn2.advisory;
    println!("--- follow-up route={:?} answer (first 300):\n{}", a2.route, a2.answer_md.chars().take(300).collect::<String>());
    assert!(!a2.answer_md.is_empty() || a2.clarifying_question.is_some());

    // ---- export: the dossier markdown bundle
    let conn = deps.appdb.lock();
    let advisory_json: String = conn
        .query_row("SELECT advisory FROM turns WHERE id = ?1", [turn.advisor_turn_id], |r| r.get(0))
        .unwrap();
    let parsed: advisor::types::Advisory = serde_json::from_str(&advisory_json).unwrap();
    let md = advisor::export::to_markdown(&parsed, "test", Some("problem statement"));
    assert!(md.contains("generator: Compendium"));
    assert!(md.contains("Evidence appendix"));
    assert!(md.contains("Attribution"));
    println!("--- export length: {} chars", md.len());
}
