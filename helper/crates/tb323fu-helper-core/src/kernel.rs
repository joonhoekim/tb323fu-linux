// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! Kernel updates (docs/notes/kernel-updates-design.md, section 3): releases
//! from GitHub Releases, kernels from a local file, the trial-boot record on
//! the state root, and the writes to `boot_a` -- install, confirm ("keep"),
//! rollback.
//!
//! Nothing here touches the network: the fetch unit downloads into a cache
//! directory, and this code only checks what it finds there -- the release
//! list of the GitHub REST API, the release's `SHA256SUMS` and the kernel
//! file (size, SHA-256 from `SHA256SUMS`, the asset digest GitHub reports,
//! the kernel's own version banner). `SHA256SUMS` comes over the same
//! channel as the kernel, so it catches transfer errors, not a changed
//! release; a minisign signature over it (`SHA256SUMS.minisig`) is checked
//! only when `kernel.require_signature` is on (off by default, no key is
//! built in).
//!
//! Releases ship a kernel `Image`, never a boot image: the boot image is made
//! here from the user's own stock image, the copy in `boot_b` (checked against
//! the recorded Android hash first), exactly as `tools/boot-repack-kernel.py`
//! does at install time. A kernel from a local file takes the same path.
//!
//! State on the state root (`/var/lib/tb323fu`, see `boot::with_state_dir`):
//!
//! ```text
//! kernel-state      good=, good_sha256=, good_version=, good_serial=, good_label=,
//!                   trial=, trial_sha256=, trial_version=, trial_serial=,
//!                   trial_channel=, trial_keep=, trial_label=, tries=, max=,
//!                   failed=, failed_sha256=
//! linux-good.img    the boot image that last ran confirmed (+ .sha256)
//! ```
//!
//! The same file is read and written by the initramfs (trial counter and
//! automatic rollback), the layer-1 confirm script and Android's Switch to
//! Linux; `*_version` is the kernel's `/proc/version` line, which tells two
//! builds with the same release string apart.

use crate::bootimg;
use crate::sys;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub type Res<T> = Result<T, String>;

/// The state directory on the state root (relative; `boot::with_state_dir`).
pub const STATE_REL: &str = "var/lib/tb323fu";
pub const STATE_FILE: &str = "kernel-state";
pub const GOOD_IMG: &str = "linux-good.img";
/// What the fetch unit writes (DynamicUser, CacheDirectory=tb323fu-kernel).
pub const CACHE_DIR: &str = "/var/cache/tb323fu-kernel";
/// Verified downloads and the last release list, on the running root.
pub const STAGE_DIR: &str = "/var/lib/tb323fu/kernel";
/// The daemon's download list for the fetch unit: `NAME MAXBYTES URL [MODE]` lines.
pub const FETCH_LIST: &str = "/run/tb323fu/kernel-fetch.list";
pub const MAX_KERNEL_FILE: u64 = 100 << 20;
/// The release list of the API (30 releases with their notes).
pub const MAX_RELEASES_FILE: u64 = 4 << 20;
pub const MAX_SUMS_FILE: u64 = 64 << 10;
pub const DEFAULT_MAX_TRIES: u32 = 2;
/// Battery level below which nothing is written to `boot_a` without a charger.
pub const MIN_BATTERY: u32 = 30;
/// Kernel assets of a release: `Image-tb323fu-tNN` and/or `Image-tb323fu-tNN.gz`.
pub const KERNEL_ASSET_PREFIX: &str = "Image-tb323fu-t";
pub const SUMS_ASSET: &str = "SHA256SUMS";
pub const SIG_ASSET: &str = "SHA256SUMS.minisig";

/// Built-in minisign keys for `kernel.require_signature`: none. The project
/// publishes through GitHub Releases with SHA256SUMS (design 3.2); signing is
/// an opt-in for whoever runs their own channel (`kernel.public_keys`, or
/// `/etc/tb323fu/keys/kernel-*.pub`).
pub const KEYS: &[&str] = &[];
/// More trusted keys, one minisign `.pub` file each (`kernel-*.pub`).
pub const KEY_DIRS: [&str; 3] = ["/usr/share/tb323fu/keys", "/usr/local/share/tb323fu/keys", "/etc/tb323fu/keys"];

// ------------------------------------------------------------------ helpers

pub fn sha_hex(d: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(d);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A file or tag name that is safe as one path component.
pub fn safe_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 96 && !s.starts_with('.') && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Compare dotted numeric versions ("0.10.1" > "0.9"); non-numeric parts as 0.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let p = |s: &str| -> Vec<u64> {
        s.split(['.', '-', '+']).take(4).map(|x| x.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)).collect()
    };
    let (mut x, mut y) = (p(a), p(b));
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    x.cmp(&y)
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Allowed URL schemes for downloads (https; http and file only for a test
/// API -- the API base decides, see `Source`).
pub fn url_ok(u: &str) -> bool {
    (u.starts_with("https://") || u.starts_with("http://") || u.starts_with("file:///"))
        && !u.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The `-tNN` serial of a release name like `7.3.0-rc4-tb323fu-t30`; 0 for
/// anything else (a development or self-built kernel).
pub fn serial_of_release(release: &str) -> u64 {
    release.rsplit_once("-tb323fu-t").and_then(|(_, n)| n.parse().ok()).unwrap_or(0)
}

// ------------------------------------------------------------------ signatures (optional)

/// Keys for `kernel.require_signature`: the built-in list (empty), the
/// configured ones and `kernel-*.pub` files in the key directories.
pub fn trusted_keys(configured: &[String]) -> Vec<minisign_verify::PublicKey> {
    let mut v: Vec<_> = KEYS.iter().copied().chain(configured.iter().map(|s| s.as_str()))
        .filter_map(|k| minisign_verify::PublicKey::from_base64(k.trim()).ok()).collect();
    for d in KEY_DIRS {
        for f in sys::list_dir(&sys::path(d)) {
            if !(f.starts_with("kernel-") && f.ends_with(".pub")) {
                continue;
            }
            if let Ok(t) = fs::read_to_string(sys::path(d).join(&f)) {
                if let Ok(k) = minisign_verify::PublicKey::decode(&t)
                    .or_else(|_| minisign_verify::PublicKey::from_base64(t.lines().last().unwrap_or("").trim()))
                {
                    v.push(k);
                }
            }
        }
    }
    v
}

/// Verify a minisign signature (`.minisig` text) over `data` with any of `keys`.
pub fn verify(data: &[u8], sig: &str, keys: &[minisign_verify::PublicKey]) -> Res<()> {
    let s = minisign_verify::Signature::decode(sig).map_err(|e| format!("signature: {e}"))?;
    if keys.iter().any(|k| k.verify(data, &s, false).is_ok()) {
        Ok(())
    } else {
        Err("signature does not verify with a trusted key".into())
    }
}

// ------------------------------------------------------------------ GitHub Releases

/// Where releases come from: `github:OWNER/REPO` and the API base.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub owner: String,
    pub repo: String,
    pub api: String,
}

impl Source {
    pub fn parse(source: &str, api: &str) -> Res<Source> {
        let Some((owner, repo)) = source.trim().strip_prefix("github:").and_then(|s| s.split_once('/')) else {
            return Err(format!("kernel.source must be github:OWNER/REPO (is {source:?})"));
        };
        let ok = |s: &str| !s.is_empty() && s.len() <= 100 && !s.starts_with('.') && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
        if !ok(owner) || !ok(repo) {
            return Err(format!("kernel.source: bad owner or repository name in {source:?}"));
        }
        let api = api.trim().trim_end_matches('/');
        if !url_ok(api) {
            return Err(format!("kernel.api_url is not an https/http/file URL: {api}"));
        }
        Ok(Source { owner: owner.into(), repo: repo.into(), api: api.into() })
    }

    /// The release list (newest first, 30 per page; the API default).
    pub fn releases_url(&self) -> String {
        format!("{}/repos/{}/{}/releases", self.api, self.owner, self.repo)
    }

    /// Every asset URL must start with this: on GitHub the release assets of
    /// this repository (so a token, when one is configured, never goes
    /// anywhere else); on a test API (another `api_url`) anything under it --
    /// a file tree cannot hold `releases` as a file and as a directory.
    pub fn asset_prefix(&self) -> String {
        if self.api == crate::config::DEFAULT_API_URL {
            format!("{}/repos/{}/{}/releases/assets/", self.api, self.owner, self.repo)
        } else {
            format!("{}/", self.api)
        }
    }

    pub fn name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

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

/// A downloadable file of a release.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    /// the API's asset URL (downloaded with Accept: application/octet-stream)
    pub url: String,
    /// SHA-256 GitHub computed at upload ("" when the API does not say)
    pub digest: String,
}

/// A kernel release on GitHub Releases.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Release {
    /// the release's git tag (e.g. `kernel-t31`)
    pub tag: String,
    pub title: String,
    pub prerelease: bool,
    pub published: String,
    /// release notes (Markdown, the release body)
    pub notes: String,
    pub notes_url: String,
    /// `tNN` from the kernel asset's name
    pub ktag: String,
    pub serial: u64,
    /// the kernel file to download (the `.gz` when the release has one)
    pub kernel: Asset,
    pub sums: Asset,
    pub sig: Option<Asset>,
    /// from a `<!-- tb323fu: min_helper=X min_platform=Y -->` line in the notes
    pub min_helper: String,
    pub min_platform: String,
}

