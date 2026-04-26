//! Settings backup driven by the bestiary catalog.
//!
//! For a given app (or all apps), gather the catalog-declared paths that
//! actually exist on disk and pack them into a `.tar.zst` archive named
//! `<out>/<bestiary-id>/<YYYY-MM-DD>-<hostname>.tar.zst`. The date is
//! derived from the most recent file mtime in the source set (with a
//! 1980-01-01 floor to filter epoch-zero artefacts), so dates reflect
//! the data, not the moment tar happened to run.
//!
//! Cache and state are opt-in; everything else from `config:` and
//! `data:` is included. `backup_exclude` patterns are forwarded to
//! `tar --exclude` so per-app skip lists (e.g. `Cache/*`, `GPUCache/*`)
//! still apply.
//!
//! Each archive carries a `.bestiary-manifest.json` at its tar root with
//! the app id, included flavors/paths, hostname, source bestiary
//! version, and which optional kinds were toggled on. Restore tooling
//! (grimoire) reads it without untarring.

use anyhow::{bail, Context, Result};
use bestiary::{Catalog, Kind};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Utc};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// 1980-01-01 in unix seconds. Anything with `mtime` below this is
/// presumed bogus (epoch-zero CMOS reset, broken archive). Files from
/// genuine vintage software (Atari 800, etc.) are typically >= 1980.
const PLAUSIBLE_MTIME_FLOOR: i64 = 315_532_800;

#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Output root. The archive lands at `<out>/<app-id>/<date>-<host>.tar.zst`.
    pub out: PathBuf,
    /// Include cache paths in the archive. Default false; useful for
    /// browsers where cache holds session credentials.
    pub include_cache: bool,
    /// Include state paths (logs, sockets). Default false.
    pub include_state: bool,
    /// Skip writing if a same-named archive already exists. Default
    /// true — re-running a backup is a no-op unless the date moved.
    pub skip_existing: bool,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            out: PathBuf::from("."),
            include_cache: false,
            include_state: false,
            skip_existing: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct Manifest {
    bestiary_app: String,
    display_name: Option<String>,
    category: Option<String>,
    hostname: String,
    archive_date: String,
    created_at: String,
    flavors: Vec<FlavorManifest>,
    include_cache: bool,
    include_state: bool,
    backup_exclude: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FlavorManifest {
    flavor: String,
    flatpak_id: Option<String>,
    snap_name: Option<String>,
    paths: Vec<PathEntry>,
}

#[derive(Debug, Serialize)]
struct PathEntry {
    kind: String,
    /// Path as declared in bestiary (with `~/` if applicable).
    declared: String,
    /// Path inside the archive, relative to `$HOME`.
    archive_path: String,
}

/// Back up one app. Returns the archive path written, or `None` when
/// the app has no on-disk presence (nothing to back up).
pub fn backup_app(
    catalog: &Catalog,
    app_id: &str,
    opts: &BackupOptions,
) -> Result<Option<PathBuf>> {
    let entry = catalog
        .get(app_id)
        .with_context(|| format!("app {app_id:?} not found in bestiary catalog"))?;
    let creature = &entry.creature;

    // Collect (kind, declared, expanded) for every path that exists on
    // disk across every flavor this app has, filtered by what the
    // caller opted into.
    let home = home_dir()?;
    let mut flavor_blocks: Vec<FlavorManifest> = Vec::new();
    let mut tar_paths: BTreeSet<PathBuf> = BTreeSet::new(); // home-relative

    for (flavor, dwelling) in &creature.dwellings {
        let mut paths: Vec<PathEntry> = Vec::new();
        for (kind, raw) in dwelling.paths() {
            if !kind_included(kind, opts) {
                continue;
            }
            // Skip wildcard paths — tar can't pack a glob, and they're
            // typically rotation matchers that aren't worth archiving.
            if raw.contains('*') {
                continue;
            }
            let expanded = expand_tilde(raw, &home);
            if !expanded.exists() {
                continue;
            }
            let archive_rel = match expanded.strip_prefix(&home) {
                Ok(r) => r.to_path_buf(),
                Err(_) => {
                    // Path outside $HOME — not something tar -C $HOME
                    // can hold cleanly. Skip with a warning.
                    eprintln!(
                        "warn: {app_id} {kind:?} path {raw} resolves outside $HOME, skipping"
                    );
                    continue;
                }
            };
            tar_paths.insert(archive_rel.clone());
            paths.push(PathEntry {
                kind: kind.as_str().to_string(),
                declared: raw.to_string(),
                archive_path: archive_rel.to_string_lossy().into_owned(),
            });
        }
        if paths.is_empty() {
            continue;
        }
        flavor_blocks.push(FlavorManifest {
            flavor: flavor.as_str().to_string(),
            flatpak_id: dwelling.flatpak_id.clone(),
            snap_name: dwelling.snap_name.clone(),
            paths,
        });
    }

    if tar_paths.is_empty() {
        return Ok(None);
    }

    let archive_date = pick_archive_date(tar_paths.iter().map(|p| home.join(p)));
    let host = short_hostname()?;
    let app_dir = opts.out.join(app_id);
    std::fs::create_dir_all(&app_dir).with_context(|| format!("mkdir {}", app_dir.display()))?;
    let archive = app_dir.join(format!("{archive_date}-{host}.tar.zst"));

    if opts.skip_existing && archive.exists() {
        return Ok(Some(archive));
    }

    let manifest = Manifest {
        bestiary_app: creature.name.clone(),
        display_name: creature.display_name.clone(),
        category: creature.category.clone(),
        hostname: host.clone(),
        archive_date: archive_date.to_string(),
        created_at: Utc::now().to_rfc3339(),
        flavors: flavor_blocks,
        include_cache: opts.include_cache,
        include_state: opts.include_state,
        backup_exclude: creature.backup_exclude.clone(),
    };

    write_archive(
        &archive,
        &home,
        &tar_paths,
        &manifest,
        &creature.backup_exclude,
    )?;
    Ok(Some(archive))
}

/// Settings for `backup_all`. `out_override` (if set) wins for every
/// app — useful for one-off CLI runs to a specific dir. When `None`,
/// each app is routed through `cfg.resolve_backup_dir(None,
/// Some(category))` so apps in different categories can land in
/// different dirs.
#[derive(Debug, Clone)]
pub struct BackupAllOptions {
    pub out_override: Option<PathBuf>,
    pub include_cache: bool,
    pub include_state: bool,
    pub skip_existing: bool,
}

/// Back up every app in the catalog that has on-disk presence. Each
/// app's destination is resolved via `cfg` (with category routing) or
/// the per-call `out_override`.
pub fn backup_all(
    catalog: &Catalog,
    cfg: &crate::config::FiliConfig,
    opts: &BackupAllOptions,
) -> Result<BackupSummary> {
    let mut summary = BackupSummary::default();
    for (name, entry) in catalog.iter() {
        let category = entry.creature.category.as_deref();
        let out = match cfg.resolve_backup_dir(opts.out_override.clone(), category) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("  {name}: {e:#}");
                summary.failed += 1;
                continue;
            }
        };
        let app_opts = BackupOptions {
            out,
            include_cache: opts.include_cache,
            include_state: opts.include_state,
            skip_existing: opts.skip_existing,
        };
        match backup_app(catalog, name, &app_opts) {
            Ok(Some(path)) => {
                if opts.skip_existing && path.exists() && was_skipped(&path) {
                    summary.skipped += 1;
                } else {
                    summary.written += 1;
                }
                println!("  {name} → {}", path.display());
            }
            Ok(None) => summary.empty += 1,
            Err(e) => {
                eprintln!("  {name}: {e:#}");
                summary.failed += 1;
            }
        }
    }
    Ok(summary)
}

