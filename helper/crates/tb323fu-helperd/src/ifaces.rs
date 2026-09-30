// SPDX-License-Identifier: MIT
//! The D-Bus objects of `io.github.joonhoekim.tb323fu.Helper` (see
//! docs/helper.md for the contract). Property getters read the device live;
//! setters check polkit, write the device, persist the setting and emit
//! PropertiesChanged (with the changed properties invalidated).

use crate::polkit;
use crate::Shared;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tb323fu_helper_core::boot;
use tb323fu_helper_core::features as f;
use std::sync::Mutex;
use zbus::fdo;
use zbus::interface;
use zbus::message::Header;
use zbus::names::InterfaceName;
use zbus::object_server::SignalEmitter;

pub const ROOT: &str = "/io/github/joonhoekim/tb323fu/Helper";
pub const P_BATTERY: &str = "/io/github/joonhoekim/tb323fu/Helper/Battery";
pub const P_ANDROID: &str = "/io/github/joonhoekim/tb323fu/Helper/Android";
pub const P_TORCH: &str = "/io/github/joonhoekim/tb323fu/Helper/Torch";
pub const P_LEDRING: &str = "/io/github/joonhoekim/tb323fu/Helper/LedRing";
pub const P_REFRESH: &str = "/io/github/joonhoekim/tb323fu/Helper/Refresh";
pub const P_GPU: &str = "/io/github/joonhoekim/tb323fu/Helper/Gpu";
pub const P_USB: &str = "/io/github/joonhoekim/tb323fu/Helper/Usb";
pub const P_EMERGENCY: &str = "/io/github/joonhoekim/tb323fu/Helper/EmergencyKey";
pub const P_DIAG: &str = "/io/github/joonhoekim/tb323fu/Helper/Diagnostics";
pub const P_BOOT: &str = "/io/github/joonhoekim/tb323fu/Helper/Boot";

fn failed(e: String) -> fdo::Error {
    fdo::Error::Failed(e)
}
fn invalid(e: String) -> fdo::Error {
    fdo::Error::InvalidArgs(e)
}

/// Emit PropertiesChanged with the given properties invalidated (clients re-read).
pub async fn invalidate(em: &SignalEmitter<'_>, iface: &str, props: &[&str]) {
    if let Ok(name) = InterfaceName::try_from(iface) {
        let _ = fdo::Properties::properties_changed(em, name, HashMap::new(), Cow::Borrowed(props)).await;
    }
}

/// Every object exposes `snapshot()` (all property values as text) for the poller.
pub trait Snapshot {
    const IFACE: &'static str;
    const PROPS: &'static [&'static str];
    fn snapshot(&self) -> String;
}

// ------------------------------------------------------------------ Battery

pub struct Battery(pub Arc<Shared>);

