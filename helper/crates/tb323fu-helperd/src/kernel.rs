// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The `Kernel` object: kernel updates through the helper
//! (docs/notes/kernel-updates-design.md, docs/helper.md "Kernel updates").
//!
//! Releases come from GitHub Releases of `kernel.source` (default the
//! project's repository). The daemon never opens a network connection:
//! `Check` and `Download` write a list of URLs to
//! /run/tb323fu/kernel-fetch.list and start `tb323fu-kernel-fetch.service`
//! (DynamicUser, network, only its cache directory writable); everything that
//! service leaves in /var/cache/tb323fu-kernel is checked here before it is
//! used: the API's release list is parsed with size limits, the kernel file
//! is checked against the release's SHA256SUMS (and a minisign signature over
//! it when `kernel.require_signature` is on), GitHub's asset digest and its
//! own version banner. Checked files are kept in /var/lib/tb323fu/kernel on
//! the running root:
//!
//! ```text
//! releases.json, releases.source, last-check
//! staged/<tag>/<kernel file>, SHA256SUMS(.minisig), verified
//! ```
//!
//! A kernel from a file (`InspectLocal`, `InstallLocal`) arrives as a file
//! descriptor the caller opened -- the daemon reads only what the caller
//! could read -- and takes the same install path.
//!
//! The long methods (Check, Download, Install, InstallLocal, Rollback, Keep)
//! answer when they are done, with a message for a person; `State` and
//! `Progress` show the running operation meanwhile, and `Finished` is emitted
//! at the end.

use crate::ifaces::{invalidate, Snapshot};
use crate::polkit;
use crate::Shared;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tb323fu_helper_core::helperupdate::{self as hu, HelperRelease};
use tb323fu_helper_core::kernel::{self as k, KernelState, Release};
use tb323fu_helper_core::{boot, features as f, sys};
use zbus::fdo;
use zbus::interface;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedFd;

pub const P_KERNEL: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Kernel";
const FETCH_UNIT: &str = "tb323fu-kernel-fetch.service";
const DAY: u64 = 86400;
/// A local file: an Image, Image.gz or a whole boot image (96 MiB).
const MAX_LOCAL_FILE: u64 = 128 << 20;

#[derive(Default)]
struct St {
    /// the operation running now ("" idle)
    op: String,
    /// the last result, for a person
    message: String,
    /// kernel-state from the state root (cached)
    ks: KernelState,
    /// the last release list of the configured source, and the newest helper in it
    releases: Option<(Vec<Release>, Option<String>)>,
    /// the helper releases of the same list (self-update)
    helpers: Vec<HelperRelease>,
    /// the Mesa releases of the same list (Mesa channel)
    mesas: Vec<tb323fu_helper_core::mesa::MesaRelease>,
    last_check: u64,
    /// installed in this boot, waiting for a restart
    pending: String,
}

pub struct Inner {
    pub sh: Arc<Shared>,
    st: Mutex<St>,
    progress: AtomicU32,
    run: k::Running,
    on_state_root: bool,
}

pub struct Kernel(pub Arc<Inner>);

fn stage() -> PathBuf {
    sys::path(k::STAGE_DIR)
}
pub(crate) fn cache() -> PathBuf {
    sys::path(k::CACHE_DIR)
}
pub(crate) fn read_limited(p: &Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut v = Vec::new();
    fs::File::open(p).and_then(|f| f.take(max + 1).read_to_end(&mut v)).map_err(|e| format!("{}: {e}", p.display()))?;
    if v.len() as u64 > max {
        return Err(format!("{}: too large", p.display()));
    }
    Ok(v)
}
pub(crate) fn read_text(p: &Path) -> Result<String, String> {
    String::from_utf8(read_limited(p, 64 << 10)?).map_err(|_| format!("{}: not text", p.display()))
}
pub(crate) fn write_file(p: &Path, d: &[u8]) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, d).and_then(|_| fs::rename(&tmp, p)).map_err(|e| format!("{}: {e}", p.display()))
}

