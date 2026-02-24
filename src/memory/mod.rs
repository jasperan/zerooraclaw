pub mod backend;
pub mod chunker;
pub mod cli;
pub mod embeddings;
pub mod hygiene;
pub mod response_cache;
pub mod snapshot;
pub mod traits;
pub mod vector;

pub use backend::{
    classify_memory_backend, default_memory_backend_key, memory_backend_profile,
    selectable_memory_backends, MemoryBackendKind, MemoryBackendProfile,
};
pub use response_cache::ResponseCache;
pub use traits::Memory;
#[allow(unused_imports)]
pub use traits::{MemoryCategory, MemoryEntry};
#[cfg(test)]
pub use traits::InMemoryTestBackend;

use crate::config::{EmbeddingRouteConfig, MemoryConfig, OracleConfig, StorageProviderConfig};
use crate::oracle::{OracleConnectionManager, OracleEmbedding, OracleMemory};
use std::path::Path;
use std::sync::Arc;

/// Legacy auto-save key used for model-authored assistant summaries.
/// These entries are treated as untrusted context and should not be re-injected.
pub fn is_assistant_autosave_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    normalized == "assistant_resp" || normalized.starts_with("assistant_resp_")
}

/// Derive the effective backend name from config + optional storage override.
///
/// In the Oracle-only build, this always returns "oracle" but preserves the
/// interface for callers that pass storage provider overrides.
pub fn effective_memory_backend_name(
    memory_backend: &str,
    _storage_provider: Option<&StorageProviderConfig>,
) -> String {
    // In Oracle-only mode, always use Oracle regardless of what's configured.
    let _ = memory_backend;
    "oracle".to_string()
}

/// Create an Oracle-backed memory using an existing connection manager.
///
/// This is the primary factory for production code paths that already have
/// an `OracleConnectionManager` in hand.
pub fn create_oracle_memory(
    conn_manager: &OracleConnectionManager,
) -> anyhow::Result<Box<dyn Memory>> {
    let embedder: Arc<dyn embeddings::EmbeddingProvider> = Arc::new(
        OracleEmbedding::new(conn_manager.conn(), conn_manager.onnx_model()),
    );
    Ok(Box::new(OracleMemory::new(
        conn_manager.conn(),
        conn_manager.agent_id(),
        embedder,
    )))
}

/// Create an Oracle-backed memory from an `OracleConfig`.
///
/// Establishes a new connection, initializes the schema (idempotent),
/// and returns a boxed `Memory` trait object.
pub fn create_oracle_memory_from_config(
    oracle_config: &OracleConfig,
) -> anyhow::Result<Box<dyn Memory>> {
    let mgr = OracleConnectionManager::new(oracle_config)?;

    // Initialize schema (idempotent — silently skips existing objects).
    {
        let conn = mgr.conn();
        let guard = conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Connection lock poisoned: {e}"))?;
        crate::oracle::schema::init_schema(&guard, mgr.agent_id())?;
    }

    create_oracle_memory(&mgr)
}

