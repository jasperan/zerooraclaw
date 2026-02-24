//! Oracle ONNX embedding provider.
//!
//! Generates embeddings in-database using `VECTOR_EMBEDDING()` with an ONNX
//! model loaded into Oracle AI Vector Search.  The oracle crate is synchronous,
//! so every DB call is wrapped in `tokio::task::spawn_blocking`.

use crate::memory::embeddings::EmbeddingProvider;
use async_trait::async_trait;
use oracle::Connection;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Embedding dimensions produced by the default ALL_MINILM_L12_V2 ONNX model.
const DEFAULT_DIMENSIONS: usize = 384;

/// Oracle in-database embedding provider backed by ONNX models.
pub struct OracleEmbedding {
    conn: Arc<Mutex<Connection>>,
    model_name: String,
    dimensions: usize,
}

impl OracleEmbedding {
    /// Create a new provider.
    ///
    /// * `conn` — shared connection from `OracleConnectionManager::conn()`
    /// * `model_name` — ONNX model name registered in Oracle (e.g. `ALL_MINILM_L12_V2`)
    pub fn new(conn: Arc<Mutex<Connection>>, model_name: &str) -> Self {
        Self {
            conn,
            model_name: model_name.to_string(),
            dimensions: DEFAULT_DIMENSIONS,
        }
    }

    /// Create with explicit dimensions override.
    pub fn with_dimensions(conn: Arc<Mutex<Connection>>, model_name: &str, dims: usize) -> Self {
        Self {
            conn,
            model_name: model_name.to_string(),
            dimensions: dims,
        }
    }

    /// Verify the ONNX model is loaded in `USER_MINING_MODELS`.
    ///
    /// Returns `Ok(true)` if found, `Ok(false)` if not, `Err` on DB error.
    pub async fn check_onnx_model(&self) -> anyhow::Result<bool> {
        let conn = self.conn.clone();
        let model = self.model_name.clone();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;
            let sql = "SELECT COUNT(*) FROM USER_MINING_MODELS WHERE MODEL_NAME = :1";
            let row = guard.query_row_as::<i64>(sql, &[&model])?;
            Ok(row > 0)
        })
        .await?
    }

    /// Generate a single embedding vector from text using the ONNX model.
    ///
    /// The SQL uses `VECTOR_EMBEDDING(<model> USING :1 AS DATA)` which
    /// returns the vector as a string like `[0.123, -0.456, ...]`.
    fn embed_text_sync(conn: &Connection, model_name: &str, text: &str) -> anyhow::Result<Vec<f32>> {
        // Oracle VECTOR_EMBEDDING returns a vector; we SELECT TO_CHAR to get a parseable string.
        let sql = format!(
            "SELECT TO_CHAR(VECTOR_EMBEDDING({model_name} USING :1 AS DATA)) FROM DUAL"
        );
        let result: String = conn.query_row_as(&sql, &[&text])?;
        parse_oracle_vector(&result)
    }
}

/// Parse Oracle's vector string representation `[0.1, 0.2, ...]` into `Vec<f32>`.
fn parse_oracle_vector(s: &str) -> anyhow::Result<Vec<f32>> {
    let trimmed = s.trim();
    // Strip surrounding brackets
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| anyhow::anyhow!("Invalid vector format (missing brackets): {trimmed}"))?;

    let vec: Vec<f32> = inner
        .split(',')
        .map(|tok| {
            tok.trim()
                .parse::<f32>()
                .map_err(|e| anyhow::anyhow!("Failed to parse vector element '{tok}': {e}"))
        })
        .collect::<anyhow::Result<Vec<f32>>>()?;

    if vec.is_empty() {
        anyhow::bail!("Parsed vector is empty");
    }

    Ok(vec)
}

#[async_trait]
impl EmbeddingProvider for OracleEmbedding {
    fn name(&self) -> &str {
        "oracle-onnx"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.clone();
        let model = self.model_name.clone();
        // Clone texts into owned Strings for the blocking closure
        let owned_texts: Vec<String> = texts.iter().map(|t| t.to_string()).collect();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let mut embeddings = Vec::with_capacity(owned_texts.len());
            for text in &owned_texts {
                let vec = Self::embed_text_sync(&guard, &model, text)?;
                debug!(
                    "Embedded text ({} chars) -> {} dims",
                    text.len(),
                    vec.len()
                );
                embeddings.push(vec);
            }
            Ok(embeddings)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_vector() {
        let v = parse_oracle_vector("[1.0, 2.5, -3.0]").unwrap();
        assert_eq!(v.len(), 3);
        assert!((v[0] - 1.0).abs() < f32::EPSILON);
        assert!((v[1] - 2.5).abs() < f32::EPSILON);
        assert!((v[2] - (-3.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_vector_with_whitespace() {
        let v = parse_oracle_vector("  [ 0.1 , 0.2 , 0.3 ]  ").unwrap();
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn parse_vector_invalid_no_brackets() {
        assert!(parse_oracle_vector("0.1, 0.2").is_err());
    }

    #[test]
    fn parse_vector_empty_brackets() {
        assert!(parse_oracle_vector("[]").is_err());
    }

    #[test]
    fn parse_vector_bad_element() {
        assert!(parse_oracle_vector("[1.0, abc, 3.0]").is_err());
    }

    #[test]
    fn parse_vector_single_element() {
        let v = parse_oracle_vector("[42.0]").unwrap();
        assert_eq!(v.len(), 1);
        assert!((v[0] - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_vector_scientific_notation() {
        let v = parse_oracle_vector("[1.5e-2, -3.0E1]").unwrap();
        assert_eq!(v.len(), 2);
        assert!((v[0] - 0.015).abs() < 1e-6);
        assert!((v[1] - (-30.0)).abs() < f32::EPSILON);
    }
}
