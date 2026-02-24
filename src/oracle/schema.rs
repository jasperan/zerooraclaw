//! Oracle schema initialization and migration.
//!
//! Creates the 8 `ZERO_*` tables, regular indexes, and vector indexes
//! required by ZeroOraClaw.  All DDL is idempotent — existing objects
//! are silently skipped via ORA-00955 / ORA-01408 error handling.

use oracle::Connection;
use tracing::{debug, info, warn};

// ── ORA error codes we intentionally ignore ─────────────────────
/// ORA-00955: name is already used by an existing object
const ORA_NAME_ALREADY_USED: i32 = 955;
/// ORA-01408: such column list already indexed
const ORA_COLUMN_ALREADY_INDEXED: i32 = 1408;

// ── Table DDL ───────────────────────────────────────────────────

const CREATE_ZERO_META: &str = "
CREATE TABLE ZERO_META (
    agent_id        VARCHAR2(128)   NOT NULL,
    schema_version  NUMBER(10)      DEFAULT 1 NOT NULL,
    created_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_meta PRIMARY KEY (agent_id)
)";

const CREATE_ZERO_MEMORIES: &str = "
CREATE TABLE ZERO_MEMORIES (
    memory_id       VARCHAR2(64)    NOT NULL,
    agent_id        VARCHAR2(128)   NOT NULL,
    key             VARCHAR2(512)   NOT NULL,
    content         CLOB            NOT NULL,
    category        VARCHAR2(64)    DEFAULT 'core' NOT NULL,
    session_id      VARCHAR2(128),
    embedding       VECTOR(384, FLOAT32),
    importance      NUMBER(5,2)     DEFAULT 0.5,
    access_count    NUMBER(10)      DEFAULT 0,
    created_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_memories PRIMARY KEY (memory_id)
)";

const CREATE_ZERO_DAILY_NOTES: &str = "
CREATE TABLE ZERO_DAILY_NOTES (
    note_id         VARCHAR2(64)    NOT NULL,
    agent_id        VARCHAR2(128)   NOT NULL,
    note_date       DATE            DEFAULT TRUNC(SYSDATE) NOT NULL,
    title           VARCHAR2(512),
    content         CLOB            NOT NULL,
    embedding       VECTOR(384, FLOAT32),
    created_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_daily_notes PRIMARY KEY (note_id)
)";

const CREATE_ZERO_SESSIONS: &str = "
CREATE TABLE ZERO_SESSIONS (
    session_key     VARCHAR2(256)   NOT NULL,
    agent_id        VARCHAR2(128)   NOT NULL,
    messages        CLOB,
    started_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_sessions PRIMARY KEY (session_key, agent_id)
)";

const CREATE_ZERO_TRANSCRIPTS: &str = "
CREATE TABLE ZERO_TRANSCRIPTS (
    transcript_id   NUMBER          GENERATED ALWAYS AS IDENTITY,
    agent_id        VARCHAR2(128)   NOT NULL,
    role            VARCHAR2(32)    NOT NULL,
    content         CLOB            NOT NULL,
    session_id      VARCHAR2(128),
    created_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_transcripts PRIMARY KEY (transcript_id)
)";

const CREATE_ZERO_STATE: &str = "
CREATE TABLE ZERO_STATE (
    state_key       VARCHAR2(256)   NOT NULL,
    agent_id        VARCHAR2(128)   NOT NULL,
    value           CLOB,
    updated_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_state PRIMARY KEY (state_key, agent_id)
)";

const CREATE_ZERO_CONFIG: &str = "
CREATE TABLE ZERO_CONFIG (
    config_key      VARCHAR2(256)   NOT NULL,
    agent_id        VARCHAR2(128)   NOT NULL,
    config_value    CLOB,
    updated_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_config PRIMARY KEY (config_key, agent_id)
)";

