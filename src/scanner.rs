//! Scanner — walks a directory tree producing entries (collections + items)
//! plus unknowns.
//!
//! At every directory the scanner:
//!
//! - Hard-skips if the path is in the rules' skip list.
//! - Asks the rules engine for a classification.
//! - Matched → creates an entry; recurses unless `stop` is set.
//! - No match → records an unknown (with shallow preview) and does NOT
//!   descend. User classifies later (CLI/UI) and a rescan / reclassify
//!   fills in the children.
//!
//! Scan always starts with a reclassification pass: unknowns whose path now
//! matches a rule are promoted to entries and removed from the queue.

use anyhow::Result;
use console::style;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::db::Database;
use crate::models::{BaseType, Entry, ExtensionCount, PrivacyLevel, Unknown};
use crate::rules::{MatchResult, RulesEngine};

/// Options for a scan.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOptions {
    /// Maximum recursion depth relative to the scan root. `None` = unlimited.
    pub max_depth: Option<u32>,
    /// Index direct files inside every classified collection using the
    /// extension map in rules.json. Off by default because a music library
    /// can add tens of thousands of rows.
    pub index_files: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanSummary {
    pub collections: u64,
    pub items: u64,
    pub files: u64,
    pub unknowns: u64,
    pub skipped: u64,
}

/// Scan the given path. Runs reclassification first, then walks.
pub fn scan_with(
    db: &mut Database,
    path: &Path,
    _interactive: bool,
    opts: ScanOptions,
) -> Result<ScanSummary> {
    // Resolve symlinks so that scanning ~/Games (a symlink to /media/games/Games)
    // and /media/games/Games both land on the same location and produce the
    // same entry paths, instead of duplicating every child under two prefixes.
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = canonical.as_path();

    let engine = RulesEngine::load();

    match crate::drives::enumerate() {
        Ok(drives) => {
            let active: Vec<String> = drives
                .iter()
                .filter_map(|d| d.current_mount.clone())
                .collect();
            db.with_transaction(|db| -> Result<()> {
                for d in &drives {
                    db.upsert_drive(d)?;
                }
                db.clear_stale_mounts(&active)?;
                Ok(())
            })?;
            let count = db.list_drives()?.len();
            println!(
                "{} {} drive{} known",
                style("💾"),
                count,
                if count == 1 { "" } else { "s" }
            );
        }
        Err(e) => eprintln!("{} drive enumeration skipped: {}", style("!").yellow(), e),
    }

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

    let stats = db.with_transaction(|db| -> Result<ScanStats> {
        let mut ctx = ScanCtx {
            db,
            engine: &engine,
            location_id,
            max_depth: opts.max_depth,
            index_files: opts.index_files,
            stats: ScanStats::default(),
            home_scopes: Vec::new(),
        };
        scan_dir(&mut ctx, path, None, 0, true)?;
        Ok(ctx.stats)
    })?;

    println!(
        "\n{} {} collections, {} items, {} files, {} unknowns, {} hard-skipped",
        style("✓").green(),
        stats.collections,
        stats.items,
        stats.files,
        stats.unknowns,
        stats.skipped,
    );

    Ok(ScanSummary {
        collections: stats.collections,
        items: stats.items,
        files: stats.files,
        unknowns: stats.unknowns,
        skipped: stats.skipped,
    })
}

/// Re-run rules against every stored unknown.
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
        let entry = build_entry(engine, u.location_id, None, &path, &result);
        db.upsert_entry(&entry)?;
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
    max_depth: Option<u32>,
    index_files: bool,
    stats: ScanStats,
    /// Stack of home-tagged ancestors discovered during the walk. The user's
    /// actual $HOME is always considered by the engine — this only holds
    /// additional scopes (backups, cloned homes, etc.) so `<home>/...` rules
    /// apply inside them.
    home_scopes: Vec<PathBuf>,
}

#[derive(Default)]
struct ScanStats {
    collections: u64,
    items: u64,
    files: u64,
    unknowns: u64,
    skipped: u64,
}

