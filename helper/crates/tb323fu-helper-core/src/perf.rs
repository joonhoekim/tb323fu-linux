// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! Performance controls: the board-temperature (quiet-thermal) profile, the
//! panel over-temperature brightness limit and per-cluster CPU frequency
//! limits. Every write goes through a fixed allow-list and hard bounds here;
//! nothing a caller or the config file passes can move them.

use crate::features::Res;
use crate::sys;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------- thermal profile

/// The only thermal zone whose trips the helper writes, and only its
/// `passive` trips (never `hot`/`critical`, never a chip zone).
pub const QUIET_ZONE: &str = "quiet-thermal";
pub const THERMAL_PROFILES: [&str; 3] = ["quiet", "default", "performance"];
/// The highest passive trip may never go above this (m°C): below Android's
/// game mode (quiet-therm set points 58-62 °C).
pub const TOP_TRIP_MAX_MC: i64 = 58_000;
/// A recorded base outside this range is not the device tree's: refuse.
const BASE_MIN_MC: i64 = 35_000;
const BASE_MAX_MC: i64 = 55_000;
/// Fall back from `performance` at these temperatures (m°C).
pub const BATTERY_GUARD_MC: i64 = 45_000;
pub const BOARD_GUARD_MARGIN_MC: i64 = 2_000;

/// Offset added to every base trip, m°C.
pub fn profile_offset(p: &str) -> Option<i64> {
    match p {
        "quiet" => Some(-3_000),
        "default" => Some(0),
        "performance" => Some(8_000),
        _ => None,
    }
}

fn zone_by_type(ty: &str) -> Option<PathBuf> {
    let base = sys::path("/sys/class/thermal");
    sys::list_dir(&base)
        .into_iter()
        .filter(|n| n.starts_with("thermal_zone"))
        .map(|n| base.join(n))
        .find(|p| sys::read_opt(&p.join("type")).as_deref() == Some(ty))
}

pub fn zone_temp_mc(ty: &str) -> Option<i64> {
    sys::read_i64(&zone_by_type(ty)?.join("temp"))
}

pub fn quiet_zone() -> Option<PathBuf> {
    zone_by_type(QUIET_ZONE)
}

/// (trip index, m°C) of the zone's passive trips, by index.
pub fn passive_trips(zone: &Path) -> Vec<(u32, i64)> {
    let mut v: Vec<(u32, i64)> = sys::list_dir(zone)
        .into_iter()
        .filter_map(|n| n.strip_prefix("trip_point_")?.strip_suffix("_type")?.parse::<u32>().ok())
        .filter(|i| sys::read_opt(&zone.join(format!("trip_point_{i}_type"))).as_deref() == Some("passive"))
        .filter_map(|i| Some((i, sys::read_i64(&zone.join(format!("trip_point_{i}_temp")))?)))
        .collect();
    v.sort();
    v
}

pub fn thermal_profile_available() -> bool {
    quiet_zone().is_some_and(|z| !passive_trips(&z).is_empty())
}

fn check_base(base: &[(u32, i64)]) -> Res<()> {
    if base.is_empty() {
        return Err("no passive trips".into());
    }
    if base.iter().any(|(_, t)| !(BASE_MIN_MC..=BASE_MAX_MC).contains(t)) {
        return Err("board trips outside 35..55 °C: not the device tree's values".into());
    }
    if base.windows(2).any(|w| w[0].1 > w[1].1) {
        return Err("board trips not in ascending order".into());
    }
    Ok(())
}

/// The writes that move `current` to `base + offset`, in an order that keeps
/// the steps ascending: highest trip first when raising, lowest first when
/// lowering. Refuses anything above TOP_TRIP_MAX_MC.
pub fn plan_trips(base: &[(u32, i64)], current: &[(u32, i64)], offset: i64) -> Res<Vec<(u32, i64)>> {
    check_base(base)?;
    if base.iter().map(|b| b.0).ne(current.iter().map(|c| c.0)) {
        return Err("board trips changed since the base was recorded".into());
    }
    let mut out: Vec<(u32, i64)> = base.iter().map(|(i, t)| (*i, t + offset)).collect();
    let top = out.iter().map(|t| t.1).max().unwrap_or(0);
    if top > TOP_TRIP_MAX_MC {
        return Err(format!("highest board trip would be {} °C (limit {} °C)", top / 1000, TOP_TRIP_MAX_MC / 1000));
    }
    let raising = out.last().map(|t| t.1) > current.last().map(|t| t.1);
    if raising {
        out.reverse();
    }
    Ok(out)
}

