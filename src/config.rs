//! User configuration at `~/.config/fili/config.toml`.
//!
//! Currently a small set of preferences for `fili backup`. We use TOML
//! rather than the existing `rules.local.json` because rules are a
//! content overlay (lots of structured data) while this is plain user
//! preferences.
//!
//! Example:
//!
//! ```toml
//! # Default destination for backups; required for `fili backup`
//! # without `--out`.
//! backup_dir = "/run/media/strider/Backup/Archives/Software Settings"
//!
//! # Optional per-bestiary-category overrides. An app whose bestiary
//! # category matches a key here goes to that path instead of
//! # `backup_dir`. Useful for routing game saves separately.
//! [backup_dir_by_category]
//! gaming = "/run/media/strider/Backup/Archives/Game saves"
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FiliConfig {
    /// Default destination for `fili backup`. When set, callers can omit
    /// `--out`. When unset, `fili backup` without `--out` errors out
    /// with instructions for setting this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<PathBuf>,

    /// Per-category destination overrides. An app whose bestiary
    /// category matches a key here goes to that path; everything else
    /// uses `backup_dir`. Common case: routing `gaming` apps to a
    /// "Game saves" directory while everything else goes to settings.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub backup_dir_by_category: HashMap<String, PathBuf>,
}

impl FiliConfig {
    /// Load the user's config from disk. Returns the default
    /// (everything `None`) if the file doesn't exist; surfaces parse
    /// errors so the user knows their config is broken.
    pub fn load() -> Result<Self> {
        let Some(path) = config_path() else {
            return Ok(Self::default());
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: FiliConfig =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    /// Resolve the effective backup directory for an app of the given
    /// category. Precedence: CLI override > category override > default.
    /// Errors with a helpful message if none of the three are set.
    pub fn resolve_backup_dir(
        &self,
        cli: Option<PathBuf>,
        category: Option<&str>,
    ) -> Result<PathBuf> {
        if let Some(p) = cli {
            return Ok(p);
        }
        if let Some(cat) = category {
            if let Some(p) = self.backup_dir_by_category.get(cat) {
                return Ok(p.clone());
            }
        }
        if let Some(p) = &self.backup_dir {
            return Ok(p.clone());
        }
        let example = config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.config/fili/config.toml".to_string());
        anyhow::bail!(
            "no backup directory set.\n  \
             Either pass `--out <DIR>`, or set a default in {}:\n\n  \
             backup_dir = \"/path/to/backups\"\n",
            example
        );
    }
}

/// Path to the config file. Returns `None` only if neither
/// `XDG_CONFIG_HOME` nor `HOME` is set.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("fili").join("config.toml"))
}