const CREATE_ZERO_PROMPTS: &str = "
CREATE TABLE ZERO_PROMPTS (
    prompt_name     VARCHAR2(256)   NOT NULL,
    agent_id        VARCHAR2(128)   NOT NULL,
    content         CLOB            NOT NULL,
    version         NUMBER(10)      DEFAULT 1 NOT NULL,
    updated_at      TIMESTAMP       DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT pk_zero_prompts PRIMARY KEY (prompt_name, agent_id)
)";

// ── Regular index DDL ───────────────────────────────────────────

const INDEXES: &[&str] = &[
    "CREATE INDEX idx_zero_memories_agent ON ZERO_MEMORIES(agent_id)",
    "CREATE INDEX idx_zero_memories_key ON ZERO_MEMORIES(key)",
    "CREATE INDEX idx_zero_memories_category ON ZERO_MEMORIES(category)",
    "CREATE INDEX idx_zero_memories_session ON ZERO_MEMORIES(session_id)",
    "CREATE INDEX idx_zero_daily_notes_agent ON ZERO_DAILY_NOTES(agent_id)",
    "CREATE INDEX idx_zero_sessions_agent ON ZERO_SESSIONS(agent_id)",
    "CREATE INDEX idx_zero_transcripts_agent ON ZERO_TRANSCRIPTS(agent_id)",
    "CREATE INDEX idx_zero_transcripts_session ON ZERO_TRANSCRIPTS(session_id)",
    "CREATE INDEX idx_zero_state_agent ON ZERO_STATE(agent_id)",
    "CREATE INDEX idx_zero_config_agent ON ZERO_CONFIG(agent_id)",
    "CREATE INDEX idx_zero_prompts_agent ON ZERO_PROMPTS(agent_id)",
];

// ── Vector index DDL ────────────────────────────────────────────

const VECTOR_INDEXES: &[&str] = &[
    "CREATE VECTOR INDEX vidx_zero_memories_emb ON ZERO_MEMORIES(embedding)
     ORGANIZATION NEIGHBOR PARTITIONS
     DISTANCE COSINE
     WITH TARGET ACCURACY 95",
    "CREATE VECTOR INDEX vidx_zero_daily_notes_emb ON ZERO_DAILY_NOTES(embedding)
     ORGANIZATION NEIGHBOR PARTITIONS
     DISTANCE COSINE
     WITH TARGET ACCURACY 95",
];

// ── Helpers ─────────────────────────────────────────────────────

/// Execute DDL, silently ignoring "already exists" errors.
fn exec_ddl_idempotent(conn: &Connection, sql: &str, ignore_codes: &[i32]) -> anyhow::Result<()> {
    match conn.execute(sql, &[]) {
        Ok(_) => Ok(()),
        Err(ref e) => {
            if let Some(db_err) = e.db_error() {
                if ignore_codes.contains(&db_err.code()) {
                    debug!("DDL skipped (ORA-{}): {}", db_err.code(), db_err.message().trim());
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("DDL failed: {e}\nSQL: {sql}"))
                }
            } else {
                Err(anyhow::anyhow!("DDL failed: {e}\nSQL: {sql}"))
            }
        }
    }
}

