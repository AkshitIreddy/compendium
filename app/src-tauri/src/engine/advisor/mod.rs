//! The advisor pipeline: a fixed state machine (S0–S9) around the local
//! retrieval engine and the Cohere API. Tiers are configuration, not code
//! paths: Quick skips planning/grading/critic, Deep adds corrective loops.
//!
//! Degradation is designed, not accidental: any API failure downgrades the
//! turn to a local advisory (ranked techniques + evidence, no prose) rather
//! than losing the turn.

pub mod context;
pub mod export;
pub mod prompts;
pub mod types;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::engine::pack::LoadedPack;
use crate::engine::search::{self, SearchOptions};
use crate::engine::{appdb, cohere::CohereClient};
use crate::error::{Error, Result};
use types::*;

/// Everything the pipeline needs, decoupled from the Tauri runtime so the
/// whole advisor is testable headlessly. `notify` receives (event, payload)
/// — the command layer forwards it to webview events.
#[derive(Clone)]
pub struct Deps {
    pub packs: Vec<Arc<LoadedPack>>,
    pub cohere: Arc<CohereClient>,
    pub appdb: Arc<Mutex<Connection>>,
    pub notify: Arc<dyn Fn(&str, Value) + Send + Sync>,
}

pub const DEFAULT_SYNTH_MODEL: &str = "command-a-03-2025";
pub const DEFAULT_UTILITY_MODEL: &str = "command-r7b-12-2024";
const RERANK_MODEL: &str = "rerank-v4.0-pro";

/// Advisor configuration resolved from settings each turn.
#[derive(Clone)]
struct Config {
    tier: Tier,
    clarifying_questions: bool,
    synth_model: String,
    utility_model: String,
    rerank_model: String,
}

#[derive(Default)]
struct CallCounts {
    embed: u32,
    chat: u32,
    rerank: u32,
}

struct CardCandidate {
    slug: String,
    pack_id: String,
    title: String,
    one_liner: String,
    stage_id: String,
    complexity: String,
    vendor_disclosure: Option<String>,
    summary: String,
    fusion_score: f64,
    arms_hit: usize,
    best_rerank: Option<f64>,
    expanded_from: Option<(String, String)>,
    matched_fms: Vec<String>,
}

