// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The device features: sysfs locations (as on the TB323FU mainline kernel)
//! and safe read/write helpers. Errors are plain strings for the D-Bus layer.

use crate::sys;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub type Res<T> = Result<T, String>;

fn werr(what: &str, e: std::io::Error) -> String {
    format!("{what}: {e}")
}

// ---------------------------------------------------------------- battery

/// The battery power supply: the first `type == Battery` (qcom-battmgr-bat).
pub fn battery_dir() -> Option<PathBuf> {
    let base = sys::path("/sys/class/power_supply");
    sys::list_dir(&base)
        .into_iter()
        .map(|n| base.join(n))
        .find(|p| sys::read_opt(&p.join("type")).as_deref() == Some("Battery"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatteryInfo {
    pub status: String,
    pub capacity: u32,
    pub current_ma: i32,
    pub voltage_mv: u32,
    pub temperature_c: f64,
    pub health: String,
    pub cycle_count: i32,
    pub design_capacity_mah: i32,
    pub charge_limit: u32,
    pub online: bool,
    /// % of the design capacity (battmgr `state_of_health`), -1 unknown
    pub state_of_health: i32,
    /// open-circuit voltage estimate, mV (0 unknown)
    pub ocv_mv: u32,
    /// charge now, mAh (`charge_counter`, 0 unknown)
    pub charge_counter_mah: u32,
}

pub fn battery_info() -> Res<BatteryInfo> {
    let b = battery_dir().ok_or("no battery power supply")?;
    let num = |f: &str| sys::read_i64(&b.join(f));
    Ok(BatteryInfo {
        status: sys::read_opt(&b.join("status")).unwrap_or_else(|| "Unknown".into()),
        capacity: num("capacity").unwrap_or(0).clamp(0, 100) as u32,
        // µA -> mA; positive while charging (series patch 0107)
        current_ma: (num("current_now").unwrap_or(0) / 1000) as i32,
        voltage_mv: (num("voltage_now").unwrap_or(0) / 1000).max(0) as u32,
        // tenths of a degree
        temperature_c: num("temp").map(|t| t as f64 / 10.0).unwrap_or(f64::NAN),
        health: sys::read_opt(&b.join("health")).unwrap_or_else(|| "Unknown".into()),
        cycle_count: num("cycle_count").map(|v| v as i32).unwrap_or(-1),
        // µAh; the TB323FU firmware answers ENODATA today
        design_capacity_mah: num("charge_full_design").map(|v| (v / 1000) as i32).unwrap_or(-1),
        charge_limit: num("charge_control_end_threshold").unwrap_or(100) as u32,
        online: external_power(),
        state_of_health: num("state_of_health").filter(|v| (0..=200).contains(v)).map(|v| v as i32).unwrap_or(-1),
        ocv_mv: (num("voltage_ocv").unwrap_or(0) / 1000).max(0) as u32,
        charge_counter_mah: (num("charge_counter").unwrap_or(0) / 1000).max(0) as u32,
    })
}

/// Any charger/host present (the battmgr USB supply, or a UCSI source).
pub fn external_power() -> bool {
    let base = sys::path("/sys/class/power_supply");
    sys::list_dir(&base).into_iter().any(|n| {
        n != "qcom-battmgr-bat"
            && sys::read_opt(&base.join(&n).join("type")).as_deref() != Some("Battery")
            && sys::read_opt(&base.join(&n).join("online")).as_deref() == Some("1")
    })
}

/// Battery current (mA, either sign) at or below which the battery counts as
/// idle: held at the limit or in bypass. The firmware never reports exactly 0
/// in bypass (185 mA seen with a 65 W charger, 9 A while charging).
pub const IDLE_MA: i32 = 300;

/// `charging` / `discharging` / `bypass` / `full` / `not-charging`.
/// battmgr keeps reporting "Charging" while the battery is held at the limit
/// and in bypass, so the current decides:
/// - with the helper's Bypass switch on (`bypass`), external power and
///   |current| <= IDLE_MA it is `bypass` whatever the status says; while the
///   current is still larger (the switch was just flipped, or the charger
///   cannot carry the load) status and current win, so it never claims an
///   idle battery that is charging or draining;
/// - without the switch, "Charging" with |current| <= IDLE_MA at or above the
///   limit (or "Not charging" there) is also `bypass` (held at the limit).
pub fn battery_state(i: &BatteryInfo, bypass: bool) -> &'static str {
    let idle = i.current_ma.abs() <= IDLE_MA;
    if bypass && i.online && idle && i.status != "Full" {
        return "bypass";
    }
    match i.status.as_str() {
        "Discharging" => "discharging",
        "Full" => "full",
        "Not charging" => {
            if i.online && i.capacity >= i.charge_limit { "bypass" } else { "not-charging" }
        }
        "Charging" => {
            if idle && i.capacity >= i.charge_limit { "bypass" } else { "charging" }
        }
        _ => "not-charging",
    }
}

pub const CHARGE_LIMIT_MIN: u32 = 20;
/// Recharge gap: charging resumes this many percent below the limit.
pub const RECHARGE_GAP_RANGE: std::ops::RangeInclusive<u32> = 3..=20;

/// Set the end threshold and start = end - gap, keeping start <= end at
/// every step.
pub fn set_charge_limit(pct: u32, gap: u32) -> Res<()> {
    if !RECHARGE_GAP_RANGE.contains(&gap) {
        return Err("recharge gap must be 3..20 %".into());
    }
    if !(CHARGE_LIMIT_MIN..=100).contains(&pct) {
        return Err(format!("charge limit must be {CHARGE_LIMIT_MIN}..100"));
    }
    let b = battery_dir().ok_or("no battery power supply")?;
    let end = b.join("charge_control_end_threshold");
    let start = b.join("charge_control_start_threshold");
    let new_start = pct.saturating_sub(gap);
    let cur_start = sys::read_i64(&start).unwrap_or(0) as u32;
    if sys::exists(&start) && new_start < cur_start {
        sys::write(&start, &new_start.to_string()).map_err(|e| werr("start threshold", e))?;
        sys::write(&end, &pct.to_string()).map_err(|e| werr("end threshold", e))?;
    } else {
        sys::write(&end, &pct.to_string()).map_err(|e| werr("end threshold", e))?;
        if sys::exists(&start) {
            sys::write(&start, &new_start.to_string()).map_err(|e| werr("start threshold", e))?;
        }
    }
    Ok(())
}

/// The active charger as the kernel reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChargerInfo {
    /// selected `usb_type` of the online UCSI source ("C", "PD", ...);
    /// "USB" when only the battmgr USB supply is online, "none" unplugged
    pub kind: String,
    /// adapter type the battery manager detected (battmgr USB `usb_type`:
    /// "SDP", "DCP", "CDP", "PD", "PD_PPS", ...; "" when unknown)
    pub adapter: String,
    /// negotiated contract from UCSI, mV / mA (0 when not reported: the
    /// TB323FU firmware answers 0 for PD/PPS chargers)
    pub contract_mv: u32,
    pub contract_ma: u32,
    /// measured charger input, mV / mA (battmgr USB `voltage_now` /
    /// `current_now`; 0 when unplugged or unknown)
    pub input_mv: u32,
    pub input_ma: u32,
}

impl ChargerInfo {
    /// Short name: "PPS" for a PPS adapter, "PD" for a PD one, else the UCSI type.
    pub fn label(&self) -> String {
        match self.adapter.as_str() {
            "PD_PPS" => "PPS".into(),
            "PD" | "PD_DRP" => "PD".into(),
            _ => self.kind.clone(),
        }
    }

    /// Whether `contract()` comes from the measured input.
    pub fn measured(&self) -> bool {
        (self.contract_mv == 0 || self.contract_ma == 0) && self.input_mv > 0
    }

    /// "PD 9.0 V 3.00 A" (negotiated), "PPS · 9.2 V in · ~40 W" (contract not
    /// reported: the measured input, marked "in"), "unknown" (powered, nothing
    /// known), "none" (unplugged).
    pub fn contract(&self) -> String {
        if self.kind == "none" {
            return "none".into();
        }
        let ty = self.label();
        if self.contract_mv > 0 && self.contract_ma > 0 {
            return format!("{ty} {:.1} V {:.2} A", self.contract_mv as f64 / 1000.0, self.contract_ma as f64 / 1000.0);
        }
        if self.input_mv > 0 {
            let v = self.input_mv as f64 / 1000.0;
            let w = self.input_mv as f64 * self.input_ma as f64 / 1e6;
            return if w >= 0.5 { format!("{ty} · {v:.1} V in · ~{w:.0} W") } else { format!("{ty} · {v:.1} V in") };
        }
        "unknown".into()
    }

    /// What should raise a change signal (not the live input samples).
    pub fn stable_key(&self) -> String {
        if self.measured() {
            format!("{} {} measured", self.kind, self.adapter)
        } else {
            format!("{} {} {}", self.kind, self.adapter, self.contract())
        }
    }
}

pub fn charger_info() -> ChargerInfo {
    let base = sys::path("/sys/class/power_supply");
    let mut c = ChargerInfo::default();
    let usb = base.join("qcom-battmgr-usb");
    let usb_online = sys::read_opt(&usb.join("online")).as_deref() == Some("1");
    if usb_online {
        c.adapter = sys::read_opt(&usb.join("usb_type"))
            .and_then(|t| sys::selected(&t))
            .filter(|t| t != "Unknown")
            .unwrap_or_default();
        c.input_mv = (sys::read_i64(&usb.join("voltage_now")).unwrap_or(0) / 1000).max(0) as u32;
        c.input_ma = (sys::read_i64(&usb.join("current_now")).unwrap_or(0) / 1000).max(0) as u32;
    }
    for n in sys::list_dir(&base) {
        if !n.starts_with("ucsi-source-psy") {
            continue;
        }
        let d = base.join(&n);
        if sys::read_opt(&d.join("online")).as_deref() != Some("1") {
            continue;
        }
        c.kind = sys::read_opt(&d.join("usb_type")).and_then(|t| sys::selected(&t)).unwrap_or_else(|| "USB".into());
        c.contract_mv = (sys::read_i64(&d.join("voltage_now")).unwrap_or(0) / 1000).max(0) as u32;
        c.contract_ma = (sys::read_i64(&d.join("current_max")).unwrap_or(0) / 1000).max(0) as u32;
        return c;
    }
    c.kind = if usb_online || external_power() { "USB".into() } else { "none".into() };
    c
}

/// The active charger: (type, contract) -- the `ChargerType` and
/// `ChargerContract` D-Bus properties (see `ChargerInfo::contract`).
pub fn charger() -> (String, String) {
    let c = charger_info();
    (c.kind.clone(), c.contract())
}

// ---------------------------------------------------------------- android

/// The platform files keep it in /etc/tb323fu; /etc/android-boot.sha256 is the
/// older development location.
pub const ANDROID_HASH: &str = "/etc/tb323fu/android-boot.sha256";
const ANDROID_HASH_OLD: &str = "/etc/android-boot.sha256";
const BACK_TO_ANDROID: [&str; 3] = ["/usr/local/sbin/back-to-android", "/usr/sbin/back-to-android", "/usr/libexec/tb323fu/back-to-android"];

pub fn android_hash() -> Option<String> {
    let h = sys::read_opt(&sys::path(ANDROID_HASH)).or_else(|| sys::read_opt(&sys::path(ANDROID_HASH_OLD)))?;
    let h = h.split_whitespace().next()?.to_lowercase();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

pub fn back_to_android_tool() -> Option<PathBuf> {
    // PATH first (NixOS: the platform package's sbin), then the FHS locations;
    // under a fake root (tests) only the fake FHS locations count
    if std::env::var_os("TB323FU_SYSFS_ROOT").is_none_or(|r| r.is_empty()) {
        if let Some(path) = std::env::var_os("PATH") {
            if let Some(p) = std::env::split_paths(&path).map(|d| d.join("back-to-android")).find(|p| p.is_file()) {
                return Some(p);
            }
        }
    }
    BACK_TO_ANDROID.iter().map(|p| sys::path(p)).find(|p| p.exists())
}

// ---------------------------------------------------------------- torch

pub fn torch_dir() -> PathBuf {
    sys::path("/sys/class/leds/white:flash")
}

/// (on, max_level). Torch mode only (the LED class brightness), never the
/// flash strobe.
pub fn torch_state() -> Res<(bool, u32)> {
    let d = torch_dir();
    let cur = sys::read_i64(&d.join("brightness")).ok_or("no torch LED")?;
    let max = sys::read_i64(&d.join("max_brightness")).unwrap_or(255) as u32;
    Ok((cur > 0, max))
}

pub fn torch_set(level: u32) -> Res<()> {
    let (_, max) = torch_state()?;
    sys::write(&torch_dir().join("brightness"), &level.min(max).to_string()).map_err(|e| werr("torch", e))
}

// ---------------------------------------------------------------- LED ring

pub fn ledring_dir() -> PathBuf {
    sys::path("/sys/class/leds/aw22127:rgb:indicator")
}

/// The colour the charge indicator shows: amber charging, green full / held /
/// bypass, red at or below `low` % on battery, otherwise off.
pub fn ledring_color(i: &BatteryInfo, low: u32, bypass: bool) -> Option<[u32; 3]> {
    match battery_state(i, bypass) {
        "charging" => Some([255, 90, 0]),
        "full" | "not-charging" | "bypass" => Some([0, 255, 0]),
        _ if i.capacity <= low => Some([255, 0, 0]),
        _ => None,
    }
}

pub fn ledring_apply(color: Option<[u32; 3]>, brightness: u32) -> Res<()> {
    let d = ledring_dir();
    match color {
        None => sys::write(&d.join("brightness"), "0").map_err(|e| werr("LED ring", e)),
        // brightness first: the LED core re-applies the old brightness after a
        // multi_intensity write and loses a brightness written right after it
        Some(c) => {
            sys::write(&d.join("brightness"), &brightness.min(255).to_string()).map_err(|e| werr("LED ring", e))?;
            sys::write(&d.join("multi_intensity"), &format!("{} {} {}", c[0], c[1], c[2])).map_err(|e| werr("LED ring", e))
        }
    }
}

pub const LED_MODES: [&str; 4] = ["off", "charge", "solid", "breathe"];

/// "#rrggbb" -> [r, g, b].
pub fn parse_color(s: &str) -> Res<[u32; 3]> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("colour must be #rrggbb".into());
    }
    let v = |i: usize| u32::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    Ok([v(0), v(2), v(4)])
}

