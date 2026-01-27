use anyhow::Result;
use clap::{Parser, Subcommand};

mod db;
mod scanner;
mod models;
mod rules;

use db::Database;

#[derive(Parser)]
#[command(name = "fili")]
#[command(about = "Personal file intelligence system", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the database
    Init,
    
    /// Scan filesystem for files and collections
    Scan {
        /// Path to scan (defaults to home directory)
        #[arg(default_value = "~")]
        path: String,
        
        /// Don't prompt for unknown paths
        #[arg(long)]
        non_interactive: bool,
    },
    
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
    
    /// Classify a path
    Classify {
        /// Path to classify
        path: String,
        
        /// Classification type
        #[arg(long, short = 't')]
        as_type: String,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Init => {
            let db = Database::init()?;
            println!("✓ Database initialized at {}", db.path().display());
            println!("✓ Default path rules loaded");
            println!("\nRun 'fili scan' to index your filesystem.");
        }
        
        Commands::Scan { path, non_interactive } => {
            let db = Database::open()?;
            let path = expand_path(&path);
            scanner::scan(&db, &path, !non_interactive)?;
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
        
        Commands::Classify { path, as_type } => {
            let db = Database::open()?;
            let path = expand_path(&path);
            db.classify_path(&path, &as_type)?;
            println!("✓ Classified {} as {}", path.display(), as_type);
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
    }
    
    Ok(())
}

fn expand_path(path: &str) -> std::path::PathBuf {
    if path.starts_with('~') {
        if let Some(home) = directories::BaseDirs::new() {
            return home.home_dir().join(&path[2..]);
        }
    }
    std::path::PathBuf::from(path)
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
