//! Tauri IPC commands — the entire surface the webview can reach.

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::engine::advisor;
use crate::engine::cohere::CohereClient;
use crate::engine::pack::{LoadedPack, PackManifest};
use crate::engine::search::{self, SearchOptions, SearchResponse};
use crate::engine::{appdb, keys};
use crate::error::{Error, Result};

pub struct AppState {
    pub packs: RwLock<Vec<Arc<LoadedPack>>>,
    pub cohere: Arc<CohereClient>,
    pub appdb: Arc<Mutex<Connection>>,
}

impl AppState {
    fn advisor_deps(&self, app: &tauri::AppHandle) -> advisor::Deps {
        let handle = app.clone();
        advisor::Deps {
            packs: self.packs.read().clone(),
            cohere: self.cohere.clone(),
            appdb: self.appdb.clone(),
            notify: Arc::new(move |event, payload| {
                use tauri::Emitter;
                let _ = handle.emit(event, payload);
            }),
        }
    }
}

#[derive(Serialize)]
pub struct PackInfo {
    #[serde(flatten)]
    pub manifest: PackManifest,
    pub healed: bool,
    pub path: String,
}

#[tauri::command]
pub fn packs_list(state: State<'_, AppState>) -> Vec<PackInfo> {
    state
        .packs
        .read()
        .iter()
        .map(|p| PackInfo {
            manifest: p.manifest.clone(),
            healed: p.healed,
            path: p.path.to_string_lossy().into_owned(),
        })
        .collect()
}

#[derive(Serialize)]
pub struct KeyStatus {
    pub present: bool,
    pub last4: Option<String>,
}

#[tauri::command]
pub async fn key_set(state: State<'_, AppState>, key: String) -> Result<KeyStatus> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(Error::Internal("empty key".into()));
    }
    state.cohere.check_key(&key).await?;
    keys::store_key(&key)?;
    state.cohere.set_key(Some(key.clone()));
    Ok(KeyStatus { present: true, last4: Some(key.chars().rev().take(4).collect::<String>().chars().rev().collect()) })
}

#[tauri::command]
pub fn key_status(state: State<'_, AppState>) -> Result<KeyStatus> {
    match keys::read_key()? {
        Some(k) => {
            state.cohere.set_key(Some(k.clone()));
            Ok(KeyStatus {
                present: true,
                last4: Some(k.chars().rev().take(4).collect::<String>().chars().rev().collect()),
            })
        }
        None => Ok(KeyStatus { present: false, last4: None }),
    }
}

#[tauri::command]
pub fn key_delete(state: State<'_, AppState>) -> Result<()> {
    keys::delete_key()?;
    state.cohere.set_key(None);
    Ok(())
}

/// Hybrid search against all loaded packs. Uses dense retrieval when a key is
/// configured; degrades to BM25 + ontology matching without one.
#[tauri::command]
pub async fn search_query(state: State<'_, AppState>, query: String) -> Result<SearchResponse> {
    let query_vec = if state.cohere.has_key() {
        match state.cohere.embed_queries(&[query.clone()]).await {
            Ok(mut v) if !v.is_empty() => {
                appdb::quota_bump(&state.appdb.lock(), 1, 0, 0)?;
                Some(v.remove(0))
            }
            Ok(_) => None, // degenerate empty embedding response → local mode
            // Degrade to local-only rather than failing the whole search.
            Err(Error::QuotaExhausted) | Err(Error::NoApiKey) => None,
            Err(e) => return Err(e),
        }
    } else {
        None
    };

    let packs = state.packs.read().clone();
    search::search(&packs, &query, query_vec.as_deref(), SearchOptions::default())
}