#[derive(Default, Debug)]
pub struct BackupSummary {
    pub written: u32,
    pub skipped: u32,
    pub empty: u32,
    pub failed: u32,
}

/// One bestiary app reported by `candidates()`: a snapshot-able app
/// that has on-disk presence on this machine, plus the most recent
/// archive in this app's backup dir if any.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppCandidate {
    pub id: String,
    pub display_name: Option<String>,
    pub category: Option<String>,
    /// `kind` strings (`config`, `data`, ...) for paths actually
    /// present on disk. Lets the UI show "config + data" at a glance.
    pub kinds: Vec<String>,
    /// Resolved destination directory for this app (category-routed).
    /// `None` if no destination is configured at all.
    pub backup_dir: Option<String>,
    /// Most-recent archive's filename date, or `None` if no archive
    /// exists for this app yet.
    pub last_backup_date: Option<String>,
    /// Most-recent archive's full path, or `None`.
    pub last_backup_path: Option<String>,
}

/// Enumerate apps the catalog covers that actually have data on this
/// machine. Each candidate carries its category-resolved backup
/// destination (per `cfg`) and the latest archive in that destination,
/// if any.
pub fn candidates(catalog: &Catalog, cfg: &crate::config::FiliConfig) -> Result<Vec<AppCandidate>> {
    let home = home_dir()?;
    let mut out = Vec::new();
    for (name, entry) in catalog.iter() {
        let mut kinds: Vec<String> = Vec::new();
        for dwelling in entry.creature.dwellings.values() {
            for (kind, raw) in dwelling.paths() {
                if raw.contains('*') {
                    continue;
                }
                let expanded = expand_tilde(raw, &home);
                if expanded.exists() {
                    let s = kind.as_str().to_string();
                    if !kinds.contains(&s) {
                        kinds.push(s);
                    }
                }
            }
        }
        if kinds.is_empty() {
            continue;
        }
        let category = entry.creature.category.as_deref();
        let resolved_dir = cfg.resolve_backup_dir(None, category).ok();
        let (last_backup_date, last_backup_path) = resolved_dir
            .as_deref()
            .map(|d| latest_archive(d, name))
            .unwrap_or((None, None));
        out.push(AppCandidate {
            id: name.clone(),
            display_name: entry.creature.display_name.clone(),
            category: entry.creature.category.clone(),
            kinds,
            backup_dir: resolved_dir.map(|p| p.display().to_string()),
            last_backup_date,
            last_backup_path,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Look at `<backup_dir>/<app_id>/*.tar.zst` and return the date+path of
/// the latest one (parsed from the filename `YYYY-MM-DD-host.tar.zst`).
fn latest_archive(backup_dir: &Path, app_id: &str) -> (Option<String>, Option<String>) {
    let dir = backup_dir.join(app_id);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return (None, None);
    };
    let mut best: Option<(String, PathBuf)> = None;
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".tar.zst") {
            continue;
        }
        // Filename convention: `YYYY-MM-DD-<host>.tar.zst`. Take the
        // leading 10 chars as the date string — it's lexicographically
        // sortable, so plain string compare gives chronological order.
        let date: String = name.chars().take(10).collect();
        if date.len() != 10 || date.as_bytes().get(4) != Some(&b'-') {
            continue;
        }
        match &best {
            None => best = Some((date, ent.path())),
            Some((cur, _)) if date.as_str() > cur.as_str() => best = Some((date, ent.path())),
            _ => {}
        }
    }
    match best {
        Some((d, p)) => (Some(d), Some(p.display().to_string())),
        None => (None, None),
    }
}

fn was_skipped(_path: &Path) -> bool {
    // Placeholder — backup_app already gates on skip_existing. This is
    // a hook for richer reporting later (e.g. distinguish "wrote new"
    // from "left existing alone").
    false
}

fn kind_included(kind: Kind, opts: &BackupOptions) -> bool {
    match kind {
        Kind::Config | Kind::Data => true,
        Kind::Cache => opts.include_cache,
        Kind::State => opts.include_state,
    }
}

fn expand_tilde(raw: &str, home: &Path) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else if raw == "~" {
        home.to_path_buf()
    } else {
        PathBuf::from(raw)
    }
}

