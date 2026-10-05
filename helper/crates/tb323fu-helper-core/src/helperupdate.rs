// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! Helper self-update from GitHub Releases (docs/helper.md "Helper updates").
//!
//! A release tagged `helper-vX.Y.Z` carries `tb323fu-helper-X.Y.Z-aarch64.tar.gz`
//! and `SHA256SUMS` (other assets, such as Debian packages, are ignored). The
//! tarball holds `MANIFEST` and `root/<path>`: the files `helper/install.sh`,
//! the settings app's `install.sh` and the GNOME extension install with
//! `PREFIX=/usr/local`. Only paths of [`ALLOWED_FILES`] and [`ALLOWED_DIRS`]
//! are accepted, so a release can never write anything else.
//!
//! Nothing here touches the network or the running system by itself: callers
//! pass the root directory (`/`, or a test tree).
//!
//! ```text
//! /var/lib/tb323fu/helper/
//!   staged/<version>/   tarball, SHA256SUMS(.minisig), verified, tree/{MANIFEST,root/...}
//!   current.manifest    the files the last update installed
//!   prev/               MANIFEST + root/...: the files before the last update
//!                       (`absent` lines: paths that did not exist), for Rollback
//!   backup/             the same while an update runs (Rollback uses it after an
//!                       interrupted update, renamed to interrupted/)
//!   update-state        result=, from=, to=, time=, reason=, backup=
//! ```

use crate::kernel::{safe_name, sha_hex, url_ok, version_cmp, Asset, Res, Source, SIG_ASSET, SUMS_ASSET};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const HELPER_DIR: &str = "/var/lib/tb323fu/helper";
pub const PREFIX: &str = "/usr/local";
pub const ARCH: &str = "aarch64";
pub const TAG_PREFIX: &str = "helper-v";
pub const MAX_TARBALL: u64 = 64 << 20;
pub const MAX_TREE: u64 = 192 << 20;
pub const MAX_FILES: usize = 256;
pub const MAX_MANIFEST: usize = 64 << 10;
pub const STATE_FILE: &str = "update-state";
pub const CURRENT_MANIFEST: &str = "current.manifest";
pub const NEW_SUFFIX: &str = ".tb323fu-new";

/// What a release may install (`PREFIX=/usr/local`; systemd, D-Bus and polkit
/// read these from /etc and /usr/share, as `install.sh` puts them).
pub const ALLOWED_FILES: &[&str] = &[
    "/usr/local/libexec/tb323fu-helperd",
    "/usr/local/libexec/tb323fu-kernel-fetch",
    "/usr/local/bin/tb323fu-ctl",
    "/usr/local/bin/tb323fu-settings",
    "/usr/local/bin/tb323fu-mesa",
    "/etc/profile.d/tb323fu-mesa.sh",
    "/etc/systemd/system/tb323fu-helperd.service",
    "/etc/systemd/system/tb323fu-kernel-fetch.service",
    "/etc/dbus-1/system.d/io.github.joonhoekim.OpenDeviceHelper1.conf",
    "/usr/share/dbus-1/system-services/io.github.joonhoekim.OpenDeviceHelper1.service",
    "/usr/share/polkit-1/actions/io.github.joonhoekim.opendevicehelper.policy",
    "/usr/local/share/applications/io.github.joonhoekim.OpenDeviceHelper.desktop",
    "/usr/local/share/icons/hicolor/scalable/apps/io.github.joonhoekim.OpenDeviceHelper.svg",
    "/usr/local/share/metainfo/io.github.joonhoekim.OpenDeviceHelper.metainfo.xml",
];
pub const ALLOWED_DIRS: &[&str] = &["/usr/local/share/doc/tb323fu-helper/", "/usr/local/share/gnome-shell/extensions/tb323fu@joonhoekim.github.io/"];
/// Every release carries these.
pub const REQUIRED_FILES: &[&str] = &["/usr/local/libexec/tb323fu-helperd", "/usr/local/bin/tb323fu-ctl", "/etc/systemd/system/tb323fu-helperd.service"];

pub fn tarball_name(version: &str) -> String {
    format!("tb323fu-helper-{version}-{ARCH}.tar.gz")
}

/// `0.3.0`, `0.2.1-test`: a digit first, then digits, letters, `.`, `-`, `+`.
pub fn safe_version(v: &str) -> bool {
    !v.is_empty() && v.len() <= 40 && v.starts_with(|c: char| c.is_ascii_digit()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
}

fn safe_component(c: &str) -> bool {
    !c.is_empty() && c != "." && c != ".." && c.len() <= 100 && c.bytes().all(|b| b.is_ascii_alphanumeric() || b"._@+-".contains(&b))
}

/// An absolute path made of plain components.
pub fn safe_path(p: &str) -> bool {
    p.len() <= 240 && p.starts_with('/') && p[1..].split('/').all(safe_component)
}

pub fn path_allowed(p: &str) -> bool {
    safe_path(p) && (ALLOWED_FILES.contains(&p) || ALLOWED_DIRS.iter().any(|d| p.len() > d.len() && p.starts_with(d)))
}

/// `root` + an absolute path.
pub fn under(root: &Path, p: &str) -> PathBuf {
    root.join(p.trim_start_matches('/'))
}

// ------------------------------------------------------------------ releases

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    url: String,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// A `helper-vX.Y.Z` release.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HelperRelease {
    pub tag: String,
    pub version: String,
    pub title: String,
    pub prerelease: bool,
    pub published: String,
    pub notes: String,
    pub notes_url: String,
    /// the self-update asset set (None: a release for packages only)
    pub tarball: Option<Asset>,
    pub sums: Option<Asset>,
    pub sig: Option<Asset>,
    /// from `<!-- tb323fu: min_kernel=tNN min_platform=X -->` in the notes
    pub min_kernel: String,
    pub min_platform: String,
}

