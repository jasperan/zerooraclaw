//! Oracle-backed agent state (key-value store).
//!
//! Stores arbitrary key-value pairs in `ZERO_STATE`, scoped per agent.
//! The `oracle` crate is synchronous so callers should wrap calls in
//! `spawn_blocking` if needed from async contexts.

use oracle::Connection;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Persistent key-value state store backed by Oracle Database.
pub struct OracleStateStore {
    conn: Arc<Mutex<Connection>>,
    agent_id: String,
}

impl OracleStateStore {
    pub fn new(conn: Arc<Mutex<Connection>>, agent_id: &str) -> Self {
        Self {
            conn,
            agent_id: agent_id.to_string(),
        }
    }

    /// Set a key-value pair (upsert).
    pub fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        conn.execute(
            "MERGE INTO ZERO_STATE s
             USING (SELECT :1 AS state_key, :2 AS agent_id FROM DUAL) src
             ON (s.state_key = src.state_key AND s.agent_id = src.agent_id)
             WHEN MATCHED THEN UPDATE SET value = :3, updated_at = CURRENT_TIMESTAMP
             WHEN NOT MATCHED THEN INSERT (state_key, agent_id, value)
                VALUES (:4, :5, :6)",
            &[&key, &self.agent_id, &value, &key, &self.agent_id, &value],
        )?;
        conn.commit()?;
        debug!("State set: '{key}'");
        Ok(())
    }

    /// Get a value by key. Returns `None` if not found.
    pub fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        match conn.query_row(
            "SELECT value FROM ZERO_STATE WHERE state_key = :1 AND agent_id = :2",
            &[&key, &self.agent_id],
        ) {
            Ok(row) => {
                let value: Option<String> = row.get(0)?;
                Ok(value)
            }
            Err(ref e) if e.kind() == oracle::ErrorKind::NoDataFound => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to get state '{key}': {e}")),
        }
    }

    /// Delete a key. Returns `true` if a row was deleted.
    pub fn delete(&self, key: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let stmt = conn.execute(
            "DELETE FROM ZERO_STATE WHERE state_key = :1 AND agent_id = :2",
            &[&key, &self.agent_id],
        )?;
        let deleted = stmt.row_count()? > 0;
        conn.commit()?;
        if deleted {
            debug!("State deleted: '{key}'");
        }
        Ok(deleted)
    }

    /// List all state keys for this agent.
    pub fn list_keys(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let rows = conn.query(
            "SELECT state_key FROM ZERO_STATE WHERE agent_id = :1 ORDER BY state_key",
            &[&self.agent_id],
        )?;
        let mut keys = Vec::new();
        for row_result in rows {
            keys.push(row_result?.get::<_, String>(0)?);
        }
        Ok(keys)
    }
}
