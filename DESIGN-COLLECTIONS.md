# Collections design — v3 (decided)

Status: **decisions locked, implementation in progress**. Prior drafts (v1, v2) recoverable from git.

---

## Core model

**Every indexed entry is either a collection or an item.** Binary distinction, stored as `is_item: bool` on the entry row.

- **Collection** — holds child rows (other collections and/or items). Example: `/home`, `~/Music`, `~/Music/ABBA`, `/bin`.
- **Item** — atomic, no child rows in fili's model. May be a directory (wine prefix, game install, `node_modules`) or a file (`.sfc` ROM, `.flac` track).

`is_item` is about **data model**, not about scanner behavior.

---

## Scanner hint: `stop` (orthogonal to `is_item`)

`stop: true` in a rule → scanner does not auto-descend. Independent of `is_item`:

| Example                       | `is_item` | `stop`  | Meaning                                        |
|-------------------------------|-----------|---------|------------------------------------------------|
| `~/Music`                     | false     | false   | Collection, walk freely.                       |
| `~/Music/ABBA/Gold` (album)   | false     | true    | Collection; files become items when indexed.   |
| `/bin`                        | false     | true    | Collection of binaries; scanner doesn't auto-walk to avoid 1000+ rows. Explicit `fili scan /bin --deep` could. |
| Wine game install             | true      | true    | Atomic — we'll never have sub-rows.            |
| `node_modules`                | true      | true    | Atomic — regenerable blob.                     |
| `.sfc` ROM file               | true      | (n/a)   | File item (once file indexing exists).         |

Rules gain an explicit `item: true` flag. `stop: true` stays as the recursion gate. Default when neither is set: `collection` + recurse.

---

## `base_type` — content type

Small closed enum. Applies equally to collections and items.

**Additions for v3:**
- `dependencies` — fetched third-party packages: `node_modules`, `.venv`, `vendor`, `.cargo` (registry).
- `build-artifact` — locally compiled/assembled output: `target/debug`, `target/release`, `dist`, `build`, `__pycache__`, `.gradle`.
- `inbox` — unsorted content to triage: `~/Downloads`, `~/Desktop`.

**Removals for v3:**
- `binaries`, `libraries` → fold into `application` with `kind=binaries` / `kind=libraries` tags.

**Kept:** image, audio, video, game, gamedata, emulator, home, application, code, document, config, archive, cache, system, boot, mount, devices, procfs, sysfs, swap, services, generic.

Language goes in the `lang=` tag on dependencies / build-artifact / code rows.

---

## Tags

Unchanged. Flexible key=value. The `library` tag is **dropped** — v2 collapsed it into base_type semantics.

---

## Filesystem-root detection (content-based)

A rootfs can appear anywhere (`/run/media/.../containers/etherpad/`, a rescued backup, a mounted VM image). Detect by content signature, not path:

```json
{"contains": ["bin", "etc", "usr"],
 "base": "system", "tags": ["kind=rootfs"], "stop": false}
```

Match → classify as `system · rootfs`, continue walking so the subtree's `etc/`, `bin/`, `usr/` get classified.

**System-path rules need glob fallbacks** so they match inside detected rootfs trees, not just at `/`. Pattern:

```json
{"path": "**/bin",  "base": "application", "tags": ["kind=binaries"], "stop": true},
{"path": "**/etc",  "base": "config",      "stop": true},
{"path": "**/usr",  "base": "system",      "stop": false},
...
```

The absolute-path variants (`/bin`, `/etc`, `/usr`) remain for the local-filesystem case and match first by rule order.

---

## Schema — Option B (unified `entries` table)

Rename `collections` → `entries`. Absorb `files` into it. Single table for everything fili has indexed:

```sql
CREATE TABLE entries (
    id            INTEGER PRIMARY KEY,
    parent_id     INTEGER REFERENCES entries(id),
    location_id   INTEGER REFERENCES locations(id),
    drive_id      INTEGER REFERENCES drives(id),       -- stage 2 of drives, can be NULL for now
    path          TEXT NOT NULL,
    name          TEXT,
    base_type     TEXT NOT NULL,
    is_item       BOOLEAN NOT NULL DEFAULT 0,
    is_dir        BOOLEAN NOT NULL DEFAULT 1,
    privacy       TEXT DEFAULT 'public',
    identifier    TEXT,
    total_size    INTEGER DEFAULT 0,
    file_count    INTEGER DEFAULT 0,
    child_count   INTEGER DEFAULT 0,
    manifest_hash TEXT,
    indexed_at    INTEGER,
    UNIQUE(location_id, path)
);

-- Tag table stays, renamed for consistency.
CREATE TABLE entry_tags (
    entry_id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    key      TEXT NOT NULL,
    value    TEXT,
    PRIMARY KEY (entry_id, key, value)
);
```

Drop `files` table (was never populated). DB-layer methods rename: `upsert_collection` → `upsert_entry`, `list_collections` → `list_entries`, etc.

Public API URL path stays `/api/collections` for the UI's compatibility.

---

## API

- `GET /api/collections?is_item=false` — collections only
- `GET /api/collections?is_item=true&type=game` — every game item
- `GET /api/collections?is_item=true&tag=platform=snes` — items with that tag
- Existing filters (type, privacy, tag, q, parent, limit, offset) unchanged.

---

## UI

- **Kind pill** stays `<Type> · <scope>`.
- **Collection vs item** shown by presence of the link affordance — items don't navigate (no children in fili), collections do.
- **Status dot** CSS class renamed `kind-collection` → `kind-indexed`. Same colors.

---

## What doesn't change

- Rule file format, with one addition: optional `"item": true` flag per rule.
- `collection_tags` concept (table renamed to `entry_tags`).
- Drive detection / enumeration.
- Browse view's filesystem-first overlay.

---

## Implementation phases (discrete commits)

1. **Schema rename + unification.** Create `entries` + `entry_tags`, migrate/recreate from fresh scan. Drop `collections`, `collection_tags`, `files`. DB methods renamed. Rust types updated (Collection → Entry). Server + UI references updated but keep the public URL path `/api/collections`.
2. **Add `is_item` flag.** Column exists from phase 1. Scanner writes it based on rule's new `item: true` flag. Rules file gets `item: true` added to genuine item rules (wine prefixes, game installs, node_modules, target, etc.). `/bin`, `/lib`, `/etc` stay `stop: true` without `item`.
3. **New base types.** Add `dependencies`, `build-artifact`, `inbox` to the BaseType enum. Migrate rules.json:
   - `~/Downloads`, `~/Desktop` → `inbox`.
   - `**/node_modules`, `**/.venv`, `**/venv`, `**/vendor` → `dependencies` (with `lang=`).
   - `**/target/debug`, `**/target/release`, `**/__pycache__`, `**/.gradle`, `**/dist`, `**/build` → `build-artifact`.
4. **Merge `binaries` + `libraries` into `application`.** Update rules and enum. Drop the two old types.
5. **Rootfs detection + glob fallbacks for system paths.** `contains: [bin, etc, usr]` content rule; `**/bin`, `**/etc`, `**/usr`, etc. glob variants after the absolute ones.
6. **UI polish.** Rename `kind-collection` CSS class, surface is_item visually (or confirm existing link affordance is enough).

Each phase is a standalone commit on master.
