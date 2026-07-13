//! Local hybrid retrieval: the engine's S0 (ontology matcher), S3 (hierarchical
//! hybrid retrieval with RRF fusion and graph expansion) and the local part of
//! S4 (exact re-scoring). Everything here is API-free and runs in single-digit
//! milliseconds; Cohere is only needed upstream to embed the query.

use std::collections::HashMap;
use std::sync::Arc;

use rusqlite::Connection;
use serde::Serialize;

use crate::engine::pack::{blob_to_f32, LoadedPack};
use crate::error::Result;

const RRF_K: f64 = 60.0;

#[derive(Debug, Clone, Serialize)]
pub struct CardHit {
    pub pack_id: String,
    pub slug: String,
    pub title: String,
    pub one_liner: String,
    pub stage_id: String,
    pub complexity: String,
    pub vendor_disclosure: Option<String>,
    pub score: f64,
    pub exact_cosine: Option<f32>,
    /// Set when this card entered via graph expansion: (relation, from_slug).
    pub expanded_from: Option<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkHit {
    pub pack_id: String,
    pub chunk_id: i64,
    pub technique_slug: Option<String>,
    pub document_id: i64,
    pub heading_path: String,
    pub kind: String,
    pub display_text: String,
    pub location: String,
    pub score: f64,
    pub exact_cosine: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailureModeHit {
    pub pack_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub best_phrasing: String,
    pub score: f32,
    pub technique_slugs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub cards: Vec<CardHit>,
    pub chunks: Vec<ChunkHit>,
    pub failure_modes: Vec<FailureModeHit>,
    pub dense_used: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    pub top_cards: usize,
    pub top_chunks: usize,
    pub expand_graph: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self { top_cards: 12, top_chunks: 24, expand_graph: true }
    }
}

/// Build an FTS5 MATCH expression from natural language: quoted significant
/// terms OR-ed together. Returns None when nothing usable remains.
pub fn fts_query(text: &str) -> Option<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "but", "with", "that", "this", "have", "from", "are",
        "was", "when", "what", "how", "why", "can", "cant", "wont", "dont", "not",
        "its", "then", "than", "they", "them", "will", "would", "should", "could",
        "about", "into", "over", "under", "keep", "keeps", "very", "just", "some",
        "there", "their", "your", "our", "out", "get", "gets", "getting", "still",
    ];
    let mut seen = std::collections::HashSet::new();
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_lowercase())
        .filter(|w| !STOP.contains(&w.as_str()))
        .filter(|w| seen.insert(w.clone()))
        .take(16)
        .map(|w| format!("\"{w}\""))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

