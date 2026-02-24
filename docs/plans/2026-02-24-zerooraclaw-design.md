# ZeroOraClaw Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fork ZeroClaw (Rust AI agent runtime) and replace ALL storage backends with Oracle AI Database as the exclusive persistence layer.

**Architecture:** Full fork of ZeroClaw with a new `src/oracle/` module implementing the `Memory` trait against Oracle tables (ZERO_* prefix). Remove SQLite, PostgreSQL, and markdown backends entirely. Add Oracle-specific CLI commands (setup-oracle, oracle-inspect), Oracle embedding provider (in-database ONNX via VECTOR_EMBEDDING), and OCI deployment templates.

**Tech Stack:** Rust 1.87+, `oracle` crate (rust-oracle) for Oracle OCI driver, tokio async runtime, TOML config, Oracle AI Database Free / Autonomous Database, ONNX ALL_MINILM_L12_V2 (384-dim embeddings)

---

## Task 1: Clone ZeroClaw and Set Up Project Scaffold

**Files:**
- Create: `Cargo.toml` (modified from zeroclaw)
- Create: `CLAUDE.md`
- Create: `README.md`
- Create: `src/main.rs` (modified from zeroclaw)
- Create: `src/oracle/mod.rs`

**Step 1: Clone zeroclaw into zerooraclaw**

```bash
cd /home/ubuntu/git/zerooraclaw
# Copy zeroclaw source
cp -r /tmp/zeroclaw-ref/* .
cp -r /tmp/zeroclaw-ref/.* . 2>/dev/null || true
```

**Step 2: Remove .git and reinitialize**

```bash
cd /home/ubuntu/git/zerooraclaw
rm -rf .git
git init
```

**Step 3: Rename package in Cargo.toml**

Replace:
```toml
name = "zeroclaw"
```
With:
```toml
name = "zerooraclaw"
```

Update description, repository, authors.

**Step 4: Add oracle crate dependency to Cargo.toml**

Add to `[dependencies]`:
```toml
# Oracle AI Database - the ONLY storage backend
oracle = { version = "0.6", features = ["chrono"] }
```

Remove `rusqlite` and `postgres` dependencies. Remove `memory-postgres` feature.

**Step 5: Create `src/oracle/mod.rs` stub**

```rust
pub mod connection;
pub mod schema;
pub mod embedding;
pub mod memory;
pub mod session;
pub mod state;
pub mod config_store;
pub mod prompt;
pub mod vector;
```

**Step 6: Create `CLAUDE.md`**

Document project conventions, Oracle-only architecture, build/test commands.

**Step 7: Commit**

```bash
git add -A
git commit -m "Fork ZeroClaw as ZeroOraClaw with Oracle AI Database scaffold"
```

---

## Task 2: Oracle Connection Manager

**Files:**
- Create: `src/oracle/connection.rs`
- Modify: `src/config/schema.rs` (add OracleConfig)

**Step 1: Add OracleConfig to config schema**

In `src/config/schema.rs`, add:
```rust
/// Oracle AI Database configuration (`[oracle]`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OracleConfig {
    /// Connection mode: "freepdb" or "adb"
    #[serde(default = "default_oracle_mode")]
    pub mode: String,
    /// Database host (FreePDB mode)
    #[serde(default = "default_oracle_host")]
    pub host: String,
    /// Database port (FreePDB mode)
    #[serde(default = "default_oracle_port")]
    pub port: u16,
    /// Service name (FreePDB mode)
    #[serde(default = "default_oracle_service")]
    pub service: String,
    /// Database user
    #[serde(default = "default_oracle_user")]
    pub user: String,
    /// Database password
    #[serde(default)]
    pub password: String,
    /// ONNX model name for in-database embeddings
    #[serde(default = "default_oracle_onnx_model")]
    pub onnx_model: String,
    /// Agent ID for multi-agent isolation
    #[serde(default = "default_oracle_agent_id")]
    pub agent_id: String,
    /// Full DSN for ADB mode
    #[serde(default)]
    pub dsn: Option<String>,
    /// Wallet path for ADB mTLS
    #[serde(default)]
    pub wallet_path: Option<String>,
    /// Max pool connections
    #[serde(default = "default_oracle_max_connections")]
    pub max_connections: u32,
}
```

Add the `oracle` field to the main `Config` struct.

**Step 2: Write connection.rs**

