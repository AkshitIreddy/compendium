//! Retrieval-quality eval against eval/retrieval_golden.json, plus the dump
//! step that feeds the DeepEval generation eval (eval/deepeval_eval.py).
//!
//! Both tests are #[ignore]d because they spend trial-key API calls:
//!   retrieval_metrics_on_golden_set — 1 batched embed call
//!   dump_generation_inputs          — ~6 Balanced advisories (~35 calls)
//!
//! Run:  cargo test --test eval_retrieval -- --ignored --nocapture

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};

use compendium_lib::engine::advisor::{self, types::Tier, Deps};
use compendium_lib::engine::cohere::CohereClient;
use compendium_lib::engine::pack::load_pack;
use compendium_lib::engine::search::{search, SearchOptions};
use compendium_lib::engine::appdb;

fn repo_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn trial_key() -> String {
    let env = std::fs::read_to_string(repo_path(".env")).expect(".env with trial key required");
    env.lines()
        .find_map(|l| l.strip_prefix("COHERE_API_KEY_TRIAL="))
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .expect("COHERE_API_KEY_TRIAL set")
        .to_string()
}

#[derive(Deserialize)]
struct Golden {
    cases: Vec<GoldenCase>,
}

#[derive(Deserialize)]
struct GoldenCase {
    query: String,
    expected: Vec<String>,
}

/// Hit@5, Recall@10, MRR over the golden set, through the same hybrid search
/// the app runs (dense + BM25 + RRF + graph expansion). One embed call total.
#[tokio::test]
#[ignore = "one live embed call on the trial key"]
async fn retrieval_metrics_on_golden_set() {
    let golden: Golden = serde_json::from_str(
        &std::fs::read_to_string(repo_path("eval/retrieval_golden.json")).unwrap(),
    )
    .unwrap();
    let pack = load_pack(&repo_path("packs-out/rag-techniques.pack")).expect("pack loads");

    let cohere = CohereClient::new();
    cohere.set_key(Some(trial_key()));
    let queries: Vec<String> = golden.cases.iter().map(|c| c.query.clone()).collect();
    let vecs = cohere.embed_queries(&queries).await.expect("batched embed");

    let n = golden.cases.len() as f64;
    let (mut hits5, mut mrr_sum, mut recall_sum) = (0usize, 0.0f64, 0.0f64);
    let mut rows: Vec<Value> = Vec::new();

    println!("\n{:-<100}", "");
    for (case, vec) in golden.cases.iter().zip(&vecs) {
        let resp = search(&[pack.clone()], &case.query, Some(vec), SearchOptions::default())
            .expect("search");
        let ranked: Vec<&str> = resp.cards.iter().map(|c| c.slug.as_str()).collect();

        let is_expected = |s: &str| case.expected.iter().any(|e| e == s);
        let hit5 = ranked.iter().take(5).any(|s| is_expected(s));
        let first_rank = ranked.iter().position(|s| is_expected(s));
        let rr = first_rank.map(|r| 1.0 / (r as f64 + 1.0)).unwrap_or(0.0);
        let found10 = case
            .expected
            .iter()
            .filter(|e| ranked.iter().take(10).any(|s| *s == e.as_str()))
            .count();
        let recall10 = found10 as f64 / case.expected.len() as f64;

        hits5 += hit5 as usize;
        mrr_sum += rr;
        recall_sum += recall10;

        println!(
            "{} hit@5={} rr={:.2} recall@10={:.2}\n   q: {}\n   top5: {:?}",
            if hit5 { "PASS" } else { "MISS" },
            hit5,
            rr,
            recall10,
            case.query,
            &ranked[..ranked.len().min(5)],
        );
        rows.push(json!({
            "query": case.query, "expected": case.expected,
            "top10": ranked.iter().take(10).collect::<Vec<_>>(),
            "hit_at_5": hit5, "reciprocal_rank": rr, "recall_at_10": recall10,
        }));
    }

    let (hit_rate, mrr, recall) = (hits5 as f64 / n, mrr_sum / n, recall_sum / n);
    println!("{:-<100}", "");
    println!(
        "AGGREGATE over {} cases:  hit@5 = {:.3}   MRR = {:.3}   recall@10 = {:.3}",
        golden.cases.len(),
        hit_rate,
        mrr,
        recall
    );

    std::fs::create_dir_all(repo_path("eval/out")).unwrap();
    std::fs::write(
        repo_path("eval/out/retrieval_results.json"),
        serde_json::to_string_pretty(&json!({
            "cases": rows,
            "aggregate": { "hit_at_5": hit_rate, "mrr": mrr, "recall_at_10": recall },
        }))
        .unwrap(),
    )
    .unwrap();

    // Regression floors — set below the recorded baseline (see eval/README.md)
    // so honest drift fails loudly while normal variance doesn't.
    assert!(hit_rate >= 0.75, "hit@5 regressed: {hit_rate:.3}");
    assert!(mrr >= 0.55, "MRR regressed: {mrr:.3}");
    assert!(recall >= 0.60, "recall@10 regressed: {recall:.3}");
}

