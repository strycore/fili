/// Built-in path classification rules

pub struct BuiltinRule {
    pub pattern: &'static str,
    pub path_type: &'static str,
    pub behavior: &'static str,
    pub priority: i32,
}

/// Get all built-in path rules
pub fn get_builtin_rules() -> Vec<BuiltinRule> {
    vec![
        // ========== SKIP ENTIRELY ==========
        // Virtual filesystems
        BuiltinRule { pattern: "/proc", path_type: "system", behavior: "skip", priority: 1000 },
        BuiltinRule { pattern: "/sys", path_type: "system", behavior: "skip", priority: 1000 },
        BuiltinRule { pattern: "/dev", path_type: "system", behavior: "skip", priority: 1000 },
        BuiltinRule { pattern: "/run", path_type: "system", behavior: "skip", priority: 1000 },
        
        // System directories (generally skip)
        BuiltinRule { pattern: "/boot", path_type: "system", behavior: "skip", priority: 900 },
        BuiltinRule { pattern: "/bin", path_type: "system", behavior: "skip", priority: 900 },
        BuiltinRule { pattern: "/sbin", path_type: "system", behavior: "skip", priority: 900 },
        BuiltinRule { pattern: "/lib", path_type: "system", behavior: "skip", priority: 900 },
        BuiltinRule { pattern: "/lib64", path_type: "system", behavior: "skip", priority: 900 },
        BuiltinRule { pattern: "/usr", path_type: "system", behavior: "skip", priority: 900 },
        BuiltinRule { pattern: "/var", path_type: "system", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "/tmp", path_type: "cache", behavior: "skip", priority: 900 },
        BuiltinRule { pattern: "/lost+found", path_type: "system", behavior: "skip", priority: 900 },
        
        // ========== CACHE (skip) ==========
        BuiltinRule { pattern: "~/.cache", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.local/share/Trash", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.thumbnails", path_type: "cache", behavior: "skip", priority: 800 },
        
        // Package manager caches
        BuiltinRule { pattern: "~/.cargo/registry", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.cargo/git", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.rustup", path_type: "cache", behavior: "skip", priority: 700 },
        BuiltinRule { pattern: "~/.npm", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.yarn", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.pnpm-store", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.gradle", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.m2", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.pip", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.local/pipx", path_type: "cache", behavior: "skip", priority: 700 },
        BuiltinRule { pattern: "~/go/pkg", path_type: "cache", behavior: "skip", priority: 800 },
        BuiltinRule { pattern: "~/.nuget", path_type: "cache", behavior: "skip", priority: 800 },
        
        // Build artifacts (skip, inside projects they'll be collection:package)
        BuiltinRule { pattern: "**/node_modules", path_type: "cache", behavior: "skip", priority: 850 },
        BuiltinRule { pattern: "**/target/debug", path_type: "cache", behavior: "skip", priority: 850 },
        BuiltinRule { pattern: "**/target/release", path_type: "cache", behavior: "skip", priority: 850 },
        BuiltinRule { pattern: "**/__pycache__", path_type: "cache", behavior: "skip", priority: 850 },
        BuiltinRule { pattern: "**/.venv", path_type: "cache", behavior: "skip", priority: 850 },
        BuiltinRule { pattern: "**/venv", path_type: "cache", behavior: "skip", priority: 850 },
        
        // ========== USER DIRECTORIES ==========
        BuiltinRule { pattern: "~/Documents", path_type: "user", behavior: "index", priority: 500 },
        BuiltinRule { pattern: "~/Desktop", path_type: "user", behavior: "index", priority: 500 },
        BuiltinRule { pattern: "~/Downloads", path_type: "user", behavior: "index", priority: 400 }, // ephemeral-ish
        
        // Media
        BuiltinRule { pattern: "~/Pictures", path_type: "media", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/Photos", path_type: "media", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/Music", path_type: "media", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/Videos", path_type: "media", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/Movies", path_type: "media", behavior: "collection", priority: 500 },
        
        // Projects
        BuiltinRule { pattern: "~/Projects", path_type: "projects", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/projects", path_type: "projects", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/src", path_type: "projects", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/code", path_type: "projects", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/dev", path_type: "projects", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/Development", path_type: "projects", behavior: "collection", priority: 500 },
        
        // Games
        BuiltinRule { pattern: "~/Games", path_type: "games", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/.steam", path_type: "games", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/.local/share/Steam", path_type: "games", behavior: "collection", priority: 500 },
        BuiltinRule { pattern: "~/.wine", path_type: "games", behavior: "collection", priority: 400 },
        BuiltinRule { pattern: "~/.local/share/lutris", path_type: "games", behavior: "collection", priority: 500 },
        
        // Config (index but low priority for backup concerns)
        BuiltinRule { pattern: "~/.config", path_type: "user", behavior: "index", priority: 300 },
        BuiltinRule { pattern: "~/.local/share", path_type: "user", behavior: "index", priority: 300 },
        
        // Cloud sync folders
        BuiltinRule { pattern: "~/Nextcloud", path_type: "cloud", behavior: "index", priority: 500 },
        BuiltinRule { pattern: "~/Dropbox", path_type: "cloud", behavior: "index", priority: 500 },
        BuiltinRule { pattern: "~/Google Drive", path_type: "cloud", behavior: "index", priority: 500 },
        BuiltinRule { pattern: "~/OneDrive", path_type: "cloud", behavior: "index", priority: 500 },
        
        // ========== MOUNTS (prompt) ==========
        BuiltinRule { pattern: "/mnt/*", path_type: "unknown", behavior: "prompt", priority: 100 },
        BuiltinRule { pattern: "/media/*", path_type: "unknown", behavior: "prompt", priority: 100 },
        BuiltinRule { pattern: "/run/media/*", path_type: "unknown", behavior: "prompt", priority: 100 },
        
        // ========== ETC (read-only, optional) ==========
        BuiltinRule { pattern: "/etc", path_type: "system", behavior: "skip", priority: 600 },
        BuiltinRule { pattern: "/opt", path_type: "system", behavior: "skip", priority: 600 },
    ]
}

/// Context-aware collection structures
/// Instead of detecting by file extension, use location to infer structure
pub struct CollectionContext {
    pub path_pattern: &'static str,
    pub expected_structure: CollectionStructure,
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
pub fn get_collection_context(path: &std::path::Path) -> CollectionStructure {
    let path_str = path.to_string_lossy().to_lowercase();
    
    // Check ancestors for known library roots
    if path_str.contains("/music/") || path_str.ends_with("/music") {
        CollectionStructure::MusicLibrary
    } else if path_str.contains("/pictures/") || path_str.ends_with("/pictures")
           || path_str.contains("/photos/") || path_str.ends_with("/photos") {
        CollectionStructure::PhotoLibrary
    } else if path_str.contains("/videos/") || path_str.ends_with("/videos")
           || path_str.contains("/movies/") || path_str.ends_with("/movies") {
        CollectionStructure::VideoLibrary
    } else if path_str.contains("/projects/") || path_str.ends_with("/projects")
           || path_str.contains("/src/") || path_str.ends_with("/src")
           || path_str.contains("/code/") || path_str.ends_with("/code") {
        CollectionStructure::ProjectsFolder
    } else if path_str.contains("/games/") || path_str.ends_with("/games") {
        CollectionStructure::GamesLibrary
    } else if path_str.contains("/documents/") || path_str.ends_with("/documents") {
        CollectionStructure::DocumentsFolder
    } else {
        CollectionStructure::Unknown
    }
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
            CollectionStructure::MusicLibrary => 2,    // Artist/Album, then stop
            CollectionStructure::PhotoLibrary => 1,    // Album, then stop
            CollectionStructure::VideoLibrary => 2,    // Series/Season, then stop
            CollectionStructure::ProjectsFolder => 1,  // Project (git root), then stop
            CollectionStructure::GamesLibrary => 1,    // Game folder, then stop
            CollectionStructure::DocumentsFolder => 1, // Category, then index files
            CollectionStructure::Unknown => 0,         // Detect based on content
        }
    }
}

/// Common system paths that might appear in backups/snapshots
pub const SYSTEM_SNAPSHOT_PATHS: &[&str] = &[
    "etc/passwd",
    "etc/hosts",
    "etc/fstab",
    "etc/nginx",
    "etc/apache2",
    "etc/postfix",
    "etc/mysql",
    "etc/postgresql",
    "var/log",
    "var/www",
    "var/lib",
    "usr/local",
    "home/",
];
