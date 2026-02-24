use crate::config::Config;
use crate::memory::{Memory, MemoryCategory};
use anyhow::{bail, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct SourceEntry {
    key: String,
    content: String,
    category: MemoryCategory,
}

#[derive(Debug, Default)]
struct MigrationStats {
    from_sqlite: usize,
    from_markdown: usize,
    imported: usize,
    skipped_unchanged: usize,
    renamed_conflicts: usize,
}

pub async fn handle_command(command: crate::MigrateCommands, config: &Config) -> Result<()> {
    match command {
        crate::MigrateCommands::Openclaw { source, dry_run } => {
            migrate_openclaw_memory(config, source, dry_run).await
        }
    }
}

async fn migrate_openclaw_memory(
    config: &Config,
    source_workspace: Option<PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let source_workspace = resolve_openclaw_workspace(source_workspace)?;
    if !source_workspace.exists() {
        bail!(
            "OpenClaw workspace not found at {}. Pass --source <path> if needed.",
            source_workspace.display()
        );
    }

    if paths_equal(&source_workspace, &config.workspace_dir) {
        bail!("Source workspace matches current ZeroClaw workspace; refusing self-migration");
    }

    let mut stats = MigrationStats::default();
    let entries = collect_source_entries(&source_workspace, &mut stats)?;

    if entries.is_empty() {
        println!(
            "No importable memory found in {}",
            source_workspace.display()
        );
        println!("Checked for: MEMORY.md, memory/*.md");
        println!("Note: SQLite source reading requires the legacy build. Use markdown export first.");
        return Ok(());
    }

    if dry_run {
        println!("Dry run: OpenClaw migration preview");
        println!("  Source: {}", source_workspace.display());
        println!("  Target: {}", config.workspace_dir.display());
        println!("  Candidates: {}", entries.len());
        println!("    - from markdown: {}", stats.from_markdown);
        if stats.from_sqlite > 0 {
            println!("    - from sqlite:   {} (skipped -- rusqlite removed)", stats.from_sqlite);
        }
        println!();
        println!("Run without --dry-run to import these entries.");
        return Ok(());
    }

    // In the Oracle-only build, migration target must be Oracle.
    // For now, bail until Oracle memory factory is wired (Task 9).
    bail!(
        "Migration target requires Oracle memory backend.\n\
         Run `zeroclaw setup-oracle` to configure Oracle AI Database, then retry."
    );
}

fn collect_source_entries(
    source_workspace: &Path,
    stats: &mut MigrationStats,
) -> Result<Vec<SourceEntry>> {
    let mut entries = Vec::new();

    // SQLite source reading is no longer available (rusqlite removed).
    // Log if brain.db exists so users know they need to export first.
    let sqlite_path = source_workspace.join("memory").join("brain.db");
    if sqlite_path.exists() {
        tracing::warn!(
            "Found source brain.db at {} but SQLite reading is disabled in Oracle-only build. \
             Export to markdown first, or use the legacy ZeroClaw build for migration.",
            sqlite_path.display()
        );
        stats.from_sqlite = 0;
    }

    let markdown_entries = read_openclaw_markdown_entries(source_workspace)?;
    stats.from_markdown = markdown_entries.len();
    entries.extend(markdown_entries);

    // De-dup exact duplicates to make re-runs deterministic.
    let mut seen = HashSet::new();
    entries.retain(|entry| {
        let sig = format!("{}\u{0}{}\u{0}{}", entry.key, entry.content, entry.category);
        seen.insert(sig)
    });

    Ok(entries)
}

fn read_openclaw_markdown_entries(source_workspace: &Path) -> Result<Vec<SourceEntry>> {
    let mut all = Vec::new();

    let core_path = source_workspace.join("MEMORY.md");
    if core_path.exists() {
        let content = fs::read_to_string(&core_path)?;
        all.extend(parse_markdown_file(
            &core_path,
            &content,
            MemoryCategory::Core,
            "openclaw_core",
        ));
    }

    let daily_dir = source_workspace.join("memory");
    if daily_dir.exists() {
        for file in fs::read_dir(&daily_dir)? {
            let file = file?;
            let path = file.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let content = fs::read_to_string(&path)?;
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("openclaw_daily");
            all.extend(parse_markdown_file(
                &path,
                &content,
                MemoryCategory::Daily,
                stem,
            ));
        }
    }

    Ok(all)
}

#[allow(clippy::needless_pass_by_value)]
fn parse_markdown_file(
    _path: &Path,
    content: &str,
    default_category: MemoryCategory,
    stem: &str,
) -> Vec<SourceEntry> {
    let mut entries = Vec::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let line = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let (key, text) = match parse_structured_memory_line(line) {
            Some((k, v)) => (normalize_key(k, idx), v.trim().to_string()),
            None => (
                format!("openclaw_{stem}_{}", idx + 1),
                line.trim().to_string(),
            ),
        };

        if text.is_empty() {
            continue;
        }

        entries.push(SourceEntry {
            key,
            content: text,
            category: default_category.clone(),
        });
    }

    entries
}

