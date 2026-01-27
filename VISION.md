# Fili - Personal File Intelligence System

> "One index to find them, one index to track them all"

## Vision

A unified file inventory across your entire digital life. Track files across desktops, laptops, phones, external drives, NAS, and cloud storage. Know where everything is, what's backed up, what's duplicated, and what's at risk — across all your devices.

## Core Problems to Solve

1. **Where is that file?** — Instant search across all devices and drives
2. **Is this backed up?** — Track which files exist on which devices/locations
3. **What's at risk?** — Surface files with no backup copy
4. **What's wasting space?** — Find duplicates across devices
5. **What changed?** — Track file movements, deletions, additions over time
6. **What's on my phone?** — Index mobile devices, cameras, etc.

## Concepts

### Devices
Physical or virtual machines that hold files:
- `desktop` — Primary workstation
- `laptop` — Secondary machine
- `phone` — Mobile device (Android/iOS)
- `nas` — Network attached storage
- `cloud` — Cloud provider (Dropbox, Google Drive, S3, etc.)

### Storage Locations
Paths within a device. A device can have multiple locations:
- `desktop:home` — /home/user
- `desktop:backup` — /mnt/backup (external drive)
- `laptop:home` — /home/user
- `phone:camera` — DCIM folder
- `cloud:dropbox` — Dropbox root

Each location has properties:
- `is_backup: bool` — counts as a backup copy
- `is_ephemeral: bool` — can be regenerated (caches, builds)
- `is_readonly: bool` — archive, don't expect changes
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

### Collections (Atomic Folders)
Some folders should be treated as single units, not indexed file-by-file:

**Auto-detected:**
- **Git repos** — folder contains `.git` → treat as one "project" entity
- **Games** — detected by common patterns (Steam manifests, .exe + data folders, ROM files)
- **App bundles** — .app (macOS), installed software
- **Package artifacts** — node_modules, venv, target/, build/

**Behavior:**
- Index the collection as ONE entry with aggregate metadata (total size, file count)
- Store a manifest hash (hash of sorted file listing) for change detection
- Don't pollute the index with thousands of internal files
- Can "expand" a collection to see contents if needed

**Example:**
```
desktop:home/Games/Half-Life/     → collection:game "Half-Life" (1.2GB, 3,400 files)
desktop:home/Projects/lutris/     → collection:git "lutris" (45MB, 800 files)
desktop:home/Projects/web/node_modules/ → collection:package (ignored)
```

**Detection heuristics:**
- `.git/` → git repo (use remote URL as identifier if available)
- `*.exe` + large file tree → Windows game
- `*.gog`, `*.steam` manifests → game with known ID
- `package.json` + `node_modules/` → npm project
- `Cargo.toml` + `target/` → Rust project
- `.iso`, `.cue/.bin`, `.rom`, `.zip` in Games folder → single game archive

### Protection Status
For each unique file (by hash):
- `protected` — exists on 2+ backup locations
- `backed-up` — exists on 1 backup location
- `local-only` — only on non-backup storage (AT RISK)
- `orphaned` — only on backup, deleted from local

## Commands

```bash
# Device & location management
fili device add desktop --hostname mypc
fili device add phone --type mobile
fili device list
fili location add desktop:home /home/user
fili location add desktop:backup /mnt/backup --is-backup
fili location add phone:camera /sdcard/DCIM --readonly

# Indexing
fili scan desktop:home              # scan a specific location
fili scan desktop                   # scan all locations on device
fili rescan                         # re-scan all known locations
fili watch                          # daemon mode, watch for changes
fili import phone.json --device phone  # import index from another device

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
-- Devices (machines, phones, cloud accounts)
CREATE TABLE devices (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,      -- 'desktop', 'laptop', 'phone'
    hostname TEXT,                   -- actual hostname if available
    device_type TEXT,                -- 'local', 'remote', 'mobile', 'cloud'
    last_seen INTEGER
);

-- Storage locations within devices
CREATE TABLE locations (
    id INTEGER PRIMARY KEY,
    device_id INTEGER REFERENCES devices(id),
    name TEXT NOT NULL,              -- 'home', 'backup', 'camera'
    path TEXT NOT NULL,
    is_backup BOOLEAN DEFAULT FALSE,
    is_ephemeral BOOLEAN DEFAULT FALSE,
    is_readonly BOOLEAN DEFAULT FALSE,
    last_scan INTEGER,
    UNIQUE(device_id, name)
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

-- Collections (atomic folders treated as single units)
CREATE TABLE collections (
    id INTEGER PRIMARY KEY,
    location_id INTEGER REFERENCES locations(id),
    path TEXT NOT NULL,
    name TEXT,                       -- "Half-Life", "lutris"
    collection_type TEXT,            -- 'git', 'game', 'package', 'app'
    identifier TEXT,                 -- git remote URL, Steam app ID, etc.
    total_size INTEGER,
    file_count INTEGER,
    manifest_hash TEXT,              -- hash of file listing for change detection
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

## Multi-Device Architecture

### Index Synchronization
Each device runs fili locally and generates its own index. Indexes are merged:

1. **Export/Import** (simple) — `fili export` on phone, copy JSON, `fili import` on desktop
2. **Sync folder** — Put index DB in Syncthing/Dropbox, auto-merge on open
3. **Server mode** (future) — Central fili server, devices push updates

### Mobile Indexing
- Android: Termux + fili binary, or dedicated app
- iOS: Harder — maybe index via USB backup, or companion app
- Cloud photos: API integration (Google Photos, iCloud)

### Cloud Storage
- Mount-based: rclone mount, scan like local
- API-based: Native integration for Dropbox, S3, B2, Google Drive
- Treat cloud as another device: `cloud:dropbox`, `cloud:s3-backup`

## Open Questions

- Use content-addressable storage like git/borg? Or just track paths?
- How to handle very large files (>10GB)? Partial hashing?
- How to handle offline devices? Stale index detection?
- Conflict resolution when same path exists on multiple devices?
- Integration with existing backup tools (borg, restic)?
- Mobile app vs CLI-only?

---

*This is a living document. Update as the project evolves.*
