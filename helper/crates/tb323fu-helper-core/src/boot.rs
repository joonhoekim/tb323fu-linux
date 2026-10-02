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
    /// "systemd" (/sbin/init), "nixos" (system profile), "none" (readable,
    /// no system), or "unknown" (it could not be mounted read-only to look)
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

/// "major:minor" of a device number (glibc's encoding).
fn majmin(dev: u64) -> String {
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
    format!("{major}:{minor}")
}

/// The device number of a block device node.
fn rdev(dev: &str) -> Option<u64> {
    use std::os::unix::fs::FileTypeExt;
    let md = fs::metadata(dev).ok()?;
    md.file_type().is_block_device().then(|| md.rdev())
}

/// /proc/self/mountinfo escapes blanks, tabs, newlines and backslashes as octal.
fn unescape(s: &str) -> String {
    s.replace("\\040", " ").replace("\\011", "\t").replace("\\012", "\n").replace("\\134", "\\")
}

/// In a mountinfo text: where the filesystem on device `mm` ("major:minor")
/// is mounted with its own root (not a bind mount of a subdirectory) and not
/// covered by a later mount on the same point, and whether read-write. The
/// first such entry wins. Matching the device number (not the source string)
/// is exact: no symlink resolution, no relative source names ("tmpfs",
/// "none") resolved against the working directory.
///
/// The covered case is the Ubuntu bug of 10-01: the Fedora root mounted on
/// /mnt/t and the SD Debian root mounted over it made `boot list` show
/// Fedora as Debian with the SD root's health warnings.
fn find_mount(mountinfo: &str, mm: &str) -> Option<(PathBuf, bool)> {
    // id parent major:minor root mount-point options ... - type source super-options
    let rows: Vec<Vec<&str>> = mountinfo.lines().map(|l| l.split(' ').collect::<Vec<_>>()).filter(|f| f.len() >= 6).collect();
    rows.iter().enumerate().find_map(|(i, f)| {
        if f[2] != mm || f[3] != "/" || rows[i + 1..].iter().any(|g| g[4] == f[4]) {
            return None;
        }
        Some((PathBuf::from(unescape(f[4])), f[5].split(',').any(|o| o == "rw")))
    })
}

/// Whether something is mounted on `dir` (per mountinfo).
fn is_mount_point(mountinfo: &str, dir: &str) -> bool {
    mountinfo.lines().any(|l| l.split(' ').nth(4).is_some_and(|m| unescape(m) == dir))
}

fn mountinfo() -> String {
    fs::read_to_string("/proc/self/mountinfo").unwrap_or_default()
}

/// Where `dev` is mounted already (and still visible there), and whether
/// read-write.
fn mounted_at(dev: &str) -> Option<(PathBuf, bool)> {
    find_mount(&mountinfo(), &majmin(rdev(dev)?)).filter(|(at, _)| is_mounted_here(dev, at))
}

/// The filesystem mounted on `dir` is the one on `dev`.
fn is_mounted_here(dev: &str, dir: &Path) -> bool {
    match (rdev(dev), fs::metadata(dir)) {
        (Some(r), Ok(md)) => md.dev() == r,
        _ => false,
    }
}

