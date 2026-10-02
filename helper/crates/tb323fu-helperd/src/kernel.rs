// SPDX-License-Identifier: MIT
//! The `Kernel` object: kernel updates through the helper
//! (docs/notes/kernel-updates-design.md, docs/helper.md "Kernel updates").
//!
//! The daemon never opens a network connection. `Check` and `Download` write a
//! list of URLs to /run/tb323fu/kernel-fetch.list and start
//! `tb323fu-kernel-fetch.service` (DynamicUser, network, only its cache
//! directory writable); everything that service leaves in
//! /var/cache/tb323fu-kernel is verified here before it is used: the index and
//! the manifests by minisign signature, the kernel file by the SHA-256 in its
//! signed manifest and by its own version banner. Verified files are kept in
//! /var/lib/tb323fu/kernel on the running root:
//!
//! ```text
//! index.json(.minisig), last-check
//! manifests/<tag>.json(.minisig), manifests/<tag>.url
//! staged/<tag>/<kernel file>
//! ```
//!
//! The long methods (Check, Download, Install, Rollback, Keep) answer when
//! they are done, with a message for a person; `State` and `Progress` show
//! the running operation meanwhile, and `Finished` is emitted at the end.

use crate::ifaces::{invalidate, Snapshot};
use crate::polkit;
use crate::Shared;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tb323fu_helper_core::kernel::{self as k, KernelState, Manifest};
use tb323fu_helper_core::{boot, features as f, sys};
use zbus::fdo;
use zbus::interface;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;

pub const P_KERNEL: &str = "/io/github/joonhoekim/tb323fu/Helper/Kernel";
const FETCH_UNIT: &str = "tb323fu-kernel-fetch.service";
const DAY: u64 = 86400;

#[derive(Default)]
struct St {
    /// the operation running now ("" idle)
    op: String,
    /// the last result, for a person
    message: String,
    /// kernel-state from the state root (cached)
    ks: KernelState,
    /// the verified index and whether it expired
    index: Option<(k::Index, bool)>,
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
fn cache() -> PathBuf {
    sys::path(k::CACHE_DIR)
}
fn read_limited(p: &Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut v = Vec::new();
    fs::File::open(p).and_then(|f| f.take(max + 1).read_to_end(&mut v)).map_err(|e| format!("{}: {e}", p.display()))?;
    if v.len() as u64 > max {
        return Err(format!("{}: too large", p.display()));
    }
    Ok(v)
}
fn read_text(p: &Path) -> Result<String, String> {
    String::from_utf8(read_limited(p, k::MAX_META_FILE)?).map_err(|_| format!("{}: not text", p.display()))
}
fn write_file(p: &Path, d: &[u8]) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, d).and_then(|_| fs::rename(&tmp, p)).map_err(|e| format!("{}: {e}", p.display()))
}