/// Fetch a document (for the source viewer) plus its technique context.
#[tauri::command]
pub fn document_get(state: State<'_, AppState>, pack_id: String, document_id: i64) -> Result<Value> {
    let packs = state.packs.read();
    let pack = packs
        .iter()
        .find(|p| p.manifest.pack_id == pack_id)
        .ok_or_else(|| Error::Pack(format!("pack '{pack_id}' not loaded")))?;
    let conn = pack.conn.lock();
    let doc = conn.query_row(
        "SELECT kind, title, source_url, license_note, content FROM documents WHERE id = ?1",
        [document_id],
        |r| {
            Ok(serde_json::json!({
                "kind": r.get::<_, String>(0)?,
                "title": r.get::<_, String>(1)?,
                "source_url": r.get::<_, String>(2)?,
                "license_note": r.get::<_, String>(3)?,
                "content": r.get::<_, String>(4)?,
                "attribution_html": pack.manifest.attribution_html,
            }))
        },
    )?;
    Ok(doc)
}

/// Full technique detail for a card view.
#[tauri::command]
pub fn technique_get(state: State<'_, AppState>, pack_id: String, slug: String) -> Result<Value> {
    let packs = state.packs.read();
    let pack = packs
        .iter()
        .find(|p| p.manifest.pack_id == pack_id)
        .ok_or_else(|| Error::Pack(format!("pack '{pack_id}' not loaded")))?;
    let conn = pack.conn.lock();
    let mut technique = conn.query_row(
        "SELECT slug, title, one_liner, stage_id, complexity, problem_solved, how_it_works,
                when_to_use, tradeoffs, key_dependencies, keywords, summary,
                vendor_disclosure, document_id
         FROM techniques WHERE slug = ?1",
        [&slug],
        |r| {
            Ok(serde_json::json!({
                "slug": r.get::<_, String>(0)?,
                "title": r.get::<_, String>(1)?,
                "one_liner": r.get::<_, String>(2)?,
                "stage_id": r.get::<_, String>(3)?,
                "complexity": r.get::<_, String>(4)?,
                "problem_solved": r.get::<_, String>(5)?,
                "how_it_works": r.get::<_, String>(6)?,
                "when_to_use": r.get::<_, String>(7)?,
                "tradeoffs": r.get::<_, String>(8)?,
                "key_dependencies": r.get::<_, String>(9)?,
                "keywords": r.get::<_, String>(10)?,
                "summary": r.get::<_, String>(11)?,
                "vendor_disclosure": r.get::<_, Option<String>>(12)?,
                "document_id": r.get::<_, i64>(13)?,
            }))
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT r.to_slug, r.relation, t.title FROM technique_relations r
         JOIN techniques t ON t.slug = r.to_slug WHERE r.from_slug = ?1",
    )?;
    let relations: Vec<Value> = stmt
        .query_map([&slug], |r| {
            Ok(serde_json::json!({
                "slug": r.get::<_, String>(0)?,
                "relation": r.get::<_, String>(1)?,
                "title": r.get::<_, String>(2)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();
    technique["relations"] = Value::Array(relations);

    let mut stmt = conn.prepare(
        "SELECT f.id, f.name FROM technique_failure_modes tfm
         JOIN failure_modes f ON f.id = tfm.failure_mode_id WHERE tfm.technique_slug = ?1",
    )?;
    let fms: Vec<Value> = stmt
        .query_map([&slug], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "name": r.get::<_, String>(1)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();
    technique["failure_modes"] = Value::Array(fms);
    Ok(technique)
}

// ------------------------------------------------------------ conversations

#[tauri::command]
pub async fn advisor_ask(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    conversation_id: Option<i64>,
    message: String,
    tier: Option<advisor::types::Tier>,
) -> Result<advisor::types::AdvisorTurn> {
    let deps = state.advisor_deps(&app);
    advisor::ask(deps, conversation_id, message, tier).await
}

#[tauri::command]
pub fn conversation_list(state: State<'_, AppState>) -> Result<Value> {
    let conn = state.appdb.lock();
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, updated_at, archived FROM conversations
         WHERE archived = 0 ORDER BY updated_at DESC",
    )?;
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "title": r.get::<_, String>(1)?,
                "created_at": r.get::<_, String>(2)?,
                "updated_at": r.get::<_, String>(3)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(Value::Array(rows))
}

#[tauri::command]
pub fn conversation_get(state: State<'_, AppState>, conversation_id: i64) -> Result<Value> {
    let conn = state.appdb.lock();
    let title: String = conn.query_row(
        "SELECT title FROM conversations WHERE id = ?1",
        [conversation_id],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, role, content_md, advisory, citations, created_at FROM turns
         WHERE conversation_id = ?1 ORDER BY id",
    )?;
    let turns: Vec<Value> = stmt
        .query_map([conversation_id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "role": r.get::<_, String>(1)?,
                "content_md": r.get::<_, String>(2)?,
                "advisory": r.get::<_, Option<String>>(3)?
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "citations": r.get::<_, Option<String>>(4)?
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "created_at": r.get::<_, String>(5)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(serde_json::json!({"id": conversation_id, "title": title, "turns": turns}))
}

#[tauri::command]
pub fn conversation_rename(
    state: State<'_, AppState>,
    conversation_id: i64,
    title: String,
) -> Result<()> {
    let conn = state.appdb.lock();
    advisor::context::conversation_set_title(&conn, conversation_id, &title)
}

#[tauri::command]
pub fn conversation_delete(state: State<'_, AppState>, conversation_id: i64) -> Result<()> {
    let conn = state.appdb.lock();
    conn.execute("DELETE FROM turn_traces WHERE turn_id IN (SELECT id FROM turns WHERE conversation_id = ?1)", [conversation_id])?;
    conn.execute("DELETE FROM turns_fts WHERE rowid IN (SELECT id FROM turns WHERE conversation_id = ?1)", [conversation_id])?;
    conn.execute("DELETE FROM turns WHERE conversation_id = ?1", [conversation_id])?;
    conn.execute("DELETE FROM summaries WHERE conversation_id = ?1", [conversation_id])?;
    conn.execute("DELETE FROM conversation_state WHERE conversation_id = ?1", [conversation_id])?;
    conn.execute("DELETE FROM conversations WHERE id = ?1", [conversation_id])?;
    Ok(())
}

#[tauri::command]
pub fn conversation_search(state: State<'_, AppState>, query: String) -> Result<Value> {
    let conn = state.appdb.lock();
    let Some(expr) = crate::engine::search::fts_query(&query) else {
        return Ok(Value::Array(Vec::new()));
    };
    let mut stmt = conn.prepare(
        "SELECT t.conversation_id, c.title, snippet(turns_fts, 0, '<b>', '</b>', '…', 12)
         FROM turns_fts JOIN turns t ON t.id = turns_fts.rowid
         JOIN conversations c ON c.id = t.conversation_id
         WHERE turns_fts MATCH ?1 ORDER BY rank LIMIT 20",
    )?;
    let rows: Vec<Value> = stmt
        .query_map([expr], |r| {
            Ok(serde_json::json!({
                "conversation_id": r.get::<_, i64>(0)?,
                "title": r.get::<_, String>(1)?,
                "snippet": r.get::<_, String>(2)?,
            }))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(Value::Array(rows))
}

/// Export a turn's advisory as the hand-to-another-AI markdown dossier.
#[tauri::command]
pub fn export_dossier(state: State<'_, AppState>, turn_id: i64) -> Result<String> {
    let conn = state.appdb.lock();
    let (conversation_id, advisory_json): (i64, Option<String>) = conn.query_row(
        "SELECT conversation_id, advisory FROM turns WHERE id = ?1",
        [turn_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let advisory: advisor::types::Advisory = advisory_json
        .and_then(|s| serde_json::from_str(&s).ok())
        .ok_or_else(|| Error::Internal("turn has no advisory".into()))?;
    let title: String = conn.query_row(
        "SELECT title FROM conversations WHERE id = ?1",
        [conversation_id],
        |r| r.get(0),
    )?;
    let problem = advisor::context::state_load(&conn, conversation_id)?
        .pinned_problem;
    Ok(advisor::export::to_markdown(&advisory, &title, problem.as_deref()))
}

#[tauri::command]
pub fn settings_get_all(state: State<'_, AppState>) -> Result<Value> {
    appdb::settings_all(&state.appdb.lock())
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, key: String, value: Value) -> Result<()> {
    appdb::setting_set(&state.appdb.lock(), &key, &value)
}

#[tauri::command]
pub fn quota_get(state: State<'_, AppState>) -> Result<Value> {
    appdb::quota_current(&state.appdb.lock())
}
