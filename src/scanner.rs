use anyhow::Result;
use console::style;
use dialoguer::{Select, Input};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::db::Database;
use crate::models::*;
use crate::rules::*;

/// Scan a path and index contents
pub fn scan(db: &Database, path: &Path, interactive: bool) -> Result<()> {
    println!("{} {}", style("Scanning").cyan().bold(), path.display());
    
    let location_id = db.get_or_create_location(path)?;
    
    // Get path rules
    let _rules = db.get_path_rules()?;
    
    // Collect top-level entries first
    let entries: Vec<_> = std::fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .collect();
    
    let pb = ProgressBar::new(entries.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
        .unwrap()
        .progress_chars("#>-"));
    
    for entry in entries {
        let entry_path = entry.path();
        let name = entry_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        
        pb.set_message(name.clone());
        
        // Skip hidden files/directories at top level (configurable later)
        if name.starts_with('.') {
            pb.inc(1);
            continue;
        }
        
        if entry_path.is_dir() {
            // Detect what kind of collection this is
            match detect_collection_type(&entry_path) {
                Some(ctype) => {
                    // It's a known collection type
                    let collection = scan_as_collection(&entry_path, location_id, ctype)?;
                    db.upsert_collection(&collection)?;
                    pb.println(format!(
                        "  {} {} ({})",
                        style("→").green(),
                        name,
                        style(ctype.as_str()).yellow()
                    ));
                }
                None if interactive => {
                    // Unknown directory - prompt user
                    pb.suspend(|| {
                        if let Some(ctype) = prompt_for_collection_type(&entry_path)? {
                            let collection = scan_as_collection(&entry_path, location_id, ctype)?;
                            db.upsert_collection(&collection)?;
                        }
                        Ok::<_, anyhow::Error>(())
                    })?;
                }
                None => {
                    // Non-interactive: treat as generic folder
                    let collection = scan_as_collection(&entry_path, location_id, CollectionType::Folder)?;
                    db.upsert_collection(&collection)?;
                }
            }
        }
        
        pb.inc(1);
    }
    
    pb.finish_with_message("done");
    
    Ok(())
}

/// Detect what type of collection a directory is
fn detect_collection_type(path: &Path) -> Option<CollectionType> {
    // Check for git repo
    if path.join(".git").exists() {
        return Some(CollectionType::Git);
    }
    
    // Check for game indicators
    for pattern in GAME_INDICATORS {
        if pattern.starts_with('*') {
            // Glob pattern - check if any matching files exist
            let ext = pattern.trim_start_matches('*');
            if has_files_with_extension(path, ext) {
                return Some(CollectionType::Game);
            }
        } else if path.join(pattern).exists() {
            return Some(CollectionType::Game);
        }
    }
    
    // Check directory name patterns
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    
    // Photo album detection
    if is_photo_album(path) {
        return Some(CollectionType::Album);
    }
    
    // Music album detection (check parent for artist pattern)
    if is_music_album(path) {
        return Some(CollectionType::MusicAlbum);
    }
    
    // Container detection by name
    if name == "projects" || name == "src" || name == "code" || name == "dev" {
        return Some(CollectionType::Projects);
    }
    
    if name == "pictures" || name == "photos" {
        return Some(CollectionType::Photos);
    }
    
    if name == "music" {
        return Some(CollectionType::Music);
    }
    
    if name == "videos" || name == "movies" {
        return Some(CollectionType::Videos);
    }
    
    if name == "games" {
        return Some(CollectionType::Games);
    }
    
    // Check for system snapshot (backup from another system)
    if looks_like_system_snapshot(path) {
        return Some(CollectionType::Snapshot);
    }
    
    None
}

fn has_files_with_extension(path: &Path, ext: &str) -> bool {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Some(name) = entry.path().file_name() {
                if name.to_string_lossy().ends_with(ext) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_photo_album(path: &Path) -> bool {
    let image_extensions = [".jpg", ".jpeg", ".png", ".heic", ".raw", ".cr2", ".nef"];
    let mut image_count = 0;
    let mut total_count = 0;
    
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()).take(50) {
            total_count += 1;
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if image_extensions.iter().any(|ext| name.ends_with(ext)) {
                image_count += 1;
            }
        }
    }
    
    // If >50% of files (sample) are images, it's probably a photo album
    total_count > 0 && image_count * 2 > total_count
}

fn is_music_album(path: &Path) -> bool {
    let audio_extensions = [".mp3", ".flac", ".m4a", ".ogg", ".opus", ".wav"];
    let mut audio_count = 0;
    let mut total_count = 0;
    
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()).take(30) {
            total_count += 1;
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if audio_extensions.iter().any(|ext| name.ends_with(ext)) {
                audio_count += 1;
            }
        }
    }
    
    // If >50% of files are audio, it's probably a music album
    total_count > 0 && audio_count * 2 > total_count
}

