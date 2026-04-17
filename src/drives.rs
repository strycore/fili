//! Enumerate mounted drives via `lsblk`.
//!
//! Stage 1: we just want an inventory of storage media. For each mounted
//! filesystem we get its identity (uuid, label, fs_type, size) and the
//! parent disk's model/serial. Mount paths are captured as `current_mount`
//! but are NOT the drive's identity — a drive re-appearing at a different
//! mount is the same drive.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
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

/// Run `lsblk -J` and emit one Drive per filesystem UUID. Pseudo mounts
/// like `[SWAP]` are filtered out. A UUID seen at multiple mount points
/// (btrfs subvolumes etc.) collapses to a single Drive whose current_mount
/// is the most canonical path (shortest, preferring `/` and `/boot`-style
/// roots over subvolume mounts like `/home` and `/swap`).
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
    let mut raw = Vec::new();
    for node in &parsed.blockdevices {
        collect(node, None, None, now, &mut raw);
    }

    let mut by_uuid: HashMap<String, Drive> = HashMap::new();
    let mut no_uuid: Vec<Drive> = Vec::new();
    for drive in raw {
        match drive.uuid.clone() {
            Some(uuid) => {
                by_uuid
                    .entry(uuid)
                    .and_modify(|existing| {
                        if mount_priority(drive.current_mount.as_deref())
                            < mount_priority(existing.current_mount.as_deref())
                        {
                            *existing = drive.clone();
                        }
                    })
                    .or_insert(drive);
            }
            None => no_uuid.push(drive),
        }
    }

    let mut drives: Vec<Drive> = by_uuid.into_values().chain(no_uuid).collect();
    for d in &mut drives {
        if d.friendly_name.is_none() {
            d.friendly_name = auto_friendly_name(
                d.current_mount.as_deref(),
                d.fs_type.as_deref(),
                d.label.as_deref(),
            );
        }
    }
    drives.sort_by(|a, b| a.current_mount.cmp(&b.current_mount));
    Ok(drives)
}

/// Lower score = more canonical mount. "/" wins over "/home", "/boot" wins
/// over "/boot/efi", etc. Deeper paths get higher scores.
fn mount_priority(mount: Option<&str>) -> (usize, String) {
    let path = mount.unwrap_or("");
    let depth = path.matches('/').count();
    (depth, path.to_string())
}

/// Pick a sensible default name for a drive without a filesystem label.
/// Driven by mount point — standard FHS paths get recognisable names.
fn auto_friendly_name(
    mount: Option<&str>,
    fs_type: Option<&str>,
    label: Option<&str>,
) -> Option<String> {
    if label.is_some() {
        return None; // labels already self-describe, leave friendly_name clear
    }
    let m = mount?;
    let name = match m {
        "/" => "System",
        "/home" => "Home",
        "/swap" => "Swap",
        "/boot" => "Boot",
        "/boot/efi" => "EFI Boot",
        "/tmp" => "Tmp",
        _ => return None,
    };
    // Narrow EFI by fs_type to avoid false positives on /boot/efi bind mounts etc.
    if m == "/boot/efi" && fs_type != Some("vfat") {
        return None;
    }
    Some(name.to_string())
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
