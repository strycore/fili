# Fili - Personal File Intelligence System

> "One index to find them, one index to track them all"

## Vision

A unified file management system for data hoarders. Know where everything is, what's backed up, what's duplicated, and what's at risk.

## Core Problems to Solve

1. **Where is that file?** — Instant search across 30TB+
2. **Is this backed up?** — Track which files exist on which drives
3. **What's at risk?** — Surface files with no backup copy
4. **What's wasting space?** — Find duplicates, old caches, cruft
5. **What changed?** — Track file movements, deletions, additions over time

## Concepts

### Storage Locations
Named mount points / paths with metadata:
- `local` — /home/strider (NVMe, fast, limited space, NOT a backup)
- `backup1` — /mnt/Backup (7.3TB, primary backup)
- `backup2` — /mnt/Backup2 (10.9TB, secondary backup)
- `data` — /mnt/data (14.6TB, bulk storage)
- `games` — /mnt/games (7.3TB, game archives)
- `cloud` — Nextcloud sync folder (off-site backup)

Each location has properties:
- `is_backup: bool` — counts as a backup copy
- `is_ephemeral: bool` — can be regenerated (caches, builds)
- `priority: int` — which copy to prefer

### File Identity
Files identified by:
- Content hash (xxhash3 for speed, with optional SHA256 for verification)
- Size + partial hash for quick matching
- Path is metadata, not identity (same file can exist in multiple places)

### File Classification
Auto-detect or manual tags:
- `document` — PDFs, office docs, important
- `code` — source files, projects
- `media/video`, `media/audio`, `media/image`
- `game` — game files, ROMs, ISOs
- `archive` — compressed files
- `cache` — regenerable, safe to delete
- `system` — OS files, configs

### Protection Status
For each unique file (by hash):
- `protected` — exists on 2+ backup locations
- `backed-up` — exists on 1 backup location
- `local-only` — only on non-backup storage (AT RISK)
- `orphaned` — only on backup, deleted from local

## Commands

```bash
# Indexing
fili scan /home/strider --location local
fili scan /mnt/Backup --location backup1
fili rescan              # re-scan all known locations
fili watch               # daemon mode, watch for changes

# Discovery
fili status              # overview: files, sizes, protection status
fili unprotected         # files with no backup (DANGER)
fili orphans             # backup files not in local
fili duplicates          # same content, multiple locations
fili duplicates --same-drive  # dupes wasting space on same drive

# Search
fili find "lutris"       # search filenames
fili find --content "TODO"  # full-text search (indexed files)
fili find --type video --size ">1GB"

# Analysis
fili largest             # biggest files
fili oldest              # oldest accessed files
fili changes             # what changed since last scan
fili waste               # space wasted by dupes, caches

# Actions
fili verify              # check hashes still match (bit rot detection)
fili backup <file>       # copy to backup location
fili dedupe --dry-run    # show what would be deduped
fili clean-cache         # remove known cache paths

# UI
fili tui                 # interactive terminal UI
fili serve               # web dashboard on localhost
```

## Data Model

### SQLite Schema (initial)

```sql
-- Storage locations
CREATE TABLE locations (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    path TEXT NOT NULL,
    is_backup BOOLEAN DEFAULT FALSE,
    is_ephemeral BOOLEAN DEFAULT FALSE,
    last_scan INTEGER
);

-- Unique file contents
CREATE TABLE contents (
    hash TEXT PRIMARY KEY,      -- xxhash3
    size INTEGER NOT NULL,
    sha256 TEXT,                -- optional verification hash
    mime_type TEXT,
    classification TEXT,
    first_seen INTEGER,
    last_verified INTEGER
);

-- File instances (path + content)
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    location_id INTEGER REFERENCES locations(id),
    path TEXT NOT NULL,
    hash TEXT REFERENCES contents(hash),
    mtime INTEGER,
    indexed_at INTEGER,
    UNIQUE(location_id, path)
);

-- For tracking changes over time
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    timestamp INTEGER,
    event_type TEXT,  -- 'added', 'removed', 'modified', 'moved'
    location_id INTEGER,
    path TEXT,
    hash TEXT
);

CREATE INDEX idx_files_hash ON files(hash);
CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_contents_size ON contents(size);
```

## Tech Stack

- **Language:** Rust
- **File walking:** walkdir + ignore (respects .gitignore)
- **Hashing:** xxhash-rust (fast), sha2 (verification)
- **Database:** SQLite via rusqlite (or sled for embedded)
- **Search:** tantivy for full-text (optional)
- **CLI:** clap
- **TUI:** ratatui
- **Parallelism:** rayon
- **Web UI:** axum + htmx (optional, later)

## Development Phases

### Phase 1: Core Index
- [ ] Rust project scaffold
- [ ] Location management (add, remove, list)
- [ ] File scanning with xxhash
- [ ] Basic search by filename
- [ ] Status command (file counts, sizes)

### Phase 2: Protection Tracking
- [ ] Cross-reference files across locations
- [ ] Unprotected files report
- [ ] Orphan detection
- [ ] Duplicate detection

### Phase 3: Intelligence
- [ ] File classification (by extension, magic bytes)
- [ ] Change tracking between scans
- [ ] Bit rot detection (periodic verification)
- [ ] Cache identification and cleanup

### Phase 4: UI
- [ ] Rich CLI output (colors, progress bars)
- [ ] TUI browser
- [ ] Web dashboard

### Phase 5: Automation
- [ ] Watch mode (inotify/fswatch)
- [ ] Scheduled scans
- [ ] Auto-backup rules
- [ ] Notifications for at-risk files

## Prior Art / Inspiration

- **fclones** — fast duplicate finder (Rust)
- **rmlint** — duplicate/lint finder
- **dust/dua** — disk usage analyzers
- **broot** — file browser
- **syncthing** — file sync (for the "is it backed up" concept)
- **borg** — backup (deduplication concepts)

## Open Questions

- Use content-addressable storage like git/borg? Or just track paths?
- How to handle very large files (>10GB)? Partial hashing?
- Include cloud storage (S3, B2) in the index?
- Integration with existing backup tools?

---

*This is a living document. Update as the project evolves.*
