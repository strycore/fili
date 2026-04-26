# fili — personal file intelligence

**fili** indexes everything on your disk, classifies it with a pluggable rule
engine, and gives you a local web UI to browse and query your files across all
your drives and machines. Think *Dolphin + Spotlight + a content-aware
classifier*, running on your own hardware against your own rules.

Written in Rust. Stores its index in SQLite under `$XDG_DATA_HOME/fili/fili.db`.
Ships a CLI (`fili scan`, `fili serve`, …) and an embedded web UI (no external
server, no cloud).

> Status: alpha. The data model and rule format are stabilizing; API/UI may
> still change between commits.

---

## What it does

- **Classifies your filesystem** by content and path patterns — your Music
  folder is tagged `audio · library`, a Steam game install as `game · title=Portal`,
  a Wine prefix as `application · runtime=wine · kind=prefix`, a HuggingFace
  model folder as `ai-model · format=safetensors`, and so on.
- **Explicit over magic**: if a folder doesn't match any rule, it's recorded as
  *unknown* and the scanner stops — you classify it in the UI (or add a rule)
  rather than have fili guess.
- **Privacy-aware**: entries carry a `public` / `personal` / `confidential`
  level, so you can see at a glance which files need extra care before you
  share a drive, ship a backup, wipe a machine, etc.
- **Cross-drive reconciliation**: drives are identified by filesystem UUID, so
  the same external disk plugged into two machines isn't counted twice.
- **Web UI** over `http://127.0.0.1:7777`: sidebar with home shortcuts + mount
  points + recent scans + live stats, table-based browse/search views with
  sortable columns, per-folder "Scan" and "Open in file manager" actions.

---

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/strycore/fili/master/install.sh | bash
```

The script downloads the latest release tarball for Linux x86_64 and drops the
binary into `~/.local/bin`. Pin a version with `VERSION=v0.1.0`, change the
target dir with `FILI_INSTALL_DIR=$HOME/bin`.

### Packages

`.deb` and `.rpm` packages are also attached to each GitHub release.

### From source

Requires Rust 1.95 (pinned via `rust-toolchain.toml`).

```sh
git clone https://github.com/strycore/fili
cd fili
cargo build --release
# Binary lives at target/release/fili
```

```sh
# Debian / Ubuntu
sudo dpkg -i fili_<version>_amd64.deb

# Fedora / RHEL / openSUSE
sudo rpm -i fili-<version>-1.x86_64.rpm
```

---

## Quick start

```sh
fili init                    # create the database
fili scan ~                  # index your home folder
fili serve                   # open http://127.0.0.1:7777
```

From the UI you can browse your filesystem overlaid with fili's classifications,
filter by type or tag, view drives, triage unknowns, and kick off additional
scans without touching the CLI again.

---

## Core concepts

### Entries: collections vs items

Every indexed folder is one of two things:

- **Collection** — holds other entries. `~/Music`, `~/Music/ABBA`, `/bin`.
- **Item** — atomic unit, no children in fili's model. A Wine prefix, a game
  install, a HuggingFace model folder, `node_modules`, a single `.flac` track.

Collections recurse; items stop.

### Base types

A small closed vocabulary — `image`, `audio`, `video`, `document`, `book`,
`code`, `game`, `gamedata`, `emulator`, `application`, `ai-model`, `vm`,
`config`, `cache`, `archive`, `dependencies`, `build-artifact`, `home`,
`system`, `mount`, `inbox`, `devices`, `boot`, `swap`, `services`, `procfs`,
`sysfs`, `generic`. Applies equally to collections and items.

### Tags

Free-form `key=value` metadata on entries — `artist=ABBA`, `app=steam`,
`title=Portal 2`, `lang=rust`, `sync=nextcloud`, `kind=album`, `library`.
Clickable in the UI: click a tag and the search view filters to everything
else carrying it.

### Privacy

Three levels:

- `public` — fine to share
- `personal` — yours but not sensitive
- `confidential` — keys, credentials, health data, finances

Set explicitly (`fili privacy <path> confidential`) or inferred from rules
(`~/.ssh`, `~/.gnupg`, `~/.password-store` are auto-confidential).

---

## Rules

Classification is driven by `rules.json` (embedded at build time — override
with `~/.config/fili/rules.local.json`). Rules live in a `match` array; first
match wins.

```json
{
  "path": "<home>/.steam/steam/steamapps/common/{title}",
  "base": "game",
  "tags": ["store=steam", "title={title}"],
  "item": true
}
```

Supported predicates: `path` (with `{captures}`, `**/` prefix for any-depth,
`<home>/` prefix for scope-relative), `contains` (literal filenames or
`*.ext` globs — all must be present), `majority_ext` (more than half of direct
files match one of the listed extensions), `where` (capture-to-group
constraints).

See `rules.json` for the shipping ruleset.

---

## CLI

```
fili init                     Create the database.
fili scan [path]              Walk a directory tree, classify, index.
                              --files        also index direct files by extension
                              --max-depth N  cap recursion depth
fili reclassify               Re-run rule matching against unknowns without walking disk.
fili unknowns                 List folders the scanner couldn't classify.
fili status                   Overview: counts, devices, unprotected.
fili find <query>             Search.
fili paths [--unknown]        Dump all indexed paths.
fili tag <path> -t key=value  Add a tag to an entry.
fili unprotected              List entries not under a backup location.
fili duplicates               Identify duplicated content.
fili export <output>          Dump the index as JSON.
fili stats                    Raw stats.
fili privacy <path> <level>   Mark privacy.
                              --marker       also drop a .fili-<level> file
fili serve [--addr X --port N]  Launch the web UI + REST API.
```

---

## Tech stack

- **Rust** — `rusqlite` (bundled SQLite), `axum` + `tokio` for the HTTP layer,
  `rust-embed` for bundling the UI assets into the binary, `walkdir` +
  `ignore` for the scanner, `clap` for the CLI.
- **Single static-ish binary**: everything (UI templates, CSS, JS, default
  rules, migrations) is `include_str!`'d at build time. Deploy is `scp` one
  file.
- **Embedded web UI**: plain HTML + CSS + vanilla JS, no bundler, no npm
  dependency. Reads from the REST API served on the same port.

---

## Roadmap-ish

Things that work today:
- Deep classification of common home layouts, Steam libraries, Wine/Proton
  prefixes, Ardour/Ableton/Reaper sessions, LLM / Stable Diffusion model
  folders, `.local/share` apps, VM images.
- Web UI with sortable tables, sidebar shortcuts, per-view status bar,
  clickable tags + base-type pills.
- Scan-time symlink-loop and repeated-segment protection.

Things not yet real:
- `fili unprotected` and `fili duplicates` are stubs. No file-level content
  hashing or cross-drive reconciliation comparator yet.
- Multi-device sync over LAN (planned — fili is meant to let you see your
  Surface Pro from your desktop, for example).
- Template auto-inference from observed folder structure.

---

## License

[GPL-3.0-or-later](LICENSE).