/// Seed a ZERO_META row for this agent (MERGE = upsert).
fn seed_meta(conn: &Connection, agent_id: &str) -> anyhow::Result<()> {
    let sql = "
        MERGE INTO ZERO_META m
        USING (SELECT :1 AS agent_id FROM DUAL) src
        ON (m.agent_id = src.agent_id)
        WHEN NOT MATCHED THEN
            INSERT (agent_id, schema_version, created_at, updated_at)
            VALUES (src.agent_id, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        WHEN MATCHED THEN
            UPDATE SET m.updated_at = CURRENT_TIMESTAMP
    ";
    conn.execute(sql, &[&agent_id])?;
    Ok(())
}

// ── Public API ──────────────────────────────────────────────────

/// Initialise the full ZeroOraClaw schema idempotently.
///
/// The caller must hold the `Mutex<Connection>` lock and pass the
/// inner `&Connection`.  This function commits on success.
pub fn init_schema(conn: &Connection, agent_id: &str) -> anyhow::Result<()> {
    info!("Initialising Oracle schema for agent '{agent_id}'...");

    // 1. Create tables (ignore ORA-00955 "name already used")
    let table_stmts = [
        ("ZERO_META", CREATE_ZERO_META),
        ("ZERO_MEMORIES", CREATE_ZERO_MEMORIES),
        ("ZERO_DAILY_NOTES", CREATE_ZERO_DAILY_NOTES),
        ("ZERO_SESSIONS", CREATE_ZERO_SESSIONS),
        ("ZERO_TRANSCRIPTS", CREATE_ZERO_TRANSCRIPTS),
        ("ZERO_STATE", CREATE_ZERO_STATE),
        ("ZERO_CONFIG", CREATE_ZERO_CONFIG),
        ("ZERO_PROMPTS", CREATE_ZERO_PROMPTS),
    ];

    for (name, ddl) in &table_stmts {
        debug!("Creating table {name}...");
        exec_ddl_idempotent(conn, ddl, &[ORA_NAME_ALREADY_USED])?;
    }

    // 2. Create regular indexes (ignore ORA-00955 / ORA-01408)
    for idx_ddl in INDEXES {
        exec_ddl_idempotent(conn, idx_ddl, &[ORA_NAME_ALREADY_USED, ORA_COLUMN_ALREADY_INDEXED])?;
    }

    // 3. Create vector indexes (ignore ORA-00955 / ORA-01408)
    for vidx_ddl in VECTOR_INDEXES {
        exec_ddl_idempotent(
            conn,
            vidx_ddl,
            &[ORA_NAME_ALREADY_USED, ORA_COLUMN_ALREADY_INDEXED],
        )?;
    }

    // 4. Seed meta row for this agent
    seed_meta(conn, agent_id)?;

    // 5. Commit the transaction
    conn.commit()?;
    info!("Oracle schema ready (agent '{agent_id}')");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_ddl_contains_primary_keys() {
        assert!(CREATE_ZERO_META.contains("pk_zero_meta"));
        assert!(CREATE_ZERO_MEMORIES.contains("pk_zero_memories"));
        assert!(CREATE_ZERO_DAILY_NOTES.contains("pk_zero_daily_notes"));
        assert!(CREATE_ZERO_SESSIONS.contains("pk_zero_sessions"));
        assert!(CREATE_ZERO_TRANSCRIPTS.contains("pk_zero_transcripts"));
        assert!(CREATE_ZERO_STATE.contains("pk_zero_state"));
        assert!(CREATE_ZERO_CONFIG.contains("pk_zero_config"));
        assert!(CREATE_ZERO_PROMPTS.contains("pk_zero_prompts"));
    }

    #[test]
    fn table_ddl_contains_vector_columns() {
        assert!(CREATE_ZERO_MEMORIES.contains("VECTOR(384, FLOAT32)"));
        assert!(CREATE_ZERO_DAILY_NOTES.contains("VECTOR(384, FLOAT32)"));
    }

    #[test]
    fn index_count_is_correct() {
        // 4 on MEMORIES + 1 on DAILY_NOTES + 1 SESSIONS + 2 TRANSCRIPTS
        // + 1 STATE + 1 CONFIG + 1 PROMPTS = 11
        assert_eq!(INDEXES.len(), 11);
    }

    #[test]
    fn vector_index_count_is_correct() {
        assert_eq!(VECTOR_INDEXES.len(), 2);
    }

    #[test]
    fn vector_indexes_use_cosine_distance() {
        for vidx in VECTOR_INDEXES {
            assert!(vidx.contains("COSINE"), "Vector index missing COSINE: {vidx}");
            assert!(
                vidx.contains("TARGET ACCURACY 95"),
                "Vector index missing TARGET ACCURACY: {vidx}"
            );
        }
    }
}
