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

use crate::config::{EmbeddingRouteConfig, MemoryConfig, StorageProviderConfig};
use std::path::Path;

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

/// Factory: create the right memory backend from config.
///
/// In the Oracle-only build, this will create an `OracleMemory` instance.
/// Currently returns an error until Task 9 wires the Oracle factory.
pub fn create_memory(
    _config: &MemoryConfig,
    _workspace_dir: &Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Box<dyn Memory>> {
    anyhow::bail!(
        "Oracle memory backend not yet wired. Run `zeroclaw setup-oracle` to configure Oracle AI Database."
    )
}

/// Factory: create memory with optional storage-provider override.
///
/// In the Oracle-only build, the storage provider config is used for Oracle
/// connection parameters. Currently returns an error until Task 9.
pub fn create_memory_with_storage(
    _config: &MemoryConfig,
    _storage_provider: Option<&StorageProviderConfig>,
    _workspace_dir: &Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Box<dyn Memory>> {
    anyhow::bail!(
        "Oracle memory backend not yet wired. Run `zeroclaw setup-oracle` to configure Oracle AI Database."
    )
}

/// Factory: create memory with optional storage-provider override and embedding routes.
///
/// In the Oracle-only build, embedding routes may be used for Oracle ONNX models.
/// Currently returns an error until Task 9.
pub fn create_memory_with_storage_and_routes(
    _config: &MemoryConfig,
    _embedding_routes: &[EmbeddingRouteConfig],
    _storage_provider: Option<&StorageProviderConfig>,
    _workspace_dir: &Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Box<dyn Memory>> {
    anyhow::bail!(
        "Oracle memory backend not yet wired. Run `zeroclaw setup-oracle` to configure Oracle AI Database."
    )
}

/// Factory: create a memory backend suitable for migration operations.
///
/// In the Oracle-only build, migration target must be Oracle.
/// Currently returns an error until Task 9.
pub fn create_memory_for_migration(
    _backend: &str,
    _workspace_dir: &Path,
) -> anyhow::Result<Box<dyn Memory>> {
    anyhow::bail!(
        "Oracle memory backend not yet wired for migration. Run `zeroclaw setup-oracle` first."
    )
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
