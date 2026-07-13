//! Tauri IPC commands — the entire surface the webview can reach.

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::engine::cohere::CohereClient;
use crate::engine::pack::{LoadedPack, PackManifest};
use crate::engine::search::{self, SearchOptions, SearchResponse};
use crate::engine::{appdb, keys};
use crate::error::{Error, Result};

pub struct AppState {
    pub packs: RwLock<Vec<Arc<LoadedPack>>>,
    pub cohere: CohereClient,
    pub appdb: Mutex<Connection>,
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
            Ok(mut v) => {
                appdb::quota_bump(&state.appdb.lock(), 1, 0, 0)?;
                Some(v.remove(0))
            }
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
