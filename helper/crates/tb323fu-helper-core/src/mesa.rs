// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The port's Mesa build from GitHub Releases (docs/notes/mesa-channel-design.md).
//!
//! A release tagged `mesa-<version>` (a date, `2026.10.06`, `.2` for a second
//! one that day) carries `tb323fu-mesa-<version>-aarch64.tar.gz` and
//! `SHA256SUMS`. The tarball holds `MANIFEST` and `tree/<relative path>`; it is
//! unpacked into its own version directory and never touches the
//! distribution's files. Applications pick it up through the environment
//! (`env.conf`, linked from /etc/environment.d and /etc/profile.d) when the
//! channel is switched on, or one at a time through `tb323fu-mesa run`.
//!
//! ```text
//! /var/lib/tb323fu/mesa/
//!   versions/<version>/   MANIFEST, lib/..., share/...
//!   current, previous     symbolic links into versions/
//!   icd/turnip.json       Vulkan ICD manifest (through current/)
//!   icd/opencl/rusticl.icd
//!   env.conf              KEY=VALUE lines, empty when switched off
//!   state                 enabled, trial, tries, max, boot, failed, failed_reason
//!   staged/<version>/     tarball, SHA256SUMS, verified
//! ```
//!
//! Nothing here touches the network; callers pass the root (`/` or a test tree).

use crate::helperupdate::{read_kv, read_tar_gz, safe_version, write_kv};
use crate::kernel::{safe_name, sha_hex, url_ok, version_cmp, Asset, Res, Source, SIG_ASSET, SUMS_ASSET};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MESA_DIR: &str = "var/lib/tb323fu/mesa";
pub const TAG_PREFIX: &str = "mesa-";
pub const ARCH: &str = "aarch64";
pub const MAX_TARBALL: u64 = 256 << 20;
pub const MAX_TREE: u64 = 768 << 20;
pub const MAX_FILES: usize = 64;
pub const MAX_MANIFEST: usize = 16 << 10;
pub const DEFAULT_MAX_TRIES: u32 = 2;
pub const VULKAN_LIB: &str = "lib/libvulkan_freedreno.so";
pub const OPENCL_LIB: &str = "lib/libRusticlOpenCL.so.1";
const TOP_DIRS: &[&str] = &["lib", "share"];

pub fn tarball_name(version: &str) -> String {
    format!("tb323fu-mesa-{version}-{ARCH}.tar.gz")
}

/// `/var/lib/tb323fu/mesa` under `root`.
pub fn dir(root: &Path) -> PathBuf {
    root.join(MESA_DIR)
}

/// The absolute path the system sees for a file under the Mesa directory
/// (what goes into ICD files and `env.conf`, independent of a test root).
fn sys_path(rel: &str) -> String {
    format!("/{MESA_DIR}/{rel}")
}

/// A relative path inside a version tree: `lib/...` or `share/...`, plain components.
pub fn safe_rel(p: &str) -> bool {
    p.len() <= 200
        && TOP_DIRS.iter().any(|d| p.strip_prefix(d).is_some_and(|r| r.starts_with('/')))
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != ".." && c.len() <= 100 && c.bytes().all(|b| b.is_ascii_alphanumeric() || b"._@+-".contains(&b)))
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

/// A `mesa-<version>` release with its asset set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MesaRelease {
    pub tag: String,
    pub version: String,
    pub title: String,
    pub prerelease: bool,
    pub published: String,
    pub notes: String,
    pub notes_url: String,
    pub tarball: Asset,
    pub sums: Asset,
    pub sig: Option<Asset>,
    /// from `<!-- tb323fu: min_helper=X -->` in the notes
    pub min_helper: String,
}

