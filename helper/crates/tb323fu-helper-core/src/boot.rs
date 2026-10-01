// SPDX-License-Identifier: MIT
//! Multiboot: the root filesystems the initramfs can boot and the one-shot /
//! persistent selection it reads (userspace/platform and the kernel initramfs,
//! docs/helper.md "Boot").
//!
//! Candidate roots are GPT partitions named `baldur-root` (UFS, the default),
//! `baldur-root-sd` and `tb323fu-*`. The selection files live on the UFS root
//! only: `/etc/tb323fu/boot-next` (consumed by the initramfs on the next boot)
//! and `/etc/tb323fu/boot-default`. When the helper runs from another root it
//! mounts the UFS root under `/run/tb323fu/ufs` for the read or write. A root
//! that is already mounted (e.g. the desktop's automounter on another distro)
//! is used where it is instead: a second ext4 mount with other options fails.
//!
//! Tests: with `TB323FU_SYSFS_ROOT` set, partitions come from the fake
//! `/sys/class/block`, the selection files from the fake `/etc/tb323fu`, the
//! content of each root from `<fake root>/roots/<name>/`, and the current root
//! from `TB323FU_CURRENT_ROOT` (default `baldur-root`). Nothing is mounted.

use crate::sys;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Res<T> = Result<T, String>;

pub const DEFAULT_ROOT: &str = "baldur-root";
const UFS_MNT: &str = "/run/tb323fu/ufs";
const PROBE_MNT: &str = "/run/tb323fu/probe";

/// One bootable root as the initramfs sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Root {
    /// GPT partition name
    pub name: String,
    /// PRETTY_NAME from that root's os-release ("" when empty / unreadable)
    pub label: String,
    /// the partition exists now
    pub present: bool,
    /// "systemd" (/sbin/init), "nixos" (system profile) or "none"
    pub init: String,
    /// what this root lacks to run the shared kernel well (see `health`)
    pub problems: Vec<String>,
}

pub fn is_candidate(n: &str) -> bool {
    n == "baldur-root" || n == "baldur-root-sd" || (n.starts_with("tb323fu-") && n.len() > "tb323fu-".len())
}

fn testing() -> bool {
    std::env::var_os("TB323FU_SYSFS_ROOT").is_some_and(|v| !v.is_empty())
}

fn uevent(p: &Path) -> (Option<String>, Option<String>) {
    let (mut name, mut dev) = (None, None);
    if let Ok(t) = fs::read_to_string(p) {
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("PARTNAME=") {
                name = Some(v.to_string());
            } else if let Some(v) = l.strip_prefix("DEVNAME=") {
                dev = Some(v.to_string());
            }
        }
    }
    (name, dev)
}

/// (partition name, /dev node) of every candidate root, sorted by name.
pub fn partitions() -> Vec<(String, String)> {
    let base = sys::path("/sys/class/block");
    let mut v: Vec<(String, String)> = sys::list_dir(&base)
        .into_iter()
        .filter_map(|e| {
            let (n, d) = uevent(&base.join(&e).join("uevent"));
            let n = n?;
            is_candidate(&n).then(|| (n, format!("/dev/{}", d.unwrap_or(e))))
        })
        .collect();
    v.sort();
    v.dedup_by(|a, b| a.0 == b.0); // the first (lowest device name) wins, as in the initramfs
    v
}

fn partition_dev(name: &str) -> Option<String> {
    partitions().into_iter().find(|(n, _)| n == name).map(|(_, d)| d)
}

/// The partition `/` was mounted from ("" when it is not a named partition).
pub fn current_root() -> String {
    if let Ok(v) = std::env::var("TB323FU_CURRENT_ROOT") {
        return v;
    }
    if testing() {
        return DEFAULT_ROOT.to_string();
    }
    let Ok(md) = fs::metadata("/") else { return String::new() };
    let dev = md.dev();
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
    uevent(Path::new(&format!("/sys/dev/block/{major}:{minor}/uevent"))).0.unwrap_or_default()
}

fn mount(dev: &str, at: &str, rw: bool) -> Res<()> {
    fs::create_dir_all(at).map_err(|e| format!("{at}: {e}"))?;
    // ro,noload: never replay a journal (write) just to look at a root
    let opts = if rw { "rw" } else { "ro,noload" };
    let st = Command::new("mount").args(["-t", "ext4", "-o", opts, dev, at]).status().map_err(|e| format!("mount: {e}"))?;
    if st.success() { Ok(()) } else { Err(format!("mount {dev} on {at} failed")) }
}

