// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! Diagnostics bundle: versions, the pstore archive (text records), the
//! previous boot's journal tail and this boot's kernel log head, with MAC
//! addresses, IPv4 addresses, the device serial, the hostname and local user
//! names replaced, packed as a tarball under /var/lib/tb323fu/diagnostics.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tb323fu_helper_core::{features, sys};

pub const OUT_DIR: &str = "/var/lib/tb323fu/diagnostics";

fn cmd(args: &[&str]) -> String {
    Command::new(args[0])
        .args(&args[1..])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn is_hex(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// Replace xx:xx:xx:xx:xx:xx sequences.
fn strip_macs(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if i + 17 <= c.len()
            && (0..17).all(|k| if k % 3 == 2 { c[i + k] == ':' } else { is_hex(c[i + k]) })
            && (i + 17 == c.len() || !is_hex(c[i + 17]))
        {
            out.push_str("<mac>");
            i += 17;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    out
}

/// Replace dotted IPv4 addresses.
fn strip_ipv4(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if c[i].is_ascii_digit() && (i == 0 || !(c[i - 1].is_ascii_digit() || c[i - 1] == '.')) {
            let mut j = i;
            let mut groups = 0;
            loop {
                let st = j;
                while j < c.len() && c[j].is_ascii_digit() && j - st < 3 {
                    j += 1;
                }
                if j == st {
                    break;
                }
                groups += 1;
                if groups == 4 || j >= c.len() || c[j] != '.' {
                    break;
                }
                j += 1;
            }
            // a trailing '.' ends a sentence unless another number follows
            let boundary = j >= c.len()
                || (!c[j].is_ascii_digit() && !(c[j] == '.' && j + 1 < c.len() && c[j + 1].is_ascii_digit()));
            if groups == 4 && boundary {
                out.push_str("<ip>");
                i = j;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

fn secrets() -> Vec<(String, &'static str)> {
    let mut v = Vec::new();
    if let Some(s) = sys::read_opt(&sys::path("/proc/device-tree/serial-number")) {
        let s = s.trim_matches(char::from(0)).to_string();
        if s.len() >= 4 {
            v.push((s, "<serial>"));
        }
    }
    if let Some(h) = sys::read_opt(&sys::path("/etc/hostname")) {
        if h.len() >= 3 {
            v.push((h, "<host>"));
        }
    }
    if let Ok(pw) = fs::read_to_string(sys::path("/etc/passwd")) {
        for l in pw.lines() {
            let f: Vec<&str> = l.split(':').collect();
            if f.len() > 2 && f[2].parse::<u32>().map(|u| (1000..60000).contains(&u)).unwrap_or(false) && f[0].len() >= 3 {
                v.push((f[0].to_string(), "<user>"));
            }
        }
    }
    v
}

pub fn sanitize(s: &str, secrets: &[(String, &str)]) -> String {
    let mut t = strip_ipv4(&strip_macs(s));
    for (k, r) in secrets {
        t = t.replace(k.as_str(), r);
    }
    t
}

pub fn export(helper_version: &str) -> Result<PathBuf, String> {
    let out_dir = sys::path(OUT_DIR);
    fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let work = out_dir.join(format!("work-{stamp}"));
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let sec = secrets();
    let put = |name: &str, body: &str| -> Result<(), String> {
        fs::write(work.join(name), sanitize(body, &sec)).map_err(|e| e.to_string())
    };
    let mut versions = format!("helper {helper_version}\nkernel {}\nseries {}\n", features::kernel_version(), features::series_tag());
    for (f, st) in features::firmware_check() {
        versions.push_str(&format!("firmware {f} {st}\n"));
    }
    put("versions.txt", &versions)?;
    for p in features::pstore_files() {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.ends_with(".z") {
            continue; // compressed records: not readable text
        }
        if let Ok(bytes) = fs::read(&p) {
            put(&format!("pstore-{name}.txt"), &String::from_utf8_lossy(&bytes))?;
        }
    }
    put("journal-previous-boot.txt", &cmd(&["journalctl", "-b", "-1", "-n", "400", "--no-pager", "-o", "short-monotonic"]))?;
    let dmesg = cmd(&["dmesg"]);
    put("dmesg-head.txt", &dmesg.lines().take(400).collect::<Vec<_>>().join("\n"))?;
    let tarball = out_dir.join(format!("tb323fu-diagnostics-{stamp}.tar.gz"));
    let ok = Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(&work)
        .arg(".")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = fs::remove_dir_all(&work);
    if !ok {
        return Err("tar failed".into());
    }
    set_mode(&tarball, 0o644);
    Ok(tarball)
}

fn set_mode(p: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(p, fs::Permissions::from_mode(mode));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips() {
        // documentation addresses (RFC 5737); the MAC is built so no literal one sits in the source
        let mac = ["02", "00", "5e", "10", "00", "01"].join(":");
        let s = sanitize(&format!("wlan {mac} got 203.0.113.5/24 v1.2.3 and 198.51.100.1"), &[("alice".into(), "<user>")]);
        assert_eq!(s, "wlan <mac> got <ip>/24 v1.2.3 and <ip>");
        assert_eq!(sanitize("user alice", &[("alice".into(), "<user>")]), "user <user>");
        assert_eq!(sanitize("with address 203.0.113.9.", &[]), "with address <ip>.");
        assert_eq!(sanitize("ver 1.2.3.4.5", &[]), "ver 1.2.3.4.5");
    }
}