```rust
use oracle::{Connection, Connector};
use std::sync::{Arc, Mutex};
use crate::config::OracleConfig;

pub struct OracleConnectionManager {
    config: OracleConfig,
    conn: Arc<Mutex<Connection>>,
}

impl OracleConnectionManager {
    pub fn new(config: &OracleConfig) -> anyhow::Result<Self> {
        let conn = match config.mode.as_str() {
            "adb" => Self::connect_adb(config)?,
            _ => Self::connect_freepdb(config)?,
        };
        Ok(Self {
            config: config.clone(),
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn connect_freepdb(config: &OracleConfig) -> anyhow::Result<Connection> {
        let connect_string = format!(
            "//{}:{}/{}",
            config.host, config.port, config.service
        );
        let conn = Connector::new(&config.user, &config.password, &connect_string)
            .connect()?;
        Ok(conn)
    }

    fn connect_adb(config: &OracleConfig) -> anyhow::Result<Connection> {
        let dsn = config.dsn.as_deref()
            .ok_or_else(|| anyhow::anyhow!("ADB mode requires 'dsn' in [oracle] config"))?;
        let conn = Connector::new(&config.user, &config.password, dsn)
            .connect()?;
        Ok(conn)
    }

    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    pub fn ping(&self) -> bool {
        self.conn.lock().map_or(false, |c| c.ping().is_ok())
    }
}
```

**Step 3: Commit**

```bash
git add src/oracle/connection.rs src/config/schema.rs
git commit -m "Add Oracle connection manager with FreePDB and ADB modes"
```

---

## Task 3: Oracle Schema Initialization

**Files:**
- Create: `src/oracle/schema.rs`

**Step 1: Write schema.rs with 8 ZERO_* tables**