pub async fn ask(
    deps: Deps,
    conversation_id: Option<i64>,
    message: String,
    tier_override: Option<Tier>,
) -> Result<AdvisorTurn> {
    let t0 = Instant::now();
    let packs = deps.packs.clone();
    if packs.is_empty() {
        return Err(Error::Pack("no knowledge packs loaded".into()));
    }

    let config = load_config(&deps, tier_override);
    let mut counts = CallCounts::default();

    // ---- conversation setup + persist the user turn
    let (conv_id, conv_state, view, user_turn_id) = {
        let conn = deps.appdb.lock();
        let conv_id = match conversation_id {
            Some(id) => id,
            None => context::conversation_create(&conn)?,
        };
        let conv_state = context::state_load(&conn, conv_id)?;
        let view = context::history(&conn, conv_id)?;
        let user_turn_id = context::insert_turn(&conn, conv_id, "user", &message, None, None)?;
        (conv_id, conv_state, view, user_turn_id)
    };
    let emit = |stage: &str| {
        (deps.notify)("advisor-progress", json!({"conversation_id": conv_id, "stage": stage}));
    };

    // ---- deterministic pre-filter: pure smalltalk never costs an API call
    if is_smalltalk(&message) {
        let advisory = meta_advisory(&packs);
        return finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, None).await;
    }

    // ---- degraded path: no key at all
    if !deps.cohere.has_key() {
        emit("retrieving");
        let advisory = local_advisory(&packs, &message, None, &conv_state, true);
        return finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, None).await;
    }

    // ---- S1: intake (router + rewriter + constraints + ontology confirmation)
    emit("analyzing");
    let ontology_hint = ontology_hint(&packs);
    let analysis = match intake(&deps.cohere, &config, &ontology_hint, &conv_state, &view, &message).await {
        Ok(a) => {
            counts.chat += 1;
            a
        }
        Err(Error::QuotaExhausted) => {
            let advisory = local_advisory(&packs, &message, None, &conv_state, true);
            return finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, None).await;
        }
        Err(_) => fallback_analysis(&message, &conv_state),
    };

    if analysis.route == Route::Meta {
        let advisory = meta_advisory(&packs);
        return finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, Some(&analysis)).await;
    }
    if config.clarifying_questions {
        if let Some(q) = analysis.clarifying_question.clone() {
            let advisory = clarify_advisory(&analysis, &q);
            return finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, Some(&analysis)).await;
        }
    }

    // ---- S2: plan (Quick fabricates one locally)
    let plan = if config.tier == Tier::Quick {
        default_plan(&analysis)
    } else {
        emit("planning");
        match planner(&deps.cohere, &config, &conv_state, &analysis).await {
            Ok(p) => {
                counts.chat += 1;
                p
            }
            Err(_) => default_plan(&analysis),
        }
    };

    // ---- embed all retrieval arms in one batch call
    emit("retrieving");
    let mut arms: Vec<String> = vec![analysis.standalone_query.clone()];
    arms.extend(plan.sub_questions.iter().cloned());
    arms.extend(plan.rewrites.iter().cloned());
    if config.tier == Tier::Deep {
        // ontology fan-out: failure-mode names as extra arms (free locally,
        // one shared embed batch)
        arms.extend(failure_mode_names(&packs, &analysis.failure_mode_ids));
    }
    arms.dedup();
    arms.truncate(9);

    let arm_vecs = match deps.cohere.embed_queries(&arms).await {
        Ok(v) => {
            counts.embed += 1;
            Some(v)
        }
        // Quota AND hard API failures both degrade — never lose the turn.
        Err(e) => {
            eprintln!("query embedding failed, degrading to local advisory: {e}");
            None
        }
    };
    let Some(arm_vecs) = arm_vecs else {
        let advisory = local_advisory(&packs, &message, None, &conv_state, true);
        return finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, Some(&analysis)).await;
    };

    // ---- S3: retrieval fan-out + cross-arm merge (all local, ~10ms per arm)
    let (mut cards, mut evidence_pool, fm_hits) =
        retrieve_merged(&packs, &arms, &arm_vecs, &analysis)?;

    // ---- S4: rerank evidence + adaptive selection
    emit("ranking");
    apply_rerank(&deps.cohere, &config, &analysis.standalone_query, &mut evidence_pool, &mut counts).await;
    let mut evidence = select_evidence(evidence_pool, &cards);
    attach_rerank_to_cards(&mut cards, &evidence);

    // ---- S5: sufficiency gate (Balanced+)
    let mut sufficiency: Vec<SufficiencyVerdict> = Vec::new();
    let mut gaps: Option<String> = None;
    if config.tier != Tier::Quick && !plan.sub_questions.is_empty() {
        emit("grading");
        match grade_sufficiency(&deps.cohere, &config, &plan.sub_questions, &evidence).await {
            Ok(v) => {
                counts.chat += 1;
                // corrective local re-query for insufficient sub-questions
                let missing: Vec<&SufficiencyVerdict> = v.iter().filter(|x| !x.sufficient).collect();
                if !missing.is_empty() {
                    corrective_requery(&packs, &missing, &mut evidence);
                    let still: Vec<String> = missing
                        .iter()
                        .filter_map(|m| m.missing.clone())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !still.is_empty() {
                        gaps = Some(format!(
                            "The knowledge packs have thin coverage on: {}.",
                            still.join("; ")
                        ));
                    }
                }
                sufficiency = v;
            }
            Err(_) => {}
        }
    }

    // ---- S6+S7: evidence assembly + grounded synthesis with span citations
    emit("writing");
    let documents = build_documents(&evidence, &cards);
    let relations_hint = relations_hint(&packs, &cards);
    let synth = synthesize(
        &deps.cohere,
        &config,
        &conv_state,
        &analysis,
        &plan,
        &documents,
        &relations_hint,
    )
    .await;

    let (answer_md, citations) = match synth {
        Ok((text, raw_citations)) => {
            counts.chat += 1;
            let cites = parse_citations(&raw_citations, &documents);
            (text, cites)
        }
        Err(e) => {
            // Degrade: local advisory but keep the retrieval work we did.
            eprintln!("synthesis failed, degrading to local advisory: {e}");
            let mut advisory = local_advisory(&packs, &message, Some(&cards), &conv_state, true);
            advisory.evidence = evidence;
            advisory.failure_modes = fm_hits;
            return finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, Some(&analysis)).await;
        }
    };

    // ---- S8: verification — local citation integrity, then the critic
    emit("verifying");
    let valid_keys: HashSet<&str> = documents.iter().filter_map(|d| d["id"].as_str()).collect();
    let citations: Vec<SpanCitation> = citations
        .into_iter()
        .filter(|c| c.doc_keys.iter().all(|k| valid_keys.contains(k.as_str())))
        .collect();

    let mut judgments: HashMap<String, (bool, Option<String>)> = HashMap::new();
    if config.tier != Tier::Quick {
        if let Ok(j) = critic(&deps.cohere, &config, &answer_md, &cards, &evidence).await {
            counts.chat += 1;
            judgments = j;
        }
    }

    let recommendations = build_recommendations(&cards, &judgments, &packs);

    // ---- S9: assemble, persist, cache evidence for follow-ups
    let advisory = Advisory {
        tier: config.tier.as_str().into(),
        route: analysis.route.clone(),
        clarifying_question: None,
        diagnosis_md: diagnosis_md(&analysis, &fm_hits),
        failure_modes: fm_hits,
        recommendations,
        answer_md,
        citations,
        evidence,
        gaps,
        sufficiency,
        degraded: false,
        attribution_html: packs.iter().map(|p| p.manifest.attribution_html.clone()).collect(),
    };

    finish(&deps, conv_id, user_turn_id, advisory, &config, counts, t0, Some(&analysis)).await
}

// --------------------------------------------------------------- persistence

