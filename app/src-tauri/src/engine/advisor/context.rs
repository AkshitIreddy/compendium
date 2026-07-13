//! Conversation persistence + the three-layer context builder:
//! (1) pinned problem statement — verbatim, never truncated or paraphrased,
//! (2) sliding window of recent raw exchanges,
//! (3) running summary, updated by folding evicted turns after the turn commits.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::types::ConversationState;
use crate::error::Result;

/// Rough token estimate (chars/3.6) — good enough for budgeting; real counts
/// come back from the API and are recorded in traces.
pub fn tokens(text: &str) -> usize {
    (text.len() as f64 / 3.6).ceil() as usize
}

const RECENT_EXCHANGES: usize = 3;
/// Fold turns into the summary once evicted history exceeds this budget.
const SUMMARY_TRIGGER_TOKENS: usize = 5_000;

pub fn conversation_create(conn: &Connection) -> Result<i64> {
    conn.execute("INSERT INTO conversations DEFAULT VALUES", [])?;
    Ok(conn.last_insert_rowid())
}

pub fn conversation_touch(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE conversations SET updated_at = datetime('now') WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

pub fn conversation_title(conn: &Connection, id: i64) -> Result<String> {
    Ok(conn.query_row("SELECT title FROM conversations WHERE id = ?1", [id], |r| r.get(0))?)
}

pub fn conversation_set_title(conn: &Connection, id: i64, title: &str) -> Result<()> {
    conn.execute("UPDATE conversations SET title = ?1 WHERE id = ?2", params![title, id])?;
    Ok(())
}

pub fn insert_turn(
    conn: &Connection,
    conversation_id: i64,
    role: &str,
    content_md: &str,
    advisory: Option<&Value>,
    citations: Option<&Value>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO turns (conversation_id, role, content_md, advisory, citations)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            conversation_id,
            role,
            content_md,
            advisory.map(|v| v.to_string()),
            citations.map(|v| v.to_string()),
        ],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO turns_fts (rowid, content) VALUES (?1, ?2)",
        params![id, content_md],
    )?;
    conversation_touch(conn, conversation_id)?;
    Ok(id)
}

pub fn write_trace(conn: &Connection, turn_id: i64, trace: &Value) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO turn_traces
           (turn_id, route, tier, standalone_query, retrieval, tokens, models, latency_ms, validation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            turn_id,
            trace["route"].as_str(),
            trace["tier"].as_str(),
            trace["standalone_query"].as_str(),
            trace["retrieval"].to_string(),
            trace["tokens"].to_string(),
            trace["models"].to_string(),
            trace["latency_ms"].as_i64(),
            trace["validation"].to_string(),
        ],
    )?;
    Ok(())
}

