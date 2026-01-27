#![allow(dead_code)] // Scaffold - not all functions used yet

/// Path classification rules - loaded from rules.json
use serde::Deserialize;
use std::path::Path;

use crate::models::{PathBehavior, PathRule, PathType};

/// Rules file structure
#[derive(Debug, Deserialize)]
pub struct RulesFile {
    pub version: u32,
    pub rules: Vec<RuleEntry>,
    pub contexts: Vec<ContextEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleEntry {
    pub pattern: String,
    #[serde(rename = "type")]
    pub path_type: String,
    pub behavior: String,
    pub privacy: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextEntry {
    pub pattern: String,
    pub structure: String,
    pub levels: Vec<String>,
}

/// Load rules from JSON file or embedded default
pub fn load_rules() -> RulesFile {
    // Try user config first
    let user_rules = directories::BaseDirs::new().map(|d| d.config_dir().join("fili/rules.json"));

    if let Some(path) = user_rules {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(rules) = serde_json::from_str(&content) {
                    return rules;
                }
            }
        }
    }

    // Fall back to embedded default
    let default_json = include_str!("../rules.json");
    serde_json::from_str(default_json).expect("Invalid embedded rules.json")
}

/// Convert loaded rules to PathRule structs for database
pub fn get_builtin_rules() -> Vec<PathRule> {
    let rules_file = load_rules();

    rules_file
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            PathRule {
                id: 0,
                pattern: r.pattern.clone(),
                path_type: PathType::from_str(&r.path_type),
                behavior: PathBehavior::from_str(&r.behavior),
                is_builtin: true,
                priority: (1000 - i as i32), // Higher priority for earlier rules
            }
        })
        .collect()
}

/// Expected hierarchy for different content types
#[derive(Debug, Clone, Copy)]
pub enum CollectionStructure {
    /// ~/Music → Artist → Album → Tracks
    MusicLibrary,
    /// ~/Pictures → Year/Event → Photos
    PhotoLibrary,
    /// ~/Videos → Series/Movie → Episodes/Files
    VideoLibrary,
    /// ~/Projects → Project (git repo)
    ProjectsFolder,
    /// ~/Games → Game folders
    GamesLibrary,
    /// ~/Documents → Folders of documents
    DocumentsFolder,
    /// Generic folder, detect by content
    Unknown,
}

/// Get expected structure based on path context
pub fn get_collection_context(path: &Path) -> CollectionStructure {
    let rules = load_rules();
    let path_str = path.to_string_lossy();
    let home = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_string_lossy().to_string())
        .unwrap_or_default();

    // Check against context patterns
    for ctx in &rules.contexts {
        let pattern = ctx.pattern.replace("~", &home);
        if path_str.starts_with(&pattern) || path_str.contains(&format!("{}/", pattern)) {
            return match ctx.structure.as_str() {
                "music" => CollectionStructure::MusicLibrary,
                "photos" => CollectionStructure::PhotoLibrary,
                "videos" => CollectionStructure::VideoLibrary,
                "projects" => CollectionStructure::ProjectsFolder,
                "games" => CollectionStructure::GamesLibrary,
                "documents" => CollectionStructure::DocumentsFolder,
                _ => CollectionStructure::Unknown,
            };
        }
    }

    CollectionStructure::Unknown
}

/// Depth expectations for collection structures
impl CollectionStructure {
    /// What does each level represent?
    pub fn level_names(&self) -> &[&'static str] {
        match self {
            CollectionStructure::MusicLibrary => &["library", "artist", "album"],
            CollectionStructure::PhotoLibrary => &["library", "album"],
            CollectionStructure::VideoLibrary => &["library", "series", "season"],
            CollectionStructure::ProjectsFolder => &["folder", "project"],
            CollectionStructure::GamesLibrary => &["library", "game"],
            CollectionStructure::DocumentsFolder => &["folder", "category"],
            CollectionStructure::Unknown => &["folder"],
        }
    }

    /// At what depth do we stop descending into individual files?
    pub fn collection_depth(&self) -> usize {
        match self {
            CollectionStructure::MusicLibrary => 2, // Artist/Album, then stop
            CollectionStructure::PhotoLibrary => 1, // Album, then stop
            CollectionStructure::VideoLibrary => 2, // Series/Season, then stop
            CollectionStructure::ProjectsFolder => 1, // Project (git root), then stop
            CollectionStructure::GamesLibrary => 1, // Game folder, then stop
            CollectionStructure::DocumentsFolder => 1, // Category, then index files
            CollectionStructure::Unknown => 0,      // Detect based on content
        }
    }
}

/// Check if a path should be skipped based on rules
pub fn should_skip_path(path: &Path) -> bool {
    let rules = load_rules();
    let path_str = path.to_string_lossy();
    let home = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_string_lossy().to_string())
        .unwrap_or_default();

    for rule in &rules.rules {
        if rule.behavior != "skip" {
            continue;
        }

        let pattern = rule.pattern.replace("~", &home);

        // Glob patterns with **
        if let Some(suffix) = pattern.strip_prefix("**/") {
            if path_str.ends_with(suffix) || path_str.contains(&format!("/{}/", suffix)) {
                return true;
            }
        }
        // Exact prefix match
        else if path_str.starts_with(&pattern) {
            return true;
        }
    }

    false
}

/// System snapshot paths (for detecting backups of other systems)
pub const SYSTEM_SNAPSHOT_PATHS: &[&str] = &[
    "bin", "boot", "dev", "etc", "home", "lib", "lib64", "mnt", "opt", "proc", "root", "run",
    "sbin", "srv", "sys", "tmp", "usr", "var",
];