#[allow(clippy::too_many_arguments)]
async fn finish(
    deps: &Deps,
    conv_id: i64,
    user_turn_id: i64,
    advisory: Advisory,
    config: &Config,
    counts: CallCounts,
    t0: Instant,
    analysis: Option<&TurnAnalysis>,
) -> Result<AdvisorTurn> {
    let advisory_json = serde_json::to_value(&advisory).map_err(|e| Error::Internal(e.to_string()))?;

    let (advisor_turn_id, needs_title, needs_fold) = {
        let conn = deps.appdb.lock();
        let content = if advisory.answer_md.is_empty() {
            advisory
                .clarifying_question
                .clone()
                .unwrap_or_else(|| advisory.diagnosis_md.clone())
        } else {
            advisory.answer_md.clone()
        };
        let citations_json = serde_json::to_value(&advisory.citations).ok();
        let turn_id = context::insert_turn(
            &conn,
            conv_id,
            "advisor",
            &content,
            Some(&advisory_json),
            citations_json.as_ref(),
        )?;
        context::write_trace(
            &conn,
            turn_id,
            &json!({
                "route": format!("{:?}", advisory.route),
                "tier": advisory.tier,
                "standalone_query": analysis.map(|a| a.standalone_query.clone()),
                "retrieval": {
                    "evidence": advisory.evidence.iter().map(|e| &e.doc_key).collect::<Vec<_>>(),
                    "recommended": advisory.recommendations.iter().map(|r| &r.slug).collect::<Vec<_>>(),
                },
                "tokens": {},
                "models": {"synth": config.synth_model, "utility": config.utility_model},
                "latency_ms": t0.elapsed().as_millis() as i64,
                "validation": {
                    "citations": advisory.citations.len(),
                    "degraded": advisory.degraded,
                },
            }),
        )?;
        appdb::quota_bump(&conn, counts.embed, counts.chat, counts.rerank)?;

        // conversation state update
        let mut cstate = context::state_load(&conn, conv_id)?;
        if let Some(a) = analysis {
            if a.route == Route::NewProblem || cstate.pinned_problem.is_none() {
                // Pinned problem = the user's own words, verbatim.
                let user_msg: String = conn.query_row(
                    "SELECT content_md FROM turns WHERE id = ?1",
                    [user_turn_id],
                    |r| r.get(0),
                )?;
                cstate.pinned_problem = Some(user_msg);
            }
            for c in &a.constraints {
                if !cstate.constraints.contains(c) {
                    cstate.constraints.push(c.clone());
                }
            }
        }
        cstate.open_question = advisory.clarifying_question.clone();
        if !advisory.recommendations.is_empty() {
            cstate.advisor_state = advisory
                .recommendations
                .iter()
                .map(|r| AdvisorStateEntry {
                    slug: r.slug.clone(),
                    pack_id: r.pack_id.clone(),
                    verdict: "recommended".into(),
                })
                .collect();
        }
        if !advisory.evidence.is_empty() {
            cstate.candidate_pool = advisory.evidence.iter().map(|e| e.doc_key.clone()).collect();
        }
        context::state_save(&conn, conv_id, &cstate)?;

        let title = context::conversation_title(&conn, conv_id)?;
        let view = context::history(&conn, conv_id)?;
        (turn_id, title == "New conversation", context::needs_summary_fold(&view))
    };

    // ---- async post-turn work: title + summary fold (never blocks the reply)
    if (needs_title || needs_fold) && deps.cohere.has_key() {
        let deps2 = deps.clone();
        let utility_model = config.utility_model.clone();
        tokio::spawn(async move {
            post_turn_maintenance(deps2, conv_id, utility_model, needs_title, needs_fold).await;
        });
    }

    (deps.notify)("advisor-progress", json!({"conversation_id": conv_id, "stage": "done"}));
    Ok(AdvisorTurn { conversation_id: conv_id, user_turn_id, advisor_turn_id, advisory })
}

async fn post_turn_maintenance(
    deps: Deps,
    conv_id: i64,
    utility_model: String,
    needs_title: bool,
    needs_fold: bool,
) {
    if needs_title {
        let pinned = {
            let conn = deps.appdb.lock();
            context::state_load(&conn, conv_id).ok().and_then(|s| s.pinned_problem)
        };
        if let Some(problem) = pinned {
            let messages = json!([
                {"role": "system", "content": prompts::title_system()},
                {"role": "user", "content": problem.chars().take(1500).collect::<String>()},
            ]);
            match deps
                .cohere
                .chat_structured(&utility_model, messages, prompts::title_schema(), 0.3)
                .await
            {
                Ok(v) => {
                    if let Some(title) = v["title"].as_str() {
                        let conn = deps.appdb.lock();
                        let _ = context::conversation_set_title(&conn, conv_id, title.trim());
                        let _ = appdb::quota_bump(&conn, 0, 1, 0);
                        (deps.notify)("conversation-titled", json!({"conversation_id": conv_id, "title": title.trim()}));
                    }
                }
                Err(_) => {
                    // fallback: truncated problem statement
                    let conn = deps.appdb.lock();
                    if let Ok(s) = context::state_load(&conn, conv_id) {
                        if let Some(p) = s.pinned_problem {
                            let title: String = p.chars().take(48).collect();
                            let _ = context::conversation_set_title(&conn, conv_id, title.trim());
                        }
                    }
                }
            }
        }
    }
    if needs_fold {
        let (unfolded, prev_summary) = {
            let conn = deps.appdb.lock();
            match context::history(&conn, conv_id) {
                Ok(v) => (v.unfolded, v.summary),
                Err(_) => return,
            }
        };
        if unfolded.is_empty() {
            return;
        }
        let last_id = unfolded.last().map(|(id, _, _)| *id).unwrap_or(0);
        let body: String = unfolded
            .iter()
            .map(|(_, role, content)| {
                format!("{}: {}\n", role, content.chars().take(2000).collect::<String>())
            })
            .collect();
        let messages = json!([
            {"role": "system", "content": prompts::summary_system()},
            {"role": "user", "content": format!(
                "Current summary:\n{}\n\nTurns to fold in:\n{}",
                prev_summary.unwrap_or_else(|| "(none)".into()),
                body
            )},
        ]);
        if let Ok(v) = deps
            .cohere
            .chat_structured(&utility_model, messages, prompts::summary_schema(), 0.2)
            .await
        {
            if let Some(summary) = v["summary"].as_str() {
                let conn = deps.appdb.lock();
                let _ = context::write_summary(&conn, conv_id, last_id, summary);
                let _ = appdb::quota_bump(&conn, 0, 1, 0);
            }
        }
    }
}