```rust
use oracle::Connection;
use tracing::{info, warn};

const TABLES: &[(&str, &str)] = &[
    ("ZERO_META", "CREATE TABLE ZERO_META (
        schema_version NUMBER DEFAULT 1,
        agent_id VARCHAR2(64) NOT NULL,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_meta PRIMARY KEY (agent_id)
    )"),
    ("ZERO_MEMORIES", "CREATE TABLE ZERO_MEMORIES (
        memory_id VARCHAR2(64) NOT NULL,
        agent_id VARCHAR2(64) NOT NULL,
        key VARCHAR2(512) NOT NULL,
        content CLOB NOT NULL,
        category VARCHAR2(64) DEFAULT 'core',
        importance NUMBER(3,2) DEFAULT 0.5,
        embedding VECTOR(384, FLOAT32),
        session_id VARCHAR2(128),
        access_count NUMBER DEFAULT 0,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        accessed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_memories PRIMARY KEY (memory_id)
    )"),
    ("ZERO_DAILY_NOTES", "CREATE TABLE ZERO_DAILY_NOTES (
        note_id VARCHAR2(64) NOT NULL,
        agent_id VARCHAR2(64) NOT NULL,
        note_date DATE NOT NULL,
        content CLOB NOT NULL,
        embedding VECTOR(384, FLOAT32),
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_daily_notes PRIMARY KEY (note_id)
    )"),
    ("ZERO_SESSIONS", "CREATE TABLE ZERO_SESSIONS (
        session_key VARCHAR2(256) NOT NULL,
        agent_id VARCHAR2(64) NOT NULL,
        messages CLOB,
        summary CLOB,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_sessions PRIMARY KEY (session_key, agent_id)
    )"),
    ("ZERO_TRANSCRIPTS", "CREATE TABLE ZERO_TRANSCRIPTS (
        transcript_id NUMBER GENERATED ALWAYS AS IDENTITY,
        session_key VARCHAR2(256) NOT NULL,
        agent_id VARCHAR2(64) NOT NULL,
        role VARCHAR2(32) NOT NULL,
        content CLOB NOT NULL,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_transcripts PRIMARY KEY (transcript_id)
    )"),
    ("ZERO_STATE", "CREATE TABLE ZERO_STATE (
        state_key VARCHAR2(256) NOT NULL,
        agent_id VARCHAR2(64) NOT NULL,
        state_value CLOB,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_state PRIMARY KEY (state_key, agent_id)
    )"),
    ("ZERO_CONFIG", "CREATE TABLE ZERO_CONFIG (
        config_key VARCHAR2(256) NOT NULL,
        agent_id VARCHAR2(64) NOT NULL,
        config_value CLOB,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_config PRIMARY KEY (config_key, agent_id)
    )"),
    ("ZERO_PROMPTS", "CREATE TABLE ZERO_PROMPTS (
        prompt_name VARCHAR2(128) NOT NULL,
        agent_id VARCHAR2(64) NOT NULL,
        content CLOB NOT NULL,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        CONSTRAINT pk_zero_prompts PRIMARY KEY (prompt_name, agent_id)
    )"),
];

const INDEXES: &[(&str, &str)] = &[
    ("IDX_ZERO_MEMORIES_AGENT", "CREATE INDEX IDX_ZERO_MEMORIES_AGENT ON ZERO_MEMORIES(agent_id)"),
    ("IDX_ZERO_MEMORIES_KEY", "CREATE INDEX IDX_ZERO_MEMORIES_KEY ON ZERO_MEMORIES(key, agent_id)"),
    ("IDX_ZERO_MEMORIES_CAT", "CREATE INDEX IDX_ZERO_MEMORIES_CAT ON ZERO_MEMORIES(category, agent_id)"),
    ("IDX_ZERO_MEMORIES_SESSION", "CREATE INDEX IDX_ZERO_MEMORIES_SESSION ON ZERO_MEMORIES(session_id)"),
    ("IDX_ZERO_TRANSCRIPTS_SESSION", "CREATE INDEX IDX_ZERO_TRANSCRIPTS_SESSION ON ZERO_TRANSCRIPTS(session_key, agent_id)"),
];

const VECTOR_INDEXES: &[(&str, &str)] = &[
    ("IDX_ZERO_MEMORIES_VEC", "CREATE VECTOR INDEX IDX_ZERO_MEMORIES_VEC ON ZERO_MEMORIES(embedding) ORGANIZATION NEIGHBOR PARTITIONS DISTANCE COSINE WITH TARGET ACCURACY 95"),
    ("IDX_ZERO_DAILY_NOTES_VEC", "CREATE VECTOR INDEX IDX_ZERO_DAILY_NOTES_VEC ON ZERO_DAILY_NOTES(embedding) ORGANIZATION NEIGHBOR PARTITIONS DISTANCE COSINE WITH TARGET ACCURACY 95"),
];

pub fn init_schema(conn: &Connection, agent_id: &str) -> anyhow::Result<()> {
    for (name, ddl) in TABLES {
        match conn.execute(ddl, &[]) {
            Ok(_) => info!("Created table {name}"),
            Err(e) if e.to_string().contains("ORA-00955") => {
                // Table already exists - idempotent
            }
            Err(e) => return Err(anyhow::anyhow!("Failed to create {name}: {e}")),
        }
    }

    for (name, ddl) in INDEXES {
        match conn.execute(ddl, &[]) {
            Ok(_) => info!("Created index {name}"),
            Err(e) if e.to_string().contains("ORA-01408") || e.to_string().contains("ORA-00955") => {}
            Err(e) => warn!("Index {name} warning: {e}"),
        }
    }

    for (name, ddl) in VECTOR_INDEXES {
        match conn.execute(ddl, &[]) {
            Ok(_) => info!("Created vector index {name}"),
            Err(e) if e.to_string().contains("ORA-01408") || e.to_string().contains("ORA-00955") => {}
            Err(e) => warn!("Vector index {name} warning: {e}"),
        }
    }

    // Seed meta row
    conn.execute(
        "MERGE INTO ZERO_META m USING (SELECT :1 AS agent_id FROM DUAL) s
         ON (m.agent_id = s.agent_id)
         WHEN NOT MATCHED THEN INSERT (agent_id, schema_version) VALUES (s.agent_id, 1)
         WHEN MATCHED THEN UPDATE SET updated_at = CURRENT_TIMESTAMP",
        &[&agent_id],
    )?;
    conn.commit()?;

    info!("Oracle schema initialized for agent '{agent_id}'");
    Ok(())
}
```

**Step 2: Commit**

```bash
git add src/oracle/schema.rs
git commit -m "Add Oracle schema with 8 ZERO_* tables and vector indexes"
```

---

## Task 4: Oracle Embedding Service

**Files:**
- Create: `src/oracle/embedding.rs`

**Step 1: Write embedding.rs with ONNX + API modes**

Implements `EmbeddingProvider` trait using Oracle's `VECTOR_EMBEDDING()` for in-database ONNX embeddings, with API fallback.

