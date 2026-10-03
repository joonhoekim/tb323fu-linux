// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The two AW86937 vibration motors (input devices "aw86927-haptics", force
//! feedback through ff-memless): the device gain (FF_GAIN, kept by the input
//! core for every application) and a short test rumble.

use crate::features::Res;
use crate::sys;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;

pub const DEVICE_NAME: &str = "aw86927-haptics";

const EV_FF: u16 = 0x15;
const FF_RUMBLE: u16 = 0x50;
const FF_GAIN: u16 = 0x60;
/// sizeof(struct ff_effect): the union holds a pointer (periodic custom data)
#[cfg(target_pointer_width = "64")]
const FF_EFFECT_SIZE: usize = 48;
#[cfg(not(target_pointer_width = "64"))]
const FF_EFFECT_SIZE: usize = 44;
/// _IOW('E', 0x80, struct ff_effect)
const EVIOCSFF: std::os::raw::c_ulong = (1 << 30) | ((FF_EFFECT_SIZE as std::os::raw::c_ulong) << 16) | (0x45 << 8) | 0x80;

extern "C" {
    fn ioctl(fd: std::os::raw::c_int, request: std::os::raw::c_ulong, ...) -> std::os::raw::c_int;
}

/// A motor: "left" (I2C 0x5b, as the device tree says) or "right", and its
/// event device node.
#[derive(Debug, Clone, PartialEq)]
pub struct Motor {
    pub name: String,
    pub node: PathBuf,
}

