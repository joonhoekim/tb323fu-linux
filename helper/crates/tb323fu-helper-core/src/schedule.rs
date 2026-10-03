// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! "Full by" charging: the charge limit is raised to 100 % early enough to
//! be full at a set time on chosen days, and lowered again two hours after
//! that time (or at once when unplugged after it). Times are minutes in the
//! local week, Monday 00:00 = 0.

use crate::features::Res;

pub const WEEK: i32 = 7 * 1440;
pub const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
/// How long after the target time the limit stays at 100 % while plugged in.
pub const HOLD_AFTER_MIN: i32 = 120;

/// "07:30" -> 450.
pub fn parse_hhmm(s: &str) -> Res<i32> {
    let (h, m) = s.split_once(':').ok_or("time must be HH:MM")?;
    let (h, m): (i32, i32) = (h.parse().map_err(|_| "time must be HH:MM")?, m.parse().map_err(|_| "time must be HH:MM")?);
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return Err("time must be HH:MM (00:00..23:59)".into());
    }
    Ok(h * 60 + m)
}

pub fn valid_days(days: &[String]) -> Res<()> {
    match days.iter().find(|d| !DAYS.contains(&d.as_str())) {
        Some(d) => Err(format!("unknown day {d:?} (mon..sun)")),
        None => Ok(()),
    }
}

/// Minutes from `now` (week minute) to the nearest scheduled target, in
/// [-HOLD_AFTER_MIN, WEEK - HOLD_AFTER_MIN). No days = every day.
pub fn until_target(now: i32, target_min: i32, days: &[String]) -> Option<i32> {
    (0..7)
        .filter(|d| days.is_empty() || days.iter().any(|x| x == DAYS[*d as usize]))
        .map(|d| {
            let t = d * 1440 + target_min;
            (t - now + HOLD_AFTER_MIN).rem_euclid(WEEK) - HOLD_AFTER_MIN
        })
        .min()
}

/// Minutes from `now` to a stored target week minute, in (-WEEK/2, WEEK/2].
pub fn delta_to(now: i32, target: i32) -> i32 {
    let d = (target - now).rem_euclid(WEEK);
    if d > WEEK / 2 { d - WEEK } else { d }
}

/// Charging time estimate to 100 %, minutes: the remaining charge from
/// `charge_counter` and the capacity, at 85 % of the measured input power
/// (at least 10 W) into a 4.2 V cell, ×1.5 for the constant-voltage taper,
/// plus 30 min margin; 30..480.
pub fn charge_minutes(capacity: u32, counter_mah: u32, input_mw: u32) -> i32 {
    let remaining = if capacity > 0 && counter_mah > 0 {
        counter_mah as f64 * (100 - capacity.min(100)) as f64 / capacity as f64
    } else {
        8600.0 * (100 - capacity.min(100)) as f64 / 100.0
    };
    let ma = (input_mw.max(10_000) as f64 * 0.85 / 4.2).min(6000.0);
    let m = remaining / ma * 60.0 * 1.5 + 30.0;
    (m.round() as i32).clamp(30, 480)
}

#[repr(C)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: std::os::raw::c_long,
    tm_zone: *const std::os::raw::c_char,
}

extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
}

/// Now as a local week minute. Under a fake device root a file
/// `/run/tb323fu-fake-clock` ("DAY HH:MM") stands in (tests).
pub fn now_week_minute() -> Option<i32> {
    if std::env::var_os("TB323FU_SYSFS_ROOT").is_some_and(|r| !r.is_empty()) {
        if let Some(s) = crate::sys::read_opt(&crate::sys::path("/run/tb323fu-fake-clock")) {
            let (d, t) = s.split_once(' ')?;
            let d = DAYS.iter().position(|x| *x == d)? as i32;
            return Some(d * 1440 + parse_hhmm(t).ok()?);
        }
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    // SAFETY: localtime_r fills the caller's struct tm (the platform layout above)
    let mut tm: Tm = unsafe { std::mem::zeroed() };
    let r = unsafe { localtime_r(&now, &mut tm) };
    if r.is_null() {
        return None;
    }
    let wd = (tm.tm_wday + 6) % 7;
    Some(wd * 1440 + tm.tm_hour * 60 + tm.tm_min)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn days(d: &[&str]) -> Vec<String> {
        d.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn hhmm() {
        assert_eq!(parse_hhmm("07:30"), Ok(450));
        assert!(parse_hhmm("24:00").is_err());
        assert!(parse_hhmm("7").is_err());
        assert!(valid_days(&days(&["mon", "sun"])).is_ok());
        assert!(valid_days(&days(&["monday"])).is_err());
    }
    #[test]
    fn targets() {
        let seven = 7 * 60;
        // Monday 05:30, every day, 07:00 -> 90 min
        assert_eq!(until_target(5 * 60 + 30, seven, &[]), Some(90));
        // Monday 08:00: the Monday target passed 60 min ago (still within the hold)
        assert_eq!(until_target(8 * 60, seven, &[]), Some(-60));
        // Monday 10:00: next is Tuesday 07:00
        assert_eq!(until_target(10 * 60, seven, &[]), Some(21 * 60));
        // weekdays only, Friday 10:00 -> Monday 07:00
        let fri10 = 4 * 1440 + 600;
        assert_eq!(until_target(fri10, seven, &days(&["mon", "tue", "wed", "thu", "fri"])), Some(2 * 1440 + 21 * 60));
        // a target just after midnight, seen from Sunday 23:00 (wraps the week)
        assert_eq!(until_target(6 * 1440 + 23 * 60, 30, &days(&["mon"])), Some(90));
        assert_eq!(delta_to(6 * 1440 + 23 * 60, 30), 90);
        assert_eq!(delta_to(100, 30), -70);
    }
    #[test]
    fn estimate() {
        // 80 % of 8.58 Ah, 40 W PPS: about 2.1 Ah left at ~8 A -> ~24 min + 30
        let m = charge_minutes(80, 6861, 40_000);
        assert!((50..=60).contains(&m), "{m}");
        // unknown input: 10 W
        assert!(charge_minutes(80, 6861, 0) > m);
        assert_eq!(charge_minutes(100, 8576, 40_000), 30);
        assert_eq!(charge_minutes(1, 400, 0), 480);
    }
    #[test]
    fn local_clock() {
        let _g = crate::sys::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("TB323FU_SYSFS_ROOT");
        let w = now_week_minute().unwrap();
        assert!((0..WEEK).contains(&w));
    }
}
