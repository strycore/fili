use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};
use std::path::{Path, PathBuf};

use crate::models::*;

/// Filters for listing entries from the API.
#[derive(Default, Debug)]
pub struct EntryFilter {
    pub base_type: Option<String>,
    pub privacy: Option<String>,
    pub parent_id: Option<Option<i64>>, // Some(None) = top-level only
    pub query: Option<String>,
    pub tag_key: Option<String>,
    pub tag_value: Option<String>,
    pub is_item: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn entry_row(row: &Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        location_id: row.get(2)?,
        path: row.get(3)?,
        name: row.get(4)?,
        base_type: BaseType::from_str(&row.get::<_, String>(5)?),
        is_item: row.get::<_, i64>(6)? != 0,
        is_dir: row.get::<_, i64>(7)? != 0,
        tags: Vec::new(),
        privacy: PrivacyLevel::from_str(&row.get::<_, String>(8)?),
        identifier: row.get(9)?,
        total_size: row.get(10)?,
        file_count: row.get(11)?,
        child_count: row.get(12)?,
        manifest_hash: row.get(13)?,
        indexed_at: row.get(14)?,
    })
}

const ENTRY_COLUMNS: &str = "id, parent_id, location_id, path, name, base_type, is_item, is_dir, privacy, identifier, total_size, file_count, child_count, manifest_hash, indexed_at";

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
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run `f` inside a single SQLite transaction.
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

            -- Unified entries table — holds both collections (have children)
            -- and items (atomic units). is_item says which; is_dir says
            -- whether the underlying fs entry is a directory or a file.
            CREATE TABLE entries (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER REFERENCES entries(id),
                location_id INTEGER REFERENCES locations(id),
                path TEXT NOT NULL,
                name TEXT,
                base_type TEXT NOT NULL,
                is_item INTEGER NOT NULL DEFAULT 0,
                is_dir INTEGER NOT NULL DEFAULT 1,
                privacy TEXT DEFAULT 'public',
                identifier TEXT,
                total_size INTEGER DEFAULT 0,
                file_count INTEGER DEFAULT 0,
                child_count INTEGER DEFAULT 0,
                manifest_hash TEXT,
                indexed_at INTEGER,
                UNIQUE(location_id, path)
            );

            -- Multi-valued tags on entries.
            CREATE TABLE entry_tags (
                entry_id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                key TEXT NOT NULL,
                value TEXT,
                PRIMARY KEY (entry_id, key, value)
            );

            CREATE TABLE events (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER,
                event_type TEXT,
                location_id INTEGER,
                path TEXT,
                hash TEXT
            );

            CREATE TABLE drives (
                id INTEGER PRIMARY KEY,
                uuid TEXT UNIQUE,
                label TEXT,
                fs_type TEXT,
                size TEXT,
                model TEXT,
                serial TEXT,
                friendly_name TEXT,
                current_mount TEXT,
                first_seen INTEGER,
                last_seen INTEGER
            );
            CREATE INDEX idx_drives_label ON drives(label);
            CREATE INDEX idx_drives_mount ON drives(current_mount);

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

            CREATE INDEX idx_entries_path ON entries(path);
            CREATE INDEX idx_entries_base ON entries(base_type);
            CREATE INDEX idx_entries_parent ON entries(parent_id);
            CREATE INDEX idx_entries_is_item ON entries(is_item);
            CREATE INDEX idx_tags_entry ON entry_tags(entry_id);
            CREATE INDEX idx_tags_key ON entry_tags(key);
            CREATE INDEX idx_tags_key_value ON entry_tags(key, value);
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

        // Use the full path as the location name to guarantee uniqueness
        // under the existing UNIQUE(device_id, name) constraint. Two paths
        // with the same basename (e.g. ~/Games and /media/data/Games) used
        // to collide. A friendly display name can live in a separate field
        // later.
        let name = path_str.as_ref().to_string();

        self.conn.execute(
            "INSERT INTO locations (device_id, name, path) VALUES (1, ?1, ?2)",
            params![name, path_str.as_ref()],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Stamp a location's last_scan timestamp to now. Called when a scan
    /// finishes so the "Recent" sidebar can sort by recency.
    pub fn touch_location(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE locations SET last_scan = strftime('%s', 'now') WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Insert or update an entry (and replace its tags).
    ///
    /// A filesystem path has a single meaning regardless of which scan root
    /// discovered it. If a row for this path already exists (possibly under
    /// a different location_id from a previous scan covering the same
    /// subtree), update it in place. Otherwise insert a new row. This
    /// keeps the entries table path-unique in practice even though the
    /// legacy `UNIQUE(location_id, path)` constraint still exists.
    pub fn upsert_entry(&self, entry: &Entry) -> Result<i64> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM entries WHERE path = ?1",
                params![entry.path],
                |row| row.get(0),
            )
            .ok();

        let id = match existing {
            Some(id) => {
                self.conn.execute(
                    r#"UPDATE entries SET
                         parent_id = ?1, location_id = ?2, name = ?3,
                         base_type = ?4, is_item = ?5, is_dir = ?6,
                         privacy = ?7, identifier = ?8, total_size = ?9,
                         file_count = ?10, child_count = ?11,
                         manifest_hash = ?12, indexed_at = ?13
                       WHERE id = ?14"#,
                    params![
                        entry.parent_id,
                        entry.location_id,
                        entry.name,
                        entry.base_type.as_str(),
                        entry.is_item as i64,
                        entry.is_dir as i64,
                        entry.privacy.as_str(),
                        entry.identifier,
                        entry.total_size,
                        entry.file_count,
                        entry.child_count,
                        entry.manifest_hash,
                        entry.indexed_at,
                        id,
                    ],
                )?;
                id
            }
            None => {
                self.conn.execute(
                    r#"INSERT INTO entries
                       (parent_id, location_id, path, name, base_type, is_item, is_dir,
                        privacy, identifier, total_size, file_count, child_count,
                        manifest_hash, indexed_at)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
                    params![
                        entry.parent_id,
                        entry.location_id,
                        entry.path,
                        entry.name,
                        entry.base_type.as_str(),
                        entry.is_item as i64,
                        entry.is_dir as i64,
                        entry.privacy.as_str(),
                        entry.identifier,
                        entry.total_size,
                        entry.file_count,
                        entry.child_count,
                        entry.manifest_hash,
                        entry.indexed_at,
                    ],
                )?;
                self.conn.last_insert_rowid()
            }
        };

        self.replace_tags(id, &entry.tags)?;

        Ok(id)
    }

    fn replace_tags(&self, entry_id: i64, tags: &[Tag]) -> Result<()> {
        self.conn.execute(
            "DELETE FROM entry_tags WHERE entry_id = ?1",
            params![entry_id],
        )?;
        for tag in tags {
            self.conn.execute(
                "INSERT OR IGNORE INTO entry_tags (entry_id, key, value) VALUES (?1, ?2, ?3)",
                params![entry_id, tag.key, tag.value],
            )?;
        }
        Ok(())
    }

    /// Add a single tag to an entry (does not remove existing tags).
    pub fn add_tag(&self, entry_id: i64, tag: &Tag) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO entry_tags (entry_id, key, value) VALUES (?1, ?2, ?3)",
            params![entry_id, tag.key, tag.value],
        )?;
        Ok(())
    }

    fn load_tags(&self, entry_id: i64) -> Result<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM entry_tags WHERE entry_id = ?1")?;
        let tags = stmt.query_map(params![entry_id], |row| {
            Ok(Tag {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;
        tags.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get statistics
    pub fn get_stats(&self) -> Result<Stats> {
        let entry_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap_or(0);

        let collection_count = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE is_item = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let item_count = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE is_item = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_size = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(total_size), 0) FROM entries WHERE parent_id IS NULL",
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
                r#"SELECT COUNT(*) FROM entries e
                   WHERE e.parent_id IS NULL
                     AND NOT EXISTS (
                         SELECT 1 FROM locations l
                         WHERE l.id = e.location_id AND l.is_backup = 1
                     )"#,
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let mut stmt = self
            .conn
            .prepare("SELECT base_type, COUNT(*) FROM entries GROUP BY base_type")?;

        let type_counts = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;

        let by_type = type_counts.filter_map(|r| r.ok()).collect();

        let unknown_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM unknowns", [], |row| row.get(0))
            .unwrap_or(0);

        Ok(Stats {
            entry_count,
            collection_count,
            item_count,
            total_size,
            device_count,
            location_count,
            unprotected_count,
            by_type,
            unknown_count,
        })
    }

    /// List entries with optional filters.
    pub fn list_entries(&self, filter: &EntryFilter) -> Result<Vec<Entry>> {
        let mut sql = format!("SELECT {ENTRY_COLUMNS} FROM entries WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(bt) = &filter.base_type {
            sql.push_str(" AND base_type = ?");
            args.push(Box::new(bt.clone()));
        }
        if let Some(p) = &filter.privacy {
            sql.push_str(" AND privacy = ?");
            args.push(Box::new(p.clone()));
        }
        if let Some(is_item) = filter.is_item {
            sql.push_str(" AND is_item = ?");
            args.push(Box::new(is_item as i64));
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
        if let Some(key) = &filter.tag_key {
            sql.push_str(" AND id IN (SELECT entry_id FROM entry_tags WHERE key = ?");
            args.push(Box::new(key.clone()));
            if let Some(value) = &filter.tag_value {
                sql.push_str(" AND value = ?");
                args.push(Box::new(value.clone()));
            }
            sql.push(')');
        }

        sql.push_str(" ORDER BY total_size DESC");

        let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
        let offset = filter.offset.unwrap_or(0).max(0);
        sql.push_str(" LIMIT ? OFFSET ?");
        args.push(Box::new(limit));
        args.push(Box::new(offset));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), entry_row)?;

        let mut out = Vec::new();
        for row in rows {
            let mut e = row?;
            e.tags = self.load_tags(e.id)?;
            out.push(e);
        }
        Ok(out)
    }

    /// Find an entry by its integer id.
    pub fn find_entry_by_id(&self, id: i64) -> Result<Option<Entry>> {
        let sql = format!("SELECT {ENTRY_COLUMNS} FROM entries WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(params![id], entry_row)?;
        match rows.next() {
            Some(Ok(mut e)) => {
                e.tags = self.load_tags(e.id)?;
                Ok(Some(e))
            }
            Some(Err(err)) => Err(err.into()),
            None => Ok(None),
        }
    }

    /// List direct children of an entry.
    pub fn list_children(&self, parent_id: i64) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM entries WHERE parent_id = ?1 ORDER BY total_size DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id], entry_row)?;
        let mut out = Vec::new();
        for row in rows {
            let mut e = row?;
            e.tags = self.load_tags(e.id)?;
            out.push(e);
        }
        Ok(out)
    }

    /// Indexed ancestors of `path`, from highest to lowest (not including `path` itself).
    pub fn list_path_ancestors(&self, path: &str) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT {ENTRY_COLUMNS} FROM entries WHERE ?1 LIKE path || '/%' ORDER BY LENGTH(path)"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![path], entry_row)?;
        let mut out = Vec::new();
        for row in rows {
            let mut e = row?;
            e.tags = self.load_tags(e.id)?;
            out.push(e);
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

    /// Find an entry by its path
    pub fn find_entry_by_path(&self, path: &Path) -> Result<Option<Entry>> {
        let path_str = path.to_string_lossy();
        let sql = format!("SELECT {ENTRY_COLUMNS} FROM entries WHERE path = ?1");
        let row = self
            .conn
            .query_row(&sql, params![path_str.as_ref()], entry_row);

        match row {
            Ok(mut e) => {
                e.tags = self.load_tags(e.id)?;
                Ok(Some(e))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set privacy level for an entry
    pub fn set_privacy(&self, entry_id: i64, privacy: &PrivacyLevel) -> Result<()> {
        self.conn.execute(
            "UPDATE entries SET privacy = ?1 WHERE id = ?2",
            params![privacy.as_str(), entry_id],
        )?;
        Ok(())
    }

    // ---------- Unknowns ----------

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

    pub fn list_unknowns(&self) -> Result<Vec<Unknown>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, location_id, path, parent_path, discovered_at,
               file_count, dir_count, total_size, top_extensions
               FROM unknowns ORDER BY total_size DESC"#,
        )?;
        let rows = stmt.query_map([], unknown_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

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

    pub fn list_unknowns_with_parent(&self, parent_path: &str) -> Result<Vec<Unknown>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, location_id, path, parent_path, discovered_at,
               file_count, dir_count, total_size, top_extensions
               FROM unknowns WHERE parent_path = ?1 ORDER BY path"#,
        )?;
        let rows = stmt.query_map(params![parent_path], unknown_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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

    // ---------- Drives ----------

    pub fn upsert_drive(&self, d: &Drive) -> Result<i64> {
        if let Some(uuid) = &d.uuid {
            self.conn.execute(
                r#"INSERT INTO drives
                   (uuid, label, fs_type, size, model, serial,
                    friendly_name, current_mount, first_seen, last_seen)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                   ON CONFLICT(uuid) DO UPDATE SET
                     label = excluded.label,
                     fs_type = excluded.fs_type,
                     size = excluded.size,
                     model = excluded.model,
                     serial = excluded.serial,
                     friendly_name = COALESCE(friendly_name, excluded.friendly_name),
                     current_mount = excluded.current_mount,
                     last_seen = excluded.last_seen"#,
                params![
                    uuid,
                    d.label,
                    d.fs_type,
                    d.size,
                    d.model,
                    d.serial,
                    d.friendly_name,
                    d.current_mount,
                    d.first_seen,
                    d.last_seen,
                ],
            )?;
            let id: i64 = self.conn.query_row(
                "SELECT id FROM drives WHERE uuid = ?1",
                params![uuid],
                |row| row.get(0),
            )?;
            return Ok(id);
        }

        let existing: Option<i64> = self
            .conn
            .query_row(
                r#"SELECT id FROM drives
                   WHERE uuid IS NULL AND label IS ?1 AND fs_type IS ?2 AND size IS ?3"#,
                params![d.label, d.fs_type, d.size],
                |row| row.get(0),
            )
            .ok();
        if let Some(id) = existing {
            self.conn.execute(
                r#"UPDATE drives SET
                     model = ?1, serial = ?2,
                     current_mount = ?3, last_seen = ?4
                   WHERE id = ?5"#,
                params![d.model, d.serial, d.current_mount, d.last_seen, id],
            )?;
            return Ok(id);
        }
        self.conn.execute(
            r#"INSERT INTO drives
               (uuid, label, fs_type, size, model, serial,
                friendly_name, current_mount, first_seen, last_seen)
               VALUES (NULL, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                d.label,
                d.fs_type,
                d.size,
                d.model,
                d.serial,
                d.friendly_name,
                d.current_mount,
                d.first_seen,
                d.last_seen,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn clear_stale_mounts(&self, active_mounts: &[String]) -> Result<()> {
        let placeholders = active_mounts
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = if active_mounts.is_empty() {
            "UPDATE drives SET current_mount = NULL".to_string()
        } else {
            format!(
                "UPDATE drives SET current_mount = NULL
                 WHERE current_mount IS NOT NULL AND current_mount NOT IN ({})",
                placeholders
            )
        };
        let params_owned: Vec<Box<dyn rusqlite::ToSql>> = active_mounts
            .iter()
            .map(|s| Box::new(s.clone()) as Box<dyn rusqlite::ToSql>)
            .collect();
        let params_ref: Vec<&dyn rusqlite::ToSql> =
            params_owned.iter().map(|b| b.as_ref()).collect();
        self.conn.execute(&sql, params_ref.as_slice())?;
        Ok(())
    }

    pub fn list_drives(&self) -> Result<Vec<Drive>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, uuid, label, fs_type, size, model, serial,
               friendly_name, current_mount, first_seen, last_seen
               FROM drives
               ORDER BY (current_mount IS NULL), friendly_name, label, uuid"#,
        )?;
        let rows = stmt.query_map([], drive_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn rename_drive(&self, id: i64, friendly_name: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE drives SET friendly_name = ?1 WHERE id = ?2",
            params![friendly_name, id],
        )?;
        Ok(())
    }
}

fn drive_row(row: &Row<'_>) -> rusqlite::Result<Drive> {
    Ok(Drive {
        id: row.get(0)?,
        uuid: row.get(1)?,
        label: row.get(2)?,
        fs_type: row.get(3)?,
        size: row.get(4)?,
        model: row.get(5)?,
        serial: row.get(6)?,
        friendly_name: row.get(7)?,
        current_mount: row.get(8)?,
        first_seen: row.get(9)?,
        last_seen: row.get(10)?,
    })
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
