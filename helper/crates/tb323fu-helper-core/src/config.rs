// SPDX-License-Identifier: MIT
//! Persistent settings: `/etc/tb323fu/helper.toml` (override with
//! `TB323FU_CONFIG`). Missing file or keys mean defaults. On the first start
//! without a file, values are migrated once from the legacy
//! `/etc/baldur/{ledring,gpu}.conf` shell-style files.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_PATH: &str = "/etc/tb323fu/helper.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub battery: Battery,
    pub android: Android,
    pub torch: Torch,
    pub ledring: LedRing,
    pub refresh: Refresh,
    pub gpu: Gpu,
    pub usb: Usb,
    pub kernel: Kernel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Battery {
    /// charge_control_end_threshold, percent; applied at every start
    pub charge_limit: u32,
    /// bypass mode: the limit was lowered to the capacity at the time
    pub bypass: bool,
    /// the limit to restore when bypass is switched off
    pub saved_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Android {
    pub require_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Torch {
    pub level: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LedRing {
    /// "charge" (charge-state indicator) or "off"
    pub mode: String,
    pub brightness: u32,
    pub low_percent: u32,
}

/// Idle refresh (kernel `msm.idle_refresh_*`): `None` = leave the kernel's value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Refresh {
    pub policy: Option<String>,
    pub hz: Option<u32>,
    pub ms60: Option<u32>,
    pub ms30: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Gpu {
    /// follow power-profiles-daemon's ActiveProfile when it runs
    pub follow_power_profiles: bool,
    /// profile used when not following (or no power-profiles-daemon)
    pub profile: String,
    /// profile -> [min_mhz, max_mhz]
    pub floors: BTreeMap<String, [u32; 2]>,
}

/// Kernel updates (docs/notes/kernel-updates-design.md).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Kernel {
    /// "stable" or "testing"
    pub channel: String,
    /// the signed index (https, http or file URL); a PC-served one for tests
    pub index_url: String,
    /// check the index once a day (never downloads by itself)
    pub auto_check: bool,
    /// show when a newer helper is published (the index's helper.latest)
    pub helper_notify: bool,
}

pub const DEFAULT_INDEX_URL: &str = "https://joonhoekim.github.io/tb323fu-linux/kernel/index.json";

impl Default for Kernel {
    fn default() -> Self {
        Kernel { channel: "stable".into(), index_url: DEFAULT_INDEX_URL.into(), auto_check: true, helper_notify: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Usb {
    /// USB host wakeup from suspend (off by default: see the platform udev rule)
    pub wake: bool,
    /// USB gadget (network + serial console): `None` = leave as booted
    pub dev_mode: Option<bool>,
}

impl Default for Battery {
    fn default() -> Self {
        Battery { charge_limit: 80, bypass: false, saved_limit: None }
    }
}
impl Default for Torch {
    fn default() -> Self {
        Torch { level: 96 }
    }
}
impl Default for LedRing {
    fn default() -> Self {
        LedRing { mode: "charge".into(), brightness: 40, low_percent: 15 }
    }
}
impl Default for Gpu {
    fn default() -> Self {
        let mut floors = BTreeMap::new();
        floors.insert("power-saver".into(), [160, 726]);
        floors.insert("balanced".into(), [160, 1200]);
        floors.insert("performance".into(), [461, 1200]);
        Gpu { follow_power_profiles: true, profile: "balanced".into(), floors }
    }
}
impl Default for Config {
    fn default() -> Self {
        Config {
            battery: Battery::default(),
            android: Android::default(),
            torch: Torch::default(),
            ledring: LedRing::default(),
            refresh: Refresh::default(),
            gpu: Gpu::default(),
            usb: Usb::default(),
            kernel: Kernel::default(),
        }
    }
}

pub fn config_path() -> PathBuf {
    std::env::var_os("TB323FU_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_PATH))
}

impl Config {
    /// Load the config; when the file does not exist yet, start from the
    /// defaults plus the legacy /etc/baldur values (returns `migrated = true`).
    pub fn load(path: &Path) -> (Config, bool) {
        match fs::read_to_string(path) {
            Ok(s) => (toml::from_str(&s).unwrap_or_default(), false),
            Err(_) => {
                let mut c = Config::default();
                c.migrate_legacy();
                (c, true)
            }
        }
    }

    /// Write atomically (tmp + rename), creating the directory.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = toml::to_string_pretty(self).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, format!("# tb323fu-helperd settings (written by the daemon)\n{body}"))?;
        fs::rename(tmp, path)
    }

    fn migrate_legacy(&mut self) {
        let led = shell_vars(&crate::sys::path("/etc/baldur/ledring.conf"));
        if let Some(v) = led.get("BRIGHTNESS").and_then(|v| v.parse().ok()) {
            self.ledring.brightness = v;
        }
        if let Some(v) = led.get("LOW").and_then(|v| v.parse().ok()) {
            self.ledring.low_percent = v;
        }
        let gpu = shell_vars(&crate::sys::path("/etc/baldur/gpu.conf"));
        for (name, key) in [("power-saver", "POWER_SAVER"), ("balanced", "BALANCED"), ("performance", "PERFORMANCE")] {
            let lo = gpu.get(&format!("{key}_MIN")).and_then(|v| v.parse().ok());
            let hi = gpu.get(&format!("{key}_MAX")).and_then(|v| v.parse().ok());
            if let (Some(lo), Some(hi)) = (lo, hi) {
                self.gpu.floors.insert(name.into(), [lo, hi]);
            }
        }
    }
}

/// `KEY=value` lines of a shell-style config (comments and blanks ignored).
pub fn shell_vars(path: &Path) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Ok(s) = fs::read_to_string(path) {
        for line in s.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            for part in l.split_whitespace() {
                if let Some((k, v)) = part.split_once('=') {
                    m.insert(k.to_string(), v.trim_matches('"').to_string());
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let dir = std::env::temp_dir().join(format!("tb323fu-cfg-{}", std::process::id()));
        let p = dir.join("helper.toml");
        let mut c = Config::default();
        c.battery.charge_limit = 70;
        c.refresh.ms60 = Some(2000);
        c.save(&p).unwrap();
        let (d, migrated) = Config::load(&p);
        assert!(!migrated);
        assert_eq!(c, d);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn shell_parse() {
        let dir = std::env::temp_dir().join(format!("tb323fu-sh-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.conf");
        fs::write(&p, "# c\nPOWER_SAVER_MIN=160 POWER_SAVER_MAX=726\nLOW=\"15\"\n").unwrap();
        let m = shell_vars(&p);
        assert_eq!(m["POWER_SAVER_MAX"], "726");
        assert_eq!(m["LOW"], "15");
        let _ = fs::remove_dir_all(dir);
    }
}
