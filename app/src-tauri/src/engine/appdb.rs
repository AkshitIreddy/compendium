//! Writable app database (%APPDATA%/Compendium/app.db, WAL): conversations,
//! turns, traces, summaries, live conversation state, settings, pack registry,
//! and the monthly API quota ledger. Schema versioned via user_version.

use std::path::Path;

use rusqlite::Connection;
use serde_json::Value;

use crate::error::Result;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS conversations (
  id          INTEGER PRIMARY KEY,
  title       TEXT NOT NULL DEFAULT 'New conversation',
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
  archived    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS turns (
  id              INTEGER PRIMARY KEY,
  conversation_id INTEGER NOT NULL REFERENCES conversations(id),
  role            TEXT NOT NULL CHECK (role IN ('user', 'advisor')),
  content_md      TEXT NOT NULL,
  advisory        TEXT,             -- validated advisory JSON (advisor turns)
  citations       TEXT,             -- span citations JSON
  created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_turns_conversation ON turns(conversation_id);

CREATE TABLE IF NOT EXISTS turn_traces (
  turn_id     INTEGER PRIMARY KEY REFERENCES turns(id),
  route       TEXT,
  tier        TEXT,
  standalone_query TEXT,
  retrieval   TEXT,                 -- per-stage candidate ids + scores (JSON)
  tokens      TEXT,                 -- per-block token accounting (JSON)
  models      TEXT,                 -- model ids used per stage (JSON)
  latency_ms  INTEGER,
  validation  TEXT                  -- validation outcomes (JSON)
);

CREATE TABLE IF NOT EXISTS summaries (
  id              INTEGER PRIMARY KEY,
  conversation_id INTEGER NOT NULL REFERENCES conversations(id),
  seq             INTEGER NOT NULL,
  content         TEXT NOT NULL,
  created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_summaries_conversation ON summaries(conversation_id, seq);

CREATE TABLE IF NOT EXISTS conversation_state (
  conversation_id INTEGER PRIMARY KEY REFERENCES conversations(id),
  pinned_problem  TEXT,
  constraints     TEXT,             -- JSON
  advisor_state   TEXT,             -- JSON: slugs + verdicts + reactions
  candidate_pool  TEXT,             -- JSON: ids + scores, stable order
  open_question   TEXT
);

CREATE VIRTUAL TABLE IF NOT EXISTS turns_fts USING fts5(
  content, tokenize='porter unicode61'
);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS pack_registry (
  pack_id      TEXT PRIMARY KEY,
  pack_version TEXT NOT NULL,
  path         TEXT NOT NULL,
  enabled      INTEGER NOT NULL DEFAULT 1,
  first_seen   TEXT NOT NULL DEFAULT (datetime('now'))
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS quota_ledger (
  month        TEXT PRIMARY KEY,   -- 'YYYY-MM'
  embed_calls  INTEGER NOT NULL DEFAULT 0,
  chat_calls   INTEGER NOT NULL DEFAULT 0,
  rerank_calls INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
";

pub fn open(dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(dir)?;
    let conn = Connection::open(dir.join("app.db"))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    conn.pragma_update(None, "user_version", 1)?;
    Ok(conn)
}

#[allow(dead_code)] // used by the context builder (Phase 4)
pub fn setting_get(conn: &Connection, key: &str) -> Result<Option<Value>> {
    let row: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn setting_set(conn: &Connection, key: &str, value: &Value) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value.to_string()],
    )?;
    Ok(())
}

pub fn settings_all(conn: &Connection) -> Result<Value> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let mut map = serde_json::Map::new();
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (k, v) = row?;
        map.insert(k, serde_json::from_str(&v).unwrap_or(Value::Null));
    }
    Ok(Value::Object(map))
}

pub fn quota_bump(conn: &Connection, embed: u32, chat: u32, rerank: u32) -> Result<()> {
    conn.execute(
        "INSERT INTO quota_ledger (month, embed_calls, chat_calls, rerank_calls)
         VALUES (strftime('%Y-%m', 'now'), ?1, ?2, ?3)
         ON CONFLICT(month) DO UPDATE SET
           embed_calls = embed_calls + excluded.embed_calls,
           chat_calls = chat_calls + excluded.chat_calls,
           rerank_calls = rerank_calls + excluded.rerank_calls",
        rusqlite::params![embed, chat, rerank],
    )?;
    Ok(())
}

pub fn quota_current(conn: &Connection) -> Result<Value> {
    let row = conn
        .query_row(
            "SELECT month, embed_calls, chat_calls, rerank_calls FROM quota_ledger
             WHERE month = strftime('%Y-%m', 'now')",
            [],
            |r| {
                Ok(serde_json::json!({
                    "month": r.get::<_, String>(0)?,
                    "embed_calls": r.get::<_, i64>(1)?,
                    "chat_calls": r.get::<_, i64>(2)?,
                    "rerank_calls": r.get::<_, i64>(3)?,
                }))
            },
        )
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(serde_json::json!({
                "month": "", "embed_calls": 0, "chat_calls": 0, "rerank_calls": 0
            })),
            other => Err(other),
        })?;
    Ok(row)
}

pub fn register_pack(conn: &Connection, pack_id: &str, version: &str, path: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO pack_registry (pack_id, pack_version, path) VALUES (?1, ?2, ?3)
         ON CONFLICT(pack_id) DO UPDATE SET pack_version = excluded.pack_version,
           path = excluded.path",
        rusqlite::params![pack_id, version, path],
    )?;
    Ok(())
}