/// The profile the trips match, if any.
pub fn match_profile(base: &[(u32, i64)], current: &[(u32, i64)]) -> Option<&'static str> {
    THERMAL_PROFILES.into_iter().find(|p| {
        let off = profile_offset(p).unwrap_or(0);
        base.len() == current.len() && base.iter().zip(current).all(|(b, c)| b.0 == c.0 && b.1 + off == c.1)
    })
}

const BASE_FILE: &str = "/var/lib/tb323fu/thermal-base";

fn boot_id() -> String {
    sys::read_opt(&sys::path("/proc/sys/kernel/random/boot_id")).unwrap_or_default()
}

/// The device-tree trips of this boot. Recorded the first time the helper
/// runs in a boot (trips are DT values until something writes them) and
/// read back afterwards, so a restarted daemon never takes an offset set as
/// the base.
pub fn thermal_base() -> Res<Vec<(u32, i64)>> {
    let zone = quiet_zone().ok_or("no quiet-thermal zone")?;
    let id = boot_id();
    let file = sys::path(BASE_FILE);
    if let Some(text) = sys::read_opt(&file) {
        let mut lines = text.lines();
        if lines.next() == Some(id.as_str()) {
            let v: Vec<(u32, i64)> = lines
                .filter_map(|l| {
                    let (i, t) = l.split_once(' ')?;
                    Some((i.parse().ok()?, t.parse().ok()?))
                })
                .collect();
            check_base(&v)?;
            return Ok(v);
        }
    }
    let v = passive_trips(&zone);
    check_base(&v)?;
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let body: String = std::iter::once(id).chain(v.iter().map(|(i, t)| format!("{i} {t}"))).collect::<Vec<_>>().join("\n");
    let _ = std::fs::write(&file, body + "\n");
    Ok(v)
}

/// Write the trips of a profile and read them back.
pub fn apply_thermal_profile(name: &str) -> Res<()> {
    let off = profile_offset(name).ok_or("profile must be quiet, default or performance")?;
    let zone = quiet_zone().ok_or("no quiet-thermal zone")?;
    let base = thermal_base()?;
    let cur = passive_trips(&zone);
    for (i, t) in plan_trips(&base, &cur, off)? {
        if cur.contains(&(i, t)) {
            continue;
        }
        sys::write(&zone.join(format!("trip_point_{i}_temp")), &t.to_string()).map_err(|e| format!("board trip {i}: {e}"))?;
    }
    match match_profile(&base, &passive_trips(&zone)) {
        Some(p) if p == name => Ok(()),
        _ => Err("board trips did not take the new values".into()),
    }
}

/// The profile in effect ("custom" when the trips match none).
pub fn thermal_profile() -> Option<String> {
    let zone = quiet_zone()?;
    let base = thermal_base().ok()?;
    Some(match_profile(&base, &passive_trips(&zone)).unwrap_or("custom").to_string())
}

/// Current passive trips, °C.
pub fn thermal_trips() -> Vec<f64> {
    quiet_zone().map(|z| passive_trips(&z).into_iter().map(|t| t.1 as f64 / 1000.0).collect()).unwrap_or_default()
}