impl HelperRelease {
    pub fn installable(&self) -> bool {
        self.tarball.is_some() && self.sums.is_some()
    }
}

/// The helper releases in an API release list (drafts left out; assets
/// whose URL is not under the source are ignored, unknown assets too).
pub fn parse_helper_releases(data: &[u8], src: &Source) -> Res<Vec<HelperRelease>> {
    let list: Vec<GhRelease> = serde_json::from_slice(data).map_err(|e| format!("release list from {}: {e}", src.name()))?;
    let prefix = src.asset_prefix();
    let mut out = Vec::new();
    for r in list {
        let Some(v) = r.tag_name.strip_prefix(TAG_PREFIX) else { continue };
        if r.draft || !safe_version(v) {
            continue;
        }
        let usable = |a: &&GhAsset| safe_name(&a.name) && a.url.starts_with(&prefix) && url_ok(&a.url) && a.size > 0;
        let asset = |name: &str, max: u64| {
            r.assets.iter().filter(usable).find(|a| a.name == name && a.size <= max).map(|a| Asset {
                name: a.name.clone(),
                size: a.size,
                url: a.url.clone(),
                digest: a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).filter(|d| d.len() == 64).unwrap_or("").to_string(),
            })
        };
        let notes = r.body.clone().unwrap_or_default();
        out.push(HelperRelease {
            tag: r.tag_name.clone(),
            version: v.to_string(),
            title: r.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            prerelease: r.prerelease,
            published: r.published_at.clone().unwrap_or_default(),
            min_kernel: crate::kernel::requirement(&notes, "min_kernel"),
            min_platform: crate::kernel::requirement(&notes, "min_platform"),
            notes,
            notes_url: r.html_url.clone(),
            tarball: asset(&tarball_name(v), MAX_TARBALL),
            sums: asset(SUMS_ASSET, crate::kernel::MAX_SUMS_FILE),
            sig: asset(SIG_ASSET, 64 << 10),
        });
    }
    Ok(out)
}

/// The newest release (not a pre-release) newer than `running`.
pub fn newest<'a>(rels: &'a [HelperRelease], running: &str) -> Option<&'a HelperRelease> {
    rels.iter().filter(|r| !r.prerelease && version_cmp(&r.version, running).is_gt()).max_by(|a, b| version_cmp(&a.version, &b.version))
}