// ------------------------------------------------------------------ stages

async fn intake(
    cohere: &CohereClient,
    config: &Config,
    ontology_hint: &str,
    conv_state: &ConversationState,
    view: &context::HistoryView,
    message: &str,
) -> Result<TurnAnalysis> {
    let system = prompts::intake_system(ontology_hint);
    let messages = context::intake_messages(&system, conv_state, view, message);
    let v = cohere
        .chat_structured(&config.utility_model, messages, prompts::intake_schema(), 0.1)
        .await?;
    serde_json::from_value(v).map_err(|e| Error::Internal(format!("intake parse: {e}")))
}

fn fallback_analysis(message: &str, conv_state: &ConversationState) -> TurnAnalysis {
    TurnAnalysis {
        route: if conv_state.pinned_problem.is_some() {
            Route::FollowupRetrieve
        } else {
            Route::NewProblem
        },
        intent: None,
        standalone_query: message.to_string(),
        constraints: conv_state.constraints.clone(),
        failure_mode_ids: Vec::new(),
        context_symptom: None,
        clarifying_question: None,
    }
}

async fn planner(
    cohere: &CohereClient,
    config: &Config,
    conv_state: &ConversationState,
    analysis: &TurnAnalysis,
) -> Result<Plan> {
    let mut user = format!("Request (standalone): {}\n", analysis.standalone_query);
    if let Some(intent) = &analysis.intent {
        user.push_str(&format!("Intent: {intent}\n"));
    }
    if let Some(p) = &conv_state.pinned_problem {
        user.push_str(&format!("\nOriginal statement (the user's words):\n{p}\n"));
    }
    if !analysis.constraints.is_empty() {
        user.push_str(&format!("\nConstraints: {}\n", analysis.constraints.join("; ")));
    }
    if let Some(cs) = &analysis.context_symptom {
        user.push_str(&format!("\nContext symptom classification: {cs}\n"));
    }
    let messages = json!([
        {"role": "system", "content": prompts::planner_system()},
        {"role": "user", "content": user},
    ]);
    let v = cohere
        .chat_structured(&config.synth_model, messages, prompts::planner_schema(), 0.4)
        .await?;
    serde_json::from_value(v).map_err(|e| Error::Internal(format!("plan parse: {e}")))
}

fn default_plan(analysis: &TurnAnalysis) -> Plan {
    let sections = if analysis.intent.as_deref() == Some("build") {
        vec![
            "Recommended approach".into(),
            "Techniques to use".into(),
            "How they fit together".into(),
        ]
    } else {
        vec![
            "Diagnosis".into(),
            "Recommended techniques".into(),
            "How they combine".into(),
        ]
    };
    Plan {
        sections,
        sub_questions: vec![analysis.standalone_query.clone()],
        rewrites: Vec::new(),
    }
}

type MergedRetrieval = (Vec<CardCandidate>, Vec<Evidence>, Vec<AdvisoryFailureMode>);

fn retrieve_merged(
    packs: &[Arc<LoadedPack>],
    arms: &[String],
    arm_vecs: &[Vec<f32>],
    analysis: &TurnAnalysis,
) -> Result<MergedRetrieval> {
    let mut card_best: HashMap<String, CardCandidate> = HashMap::new();
    let mut chunk_best: HashMap<String, Evidence> = HashMap::new();
    let mut chunk_cos: HashMap<String, f32> = HashMap::new();
    let mut fm_best: HashMap<String, AdvisoryFailureMode> = HashMap::new();

    for (arm_idx, arm) in arms.iter().enumerate() {
        let vec = arm_vecs.get(arm_idx).map(|v| v.as_slice());
        let resp = search::search(packs, arm, vec, SearchOptions::default())?;

        for hit in resp.cards {
            let key = format!("{}:{}", hit.pack_id, hit.slug);
            let entry = card_best.entry(key).or_insert_with(|| CardCandidate {
                slug: hit.slug.clone(),
                pack_id: hit.pack_id.clone(),
                title: hit.title.clone(),
                one_liner: hit.one_liner.clone(),
                stage_id: hit.stage_id.clone(),
                complexity: hit.complexity.clone(),
                vendor_disclosure: hit.vendor_disclosure.clone(),
                summary: String::new(),
                fusion_score: 0.0,
                arms_hit: 0,
                best_rerank: None,
                expanded_from: hit.expanded_from.clone(),
                matched_fms: Vec::new(),
            });
            entry.fusion_score += hit.score;
            entry.arms_hit += 1;
            if entry.expanded_from.is_some() && hit.expanded_from.is_none() {
                entry.expanded_from = None; // directly retrieved by another arm
            }
        }
        for hit in resp.chunks.into_iter().take(20) {
            let key = format!("{}:chunk:{}", hit.pack_id, hit.chunk_id);
            let cos = hit.exact_cosine.unwrap_or(0.0);
            if chunk_cos.get(&key).copied().unwrap_or(f32::MIN) < cos {
                chunk_cos.insert(key.clone(), cos);
                chunk_best.insert(
                    key.clone(),
                    Evidence {
                        doc_key: key,
                        pack_id: hit.pack_id,
                        chunk_id: hit.chunk_id,
                        document_id: hit.document_id,
                        technique_slug: hit.technique_slug,
                        heading_path: hit.heading_path,
                        text: hit.display_text,
                        location: hit.location,
                        rerank_score: None,
                    },
                );
            }
        }
        for fm in resp.failure_modes {
            let entry = fm_best.entry(fm.id.clone()).or_insert(AdvisoryFailureMode {
                id: fm.id.clone(),
                name: fm.name.clone(),
                score: fm.score,
            });
            if fm.score > entry.score {
                entry.score = fm.score;
            }
        }
    }

    // annotate cards with matched failure modes from intake
    for pack in packs {
        let conn = pack.conn.lock();
        for fm_id in &analysis.failure_mode_ids {
            let mut stmt = conn.prepare(
                "SELECT technique_slug FROM technique_failure_modes WHERE failure_mode_id = ?1",
            )?;
            for slug in stmt.query_map([fm_id], |r| r.get::<_, String>(0))?.filter_map(|r| r.ok()) {
                let key = format!("{}:{}", pack.manifest.pack_id, slug);
                if let Some(c) = card_best.get_mut(&key) {
                    if !c.matched_fms.contains(fm_id) {
                        c.matched_fms.push(fm_id.clone());
                    }
                }
            }
        }
        // pull summaries for the top candidates (used in rerank + documents)
        for card in card_best.values_mut() {
            if card.pack_id == pack.manifest.pack_id && card.summary.is_empty() {
                if let Ok(s) = conn.query_row(
                    "SELECT summary FROM techniques WHERE slug = ?1",
                    [&card.slug],
                    |r| r.get::<_, String>(0),
                ) {
                    card.summary = s;
                }
            }
        }
    }

    let mut cards: Vec<CardCandidate> = card_best.into_values().collect();
    cards.sort_by(|a, b| {
        b.fusion_score
            .total_cmp(&a.fusion_score)
            .then(b.arms_hit.cmp(&a.arms_hit))
    });
    cards.truncate(14);

    let mut evidence: Vec<Evidence> = chunk_best.into_values().collect();
    evidence.sort_by(|a, b| {
        chunk_cos
            .get(&b.doc_key)
            .unwrap_or(&0.0)
            .total_cmp(chunk_cos.get(&a.doc_key).unwrap_or(&0.0))
    });
    evidence.truncate(40);

    let mut fms: Vec<AdvisoryFailureMode> = fm_best.into_values().collect();
    fms.sort_by(|a, b| b.score.total_cmp(&a.score));
    fms.truncate(4);

    Ok((cards, evidence, fms))
}