```rust
use async_trait::async_trait;
use crate::memory::embeddings::EmbeddingProvider;
use oracle::Connection;
use std::sync::{Arc, Mutex};

/// Oracle in-database ONNX embedding provider
pub struct OracleEmbedding {
    conn: Arc<Mutex<Connection>>,
    model_name: String,
}

impl OracleEmbedding {
    pub fn new(conn: Arc<Mutex<Connection>>, model_name: &str) -> Self {
        Self {
            conn,
            model_name: model_name.to_string(),
        }
    }

    pub fn check_onnx_model(&self) -> anyhow::Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let count: i32 = conn.query_row_as::<i32>(
            "SELECT COUNT(*) FROM USER_MINING_MODELS WHERE MODEL_NAME = :1",
            &[&self.model_name],
        )?;
        Ok(count > 0)
    }
}

#[async_trait]
impl EmbeddingProvider for OracleEmbedding {
    fn name(&self) -> &str {
        "oracle-onnx"
    }

    fn dimensions(&self) -> usize {
        384 // ALL_MINILM_L12_V2
    }

    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let conn = self.conn.clone();
        let model = self.model_name.clone();
        let texts: Vec<String> = texts.iter().map(|t| t.to_string()).collect();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut results = Vec::with_capacity(texts.len());

            for text in &texts {
                let sql = format!(
                    "SELECT TO_VECTOR(VECTOR_EMBEDDING({} USING :1 AS DATA)) FROM DUAL",
                    model
                );
                let row: oracle::Row = conn.query_row(&sql, &[text])?;
                let vec_str: String = row.get(0)?;
                let embedding = parse_oracle_vector(&vec_str)?;
                results.push(embedding);
            }
            Ok(results)
        }).await?
    }
}

/// Parse Oracle vector string "[0.1,0.2,...]" into Vec<f32>
fn parse_oracle_vector(s: &str) -> anyhow::Result<Vec<f32>> {
    let trimmed = s.trim_start_matches('[').trim_end_matches(']');
    let values: Vec<f32> = trimmed
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(values)
}
```

**Step 2: Commit**

```bash
git add src/oracle/embedding.rs
git commit -m "Add Oracle ONNX embedding provider with VECTOR_EMBEDDING support"
```

---

## Task 5: Oracle Memory Implementation (Core)

**Files:**
- Create: `src/oracle/memory.rs`
- Create: `src/oracle/vector.rs`

**Step 1: Write vector.rs helper**

```rust
/// Convert f32 vector to Oracle VECTOR string format
pub fn vec_to_oracle_string(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(|f| format!("{f}")).collect();
    format!("[{}]", parts.join(","))
}

/// Compute cosine similarity (1.0 - distance)
pub fn similarity_from_distance(distance: f64) -> f64 {
    (1.0 - distance).max(0.0)
}
```

**Step 2: Write memory.rs implementing Memory trait**