/// The release's needs against this system. Unknown values (a development
/// kernel without `-tNN`, no platform-version file, no `getconf`) pass.
pub fn requirements(r: &HelperRelease, min_glibc: &str, platform: Option<&str>, kernel_serial: u64, glibc: Option<&str>) -> Res<()> {
    if !r.min_kernel.is_empty() && kernel_serial > 0 {
        let need: u64 = r.min_kernel.trim_start_matches('t').parse().unwrap_or(0);
        if kernel_serial < need {
            return Err(format!("helper {} needs kernel t{need} or newer (running t{kernel_serial}); update the kernel first", r.version));
        }
    }
    if let (false, Some(p)) = (r.min_platform.is_empty(), platform) {
        if version_cmp(p, &r.min_platform).is_lt() {
            return Err(format!("helper {} needs tb323fu-platform {} or newer (installed: {p}); update it first", r.version, r.min_platform));
        }
    }
    if let (false, Some(g)) = (min_glibc.is_empty(), glibc) {
        if version_cmp(g, min_glibc).is_lt() {
            return Err(format!("helper {} is built for glibc {min_glibc} or newer; this system has {g}", r.version));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ manifest

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub mode: u32,
    pub size: u64,
}

/// `MANIFEST` of a release, or of a snapshot of what was there before.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Manifest {
    pub version: String,
    pub arch: String,
    pub prefix: String,
    pub min_glibc: String,
    pub commit: String,
    pub files: Vec<Entry>,
    /// paths that did not exist (snapshots only)
    pub absent: Vec<String>,
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Manifest {
    pub fn parse(t: &str) -> Res<Manifest> {
        let mut m = Manifest::default();
        let mut format = false;
        let mut seen = BTreeSet::new();
        for (n, l) in t.lines().enumerate() {
            let l = l.trim_end_matches('\r');
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let bad = || format!("MANIFEST line {}: {l}", n + 1);
            if let Some((k, v)) = l.split_once('=').filter(|(k, _)| !k.contains(' ')) {
                match k {
                    "format" if v == "1" => format = true,
                    "format" => return Err(format!("MANIFEST format {v} is not known to this helper")),
                    "version" if safe_version(v) => m.version = v.into(),
                    "arch" => m.arch = v.into(),
                    "prefix" => m.prefix = v.into(),
                    "min_glibc" => m.min_glibc = v.into(),
                    "commit" => m.commit = v.chars().filter(|c| c.is_ascii_alphanumeric()).take(64).collect(),
                    _ => {}
                }
                continue;
            }
            let f: Vec<&str> = l.split(' ').collect();
            let path = match f.as_slice() {
                ["file", sha, mode, size, path] => {
                    let mode = u32::from_str_radix(mode, 8).map_err(|_| bad())?;
                    let size = size.parse().map_err(|_| bad())?;
                    if !is_hex64(sha) || mode & !0o777 != 0 {
                        return Err(bad());
                    }
                    m.files.push(Entry { path: path.to_string(), sha256: sha.to_string(), mode, size });
                    *path
                }
                ["absent", path] => {
                    m.absent.push(path.to_string());
                    *path
                }
                _ => return Err(bad()),
            };
            if !path_allowed(path) {
                return Err(format!("MANIFEST names {path}, which a helper release may not install"));
            }
            if !seen.insert(path.to_string()) {
                return Err(format!("MANIFEST names {path} twice"));
            }
        }
        if !format || m.version.is_empty() {
            return Err("MANIFEST has no format=1 or version= line".into());
        }
        Ok(m)
    }

    pub fn to_text(&self) -> String {
        let mut o = format!("# tb323fu-helper release manifest\nformat=1\nversion={}\n", self.version);
        for (k, v) in [("arch", &self.arch), ("prefix", &self.prefix), ("min_glibc", &self.min_glibc), ("commit", &self.commit)] {
            if !v.is_empty() {
                o.push_str(&format!("{k}={v}\n"));
            }
        }
        for e in &self.files {
            o.push_str(&format!("file {} {:o} {} {}\n", e.sha256, e.mode, e.size, e.path));
        }
        for a in &self.absent {
            o.push_str(&format!("absent {a}\n"));
        }
        o
    }

    /// Every path the manifest speaks for (files and absent ones).
    pub fn paths(&self) -> BTreeSet<String> {
        self.files.iter().map(|e| e.path.clone()).chain(self.absent.iter().cloned()).collect()
    }

    pub fn load(p: &Path) -> Res<Manifest> {
        let t = read_limited(p, MAX_MANIFEST as u64)?;
        Manifest::parse(&String::from_utf8(t).map_err(|_| format!("{}: not text", p.display()))?)
    }
}

fn read_limited(p: &Path, max: u64) -> Res<Vec<u8>> {
    let mut v = Vec::new();
    fs::File::open(p).and_then(|f| f.take(max + 1).read_to_end(&mut v)).map_err(|e| format!("{}: {e}", p.display()))?;
    if v.len() as u64 > max {
        return Err(format!("{}: too large", p.display()));
    }
    Ok(v)
}

// ------------------------------------------------------------------ tarball

/// The regular files of a gzip'd ustar archive: (name, data). Directories are
/// skipped; links, devices, long-name extensions and unsafe names are errors.
pub fn read_tar_gz(data: &[u8], max_total: u64, max_files: usize) -> Res<Vec<(String, Vec<u8>)>> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(data).take(max_total + (1 << 20)).read_to_end(&mut raw).map_err(|e| format!("tarball: {e}"))?;
    if raw.len() as u64 > max_total {
        return Err("tarball: unpacked contents too large".into());
    }
    let octal = |b: &[u8]| -> Res<u64> {
        let s: String = b.iter().take_while(|c| **c != 0).map(|c| *c as char).collect();
        let s = s.trim();
        if s.is_empty() { Ok(0) } else { u64::from_str_radix(s, 8).map_err(|_| "tarball: bad header number".to_string()) }
    };
    let text = |b: &[u8]| -> String { b.iter().take_while(|c| **c != 0).map(|c| *c as char).collect() };
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 512 <= raw.len() {
        let h = &raw[off..off + 512];
        if h.iter().all(|b| *b == 0) {
            break;
        }
        let sum: u64 = h.iter().enumerate().map(|(i, b)| if (148..156).contains(&i) { 32 } else { *b as u64 }).sum();
        if octal(&h[148..156])? != sum {
            return Err("tarball: header checksum mismatch".into());
        }
        if &h[257..262] != b"ustar" {
            return Err("tarball: not a ustar archive".into());
        }
        let (name, prefix) = (text(&h[0..100]), text(&h[345..500]));
        let mut name = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        while let Some(n) = name.strip_prefix("./") {
            name = n.to_string();
        }
        let size = octal(&h[124..136])? as usize;
        let typ = h[156];
        off += 512;
        let end = off.checked_add(size).filter(|e| *e <= raw.len()).ok_or("tarball: truncated")?;
        match typ {
            b'0' | 0 => {
                let n = name.trim_end_matches('/');
                if n.is_empty() || !n.split('/').all(safe_component) {
                    return Err(format!("tarball: unsafe name {name:?}"));
                }
                if out.len() >= max_files {
                    return Err("tarball: too many files".into());
                }
                out.push((n.to_string(), raw[off..end].to_vec()));
            }
            b'5' => {}
            t => return Err(format!("tarball: {name:?} is not a regular file or directory (type {:?})", t as char)),
        }
        off = end + (512 - size % 512) % 512;
    }
    Ok(out)
}

fn write_mode(p: &Path, data: &[u8], mode: u32) -> Res<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let _ = fs::remove_file(p);
    let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    f.write_all(data).and_then(|_| f.sync_all()).map_err(|e| format!("{}: {e}", p.display()))?;
    fs::set_permissions(p, fs::Permissions::from_mode(mode)).map_err(|e| format!("{}: {e}", p.display()))
}