fn rrf_fuse(rank_lists: &[Vec<u64>]) -> HashMap<u64, f64> {
    let mut scores: HashMap<u64, f64> = HashMap::new();
    for list in rank_lists {
        for (rank, key) in list.iter().enumerate() {
            *scores.entry(*key).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }
    scores
}

fn exact_cosine(conn: &Connection, sql: &str, key: i64, query: &[f32]) -> Option<f32> {
    let blob: Vec<u8> = conn.query_row(sql, [key], |r| r.get(0)).ok()?;
    let vec = blob_to_f32(&blob, query.len()).ok()?;
    Some(vec.iter().zip(query).map(|(a, b)| a * b).sum())
}

pub fn search(
    packs: &[Arc<LoadedPack>],
    query_text: &str,
    query_vec: Option<&[f32]>,
    opts: SearchOptions,
) -> Result<SearchResponse> {
    let mut all_cards: Vec<CardHit> = Vec::new();
    let mut all_chunks: Vec<ChunkHit> = Vec::new();
    let mut all_fms: Vec<FailureModeHit> = Vec::new();
    let fts = fts_query(query_text);

    for pack in packs {
        let conn = pack.conn.lock();
        let pack_id = pack.manifest.pack_id.clone();

        // ---- S0: failure-mode phrasing match (dense + keyword)
        let fm_hits = failure_modes(pack, &conn, query_vec, fts.as_deref())?;

        // ---- candidate rank lists per tier
        let mut card_lists: Vec<Vec<u64>> = Vec::new();
        let mut chunk_lists: Vec<Vec<u64>> = Vec::new();

        if let Some(q) = query_vec {
            if let Ok(m) = pack.cards_index.search(q, 20) {
                card_lists.push(m.keys.clone());
            }
            if let Ok(m) = pack.chunks_index.search(q, 50) {
                chunk_lists.push(m.keys.clone());
            }
        }
        if let Some(expr) = &fts {
            let mut stmt = conn.prepare(
                "SELECT t.card_key FROM cards_fts f JOIN techniques t ON t.slug = f.slug
                 WHERE cards_fts MATCH ?1 ORDER BY rank LIMIT 20",
            )?;
            let keys: Vec<u64> = stmt
                .query_map([expr], |r| r.get::<_, i64>(0))?
                .filter_map(|r| r.ok())
                .map(|k| k as u64)
                .collect();
            if !keys.is_empty() {
                card_lists.push(keys);
            }
            let mut stmt = conn.prepare(
                "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY rank LIMIT 50",
            )?;
            let keys: Vec<u64> = stmt
                .query_map([expr], |r| r.get::<_, i64>(0))?
                .filter_map(|r| r.ok())
                .map(|k| k as u64)
                .collect();
            if !keys.is_empty() {
                chunk_lists.push(keys);
            }
        }
        // Ontology-informed list: techniques addressing the top matched failure
        // modes join the card fusion as their own ranked voice (S0 feeding S3).
        // Dedup preserves RRF's one-contribution-per-list semantics for
        // techniques linked to several matched failure modes.
        let mut seen_keys = std::collections::HashSet::new();
        let ontology_list: Vec<u64> = fm_hits
            .iter()
            .take(3)
            .flat_map(|fm| fm.technique_slugs.iter())
            .filter_map(|slug| {
                pack.card_slugs
                    .iter()
                    .find(|(_, s)| *s == slug)
                    .map(|(k, _)| *k)
            })
            .filter(|k| seen_keys.insert(*k))
            .collect();
        if !ontology_list.is_empty() {
            card_lists.push(ontology_list);
        }

        // ---- RRF fusion + exact re-score
        let card_scores = rrf_fuse(&card_lists);
        let chunk_scores = rrf_fuse(&chunk_lists);

        let mut fused_cards: Vec<(u64, f64)> = card_scores.into_iter().collect();
        fused_cards.sort_by(|a, b| b.1.total_cmp(&a.1));
        fused_cards.truncate(opts.top_cards);

        for (key, score) in &fused_cards {
            let Some(slug) = pack.card_slugs.get(key) else { continue };
            let row = conn.query_row(
                "SELECT title, one_liner, stage_id, complexity, vendor_disclosure
                 FROM techniques WHERE slug = ?1",
                [slug],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                },
            )?;
            let exact = query_vec.and_then(|q| {
                exact_cosine(
                    &conn,
                    "SELECT e.vector FROM card_embeddings e JOIN techniques t
                     ON t.slug = e.technique_slug WHERE t.card_key = ?1",
                    *key as i64,
                    q,
                )
            });
            all_cards.push(CardHit {
                pack_id: pack_id.clone(),
                slug: slug.clone(),
                title: row.0,
                one_liner: row.1,
                stage_id: row.2,
                complexity: row.3,
                vendor_disclosure: row.4,
                score: *score,
                exact_cosine: exact,
                expanded_from: None,
            });
        }

        let mut fused_chunks: Vec<(u64, f64)> = chunk_scores.into_iter().collect();
        fused_chunks.sort_by(|a, b| b.1.total_cmp(&a.1));
        fused_chunks.truncate(opts.top_chunks);

        for (key, score) in &fused_chunks {
            let row = conn.query_row(
                "SELECT technique_slug, document_id, heading_path, kind, display_text, location
                 FROM chunks WHERE id = ?1",
                [*key as i64],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )?;
            let exact = query_vec.and_then(|q| {
                exact_cosine(
                    &conn,
                    "SELECT vector FROM chunk_embeddings WHERE chunk_id = ?1",
                    *key as i64,
                    q,
                )
            });
            all_chunks.push(ChunkHit {
                pack_id: pack_id.clone(),
                chunk_id: *key as i64,
                technique_slug: row.0,
                document_id: row.1,
                heading_path: row.2,
                kind: row.3,
                display_text: row.4,
                location: row.5,
                score: *score,
                exact_cosine: exact,
            });
        }

        // ---- 1-hop typed-graph expansion from the top cards
        if opts.expand_graph {
            // Keyed by (pack, slug) and updated as expansions land, so a
            // neighbor shared by two parents is added exactly once.
            let mut present: std::collections::HashSet<(String, String)> = all_cards
                .iter()
                .map(|c| (c.pack_id.clone(), c.slug.clone()))
                .collect();
            let top_slugs: Vec<(String, f64)> = all_cards
                .iter()
                .filter(|c| c.pack_id == pack_id)
                .take(6)
                .map(|c| (c.slug.clone(), c.score))
                .collect();
            for (from_slug, parent_score) in top_slugs {
                let mut stmt = conn.prepare(
                    "SELECT r.to_slug, r.relation, t.title, t.one_liner, t.stage_id,
                            t.complexity, t.vendor_disclosure
                     FROM technique_relations r JOIN techniques t ON t.slug = r.to_slug
                     WHERE r.from_slug = ?1",
                )?;
                let neighbors = stmt.query_map([&from_slug], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, Option<String>>(6)?,
                    ))
                })?;
                for n in neighbors.filter_map(|n| n.ok()) {
                    if !present.insert((pack_id.clone(), n.0.clone())) {
                        continue;
                    }
                    all_cards.push(CardHit {
                        pack_id: pack_id.clone(),
                        slug: n.0,
                        title: n.2,
                        one_liner: n.3,
                        stage_id: n.4,
                        complexity: n.5,
                        vendor_disclosure: n.6,
                        score: parent_score * 0.4,
                        exact_cosine: None,
                        expanded_from: Some((n.1, from_slug.clone())),
                    });
                }
            }
        }

        all_fms.extend(fm_hits);
    }

    // Cards: fusion-primary ordering — the ontology voice in RRF is deliberate
    // (S0 feeding S3), and the advisor's rerank stage refines later.
    all_cards.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.exact_cosine.unwrap_or(0.0).total_cmp(&a.exact_cosine.unwrap_or(0.0)))
    });
    // Chunks are evidence: RRF selects the candidate set, exact cosine over the
    // f32 vectors of record orders it (cosines are comparable across packs —
    // same embedding model). Falls back to fusion order without a query vector.
    all_chunks.sort_by(|a, b| {
        b.exact_cosine
            .unwrap_or(-1.0)
            .total_cmp(&a.exact_cosine.unwrap_or(-1.0))
            .then(b.score.total_cmp(&a.score))
    });
    all_fms.sort_by(|a, b| b.score.total_cmp(&a.score));
    all_fms.truncate(5);

    Ok(SearchResponse {
        cards: all_cards,
        chunks: all_chunks,
        failure_modes: all_fms,
        dense_used: query_vec.is_some(),
    })
}