/// The Mesa releases in an API release list. Drafts, unsafe versions and
/// releases without the tarball and `SHA256SUMS` are left out.
pub fn parse_releases(data: &[u8], src: &Source) -> Res<Vec<MesaRelease>> {
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
        let (Some(tarball), Some(sums)) = (asset(&tarball_name(v), MAX_TARBALL), asset(SUMS_ASSET, crate::kernel::MAX_SUMS_FILE)) else { continue };
        let notes = r.body.clone().unwrap_or_default();
        out.push(MesaRelease {
            tag: r.tag_name.clone(),
            version: v.to_string(),
            title: r.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            prerelease: r.prerelease,
            published: r.published_at.clone().unwrap_or_default(),
            min_helper: crate::kernel::requirement(&notes, "min_helper"),
            notes,
            notes_url: r.html_url.clone(),
            tarball,
            sums,
            sig: asset(SIG_ASSET, 64 << 10),
        });
    }
    Ok(out)
}

/// What a channel offers: the newest release (pre-releases only on testing).
pub fn pick<'a>(rels: &'a [MesaRelease], channel: &str) -> Option<&'a MesaRelease> {
    rels.iter().filter(|r| channel == "testing" || !r.prerelease).max_by(|a, b| version_cmp(&a.version, &b.version).then(a.published.cmp(&b.published)))
}

// ------------------------------------------------------------------ manifest

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub mode: u32,
    pub size: u64,
}

/// `MANIFEST` of a Mesa release.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Manifest {
    pub version: String,
    pub arch: String,
    /// the port's Mesa commit and the upstream commit it is based on
    pub commit: String,
    pub base: String,
    pub mesa_version: String,
    pub min_glibc: String,
    /// `vulkan`, `opencl` (and later `gl`)
    pub drivers: Vec<String>,
    pub files: Vec<Entry>,
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
                let clean = |v: &str| v.chars().filter(|c| c.is_ascii_alphanumeric() || ".-+_".contains(*c)).take(64).collect::<String>();
                match k {
                    "format" if v == "1" => format = true,
                    "format" => return Err(format!("MANIFEST format {v} is not known to this helper")),
                    "version" if safe_version(v) => m.version = v.into(),
                    "arch" => m.arch = clean(v),
                    "commit" => m.commit = clean(v),
                    "base" => m.base = clean(v),
                    "mesa_version" => m.mesa_version = clean(v),
                    "min_glibc" => m.min_glibc = clean(v),
                    "drivers" => m.drivers = v.split(',').map(clean).filter(|d| !d.is_empty()).collect(),
                    _ => {}
                }
                continue;
            }
            let f: Vec<&str> = l.split(' ').collect();
            let ["file", sha, mode, size, path] = f.as_slice() else { return Err(bad()) };
            let mode = u32::from_str_radix(mode, 8).map_err(|_| bad())?;
            let size = size.parse().map_err(|_| bad())?;
            if !is_hex64(sha) || (mode != 0o644 && mode != 0o755) {
                return Err(bad());
            }
            if !safe_rel(path) {
                return Err(format!("MANIFEST names {path}, which a Mesa release may not install"));
            }
            if !seen.insert(path.to_string()) {
                return Err(format!("MANIFEST names {path} twice"));
            }
            m.files.push(Entry { path: path.to_string(), sha256: sha.to_string(), mode, size });
        }
        if !format || m.version.is_empty() {
            return Err("MANIFEST has no format=1 or version= line".into());
        }
        if m.arch != ARCH {
            return Err(format!("the release is for {}, not {ARCH}", m.arch));
        }
        for (d, lib) in [("vulkan", VULKAN_LIB), ("opencl", OPENCL_LIB)] {
            if m.drivers.iter().any(|x| x == d) && !m.files.iter().any(|e| e.path == lib) {
                return Err(format!("MANIFEST says {d} but has no {lib}"));
            }
        }
        if m.drivers.is_empty() {
            return Err("MANIFEST names no drivers".into());
        }
        Ok(m)
    }

    pub fn has(&self, driver: &str) -> bool {
        self.drivers.iter().any(|d| d == driver)
    }

    pub fn load(p: &Path) -> Res<Manifest> {
        let t = fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
        if t.len() > MAX_MANIFEST {
            return Err(format!("{}: too large", p.display()));
        }
        Manifest::parse(&String::from_utf8(t).map_err(|_| format!("{}: not text", p.display()))?)
    }
}