fn looks_like_system_snapshot(path: &Path) -> bool {
    // Check if this looks like a backup/migration from another system
    for indicator in SYSTEM_SNAPSHOT_PATHS {
        if path.join(indicator).exists() {
            return true;
        }
    }
    
    // Check for nested home directory
    if path.join("home").is_dir() {
        if let Ok(entries) = std::fs::read_dir(path.join("home")) {
            let has_user_dirs = entries
                .filter_map(|e| e.ok())
                .any(|e| e.path().is_dir());
            if has_user_dirs {
                return true;
            }
        }
    }
    
    false
}

/// Prompt user to classify an unknown directory
fn prompt_for_collection_type(path: &Path) -> Result<Option<CollectionType>> {
    println!("\n{} Unknown directory: {}", 
        style("?").yellow().bold(),
        style(path.display()).cyan()
    );
    
    // Show some info about the directory
    let (file_count, total_size) = get_dir_stats(path);
    println!("  {} files, {}", file_count, format_size(total_size));
    
    let options = vec![
        "Git/Software project",
        "Game",
        "Photo album",
        "Music album/Artist",
        "Video collection",
        "System backup/snapshot",
        "Generic folder",
        "Skip (don't index)",
    ];
    
    let selection = Select::new()
        .with_prompt("What is this?")
        .items(&options)
        .default(6)
        .interact()?;
    
    let ctype = match selection {
        0 => Some(CollectionType::Git),
        1 => Some(CollectionType::Game),
        2 => Some(CollectionType::Album),
        3 => Some(CollectionType::MusicAlbum),
        4 => Some(CollectionType::Videos),
        5 => {
            // Ask for source system name
            let name: String = Input::new()
                .with_prompt("Name of source system")
                .interact_text()?;
            println!("  Tagged as snapshot from '{}'", name);
            Some(CollectionType::Snapshot)
        }
        6 => Some(CollectionType::Folder),
        7 => None, // Skip
        _ => Some(CollectionType::Unknown),
    };
    
    Ok(ctype)
}

/// Scan a directory as a collection (don't recurse into files)
fn scan_as_collection(path: &Path, location_id: i64, ctype: CollectionType) -> Result<Collection> {
    let (file_count, total_size) = get_dir_stats_recursive(path);
    let child_count = count_child_collections(path, &ctype);
    
    let name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    
    let identifier = get_collection_identifier(path, &ctype);
    let manifest_hash = compute_manifest_hash(path)?;
    
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs() as i64;
    
    Ok(Collection {
        id: 0, // Will be set by database
        parent_id: None, // TODO: handle nested collections
        location_id,
        path: path.to_string_lossy().to_string(),
        name,
        collection_type: ctype,
        identifier,
        total_size,
        file_count,
        child_count,
        manifest_hash: Some(manifest_hash),
        indexed_at: now,
    })
}

fn get_dir_stats(path: &Path) -> (u64, u64) {
    let mut file_count = 0u64;
    let mut total_size = 0u64;
    
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    file_count += 1;
                    total_size += meta.len();
                }
            }
        }
    }
    
    (file_count, total_size)
}

fn get_dir_stats_recursive(path: &Path) -> (u64, u64) {
    let mut file_count = 0u64;
    let mut total_size = 0u64;
    
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                file_count += 1;
                total_size += meta.len();
            }
        }
    }
    
    (file_count, total_size)
}

fn count_child_collections(path: &Path, parent_type: &CollectionType) -> u64 {
    // For container types, count subdirectories as potential child collections
    if !parent_type.is_container() {
        return 0;
    }
    
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .count() as u64
        })
        .unwrap_or(0)
}

fn get_collection_identifier(path: &Path, ctype: &CollectionType) -> Option<String> {
    match ctype {
        CollectionType::Git => {
            // Try to get git remote URL
            let git_config = path.join(".git/config");
            if let Ok(content) = std::fs::read_to_string(git_config) {
                for line in content.lines() {
                    if line.trim().starts_with("url = ") {
                        return Some(line.trim().trim_start_matches("url = ").to_string());
                    }
                }
            }
            None
        }
        CollectionType::Game => {
            // Try to get Steam app ID
            let steam_appid = path.join("steam_appid.txt");
            if let Ok(content) = std::fs::read_to_string(steam_appid) {
                return Some(format!("steam:{}", content.trim()));
            }
            None
        }
        _ => None,
    }
}

fn compute_manifest_hash(path: &Path) -> Result<String> {
    use xxhash_rust::xxh3::xxh3_64;
    
    // Create a sorted list of relative paths
    let mut entries: Vec<String> = Vec::new();
    
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .take(10000) // Limit for very large directories
    {
        if let Ok(rel) = entry.path().strip_prefix(path) {
            entries.push(rel.to_string_lossy().to_string());
        }
    }
    
    entries.sort();
    let manifest = entries.join("\n");
    let hash = xxh3_64(manifest.as_bytes());
    
    Ok(format!("{:016x}", hash))
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