async fn apply_rerank(
    cohere: &CohereClient,
    config: &Config,
    query: &str,
    evidence: &mut [Evidence],
    counts: &mut CallCounts,
) {
    if evidence.is_empty() {
        return;
    }
    let docs: Vec<String> = evidence
        .iter()
        .map(|e| format!("{}\n{}", e.heading_path, e.text.chars().take(3000).collect::<String>()))
        .collect();
    match cohere.rerank(&config.rerank_model, query, &docs, docs.len()).await {
        Ok(results) => {
            counts.rerank += 1;
            for r in results {
                if let Some(e) = evidence.get_mut(r.index) {
                    e.rerank_score = Some(r.relevance_score);
                }
            }
        }
        Err(_) => {} // exact-cosine order remains the fallback
    }
}

/// Adaptive-k + diversity selection: cut at the score cliff, cap chunks per
/// technique (dartboard-style coverage), keep within the evidence token budget.
fn select_evidence(mut pool: Vec<Evidence>, cards: &[CardCandidate]) -> Vec<Evidence> {
    pool.sort_by(|a, b| {
        b.rerank_score
            .unwrap_or(0.0)
            .total_cmp(&a.rerank_score.unwrap_or(0.0))
    });

    // adaptive-k: stop at a large relative drop once scores are weak
    let mut cut = pool.len().min(18);
    if pool.first().and_then(|e| e.rerank_score).is_some() {
        for i in 1..pool.len().min(18) {
            let prev = pool[i - 1].rerank_score.unwrap_or(0.0);
            let cur = pool[i].rerank_score.unwrap_or(0.0);
            if cur < 0.30 && prev > 0.0 && cur / prev.max(1e-6) < 0.45 {
                cut = i;
                break;
            }
        }
    }
    pool.truncate(cut.max(6).min(pool.len()));

    let top_slugs: HashSet<&str> = cards.iter().take(6).map(|c| c.slug.as_str()).collect();
    let mut per_technique: HashMap<String, usize> = HashMap::new();
    let mut budget_tokens: i64 = 14_000;
    let mut selected = Vec::new();
    // first pass: honor diversity caps
    for e in pool {
        let slug = e.technique_slug.clone().unwrap_or_default();
        let n = per_technique.entry(slug.clone()).or_insert(0);
        let cap = if top_slugs.contains(slug.as_str()) { 3 } else { 2 };
        if *n >= cap {
            continue;
        }
        let cost = context::tokens(&e.text) as i64;
        if budget_tokens - cost < 0 {
            continue;
        }
        budget_tokens -= cost;
        *n += 1;
        selected.push(e);
    }
    selected
}

fn attach_rerank_to_cards(cards: &mut [CardCandidate], evidence: &[Evidence]) {
    for card in cards.iter_mut() {
        card.best_rerank = evidence
            .iter()
            .filter(|e| e.technique_slug.as_deref() == Some(card.slug.as_str()))
            .filter_map(|e| e.rerank_score)
            .fold(None, |acc, s| Some(acc.map_or(s, |a: f64| a.max(s))));
    }
}

