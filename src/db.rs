use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

use crate::models::*;
use crate::rules::get_builtin_rules;

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    /// Initialize a new database
    pub fn init() -> Result<Self> {
        let path = Self::default_path()?;
        
        if path.exists() {
            anyhow::bail!("Database already exists at {}. Use 'fili scan' to add files.", path.display());
        }
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let conn = Connection::open(&path)?;
        let db = Self { conn, path };
        db.create_schema()?;
        db.load_builtin_rules()?;
        db.create_local_device()?;
        
        Ok(db)
    }
    
    /// Open existing database
    pub fn open() -> Result<Self> {
        let path = Self::default_path()?;
        
        if !path.exists() {
            anyhow::bail!("Database not found. Run 'fili init' first.");
        }
        
        let conn = Connection::open(&path)?;
        Ok(Self { conn, path })
    }
    
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    fn default_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "fili", "fili")
            .context("Could not determine config directory")?;
        Ok(dirs.data_dir().join("fili.db"))
    }
    
    fn create_schema(&self) -> Result<()> {
        self.conn.execute_batch(r#"
            -- Devices (machines, phones, cloud accounts)
            CREATE TABLE devices (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                hostname TEXT,
                device_type TEXT NOT NULL,
                last_seen INTEGER
            );

            -- Storage locations within devices
            CREATE TABLE locations (
                id INTEGER PRIMARY KEY,
                device_id INTEGER REFERENCES devices(id),
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                is_backup INTEGER DEFAULT 0,
                is_ephemeral INTEGER DEFAULT 0,
                is_readonly INTEGER DEFAULT 0,
                last_scan INTEGER,
                UNIQUE(device_id, name)
            );

            -- Collections (hierarchical groupings)
            CREATE TABLE collections (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER REFERENCES collections(id),
                location_id INTEGER REFERENCES locations(id),
                path TEXT NOT NULL,
                name TEXT,
                collection_type TEXT,
                privacy TEXT DEFAULT 'public',  -- public/personal/confidential
                identifier TEXT,
                total_size INTEGER DEFAULT 0,
                file_count INTEGER DEFAULT 0,
                child_count INTEGER DEFAULT 0,
                manifest_hash TEXT,
                indexed_at INTEGER,
                UNIQUE(location_id, path)
            );

            -- Unique file contents
            CREATE TABLE contents (
                hash TEXT PRIMARY KEY,
                size INTEGER NOT NULL,
                sha256 TEXT,
                mime_type TEXT,
                first_seen INTEGER,
                last_verified INTEGER
            );

            -- File instances (path + content)
            CREATE TABLE files (
                id INTEGER PRIMARY KEY,
                location_id INTEGER REFERENCES locations(id),
                collection_id INTEGER REFERENCES collections(id),
                path TEXT NOT NULL,
                hash TEXT REFERENCES contents(hash),
                mtime INTEGER,
                indexed_at INTEGER,
                UNIQUE(location_id, path)
            );

            -- Path classification rules
            CREATE TABLE path_rules (
                id INTEGER PRIMARY KEY,
                pattern TEXT NOT NULL,
                path_type TEXT,
                behavior TEXT,
                is_builtin INTEGER DEFAULT 0,
                priority INTEGER DEFAULT 0
            );

            -- Events for tracking changes
            CREATE TABLE events (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER,
                event_type TEXT,
                location_id INTEGER,
                path TEXT,
                hash TEXT
            );

            -- Indexes
            CREATE INDEX idx_files_hash ON files(hash);
            CREATE INDEX idx_files_path ON files(path);
            CREATE INDEX idx_files_collection ON files(collection_id);
            CREATE INDEX idx_collections_path ON collections(path);
            CREATE INDEX idx_collections_type ON collections(collection_type);
            CREATE INDEX idx_collections_parent ON collections(parent_id);
            CREATE INDEX idx_contents_size ON contents(size);
            CREATE INDEX idx_path_rules_pattern ON path_rules(pattern);
        "#)?;
        
        Ok(())
    }
    
    fn load_builtin_rules(&self) -> Result<()> {
        let rules = get_builtin_rules();
        
        for rule in rules {
            self.conn.execute(
                "INSERT INTO path_rules (pattern, path_type, behavior, is_builtin, priority) VALUES (?1, ?2, ?3, 1, ?4)",
                params![rule.pattern, rule.path_type, rule.behavior, rule.priority],
            )?;
        }
        
        Ok(())
    }
    
    fn create_local_device(&self) -> Result<()> {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        
        self.conn.execute(
            "INSERT INTO devices (name, hostname, device_type, last_seen) VALUES ('local', ?1, 'local', strftime('%s', 'now'))",
            params![hostname],
        )?;
        
        Ok(())
    }
    
    /// Get or create location for a path
    pub fn get_or_create_location(&self, path: &Path) -> Result<i64> {
        let path_str = path.to_string_lossy();
        
        // Check if location exists
        let existing: Option<i64> = self.conn.query_row(
            "SELECT id FROM locations WHERE path = ?1",
            params![path_str.as_ref()],
            |row| row.get(0),
        ).ok();
        
        if let Some(id) = existing {
            return Ok(id);
        }
        
        // Create new location
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "root".to_string());
        
        self.conn.execute(
            "INSERT INTO locations (device_id, name, path) VALUES (1, ?1, ?2)",
            params![name, path_str.as_ref()],
        )?;
        
        Ok(self.conn.last_insert_rowid())
    }
    
    /// Insert or update a collection
    pub fn upsert_collection(&self, collection: &Collection) -> Result<i64> {
        self.conn.execute(
            r#"INSERT INTO collections 
               (parent_id, location_id, path, name, collection_type, identifier, 
                total_size, file_count, child_count, manifest_hash, indexed_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
               ON CONFLICT(location_id, path) DO UPDATE SET
                 name = excluded.name,
                 collection_type = excluded.collection_type,
                 total_size = excluded.total_size,
                 file_count = excluded.file_count,
                 child_count = excluded.child_count,
                 manifest_hash = excluded.manifest_hash,
                 indexed_at = excluded.indexed_at"#,
            params![
                collection.parent_id,
                collection.location_id,
                collection.path,
                collection.name,
                collection.collection_type.as_str(),
                collection.identifier,
                collection.total_size,
                collection.file_count,
                collection.child_count,
                collection.manifest_hash,
                collection.indexed_at,
            ],
        )?;
        
        // Get the ID (either new or existing)
        let id: i64 = self.conn.query_row(
            "SELECT id FROM collections WHERE location_id = ?1 AND path = ?2",
            params![collection.location_id, collection.path],
            |row| row.get(0),
        )?;
        
        Ok(id)
    }
    
    /// Get path rules ordered by priority
    pub fn get_path_rules(&self) -> Result<Vec<PathRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, pattern, path_type, behavior, is_builtin, priority FROM path_rules ORDER BY priority DESC"
        )?;
        
        let rules = stmt.query_map([], |row| {
            Ok(PathRule {
                id: row.get(0)?,
                pattern: row.get(1)?,
                path_type: PathType::Unknown, // TODO: parse from string
                behavior: PathBehavior::Index, // TODO: parse from string
                is_builtin: row.get::<_, i32>(4)? != 0,
                priority: row.get(5)?,
            })
        })?;
        
        rules.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    
    /// Classify a path with a given type
    pub fn classify_path(&self, path: &Path, path_type: &str) -> Result<()> {
        let pattern = path.to_string_lossy();
        
        self.conn.execute(
            "INSERT INTO path_rules (pattern, path_type, behavior, is_builtin, priority) VALUES (?1, ?2, 'index', 0, 100)",
            params![pattern.as_ref(), path_type],
        )?;
        
        Ok(())
    }
    
    /// Get statistics
    pub fn get_stats(&self) -> Result<Stats> {
        let mut stats = Stats::default();
        
        stats.collection_count = self.conn.query_row(
            "SELECT COUNT(*) FROM collections",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        
        stats.total_size = self.conn.query_row(
            "SELECT COALESCE(SUM(total_size), 0) FROM collections WHERE parent_id IS NULL",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        
        stats.device_count = self.conn.query_row(
            "SELECT COUNT(*) FROM devices",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        
        stats.location_count = self.conn.query_row(
            "SELECT COUNT(*) FROM locations",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        
        // Collections by type
        let mut stmt = self.conn.prepare(
            "SELECT collection_type, COUNT(*) FROM collections GROUP BY collection_type"
        )?;
        
        let type_counts = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;
        
        stats.by_type = type_counts.filter_map(|r| r.ok()).collect();
        
        Ok(stats)
    }
    
    /// Find a collection by its path
    pub fn find_collection_by_path(&self, path: &Path) -> Result<Option<Collection>> {
        let path_str = path.to_string_lossy();
        
        let result = self.conn.query_row(
            r#"SELECT id, parent_id, location_id, path, name, collection_type, privacy,
               identifier, total_size, file_count, child_count, manifest_hash, indexed_at
               FROM collections WHERE path = ?1"#,
            params![path_str.as_ref()],
            |row| {
                Ok(Collection {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    location_id: row.get(2)?,
                    path: row.get(3)?,
                    name: row.get(4)?,
                    collection_type: CollectionType::from_str(&row.get::<_, String>(5)?),
                    privacy: PrivacyLevel::from_str(&row.get::<_, String>(6)?),
                    identifier: row.get(7)?,
                    total_size: row.get(8)?,
                    file_count: row.get(9)?,
                    child_count: row.get(10)?,
                    manifest_hash: row.get(11)?,
                    indexed_at: row.get(12)?,
                })
            },
        );
        
        match result {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Set privacy level for a collection
    pub fn set_privacy(&self, collection_id: i64, privacy: &PrivacyLevel) -> Result<()> {
        self.conn.execute(
            "UPDATE collections SET privacy = ?1 WHERE id = ?2",
            params![privacy.as_str(), collection_id],
        )?;
        Ok(())
    }
}