```rust
use async_trait::async_trait;
use crate::memory::traits::{Memory, MemoryCategory, MemoryEntry};
use crate::memory::embeddings::EmbeddingProvider;
use oracle::Connection;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct OracleMemory {
    conn: Arc<Mutex<Connection>>,
    agent_id: String,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl OracleMemory {
    pub fn new(
        conn: Arc<Mutex<Connection>>,
        agent_id: &str,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            conn,
            agent_id: agent_id.to_string(),
            embedder,
        }
    }
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
        let id = Uuid::new_v4().to_string();
        let cat = category.to_string();
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let key = key.to_string();
        let content = content.to_string();
        let session_id = session_id.map(|s| s.to_string());

        // Generate embedding
        let embedding = self.embedder.embed_one(&content).await.ok();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;

            if let Some(emb) = embedding {
                let vec_str = super::vector::vec_to_oracle_string(&emb);
                conn.execute(
                    "MERGE INTO ZERO_MEMORIES m
                     USING (SELECT :1 AS key, :2 AS agent_id FROM DUAL) s
                     ON (m.key = s.key AND m.agent_id = s.agent_id)
                     WHEN MATCHED THEN UPDATE SET
                       content = :3, category = :4, embedding = TO_VECTOR(:5),
                       session_id = :6, updated_at = CURRENT_TIMESTAMP
                     WHEN NOT MATCHED THEN INSERT
                       (memory_id, agent_id, key, content, category, embedding, session_id)
                       VALUES (:7, :8, :9, :10, :11, TO_VECTOR(:12), :13)",
                    &[&key, &agent_id, &content, &cat, &vec_str,
                      &session_id, &id, &agent_id, &key, &content, &cat, &vec_str, &session_id],
                )?;
            } else {
                conn.execute(
                    "MERGE INTO ZERO_MEMORIES m
                     USING (SELECT :1 AS key, :2 AS agent_id FROM DUAL) s
                     ON (m.key = s.key AND m.agent_id = s.agent_id)
                     WHEN MATCHED THEN UPDATE SET
                       content = :3, category = :4,
                       session_id = :5, updated_at = CURRENT_TIMESTAMP
                     WHEN NOT MATCHED THEN INSERT
                       (memory_id, agent_id, key, content, category, session_id)
                       VALUES (:6, :7, :8, :9, :10, :11)",
                    &[&key, &agent_id, &content, &cat,
                      &session_id, &id, &agent_id, &key, &content, &cat, &session_id],
                )?;
            }
            conn.commit()?;
            Ok(())
        }).await?
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let limit = limit;
        let session_id = session_id.map(|s| s.to_string());

        // Generate query embedding
        let query_embedding = self.embedder.embed_one(query).await.ok();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut entries = Vec::new();

            if let Some(emb) = query_embedding {
                let vec_str = super::vector::vec_to_oracle_string(&emb);
                let sql = if session_id.is_some() {
                    "SELECT memory_id, key, content, category, created_at, session_id,
                            VECTOR_DISTANCE(embedding, TO_VECTOR(:1), COSINE) as distance
                     FROM ZERO_MEMORIES
                     WHERE agent_id = :2 AND session_id = :3 AND embedding IS NOT NULL
                     ORDER BY distance ASC
                     FETCH FIRST :4 ROWS ONLY"
                } else {
                    "SELECT memory_id, key, content, category, created_at, session_id,
                            VECTOR_DISTANCE(embedding, TO_VECTOR(:1), COSINE) as distance
                     FROM ZERO_MEMORIES
                     WHERE agent_id = :2 AND embedding IS NOT NULL
                     ORDER BY distance ASC
                     FETCH FIRST :3 ROWS ONLY"
                };

                let rows = if let Some(ref sid) = session_id {
                    conn.query(sql, &[&vec_str, &agent_id, sid, &(limit as i32)])?
                } else {
                    conn.query(sql, &[&vec_str, &agent_id, &(limit as i32)])?
                };

                for row_result in rows {
                    let row = row_result?;
                    let distance: f64 = row.get("DISTANCE")?;
                    let score = super::vector::similarity_from_distance(distance);
                    if score < 0.3 { continue; }

                    entries.push(MemoryEntry {
                        id: row.get("MEMORY_ID")?,
                        key: row.get("KEY")?,
                        content: row.get("CONTENT")?,
                        category: parse_category(&row.get::<_, String>("CATEGORY")?),
                        timestamp: row.get("CREATED_AT")?,
                        session_id: row.get("SESSION_ID")?,
                        score: Some(score),
                    });
                }
            } else {
                // Fallback: keyword search using LIKE
                let sql = "SELECT memory_id, key, content, category, created_at, session_id
                           FROM ZERO_MEMORIES
                           WHERE agent_id = :1 AND UPPER(content) LIKE '%' || UPPER(:2) || '%'
                           ORDER BY updated_at DESC
                           FETCH FIRST :3 ROWS ONLY";
                let rows = conn.query(sql, &[&agent_id, &query.to_string(), &(limit as i32)])?;

                for row_result in rows {
                    let row = row_result?;
                    entries.push(MemoryEntry {
                        id: row.get("MEMORY_ID")?,
                        key: row.get("KEY")?,
                        content: row.get("CONTENT")?,
                        category: parse_category(&row.get::<_, String>("CATEGORY")?),
                        timestamp: row.get("CREATED_AT")?,
                        session_id: row.get("SESSION_ID")?,
                        score: None,
                    });
                }
            }

            // Update access timestamps
            for entry in &entries {
                let _ = conn.execute(
                    "UPDATE ZERO_MEMORIES SET access_count = access_count + 1,
                            accessed_at = CURRENT_TIMESTAMP WHERE memory_id = :1",
                    &[&entry.id],
                );
            }
            let _ = conn.commit();

            Ok(entries)
        }).await?
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let key = key.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let result = conn.query_row(
                "SELECT memory_id, key, content, category, created_at, session_id
                 FROM ZERO_MEMORIES WHERE key = :1 AND agent_id = :2",
                &[&key, &agent_id],
            );

            match result {
                Ok(row) => Ok(Some(MemoryEntry {
                    id: row.get("MEMORY_ID")?,
                    key: row.get("KEY")?,
                    content: row.get("CONTENT")?,
                    category: parse_category(&row.get::<_, String>("CATEGORY")?),
                    timestamp: row.get("CREATED_AT")?,
                    session_id: row.get("SESSION_ID")?,
                    score: None,
                })),
                Err(_) => Ok(None),
            }
        }).await?
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let cat = category.map(|c| c.to_string());
        let session_id = session_id.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut entries = Vec::new();

            let (sql, params): (String, Vec<Box<dyn oracle::sql_type::ToSql>>) = match (&cat, &session_id) {
                (Some(c), Some(s)) => (
                    "SELECT memory_id, key, content, category, created_at, session_id
                     FROM ZERO_MEMORIES WHERE agent_id = :1 AND category = :2 AND session_id = :3
                     ORDER BY updated_at DESC".to_string(),
                    vec![Box::new(agent_id.clone()), Box::new(c.clone()), Box::new(s.clone())],
                ),
                (Some(c), None) => (
                    "SELECT memory_id, key, content, category, created_at, session_id
                     FROM ZERO_MEMORIES WHERE agent_id = :1 AND category = :2
                     ORDER BY updated_at DESC".to_string(),
                    vec![Box::new(agent_id.clone()), Box::new(c.clone())],
                ),
                (None, Some(s)) => (
                    "SELECT memory_id, key, content, category, created_at, session_id
                     FROM ZERO_MEMORIES WHERE agent_id = :1 AND session_id = :2
                     ORDER BY updated_at DESC".to_string(),
                    vec![Box::new(agent_id.clone()), Box::new(s.clone())],
                ),
                (None, None) => (
                    "SELECT memory_id, key, content, category, created_at, session_id
                     FROM ZERO_MEMORIES WHERE agent_id = :1
                     ORDER BY updated_at DESC".to_string(),
                    vec![Box::new(agent_id.clone())],
                ),
            };

            // Execute with appropriate params
            let rows = conn.query(&sql, params.as_slice())?;
            for row_result in rows {
                let row = row_result?;
                entries.push(MemoryEntry {
                    id: row.get("MEMORY_ID")?,
                    key: row.get("KEY")?,
                    content: row.get("CONTENT")?,
                    category: parse_category(&row.get::<_, String>("CATEGORY")?),
                    timestamp: row.get("CREATED_AT")?,
                    session_id: row.get("SESSION_ID")?,
                    score: None,
                });
            }
            Ok(entries)
        }).await?
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();
        let key = key.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let deleted = conn.execute(
                "DELETE FROM ZERO_MEMORIES WHERE key = :1 AND agent_id = :2",
                &[&key, &agent_id],
            )?;
            conn.commit()?;
            Ok(deleted > 0)
        }).await?
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let conn = self.conn.clone();
        let agent_id = self.agent_id.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            let count: i32 = conn.query_row_as(
                "SELECT COUNT(*) FROM ZERO_MEMORIES WHERE agent_id = :1",
                &[&agent_id],
            )?;
            Ok(count as usize)
        }).await?
    }

    async fn health_check(&self) -> bool {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            conn.lock().map_or(false, |c| c.ping().is_ok())
        }).await.unwrap_or(false)
    }
}

fn parse_category(s: &str) -> MemoryCategory {
    match s {
        "core" => MemoryCategory::Core,
        "daily" => MemoryCategory::Daily,
        "conversation" => MemoryCategory::Conversation,
        other => MemoryCategory::Custom(other.to_string()),
    }
}
```