// ------------------------------------------------------------------ unpack, versions

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

/// Check a tarball (size, SHA256SUMS, GitHub's digest), unpack it into
/// `versions/<version>` under `root` (through a temporary directory renamed
/// at the end) and check every file. `expect` is the version the release says.
pub fn unpack(root: &Path, expect: &str, tarball: &Asset, sums: &BTreeMap<String, String>, tgz: &[u8]) -> Res<Manifest> {
    if tgz.len() as u64 != tarball.size {
        return Err(format!("{}: {} bytes, the release says {}", tarball.name, tgz.len(), tarball.size));
    }
    let sha = sha_hex(tgz);
    if sums.get(&tarball.name) != Some(&sha) {
        return Err(format!("{}: SHA-256 does not match SHA256SUMS (a damaged download?)", tarball.name));
    }
    if !tarball.digest.is_empty() && tarball.digest != sha {
        return Err(format!("{}: SHA-256 does not match the digest GitHub reports", tarball.name));
    }
    let files = read_tar_gz(tgz, MAX_TREE, MAX_FILES)?;
    let mtext = files.iter().find(|(n, _)| n == "MANIFEST").map(|(_, d)| d).ok_or("the tarball has no MANIFEST")?;
    if mtext.len() > MAX_MANIFEST {
        return Err("MANIFEST too large".into());
    }
    let m = Manifest::parse(std::str::from_utf8(mtext).map_err(|_| "MANIFEST is not text")?)?;
    if m.version != expect {
        return Err(format!("the tarball is Mesa {}, the release is {expect}", m.version));
    }
    let versions = dir(root).join("versions");
    let tmp = versions.join(format!(".{}.tmp", m.version));
    let _ = fs::remove_dir_all(&tmp);
    let mut want: BTreeMap<&str, &Entry> = m.files.iter().map(|e| (e.path.as_str(), e)).collect();
    for (n, d) in &files {
        if n == "MANIFEST" {
            continue;
        }
        let Some(rel) = n.strip_prefix("tree/") else { return Err(format!("the tarball has {n} outside tree/")) };
        let e = want.remove(rel).ok_or_else(|| format!("the tarball has {rel}, which MANIFEST does not list"))?;
        if d.len() as u64 != e.size || sha_hex(d) != e.sha256 {
            return Err(format!("{rel} does not match MANIFEST"));
        }
        write_mode(&tmp.join(rel), d, e.mode)?;
    }
    if let Some(p) = want.keys().next() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(format!("MANIFEST lists {p}, which the tarball does not have"));
    }
    write_mode(&tmp.join("MANIFEST"), mtext, 0o644)?;
    let dest = versions.join(&m.version);
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&tmp, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(m)
}

/// The version a link (`current`, `previous`) points to.
pub fn link_version(root: &Path, name: &str) -> Option<String> {
    let t = fs::read_link(dir(root).join(name)).ok()?;
    let v = t.file_name()?.to_str()?.to_string();
    (safe_version(&v) && dir(root).join("versions").join(&v).join("MANIFEST").is_file()).then_some(v)
}

fn set_link(root: &Path, name: &str, version: Option<&str>) -> Res<()> {
    let p = dir(root).join(name);
    match version {
        None => match fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", p.display())),
        },
        Some(v) => {
            let tmp = dir(root).join(format!(".{name}.tmp"));
            let _ = fs::remove_file(&tmp);
            std::os::unix::fs::symlink(format!("versions/{v}"), &tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
            fs::rename(&tmp, &p).map_err(|e| format!("{}: {e}", p.display()))
        }
    }
}

/// The installed version directories, newest first.
pub fn versions(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir(root).join("versions"))
        .map(|rd| rd.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| safe_version(n)).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| version_cmp(b, a));
    v
}

