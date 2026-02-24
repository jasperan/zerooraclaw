//! Response cache -- avoid burning tokens on repeated prompts.
//!
//! This is a lightweight in-memory implementation that replaces the previous
//! SQLite-backed cache. The cache is optional and disabled by default -- users
//! opt in via `[memory] response_cache_enabled = true`.
//!
//! Note: Since we removed the SQLite dependency, this cache is now purely
//! in-memory (not persisted across restarts). A future version may store
//! cache entries in Oracle if persistence is desired.

use anyhow::Result;
use chrono::{Duration, Local};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single cached response entry.
#[derive(Clone)]
struct CacheEntry {
    model: String,
    response: String,
    token_count: u32,
    created_at: chrono::DateTime<chrono::Local>,
    accessed_at: chrono::DateTime<chrono::Local>,
    hit_count: u64,
}

/// Response cache backed by an in-memory HashMap.
///
/// Replaces the previous SQLite-backed cache. Lives alongside the workspace
/// for configuration purposes but does not persist across restarts.
pub struct ResponseCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
    #[allow(dead_code)]
    db_path: PathBuf,
    ttl_minutes: i64,
    max_entries: usize,
}

impl ResponseCache {
    /// Open (or create) the response cache.
    pub fn new(workspace_dir: &Path, ttl_minutes: u32, max_entries: usize) -> Result<Self> {
        let db_dir = workspace_dir.join("memory");
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("response_cache.db");

        Ok(Self {
            entries: Mutex::new(HashMap::new()),
            db_path,
            ttl_minutes: i64::from(ttl_minutes),
            max_entries,
        })
    }

    /// Build a deterministic cache key from model + system prompt + user prompt.
    pub fn cache_key(model: &str, system_prompt: Option<&str>, user_prompt: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(model.as_bytes());
        hasher.update(b"|");
        if let Some(sys) = system_prompt {
            hasher.update(sys.as_bytes());
        }
        hasher.update(b"|");
        hasher.update(user_prompt.as_bytes());
        let hash = hasher.finalize();
        format!("{:064x}", hash)
    }

    /// Look up a cached response. Returns `None` on miss or expired entry.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let now = Local::now();
        let cutoff = now - Duration::minutes(self.ttl_minutes);
        let mut entries = self.entries.lock();

        if let Some(entry) = entries.get_mut(key) {
            if entry.created_at > cutoff {
                entry.hit_count += 1;
                entry.accessed_at = now;
                return Ok(Some(entry.response.clone()));
            }
            // Expired -- remove it
            entries.remove(key);
        }