/// The motors, left first. Named by I2C address when the sysfs link shows it,
/// else by order.
pub fn motors() -> Vec<Motor> {
    let base = sys::path("/sys/class/input");
    let mut found: Vec<(Option<&'static str>, String)> = sys::list_dir(&base)
        .into_iter()
        .filter(|n| n.starts_with("event"))
        .filter(|n| sys::read_opt(&base.join(n).join("device/name")).as_deref() == Some(DEVICE_NAME))
        .map(|n| {
            let real = std::fs::canonicalize(base.join(&n).join("device")).map(|p| p.display().to_string()).unwrap_or_default();
            let side = if real.contains("-005b") { Some("left") } else if real.contains("-005a") { Some("right") } else { None };
            (side, n)
        })
        .collect();
    found.sort_by_key(|(s, n)| (s.map(|s| s != "left"), n.trim_start_matches("event").parse::<u32>().unwrap_or(0)));
    let mut out = Vec::new();
    for (i, (side, n)) in found.into_iter().enumerate() {
        let name = side.map(str::to_string).unwrap_or_else(|| if i == 0 { "left".into() } else { "right".into() });
        out.push(Motor { name, node: sys::path(&format!("/dev/input/{n}")) });
    }
    out
}

/// struct input_event for this ABI (time left zero: the kernel ignores it).
fn input_event(ty: u16, code: u16, value: i32) -> Vec<u8> {
    let tv = std::mem::size_of::<std::os::raw::c_long>() * 2;
    let mut b = vec![0u8; tv];
    b.extend_from_slice(&ty.to_ne_bytes());
    b.extend_from_slice(&code.to_ne_bytes());
    b.extend_from_slice(&value.to_ne_bytes());
    b
}

/// 0..100 % -> FF_GAIN 0..0xffff.
pub fn gain_of(strength: u32) -> i32 {
    (0xffff * strength.min(100) / 100) as i32
}

/// Set the gain of every motor.
pub fn set_strength(strength: u32) -> Res<()> {
    if strength > 100 {
        return Err("strength must be 0..100".into());
    }
    let m = motors();
    if m.is_empty() {
        return Err("no vibration motors".into());
    }
    for mo in m {
        let mut f = std::fs::OpenOptions::new().write(true).open(&mo.node).map_err(|e| format!("{}: {e}", mo.node.display()))?;
        f.write_all(&input_event(EV_FF, FF_GAIN, gain_of(strength))).map_err(|e| format!("{}: {e}", mo.node.display()))?;
    }
    Ok(())
}

/// A 0.3 s rumble on "left", "right" or "both" (at the current gain).
pub fn test(which: &str) -> Res<()> {
    let all = motors();
    let sel: Vec<&Motor> = all.iter().filter(|m| which == "both" || m.name == which).collect();
    if !["left", "right", "both"].contains(&which) {
        return Err("motor must be left, right or both".into());
    }
    if sel.is_empty() {
        return Err(format!("no {which} vibration motor"));
    }
    let mut open = Vec::new();
    for m in sel {
        let f = std::fs::OpenOptions::new().read(true).write(true).open(&m.node).map_err(|e| format!("{}: {e}", m.node.display()))?;
        let mut eff = [0u8; FF_EFFECT_SIZE];
        eff[0..2].copy_from_slice(&FF_RUMBLE.to_ne_bytes());
        eff[2..4].copy_from_slice(&(-1i16).to_ne_bytes());
        eff[10..12].copy_from_slice(&300u16.to_ne_bytes()); // replay.length, ms
        eff[16..18].copy_from_slice(&0xc000u16.to_ne_bytes()); // strong magnitude
        eff[18..20].copy_from_slice(&0xc000u16.to_ne_bytes()); // weak magnitude
        // SAFETY: EVIOCSFF reads and writes back (the id) a struct ff_effect of this size
        let r = unsafe { ioctl(f.as_raw_fd(), EVIOCSFF, eff.as_mut_ptr()) };
        if r < 0 {
            return Err(format!("{}: upload: {}", m.node.display(), std::io::Error::last_os_error()));
        }
        let id = i16::from_ne_bytes([eff[2], eff[3]]);
        open.push((f, id));
    }
    for (f, id) in open.iter_mut() {
        f.write_all(&input_event(EV_FF, *id as u16, 1)).map_err(|e| format!("play: {e}"))?;
    }
    std::thread::sleep(std::time::Duration::from_millis(350));
    // closing the device removes the uploaded effect
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ioctl_number_and_gain() {
        #[cfg(target_pointer_width = "64")]
        assert_eq!(EVIOCSFF, 0x4030_4580);
        assert_eq!(gain_of(100), 0xffff);
        assert_eq!(gain_of(50), 0x7fff);
        assert_eq!(gain_of(0), 0);
        assert_eq!(input_event(EV_FF, FF_GAIN, 7).len(), std::mem::size_of::<std::os::raw::c_long>() * 2 + 8);
    }
    #[test]
    fn motors_on_fake_sysfs() {
        let _g = crate::sys::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let r = std::env::temp_dir().join(format!("tb323fu-hap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        for (n, name) in [("event5", DEVICE_NAME), ("event6", DEVICE_NAME), ("event2", "gpio-keys")] {
            let d = r.join(format!("sys/class/input/{n}/device"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("name"), format!("{name}\n")).unwrap();
        }
        std::fs::create_dir_all(r.join("dev/input")).unwrap();
        std::fs::write(r.join("dev/input/event5"), b"").unwrap();
        std::fs::write(r.join("dev/input/event6"), b"").unwrap();
        std::env::set_var("TB323FU_SYSFS_ROOT", &r);
        let m = motors();
        assert_eq!(m.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["left", "right"]);
        set_strength(50).unwrap();
        let ev = std::fs::read(r.join("dev/input/event6")).unwrap();
        assert_eq!(&ev[ev.len() - 8..], &[&EV_FF.to_ne_bytes()[..], &FF_GAIN.to_ne_bytes(), &0x7fffi32.to_ne_bytes()].concat()[..]);
        assert!(set_strength(101).is_err());
        assert!(test("middle").is_err());
        std::env::remove_var("TB323FU_SYSFS_ROOT");
        let _ = std::fs::remove_dir_all(&r);
    }
}
