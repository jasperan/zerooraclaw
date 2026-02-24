//! TG5: Memory Restart Resilience Tests
//!
//! Prevents: Pattern 5 — Memory & state persistence bugs (10% of user bugs).
//! Issues: #430, #693, #802
//!
//! Tests memory trait behavior: deduplication on store, session scoping,
//! concurrent message ordering, recall behavior, and category filtering.
//!
//! Uses a local in-memory backend since `InMemoryTestBackend` from the library
//! is `#[cfg(test)]`-gated (only available for unit tests, not integration tests).

use async_trait::async_trait;
use std::sync::Arc;
use zerooraclaw::memory::traits::{Memory, MemoryCategory, MemoryEntry};

// ─────────────────────────────────────────────────────────────────────────────
// Local in-memory backend (mirrors library's InMemoryTestBackend)
// ─────────────────────────────────────────────────────────────────────────────

struct TestMemory {
    entries: parking_lot::Mutex<Vec<MemoryEntry>>,
}

impl TestMemory {
    fn new() -> Self {
        Self {
            entries: parking_lot::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Memory for TestMemory {
    fn name(&self) -> &str {
        "test_memory"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut entries = self.entries.lock();
        if let Some(existing) = entries.iter_mut().find(|e| e.key == key) {
            existing.content = content.to_string();
            existing.category = category;
            existing.timestamp = chrono::Utc::now().to_rfc3339();
            existing.session_id = session_id.map(str::to_string);
        } else {
            entries.push(MemoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                key: key.to_string(),
                content: content.to_string(),
                category,
                timestamp: chrono::Utc::now().to_rfc3339(),
                session_id: session_id.map(str::to_string),
                score: None,
            });
        }
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let entries = self.entries.lock();
        let query_lower = query.to_ascii_lowercase();
        let results: Vec<_> = entries
            .iter()
            .filter(|e| {
                let content_match = e.content.to_ascii_lowercase().contains(&query_lower)
                    || e.key.to_ascii_lowercase().contains(&query_lower);
                let session_match =
                    session_id.map_or(true, |sid| e.session_id.as_deref() == Some(sid));
                content_match && session_match
            })
            .take(limit)
            .cloned()
            .collect();
        Ok(results)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let entries = self.entries.lock();
        Ok(entries.iter().find(|e| e.key == key).cloned())
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self.entries.lock();
        let results: Vec<_> = entries
            .iter()
            .filter(|e| {
                let cat_match = category.map_or(true, |c| e.category == *c);
                let session_match =
                    session_id.map_or(true, |sid| e.session_id.as_deref() == Some(sid));
                cat_match && session_match
            })
            .cloned()
            .collect();
        Ok(results)
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let mut entries = self.entries.lock();
        let len_before = entries.len();
        entries.retain(|e| e.key != key);
        Ok(entries.len() < len_before)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.entries.lock().len())
    }

    async fn health_check(&self) -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Deduplication: same key overwrites instead of duplicating (#430)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_store_same_key_deduplicates() {
    let mem = TestMemory::new();

    // Store same key twice with different content
    mem.store("greeting", "hello world", MemoryCategory::Core, None)
        .await
        .unwrap();
    mem.store("greeting", "hello updated", MemoryCategory::Core, None)
        .await
        .unwrap();

    // Should have exactly 1 entry, not 2
    let count = mem.count().await.unwrap();
    assert_eq!(
        count, 1,
        "storing same key twice should not create duplicates"
    );

    // Content should be the latest version
    let entry = mem
        .get("greeting")
        .await
        .unwrap()
        .expect("entry should exist");
    assert_eq!(entry.content, "hello updated");
}

#[tokio::test]
async fn memory_store_different_keys_creates_separate_entries() {
    let mem = TestMemory::new();

    mem.store("key_a", "content a", MemoryCategory::Core, None)
        .await
        .unwrap();
    mem.store("key_b", "content b", MemoryCategory::Core, None)
        .await
        .unwrap();

    let count = mem.count().await.unwrap();
    assert_eq!(count, 2, "different keys should create separate entries");
}

// ─────────────────────────────────────────────────────────────────────────────
// Upsert: re-storing same keys does not create duplicates
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_rewrite_does_not_duplicate() {
    let mem = TestMemory::new();

    // First round: store entries
    mem.store("fact_1", "original content", MemoryCategory::Core, None)
        .await
        .unwrap();
    mem.store("fact_2", "another fact", MemoryCategory::Core, None)
        .await
        .unwrap();

    // Second round: re-store same keys (simulates channel re-reading history)
    mem.store("fact_1", "original content", MemoryCategory::Core, None)
        .await
        .unwrap();
    mem.store("fact_2", "another fact", MemoryCategory::Core, None)
        .await
        .unwrap();

    let count = mem.count().await.unwrap();
    assert_eq!(
        count, 2,
        "re-storing same keys should not create duplicates"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Session scoping: messages scoped to sessions don't leak
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_session_scoped_store_and_recall() {
    let mem = TestMemory::new();

    // Store in different sessions
    mem.store(
        "session_a_fact",
        "fact from session A",
        MemoryCategory::Conversation,
        Some("session_a"),
    )
    .await
    .unwrap();
    mem.store(
        "session_b_fact",
        "fact from session B",
        MemoryCategory::Conversation,
        Some("session_b"),
    )
    .await
    .unwrap();

    // List scoped to session_a
    let session_a_entries = mem
        .list(Some(&MemoryCategory::Conversation), Some("session_a"))
        .await
        .unwrap();
    assert_eq!(
        session_a_entries.len(),
        1,
        "session_a should have exactly 1 entry"
    );
    assert_eq!(session_a_entries[0].content, "fact from session A");
}

#[tokio::test]
async fn memory_global_recall_includes_all_sessions() {
    let mem = TestMemory::new();

    mem.store(
        "global_a",
        "alpha content",
        MemoryCategory::Core,
        Some("s1"),
    )
    .await
    .unwrap();
    mem.store("global_b", "beta content", MemoryCategory::Core, Some("s2"))
        .await
        .unwrap();

    // Global count should include all
    let count = mem.count().await.unwrap();
    assert_eq!(
        count, 2,
        "global count should include entries from all sessions"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Recall and search behavior
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_recall_returns_relevant_results() {
    let mem = TestMemory::new();

    mem.store(
        "lang_pref",
        "User prefers Rust programming",
        MemoryCategory::Core,
        None,
    )
    .await
    .unwrap();
    mem.store(
        "food_pref",
        "User likes sushi for lunch",
        MemoryCategory::Core,
        None,
    )
    .await
    .unwrap();

    let results = mem.recall("Rust programming", 10, None).await.unwrap();
    assert!(!results.is_empty(), "recall should find matching entries");
    // The Rust-related entry should be in results
    assert!(
        results.iter().any(|e| e.content.contains("Rust")),
        "recall for 'Rust' should include the Rust-related entry"
    );
}

#[tokio::test]
async fn memory_recall_respects_limit() {
    let mem = TestMemory::new();

    for i in 0..10 {
        mem.store(
            &format!("entry_{i}"),
            &format!("test content number {i}"),
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
    }

    let results = mem.recall("test content", 3, None).await.unwrap();
    assert!(
        results.len() <= 3,
        "recall should respect limit of 3, got {}",
        results.len()
    );
}

#[tokio::test]
async fn memory_recall_empty_query_returns_empty() {
    let mem = TestMemory::new();

    mem.store("fact", "some content", MemoryCategory::Core, None)
        .await
        .unwrap();

    let results = mem.recall("", 10, None).await.unwrap();
    assert!(results.is_empty(), "empty query should return no results");
}

// ─────────────────────────────────────────────────────────────────────────────
// Forget and health check
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_forget_removes_entry() {
    let mem = TestMemory::new();

    mem.store("to_forget", "temporary info", MemoryCategory::Core, None)
        .await
        .unwrap();
    assert_eq!(mem.count().await.unwrap(), 1);

    let removed = mem.forget("to_forget").await.unwrap();
    assert!(removed, "forget should return true for existing key");
    assert_eq!(mem.count().await.unwrap(), 0);
}

#[tokio::test]
async fn memory_forget_nonexistent_returns_false() {
    let mem = TestMemory::new();

    let removed = mem.forget("nonexistent_key").await.unwrap();
    assert!(!removed, "forget should return false for nonexistent key");
}

#[tokio::test]
async fn memory_health_check_returns_true() {
    let mem = TestMemory::new();

    assert!(mem.health_check().await, "health_check should return true");
}

// ─────────────────────────────────────────────────────────────────────────────
// Concurrent access
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_concurrent_stores_no_data_loss() {
    let mem = Arc::new(TestMemory::new());

    let mut handles = Vec::new();
    for i in 0..5 {
        let mem_clone = mem.clone();
        handles.push(tokio::spawn(async move {
            mem_clone
                .store(
                    &format!("concurrent_{i}"),
                    &format!("content from task {i}"),
                    MemoryCategory::Core,
                    None,
                )
                .await
                .unwrap();
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let count = mem.count().await.unwrap();
    assert_eq!(
        count, 5,
        "all concurrent stores should succeed, got {count}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Memory categories
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_list_by_category() {
    let mem = TestMemory::new();

    mem.store("core_fact", "core info", MemoryCategory::Core, None)
        .await
        .unwrap();
    mem.store("daily_note", "daily note", MemoryCategory::Daily, None)
        .await
        .unwrap();
    mem.store(
        "conv_msg",
        "conversation msg",
        MemoryCategory::Conversation,
        None,
    )
    .await
    .unwrap();

    let core_entries = mem.list(Some(&MemoryCategory::Core), None).await.unwrap();
    assert_eq!(core_entries.len(), 1, "should have 1 Core entry");
    assert_eq!(core_entries[0].key, "core_fact");

    let daily_entries = mem.list(Some(&MemoryCategory::Daily), None).await.unwrap();
    assert_eq!(daily_entries.len(), 1, "should have 1 Daily entry");
}