#[derive(Deserialize)]
struct Questions {
    questions: Vec<String>,
}

/// Runs each generation-eval question through the real advisor (Balanced) and
/// dumps (question, answer, contexts) for RAGAS scoring in Python.
#[tokio::test]
#[ignore = "live advisor calls (~35 on the trial key); writes eval/out/generation_inputs.json"]
async fn dump_generation_inputs() {
    let qs: Questions = serde_json::from_str(
        &std::fs::read_to_string(repo_path("eval/generation_questions.json")).unwrap(),
    )
    .unwrap();

    let packs = vec![
        load_pack(&repo_path("packs-out/rag-techniques.pack")).expect("rag pack"),
        load_pack(&repo_path("packs-out/framework-docs.pack")).expect("docs pack"),
    ];
    let cohere = Arc::new(CohereClient::new());
    cohere.set_key(Some(trial_key()));
    let tmp = std::env::temp_dir().join(format!(
        "compendium-eval-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let conn = appdb::open(&tmp).unwrap();
    // Eval scores answers, not the clarify UX — take the best interpretation
    // instead of asking (the same toggle the Settings panel exposes).
    appdb::setting_set(&conn, "advisor", &json!({ "clarifying_questions": false })).unwrap();
    let deps = Deps {
        packs,
        cohere,
        appdb: Arc::new(Mutex::new(conn)),
        notify: Arc::new(|_, _| {}),
    };

    let mut rows: Vec<Value> = Vec::new();
    for (i, q) in qs.questions.iter().enumerate() {
        println!("[{}/{}] {}", i + 1, qs.questions.len(), q);
        let turn = advisor::ask(deps.clone(), None, q.clone(), Some(Tier::Balanced))
            .await
            .expect("advisory");
        let a = &turn.advisory;
        assert!(!a.degraded, "live eval run must not degrade (question {i})");
        assert!(
            !a.answer_md.is_empty(),
            "question {i} produced an empty answer (clarify: {:?})",
            a.clarifying_question
        );
        let contexts: Vec<&str> = a.evidence.iter().map(|e| e.text.as_str()).collect();
        println!(
            "    -> {} chars, {} evidence chunks, {} citations",
            a.answer_md.len(),
            contexts.len(),
            a.citations.len()
        );
        rows.push(json!({
            "question": q,
            "answer": a.answer_md,
            "contexts": contexts,
        }));
    }

    std::fs::create_dir_all(repo_path("eval/out")).unwrap();
    std::fs::write(
        repo_path("eval/out/generation_inputs.json"),
        serde_json::to_string_pretty(&rows).unwrap(),
    )
    .unwrap();
    println!("wrote eval/out/generation_inputs.json ({} samples)", rows.len());
}
