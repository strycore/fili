#![allow(dead_code)] // Scaffold - not all types used yet

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
    Local,
    Remote,
    Mobile,
    Cloud,
    Removable,
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

/// Intrinsic content type — applied to every indexed entry (collection or item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseType {
    Image,
    Audio,
    Video,
    Game,
    Application,
    Document,
    Code,
    Archive,
    Cache,
    /// A user home directory (or a backup of one).
    Home,
    /// OS-managed directories: /usr, /var, /opt, /home, ...
    System,
    /// Configuration data: /etc and equivalents.
    Config,
    /// Boot / kernel data.
    Boot,
    /// Device nodes: /dev.
    Devices,
    /// Swap space: /swap.
    Swap,
    /// Service data root: /srv.
    Services,
    /// procfs — /proc.
    Procfs,
    /// sysfs — /sys.
    Sysfs,
    /// Mount points for external storage.
    Mount,
    /// Game saves and per-game configuration.
    GameData,
    /// Emulator installs.
    Emulator,
    /// Third-party packages fetched by a package manager:
    /// node_modules, .venv, vendor, .cargo registry, etc.
    Dependencies,
    /// Locally compiled / assembled output:
    /// target/debug, target/release, dist, build, __pycache__, .gradle.
    BuildArtifact,
    /// Unsorted content to triage: ~/Downloads, ~/Desktop, email attachments.
    Inbox,
    Generic,
}

impl BaseType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BaseType::Image => "image",
            BaseType::Audio => "audio",
            BaseType::Video => "video",
            BaseType::Game => "game",
            BaseType::Application => "application",
            BaseType::Document => "document",
            BaseType::Code => "code",
            BaseType::Archive => "archive",
            BaseType::Cache => "cache",
            BaseType::Home => "home",
            BaseType::System => "system",
            BaseType::Config => "config",
            BaseType::Boot => "boot",
            BaseType::Devices => "devices",
            BaseType::Swap => "swap",
            BaseType::Services => "services",
            BaseType::Procfs => "procfs",
            BaseType::Sysfs => "sysfs",
            BaseType::Mount => "mount",
            BaseType::GameData => "gamedata",
            BaseType::Emulator => "emulator",
            BaseType::Dependencies => "dependencies",
            BaseType::BuildArtifact => "build-artifact",
            BaseType::Inbox => "inbox",
            BaseType::Generic => "generic",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "image" => BaseType::Image,
            "audio" => BaseType::Audio,
            "video" => BaseType::Video,
            "game" => BaseType::Game,
            "application" => BaseType::Application,
            "document" => BaseType::Document,
            "code" => BaseType::Code,
            "archive" => BaseType::Archive,
            "cache" => BaseType::Cache,
            "home" => BaseType::Home,
            "system" => BaseType::System,
            "config" => BaseType::Config,
            "boot" => BaseType::Boot,
            "devices" => BaseType::Devices,
            "swap" => BaseType::Swap,
            "services" => BaseType::Services,
            "procfs" => BaseType::Procfs,
            "sysfs" => BaseType::Sysfs,
            "mount" => BaseType::Mount,
            "gamedata" => BaseType::GameData,
            "emulator" => BaseType::Emulator,
            "dependencies" => BaseType::Dependencies,
            "build-artifact" => BaseType::BuildArtifact,
            "inbox" => BaseType::Inbox,
            _ => BaseType::Generic,
        }
    }
}

/// A key=value tag. Value is optional (flag-style tags allowed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: Option<String>,
}

impl Tag {
    pub fn flag(key: impl Into<String>) -> Self {
        Tag { key: key.into(), value: None }
    }

    pub fn kv(key: impl Into<String>, value: impl Into<String>) -> Self {
        Tag { key: key.into(), value: Some(value.into()) }
    }

    /// Parse "key=value" or "key".
    pub fn parse(s: &str) -> Self {
        match s.split_once('=') {
            Some((k, v)) => Tag::kv(k.trim(), v.trim()),
            None => Tag::flag(s.trim()),
        }
    }

    pub fn render(&self) -> String {
        match &self.value {
            Some(v) => format!("{}={}", self.key, v),
            None => self.key.clone(),
        }
    }
}

/// An indexed entry — either a collection (holds children) or an item (atomic).
/// `is_item` stores the data-model distinction; `is_dir` stores whether the
/// underlying filesystem entry is a directory (true) or a file (false).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub location_id: i64,
    pub path: String,
    pub name: String,
    pub base_type: BaseType,
    pub is_item: bool,
    pub is_dir: bool,
    pub tags: Vec<Tag>,
    pub privacy: PrivacyLevel,
    pub identifier: Option<String>, // git remote, Steam ID, etc.
    pub total_size: u64,
    pub file_count: u64,
    pub child_count: u64,
    pub manifest_hash: Option<String>,
    pub indexed_at: i64,
}

/// Unique file content (by hash). Unused for now; kept for future file hashing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub hash: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub mime_type: Option<String>,
    pub first_seen: i64,
    pub last_verified: Option<i64>,
}

/// Privacy level for entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyLevel {
    #[default]
    Public,
    Personal,
    Confidential,
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

/// A storage drive (partition / filesystem) as a first-class entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    pub id: i64,
    pub uuid: Option<String>,
    pub label: Option<String>,
    pub fs_type: Option<String>,
    pub size: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub friendly_name: Option<String>,
    pub current_mount: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
}

/// A directory that the scanner discovered but couldn't classify.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unknown {
    pub id: i64,
    pub location_id: i64,
    pub path: String,
    pub parent_path: Option<String>,
    pub discovered_at: i64,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub top_extensions: Vec<ExtensionCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionCount {
    pub ext: String,
    pub count: u64,
}

/// Statistics for status display
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    pub entry_count: u64,
    pub collection_count: u64,
    pub item_count: u64,
    pub total_size: u64,
    pub by_type: Vec<(String, u64)>,
    pub unprotected_count: u64,
    pub device_count: u64,
    pub location_count: u64,
    #[serde(default)]
    pub unknown_count: u64,
}
