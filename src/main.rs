use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

mod backup;
mod config;
mod db;
mod drives;
mod models;
mod rules;
mod scanner;
mod server;

use db::Database;

#[derive(Parser)]
#[command(name = "fili")]
#[command(about = "Personal file intelligence system", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Combine with `--help` to dump help for every subcommand. Intercepted
    /// before clap's help renderer, so it has no effect on its own.
    #[arg(long)]
    all: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the database
    Init,

    /// Scan filesystem for files and collections
    Scan {
        /// Path to scan (defaults to filesystem root)
        #[arg(default_value = "/")]
        path: String,

        /// Don't prompt for unknown paths
        #[arg(long)]
        non_interactive: bool,

        /// Index direct files inside every classified collection
        /// (by extension — see `extensions` in rules.json).
        #[arg(long)]
        files: bool,

        /// Cap recursion depth relative to the scan root.
        #[arg(long)]
        max_depth: Option<u32>,
    },

    /// Re-run rule matching against stored unknowns (no filesystem walk).
    Reclassify,

    /// List directories the scanner couldn't classify.
    Unknowns,

    /// Show status overview
    Status,

    /// Search for files or collections
    Find {
        /// Search query
        query: String,

        /// Search in collection names only
        #[arg(long)]
        collections: bool,
    },

    /// List all known paths and their classifications
    Paths {
        /// Show only unclassified paths
        #[arg(long)]
        unknown: bool,
    },

    /// Add a tag to an indexed collection (e.g. `fili tag ~/Games/FalloutNewVegas -t platform=windows`)
    Tag {
        /// Path of the collection to tag
        path: String,

        /// Tag in `key=value` or `key` form
        #[arg(long, short = 't')]
        tag: String,
    },

    /// Show files that aren't backed up
    Unprotected,

    /// Show duplicate files/collections
    Duplicates {
        /// Only show duplicates on the same device
        #[arg(long)]
        same_device: bool,
    },

    /// Export index to JSON
    Export {
        /// Output file
        #[arg(default_value = "fili-export.json")]
        output: String,
    },

    /// Show statistics
    Stats,

    /// Start a local web UI + REST API for browsing the index
    Serve {
        /// Address to bind. Use 0.0.0.0 (or pass --lan) to expose the UI
        /// on every interface so other machines on your network can reach it.
        #[arg(long, default_value = "127.0.0.1")]
        addr: String,

        /// Port to bind
        #[arg(long, short = 'p', default_value_t = 7777)]
        port: u16,

        /// Shortcut for `--addr 0.0.0.0`: makes the UI reachable from
        /// other devices on your local network.
        #[arg(long)]
        lan: bool,
    },

    /// Back up app settings using bestiary as the catalog of paths.
    /// Produces `<out>/<app-id>/<YYYY-MM-DD>-<host>.tar.zst`. The date
    /// is the latest mtime in the source set (with a 1980-01-01 floor
    /// against epoch-zero artifacts), so dates reflect the data, not
    /// when tar happened to run.
    Backup {
        /// Bestiary app id to back up. Use `--all` to back up everything
        /// the catalog knows about that has on-disk presence.
        app: Option<String>,

        /// Back up every catalogued app present on this machine.
        #[arg(long)]
        all: bool,

        /// Include cache paths (e.g. browser session cookies). Default
        /// behavior is to skip cache as regenerable; opt in when you
        /// want to preserve sign-ins.
        #[arg(long)]
        include_cache: bool,

        /// Include state paths (logs, sockets). Usually noise.
        #[arg(long)]
        include_state: bool,

        /// Output directory. Each app gets a subfolder. Defaults to
        /// `backup_dir` from `~/.config/fili/config.toml`.
        #[arg(long, short = 'o')]
        out: Option<std::path::PathBuf>,

        /// Re-create archives that already exist (default is to leave
        /// existing files alone since the date encodes the data state).
        #[arg(long)]
        force: bool,
    },

    /// Set privacy level for a path
    Privacy {
        /// Path to update
        path: String,

        /// Privacy level: public, personal, or confidential
        level: String,

        /// Create marker file instead of just updating DB
        #[arg(long)]
        marker: bool,
    },
}

/// Render the top-level help, then the full help for every subcommand.
/// Subcommands whose only arg is the auto-injected `--help` collapse to a
/// single header line so the dump doesn't drown in identical empty
/// "Options" blocks.
fn print_full_help() {
    let mut cmd = Cli::command();
    let _ = cmd.print_help();
    println!();
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in names {
        if let Some(sub) = cmd.find_subcommand_mut(&name) {
            let has_real_args = sub.get_arguments().any(|a| {
                let id = a.get_id().as_str();
                id != "help" && id != "version"
            });
            if has_real_args {
                println!("\n──── fili {} ────\n", name);
                let _ = sub.print_help();
                println!();
            } else {
                let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
                if about.is_empty() {
                    println!("\n──── fili {} ────", name);
                } else {
                    println!("\n──── fili {} ────  {}", name, about);
                }
            }
        }
    }
}