/// Only the brightness (the colour stays).
pub fn ledring_brightness(v: u32) -> Res<()> {
    sys::write(&ledring_dir().join("brightness"), &v.min(255).to_string()).map_err(|e| werr("LED ring", e))
}

/// Breathing: brightness at `t_ms` into a cycle of `period_ms` (8..100 % of
/// `max`, never 0 so the colour stays latched).
pub fn breathe_level(max: u32, t_ms: u64, period_ms: u32) -> u32 {
    let x = (t_ms % period_ms.max(1) as u64) as f64 / period_ms.max(1) as f64;
    let level = 0.08 + 0.92 * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * x).cos());
    ((max as f64 * level).round() as u32).max(1)
}

// ---------------------------------------------------------------- idle refresh

pub fn refresh_param(name: &str) -> PathBuf {
    sys::path(&format!("/sys/module/msm/parameters/idle_refresh_{name}"))
}

pub fn refresh_available() -> bool {
    refresh_param("policy").exists()
}

pub fn policy_name(v: i64) -> &'static str {
    match v {
        0 => "off",
        1 => "manual",
        _ => "auto",
    }
}

pub fn policy_value(s: &str) -> Res<u32> {
    match s {
        "off" => Ok(0),
        "manual" => Ok(1),
        "auto" => Ok(2),
        _ => Err("policy must be off, manual or auto".into()),
    }
}

