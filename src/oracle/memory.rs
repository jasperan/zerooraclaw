//! Oracle Memory trait implementation.
//!
//! Implements the `Memory` trait from `crate::memory::traits` using Oracle AI
//! Database as the backend.  Embeddings are computed **inline** using
//! `VECTOR_EMBEDDING(<model> USING :text AS DATA)` — Oracle does the
//! embedding in-database, so we never need to extract/serialize vectors.
//!
//! All DB calls are wrapped in `tokio::task::spawn_blocking` because the
//! `oracle` crate is synchronous.

use crate::memory::traits::{ExportFilter, Memory, MemoryCategory, MemoryEntry};
use crate::oracle::vector::similarity_from_distance;
use async_trait::async_trait;
use oracle::Connection;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};
use uuid::Uuid;

/// Minimum similarity score to include in recall results.
/// Results with distance-based similarity below this are filtered out.
const MIN_SIMILARITY: f64 = 0.3;

/// Oracle-backed memory store with inline VECTOR_EMBEDDING.
///
/// Uses Oracle's in-database ONNX model to compute embeddings directly in
/// INSERT/SELECT SQL — no external embedding calls needed.
pub struct OracleMemory {
    conn: Arc<Mutex<Connection>>,
    agent_id: String,
    /// ONNX model name for VECTOR_EMBEDDING() (e.g. "ALL_MINILM_L12_V2").
    model_name: String,
}

impl OracleMemory {
    /// Create a new Oracle memory backend.
    ///
    /// * `conn` — shared connection from `OracleConnectionManager::conn()`
    /// * `agent_id` — agent identifier for data isolation
    /// * `model_name` — ONNX model name for in-database VECTOR_EMBEDDING()
    pub fn new(
        conn: Arc<Mutex<Connection>>,
        agent_id: &str,
        model_name: &str,
    ) -> Self {
        Self {
            conn,
            agent_id: agent_id.to_string(),
            model_name: model_name.to_string(),
        }
    }
}

// ── Category helpers ────────────────────────────────────────────

/// Parse a category string into `MemoryCategory`.
fn parse_category(s: &str) -> MemoryCategory {
    match s.to_ascii_lowercase().as_str() {
        "core" => MemoryCategory::Core,
        "daily" => MemoryCategory::Daily,
        "conversation" => MemoryCategory::Conversation,
        other => MemoryCategory::Custom(other.to_string()),
    }
}

// ── Row mapping helper ──────────────────────────────────────────

/// Map a query row to a `MemoryEntry`.
///
/// Expected column order: memory_id, key, content, category, created_at,
/// session_id, namespace, importance, superseded_by
fn row_to_entry(row: &oracle::Row) -> anyhow::Result<MemoryEntry> {
    let id: String = row.get(0)?;
    let key: String = row.get(1)?;
    let content: String = row.get(2)?;
    let cat_str: String = row.get(3)?;
    let ts: String = row.get(4)?;
    let session_id: Option<String> = row.get(5)?;
    let namespace: Option<String> = row.get(6)?;
    let importance: Option<f64> = row.get(7)?;
    let superseded_by: Option<String> = row.get(8)?;

    Ok(MemoryEntry {
        id,
        key,
        content,
        category: parse_category(&cat_str),
        timestamp: ts,
        session_id,
        score: None,
        namespace: namespace.unwrap_or_else(|| "default".to_string()),
        importance,
        superseded_by,
    })
}

fn within_timestamp_range(timestamp: &str, since: Option<&str>, until: Option<&str>) -> bool {
    if let Some(lower) = since {
        if timestamp < lower {
            return false;
        }
    }
    if let Some(upper) = until {
        if timestamp > upper {
            return false;
        }
    }
    true
}

