// SPDX-License-Identifier: MIT
//! Kernel updates (docs/notes/kernel-updates-design.md, section 3): the signed
//! release index and manifests, the trial-boot record on the state root, and
//! the writes to `boot_a` -- install, confirm ("keep"), rollback.
//!
//! Nothing here touches the network: the fetch unit downloads into a cache
//! directory, and this code only verifies what it finds there (minisign
//! signature over the index and each manifest, SHA-256 of the kernel file from
//! the signed manifest, the kernel's own version banner).
//!
//! Releases ship a kernel `Image`, never a boot image: the boot image is made
//! here from the user's own stock image, the copy in `boot_b` (checked against
//! the recorded Android hash first), exactly as `tools/boot-repack-kernel.py`
//! does at install time.
//!
//! State on the state root (`/var/lib/tb323fu`, see `boot::with_state_dir`):
//!
//! ```text
//! kernel-state      good=, good_sha256=, good_version=, good_serial=,
//!                   trial=, trial_sha256=, trial_version=, trial_serial=,
//!                   trial_channel=, tries=, max=, failed=, failed_sha256=
//! linux-good.img    the boot image that last ran confirmed (+ .sha256)
//! ```
//!
//! The same file is read and written by the initramfs (trial counter and
//! automatic rollback), the layer-1 confirm script and Android's Switch to
//! Linux; `*_version` is the kernel's `/proc/version` line, which tells two
//! builds with the same release string apart.

use crate::bootimg;
use crate::sys;
use serde::{Deserialize, Serialize};
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
/// Verified downloads and the last verified index, on the running root.
pub const STAGE_DIR: &str = "/var/lib/tb323fu/kernel";
/// The daemon's download list for the fetch unit: `NAME MAXBYTES URL` lines.
pub const FETCH_LIST: &str = "/run/tb323fu/kernel-fetch.list";
pub const MAX_KERNEL_FILE: u64 = 100 << 20;
pub const MAX_META_FILE: u64 = 1 << 20;
pub const DEFAULT_MAX_TRIES: u32 = 2;
/// Battery level below which nothing is written to `boot_a` without a charger.
pub const MIN_BATTERY: u32 = 30;

/// The project's kernel signing keys (minisign, Ed25519). A rotation adds the
/// new key here in a helper release, signs with both for a cycle, then drops
/// the old one. Key id A5D2DA7287637413.
pub const KEYS: &[&str] = &["RWQTdGOHctrSpdjtMrCjOtfy+pa7Wtbzg49vcR4WX9QaVLK1Mjh8J/qJ"];
/// More trusted keys, one minisign `.pub` file each (`kernel-*.pub`): the
/// packaged copy, and an administrator's own (e.g. a local test channel).
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