pub fn refresh_get(name: &str) -> Option<i64> {
    sys::read_i64(&refresh_param(name))
}

pub fn refresh_set(name: &str, v: u32) -> Res<()> {
    sys::write(&refresh_param(name), &v.to_string()).map_err(|e| werr(&format!("idle_refresh_{name}"), e))
}

/// A field of `idle_refresh_state` ("policy 2 enabled 1 ... hz 60 ...").
pub fn refresh_state_field(key: &str) -> Option<i64> {
    let s = sys::read_opt(&refresh_param("state"))?;
    let mut it = s.split_whitespace();
    while let Some(k) = it.next() {
        if k == key {
            return it.next()?.parse().ok();
        }
    }
    None
}

/// Named idle-time presets (ms before 60 Hz, ms before 30 Hz).
pub fn refresh_preset(name: &str) -> Res<(u32, u32)> {
    match name {
        "power-saver" => Ok((500, 2000)),
        "balanced" => Ok((1000, 5000)),
        "smooth" => Ok((3000, 15000)),
        _ => Err("preset must be power-saver, balanced or smooth".into()),
    }
}

// ---------------------------------------------------------------- GPU

pub fn gpu_dir() -> PathBuf {
    sys::path("/sys/class/devfreq/3d00000.gpu")
}

