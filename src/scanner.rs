//! Scanner — walks a directory tree producing collections + unknowns.
//!
//! At every directory the scanner:
//!   1. Hard-skips if the path is in the rules' skip list.
//!   2. Asks the rules engine for a classification.
//!   3a. Matched → creates a collection; recurses unless `stop`.
//!   3b. No match → records an unknown (with shallow preview) and does NOT
//!       descend. User classifies later (CLI/UI) and a rescan / reclassify
//!       fills in the children.
//!
//! Scan always starts with a reclassification pass: unknowns whose path now
//! matches a rule are promoted to collections and removed from the queue.

use anyhow::Result;
use console::style;
use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

use crate::db::Database;
use crate::models::{Collection, ExtensionCount, PrivacyLevel, Unknown};
use crate::rules::{MatchResult, RulesEngine};

/// Scan the given path. Runs reclassification first, then walks.
pub fn scan(db: &mut Database, path: &Path, _interactive: bool) -> Result<()> {
    let engine = RulesEngine::load();

    let promoted = db.with_transaction(|db| reclassify(db, &engine))?;
    if promoted > 0 {
        println!(
            "{} {} previously-unknown paths reclassified",
            style("↑").green(),
            promoted
        );
    }

    println!("{} {}", style("Scanning").cyan().bold(), path.display());

    let location_id = db.get_or_create_location(path)?;

    // One transaction for the whole walk — autocommit per row is ~100x slower.
    let stats = db.with_transaction(|db| -> Result<ScanStats> {
        let mut ctx = ScanCtx {
            db,
            engine: &engine,
            location_id,
            stats: ScanStats::default(),
        };
        scan_dir(&mut ctx, path, None, true)?;
        Ok(ctx.stats)
    })?;

    println!(
        "\n{} {} collections, {} unknowns, {} hard-skipped",
        style("✓").green(),
        stats.collections,
        stats.unknowns,
        stats.skipped,
    );

    Ok(())
}

/// Re-run the rules against every stored unknown. Paths that now match
/// become collections; unknowns are removed on success.
/// Returns the number of unknowns promoted.
pub fn reclassify(db: &Database, engine: &RulesEngine) -> Result<u64> {
    let unknowns = db.list_unknowns()?;
    let mut promoted = 0u64;

    for u in unknowns {
        let path = std::path::PathBuf::from(&u.path);
        if !path.is_dir() {
            db.remove_unknown_by_id(u.id)?;
            continue;
        }
        if engine.should_skip(&path) {
            db.remove_unknown_by_id(u.id)?;
            continue;
        }
        let Some(result) = engine.match_path(&path) else {
            continue;
        };
        // Parent linking happens on the next scan; reclassify only flips
        // the row from unknown to classified.
        let collection = build_collection(engine, u.location_id, None, &path, &result);
        db.upsert_collection(&collection)?;
        db.remove_unknown_by_id(u.id)?;
        promoted += 1;
    }

    Ok(promoted)
}

// ---------- Walk ----------

struct ScanCtx<'a> {
    db: &'a Database,
    engine: &'a RulesEngine,
    location_id: i64,
    stats: ScanStats,
}

#[derive(Default)]
struct ScanStats {
    collections: u64,
    unknowns: u64,
    skipped: u64,
}

fn scan_dir(
    ctx: &mut ScanCtx,
    path: &Path,
    parent_id: Option<i64>,
    is_root: bool,
) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    if ctx.engine.should_skip(path) {
        ctx.stats.skipped += 1;
        return Ok(());
    }

    let matched = ctx.engine.match_path(path);

    // If this path becomes a collection, its id flows to children as their parent.
    let mut next_parent = parent_id;

    match matched {
        Some(ref m) => {
            let collection =
                build_collection(ctx.engine, ctx.location_id, parent_id, path, m);
            let tag_str = collection
                .tags
                .iter()
                .map(|t| t.render())
                .collect::<Vec<_>>()
                .join(", ");
            let id = ctx.db.upsert_collection(&collection)?;
            ctx.db.remove_unknown_at_path(&collection.path)?;
            ctx.stats.collections += 1;
            next_parent = Some(id);
            println!(
                "  {} {}  {}  [{}]",
                style("→").green(),
                path.display(),
                style(collection.base_type.as_str()).yellow(),
                tag_str,
            );
        }
        None if is_root => {
            // Unmatched root: explore its children but don't record the root itself.
        }
        None => {
            record_unknown(ctx, path)?;
            return Ok(()); // discovery: don't descend into unknowns
        }
    }

    let should_recurse = match &matched {
        Some(m) => !m.stop,
        None => true, // only reached when is_root
    };

    if should_recurse {
        for child in list_visible_children(path) {
            scan_dir(ctx, &child, next_parent, false)?;
        }
    }

    Ok(())
}

