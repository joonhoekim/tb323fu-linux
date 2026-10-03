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
mod nm;
mod polkit;

use ifaces::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tb323fu_helper_core::config::{config_path, Config};
use tb323fu_helper_core::features as f;
use tb323fu_helper_core::perf;
use zbus::object_server::SignalEmitter;

pub const BUS: &str = "io.github.joonhoekim.OpenDeviceHelper1";

/// State that lives only while the daemon runs.
#[derive(Default)]
pub struct Runtime {
    /// thermal profile last written
    thermal_applied: Option<String>,
    /// the user switched Bypass off while performance had switched it on
    pub bypass_declined: bool,
    /// the panel heat limit holds the backlight; the brightness to restore
    panel_limited: bool,
    panel_saved: Option<u32>,
    /// CPU limits last written: (profile, limits, boost)
    cpu_applied: Option<(String, [u32; 4], Option<bool>)>,
    /// Wi-Fi: power saving wanted off; interfaces the helper switched off
    wifi_want_off: bool,
    wifi_off: Vec<String>,
    wifi_ticks: u32,
    /// motors the gain was written to (a reloaded driver gets it again)
    haptics_seen: Vec<std::path::PathBuf>,
    /// "full by": the target (local week minute) of the window in progress
    pub full_by_target: Option<i32>,
}

pub struct Shared {
    cfg: Mutex<Config>,
    cfg_path: std::path::PathBuf,
    pub no_polkit: bool,
    pub features: Vec<String>,
    pub firmware: Mutex<Option<std::collections::BTreeMap<String, String>>>,
    last_led: Mutex<Option<Option<[u32; 3]>>>,
    last_gpu: Mutex<Option<(String, [u32; 2])>>,
    ppd_profile: Mutex<Option<String>>,
    pub rt: Mutex<Runtime>,
}

