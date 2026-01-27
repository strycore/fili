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

### Collections (Hierarchical)
Collections are groups of related items. They can be nested — collections of collections.

**Collection types:**
- **Git repos** — folder contains `.git` → software project
- **Games** — detected by patterns (Steam manifests, .exe + data, ROMs)
- **Photo albums** — folder of images, often with date/event name
- **Music albums** — folder of audio files, often Artist/Album structure
- **Video series** — TV shows, movie collections
- **App bundles** — .app (macOS), installed software
- **Package artifacts** — node_modules, venv, target/ (ephemeral, skip)

**Hierarchy examples:**
```
~/Projects/                        → collection:projects (contains collections)
  └── lutris/                      → collection:git "lutris"
  └── fili/                        → collection:git "fili"
  └── lunchcraft/                  → collection:git "lunchcraft"

~/Pictures/                        → collection:photos (contains collections)
  └── 2024-vacation-japan/         → collection:album "Japan Vacation 2024"
  └── 2023-wedding/                → collection:album "Wedding 2023"

~/Music/                           → collection:music (contains collections)
  └── Pink Floyd/                  → collection:artist
      └── Dark Side of the Moon/   → collection:album
      └── The Wall/                → collection:album

/mnt/Backup/Games/DOS/             → collection:games (contains collections)
  └── DOOM/                        → collection:game "DOOM"
  └── Duke Nukem 3D/               → collection:game "Duke Nukem 3D"
```

**Behavior:**
- Collections can contain files, other collections, or both
- Index the collection as ONE entry with aggregate metadata (total size, file count, child count)
- Store a manifest hash for change detection
- Parent collections track their children, not individual files
- Can "drill down" into nested collections

**What gets tracked at each level:**
```
~/Projects/                        → 45 projects, 2.3GB total
  └── lutris/                      → 800 files, 45MB, git:github.com/lutris/lutris
```

Not:
```
~/Projects/lutris/lutris/game.py   → 15KB (NO - too granular)
```

**Detection heuristics:**
- `.git/` → git repo (use remote URL as identifier if available)
- `*.exe` + large file tree → Windows game
- `*.gog`, `*.steam` manifests → game with known ID
- `package.json` + `node_modules/` → npm project
- `Cargo.toml` + `target/` → Rust project
- `.iso`, `.cue/.bin`, `.rom`, `.zip` in Games folder → single game archive
- Nested system paths → external system snapshot (see below)

### External System Snapshots
Detect directory structures from other systems (backups, migrations, old installs):

**Patterns to detect:**
```
~/Projects/email-migration/postfix/     → looks like /etc/postfix
~/backups/server/var/log/              → looks like /var/log from another system
~/old-laptop/home/user/.config/        → another user's home directory
/mnt/Backup/root-2023/etc/             → full system backup
```

**Detection signals:**
- System paths (`/etc/*`, `/var/*`, `/usr/*`) nested inside user directories
- Home directory patterns (`/home/username/`) nested unnaturally
- Common config files in unexpected locations (postfix/main.cf, nginx/nginx.conf)
- Multiple top-level system dirs together (etc + var + home = system snapshot)

**Behavior:**
- Flag as `snapshot:system` or `snapshot:partial`
- Track the apparent source (server backup, old laptop, etc.)
- Treat as collection — don't index internals file-by-file
- Offer to tag with source system name

**Example interaction:**
```
$ fili scan ~/Projects

Found nested system structure:
  ~/Projects/email-migration/postfix/
  
  This looks like /etc/postfix from another system.
  [t] Tag as external snapshot
  [i] Index as normal files  
  [s] Skip
  > t
  
  Name this source system: old-mail-server
  ✓ Tagged as snapshot from "old-mail-server"
```

### Protection Status
For each unique file (by hash):
- `protected` — exists on 2+ backup locations
- `backed-up` — exists on 1 backup location
- `local-only` — only on non-backup storage (AT RISK)
- `orphaned` — only on backup, deleted from local