fn main() -> Result<()> {
    // `--help --all` (and its `-h --all` variant) prints the top-level help
    // followed by the full help for every subcommand. Has to run before
    // `Cli::parse()` because clap consumes `--help` and exits.
    let argv: Vec<String> = std::env::args().collect();
    let wants_help = argv.iter().any(|a| a == "--help" || a == "-h");
    let wants_all = argv.iter().any(|a| a == "--all");
    if wants_help && wants_all {
        print_full_help();
        return Ok(());
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let db = Database::init()?;
            println!("✓ Database initialized at {}", db.path().display());
            println!("\nRun 'fili scan' to index your filesystem.");
        }

        Commands::Scan {
            path,
            non_interactive,
            files,
            max_depth,
        } => {
            let mut db = Database::open()?;
            let path = expand_path(&path);
            let opts = scanner::ScanOptions {
                max_depth,
                index_files: files,
            };
            scanner::scan_with(&mut db, &path, !non_interactive, opts)?;
        }

        Commands::Reclassify => {
            let mut db = Database::open()?;
            let engine = rules::RulesEngine::load();
            let promoted = db.with_transaction(|db| scanner::reclassify(db, &engine))?;
            println!("✓ Reclassified {} paths", promoted);
        }

        Commands::Unknowns => {
            let db = Database::open()?;
            list_unknowns(&db)?;
        }

        Commands::Status => {
            let db = Database::open()?;
            show_status(&db)?;
        }

        Commands::Find { query, collections } => {
            let db = Database::open()?;
            if collections {
                find_collections(&db, &query)?;
            } else {
                find_files(&db, &query)?;
            }
        }

        Commands::Paths { unknown } => {
            let db = Database::open()?;
            list_paths(&db, unknown)?;
        }

        Commands::Tag { path, tag } => {
            let db = Database::open()?;
            let path = expand_path(&path);
            let collection = db
                .find_entry_by_path(&path)?
                .ok_or_else(|| anyhow::anyhow!("No indexed collection at {}", path.display()))?;
            let parsed = models::Tag::parse(&tag);
            db.add_tag(collection.id, &parsed)?;
            println!("✓ Tagged {} with {}", path.display(), parsed.render());
        }

        Commands::Unprotected => {
            let db = Database::open()?;
            show_unprotected(&db)?;
        }

        Commands::Duplicates { same_device } => {
            let db = Database::open()?;
            show_duplicates(&db, same_device)?;
        }

        Commands::Export { output } => {
            let db = Database::open()?;
            export_index(&db, &output)?;
            println!("✓ Exported index to {}", output);
        }

        Commands::Stats => {
            let db = Database::open()?;
            show_stats(&db)?;
        }

        Commands::Serve { addr, port, lan } => {
            let db = Database::open()?;
            let bind = if lan { "0.0.0.0".to_string() } else { addr };
            let socket: std::net::SocketAddr = format!("{}:{}", bind, port)
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid bind address: {e}"))?;
            server::run(db, socket)?;
        }

        Commands::Backup {
            app,
            all,
            include_cache,
            include_state,
            out,
            force,
        } => {
            let cfg = config::FiliConfig::load()?;
            let catalog = bestiary::Catalog::load()?;
            match (all, app.as_deref()) {
                (true, _) => {
                    let opts = backup::BackupAllOptions {
                        out_override: out,
                        include_cache,
                        include_state,
                        skip_existing: !force,
                    };
                    let summary = backup::backup_all(&catalog, &cfg, &opts)?;
                    println!(
                        "✓ wrote {}, skipped {}, empty {}, failed {}",
                        summary.written, summary.skipped, summary.empty, summary.failed
                    );
                }
                (false, Some(arg)) => {
                    // Accept either a bestiary app id ("aquaria") or a
                    // path on disk ("~/.aquaria"). Path detection is
                    // shape-based: leading `/` or `~/`, or contains `/`.
                    let id = resolve_app_id(&catalog, arg)?;
                    let category = catalog
                        .get(&id)
                        .and_then(|e| e.creature.category.as_deref());
                    let resolved = cfg.resolve_backup_dir(out, category)?;
                    let opts = backup::BackupOptions {
                        out: resolved,
                        include_cache,
                        include_state,
                        skip_existing: !force,
                    };
                    match backup::backup_app(&catalog, &id, &opts)? {
                        Some(p) => println!("✓ {}", p.display()),
                        None => println!("- {id}: nothing on disk to archive"),
                    }
                }
                (false, None) => {
                    anyhow::bail!("specify either an app id or --all");
                }
            }
        }

        Commands::Privacy {
            path,
            level,
            marker,
        } => {
            let path = expand_path(&path);
            let level = match level.to_lowercase().as_str() {
                "public" => models::PrivacyLevel::Public,
                "personal" | "private" => models::PrivacyLevel::Personal,
                "confidential" | "secret" => models::PrivacyLevel::Confidential,
                _ => {
                    eprintln!(
                        "Unknown privacy level: {}. Use: public, personal, or confidential",
                        level
                    );
                    std::process::exit(1);
                }
            };

            if marker {
                // Create marker file for persistence across rescans
                let marker_name = match level {
                    models::PrivacyLevel::Public => ".fili-public",
                    models::PrivacyLevel::Personal => ".fili-private",
                    models::PrivacyLevel::Confidential => ".fili-confidential",
                };
                std::fs::write(path.join(marker_name), "")?;
                println!("✓ Created {} in {}", marker_name, path.display());
            }

            // Update in DB if collection exists
            let db = Database::open()?;
            if let Some(collection) = db.find_entry_by_path(&path)? {
                db.set_privacy(collection.id, &level)?;
                println!("✓ Set {} to {}", path.display(), level.as_str());
            } else if !marker {
                println!("Collection not indexed yet. Use --marker to create a marker file.");
            }
        }
    }

    Ok(())
}

