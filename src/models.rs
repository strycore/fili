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

/// Intrinsic content type — applied to both files and collections.
/// A collection's base type describes the kind of content it holds.
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
    /// A user home directory (or a backup of one). Holds mixed content and
    /// is itself a first-class target: fili should help consolidate stray
    /// copies by relocating their content to canonical locations.
    Home,
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
            _ => BaseType::Generic,
        }
    }
}

/// A key=value tag. Value is optional (flag-style tags allowed, e.g. "library").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: Option<String>,
}

impl Tag {
    pub fn flag(key: impl Into<String>) -> Self {
        Tag {
            key: key.into(),
            value: None,
        }
    }

    pub fn kv(key: impl Into<String>, value: impl Into<String>) -> Self {
        Tag {
            key: key.into(),
            value: Some(value.into()),
        }
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

/// A collection of related files (album, game, project, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub location_id: i64,
    pub path: String,
    pub name: String,
    pub base_type: BaseType,
    pub tags: Vec<Tag>,
    pub privacy: PrivacyLevel,
    pub identifier: Option<String>, // git remote, Steam ID, etc.
    pub total_size: u64,
    pub file_count: u64,
    pub child_count: u64,
    pub manifest_hash: Option<String>,
    pub indexed_at: i64,
}

/// Unique file content (by hash)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub hash: String,
    pub size: u64,
    pub sha256: Option<String>,
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
    pub base_type: BaseType,
    pub hash: String,
    pub mtime: i64,
    pub indexed_at: i64,
}

/// Privacy level for files/collections
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

/// A directory that the scanner discovered but couldn't classify.
/// Holds enough preview data for the UI to suggest a classification.
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
    pub collection_count: u64,
    pub file_count: u64,
    pub total_size: u64,
    pub by_type: Vec<(String, u64)>,
    pub unprotected_count: u64,
    pub device_count: u64,
    pub location_count: u64,
    #[serde(default)]
    pub unknown_count: u64,
}
