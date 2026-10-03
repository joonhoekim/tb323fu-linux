// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The `HelperUpdate` object: the helper updates itself from a `helper-vX.Y.Z`
//! release on GitHub Releases (docs/helper.md "Helper updates"; the checks
//! are in `tb323fu_helper_core::helperupdate`).
//!
//! The release list and the downloads go through the kernel updates' fetch
//! unit. `Install` and `Rollback` do not write anything outside
//! /var/lib/tb323fu themselves (the daemon cannot: `ProtectSystem=strict`):
//! they start this binary as `tb323fu-helperd --apply DIR` in a transient
//! unit (`systemd-run`), which keeps running while it restarts the daemon:
//! snapshot of every path it will touch, the swap, daemon-reload, D-Bus
//! reload, restart, then the health check (the restarted daemon answers on
//! the bus with the new version and polkit knows its actions) -- else the
//! snapshot goes back and the old daemon is restarted.

use crate::ifaces::{invalidate, Snapshot, ROOT};
use crate::kernel::{self, cache, read_limited, write_file};
use crate::polkit;
use crate::{Shared, BUS};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tb323fu_helper_core::helperupdate::{self as hu, HelperRelease, Manifest, Owner};
use tb323fu_helper_core::kernel as k;
use tb323fu_helper_core::sys;
use zbus::fdo;
use zbus::interface;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;

pub const P_HELPER_UPDATE: &str = "/io/github/joonhoekim/OpenDeviceHelper1/HelperUpdate";
const UNIT: &str = "tb323fu-helper-update.service";
const HEALTH_SECS: u64 = 30;

fn health_secs() -> u64 {
    std::env::var("TB323FU_HEALTH_SECS").ok().filter(|_| test_mode()).and_then(|s| s.parse().ok()).unwrap_or(HEALTH_SECS)
}

fn test_mode() -> bool {
    std::env::var_os("TB323FU_SYSFS_ROOT").is_some_and(|v| !v.is_empty())
}

/// This helper's version (tests on a fake tree may set `TB323FU_TEST_VERSION`).
pub fn version() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| match std::env::var("TB323FU_TEST_VERSION") {
        Ok(v) if test_mode() && hu::safe_version(&v) => v,
        _ => env!("CARGO_PKG_VERSION").to_string(),
    })
}

fn dir() -> PathBuf {
    sys::path(hu::HELPER_DIR)
}
fn root() -> PathBuf {
    sys::path("/")
}
fn staged(v: &str) -> PathBuf {
    dir().join("staged").join(v)
}

fn exe() -> PathBuf {
    let e = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/usr/local/libexec/tb323fu-helperd"));
    PathBuf::from(e.to_string_lossy().trim_end_matches(" (deleted)"))
}

fn run_cmd(c: &str, a: &[&str]) -> Option<(bool, String)> {
    let out = Command::new(c).args(a).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    Some((out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned()))
}

static OWNER: Mutex<Option<Owner>> = Mutex::new(None);

/// Who installed the running helper (cached; `Check` looks again).
pub fn owner() -> Owner {
    if let Some(o) = OWNER.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return o;
    }
    refresh_owner()
}

fn refresh_owner() -> Owner {
    let e = exe().to_string_lossy().into_owned();
    let mut paths = vec![e.as_str()];
    for p in ["/usr/share/polkit-1/actions/io.github.joonhoekim.opendevicehelper.policy", "/usr/share/dbus-1/system-services/io.github.joonhoekim.OpenDeviceHelper1.service"] {
        if sys::path(p).exists() {
            paths.push(p);
        }
    }
    let o = hu::owner_of(&paths, sys::path("/etc/NIXOS").exists(), &run_cmd);
    *OWNER.lock().unwrap_or_else(|e| e.into_inner()) = Some(o.clone());
    o
}

fn glibc() -> Option<String> {
    run_cmd("getconf", &["GNU_LIBC_VERSION"]).filter(|(ok, _)| *ok).and_then(|(_, o)| o.split_whitespace().nth(1).map(str::to_string))
}

fn last_update() -> BTreeMap<String, String> {
    hu::read_kv(&dir().join(hu::STATE_FILE))
}

/// The `--apply` process of an update or rollback is running.
fn applier_running() -> bool {
    let Some(pid) = sys::read_opt(&dir().join("apply.pid")).and_then(|p| p.parse::<u32>().ok()) else { return false };
    fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|c| c.windows(7).any(|w| w == b"--apply"))
}