/// Factory: create the right memory backend from config.
///
/// In the Oracle-only build, creates an `OracleMemory` instance.
/// The `OracleConfig` is read from the global config via `crate::config::Config`.
/// This function reads `ZEROCLAW_ORACLE_PASSWORD` from the environment as a
/// fallback when the password is empty in config.
pub fn create_memory(
    _config: &MemoryConfig,
    _workspace_dir: &Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Box<dyn Memory>> {
    let oracle_config = resolve_oracle_config()?;
    create_oracle_memory_from_config(&oracle_config)
}

/// Factory: create memory with optional storage-provider override.
///
/// In the Oracle-only build, the storage provider config is ignored;
/// Oracle connection parameters come from the `[oracle]` config section.
pub fn create_memory_with_storage(
    _config: &MemoryConfig,
    _storage_provider: Option<&StorageProviderConfig>,
    _workspace_dir: &Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Box<dyn Memory>> {
    let oracle_config = resolve_oracle_config()?;
    create_oracle_memory_from_config(&oracle_config)
}

/// Factory: create memory with optional storage-provider override and embedding routes.
///
/// In the Oracle-only build, embedding routes are not used — the Oracle ONNX
/// model name comes from `[oracle].onnx_model`.
pub fn create_memory_with_storage_and_routes(
    _config: &MemoryConfig,
    _embedding_routes: &[EmbeddingRouteConfig],
    _storage_provider: Option<&StorageProviderConfig>,
    _workspace_dir: &Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Box<dyn Memory>> {
    let oracle_config = resolve_oracle_config()?;
    create_oracle_memory_from_config(&oracle_config)
}

/// Factory: create a memory backend suitable for migration operations.
///
/// In the Oracle-only build, migration target is always Oracle.
pub fn create_memory_for_migration(
    _backend: &str,
    _workspace_dir: &Path,
) -> anyhow::Result<Box<dyn Memory>> {
    let oracle_config = resolve_oracle_config()?;
    create_oracle_memory_from_config(&oracle_config)
}

/// Resolve the Oracle config, falling back to environment variables.
///
/// This allows the factory functions (which don't receive a full `Config`)
/// to still build an `OracleConfig` from defaults + env overrides.
fn resolve_oracle_config() -> anyhow::Result<OracleConfig> {
    let mut cfg = OracleConfig::default();

    // Apply environment variable overrides (same pattern as setup-oracle)
    if let Ok(host) = std::env::var("ZEROORACLAW_ORACLE_HOST") {
        cfg.host = host;
    }
    if let Ok(port) = std::env::var("ZEROORACLAW_ORACLE_PORT") {
        if let Ok(p) = port.parse::<u16>() {
            cfg.port = p;
        }
    }
    if let Ok(service) = std::env::var("ZEROORACLAW_ORACLE_SERVICE") {
        cfg.service = service;
    }
    if let Ok(user) = std::env::var("ZEROORACLAW_ORACLE_USER") {
        cfg.user = user;
    }
    if let Ok(password) = std::env::var("ZEROORACLAW_ORACLE_PASSWORD") {
        cfg.password = password;
    }
    if let Ok(mode) = std::env::var("ZEROORACLAW_ORACLE_MODE") {
        cfg.mode = mode;
    }
    if let Ok(dsn) = std::env::var("ZEROORACLAW_ORACLE_DSN") {
        cfg.dsn = Some(dsn);
    }
    if let Ok(model) = std::env::var("ZEROORACLAW_ORACLE_ONNX_MODEL") {
        cfg.onnx_model = model;
    }
    if let Ok(agent_id) = std::env::var("ZEROORACLAW_ORACLE_AGENT_ID") {
        cfg.agent_id = agent_id;
    }

    Ok(cfg)
}

/// Factory: create an optional response cache from config.
pub fn create_response_cache(
    config: &MemoryConfig,
    workspace_dir: &Path,
) -> Option<ResponseCache> {
    if !config.response_cache_enabled {
        return None;
    }

    match ResponseCache::new(
        workspace_dir,
        config.response_cache_ttl_minutes,
        config.response_cache_max_entries,
    ) {
        Ok(cache) => {
            tracing::info!(
                "Response cache enabled (TTL: {}min, max: {} entries)",
                config.response_cache_ttl_minutes,
                config.response_cache_max_entries
            );
            Some(cache)
        }
        Err(e) => {
            tracing::warn!("Response cache disabled due to error: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_autosave_key_detection_matches_legacy_patterns() {
        assert!(is_assistant_autosave_key("assistant_resp"));
        assert!(is_assistant_autosave_key("assistant_resp_1234"));
        assert!(is_assistant_autosave_key("ASSISTANT_RESP_abcd"));
        assert!(!is_assistant_autosave_key("assistant_response"));
        assert!(!is_assistant_autosave_key("user_msg_1234"));
    }

    #[test]
    fn effective_backend_always_returns_oracle() {
        assert_eq!(effective_memory_backend_name("sqlite", None), "oracle");
        assert_eq!(effective_memory_backend_name("postgres", None), "oracle");
        assert_eq!(effective_memory_backend_name("oracle", None), "oracle");

        let storage = StorageProviderConfig {
            provider: "postgres".into(),
            ..StorageProviderConfig::default()
        };
        assert_eq!(
            effective_memory_backend_name("sqlite", Some(&storage)),
            "oracle"
        );
    }
}