/// Available OPPs in MHz.
pub fn gpu_opps() -> Vec<u32> {
    sys::read_opt(&gpu_dir().join("available_frequencies"))
        .map(|s| s.split_whitespace().filter_map(|v| v.parse::<u64>().ok()).map(|hz| (hz / 1_000_000) as u32).collect())
        .unwrap_or_default()
}

/// Set floor and cap, widening before narrowing so min <= max at each step.
pub fn gpu_apply(min_mhz: u32, max_mhz: u32) -> Res<()> {
    if min_mhz > max_mhz {
        return Err("min above max".into());
    }
    let d = gpu_dir();
    let hz = |m: u32| (m as u64 * 1_000_000).to_string();
    let _ = sys::write(&d.join("max_freq"), &hz(max_mhz));
    sys::write(&d.join("min_freq"), &hz(min_mhz)).map_err(|e| werr("GPU min_freq", e))?;
    sys::write(&d.join("max_freq"), &hz(max_mhz)).map_err(|e| werr("GPU max_freq", e))
}

pub fn gpu_limits_valid(min_mhz: u32, max_mhz: u32) -> Res<()> {
    let opps = gpu_opps();
    let (lo, hi) = (opps.first().copied().unwrap_or(0), opps.last().copied().unwrap_or(u32::MAX));
    if min_mhz > max_mhz || min_mhz < lo || max_mhz > hi {
        return Err(format!("limits must satisfy {lo} <= min <= max <= {hi} MHz"));
    }
    Ok(())
}

// ---------------------------------------------------------------- USB

pub fn usb_wake_file() -> PathBuf {
    sys::path("/sys/bus/platform/devices/a600000.usb/power/wakeup")
}

fn cpu_boost_file() -> PathBuf {
    sys::path("/sys/devices/system/cpu/cpufreq/boost")
}

/// cpufreq boost, `None` when the cpufreq driver has no boost frequencies.
pub fn cpu_boost() -> Option<bool> {
    sys::read_opt(&cpu_boost_file()).map(|s| s == "1")
}

pub fn set_cpu_boost(on: bool) -> Res<()> {
    sys::write(&cpu_boost_file(), if on { "1" } else { "0" }).map_err(|e| werr("CPU boost", e))
}

pub fn usb_wake() -> Option<bool> {
    sys::read_opt(&usb_wake_file()).map(|s| s == "enabled")
}

pub fn set_usb_wake(on: bool) -> Res<()> {
    sys::write(&usb_wake_file(), if on { "enabled" } else { "disabled" }).map_err(|e| werr("USB wakeup", e))
}

/// Wakeup files of the power supplies (battery manager, USB, UCSI sources):
/// a charger plugged in or out wakes the tablet through them.
pub fn charger_wake_files() -> Vec<PathBuf> {
    let base = sys::path("/sys/class/power_supply");
    sys::list_dir(&base).into_iter().map(|n| base.join(n).join("power/wakeup")).filter(|p| p.exists()).collect()
}

/// Charger plug wakes the tablet (any power supply may wake it).
pub fn charger_wake() -> Option<bool> {
    let f = charger_wake_files();
    if f.is_empty() {
        return None;
    }
    Some(f.iter().any(|p| sys::read_opt(p).as_deref() == Some("enabled")))
}

pub fn set_charger_wake(on: bool) -> Res<()> {
    let files = charger_wake_files();
    if files.is_empty() {
        return Err("no power supply wakeup sources".into());
    }
    for p in files {
        sys::write(&p, if on { "enabled" } else { "disabled" }).map_err(|e| werr("charger wakeup", e))?;
    }
    Ok(())
}

/// USB-C ports: (port, data role, power role, partner attached), roles as
/// the selected word of the typec class ("device", "sink", ...).
pub fn typec_ports() -> Vec<(String, String, String, bool)> {
    let base = sys::path("/sys/class/typec");
    let names = sys::list_dir(&base);
    names
        .iter()
        .filter(|n| n.starts_with("port") && !n.contains('-'))
        .map(|n| {
            let d = base.join(n);
            let sel = |f: &str| sys::read_opt(&d.join(f)).and_then(|s| sys::selected(&s)).unwrap_or_default();
            (n.clone(), sel("data_role"), sel("power_role"), names.iter().any(|m| *m == format!("{n}-partner")))
        })
        .collect()
}