**Step 3: Commit**

```bash
git add src/oracle/memory.rs src/oracle/vector.rs
git commit -m "Implement Memory trait for Oracle with vector search and keyword fallback"
```

---

## Task 6: Remove Non-Oracle Backends

**Files:**
- Modify: `src/memory/mod.rs`
- Modify: `src/memory/backend.rs`
- Delete: `src/memory/sqlite.rs`
- Delete: `src/memory/postgres.rs`
- Delete: `src/memory/markdown.rs`
- Delete: `src/memory/lucid.rs`
- Delete: `src/memory/none.rs`
- Modify: `Cargo.toml`

**Step 1: Replace backend.rs with Oracle-only backend**

```rust
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MemoryBackendKind {
    Oracle,
}

pub struct MemoryBackendProfile {
    pub key: &'static str,
    pub label: &'static str,
}

const ORACLE_PROFILE: MemoryBackendProfile = MemoryBackendProfile {
    key: "oracle",
    label: "Oracle AI Database — in-database ONNX vector search, the only backend",
};

pub fn default_memory_backend_key() -> &'static str {
    "oracle"
}

pub fn classify_memory_backend(_backend: &str) -> MemoryBackendKind {
    MemoryBackendKind::Oracle
}

pub fn memory_backend_profile(_backend: &str) -> MemoryBackendProfile {
    ORACLE_PROFILE
}
```