async fn grade_sufficiency(
    cohere: &CohereClient,
    config: &Config,
    sub_questions: &[String],
    evidence: &[Evidence],
) -> Result<Vec<SufficiencyVerdict>> {
    let evidence_block: String = evidence
        .iter()
        .map(|e| {
            format!(
                "[{}] {} — {}\n{}\n\n",
                e.doc_key,
                e.technique_slug.as_deref().unwrap_or("-"),
                e.heading_path,
                e.text.chars().take(700).collect::<String>()
            )
        })
        .collect();
    let user = format!(
        "Sub-questions:\n{}\n\nRetrieved evidence:\n{}",
        sub_questions
            .iter()
            .enumerate()
            .map(|(i, q)| format!("{}. {q}\n", i + 1))
            .collect::<String>(),
        evidence_block
    );
    let messages = json!([
        {"role": "system", "content": prompts::sufficiency_system()},
        {"role": "user", "content": user},
    ]);
    let v = cohere
        .chat_structured(&config.utility_model, messages, prompts::sufficiency_schema(), 0.1)
        .await?;
    serde_json::from_value::<Value>(v["verdicts"].clone())
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| Error::Internal("sufficiency parse".into()))
}

/// Zero-API corrective loop: BM25 with the grader's "missing" terms.
fn corrective_requery(
    packs: &[Arc<LoadedPack>],
    missing: &[&SufficiencyVerdict],
    evidence: &mut Vec<Evidence>,
) {
    let mut have: HashSet<String> = evidence.iter().map(|e| e.doc_key.clone()).collect();
    let mut added = 0;
    for verdict in missing {
        let query = verdict
            .missing
            .clone()
            .unwrap_or_else(|| verdict.sub_question.clone());
        if let Ok(resp) = search::search(packs, &query, None, SearchOptions::default()) {
            for hit in resp.chunks.into_iter().take(3) {
                let key = format!("{}:chunk:{}", hit.pack_id, hit.chunk_id);
                if added >= 6 || !have.insert(key.clone()) {
                    continue;
                }
                evidence.push(Evidence {
                    doc_key: key,
                    pack_id: hit.pack_id,
                    chunk_id: hit.chunk_id,
                    document_id: hit.document_id,
                    technique_slug: hit.technique_slug,
                    heading_path: hit.heading_path,
                    text: hit.display_text,
                    location: hit.location,
                    rerank_score: None,
                });
                added += 1;
            }
        }
    }
}

fn build_documents(evidence: &[Evidence], cards: &[CardCandidate]) -> Vec<Value> {
    let mut documents = Vec::new();
    for card in cards.iter().take(8) {
        documents.push(json!({
            "id": format!("{}:card:{}", card.pack_id, card.slug),
            "data": {
                "title": format!("Technique card: {}", card.title),
                "snippet": format!("{}\n\n{}", card.one_liner, card.summary),
            }
        }));
    }
    for e in evidence {
        documents.push(json!({
            "id": e.doc_key,
            "data": {
                "title": format!(
                    "{} — {}",
                    e.technique_slug.as_deref().unwrap_or("source"),
                    e.heading_path
                ),
                "snippet": e.text,
            }
        }));
    }
    documents
}

fn relations_hint(packs: &[Arc<LoadedPack>], cards: &[CardCandidate]) -> String {
    let slugs: HashSet<&str> = cards.iter().map(|c| c.slug.as_str()).collect();
    let mut lines = Vec::new();
    for pack in packs {
        let conn = pack.conn.lock();
        let Ok(mut stmt) = conn.prepare(
            "SELECT from_slug, relation, to_slug FROM technique_relations",
        ) else {
            continue;
        };
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .ok();
        if let Some(rows) = rows {
            for (from, rel, to) in rows.filter_map(|r| r.ok()) {
                if slugs.contains(from.as_str()) && slugs.contains(to.as_str()) {
                    lines.push(format!("{from} {rel} {to}"));
                }
            }
        }
    }
    lines.sort();
    lines.dedup();
    lines.truncate(30);
    if lines.is_empty() {
        "(none among candidates)".into()
    } else {
        lines.join("\n")
    }
}

async fn synthesize(
    cohere: &CohereClient,
    config: &Config,
    conv_state: &ConversationState,
    analysis: &TurnAnalysis,
    plan: &Plan,
    documents: &[Value],
    relations_hint: &str,
) -> Result<(String, Value)> {
    let system = prompts::synthesis_system(&analysis.constraints, relations_hint, &plan.sections);
    let mut user = String::new();
    if let Some(p) = &conv_state.pinned_problem {
        user.push_str(&format!("The practitioner's request (their words):\n{p}\n\n"));
    }
    user.push_str(&format!("Standalone restatement: {}\n", analysis.standalone_query));
    if let Some(intent) = &analysis.intent {
        user.push_str(&format!("Intent: {intent}\n"));
    }
    if let Some(cs) = &analysis.context_symptom {
        if cs != "not_applicable" {
            user.push_str(&format!("Context symptom: {cs}\n"));
        }
    }
    if !analysis.failure_mode_ids.is_empty() {
        user.push_str(&format!(
            "Matched failure modes: {}\n",
            analysis.failure_mode_ids.join(", ")
        ));
    }
    user.push_str("\nWrite the dossier now.");
    let messages = json!([
        {"role": "system", "content": system},
        {"role": "user", "content": user},
    ]);
    cohere
        .chat_with_documents(&config.synth_model, messages, json!(documents), 0.3)
        .await
}