/// Unix seconds of "YYYY-MM-DD" or "YYYY-MM-DDTHH:MM:SSZ" (UTC).
pub fn parse_time(s: &str) -> Option<u64> {
    let s = s.trim();
    let (date, time) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t.trim_end_matches('Z'))),
        None => (s, None),
    };
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, dd) = (d.next()??, d.next()??, d.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&dd) || d.next().is_some() {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + dd - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let mut secs = days * 86400;
    if let Some(t) = time {
        let mut p = t.split(':').map(|x| x.parse::<i64>().ok());
        let (h, mi, se) = (p.next()??, p.next().flatten().unwrap_or(0), p.next().flatten().unwrap_or(0));
        secs += h * 3600 + mi * 60 + se;
    }
    u64::try_from(secs).ok()
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `rel` relative to the URL `base` (a file name or path next to it), or
/// `rel` itself when it is already a URL.
pub fn resolve(base: &str, rel: &str) -> String {
    if rel.contains("://") {
        return rel.to_string();
    }
    match base.rfind('/') {
        Some(i) => format!("{}/{}", &base[..i], rel.trim_start_matches("./")),
        None => rel.to_string(),
    }
}

/// Allowed URL schemes for the index and its files (everything is verified
/// by signature and hash, so the transport only has to deliver).
pub fn url_ok(u: &str) -> bool {
    (u.starts_with("https://") || u.starts_with("http://") || u.starts_with("file:///"))
        && !u.chars().any(|c| c.is_whitespace() || c.is_control())
}

// ------------------------------------------------------------------ signatures

pub fn trusted_keys() -> Vec<minisign_verify::PublicKey> {
    let mut v: Vec<_> = KEYS.iter().filter_map(|k| minisign_verify::PublicKey::from_base64(k).ok()).collect();
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

// ------------------------------------------------------------------ index, manifest

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChannelEntry {
    pub tag: String,
    pub serial: u64,
    /// URL of the manifest (relative: next to the index)
    pub manifest: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HelperInfo {
    pub latest: String,
    pub notes: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Index {
    pub format: u32,
    pub generated: String,
    pub expires: String,
    pub channels: BTreeMap<String, ChannelEntry>,
    pub helper: HelperInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct KernelFile {
    /// file name (also the name in the cache and staging directories)
    pub file: String,
    /// URL, default: `file` next to the manifest
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Manifest {
    pub format: u32,
    pub tag: String,
    /// `uname -r` of the kernel
    pub release: String,
    /// its `/proc/version` line (informational)
    pub build: String,
    pub serial: u64,
    pub channel: String,
    pub kernel: KernelFile,
    /// SHA-256 of the uncompressed Image (optional)
    pub image_sha256: String,
    pub min_helper: String,
    pub min_platform: String,
    /// release notes, Markdown
    pub notes: String,
    pub notes_url: String,
    pub source_tag: String,
    pub source_commit: String,
    pub gpl_sources: Vec<String>,
}

/// A verified index and whether it is past its `expires` date.
pub fn index_from(data: &[u8], sig: &str, keys: &[minisign_verify::PublicKey], now: u64) -> Res<(Index, bool)> {
    verify(data, sig, keys).map_err(|e| format!("index: {e}"))?;
    let i: Index = serde_json::from_slice(data).map_err(|e| format!("index: {e}"))?;
    if i.format != 1 {
        return Err(format!("index format {} not supported (helper too old?)", i.format));
    }
    let exp = parse_time(&i.expires).ok_or("index: no valid expires date")?;
    for (name, c) in &i.channels {
        if !safe_name(name) || !safe_name(&c.tag) || c.manifest.is_empty() {
            return Err(format!("index: bad channel entry {name}"));
        }
    }
    Ok((i, now > exp))
}

/// A verified manifest.
pub fn manifest_from(data: &[u8], sig: &str, keys: &[minisign_verify::PublicKey]) -> Res<Manifest> {
    verify(data, sig, keys).map_err(|e| format!("manifest: {e}"))?;
    let m: Manifest = serde_json::from_slice(data).map_err(|e| format!("manifest: {e}"))?;
    if m.format != 1 {
        return Err(format!("manifest format {} not supported (helper too old?)", m.format));
    }
    if !safe_name(&m.tag) || !safe_name(&m.kernel.file) || m.kernel.file.ends_with(".json") || m.kernel.file.ends_with(".minisig") {
        return Err("manifest: bad tag or file name".into());
    }
    if !is_hex64(&m.kernel.sha256) || (!m.image_sha256.is_empty() && !is_hex64(&m.image_sha256)) {
        return Err("manifest: bad sha256".into());
    }
    if m.kernel.size == 0 || m.kernel.size > MAX_KERNEL_FILE || m.release.is_empty() || m.release.contains(char::is_whitespace) {
        return Err("manifest: bad size or release".into());
    }
    Ok(m)
}

/// Check a downloaded kernel file against its manifest; the raw Image.
pub fn check_kernel_file(m: &Manifest, data: &[u8]) -> Res<Vec<u8>> {
    if data.len() as u64 != m.kernel.size {
        return Err(format!("{}: {} bytes, the manifest says {}", m.kernel.file, data.len(), m.kernel.size));
    }
    if sha_hex(data) != m.kernel.sha256 {
        return Err(format!("{}: SHA-256 does not match the signed manifest", m.kernel.file));
    }
    let raw = bootimg::kernel_raw(data)?;
    if !m.image_sha256.is_empty() && sha_hex(&raw) != m.image_sha256 {
        return Err(format!("{}: the uncompressed Image does not match the manifest", m.kernel.file));
    }
    if bootimg::banner(&raw, &m.release).is_none() {
        return Err(format!("{}: the kernel is not release {}", m.kernel.file, m.release));
    }
    Ok(raw)
}

/// The manifest's needs against this helper and the platform files.
pub fn requirements(m: &Manifest, helper: &str, platform: Option<&str>) -> Res<()> {
    if !m.min_helper.is_empty() && version_cmp(helper, &m.min_helper).is_lt() {
        return Err(format!("{} needs tb323fu-helper {} or newer (this is {helper}); update the helper first", m.tag, m.min_helper));
    }
    if let (false, Some(p)) = (m.min_platform.is_empty(), platform) {
        if version_cmp(p, &m.min_platform).is_lt() {
            return Err(format!("{} needs tb323fu-platform {} or newer (installed: {p}); update it first", m.tag, m.min_platform));
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
    pub trial: String,
    pub trial_sha256: String,
    pub trial_version: String,
    pub trial_serial: u64,
    pub trial_channel: String,
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
                "trial" => s.trial = v,
                "trial_sha256" => s.trial_sha256 = v,
                "trial_version" => s.trial_version = v,
                "trial_serial" => s.trial_serial = n(),
                "trial_channel" => s.trial_channel = v,
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
        put("trial", &self.trial);
        put("trial_sha256", &self.trial_sha256);
        put("trial_version", &self.trial_version);
        put("trial_serial", &num(self.trial_serial));
        put("trial_channel", &self.trial_channel);
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

/// Install a verified release kernel into `boot_a` as a trial (design 3.4).
/// `state` is the state root's /var/lib/tb323fu; `power` the battery check.
pub fn install(dev: &Device, android_hash: &str, run: &Running, state: &Path, m: &Manifest, kernel_file: &[u8], power: Res<()>) -> Res<String> {
    power?;
    let mut st = KernelState::load(state);
    if st.trial_running(run) {
        return Err(format!("the running kernel {} is still on trial: wait until it is confirmed (or keep it) first", st.trial));
    }
    let raw = check_kernel_file(m, kernel_file)?;
    let version = bootimg::banner(&raw, &m.release).ok_or("no version banner in the kernel")?;
    drop(raw);
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

    st.trial = m.release.clone();
    st.trial_sha256 = img_sha;
    st.trial_version = version;
    st.trial_serial = m.serial;
    // the channel installed from: the daemon puts its configured channel into m.channel
    st.trial_channel = if m.channel.is_empty() { "stable".into() } else { m.channel.clone() };
    st.tries = 0;
    if st.max == 0 {
        st.max = DEFAULT_MAX_TRIES;
    }
    st.save(state)?;

    if let Err(e) = dev.write_a(&img) {
        let back = good_image(state, &st, dev.size).and_then(|g| dev.write_a(&g));
        st.clear_trial();
        let _ = st.save(state);
        return Err(match back {
            Ok(()) => format!("{e}; boot_a holds the previous kernel again"),
            Err(e2) => format!("{e}; restoring linux-good.img failed too ({e2}) -- do NOT restart; use fastboot or EDL (docs/recovery.md)"),
        });
    }
    Ok(format!("{} installed in boot_a; it is tried on the next start ({} tries, then back to {})", m.release, st.max, st.good))
}

/// Confirm the running trial kernel ("keep"): its image in boot_a becomes
/// linux-good.img. The layer-1 script tb323fu-kernel-confirm does the same
/// for the stable channel 90 s after a system has started.
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
    use crate::bootimg::tests::{fake_kernel, fake_stock};

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
        assert_eq!(parse_time("1970-01-02"), Some(86400));
        assert_eq!(parse_time("2026-10-02T12:00:00Z"), Some(1790942400));
        assert_eq!(parse_time("2026-13-01"), None);
        assert_eq!(resolve("https://x.io/k/index.json", "kernel-t28/m.json"), "https://x.io/k/kernel-t28/m.json");
        assert_eq!(resolve("file:///srv/ch/index.json", "./a.json"), "file:///srv/ch/a.json");
        assert_eq!(resolve("https://x.io/k/index.json", "https://y.io/m.json"), "https://y.io/m.json");
        assert!(url_ok("http://192.168.7.1:8000/index.json"));
        assert!(!url_ok("ftp://x"));
        assert!(!url_ok("https://x y"));
        assert!(safe_name("Image-tb323fu-t28.gz"));
        assert!(!safe_name("../x") && !safe_name(".hidden") && !safe_name("a/b"));
        let r = Running { release: "7.3.0-rc4-tb323fu-t27".into(), version: "v".into() };
        assert_eq!(running_serial(&KernelState::default(), &r), 27);
    }

    #[test]
    fn state_roundtrip() {
        let t = "good=7.3.0-t27\ngood_version=Linux version 7.3.0-t27 (a@b) (clang) #1 SMP\ntrial=7.3.0-t28\ntries=1\nmax=2\nfuture_key=x\n";
        let s = KernelState::parse(t);
        assert_eq!(s.tries, 1);
        assert_eq!(s.good_version, "Linux version 7.3.0-t27 (a@b) (clang) #1 SMP");
        assert_eq!(s.other, vec![("future_key".to_string(), "x".to_string())]);
        assert_eq!(KernelState::parse(&s.to_text()), s);
        assert_eq!(KernelState::parse("").max, 2);
    }

    fn manifest_for(kfile: &[u8], release: &str, serial: u64, channel: &str) -> Manifest {
        Manifest {
            format: 1,
            tag: format!("kernel-t{serial}"),
            release: release.into(),
            serial,
            channel: channel.into(),
            kernel: KernelFile { file: format!("Image-tb323fu-t{serial}"), url: String::new(), sha256: sha_hex(kfile), size: kfile.len() as u64 },
            min_helper: "0.1.0".into(),
            notes: "## t28\n- fixes".into(),
            ..Default::default()
        }
    }

    #[test]
    fn signed_index_and_manifest() {
        let (kp, pk) = keypair();
        let (kp2, _) = keypair();
        let keys = vec![pk];
        let idx = br#"{"format":1,"expires":"2026-11-01","channels":{"stable":{"tag":"kernel-t28","serial":28,"manifest":"kernel-t28/m.json"}},"helper":{"latest":"0.2.0"}}"#;
        let (i, expired) = index_from(idx, &sign(&kp, idx), &keys, parse_time("2026-10-02").unwrap()).unwrap();
        assert!(!expired);
        assert_eq!(i.channels["stable"].serial, 28);
        assert_eq!(i.helper.latest, "0.2.0");
        assert!(index_from(idx, &sign(&kp, idx), &keys, parse_time("2026-11-02").unwrap()).unwrap().1, "expired");
        assert!(index_from(idx, &sign(&kp2, idx), &keys, 0).unwrap_err().contains("trusted key"), "other key");
        let mut bad = idx.to_vec();
        bad[30] ^= 1;
        assert!(index_from(&bad, &sign(&kp, idx), &keys, 0).is_err(), "tampered");

        let k = fake_kernel(100_000, "Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang) #1 SMP PREEMPT");
        let m = manifest_for(&k, "7.3.0-rc4-tb323fu-t28", 28, "stable");
        let mj = serde_json::to_vec(&m).unwrap();
        let m2 = manifest_from(&mj, &sign(&kp, &mj), &keys).unwrap();
        assert_eq!(m2, m);
        assert!(check_kernel_file(&m, &k).is_ok());
        let mut k2 = k.clone();
        k2[5000] ^= 1;
        assert!(check_kernel_file(&m, &k2).unwrap_err().contains("SHA-256"));
        let wrong = manifest_for(&k, "7.3.0-rc4-tb323fu-t29", 29, "stable");
        assert!(check_kernel_file(&wrong, &k).unwrap_err().contains("not release"));
        let mut evil = m.clone();
        evil.kernel.file = "../../etc/passwd".into();
        let ej = serde_json::to_vec(&evil).unwrap();
        assert!(manifest_from(&ej, &sign(&kp, &ej), &keys).is_err());
        assert!(requirements(&m, "0.1.0", None).is_ok());
        let mut needs = m.clone();
        needs.min_helper = "0.3.0".into();
        needs.min_platform = "0.2.0".into();
        assert!(requirements(&needs, "0.2.9", None).unwrap_err().contains("helper"));
        assert!(requirements(&needs, "0.3.0", Some("0.1.0")).unwrap_err().contains("platform"));
        assert!(requirements(&needs, "0.3.0", None).is_ok(), "unknown platform version: allowed");
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

    fn k28() -> (Vec<u8>, Manifest, Running) {
        let v = "Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang) #1 SMP PREEMPT Thu Oct 2";
        let k = fake_kernel(500_000, v);
        let m = manifest_for(&k, "7.3.0-rc4-tb323fu-t28", 28, "testing");
        (k, m, Running { release: "7.3.0-rc4-tb323fu-t28".into(), version: v.into() })
    }

    #[test]
    fn install_confirm_rollback() {
        let f = fake();
        let (k, m, run28) = k28();
        let a27 = fs::read(&f.dev.boot_a).unwrap();

        // refusals: no way back, wrong Android hash, low battery
        assert!(install(&f.dev, "", &f.run27, &f.state, &m, &k, Ok(())).unwrap_err().contains("Android image hash"));
        assert!(install(&f.dev, &"0".repeat(64), &f.run27, &f.state, &m, &k, Ok(())).unwrap_err().contains("boot_b"));
        assert!(install(&f.dev, &f.stock_sha, &f.run27, &f.state, &m, &k, Err("battery at 10 %".into())).is_err());
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27, "nothing written");

        // install t28 while t27 runs from boot_a
        let msg = install(&f.dev, &f.stock_sha, &f.run27, &f.state, &m, &k, Ok(())).unwrap();
        assert!(msg.contains("t28 installed"), "{msg}");
        assert_eq!(fs::read(f.state.join(GOOD_IMG)).unwrap(), a27, "linux-good = the t27 image");
        let st = KernelState::load(&f.state);
        assert_eq!(st.good, "7.3.0-rc4-tb323fu-t27");
        assert_eq!(st.good_sha256, sha_hex(&a27));
        assert_eq!(st.good_serial, 27);
        assert_eq!(st.trial, "7.3.0-rc4-tb323fu-t28");
        assert_eq!(st.trial_version, run28.version);
        assert_eq!(st.trial_channel, "testing");
        assert_eq!(st.tries, 0);
        let a28 = fs::read(&f.dev.boot_a).unwrap();
        assert_eq!(sha_hex(&a28), st.trial_sha256);
        assert_eq!(bootimg::boot_kernel(&a28).unwrap(), &k[..]);
        assert_eq!(&a28[SIZE - 64..SIZE - 60], b"AVBf");

        // pending (t27 still running, boot_a = t28): installing again keeps linux-good
        let again = install(&f.dev, &f.stock_sha, &f.run27, &f.state, &m, &k, Ok(()));
        assert!(again.is_ok(), "{again:?}");
        assert_eq!(fs::read(f.state.join(GOOD_IMG)).unwrap(), a27);
        assert_eq!(confirm(&f.dev, &f.run27, &f.state).unwrap_err(), "7.3.0-rc4-tb323fu-t28 is installed but not running yet; restart first");

        // booted t28 (the initramfs counted a try): on trial, no new install
        let mut st = KernelState::load(&f.state);
        st.tries = 1;
        st.save(&f.state).unwrap();
        assert_eq!(running_serial(&st, &run28), 28);
        assert!(install(&f.dev, &f.stock_sha, &run28, &f.state, &m, &k, Ok(())).unwrap_err().contains("still on trial"));

        // keep it
        assert!(confirm(&f.dev, &run28, &f.state).unwrap().contains("kept"));
        let st = KernelState::load(&f.state);
        assert_eq!((st.good.as_str(), st.trial.as_str(), st.tries, st.good_serial), ("7.3.0-rc4-tb323fu-t28", "", 0, 28));
        assert_eq!(fs::read(f.state.join(GOOD_IMG)).unwrap(), a28);
        assert_eq!(confirm(&f.dev, &run28, &f.state).unwrap(), "nothing to confirm: no kernel on trial");

        // a manual rollback writes linux-good back (here: t28 itself) and records nothing
        assert!(rollback(&f.dev, &f.state, Ok(())).is_ok());
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a28);
    }

    #[test]
    fn rollback_of_a_pending_trial_and_corrupt_good() {
        let f = fake();
        let (k, m, _) = k28();
        let a27 = fs::read(&f.dev.boot_a).unwrap();
        install(&f.dev, &f.stock_sha, &f.run27, &f.state, &m, &k, Ok(())).unwrap();
        let msg = rollback(&f.dev, &f.state, Ok(())).unwrap();
        assert!(msg.contains("t27"), "{msg}");
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27);
        let st = KernelState::load(&f.state);
        assert_eq!((st.trial.as_str(), st.failed.as_str()), ("", "7.3.0-rc4-tb323fu-t28"));

        // a damaged linux-good.img is never written
        let mut g = fs::read(f.state.join(GOOD_IMG)).unwrap();
        g[100] ^= 1;
        fs::write(f.state.join(GOOD_IMG), g).unwrap();
        assert!(rollback(&f.dev, &f.state, Ok(())).unwrap_err().contains("does not match"));
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27);
    }

    #[test]
    fn install_needs_a_known_good_image() {
        let f = fake();
        let (k, m, _) = k28();
        // boot_a holds something that is not the running kernel, and no linux-good
        let other = Running { release: "7.3.0-dev".into(), version: "Linux version 7.3.0-dev (x@y) (c) #9".into() };
        let e = install(&f.dev, &f.stock_sha, &other, &f.state, &m, &k, Ok(())).unwrap_err();
        assert!(e.contains("restart once"), "{e}");
        assert!(!f.state.join(STATE_FILE).exists());
    }

    #[test]
    fn readback_mismatch_restores_good() {
        let f = fake();
        let (k, m, _) = k28();
        let a27 = fs::read(&f.dev.boot_a).unwrap();
        CORRUPT_NEXT_WRITE.with(|c| c.set(true));
        let e = install(&f.dev, &f.stock_sha, &f.run27, &f.state, &m, &k, Ok(())).unwrap_err();
        assert!(e.contains("does not read back") && e.contains("previous kernel again"), "{e}");
        assert_eq!(fs::read(&f.dev.boot_a).unwrap(), a27, "linux-good written back");
        assert!(KernelState::load(&f.state).trial.is_empty(), "no trial record after a failed write");
    }
}