fn failure_modes(
    pack: &LoadedPack,
    conn: &Connection,
    query_vec: Option<&[f32]>,
    fts: Option<&str>,
) -> Result<Vec<FailureModeHit>> {
    let mut best: HashMap<String, (f32, String)> = HashMap::new();

    if let Some(q) = query_vec {
        for (row, score) in pack.phrasings.scores(q).into_iter().enumerate() {
            let fm = &pack.phrasings.fm_ids[row];
            let entry = best.entry(fm.clone()).or_insert((f32::MIN, String::new()));
            if score > entry.0 {
                *entry = (score, pack.phrasings.phrasings[row].clone());
            }
        }
    }
    if let Some(expr) = fts {
        let mut stmt = conn.prepare(
            "SELECT failure_mode_id, phrasing FROM phrasings_fts
             WHERE phrasings_fts MATCH ?1 ORDER BY rank LIMIT 10",
        )?;
        for row in stmt.query_map([expr], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (fm, phrasing) = row?;
            // Keyword hits matter most in degraded (no-key) mode; give them a
            // floor score so they surface without displacing strong dense hits.
            let entry = best.entry(fm).or_insert((0.45, phrasing.clone()));
            entry.0 += 0.02;
        }
    }

    let mut hits = Vec::new();
    for (fm_id, (score, phrasing)) in best {
        if score < 0.40 {
            continue;
        }
        let Ok((name, description)) = conn.query_row(
            "SELECT name, description FROM failure_modes WHERE id = ?1",
            [&fm_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        ) else {
            continue;
        };
        let mut stmt = conn.prepare(
            "SELECT technique_slug FROM technique_failure_modes WHERE failure_mode_id = ?1",
        )?;
        let slugs: Vec<String> = stmt
            .query_map([&fm_id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        hits.push(FailureModeHit {
            pack_id: pack.manifest.pack_id.clone(),
            id: fm_id,
            name,
            description,
            best_phrasing: phrasing,
            score,
            technique_slugs: slugs,
        });
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(hits)
}
