//! Enumerate mounted drives via `lsblk`.
//!
//! Stage 1: we just want an inventory of storage media. For each mounted
//! filesystem we get its identity (uuid, label, fs_type, size) and the
//! parent disk's model/serial. Mount paths are captured as `current_mount`
//! but are NOT the drive's identity — a drive re-appearing at a different
//! mount is the same drive.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::Drive;

#[derive(Debug, Deserialize)]
struct LsblkRoot {
    blockdevices: Vec<LsblkNode>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct LsblkNode {
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    fstype: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial: Option<String>,
    #[serde(default)]
    mountpoints: Vec<Option<String>>,
    #[serde(default)]
    children: Vec<LsblkNode>,
}

/// Run `lsblk -J` and emit one Drive per mounted filesystem. Pseudo mounts
/// like `[SWAP]` are filtered out.
pub fn enumerate() -> Result<Vec<Drive>> {
    let output = Command::new("lsblk")
        .args([
            "-J",
            "-o",
            "NAME,UUID,LABEL,FSTYPE,SIZE,MODEL,SERIAL,MOUNTPOINTS",
        ])
        .output()
        .context("failed to run lsblk — is it installed?")?;

    if !output.status.success() {
        anyhow::bail!(
            "lsblk failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let parsed: LsblkRoot = serde_json::from_slice(&output.stdout)
        .context("failed to parse lsblk JSON output")?;

    let now = now_secs();
    let mut drives = Vec::new();
    for node in &parsed.blockdevices {
        collect(node, None, None, now, &mut drives);
    }
    Ok(drives)
}

/// Walk the lsblk tree. A node with a real mount point becomes a Drive.
/// model/serial live on the parent disk, so we pass them down.
fn collect(
    node: &LsblkNode,
    parent_model: Option<&str>,
    parent_serial: Option<&str>,
    now: i64,
    out: &mut Vec<Drive>,
) {
    let model = node.model.as_deref().or(parent_model);
    let serial = node.serial.as_deref().or(parent_serial);

    for mp in &node.mountpoints {
        let Some(path) = mp else { continue };
        let path = path.trim();
        // Pseudo mounts like "[SWAP]" or empty
        if path.is_empty() || path.starts_with('[') {
            continue;
        }
        out.push(Drive {
            id: 0,
            uuid: node.uuid.clone(),
            label: node.label.clone(),
            fs_type: node.fstype.clone(),
            size: node.size.clone(),
            model: model.map(str::to_string),
            serial: serial.map(str::to_string),
            friendly_name: None,
            current_mount: Some(path.to_string()),
            first_seen: now,
            last_seen: now,
        });
    }

    for child in &node.children {
        collect(child, model, serial, now, out);
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