fn parse_citations(raw: &Value, documents: &[Value]) -> Vec<SpanCitation> {
    let valid: HashSet<&str> = documents.iter().filter_map(|d| d["id"].as_str()).collect();
    let Some(arr) = raw.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|c| {
            let start = c["start"].as_u64()? as usize;
            let end = c["end"].as_u64()? as usize;
            let text = c["text"].as_str().unwrap_or_default().to_string();
            let doc_keys: Vec<String> = c["sources"]
                .as_array()?
                .iter()
                .filter_map(|s| {
                    s["id"]
                        .as_str()
                        .or_else(|| s["document"]["id"].as_str())
                        .filter(|id| valid.contains(id))
                        .map(String::from)
                })
                .collect();
            if doc_keys.is_empty() {
                return None;
            }
            Some(SpanCitation { start, end, text, doc_keys })
        })
        .collect()
}

async fn critic(
    cohere: &CohereClient,
    config: &Config,
    answer_md: &str,
    cards: &[CardCandidate],
    evidence: &[Evidence],
) -> Result<HashMap<String, (bool, Option<String>)>> {
    let evidence_block: String = evidence
        .iter()
        .map(|e| {
            format!(
                "[{}] {}\n{}\n\n",
                e.technique_slug.as_deref().unwrap_or("-"),
                e.heading_path,
                e.text.chars().take(600).collect::<String>()
            )
        })
        .collect();
    let user = format!(
        "Candidate techniques: {}\n\nDossier:\n{}\n\nEvidence excerpts:\n{}",
        cards
            .iter()
            .take(8)
            .map(|c| c.slug.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        answer_md.chars().take(9000).collect::<String>(),
        evidence_block.chars().take(9000).collect::<String>(),
    );
    let messages = json!([
        {"role": "system", "content": prompts::critic_system()},
        {"role": "user", "content": user},
    ]);
    let v = cohere
        .chat_structured(&config.utility_model, messages, prompts::critic_schema(), 0.1)
        .await?;
    let mut out = HashMap::new();
    if let Some(arr) = v["judgments"].as_array() {
        for j in arr {
            if let Some(slug) = j["slug"].as_str() {
                out.insert(
                    slug.to_string(),
                    (
                        j["supported"].as_bool().unwrap_or(false),
                        j["weakest_claim"].as_str().map(String::from),
                    ),
                );
            }
        }
    }
    Ok(out)
}

fn build_recommendations(
    cards: &[CardCandidate],
    judgments: &HashMap<String, (bool, Option<String>)>,
    packs: &[Arc<LoadedPack>],
) -> Vec<Recommendation> {
    let max_fusion = cards.first().map(|c| c.fusion_score).unwrap_or(1.0).max(1e-9);
    let mut recs: Vec<Recommendation> = cards
        .iter()
        .take(6)
        .map(|c| {
            let retrieval_strength = (c.fusion_score / max_fusion).min(1.0);
            let rerank_part = c.best_rerank.unwrap_or(0.35);
            let critic_part = match judgments.get(&c.slug) {
                Some((true, _)) => 1.0,
                Some((false, _)) => 0.25,
                None => 0.6,
            };
            let confidence =
                (0.35 * retrieval_strength + 0.35 * rerank_part + 0.30 * critic_part).clamp(0.05, 0.99);
            let label = if confidence >= 0.7 {
                "high"
            } else if confidence >= 0.45 {
                "medium"
            } else {
                "low"
            };

            let mut fit = if c.matched_fms.is_empty() {
                c.one_liner.clone()
            } else {
                format!("Addresses {}. {}", c.matched_fms.join(", "), c.one_liner)
            };
            if let Some((false, Some(weak))) = judgments.get(&c.slug).map(|(s, w)| (*s, w.clone())) {
                fit.push_str(&format!(" (Critic note: weakly supported — {weak})"));
            }

            let pair_with = pair_suggestions(packs, &c.pack_id, &c.slug);
            Recommendation {
                slug: c.slug.clone(),
                pack_id: c.pack_id.clone(),
                title: c.title.clone(),
                stage_id: c.stage_id.clone(),
                complexity: c.complexity.clone(),
                fit,
                tradeoffs: String::new(), // full tradeoffs live on the technique card view
                pair_with,
                vendor_disclosure: c.vendor_disclosure.clone(),
                confidence,
                confidence_label: label.into(),
            }
        })
        .collect();
    recs.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    recs
}

fn pair_suggestions(packs: &[Arc<LoadedPack>], pack_id: &str, slug: &str) -> Vec<String> {
    for pack in packs {
        if pack.manifest.pack_id != pack_id {
            continue;
        }
        let conn = pack.conn.lock();
        let Ok(mut stmt) = conn.prepare(
            "SELECT to_slug FROM technique_relations
             WHERE from_slug = ?1 AND relation = 'composes_with' LIMIT 3",
        ) else {
            return Vec::new();
        };
        return stmt
            .query_map([slug], |r| r.get::<_, String>(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
    }
    Vec::new()
}

// ------------------------------------------------------- degraded & helpers

fn local_advisory(
    packs: &[Arc<LoadedPack>],
    message: &str,
    prior_cards: Option<&[CardCandidate]>,
    _conv_state: &ConversationState,
    degraded: bool,
) -> Advisory {
    let resp = search::search(packs, message, None, SearchOptions::default()).ok();
    let (fms, recs) = match (&resp, prior_cards) {
        (_, Some(cards)) => {
            let recs = build_recommendations(cards, &HashMap::new(), packs);
            let fms = Vec::new();
            (fms, recs)
        }
        (Some(r), None) => {
            let fms: Vec<AdvisoryFailureMode> = r
                .failure_modes
                .iter()
                .map(|f| AdvisoryFailureMode { id: f.id.clone(), name: f.name.clone(), score: f.score })
                .collect();
            let recs: Vec<Recommendation> = r
                .cards
                .iter()
                .take(6)
                .map(|c| Recommendation {
                    slug: c.slug.clone(),
                    pack_id: c.pack_id.clone(),
                    title: c.title.clone(),
                    stage_id: c.stage_id.clone(),
                    complexity: c.complexity.clone(),
                    fit: c.one_liner.clone(),
                    tradeoffs: String::new(),
                    pair_with: Vec::new(),
                    vendor_disclosure: c.vendor_disclosure.clone(),
                    confidence: 0.3,
                    confidence_label: "local match".into(),
                })
                .collect();
            (fms, recs)
        }
        _ => (Vec::new(), Vec::new()),
    };

    Advisory {
        tier: "local".into(),
        route: Route::NewProblem,
        clarifying_question: None,
        diagnosis_md: String::new(),
        failure_modes: fms,
        recommendations: recs,
        answer_md: String::new(),
        citations: Vec::new(),
        evidence: Vec::new(),
        gaps: None,
        sufficiency: Vec::new(),
        degraded,
        attribution_html: packs.iter().map(|p| p.manifest.attribution_html.clone()).collect(),
    }
}

fn clarify_advisory(analysis: &TurnAnalysis, question: &str) -> Advisory {
    Advisory {
        tier: "clarify".into(),
        route: analysis.route.clone(),
        clarifying_question: Some(question.to_string()),
        diagnosis_md: String::new(),
        failure_modes: Vec::new(),
        recommendations: Vec::new(),
        answer_md: String::new(),
        citations: Vec::new(),
        evidence: Vec::new(),
        gaps: None,
        sufficiency: Vec::new(),
        degraded: false,
        attribution_html: Vec::new(),
    }
}

fn meta_advisory(packs: &[Arc<LoadedPack>]) -> Advisory {
    let mut technique_count = 0usize;
    let names: Vec<String> = packs
        .iter()
        .map(|p| {
            technique_count += p.card_slugs.len();
            p.manifest.name.clone()
        })
        .collect();
    Advisory {
        tier: "meta".into(),
        route: Route::Meta,
        clarifying_question: None,
        diagnosis_md: String::new(),
        failure_modes: Vec::new(),
        recommendations: Vec::new(),
        answer_md: format!(
            "I'm Compendium — tell me about the system you're planning (your use case and \
constraints) or a problem you're hitting in an existing one, and I'll recommend the best-fit \
techniques from my curated knowledge packs ({}; {} techniques total), with cited source material \
you can read here or hand to another AI.",
            names.join(", "),
            technique_count
        ),
        citations: Vec::new(),
        evidence: Vec::new(),
        gaps: None,
        sufficiency: Vec::new(),
        degraded: false,
        attribution_html: Vec::new(),
    }
}

fn diagnosis_md(analysis: &TurnAnalysis, fms: &[AdvisoryFailureMode]) -> String {
    let mut out = String::new();
    if let Some(cs) = &analysis.context_symptom {
        if cs == "starved" || cs == "polluted" {
            out.push_str(&format!("Context symptom: **{cs}**. "));
        }
    }
    if !fms.is_empty() {
        // For planned systems the matched failure modes are design targets,
        // not a diagnosis of something already broken.
        let label = if analysis.intent.as_deref() == Some("build") {
            "Failure modes to design against"
        } else {
            "Matched failure modes"
        };
        out.push_str(&format!(
            "{label}: {}.",
            fms.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join("; ")
        ));
    }
    out
}

fn is_smalltalk(message: &str) -> bool {
    let m = message.trim().to_lowercase();
    m.len() < 25
        && [
            "hi", "hello", "hey", "thanks", "thank you", "ok", "okay", "cool", "nice", "great",
            "good morning", "good evening", "yo",
        ]
        .iter()
        .any(|w| m == *w || m.starts_with(&format!("{w} ")) || m.starts_with(&format!("{w}!")))
}

fn ontology_hint(packs: &[Arc<LoadedPack>]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for pack in packs {
        let conn = pack.conn.lock();
        let collected: Vec<String> = match conn.prepare("SELECT id, name FROM failure_modes ORDER BY id") {
            Ok(mut stmt) => stmt
                .query_map([], |r| {
                    Ok(format!("- {}: {}", r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        lines.extend(collected);
    }
    lines.join("\n")
}

fn failure_mode_names(packs: &[Arc<LoadedPack>], fm_ids: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for pack in packs {
        let conn = pack.conn.lock();
        for id in fm_ids {
            if let Ok(name) = conn.query_row(
                "SELECT name FROM failure_modes WHERE id = ?1",
                [id],
                |r| r.get::<_, String>(0),
            ) {
                names.push(name);
            }
        }
    }
    names
}

fn load_config(deps: &Deps, tier_override: Option<Tier>) -> Config {
    let settings = {
        let conn = deps.appdb.lock();
        appdb::setting_get(&conn, "advisor").ok().flatten().unwrap_or(Value::Null)
    };
    let tier = tier_override.unwrap_or_else(|| {
        match settings["tier"].as_str() {
            Some("quick") => Tier::Quick,
            Some("deep") => Tier::Deep,
            _ => Tier::Balanced,
        }
    });
    Config {
        tier,
        clarifying_questions: settings["clarifying_questions"].as_bool().unwrap_or(true),
        synth_model: settings["model_synthesizer"]
            .as_str()
            .unwrap_or(DEFAULT_SYNTH_MODEL)
            .to_string(),
        utility_model: settings["model_utility"]
            .as_str()
            .unwrap_or(DEFAULT_UTILITY_MODEL)
            .to_string(),
        rerank_model: settings["model_rerank"].as_str().unwrap_or(RERANK_MODEL).to_string(),
    }
}