fn expand_path(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = directories::BaseDirs::new() {
            return home.home_dir().join(rest);
        }
    } else if path == "~" {
        if let Some(home) = directories::BaseDirs::new() {
            return home.home_dir().to_path_buf();
        }
    }
    std::path::PathBuf::from(path)
}

/// Resolve an `fili backup <arg>` argument into a bestiary app id.
/// Accepts either the id directly ("aquaria") or a path that bestiary
/// owns ("~/.aquaria", "/home/user/.config/discord"). Errors with a
/// helpful message if the arg is neither.
fn resolve_app_id(catalog: &bestiary::Catalog, arg: &str) -> Result<String> {
    // Path-shaped: starts with `/`, `~`, or contains a `/`.
    let is_pathy = arg.starts_with('/') || arg.starts_with('~') || arg.contains('/');
    if is_pathy {
        let p = expand_path(arg);
        match catalog.lookup_path(&p) {
            Some(entry) => return Ok(entry.creature.name.clone()),
            None => anyhow::bail!(
                "no bestiary app owns {} — try the app id directly, or check `bestiary lookup {}`",
                p.display(),
                p.display(),
            ),
        }
    }
    if catalog.get(arg).is_some() {
        return Ok(arg.to_string());
    }
    anyhow::bail!(
        "app {arg:?} not found in bestiary catalog — pass either an app id or a path that bestiary covers"
    );
}

fn show_status(db: &Database) -> Result<()> {
    let stats = db.get_stats()?;

    println!("Fili Status");
    println!("===========\n");
    println!("Collections: {}", stats.collection_count);
    println!("Total size:  {}", format_size(stats.total_size));
    println!("\nBy type:");
    for (ctype, count) in &stats.by_type {
        println!("  {}: {}", ctype, count);
    }

    if stats.unprotected_count > 0 {
        println!("\n⚠ {} collections not backed up", stats.unprotected_count);
    }

    Ok(())
}

fn find_collections(_db: &Database, query: &str) -> Result<()> {
    println!("Searching collections for '{}'...", query);
    // TODO: implement
    Ok(())
}

fn find_files(_db: &Database, query: &str) -> Result<()> {
    println!("Searching files for '{}'...", query);
    // TODO: implement
    Ok(())
}

fn list_paths(_db: &Database, _unknown_only: bool) -> Result<()> {
    println!("Known paths:");
    // TODO: implement
    Ok(())
}

fn show_unprotected(_db: &Database) -> Result<()> {
    println!("Unprotected collections:");
    // TODO: implement
    Ok(())
}

fn show_duplicates(_db: &Database, _same_device: bool) -> Result<()> {
    println!("Duplicate collections:");
    // TODO: implement
    Ok(())
}

fn export_index(_db: &Database, _output: &str) -> Result<()> {
    // TODO: implement
    Ok(())
}

fn show_stats(_db: &Database) -> Result<()> {
    println!("Statistics:");
    // TODO: implement
    Ok(())
}

fn list_unknowns(db: &Database) -> Result<()> {
    let unknowns = db.list_unknowns()?;
    if unknowns.is_empty() {
        println!("No unknowns. Run 'fili scan' to discover paths.");
        return Ok(());
    }
    println!("{} unclassified paths:\n", unknowns.len());
    for u in &unknowns {
        let exts: Vec<String> = u
            .top_extensions
            .iter()
            .map(|e| format!("{}×{}", e.ext, e.count))
            .collect();
        let ext_str = if exts.is_empty() {
            String::new()
        } else {
            format!(" [{}]", exts.join(", "))
        };
        println!(
            "  {}  ({} files, {} dirs, {}){}",
            u.path,
            u.file_count,
            u.dir_count,
            format_size(u.total_size),
            ext_str,
        );
    }
    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