fn umount(at: &str) {
    let _ = Command::new("umount").arg(at).status();
}

/// Where `dev` is mounted already, and whether read-write (first entry wins).
fn mounted_at(dev: &str) -> Option<(PathBuf, bool)> {
    let real = fs::canonicalize(dev).ok()?;
    let t = fs::read_to_string("/proc/self/mounts").ok()?;
    t.lines().find_map(|l| {
        let mut f = l.split_whitespace();
        let (src, dir, _ty, opts) = (f.next()?, f.next()?, f.next()?, f.next()?);
        if fs::canonicalize(src).ok()? != real {
            return None;
        }
        // /proc/mounts escapes blanks in paths as octal (\040)
        let dir = dir.replace("\\040", " ");
        Some((PathBuf::from(dir), opts.split(',').any(|o| o == "rw")))
    })
}

/// Run `f` with the UFS root's /etc/tb323fu (read-only unless `write`).
fn with_ufs_etc<T>(write: bool, f: impl FnOnce(&Path) -> Res<T>) -> Res<T> {
    if testing() || current_root() == DEFAULT_ROOT {
        let d = sys::path("/etc/tb323fu");
        if write {
            fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        return f(&d);
    }
    let dev = partition_dev(DEFAULT_ROOT).ok_or("no baldur-root partition")?;
    if let Some((at, rw)) = mounted_at(&dev) {
        if write && !rw {
            return Err(format!("{dev} is mounted read-only on {}", at.display()));
        }
        let d = at.join("etc/tb323fu");
        if write {
            fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let r = f(&d);
        if write {
            let _ = Command::new("sync").status();
        }
        return r;
    }
    mount(&dev, UFS_MNT, write)?;
    let d = PathBuf::from(UFS_MNT).join("etc/tb323fu");
    let r = (|| {
        if write {
            fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        f(&d)
    })();
    if write {
        let _ = Command::new("sync").status();
    }
    umount(UFS_MNT);
    r
}

fn first_word(p: &Path) -> Option<String> {
    let t = fs::read_to_string(p).ok()?;
    let w = t.split_whitespace().next()?.to_string();
    (!w.is_empty()).then_some(w)
}

/// The one-shot selection ("" when none).
pub fn next() -> String {
    with_ufs_etc(false, |d| Ok(first_word(&d.join("boot-next")).unwrap_or_default())).unwrap_or_default()
}

/// The persistent default (baldur-root when not set).
pub fn default_root() -> String {
    with_ufs_etc(false, |d| Ok(first_word(&d.join("boot-default")).unwrap_or_else(|| DEFAULT_ROOT.to_string())))
        .unwrap_or_else(|_| DEFAULT_ROOT.to_string())
}

fn check_name(name: &str) -> Res<()> {
    if !is_candidate(name) {
        return Err(format!("{name}: not a root partition name (baldur-root, baldur-root-sd, tb323fu-*)"));
    }
    if partition_dev(name).is_none() {
        return Err(format!("{name}: no such partition"));
    }
    Ok(())
}

pub fn set_next(name: &str) -> Res<()> {
    check_name(name)?;
    with_ufs_etc(true, |d| fs::write(d.join("boot-next"), format!("{name}\n")).map_err(|e| format!("boot-next: {e}")))
}

pub fn clear_next() -> Res<()> {
    with_ufs_etc(true, |d| match fs::remove_file(d.join("boot-next")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("boot-next: {e}")),
    })
}

pub fn set_default(name: &str) -> Res<()> {
    check_name(name)?;
    with_ufs_etc(true, |d| {
        let p = d.join("boot-default");
        if name == DEFAULT_ROOT {
            match fs::remove_file(&p) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("boot-default: {e}")),
            }
        } else {
            fs::write(&p, format!("{name}\n")).map_err(|e| format!("boot-default: {e}"))
        }
    })
}