#[async_trait]
impl Memory for OracleMemory {
    fn name(&self) -> &str {
        "oracle"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.store_with_metadata(key, content, category, session_id, None, None)
            .await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let query_str = query.to_string();
        let session_id = session_id.map(|s| s.to_string());
        let since = since.map(|s| s.to_string());
        let until = until.map(|s| s.to_string());
        let limit_i64 = limit as i64;
        let model = self.model_name.clone();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let mut entries = Vec::new();

            // Vector similarity search using inline VECTOR_EMBEDDING for query
            let vector_result: anyhow::Result<()> = (|| {
                let (sql, params): (String, Vec<Box<dyn oracle::sql_type::ToSql>>) =
                    if let Some(ref sid) = session_id {
                        (
                            format!(
                                "SELECT memory_id, key, content, category,
                                        TO_CHAR(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS ts,
                                        session_id,
                                        namespace,
                                        importance,
                                        superseded_by,
                                        VECTOR_DISTANCE(embedding, VECTOR_EMBEDDING({model} USING :1 AS DATA), COSINE) AS dist
                                 FROM ZERO_MEMORIES
                                 WHERE agent_id = :2
                                   AND embedding IS NOT NULL
                                   AND session_id = :3
                                   AND superseded_by IS NULL
                                 ORDER BY dist ASC
                                 FETCH FIRST :4 ROWS ONLY"
                            ),
                            vec![
                                Box::new(query_str.clone()),
                                Box::new(agent_id.clone()),
                                Box::new(sid.clone()),
                                Box::new(limit_i64),
                            ],
                        )
                    } else {
                        (
                            format!(
                                "SELECT memory_id, key, content, category,
                                        TO_CHAR(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS ts,
                                        session_id,
                                        namespace,
                                        importance,
                                        superseded_by,
                                        VECTOR_DISTANCE(embedding, VECTOR_EMBEDDING({model} USING :1 AS DATA), COSINE) AS dist
                                 FROM ZERO_MEMORIES
                                 WHERE agent_id = :2
                                   AND embedding IS NOT NULL
                                   AND superseded_by IS NULL
                                 ORDER BY dist ASC
                                 FETCH FIRST :3 ROWS ONLY"
                            ),
                            vec![
                                Box::new(query_str.clone()),
                                Box::new(agent_id.clone()),
                                Box::new(limit_i64),
                            ],
                        )
                    };

                let param_refs: Vec<&dyn oracle::sql_type::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let rows = guard.query(&sql, param_refs.as_slice())?;
                for row_result in rows {
                    let row = row_result?;
                    let id: String = row.get(0)?;
                    let key: String = row.get(1)?;
                    let content: String = row.get(2)?;
                    let cat_str: String = row.get(3)?;
                    let ts: String = row.get(4)?;
                    let sid: Option<String> = row.get(5)?;
                    let namespace: Option<String> = row.get(6)?;
                    let importance: Option<f64> = row.get(7)?;
                    let superseded_by: Option<String> = row.get(8)?;
                    let dist: f64 = row.get(9)?;
                    let similarity = similarity_from_distance(dist);

                    if similarity < MIN_SIMILARITY {
                        continue;
                    }

                    let entry = MemoryEntry {
                        id,
                        key,
                        content,
                        category: parse_category(&cat_str),
                        timestamp: ts,
                        session_id: sid,
                        score: Some(similarity),
                        namespace: namespace.unwrap_or_else(|| "default".to_string()),
                        importance,
                        superseded_by,
                    };

                    if within_timestamp_range(
                        &entry.timestamp,
                        since.as_deref(),
                        until.as_deref(),
                    ) {
                        entries.push(entry);
                    }
                }
                Ok(())
            })();

            if let Err(e) = vector_result {
                warn!("Vector search failed, falling back to keyword: {e}");
            }

            // Fallback: keyword search if vector search failed or returned no results
            if entries.is_empty() {
                // Escape LIKE wildcards in user input to prevent unintended matching
                let escaped_query: String = query_str
                    .chars()
                    .flat_map(|c| match c {
                        '\\' => vec!['\\', '\\'],
                        '%' => vec!['\\', '%'],
                        '_' => vec!['\\', '_'],
                        other => vec![other],
                    })
                    .collect();
                let like_pattern = format!("%{escaped_query}%");

                let (sql, params): (String, Vec<Box<dyn oracle::sql_type::ToSql>>) =
                    if let Some(ref sid) = session_id {
                        (
                            "SELECT memory_id, key, content, category,
                                    TO_CHAR(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS ts,
                                    session_id,
                                    namespace,
                                    importance,
                                    superseded_by
                             FROM ZERO_MEMORIES
                             WHERE agent_id = :1
                               AND (LOWER(content) LIKE LOWER(:2) ESCAPE '\\' OR LOWER(key) LIKE LOWER(:3) ESCAPE '\\')
                               AND session_id = :4
                               AND superseded_by IS NULL
                             ORDER BY updated_at DESC
                             FETCH FIRST :5 ROWS ONLY"
                                .to_string(),
                            vec![
                                Box::new(agent_id.clone()),
                                Box::new(like_pattern.clone()),
                                Box::new(like_pattern.clone()),
                                Box::new(sid.clone()),
                                Box::new(limit_i64),
                            ],
                        )
                    } else {
                        (
                            "SELECT memory_id, key, content, category,
                                    TO_CHAR(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS ts,
                                    session_id,
                                    namespace,
                                    importance,
                                    superseded_by
                             FROM ZERO_MEMORIES
                             WHERE agent_id = :1
                               AND (LOWER(content) LIKE LOWER(:2) ESCAPE '\\' OR LOWER(key) LIKE LOWER(:3) ESCAPE '\\')
                               AND superseded_by IS NULL
                             ORDER BY updated_at DESC
                             FETCH FIRST :4 ROWS ONLY"
                                .to_string(),
                            vec![
                                Box::new(agent_id.clone()),
                                Box::new(like_pattern.clone()),
                                Box::new(like_pattern.clone()),
                                Box::new(limit_i64),
                            ],
                        )
                    };

                let param_refs: Vec<&dyn oracle::sql_type::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let rows = guard.query(&sql, param_refs.as_slice())?;
                for row_result in rows {
                    let row = row_result?;
                    let mut entry = row_to_entry(&row)?;
                    entry.score = Some(0.5);
                    if within_timestamp_range(&entry.timestamp, since.as_deref(), until.as_deref()) {
                        entries.push(entry);
                    }
                }

                if !entries.is_empty() {
                    debug!(
                        "Keyword fallback returned {} results for '{query_str}'",
                        entries.len()
                    );
                }
            }

            Ok(entries)
        })
        .await?
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let key = key.to_string();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let sql = "
                SELECT memory_id, key, content, category,
                       TO_CHAR(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS ts,
                       session_id,
                       namespace,
                       importance,
                       superseded_by
                FROM ZERO_MEMORIES
                WHERE key = :1 AND agent_id = :2 AND superseded_by IS NULL
                ORDER BY updated_at DESC
                FETCH FIRST 1 ROWS ONLY
            ";

            let result = guard.query_row(sql, &[&key, &agent_id]);
            match result {
                Ok(row) => {
                    let entry = row_to_entry(&row)?;

                    // Bump access count (best-effort, don't fail the read)
                    let _ = guard.execute(
                        "UPDATE ZERO_MEMORIES SET access_count = access_count + 1 WHERE key = :1 AND agent_id = :2",
                        &[&key, &agent_id],
                    );
                    let _ = guard.commit();

                    Ok(Some(entry))
                }
                Err(ref e) if e.kind() == oracle::ErrorKind::NoDataFound => Ok(None),
                Err(e) => Err(anyhow::anyhow!("Failed to get memory '{key}': {e}")),
            }
        })
        .await?
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let cat_str = category.map(|c| c.to_string());
        let session_id = session_id.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            // Build SQL dynamically based on filters
            let mut sql = String::from(
                "SELECT memory_id, key, content, category,
                        TO_CHAR(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS ts,
                        session_id,
                        namespace,
                        importance,
                        superseded_by
                 FROM ZERO_MEMORIES
                 WHERE agent_id = :1
                   AND superseded_by IS NULL",
            );

            let mut params: Vec<Box<dyn oracle::sql_type::ToSql>> =
                vec![Box::new(agent_id.clone())];

            if let Some(ref cat) = cat_str {
                sql.push_str(&format!(" AND category = :{}", params.len() + 1));
                params.push(Box::new(cat.clone()));
            }

            if let Some(ref sid) = session_id {
                sql.push_str(&format!(" AND session_id = :{}", params.len() + 1));
                params.push(Box::new(sid.clone()));
            }

            sql.push_str(" ORDER BY updated_at DESC");

            let param_refs: Vec<&dyn oracle::sql_type::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();

            let rows = guard.query(&sql, param_refs.as_slice())?;
            let mut entries = Vec::new();
            for row_result in rows {
                let row = row_result?;
                entries.push(row_to_entry(&row)?);
            }

            Ok(entries)
        })
        .await?
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let key = key.to_string();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let stmt = guard.execute(
                "DELETE FROM ZERO_MEMORIES WHERE key = :1 AND agent_id = :2",
                &[&key, &agent_id],
            )?;

            let deleted = stmt.row_count()? > 0;
            guard.commit()?;

            if deleted {
                debug!("Forgot memory '{key}' (agent={agent_id})");
            }
            Ok(deleted)
        })
        .await?
    }

    async fn store_with_metadata(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
    ) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let key = key.to_string();
        let content = content.to_string();
        let cat_str = category.to_string();
        let session_id = session_id.map(|s| s.to_string());
        let namespace = namespace.unwrap_or("default").to_string();
        let importance = importance.unwrap_or(0.5);
        let memory_id = Uuid::new_v4().to_string();
        let model = self.model_name.clone();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let sql = format!(
                "MERGE INTO ZERO_MEMORIES m
                 USING (SELECT :1 AS key, :2 AS agent_id FROM DUAL) src
                 ON (m.key = src.key AND m.agent_id = src.agent_id)
                 WHEN MATCHED THEN
                     UPDATE SET
                         m.content       = :3,
                         m.category      = :4,
                         m.session_id    = :5,
                         m.namespace     = :6,
                         m.embedding     = VECTOR_EMBEDDING({model} USING :7 AS DATA),
                         m.importance    = :8,
                         m.superseded_by = NULL,
                         m.updated_at    = CURRENT_TIMESTAMP
                 WHEN NOT MATCHED THEN
                     INSERT (
                         memory_id,
                         agent_id,
                         key,
                         content,
                         category,
                         session_id,
                         namespace,
                         embedding,
                         importance,
                         superseded_by,
                         created_at,
                         updated_at
                     )
                     VALUES (
                         :9,
                         :10,
                         :11,
                         :12,
                         :13,
                         :14,
                         :15,
                         VECTOR_EMBEDDING({model} USING :16 AS DATA),
                         :17,
                         NULL,
                         CURRENT_TIMESTAMP,
                         CURRENT_TIMESTAMP
                     )"
            );

            guard.execute(
                &sql,
                &[
                    &key,
                    &agent_id,
                    &content,
                    &cat_str,
                    &session_id,
                    &namespace,
                    &content,
                    &importance,
                    &memory_id,
                    &agent_id,
                    &key,
                    &content,
                    &cat_str,
                    &session_id,
                    &namespace,
                    &content,
                    &importance,
                ],
            )?;

            guard.commit()?;
            debug!(
                "Stored memory '{key}' with inline embedding (agent={agent_id}, namespace={namespace})"
            );
            Ok(())
        })
        .await?
    }

    async fn purge_namespace(&self, namespace: &str) -> anyhow::Result<usize> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let namespace = namespace.to_string();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let stmt = guard.execute(
                "DELETE FROM ZERO_MEMORIES WHERE agent_id = :1 AND namespace = :2",
                &[&agent_id, &namespace],
            )?;
            let deleted = stmt.row_count()? as usize;
            guard.commit()?;
            Ok(deleted)
        })
        .await?
    }

    async fn purge_session(&self, session_id: &str) -> anyhow::Result<usize> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let session_id = session_id.to_string();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let stmt = guard.execute(
                "DELETE FROM ZERO_MEMORIES WHERE agent_id = :1 AND session_id = :2",
                &[&agent_id, &session_id],
            )?;
            let deleted = stmt.row_count()? as usize;
            guard.commit()?;
            Ok(deleted)
        })
        .await?
    }

    async fn export(&self, filter: &ExportFilter) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self
            .list(filter.category.as_ref(), filter.session_id.as_deref())
            .await?;
        Ok(entries
            .into_iter()
            .filter(|entry| {
                if let Some(ref namespace) = filter.namespace {
                    if entry.namespace != *namespace {
                        return false;
                    }
                }
                within_timestamp_range(
                    &entry.timestamp,
                    filter.since.as_deref(),
                    filter.until.as_deref(),
                )
            })
            .collect())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();

        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;

            let count: i64 = guard.query_row_as(
                "SELECT COUNT(DISTINCT key) FROM ZERO_MEMORIES WHERE agent_id = :1 AND superseded_by IS NULL",
                &[&agent_id],
            )?;

            Ok(count as usize)
        })
        .await?
    }

    async fn health_check(&self) -> bool {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            conn.lock().map_or(false, |guard| guard.ping().is_ok())
        })
        .await
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_category_core() {
        assert_eq!(parse_category("core"), MemoryCategory::Core);
        assert_eq!(parse_category("CORE"), MemoryCategory::Core);
        assert_eq!(parse_category("Core"), MemoryCategory::Core);
    }

    #[test]
    fn parse_category_daily() {
        assert_eq!(parse_category("daily"), MemoryCategory::Daily);
        assert_eq!(parse_category("DAILY"), MemoryCategory::Daily);
    }

    #[test]
    fn parse_category_conversation() {
        assert_eq!(parse_category("conversation"), MemoryCategory::Conversation);
        assert_eq!(parse_category("CONVERSATION"), MemoryCategory::Conversation);
    }

    #[test]
    fn parse_category_custom() {
        assert_eq!(
            parse_category("project_notes"),
            MemoryCategory::Custom("project_notes".into())
        );
    }

    #[test]
    fn min_similarity_threshold_is_reasonable() {
        assert!(MIN_SIMILARITY > 0.0);
        assert!(MIN_SIMILARITY < 1.0);
    }
}