/// The thermal profile that goes with a performance profile.
pub fn thermal_for(perf_profile: &str) -> &'static str {
    match perf_profile {
        "power-saver" => "quiet",
        "performance" => "performance",
        _ => "default",
    }
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
        let ppd = if self.cfg().gpu.follow_power_profiles { ppd_active_profile(conn).await } else { None };
        *self.ppd_profile.lock().unwrap() = ppd;
        let profile = self.gpu_profile();
        if let (true, Some(lim)) = (f::gpu_dir().exists(), self.cfg().gpu.floors.get(&profile).copied()) {
            let mut last = self.last_gpu.lock().unwrap();
            if force || last.as_ref() != Some(&(profile.clone(), lim)) {
                match f::gpu_apply(lim[0], lim[1]) {
                    Ok(()) => eprintln!("tb323fu-helperd: GPU profile {profile}: {}..{} MHz", lim[0], lim[1]),
                    Err(e) => eprintln!("tb323fu-helperd: GPU: {e}"),
                }
                *last = Some((profile, lim));
            }
        }
        self.cpu_tick(force);
    }

    /// The CPU limits of a performance profile (the whole range without an entry).
    pub fn cpu_limits_of(&self, profile: &str) -> Option<[u32; 4]> {
        self.cfg().cpu.limits.get(profile).copied().or_else(perf::cpu_full_range)
    }

    /// Apply the CPU limits of the current profile when they (or boost, which
    /// resets the maximum) changed.
    pub fn cpu_tick(&self, force: bool) {
        if !perf::cpu_limits_available() {
            return;
        }
        let profile = self.gpu_profile();
        let Some(lim) = self.cpu_limits_of(&profile) else { return };
        let key = (profile.clone(), lim, f::cpu_boost());
        let mut rt = self.rt.lock().unwrap();
        if force || rt.cpu_applied.as_ref() != Some(&key) {
            match perf::cpu_apply(lim) {
                Ok(()) => eprintln!("tb323fu-helperd: CPU profile {profile}: little {}..{}, big {}..{} MHz", lim[0], lim[1], lim[2], lim[3]),
                Err(e) => eprintln!("tb323fu-helperd: CPU: {e}"),
            }
            rt.cpu_applied = Some(key);
        }
    }

    /// The thermal profile that should be in effect.
    pub fn thermal_target(&self) -> String {
        let c = self.cfg();
        if c.thermal.follow_performance { thermal_for(&self.gpu_profile()).to_string() } else { c.thermal.profile }
    }

    /// Write the thermal profile when it changed; end `performance` when a
    /// battery or the board gets too hot (persisted as `default`, signalled).
    pub async fn thermal_tick(&self, conn: &zbus::Connection, force: bool) {
        if !perf::thermal_profile_available() {
            return;
        }
        let mut want = self.thermal_target();
        if want == "performance" {
            if let Some(reason) = perf::performance_guard_now() {
                eprintln!("tb323fu-helperd: thermal profile performance ended: {reason}");
                self.update(|c| {
                    c.thermal.profile = "default".into();
                    c.thermal.follow_performance = false;
                });
                want = "default".into();
                if let Ok(em) = SignalEmitter::new(conn, P_THERMAL) {
                    let _ = Thermal::profile_fallback(&em, &reason).await;
                    invalidate(&em, Thermal::IFACE, Thermal::PROPS).await;
                }
            }
        }
        let changed = force || self.rt.lock().unwrap().thermal_applied.as_deref() != Some(want.as_str());
        if changed {
            match perf::apply_thermal_profile(&want) {
                Ok(()) => eprintln!("tb323fu-helperd: thermal profile {want}"),
                Err(e) => eprintln!("tb323fu-helperd: thermal profile {want}: {e}"),
            }
            self.rt.lock().unwrap().thermal_applied = Some(want.clone());
        }
        self.performance_bypass_tick(want == "performance");
    }

    /// Bypass while the thermal profile is performance on external power
    /// (when `performance_bypass`), undone when performance ends.
    fn performance_bypass_tick(&self, perf_on: bool) {
        let c = self.cfg();
        let declined = self.rt.lock().unwrap().bypass_declined;
        if perf_on {
            if c.thermal.performance_bypass && !c.battery.bypass && !declined && f::battery_dir().is_some() && f::external_power() {
                match self.set_bypass(true) {
                    Ok(()) => {
                        self.update(|c| c.thermal.bypass_auto = true);
                        eprintln!("tb323fu-helperd: Bypass on for the performance thermal profile");
                    }
                    Err(e) => eprintln!("tb323fu-helperd: Bypass: {e}"),
                }
            }
            return;
        }
        self.rt.lock().unwrap().bypass_declined = false;
        if c.thermal.bypass_auto {
            if c.battery.bypass {
                if let Err(e) = self.set_bypass(false) {
                    eprintln!("tb323fu-helperd: Bypass: {e}");
                }
            }
            self.update(|c| c.thermal.bypass_auto = false);
        }
    }

    /// Bypass: hold the battery where it is (limit = current capacity);
    /// off restores the previous limit.
    pub fn set_bypass(&self, on: bool) -> Result<(), String> {
        let cfg = self.cfg();
        if on && !cfg.battery.bypass {
            let info = f::battery_info()?;
            let hold = info.capacity.max(f::CHARGE_LIMIT_MIN).min(100);
            f::set_charge_limit(hold, cfg.battery.recharge_gap)?;
            let prev = cfg.battery.charge_limit;
            self.update(|c| {
                c.battery.saved_limit = Some(prev);
                c.battery.bypass = true;
            });
        } else if !on && cfg.battery.bypass {
            let back = cfg.battery.saved_limit.unwrap_or(cfg.battery.charge_limit);
            f::set_charge_limit(back, cfg.battery.recharge_gap)?;
            self.update(|c| {
                c.battery.charge_limit = back;
                c.battery.bypass = false;
                c.battery.saved_limit = None;
            });
        }
        Ok(())
    }

    /// Hold the backlight at 178/255 while the panel is hot; give back the
    /// brightness afterwards unless someone changed it meanwhile.
    pub fn panel_tick(&self) {
        if !perf::panel_limit_available() {
            return;
        }
        let on = self.cfg().thermal.panel_limit;
        let mut rt = self.rt.lock().unwrap();
        let limit = on && perf::panel_temp_mc().is_some_and(|t| perf::panel_should_limit(t, rt.panel_limited));
        let Some((b, max)) = perf::backlight() else { return };
        let cap = perf::panel_cap(max);
        if limit {
            if b > cap {
                if !rt.panel_limited {
                    eprintln!("tb323fu-helperd: panel hot: backlight held at {cap}/{max}");
                }
                rt.panel_saved = Some(b);
                if let Err(e) = perf::set_backlight(cap) {
                    eprintln!("tb323fu-helperd: {e}");
                }
            }
            rt.panel_limited = true;
        } else if rt.panel_limited {
            rt.panel_limited = false;
            if let Some(s) = rt.panel_saved.take() {
                if b == cap {
                    let _ = perf::set_backlight(s);
                }
            }
        }
    }

    pub fn panel_limited(&self) -> bool {
        self.rt.lock().unwrap().panel_limited
    }

    /// Wi-Fi power saving off while the performance profile is in effect
    /// (`wifi.low_latency_performance`); back on afterwards. Interfaces whose
    /// connection profile sets power saving belong to NetworkManager and are
    /// left alone. Re-checked once a minute (a reconnect can reset it).
    pub async fn wifi_tick(&self, conn: &zbus::Connection, force: bool) {
        let want_off = self.cfg().wifi.low_latency_performance && self.gpu_profile() == "performance";
        let (run, restore) = {
            let mut rt = self.rt.lock().unwrap();
            rt.wifi_ticks = rt.wifi_ticks.wrapping_add(1);
            let changed = rt.wifi_want_off != want_off;
            rt.wifi_want_off = want_off;
            let run = want_off && (force || changed || rt.wifi_ticks % 12 == 0);
            let restore = if !want_off { std::mem::take(&mut rt.wifi_off) } else { Vec::new() };
            (run, restore)
        };
        for i in restore {
            match perf::set_wifi_power_save(&i, true) {
                Ok(()) => eprintln!("tb323fu-helperd: Wi-Fi {i}: power saving back on"),
                Err(e) => eprintln!("tb323fu-helperd: Wi-Fi {i}: {e}"),
            }
        }
        if !run || perf::iw().is_none() {
            return;
        }
        for i in perf::wifi_interfaces() {
            if nm::owns_power_save(conn, &i).await || perf::wifi_power_save(&i) != Some(true) {
                continue;
            }
            match perf::set_wifi_power_save(&i, false) {
                Ok(()) => {
                    eprintln!("tb323fu-helperd: Wi-Fi {i}: power saving off (performance)");
                    let mut rt = self.rt.lock().unwrap();
                    if !rt.wifi_off.contains(&i) {
                        rt.wifi_off.push(i);
                    }
                }
                Err(e) => eprintln!("tb323fu-helperd: Wi-Fi {i}: {e}"),
            }
        }
    }

    pub fn wifi_available(&self) -> bool {
        perf::iw().is_some() && !perf::wifi_interfaces().is_empty()
    }

    /// "Full by": raise the limit to 100 % when the estimated charging time
    /// before the target begins; back to the configured limit two hours after
    /// the target, or at once when unplugged after it. Not while Bypass is on.
    pub fn full_by_tick(&self) {
        use tb323fu_helper_core::schedule as sch;
        let c = self.cfg();
        let Ok(target_min) = sch::parse_hhmm(&c.battery.full_by) else {
            self.full_by_end(false);
            return;
        };
        if c.battery.bypass || f::battery_dir().is_none() {
            self.full_by_end(false);
            return;
        }
        let Some(now) = sch::now_week_minute() else { return };
        let plugged = f::external_power();
        let active = self.rt.lock().unwrap().full_by_target;
        match active {
            Some(t) => {
                let d = sch::delta_to(now, t);
                if d <= -sch::HOLD_AFTER_MIN || (d <= 0 && !plugged) {
                    self.full_by_end(true);
                }
            }
            None => {
                let Some(until) = sch::until_target(now, target_min, &c.battery.full_by_days) else { return };
                let Ok(i) = f::battery_info() else { return };
                let input = f::charger_info();
                let need = sch::charge_minutes(i.capacity, i.charge_counter_mah, input.input_mv * input.input_ma / 1000);
                if until > 0 && until <= need && i.charge_limit < 100 {
                    match f::set_charge_limit(100, c.battery.recharge_gap) {
                        Ok(()) => {
                            eprintln!("tb323fu-helperd: full by {}: charging to 100 % ({until} min left, ~{need} min needed)", c.battery.full_by);
                            self.rt.lock().unwrap().full_by_target = Some((now + until).rem_euclid(sch::WEEK));
                        }
                        Err(e) => eprintln!("tb323fu-helperd: full by: {e}"),
                    }
                }
            }
        }
    }

    /// End a "full by" window (with `restore`, write the configured limit back).
    pub fn full_by_end(&self, restore: bool) {
        if self.rt.lock().unwrap().full_by_target.take().is_none() {
            return;
        }
        let c = self.cfg();
        if restore && !c.battery.bypass {
            match f::set_charge_limit(c.battery.charge_limit, c.battery.recharge_gap) {
                Ok(()) => eprintln!("tb323fu-helperd: full by: limit back to {} %", c.battery.charge_limit),
                Err(e) => eprintln!("tb323fu-helperd: full by: {e}"),
            }
        }
    }

    pub fn full_by_active(&self) -> bool {
        self.rt.lock().unwrap().full_by_target.is_some()
    }

    /// Write the vibration strength to motors that appeared (start, driver reload).
    pub fn haptics_tick(&self) {
        let nodes: Vec<std::path::PathBuf> = tb323fu_helper_core::haptics::motors().into_iter().map(|m| m.node).collect();
        let mut rt = self.rt.lock().unwrap();
        if nodes.is_empty() || rt.haptics_seen == nodes {
            return;
        }
        let s = self.cfg().haptics.strength;
        match tb323fu_helper_core::haptics::set_strength(s) {
            Ok(()) => eprintln!("tb323fu-helperd: vibration strength {s} %"),
            Err(e) => eprintln!("tb323fu-helperd: vibration: {e}"),
        }
        rt.haptics_seen = nodes;
    }

    /// Everything that follows the performance profile.
    pub async fn profile_tick(&self, conn: &zbus::Connection, force: bool) {
        self.gpu_tick(conn, force).await;
        self.thermal_tick(conn, force).await;
        self.wifi_tick(conn, force).await;
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
        if let Err(e) = f::set_charge_limit(c.battery.charge_limit, c.battery.recharge_gap) {
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
    let has_gpu = f::gpu_dir().exists() || perf::cpu_limits_available();
    let has_usb = f::usb_wake().is_some() || f::gadget().is_some();
    let has_boot = !tb323fu_helper_core::boot::partitions().is_empty();
    let has_thermal = f::thermal().is_some() || perf::thermal_profile_available();
    let has_kernel = tb323fu_helper_core::kernel::find_device().is_ok();
    let has_haptics = !tb323fu_helper_core::haptics::motors().is_empty();
    for (on, name) in [(has_battery, "Battery"), (has_android, "Android"), (has_torch, "Torch"), (has_ledring, "LedRing"),
        (has_refresh, "Refresh"), (has_gpu, "Gpu"), (has_usb, "Usb"), (true, "EmergencyKey"), (true, "Diagnostics"), (has_boot, "Boot"),
        (has_thermal, "Thermal"), (has_kernel, "Kernel"), (has_haptics, "Haptics")] {
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
        rt: Mutex::new(Runtime::default()),
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
        b = b.serve_at(P_THERMAL, Thermal(shared.clone()))?;
    }
    if has_haptics {
        b = b.serve_at(P_HAPTICS, Haptics(shared.clone()))?;
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
        shared.profile_tick(&conn, false).await;
        shared.panel_tick();
        shared.haptics_tick();
        shared.full_by_tick();
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
        if has_haptics { watch::<Haptics>(&conn, P_HAPTICS, &mut last).await; }
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
