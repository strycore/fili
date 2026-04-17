use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};
use std::path::{Path, PathBuf};

use crate::models::*;

/// Filters for listing collections from the API.
#[derive(Default, Debug)]
pub struct CollectionFilter {
    pub base_type: Option<String>,
    pub privacy: Option<String>,
    pub parent_id: Option<Option<i64>>, // Some(None) = top-level only
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn collection_row(row: &Row<'_>) -> rusqlite::Result<Collection> {
    Ok(Collection {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        location_id: row.get(2)?,
        path: row.get(3)?,
        name: row.get(4)?,
        base_type: BaseType::from_str(&row.get::<_, String>(5)?),
        tags: Vec::new(),
        privacy: PrivacyLevel::from_str(&row.get::<_, String>(6)?),
        identifier: row.get(7)?,
        total_size: row.get(8)?,
        file_count: row.get(9)?,
        child_count: row.get(10)?,
        manifest_hash: row.get(11)?,
        indexed_at: row.get(12)?,
    })
}

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    /// Initialize a new database
    pub fn init() -> Result<Self> {
        let path = Self::default_path()?;

        if path.exists() {
            anyhow::bail!(
                "Database already exists at {}. Use 'fili scan' to add files.",
                path.display()
            );
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;
        Self::configure_connection(&conn)?;
        let db = Self { conn, path };
        db.create_schema()?;
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
        Self::configure_connection(&conn)?;
        Ok(Self { conn, path })
    }

    /// WAL mode lets readers and writers coexist, so `fili serve` doesn't race
    /// with concurrent `fili scan`. busy_timeout makes contended connections
    /// wait instead of returning SQLITE_BUSY.
    fn configure_connection(conn: &Connection) -> Result<()> {
        // `PRAGMA journal_mode = WAL` returns a row with the new mode, so
        // rusqlite's pragma_update (which expects no rows) doesn't work here.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run `f` inside a single SQLite transaction. The scanner wraps the whole
    /// walk in one — autocommit-per-row made a full scan ~100x slower.
    pub fn with_transaction<T>(&mut self, f: impl FnOnce(&Database) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        match f(self) {
            Ok(v) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    fn default_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "fili", "fili")
            .context("Could not determine config directory")?;
        Ok(dirs.data_dir().join("fili.db"))
    }

    fn create_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE devices (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                hostname TEXT,
                device_type TEXT NOT NULL,
                last_seen INTEGER
            );

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

            CREATE TABLE collections (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER REFERENCES collections(id),
                location_id INTEGER REFERENCES locations(id),
                path TEXT NOT NULL,
                name TEXT,
                base_type TEXT NOT NULL,
                privacy TEXT DEFAULT 'public',
                identifier TEXT,
                total_size INTEGER DEFAULT 0,
                file_count INTEGER DEFAULT 0,
                child_count INTEGER DEFAULT 0,
                manifest_hash TEXT,
                indexed_at INTEGER,
                UNIQUE(location_id, path)
            );

            -- Multi-valued tags on collections. Value may be NULL for flag tags.
            CREATE TABLE collection_tags (
                collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                key TEXT NOT NULL,
                value TEXT,
                PRIMARY KEY (collection_id, key, value)
            );

            CREATE TABLE contents (
                hash TEXT PRIMARY KEY,
                size INTEGER NOT NULL,
                sha256 TEXT,
                mime_type TEXT,
                first_seen INTEGER,
                last_verified INTEGER
            );

            CREATE TABLE files (
                id INTEGER PRIMARY KEY,
                location_id INTEGER REFERENCES locations(id),
                collection_id INTEGER REFERENCES collections(id),
                path TEXT NOT NULL,
                base_type TEXT,
                hash TEXT REFERENCES contents(hash),
                mtime INTEGER,
                indexed_at INTEGER,
                UNIQUE(location_id, path)
            );

            CREATE TABLE events (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER,
                event_type TEXT,
                location_id INTEGER,
                path TEXT,
                hash TEXT
            );

            -- Directories the scanner discovered but couldn't classify.
            -- User (or the UI) classifies them; rule changes can reclassify in bulk.
            CREATE TABLE unknowns (
                id INTEGER PRIMARY KEY,
                location_id INTEGER REFERENCES locations(id),
                path TEXT NOT NULL UNIQUE,
                parent_path TEXT,
                discovered_at INTEGER,
                file_count INTEGER DEFAULT 0,
                dir_count INTEGER DEFAULT 0,
                total_size INTEGER DEFAULT 0,
                top_extensions TEXT
            );
            CREATE INDEX idx_unknowns_parent ON unknowns(parent_path);

            CREATE INDEX idx_files_hash ON files(hash);
            CREATE INDEX idx_files_path ON files(path);
            CREATE INDEX idx_files_collection ON files(collection_id);
            CREATE INDEX idx_collections_path ON collections(path);
            CREATE INDEX idx_collections_base ON collections(base_type);
            CREATE INDEX idx_collections_parent ON collections(parent_id);
            CREATE INDEX idx_tags_collection ON collection_tags(collection_id);
            CREATE INDEX idx_tags_key ON collection_tags(key);
            CREATE INDEX idx_tags_key_value ON collection_tags(key, value);
            CREATE INDEX idx_contents_size ON contents(size);
        "#,
        )?;

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

        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM locations WHERE path = ?1",
                params![path_str.as_ref()],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing {
            return Ok(id);
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "root".to_string());

        self.conn.execute(
            "INSERT INTO locations (device_id, name, path) VALUES (1, ?1, ?2)",
            params![name, path_str.as_ref()],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Insert or update a collection (and replace its tags).
    pub fn upsert_collection(&self, collection: &Collection) -> Result<i64> {
        self.conn.execute(
            r#"INSERT INTO collections
               (parent_id, location_id, path, name, base_type, privacy, identifier,
                total_size, file_count, child_count, manifest_hash, indexed_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
               ON CONFLICT(location_id, path) DO UPDATE SET
                 name = excluded.name,
                 base_type = excluded.base_type,
                 privacy = excluded.privacy,
                 identifier = excluded.identifier,
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
                collection.base_type.as_str(),
                collection.privacy.as_str(),
                collection.identifier,
                collection.total_size,
                collection.file_count,
                collection.child_count,
                collection.manifest_hash,
                collection.indexed_at,
            ],
        )?;

        let id: i64 = self.conn.query_row(
            "SELECT id FROM collections WHERE location_id = ?1 AND path = ?2",
            params![collection.location_id, collection.path],
            |row| row.get(0),
        )?;

        self.replace_tags(id, &collection.tags)?;

        Ok(id)
    }

    fn replace_tags(&self, collection_id: i64, tags: &[Tag]) -> Result<()> {
        self.conn.execute(
            "DELETE FROM collection_tags WHERE collection_id = ?1",
            params![collection_id],
        )?;
        for tag in tags {
            self.conn.execute(
                "INSERT OR IGNORE INTO collection_tags (collection_id, key, value) VALUES (?1, ?2, ?3)",
                params![collection_id, tag.key, tag.value],
            )?;
        }
        Ok(())
    }

    /// Add a single tag to a collection (does not remove existing tags).
    pub fn add_tag(&self, collection_id: i64, tag: &Tag) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO collection_tags (collection_id, key, value) VALUES (?1, ?2, ?3)",
            params![collection_id, tag.key, tag.value],
        )?;
        Ok(())
    }