fn record_unknown(ctx: &mut ScanCtx, path: &Path) -> Result<()> {
    let preview = preview_directory(path);
    let now = now_secs();
    let parent_path = path
        .parent()
        .map(|p| p.to_string_lossy().to_string());

    let u = Unknown {
        id: 0,
        location_id: ctx.location_id,
        path: path.to_string_lossy().to_string(),
        parent_path,
        discovered_at: now,
        file_count: preview.file_count,
        dir_count: preview.dir_count,
        total_size: preview.total_size,
        top_extensions: preview.top_extensions,
    };
    ctx.db.upsert_unknown(&u)?;
    ctx.stats.unknowns += 1;
    println!(
        "  {} {}  ({} files, {} dirs)",
        style("?").dim(),
        path.display(),
        preview.file_count,
        preview.dir_count,
    );
    Ok(())
}

fn list_visible_children(path: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let child = entry.path();
        if !child.is_dir() {
            continue;
        }
        let name = child
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        out.push(child);
    }
    out.sort();
    out
}

// ---------- Collection building ----------

fn build_collection(
    engine: &RulesEngine,
    location_id: i64,
    parent_id: Option<i64>,
    path: &Path,
    m: &MatchResult,
) -> Collection {
    // Always shallow: a recursive walk on a classified leaf (say /usr or a
    // 500GB music album) can dominate scan time. Accurate aggregate size is
    // computed on demand elsewhere.
    let (file_count, total_size) = dir_stats_shallow(path);

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let privacy = detect_privacy(engine, path);
    let identifier = collection_identifier(path);

    Collection {
        id: 0,
        parent_id,
        location_id,
        path: path.to_string_lossy().to_string(),
        name,
        base_type: m.base_type,
        tags: m.tags.clone(),
        privacy,
        identifier,
        total_size,
        file_count,
        child_count: 0,
        manifest_hash: None,
        indexed_at: now_secs(),
    }
}

fn detect_privacy(engine: &RulesEngine, path: &Path) -> PrivacyLevel {
    if path.join(".fili-confidential").exists() || path.join(".confidential").exists() {
        return PrivacyLevel::Confidential;
    }
    if path.join(".fili-private").exists() || path.join(".private").exists() {
        return PrivacyLevel::Personal;
    }
    if path.join(".fili-public").exists() {
        return PrivacyLevel::Public;
    }
    engine.privacy_for(path).unwrap_or(PrivacyLevel::Public)
}

fn collection_identifier(path: &Path) -> Option<String> {
    let git_config = path.join(".git/config");
    if let Ok(content) = std::fs::read_to_string(&git_config) {
        for line in content.lines() {
            if let Some(url) = line.trim().strip_prefix("url = ") {
                return Some(url.to_string());
            }
        }
    }
    let steam_appid = path.join("steam_appid.txt");
    if let Ok(content) = std::fs::read_to_string(&steam_appid) {
        return Some(format!("steam:{}", content.trim()));
    }
    None
}

// ---------- Stats helpers ----------

fn dir_stats_shallow(path: &Path) -> (u64, u64) {
    let mut count = 0u64;
    let mut size = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    count += 1;
                    size += meta.len();
                }
            }
        }
    }
    (count, size)
}

struct Preview {
    file_count: u64,
    dir_count: u64,
    total_size: u64,
    top_extensions: Vec<ExtensionCount>,
}

fn preview_directory(path: &Path) -> Preview {
    const SAMPLE_CAP: usize = 500;

    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut total_size = 0u64;
    let mut ext_counts: HashMap<String, u64> = HashMap::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten().take(SAMPLE_CAP) {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                dir_count += 1;
            } else if ft.is_file() {
                file_count += 1;
                if let Ok(meta) = entry.metadata() {
                    total_size += meta.len();
                }
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if let Some(dot) = name.rfind('.') {
                    let ext = name[dot + 1..].to_lowercase();
                    *ext_counts.entry(ext).or_insert(0) += 1;
                }
            }
        }
    }

    let mut exts: Vec<ExtensionCount> = ext_counts
        .into_iter()
        .map(|(ext, count)| ExtensionCount { ext, count })
        .collect();
    exts.sort_by(|a, b| b.count.cmp(&a.count));
    exts.truncate(5);

    Preview {
        file_count,
        dir_count,
        total_size,
        top_extensions: exts,
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