/// Why `performance` must end now, if it must: a battery at 45 °C, or the
/// board 2 °C past its highest (raised) trip.
pub fn performance_guard(batt_mc: &[i64], board_mc: Option<i64>, top_trip_mc: Option<i64>) -> Option<String> {
    if let Some(t) = batt_mc.iter().copied().filter(|t| *t >= BATTERY_GUARD_MC).max() {
        return Some(format!("battery at {:.1} °C", t as f64 / 1000.0));
    }
    match (board_mc, top_trip_mc) {
        (Some(b), Some(top)) if b >= top + BOARD_GUARD_MARGIN_MC => Some(format!("board at {:.1} °C", b as f64 / 1000.0)),
        _ => None,
    }
}

/// performance_guard() on the live sensors.
pub fn performance_guard_now() -> Option<String> {
    let batt: Vec<i64> = ["batt-thermal", "batt2-thermal"].iter().filter_map(|z| zone_temp_mc(z)).collect();
    let top = quiet_zone().and_then(|z| passive_trips(&z).last().map(|t| t.1));
    performance_guard(&batt, zone_temp_mc(QUIET_ZONE), top)
}

// ---------------------------------------------------------------- panel heat limit

/// Android's lcm-thermal policy: at 55 °C the backlight is held at 178/255.
pub const PANEL_HOT_MC: i64 = 55_000;
pub const PANEL_COOL_MC: i64 = 52_000;
pub const PANEL_CAP_NUM: u32 = 178;
pub const PANEL_CAP_DEN: u32 = 255;

pub fn backlight_dir() -> Option<PathBuf> {
    let base = sys::path("/sys/class/backlight");
    sys::list_dir(&base).into_iter().next().map(|n| base.join(n))
}

pub fn panel_limit_available() -> bool {
    backlight_dir().is_some() && zone_by_type("lcm-thermal").is_some()
}

/// Limited after this sample (hysteresis between 52 and 55 °C).
pub fn panel_should_limit(temp_mc: i64, limited: bool) -> bool {
    if limited { temp_mc >= PANEL_COOL_MC } else { temp_mc >= PANEL_HOT_MC }
}

/// The brightness cap for a backlight with this maximum.
pub fn panel_cap(max: u32) -> u32 {
    (max as u64 * PANEL_CAP_NUM as u64 / PANEL_CAP_DEN as u64) as u32
}

/// (brightness, max_brightness).
pub fn backlight() -> Option<(u32, u32)> {
    let d = backlight_dir()?;
    Some((sys::read_i64(&d.join("brightness"))? as u32, sys::read_i64(&d.join("max_brightness"))? as u32))
}

pub fn set_backlight(v: u32) -> Res<()> {
    let d = backlight_dir().ok_or("no backlight")?;
    sys::write(&d.join("brightness"), &v.to_string()).map_err(|e| format!("backlight: {e}"))
}

pub fn panel_temp_mc() -> Option<i64> {
    zone_temp_mc("lcm-thermal")
}

/// The screen is off (backlight at 0 or blanked).
pub fn screen_off() -> bool {
    let Some(d) = backlight_dir() else { return false };
    sys::read_i64(&d.join("bl_power")).is_some_and(|p| p != 0) || sys::read_i64(&d.join("actual_brightness")) == Some(0)
}

// ---------------------------------------------------------------- CPU limits

/// The two clusters (cpufreq policies) as named in the API.
pub const CLUSTERS: [(&str, &str); 2] = [("little", "policy0"), ("big", "policy6")];
/// Highest allowed floor per cluster, MHz: the first thermal step's
/// frequency; a higher floor only adds heat.
pub fn floor_max_mhz(cluster: &str) -> u32 {
    if cluster == "big" { 2880 } else { 1996 }
}

pub fn policy_dir(policy: &str) -> PathBuf {
    sys::path(&format!("/sys/devices/system/cpu/cpufreq/{policy}"))
}

fn khz_list(p: &Path, f: &str) -> Vec<u32> {
    sys::read_opt(&p.join(f)).map(|s| s.split_whitespace().filter_map(|v| v.parse().ok()).collect()).unwrap_or_default()
}

/// Frequencies (kHz) of a cluster without the boost ones, ascending.
pub fn cluster_freqs(cluster: &str) -> Vec<u32> {
    let Some((_, pol)) = CLUSTERS.iter().find(|c| c.0 == cluster) else { return Vec::new() };
    let mut v = khz_list(&policy_dir(pol), "scaling_available_frequencies");
    v.sort();
    v
}