fn parse_structured_memory_line(line: &str) -> Option<(&str, &str)> {
    if !line.starts_with("**") {
        return None;
    }

    let rest = line.strip_prefix("**")?;
    let key_end = rest.find("**:")?;
    let key = rest.get(..key_end)?.trim();
    let value = rest.get(key_end + 3..)?.trim();

    if key.is_empty() || value.is_empty() {
        return None;
    }

    Some((key, value))
}

fn parse_category(raw: &str) -> MemoryCategory {
    match raw.trim().to_ascii_lowercase().as_str() {
        "core" | "" => MemoryCategory::Core,
        "daily" => MemoryCategory::Daily,
        "conversation" => MemoryCategory::Conversation,
        other => MemoryCategory::Custom(other.to_string()),
    }
}

fn normalize_key(key: &str, fallback_idx: usize) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return format!("openclaw_{fallback_idx}");
    }
    trimmed.to_string()
}

#[allow(dead_code)]
async fn next_available_key(memory: &dyn Memory, base: &str) -> Result<String> {
    for i in 1..=10_000 {
        let candidate = format!("{base}__openclaw_{i}");
        if memory.get(&candidate).await?.is_none() {
            return Ok(candidate);
        }
    }

    bail!("Unable to allocate non-conflicting key for '{base}'")
}

fn resolve_openclaw_workspace(source: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(src) = source {
        return Ok(src);
    }

    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .context("Could not find home directory")?;

    Ok(home.join(".openclaw").join("workspace"))
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

use anyhow::Context;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_structured_markdown_line() {
        let line = "**user_pref**: likes Rust";
        let parsed = parse_structured_memory_line(line).unwrap();
        assert_eq!(parsed.0, "user_pref");
        assert_eq!(parsed.1, "likes Rust");
    }

    #[test]
    fn parse_unstructured_markdown_generates_key() {
        let entries = parse_markdown_file(
            Path::new("/tmp/MEMORY.md"),
            "- plain note",
            MemoryCategory::Core,
            "core",
        );
        assert_eq!(entries.len(), 1);
        assert!(entries[0].key.starts_with("openclaw_core_"));
        assert_eq!(entries[0].content, "plain note");
    }

    #[test]
    fn parse_category_handles_all_variants() {
        assert_eq!(parse_category("core"), MemoryCategory::Core);
        assert_eq!(parse_category("daily"), MemoryCategory::Daily);
        assert_eq!(parse_category("conversation"), MemoryCategory::Conversation);
        assert_eq!(parse_category(""), MemoryCategory::Core);
        assert_eq!(
            parse_category("custom_type"),
            MemoryCategory::Custom("custom_type".to_string())
        );
    }

    #[test]
    fn parse_category_case_insensitive() {
        assert_eq!(parse_category("CORE"), MemoryCategory::Core);
        assert_eq!(parse_category("Daily"), MemoryCategory::Daily);
        assert_eq!(parse_category("CONVERSATION"), MemoryCategory::Conversation);
    }

    #[test]
    fn normalize_key_handles_empty_string() {
        let key = normalize_key("", 42);
        assert_eq!(key, "openclaw_42");
    }

    #[test]
    fn normalize_key_trims_whitespace() {
        let key = normalize_key("  my_key  ", 0);
        assert_eq!(key, "my_key");
    }

    #[test]
    fn parse_structured_markdown_rejects_empty_key() {
        assert!(parse_structured_memory_line("****:value").is_none());
    }

    #[test]
    fn parse_structured_markdown_rejects_empty_value() {
        assert!(parse_structured_memory_line("**key**:").is_none());
    }

    #[test]
    fn parse_structured_markdown_rejects_no_stars() {
        assert!(parse_structured_memory_line("key: value").is_none());
    }

    #[test]
    fn collect_entries_from_markdown_only() {
        let source = TempDir::new().unwrap();
        let core_file = source.path().join("MEMORY.md");
        fs::write(&core_file, "- **pref**: likes Rust\n").unwrap();

        let mut stats = MigrationStats::default();
        let entries = collect_source_entries(source.path(), &mut stats).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(stats.from_markdown, 1);
        assert_eq!(entries[0].content, "likes Rust");
    }
}
