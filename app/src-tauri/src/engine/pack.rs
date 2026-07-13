//! Pack loading: open read-only, verify identity, restore usearch indexes
//! (self-healing rebuild from the f32 vectors of record on any mismatch),
//! and hold the small in-memory structures search needs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use usearch::{new_index, Index, IndexOptions, MetricKind, ScalarKind};

use crate::error::{Error, Result};

pub const PACK_APPLICATION_ID: i64 = 0x434D_5044; // 'CMPD'
pub const SUPPORTED_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct PackManifest {
    pub pack_id: String,
    pub pack_version: String,
    pub name: String,
    pub description: String,
    pub source_type: String,
    pub embedding_model: String,
    pub embedding_dims: usize,
    pub attribution_html: String,
    pub license_id: String,
    pub source_ref: String,
    pub built_at: String,
    pub counts: serde_json::Value,
}

/// Failure-mode phrasing embeddings held contiguously for the zero-LLM S0 matcher.
pub struct PhrasingMatrix {
    pub fm_ids: Vec<String>,
    pub phrasings: Vec<String>,
    pub matrix: Vec<f32>, // row-major, rows = phrasings.len(), cols = dims
    pub dims: usize,
}

impl PhrasingMatrix {
    /// Cosine of the query against every phrasing (vectors are pre-normalized).
    pub fn scores(&self, query: &[f32]) -> Vec<f32> {
        self.phrasings
            .iter()
            .enumerate()
            .map(|(row, _)| {
                let start = row * self.dims;
                self.matrix[start..start + self.dims]
                    .iter()
                    .zip(query)
                    .map(|(a, b)| a * b)
                    .sum()
            })
            .collect()
    }
}

pub struct LoadedPack {
    pub manifest: PackManifest,
    pub path: PathBuf,
    /// Read-only connection; rusqlite Connection is !Sync so all statement
    /// execution goes through this mutex (queries are sub-millisecond).
    pub conn: Mutex<Connection>,
    pub cards_index: Index,
    pub chunks_index: Index,
    pub card_slugs: HashMap<u64, String>,
    pub phrasings: PhrasingMatrix,
    /// True if any usearch index had to be rebuilt from stored vectors.
    pub healed: bool,
}

// usearch's Index wraps a thread-safe C++ index (concurrent search is
// supported); the raw pointer makes the Rust type !Send/!Sync by default.
unsafe impl Send for LoadedPack {}
unsafe impl Sync for LoadedPack {}

pub fn load_pack(path: &Path) -> Result<Arc<LoadedPack>> {
    // Windows verbatim prefixes (\\?\C:\...) break SQLite's URI parser; strip
    // them and use the file:///C:/... form.
    let normalized = path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let uri = format!("file:///{}?mode=ro&immutable=1", normalized.trim_start_matches('/'));
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;

    let app_id: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    if app_id != PACK_APPLICATION_ID {
        return Err(Error::Pack(format!(
            "{} is not a Compendium pack (application_id {app_id:#x})",
            path.display()
        )));
    }
    let schema: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if schema != SUPPORTED_SCHEMA_VERSION {
        return Err(Error::Pack(format!(
            "pack schema v{schema} unsupported (app supports v{SUPPORTED_SCHEMA_VERSION})"
        )));
    }

    let mut manifest_map: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT key, value FROM manifest")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (k, v) = row?;
            manifest_map.insert(k, v);
        }
    }
    let get = |k: &str| -> Result<String> {
        manifest_map
            .get(k)
            .filter(|v| !v.trim().is_empty())
            .cloned()
            .ok_or_else(|| Error::Pack(format!("pack manifest missing required key '{k}'")))
    };
    // Attribution is structural: a pack without it does not load (license terms).
    let manifest = PackManifest {
        pack_id: get("pack_id")?,
        pack_version: get("pack_version")?,
        name: get("name")?,
        description: get("description")?,
        source_type: get("source_type")?,
        embedding_model: get("embedding_model")?,
        embedding_dims: get("embedding_dims")?
            .parse()
            .map_err(|_| Error::Pack("embedding_dims not an integer".into()))?,
        attribution_html: get("attribution_html")?,
        license_id: get("license_id")?,
        source_ref: get("source_ref")?,
        built_at: get("built_at")?,
        counts: serde_json::from_str(manifest_map.get("counts").map(String::as_str).unwrap_or("{}"))
            .unwrap_or_default(),
    };

    let dims = manifest.embedding_dims;
    let (cards_index, healed_cards) = load_or_rebuild_index(&conn, "cards", dims)?;
    let (chunks_index, healed_chunks) = load_or_rebuild_index(&conn, "chunks", dims)?;

    let mut card_slugs = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT card_key, slug FROM techniques")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (k, slug) = row?;
            card_slugs.insert(k, slug);
        }
    }

    let phrasings = load_phrasings(&conn, dims)?;

    Ok(Arc::new(LoadedPack {
        manifest,
        path: path.to_path_buf(),
        conn: Mutex::new(conn),
        cards_index,
        chunks_index,
        card_slugs,
        phrasings,
        healed: healed_cards || healed_chunks,
    }))
}

