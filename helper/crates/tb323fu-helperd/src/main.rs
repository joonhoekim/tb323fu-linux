// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! tb323fu-helperd: system D-Bus service (`io.github.joonhoekim.OpenDeviceHelper1`)
//! for the device-specific features of the Lenovo TB323FU. See docs/helper.md.
//!
//!   tb323fu-helperd [--session] [--no-polkit]
//!     --session    own the name on the session bus (tests)
//!     --no-polkit  skip authorization (tests on a private bus only)
//! Environment: TB323FU_SYSFS_ROOT (fake device tree), TB323FU_CONFIG (config file).

mod diag;
mod ifaces;
mod kernel;
mod polkit;

use ifaces::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tb323fu_helper_core::config::{config_path, Config};
use tb323fu_helper_core::features as f;
use zbus::object_server::SignalEmitter;

pub const BUS: &str = "io.github.joonhoekim.OpenDeviceHelper1";

pub struct Shared {
    cfg: Mutex<Config>,
    cfg_path: std::path::PathBuf,
    pub no_polkit: bool,
    pub features: Vec<String>,
    pub firmware: Mutex<Option<std::collections::BTreeMap<String, String>>>,
    last_led: Mutex<Option<Option<[u32; 3]>>>,
    last_gpu: Mutex<Option<(String, [u32; 2])>>,
    ppd_profile: Mutex<Option<String>>,
}

impl Shared {
    pub fn cfg(&self) -> Config {
        self.cfg.lock().unwrap().clone()
    }

    /// Change the configuration and persist it.
    pub fn update(&self, change: impl FnOnce(&mut Config)) {
        let mut g = self.cfg.lock().unwrap();
        change(&mut g);
        if let Err(e) = g.save(&self.cfg_path) {
            eprintln!("tb323fu-helperd: saving {}: {e}", self.cfg_path.display());
        }
    }

    pub fn reload(&self) {
        let (c, _) = Config::load(&self.cfg_path);
        *self.cfg.lock().unwrap() = c;
        apply_startup(self);
    }

    /// The profile whose GPU limits are in effect.
    pub fn gpu_profile(&self) -> String {
        let c = self.cfg();
        if c.gpu.follow_power_profiles {
            if let Some(p) = self.ppd_profile.lock().unwrap().clone() {
                return p;
            }
        }
        c.gpu.profile
    }

    /// Apply the GPU floor/cap of the current profile when it changed.
    pub async fn gpu_tick(&self, conn: &zbus::Connection, force: bool) {
        if !f::gpu_dir().exists() {
            return;
        }
        let ppd = if self.cfg().gpu.follow_power_profiles { ppd_active_profile(conn).await } else { None };
        *self.ppd_profile.lock().unwrap() = ppd;
        let profile = self.gpu_profile();
        let Some(lim) = self.cfg().gpu.floors.get(&profile).copied() else { return };
        let mut last = self.last_gpu.lock().unwrap();
        if force || last.as_ref() != Some(&(profile.clone(), lim)) {
            match f::gpu_apply(lim[0], lim[1]) {
                Ok(()) => eprintln!("tb323fu-helperd: GPU profile {profile}: {}..{} MHz", lim[0], lim[1]),
                Err(e) => eprintln!("tb323fu-helperd: GPU: {e}"),
            }
            *last = Some((profile, lim));
        }
    }

    /// Charge indicator on the RGB ring (only writes on a colour change).
    pub fn ledring_tick(&self, force: bool) {
        if !f::ledring_dir().exists() {
            return;
        }
        let cfg = self.cfg();
        let (c, bypass) = (cfg.ledring, cfg.battery.bypass);
        let color = if c.mode == "charge" {
            f::battery_info().ok().and_then(|i| f::ledring_color(&i, c.low_percent, bypass))
        } else {
            None
        };
        let mut last = self.last_led.lock().unwrap();
        if force || last.as_ref() != Some(&color) {
            if let Err(e) = f::ledring_apply(color, c.brightness) {
                eprintln!("tb323fu-helperd: {e}");
            }
            *last = Some(color);
        }
    }
}