/// Unmount everything stacked on `dir` (a probe left behind by an earlier
/// umount that failed, e.g. busy); lazily as the last resort.
fn clear_mount_point(dir: &str) {
    for _ in 0..4 {
        if !is_mount_point(&mountinfo(), dir) {
            return;
        }
        umount(dir);
    }
    while is_mount_point(&mountinfo(), dir) {
        let ok = Command::new("umount").args(["-l", dir]).status().is_ok_and(|s| s.success());
        if !ok {
            eprintln!("tb323fu-helperd: cannot unmount {dir}");
            return;
        }
    }
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
    clear_mount_point(UFS_MNT);
    mount(&dev, UFS_MNT, write)?;
    if !is_mounted_here(&dev, Path::new(UFS_MNT)) {
        clear_mount_point(UFS_MNT);
        return Err(format!("{UFS_MNT} does not hold {dev} after mounting it"));
    }
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
    clear_mount_point(UFS_MNT);
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

/// Mount `dev` read-only on the probe point and run `f` on it -- only when
/// the probe point then really holds `dev` (a stale mount left there must
/// never be read as this root).
fn probe<T>(dev: &str, f: impl FnOnce(&Path) -> T) -> Option<T> {
    clear_mount_point(PROBE_MNT);
    mount(dev, PROBE_MNT, false).ok()?;
    let dir = Path::new(PROBE_MNT);
    let r = if is_mounted_here(dev, dir) {
        Some(f(dir))
    } else {
        eprintln!("tb323fu-helperd: {PROBE_MNT} does not hold {dev} after mounting it");
        None
    };
    clear_mount_point(PROBE_MNT);
    r
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
            } else if let Some(r) = probe(&dev, &look) {
                r
            } else {
                // e.g. an ext4 journal left by an unclean shutdown: a read-only
                // mount without replay ("noload") refuses it. Not "empty".
                (
                    String::new(),
                    "unknown".to_string(),
                    vec!["could not be mounted read-only to look: unclean shutdown? Boot it once or check the filesystem (not readable)".to_string()],
                )
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
    fn mount_lookup() {
        // an Ubuntu root on mmcblk0p2 (179:2) inside the daemon's namespace
        // (ProtectSystem=strict: bind mounts of its own subdirectories), the
        // SD Debian root (179:1) bind-mounted from a subdirectory only, the
        // Fedora root (179:5) automounted with a blank in the path, and a
        // stale probe mount of 179:1 under a later one of 179:3
        let mi = "\
24 1 179:2 / / ro,relatime shared:1 - ext4 /dev/mmcblk0p2 rw\n\
25 24 179:2 /var/lib /var/lib rw,relatime shared:1 - ext4 /dev/mmcblk0p2 rw\n\
26 24 0:5 / /dev rw,nosuid shared:2 - devtmpfs devtmpfs rw\n\
27 24 0:40 / /tmp rw,nosuid - tmpfs tmpfs rw\n\
30 24 179:1 /srv/x /srv/x ro,relatime - ext4 /dev/mmcblk0p1 rw\n\
31 24 179:5 / /media/baldur/fedora\\040root rw,nosuid shared:9 - ext4 /dev/mmcblk0p5 rw\n\
40 24 179:1 / /run/tb323fu/probe ro,relatime - ext4 /dev/mmcblk0p1 ro,norecovery\n\
41 40 179:3 / /run/tb323fu/probe ro,relatime - ext4 /dev/mmcblk0p3 ro,norecovery\n";
        assert_eq!(find_mount(mi, "179:2"), Some((PathBuf::from("/"), false)));
        assert_eq!(find_mount(mi, "179:5"), Some((PathBuf::from("/media/baldur/fedora root"), true)));
        // a bind mount of a subdirectory is not the root; a covered mount is not visible
        assert_eq!(find_mount(mi, "179:1"), None);
        assert_eq!(find_mount(mi, "179:3"), Some((PathBuf::from("/run/tb323fu/probe"), false)));
        assert_eq!(find_mount(mi, "179:4"), None);
        // 10-01 on Ubuntu: Fedora on /mnt/t, the SD Debian root mounted over it
        let over = "\
24 1 179:2 / / rw,noatime shared:1 - ext4 /dev/mmcblk0p2 rw\n\
50 24 179:5 / /mnt/t ro,relatime shared:30 - ext4 /dev/mmcblk0p5 ro,norecovery\n\
51 50 179:1 / /mnt/t ro,relatime shared:31 - ext4 /dev/mmcblk0p1 ro,norecovery\n";
        assert_eq!(find_mount(over, "179:5"), None, "Fedora is covered: probe it instead");
        assert_eq!(find_mount(over, "179:1"), Some((PathBuf::from("/mnt/t"), false)));
        assert!(is_mount_point(mi, "/run/tb323fu/probe"));
        assert!(!is_mount_point(mi, "/run/tb323fu/ufs"));
        assert_eq!(majmin((179 << 8) | 5), "179:5");
        assert_eq!(majmin((259u64 << 8) | (0x12345 & 0xff) | ((0x12345u64 & !0xff) << 12)), "259:74565");
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
