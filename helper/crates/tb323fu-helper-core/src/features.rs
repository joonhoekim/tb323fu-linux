// SPDX-License-Identifier: MIT
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

/// `charging` / `discharging` / `bypass` / `full` / `not-charging`.
/// battmgr keeps reporting "Charging" while the battery is held at the limit
/// with no current (and in bypass), so the current decides.
pub fn battery_state(i: &BatteryInfo) -> &'static str {
    match i.status.as_str() {
        "Discharging" => "discharging",
        "Full" => "full",
        "Not charging" => {
            if i.online && i.capacity >= i.charge_limit { "bypass" } else { "not-charging" }
        }
        "Charging" => {
            if i.current_ma.abs() < 50 && i.capacity >= i.charge_limit { "bypass" } else { "charging" }
        }
        _ => "not-charging",
    }
}

pub const CHARGE_LIMIT_MIN: u32 = 20;

/// Set the end threshold (and start = end - 10, as UPower did), keeping
/// start <= end at every step.
pub fn set_charge_limit(pct: u32) -> Res<()> {
    if !(CHARGE_LIMIT_MIN..=100).contains(&pct) {
        return Err(format!("charge limit must be {CHARGE_LIMIT_MIN}..100"));
    }
    let b = battery_dir().ok_or("no battery power supply")?;
    let end = b.join("charge_control_end_threshold");
    let start = b.join("charge_control_start_threshold");
    let new_start = pct.saturating_sub(10);
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

/// The active charger: selected type and negotiated voltage/current of the
/// online UCSI source (e.g. "PD", "PD 9.0 V 3.00 A").
pub fn charger() -> (String, String) {
    let base = sys::path("/sys/class/power_supply");
    for n in sys::list_dir(&base) {
        if !n.starts_with("ucsi-source-psy") {
            continue;
        }
        let d = base.join(&n);
        if sys::read_opt(&d.join("online")).as_deref() != Some("1") {
            continue;
        }
        let ty = sys::read_opt(&d.join("usb_type")).and_then(|c| sys::selected(&c)).unwrap_or_else(|| "USB".into());
        let v = sys::read_i64(&d.join("voltage_now")).unwrap_or(0) as f64 / 1e6;
        let a = sys::read_i64(&d.join("current_max")).unwrap_or(0) as f64 / 1e6;
        return (ty.clone(), format!("{ty} {v:.1} V {a:.2} A"));
    }
    if external_power() {
        ("USB".into(), "unknown".into())
    } else {
        ("none".into(), "none".into())
    }
}

// ---------------------------------------------------------------- android

pub const ANDROID_HASH: &str = "/etc/android-boot.sha256";
const BACK_TO_ANDROID: [&str; 3] = ["/usr/local/sbin/back-to-android", "/usr/sbin/back-to-android", "/usr/libexec/tb323fu/back-to-android"];

pub fn android_hash() -> Option<String> {
    let h = sys::read_opt(&sys::path(ANDROID_HASH))?;
    let h = h.split_whitespace().next()?.to_lowercase();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

pub fn back_to_android_tool() -> Option<PathBuf> {
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
pub fn ledring_color(i: &BatteryInfo, low: u32) -> Option<[u32; 3]> {
    match battery_state(i) {
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
        Some(c) => {
            sys::write(&d.join("multi_intensity"), &format!("{} {} {}", c[0], c[1], c[2])).map_err(|e| werr("LED ring", e))?;
            sys::write(&d.join("brightness"), &brightness.min(255).to_string()).map_err(|e| werr("LED ring", e))
        }
    }
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

pub fn usb_wake() -> Option<bool> {
    sys::read_opt(&usb_wake_file()).map(|s| s == "enabled")
}

pub fn set_usb_wake(on: bool) -> Res<()> {
    sys::write(&usb_wake_file(), if on { "enabled" } else { "disabled" }).map_err(|e| werr("USB wakeup", e))
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

/// The previous boot ended cleanly: no panic/oops record (dmesg-ramoops)
/// archived during this boot, and the previous boot's journal ends with
/// journald's own "Journal stopped" (a clean shutdown). The journal check
/// matters because a crash into the Qualcomm dump mode (900E) wipes ramoops,
/// leaving no pstore record at all. console-ramoops is written on every boot,
/// clean or not, so it does not count. Computed once per boot.
pub fn last_boot_clean() -> bool {
    static CLEAN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CLEAN.get_or_init(|| !pstore_crash_this_boot() && previous_journal_clean().unwrap_or(true))
}

fn pstore_crash_this_boot() -> bool {
    let bt = boot_time().unwrap_or(0);
    pstore_files().iter().any(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        name.starts_with("dmesg-") && std::fs::metadata(p).and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() + 5 >= bt).unwrap_or(false)
    })
}

/// `None` when there is no previous boot in the journal (or under a fake root).
fn previous_journal_clean() -> Option<bool> {
    if std::env::var_os("TB323FU_SYSFS_ROOT").is_some_and(|r| !r.is_empty()) {
        return None;
    }
    let out = std::process::Command::new("journalctl").args(["-b", "-1", "-n", "30", "-o", "cat", "--no-pager"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || text.trim().is_empty() {
        return None;
    }
    Some(text.contains("Journal stopped"))
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
    fn info(status: &str, cap: u32, ma: i32, limit: u32) -> BatteryInfo {
        BatteryInfo { status: status.into(), capacity: cap, current_ma: ma, voltage_mv: 4000, temperature_c: 25.0,
            health: "Good".into(), cycle_count: 1, design_capacity_mah: -1, charge_limit: limit, online: true }
    }
    #[test]
    fn states() {
        assert_eq!(battery_state(&info("Charging", 60, 1500, 80)), "charging");
        assert_eq!(battery_state(&info("Charging", 80, 0, 80)), "bypass");
        assert_eq!(battery_state(&info("Discharging", 50, -300, 80)), "discharging");
        assert_eq!(battery_state(&info("Full", 100, 0, 100)), "full");
    }
    #[test]
    fn colours() {
        assert_eq!(ledring_color(&info("Charging", 60, 1500, 80), 15), Some([255, 90, 0]));
        assert_eq!(ledring_color(&info("Charging", 80, 0, 80), 15), Some([0, 255, 0]));
        assert_eq!(ledring_color(&info("Discharging", 10, -300, 80), 15), Some([255, 0, 0]));
        assert_eq!(ledring_color(&info("Discharging", 50, -300, 80), 15), None);
    }
    #[test]
    fn policy_names() {
        assert_eq!(policy_value("auto"), Ok(2));
        assert_eq!(policy_name(1), "manual");
        assert!(refresh_preset("smooth").is_ok());
    }
}
