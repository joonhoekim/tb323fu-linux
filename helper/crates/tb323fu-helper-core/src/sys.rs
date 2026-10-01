// SPDX-License-Identifier: MIT
//! Filesystem access with an optional root prefix: every absolute device path
//! (sysfs, /etc files the device ships, pstore archive) is resolved under
//! `TB323FU_SYSFS_ROOT` when that variable is set, so tests run against a fake
//! tree without touching the machine.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Resolve an absolute device path under the optional test root.
pub fn path(p: &str) -> PathBuf {
    match std::env::var_os("TB323FU_SYSFS_ROOT") {
        Some(root) if !root.is_empty() => {
            let mut r = PathBuf::from(root);
            r.push(p.trim_start_matches('/'));
            r
        }
        _ => PathBuf::from(p),
    }
}

pub fn exists(p: &Path) -> bool {
    p.exists()
}

/// Read a sysfs attribute, trimmed.
pub fn read(p: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(p)?.trim().to_string())
}

pub fn read_opt(p: &Path) -> Option<String> {
    read(p).ok()
}

pub fn read_i64(p: &Path) -> Option<i64> {
    read(p).ok()?.parse().ok()
}

pub fn write(p: &Path, v: &str) -> io::Result<()> {
    fs::write(p, v)
}

/// The word in brackets of a sysfs "choice" attribute ("a [b] c" -> "b").
pub fn selected(choice: &str) -> Option<String> {
    let s = choice.find('[')?;
    let e = choice[s..].find(']')? + s;
    Some(choice[s + 1..e].to_string())
}

/// Entries of a directory (file names), sorted; empty when missing.
pub fn list_dir(p: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(p)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Tests that set `TB323FU_SYSFS_ROOT` hold this (the variable is process-wide).
#[cfg(test)]
pub static TEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_word() {
        assert_eq!(selected("Unknown [SDP] DCP").as_deref(), Some("SDP"));
        assert_eq!(selected("[C] PD PD_PPS").as_deref(), Some("C"));
        assert_eq!(selected("none"), None);
    }
}
