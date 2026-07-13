//! Generates app/src/demo/fixture.json for the README demo GIF: one real
//! Balanced advisory (live Cohere calls on the trial key) plus the technique
//! cards and documents the demo UI can open. Run explicitly:
//!   cargo test --test dump_demo_fixture -- --ignored --nocapture

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{json, Value};

use compendium_lib::engine::advisor::{self, types::Tier, Deps};
use compendium_lib::engine::cohere::CohereClient;
use compendium_lib::engine::{appdb, pack};

const PROMPT: &str = "I'm building a RAG assistant over 10,000 legal PDFs where every answer \
must cite exact clauses — how should I design retrieval?";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[tokio::test]
#[ignore = "live Cohere calls; regenerates the demo fixture"]
async fn dump_demo_fixture() {
    let packs_dir = manifest_dir().join("../../packs-out");
    let rag = pack::load_pack(&packs_dir.join("rag-techniques.pack")).unwrap();
    let docs = pack::load_pack(&packs_dir.join("framework-docs.pack")).unwrap();
    let packs = vec![rag.clone(), docs.clone()];

    let env = std::fs::read_to_string(manifest_dir().join("../../.env")).unwrap();
    let key = env
        .lines()
        .find_map(|l| l.strip_prefix("COHERE_API_KEY_TRIAL="))
        .map(str::trim)
        .expect("trial key");
    let cohere = Arc::new(CohereClient::new());
    cohere.set_key(Some(key.to_string()));

    let tmp = std::env::temp_dir().join(format!(
        "compendium-fixture-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let conn = appdb::open(&tmp).unwrap();

    let deps = Deps {
        packs: packs.clone(),
        cohere,
        appdb: Arc::new(Mutex::new(conn)),
        notify: Arc::new(|_, _| {}),
    };

    let turn = advisor::ask(deps, None, PROMPT.into(), Some(Tier::Balanced))
        .await
        .expect("live advisory");
    let advisory = &turn.advisory;
    assert!(!advisory.degraded, "fixture must come from a full run");
    assert!(!advisory.citations.is_empty());

    // collect techniques + documents the demo can open: every recommendation's
    // card + notebook document, and every evidence chunk's document
    let mut techniques: HashMap<String, Value> = HashMap::new();
    let mut documents: HashMap<String, Value> = HashMap::new();

    let technique_json = |pack: &Arc<pack::LoadedPack>, slug: &str| -> Option<Value> {
        let conn = pack.conn.lock();
        let mut t = conn
            .query_row(
                "SELECT slug, title, one_liner, stage_id, complexity, problem_solved,
                        how_it_works, when_to_use, tradeoffs, key_dependencies, keywords,
                        summary, vendor_disclosure, document_id
                 FROM techniques WHERE slug = ?1",
                [slug],
                |r| {
                    Ok(json!({
                        "slug": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?,
                        "one_liner": r.get::<_, String>(2)?, "stage_id": r.get::<_, String>(3)?,
                        "complexity": r.get::<_, String>(4)?, "problem_solved": r.get::<_, String>(5)?,
                        "how_it_works": r.get::<_, String>(6)?, "when_to_use": r.get::<_, String>(7)?,
                        "tradeoffs": r.get::<_, String>(8)?, "key_dependencies": r.get::<_, String>(9)?,
                        "keywords": r.get::<_, String>(10)?, "summary": r.get::<_, String>(11)?,
                        "vendor_disclosure": r.get::<_, Option<String>>(12)?,
                        "document_id": r.get::<_, i64>(13)?,
                    }))
                },
            )
            .ok()?;
        let mut stmt = conn
            .prepare(
                "SELECT r.to_slug, r.relation, t.title FROM technique_relations r
                 JOIN techniques t ON t.slug = r.to_slug WHERE r.from_slug = ?1",
            )
            .ok()?;
        let rels: Vec<Value> = stmt
            .query_map([slug], |r| {
                Ok(json!({"slug": r.get::<_, String>(0)?, "relation": r.get::<_, String>(1)?, "title": r.get::<_, String>(2)?}))
            })
            .ok()?
            .filter_map(|r| r.ok())
            .collect();
        t["relations"] = Value::Array(rels);
        t["failure_modes"] = json!([]);
        Some(t)
    };

    let document_json = |pack: &Arc<pack::LoadedPack>, id: i64| -> Option<Value> {
        let conn = pack.conn.lock();
        conn.query_row(
            "SELECT kind, title, source_url, license_note, content FROM documents WHERE id = ?1",
            [id],
            |r| {
                Ok(json!({
                    "kind": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?,
                    "source_url": r.get::<_, String>(2)?, "license_note": r.get::<_, String>(3)?,
                    "content": r.get::<_, String>(4)?,
                    "attribution_html": pack.manifest.attribution_html,
                }))
            },
        )
        .ok()
    };

    let pack_by_id = |id: &str| packs.iter().find(|p| p.manifest.pack_id == id);

    for rec in &advisory.recommendations {
        if let Some(p) = pack_by_id(&rec.pack_id) {
            if let Some(t) = technique_json(p, &rec.slug) {
                let doc_id = t["document_id"].as_i64().unwrap();
                if let Some(d) = document_json(p, doc_id) {
                    documents.insert(format!("{}:{}", rec.pack_id, doc_id), d);
                }
                techniques.insert(format!("{}:{}", rec.pack_id, rec.slug), t);
            }
        }
    }
    for ev in &advisory.evidence {
        if let Some(p) = pack_by_id(&ev.pack_id) {
            let key = format!("{}:{}", ev.pack_id, ev.document_id);
            if !documents.contains_key(&key) {
                if let Some(d) = document_json(p, ev.document_id) {
                    documents.insert(key, d);
                }
            }
        }
    }

    let pack_infos: Vec<Value> = packs
        .iter()
        .map(|p| {
            let mut v = serde_json::to_value(&p.manifest).unwrap();
            v["healed"] = json!(false);
            v["path"] = json!("");
            v
        })
        .collect();

    let fixture = json!({
        "packs": pack_infos,
        "user_message": PROMPT,
        "advisory_turn": serde_json::to_value(&turn).unwrap(),
        "techniques": techniques,
        "documents": documents,
    });

    let out = manifest_dir().join("../src/demo/fixture.json");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    std::fs::write(&out, serde_json::to_string(&fixture).unwrap()).unwrap();
    println!(
        "fixture written: {} ({} techniques, {} documents, {} citations, {} KB)",
        out.display(),
        fixture["techniques"].as_object().unwrap().len(),
        fixture["documents"].as_object().unwrap().len(),
        advisory.citations.len(),
        std::fs::metadata(&out).unwrap().len() / 1024
    );
}