/// `tNN` serial of a kernel asset name (`Image-tb323fu-t31`, `….gz`).
fn kernel_asset_serial(name: &str) -> Option<u64> {
    let n = name.strip_prefix(KERNEL_ASSET_PREFIX)?;
    let n = n.strip_suffix(".gz").unwrap_or(n);
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}

fn requirement(notes: &str, key: &str) -> String {
    notes.lines().filter_map(|l| l.trim().strip_prefix("<!-- tb323fu:")).flat_map(|l| l.trim_end_matches("-->").split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")).map(str::to_string)).unwrap_or_default()
}

/// The kernel releases in an API release list, and the newest helper
/// version published as a `helper-vX.Y.Z` release (not a pre-release).
/// Drafts and releases without exactly one kernel serial and a `SHA256SUMS`
/// are left out; so are assets whose URL is not under this source.
pub fn parse_releases(data: &[u8], src: &Source) -> Res<(Vec<Release>, Option<String>)> {
    let list: Vec<GhRelease> = serde_json::from_slice(data).map_err(|e| format!("release list from {}: {e}", src.name()))?;
    let prefix = src.asset_prefix();
    let mut out = Vec::new();
    let mut helper: Option<String> = None;
    for r in list {
        if r.draft {
            continue;
        }
        if let Some(v) = r.tag_name.strip_prefix("helper-v") {
            if !r.prerelease && helper.as_deref().is_none_or(|h| version_cmp(v, h).is_gt()) {
                helper = Some(v.to_string());
            }
            continue;
        }
        if !safe_name(&r.tag_name) {
            continue;
        }
        let asset = |a: &GhAsset| Asset {
            name: a.name.clone(),
            size: a.size,
            url: a.url.clone(),
            digest: a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).filter(|d| is_hex64(d)).unwrap_or("").to_string(),
        };
        let usable = |a: &&GhAsset| safe_name(&a.name) && a.url.starts_with(&prefix) && url_ok(&a.url) && a.size > 0;
        let kernels: Vec<&GhAsset> = r.assets.iter().filter(usable).filter(|a| kernel_asset_serial(&a.name).is_some()).collect();
        let serials: std::collections::BTreeSet<u64> = kernels.iter().filter_map(|a| kernel_asset_serial(&a.name)).collect();
        if serials.len() != 1 {
            continue;
        }
        let serial = *serials.iter().next().unwrap_or(&0);
        let Some(k) = kernels.iter().find(|a| a.name.ends_with(".gz")).or(kernels.first()) else { continue };
        if k.size > MAX_KERNEL_FILE {
            continue;
        }
        let Some(sums) = r.assets.iter().filter(usable).find(|a| a.name == SUMS_ASSET) else { continue };
        let sig = r.assets.iter().filter(usable).find(|a| a.name == SIG_ASSET).map(asset);
        let notes = r.body.clone().unwrap_or_default();
        out.push(Release {
            tag: r.tag_name.clone(),
            title: r.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            prerelease: r.prerelease,
            published: r.published_at.clone().unwrap_or_default(),
            min_helper: requirement(&notes, "min_helper"),
            min_platform: requirement(&notes, "min_platform"),
            notes,
            notes_url: r.html_url.clone(),
            ktag: format!("t{serial}"),
            serial,
            kernel: asset(k),
            sums: asset(sums),
            sig,
        });
    }
    Ok((out, helper))
}

/// The release a channel offers: stable -- the newest release (by serial)
/// that is not a pre-release; testing -- the newest of all, pre-releases
/// included (a pre-release that was turned into a release stays the newest).
pub fn pick<'a>(rels: &'a [Release], channel: &str) -> Option<&'a Release> {
    rels.iter().filter(|r| channel == "testing" || !r.prerelease).max_by(|a, b| a.serial.cmp(&b.serial).then(a.published.cmp(&b.published)))
}

/// `sha256sum` output: name -> lower-case hex (`<hex>  <name>` or `<hex> *<name>`).
pub fn parse_sums(text: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for l in text.lines() {
        let l = l.trim_end_matches('\r');
        let Some((h, n)) = l.split_once(' ') else { continue };
        let h = h.to_ascii_lowercase();
        let n = n.trim_start_matches(' ').trim_start_matches('*');
        if is_hex64(&h) && safe_name(n) {
            m.insert(n.to_string(), h);
        }
    }
    m
}

/// `SHA256SUMS` of a release, with the signature over it when one is
/// required (`require_signature`): no key configured, or no or a wrong
/// signature, is an error then.
pub fn read_sums(data: &[u8], sig: Option<&str>, require_signature: bool, keys: &[minisign_verify::PublicKey]) -> Res<BTreeMap<String, String>> {
    if require_signature {
        if keys.is_empty() {
            return Err("kernel.require_signature is on, but no public key is configured (kernel.public_keys or /etc/tb323fu/keys/kernel-*.pub)".into());
        }
        let sig = sig.ok_or("kernel.require_signature is on, but the release has no SHA256SUMS.minisig")?;
        verify(data, sig, keys).map_err(|e| format!("SHA256SUMS: {e}"))?;
    }
    let text = std::str::from_utf8(data).map_err(|_| "SHA256SUMS is not text".to_string())?;
    let m = parse_sums(text);
    if m.is_empty() {
        return Err("SHA256SUMS lists no files".into());
    }
    Ok(m)
}