/// Read a file the caller opened and passed (bounded; regular files only).
fn read_fd(fd: OwnedFd) -> Result<Vec<u8>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::from(std::os::fd::OwnedFd::from(fd));
    let md = file.metadata().map_err(|e| format!("the file: {e}"))?;
    if !md.is_file() {
        return Err("not a regular file".into());
    }
    if md.len() > MAX_LOCAL_FILE {
        return Err(format!("the file is larger than {} MiB", MAX_LOCAL_FILE >> 20));
    }
    let _ = file.seek(SeekFrom::Start(0));
    let mut v = Vec::new();
    file.take(MAX_LOCAL_FILE + 1).read_to_end(&mut v).map_err(|e| format!("reading the file: {e}"))?;
    if v.len() as u64 > MAX_LOCAL_FILE {
        return Err(format!("the file is larger than {} MiB", MAX_LOCAL_FILE >> 20));
    }
    Ok(v)
}

/// A name for a person, safe as one `key=value` line of kernel-state.
fn clean_label(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(64).collect::<String>().trim().to_string()
}

/// What updates the helper on this system (who installed it).
pub fn helper_update_command() -> String {
    crate::selfupdate::owner().hint()
}

impl Inner {
    pub fn new(sh: Arc<Shared>) -> Arc<Self> {
        let inner = Arc::new(Inner {
            sh,
            st: Mutex::new(St::default()),
            progress: AtomicU32::new(0),
            run: k::running(),
            on_state_root: boot::on_state_root(),
        });
        {
            let mut s = inner.st();
            s.last_check = sys::read_opt(&stage().join("last-check")).and_then(|v| v.parse().ok()).unwrap_or(0);
            if let Some((r, h, m)) = inner.stored_releases() {
                s.releases = Some(r);
                s.helpers = h;
                s.mesas = m;
            }
        }
        inner
    }

    fn st(&self) -> std::sync::MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn source(&self) -> Result<k::Source, String> {
        let c = self.sh.cfg().kernel;
        k::Source::parse(&c.source, &c.api_url)
    }

    /// Re-read kernel-state from the state root (mounts it read-only when
    /// running from another root). Blocking.
    pub fn reload_state(&self) {
        match boot::with_state_dir(false, k::STATE_REL, |d| Ok(KernelState::load(d))) {
            Ok(ks) => self.st().ks = ks,
            Err(e) => eprintln!("tb323fu-helperd: kernel-state: {e}"),
        }
    }

    /// Cheap enough for the 5 s poller only when the state root is this root.
    pub fn poll_state(&self) {
        // read directly (no state-root lock: an install may hold it for seconds)
        if self.on_state_root && self.st().op.is_empty() {
            let ks = KernelState::load(&sys::path(&format!("/{}", k::STATE_REL)));
            self.st().ks = ks;
        }
    }

    /// The last release list on disk, when it came from the configured source.
    #[allow(clippy::type_complexity)]
    fn stored_releases(&self) -> Option<((Vec<Release>, Option<String>), Vec<HelperRelease>, Vec<tb323fu_helper_core::mesa::MesaRelease>)> {
        let src = self.source().ok()?;
        if read_text(&stage().join("releases.source")).ok()?.trim() != src.releases_url() {
            return None;
        }
        let d = read_limited(&stage().join("releases.json"), k::MAX_RELEASES_FILE).ok()?;
        Some((k::parse_releases(&d, &src).ok()?, hu::parse_helper_releases(&d, &src).unwrap_or_default(),
            tb323fu_helper_core::mesa::parse_releases(&d, &src).unwrap_or_default()))
    }

    pub fn helper_releases(&self) -> Vec<HelperRelease> {
        self.st().helpers.clone()
    }

    pub fn mesa_releases(&self) -> Vec<tb323fu_helper_core::mesa::MesaRelease> {
        self.st().mesas.clone()
    }

    pub fn last_check(&self) -> u64 {
        self.st().last_check
    }

    pub fn kernel_serial(&self) -> u64 {
        self.running_serial()
    }

    fn release(&self, tag: &str) -> Result<Release, String> {
        if !k::safe_name(tag) {
            return Err(format!("{tag}: not a release tag"));
        }
        let s = self.st();
        s.releases.as_ref().and_then(|(r, _)| r.iter().find(|r| r.tag == tag).cloned())
            .ok_or_else(|| format!("{tag}: unknown release (check first)"))
    }