fn index_options(dims: usize, connectivity: usize, expansion_add: usize, expansion_search: usize) -> IndexOptions {
    IndexOptions {
        dimensions: dims,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F16,
        connectivity,
        expansion_add,
        expansion_search,
        multi: false,
    }
}

fn load_or_rebuild_index(conn: &Connection, tier: &str, dims: usize) -> Result<(Index, bool)> {
    let row = conn
        .query_row(
            "SELECT blob, sha256, count, connectivity, expansion_add, expansion_search
             FROM vector_indexes WHERE tier = ?1",
            [tier],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            },
        )
        .map_err(|e| Error::Pack(format!("vector index '{tier}' missing: {e}")))?;
    let (blob, expected_sha, count, connectivity, exp_add, exp_search) = row;

    let opts = index_options(dims, connectivity as usize, exp_add as usize, exp_search as usize);
    let hash_ok = hex::encode(Sha256::digest(&blob)) == expected_sha;
    if hash_ok {
        if let Ok(index) = new_index(&opts) {
            if index.load_from_buffer(&blob).is_ok() && index.size() == count as usize {
                return Ok((index, false));
            }
        }
    }
    // Self-heal: the f32 embeddings are the vectors of record.
    let index = rebuild_index(conn, tier, dims, &opts)?;
    Ok((index, true))
}

fn rebuild_index(conn: &Connection, tier: &str, dims: usize, opts: &IndexOptions) -> Result<Index> {
    let sql = match tier {
        "cards" => {
            "SELECT t.card_key, e.vector FROM techniques t
             JOIN card_embeddings e ON e.technique_slug = t.slug"
        }
        _ => "SELECT chunk_id, vector FROM chunk_embeddings",
    };
    let index = new_index(opts).map_err(|e| Error::Pack(format!("usearch init: {e}")))?;
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, Vec<u8>>(1)?)))?;
    let mut pending: Vec<(u64, Vec<f32>)> = Vec::new();
    for row in rows {
        let (key, blob) = row?;
        pending.push((key, blob_to_f32(&blob, dims)?));
    }
    index
        .reserve(pending.len())
        .map_err(|e| Error::Pack(format!("usearch reserve: {e}")))?;
    for (key, vec) in &pending {
        index
            .add(*key, vec)
            .map_err(|e| Error::Pack(format!("usearch add: {e}")))?;
    }
    Ok(index)
}

fn load_phrasings(conn: &Connection, dims: usize) -> Result<PhrasingMatrix> {
    let mut stmt =
        conn.prepare("SELECT failure_mode_id, phrasing, vector FROM phrasing_embeddings ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Vec<u8>>(2)?,
        ))
    })?;
    let mut fm_ids = Vec::new();
    let mut phrasings = Vec::new();
    let mut matrix = Vec::new();
    for row in rows {
        let (fm, text, blob) = row?;
        fm_ids.push(fm);
        phrasings.push(text);
        matrix.extend(blob_to_f32(&blob, dims)?);
    }
    Ok(PhrasingMatrix { fm_ids, phrasings, matrix, dims })
}

pub fn blob_to_f32(blob: &[u8], dims: usize) -> Result<Vec<f32>> {
    if blob.len() != dims * 4 {
        return Err(Error::Pack(format!(
            "vector blob is {} bytes, expected {}",
            blob.len(),
            dims * 4
        )));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

/// Discover pack files: every *.pack in the packs directory.
pub fn discover_packs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "pack"))
        .collect();
    paths.sort();
    paths
}