/// The configfs gadget (first one) and the UDC it can bind to.
pub fn gadget() -> Option<(PathBuf, String)> {
    let base = sys::path("/sys/kernel/config/usb_gadget");
    let g = sys::list_dir(&base).into_iter().next().map(|n| base.join(n))?;
    let udc = sys::list_dir(&sys::path("/sys/class/udc")).into_iter().next()?;
    Some((g, udc))
}

pub fn dev_mode() -> Option<bool> {
    let (g, _) = gadget()?;
    Some(!sys::read_opt(&g.join("UDC")).unwrap_or_default().is_empty())
}

pub fn set_dev_mode(on: bool) -> Res<()> {
    let (g, udc) = gadget().ok_or("no USB gadget configured")?;
    let cur = dev_mode().unwrap_or(false);
    if cur == on {
        return Ok(());
    }
    sys::write(&g.join("UDC"), if on { udc.as_str() } else { "" }).map_err(|e| werr("USB gadget", e))
}

// ---------------------------------------------------------------- emergency key

/// Shell-style file read by the layer-1 emergency-chord service.
pub const EMERGENCY_CONF: &str = "/etc/tb323fu/emergency-key.conf";

pub fn emergency_get() -> (bool, u32) {
    let m = crate::config::shell_vars(&sys::path(EMERGENCY_CONF));
    let en = m.get("ENABLED").map(|v| v != "0").unwrap_or(true);
    let hold = m.get("HOLD_SECONDS").and_then(|v| v.parse().ok()).unwrap_or(10);
    (en, hold)
}

pub fn emergency_set(enabled: bool, hold: u32) -> Res<()> {
    if !(3..=30).contains(&hold) {
        return Err("hold time must be 3..30 s".into());
    }
    let p = sys::path(EMERGENCY_CONF);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| werr("emergency key", e))?;
    }
    let body = format!(
        "# Emergency way back to Android: hold volume up + down (written by tb323fu-helperd)\nENABLED={}\nHOLD_SECONDS={}\n",
        enabled as u8, hold
    );
    std::fs::write(p, body).map_err(|e| werr("emergency key", e))
}

// ---------------------------------------------------------------- diagnostics

pub const PSTORE_ARCHIVE: &str = "/var/lib/systemd/pstore";

fn files_under(p: &std::path::Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let q = e.path();
            if q.is_dir() { files_under(&q, out) } else { out.push(q) }
        }
    }
}

pub fn pstore_files() -> Vec<PathBuf> {
    let mut v = Vec::new();
    files_under(&sys::path(PSTORE_ARCHIVE), &mut v);
    files_under(&sys::path("/sys/fs/pstore"), &mut v);
    v.sort();
    v
}

/// Boot time as UNIX seconds (now - uptime).
pub fn boot_time() -> Option<u64> {
    let up: f64 = sys::read_opt(&sys::path("/proc/uptime"))?.split_whitespace().next()?.parse().ok()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs_f64();
    Some((now - up) as u64)
}

/// The previous boot ended cleanly. Clocks and journal boot order are not
/// trusted: this device boots with its clock near 1970 until NTP, so pstore
/// file mtimes and `journalctl -b -1` (boots sorted by wall-clock time) point
/// at the wrong thing. Instead the helper keeps its own state in
/// /var/lib/tb323fu:
///   last-boot-id  the boot_id of the boot it last ran in; the previous
///                 boot's journal is read by that id and must end with an
///                 orderly shutdown ("Journal stopped", reboot/power-off
///                 target) -- a crash into the Qualcomm dump mode (900E)
///                 leaves no pstore record, so this is the main signal;
///   pstore-seen   the dmesg-* crash records already seen; a record not in
///                 the list appeared during this boot = the previous boot
///                 crashed (console-/pmsg-ramoops are written on every boot
///                 and do not count).
/// First run (no state yet): clean. Computed once per boot, after
/// systemd-pstore has archived this boot's records (unit ordering).
pub fn last_boot_clean() -> bool {
    static CLEAN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CLEAN.get_or_init(compute_last_boot_clean)
}

pub const STATE_DIR: &str = "/var/lib/tb323fu";

fn compute_last_boot_clean() -> bool {
    let dir = sys::path(STATE_DIR);
    let _ = std::fs::create_dir_all(&dir);
    // previous boot's journal, by boot id
    let cur = sys::read_opt(&sys::path("/proc/sys/kernel/random/boot_id")).map(|s| s.trim().to_string());
    let prev = sys::read_opt(&dir.join("last-boot-id")).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let journal_ok = match (&prev, &cur) {
        (Some(p), Some(c)) if p != c => journal_of_boot(p).map(|t| journal_tail_clean(&t)).unwrap_or(true),
        _ => true,
    };
    if let Some(c) = &cur {
        let _ = std::fs::write(dir.join("last-boot-id"), format!("{c}\n"));
    }
    // new pstore crash records
    let seen_path = dir.join("pstore-seen");
    let seen = sys::read_opt(&seen_path);
    let now = crash_records();
    let fresh = new_crash_records(seen.as_deref(), &now);
    let _ = std::fs::write(&seen_path, now.iter().map(|(n, z)| format!("{n} {z}\n")).collect::<String>());
    journal_ok && fresh.is_empty()
}