impl Snapshot for Battery {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Battery";
    const PROPS: &'static [&'static str] = &["ChargeLimit", "Bypass", "Status", "State", "Capacity", "CurrentMa", "VoltageMv",
        "TemperatureC", "Health", "CycleCount", "DesignCapacityMah", "ChargerType", "ChargerContract"];
    fn snapshot(&self) -> String {
        // current/voltage/temperature move all the time: only state-like values
        // trigger a signal (clients poll the live values while showing them)
        let i = f::battery_info().ok();
        let (t, c) = f::charger();
        format!("{:?} {} {:?} {t} {c}", i.as_ref().map(|i| (i.charge_limit, i.capacity, &i.status, &i.health)),
            self.0.cfg().battery.bypass, i.as_ref().map(f::battery_state))
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Battery")]
impl Battery {
    #[zbus(property)]
    fn charge_limit(&self) -> u32 {
        f::battery_info().map(|i| i.charge_limit).unwrap_or(100)
    }
    #[zbus(property)]
    fn bypass(&self) -> bool {
        self.0.cfg().battery.bypass
    }
    #[zbus(property)]
    fn status(&self) -> String {
        f::battery_info().map(|i| i.status).unwrap_or_else(|_| "Unknown".into())
    }
    #[zbus(property)]
    fn state(&self) -> String {
        f::battery_info().map(|i| f::battery_state(&i).to_string()).unwrap_or_else(|_| "unknown".into())
    }
    #[zbus(property)]
    fn capacity(&self) -> u32 {
        f::battery_info().map(|i| i.capacity).unwrap_or(0)
    }
    #[zbus(property)]
    fn current_ma(&self) -> i32 {
        f::battery_info().map(|i| i.current_ma).unwrap_or(0)
    }
    #[zbus(property)]
    fn voltage_mv(&self) -> u32 {
        f::battery_info().map(|i| i.voltage_mv).unwrap_or(0)
    }
    #[zbus(property)]
    fn temperature_c(&self) -> f64 {
        f::battery_info().map(|i| i.temperature_c).unwrap_or(f64::NAN)
    }
    #[zbus(property)]
    fn health(&self) -> String {
        f::battery_info().map(|i| i.health).unwrap_or_else(|_| "Unknown".into())
    }
    #[zbus(property)]
    fn cycle_count(&self) -> i32 {
        f::battery_info().map(|i| i.cycle_count).unwrap_or(-1)
    }
    #[zbus(property)]
    fn design_capacity_mah(&self) -> i32 {
        f::battery_info().map(|i| i.design_capacity_mah).unwrap_or(-1)
    }
    #[zbus(property)]
    fn charger_type(&self) -> String {
        f::charger().0
    }
    #[zbus(property)]
    fn charger_contract(&self) -> String {
        f::charger().1
    }

    async fn set_charge_limit(&self, percent: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "charge-limit", self.0.no_polkit).await?;
        f::set_charge_limit(percent).map_err(invalid)?;
        self.0.update(|c| {
            c.battery.charge_limit = percent;
            c.battery.bypass = false;
            c.battery.saved_limit = None;
        });
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    /// Bypass: hold the battery where it is (limit = current capacity) while
    /// external power feeds the system; off restores the previous limit.
    async fn set_bypass(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "charge-limit", self.0.no_polkit).await?;
        let info = f::battery_info().map_err(failed)?;
        let cfg = self.0.cfg();
        if on && !cfg.battery.bypass {
            let hold = info.capacity.max(f::CHARGE_LIMIT_MIN).min(100);
            f::set_charge_limit(hold).map_err(failed)?;
            let prev = cfg.battery.charge_limit;
            self.0.update(|c| {
                c.battery.saved_limit = Some(prev);
                c.battery.bypass = true;
            });
        } else if !on && cfg.battery.bypass {
            let back = cfg.battery.saved_limit.unwrap_or(cfg.battery.charge_limit);
            f::set_charge_limit(back).map_err(failed)?;
            self.0.update(|c| {
                c.battery.charge_limit = back;
                c.battery.bypass = false;
                c.battery.saved_limit = None;
            });
        }
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    #[zbus(signal)]
    pub async fn changed(em: &SignalEmitter<'_>) -> zbus::Result<()>;
}

// ------------------------------------------------------------------ Android

pub struct Android(pub Arc<Shared>);

impl Snapshot for Android {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Android";
    const PROPS: &'static [&'static str] = &["Available", "ImageSha256", "RequireAuth"];
    fn snapshot(&self) -> String {
        format!("{:?} {:?} {}", f::android_hash(), f::back_to_android_tool(), self.0.cfg().android.require_auth)
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Android")]
impl Android {
    #[zbus(property)]
    fn available(&self) -> bool {
        f::android_hash().is_some() && f::back_to_android_tool().is_some()
    }
    #[zbus(property)]
    fn image_sha256(&self) -> String {
        f::android_hash().unwrap_or_default()
    }
    #[zbus(property)]
    fn require_auth(&self) -> bool {
        self.0.cfg().android.require_auth
    }

    /// Restore the Android boot image from boot_b into boot_a and reboot
    /// (`back-to-android`, which refuses unless boot_b matches the recorded hash).
    async fn switch_to_android(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        let action = if self.0.cfg().android.require_auth { "android-switch-auth" } else { "android-switch" };
        polkit::check(conn, &hdr, action, self.0.no_polkit).await?;
        let hash = f::android_hash().ok_or_else(|| failed("no Android image hash (/etc/android-boot.sha256)".into()))?;
        let tool = f::back_to_android_tool().ok_or_else(|| failed("back-to-android not installed".into()))?;
        let _ = Self::switching_to_android(&em).await;
        std::process::Command::new(tool)
            .arg(hash)
            .spawn()
            .map_err(|e| failed(format!("back-to-android: {e}")))?;
        Ok(())
    }

    async fn set_require_auth(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "admin", self.0.no_polkit).await?;
        self.0.update(|c| c.android.require_auth = on);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    #[zbus(signal)]
    async fn switching_to_android(em: &SignalEmitter<'_>) -> zbus::Result<()>;
}

// ------------------------------------------------------------------ Torch

pub struct Torch(pub Arc<Shared>);

impl Snapshot for Torch {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Torch";
    const PROPS: &'static [&'static str] = &["On", "Level", "MaxLevel"];
    fn snapshot(&self) -> String {
        format!("{:?} {}", f::torch_state().ok(), self.0.cfg().torch.level)
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Torch")]
impl Torch {
    #[zbus(property)]
    fn on(&self) -> bool {
        f::torch_state().map(|s| s.0).unwrap_or(false)
    }
    #[zbus(property)]
    fn level(&self) -> u32 {
        self.0.cfg().torch.level
    }
    #[zbus(property)]
    fn max_level(&self) -> u32 {
        f::torch_state().map(|s| s.1).unwrap_or(0)
    }

    async fn set(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "torch", self.0.no_polkit).await?;
        f::torch_set(if on { self.0.cfg().torch.level } else { 0 }).map_err(failed)?;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    async fn set_level(&self, level: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "torch", self.0.no_polkit).await?;
        let (on, max) = f::torch_state().map_err(failed)?;
        if level == 0 || level > max {
            return Err(invalid(format!("level must be 1..{max}")));
        }
        self.0.update(|c| c.torch.level = level);
        if on {
            f::torch_set(level).map_err(failed)?;
        }
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

// ------------------------------------------------------------------ LedRing

pub struct LedRing(pub Arc<Shared>);

impl Snapshot for LedRing {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.LedRing";
    const PROPS: &'static [&'static str] = &["Mode", "Brightness", "LowPercent"];
    fn snapshot(&self) -> String {
        format!("{:?}", self.0.cfg().ledring)
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.LedRing")]
impl LedRing {
    #[zbus(property)]
    fn mode(&self) -> String {
        self.0.cfg().ledring.mode
    }
    #[zbus(property)]
    fn brightness(&self) -> u32 {
        self.0.cfg().ledring.brightness
    }
    #[zbus(property)]
    fn low_percent(&self) -> u32 {
        self.0.cfg().ledring.low_percent
    }

    async fn set_mode(&self, mode: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        if mode != "charge" && mode != "off" {
            return Err(invalid("mode must be charge or off".into()));
        }
        self.0.update(|c| c.ledring.mode = mode.clone());
        self.0.ledring_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_brightness(&self, brightness: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        if brightness > 255 {
            return Err(invalid("brightness must be 0..255".into()));
        }
        self.0.update(|c| c.ledring.brightness = brightness);
        self.0.ledring_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_low_percent(&self, percent: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        if percent > 50 {
            return Err(invalid("low percent must be 0..50".into()));
        }
        self.0.update(|c| c.ledring.low_percent = percent);
        self.0.ledring_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

// ------------------------------------------------------------------ Refresh

pub struct Refresh(pub Arc<Shared>);

impl Snapshot for Refresh {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Refresh";
    const PROPS: &'static [&'static str] = &["Policy", "Rate", "IdleMs60", "IdleMs30", "MinHz", "InputWakes", "LiveRate"];
    fn snapshot(&self) -> String {
        format!("{:?}", ["policy", "hz", "ms60", "ms30", "min_hz", "input"].map(f::refresh_get))
            + &format!(" {:?}", f::refresh_state_field("hz"))
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Refresh")]
impl Refresh {
    #[zbus(property)]
    fn policy(&self) -> String {
        f::policy_name(f::refresh_get("policy").unwrap_or(2)).to_string()
    }
    #[zbus(property)]
    fn rate(&self) -> u32 {
        f::refresh_get("hz").unwrap_or(120) as u32
    }
    #[zbus(property)]
    fn idle_ms60(&self) -> u32 {
        f::refresh_get("ms60").unwrap_or(0) as u32
    }
    #[zbus(property)]
    fn idle_ms30(&self) -> u32 {
        f::refresh_get("ms30").unwrap_or(0) as u32
    }
    #[zbus(property)]
    fn min_hz(&self) -> u32 {
        f::refresh_get("min_hz").unwrap_or(30) as u32
    }
    #[zbus(property)]
    fn input_wakes(&self) -> bool {
        f::refresh_get("input").unwrap_or(1) != 0
    }
    #[zbus(property)]
    fn live_rate(&self) -> u32 {
        f::refresh_state_field("hz").unwrap_or(0) as u32
    }

    async fn set_policy(&self, policy: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "refresh", self.0.no_polkit).await?;
        let v = f::policy_value(&policy).map_err(invalid)?;
        f::refresh_set("policy", v).map_err(failed)?;
        self.0.update(|c| c.refresh.policy = Some(policy.clone()));
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// The rate used by the manual policy (Hz).
    async fn set_rate(&self, hz: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "refresh", self.0.no_polkit).await?;
        let min = f::refresh_get("min_hz").unwrap_or(30) as u32;
        if hz < min || hz > 120 {
            return Err(invalid(format!("rate must be {min}..120 Hz")));
        }
        f::refresh_set("hz", hz).map_err(failed)?;
        self.0.update(|c| c.refresh.hz = Some(hz));
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Idle time before 60 Hz and before 30 Hz (ms) for the auto policy.
    async fn set_idle(&self, ms60: u32, ms30: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "refresh", self.0.no_polkit).await?;
        apply_idle(&self.0, ms60, ms30)?;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// `power-saver` (0.5 s / 2 s), `balanced` (1 s / 5 s), `smooth` (3 s / 15 s).
    async fn apply_preset(&self, name: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "refresh", self.0.no_polkit).await?;
        let (a, b) = f::refresh_preset(&name).map_err(invalid)?;
        apply_idle(&self.0, a, b)?;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

fn apply_idle(s: &Shared, ms60: u32, ms30: u32) -> fdo::Result<()> {
    if ms60 < 100 || ms30 < ms60 || ms30 > 600_000 {
        return Err(invalid("need 100 <= ms60 <= ms30 <= 600000".into()));
    }
    f::refresh_set("ms60", ms60).map_err(failed)?;
    f::refresh_set("ms30", ms30).map_err(failed)?;
    s.update(|c| {
        c.refresh.ms60 = Some(ms60);
        c.refresh.ms30 = Some(ms30);
    });
    Ok(())
}

// ------------------------------------------------------------------ Gpu

pub struct Gpu(pub Arc<Shared>);

impl Snapshot for Gpu {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Gpu";
    const PROPS: &'static [&'static str] = &["Profile", "FollowPowerProfiles", "Floors"];
    fn snapshot(&self) -> String {
        format!("{:?} {}", self.0.cfg().gpu, self.0.gpu_profile())
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Gpu")]
impl Gpu {
    /// The profile whose limits are in effect.
    #[zbus(property)]
    fn profile(&self) -> String {
        self.0.gpu_profile()
    }
    #[zbus(property)]
    fn follow_power_profiles(&self) -> bool {
        self.0.cfg().gpu.follow_power_profiles
    }
    #[zbus(property)]
    fn floors(&self) -> HashMap<String, (u32, u32)> {
        self.0.cfg().gpu.floors.into_iter().map(|(k, v)| (k, (v[0], v[1]))).collect()
    }

    /// Use this profile's limits and stop following power-profiles-daemon.
    async fn set_profile(&self, profile: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        if !self.0.cfg().gpu.floors.contains_key(&profile) {
            return Err(invalid("unknown profile".into()));
        }
        self.0.update(|c| {
            c.gpu.profile = profile.clone();
            c.gpu.follow_power_profiles = false;
        });
        self.0.gpu_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_follow_power_profiles(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        self.0.update(|c| c.gpu.follow_power_profiles = on);
        self.0.gpu_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_limits(&self, profile: String, min_mhz: u32, max_mhz: u32, #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &zbus::Connection, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        if !["power-saver", "balanced", "performance"].contains(&profile.as_str()) {
            return Err(invalid("profile must be power-saver, balanced or performance".into()));
        }
        f::gpu_limits_valid(min_mhz, max_mhz).map_err(invalid)?;
        self.0.update(|c| {
            c.gpu.floors.insert(profile.clone(), [min_mhz, max_mhz]);
        });
        self.0.gpu_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

// ------------------------------------------------------------------ Usb

pub struct Usb(pub Arc<Shared>);

impl Snapshot for Usb {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Usb";
    const PROPS: &'static [&'static str] = &["WakeEnabled", "DevMode"];
    fn snapshot(&self) -> String {
        format!("{:?} {:?}", f::usb_wake(), f::dev_mode())
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Usb")]
impl Usb {
    #[zbus(property)]
    fn wake_enabled(&self) -> bool {
        f::usb_wake().unwrap_or(false)
    }
    #[zbus(property)]
    fn dev_mode(&self) -> bool {
        f::dev_mode().unwrap_or(false)
    }
    async fn set_wake(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "usb-wake", self.0.no_polkit).await?;
        f::set_usb_wake(on).map_err(failed)?;
        self.0.update(|c| c.usb.wake = on);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// USB gadget network + serial console for developers (binds/unbinds the UDC).
    async fn set_dev_mode(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "dev-mode", self.0.no_polkit).await?;
        f::set_dev_mode(on).map_err(failed)?;
        self.0.update(|c| c.usb.dev_mode = Some(on));
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

// ------------------------------------------------------------------ EmergencyKey

pub struct EmergencyKey(pub Arc<Shared>);

impl Snapshot for EmergencyKey {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.EmergencyKey";
    const PROPS: &'static [&'static str] = &["Enabled", "HoldSeconds"];
    fn snapshot(&self) -> String {
        format!("{:?}", f::emergency_get())
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.EmergencyKey")]
impl EmergencyKey {
    #[zbus(property)]
    fn enabled(&self) -> bool {
        f::emergency_get().0
    }
    #[zbus(property)]
    fn hold_seconds(&self) -> u32 {
        f::emergency_get().1
    }
    async fn set_enabled(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        let action = if on { "emergency-key" } else { "emergency-key-disable" };
        polkit::check(conn, &hdr, action, self.0.no_polkit).await?;
        f::emergency_set(on, f::emergency_get().1).map_err(failed)?;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_hold_seconds(&self, seconds: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "emergency-key", self.0.no_polkit).await?;
        f::emergency_set(f::emergency_get().0, seconds).map_err(invalid)?;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

// ------------------------------------------------------------------ Diagnostics

pub struct Diagnostics(pub Arc<Shared>);

impl Snapshot for Diagnostics {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Diagnostics";
    const PROPS: &'static [&'static str] = &["CrashRecords", "LastBootClean"];
    fn snapshot(&self) -> String {
        format!("{} {}", f::pstore_files().len(), f::last_boot_clean())
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Diagnostics")]
impl Diagnostics {
    #[zbus(property)]
    fn crash_records(&self) -> u32 {
        f::pstore_files().len() as u32
    }
    #[zbus(property)]
    fn last_boot_clean(&self) -> bool {
        f::last_boot_clean()
    }
    /// Write a sanitized diagnostics tarball and return its path.
    async fn export(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<String> {
        polkit::check(conn, &hdr, "diagnostics", self.0.no_polkit).await?;
        crate::diag::export(env!("CARGO_PKG_VERSION")).map(|p| p.display().to_string()).map_err(failed)
    }
}

// ------------------------------------------------------------------ root

pub struct Helper(pub Arc<Shared>);

#[interface(name = "io.github.joonhoekim.tb323fu.Helper")]
impl Helper {
    #[zbus(property)]
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
    #[zbus(property)]
    fn features(&self) -> Vec<String> {
        self.0.features.clone()
    }
    #[zbus(property)]
    fn kernel(&self) -> String {
        f::kernel_version()
    }
    #[zbus(property)]
    fn series_tag(&self) -> String {
        f::series_tag()
    }
    /// Firmware files of the manifest -> ok / mismatch / missing (hashed once, cached).
    #[zbus(property)]
    fn firmware(&self) -> HashMap<String, String> {
        let mut g = self.0.firmware.lock().unwrap();
        if g.is_none() {
            *g = Some(f::firmware_check());
        }
        g.clone().unwrap_or_default().into_iter().collect()
    }
    /// Re-read the configuration file and apply it.
    async fn reload(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "admin", self.0.no_polkit).await?;
        self.0.reload();
        *self.0.firmware.lock().unwrap() = None;
        self.0.gpu_tick(conn, true).await;
        self.0.ledring_tick(true);
        Ok(())
    }
}

// ------------------------------------------------------------------ Boot

/// Cached multiboot state: listing roots mounts other partitions, and reading
/// the selection mounts the UFS root when running elsewhere -- so the poller
/// only compares the cache and the partition list; `Rescan()` and the setters
/// refresh it.
#[derive(Default)]
pub struct BootCache {
    roots: Option<Vec<boot::Root>>,
    next: Option<String>,
    default: Option<String>,
    current: Option<String>,
}

pub struct Boot(pub Arc<Shared>, pub Mutex<BootCache>);

impl Boot {
    pub fn new(sh: Arc<Shared>) -> Self {
        Boot(sh, Mutex::new(BootCache::default()))
    }
    fn cache(&self) -> std::sync::MutexGuard<'_, BootCache> {
        self.1.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn refresh_selection(&self) {
        let mut c = self.cache();
        c.next = Some(boot::next());
        c.default = Some(boot::default_root());
    }
}

impl Snapshot for Boot {
    const IFACE: &'static str = "io.github.joonhoekim.tb323fu.Helper.Boot";
    const PROPS: &'static [&'static str] = &["Roots", "Default", "Next", "Current"];
    fn snapshot(&self) -> String {
        let c = self.cache();
        format!("{:?} {:?} {:?}", boot::partitions(), c.next, c.default)
    }
}

#[interface(name = "io.github.joonhoekim.tb323fu.Helper.Boot")]
impl Boot {
    /// (partition name, os-release PRETTY_NAME, present, init kind: systemd / nixos / none)
    #[zbus(property)]
    fn roots(&self) -> Vec<(String, String, bool, String)> {
        let mut c = self.cache();
        let names: Vec<String> = boot::partitions().into_iter().map(|p| p.0).collect();
        let stale = c.roots.as_ref().is_none_or(|r| r.iter().map(|x| &x.name).ne(names.iter()));
        if stale {
            c.roots = Some(boot::roots());
        }
        c.roots.clone().unwrap_or_default().into_iter().map(|r| (r.name, r.label, r.present, r.init)).collect()
    }
    #[zbus(property)]
    fn default(&self) -> String {
        let need = self.cache().default.is_none();
        if need { self.refresh_selection(); }
        self.cache().default.clone().unwrap_or_else(|| boot::DEFAULT_ROOT.into())
    }
    #[zbus(property)]
    fn next(&self) -> String {
        let need = self.cache().next.is_none();
        if need { self.refresh_selection(); }
        self.cache().next.clone().unwrap_or_default()
    }
    #[zbus(property)]
    fn current(&self) -> String {
        let mut c = self.cache();
        c.current.get_or_insert_with(boot::current_root).clone()
    }

    /// Boot this root once on the next boot (the initramfs forgets it after reading).
    async fn set_next(&self, name: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "boot-next", self.0.no_polkit).await?;
        boot::set_next(&name).map_err(invalid)?;
        self.refresh_selection();
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn clear_next(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "boot-next", self.0.no_polkit).await?;
        boot::clear_next().map_err(failed)?;
        self.refresh_selection();
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Persistent default root (baldur-root removes the override).
    async fn set_default(&self, name: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "boot-default", self.0.no_polkit).await?;
        boot::set_default(&name).map_err(invalid)?;
        self.refresh_selection();
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Set the one-shot root and reboot now.
    async fn reboot_into(&self, name: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "reboot-into", self.0.no_polkit).await?;
        boot::set_next(&name).map_err(invalid)?;
        std::process::Command::new("systemctl").arg("reboot").spawn().map_err(|e| failed(format!("systemctl reboot: {e}")))?;
        Ok(())
    }
    /// Re-read the partitions, their os-release and the selection files.
    async fn rescan(&self, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        {
            let mut c = self.cache();
            c.roots = Some(boot::roots());
            c.current = Some(boot::current_root());
        }
        self.refresh_selection();
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}