    fn running_serial(&self) -> u64 {
        k::running_serial(&self.st().ks, &self.run)
    }

    /// (tag, title, notes URL, serial) of the channel's release when it is
    /// newer than the running kernel.
    fn available(&self) -> Vec<(String, String, String, u32)> {
        let ch = self.sh.cfg().kernel.channel;
        let serial = self.running_serial();
        let s = self.st();
        let Some((rels, _)) = &s.releases else { return Vec::new() };
        match k::pick(rels, &ch) {
            Some(r) if r.serial > serial => vec![(r.tag.clone(), r.title.clone(), r.notes_url.clone(), r.serial as u32)],
            _ => Vec::new(),
        }
    }

    /// Tags whose kernel was downloaded and checked (the `verified` record
    /// names the file and its SHA-256; the size is checked again here).
    fn downloaded(&self) -> Vec<String> {
        let dir = stage().join("staged");
        sys::list_dir(&dir)
            .into_iter()
            .filter(|t| {
                let Ok(v) = read_text(&dir.join(t).join("verified")) else { return false };
                let Some(name) = v.split_whitespace().next().filter(|n| k::safe_name(n)) else { return false };
                self.release(t).is_ok_and(|r| r.kernel.name == name && fs::metadata(dir.join(t).join(name)).is_ok_and(|m| m.len() == r.kernel.size))
            })
            .collect()
    }

    fn state_name(&self) -> String {
        let s = self.st();
        if !s.op.is_empty() {
            return s.op.clone();
        }
        let ks = &s.ks;
        if ks.trial_running(&self.run) {
            return "trial".into();
        }
        if !s.pending.is_empty() || !ks.trial.is_empty() {
            return "pending-reboot".into();
        }
        if !ks.failed.is_empty() && ks.other.iter().find(|(k, _)| k == "failed_seen").map(|(_, v)| v.as_str()) != Some(ks.failed.as_str()) {
            return "rolled-back".into();
        }
        drop(s);
        let avail = self.available();
        if avail.first().is_some_and(|a| self.downloaded().contains(&a.0)) {
            "ready".into()
        } else {
            "idle".into()
        }
    }

