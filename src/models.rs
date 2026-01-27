use serde::{Deserialize, Serialize};

/// A device that holds files (desktop, laptop, phone, cloud, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: i64,
    pub name: String,
    pub hostname: Option<String>,
    pub device_type: DeviceType,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Local,      // This machine
    Remote,     // Another computer
    Mobile,     // Phone/tablet
    Cloud,      // Cloud storage
    Removable,  // USB drives, SD cards
}

impl DeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeviceType::Local => "local",
            DeviceType::Remote => "remote",
            DeviceType::Mobile => "mobile",
            DeviceType::Cloud => "cloud",
            DeviceType::Removable => "removable",
        }
    }
}

/// A storage location within a device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: i64,
    pub device_id: i64,
    pub name: String,
    pub path: String,
    pub is_backup: bool,
    pub is_ephemeral: bool,
    pub is_readonly: bool,
    pub last_scan: Option<i64>,
}

/// A collection of related files (project, album, game, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub location_id: i64,
    pub path: String,
    pub name: String,
    pub collection_type: CollectionType,
    pub privacy: PrivacyLevel,        // public/personal/confidential
    pub identifier: Option<String>,   // git remote, Steam ID, etc.
    pub total_size: u64,
    pub file_count: u64,
    pub child_count: u64,
    pub manifest_hash: Option<String>,
    pub indexed_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionType {
    // Code
    Git,
    Projects,       // Container for projects
    
    // Media
    Photos,         // Container for albums
    Album,          // Photo album
    Music,          // Container for music
    Artist,         // Music artist folder
    MusicAlbum,     // Music album
    Videos,         // Container for videos
    VideoSeries,    // TV show, movie series
    
    // Games
    Games,          // Container for games
    Game,           // Single game
    
    // System
    Snapshot,       // Backup/migration from another system
    App,            // Application bundle
    Package,        // node_modules, target/, etc. (ephemeral)
    
    // Generic
    Folder,         // Generic collection
    Unknown,
}

impl CollectionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CollectionType::Git => "git",
            CollectionType::Projects => "projects",
            CollectionType::Photos => "photos",
            CollectionType::Album => "album",
            CollectionType::Music => "music",
            CollectionType::Artist => "artist",
            CollectionType::MusicAlbum => "music-album",
            CollectionType::Videos => "videos",
            CollectionType::VideoSeries => "video-series",
            CollectionType::Games => "games",
            CollectionType::Game => "game",
            CollectionType::Snapshot => "snapshot",
            CollectionType::App => "app",
            CollectionType::Package => "package",
            CollectionType::Folder => "folder",
            CollectionType::Unknown => "unknown",
        }
    }
    
    pub fn from_str(s: &str) -> Self {
        match s {
            "git" => CollectionType::Git,
            "projects" => CollectionType::Projects,
            "photos" => CollectionType::Photos,
            "album" => CollectionType::Album,
            "music" => CollectionType::Music,
            "artist" => CollectionType::Artist,
            "music-album" => CollectionType::MusicAlbum,
            "videos" => CollectionType::Videos,
            "video-series" => CollectionType::VideoSeries,
            "games" => CollectionType::Games,
            "game" => CollectionType::Game,
            "snapshot" => CollectionType::Snapshot,
            "app" => CollectionType::App,
            "package" => CollectionType::Package,
            "folder" => CollectionType::Folder,
            _ => CollectionType::Unknown,
        }
    }
    
    /// Is this an ephemeral collection that can be regenerated?
    pub fn is_ephemeral(&self) -> bool {
        matches!(self, CollectionType::Package)
    }
    
    /// Is this a container for other collections?
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            CollectionType::Projects
                | CollectionType::Photos
                | CollectionType::Music
                | CollectionType::Artist
                | CollectionType::Videos
                | CollectionType::Games
        )
    }
}

/// Unique file content (by hash)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub hash: String,           // xxhash3
    pub size: u64,
    pub sha256: Option<String>, // Optional verification hash
    pub mime_type: Option<String>,
    pub first_seen: i64,
    pub last_verified: Option<i64>,
}