/// dmesg-* records in the pstore archive / live pstore as (name, size).
fn crash_records() -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = pstore_files().iter().filter_map(|p| {
        let name = p.file_name()?.to_string_lossy().into_owned();
        if !name.starts_with("dmesg-") { return None; }
        Some((name, std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)))
    }).collect();
    v.sort();
    v.dedup();
    v
}

/// Records in `now` that were not in the `seen` list ("name size" lines).
/// Without a list (first run) nothing counts as new.
pub fn new_crash_records(seen: Option<&str>, now: &[(String, u64)]) -> Vec<String> {
    let Some(seen) = seen else { return Vec::new() };
    let old: std::collections::HashSet<(String, u64)> = seen.lines().filter_map(|l| {
        let mut it = l.split_whitespace();
        Some((it.next()?.to_string(), it.next()?.parse().ok()?))
    }).collect();
    now.iter().filter(|r| !old.contains(*r)).map(|(n, _)| n.clone()).collect()
}

/// The tail of a boot's journal shows an orderly shutdown.
pub fn journal_tail_clean(text: &str) -> bool {
    ["Journal stopped", "Reached target reboot.target", "Reached target poweroff.target",
     "System Reboot.", "System Power Off.", "Shutting down."].iter().any(|m| text.contains(m))
}