/// Make an unpacked version current (the old current becomes previous),
/// rewrite the ICD files, drop versions that are neither, and start a trial
/// when the channel is switched on.
pub fn activate(root: &Path, version: &str) -> Res<()> {
    let m = Manifest::load(&dir(root).join("versions").join(version).join("MANIFEST"))?;
    let old = link_version(root, "current");
    if old.as_deref() != Some(version) {
        set_link(root, "previous", old.as_deref())?;
    }
    set_link(root, "current", Some(version))?;
    write_icds(root, &m)?;
    let keep: BTreeSet<String> = [link_version(root, "current"), link_version(root, "previous")].into_iter().flatten().collect();
    for v in versions(root) {
        if !keep.contains(&v) {
            let _ = fs::remove_dir_all(dir(root).join("versions").join(&v));
        }
    }
    let mut st = State::load(root);
    if st.enabled {
        st.start_trial(version);
        st.save(root)?;
    }
    Ok(())
}

/// Swap current and previous.
pub fn rollback(root: &Path) -> Res<String> {
    let (Some(cur), Some(prev)) = (link_version(root, "current"), link_version(root, "previous")) else {
        return Err("there is no previous Mesa version to go back to".into());
    };
    set_link(root, "current", Some(&prev))?;
    set_link(root, "previous", Some(&cur))?;
    let m = Manifest::load(&dir(root).join("versions").join(&prev).join("MANIFEST"))?;
    write_icds(root, &m)?;
    let mut st = State::load(root);
    st.trial.clear();
    st.tries = 0;
    st.save(root)?;
    Ok(format!("Mesa {prev} is current again ({cur} kept as the previous version)"))
}

// ------------------------------------------------------------------ ICD files and the environment

/// Vulkan ICD manifest and OpenCL vendor file pointing into `current/`.
pub fn write_icds(root: &Path, m: &Manifest) -> Res<()> {
    let icd = dir(root).join("icd");
    let vk = icd.join("turnip.json");
    let cl = icd.join("opencl").join("rusticl.icd");
    if m.has("vulkan") {
        let j = format!(
            "{{\"file_format_version\": \"1.0.1\", \"ICD\": {{\"library_path\": \"{}\", \"api_version\": \"1.4.0\"}}}}\n",
            sys_path(&format!("current/{VULKAN_LIB}"))
        );
        write_mode(&vk, j.as_bytes(), 0o644)?;
    } else {
        let _ = fs::remove_file(&vk);
    }
    if m.has("opencl") {
        write_mode(&cl, format!("{}\n", sys_path(&format!("current/{OPENCL_LIB}"))).as_bytes(), 0o644)?;
    } else {
        let _ = fs::remove_file(&cl);
    }
    Ok(())
}

/// The variables that select the current build for the drivers it has.
pub fn env_vars(root: &Path) -> Vec<(String, String)> {
    let mut v = Vec::new();
    let Some(cur) = link_version(root, "current") else { return v };
    let Ok(m) = Manifest::load(&dir(root).join("versions").join(&cur).join("MANIFEST")) else { return v };
    if m.has("vulkan") {
        let p = sys_path("icd/turnip.json");
        v.push(("VK_DRIVER_FILES".into(), p.clone()));
        v.push(("VK_ICD_FILENAMES".into(), p));
    }
    if m.has("opencl") {
        v.push(("OCL_ICD_VENDORS".into(), sys_path("icd/opencl")));
        v.push(("RUSTICL_ENABLE".into(), "freedreno".into()));
    }
    v
}

/// Write `env.conf`: the variables when the channel is on (and a version is
/// installed), nothing otherwise.
pub fn write_env(root: &Path, on: bool) -> Res<()> {
    let t: String = if on { env_vars(root).iter().map(|(k, v)| format!("{k}={v}\n")).collect() } else { String::new() };
    let p = dir(root).join("env.conf");
    let tmp = dir(root).join(".env.conf.tmp");
    write_mode(&tmp, t.as_bytes(), 0o644)?;
    fs::rename(&tmp, &p).map_err(|e| format!("{}: {e}", p.display()))
}