/// Check a downloaded tarball (size, SHA256SUMS, GitHub's digest), unpack it
/// into `out/` (`MANIFEST`, `root/...`) and check every file against the
/// manifest. Returns the manifest.
pub fn unpack_release(r: &HelperRelease, sums: &BTreeMap<String, String>, tgz: &[u8], out: &Path) -> Res<Manifest> {
    let a = r.tarball.as_ref().ok_or_else(|| format!("{} has no {}", r.tag, tarball_name(&r.version)))?;
    if tgz.len() as u64 != a.size {
        return Err(format!("{}: {} bytes, the release says {}", a.name, tgz.len(), a.size));
    }
    let sha = sha_hex(tgz);
    if sums.get(&a.name) != Some(&sha) {
        return Err(format!("{}: SHA-256 does not match SHA256SUMS (a damaged download?)", a.name));
    }
    if !a.digest.is_empty() && a.digest != sha {
        return Err(format!("{}: SHA-256 does not match the digest GitHub reports", a.name));
    }
    let files = read_tar_gz(tgz, MAX_TREE, MAX_FILES)?;
    let mtext = files.iter().find(|(n, _)| n == "MANIFEST").map(|(_, d)| d).ok_or("the tarball has no MANIFEST")?;
    if mtext.len() > MAX_MANIFEST {
        return Err("MANIFEST too large".into());
    }
    let m = Manifest::parse(std::str::from_utf8(mtext).map_err(|_| "MANIFEST is not text")?)?;
    if m.version != r.version {
        return Err(format!("the tarball is helper {}, the release is {}", m.version, r.version));
    }
    if m.arch != ARCH || m.prefix != PREFIX || !m.absent.is_empty() {
        return Err(format!("the tarball is for {} with prefix {}, not {ARCH} {PREFIX}", m.arch, m.prefix));
    }
    if m.files.iter().any(|e| e.mode != 0o644 && e.mode != 0o755) {
        return Err("MANIFEST: file modes must be 644 or 755".into());
    }
    for req in REQUIRED_FILES {
        if !m.files.iter().any(|e| e.path == *req) {
            return Err(format!("the tarball has no {req}"));
        }
    }
    let mut want: BTreeMap<&str, &Entry> = m.files.iter().map(|e| (e.path.as_str(), e)).collect();
    let _ = fs::remove_dir_all(out);
    for (n, d) in &files {
        if n == "MANIFEST" {
            continue;
        }
        let Some(rel) = n.strip_prefix("root/") else { return Err(format!("the tarball has {n} outside root/")) };
        let p = format!("/{rel}");
        let e = want.remove(p.as_str()).ok_or_else(|| format!("the tarball has {p}, which MANIFEST does not list"))?;
        if d.len() as u64 != e.size || sha_hex(d) != e.sha256 {
            return Err(format!("{p} does not match MANIFEST"));
        }
        write_mode(&under(&out.join("root"), &p), d, e.mode)?;
    }
    if let Some(p) = want.keys().next() {
        return Err(format!("MANIFEST lists {p}, which the tarball does not have"));
    }
    write_mode(&out.join("MANIFEST"), mtext, 0o644)?;
    Ok(m)
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) -> Res<()> {
    for e in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let t = e.file_type().map_err(|e| e.to_string())?;
        let p = e.path();
        if t.is_dir() {
            walk(&p, base, out)?;
        } else if t.is_file() {
            out.push(format!("/{}", p.strip_prefix(base).unwrap_or(&p).to_string_lossy()));
        } else {
            return Err(format!("{}: not a regular file", p.display()));
        }
    }
    Ok(())
}

/// The files of an unpacked tree (`dir/root`) are exactly the manifest's,
/// with the recorded contents.
pub fn check_tree(dir: &Path, m: &Manifest) -> Res<()> {
    let root = dir.join("root");
    let mut have = Vec::new();
    if root.exists() {
        walk(&root, &root, &mut have)?;
    }
    let want: BTreeSet<String> = m.files.iter().map(|e| e.path.clone()).collect();
    if have.iter().cloned().collect::<BTreeSet<_>>() != want {
        return Err(format!("{}: the files differ from MANIFEST", dir.display()));
    }
    for e in &m.files {
        let d = read_limited(&under(&root, &e.path), MAX_TREE)?;
        if d.len() as u64 != e.size || sha_hex(&d) != e.sha256 {
            return Err(format!("{}: {} changed since it was checked", dir.display(), e.path));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ snapshot and swap

/// Copy what is at `paths` under `root` now into `out/` (`MANIFEST`,
/// `root/...`); missing paths are recorded as absent. Anything that is not a
/// regular file (a directory, a symlink) is refused: nothing is replaced then.
pub fn snapshot(root: &Path, paths: &BTreeSet<String>, out: &Path, version: &str) -> Res<Manifest> {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::remove_dir_all(out);
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut m = Manifest { version: version.into(), arch: ARCH.into(), prefix: PREFIX.into(), ..Default::default() };
    for p in paths {
        if !path_allowed(p) {
            return Err(format!("{p}: not a helper file"));
        }
        let t = under(root, p);
        match fs::symlink_metadata(&t) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => m.absent.push(p.clone()),
            Err(e) => return Err(format!("{}: {e}", t.display())),
            Ok(md) if !md.file_type().is_file() => return Err(format!("{}: not a regular file; not replacing it", t.display())),
            Ok(md) => {
                let d = read_limited(&t, MAX_TREE)?;
                let mode = md.permissions().mode() & 0o777;
                write_mode(&under(&out.join("root"), p), &d, mode)?;
                m.files.push(Entry { path: p.clone(), sha256: sha_hex(&d), mode, size: d.len() as u64 });
            }
        }
    }
    write_mode(&out.join("MANIFEST"), m.to_text().as_bytes(), 0o644)?;
    Ok(m)
}

fn sync_dir(d: &Path) {
    if let Ok(f) = fs::File::open(d) {
        let _ = f.sync_all();
    }
}

/// Put the files of `src/root` (checked against `m`) in place under `root`
/// and delete `remove` (and `m.absent`). All new files are written next to
/// their targets first (`*.tb323fu-new`), then renamed over them one by one,
/// so each file changes atomically and a failure while writing changes nothing.
pub fn apply(root: &Path, src: &Path, m: &Manifest, remove: &BTreeSet<String>) -> Res<()> {
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let res = (|| {
        for e in &m.files {
            let t = under(root, &e.path);
            if fs::symlink_metadata(&t).is_ok_and(|md| !md.file_type().is_file()) {
                return Err(format!("{}: not a regular file; not replacing it", t.display()));
            }
            let d = read_limited(&under(&src.join("root"), &e.path), MAX_TREE)?;
            if d.len() as u64 != e.size || sha_hex(&d) != e.sha256 {
                return Err(format!("{}: does not match MANIFEST", e.path));
            }
            let n = PathBuf::from(format!("{}{NEW_SUFFIX}", t.display()));
            write_mode(&n, &d, e.mode)?;
            staged.push((n, t));
        }
        Ok(())
    })();
    if let Err(e) = res {
        for (n, _) in &staged {
            let _ = fs::remove_file(n);
        }
        return Err(e);
    }
    for (n, t) in &staged {
        fs::rename(n, t).map_err(|e| format!("{}: {e}", t.display()))?;
    }
    for p in remove.iter().chain(m.absent.iter()) {
        if m.files.iter().any(|e| &e.path == p) || !path_allowed(p) {
            continue;
        }
        let t = under(root, p);
        if fs::symlink_metadata(&t).is_ok_and(|md| md.file_type().is_file()) {
            fs::remove_file(&t).map_err(|e| format!("{}: {e}", t.display()))?;
            // the directory of our own (the extension, the docs) once it is empty
            if let Some(d) = ALLOWED_DIRS.iter().find(|d| p.starts_with(*d)) {
                let _ = fs::remove_dir(under(root, d.trim_end_matches('/')));
            }
        }
    }
    let dirs: BTreeSet<PathBuf> = m.files.iter().filter_map(|e| under(root, &e.path).parent().map(Path::to_path_buf)).collect();
    for d in dirs {
        sync_dir(&d);
    }
    Ok(())
}

// ------------------------------------------------------------------ state file

/// `key=value` lines (update-state).
pub fn read_kv(p: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.to_string())))
        .collect()
}