#[derive(Default)]
struct St {
    op: String,
    message: String,
}

pub struct Inner {
    pub sh: Arc<Shared>,
    kern: Arc<kernel::Inner>,
    st: Mutex<St>,
    progress: AtomicU32,
}

pub struct HelperUpdate(pub Arc<Inner>);

impl Inner {
    pub fn new(sh: Arc<Shared>, kern: Arc<kernel::Inner>) -> Arc<Self> {
        let i = Arc::new(Inner { sh, kern, st: Mutex::new(St::default()), progress: AtomicU32::new(0) });
        for v in sys::list_dir(&dir().join("staged")) {
            if !hu::safe_version(&v) || k::version_cmp(&v, version()).is_le() {
                let _ = fs::remove_dir_all(staged(&v));
            }
        }
        i
    }

    fn st(&self) -> std::sync::MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A release by version; "" = the newest one newer than this helper.
    fn release(&self, v: &str) -> Result<HelperRelease, String> {
        let rels = self.kern.helper_releases();
        if v.is_empty() {
            return hu::newest(&rels, version()).cloned().ok_or_else(|| format!("no helper release newer than {} (check first)", version()));
        }
        if !hu::safe_version(v) {
            return Err(format!("{v}: not a version"));
        }
        rels.into_iter().find(|r| r.version == v).ok_or_else(|| format!("helper {v}: unknown release (check first)"))
    }

    fn newest(&self) -> Option<HelperRelease> {
        hu::newest(&self.kern.helper_releases(), version()).cloned()
    }

    /// The newer release to show ("" none, or notices off: `kernel.helper_notify`).
    fn available(&self) -> String {
        if !self.sh.cfg().kernel.helper_notify {
            return String::new();
        }
        self.newest().map(|r| r.version).unwrap_or_default()
    }

    fn installable(&self) -> bool {
        self.newest().is_some_and(|r| r.installable()) && owner() == Owner::None && (test_mode() || std::env::consts::ARCH == hu::ARCH)
    }

    /// The newest downloaded and checked version newer than this helper.
    fn downloaded(&self) -> String {
        sys::list_dir(&dir().join("staged"))
            .into_iter()
            .filter(|v| hu::safe_version(v) && k::version_cmp(v, version()).is_gt() && staged(v).join("verified").exists())
            .filter(|v| Manifest::load(&staged(v).join("tree/MANIFEST")).is_ok_and(|m| &m.version == v))
            .max_by(|a, b| k::version_cmp(a, b))
            .unwrap_or_default()
    }

    fn previous(&self) -> String {
        Manifest::load(&dir().join("prev/MANIFEST")).map(|m| m.version).unwrap_or_default()
    }