/// Check a downloaded kernel file of a release: size, SHA-256 from
/// `SHA256SUMS` and GitHub's digest, a bootable format, and a version banner
/// whose release ends in `-tb323fu-tNN` of the asset. Returns (release, banner).
pub fn check_kernel_file(r: &Release, sums: &BTreeMap<String, String>, data: &[u8]) -> Res<(String, String)> {
    let name = &r.kernel.name;
    if data.len() as u64 != r.kernel.size {
        return Err(format!("{name}: {} bytes, the release says {}", data.len(), r.kernel.size));
    }
    let sha = sha_hex(data);
    let want = sums.get(name).ok_or_else(|| format!("{name} is not listed in SHA256SUMS"))?;
    if &sha != want {
        return Err(format!("{name}: SHA-256 does not match SHA256SUMS (a damaged download?)"));
    }
    if !r.kernel.digest.is_empty() && sha != r.kernel.digest {
        return Err(format!("{name}: SHA-256 does not match the digest GitHub reports"));
    }
    let raw = bootimg::kernel_raw(data)?;
    let (rel, banner) = bootimg::find_banner(&raw).ok_or_else(|| format!("{name}: no kernel version banner"))?;
    if !rel.ends_with(&format!("-tb323fu-{}", r.ktag)) {
        return Err(format!("{name}: the kernel is release {rel}, not a -tb323fu-{} build", r.ktag));
    }
    Ok((rel, banner))
}

/// The release's needs against this helper and the platform files.
pub fn requirements(r: &Release, helper: &str, platform: Option<&str>) -> Res<()> {
    if !r.min_helper.is_empty() && version_cmp(helper, &r.min_helper).is_lt() {
        return Err(format!("{} needs tb323fu-helper {} or newer (this is {helper}); update the helper first", r.tag, r.min_helper));
    }
    if let (false, Some(p)) = (r.min_platform.is_empty(), platform) {
        if version_cmp(p, &r.min_platform).is_lt() {
            return Err(format!("{} needs tb323fu-platform {} or newer (installed: {p}); update it first", r.tag, r.min_platform));
        }
    }
    Ok(())
}

/// The installed platform files' version (`share/tb323fu/platform-version`).
pub fn platform_version() -> Option<String> {
    ["/usr/share/tb323fu/platform-version", "/usr/local/share/tb323fu/platform-version"]
        .iter()
        .find_map(|p| sys::read_opt(&sys::path(p)))
        .filter(|v| !v.is_empty())
}

// ------------------------------------------------------------------ a kernel from a file

/// A kernel someone picked from a file (`tb323fu-ctl kernel install-local`,
/// the settings app), looked at before it is installed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalKernel {
    /// what is repacked into the stock image (an Image or Image.gz; the
    /// kernel field of a boot image)
    pub kernel: Vec<u8>,
    /// "Image", "Image.gz" or "boot image"
    pub format: String,
    pub release: String,
    /// its `/proc/version` line
    pub banner: String,
    /// the initramfs carries `/lib/modules/<release>.sqfs` (shared modules)
    pub shared_modules: bool,
    /// things a person should know before installing it
    pub warnings: Vec<String>,
}

/// Look at a kernel file: a raw arm64 Image, an Image.gz, or an Android boot
/// image (its kernel field is used). Errors for anything that cannot boot here.
pub fn inspect_local(data: &[u8], run: &Running) -> Res<LocalKernel> {
    let mut warnings = Vec::new();
    let (kernel, boot) = if data.starts_with(b"ANDROID!") {
        let k = bootimg::boot_kernel(data).map_err(|e| format!("boot image: {e}"))?;
        warnings.push("This is a boot image: only its kernel is used. The header, signature and vbmeta come from your own stock image in boot_b.".into());
        (k.to_vec(), true)
    } else {
        (data.to_vec(), false)
    };
    if kernel.len() as u64 > MAX_KERNEL_FILE {
        return Err(format!("the kernel is larger than {} MiB", MAX_KERNEL_FILE >> 20));
    }
    let fmt = bootimg::kernel_format(&kernel)?;
    let raw = bootimg::kernel_raw(&kernel)?;
    let (release, banner) = bootimg::find_banner(&raw).ok_or("no Linux version banner in the kernel: not a kernel Image?")?;
    let modules = bootimg::has_modules_image(&raw, &release);
    drop(raw);
    let shared_modules = modules == Some(true);
    if !shared_modules {
        warnings.push(format!("No shared modules: the kernel's initramfs has no /lib/modules/{release}.sqfs. Every system you boot needs \
            its own modules for {release} (own mode: /etc/tb323fu/modules), or it starts without them (no sound, Wi-Fi, ...)."));
    }
    if banner == run.version {
        warnings.push("This is the kernel that is running now (the same build).".into());
    }
    if serial_of_release(&release) == 0 {
        warnings.push(format!("{release} is not a -tb323fu-tNN release: official releases will always be offered as newer."));
    }
    let format = if boot { "boot image" } else if fmt == bootimg::KernelFormat::Gzip { "Image.gz" } else { "Image" };
    Ok(LocalKernel { kernel, format: format.into(), release, banner, shared_modules, warnings })
}

// ------------------------------------------------------------------ running kernel

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Running {
    /// `uname -r`
    pub release: String,
    /// `/proc/version`, without the newline
    pub version: String,
}

pub fn running() -> Running {
    Running {
        release: sys::read_opt(&sys::path("/proc/sys/kernel/osrelease")).unwrap_or_default(),
        version: fs::read_to_string(sys::path("/proc/version")).map(|s| s.trim_end().to_string()).unwrap_or_default(),
    }
}

/// `/lib/modules/<release>` is the modules image the initramfs mounted (a
/// squashfs), not a tree in the root.
pub fn shared_modules(release: &str) -> bool {
    let mi = fs::read_to_string(sys::path("/proc/self/mountinfo")).unwrap_or_default();
    let want = [format!("/lib/modules/{release}"), format!("/usr/lib/modules/{release}")];
    mi.lines().any(|l| {
        let f: Vec<&str> = l.split(' ').collect();
        let fstype = f.iter().position(|x| *x == "-").and_then(|i| f.get(i + 1)).copied().unwrap_or("");
        f.len() > 4 && want.iter().any(|w| w == f[4]) && fstype == "squashfs"
    })
}

// ------------------------------------------------------------------ kernel-state

#[derive(Debug, Clone, Default, PartialEq)]
pub struct KernelState {
    pub good: String,
    pub good_sha256: String,
    pub good_version: String,
    pub good_serial: u64,
    /// a name given at install (local files), ""
    pub good_label: String,
    pub trial: String,
    pub trial_sha256: String,
    pub trial_version: String,
    pub trial_serial: u64,
    pub trial_channel: String,
    /// confirmed only by Keep, not by the confirm unit (testing; local files)
    pub trial_keep: bool,
    pub trial_label: String,
    pub tries: u32,
    pub max: u32,
    pub failed: String,
    pub failed_sha256: String,
    /// keys this helper does not know, kept as they are
    pub other: Vec<(String, String)>,
}

