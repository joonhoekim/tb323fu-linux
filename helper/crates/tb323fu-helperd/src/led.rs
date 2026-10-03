// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! The LED ring: one thread owns every write (charge indicator, solid colour,
//! breathing, notification pulses), so effects never race the setters.
//! Breathing runs on the LED chip (pattern trigger, `hw_pattern`) where the
//! kernel offers it, else it is drawn here frame by frame.

use crate::Shared;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tb323fu_helper_core::features as f;
use tb323fu_helper_core::perf;

#[derive(Default)]
struct Req {
    force: bool,
    pulse: Option<([u32; 3], u32)>,
}

#[derive(Default)]
pub struct Led {
    req: Mutex<Req>,
    cv: Condvar,
}

const FRAME: Duration = Duration::from_millis(80);
const IDLE: Duration = Duration::from_secs(5);
const PULSE_ON: Duration = Duration::from_millis(250);
const PULSE_OFF: Duration = Duration::from_millis(250);

impl Led {
    /// Redraw now (a setting changed).
    pub fn kick(&self) {
        self.req.lock().unwrap().force = true;
        self.cv.notify_all();
    }

    /// Blink `count` times in `color`, then go back to the mode.
    pub fn pulse(&self, color: [u32; 3], count: u32) {
        self.req.lock().unwrap().pulse = Some((color, count));
        self.cv.notify_all();
    }
}

pub fn spawn(sh: Arc<Shared>) {
    std::thread::spawn(move || run(sh));
}

fn run(sh: Arc<Shared>) {
    let start = Instant::now();
    // (colour, brightness) last written; the charge colour, re-read every 5 s
    let mut last: Option<(Option<[u32; 3]>, u32)> = None;
    let mut charge: (Option<Instant>, Option<[u32; 3]>) = (None, None);
    // (colour, brightness, cycle) of the breathing the chip runs, and of the
    // last one it refused (drawn in software instead)
    let mut hw: Option<([u32; 3], u32, u32)> = None;
    let mut hw_refused: Option<([u32; 3], u32, u32)> = None;
    loop {
        let (force, pulse) = {
            let mut r = sh.led.req.lock().unwrap();
            (std::mem::take(&mut r.force), r.pulse.take())
        };
        let cfg = sh.cfg();
        let c = &cfg.ledring;
        if let Some((color, count)) = pulse {
            for _ in 0..count.min(10) {
                let _ = f::ledring_apply(Some(color), c.brightness.max(1));
                std::thread::sleep(PULSE_ON);
                let _ = f::ledring_apply(None, 0);
                std::thread::sleep(PULSE_OFF);
            }
            last = None;
            hw = None;
        }
        if force || charge.0.is_none_or(|t| t.elapsed() >= IDLE) {
            charge = (Some(Instant::now()), f::battery_info().ok().and_then(|i| f::ledring_color(&i, c.low_percent, cfg.battery.bypass)));
        }
        let own = f::parse_color(&c.color).ok();
        let overridden = c.charge_override && charge.1.is_some();
        if let (Some(col), "breathe", false) = (own, c.mode.as_str(), overridden) {
            let key = (col, c.brightness, c.speed);
            if hw != Some(key) || force || pulse.is_some() {
                hw = None;
                if hw_refused != Some(key) && f::ledring_hw_pattern_available() {
                    match f::ledring_hw_breathe(col, c.brightness, c.speed) {
                        Ok(()) => hw = Some(key),
                        Err(e) => {
                            eprintln!("tb323fu-helperd: {e}; breathing in software");
                            hw_refused = Some(key);
                        }
                    }
                }
            }
            if hw.is_some() {
                last = None;
                let g = sh.led.req.lock().unwrap();
                let _ = sh.led.cv.wait_timeout_while(g, IDLE, |r| !r.force && r.pulse.is_none());
                continue;
            }
        } else if hw.take().is_some() {
            let _ = f::ledring_trigger_clear();
            last = None;
        }
        let (color, animate) = match c.mode.as_str() {
            "charge" => (charge.1, false),
            "solid" | "breathe" if overridden => (charge.1, false),
            "solid" => (own, false),
            "breathe" => (own, !perf::screen_off()),
            _ => (None, false),
        };
        let bright = if animate { f::breathe_level(c.brightness, start.elapsed().as_millis() as u64, c.speed) } else { c.brightness };
        let want = (color, bright);
        if pulse.is_some() || force || last != Some(want) {
            let res = match (&last, color) {
                (Some((lc, _)), Some(_)) if *lc == color && !force => f::ledring_brightness(bright),
                _ => f::ledring_apply(color, bright),
            };
            if let Err(e) = res {
                eprintln!("tb323fu-helperd: {e}");
            }
            last = Some(want);
        }
        let wait = if animate { FRAME } else { IDLE };
        let g = sh.led.req.lock().unwrap();
        let _ = sh.led.cv.wait_timeout_while(g, wait, |r| !r.force && r.pulse.is_none());
    }
}