    fn load_tags(&self, collection_id: i64) -> Result<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM collection_tags WHERE collection_id = ?1")?;
        let tags = stmt.query_map(params![collection_id], |row| {
            Ok(Tag {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;
        tags.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get statistics
    pub fn get_stats(&self) -> Result<Stats> {
        let collection_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM collections", [], |row| row.get(0))
            .unwrap_or(0);

        let file_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap_or(0);

        let total_size = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_size), 0) FROM collections WHERE parent_id IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let device_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM devices", [], |row| row.get(0))
            .unwrap_or(0);

        let location_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM locations", [], |row| row.get(0))
            .unwrap_or(0);

        let unprotected_count = self
            .conn
            .query_row(
                r#"SELECT COUNT(*) FROM collections c
                   WHERE c.parent_id IS NULL
                     AND NOT EXISTS (
                         SELECT 1 FROM locations l
                         WHERE l.id = c.location_id AND l.is_backup = 1
                     )"#,
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let mut stmt = self
            .conn
            .prepare("SELECT base_type, COUNT(*) FROM collections GROUP BY base_type")?;

        let type_counts = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;

        let by_type = type_counts.filter_map(|r| r.ok()).collect();

        let unknown_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM unknowns", [], |row| row.get(0))
            .unwrap_or(0);

        Ok(Stats {
            collection_count,
            file_count,
            total_size,
            device_count,
            location_count,
            unprotected_count,
            by_type,
            unknown_count,
        })
    }

    /// List collections with optional filters.
    pub fn list_collections(&self, filter: &CollectionFilter) -> Result<Vec<Collection>> {
        let mut sql = String::from(
            r#"SELECT id, parent_id, location_id, path, name, base_type, privacy,
               identifier, total_size, file_count, child_count, manifest_hash, indexed_at
               FROM collections WHERE 1=1"#,
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(bt) = &filter.base_type {
            sql.push_str(" AND base_type = ?");
            args.push(Box::new(bt.clone()));
        }
        if let Some(p) = &filter.privacy {
            sql.push_str(" AND privacy = ?");
            args.push(Box::new(p.clone()));
        }
        if let Some(parent) = &filter.parent_id {
            match parent {
                Some(pid) => {
                    sql.push_str(" AND parent_id = ?");
                    args.push(Box::new(*pid));
                }
                None => sql.push_str(" AND parent_id IS NULL"),
            }
        }
        if let Some(q) = &filter.query {
            sql.push_str(" AND (name LIKE ? OR path LIKE ?)");
            let pat = format!("%{}%", q);
            args.push(Box::new(pat.clone()));
            args.push(Box::new(pat));
        }

        sql.push_str(" ORDER BY total_size DESC");

        let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
        let offset = filter.offset.unwrap_or(0).max(0);
        sql.push_str(" LIMIT ? OFFSET ?");
        args.push(Box::new(limit));
        args.push(Box::new(offset));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), collection_row)?;

        let mut out = Vec::new();
        for row in rows {
            let mut c = row?;
            c.tags = self.load_tags(c.id)?;
            out.push(c);
        }
        Ok(out)
    }

    /// Find a collection by its integer id.
    pub fn find_collection_by_id(&self, id: i64) -> Result<Option<Collection>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, parent_id, location_id, path, name, base_type, privacy,
               identifier, total_size, file_count, child_count, manifest_hash, indexed_at
               FROM collections WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query_map(params![id], collection_row)?;
        match rows.next() {
            Some(Ok(mut c)) => {
                c.tags = self.load_tags(c.id)?;
                Ok(Some(c))
            }
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// List direct children of a collection.
    pub fn list_children(&self, parent_id: i64) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, parent_id, location_id, path, name, base_type, privacy,
               identifier, total_size, file_count, child_count, manifest_hash, indexed_at
               FROM collections WHERE parent_id = ?1 ORDER BY total_size DESC"#,
        )?;
        let rows = stmt.query_map(params![parent_id], collection_row)?;
        let mut out = Vec::new();
        for row in rows {
            let mut c = row?;
            c.tags = self.load_tags(c.id)?;
            out.push(c);
        }
        Ok(out)
    }

    /// List files in a collection (capped).
    pub fn list_files_in_collection(&self, collection_id: i64, limit: i64) -> Result<Vec<File>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, location_id, collection_id, path, base_type, hash, mtime, indexed_at
               FROM files WHERE collection_id = ?1 ORDER BY path LIMIT ?2"#,
        )?;
        let rows = stmt.query_map(params![collection_id, limit], |row| {
            Ok(File {
                id: row.get(0)?,
                location_id: row.get(1)?,
                collection_id: row.get::<_, Option<i64>>(2)?,
                path: row.get(3)?,
                base_type: BaseType::from_str(&row.get::<_, Option<String>>(4)?.unwrap_or_default()),
                hash: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                mtime: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                indexed_at: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Indexed ancestors of `path`, from highest to lowest (not including `path` itself).
    pub fn list_path_ancestors(&self, path: &str) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, parent_id, location_id, path, name, base_type, privacy,
               identifier, total_size, file_count, child_count, manifest_hash, indexed_at
               FROM collections
               WHERE ?1 LIKE path || '/%'
               ORDER BY LENGTH(path)"#,
        )?;
        let rows = stmt.query_map(params![path], collection_row)?;
        let mut out = Vec::new();
        for row in rows {
            let mut c = row?;
            c.tags = self.load_tags(c.id)?;
            out.push(c);
        }
        Ok(out)
    }

    /// List all known locations.
    pub fn list_locations(&self) -> Result<Vec<Location>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, device_id, name, path, is_backup, is_ephemeral, is_readonly, last_scan
               FROM locations ORDER BY name"#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Location {
                id: row.get(0)?,
                device_id: row.get(1)?,
                name: row.get(2)?,
                path: row.get(3)?,
                is_backup: row.get::<_, i64>(4)? != 0,
                is_ephemeral: row.get::<_, i64>(5)? != 0,
                is_readonly: row.get::<_, i64>(6)? != 0,
                last_scan: row.get::<_, Option<i64>>(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Find a collection by its path
    pub fn find_collection_by_path(&self, path: &Path) -> Result<Option<Collection>> {
        let path_str = path.to_string_lossy();

        let row = self.conn.query_row(
            r#"SELECT id, parent_id, location_id, path, name, base_type, privacy,
               identifier, total_size, file_count, child_count, manifest_hash, indexed_at
               FROM collections WHERE path = ?1"#,
            params![path_str.as_ref()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, u64>(8)?,
                    row.get::<_, u64>(9)?,
                    row.get::<_, u64>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, i64>(12)?,
                ))
            },
        );

        match row {
            Ok((
                id,
                parent_id,
                location_id,
                path,
                name,
                base,
                privacy,
                identifier,
                total_size,
                file_count,
                child_count,
                manifest_hash,
                indexed_at,
            )) => {
                let tags = self.load_tags(id)?;
                Ok(Some(Collection {
                    id,
                    parent_id,
                    location_id,
                    path,
                    name,
                    base_type: BaseType::from_str(&base),
                    tags,
                    privacy: PrivacyLevel::from_str(&privacy),
                    identifier,
                    total_size,
                    file_count,
                    child_count,
                    manifest_hash,
                    indexed_at,
                }))
            }
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

    // ---------- Unknowns ----------

    /// Record (or refresh) an unclassified directory.
    pub fn upsert_unknown(&self, u: &Unknown) -> Result<i64> {
        let top_ext_json = serde_json::to_string(&u.top_extensions)?;
        self.conn.execute(
            r#"INSERT INTO unknowns
               (location_id, path, parent_path, discovered_at,
                file_count, dir_count, total_size, top_extensions)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
               ON CONFLICT(path) DO UPDATE SET
                 parent_path = excluded.parent_path,
                 discovered_at = excluded.discovered_at,
                 file_count = excluded.file_count,
                 dir_count = excluded.dir_count,
                 total_size = excluded.total_size,
                 top_extensions = excluded.top_extensions"#,
            params![
                u.location_id,
                u.path,
                u.parent_path,
                u.discovered_at,
                u.file_count,
                u.dir_count,
                u.total_size,
                top_ext_json,
            ],
        )?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM unknowns WHERE path = ?1",
            params![u.path],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// List all unknowns, largest-first.
    pub fn list_unknowns(&self) -> Result<Vec<Unknown>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, location_id, path, parent_path, discovered_at,
               file_count, dir_count, total_size, top_extensions
               FROM unknowns ORDER BY total_size DESC"#,
        )?;
        let rows = stmt.query_map([], unknown_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Fetch a single unknown by id.
    pub fn find_unknown_by_id(&self, id: i64) -> Result<Option<Unknown>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, location_id, path, parent_path, discovered_at,
               file_count, dir_count, total_size, top_extensions
               FROM unknowns WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query_map(params![id], unknown_row)?;
        match rows.next() {
            Some(Ok(u)) => Ok(Some(u)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    pub fn remove_unknown_by_id(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM unknowns WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn remove_unknown_at_path(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM unknowns WHERE path = ?1", params![path])?;
        Ok(())
    }

    pub fn find_unknown_by_path(&self, path: &str) -> Result<Option<Unknown>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, location_id, path, parent_path, discovered_at,
               file_count, dir_count, total_size, top_extensions
               FROM unknowns WHERE path = ?1"#,
        )?;
        let mut rows = stmt.query_map(params![path], unknown_row)?;
        match rows.next() {
            Some(Ok(u)) => Ok(Some(u)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }
}

fn unknown_row(row: &Row<'_>) -> rusqlite::Result<Unknown> {
    let ext_json: Option<String> = row.get(8)?;
    let top_extensions = ext_json
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();
    Ok(Unknown {
        id: row.get(0)?,
        location_id: row.get(1)?,
        path: row.get(2)?,
        parent_path: row.get(3)?,
        discovered_at: row.get(4)?,
        file_count: row.get(5)?,
        dir_count: row.get(6)?,
        total_size: row.get(7)?,
        top_extensions,
    })
}