        Ok(None)
    }

    /// Store a response in the cache.
    pub fn put(&self, key: &str, model: &str, response: &str, token_count: u32) -> Result<()> {
        let now = Local::now();
        let cutoff = now - Duration::minutes(self.ttl_minutes);
        let mut entries = self.entries.lock();

        // Insert or replace
        entries.insert(
            key.to_string(),
            CacheEntry {
                model: model.to_string(),
                response: response.to_string(),
                token_count,
                created_at: now,
                accessed_at: now,
                hit_count: 0,
            },
        );

        // Evict expired entries
        entries.retain(|_, entry| entry.created_at > cutoff);

        // LRU eviction if over max_entries
        while entries.len() > self.max_entries {
            // Find the least recently accessed entry
            let lru_key = entries
                .iter()
                .min_by_key(|(_, entry)| entry.accessed_at)
                .map(|(k, _)| k.clone());

            if let Some(k) = lru_key {
                entries.remove(&k);
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Return cache statistics: (total_entries, total_hits, total_tokens_saved).
    pub fn stats(&self) -> Result<(usize, u64, u64)> {
        let entries = self.entries.lock();
        let count = entries.len();
        let hits: u64 = entries.values().map(|e| e.hit_count).sum();
        let tokens_saved: u64 = entries
            .values()
            .map(|e| u64::from(e.token_count) * e.hit_count)
            .sum();

        Ok((count, hits, tokens_saved))
    }

    /// Wipe the entire cache (useful for `zeroclaw cache clear`).
    pub fn clear(&self) -> Result<usize> {
        let mut entries = self.entries.lock();
        let count = entries.len();
        entries.clear();
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_cache(ttl_minutes: u32) -> (TempDir, ResponseCache) {
        let tmp = TempDir::new().unwrap();
        let cache = ResponseCache::new(tmp.path(), ttl_minutes, 1000).unwrap();
        (tmp, cache)
    }

    #[test]
    fn cache_key_deterministic() {
        let k1 = ResponseCache::cache_key("gpt-4", Some("sys"), "hello");
        let k2 = ResponseCache::cache_key("gpt-4", Some("sys"), "hello");
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn cache_key_varies_by_model() {
        let k1 = ResponseCache::cache_key("gpt-4", None, "hello");
        let k2 = ResponseCache::cache_key("claude-3", None, "hello");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_varies_by_system_prompt() {
        let k1 = ResponseCache::cache_key("gpt-4", Some("You are helpful"), "hello");
        let k2 = ResponseCache::cache_key("gpt-4", Some("You are rude"), "hello");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_varies_by_prompt() {
        let k1 = ResponseCache::cache_key("gpt-4", None, "hello");
        let k2 = ResponseCache::cache_key("gpt-4", None, "goodbye");
        assert_ne!(k1, k2);
    }

    #[test]
    fn put_and_get() {
        let (_tmp, cache) = temp_cache(60);
        let key = ResponseCache::cache_key("gpt-4", None, "What is Rust?");

        cache
            .put(&key, "gpt-4", "Rust is a systems programming language.", 25)
            .unwrap();

        let result = cache.get(&key).unwrap();
        assert_eq!(
            result.as_deref(),
            Some("Rust is a systems programming language.")
        );
    }

    #[test]
    fn miss_returns_none() {
        let (_tmp, cache) = temp_cache(60);
        let result = cache.get("nonexistent_key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn expired_entry_returns_none() {
        let (_tmp, cache) = temp_cache(0); // 0-minute TTL -- everything is instantly expired
        let key = ResponseCache::cache_key("gpt-4", None, "test");

        cache.put(&key, "gpt-4", "response", 10).unwrap();

        // The entry was created with created_at = now(), but TTL is 0 minutes,
        // so cutoff = now() - 0 = now(). The entry's created_at is NOT > cutoff.
        let result = cache.get(&key).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn hit_count_incremented() {
        let (_tmp, cache) = temp_cache(60);
        let key = ResponseCache::cache_key("gpt-4", None, "hello");

        cache.put(&key, "gpt-4", "Hi!", 5).unwrap();

        // 3 hits
        for _ in 0..3 {
            let _ = cache.get(&key).unwrap();
        }

        let (_, total_hits, _) = cache.stats().unwrap();
        assert_eq!(total_hits, 3);
    }

    #[test]
    fn tokens_saved_calculated() {
        let (_tmp, cache) = temp_cache(60);
        let key = ResponseCache::cache_key("gpt-4", None, "explain rust");

        cache.put(&key, "gpt-4", "Rust is...", 100).unwrap();

        // 5 cache hits x 100 tokens = 500 tokens saved
        for _ in 0..5 {
            let _ = cache.get(&key).unwrap();
        }

        let (_, _, tokens_saved) = cache.stats().unwrap();
        assert_eq!(tokens_saved, 500);
    }

    #[test]
    fn lru_eviction() {
        let tmp = TempDir::new().unwrap();
        let cache = ResponseCache::new(tmp.path(), 60, 3).unwrap(); // max 3 entries

        for i in 0..5 {
            let key = ResponseCache::cache_key("gpt-4", None, &format!("prompt {i}"));
            cache
                .put(&key, "gpt-4", &format!("response {i}"), 10)
                .unwrap();
        }

        let (count, _, _) = cache.stats().unwrap();
        assert!(count <= 3, "Should have at most 3 entries after eviction");
    }

    #[test]
    fn clear_wipes_all() {
        let (_tmp, cache) = temp_cache(60);

        for i in 0..10 {
            let key = ResponseCache::cache_key("gpt-4", None, &format!("prompt {i}"));
            cache
                .put(&key, "gpt-4", &format!("response {i}"), 10)
                .unwrap();
        }

        let cleared = cache.clear().unwrap();
        assert_eq!(cleared, 10);

        let (count, _, _) = cache.stats().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn stats_empty_cache() {
        let (_tmp, cache) = temp_cache(60);
        let (count, hits, tokens) = cache.stats().unwrap();
        assert_eq!(count, 0);
        assert_eq!(hits, 0);
        assert_eq!(tokens, 0);
    }

    #[test]
    fn overwrite_same_key() {
        let (_tmp, cache) = temp_cache(60);
        let key = ResponseCache::cache_key("gpt-4", None, "question");

        cache.put(&key, "gpt-4", "answer v1", 20).unwrap();
        cache.put(&key, "gpt-4", "answer v2", 25).unwrap();

        let result = cache.get(&key).unwrap();
        assert_eq!(result.as_deref(), Some("answer v2"));

        let (count, _, _) = cache.stats().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn unicode_prompt_handling() {
        let (_tmp, cache) = temp_cache(60);
        let key = ResponseCache::cache_key("gpt-4", None, "test 🦀");

        cache
            .put(&key, "gpt-4", "Rust is great", 30)
            .unwrap();

        let result = cache.get(&key).unwrap();
        assert_eq!(result.as_deref(), Some("Rust is great"));
    }

    #[test]
    fn cache_handles_zero_max_entries() {
        let tmp = TempDir::new().unwrap();
        let cache = ResponseCache::new(tmp.path(), 60, 0).unwrap();

        let key = ResponseCache::cache_key("gpt-4", None, "test");
        // Should not panic even with max_entries=0
        cache.put(&key, "gpt-4", "response", 10).unwrap();

        let (count, _, _) = cache.stats().unwrap();
        assert_eq!(count, 0, "cache with max_entries=0 should evict everything");
    }

    #[test]
    fn cache_concurrent_reads_no_panic() {
        let tmp = TempDir::new().unwrap();
        let cache = std::sync::Arc::new(ResponseCache::new(tmp.path(), 60, 100).unwrap());

        let key = ResponseCache::cache_key("gpt-4", None, "concurrent");
        cache.put(&key, "gpt-4", "response", 10).unwrap();

        let mut handles = Vec::new();
        for _ in 0..10 {
            let cache = std::sync::Arc::clone(&cache);
            let key = key.clone();
            handles.push(std::thread::spawn(move || {
                let _ = cache.get(&key).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let (_, hits, _) = cache.stats().unwrap();
        assert_eq!(hits, 10, "all concurrent reads should register as hits");
    }
}