pub fn cpu_limits_available() -> bool {
    CLUSTERS.iter().all(|(c, _)| !cluster_freqs(c).is_empty())
}

/// Hardware range of a cluster in MHz (boost frequencies not counted).
pub fn cluster_range(cluster: &str) -> Option<(u32, u32)> {
    let f = cluster_freqs(cluster);
    Some((f.first()? / 1000, f.last()? / 1000))
}

/// The available frequency (kHz) nearest to `mhz`.
pub fn nearest_khz(freqs: &[u32], mhz: u32) -> Option<u32> {
    let want = mhz as i64 * 1000;
    freqs.iter().copied().min_by_key(|f| (*f as i64 - want).abs())
}

/// Check limits [little_min, little_max, big_min, big_max] (MHz).
pub fn cpu_limits_valid(l: [u32; 4]) -> Res<()> {
    for (k, (c, _)) in CLUSTERS.iter().enumerate() {
        let (lo, hi) = cluster_range(c).ok_or("no cpufreq")?;
        let (mn, mx) = (l[2 * k], l[2 * k + 1]);
        if mn > mx || mn < lo || mx > hi {
            return Err(format!("{c} cluster: need {lo} <= min <= max <= {hi} MHz"));
        }
        if mn > floor_max_mhz(c) {
            return Err(format!("{c} cluster: minimum at most {} MHz", floor_max_mhz(c)));
        }
    }
    Ok(())
}

/// Default limits: the whole hardware range.
pub fn cpu_full_range() -> Option<[u32; 4]> {
    let (a, b) = cluster_range("little")?;
    let (c, d) = cluster_range("big")?;
    Some([a, b, c, d])
}

/// Write the limits, widening before narrowing (min <= max at every step).
/// A maximum at the top of the range means no cap: cpuinfo_max_freq, which
/// includes the boost frequencies while boost is on.
pub fn cpu_apply(l: [u32; 4]) -> Res<()> {
    cpu_limits_valid(l)?;
    for (k, (c, pol)) in CLUSTERS.iter().enumerate() {
        let d = policy_dir(pol);
        let freqs = cluster_freqs(c);
        let top = *freqs.last().ok_or("no cpufreq")?;
        let mn = nearest_khz(&freqs, l[2 * k]).ok_or("no cpufreq")?;
        let mut mx = nearest_khz(&freqs, l[2 * k + 1]).ok_or("no cpufreq")?;
        if mx >= top {
            mx = sys::read_i64(&d.join("cpuinfo_max_freq")).map(|v| v as u32).unwrap_or(top).max(top);
        }
        let w = |f: &str, v: u32| sys::write(&d.join(f), &v.to_string()).map_err(|e| format!("{c} cluster {f}: {e}"));
        let cur_min = sys::read_i64(&d.join("scaling_min_freq")).unwrap_or(0) as u32;
        if mx < cur_min {
            w("scaling_min_freq", mn)?;
            w("scaling_max_freq", mx)?;
        } else {
            w("scaling_max_freq", mx)?;
            w("scaling_min_freq", mn)?;
        }
    }
    Ok(())
}

/// The limits in effect, MHz (from scaling_{min,max}_freq).
pub fn cpu_current() -> Option<[u32; 4]> {
    let mut out = [0u32; 4];
    for (k, (_, pol)) in CLUSTERS.iter().enumerate() {
        let d = policy_dir(pol);
        out[2 * k] = (sys::read_i64(&d.join("scaling_min_freq"))? / 1000) as u32;
        out[2 * k + 1] = (sys::read_i64(&d.join("scaling_max_freq"))? / 1000) as u32;
    }
    Some(out)
}

// ---------------------------------------------------------------- Wi-Fi power save

/// Wireless network interfaces (those with a `wireless` directory).
pub fn wifi_interfaces() -> Vec<String> {
    let base = sys::path("/sys/class/net");
    sys::list_dir(&base).into_iter().filter(|n| base.join(n).join("wireless").exists()).collect()
}