    /// Start one operation; Err when another one runs.
    fn begin(&self, op: &str) -> fdo::Result<()> {
        let mut s = self.st();
        if !s.op.is_empty() {
            return Err(fdo::Error::Failed(format!("busy: {} is running", s.op)));
        }
        s.op = op.into();
        self.progress.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn end(&self, res: &Result<String, String>) {
        let mut s = self.st();
        s.op.clear();
        s.message = match res {
            Ok(m) => m.clone(),
            Err(e) => e.clone(),
        };
        self.progress.store(0, Ordering::Relaxed);
    }

    // ------------------------------------------------------------ fetching

    /// Let the fetch unit download `items` (name, max bytes, URL, mode:
    /// "api" for the release list, "asset" for release files) into the cache
    /// directory. Blocking.
    pub fn fetch(&self, items: &[(String, u64, String, &str)], progress_of: Option<(&str, u64, &AtomicU32)>) -> Result<(), String> {
        // one download list and one fetch unit for kernel and helper updates
        static FETCH: Mutex<()> = Mutex::new(());
        let _one = FETCH.lock().unwrap_or_else(|e| e.into_inner());
        for (name, _, url, _) in items {
            if !k::safe_name(name) || !k::url_ok(url) {
                return Err(format!("refusing to fetch {name} from {url}"));
            }
            let _ = fs::remove_file(cache().join(name));
        }
        let list: String = items.iter().map(|(n, m, u, mode)| format!("{n} {m} {u} {mode}\n")).collect();
        let lp = sys::path(k::FETCH_LIST);
        write_file(&lp, list.as_bytes())?;
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&lp, fs::Permissions::from_mode(0o644));
        }
        // progress: watch the partial file grow while the unit runs
        let done = std::sync::atomic::AtomicBool::new(false);
        let out = std::thread::scope(|sc| {
            if let Some((name, size, progress)) = progress_of {
                let part = cache().join(format!("{name}.part"));
                let done = &done;
                sc.spawn(move || {
                    while !done.load(Ordering::Relaxed) {
                        if let Ok(md) = fs::metadata(&part) {
                            progress.store(((md.len() * 100) / size.max(1)).min(99) as u32, Ordering::Relaxed);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                });
            }
            let out = match std::env::var_os("TB323FU_KERNEL_FETCH") {
                // tests: run the fetch script directly
                Some(prog) => std::process::Command::new("sh").arg(prog).arg(&lp).env("CACHE_DIRECTORY", cache()).output(),
                None => std::process::Command::new("systemctl").args(["start", FETCH_UNIT]).output(),
            };
            done.store(true, Ordering::Relaxed);
            out
        });
        let out = out.map_err(|e| format!("starting the download: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let log = fs::read_to_string(cache().join("fetch.log")).unwrap_or_default();
        let why = log.lines().filter(|l| !l.starts_with("ok ")).last().map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string());
        Err(format!("download failed: {}", if why.is_empty() { "see journalctl -u tb323fu-kernel-fetch".into() } else { why }))
    }

    /// Fetch the source's release list (kernels and helpers) and keep it. Blocking.
    pub fn refresh_list(&self) -> Result<(), String> {
        let src = self.source()?;
        let url = src.releases_url();
        self.fetch(&[("releases.json".into(), k::MAX_RELEASES_FILE, url.clone(), "api")], None)?;
        let data = read_limited(&cache().join("releases.json"), k::MAX_RELEASES_FILE)?;
        let (rels, helper) = k::parse_releases(&data, &src)?;
        let helpers = hu::parse_helper_releases(&data, &src)?;
        let mesas = tb323fu_helper_core::mesa::parse_releases(&data, &src)?;
        let now = k::now();
        write_file(&stage().join("releases.json"), &data)?;
        write_file(&stage().join("releases.source"), url.as_bytes())?;
        write_file(&stage().join("last-check"), now.to_string().as_bytes())?;
        let mut s = self.st();
        s.releases = Some((rels, helper));
        s.helpers = helpers;
        s.mesas = mesas;
        s.last_check = now;
        Ok(())
    }

    /// Check: fetch the source's release list and pick the channel's
    /// release. Blocking.
    pub fn check(&self) -> Result<String, String> {
        let cfg = self.sh.cfg().kernel;
        let src = self.source()?;
        self.refresh_list()?;
        let (pick, n) = {
            let s = self.st();
            let rels = s.releases.as_ref().map(|(r, _)| r.as_slice()).unwrap_or(&[]);
            (k::pick(rels, &cfg.channel).cloned(), rels.len())
        };
        let Some(r) = pick else {
            return Ok(format!("no {} kernel release in {} ({n} kernel releases found)", cfg.channel, src.name()));
        };
        if r.serial <= self.running_serial() {
            return Ok(format!("up to date ({} channel: {})", cfg.channel, r.tag));
        }
        let note = match k::requirements(&r, crate::selfupdate::version(), k::platform_version().as_deref()) {
            Ok(()) => String::new(),
            Err(why) => format!(" ({why})"),
        };
        Ok(format!("{} available: {}{note}", r.tag, r.title))
    }

    /// SHA256SUMS of a release (in `dir`), with its signature when required.
    pub fn sums_in(&self, dir: &Path) -> Result<std::collections::BTreeMap<String, String>, String> {
        let cfg = self.sh.cfg().kernel;
        let data = read_limited(&dir.join(k::SUMS_ASSET), k::MAX_SUMS_FILE)?;
        let sig = if cfg.require_signature { read_text(&dir.join(k::SIG_ASSET)).ok() } else { None };
        k::read_sums(&data, sig.as_deref(), cfg.require_signature, &k::trusted_keys(&cfg.public_keys))
    }

    /// Download and check the kernel file of `tag`. Blocking.
    pub fn download(&self, tag: &str) -> Result<String, String> {
        let r = self.release(tag)?;
        let need_sig = self.sh.cfg().kernel.require_signature;
        let mut items = vec![(k::SUMS_ASSET.to_string(), k::MAX_SUMS_FILE, r.sums.url.clone(), "asset")];
        if need_sig {
            let sig = r.sig.as_ref().ok_or("kernel.require_signature is on, but the release has no SHA256SUMS.minisig")?;
            items.push((k::SIG_ASSET.to_string(), 64 << 10, sig.url.clone(), "asset"));
        }
        items.push((r.kernel.name.clone(), r.kernel.size, r.kernel.url.clone(), "asset"));
        self.fetch(&items, Some((&r.kernel.name, r.kernel.size, &self.progress)))?;
        self.st().op = "verifying".into();
        let sums = self.sums_in(&cache())?;
        let data = read_limited(&cache().join(&r.kernel.name), k::MAX_KERNEL_FILE)?;
        let (rel, banner) = k::check_kernel_file(&r, &sums, &data)?;
        let dir = stage().join("staged").join(tag);
        let _ = fs::remove_dir_all(&dir);
        for (n, _, _, _) in &items[..items.len() - 1] {
            write_file(&dir.join(n), &read_limited(&cache().join(n), k::MAX_SUMS_FILE)?)?;
        }
        write_file(&dir.join(&r.kernel.name), &data)?;
        write_file(&dir.join("verified"), format!("{} {} {rel}\n{banner}\n", r.kernel.name, k::sha_hex(&data)).as_bytes())?;
        let _ = fs::remove_file(cache().join(&r.kernel.name));
        Ok(format!("{tag} downloaded and checked ({}, {} MB)", rel, data.len() / 1_000_000))
    }

    /// Install a downloaded release into boot_a as a trial. Blocking.
    pub fn install(&self, tag: &str) -> Result<String, String> {
        let r = self.release(tag)?;
        let channel = self.sh.cfg().kernel.channel;
        k::requirements(&r, crate::selfupdate::version(), k::platform_version().as_deref())?;
        if r.serial <= self.running_serial() {
            return Err(format!("{tag} is not newer than the running kernel"));
        }
        let dir = stage().join("staged").join(tag);
        let data = read_limited(&dir.join(&r.kernel.name), k::MAX_KERNEL_FILE).map_err(|_| format!("{tag} is not downloaded"))?;
        // checked again: the staged files are on disk since the download
        let sums = self.sums_in(&dir)?;
        let (release, version) = k::check_kernel_file(&r, &sums, &data)?;
        // Q7 follows the channel it was installed from (a release moves from
        // pre-release to release without a new upload); a pre-release always
        // waits for Keep
        let c = k::Candidate { release, version, serial: r.serial, keep: channel == "testing" || r.prerelease, channel, label: String::new() };
        let msg = self.write(&c, &data)?;
        self.st().pending = tag.to_string();
        Ok(msg)
    }

    fn write(&self, c: &k::Candidate, kernel: &[u8]) -> Result<String, String> {
        let dev = k::find_device()?;
        let hash = f::android_hash().unwrap_or_default();
        let run = self.run.clone();
        boot::with_state_dir(true, k::STATE_REL, |d| k::install(&dev, &hash, &run, d, c, kernel, k::power_ok()))
    }

    /// Install a kernel from a file the caller opened. Blocking.
    pub fn install_local(&self, data: &[u8], label: &str, auto_confirm: bool) -> Result<String, String> {
        let l = k::inspect_local(data, &self.run)?;
        let c = k::Candidate {
            serial: k::serial_of_release(&l.release),
            release: l.release.clone(),
            version: l.banner.clone(),
            channel: "local".into(),
            keep: !auto_confirm,
            label: clean_label(label),
        };
        let mut msg = self.write(&c, &l.kernel)?;
        if !l.shared_modules {
            msg.push_str(&format!(" -- no shared modules in this kernel: systems need their own modules for {}", l.release));
        }
        self.st().pending = format!("local:{}", l.release);
        Ok(msg)
    }

    pub fn keep(&self) -> Result<String, String> {
        let dev = k::find_device()?;
        let run = self.run.clone();
        boot::with_state_dir(true, k::STATE_REL, |d| k::confirm(&dev, &run, d))
    }

    pub fn rollback(&self) -> Result<String, String> {
        let dev = k::find_device()?;
        let r = boot::with_state_dir(true, k::STATE_REL, |d| k::rollback(&dev, d, k::power_ok()))?;
        self.st().pending.clear();
        Ok(r)
    }

    /// Hide the "did not start" notice (the record itself stays).
    pub fn dismiss(&self) -> Result<String, String> {
        boot::with_state_dir(true, k::STATE_REL, |d| {
            let mut ks = KernelState::load(d);
            if ks.failed.is_empty() {
                return Ok("nothing to dismiss".into());
            }
            ks.other.retain(|(k, _)| k != "failed_seen");
            ks.other.push(("failed_seen".into(), ks.failed.clone()));
            ks.save(d)?;
            Ok("dismissed".into())
        })
    }

    /// Remove staged downloads that are not newer than the running kernel
    /// (or no longer in the release list).
    pub fn clean_staged(&self) {
        let serial = self.running_serial();
        let dir = stage().join("staged");
        for t in sys::list_dir(&dir) {
            if self.release(&t).map(|r| r.serial <= serial).unwrap_or(true) {
                let _ = fs::remove_dir_all(dir.join(&t));
            }
        }
    }

    /// Once a day, when allowed: a check in the background (never a download).
    pub fn auto_tick(self: &Arc<Self>) {
        let cfg = self.sh.cfg().kernel;
        if !cfg.auto_check {
            return;
        }
        let up = sys::read_opt(&sys::path("/proc/uptime")).and_then(|u| u.split('.').next().and_then(|s| s.parse::<u64>().ok())).unwrap_or(0);
        let due = { let s = self.st(); s.op.is_empty() && k::now().saturating_sub(s.last_check) > DAY };
        if !due || up < 300 {
            return;
        }
        if self.begin("checking").is_err() {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            let r = me.check();
            if let Err(e) = &r {
                eprintln!("tb323fu-helperd: kernel check: {e}");
                // try again tomorrow, not every 5 s
                me.st().last_check = k::now();
            }
            me.end(&r);
        });
    }
}

impl Snapshot for Kernel {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Kernel";
    const PROPS: &'static [&'static str] = &["Running", "RunningBuild", "SharedModules", "Channel", "Source", "Available", "Downloaded", "State",
        "Progress", "Trial", "TrialChannel", "TrialLabel", "Tries", "MaxTries", "KeepPending", "Good", "GoodLabel", "LastFailed", "LastCheck",
        "IndexExpired", "AutoCheck", "RequireSignature", "HelperLatest", "HelperUpdateCommand", "Message"];
    fn snapshot(&self) -> String {
        let i = &self.0;
        let s = i.st();
        format!("{} {} {} {:?} {:?} {} {} {:?}", s.op, i.progress.load(Ordering::Relaxed), s.message, s.ks, s.releases, s.last_check, s.pending,
            i.sh.cfg().kernel)
    }
}