pub fn write_kv(p: &Path, kv: &BTreeMap<String, String>) -> Res<()> {
    let t: String = kv.iter().map(|(k, v)| format!("{k}={}\n", v.replace('\n', " "))).collect();
    let tmp = p.with_extension("tmp");
    write_mode(&tmp, t.as_bytes(), 0o644)?;
    fs::rename(&tmp, p).map_err(|e| format!("{}: {e}", p.display()))
}

// ------------------------------------------------------------------ who owns the helper

/// Who installed the running helper: the self-update works only when nobody did.
#[derive(Debug, Clone, PartialEq)]
pub enum Owner {
    None,
    Dpkg(String),
    Pacman(String),
    Rpm(String),
    Nix,
}

impl Owner {
    pub fn method(&self) -> &'static str {
        match self {
            Owner::None => "self",
            Owner::Dpkg(_) => "dpkg",
            Owner::Pacman(_) => "pacman",
            Owner::Rpm(_) => "rpm",
            Owner::Nix => "nix",
        }
    }

    /// What updates the helper on this system.
    pub fn hint(&self) -> String {
        match self {
            Owner::None => "tb323fu-ctl helper update".into(),
            Owner::Dpkg(p) => format!("the helper belongs to the Debian package {p}: install the new version's .deb files (apt install ./{p}_*.deb)"),
            Owner::Pacman(p) => format!("the helper belongs to the pacman package {p}: build the new version (packaging/arch/PKGBUILD) and pacman -U it"),
            Owner::Rpm(p) => format!("the helper belongs to the RPM package {p}: update it with your package manager"),
            Owner::Nix => "nix flake update tb323fu-linux && sudo nixos-rebuild switch".into(),
        }
    }

    pub fn reason(&self) -> String {
        match self {
            Owner::None => String::new(),
            Owner::Nix => "NixOS: the system configuration updates the helper".into(),
            Owner::Dpkg(p) | Owner::Pacman(p) | Owner::Rpm(p) => format!("installed by {} (package {p})", self.method()),
        }
    }
}