**Step 2: Replace mod.rs factory to create Oracle memory**

```rust
pub mod backend;
pub mod chunker;
pub mod cli;
pub mod embeddings;
pub mod hygiene;
pub mod response_cache;
pub mod snapshot;
pub mod traits;
pub mod vector;

pub use backend::{classify_memory_backend, default_memory_backend_key, MemoryBackendKind};
pub use response_cache::ResponseCache;
pub use traits::Memory;
pub use traits::{MemoryCategory, MemoryEntry};

use crate::config::MemoryConfig;
use crate::oracle::{OracleConnectionManager, OracleEmbedding, OracleMemory};
use std::sync::Arc;

/// Factory: create Oracle memory backend (the only backend)
pub fn create_memory(
    conn_manager: &OracleConnectionManager,
    agent_id: &str,
) -> anyhow::Result<Box<dyn Memory>> {
    let embedder: Arc<dyn embeddings::EmbeddingProvider> =
        Arc::new(OracleEmbedding::new(conn_manager.conn(), &agent_id));
    Ok(Box::new(OracleMemory::new(conn_manager.conn(), agent_id, embedder)))
}
```

**Step 3: Remove rusqlite from Cargo.toml, remove memory-postgres feature**

**Step 4: Delete sqlite.rs, postgres.rs, markdown.rs, lucid.rs, none.rs**

**Step 5: Commit**

```bash
git add -A
git commit -m "Remove all non-Oracle backends: Oracle is the exclusive storage"
```

---

## Task 7: Oracle Session and State Stores

**Files:**
- Create: `src/oracle/session.rs`
- Create: `src/oracle/state.rs`
- Create: `src/oracle/config_store.rs`
- Create: `src/oracle/prompt.rs`

**Step 1: Write session.rs**

Store chat sessions as JSON in ZERO_SESSIONS, transcripts in ZERO_TRANSCRIPTS.

**Step 2: Write state.rs**

Key-value store using ZERO_STATE with MERGE INTO for upserts.

**Step 3: Write config_store.rs**

Store/load config JSON using ZERO_CONFIG.

**Step 4: Write prompt.rs**

Store/load system prompts (IDENTITY.md, SOUL.md, etc.) from ZERO_PROMPTS. Seed from workspace files.

**Step 5: Commit**

```bash
git add src/oracle/session.rs src/oracle/state.rs src/oracle/config_store.rs src/oracle/prompt.rs
git commit -m "Add Oracle session, state, config, and prompt stores"
```

---

## Task 8: CLI Commands (setup-oracle, oracle-inspect)

**Files:**
- Modify: `src/main.rs`

**Step 1: Add `SetupOracle` subcommand**

```rust
/// Initialize Oracle AI Database schema and load ONNX model
SetupOracle {
    /// Force re-creation of tables
    #[arg(long)]
    force: bool,
},
```

Implementation: connect to Oracle, call `init_schema()`, load ONNX model, seed prompts from workspace.

**Step 2: Add `OracleInspect` subcommand**

```rust
/// Inspect Oracle AI Database contents
OracleInspect {
    /// Table to inspect: memories, sessions, state, prompts, notes, transcripts
    #[arg(default_value = "all")]
    table: String,
    /// Semantic search query (for memories)
    #[arg(short, long)]
    search: Option<String>,
},
```

