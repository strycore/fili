//! User configuration at `~/.config/fili/config.toml`.
//!
//! Currently the only field is `backup_dir`, which makes `fili backup`
//! work without `--out` on every invocation. We use TOML rather than the
//! existing `rules.local.json` because rules are a content overlay (lots
//! of structured data) while this is plain user preferences.
//!
//! Example:
//!
//! ```toml
//! backup_dir = "/run/media/strider/Backup/Archives/Software Settings"
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FiliConfig {
    /// Default destination for `fili backup`. When set, callers can omit
    /// `--out`. When unset, `fili backup` without `--out` errors out
    /// with instructions for setting this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_dir: Option<PathBuf>,
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

    /// Resolve the effective backup directory: prefer the CLI-supplied
    /// path, fall back to config, fail with a helpful message otherwise.
    pub fn resolve_backup_dir(&self, cli: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(p) = cli {
            return Ok(p);
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
