//! Oracle-backed configuration store.
//!
//! Stores configuration key-value pairs in `ZERO_CONFIG`, scoped per agent.
//! Includes convenience methods for the full agent configuration blob.

use oracle::Connection;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Persistent configuration store backed by Oracle Database.
pub struct OracleConfigStore {
    conn: Arc<Mutex<Connection>>,
    agent_id: String,
}

impl OracleConfigStore {
    pub fn new(conn: Arc<Mutex<Connection>>, agent_id: &str) -> Self {
        Self {
            conn,
            agent_id: agent_id.to_string(),
        }
    }

    /// Save the full agent configuration as a single JSON blob.
    pub fn save_config(&self, config_json: &str) -> anyhow::Result<()> {
        self.set("full_config", config_json)
    }

    /// Load the full agent configuration JSON blob.
    pub fn load_config(&self) -> anyhow::Result<Option<String>> {
        self.get("full_config")
    }

    /// Set a config key-value pair (upsert).
    pub fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        conn.execute(
            "MERGE INTO ZERO_CONFIG c
             USING (SELECT :1 AS config_key, :2 AS agent_id FROM DUAL) src
             ON (c.config_key = src.config_key AND c.agent_id = src.agent_id)
             WHEN MATCHED THEN UPDATE SET config_value = :3, updated_at = CURRENT_TIMESTAMP
             WHEN NOT MATCHED THEN INSERT (config_key, agent_id, config_value)
                VALUES (:4, :5, :6)",
            &[&key, &self.agent_id, &value, &key, &self.agent_id, &value],
        )?;
        conn.commit()?;
        debug!("Config set: '{key}'");
        Ok(())
    }

    /// Get a config value by key. Returns `None` if not found.
    pub fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        match conn.query_row(
            "SELECT config_value FROM ZERO_CONFIG WHERE config_key = :1 AND agent_id = :2",
            &[&key, &self.agent_id],
        ) {
            Ok(row) => {
                let value: Option<String> = row.get(0)?;
                Ok(value)
            }
            Err(ref e) if e.kind() == oracle::ErrorKind::NoDataFound => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to get config '{key}': {e}")),
        }
    }

    /// List all config keys for this agent.
    pub fn list_keys(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let rows = conn.query(
            "SELECT config_key FROM ZERO_CONFIG WHERE agent_id = :1 ORDER BY config_key",
            &[&self.agent_id],
        )?;
        let mut keys = Vec::new();
        for row_result in rows {
            keys.push(row_result?.get::<_, String>(0)?);
        }
        Ok(keys)
    }
}