/// Pick the archive date: the latest mtime in the source set that's
/// above the bogus-mtime floor. Falls back to today if every mtime is
/// below the floor (warning is emitted by the caller on demand).
fn pick_archive_date<I: IntoIterator<Item = PathBuf>>(sources: I) -> NaiveDate {
    let floor = Utc.timestamp_opt(PLAUSIBLE_MTIME_FLOOR, 0).unwrap();
    let mut latest: Option<DateTime<Local>> = None;
    for path in sources {
        for_each_mtime(&path, &mut |st| {
            let dt: DateTime<Utc> = st.into();
            if dt < floor {
                return;
            }
            let local: DateTime<Local> = dt.into();
            if latest.is_none_or(|cur| local > cur) {
                latest = Some(local);
            }
        });
    }
    let chosen = latest.unwrap_or_else(Local::now);
    NaiveDate::from_ymd_opt(chosen.year(), chosen.month(), chosen.day())
        .unwrap_or_else(|| Local::now().date_naive())
}

/// Walk `path` recursively, calling `f` for each entry's mtime.
fn for_each_mtime(path: &Path, f: &mut dyn FnMut(SystemTime)) {
    let Ok(meta) = path.symlink_metadata() else {
        return;
    };
    if let Ok(t) = meta.modified() {
        f(t);
    }
    if !meta.is_dir() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(path) else {
        return;
    };
    for ent in rd.flatten() {
        for_each_mtime(&ent.path(), f);
    }
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("$HOME is not set"))
}

fn short_hostname() -> Result<String> {
    let h = hostname::get().context("reading hostname")?;
    let s = h.to_string_lossy().into_owned();
    let short = s.split('.').next().unwrap_or(&s).to_lowercase();
    if short.is_empty() {
        bail!("hostname is empty");
    }
    Ok(short)
}

/// Drive the actual archiving: write the manifest to a tempfile, then
/// invoke `tar --zstd` once with the tarred paths plus the manifest at
/// the archive's logical root.
fn write_archive(
    archive: &Path,
    home: &Path,
    tar_paths: &BTreeSet<PathBuf>,
    manifest: &Manifest,
    backup_exclude: &[String],
) -> Result<()> {
    // Write manifest to a temp file we'll splice in via a second `-C`
    // segment in the tar invocation.
    let tmp = tempfile::tempdir().context("mktemp dir for manifest")?;
    let manifest_path = tmp.path().join(".bestiary-manifest.json");
    let json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(&manifest_path, json).context("writing manifest")?;

    let mut cmd = Command::new("tar");
    cmd.arg("--zstd").arg("--create").arg("--file").arg(archive);
    for excl in backup_exclude {
        cmd.arg(format!("--exclude={excl}"));
    }
    cmd.arg("-C").arg(home);
    for p in tar_paths {
        cmd.arg(p);
    }
    // Splice the manifest into the archive at its root via a second -C.
    cmd.arg("-C").arg(tmp.path()).arg(".bestiary-manifest.json");

    let status = cmd.status().context("spawning tar")?;
    if !status.success() {
        bail!("tar exited with {status}");
    }
    Ok(())
}