async fn finished(em: &SignalEmitter<'_>, op: &str, res: &Result<String, String>) {
    let (ok, msg) = match res {
        Ok(m) => (true, m.as_str()),
        Err(e) => (false, e.as_str()),
    };
    let _ = Kernel::finished(em, op, ok, msg).await;
    invalidate(em, Kernel::IFACE, Kernel::PROPS).await;
}

impl Kernel {
    /// Run a blocking operation as `op`, with Finished + PropertiesChanged.
    async fn run(&self, op: &'static str, em: &SignalEmitter<'_>, f: impl FnOnce(&Inner) -> Result<String, String> + Send + 'static) -> fdo::Result<String> {
        self.0.begin(op)?;
        invalidate(em, Self::IFACE, Self::PROPS).await;
        let inner = self.0.clone();
        let res = blocking::unblock(move || {
            let r = f(&inner);
            inner.reload_state();
            r
        }).await;
        self.0.end(&res);
        finished(em, op, &res).await;
        res.map_err(fdo::Error::Failed)
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Kernel")]
impl Kernel {
    /// `uname -r`
    #[zbus(property)]
    fn running(&self) -> String {
        self.0.run.release.clone()
    }
    /// `/proc/version`
    #[zbus(property)]
    fn running_build(&self) -> String {
        self.0.run.version.clone()
    }
    /// /lib/modules/<release> is the modules image of the boot image
    #[zbus(property)]
    fn shared_modules(&self) -> bool {
        k::shared_modules(&self.0.run.release)
    }
    #[zbus(property)]
    fn channel(&self) -> String {
        self.0.sh.cfg().kernel.channel
    }
    /// where releases come from ("github:OWNER/REPO")
    #[zbus(property)]
    fn source(&self) -> String {
        self.0.sh.cfg().kernel.source
    }
    /// (tag, title, notes URL, serial): the channel's release when newer
    #[zbus(property)]
    fn available(&self) -> Vec<(String, String, String, u32)> {
        self.0.available()
    }
    /// tags downloaded and checked, ready to install
    #[zbus(property)]
    fn downloaded(&self) -> Vec<String> {
        self.0.downloaded()
    }
    /// idle / checking / downloading / verifying / ready / installing /
    /// pending-reboot / trial / rolled-back (also rolling-back, keeping)
    #[zbus(property)]
    fn state(&self) -> String {
        self.0.state_name()
    }
    /// download progress, %
    #[zbus(property)]
    fn progress(&self) -> u32 {
        self.0.progress.load(Ordering::Relaxed)
    }
    /// release on trial (installed, not yet confirmed), ""
    #[zbus(property)]
    fn trial(&self) -> String {
        self.0.st().ks.trial.clone()
    }
    /// stable, testing or local
    #[zbus(property)]
    fn trial_channel(&self) -> String {
        self.0.st().ks.trial_channel.clone()
    }
    /// the name given to a local install, ""
    #[zbus(property)]
    fn trial_label(&self) -> String {
        self.0.st().ks.trial_label.clone()
    }
    /// starts of the trial kernel counted by the initramfs
    #[zbus(property)]
    fn tries(&self) -> u32 {
        self.0.st().ks.tries
    }
    #[zbus(property)]
    fn max_tries(&self) -> u32 {
        self.0.st().ks.max
    }
    /// the running kernel is a trial that is kept only when someone presses
    /// Keep (testing channel, pre-releases, local files by default; Q7)
    #[zbus(property)]
    fn keep_pending(&self) -> bool {
        let s = self.0.st();
        s.ks.trial_running(&self.0.run) && s.ks.keep_needed()
    }
    /// release of linux-good.img
    #[zbus(property)]
    fn good(&self) -> String {
        self.0.st().ks.good.clone()
    }
    #[zbus(property)]
    fn good_label(&self) -> String {
        self.0.st().ks.good_label.clone()
    }
    #[zbus(property)]
    fn last_failed(&self) -> String {
        self.0.st().ks.failed.clone()
    }
    /// unix time of the last successful check (0 never)
    #[zbus(property)]
    fn last_check(&self) -> u64 {
        self.0.st().last_check
    }
    /// always false: there is no signed index with an expiry date any more
    /// (kept for clients of the first API)
    #[zbus(property)]
    fn index_expired(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn auto_check(&self) -> bool {
        self.0.sh.cfg().kernel.auto_check
    }
    /// SHA256SUMS must carry a minisign signature from a configured key
    #[zbus(property)]
    fn require_signature(&self) -> bool {
        self.0.sh.cfg().kernel.require_signature
    }
    /// a newer helper published as a `helper-vX.Y.Z` release ("" none, or notifications off)
    #[zbus(property)]
    fn helper_latest(&self) -> String {
        if !self.0.sh.cfg().kernel.helper_notify {
            return String::new();
        }
        let s = self.0.st();
        match &s.releases {
            Some((_, Some(h))) if k::version_cmp(h, crate::selfupdate::version()).is_gt() => h.clone(),
            _ => String::new(),
        }
    }
    #[zbus(property)]
    fn helper_update_command(&self) -> String {
        helper_update_command()
    }
    /// the last operation's result
    #[zbus(property)]
    fn message(&self) -> String {
        self.0.st().message.clone()
    }

    async fn check(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-check", self.0.sh.no_polkit).await?;
        self.run("checking", &em, |i| i.check()).await
    }
    async fn download(&self, tag: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-download", self.0.sh.no_polkit).await?;
        self.run("downloading", &em, move |i| i.download(&tag)).await
    }
    /// Install a downloaded release into boot_a (a trial); reboot when asked.
    async fn install(&self, tag: String, reboot: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-install", self.0.sh.no_polkit).await?;
        let r = self.run("installing", &em, move |i| {
            let r = i.install(&tag);
            i.clean_staged();
            r
        }).await?;
        if reboot {
            reboot_now()?;
        }
        Ok(r)
    }
    /// Look at a kernel file (an Image, Image.gz or boot image the caller
    /// opened): (release, /proc/version banner, format, carries shared
    /// modules, warnings). Nothing is written.
    async fn inspect_local(&self, file: OwnedFd, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection)
        -> fdo::Result<(String, String, String, bool, Vec<String>)> {
        polkit::check(conn, &hdr, "kernel-check", self.0.sh.no_polkit).await?;
        let run = self.0.run.clone();
        let l = blocking::unblock(move || read_fd(file).and_then(|d| k::inspect_local(&d, &run))).await.map_err(fdo::Error::Failed)?;
        Ok((l.release, l.banner, l.format, l.shared_modules, l.warnings))
    }
    /// Install a kernel from a file the caller opened, the same way as a
    /// release: repacked into the stock image of boot_b, linux-good.img kept,
    /// a trial with automatic rollback. `auto_confirm` false: kept only by
    /// Keep; true: kept once a system has run 90 s (like the stable channel).
    async fn install_local(&self, file: OwnedFd, name: String, auto_confirm: bool, reboot: bool, #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &zbus::Connection, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-install-local", self.0.sh.no_polkit).await?;
        let r = self.run("installing", &em, move |i| {
            let data = read_fd(file)?;
            i.install_local(&data, &name, auto_confirm)
        }).await?;
        if reboot {
            reboot_now()?;
        }
        Ok(r)
    }
    /// Write linux-good.img back into boot_a.
    async fn rollback(&self, reboot: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-rollback", self.0.sh.no_polkit).await?;
        let r = self.run("rolling-back", &em, |i| i.rollback()).await?;
        if reboot {
            reboot_now()?;
        }
        Ok(r)
    }
    /// Keep the running trial kernel (testing channel, local files; stable confirms by itself).
    async fn keep(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-keep", self.0.sh.no_polkit).await?;
        self.run("keeping", &em, |i| {
            let r = i.keep();
            i.clean_staged();
            r
        }).await
    }
    /// Hide the "did not start" notice after an automatic rollback.
    async fn dismiss(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-check", self.0.sh.no_polkit).await?;
        self.run("dismissing", &em, |i| i.dismiss()).await
    }
    async fn set_channel(&self, channel: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "kernel-channel", self.0.sh.no_polkit).await?;
        if channel != "stable" && channel != "testing" {
            return Err(fdo::Error::InvalidArgs("channel must be stable or testing".into()));
        }
        self.0.sh.update(|c| c.kernel.channel = channel.clone());
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_auto_check(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "kernel-channel", self.0.sh.no_polkit).await?;
        self.0.sh.update(|c| c.kernel.auto_check = on);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_helper_notify(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "kernel-channel", self.0.sh.no_polkit).await?;
        self.0.sh.update(|c| c.kernel.helper_notify = on);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Release notes (Markdown, the release body).
    fn notes(&self, tag: String) -> fdo::Result<String> {
        let r = self.0.release(&tag).map_err(fdo::Error::Failed)?;
        Ok(if r.notes.is_empty() { r.notes_url } else { r.notes })
    }
    /// Re-read kernel-state (tb323fu-kernel-confirm calls this after confirming).
    async fn refresh(&self, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        let inner = self.0.clone();
        blocking::unblock(move || {
            inner.reload_state();
            inner.clean_staged();
        }).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    #[zbus(signal)]
    async fn finished(em: &SignalEmitter<'_>, operation: &str, ok: bool, message: &str) -> zbus::Result<()>;
}

fn reboot_now() -> fdo::Result<()> {
    std::process::Command::new("systemctl").arg("reboot").spawn().map(|_| ()).map_err(|e| fdo::Error::Failed(format!("systemctl reboot: {e}")))
}
