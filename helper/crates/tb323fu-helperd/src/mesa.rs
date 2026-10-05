// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The `Mesa` object: the port's Mesa build from `mesa-<version>` releases on
//! GitHub Releases (docs/notes/mesa-channel-design.md; the checks are in
//! `tb323fu_helper_core::mesa`). The release list and the downloads go through
//! the kernel updates' fetch unit; everything is written under
//! /var/lib/tb323fu/mesa, which the daemon may write.

use crate::ifaces::invalidate;
use crate::kernel::{self, cache, read_limited};
use crate::polkit;
use crate::selfupdate;
use crate::Shared;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tb323fu_helper_core::kernel as k;
use tb323fu_helper_core::mesa::{self as m, MesaRelease};
use tb323fu_helper_core::sys;
use zbus::fdo;
use zbus::interface;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;

pub const P_MESA: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Mesa";
const IFACE: &str = "io.github.joonhoekim.OpenDeviceHelper1.Mesa";
const PROPS: &[&str] = &["Installed", "Previous", "Available", "Downloaded", "Enabled", "Trial", "Tries", "MaxTries", "Failed", "FailedReason",
    "Drivers", "Base", "State", "Progress", "Message"];

fn root() -> PathBuf {
    sys::path("/")
}

fn glibc() -> Option<String> {
    let out = Command::new("getconf").arg("GNU_LIBC_VERSION").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    String::from_utf8_lossy(&out.stdout).split_whitespace().nth(1).map(str::to_string)
}

#[derive(Default)]
struct St {
    op: String,
    message: String,
}

pub struct Inner {
    sh: Arc<Shared>,
    kern: Arc<kernel::Inner>,
    st: Mutex<St>,
    progress: AtomicU32,
}

pub struct Mesa(pub Arc<Inner>);

/// This boot's id ("" when unknown).
pub fn boot_id() -> String {
    if std::env::var_os("TB323FU_SYSFS_ROOT").is_some_and(|v| !v.is_empty()) {
        if let Ok(b) = std::env::var("TB323FU_BOOT_ID") {
            return b;
        }
    }
    fs::read_to_string("/proc/sys/kernel/random/boot_id").map(|s| s.trim().to_string()).unwrap_or_default()
}

impl Inner {
    /// Also counts this boot for a version on trial (once per boot).
    pub fn new(sh: Arc<Shared>, kern: Arc<kernel::Inner>) -> Arc<Self> {
        match m::boot_tick(&root(), &boot_id()) {
            Ok(msg) if msg.starts_with("no Mesa") => {}
            Ok(msg) => eprintln!("tb323fu-helperd: {msg}"),
            Err(e) => eprintln!("tb323fu-helperd: mesa trial: {e}"),
        }
        Arc::new(Inner { sh, kern, st: Mutex::new(St::default()), progress: AtomicU32::new(0) })
    }

    fn st(&self) -> std::sync::MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn installed(&self) -> String {
        m::link_version(&root(), "current").unwrap_or_default()
    }

    fn manifest(&self) -> Option<m::Manifest> {
        let v = m::link_version(&root(), "current")?;
        m::Manifest::load(&m::dir(&root()).join("versions").join(v).join("MANIFEST")).ok()
    }

    /// The channel's release newer than the installed version.
    fn newest(&self) -> Option<MesaRelease> {
        let rels = self.kern.mesa_releases();
        let r = m::pick(&rels, &self.sh.cfg().kernel.channel)?;
        let cur = self.installed();
        (cur.is_empty() || k::version_cmp(&r.version, &cur).is_gt()).then(|| r.clone())
    }

    /// A release by version; "" = the channel's newest.
    fn release(&self, v: &str) -> Result<MesaRelease, String> {
        if v.is_empty() {
            return self.newest().ok_or_else(|| "no newer Mesa release (check first)".to_string());
        }
        self.kern.mesa_releases().into_iter().find(|r| r.version == v).ok_or_else(|| format!("Mesa {v}: unknown release (check first)"))
    }