/// Last lines of the journal of the boot with id `boot` (`None`: no such boot
/// in the journal, journalctl missing, or running under a fake root).
fn journal_of_boot(boot: &str) -> Option<String> {
    if std::env::var_os("TB323FU_SYSFS_ROOT").is_some_and(|r| !r.is_empty()) {
        return None;
    }
    let out = std::process::Command::new("journalctl")
        .args(["-b", boot, "-n", "40", "-o", "cat", "--no-pager"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status.success() && !text.trim().is_empty()).then_some(text)
}

// ---------------------------------------------------------------- thermal (read-only)

/// Board sensors shown by name (thermal zone type without "-thermal"): the
/// skin/quiet NTCs, battery, USB connector, panel, Wi-Fi, memory, cameras.
/// The SoC's many tsens zones are folded into CPU and GPU maxima instead.
pub const THERMAL_ZONES: [&str; 14] = ["skin", "quiet", "batt", "batt2", "usb", "usb2-conn", "lcm", "wlan", "ddr", "ufs", "xo",
    "rear-cam", "fcam", "wls"];

#[derive(Debug, Clone, PartialEq)]
pub struct Thermal {
    /// skin (else quiet) NTC, °C; NaN when neither exists
    pub surface: f64,
    /// hottest CPU tsens zone (cpu-*, cpullc-*), °C; NaN when none
    pub cpu_max: f64,
    /// hottest GPU tsens zone (gpuss-*), °C; NaN when none
    pub gpu_max: f64,
    /// a CPU or GPU cooling device is above state 0
    pub throttling: bool,
    /// the whitelisted zones that exist, °C
    pub zones: BTreeMap<String, f64>,
}

fn is_cpu_zone(t: &str) -> bool {
    t.starts_with("cpu-") || t.starts_with("cpullc-")
}

/// Read the thermal zones once (only reads: no trip or policy is touched).
/// None when the kernel exposes none of the known zones.
pub fn thermal() -> Option<Thermal> {
    let base = sys::path("/sys/class/thermal");
    let (mut cpu, mut gpu) = (f64::NAN, f64::NAN);
    let mut zones = BTreeMap::new();
    let mut throttling = false;
    for e in sys::list_dir(&base) {
        let d = base.join(&e);
        if e.starts_with("thermal_zone") {
            let Some(ty) = sys::read_opt(&d.join("type")) else { continue };
            let t = ty.strip_suffix("-thermal").unwrap_or(&ty).to_string();
            let wanted = THERMAL_ZONES.contains(&t.as_str());
            let (c, g) = (is_cpu_zone(&t), t.starts_with("gpuss-"));
            if !(wanted || c || g) {
                continue;
            }
            let Some(milli) = sys::read_i64(&d.join("temp")) else { continue };
            let deg = milli as f64 / 1000.0;
            if c {
                cpu = if cpu.is_nan() { deg } else { cpu.max(deg) };
            } else if g {
                gpu = if gpu.is_nan() { deg } else { gpu.max(deg) };
            } else {
                zones.insert(t, deg);
            }
        } else if e.starts_with("cooling_device") {
            let ty = sys::read_opt(&d.join("type")).unwrap_or_default();
            if (ty.starts_with("cpufreq-") || ty.starts_with("devfreq-")) && sys::read_i64(&d.join("cur_state")).unwrap_or(0) > 0 {
                throttling = true;
            }
        }
    }
    if zones.is_empty() && cpu.is_nan() && gpu.is_nan() {
        return None;
    }
    let surface = zones.get("skin").or_else(|| zones.get("quiet")).copied().unwrap_or(f64::NAN);
    Some(Thermal { surface, cpu_max: cpu, gpu_max: gpu, throttling, zones })
}

// ---------------------------------------------------------------- versions / firmware

pub fn kernel_version() -> String {
    sys::read_opt(&sys::path("/proc/version")).unwrap_or_else(|| "unknown".into())
}

pub fn series_tag() -> String {
    sys::read_opt(&sys::path("/etc/tb323fu/series"))
        .or_else(|| sys::read_opt(&sys::path("/usr/share/tb323fu/series")))
        .unwrap_or_else(|| "unknown".into())
}

const MANIFESTS: [&str; 2] = ["/usr/share/tb323fu/firmware/manifest.tsv", "/etc/tb323fu/firmware-manifest.tsv"];

/// target (as in the manifest, relative to /) -> "ok" / "mismatch" / "missing".
pub fn firmware_check() -> BTreeMap<String, String> {
    use sha2::{Digest, Sha256};
    let mut out = BTreeMap::new();
    let Some(text) = MANIFESTS.iter().find_map(|m| std::fs::read_to_string(sys::path(m)).ok()) else {
        return out;
    };
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let mut cols = l.split('\t');
        let (Some(target), Some(want)) = (cols.next(), cols.next()) else { continue };
        let p = sys::path(&format!("/{}", target.trim_start_matches('/')));
        let state = match std::fs::read(&p) {
            Err(_) => "missing",
            Ok(data) => {
                let got: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
                if got.eq_ignore_ascii_case(want.trim()) { "ok" } else { "mismatch" }
            }
        };
        out.insert(target.to_string(), state.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_records_first_run_and_new() {
        let now = vec![("dmesg-ramoops-0".to_string(), 95662u64)];
        assert!(new_crash_records(None, &now).is_empty(), "first run: nothing is new");
        assert!(new_crash_records(Some("dmesg-ramoops-0 95662\n"), &now).is_empty(), "already seen");
        let now2 = vec![("dmesg-ramoops-0".to_string(), 95662u64), ("dmesg-ramoops-1".to_string(), 1200u64)];
        assert_eq!(new_crash_records(Some("dmesg-ramoops-0 95662\n"), &now2), vec!["dmesg-ramoops-1"]);
        // same name, different size = a new record that replaced the old one
        let now3 = vec![("dmesg-ramoops-0".to_string(), 4000u64)];
        assert_eq!(new_crash_records(Some("dmesg-ramoops-0 95662\n"), &now3), vec!["dmesg-ramoops-0"]);
    }

    #[test]
    fn journal_tail_orderly_shutdown() {
        let clean = "Stopped target basic.target\nReached target reboot.target - System Reboot.\nsystemd-journald.service: Deactivated\nJournal stopped\n";
        assert!(journal_tail_clean(clean));
        let crashed = "New session 31 of user root.\nStarted session-31.scope - Session 31 of User root.\n";
        assert!(!journal_tail_clean(crashed));
    }
    fn info(status: &str, cap: u32, ma: i32, limit: u32) -> BatteryInfo {
        BatteryInfo { status: status.into(), capacity: cap, current_ma: ma, voltage_mv: 4000, temperature_c: 25.0,
            health: "Good".into(), cycle_count: 1, design_capacity_mah: -1, charge_limit: limit, online: true,
            state_of_health: 100, ocv_mv: 0, charge_counter_mah: 0 }
    }
    #[test]
    fn states() {
        assert_eq!(battery_state(&info("Charging", 60, 1500, 80), false), "charging");
        assert_eq!(battery_state(&info("Charging", 80, 0, 80), false), "bypass");
        assert_eq!(battery_state(&info("Discharging", 50, -300, 80), false), "discharging");
        assert_eq!(battery_state(&info("Full", 100, 0, 100), false), "full");
        // Bypass on: the firmware still says Charging, 185 mA (t26, 65 W PPS charger)
        assert_eq!(battery_state(&info("Charging", 99, 185, 99), true), "bypass");
        // capacity drifted under the held limit: still bypass
        assert_eq!(battery_state(&info("Charging", 98, 120, 99), true), "bypass");
        assert_eq!(battery_state(&info("Discharging", 70, -150, 70), true), "bypass");
        // just switched on, current still large: say what the battery does
        assert_eq!(battery_state(&info("Charging", 70, 9000, 70), true), "charging");
        assert_eq!(battery_state(&info("Discharging", 70, -1200, 70), true), "discharging");
        // no external power: the switch means nothing
        let mut i = info("Discharging", 70, -100, 70);
        i.online = false;
        assert_eq!(battery_state(&i, true), "discharging");
        // without the switch, 185 mA under the limit is charging
        assert_eq!(battery_state(&info("Charging", 60, 185, 80), false), "charging");
    }
    #[test]
    fn colours() {
        assert_eq!(ledring_color(&info("Charging", 60, 1500, 80), 15, false), Some([255, 90, 0]));
        assert_eq!(ledring_color(&info("Charging", 80, 0, 80), 15, false), Some([0, 255, 0]));
        assert_eq!(ledring_color(&info("Charging", 99, 185, 99), 15, true), Some([0, 255, 0]));
        assert_eq!(ledring_color(&info("Discharging", 10, -300, 80), 15, false), Some([255, 0, 0]));
        assert_eq!(ledring_color(&info("Discharging", 50, -300, 80), 15, false), None);
    }
    #[test]
    fn charger_contract() {
        let c = |kind: &str, adapter: &str, cmv, cma, imv, ima| ChargerInfo {
            kind: kind.into(), adapter: adapter.into(), contract_mv: cmv, contract_ma: cma, input_mv: imv, input_ma: ima };
        // negotiated contract reported
        assert_eq!(c("PD", "PD", 9000, 3000, 9100, 2000).contract(), "PD 9.0 V 3.00 A");
        assert_eq!(c("C", "SDP", 5000, 100, 5023, 187).contract(), "C 5.0 V 0.10 A");
        // t26 65 W PPS charger: UCSI contract 0, input ~9.25 V, ~4.3 A
        let pps = c("PD", "PD_PPS", 0, 0, 9248, 4300);
        assert!(pps.measured());
        assert_eq!(pps.contract(), "PPS · 9.2 V in · ~40 W");
        assert_eq!(c("PD", "PD", 0, 0, 9192, 0).contract(), "PD · 9.2 V in");
        assert_eq!(c("USB", "", 0, 0, 0, 0).contract(), "unknown");
        assert_eq!(c("none", "", 0, 0, 0, 0).contract(), "none");
        // live samples do not change the signal key
        assert_eq!(pps.stable_key(), c("PD", "PD_PPS", 0, 0, 9180, 4100).stable_key());
    }
    #[test]
    fn charger_from_sysfs() {
        let _g = crate::sys::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let r = std::env::temp_dir().join(format!("tb323fu-charger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        let mk = |rel: String, v: &str| {
            let p = r.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, format!("{v}\n")).unwrap();
        };
        let u = "sys/class/power_supply/qcom-battmgr-usb";
        mk(format!("{u}/type"), "USB");
        mk(format!("{u}/online"), "1");
        mk(format!("{u}/usb_type"), "Unknown SDP DCP CDP ACA C PD PD_DRP [PD_PPS] BrickID");
        mk(format!("{u}/voltage_now"), "9192000");
        mk(format!("{u}/current_now"), "4310000");
        let s = "sys/class/power_supply/ucsi-source-psy-pmic_glink.ucsi.02";
        mk(format!("{s}/type"), "USB");
        mk(format!("{s}/online"), "1");
        mk(format!("{s}/usb_type"), "C [PD] PD_PPS");
        mk(format!("{s}/voltage_now"), "0");
        mk(format!("{s}/current_max"), "0");
        std::env::set_var("TB323FU_SYSFS_ROOT", &r);
        let c = charger_info();
        assert_eq!((c.kind.as_str(), c.adapter.as_str(), c.input_mv, c.input_ma), ("PD", "PD_PPS", 9192, 4310));
        assert_eq!(charger(), ("PD".to_string(), "PPS · 9.2 V in · ~40 W".to_string()));
        mk(format!("{u}/online"), "0");
        mk(format!("{s}/online"), "0");
        assert_eq!(charger(), ("none".to_string(), "none".to_string()));
        std::env::remove_var("TB323FU_SYSFS_ROOT");
        let _ = std::fs::remove_dir_all(&r);
    }
    #[test]
    fn thermal_zones() {
        let _g = crate::sys::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let r = std::env::temp_dir().join(format!("tb323fu-thermal-{}", std::process::id()));
        let zone = |n: u32, ty: &str, t: i64| {
            let d = r.join(format!("sys/class/thermal/thermal_zone{n}"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("type"), format!("{ty}\n")).unwrap();
            std::fs::write(d.join("temp"), format!("{t}\n")).unwrap();
        };
        zone(0, "qcom-battmgr-bat", 28700); // not listed
        zone(1, "cpu-0-0-0-thermal", 35200);
        zone(2, "cpullc-1-0-thermal", 41000);
        zone(3, "gpuss-3-thermal", 39500);
        zone(4, "skin-thermal", 32371);
        zone(5, "batt-thermal", 29464);
        let c = r.join("sys/class/thermal/cooling_device2");
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(c.join("type"), "devfreq-3d00000.gpu\n").unwrap();
        std::fs::write(c.join("cur_state"), "0\n").unwrap();
        std::env::set_var("TB323FU_SYSFS_ROOT", &r);
        let t = thermal().expect("zones present");
        assert_eq!(t.cpu_max, 41.0);
        assert_eq!(t.gpu_max, 39.5);
        assert!((t.surface - 32.371).abs() < 1e-9);
        assert!(!t.throttling);
        assert_eq!(t.zones.keys().collect::<Vec<_>>(), ["batt", "skin"]);
        std::fs::write(c.join("cur_state"), "3\n").unwrap();
        assert!(thermal().unwrap().throttling);
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(&r).unwrap();
        assert!(thermal().is_none());
        std::env::remove_var("TB323FU_SYSFS_ROOT");
        let _ = std::fs::remove_dir_all(&r);
    }
    #[test]
    fn led_effects() {
        assert_eq!(parse_color("#ff5a00"), Ok([255, 90, 0]));
        assert_eq!(parse_color("00a0FF"), Ok([0, 160, 255]));
        assert!(parse_color("#ff5a0").is_err());
        assert!(parse_color("#gg0000").is_err());
        assert_eq!(breathe_level(200, 0, 4000), 16);
        assert_eq!(breathe_level(200, 2000, 4000), 200);
        assert_eq!(breathe_level(5, 0, 4000), 1);
    }
    #[test]
    fn policy_names() {
        assert_eq!(policy_value("auto"), Ok(2));
        assert_eq!(policy_name(1), "manual");
        assert!(refresh_preset("smooth").is_ok());
    }
}
