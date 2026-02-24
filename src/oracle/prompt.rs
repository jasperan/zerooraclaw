//! Oracle-backed system prompt store.
//!
//! Persists named prompt templates in `ZERO_PROMPTS`, scoped per agent.
//! Includes `seed_from_workspace` to bootstrap prompts from workspace
//! markdown files (IDENTITY.md, SOUL.md, USER.md, AGENT.md, AGENTS.md).

use oracle::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Persistent prompt store backed by Oracle Database.
pub struct OraclePromptStore {
    conn: Arc<Mutex<Connection>>,
    agent_id: String,
}

impl OraclePromptStore {
    pub fn new(conn: Arc<Mutex<Connection>>, agent_id: &str) -> Self {
        Self {
            conn,
            agent_id: agent_id.to_string(),
        }
    }

    /// Get a prompt by name. Returns `None` if not found.
    pub fn get_prompt(&self, name: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        match conn.query_row(
            "SELECT content FROM ZERO_PROMPTS WHERE prompt_name = :1 AND agent_id = :2",
            &[&name, &self.agent_id],
        ) {
            Ok(row) => {
                let content: Option<String> = row.get(0)?;
                Ok(content)
            }
            Err(oracle::Error::NoDataFound) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to get prompt '{name}': {e}")),
        }
    }

    /// Set a prompt by name (upsert).
    pub fn set_prompt(&self, name: &str, content: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        conn.execute(
            "MERGE INTO ZERO_PROMPTS p
             USING (SELECT :1 AS prompt_name, :2 AS agent_id FROM DUAL) src
             ON (p.prompt_name = src.prompt_name AND p.agent_id = src.agent_id)
             WHEN MATCHED THEN UPDATE SET content = :3, updated_at = CURRENT_TIMESTAMP
             WHEN NOT MATCHED THEN INSERT (prompt_name, agent_id, content)
                VALUES (:4, :5, :6)",
            &[
                &name,
                &self.agent_id,
                &content,
                &name,
                &self.agent_id,
                &content,
            ],
        )?;
        conn.commit()?;
        debug!("Prompt set: '{name}'");
        Ok(())
    }

    /// List all prompt names for this agent.
    pub fn list_prompts(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let rows = conn.query(
            "SELECT prompt_name FROM ZERO_PROMPTS WHERE agent_id = :1 ORDER BY prompt_name",
            &[&self.agent_id],
        )?;
        let mut names = Vec::new();
        for row_result in rows {
            names.push(row_result?.get::<_, String>(0)?);
        }
        Ok(names)
    }

    /// Seed prompts from workspace `.md` files.
    ///
    /// Looks for `IDENTITY.md`, `SOUL.md`, `USER.md`, `AGENT.md`, and
    /// `AGENTS.md` in `workspace_dir`.  Each non-empty file is upserted as
    /// a prompt with the base name (e.g. "IDENTITY").
    ///
    /// Returns the number of prompts seeded.
    pub fn seed_from_workspace(&self, workspace_dir: &Path) -> anyhow::Result<usize> {
        let prompt_files = [
            ("IDENTITY", "IDENTITY.md"),
            ("SOUL", "SOUL.md"),
            ("USER", "USER.md"),
            ("AGENT", "AGENT.md"),
            ("AGENTS", "AGENTS.md"),
        ];

        let mut count = 0;
        for (name, filename) in &prompt_files {
            let path = workspace_dir.join(filename);
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) if !content.trim().is_empty() => {
                        self.set_prompt(name, content.trim())?;
                        info!("Seeded prompt '{name}' from {filename}");
                        count += 1;
                    }
                    Ok(_) => warn!("Skipping empty prompt file: {filename}"),
                    Err(e) => warn!("Failed to read {filename}: {e}"),
                }
            }
        }
        Ok(count)
    }
}
