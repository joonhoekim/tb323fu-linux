// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
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
    pub cpu: Cpu,
    pub thermal: Thermal,
    pub wifi: Wifi,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cpu {
    /// cpufreq boost: the fast cores' top frequencies (4.512 and 4.608 GHz on
    /// the TB323FU, which Android uses); applied at every start
    pub boost: bool,
    /// performance profile -> [little_min, little_max, big_min, big_max] MHz;
    /// a profile without an entry uses the whole range
    pub limits: BTreeMap<String, [u32; 4]>,
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu { boost: true, limits: BTreeMap::new() }
    }
}

/// Board-temperature profile and the panel heat limit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Thermal {
    /// "quiet", "default" (the device tree's steps) or "performance"
    pub profile: String,
    /// follow the performance profile instead: power-saver -> quiet,
    /// balanced -> default, performance -> performance
    pub follow_performance: bool,
    /// switch Bypass on while the thermal profile is performance on external
    /// power (Linux has no temperature-based charge-current reduction)
    pub performance_bypass: bool,
    /// hold the backlight at 178/255 while the panel is at 55 °C or more
    pub panel_limit: bool,
    /// Bypass was switched on by performance_bypass (restored on leaving)
    pub bypass_auto: bool,
}

impl Default for Thermal {
    fn default() -> Self {
        Thermal { profile: "default".into(), follow_performance: false, performance_bypass: true, panel_limit: true, bypass_auto: false }
    }
}

/// Wi-Fi power saving, set on the interface (iw) where the NetworkManager
/// connection leaves it alone; connection profiles are never changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Wifi {
    /// turn Wi-Fi power saving off while the performance profile is in effect
    pub low_latency_performance: bool,
}

/// Kernel updates (docs/notes/kernel-updates-design.md).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Kernel {
    /// "stable" (the newest release) or "testing" (the newest release or
    /// pre-release)
    pub channel: String,
    /// where releases come from: "github:OWNER/REPO" (GitHub Releases)
    pub source: String,
    /// the GitHub REST API base; another URL (http, file) only for tests
    pub api_url: String,
    /// look once a day (never downloads by itself)
    pub auto_check: bool,
    /// show when a newer helper is published (a `helper-vX.Y.Z` release)
    pub helper_notify: bool,
    /// also require a minisign signature over SHA256SUMS (SHA256SUMS.minisig)
    /// from one of `public_keys` or /etc/tb323fu/keys/kernel-*.pub; off by
    /// default: SHA256SUMS alone only catches transfer errors
    pub require_signature: bool,
    /// minisign public keys (the base64 line of a .pub file)
    pub public_keys: Vec<String>,
}

pub const DEFAULT_SOURCE: &str = "github:joonhoekim/tb323fu-linux";
pub const DEFAULT_API_URL: &str = "https://api.github.com";

impl Default for Kernel {
    fn default() -> Self {
        Kernel {
            channel: "stable".into(),
            source: DEFAULT_SOURCE.into(),
            api_url: DEFAULT_API_URL.into(),
            auto_check: true,
            helper_notify: true,
            require_signature: false,
            public_keys: Vec::new(),
        }
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
            cpu: Cpu::default(),
            thermal: Thermal::default(),
            wifi: Wifi::default(),
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
        c.kernel.public_keys = vec!["RWQx".into()];
        c.save(&p).unwrap();
        let (d, migrated) = Config::load(&p);
        assert!(!migrated);
        assert_eq!(c, d);
        // a file from the signed-index helper (index_url) still loads
        fs::write(&p, "[kernel]\nchannel = \"testing\"\nindex_url = \"https://x/index.json\"\n").unwrap();
        let (d, _) = Config::load(&p);
        assert_eq!((d.kernel.channel.as_str(), d.kernel.source.as_str()), ("testing", DEFAULT_SOURCE));
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
