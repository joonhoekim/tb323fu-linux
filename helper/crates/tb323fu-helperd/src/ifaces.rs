// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The D-Bus objects of `io.github.joonhoekim.OpenDeviceHelper1` (see
//! docs/helper-reference.md "D-Bus API"). Property getters read the device live;
//! setters check polkit, write the device, persist the setting and emit
//! PropertiesChanged (with the changed properties invalidated).

use crate::polkit;
use crate::{thermal_for, Shared};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tb323fu_helper_core::boot;
use tb323fu_helper_core::features as f;
use tb323fu_helper_core::haptics;
use tb323fu_helper_core::perf;
use std::sync::Mutex;
use zbus::fdo;
use zbus::interface;
use zbus::message::Header;
use zbus::names::InterfaceName;
use zbus::object_server::SignalEmitter;

pub const ROOT: &str = "/io/github/joonhoekim/OpenDeviceHelper1";
pub const P_BATTERY: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Battery";
pub const P_ANDROID: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Android";
pub const P_TORCH: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Torch";
pub const P_LEDRING: &str = "/io/github/joonhoekim/OpenDeviceHelper1/LedRing";
pub const P_REFRESH: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Refresh";
pub const P_GPU: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Gpu";
pub const P_USB: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Usb";
pub const P_EMERGENCY: &str = "/io/github/joonhoekim/OpenDeviceHelper1/EmergencyKey";
pub const P_DIAG: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Diagnostics";
pub const P_BOOT: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Boot";
pub const P_THERMAL: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Thermal";
pub const P_HAPTICS: &str = "/io/github/joonhoekim/OpenDeviceHelper1/Haptics";

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
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Battery";
    const PROPS: &'static [&'static str] = &["ChargeLimit", "Bypass", "Status", "State", "Capacity", "CurrentMa", "VoltageMv",
        "TemperatureC", "Health", "CycleCount", "DesignCapacityMah", "ChargerType", "ChargerContract", "ChargerAdapter", "InputVoltageMv", "InputCurrentMa",
        "RechargeGap", "FullBy", "FullByDays", "FullByActive", "StateOfHealth", "OcvMv"];
    fn snapshot(&self) -> String {
        // current/voltage/temperature move all the time: only state-like values
        // trigger a signal (clients poll the live values while showing them)
        let i = f::battery_info().ok();
        let bypass = self.0.cfg().battery.bypass;
        let c = self.0.cfg().battery;
        format!("{:?} {bypass} {:?} {} {} {} {:?} {}", i.as_ref().map(|i| (i.charge_limit, i.capacity, &i.status, &i.health, i.state_of_health)),
            i.as_ref().map(|i| f::battery_state(i, bypass)), f::charger_info().stable_key(), c.recharge_gap, c.full_by, c.full_by_days,
            self.0.full_by_active())
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Battery")]
impl Battery {
    #[zbus(property)]
    fn charge_limit(&self) -> u32 {
        if self.0.full_by_active() {
            return self.0.cfg().battery.charge_limit;
        }
        f::battery_info().map(|i| i.charge_limit).unwrap_or(100)
    }
    /// charging resumes this many percent below the limit
    #[zbus(property)]
    fn recharge_gap(&self) -> u32 {
        self.0.cfg().battery.recharge_gap
    }
    /// "HH:MM" local time to be full by, "" off
    #[zbus(property)]
    fn full_by(&self) -> String {
        self.0.cfg().battery.full_by
    }
    /// days of the week for FullBy ("mon".."sun"); empty = every day
    #[zbus(property)]
    fn full_by_days(&self) -> Vec<String> {
        self.0.cfg().battery.full_by_days
    }
    /// charging to 100 % for FullBy now
    #[zbus(property)]
    fn full_by_active(&self) -> bool {
        self.0.full_by_active()
    }
    /// % of the design capacity, -1 unknown
    #[zbus(property)]
    fn state_of_health(&self) -> i32 {
        f::battery_info().map(|i| i.state_of_health).unwrap_or(-1)
    }
    /// open-circuit voltage estimate, mV (0 unknown)
    #[zbus(property)]
    fn ocv_mv(&self) -> u32 {
        f::battery_info().map(|i| i.ocv_mv).unwrap_or(0)
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
        let bypass = self.0.cfg().battery.bypass;
        f::battery_info().map(|i| f::battery_state(&i, bypass).to_string()).unwrap_or_else(|_| "unknown".into())
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
    /// battmgr adapter type: SDP, DCP, CDP, PD, PD_PPS, ... ("" unknown)
    #[zbus(property)]
    fn charger_adapter(&self) -> String {
        f::charger_info().adapter
    }
    /// measured charger input (0 unplugged / unknown)
    #[zbus(property)]
    fn input_voltage_mv(&self) -> u32 {
        f::charger_info().input_mv
    }
    #[zbus(property)]
    fn input_current_ma(&self) -> u32 {
        f::charger_info().input_ma
    }

    async fn set_charge_limit(&self, percent: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "charge-limit", self.0.no_polkit).await?;
        self.0.full_by_end(false);
        f::set_charge_limit(percent, self.0.cfg().battery.recharge_gap).map_err(invalid)?;
        self.0.update(|c| {
            c.battery.charge_limit = percent;
            c.battery.bypass = false;
            c.battery.saved_limit = None;
            c.thermal.bypass_auto = false;
        });
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    /// Bypass: hold the battery where it is (limit = current capacity) while
    /// external power feeds the system; off restores the previous limit.
    async fn set_bypass(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "charge-limit", self.0.no_polkit).await?;
        let auto = self.0.cfg().thermal.bypass_auto;
        self.0.set_bypass(on).map_err(failed)?;
        if auto {
            // the user decides now; off also stops performance from switching it back on
            self.0.update(|c| c.thermal.bypass_auto = false);
            self.0.rt.lock().unwrap().bypass_declined = !on;
        }
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    /// Charging resumes `percent` below the limit (3..20).
    async fn set_recharge_gap(&self, percent: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "charge-limit", self.0.no_polkit).await?;
        if !f::RECHARGE_GAP_RANGE.contains(&percent) {
            return Err(invalid("recharge gap must be 3..20 %".into()));
        }
        let c = self.0.cfg().battery;
        let end = f::battery_info().map(|i| i.charge_limit).unwrap_or(c.charge_limit);
        f::set_charge_limit(end, percent).map_err(failed)?;
        self.0.update(|c| c.battery.recharge_gap = percent);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Be at 100 % by `time` ("HH:MM", local; "" switches it off) on `days`
    /// ("mon".."sun"; none = every day).
    async fn set_full_by(&self, time: String, days: Vec<String>, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "charge-limit", self.0.no_polkit).await?;
        if !time.is_empty() {
            tb323fu_helper_core::schedule::parse_hhmm(&time).map_err(invalid)?;
        }
        tb323fu_helper_core::schedule::valid_days(&days).map_err(invalid)?;
        let mut days = days;
        days.sort_by_key(|d| tb323fu_helper_core::schedule::DAYS.iter().position(|x| x == d));
        days.dedup();
        self.0.full_by_end(true);
        self.0.update(|c| {
            c.battery.full_by = time.clone();
            c.battery.full_by_days = days.clone();
        });
        self.0.full_by_tick();
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    #[zbus(signal)]
    pub async fn changed(em: &SignalEmitter<'_>) -> zbus::Result<()>;
}

// ------------------------------------------------------------------ Android

pub struct Android(pub Arc<Shared>);

impl Snapshot for Android {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Android";
    const PROPS: &'static [&'static str] = &["Available", "ImageSha256", "RequireAuth"];
    fn snapshot(&self) -> String {
        format!("{:?} {:?} {}", f::android_hash(), f::back_to_android_tool(), self.0.cfg().android.require_auth)
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Android")]
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
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Torch";
    const PROPS: &'static [&'static str] = &["On", "Level", "MaxLevel"];
    fn snapshot(&self) -> String {
        format!("{:?} {}", f::torch_state().ok(), self.0.cfg().torch.level)
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Torch")]
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
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.LedRing";
    const PROPS: &'static [&'static str] = &["Mode", "Brightness", "LowPercent", "Color", "Speed", "ChargeOverride", "NotifyPulse"];
    fn snapshot(&self) -> String {
        format!("{:?}", self.0.cfg().ledring)
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.LedRing")]
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
    /// "#rrggbb" of solid and breathe
    #[zbus(property)]
    fn color(&self) -> String {
        self.0.cfg().ledring.color
    }
    /// breathing cycle, ms
    #[zbus(property)]
    fn speed(&self) -> u32 {
        self.0.cfg().ledring.speed
    }
    #[zbus(property)]
    fn charge_override(&self) -> bool {
        self.0.cfg().ledring.charge_override
    }
    /// front-ends pulse the ring for desktop notifications
    #[zbus(property)]
    fn notify_pulse(&self) -> bool {
        self.0.cfg().ledring.notify
    }

    async fn set_color(&self, color: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        let c = f::parse_color(&color).map_err(invalid)?;
        self.0.update(|cf| cf.ledring.color = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
        self.0.ledring_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Breathing cycle, 1000..20000 ms.
    async fn set_speed(&self, ms: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        if !(1000..=20000).contains(&ms) {
            return Err(invalid("speed must be 1000..20000 ms".into()));
        }
        self.0.update(|c| c.ledring.speed = ms);
        self.0.ledring_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_charge_override(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        self.0.update(|c| c.ledring.charge_override = on);
        self.0.ledring_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_notify_pulse(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        self.0.update(|c| c.ledring.notify = on);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Blink `count` times (1..10) in `color` ("#rrggbb", "" = the ring's
    /// color), then back to the mode: for notifications and scripts.
    async fn pulse(&self, color: String, count: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        if !(1..=10).contains(&count) {
            return Err(invalid("count must be 1..10".into()));
        }
        let c = if color.is_empty() { self.0.cfg().ledring.color } else { color };
        self.0.led.pulse(f::parse_color(&c).map_err(invalid)?, count);
        Ok(())
    }

    async fn set_mode(&self, mode: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "led-ring", self.0.no_polkit).await?;
        if !f::LED_MODES.contains(&mode.as_str()) {
            return Err(invalid("mode must be off, charge, solid or breathe".into()));
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
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Refresh";
    const PROPS: &'static [&'static str] = &["Policy", "Rate", "IdleMs60", "IdleMs30", "MinHz", "InputWakes", "LiveRate"];
    fn snapshot(&self) -> String {
        format!("{:?}", ["policy", "hz", "ms60", "ms30", "min_hz", "input"].map(f::refresh_get))
            + &format!(" {:?}", f::refresh_state_field("hz"))
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Refresh")]
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

/// The performance profile (named Gpu for compatibility): GPU limits, CPU
/// limits and boost, and what follows the profile (the thermal profile when
/// linked, Wi-Fi power saving).
pub struct Gpu(pub Arc<Shared>);

const PERF_PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];

impl Snapshot for Gpu {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Gpu";
    const PROPS: &'static [&'static str] = &["Profile", "FollowPowerProfiles", "Floors", "CpuBoost", "CpuLimits", "CpuRange",
        "WifiLowLatency", "WifiAvailable"];
    fn snapshot(&self) -> String {
        let c = self.0.cfg();
        format!("{:?} {:?} {:?} {} {:?}", c.gpu, c.cpu, c.wifi, self.0.gpu_profile(), f::cpu_boost())
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Gpu")]
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
    /// profile -> (little min, little max, big min, big max), MHz
    #[zbus(property)]
    fn cpu_limits(&self) -> HashMap<String, (u32, u32, u32, u32)> {
        PERF_PROFILES
            .iter()
            .filter_map(|p| self.0.cpu_limits_of(p).map(|l| (p.to_string(), (l[0], l[1], l[2], l[3]))))
            .collect()
    }
    /// cluster -> hardware (min, max) MHz, boost frequencies not counted
    #[zbus(property)]
    fn cpu_range(&self) -> HashMap<String, (u32, u32)> {
        perf::CLUSTERS.iter().filter_map(|(c, _)| perf::cluster_range(c).map(|r| (c.to_string(), r))).collect()
    }
    #[zbus(property)]
    fn wifi_low_latency(&self) -> bool {
        self.0.cfg().wifi.low_latency_performance
    }
    #[zbus(property)]
    fn wifi_available(&self) -> bool {
        self.0.wifi_available()
    }

    /// cpufreq boost (the fast cores' top frequencies); false also when the
    /// kernel offers none
    #[zbus(property)]
    fn cpu_boost(&self) -> bool {
        f::cpu_boost().unwrap_or(false)
    }
    async fn set_cpu_boost(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        if f::cpu_boost().is_none() {
            return Err(fdo::Error::NotSupported("this kernel has no CPU boost frequencies".into()));
        }
        f::set_cpu_boost(on).map_err(failed)?;
        self.0.update(|c| c.cpu.boost = on);
        // changing boost resets scaling_max_freq
        self.0.cpu_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    /// Use this profile's limits and stop following power-profiles-daemon.
    async fn set_profile(&self, profile: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        if !self.0.cfg().gpu.floors.contains_key(&profile) {
            return Err(invalid("unknown profile".into()));
        }
        if self.0.cfg().thermal.follow_performance && thermal_for(&profile) == "performance" && perf::thermal_profile_available() {
            polkit::check(conn, &hdr, "thermal-performance", self.0.no_polkit).await?;
        }
        self.0.update(|c| {
            c.gpu.profile = profile.clone();
            c.gpu.follow_power_profiles = false;
        });
        self.0.profile_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        invalidate_thermal(conn).await;
        Ok(())
    }
    async fn set_follow_power_profiles(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        self.0.update(|c| c.gpu.follow_power_profiles = on);
        self.0.profile_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        invalidate_thermal(conn).await;
        Ok(())
    }
    async fn set_limits(&self, profile: String, min_mhz: u32, max_mhz: u32, #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &zbus::Connection, #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        if !PERF_PROFILES.contains(&profile.as_str()) {
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
    /// CPU limits of a profile, MHz (rounded to the nearest available
    /// frequency; a maximum at the top of the range leaves boost reachable).
    #[allow(clippy::too_many_arguments)]
    async fn set_cpu_limits(&self, profile: String, little_min: u32, little_max: u32, big_min: u32, big_max: u32,
        #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        if !PERF_PROFILES.contains(&profile.as_str()) {
            return Err(invalid("profile must be power-saver, balanced or performance".into()));
        }
        if !perf::cpu_limits_available() {
            return Err(fdo::Error::NotSupported("no cpufreq policies".into()));
        }
        let l = [little_min, little_max, big_min, big_max];
        perf::cpu_limits_valid(l).map_err(invalid)?;
        self.0.update(|c| {
            if perf::cpu_full_range() == Some(l) {
                c.cpu.limits.remove(&profile);
            } else {
                c.cpu.limits.insert(profile.clone(), l);
            }
        });
        self.0.cpu_tick(true);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Wi-Fi power saving off while the performance profile is in effect.
    async fn set_wifi_low_latency(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "gpu", self.0.no_polkit).await?;
        if on && !self.0.wifi_available() {
            return Err(fdo::Error::NotSupported("no Wi-Fi interface, or iw is not installed".into()));
        }
        self.0.update(|c| c.wifi.low_latency_performance = on);
        self.0.wifi_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

async fn invalidate_thermal(conn: &zbus::Connection) {
    if let Ok(em) = SignalEmitter::new(conn, P_THERMAL) {
        invalidate(&em, Thermal::IFACE, Thermal::PROPS).await;
    }
}

// ------------------------------------------------------------------ Usb

pub struct Usb(pub Arc<Shared>);

impl Snapshot for Usb {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Usb";
    const PROPS: &'static [&'static str] = &["WakeEnabled", "DevMode", "ChargerWake", "Ports"];
    fn snapshot(&self) -> String {
        format!("{:?} {:?} {:?} {:?}", f::usb_wake(), f::dev_mode(), f::charger_wake(), f::typec_ports())
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Usb")]
impl Usb {
    #[zbus(property)]
    fn wake_enabled(&self) -> bool {
        f::usb_wake().unwrap_or(false)
    }
    #[zbus(property)]
    fn dev_mode(&self) -> bool {
        f::dev_mode().unwrap_or(false)
    }
    /// plugging or unplugging a charger wakes the tablet
    #[zbus(property)]
    fn charger_wake(&self) -> bool {
        f::charger_wake().unwrap_or(false)
    }
    /// USB-C ports: (port, data role, power role, partner attached)
    #[zbus(property)]
    fn ports(&self) -> Vec<(String, String, String, bool)> {
        f::typec_ports()
    }
    async fn set_charger_wake(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "usb-wake", self.0.no_polkit).await?;
        f::set_charger_wake(on).map_err(failed)?;
        self.0.update(|c| c.usb.charger_wake = Some(on));
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
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
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.EmergencyKey";
    const PROPS: &'static [&'static str] = &["Enabled", "HoldSeconds"];
    fn snapshot(&self) -> String {
        format!("{:?}", f::emergency_get())
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.EmergencyKey")]
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
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Diagnostics";
    const PROPS: &'static [&'static str] = &["CrashRecords", "LastBootClean"];
    fn snapshot(&self) -> String {
        format!("{} {}", f::pstore_files().len(), f::last_boot_clean())
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Diagnostics")]
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
        crate::diag::export(crate::selfupdate::version()).map(|p| p.display().to_string()).map_err(failed)
    }
}

// ------------------------------------------------------------------ root

pub struct Helper(pub Arc<Shared>);

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1")]
impl Helper {
    #[zbus(property)]
    fn version(&self) -> String {
        crate::selfupdate::version().to_string()
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
        self.0.profile_tick(conn, true).await;
        self.0.ledring_tick(true);
        Ok(())
    }
}

// ------------------------------------------------------------------ Boot

/// Cached multiboot state: listing roots mounts other partitions, and reading
/// the selection mounts the state root when running elsewhere -- so the poller
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
    /// The roots, rescanned when the partition list changed. Scanning mounts
    /// SD roots, so it runs on a blocking thread: the daemon's single executor
    /// keeps answering the other objects meanwhile.
    async fn cached_roots(&self) -> Vec<boot::Root> {
        let names: Vec<String> = boot::partitions().into_iter().map(|p| p.0).collect();
        let cached = self.cache().roots.clone();
        match cached {
            Some(r) if r.iter().map(|x| &x.name).eq(names.iter()) => r,
            _ => {
                let r = blocking::unblock(boot::roots).await;
                self.cache().roots = Some(r.clone());
                r
            }
        }
    }
    fn refresh_selection(&self) {
        let mut c = self.cache();
        c.next = Some(boot::next());
        c.default = Some(boot::default_root());
    }
}

impl Snapshot for Boot {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Boot";
    const PROPS: &'static [&'static str] = &["Roots", "Default", "Next", "Current", "RootHealth"];
    fn snapshot(&self) -> String {
        let c = self.cache();
        format!("{:?} {:?} {:?}", boot::partitions(), c.next, c.default)
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Boot")]
impl Boot {
    /// (partition name, os-release PRETTY_NAME, present, init kind: systemd / nixos / none)
    #[zbus(property)]
    async fn roots(&self) -> Vec<(String, String, bool, String)> {
        self.cached_roots().await.into_iter().map(|r| (r.name, r.label, r.present, r.init)).collect()
    }
    /// Root -> what it lacks for the running kernel (modules, extra/ amplifier
    /// driver, key firmware); roots without problems are left out. A separate
    /// property so the Roots signature stays the same.
    #[zbus(property)]
    async fn root_health(&self) -> HashMap<String, Vec<String>> {
        self.cached_roots().await.into_iter().filter(|r| !r.problems.is_empty()).map(|r| (r.name, r.problems)).collect()
    }
    #[zbus(property)]
    fn default(&self) -> String {
        let need = self.cache().default.is_none();
        if need { self.refresh_selection(); }
        self.cache().default.clone().unwrap_or_else(boot::state_root)
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
    /// Persistent default root (the state root removes the override).
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
        // off the executor (mounts), like cached_roots()
        let (roots, current, next, default) =
            blocking::unblock(|| (boot::roots(), boot::current_root(), boot::next(), boot::default_root())).await;
        {
            let mut c = self.cache();
            c.roots = Some(roots);
            c.current = Some(current);
            c.next = Some(next);
            c.default = Some(default);
        }
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
}

// ------------------------------------------------------------------ Thermal

/// Temperatures (read-only) and the board-temperature profile: only the
/// quiet-thermal passive trips are ever written (core `perf`), never a chip
/// zone, `hot`/`critical` trip, policy or mode.
pub struct Thermal(pub Arc<Shared>);

fn or_nan(t: &Option<f::Thermal>, g: impl Fn(&f::Thermal) -> f64) -> f64 {
    t.as_ref().map(g).unwrap_or(f64::NAN)
}

impl Snapshot for Thermal {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Thermal";
    const PROPS: &'static [&'static str] = &["Surface", "CpuMax", "GpuMax", "Throttling", "Zones", "Profile", "Profiles", "TripOffset",
        "Trips", "FollowPerformance", "PerformanceBypass", "PanelLimit", "PanelLimited"];
    fn snapshot(&self) -> String {
        // whole degrees: a signal per degree of change, not per sample
        let t = f::thermal();
        let r = |v: f64| if v.is_nan() { i64::MIN } else { v.round() as i64 };
        format!("{:?} {:?} {:?} {}", t.map(|t| (r(t.surface), r(t.cpu_max), r(t.gpu_max), t.throttling,
            t.zones.values().map(|v| r(*v)).collect::<Vec<_>>())), perf::thermal_trips(), self.0.cfg().thermal, self.0.panel_limited())
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Thermal")]
impl Thermal {
    /// Skin (else quiet) sensor, °C; NaN when absent.
    #[zbus(property)]
    fn surface(&self) -> f64 {
        or_nan(&f::thermal(), |t| t.surface)
    }
    #[zbus(property)]
    fn cpu_max(&self) -> f64 {
        or_nan(&f::thermal(), |t| t.cpu_max)
    }
    #[zbus(property)]
    fn gpu_max(&self) -> f64 {
        or_nan(&f::thermal(), |t| t.gpu_max)
    }
    /// A CPU or GPU cooling device is above state 0.
    #[zbus(property)]
    fn throttling(&self) -> bool {
        f::thermal().is_some_and(|t| t.throttling)
    }
    /// Board sensors by name (zone type without "-thermal"), °C.
    #[zbus(property)]
    fn zones(&self) -> HashMap<String, f64> {
        f::thermal().map(|t| t.zones.into_iter().collect()).unwrap_or_default()
    }
    /// The board-temperature profile the trips match: quiet / default /
    /// performance, "custom" when none, "" when the kernel has no such zone.
    #[zbus(property)]
    fn profile(&self) -> String {
        perf::thermal_profile().unwrap_or_default()
    }
    #[zbus(property)]
    fn profiles(&self) -> Vec<String> {
        if perf::thermal_profile_available() { perf::THERMAL_PROFILES.iter().map(|s| s.to_string()).collect() } else { Vec::new() }
    }
    /// °C added to the device tree's steps by the profile in effect.
    #[zbus(property)]
    fn trip_offset(&self) -> i32 {
        perf::thermal_profile().and_then(|p| perf::profile_offset(&p)).map(|o| (o / 1000) as i32).unwrap_or(0)
    }
    /// The board steps now (°C, lowest first).
    #[zbus(property)]
    fn trips(&self) -> Vec<f64> {
        perf::thermal_trips()
    }
    #[zbus(property)]
    fn follow_performance(&self) -> bool {
        self.0.cfg().thermal.follow_performance
    }
    #[zbus(property)]
    fn performance_bypass(&self) -> bool {
        self.0.cfg().thermal.performance_bypass
    }
    #[zbus(property)]
    fn panel_limit(&self) -> bool {
        self.0.cfg().thermal.panel_limit && perf::panel_limit_available()
    }
    #[zbus(property)]
    fn panel_limited(&self) -> bool {
        self.0.panel_limited()
    }

    /// Choose the profile (and stop following the performance profile).
    async fn set_profile(&self, profile: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        if perf::profile_offset(&profile).is_none() {
            return Err(invalid("profile must be quiet, default or performance".into()));
        }
        let action = if profile == "performance" { "thermal-performance" } else { "thermal" };
        polkit::check(conn, &hdr, action, self.0.no_polkit).await?;
        if !perf::thermal_profile_available() {
            return Err(fdo::Error::NotSupported("no quiet-thermal zone with passive trips".into()));
        }
        if profile == "performance" {
            if let Some(reason) = perf::performance_guard_now() {
                return Err(failed(format!("too hot for the performance profile: {reason}")));
            }
        }
        self.0.update(|c| {
            c.thermal.profile = profile.clone();
            c.thermal.follow_performance = false;
        });
        self.0.thermal_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Follow the performance profile: power-saver -> quiet, balanced ->
    /// default, performance -> performance.
    async fn set_follow_performance(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        let hot = on && thermal_for(&self.0.gpu_profile()) == "performance";
        polkit::check(conn, &hdr, if hot { "thermal-performance" } else { "thermal" }, self.0.no_polkit).await?;
        self.0.update(|c| c.thermal.follow_performance = on);
        self.0.thermal_tick(conn, true).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    async fn set_performance_bypass(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "thermal", self.0.no_polkit).await?;
        let auto = self.0.cfg().thermal.bypass_auto;
        if !on && auto {
            self.0.set_bypass(false).map_err(failed)?;
        }
        self.0.update(|c| {
            c.thermal.performance_bypass = on;
            if !on {
                c.thermal.bypass_auto = false;
            }
        });
        self.0.thermal_tick(conn, false).await;
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// Panel heat protection (backlight held at 178/255 from 55 °C);
    /// switching it off asks for authentication.
    async fn set_panel_limit(&self, on: bool, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, if on { "thermal" } else { "thermal-performance" }, self.0.no_polkit).await?;
        if !perf::panel_limit_available() {
            return Err(fdo::Error::NotSupported("no panel temperature sensor or backlight".into()));
        }
        self.0.update(|c| c.thermal.panel_limit = on);
        self.0.panel_tick();
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }

    /// The performance profile ended by itself (a battery or the board too hot).
    #[zbus(signal)]
    pub async fn profile_fallback(em: &SignalEmitter<'_>, reason: &str) -> zbus::Result<()>;
}

// ------------------------------------------------------------------ Haptics

/// The vibration motors: device gain for every application, and a test buzz.
pub struct Haptics(pub Arc<Shared>);

impl Snapshot for Haptics {
    const IFACE: &'static str = "io.github.joonhoekim.OpenDeviceHelper1.Haptics";
    const PROPS: &'static [&'static str] = &["Strength", "Motors"];
    fn snapshot(&self) -> String {
        format!("{} {:?}", self.0.cfg().haptics.strength, haptics::motors())
    }
}

#[interface(name = "io.github.joonhoekim.OpenDeviceHelper1.Haptics")]
impl Haptics {
    /// 0..100 %
    #[zbus(property)]
    fn strength(&self) -> u32 {
        self.0.cfg().haptics.strength
    }
    /// "left", "right"
    #[zbus(property)]
    fn motors(&self) -> Vec<String> {
        haptics::motors().into_iter().map(|m| m.name).collect()
    }
    async fn set_strength(&self, percent: u32, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "haptics", self.0.no_polkit).await?;
        if percent > 100 {
            return Err(invalid("strength must be 0..100".into()));
        }
        haptics::set_strength(percent).map_err(failed)?;
        self.0.update(|c| c.haptics.strength = percent);
        invalidate(&em, Self::IFACE, Self::PROPS).await;
        Ok(())
    }
    /// A 0.3 s buzz on "left", "right" or "both" at the current strength.
    async fn test(&self, motor: String, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<()> {
        polkit::check(conn, &hdr, "haptics", self.0.no_polkit).await?;
        if !["left", "right", "both"].contains(&motor.as_str()) {
            return Err(invalid("motor must be left, right or both".into()));
        }
        blocking::unblock(move || haptics::test(&motor)).await.map_err(failed)
    }
}