// ------------------------------------------------------------------ state, trial

/// `state`: whether the channel is on, and the trial of a newly used version.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub enabled: bool,
    /// the version on trial ("" when none)
    pub trial: String,
    pub tries: u32,
    pub max: u32,
    pub failed: String,
    pub failed_reason: String,
    /// the boot (`/proc/sys/kernel/random/boot_id`) last counted
    pub boot: String,
}

impl State {
    pub fn load(root: &Path) -> State {
        let kv = read_kv(&dir(root).join("state"));
        let num = |k: &str, d: u32| kv.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        State {
            enabled: kv.get("enabled").is_some_and(|v| v == "1"),
            trial: kv.get("trial").cloned().filter(|v| safe_version(v)).unwrap_or_default(),
            tries: num("tries", 0),
            max: num("max", DEFAULT_MAX_TRIES),
            failed: kv.get("failed").cloned().filter(|v| safe_version(v)).unwrap_or_default(),
            failed_reason: kv.get("failed_reason").cloned().unwrap_or_default(),
            boot: kv.get("boot").cloned().unwrap_or_default(),
        }
    }

    pub fn save(&self, root: &Path) -> Res<()> {
        fs::create_dir_all(dir(root)).map_err(|e| format!("{}: {e}", dir(root).display()))?;
        let mut kv = BTreeMap::new();
        kv.insert("enabled".to_string(), if self.enabled { "1" } else { "0" }.to_string());
        kv.insert("tries".to_string(), self.tries.to_string());
        kv.insert("max".to_string(), self.max.to_string());
        for (k, v) in [("trial", &self.trial), ("failed", &self.failed), ("failed_reason", &self.failed_reason), ("boot", &self.boot)] {
            if !v.is_empty() {
                kv.insert(k.to_string(), v.clone());
            }
        }
        write_kv(&dir(root).join("state"), &kv)
    }

    fn start_trial(&mut self, version: &str) {
        self.trial = version.to_string();
        self.tries = 0;
        if self.max == 0 {
            self.max = DEFAULT_MAX_TRIES;
        }
    }
}

/// Switch the channel on or off. On starts a trial of the current version.
pub fn set_enabled(root: &Path, on: bool) -> Res<String> {
    let cur = link_version(root, "current");
    if on && cur.is_none() {
        return Err("no Mesa version is installed yet".into());
    }
    let mut st = State::load(root);
    st.enabled = on;
    if on {
        st.start_trial(cur.as_deref().unwrap_or(""));
        st.failed.clear();
        st.failed_reason.clear();
    } else {
        st.trial.clear();
        st.tries = 0;
    }
    st.save(root)?;
    write_env(root, on)?;
    Ok(if on {
        format!("Mesa {} is used from the next login; keep it once applications work, or it is switched off again after {} starts", cur.unwrap_or_default(), st.max)
    } else {
        "the distribution's Mesa is used from the next login".into()
    })
}

/// Confirm the version on trial.
pub fn keep(root: &Path) -> Res<String> {
    let mut st = State::load(root);
    if st.trial.is_empty() {
        return Ok("nothing to keep: no Mesa version is on trial".into());
    }
    let v = std::mem::take(&mut st.trial);
    st.tries = 0;
    st.save(root)?;
    Ok(format!("Mesa {v} kept"))
}