/// What a mounted (or current) root contains: (PRETTY_NAME, init kind).
/// `rel` inside the root at `dir`, following symlinks (absolute ones relative
/// to `dir`) component by component -- NixOS's /etc/os-release is
/// /etc/static/os-release, itself a link into /nix/store.
fn in_root(dir: &Path, rel: &str) -> PathBuf {
    let mut todo: Vec<String> = rel.split('/').filter(|c| !c.is_empty()).rev().map(String::from).collect();
    let mut cur: Vec<String> = Vec::new();
    let mut hops = 0;
    while let Some(c) = todo.pop() {
        match c.as_str() {
            "." => continue,
            ".." => {
                cur.pop();
                continue;
            }
            _ => {}
        }
        let here = dir.join(cur.join("/")).join(&c);
        match fs::read_link(&here) {
            Ok(t) if hops < 40 => {
                hops += 1;
                let t = t.to_string_lossy().into_owned();
                if t.starts_with('/') {
                    cur.clear();
                }
                todo.extend(t.split('/').filter(|c| !c.is_empty()).rev().map(String::from));
            }
            _ => cur.push(c),
        }
    }
    dir.join(cur.join("/"))
}

/// The running kernel's release (`uname -r`).
pub fn kernel_release() -> String {
    sys::read_opt(&sys::path("/proc/sys/kernel/osrelease")).unwrap_or_default()
}

/// A firmware file, also compressed (Fedora and Arch ship .zst / .xz).
fn has_firmware(dir: &Path, rel: &str) -> bool {
    ["", ".zst", ".xz"].iter().any(|ext| fs::symlink_metadata(in_root(dir, &format!("{rel}{ext}"))).is_ok())
}

/// Firmware the root needs: (path under lib/firmware, what breaks without it).
const KEY_FIRMWARE: [(&str, &str); 2] = [
    ("qcom/kaanapali/lenovo/baldur/adsp.mbn", "audio DSP firmware missing (no sound)"),
    ("ath12k/WCN7860/hw2.0/amss.bin", "Wi-Fi firmware missing"),
];

/// What a root with /sbin/init lacks for the shared kernel `release`: its
/// modules (modules.dep), the out-of-tree amplifier driver (extra/), key
/// firmware. NixOS roots carry modules and firmware in the store: not checked.
pub fn health(dir: &Path, init: &str, release: &str) -> Vec<String> {
    let mut p = Vec::new();
    if init != "systemd" || release.is_empty() {
        return p;
    }
    let m = format!("lib/modules/{release}");
    if !in_root(dir, &format!("{m}/modules.dep")).exists() {
        p.push(format!("missing modules for {release} (no sound or Wi-Fi)"));
    } else if !in_root(dir, &format!("{m}/extra")).is_dir() {
        p.push(format!("missing extra/ modules for {release} (no speakers)"));
    }
    for (fw, what) in KEY_FIRMWARE {
        if !has_firmware(dir, &format!("lib/firmware/{fw}")) {
            p.push(what.to_string());
        }
    }
    p
}

fn inspect(dir: &Path) -> (String, String) {
    let label = fs::read_to_string(in_root(dir, "etc/os-release"))
        .or_else(|_| fs::read_to_string(in_root(dir, "usr/lib/os-release")))
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        })
        .unwrap_or_default();
    // symlink_metadata: /sbin/init is often an absolute symlink into that root
    let init = if fs::symlink_metadata(dir.join("sbin/init")).is_ok() && !dir.join("debootstrap").exists() {
        "systemd"
    } else if fs::symlink_metadata(dir.join("nix/var/nix/profiles/system")).is_ok() {
        "nixos"
    } else {
        "none"
    };
    (label, init.to_string())
}