/// power-profiles-daemon's ActiveProfile, if it runs.
async fn ppd_active_profile(conn: &zbus::Connection) -> Option<String> {
    for (dest, path, iface) in [
        ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles", "org.freedesktop.UPower.PowerProfiles"),
        ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles", "net.hadess.PowerProfiles"),
    ] {
        let reply = conn
            .call_method(Some(dest), path, Some("org.freedesktop.DBus.Properties"), "Get", &(iface, "ActiveProfile"))
            .await;
        if let Ok(m) = reply {
            if let Ok(v) = m.body().deserialize::<zbus::zvariant::OwnedValue>() {
                if let Ok(s) = String::try_from(v) {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Settings applied once at start (and on Reload).
fn apply_startup(s: &Shared) {
    let c = s.cfg();
    if f::battery_dir().is_some() && !c.battery.bypass {
        if let Err(e) = f::set_charge_limit(c.battery.charge_limit) {
            eprintln!("tb323fu-helperd: charge limit: {e}");
        }
    }
    if f::refresh_available() {
        if let Some(p) = c.refresh.policy.as_deref().and_then(|p| f::policy_value(p).ok()) {
            let _ = f::refresh_set("policy", p);
        }
        for (k, v) in [("hz", c.refresh.hz), ("ms60", c.refresh.ms60), ("ms30", c.refresh.ms30)] {
            if let Some(v) = v {
                let _ = f::refresh_set(k, v);
            }
        }
    }
    if f::cpu_boost().is_some() {
        if let Err(e) = f::set_cpu_boost(c.cpu.boost) {
            eprintln!("tb323fu-helperd: {e}");
        }
    }
    if f::usb_wake().is_some() {
        let _ = f::set_usb_wake(c.usb.wake);
    }
    if let Some(on) = c.usb.dev_mode {
        if f::gadget().is_some() {
            let _ = f::set_dev_mode(on);
        }
    }
}

/// Emit PropertiesChanged for an object whose snapshot changed.
async fn watch<T: zbus::object_server::Interface + Snapshot>(conn: &zbus::Connection, path: &'static str, last: &mut HashMap<&'static str, String>) {
    let Ok(iface) = conn.object_server().interface::<_, T>(path).await else { return };
    let snap = iface.get().await.snapshot();
    if last.get(path) != Some(&snap) {
        if last.contains_key(path) {
            if let Ok(em) = SignalEmitter::new(conn, path) {
                invalidate(&em, T::IFACE, T::PROPS).await;
            }
        }
        last.insert(path, snap);
    }
}

async fn run(no_polkit: bool, session: bool) -> zbus::Result<()> {
    let cfg_path = config_path();
    let (cfg, migrated) = Config::load(&cfg_path);

    let mut features = Vec::new();
    let has_battery = f::battery_dir().is_some();
    let has_android = f::back_to_android_tool().is_some() || f::android_hash().is_some();
    let has_torch = f::torch_dir().exists();
    let has_ledring = f::ledring_dir().exists();
    let has_refresh = f::refresh_available();
    let has_gpu = f::gpu_dir().exists();
    let has_usb = f::usb_wake().is_some() || f::gadget().is_some();
    let has_boot = !tb323fu_helper_core::boot::partitions().is_empty();
    let has_thermal = f::thermal().is_some();
    let has_kernel = tb323fu_helper_core::kernel::find_device().is_ok();
    for (on, name) in [(has_battery, "Battery"), (has_android, "Android"), (has_torch, "Torch"), (has_ledring, "LedRing"),
        (has_refresh, "Refresh"), (has_gpu, "Gpu"), (has_usb, "Usb"), (true, "EmergencyKey"), (true, "Diagnostics"), (has_boot, "Boot"),
        (has_thermal, "Thermal"), (has_kernel, "Kernel")] {
        if on {
            features.push(name.to_string());
        }
    }

    let shared = Arc::new(Shared {
        cfg: Mutex::new(cfg),
        cfg_path,
        no_polkit,
        features,
        firmware: Mutex::new(None),
        last_led: Mutex::new(None),
        last_gpu: Mutex::new(None),
        ppd_profile: Mutex::new(None),
    });
    if migrated {
        shared.update(|_| {}); // write the defaults (+ migrated legacy values) once
    }
    apply_startup(&shared);

    let b = if session { zbus::connection::Builder::session()? } else { zbus::connection::Builder::system()? };
    let mut b = b.serve_at(ROOT, Helper(shared.clone()))?;
    if has_battery {
        b = b.serve_at(P_BATTERY, Battery(shared.clone()))?;
    }
    if has_android {
        b = b.serve_at(P_ANDROID, Android(shared.clone()))?;
    }
    if has_torch {
        b = b.serve_at(P_TORCH, Torch(shared.clone()))?;
    }
    if has_ledring {
        b = b.serve_at(P_LEDRING, LedRing(shared.clone()))?;
    }
    if has_refresh {
        b = b.serve_at(P_REFRESH, Refresh(shared.clone()))?;
    }
    if has_gpu {
        b = b.serve_at(P_GPU, Gpu(shared.clone()))?;
    }
    if has_usb {
        b = b.serve_at(P_USB, Usb(shared.clone()))?;
    }
    b = b.serve_at(P_EMERGENCY, EmergencyKey(shared.clone()))?;
    b = b.serve_at(P_DIAG, Diagnostics(shared.clone()))?;
    if has_boot {
        b = b.serve_at(P_BOOT, Boot::new(shared.clone()))?;
    }
    let kern = kernel::Inner::new(shared.clone());
    if has_kernel {
        b = b.serve_at(kernel::P_KERNEL, kernel::Kernel(kern.clone()))?;
        // kernel-state may sit on another root: read it off the executor
        let k2 = kern.clone();
        std::thread::spawn(move || {
            k2.reload_state();
            k2.clean_staged();
        });
    }
    if has_thermal {
        b = b.serve_at(P_THERMAL, Thermal)?;
    }
    let conn = b.name(BUS)?.build().await?;
    eprintln!("tb323fu-helperd {} on {} bus: {}", env!("CARGO_PKG_VERSION"), if session { "session" } else { "system" },
        shared.features.join(" "));

    // Poller: LED ring and GPU follow, re-assert USB wake, and PropertiesChanged
    // for objects whose values changed underneath (charger plugged, kernel knob
    // written by someone else, ...). 5 s is enough for all of these.
    let mut last: HashMap<&'static str, String> = HashMap::new();
    loop {
        shared.ledring_tick(false);
        shared.gpu_tick(&conn, false).await;
        let want = shared.cfg().usb.wake;
        if f::usb_wake().is_some_and(|w| w != want) {
            let _ = f::set_usb_wake(want);
        }
        if has_battery {
            let before = last.get(P_BATTERY).cloned();
            watch::<Battery>(&conn, P_BATTERY, &mut last).await;
            if before.is_some() && before.as_ref() != last.get(P_BATTERY) {
                if let Ok(em) = SignalEmitter::new(&conn, P_BATTERY) {
                    let _ = Battery::changed(&em).await;
                }
            }
        }
        if has_android { watch::<Android>(&conn, P_ANDROID, &mut last).await; }
        if has_torch { watch::<Torch>(&conn, P_TORCH, &mut last).await; }
        if has_ledring { watch::<LedRing>(&conn, P_LEDRING, &mut last).await; }
        if has_refresh { watch::<Refresh>(&conn, P_REFRESH, &mut last).await; }
        if has_gpu { watch::<Gpu>(&conn, P_GPU, &mut last).await; }
        if has_usb { watch::<Usb>(&conn, P_USB, &mut last).await; }
        watch::<EmergencyKey>(&conn, P_EMERGENCY, &mut last).await;
        watch::<Diagnostics>(&conn, P_DIAG, &mut last).await;
        if has_boot { watch::<Boot>(&conn, P_BOOT, &mut last).await; }
        if has_thermal { watch::<Thermal>(&conn, P_THERMAL, &mut last).await; }
        if has_kernel {
            kern.poll_state();
            kern.auto_tick();
            watch::<kernel::Kernel>(&conn, kernel::P_KERNEL, &mut last).await;
        }
        async_io::Timer::after(Duration::from_secs(5)).await;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("usage: tb323fu-helperd [--session] [--no-polkit]");
        return;
    }
    let no_polkit = args.iter().any(|a| a == "--no-polkit");
    let session = args.iter().any(|a| a == "--session");
    if no_polkit {
        eprintln!("tb323fu-helperd: WARNING: --no-polkit, every caller is authorized (tests only)");
    }
    if let Err(e) = zbus::block_on(run(no_polkit, session)) {
        eprintln!("tb323fu-helperd: {e}");
        std::process::exit(1);
    }
}