pub fn state_load(conn: &Connection, conversation_id: i64) -> Result<ConversationState> {
    let row: Option<(Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        conn.query_row(
            "SELECT pinned_problem, constraints, advisor_state, candidate_pool, open_question
             FROM conversation_state WHERE conversation_id = ?1",
            [conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((pinned, constraints, advisor_state, pool, open_q)) = row else {
        return Ok(ConversationState::default());
    };
    Ok(ConversationState {
        pinned_problem: pinned,
        constraints: constraints
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        advisor_state: advisor_state
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        candidate_pool: pool.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
        open_question: open_q,
    })
}

pub fn state_save(conn: &Connection, conversation_id: i64, state: &ConversationState) -> Result<()> {
    conn.execute(
        "INSERT INTO conversation_state
           (conversation_id, pinned_problem, constraints, advisor_state, candidate_pool, open_question)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(conversation_id) DO UPDATE SET
           pinned_problem = excluded.pinned_problem,
           constraints = excluded.constraints,
           advisor_state = excluded.advisor_state,
           candidate_pool = excluded.candidate_pool,
           open_question = excluded.open_question",
        params![
            conversation_id,
            state.pinned_problem,
            serde_json::to_string(&state.constraints).unwrap(),
            serde_json::to_string(&state.advisor_state).unwrap(),
            serde_json::to_string(&state.candidate_pool).unwrap(),
            state.open_question,
        ],
    )?;
    Ok(())
}

pub struct HistoryView {
    /// Chat-API-shaped recent messages (role, content), oldest first.
    pub recent: Vec<(String, String)>,
    pub summary: Option<String>,
    /// Turns older than the window that are not yet folded into the summary.
    pub unfolded: Vec<(i64, String, String)>, // (turn_id, role, content)
}

pub fn history(conn: &Connection, conversation_id: i64) -> Result<HistoryView> {
    let summary: Option<String> = conn
        .query_row(
            "SELECT content FROM summaries WHERE conversation_id = ?1 ORDER BY seq DESC LIMIT 1",
            [conversation_id],
            |r| r.get(0),
        )
        .optional()?;
    let last_folded: i64 = conn
        .query_row(
            "SELECT seq FROM summaries WHERE conversation_id = ?1 ORDER BY seq DESC LIMIT 1",
            [conversation_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);

    let mut stmt = conn.prepare(
        "SELECT id, role, content_md FROM turns WHERE conversation_id = ?1 ORDER BY id",
    )?;
    let all: Vec<(i64, String, String)> = stmt
        .query_map([conversation_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    // Window = last N exchanges (user+advisor pairs ≈ 2N turns).
    let window = RECENT_EXCHANGES * 2;
    let split = all.len().saturating_sub(window);
    let recent = all[split..]
        .iter()
        .map(|(_, role, content)| {
            let api_role = if role == "advisor" { "assistant" } else { "user" };
            (api_role.to_string(), content.clone())
        })
        .collect();
    let unfolded = all[..split]
        .iter()
        .filter(|(id, _, _)| *id > last_folded)
        .cloned()
        .collect();

    Ok(HistoryView { recent, summary, unfolded })
}

/// Should we fold now? (called after the turn commits; the fold itself is async)
pub fn needs_summary_fold(view: &HistoryView) -> bool {
    view.unfolded.iter().map(|(_, _, c)| tokens(c)).sum::<usize>() > SUMMARY_TRIGGER_TOKENS
}

pub fn write_summary(conn: &Connection, conversation_id: i64, up_to_turn: i64, content: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO summaries (conversation_id, seq, content) VALUES (?1, ?2, ?3)",
        params![conversation_id, up_to_turn, content],
    )?;
    Ok(())
}

/// Build the message array for the intake call: summary + advisor state up
/// front, then recent raw turns, then the new message. The pinned problem is
/// injected verbatim and never truncated.
pub fn intake_messages(
    system: &str,
    state: &ConversationState,
    view: &HistoryView,
    message: &str,
) -> Value {
    let mut messages = vec![json!({"role": "system", "content": system})];

    let mut context_block = String::new();
    if let Some(p) = &state.pinned_problem {
        context_block.push_str(&format!("Original problem statement (verbatim):\n{p}\n\n"));
    }
    if !state.constraints.is_empty() {
        context_block.push_str(&format!("Known constraints: {}\n\n", state.constraints.join("; ")));
    }
    if !state.advisor_state.is_empty() {
        let told: Vec<String> = state
            .advisor_state
            .iter()
            .map(|e| format!("{} ({})", e.slug, e.verdict))
            .collect();
        context_block.push_str(&format!("Already recommended: {}\n\n", told.join(", ")));
    }
    if let Some(q) = &state.open_question {
        context_block.push_str(&format!("Advisor's open clarifying question: {q}\n\n"));
    }
    if let Some(s) = &view.summary {
        context_block.push_str(&format!("Summary of earlier conversation:\n{s}\n"));
    }
    if !context_block.is_empty() {
        messages.push(json!({"role": "system", "content": format!("Conversation context:\n{context_block}")}));
    }
    for (role, content) in &view.recent {
        // Advisor turns can be long dossiers; the window keeps only the tail.
        let clipped: String = content.chars().take(4000).collect();
        messages.push(json!({"role": role, "content": clipped}));
    }
    messages.push(json!({"role": "user", "content": message}));
    Value::Array(messages)
}