/// The command that updates the helper on this system (os-release ID/ID_LIKE).
pub fn helper_update_command() -> String {
    let osr = fs::read_to_string(sys::path("/etc/os-release")).or_else(|_| fs::read_to_string(sys::path("/usr/lib/os-release"))).unwrap_or_default();
    let get = |k: &str| osr.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").trim_matches('"').to_string();
    let ids = format!("{} {}", get("ID"), get("ID_LIKE"));
    let has = |x: &str| ids.split_whitespace().any(|w| w == x);
    if has("nixos") {
        "nix flake update tb323fu-linux && sudo nixos-rebuild switch".into()
    } else if has("steamos") {
        "sudo tb323fu-ctl self-update (not available yet: see docs/helper.md)".into()
    } else if has("debian") || has("ubuntu") {
        "sudo apt update && sudo apt install tb323fu-helper".into()
    } else if has("arch") {
        "sudo pacman -Syu tb323fu-helper".into()
    } else if has("fedora") {
        "sudo dnf upgrade tb323fu-helper".into()
    } else {
        "see https://github.com/joonhoekim/tb323fu-linux/blob/main/docs/helper.md".into()
    }
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
            s.index = inner.stored_index();
        }
        inner
    }

    fn st(&self) -> std::sync::MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
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

    /// The last verified index on disk (re-verified: the keys may have changed).
    fn stored_index(&self) -> Option<(k::Index, bool)> {
        let d = read_limited(&stage().join("index.json"), k::MAX_META_FILE).ok()?;
        let s = read_text(&stage().join("index.json.minisig")).ok()?;
        k::index_from(&d, &s, &k::trusted_keys(), k::now()).ok()
    }

    fn manifest(&self, tag: &str) -> Result<Manifest, String> {
        if !k::safe_name(tag) {
            return Err(format!("{tag}: not a release tag"));
        }
        let dir = stage().join("manifests");
        let d = read_limited(&dir.join(format!("{tag}.json")), k::MAX_META_FILE).map_err(|_| format!("{tag}: unknown release (check first)"))?;
        let s = read_text(&dir.join(format!("{tag}.json.minisig")))?;
        k::manifest_from(&d, &s, &k::trusted_keys())
    }

    fn running_serial(&self) -> u64 {
        k::running_serial(&self.st().ks, &self.run)
    }

    /// (tag, release, notes URL, serial) of the channel's release when it is
    /// newer than the running kernel and the index has not expired.
    fn available(&self) -> Vec<(String, String, String, u32)> {
        let ch = self.sh.cfg().kernel.channel;
        let serial = self.running_serial();
        let s = self.st();
        let Some((idx, false)) = &s.index else { return Vec::new() };
        let Some(e) = idx.channels.get(&ch).cloned() else { return Vec::new() };
        drop(s);
        if e.serial <= serial {
            return Vec::new();
        }
        match self.manifest(&e.tag) {
            Ok(m) if m.serial == e.serial => vec![(m.tag, m.release, m.notes_url, m.serial as u32)],
            _ => Vec::new(),
        }
    }

    fn downloaded(&self) -> Vec<String> {
        let dir = stage().join("staged");
        sys::list_dir(&dir)
            .into_iter()
            .filter(|t| {
                self.manifest(t).is_ok_and(|m| fs::metadata(dir.join(t).join(&m.kernel.file)).is_ok_and(|md| md.len() == m.kernel.size))
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

    /// Let the fetch unit download `items` (name, max bytes, URL) into the
    /// cache directory. Blocking.
    fn fetch(&self, items: &[(String, u64, String)], progress_of: Option<(&str, u64)>) -> Result<(), String> {
        for (name, _, url) in items {
            if !k::safe_name(name) || !k::url_ok(url) {
                return Err(format!("refusing to fetch {name} from {url}"));
            }
            let _ = fs::remove_file(cache().join(name));
        }
        let list: String = items.iter().map(|(n, m, u)| format!("{n} {m} {u}\n")).collect();
        let lp = sys::path(k::FETCH_LIST);
        write_file(&lp, list.as_bytes())?;
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&lp, fs::Permissions::from_mode(0o644));
        }
        // progress: watch the partial file grow while the unit runs
        let done = std::sync::atomic::AtomicBool::new(false);
        let out = std::thread::scope(|sc| {
            if let Some((name, size)) = progress_of {
                let part = cache().join(format!("{name}.part"));
                let (done, progress) = (&done, &self.progress);
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

    /// Check: fetch and verify the index, then the manifest of this
    /// channel's release. Blocking.
    pub fn check(&self) -> Result<String, String> {
        let cfg = self.sh.cfg().kernel;
        let url = cfg.index_url.clone();
        if !k::url_ok(&url) {
            return Err(format!("kernel.index_url is not an https/http/file URL: {url}"));
        }
        self.fetch(&[
            ("index.json".into(), k::MAX_META_FILE, url.clone()),
            ("index.json.minisig".into(), 64 << 10, format!("{url}.minisig")),
        ], None)?;
        let keys = k::trusted_keys();
        let data = read_limited(&cache().join("index.json"), k::MAX_META_FILE)?;
        let sig = read_text(&cache().join("index.json.minisig"))?;
        let now = k::now();
        let (idx, expired) = k::index_from(&data, &sig, &keys, now)?;
        write_file(&stage().join("index.json"), &data)?;
        write_file(&stage().join("index.json.minisig"), sig.as_bytes())?;
        write_file(&stage().join("last-check"), now.to_string().as_bytes())?;
        {
            let mut s = self.st();
            s.index = Some((idx.clone(), expired));
            s.last_check = now;
        }
        if expired {
            return Err(format!("the release index expired on {}; not offering updates from it", idx.expires));
        }
        let Some(e) = idx.channels.get(&cfg.channel) else {
            return Ok(format!("no {} channel in the index", cfg.channel));
        };
        if e.serial <= self.running_serial() {
            return Ok(format!("up to date ({} channel: {})", cfg.channel, e.tag));
        }
        let murl = k::resolve(&url, &e.manifest);
        let name = format!("{}.json", e.tag);
        self.fetch(&[
            (name.clone(), k::MAX_META_FILE, murl.clone()),
            (format!("{name}.minisig"), 64 << 10, format!("{murl}.minisig")),
        ], None)?;
        let md = read_limited(&cache().join(&name), k::MAX_META_FILE)?;
        let ms = read_text(&cache().join(format!("{name}.minisig")))?;
        let m = k::manifest_from(&md, &ms, &keys)?;
        if m.tag != e.tag || m.serial != e.serial {
            return Err(format!("manifest {} does not match the index entry {} ({})", m.tag, e.tag, e.serial));
        }
        let dir = stage().join("manifests");
        write_file(&dir.join(&name), &md)?;
        write_file(&dir.join(format!("{name}.minisig")), ms.as_bytes())?;
        write_file(&dir.join(format!("{}.url", e.tag)), murl.as_bytes())?;
        let note = match k::requirements(&m, env!("CARGO_PKG_VERSION"), k::platform_version().as_deref()) {
            Ok(()) => String::new(),
            Err(why) => format!(" ({why})"),
        };
        Ok(format!("{} available: {}{note}", m.tag, m.release))
    }

    /// Download and verify the kernel file of `tag`. Blocking.
    pub fn download(&self, tag: &str) -> Result<String, String> {
        let m = self.manifest(tag)?;
        let murl = read_text(&stage().join("manifests").join(format!("{tag}.url")))?;
        let kurl = k::resolve(murl.trim(), if m.kernel.url.is_empty() { &m.kernel.file } else { &m.kernel.url });
        self.fetch(&[(m.kernel.file.clone(), m.kernel.size, kurl)], Some((&m.kernel.file, m.kernel.size)))?;
        self.st().op = "verifying".into();
        let data = read_limited(&cache().join(&m.kernel.file), k::MAX_KERNEL_FILE)?;
        k::check_kernel_file(&m, &data)?;
        let dir = stage().join("staged").join(tag);
        write_file(&dir.join(&m.kernel.file), &data)?;
        let _ = fs::remove_file(cache().join(&m.kernel.file));
        Ok(format!("{tag} downloaded and verified ({} MB)", data.len() / 1_000_000))
    }

    /// Install a downloaded release into boot_a as a trial. Blocking.
    pub fn install(&self, tag: &str) -> Result<String, String> {
        let m = self.manifest(tag)?;
        k::requirements(&m, env!("CARGO_PKG_VERSION"), k::platform_version().as_deref())?;
        if m.serial <= self.running_serial() {
            return Err(format!("{tag} is not newer than the running kernel"));
        }
        let data = read_limited(&stage().join("staged").join(tag).join(&m.kernel.file), k::MAX_KERNEL_FILE)
            .map_err(|_| format!("{tag} is not downloaded"))?;
        let dev = k::find_device()?;
        let hash = f::android_hash().unwrap_or_default();
        let run = self.run.clone();
        let msg = boot::with_state_dir(true, k::STATE_REL, |d| k::install(&dev, &hash, &run, d, &m, &data, k::power_ok()))?;
        self.st().pending = tag.to_string();
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

    /// Remove staged downloads that are not newer than the running kernel.
    fn clean_staged(&self) {
        let serial = self.running_serial();
        let dir = stage().join("staged");
        for t in sys::list_dir(&dir) {
            if self.manifest(&t).map(|m| m.serial <= serial).unwrap_or(true) {
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
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Kernel";
    const PROPS: &'static [&'static str] = &["Running", "RunningBuild", "SharedModules", "Channel", "Available", "Downloaded", "State",
        "Progress", "Trial", "TrialChannel", "Tries", "MaxTries", "KeepPending", "Good", "LastFailed", "LastCheck", "IndexExpired",
        "AutoCheck", "HelperLatest", "HelperUpdateCommand", "Message"];
    fn snapshot(&self) -> String {
        let i = &self.0;
        let s = i.st();
        format!("{} {} {} {:?} {:?} {} {} {:?}", s.op, i.progress.load(Ordering::Relaxed), s.message, s.ks, s.index, s.last_check, s.pending,
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

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Kernel")]
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
    /// (tag, release, notes URL, serial): the channel's release when newer
    #[zbus(property)]
    fn available(&self) -> Vec<(String, String, String, u32)> {
        self.0.available()
    }
    /// tags downloaded and verified, ready to install
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
    #[zbus(property)]
    fn trial_channel(&self) -> String {
        self.0.st().ks.trial_channel.clone()
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
    /// the running kernel is a testing-channel trial: it is kept only when
    /// someone presses Keep (Q7)
    #[zbus(property)]
    fn keep_pending(&self) -> bool {
        let s = self.0.st();
        s.ks.trial_running(&self.0.run) && s.ks.trial_channel == "testing"
    }
    /// release of linux-good.img
    #[zbus(property)]
    fn good(&self) -> String {
        self.0.st().ks.good.clone()
    }
    #[zbus(property)]
    fn last_failed(&self) -> String {
        self.0.st().ks.failed.clone()
    }
    /// unix time of the last successful index check (0 never)
    #[zbus(property)]
    fn last_check(&self) -> u64 {
        self.0.st().last_check
    }
    #[zbus(property)]
    fn index_expired(&self) -> bool {
        matches!(self.0.st().index, Some((_, true)))
    }
    #[zbus(property)]
    fn auto_check(&self) -> bool {
        self.0.sh.cfg().kernel.auto_check
    }
    /// a newer helper published in the index ("" none, or notifications off)
    #[zbus(property)]
    fn helper_latest(&self) -> String {
        if !self.0.sh.cfg().kernel.helper_notify {
            return String::new();
        }
        let s = self.0.st();
        match &s.index {
            Some((i, false)) if !i.helper.latest.is_empty() && k::version_cmp(&i.helper.latest, env!("CARGO_PKG_VERSION")).is_gt() => i.helper.latest.clone(),
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
    /// Keep the running trial kernel (testing channel; stable confirms by itself).
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
    /// Release notes (Markdown) from the signed manifest.
    fn notes(&self, tag: String) -> fdo::Result<String> {
        let m = self.0.manifest(&tag).map_err(fdo::Error::Failed)?;
        Ok(if m.notes.is_empty() { m.notes_url } else { m.notes })
    }
    /// Re-read kernel-state (tb323fu-kernel-confirm calls this after confirming).
    async fn refresh(&self, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        let inner = self.0.clone();
        blocking::unblock(move || inner.reload_state()).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    #[zbus(signal)]
    async fn finished(em: &SignalEmitter<'_>, operation: &str, ok: bool, message: &str) -> zbus::Result<()>;
}

fn reboot_now() -> fdo::Result<()> {
    std::process::Command::new("systemctl").arg("reboot").spawn().map(|_| ()).map_err(|e| fdo::Error::Failed(format!("systemctl reboot: {e}")))
}