    fn state_name(&self) -> String {
        let op = self.st().op.clone();
        if !op.is_empty() {
            return op;
        }
        if last_update().get("result").map(String::as_str) == Some("installing") {
            return if applier_running() { "installing".into() } else { "interrupted".into() };
        }
        let d = self.downloaded();
        if !d.is_empty() && self.newest().is_some_and(|r| r.version == d) {
            "ready".into()
        } else {
            "idle".into()
        }
    }

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
            Ok(m) | Err(m) => m.clone(),
        };
        self.progress.store(0, Ordering::Relaxed);
    }

    fn may_replace(&self) -> Result<(), String> {
        let o = owner();
        if o != Owner::None {
            return Err(format!("{}; this helper does not replace itself: {}", o.reason(), o.hint()));
        }
        if !test_mode() && std::env::consts::ARCH != hu::ARCH {
            return Err(format!("helper releases are built for {}, this is {}", hu::ARCH, std::env::consts::ARCH));
        }
        if applier_running() {
            return Err("a helper update is running".into());
        }
        Ok(())
    }

    /// Fetch the release list (shared with kernel updates). Blocking.
    pub fn check(&self) -> Result<String, String> {
        self.kern.refresh_list()?;
        let o = refresh_owner();
        let Some(r) = self.newest() else {
            return Ok(format!("helper up to date ({})", version()));
        };
        let how = if !r.installable() {
            format!("published without {}: update it as you installed it", hu::tarball_name(&r.version))
        } else {
            o.hint()
        };
        Ok(format!("helper {} available ({how})", r.version))
    }

    /// Download the release's tarball and SHA256SUMS, check and unpack it. Blocking.
    pub fn download(&self, v: &str) -> Result<String, String> {
        let r = self.release(v)?;
        let (Some(tb), Some(sums)) = (&r.tarball, &r.sums) else {
            return Err(format!("{} has no {} and SHA256SUMS: nothing to update this helper with", r.tag, hu::tarball_name(&r.version)));
        };
        let mut items = vec![(k::SUMS_ASSET.to_string(), k::MAX_SUMS_FILE, sums.url.clone(), "asset")];
        if self.sh.cfg().kernel.require_signature {
            let sig = r.sig.as_ref().ok_or("kernel.require_signature is on, but the release has no SHA256SUMS.minisig")?;
            items.push((k::SIG_ASSET.to_string(), 64 << 10, sig.url.clone(), "asset"));
        }
        items.push((tb.name.clone(), tb.size, tb.url.clone(), "asset"));
        self.kern.fetch(&items, Some((&tb.name, tb.size, &self.progress)))?;
        self.st().op = "verifying".into();
        let sums_map = self.kern.sums_in(&cache())?;
        let data = read_limited(&cache().join(&tb.name), hu::MAX_TARBALL)?;
        let d = staged(&r.version);
        let _ = fs::remove_dir_all(&d);
        let m = hu::unpack_release(&r, &sums_map, &data, &d.join("tree"))?;
        for (n, _, _, _) in &items[..items.len() - 1] {
            write_file(&d.join(n), &read_limited(&cache().join(n), k::MAX_SUMS_FILE)?)?;
        }
        write_file(&d.join("verified"), format!("{} {}\n", r.version, k::sha_hex(&data)).as_bytes())?;
        let _ = fs::remove_file(cache().join(&tb.name));
        let note = match hu::requirements(&r, &m.min_glibc, k::platform_version().as_deref(), self.kern.kernel_serial(), glibc().as_deref()) {
            Ok(()) => String::new(),
            Err(e) => format!(" ({e})"),
        };
        Ok(format!("helper {} downloaded and checked ({} files){note}", r.version, m.files.len()))
    }

    /// Start the transient unit that installs a downloaded release. Blocking.
    pub fn install(&self, v: &str) -> Result<String, String> {
        self.may_replace()?;
        let r = self.release(v)?;
        if !k::version_cmp(&r.version, version()).is_gt() {
            return Err(format!("helper {} is not newer than this one ({})", r.version, version()));
        }
        if !staged(&r.version).join("verified").exists() {
            return Err(format!("helper {} is not downloaded", r.version));
        }
        let tree = staged(&r.version).join("tree");
        let m = Manifest::load(&tree.join("MANIFEST"))?;
        hu::check_tree(&tree, &m)?;
        if m.version != r.version {
            return Err(format!("the download is helper {}, not {}", m.version, r.version));
        }
        hu::requirements(&r, &m.min_glibc, k::platform_version().as_deref(), self.kern.kernel_serial(), glibc().as_deref())?;
        start_applier(&tree, &r.version, false)?;
        Ok(format!("installing helper {}: the helper restarts now; if the new one does not answer within {HEALTH_SECS} s, {} comes back by itself",
            r.version, version()))
    }

    /// Put back the files from before the last update. Blocking.
    pub fn rollback(&self) -> Result<String, String> {
        self.may_replace()?;
        let interrupted = last_update().get("result").map(String::as_str) == Some("installing") && dir().join("backup/MANIFEST").exists();
        let src = dir().join(if interrupted { "backup" } else { "prev" });
        let m = Manifest::load(&src.join("MANIFEST")).map_err(|_| "nothing to go back to: no helper update was installed here".to_string())?;
        hu::check_tree(&src, &m)?;
        start_applier(&src, &m.version, true)?;
        Ok(format!("going back to helper {}: the helper restarts now", m.version))
    }

    fn notes(&self, v: &str) -> Result<String, String> {
        let r = self.release(v)?;
        Ok(if r.notes.is_empty() { r.notes_url } else { r.notes })
    }
}