/// `iw` (nl80211): NetworkManager cannot change power saving on an active
/// connection, so the helper sets it on the interface directly.
pub fn iw() -> Option<PathBuf> {
    if std::env::var_os("TB323FU_SYSFS_ROOT").is_some_and(|r| !r.is_empty()) {
        return Some(sys::path("/usr/sbin/iw")).filter(|p| p.exists());
    }
    ["/usr/sbin/iw", "/sbin/iw", "/usr/bin/iw", "/run/current-system/sw/bin/iw"].iter().map(PathBuf::from).find(|p| p.exists())
}

pub fn wifi_power_save(iface: &str) -> Option<bool> {
    let out = std::process::Command::new(iw()?).args(["dev", iface, "get", "power_save"]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.split(':').nth(1).map(|v| v.trim() == "on")
}

pub fn set_wifi_power_save(iface: &str, on: bool) -> Res<()> {
    let iw = iw().ok_or("iw not installed")?;
    let st = std::process::Command::new(iw)
        .args(["dev", iface, "set", "power_save", if on { "on" } else { "off" }])
        .status()
        .map_err(|e| format!("iw: {e}"))?;
    if st.success() { Ok(()) } else { Err(format!("iw: {iface}: power_save failed")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt() -> Vec<(u32, i64)> {
        (1..=8).map(|i| (i, 42_000 + i as i64 * 1000)).collect()
    }

    #[test]
    fn trip_plan_order_and_bounds() {
        let base = dt();
        // default -> performance: raise, top trip first
        let p = plan_trips(&base, &base, 8_000).unwrap();
        assert_eq!(p.first(), Some(&(8, 58_000)));
        assert_eq!(p.last(), Some(&(1, 51_000)));
        // performance -> quiet: lower, bottom trip first
        let perf: Vec<_> = base.iter().map(|(i, t)| (*i, t + 8_000)).collect();
        let p = plan_trips(&base, &perf, -3_000).unwrap();
        assert_eq!(p.first(), Some(&(1, 40_000)));
        assert_eq!(p.last(), Some(&(8, 47_000)));
        // never above 58 °C
        assert!(plan_trips(&base, &base, 9_000).is_err());
        // a base that is already offset (or a chip zone) is refused
        let hot: Vec<_> = base.iter().map(|(i, t)| (*i, t + 50_000)).collect();
        assert!(plan_trips(&hot, &hot, 0).is_err());
        let unordered = vec![(1, 45_000), (2, 44_000)];
        assert!(plan_trips(&unordered, &unordered, 0).is_err());
        assert!(plan_trips(&base, &base[..3], 0).is_err());
        assert_eq!(match_profile(&base, &perf), Some("performance"));
        assert_eq!(match_profile(&base, &base), Some("default"));
        assert_eq!(match_profile(&base, &[(1, 1)]), None);
    }

    #[test]
    fn guard() {
        assert_eq!(performance_guard(&[30_000, 45_200], Some(40_000), Some(58_000)).as_deref(), Some("battery at 45.2 °C"));
        assert_eq!(performance_guard(&[30_000], Some(60_000), Some(58_000)).as_deref(), Some("board at 60.0 °C"));
        assert_eq!(performance_guard(&[30_000], Some(59_000), Some(58_000)), None);
        assert_eq!(performance_guard(&[], None, None), None);
    }

    #[test]
    fn panel_hysteresis() {
        assert!(!panel_should_limit(54_900, false));
        assert!(panel_should_limit(55_000, false));
        assert!(panel_should_limit(53_000, true));
        assert!(!panel_should_limit(51_900, true));
        assert_eq!(panel_cap(4095), 2858);
    }

    fn mk(r: &Path, rel: &str, v: &str) {
        let p = r.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, format!("{v}\n")).unwrap();
    }

    #[test]
    fn profile_on_fake_sysfs() {
        let _g = crate::sys::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let r = std::env::temp_dir().join(format!("tb323fu-perf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        let z = "sys/class/thermal/thermal_zone65";
        mk(&r, &format!("{z}/type"), "quiet-thermal");
        mk(&r, &format!("{z}/temp"), "30000");
        mk(&r, &format!("{z}/trip_point_0_type"), "hot");
        mk(&r, &format!("{z}/trip_point_0_temp"), "80000");
        for i in 1..=8 {
            mk(&r, &format!("{z}/trip_point_{i}_type"), "passive");
            mk(&r, &format!("{z}/trip_point_{i}_temp"), &(42_000 + i * 1000).to_string());
        }
        let c = "sys/class/thermal/thermal_zone1";
        mk(&r, &format!("{c}/type"), "cpu-0-0-0-thermal");
        mk(&r, &format!("{c}/trip_point_0_type"), "passive");
        mk(&r, &format!("{c}/trip_point_0_temp"), "95000");
        mk(&r, "proc/sys/kernel/random/boot_id", "boot-a");
        std::env::set_var("TB323FU_SYSFS_ROOT", &r);
        assert_eq!(thermal_profile().as_deref(), Some("default"));
        apply_thermal_profile("performance").unwrap();
        assert_eq!(sys::read_opt(&r.join(format!("{z}/trip_point_8_temp"))).as_deref(), Some("58000"));
        assert_eq!(sys::read_opt(&r.join(format!("{z}/trip_point_0_temp"))).as_deref(), Some("80000"));
        assert_eq!(sys::read_opt(&r.join(format!("{c}/trip_point_0_temp"))).as_deref(), Some("95000"));
        // a restarted daemon in the same boot keeps the recorded base
        assert_eq!(thermal_profile().as_deref(), Some("performance"));
        apply_thermal_profile("quiet").unwrap();
        assert_eq!(thermal_trips().first(), Some(&40.0));
        // new boot with offset trips (cannot happen on the device): refused
        mk(&r, "proc/sys/kernel/random/boot_id", "boot-b");
        mk(&r, &format!("{z}/trip_point_1_temp"), "60000");
        assert!(apply_thermal_profile("default").is_err());
        std::env::remove_var("TB323FU_SYSFS_ROOT");
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn cpu_on_fake_sysfs() {
        let _g = crate::sys::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let r = std::env::temp_dir().join(format!("tb323fu-cpu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        let p0 = "sys/devices/system/cpu/cpufreq/policy0";
        let p6 = "sys/devices/system/cpu/cpufreq/policy6";
        mk(&r, &format!("{p0}/scaling_available_frequencies"), "384000 1996800 3628800");
        mk(&r, &format!("{p0}/scaling_min_freq"), "384000");
        mk(&r, &format!("{p0}/scaling_max_freq"), "3628800");
        mk(&r, &format!("{p0}/cpuinfo_max_freq"), "3628800");
        mk(&r, &format!("{p6}/scaling_available_frequencies"), "768000 2880000 4396800");
        mk(&r, &format!("{p6}/scaling_min_freq"), "768000");
        mk(&r, &format!("{p6}/scaling_max_freq"), "4608000");
        mk(&r, &format!("{p6}/cpuinfo_max_freq"), "4608000");
        std::env::set_var("TB323FU_SYSFS_ROOT", &r);
        assert_eq!(cpu_full_range(), Some([384, 3628, 768, 4396]));
        cpu_apply([384, 1996, 768, 2880]).unwrap();
        assert_eq!(cpu_current(), Some([384, 1996, 768, 2880]));
        // top of the range = no cap: boost frequencies stay reachable
        cpu_apply([1996, 3628, 768, 4396]).unwrap();
        assert_eq!(cpu_current(), Some([1996, 3628, 768, 4608]));
        assert!(cpu_apply([3628, 3628, 768, 4396]).is_err(), "floor above 1996 MHz");
        assert!(cpu_apply([384, 3628, 768, 5000]).is_err());
        std::env::remove_var("TB323FU_SYSFS_ROOT");
        let _ = std::fs::remove_dir_all(&r);
    }
}