impl KernelState {
    pub fn parse(t: &str) -> Self {
        let mut s = KernelState { max: DEFAULT_MAX_TRIES, ..Default::default() };
        for l in t.lines() {
            let l = l.trim_end_matches('\r');
            if l.starts_with('#') {
                continue;
            }
            let Some((k, v)) = l.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.to_string());
            let n = || v.trim().parse().unwrap_or(0);
            match k {
                "good" => s.good = v,
                "good_sha256" => s.good_sha256 = v,
                "good_version" => s.good_version = v,
                "good_serial" => s.good_serial = n(),
                "good_label" => s.good_label = v,
                "trial" => s.trial = v,
                "trial_sha256" => s.trial_sha256 = v,
                "trial_version" => s.trial_version = v,
                "trial_serial" => s.trial_serial = n(),
                "trial_channel" => s.trial_channel = v,
                "trial_keep" => s.trial_keep = v.trim() == "1",
                "trial_label" => s.trial_label = v,
                "tries" => s.tries = n() as u32,
                "max" => s.max = v.trim().parse().unwrap_or(DEFAULT_MAX_TRIES),
                "failed" => s.failed = v,
                "failed_sha256" => s.failed_sha256 = v,
                _ => s.other.push((k.to_string(), v)),
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let mut o = String::from("# written by tb323fu-helperd, the initramfs and tb323fu-kernel-confirm\n");
        let mut put = |k: &str, v: &str| {
            if !v.is_empty() {
                o.push_str(&format!("{k}={v}\n"));
            }
        };
        let num = |n: u64| if n == 0 { String::new() } else { n.to_string() };
        put("good", &self.good);
        put("good_sha256", &self.good_sha256);
        put("good_version", &self.good_version);
        put("good_serial", &num(self.good_serial));
        put("good_label", &self.good_label);
        put("trial", &self.trial);
        put("trial_sha256", &self.trial_sha256);
        put("trial_version", &self.trial_version);
        put("trial_serial", &num(self.trial_serial));
        put("trial_channel", &self.trial_channel);
        put("trial_keep", if self.trial_keep { "1" } else { "" });
        put("trial_label", &self.trial_label);
        put("tries", &num(self.tries as u64));
        put("max", &self.max.to_string());
        put("failed", &self.failed);
        put("failed_sha256", &self.failed_sha256);
        for (k, v) in &self.other {
            put(k, v);
        }
        o
    }

    pub fn load(dir: &Path) -> Self {
        Self::parse(&fs::read_to_string(dir.join(STATE_FILE)).unwrap_or_default())
    }

    pub fn save(&self, dir: &Path) -> Res<()> {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let tmp = dir.join(format!("{STATE_FILE}.tmp"));
        let mut f = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        f.write_all(self.to_text().as_bytes()).and_then(|_| f.sync_all()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        fs::rename(&tmp, dir.join(STATE_FILE)).map_err(|e| format!("{STATE_FILE}: {e}"))?;
        sync_dir(dir);
        Ok(())
    }

    /// The running kernel is the one on trial (installed, not yet confirmed).
    pub fn trial_running(&self, r: &Running) -> bool {
        if !self.trial_version.is_empty() {
            return self.trial_version.trim_end() == r.version;
        }
        !self.trial.is_empty() && self.trial == r.release
    }

    /// The trial waits for Keep (testing channel, or a local install that
    /// asked for it); the confirm unit leaves it alone.
    pub fn keep_needed(&self) -> bool {
        !self.trial.is_empty() && (self.trial_keep || self.trial_channel == "testing")
    }

    /// The running kernel is the recorded good one.
    pub fn good_running(&self, r: &Running) -> bool {
        if !self.good_version.is_empty() {
            return self.good_version.trim_end() == r.version;
        }
        !self.good.is_empty() && self.good == r.release
    }

    fn clear_trial(&mut self) {
        self.trial.clear();
        self.trial_sha256.clear();
        self.trial_version.clear();
        self.trial_serial = 0;
        self.trial_channel.clear();
        self.trial_keep = false;
        self.trial_label.clear();
        self.tries = 0;
    }
}

/// The running kernel's serial: from the record, else the `-tNN` of a
/// release build's name, else 0 (a development kernel: every release is newer).
pub fn running_serial(st: &KernelState, r: &Running) -> u64 {
    if st.trial_running(r) && st.trial_serial > 0 {
        return st.trial_serial;
    }
    if st.good_running(r) && st.good_serial > 0 {
        return st.good_serial;
    }
    r.release
        .rsplit_once("-tb323fu-t")
        .and_then(|(_, n)| n.parse().ok())
        .unwrap_or(0)
}

// ------------------------------------------------------------------ the boot partitions

pub struct Device {
    pub boot_a: PathBuf,
    pub boot_b: PathBuf,
    pub size: u64,
}

/// Devices whose GPT name is `name` (block devices under the test root too).
pub fn find_part(name: &str) -> Vec<PathBuf> {
    let base = sys::path("/sys/class/block");
    let mut v = Vec::new();
    for e in sys::list_dir(&base) {
        let Ok(t) = fs::read_to_string(base.join(&e).join("uevent")) else { continue };
        if t.lines().any(|l| l == format!("PARTNAME={name}")) {
            let dev = t.lines().find_map(|l| l.strip_prefix("DEVNAME=")).unwrap_or(&e).to_string();
            v.push(sys::path(&format!("/dev/{dev}")));
        }
    }
    v
}

fn dev_size(p: &Path) -> Res<u64> {
    let mut f = fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    f.seek(SeekFrom::End(0)).map_err(|e| format!("{}: {e}", p.display()))
}

/// `boot_a` and `boot_b`, each found once by GPT name, of the same size.
pub fn find_device() -> Res<Device> {
    let (a, b) = (find_part("boot_a"), find_part("boot_b"));
    if a.len() != 1 || b.len() != 1 {
        return Err(format!("boot_a/boot_b not found once each ({} / {})", a.len(), b.len()));
    }
    let (sa, sb) = (dev_size(&a[0])?, dev_size(&b[0])?);
    if sa != sb || sa == 0 {
        return Err(format!("boot_a ({sa} bytes) and boot_b ({sb} bytes) differ in size"));
    }
    Ok(Device { boot_a: a[0].clone(), boot_b: b[0].clone(), size: sa })
}

fn read_all(p: &Path, max: u64) -> Res<Vec<u8>> {
    let mut v = Vec::new();
    fs::File::open(p)
        .and_then(|f| f.take(max + 1).read_to_end(&mut v))
        .map_err(|e| format!("{}: {e}", p.display()))?;
    if v.len() as u64 > max {
        return Err(format!("{}: larger than {max} bytes", p.display()));
    }
    Ok(v)
}

fn sync_dir(d: &Path) {
    if let Ok(f) = fs::File::open(d) {
        let _ = f.sync_all();
    }
}

fn drop_caches() {
    let _ = fs::write(sys::path("/proc/sys/vm/drop_caches"), "3");
}

impl Device {
    pub fn read_a(&self) -> Res<Vec<u8>> {
        read_all(&self.boot_a, self.size)
    }
    pub fn read_b(&self) -> Res<Vec<u8>> {
        read_all(&self.boot_b, self.size)
    }

    /// Write a full image to `boot_a`, flush, drop the page cache and compare
    /// what reads back.
    pub fn write_a(&self, img: &[u8]) -> Res<()> {
        if img.len() as u64 != self.size {
            return Err(format!("image is {} bytes, boot_a is {}", img.len(), self.size));
        }
        let mut f = fs::OpenOptions::new().write(true).open(&self.boot_a).map_err(|e| format!("{}: {e}", self.boot_a.display()))?;
        #[cfg(test)]
        let corrupted: Vec<u8>;
        #[cfg(test)]
        let img_w: &[u8] = if CORRUPT_NEXT_WRITE.with(|c| c.replace(false)) {
            corrupted = img.iter().enumerate().map(|(i, b)| if i == 4096 { b ^ 1 } else { *b }).collect();
            &corrupted
        } else {
            img
        };
        #[cfg(not(test))]
        let img_w = img;
        f.write_all(img_w).and_then(|_| f.sync_all()).map_err(|e| format!("writing boot_a: {e}"))?;
        drop(f);
        drop_caches();
        let back = self.read_a()?;
        if sha_hex(&back) != sha_hex(img) {
            return Err("boot_a does not read back as written".into());
        }
        Ok(())
    }
}

#[cfg(test)]
thread_local! {
    /// tests: the next boot_a write flips a bit (a read-back mismatch)
    pub static CORRUPT_NEXT_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The stock image in `boot_b`, checked against the recorded Android hash
/// (the way back to Android exists, and it is the user's own stock image).
pub fn stock_image(dev: &Device, android_hash: &str) -> Res<Vec<u8>> {
    if !is_hex64(android_hash) {
        return Err("no Android image hash (/etc/tb323fu/android-boot.sha256): the way back is not set up".into());
    }
    let b = dev.read_b()?;
    if sha_hex(&b) != android_hash {
        return Err("boot_b is not the recorded Android image; not writing boot_a (no way back)".into());
    }
    Ok(b)
}

/// Save `img` (sha `sha`) as linux-good.img in `dir`: tmp file, hash check, rename, sync.
pub fn save_good(dir: &Path, img: &[u8], sha: &str) -> Res<()> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = dir.join(format!("{GOOD_IMG}.tmp"));
    let res = (|| {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        f.write_all(img).and_then(|_| f.sync_all()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        drop(f);
        drop_caches();
        if sha_hex(&read_all(&tmp, img.len() as u64)?) != sha {
            return Err(format!("{}: does not read back as written", tmp.display()));
        }
        fs::rename(&tmp, dir.join(GOOD_IMG)).map_err(|e| format!("{GOOD_IMG}: {e}"))?;
        fs::write(dir.join(format!("{GOOD_IMG}.sha256")), format!("{sha}\n")).map_err(|e| format!("{GOOD_IMG}.sha256: {e}"))?;
        sync_dir(dir);
        Ok(())
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// linux-good.img when it is there and matches the record.
pub fn good_image(dir: &Path, st: &KernelState, size: u64) -> Res<Vec<u8>> {
    if !is_hex64(&st.good_sha256) {
        return Err("no linux-good.img recorded".into());
    }
    let g = read_all(&dir.join(GOOD_IMG), size).map_err(|e| format!("linux-good.img: {e}"))?;
    if g.len() as u64 != size || sha_hex(&g) != st.good_sha256 {
        return Err("linux-good.img does not match its record".into());
    }
    Ok(g)
}

// ------------------------------------------------------------------ operations

/// What is installed: a checked release kernel or a local file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    /// `uname -r` of the kernel
    pub release: String,
    /// its `/proc/version` line
    pub version: String,
    /// `tNN` serial (0: not a numbered release)
    pub serial: u64,
    /// where it came from: "stable", "testing" or "local"
    pub channel: String,
    /// confirmed only by Keep (testing channel; local files unless asked
    /// otherwise), not by the confirm unit
    pub keep: bool,
    /// a name for a person (local files: `--name`), "" none
    pub label: String,
}

/// Install a checked kernel into `boot_a` as a trial (design 3.4).
/// `state` is the state root's /var/lib/tb323fu; `power` the battery check.
pub fn install(dev: &Device, android_hash: &str, run: &Running, state: &Path, c: &Candidate, kernel_file: &[u8], power: Res<()>) -> Res<String> {
    power?;
    let mut st = KernelState::load(state);
    if st.trial_running(run) {
        return Err(format!("the running kernel {} is still on trial: wait until it is confirmed (or keep it) first", st.trial));
    }
    if c.release.is_empty() || c.version.is_empty() {
        return Err("no kernel release or version banner".into());
    }
    let stock = stock_image(dev, android_hash)?;
    let img = bootimg::StockBoot::parse(&stock).map_err(|e| format!("stock boot image in boot_b: {e}"))?.repack(kernel_file)?;
    drop(stock);
    let img_sha = sha_hex(&img);

    // keep the image that works
    let a = dev.read_a()?;
    let a_sha = sha_hex(&a);
    let a_raw = bootimg::boot_kernel(&a).and_then(bootimg::kernel_raw).unwrap_or_default();
    let a_runs = bootimg::has_banner(&a_raw, &run.version);
    drop(a_raw);
    if a_runs {
        let have = good_image(state, &st, dev.size).is_ok() && st.good_sha256 == a_sha;
        if !have {
            save_good(state, &a, &a_sha)?;
        }
        if !st.good_running(run) || st.good_sha256 != a_sha {
            st.good_serial = if st.good_running(run) { st.good_serial } else { running_serial(&st, run) };
            if !st.good_running(run) {
                st.good_label.clear();
            }
            st.good = run.release.clone();
            st.good_version = run.version.clone();
        }
        st.good_sha256 = a_sha;
    } else {
        good_image(state, &st, dev.size).map_err(|e| {
            format!("boot_a does not hold the running kernel and {e}; restart once, then install")
        })?;
    }
    drop(a);

    st.trial = c.release.clone();
    st.trial_sha256 = img_sha;
    st.trial_version = c.version.clone();
    st.trial_serial = c.serial;
    st.trial_channel = if c.channel.is_empty() { "stable".into() } else { c.channel.clone() };
    st.trial_keep = c.keep;
    st.trial_label = c.label.clone();
    st.tries = 0;
    if st.max == 0 {
        st.max = DEFAULT_MAX_TRIES;
    }
    if st.failed == st.trial {
        st.failed.clear();
        st.failed_sha256.clear();
        st.other.retain(|(k, _)| k != "failed_seen");
    }
    st.save(state)?;

    if let Err(e) = dev.write_a(&img) {
        let back = good_image(state, &st, dev.size).and_then(|g| dev.write_a(&g));
        st.clear_trial();
        let _ = st.save(state);
        return Err(match back {
            Ok(()) => format!("{e}; boot_a holds the previous kernel again"),
            Err(e2) => format!("{e}; restoring linux-good.img failed too ({e2}) -- do NOT restart; restore boot_a over EDL (docs/recovery.md)"),
        });
    }
    let how = if c.keep { "kept when you press Keep" } else { "kept once a system has run 90 s with it" };
    Ok(format!("{} installed in boot_a; it is tried on the next start ({how}; otherwise back to {} after {} starts)", c.release, st.good, st.max))
}

/// Confirm the running trial kernel ("keep"): its image in boot_a becomes
/// linux-good.img. The layer-1 script tb323fu-kernel-confirm does the same
/// 90 s after a system has started, for trials that do not wait for Keep.
pub fn confirm(dev: &Device, run: &Running, state: &Path) -> Res<String> {
    let mut st = KernelState::load(state);
    if !st.trial_running(run) {
        return if st.trial.is_empty() { Ok("nothing to confirm: no kernel on trial".into()) } else {
            Err(format!("{} is installed but not running yet; restart first", st.trial))
        };
    }
    let a = dev.read_a()?;
    let a_sha = sha_hex(&a);
    if a_sha != st.trial_sha256 {
        return Err("boot_a no longer holds the trial image; not confirming".into());
    }
    save_good(state, &a, &a_sha)?;
    st.good = st.trial.clone();
    st.good_sha256 = a_sha;
    st.good_version = st.trial_version.clone();
    st.good_serial = st.trial_serial;
    st.good_label = st.trial_label.clone();
    if st.failed == st.trial {
        st.failed.clear();
        st.failed_sha256.clear();
        st.other.retain(|(k, _)| k != "failed_seen");
    }
    let rel = st.trial.clone();
    st.clear_trial();
    st.save(state)?;
    Ok(format!("kept {rel}: it is the good kernel now"))
}

/// Write linux-good.img back into boot_a (design 3.6 Rollback). A pending or
/// running trial is recorded as failed.
pub fn rollback(dev: &Device, state: &Path, power: Res<()>) -> Res<String> {
    power?;
    let mut st = KernelState::load(state);
    let g = good_image(state, &st, dev.size)?;
    dev.write_a(&g)?;
    if !st.trial.is_empty() {
        st.failed = st.trial.clone();
        st.failed_sha256 = st.trial_sha256.clone();
        st.clear_trial();
    }
    st.save(state)?;
    Ok(format!("boot_a holds {} again (linux-good.img); restart to use it", st.good))
}

/// The battery allows a boot_a write: at least 30 % or on external power.
pub fn power_ok() -> Res<()> {
    if crate::features::external_power() {
        return Ok(());
    }
    match crate::features::battery_info() {
        Ok(i) if i.capacity >= MIN_BATTERY => Ok(()),
        Ok(i) => Err(format!("battery at {} %: connect a charger first (writing boot_a needs {MIN_BATTERY} % or external power)", i.capacity)),
        Err(e) => Err(format!("battery state unknown ({e}); connect a charger first")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootimg::tests::{fake_kernel, fake_kernel_with_initramfs, fake_stock};

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("tb323fu-kern-{tag}-{}-{}", std::process::id(),
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

    fn keypair() -> (minisign::KeyPair, minisign_verify::PublicKey) {
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let pk = minisign_verify::PublicKey::from_base64(&kp.pk.to_base64()).unwrap();
        (kp, pk)
    }

    fn sign(kp: &minisign::KeyPair, data: &[u8]) -> String {
        minisign::sign(None, &kp.sk, std::io::Cursor::new(data), Some("test"), None).unwrap().into_string()
    }

    #[test]
    fn helpers() {
        assert!(version_cmp("0.10.0", "0.9.3").is_gt());
        assert!(version_cmp("0.1", "0.1.0").is_eq());
        assert!(version_cmp("0.1.0", "0.2.0").is_lt());
        assert!(url_ok("http://192.168.7.1:8000/repos/a/b/releases"));
        assert!(!url_ok("ftp://x"));
        assert!(!url_ok("https://x y"));
        assert!(safe_name("Image-tb323fu-t28.gz"));
        assert!(!safe_name("../x") && !safe_name(".hidden") && !safe_name("a/b"));
        let r = Running { release: "7.3.0-rc4-tb323fu-t27".into(), version: "v".into() };
        assert_eq!(running_serial(&KernelState::default(), &r), 27);
        assert_eq!(serial_of_release("7.3.0-rc4-tb323fu-t30-jh"), 0);
        assert!(KEYS.is_empty(), "no key is trusted by default");
        let s = Source::parse("github:joonhoekim/tb323fu-linux", "https://api.github.com/").unwrap();
        assert_eq!(s.releases_url(), "https://api.github.com/repos/joonhoekim/tb323fu-linux/releases");
        assert!(Source::parse("joonhoekim/tb323fu-linux", "https://api.github.com").is_err());
        assert!(Source::parse("github:a/../b", "https://api.github.com").is_err());
        assert!(Source::parse("github:a/b", "ftp://x").is_err());
    }

    #[test]
    fn state_roundtrip() {
        let t = "good=7.3.0-t27\ngood_version=Linux version 7.3.0-t27 (a@b) (clang) #1 SMP\ntrial=7.3.0-t28\ntrial_keep=1\ntrial_label=my build\ntries=1\nmax=2\nfuture_key=x\n";
        let s = KernelState::parse(t);
        assert_eq!(s.tries, 1);
        assert!(s.trial_keep && s.keep_needed());
        assert_eq!(s.trial_label, "my build");
        assert_eq!(s.good_version, "Linux version 7.3.0-t27 (a@b) (clang) #1 SMP");
        assert_eq!(s.other, vec![("future_key".to_string(), "x".to_string())]);
        assert_eq!(KernelState::parse(&s.to_text()), s);
        assert_eq!(KernelState::parse("").max, 2);
        assert!(KernelState::parse("trial=x\ntrial_channel=testing\n").keep_needed(), "testing waits for Keep (older records)");
        assert!(!KernelState::parse("trial=x\ntrial_channel=stable\n").keep_needed());
    }

    const API: &str = "https://api.github.com";
    fn src() -> Source {
        Source::parse("github:o/r", API).unwrap()
    }
    fn gh_asset(id: u32, name: &str, size: u64, digest: Option<&str>) -> serde_json::Value {
        let mut a = serde_json::json!({"name": name, "size": size, "url": format!("{API}/repos/o/r/releases/assets/{id}"),
            "browser_download_url": format!("https://github.com/o/r/releases/download/x/{name}")});
        if let Some(d) = digest {
            a["digest"] = serde_json::json!(format!("sha256:{d}"));
        }
        a
    }

    #[test]
    fn release_list() {
        let list = serde_json::json!([
            {"tag_name": "kernel-t32", "name": "t32 test", "draft": false, "prerelease": true, "published_at": "2026-10-04T10:00:00Z",
             "body": "## t32\n<!-- tb323fu: min_helper=0.2.0 min_platform=0.1.5 -->", "html_url": "https://github.com/o/r/releases/tag/kernel-t32",
             "assets": [gh_asset(1, "Image-tb323fu-t32", 1000, None), gh_asset(2, "Image-tb323fu-t32.gz", 400, None), gh_asset(3, "SHA256SUMS", 200, None)]},
            {"tag_name": "kernel-t33", "draft": true, "prerelease": false, "assets": [gh_asset(4, "Image-tb323fu-t33", 1000, None), gh_asset(5, "SHA256SUMS", 1, None)]},
            {"tag_name": "helper-v0.3.0", "draft": false, "prerelease": false, "assets": []},
            {"tag_name": "helper-v0.4.0", "draft": false, "prerelease": true, "assets": []},
            {"tag_name": "kernel-t31", "name": "", "draft": false, "prerelease": false, "published_at": "2026-10-03T10:00:00Z", "body": "## t31",
             "html_url": "https://github.com/o/r/releases/tag/kernel-t31",
             "assets": [gh_asset(6, "Image-tb323fu-t31", 1000, Some(&"a".repeat(64))), gh_asset(7, "SHA256SUMS", 100, None), gh_asset(8, "SHA256SUMS.minisig", 100, None)]},
            {"tag_name": "mixed", "draft": false, "prerelease": false, "assets": [gh_asset(9, "Image-tb323fu-t40", 1, None), gh_asset(10, "Image-tb323fu-t41.gz", 1, None), gh_asset(11, "SHA256SUMS", 1, None)]},
            {"tag_name": "no-sums", "draft": false, "prerelease": false, "assets": [gh_asset(12, "Image-tb323fu-t50", 1, None)]},
            {"tag_name": "elsewhere", "draft": false, "prerelease": false, "assets": [
                {"name": "Image-tb323fu-t60", "size": 1, "url": "https://evil.example/x"}, gh_asset(13, "SHA256SUMS", 1, None)]},
            {"tag_name": "docs-only", "draft": false, "prerelease": false, "assets": [gh_asset(14, "notes.pdf", 1, None)]}
        ]);
        let (rels, helper) = parse_releases(list.to_string().as_bytes(), &src()).unwrap();
        assert_eq!(helper.as_deref(), Some("0.3.0"), "pre-releases of the helper are not announced");
        assert_eq!(rels.iter().map(|r| r.tag.as_str()).collect::<Vec<_>>(), ["kernel-t32", "kernel-t31"]);
        let t32 = &rels[0];
        assert_eq!((t32.serial, t32.ktag.as_str(), t32.kernel.name.as_str(), t32.prerelease), (32, "t32", "Image-tb323fu-t32.gz", true));
        assert_eq!((t32.min_helper.as_str(), t32.min_platform.as_str(), t32.title.as_str()), ("0.2.0", "0.1.5", "t32 test"));
        let t31 = &rels[1];
        assert_eq!((t31.title.as_str(), t31.kernel.digest.len(), t31.sig.is_some()), ("kernel-t31", 64, true));
        assert_eq!(pick(&rels, "stable").unwrap().tag, "kernel-t31");
        assert_eq!(pick(&rels, "testing").unwrap().tag, "kernel-t32");
        assert!(requirements(t32, "0.1.0", None).unwrap_err().contains("helper"));
        assert!(requirements(t32, "0.2.0", Some("0.1.0")).unwrap_err().contains("platform"));
        assert!(requirements(t32, "0.2.0", None).is_ok(), "unknown platform version: allowed");
        assert!(parse_releases(b"{\"message\": \"API rate limit exceeded\"}", &src()).is_err());
        // a test API (file://) works the same way
        let f = Source::parse("github:o/r", "file:///srv/api").unwrap();
        assert_eq!(f.asset_prefix(), "file:///srv/api/");
    }

    #[test]
    fn sums_signature_and_kernel_check() {
        let v = "Linux version 7.3.0-rc4-tb323fu-t31 (u@h) (clang) #1 SMP PREEMPT Fri Oct 3";
        let k = fake_kernel(100_000, v);
        let sums = format!("{}  Image-tb323fu-t31\n{} *other\nnot a line\n", sha_hex(&k), "b".repeat(64));
        let m = parse_sums(&sums);
        assert_eq!(m.len(), 2);
        let r = Release { tag: "kernel-t31".into(), ktag: "t31".into(), serial: 31,
            kernel: Asset { name: "Image-tb323fu-t31".into(), size: k.len() as u64, url: String::new(), digest: String::new() }, ..Default::default() };
        assert_eq!(check_kernel_file(&r, &m, &k).unwrap(), ("7.3.0-rc4-tb323fu-t31".to_string(), v.to_string()));
        let mut k2 = k.clone();
        k2[5000] ^= 1;
        assert!(check_kernel_file(&r, &m, &k2).unwrap_err().contains("SHA256SUMS"));
        let mut gd = r.clone();
        gd.kernel.digest = "c".repeat(64);
        assert!(check_kernel_file(&gd, &m, &k).unwrap_err().contains("digest"));
        let mut wrong = r.clone();
        wrong.ktag = "t32".into();
        assert!(check_kernel_file(&wrong, &m, &k).unwrap_err().contains("not a -tb323fu-t32"));
        let mut unlisted = r.clone();
        unlisted.kernel.name = "Image-tb323fu-t31.gz".into();
        assert!(check_kernel_file(&unlisted, &m, &k).unwrap_err().contains("not listed"));

        // signatures: off by default; on, they need a key and a good signature
        let (kp, pk) = keypair();
        let (kp2, _) = keypair();
        assert!(read_sums(sums.as_bytes(), None, false, &[]).is_ok(), "no signature needed by default");
        assert!(read_sums(sums.as_bytes(), None, true, &[]).unwrap_err().contains("no public key"));
        assert!(read_sums(sums.as_bytes(), None, true, &[pk.clone()]).unwrap_err().contains("no SHA256SUMS.minisig"));
        assert!(read_sums(sums.as_bytes(), Some(&sign(&kp, sums.as_bytes())), true, &[pk.clone()]).is_ok());
        assert!(read_sums(sums.as_bytes(), Some(&sign(&kp2, sums.as_bytes())), true, &[pk.clone()]).unwrap_err().contains("trusted key"));
        let changed = sums.replace("other", "other2");
        assert!(read_sums(changed.as_bytes(), Some(&sign(&kp, sums.as_bytes())), true, &[pk]).is_err(), "tampered");
        assert!(read_sums(b"nothing\n", None, false, &[]).is_err());
    }

    #[test]
    fn local_files() {
        let run = Running { release: "7.3.0-rc4-tb323fu-t30".into(), version: "Linux version 7.3.0-rc4-tb323fu-t30 (u@h) (clang) #2 SMP".into() };
        let v = "Linux version 7.3.0-rc4-tb323fu-t30-jh (u@h) (clang) #5 SMP PREEMPT Fri Oct 3";
        let shared = fake_kernel_with_initramfs(200_000, v, &["init", "lib/modules/7.3.0-rc4-tb323fu-t30-jh.sqfs"]);
        let l = inspect_local(&shared, &run).unwrap();
        assert_eq!((l.release.as_str(), l.banner.as_str(), l.format.as_str(), l.shared_modules), ("7.3.0-rc4-tb323fu-t30-jh", v, "Image", true));
        assert_eq!(l.warnings.len(), 1, "{:?}", l.warnings);
        assert!(l.warnings[0].contains("not a -tb323fu-tNN release"));

        let own = fake_kernel_with_initramfs(200_000, &run.version, &["init"]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&own).unwrap();
        let gz = gz.finish().unwrap();
        let l = inspect_local(&gz, &run).unwrap();
        assert_eq!((l.format.as_str(), l.shared_modules, l.kernel.clone()), ("Image.gz", false, gz.clone()));
        assert!(l.warnings.iter().any(|w| w.contains("No shared modules")) && l.warnings.iter().any(|w| w.contains("running now")), "{:?}", l.warnings);

        // a boot image: its kernel field
        let stock = fake_stock(2 << 20, &fake_kernel(100_000, "Linux version 6.6.0-android (b@h) (clang) #1 SMP"));
        let img = bootimg::StockBoot::parse(&stock).unwrap().repack(&shared).unwrap();
        let l = inspect_local(&img, &run).unwrap();
        assert_eq!((l.format.as_str(), l.kernel == shared), ("boot image", true));
        assert!(l.warnings[0].contains("only its kernel is used"));

        assert!(inspect_local(b"hello", &run).is_err());
        assert!(inspect_local(&[0x04, 0x22, 0x4d, 0x18, 0, 0, 0, 0], &run).unwrap_err().contains("LZ4"));
        assert!(inspect_local(&fake_kernel(5000, "nothing"), &run).unwrap_err().contains("banner"));
    }

    /// A fake tablet: boot_a (running t27), boot_b (stock), the state dir.
    struct Fake {
        _t: Tmp,
        dev: Device,
        state: PathBuf,
        stock_sha: String,
        run27: Running,
    }

    const SIZE: usize = 2 << 20;

    fn fake() -> Fake {
        let t = Tmp::new("dev");
        let stock_k = fake_kernel(300_000, "Linux version 6.6.0-android (b@h) (clang) #1 SMP PREEMPT");
        let stock = fake_stock(SIZE, &stock_k);
        let v27 = "Linux version 7.3.0-rc4-tb323fu-t27 (u@h) (clang) #1 SMP PREEMPT Wed Oct 1";
        let k27 = fake_kernel(400_000, v27);
        let a = bootimg::StockBoot::parse(&stock).unwrap().repack(&k27).unwrap();
        fs::write(t.0.join("boot_a"), &a).unwrap();
        fs::write(t.0.join("boot_b"), &stock).unwrap();
        let dev = Device { boot_a: t.0.join("boot_a"), boot_b: t.0.join("boot_b"), size: SIZE as u64 };
        let state = t.0.join("state");
        Fake { stock_sha: sha_hex(&stock), dev, state, run27: Running { release: "7.3.0-rc4-tb323fu-t27".into(), version: v27.into() }, _t: t }
    }

    fn k28() -> (Vec<u8>, Candidate, Running) {
        let v = "Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang) #1 SMP PREEMPT Thu Oct 2";
        let k = fake_kernel(500_000, v);
        let c = Candidate { release: "7.3.0-rc4-tb323fu-t28".into(), version: v.into(), serial: 28, channel: "testing".into(), keep: true, label: String::new() };
        (k, c, Running { release: "7.3.0-rc4-tb323fu-t28".into(), version: v.into() })
    }

    #[test]
    fn install_confirm_rollback() {
        let f = fake();
        let (k, c, run28) = k28();
        let a27 = fs::read(&f.dev.boot_a).unwrap();

        // refusals: no way back, wrong Android hash, low battery
        assert!(install(&f.dev, "", &f.run27, &f.state, &c, &k, Ok(())).unwrap_err().contains("Android image hash"));
        assert!(install(&f.dev, &"0".repeat(64), &f.run27, &f.state, &c, &k, Ok(())).unwrap_err().contains("boot_b"));
        assert!(install(&f.dev, &f.stock_sha, &f.run27, &f.state, &c, &k, Err("battery at 10 %".into())).is_err());
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27, "nothing written");

        // install t28 while t27 runs from boot_a
        let msg = install(&f.dev, &f.stock_sha, &f.run27, &f.state, &c, &k, Ok(())).unwrap();
        assert!(msg.contains("t28 installed") && msg.contains("press Keep"), "{msg}");
        assert_eq!(fs::read(f.state.join(GOOD_IMG)).unwrap(), a27, "linux-good = the t27 image");
        let st = KernelState::load(&f.state);
        assert_eq!(st.good, "7.3.0-rc4-tb323fu-t27");
        assert_eq!(st.good_sha256, sha_hex(&a27));
        assert_eq!(st.good_serial, 27);
        assert_eq!(st.trial, "7.3.0-rc4-tb323fu-t28");
        assert_eq!(st.trial_version, run28.version);
        assert_eq!(st.trial_channel, "testing");
        assert!(st.trial_keep);
        assert_eq!(st.tries, 0);
        let a28 = fs::read(&f.dev.boot_a).unwrap();
        assert_eq!(sha_hex(&a28), st.trial_sha256);
        assert_eq!(bootimg::boot_kernel(&a28).unwrap(), &k[..]);
        assert_eq!(&a28[SIZE - 64..SIZE - 60], b"AVBf");

        // pending (t27 still running, boot_a = t28): installing again keeps linux-good
        let again = install(&f.dev, &f.stock_sha, &f.run27, &f.state, &c, &k, Ok(()));
        assert!(again.is_ok(), "{again:?}");
        assert_eq!(fs::read(f.state.join(GOOD_IMG)).unwrap(), a27);
        assert_eq!(confirm(&f.dev, &f.run27, &f.state).unwrap_err(), "7.3.0-rc4-tb323fu-t28 is installed but not running yet; restart first");

        // booted t28 (the initramfs counted a try): on trial, no new install
        let mut st = KernelState::load(&f.state);
        st.tries = 1;
        st.save(&f.state).unwrap();
        assert_eq!(running_serial(&st, &run28), 28);
        assert!(install(&f.dev, &f.stock_sha, &run28, &f.state, &c, &k, Ok(())).unwrap_err().contains("still on trial"));

        // keep it
        assert!(confirm(&f.dev, &run28, &f.state).unwrap().contains("kept"));
        let st = KernelState::load(&f.state);
        assert_eq!((st.good.as_str(), st.trial.as_str(), st.tries, st.good_serial, st.trial_keep), ("7.3.0-rc4-tb323fu-t28", "", 0, 28, false));
        assert_eq!(fs::read(f.state.join(GOOD_IMG)).unwrap(), a28);
        assert_eq!(confirm(&f.dev, &run28, &f.state).unwrap(), "nothing to confirm: no kernel on trial");

        // a manual rollback writes linux-good back (here: t28 itself) and records nothing
        assert!(rollback(&f.dev, &f.state, Ok(())).is_ok());
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a28);
    }

    #[test]
    fn local_install_keeps_its_label() {
        let f = fake();
        let v = "Linux version 7.3.0-rc4-tb323fu-t27 (me@pc) (clang) #9 SMP PREEMPT Fri Oct 3";
        let k = fake_kernel(450_000, v);
        let run = f.run27.clone();
        let l = inspect_local(&k, &run).unwrap();
        let c = Candidate { release: l.release.clone(), version: l.banner.clone(), serial: serial_of_release(&l.release), channel: "local".into(),
            keep: false, label: "speaker test".into() };
        let msg = install(&f.dev, &f.stock_sha, &run, &f.state, &c, &l.kernel, Ok(())).unwrap();
        assert!(msg.contains("90 s"), "{msg}");
        let st = KernelState::load(&f.state);
        assert_eq!((st.trial_channel.as_str(), st.trial_keep, st.trial_label.as_str()), ("local", false, "speaker test"));
        // same release string, another build: the trial is told apart by its banner
        let run9 = Running { release: run.release.clone(), version: v.into() };
        assert!(st.trial_running(&run9) && !st.trial_running(&run));
        assert!(confirm(&f.dev, &run9, &f.state).is_ok());
        let st = KernelState::load(&f.state);
        assert_eq!((st.good_label.as_str(), st.good_version.as_str(), st.trial_label.as_str()), ("speaker test", v, ""));
    }

    #[test]
    fn rollback_of_a_pending_trial_and_corrupt_good() {
        let f = fake();
        let (k, c, _) = k28();
        let a27 = fs::read(&f.dev.boot_a).unwrap();
        install(&f.dev, &f.stock_sha, &f.run27, &f.state, &c, &k, Ok(())).unwrap();
        let msg = rollback(&f.dev, &f.state, Ok(())).unwrap();
        assert!(msg.contains("t27"), "{msg}");
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27);
        let st = KernelState::load(&f.state);
        assert_eq!((st.trial.as_str(), st.failed.as_str(), st.trial_keep), ("", "7.3.0-rc4-tb323fu-t28", false));

        // a damaged linux-good.img is never written
        let mut g = fs::read(f.state.join(GOOD_IMG)).unwrap();
        g[100] ^= 1;
        fs::write(f.state.join(GOOD_IMG), g).unwrap();
        assert!(rollback(&f.dev, &f.state, Ok(())).unwrap_err().contains("does not match"));
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27);
    }

    #[test]
    fn reinstalling_a_failed_release_forgets_the_failure() {
        let f = fake();
        let (k, c, _) = k28();
        install(&f.dev, &f.stock_sha, &f.run27, &f.state, &c, &k, Ok(())).unwrap();
        rollback(&f.dev, &f.state, Ok(())).unwrap();
        assert_eq!(KernelState::load(&f.state).failed, "7.3.0-rc4-tb323fu-t28");
        install(&f.dev, &f.stock_sha, &f.run27, &f.state, &c, &k, Ok(())).unwrap();
        let st = KernelState::load(&f.state);
        assert_eq!((st.trial.as_str(), st.failed.as_str(), st.failed_sha256.as_str()), ("7.3.0-rc4-tb323fu-t28", "", ""));
    }

    #[test]
    fn install_needs_a_known_good_image() {
        let f = fake();
        let (k, c, _) = k28();
        // boot_a holds something that is not the running kernel, and no linux-good
        let other = Running { release: "7.3.0-dev".into(), version: "Linux version 7.3.0-dev (x@y) (c) #9".into() };
        let e = install(&f.dev, &f.stock_sha, &other, &f.state, &c, &k, Ok(())).unwrap_err();
        assert!(e.contains("restart once"), "{e}");
        assert!(!f.state.join(STATE_FILE).exists());
    }

    #[test]
    fn readback_mismatch_restores_good() {
        let f = fake();
        let (k, c, _) = k28();
        let a27 = fs::read(&f.dev.boot_a).unwrap();
        CORRUPT_NEXT_WRITE.with(|x| x.set(true));
        let e = install(&f.dev, &f.stock_sha, &f.run27, &f.state, &c, &k, Ok(())).unwrap_err();
        assert!(e.contains("does not read back") && e.contains("previous kernel again"), "{e}");
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27, "linux-good written back");
        assert!(KernelState::load(&f.state).trial.is_empty(), "no trial record after a failed write");
    }
}