fn start_applier(src: &Path, to: &str, rollback: bool) -> Result<(), String> {
    let mut args: Vec<String> = vec!["--apply".into(), src.display().to_string(), "--from".into(), version().into(), "--to".into(), to.into()];
    if rollback {
        args.push("--rollback".into());
    }
    if test_mode() {
        args.push("--session".into());
        return Command::new(exe()).args(&args).stdin(Stdio::null()).spawn().map(|_| ()).map_err(|e| format!("starting the update: {e}"));
    }
    let out = Command::new("systemd-run")
        .args(["--unit", UNIT, "--description", "Open Device Helper update", "--collect", "--no-block", "--quiet", "-p", "Type=oneshot", "-p", "TimeoutStartSec=10min", "--"])
        .arg(exe())
        .args(&args)
        .output()
        .map_err(|e| format!("systemd-run: {e}"))?;
    if !out.status.success() {
        return Err(format!("systemd-run: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

// ------------------------------------------------------------------ the --apply process

fn log(msg: &str) {
    eprintln!("tb323fu-helper-update: {msg}");
    let p = dir().join("update.log");
    let mut t = fs::read_to_string(&p).unwrap_or_default();
    if t.len() > 64 << 10 {
        t = t[t.len() - (32 << 10)..].to_string();
        t = t.split_once('\n').map(|(_, r)| r.to_string()).unwrap_or_default();
    }
    t.push_str(&format!("{} {msg}\n", k::now()));
    let _ = fs::write(&p, t);
}

fn set_state(kv: &[(&str, &str)]) {
    let m: BTreeMap<String, String> = kv.iter().map(|(a, b)| (a.to_string(), b.to_string())).chain([("time".to_string(), k::now().to_string())]).collect();
    if let Err(e) = hu::write_kv(&dir().join(hu::STATE_FILE), &m) {
        log(&format!("update-state: {e}"));
    }
}

fn save_current(m: Option<&Manifest>) -> Result<(), String> {
    let p = dir().join(hu::CURRENT_MANIFEST);
    match m.filter(|m| !m.files.is_empty()) {
        Some(m) => {
            let c = Manifest { absent: Vec::new(), ..m.clone() };
            write_file(&p, c.to_text().as_bytes())
        }
        None => {
            let _ = fs::remove_file(&p);
            Ok(())
        }
    }
}

/// daemon-reload, D-Bus configuration reload, restart of the daemon.
fn restart() -> Result<(), String> {
    if test_mode() {
        if let Ok(cmd) = std::env::var("TB323FU_HELPER_RESTART") {
            let _ = Command::new("sh").args(["-c", &cmd]).status();
        }
        return Ok(());
    }
    let sc = |a: &[&str]| -> Result<(), String> {
        let o = Command::new("systemctl").args(a).output().map_err(|e| format!("systemctl: {e}"))?;
        if o.status.success() { Ok(()) } else { Err(format!("systemctl {}: {}", a.join(" "), String::from_utf8_lossy(&o.stderr).trim())) }
    };
    sc(&["daemon-reload"])?;
    let reload = zbus::block_on(async {
        let c = zbus::Connection::system().await?;
        c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "ReloadConfig", &()).await.map(|_| ())
    });
    if let Err(e) = reload {
        log(&format!("D-Bus ReloadConfig: {e}"));
    }
    sc(&["restart", "tb323fu-helperd.service"])
}

async fn answers(expected: &str, session: bool) -> Result<(), String> {
    let c = if session { zbus::Connection::session().await } else { zbus::Connection::system().await }.map_err(|e| e.to_string())?;
    let r = c.call_method(Some(BUS), ROOT, Some("org.freedesktop.DBus.Properties"), "Get", &(BUS, "Version")).await.map_err(|e| e.to_string())?;
    let v: zbus::zvariant::OwnedValue = r.body().deserialize().map_err(|e| e.to_string())?;
    let v = String::try_from(v).map_err(|e| e.to_string())?;
    if v != expected {
        return Err(format!("it answers as {v}"));
    }
    if !session {
        type Action = (String, String, String, String, String, String, u32, u32, u32, std::collections::HashMap<String, String>);
        let r = c.call_method(Some("org.freedesktop.PolicyKit1"), "/org/freedesktop/PolicyKit1/Authority", Some("org.freedesktop.PolicyKit1.Authority"),
            "EnumerateActions", &("",)).await.map_err(|e| format!("polkit: {e}"))?;
        let acts: Vec<Action> = r.body().deserialize().map_err(|e| format!("polkit: {e}"))?;
        let want = format!("{}kernel-check", polkit::PREFIX);
        if !acts.iter().any(|a| a.0 == want) {
            return Err(format!("polkit does not know {want} (the policy file did not load)"));
        }
    }
    Ok(())
}

/// The restarted daemon answers on the bus as `expected` within HEALTH_SECS.
fn health(expected: &str, session: bool) -> Result<(), String> {
    zbus::block_on(async {
        let secs = health_secs();
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            let why = match answers(expected, session).await {
                Ok(()) => return Ok(()),
                Err(e) => e,
            };
            if Instant::now() > end {
                return Err(format!("no answer as helper {expected} within {secs} s ({why})"));
            }
            async_io::Timer::after(Duration::from_secs(1)).await;
        }
    })
}

/// `tb323fu-helperd --apply DIR --from V --to V [--rollback] [--session]`
pub fn apply_main(args: &[String]) -> i32 {
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
    let (src, from, to) = (PathBuf::from(get("--apply")), get("--from"), get("--to"));
    let session = args.iter().any(|a| a == "--session");
    let rollback = args.iter().any(|a| a == "--rollback");
    let d = dir();
    let _ = fs::create_dir_all(&d);
    let _ = fs::write(d.join("apply.pid"), std::process::id().to_string());
    let r = apply_run(&src, &from, &to, rollback, session);
    let _ = fs::remove_file(d.join("apply.pid"));
    match r {
        Ok(m) => {
            log(&m);
            0
        }
        Err(e) => {
            log(&e);
            1
        }
    }
}

fn apply_run(src: &Path, from: &str, to: &str, rollback: bool, session: bool) -> Result<String, String> {
    let d = dir();
    let kind = if rollback { "rollback" } else { "update" };
    log(&format!("{kind} {from} -> {to} from {}", src.display()));
    let fail = |why: String| {
        set_state(&[("result", "failed"), ("from", from), ("to", to), ("kind", kind), ("reason", &why)]);
        why
    };
    let mut src = src.to_path_buf();
    if src == d.join("backup") {
        let i = d.join("interrupted");
        let _ = fs::remove_dir_all(&i);
        fs::rename(&src, &i).map_err(|e| fail(format!("{}: {e}", src.display())))?;
        src = i;
    }
    let m = Manifest::load(&src.join("MANIFEST")).map_err(fail)?;
    hu::check_tree(&src, &m).map_err(fail)?;
    if m.version != to {
        return Err(fail(format!("{} holds helper {}, not {to}", src.display(), m.version)));
    }
    let cur = Manifest::load(&d.join(hu::CURRENT_MANIFEST)).ok();
    let cur_files: BTreeSet<String> = cur.iter().flat_map(|c| c.files.iter().map(|e| e.path.clone())).collect();
    let bak = hu::snapshot(&root(), &(&m.paths() | &cur_files), &d.join("backup"), from).map_err(fail)?;
    set_state(&[("result", "installing"), ("from", from), ("to", to), ("kind", kind), ("backup", "backup")]);

    let up = hu::apply(&root(), &src, &m, &cur_files)
        .and_then(|_| save_current(Some(&m)))
        .and_then(|_| restart())
        .and_then(|_| health(to, session));
    match up {
        Ok(()) => {
            let _ = fs::remove_dir_all(d.join("prev"));
            fs::rename(d.join("backup"), d.join("prev")).map_err(|e| format!("keeping the previous version: {e}"))?;
            if src.starts_with(d.join("staged")) {
                let _ = fs::remove_dir_all(d.join("staged"));
            }
            set_state(&[("result", "ok"), ("from", from), ("to", to), ("kind", kind)]);
            Ok(format!("{} {from} -> {to}", if rollback { "went back" } else { "updated" }))
        }
        Err(why) => {
            log(&format!("{to}: {why}; putting {from} back"));
            let back = hu::apply(&root(), &d.join("backup"), &bak, &BTreeSet::new())
                .and_then(|_| save_current(cur.as_ref()))
                .and_then(|_| restart())
                .and_then(|_| health(from, session));
            let _ = fs::remove_dir_all(d.join("backup"));
            let note = match &back {
                Ok(()) => String::new(),
                Err(e) => format!("; going back failed too: {e}"),
            };
            set_state(&[("result", "rolled-back"), ("from", from), ("to", to), ("kind", kind), ("reason", &format!("{why}{note}"))]);
            Err(format!("helper {to} did not come up ({why}); back on {from}{note}"))
        }
    }
}

// ------------------------------------------------------------------ D-Bus

impl Snapshot for HelperUpdate {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.HelperUpdate";
    const PROPS: &'static [&'static str] = &["Version", "Available", "Installable", "Method", "UpdateCommand", "Reason", "Downloaded", "Previous", "State",
        "Progress", "LastUpdate", "Message", "LastCheck"];
    fn snapshot(&self) -> String {
        let i = &self.0;
        let (op, msg) = { let s = i.st(); (s.op.clone(), s.message.clone()) };
        format!("{op} {} {msg} {} {} {} {} {:?} {} {}", i.progress.load(Ordering::Relaxed), i.available(), i.installable(), i.downloaded(), i.previous(),
            last_update(), i.state_name(), i.kern.last_check())
    }
}

async fn finished(em: &SignalEmitter<'_>, op: &str, res: &Result<String, String>) {
    let (ok, msg) = match res {
        Ok(m) => (true, m.as_str()),
        Err(e) => (false, e.as_str()),
    };
    let _ = HelperUpdate::finished(em, op, ok, msg).await;
    invalidate(em, HelperUpdate::IFACE, HelperUpdate::PROPS).await;
}

impl HelperUpdate {
    async fn run(&self, op: &'static str, em: &SignalEmitter<'_>, f: impl FnOnce(&Inner) -> Result<String, String> + Send + 'static) -> fdo::Result<String> {
        self.0.begin(op)?;
        invalidate(em, Self::IFACE, Self::PROPS).await;
        let inner = self.0.clone();
        let res = blocking::unblock(move || f(&inner)).await;
        self.0.end(&res);
        finished(em, op, &res).await;
        res.map_err(fdo::Error::Failed)
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.HelperUpdate")]
impl HelperUpdate {
    /// this helper's version
    #[zbus(property)]
    fn version(&self) -> String {
        version().into()
    }
    /// the newest `helper-vX.Y.Z` release newer than this helper ("" none, or `kernel.helper_notify` off)
    #[zbus(property)]
    fn available(&self) -> String {
        self.0.available()
    }
    /// that release carries the asset set and this helper may replace itself
    #[zbus(property)]
    fn installable(&self) -> bool {
        self.0.installable()
    }
    /// who updates the helper: self, dpkg, pacman, rpm or nix
    #[zbus(property)]
    fn method(&self) -> String {
        owner().method().into()
    }
    /// what to run to update the helper on this system
    #[zbus(property)]
    fn update_command(&self) -> String {
        owner().hint()
    }
    /// why the helper does not update itself here ("" it does)
    #[zbus(property)]
    fn reason(&self) -> String {
        owner().reason()
    }
    /// a newer version downloaded and checked, ready to install
    #[zbus(property)]
    fn downloaded(&self) -> String {
        self.0.downloaded()
    }
    /// the version Rollback puts back ("" none kept)
    #[zbus(property)]
    fn previous(&self) -> String {
        self.0.previous()
    }
    /// idle / checking / downloading / verifying / ready / installing / interrupted (also rolling-back)
    #[zbus(property)]
    fn state(&self) -> String {
        self.0.state_name()
    }
    #[zbus(property)]
    fn progress(&self) -> u32 {
        self.0.progress.load(Ordering::Relaxed)
    }
    /// the last update or rollback: result (installing / ok / rolled-back / failed), kind, from, to, time, reason
    #[zbus(property)]
    fn last_update(&self) -> std::collections::HashMap<String, String> {
        last_update().into_iter().collect()
    }
    #[zbus(property)]
    fn message(&self) -> String {
        self.0.st().message.clone()
    }
    /// unix time of the last release list (shared with kernel updates)
    #[zbus(property)]
    fn last_check(&self) -> u64 {
        self.0.kern.last_check()
    }

    async fn check(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-check", self.0.sh.no_polkit).await?;
        self.run("checking", &em, |i| i.check()).await
    }
    /// "" = the newest release
    async fn download(&self, version: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-download", self.0.sh.no_polkit).await?;
        self.run("downloading", &em, move |i| i.download(&version)).await
    }
    /// Install a downloaded release ("" = the newest); the helper restarts.
    async fn install(&self, version: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "helper-update", self.0.sh.no_polkit).await?;
        self.run("installing", &em, move |i| i.install(&version)).await
    }
    /// Put back the version from before the last update; the helper restarts.
    async fn rollback(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "helper-update", self.0.sh.no_polkit).await?;
        self.run("rolling-back", &em, |i| i.rollback()).await
    }
    /// Release notes (Markdown); "" = the newest release.
    fn notes(&self, version: String) -> fdo::Result<String> {
        self.0.notes(&version).map_err(fdo::Error::Failed)
    }

    #[zbus(signal)]
    async fn finished(em: &SignalEmitter<'_>, operation: &str, ok: bool, message: &str) -> zbus::Result<()>;
}