/// A file instance (path + content)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub id: i64,
    pub location_id: i64,
    pub collection_id: Option<i64>,
    pub path: String,
    pub hash: String,
    pub mtime: i64,
    pub indexed_at: i64,
}

/// Privacy level for files/collections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyLevel {
    #[default]
    Public,       // Open source, shareable, no concerns
    Personal,     // Photos, personal docs — keep private but not secret
    Confidential, // Passwords, tax docs, medical — encrypt, restrict access
}

impl PrivacyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrivacyLevel::Public => "public",
            PrivacyLevel::Personal => "personal",
            PrivacyLevel::Confidential => "confidential",
        }
    }
    
    pub fn from_str(s: &str) -> Self {
        match s {
            "personal" => PrivacyLevel::Personal,
            "confidential" => PrivacyLevel::Confidential,
            _ => PrivacyLevel::Public,
        }
    }
}

/// Path classification rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRule {
    pub id: i64,
    pub pattern: String,
    pub path_type: PathType,
    pub behavior: PathBehavior,
    pub is_builtin: bool,
    pub priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathType {
    System,
    User,
    Projects,
    Games,
    Media,
    Backup,
    Cloud,
    Cache,
    Config,
    Secrets,
    Trash,
    Temp,
    Build,
    Dependencies,
    Ide,
    Flatpak,
    Logs,
    Local,
    Unknown,
}

impl PathType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PathType::System => "system",
            PathType::User => "user",
            PathType::Projects => "projects",
            PathType::Games => "games",
            PathType::Media => "media",
            PathType::Backup => "backup",
            PathType::Cloud => "cloud",
            PathType::Cache => "cache",
            PathType::Config => "config",
            PathType::Secrets => "secrets",
            PathType::Trash => "trash",
            PathType::Temp => "temp",
            PathType::Build => "build",
            PathType::Dependencies => "dependencies",
            PathType::Ide => "ide",
            PathType::Flatpak => "flatpak",
            PathType::Logs => "logs",
            PathType::Local => "local",
            PathType::Unknown => "unknown",
        }
    }
    
    pub fn from_str(s: &str) -> Self {
        match s {
            "system" => PathType::System,
            "user" => PathType::User,
            "projects" => PathType::Projects,
            "games" => PathType::Games,
            "media" => PathType::Media,
            "backup" => PathType::Backup,
            "cloud" => PathType::Cloud,
            "cache" => PathType::Cache,
            "config" => PathType::Config,
            "secrets" => PathType::Secrets,
            "trash" => PathType::Trash,
            "temp" => PathType::Temp,
            "build" => PathType::Build,
            "dependencies" => PathType::Dependencies,
            "ide" => PathType::Ide,
            "flatpak" => PathType::Flatpak,
            "logs" => PathType::Logs,
            "local" => PathType::Local,
            _ => PathType::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathBehavior {
    Index,      // Normal indexing
    Skip,       // Don't index at all
    Shallow,    // Index top level only
    Collection, // Treat as atomic collection
    Prompt,     // Ask user for classification
}

impl PathBehavior {
    pub fn as_str(&self) -> &'static str {
        match self {
            PathBehavior::Index => "index",
            PathBehavior::Skip => "skip",
            PathBehavior::Shallow => "shallow",
            PathBehavior::Collection => "collection",
            PathBehavior::Prompt => "prompt",
        }
    }
    
    pub fn from_str(s: &str) -> Self {
        match s {
            "index" => PathBehavior::Index,
            "skip" => PathBehavior::Skip,
            "shallow" => PathBehavior::Shallow,
            "collection" => PathBehavior::Collection,
            "prompt" => PathBehavior::Prompt,
            _ => PathBehavior::Index,
        }
    }
}

/// Statistics for status display
#[derive(Debug, Default)]
pub struct Stats {
    pub collection_count: u64,
    pub file_count: u64,
    pub total_size: u64,
    pub by_type: Vec<(String, u64)>,
    pub unprotected_count: u64,
    pub device_count: u64,
    pub location_count: u64,
}