/// The owner of `paths` (the first one a package manager claims). `run(cmd,
/// args)` gives Some((success, stdout)), None when the command is missing.
pub fn owner_of(paths: &[&str], nixos: bool, run: &dyn Fn(&str, &[&str]) -> Option<(bool, String)>) -> Owner {
    if nixos || paths.iter().any(|p| p.starts_with("/nix/store/")) {
        return Owner::Nix;
    }
    let first = |s: &str| s.lines().next().unwrap_or("").trim().to_string();
    for p in paths {
        if let Some((true, o)) = run("dpkg-query", &["-S", p]) {
            let pkg = first(&o).split(':').next().unwrap_or("").trim().to_string();
            if !pkg.is_empty() {
                return Owner::Dpkg(pkg);
            }
        }
        if let Some((true, o)) = run("pacman", &["-Qqo", p]) {
            if !first(&o).is_empty() {
                return Owner::Pacman(first(&o));
            }
        }
        if let Some((true, o)) = run("rpm", &["-qf", "--qf", "%{NAME}", p]) {
            if !first(&o).is_empty() && !o.contains("not owned") {
                return Owner::Rpm(first(&o));
            }
        }
    }
    Owner::None
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::kernel::parse_sums;

    pub struct Tmp(pub PathBuf);
    impl Tmp {
        pub fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("tb323fu-hupd-{tag}-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A ustar archive of (name, data, type) entries, gzip'd.
    pub fn tar_gz(entries: &[(&str, &[u8], u8)]) -> Vec<u8> {
        let mut t = Vec::new();
        for (name, data, typ) in entries {
            let mut h = [0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            h[100..108].copy_from_slice(b"0000644\0");
            h[108..116].copy_from_slice(b"0000000\0");
            h[116..124].copy_from_slice(b"0000000\0");
            h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
            h[136..148].copy_from_slice(b"00000000000\0");
            h[156] = *typ;
            h[257..263].copy_from_slice(b"ustar\0");
            h[263..265].copy_from_slice(b"00");
            h[148..156].copy_from_slice(b"        ");
            let sum: u32 = h.iter().map(|b| *b as u32).sum();
            h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
            t.extend_from_slice(&h);
            t.extend_from_slice(data);
            t.resize(t.len() + (512 - data.len() % 512) % 512, 0);
        }
        t.resize(t.len() + 1024, 0);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&t).unwrap();
        gz.finish().unwrap()
    }

    /// A release tarball: the given files (path, data, mode) + MANIFEST.
    pub fn release(version: &str, files: &[(&str, &[u8], u32)]) -> (Vec<u8>, Manifest) {
        let m = Manifest {
            version: version.into(),
            arch: ARCH.into(),
            prefix: PREFIX.into(),
            min_glibc: "2.36".into(),
            files: files.iter().map(|(p, d, mode)| Entry { path: p.to_string(), sha256: sha_hex(d), mode: *mode, size: d.len() as u64 }).collect(),
            ..Default::default()
        };
        let text = m.to_text();
        let names: Vec<String> = files.iter().map(|(p, _, _)| format!("root{p}")).collect();
        let mut e: Vec<(&str, &[u8], u8)> = vec![("MANIFEST", text.as_bytes(), b'0'), ("root/", b"", b'5')];
        for (i, (_, d, _)) in files.iter().enumerate() {
            e.push((names[i].as_str(), d, b'0'));
        }
        (tar_gz(&e), m)
    }

    const D: &str = "/usr/local/libexec/tb323fu-helperd";
    const C: &str = "/usr/local/bin/tb323fu-ctl";
    const U: &str = "/etc/systemd/system/tb323fu-helperd.service";
    const P: &str = "/usr/share/polkit-1/actions/io.github.joonhoekim.opendevicehelper.policy";
    const X: &str = "/usr/local/share/gnome-shell/extensions/tb323fu@joonhoekim.github.io/extension.js";

    fn rel_for(tgz: &[u8], version: &str) -> (HelperRelease, BTreeMap<String, String>) {
        let name = tarball_name(version);
        let r = HelperRelease {
            tag: format!("helper-v{version}"),
            version: version.into(),
            tarball: Some(Asset { name: name.clone(), size: tgz.len() as u64, url: String::new(), digest: sha_hex(tgz) }),
            sums: Some(Asset::default()),
            ..Default::default()
        };
        let sums = parse_sums(&format!("{}  {name}\n{}  tb323fu-helper_0.3.0_arm64.deb\n", sha_hex(tgz), "a".repeat(64)));
        (r, sums)
    }

    #[test]
    fn paths_and_versions() {
        assert!(path_allowed(D) && path_allowed(X) && path_allowed("/usr/local/share/doc/tb323fu-helper/helper.toml.example"));
        assert!(!path_allowed("/etc/shadow") && !path_allowed("/usr/local/share/doc/tb323fu-helper/"));
        assert!(!path_allowed("/usr/local/share/doc/tb323fu-helper/../../../../etc/shadow"));
        assert!(!path_allowed("/usr/local/libexec/tb323fu-helperd/"), "trailing slash");
        assert!(!path_allowed("/usr/lib/systemd/system/tb323fu-helperd.service"), "a package's place, not ours");
        assert!(safe_version("0.2.1-test") && !safe_version("../1") && !safe_version("v1"));
        assert_eq!(tarball_name("0.3.0"), "tb323fu-helper-0.3.0-aarch64.tar.gz");
    }

    #[test]
    fn manifest_roundtrip_and_refusals() {
        let (_, m) = release("0.3.0", &[(D, b"daemon", 0o755), (U, b"[Unit]", 0o644)]);
        let mut m2 = m.clone();
        m2.absent.push(C.into());
        assert_eq!(Manifest::parse(&m2.to_text()).unwrap(), m2);
        assert_eq!(m2.paths().len(), 3);
        let t = m.to_text();
        assert!(Manifest::parse(&t.replace(D, "/etc/passwd")).unwrap_err().contains("may not install"));
        assert!(Manifest::parse(&format!("{t}file {} 755 6 {D}\n", "b".repeat(64))).unwrap_err().contains("twice"));
        assert!(Manifest::parse(&t.replace("format=1", "format=2")).unwrap_err().contains("format 2"));
        assert!(Manifest::parse(&t.replace(" 755 ", " 4755 ")).is_err(), "setuid");
    }

    #[test]
    fn release_list() {
        let src = Source::parse("github:o/r", "https://api.github.com").unwrap();
        let a = |id: u32, name: &str| serde_json::json!({"name": name, "size": 10, "url": format!("https://api.github.com/repos/o/r/releases/assets/{id}")});
        let list = serde_json::json!([
            {"tag_name": "helper-v0.4.0", "prerelease": true, "assets": [a(1, "tb323fu-helper-0.4.0-aarch64.tar.gz"), a(2, "SHA256SUMS")]},
            {"tag_name": "helper-v0.3.0", "body": "x\n<!-- tb323fu: min_kernel=t37 min_platform=0.2.0 -->", "assets": [
                a(3, "tb323fu-helper_0.3.0_arm64.deb"), a(4, "tb323fu-helper-0.3.0-aarch64.tar.gz"), a(5, "SHA256SUMS")]},
            {"tag_name": "helper-v0.2.0", "prerelease": true, "assets": [a(6, "tb323fu-helper_0.2.0_arm64.deb"), a(7, "SHA256SUMS")]},
            {"tag_name": "helper-v0.2.5", "draft": true, "assets": []},
            {"tag_name": "kernel-t38", "assets": []}
        ]);
        let r = parse_helper_releases(list.to_string().as_bytes(), &src).unwrap();
        assert_eq!(r.iter().map(|x| x.version.as_str()).collect::<Vec<_>>(), ["0.4.0", "0.3.0", "0.2.0"]);
        assert!(r[1].installable() && !r[2].installable(), "packages only: not installable");
        assert_eq!((r[1].min_kernel.as_str(), r[1].min_platform.as_str()), ("t37", "0.2.0"));
        assert_eq!(newest(&r, "0.2.0").unwrap().version, "0.3.0", "pre-releases are not offered");
        assert!(newest(&r, "0.3.0").is_none());
        assert!(requirements(&r[1], "", None, 36, None).unwrap_err().contains("kernel t37"));
        assert!(requirements(&r[1], "", None, 0, None).is_ok(), "a development kernel passes");
        assert!(requirements(&r[1], "", Some("0.1.9"), 38, None).unwrap_err().contains("platform"));
        assert!(requirements(&r[1], "2.41", Some("0.2.0"), 38, Some("2.36")).unwrap_err().contains("glibc"));
        assert!(requirements(&r[1], "2.41", Some("0.2.0"), 38, Some("2.41")).is_ok());
    }

    #[test]
    fn unpack_checks_everything() {
        let t = Tmp::new("unpack");
        let (tgz, m) = release("0.3.0", &[(D, b"daemon", 0o755), (C, b"ctl", 0o755), (U, b"[Unit]", 0o644), (X, b"js", 0o644)]);
        let (r, sums) = rel_for(&tgz, "0.3.0");
        let out = t.0.join("tree");
        assert_eq!(unpack_release(&r, &sums, &tgz, &out).unwrap(), m);
        assert_eq!(fs::read(under(&out.join("root"), D)).unwrap(), b"daemon");
        check_tree(&out, &m).unwrap();
        fs::write(under(&out.join("root"), C), b"changed").unwrap();
        assert!(check_tree(&out, &m).unwrap_err().contains("changed"));
        fs::write(out.join("root/extra"), b"").unwrap();
        assert!(check_tree(&out, &m).unwrap_err().contains("differ"));

        let mut bad = tgz.clone();
        let n = bad.len();
        bad[n / 2] ^= 1;
        assert!(unpack_release(&r, &sums, &bad, &out).unwrap_err().contains("SHA256SUMS"));
        let (wrong, _) = rel_for(&tgz, "0.3.1");
        assert!(unpack_release(&HelperRelease { version: "0.3.1".into(), ..wrong.clone() }, &parse_sums(&format!("{}  {}\n", sha_hex(&tgz), tarball_name("0.3.1"))), &tgz, &out)
            .unwrap_err().contains("is helper 0.3.0"));

        // a file outside the allow-list, a symlink, a missing daemon, an unlisted file
        let evil = tar_gz(&[("MANIFEST", m.to_text().replace(X, "/etc/cron.d/x").as_bytes(), b'0')]);
        let (r2, s2) = rel_for(&evil, "0.3.0");
        assert!(unpack_release(&r2, &s2, &evil, &out).unwrap_err().contains("may not install"));
        let link = tar_gz(&[("MANIFEST", m.to_text().as_bytes(), b'0'), ("root/usr/local/bin/tb323fu-ctl", b"", b'2')]);
        let (r3, s3) = rel_for(&link, "0.3.0");
        assert!(unpack_release(&r3, &s3, &link, &out).unwrap_err().contains("not a regular file"));
        let (nod, _) = release("0.3.0", &[(C, b"ctl", 0o755), (U, b"u", 0o644)]);
        let (r4, s4) = rel_for(&nod, "0.3.0");
        assert!(unpack_release(&r4, &s4, &nod, &out).unwrap_err().contains("no /usr/local/libexec/tb323fu-helperd"));
        let mt = m.to_text();
        let mut e: Vec<(&str, &[u8], u8)> = vec![("MANIFEST", mt.as_bytes(), b'0')];
        e.extend([("root/usr/local/libexec/tb323fu-helperd", &b"daemon"[..], b'0'), ("root/usr/local/bin/tb323fu-ctl", b"ctl", b'0'),
            ("root/etc/systemd/system/tb323fu-helperd.service", b"[Unit]", b'0'), ("root/usr/local/share/gnome-shell/extensions/tb323fu@joonhoekim.github.io/extension.js", b"js", b'0'),
            ("root/usr/local/bin/other", b"x", b'0')]);
        let more = tar_gz(&e);
        let (r5, s5) = rel_for(&more, "0.3.0");
        assert!(unpack_release(&r5, &s5, &more, &out).unwrap_err().contains("does not list"));
        assert!(read_tar_gz(&tar_gz(&[("../etc/passwd", b"x", b'0')]), 1 << 20, 10).unwrap_err().contains("unsafe"));
    }

    #[test]
    fn snapshot_apply_and_back() {
        let t = Tmp::new("swap");
        let root = t.0.join("root");
        let w = |p: &str, d: &[u8]| {
            let f = under(&root, p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, d).unwrap();
        };
        // the installed 0.2.0 (install.sh), an unrelated file next to it
        w(D, b"old daemon");
        w(C, b"old ctl");
        w(U, b"old unit");
        w("/usr/local/bin/other-tool", b"not ours");
        let (tgz, m) = release("0.3.0", &[(D, b"new daemon", 0o755), (C, b"new ctl", 0o755), (U, b"new unit", 0o644), (P, b"<policy/>", 0o644), (X, b"js", 0o644)]);
        let (r, sums) = rel_for(&tgz, "0.3.0");
        let tree = t.0.join("tree");
        unpack_release(&r, &sums, &tgz, &tree).unwrap();

        let prev = snapshot(&root, &m.paths(), &t.0.join("prev"), "0.2.0").unwrap();
        assert_eq!((prev.files.len(), prev.absent.clone()), (3, vec![X.to_string(), P.to_string()]));
        apply(&root, &tree, &m, &BTreeSet::new()).unwrap();
        assert_eq!(fs::read(under(&root, D)).unwrap(), b"new daemon");
        assert_eq!(fs::read(under(&root, P)).unwrap(), b"<policy/>");
        assert!(!under(&root, &format!("{D}{NEW_SUFFIX}")).exists());
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(under(&root, D)).unwrap().permissions().mode() & 0o777, 0o755);
        }

        // back: the snapshot (its absent policy is deleted again)
        let fwd = snapshot(&root, &(&m.paths() | &prev.paths()), &t.0.join("fwd"), "0.3.0").unwrap();
        apply(&root, &t.0.join("prev"), &prev, &m.paths()).unwrap();
        assert_eq!(fs::read(under(&root, D)).unwrap(), b"old daemon");
        assert!(!under(&root, P).exists());
        assert!(!under(&root, X).parent().unwrap().exists(), "the extension directory goes with its last file");
        assert_eq!(fs::read(under(&root, "/usr/local/bin/other-tool")).unwrap(), b"not ours");
        // and forward again
        apply(&root, &t.0.join("fwd"), &fwd, &BTreeSet::new()).unwrap();
        assert_eq!(fs::read(under(&root, C)).unwrap(), b"new ctl");

        // a symlink in the way: refused before anything changes
        fs::remove_file(under(&root, C)).unwrap();
        std::os::unix::fs::symlink("/bin/true", under(&root, C)).unwrap();
        assert!(snapshot(&root, &m.paths(), &t.0.join("x"), "0.3.0").unwrap_err().contains("not a regular file"));
        let before = fs::read(under(&root, D)).unwrap();
        assert!(apply(&root, &t.0.join("prev"), &prev, &BTreeSet::new()).unwrap_err().contains("not a regular file"));
        assert_eq!(fs::read(under(&root, D)).unwrap(), before, "nothing replaced");
        assert!(!under(&root, &format!("{D}{NEW_SUFFIX}")).exists(), "staged files cleaned up");
        // a damaged source: refused while staging
        fs::remove_file(under(&root, C)).unwrap();
        fs::write(under(&tree.join("root"), U), b"tampered").unwrap();
        assert!(apply(&root, &tree, &m, &BTreeSet::new()).unwrap_err().contains("does not match"));
        assert_eq!(fs::read(under(&root, D)).unwrap(), before);
    }

    #[test]
    fn owners() {
        let none = |_: &str, _: &[&str]| -> Option<(bool, String)> { None };
        assert_eq!(owner_of(&[D], false, &none), Owner::None);
        assert_eq!(owner_of(&[D], true, &none), Owner::Nix);
        assert_eq!(owner_of(&["/nix/store/abc-helper/libexec/tb323fu-helperd"], false, &none), Owner::Nix);
        let dpkg = |c: &str, a: &[&str]| -> Option<(bool, String)> {
            (c == "dpkg-query").then(|| if a[1].starts_with("/usr/libexec") { (true, format!("tb323fu-helper: {}\n", a[1])) } else { (false, String::new()) })
        };
        assert_eq!(owner_of(&["/usr/local/libexec/tb323fu-helperd"], false, &dpkg), Owner::None);
        let o = owner_of(&["/usr/libexec/tb323fu/tb323fu-helperd"], false, &dpkg);
        assert_eq!(o, Owner::Dpkg("tb323fu-helper".into()));
        assert!(o.hint().contains("apt install ./tb323fu-helper_") && o.reason().contains("dpkg"));
        let rpm = |c: &str, _: &[&str]| -> Option<(bool, String)> { (c == "rpm").then(|| (false, "file /x is not owned by any package\n".into())) };
        assert_eq!(owner_of(&[D], false, &rpm), Owner::None);
        let pac = |c: &str, _: &[&str]| -> Option<(bool, String)> { (c == "pacman").then(|| (true, "tb323fu-helper\n".into())) };
        assert_eq!(owner_of(&[D], false, &pac).method(), "pacman");
    }

    #[test]
    fn state_file() {
        let t = Tmp::new("kv");
        let p = t.0.join(STATE_FILE);
        let kv: BTreeMap<String, String> = [("result", "ok"), ("reason", "a\nb")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        write_kv(&p, &kv).unwrap();
        assert_eq!(read_kv(&p).get("reason").map(String::as_str), Some("a b"));
    }
}