### Smart Traversal
Instead of manually configuring every path, fili starts from `/` and uses built-in knowledge:

**Known path types (preconfigured):**
```
# System (skip or index as read-only)
/usr, /bin, /lib, /opt        → system:packages
/etc                          → system:config
/var                          → system:variable (mostly skip)
/boot, /proc, /sys, /dev      → skip entirely

# User directories (XDG + common patterns)
~/Documents                   → user:documents (important!)
~/Pictures, ~/Photos          → user:media
~/Videos, ~/Movies            → user:media  
~/Music                       → user:media
~/Downloads                   → user:downloads (ephemeral)
~/Desktop                     → user:desktop
~/.config                     → user:config
~/.local/share                → user:data
~/.cache                      → skip (ephemeral)

# Projects & code
~/Projects, ~/src, ~/code     → user:projects (detect git repos)
~/go, ~/.cargo, ~/.rustup     → user:toolchains (ephemeral)

# Games
~/.steam, ~/.local/share/Steam → games:steam
~/Games                        → games:library
~/.wine                        → games:wine

# Mounts (prompt for classification)
/mnt/*, /media/*              → unknown:mount (ask user)
/run/media/*                  → unknown:removable
```

**Traversal behavior:**
1. Start from `/` (or `~` for user-focused scan)
2. Match paths against known patterns
3. Apply appropriate behavior (index, skip, treat as collection)
4. **Stop and prompt** when hitting unknown paths (especially mounts)
5. Remember user classifications for future scans

**Unknown path handling:**
```
$ fili scan /

Scanning /home/user... ✓
Scanning /mnt/Backup... 

⚠ Unknown mount point: /mnt/Backup (7.3TB ext4)
  What is this?
  [b] Backup drive
  [d] Data/media storage  
  [g] Games library
  [t] Temporary/scratch
  [s] Skip (don't index)
  [?] Explore first
  > b

Classifying /mnt/Backup as backup storage...
```

## Commands

```bash
# Smart scanning (recommended)
fili scan                           # full system scan with prompts
fili scan ~                         # user directory only
fili scan --non-interactive         # skip unknowns, don't prompt

# Manual management (when needed)
fili classify /mnt/Backup --as backup
fili classify ~/Dropbox --as cloud:dropbox
fili ignore /path/to/junk           # permanently skip this path

# Review configuration
fili paths                          # show all known path classifications
fili paths --unknown                # show paths that need classification

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

-- Collections (hierarchical groupings)
CREATE TABLE collections (
    id INTEGER PRIMARY KEY,
    parent_id INTEGER REFERENCES collections(id),  -- NULL = top-level
    location_id INTEGER REFERENCES locations(id),
    path TEXT NOT NULL,
    name TEXT,                       -- "Half-Life", "lutris", "Japan Vacation 2024"
    collection_type TEXT,            -- 'git', 'game', 'album', 'artist', 'projects', etc.
    identifier TEXT,                 -- git remote URL, Steam app ID, MusicBrainz ID, etc.
    total_size INTEGER,              -- aggregate including children
    file_count INTEGER,              -- files directly in this collection
    child_count INTEGER,             -- number of child collections
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

-- Path classification rules (built-in + user-defined)
CREATE TABLE path_rules (
    id INTEGER PRIMARY KEY,
    pattern TEXT NOT NULL,           -- glob or prefix: '/mnt/*', '~/Downloads'
    path_type TEXT,                  -- 'system', 'user', 'games', 'backup', etc.
    behavior TEXT,                   -- 'index', 'skip', 'collection', 'prompt'
    is_builtin BOOLEAN DEFAULT FALSE,
    priority INTEGER DEFAULT 0       -- higher = matched first
);

CREATE INDEX idx_files_hash ON files(hash);
CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_contents_size ON contents(size);
CREATE INDEX idx_path_rules_pattern ON path_rules(pattern);
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