/// Every candidate root with its label and init kind. Mounts other roots
/// read-only (no journal replay) for a moment; the daemon caches the result.
pub fn roots() -> Vec<Root> {
    let cur = current_root();
    let rel = kernel_release();
    let look = |dir: &Path| {
        let (label, init) = inspect(dir);
        let problems = health(dir, &init, &rel);
        (label, init, problems)
    };
    partitions()
        .into_iter()
        .map(|(name, dev)| {
            let (label, init, problems) = if testing() {
                look(&sys::path(&format!("/roots/{name}")))
            } else if name == cur {
                look(Path::new("/"))
            } else if let Some((at, _)) = mounted_at(&dev) {
                look(&at)
            } else if mount(&dev, PROBE_MNT, false).is_ok() {
                let r = look(Path::new(PROBE_MNT));
                umount(PROBE_MNT);
                r
            } else {
                (String::new(), "none".to_string(), Vec::new())
            };
            Root { name, label, present: true, init, problems }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::TEST_ENV as LOCK;

    fn fake() -> tempdir::Dir {
        tempdir::Dir::new()
    }

    mod tempdir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                let p = std::env::temp_dir().join(format!("tb323fu-boot-{}-{}", std::process::id(),
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn part(root: &Path, dev: &str, name: &str) {
        let d = root.join("sys/class/block").join(dev);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("uevent"), format!("MAJOR=8\nDEVNAME={dev}\nDEVTYPE=partition\nPARTNAME={name}\n")).unwrap();
    }

    #[test]
    fn roots_and_selection() {
        let _g = LOCK.lock().unwrap();
        let t = fake();
        let r = &t.0;
        part(r, "sda17", "baldur-root");
        part(r, "mmcblk0p1", "baldur-root-sd");
        part(r, "mmcblk0p2", "tb323fu-ubuntu");
        part(r, "mmcblk0p4", "tb323fu-nixos");
        part(r, "sda1", "persist"); // not a candidate
        fs::create_dir_all(r.join("roots/baldur-root/etc")).unwrap();
        fs::write(r.join("roots/baldur-root/etc/os-release"), "NAME=Debian\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\n").unwrap();
        fs::create_dir_all(r.join("roots/baldur-root/sbin")).unwrap();
        std::os::unix::fs::symlink("/lib/systemd/systemd", r.join("roots/baldur-root/sbin/init")).unwrap();
        fs::create_dir_all(r.join("roots/tb323fu-nixos/nix/var/nix/profiles")).unwrap();
        std::os::unix::fs::symlink("/nix/store/x-system", r.join("roots/tb323fu-nixos/nix/var/nix/profiles/system")).unwrap();
        std::env::set_var("TB323FU_SYSFS_ROOT", r);
        std::env::remove_var("TB323FU_CURRENT_ROOT");

        let names: Vec<String> = partitions().into_iter().map(|p| p.0).collect();
        assert_eq!(names, ["baldur-root", "baldur-root-sd", "tb323fu-nixos", "tb323fu-ubuntu"]);
        let rs = roots();
        let deb = rs.iter().find(|x| x.name == "baldur-root").unwrap();
        assert_eq!(deb.label, "Debian GNU/Linux 13 (trixie)");
        assert_eq!(deb.init, "systemd");
        assert_eq!(rs.iter().find(|x| x.name == "tb323fu-nixos").unwrap().init, "nixos");
        assert_eq!(rs.iter().find(|x| x.name == "tb323fu-ubuntu").unwrap().init, "none");
        // health: no /proc/sys/kernel/osrelease in the fake tree -> not checked
        assert!(deb.problems.is_empty());
        assert!(rs.iter().all(|x| x.problems.is_empty()));

        assert_eq!(next(), "");
        assert_eq!(default_root(), "baldur-root");
        set_next("tb323fu-ubuntu").unwrap();
        assert_eq!(next(), "tb323fu-ubuntu");
        assert_eq!(fs::read_to_string(r.join("etc/tb323fu/boot-next")).unwrap(), "tb323fu-ubuntu\n");
        clear_next().unwrap();
        assert_eq!(next(), "");
        assert!(set_next("tb323fu-missing").is_err());
        assert!(set_next("persist").is_err());
        set_default("tb323fu-nixos").unwrap();
        assert_eq!(default_root(), "tb323fu-nixos");
        set_default("baldur-root").unwrap();
        assert!(!r.join("etc/tb323fu/boot-default").exists());
        std::env::remove_var("TB323FU_SYSFS_ROOT");
    }

    #[test]
    fn root_health() {
        let t = fake();
        let d = &t.0;
        let rel = "7.3.0-test";
        assert!(health(d, "nixos", rel).is_empty(), "NixOS roots are not checked");
        let p = health(d, "systemd", rel);
        assert_eq!(p.len(), 3, "{p:?}");
        assert!(p[0].starts_with("missing modules for 7.3.0-test"));
        fs::create_dir_all(d.join("usr/lib/modules").join(rel)).unwrap();
        std::os::unix::fs::symlink("usr/lib", d.join("lib")).unwrap(); // merged /usr
        fs::write(d.join("usr/lib/modules").join(rel).join("modules.dep"), "").unwrap();
        let p = health(d, "systemd", rel);
        assert!(p[0].starts_with("missing extra/"), "{p:?}");
        fs::create_dir_all(d.join("usr/lib/modules").join(rel).join("extra")).unwrap();
        for (fw, _) in KEY_FIRMWARE {
            let f = d.join("usr/lib/firmware").join(format!("{fw}.zst"));
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, "").unwrap();
        }
        assert!(health(d, "systemd", rel).is_empty());
    }
}