Implementation: query each table and display counts + sample data in a formatted dashboard.

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "Add setup-oracle and oracle-inspect CLI commands"
```

---

## Task 9: Wire Oracle Into Agent Loop

**Files:**
- Modify: `src/agent/agent.rs`
- Modify: `src/agent/memory_loader.rs`

**Step 1: Update agent initialization to use Oracle memory**

Replace the memory factory call to use `oracle::create_memory()` instead of the old SQLite-based factory.

**Step 2: Update memory_loader to use Oracle-backed recall**

Ensure the context builder queries Oracle for relevant memories before each LLM call.

**Step 3: Commit**

```bash
git add src/agent/
git commit -m "Wire Oracle memory into agent loop and context builder"
```

---

## Task 10: Docker & Deployment

**Files:**
- Modify: `docker-compose.yml`
- Modify: `Dockerfile`
- Create: `scripts/setup-oracle.sh`
- Create: `deploy/oci/schema.yaml`

**Step 1: Update docker-compose.yml**

Add Oracle Database Free container:
```yaml
services:
  oracle-db:
    image: container-registry.oracle.com/database/free:latest
    environment:
      ORACLE_PWD: ${ORACLE_PWD:-ZeroOraClaw2026}
    ports:
      - "1521:1521"
    volumes:
      - oracle-data:/opt/oracle/oradata
    healthcheck:
      test: ["CMD", "sqlplus", "-s", "sys/$$ORACLE_PWD@FREEPDB1 as sysdba", "<<<", "SELECT 1 FROM DUAL;"]
      interval: 30s
      timeout: 10s
      retries: 10

  zerooraclaw:
    build: .
    depends_on:
      oracle-db:
        condition: service_healthy
    environment:
      ZEROORACLAW_ORACLE_HOST: oracle-db
      ZEROORACLAW_ORACLE_PORT: 1521
      ZEROORACLAW_ORACLE_SERVICE: FREEPDB1
      ZEROORACLAW_ORACLE_USER: zerooraclaw
      ZEROORACLAW_ORACLE_PASSWORD: ${ORACLE_PWD:-ZeroOraClaw2026}
```

**Step 2: Create scripts/setup-oracle.sh**

Automated script: start Oracle container, wait for health, create user, grant permissions, run setup-oracle.

**Step 3: Update Dockerfile**

Add Oracle Instant Client to the runtime stage for the `oracle` crate's OCI driver.

**Step 4: Commit**

```bash
git add docker-compose.yml Dockerfile scripts/ deploy/
git commit -m "Add Docker and OCI deployment with Oracle Database Free"
```

---

## Task 11: Update README and CLAUDE.md

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

**Step 1: Write README.md**

Cover: what ZeroOraClaw is, why Oracle AI Database, quickstart, setup-oracle, oracle-inspect, Docker Compose, OCI deploy.

**Step 2: Finalize CLAUDE.md**

Document all project conventions, Oracle-only architecture, build/test instructions.

**Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "Add README and CLAUDE.md documentation"
```

---

## Task 12: Config Example and Workspace Templates

**Files:**
- Create: `config/config.example.toml`
- Modify: `workspace/IDENTITY.md`
- Modify: `workspace/SOUL.md`

**Step 1: Create config.example.toml with Oracle section**

```toml
# ZeroOraClaw Configuration
default_provider = "ollama"
default_model = "qwen3:latest"
default_temperature = 0.7

[oracle]
mode = "freepdb"
host = "localhost"
port = 1521
service = "FREEPDB1"
user = "zerooraclaw"
password = "ZeroOraClaw2026"
onnx_model = "ALL_MINILM_L12_V2"
agent_id = "default"
max_connections = 5

[memory]
backend = "oracle"
auto_save = true
embedding_provider = "oracle-onnx"
embedding_dimensions = 384
vector_weight = 0.7
keyword_weight = 0.3
min_relevance_score = 0.3
```

**Step 2: Update workspace identity files for ZeroOraClaw branding**

**Step 3: Commit**

```bash
git add config/ workspace/
git commit -m "Add config example and workspace templates for ZeroOraClaw"
```

---

## Summary of Deliverables

| # | Component | Files | Purpose |
|---|-----------|-------|---------|
| 1 | Scaffold | Cargo.toml, src/oracle/mod.rs | Project setup from zeroclaw fork |
| 2 | Connection | src/oracle/connection.rs, config/schema.rs | Oracle pool (FreePDB + ADB) |
| 3 | Schema | src/oracle/schema.rs | 8 ZERO_* tables + vector indexes |
| 4 | Embedding | src/oracle/embedding.rs | ONNX in-database + API fallback |
| 5 | Memory | src/oracle/memory.rs, vector.rs | Memory trait → Oracle |
| 6 | Cleanup | src/memory/*.rs | Remove SQLite/PG/markdown |
| 7 | Stores | src/oracle/session,state,config,prompt.rs | Full Oracle persistence |
| 8 | CLI | src/main.rs | setup-oracle, oracle-inspect |
| 9 | Agent | src/agent/*.rs | Wire Oracle into agent loop |
| 10 | Deploy | docker-compose, Dockerfile, scripts/ | Docker + OCI deployment |
| 11 | Docs | README.md, CLAUDE.md | Documentation |
| 12 | Config | config.example.toml, workspace/ | Templates and branding |