fn scan_dir(
    ctx: &mut ScanCtx,
    path: &Path,
    parent_id: Option<i64>,
    depth: u32,
    is_root: bool,
) -> Result<()> {
    if !path.is_dir() {
        return Ok(());
    }
    if ctx.engine.should_skip_scoped(path, &ctx.home_scopes) {
        ctx.stats.skipped += 1;
        return Ok(());
    }

    let matched = ctx.engine.match_path_scoped(path, &ctx.home_scopes);

    let mut next_parent = parent_id;

    match matched {
        Some(ref m) => {
            let entry = build_entry(ctx.engine, ctx.location_id, parent_id, path, m);
            let tag_str = entry
                .tags
                .iter()
                .map(|t| t.render())
                .collect::<Vec<_>>()
                .join(", ");
            let id = ctx.db.upsert_entry(&entry)?;
            ctx.db.remove_unknown_at_path(&entry.path)?;
            let is_item = entry.is_item;
            if is_item {
                ctx.stats.items += 1;
            } else {
                ctx.stats.collections += 1;
            }
            next_parent = Some(id);
            let marker = if is_item { "●" } else { "◆" };
            println!(
                "  {} {}  {}  [{}]",
                style(marker).green(),
                path.display(),
                style(entry.base_type.as_str()).yellow(),
                tag_str,
            );

            // Index direct files when opted in and this is a collection.
            // Items are atomic — their internal files aren't meaningful rows.
            if ctx.index_files && !is_item {
                index_files_in(ctx, path, id, entry.base_type)?;
            }
        }
        None if is_root => {
            // Unmatched root: explore its children but don't record the root itself.
        }
        None => {
            record_unknown(ctx, path)?;
            return Ok(()); // discovery: don't descend into unknowns
        }
    }

    let at_depth_limit = matches!(ctx.max_depth, Some(limit) if depth >= limit);
    let should_recurse = !at_depth_limit
        && match &matched {
            Some(m) => !m.stop,
            None => true, // only reached when is_root
        };

    if should_recurse {
        let pushed_scope = match &matched {
            Some(m) if m.base_type == BaseType::Home => {
                ctx.home_scopes.push(path.to_path_buf());
                true
            }
            _ => false,
        };
        for child in list_visible_children(path) {
            scan_dir(ctx, &child, next_parent, depth + 1, false)?;
        }
        if pushed_scope {
            ctx.home_scopes.pop();
        }
    }

    Ok(())
}

fn record_unknown(ctx: &mut ScanCtx, path: &Path) -> Result<()> {
    let preview = preview_directory(path);
    let now = now_secs();
    let parent_path = path.parent().map(|p| p.to_string_lossy().to_string());

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

/// Index the direct files of a collection. Each file becomes an Entry row
/// with is_dir=false, is_item=true, base_type from its extension. Files
/// without a recognized extension are skipped (left to the filesystem
/// overlay in the browse view).
///
/// `parent_base_type` lets the extension resolver apply context-aware
/// overrides (e.g. .pdf → book inside a book library).
fn index_files_in(
    ctx: &mut ScanCtx,
    path: &Path,
    parent_id: i64,
    parent_base_type: crate::models::BaseType,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Ok(());
    };
    let now = now_secs();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Some(base_type) = ctx.engine.lookup_extension(&name, Some(parent_base_type)) else {
            continue;
        };
        let file_path = entry.path();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let file_entry = crate::models::Entry {
            id: 0,
            parent_id: Some(parent_id),
            location_id: ctx.location_id,
            path: file_path.to_string_lossy().to_string(),
            name,
            base_type,
            is_item: true,
            is_dir: false,
            tags: Vec::new(),
            privacy: PrivacyLevel::Public,
            identifier: None,
            total_size: size,
            file_count: 0,
            child_count: 0,
            manifest_hash: None,
            indexed_at: now,
        };
        ctx.db.upsert_entry(&file_entry)?;
        ctx.stats.files += 1;
    }
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
        out.push(child);
    }
    out.sort();
    out
}

// ---------- Entry building ----------

fn build_entry(
    engine: &RulesEngine,
    location_id: i64,
    parent_id: Option<i64>,
    path: &Path,
    m: &MatchResult,
) -> Entry {
    let (file_count, total_size) = dir_stats_shallow(path);

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let privacy = detect_privacy(engine, path);
    let identifier = entry_identifier(path);

    Entry {
        id: 0,
        parent_id,
        location_id,
        path: path.to_string_lossy().to_string(),
        name,
        base_type: m.base_type,
        is_item: m.item,
        is_dir: true,
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

fn entry_identifier(path: &Path) -> Option<String> {
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
