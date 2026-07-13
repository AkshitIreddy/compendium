//! Typed Cohere v2 REST client.
//!
//! Design constraints from research:
//! - `response_format: json_schema` must NEVER be combined with tools or
//!   documents — the request builders make that unrepresentable.
//! - Trial keys are throttled per minute (20 chat / 10 rerank) and capped
//!   monthly; a token-bucket pacer smooths bursts and 429s surface as typed
//!   errors (monthly-cap 429s become QuotaExhausted).
//! - The API key lives only on this side of the IPC boundary.

// rerank/chat methods and their pacers are consumed by the advisor pipeline
// (Phase 4); allow dead_code until then so intermediate commits stay clean.
#![allow(dead_code)]

use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::time::Instant;

use crate::error::{Error, Result};

const BASE: &str = "https://api.cohere.com";
pub const EMBED_MODEL: &str = "embed-v4.0";
pub const EMBED_DIMS: usize = 1024;

/// Minute-window pacer: allows `limit` acquisitions per rolling window.
pub struct Pacer {
    window: Duration,
    limit: u32,
    stamps: Mutex<Vec<Instant>>,
}

impl Pacer {
    pub fn new(limit: u32) -> Self {
        Self { window: Duration::from_secs(60), limit, stamps: Mutex::new(Vec::new()) }
    }

    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut stamps = self.stamps.lock();
                let now = Instant::now();
                stamps.retain(|t| now.duration_since(*t) < self.window);
                if (stamps.len() as u32) < self.limit {
                    stamps.push(now);
                    None
                } else {
                    Some(self.window - now.duration_since(stamps[0]) + Duration::from_millis(50))
                }
            };
            match wait {
                None => return,
                Some(d) => tokio::time::sleep(d).await,
            }
        }
    }
}

pub struct CohereClient {
    http: reqwest::Client,
    key: RwLock<Option<String>>,
    pub embed_pacer: Pacer,
    pub chat_pacer: Pacer,
    pub rerank_pacer: Pacer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankResult {
    pub index: usize,
    pub relevance_score: f64,
}

impl CohereClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("reqwest client"),
            key: RwLock::new(None),
            // Trial ceilings with headroom (trial: 100 embed, 20 chat, 10 rerank per min).
            embed_pacer: Pacer::new(80),
            chat_pacer: Pacer::new(18),
            rerank_pacer: Pacer::new(9),
        }
    }

    pub fn set_key(&self, key: Option<String>) {
        *self.key.write() = key;
    }

    pub fn has_key(&self) -> bool {
        self.key.read().is_some()
    }

    fn key(&self) -> Result<String> {
        self.key.read().clone().ok_or(Error::NoApiKey)
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let key = self.key()?;
        let mut last_err: Option<Error> = None;
        for attempt in 0u32..5 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
            }
            let resp = match self
                .http
                .post(format!("{BASE}{path}"))
                .bearer_auth(&key)
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
            };
            let status = resp.status();
            if status.is_success() {
                return Ok(resp.json().await?);
            }
            let text = resp.text().await.unwrap_or_default();
            if status.as_u16() == 429 {
                // Monthly cap 429s mention the monthly limit; per-minute ones don't.
                if text.to_lowercase().contains("month") {
                    return Err(Error::QuotaExhausted);
                }
                last_err = Some(Error::Api { status: 429, message: "rate limited".into() });
                continue;
            }
            if status.is_server_error() {
                last_err = Some(Error::Api { status: status.as_u16(), message: text });
                continue;
            }
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str().map(String::from)))
                .unwrap_or(text);
            return Err(Error::Api { status: status.as_u16(), message });
        }
        Err(last_err.unwrap_or(Error::Internal("request failed".into())))
    }

    /// Embed search queries (input_type=search_query — the required pairing
    /// with packs built as search_document). Batches up to 96 texts.
    pub async fn embed_queries(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_pacer.acquire().await;
        let body = json!({
            "model": EMBED_MODEL,
            "texts": texts,
            "input_type": "search_query",
            "embedding_types": ["float"],
            "output_dimension": EMBED_DIMS,
        });
        let v = self.post("/v2/embed", body).await?;
        let arr = v["embeddings"]["float"]
            .as_array()
            .ok_or_else(|| Error::Api { status: 200, message: "malformed embed response".into() })?;
        arr.iter()
            .map(|e| {
                e.as_array()
                    .map(|xs| xs.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect())
                    .ok_or_else(|| Error::Api { status: 200, message: "malformed embedding".into() })
            })
            .collect()
    }

    pub async fn rerank(
        &self,
        model: &str,
        query: &str,
        documents: &[String],
        top_n: usize,
    ) -> Result<Vec<RerankResult>> {
        self.rerank_pacer.acquire().await;
        let body = json!({
            "model": model,
            "query": query,
            "documents": documents,
            "top_n": top_n,
        });
        let v = self.post("/v2/rerank", body).await?;
        let results = v["results"]
            .as_array()
            .ok_or_else(|| Error::Api { status: 200, message: "malformed rerank response".into() })?;
        Ok(results
            .iter()
            .filter_map(|r| {
                Some(RerankResult {
                    index: r["index"].as_u64()? as usize,
                    relevance_score: r["relevance_score"].as_f64()?,
                })
            })
            .collect())
    }

    /// Structured-output chat (json_schema). By construction takes no tools
    /// and no documents — Cohere rejects those combinations.
    pub async fn chat_structured(
        &self,
        model: &str,
        messages: Value,
        schema: Value,
        temperature: f64,
    ) -> Result<Value> {
        self.chat_pacer.acquire().await;
        let body = json!({
            "model": model,
            "messages": messages,
            "response_format": {"type": "json_object", "schema": schema},
            "temperature": temperature,
        });
        let v = self.post("/v2/chat", body).await?;
        let text = extract_text(&v)?;
        serde_json::from_str(&text)
            .map_err(|e| Error::Api { status: 200, message: format!("model returned invalid JSON: {e}") })
    }

    /// Documents-mode chat with native span citations (no response_format —
    /// incompatible with documents). Returns (text, citations).
    pub async fn chat_with_documents(
        &self,
        model: &str,
        messages: Value,
        documents: Value,
        temperature: f64,
    ) -> Result<(String, Value)> {
        self.chat_pacer.acquire().await;
        let body = json!({
            "model": model,
            "messages": messages,
            "documents": documents,
            "citation_options": {"mode": "ACCURATE"},
            "temperature": temperature,
        });
        let v = self.post("/v2/chat", body).await?;
        let text = extract_text(&v)?;
        let citations = v["message"]["citations"].clone();
        Ok((text, citations))
    }

    /// Cheapest authenticated call — used to validate a key the user enters.
    pub async fn check_key(&self, candidate: &str) -> Result<()> {
        let resp = self
            .http
            .get(format!("{BASE}/v1/models?page_size=1"))
            .bearer_auth(candidate)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Error::Api {
                status: resp.status().as_u16(),
                message: "key rejected by Cohere".into(),
            })
        }
    }
}

fn extract_text(v: &Value) -> Result<String> {
    let items = v["message"]["content"]
        .as_array()
        .ok_or_else(|| Error::Api { status: 200, message: "empty chat response".into() })?;
    let text: String = items
        .iter()
        .filter(|c| c["type"] == "text")
        .filter_map(|c| c["text"].as_str())
        .collect();
    if text.is_empty() {
        return Err(Error::Api { status: 200, message: "no text in chat response".into() });
    }
    Ok(text)
}