    /// Unpacked versions that are neither current nor previous (ready to install).
    fn downloaded(&self) -> String {
        let r = root();
        let keep = [m::link_version(&r, "current"), m::link_version(&r, "previous")];
        m::versions(&r).into_iter().find(|v| !keep.iter().flatten().any(|x| x == v)).unwrap_or_default()
    }

    fn state_name(&self) -> String {
        let op = self.st().op.clone();
        if !op.is_empty() {
            return op;
        }
        if !self.downloaded().is_empty() {
            return "ready".into();
        }
        if m::State::load(&root()).trial.is_empty() { "idle".into() } else { "trial".into() }
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

    pub fn check(&self) -> Result<String, String> {
        self.kern.refresh_list()?;
        Ok(match self.newest() {
            Some(r) => format!("Mesa {} available", r.version),
            None if self.installed().is_empty() => "no Mesa release published for this channel".into(),
            None => format!("Mesa up to date ({})", self.installed()),
        })
    }

    pub fn download(&self, v: &str) -> Result<String, String> {
        let r = self.release(v)?;
        if !r.min_helper.is_empty() && k::version_cmp(selfupdate::version(), &r.min_helper).is_lt() {
            return Err(format!("Mesa {} needs helper {} or newer (this is {})", r.version, r.min_helper, selfupdate::version()));
        }
        let mut items = vec![(k::SUMS_ASSET.to_string(), k::MAX_SUMS_FILE, r.sums.url.clone(), "asset")];
        if self.sh.cfg().kernel.require_signature {
            let sig = r.sig.as_ref().ok_or("kernel.require_signature is on, but the release has no SHA256SUMS.minisig")?;
            items.push((k::SIG_ASSET.to_string(), 64 << 10, sig.url.clone(), "asset"));
        }
        items.push((r.tarball.name.clone(), r.tarball.size, r.tarball.url.clone(), "asset"));
        self.kern.fetch(&items, Some((&r.tarball.name, r.tarball.size, &self.progress)))?;
        self.st().op = "verifying".into();
        let sums = self.kern.sums_in(&cache())?;
        let data = read_limited(&cache().join(&r.tarball.name), m::MAX_TARBALL)?;
        let man = m::unpack(&root(), &r.version, &r.tarball, &sums, &data)?;
        let _ = fs::remove_file(cache().join(&r.tarball.name));
        if let (false, Some(g)) = (man.min_glibc.is_empty(), glibc()) {
            if k::version_cmp(&g, &man.min_glibc).is_lt() {
                let _ = fs::remove_dir_all(m::dir(&root()).join("versions").join(&r.version));
                return Err(format!("Mesa {} is built for glibc {} or newer; this system has {g}", r.version, man.min_glibc));
            }
        }
        Ok(format!("Mesa {} downloaded and checked ({}, based on upstream {})", r.version, man.drivers.join(", "), man.base))
    }

    pub fn install(&self, v: &str) -> Result<String, String> {
        let v = if v.is_empty() { self.downloaded() } else { v.to_string() };
        if v.is_empty() {
            return Err("no downloaded Mesa version to install".into());
        }
        m::activate(&root(), &v)?;
        let st = m::State::load(&root());
        Ok(if st.enabled {
            format!("Mesa {v} installed and used from the next login (on trial: keep it once applications work)")
        } else {
            format!("Mesa {v} installed; switch the channel on to use it")
        })
    }
}

impl Mesa {
    async fn run(&self, op: &'static str, em: &SignalEmitter<'_>, f: impl FnOnce(&Inner) -> Result<String, String> + Send + 'static) -> fdo::Result<String> {
        self.0.begin(op)?;
        invalidate(em, IFACE, PROPS).await;
        let inner = self.0.clone();
        let res = blocking::unblock(move || f(&inner)).await;
        self.0.end(&res);
        let (ok, msg) = match &res {
            Ok(m) => (true, m.as_str()),
            Err(e) => (false, e.as_str()),
        };
        let _ = Mesa::finished(em, op, ok, msg).await;
        invalidate(em, IFACE, PROPS).await;
        res.map_err(fdo::Error::Failed)
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Mesa")]
impl Mesa {
    /// the version in use when the channel is on ("" none installed)
    #[zbus(property)]
    fn installed(&self) -> String {
        self.0.installed()
    }
    /// the version Rollback puts back ("" none)
    #[zbus(property)]
    fn previous(&self) -> String {
        m::link_version(&root(), "previous").unwrap_or_default()
    }
    /// the channel's newer release ("" none, or not checked)
    #[zbus(property)]
    fn available(&self) -> String {
        self.0.newest().map(|r| r.version).unwrap_or_default()
    }
    /// downloaded and checked, ready to install
    #[zbus(property)]
    fn downloaded(&self) -> String {
        self.0.downloaded()
    }
    /// applications use this Mesa (from the next login)
    #[zbus(property)]
    fn enabled(&self) -> bool {
        m::State::load(&root()).enabled
    }
    /// the version on trial ("" none): Keep, or it is switched off after MaxTries starts
    #[zbus(property)]
    fn trial(&self) -> String {
        m::State::load(&root()).trial
    }
    #[zbus(property)]
    fn tries(&self) -> u32 {
        m::State::load(&root()).tries
    }
    #[zbus(property)]
    fn max_tries(&self) -> u32 {
        m::State::load(&root()).max
    }
    /// the version switched off by the trial ("" none)
    #[zbus(property)]
    fn failed(&self) -> String {
        m::State::load(&root()).failed
    }
    #[zbus(property)]
    fn failed_reason(&self) -> String {
        m::State::load(&root()).failed_reason
    }
    /// drivers of the installed version: vulkan, opencl
    #[zbus(property)]
    fn drivers(&self) -> Vec<String> {
        self.0.manifest().map(|m| m.drivers).unwrap_or_default()
    }
    /// the upstream Mesa commit the installed version is based on
    #[zbus(property)]
    fn base(&self) -> String {
        self.0.manifest().map(|m| m.base).unwrap_or_default()
    }
    /// idle / checking / downloading / verifying / ready / trial
    #[zbus(property)]
    fn state(&self) -> String {
        self.0.state_name()
    }
    #[zbus(property)]
    fn progress(&self) -> u32 {
        self.0.progress.load(Ordering::Relaxed)
    }
    #[zbus(property)]
    fn message(&self) -> String {
        self.0.st().message.clone()
    }

    async fn check(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-check", self.0.sh.no_polkit).await?;
        self.run("checking", &em, |i| i.check()).await
    }
    /// "" = the channel's newest release
    async fn download(&self, version: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-download", self.0.sh.no_polkit).await?;
        self.run("downloading", &em, move |i| i.download(&version)).await
    }
    /// Make a downloaded version current ("" = the downloaded one).
    async fn install(&self, version: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "mesa-install", self.0.sh.no_polkit).await?;
        self.run("installing", &em, move |i| i.install(&version)).await
    }
    /// Use this Mesa for applications from the next login (starts a trial), or not.
    async fn set_enabled(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "mesa-install", self.0.sh.no_polkit).await?;
        self.run("switching", &em, move |_| m::set_enabled(&root(), on)).await
    }
    /// Confirm the version on trial.
    async fn keep(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "kernel-keep", self.0.sh.no_polkit).await?;
        self.run("keeping", &em, |_| m::keep(&root())).await
    }
    /// Swap the current and the previous version.
    async fn rollback(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "mesa-install", self.0.sh.no_polkit).await?;
        self.run("rolling-back", &em, |_| m::rollback(&root())).await
    }
    /// Release notes (Markdown); "" = the channel's newest release.
    fn notes(&self, version: String) -> fdo::Result<String> {
        let r = self.0.release(&version).map_err(fdo::Error::Failed)?;
        Ok(if r.notes.is_empty() { r.notes_url } else { r.notes })
    }

    #[zbus(signal)]
    async fn finished(em: &SignalEmitter<'_>, operation: &str, ok: bool, message: &str) -> zbus::Result<()>;
}
