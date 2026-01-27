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

/// Patterns that indicate a folder is a git repository
pub const GIT_INDICATORS: &[&str] = &[".git"];

/// Patterns that indicate a folder is a game
pub const GAME_INDICATORS: &[&str] = &[
    // Windows games
    "*.exe",
    // Steam
    "steam_appid.txt",
    "steamapps",
    // GOG
    "goggame-*.info",
    // Unity
    "UnityPlayer.dll",
    "*_Data",
    // Unreal
    "Engine",
    // Common patterns
    "dosbox.conf",
    "*.gog",
];

/// Patterns that indicate a folder is a photo album
pub const PHOTO_ALBUM_INDICATORS: &[&str] = &[
    "*.jpg",
    "*.jpeg", 
    "*.png",
    "*.heic",
    "*.raw",
    "*.cr2",
    "*.nef",
];

/// Patterns that indicate a folder is a music album  
pub const MUSIC_ALBUM_INDICATORS: &[&str] = &[
    "*.mp3",
    "*.flac",
    "*.m4a",
    "*.ogg",
    "*.opus",
    "cover.jpg",
    "folder.jpg",
];

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