/// Count a start of the version on trial (once per `boot_id`, so a daemon
/// restart in the same boot does not count), and switch the channel off when
/// it has had `max` starts without Keep.
pub fn boot_tick(root: &Path, boot_id: &str) -> Res<String> {
    let mut st = State::load(root);
    if !st.enabled || st.trial.is_empty() {
        return Ok("no Mesa trial pending".into());
    }
    if !boot_id.is_empty() && st.boot == boot_id {
        return Ok(format!("Mesa {} on trial: this boot is counted already", st.trial));
    }
    st.boot = boot_id.to_string();
    st.tries += 1;
    if st.tries <= st.max {
        st.save(root)?;
        return Ok(format!("Mesa {} on trial: start {} of {}", st.trial, st.tries, st.max));
    }
    st.failed = std::mem::take(&mut st.trial);
    st.failed_reason = format!("not kept after {} starts", st.max);
    st.enabled = false;
    st.tries = 0;
    st.save(root)?;
    write_env(root, false)?;
    Ok(format!("Mesa {} was not kept after {} starts: switched back to the distribution's Mesa", st.failed, st.max))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tb323fu-mesa-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut raw = Vec::new();
        for (name, data) in files {
            let mut h = [0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            h[100..107].copy_from_slice(b"0000644");
            h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
            h[156] = b'0';
            h[257..262].copy_from_slice(b"ustar");
            h[148..156].fill(b' ');
            let sum: u64 = h.iter().map(|b| *b as u64).sum();
            h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
            raw.extend_from_slice(&h);
            raw.extend_from_slice(data);
            raw.resize(raw.len().div_ceil(512) * 512, 0);
        }
        raw.resize(raw.len() + 1024, 0);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&raw).unwrap();
        gz.finish().unwrap()
    }

    fn release(version: &str) -> (Asset, BTreeMap<String, String>, Vec<u8>) {
        let vk: &[u8] = b"vulkan driver";
        let cl: &[u8] = b"opencl driver";
        let manifest = format!(
            "format=1\nversion={version}\narch=aarch64\ncommit=abc\nbase=def\ndrivers=vulkan,opencl\nfile {} 755 {} {VULKAN_LIB}\nfile {} 755 {} {OPENCL_LIB}\n",
            sha_hex(vk), vk.len(), sha_hex(cl), cl.len()
        );
        let tgz = tar(&[("MANIFEST", manifest.as_bytes()), (&format!("tree/{VULKAN_LIB}"), vk), (&format!("tree/{OPENCL_LIB}"), cl)]);
        let name = tarball_name(version);
        let sums = BTreeMap::from([(name.clone(), sha_hex(&tgz))]);
        (Asset { name, size: tgz.len() as u64, url: String::new(), digest: String::new() }, sums, tgz)
    }

    #[test]
    fn rel_paths() {
        assert!(safe_rel("lib/libvulkan_freedreno.so"));
        assert!(safe_rel("share/licenses/mesa/LICENSE"));
        assert!(!safe_rel("etc/passwd"));
        assert!(!safe_rel("lib/../etc"));
        assert!(!safe_rel("/lib/x"));
        assert!(!safe_rel("lib"));
    }

    #[test]
    fn manifest_rules() {
        assert!(Manifest::parse("format=1\nversion=2026.10.06\narch=aarch64\ndrivers=vulkan\n").is_err());
        assert!(Manifest::parse("format=1\nversion=2026.10.06\narch=x86_64\ndrivers=opencl\n").is_err());
        let e = Manifest::parse(&format!("format=1\nversion=1\narch=aarch64\ndrivers=vulkan\nfile {} 644 1 etc/x\n", "0".repeat(64))).unwrap_err();
        assert!(e.contains("may not install"), "{e}");
    }

    #[test]
    fn install_enable_trial_revert() {
        let root = tmp("flow");
        let (a, sums, tgz) = release("2026.10.06");
        unpack(&root, "2026.10.06", &a, &sums, &tgz).unwrap();
        activate(&root, "2026.10.06").unwrap();
        assert_eq!(link_version(&root, "current").as_deref(), Some("2026.10.06"));
        let icd = fs::read_to_string(dir(&root).join("icd/turnip.json")).unwrap();
        assert!(icd.contains("/var/lib/tb323fu/mesa/current/lib/libvulkan_freedreno.so"), "{icd}");
        assert_eq!(fs::read_to_string(dir(&root).join("icd/opencl/rusticl.icd")).unwrap().trim(), "/var/lib/tb323fu/mesa/current/lib/libRusticlOpenCL.so.1");

        set_enabled(&root, true).unwrap();
        let env = fs::read_to_string(dir(&root).join("env.conf")).unwrap();
        assert!(env.contains("VK_DRIVER_FILES=/var/lib/tb323fu/mesa/icd/turnip.json") && env.contains("RUSTICL_ENABLE=freedreno"), "{env}");
        assert_eq!(State::load(&root).trial, "2026.10.06");

        // two starts are allowed, the third without Keep switches off
        boot_tick(&root, "b1").unwrap();
        boot_tick(&root, "b1").unwrap();
        boot_tick(&root, "b2").unwrap();
        assert!(State::load(&root).enabled);
        let msg = boot_tick(&root, "b3").unwrap();
        assert!(msg.contains("not kept"), "{msg}");
        let st = State::load(&root);
        assert!(!st.enabled && st.failed == "2026.10.06");
        assert_eq!(fs::read_to_string(dir(&root).join("env.conf")).unwrap(), "");

        // on again, kept: ticks do nothing
        set_enabled(&root, true).unwrap();
        keep(&root).unwrap();
        for b in ["c1", "c2", "c3", "c4"] {
            boot_tick(&root, b).unwrap();
        }
        assert!(State::load(&root).enabled);

        // a second version: previous, trial restarts, rollback swaps
        let (a2, sums2, tgz2) = release("2026.10.07");
        unpack(&root, "2026.10.07", &a2, &sums2, &tgz2).unwrap();
        activate(&root, "2026.10.07").unwrap();
        assert_eq!(link_version(&root, "previous").as_deref(), Some("2026.10.06"));
        assert_eq!(State::load(&root).trial, "2026.10.07");
        rollback(&root).unwrap();
        assert_eq!(link_version(&root, "current").as_deref(), Some("2026.10.06"));
        assert_eq!(link_version(&root, "previous").as_deref(), Some("2026.10.07"));

        // a third version drops the oldest that is neither current nor previous
        let (a3, sums3, tgz3) = release("2026.10.08");
        unpack(&root, "2026.10.08", &a3, &sums3, &tgz3).unwrap();
        activate(&root, "2026.10.08").unwrap();
        assert_eq!(versions(&root), vec!["2026.10.08".to_string(), "2026.10.06".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unpack_rejects_damage() {
        let root = tmp("damage");
        let (a, mut sums, tgz) = release("2026.10.06");
        assert!(unpack(&root, "2026.10.05", &a, &sums, &tgz).unwrap_err().contains("the release is"));
        sums.insert(a.name.clone(), "0".repeat(64));
        assert!(unpack(&root, "2026.10.06", &a, &sums, &tgz).unwrap_err().contains("SHA256SUMS"));
        assert!(versions(&root).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn releases_and_pick() {
        let src = Source::parse("github:o/r", "file:///api").unwrap();
        let json = r#"[
          {"tag_name":"mesa-2026.10.07","prerelease":true,"assets":[
            {"name":"tb323fu-mesa-2026.10.07-aarch64.tar.gz","size":10,"url":"file:///api/a1"},
            {"name":"SHA256SUMS","size":10,"url":"file:///api/a2"}]},
          {"tag_name":"mesa-2026.10.06","assets":[
            {"name":"tb323fu-mesa-2026.10.06-aarch64.tar.gz","size":10,"url":"file:///api/b1"},
            {"name":"SHA256SUMS","size":10,"url":"file:///api/b2"}]},
          {"tag_name":"mesa-2026.10.05","assets":[{"name":"SHA256SUMS","size":10,"url":"file:///api/c2"}]},
          {"tag_name":"kernel-t40","assets":[]}]"#;
        let rels = parse_releases(json.as_bytes(), &src).unwrap();
        assert_eq!(rels.len(), 2);
        assert_eq!(pick(&rels, "stable").unwrap().version, "2026.10.06");
        assert_eq!(pick(&rels, "testing").unwrap().version, "2026.10.07");
    }
}
