//! Oracle-backed session store for chat history persistence.
//!
//! Persists session messages, summaries, and transcript entries in
//! `ZERO_SESSIONS` and `ZERO_TRANSCRIPTS`.  The `oracle` crate is
//! synchronous so callers should wrap calls in `spawn_blocking` if
//! needed from async contexts.

use oracle::Connection;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// Persistent chat session store backed by Oracle Database.
pub struct OracleSessionStore {
    conn: Arc<Mutex<Connection>>,
    agent_id: String,
}

impl OracleSessionStore {
    pub fn new(conn: Arc<Mutex<Connection>>, agent_id: &str) -> Self {
        Self {
            conn,
            agent_id: agent_id.to_string(),
        }
    }

    /// Save messages JSON for a session key (upsert).
    pub fn save_messages(&self, session_key: &str, messages_json: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        conn.execute(
            "MERGE INTO ZERO_SESSIONS s
             USING (SELECT :1 AS session_key, :2 AS agent_id FROM DUAL) src
             ON (s.session_key = src.session_key AND s.agent_id = src.agent_id)
             WHEN MATCHED THEN UPDATE SET messages = :3, updated_at = CURRENT_TIMESTAMP
             WHEN NOT MATCHED THEN INSERT (session_key, agent_id, messages)
                VALUES (:4, :5, :6)",
            &[
                &session_key,
                &self.agent_id,
                &messages_json,
                &session_key,
                &self.agent_id,
                &messages_json,
            ],
        )?;
        conn.commit()?;
        debug!("Saved messages for session '{session_key}'");
        Ok(())
    }

    /// Load messages JSON for a session key. Returns `None` if not found.
    pub fn load_messages(&self, session_key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        match conn.query_row(
            "SELECT messages FROM ZERO_SESSIONS WHERE session_key = :1 AND agent_id = :2",
            &[&session_key, &self.agent_id],
        ) {
            Ok(row) => {
                let messages: Option<String> = row.get(0)?;
                Ok(messages)
            }
            Err(oracle::Error::NoDataFound) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to load session '{session_key}': {e}")),
        }
    }

    /// Append a transcript entry to `ZERO_TRANSCRIPTS`.
    pub fn append_transcript(
        &self,
        session_key: &str,
        role: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        conn.execute(
            "INSERT INTO ZERO_TRANSCRIPTS (agent_id, role, content, session_id)
             VALUES (:1, :2, :3, :4)",
            &[&self.agent_id, &role, &content, &session_key],
        )?;
        conn.commit()?;
        debug!("Appended transcript ({role}) for session '{session_key}'");
        Ok(())
    }

    /// List all session keys for this agent, most recently updated first.
    pub fn list_sessions(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let rows = conn.query(
            "SELECT session_key FROM ZERO_SESSIONS WHERE agent_id = :1 ORDER BY updated_at DESC",
            &[&self.agent_id],
        )?;
        let mut keys = Vec::new();
        for row_result in rows {
            let row = row_result?;
            keys.push(row.get::<_, String>(0)?);
        }
        Ok(keys)
    }

    /// Delete a session and its transcripts.
    pub fn delete_session(&self, session_key: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        // Delete transcripts first (foreign-key-like relationship via session_id)
        conn.execute(
            "DELETE FROM ZERO_TRANSCRIPTS WHERE session_id = :1 AND agent_id = :2",
            &[&session_key, &self.agent_id],
        )?;
        let stmt = conn.execute(
            "DELETE FROM ZERO_SESSIONS WHERE session_key = :1 AND agent_id = :2",
            &[&session_key, &self.agent_id],
        )?;
        let deleted = stmt.row_count()? > 0;
        conn.commit()?;
        if deleted {
            info!("Deleted session '{session_key}'");
        }
        Ok(deleted)
    }
}
