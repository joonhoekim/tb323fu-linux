// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! Compressed swap in RAM: the platform's `tb323fu-zram.service` sets it up
//! from `/etc/tb323fu/zram.conf`; the helper writes that file and reads the
//! state back from /proc and sysfs.

use crate::config::shell_vars;
use crate::sys;
use std::fs;
use std::io;

pub const ZRAM_CONF: &str = "/etc/tb323fu/zram.conf";
pub const ZRAM_UNIT: &str = "tb323fu-zram.service";
/// Written by the platform script: the device it set up.
pub const ZRAM_STATE: &str = "/run/tb323fu-zram/device";
pub const SIZE_MIN_MIB: u32 = 256;
pub const SIZE_MAX_MIB: u32 = 65536;

/// The settings of `zram.conf` (defaults when the file or a key is absent).
#[derive(Debug, Clone, PartialEq)]
pub struct ZramConf {
    pub enabled: bool,
    /// uncompressed size, MiB; 0 = the RAM size
    pub size_mib: u32,
    pub algorithm: String,
}

impl Default for ZramConf {
    fn default() -> Self {
        ZramConf { enabled: true, size_mib: 0, algorithm: "zstd".into() }
    }
}

impl ZramConf {
    pub fn read() -> ZramConf {
        let m = shell_vars(&sys::path(ZRAM_CONF));
        let d = ZramConf::default();
        ZramConf {
            enabled: m.get("ENABLED").map(|v| !matches!(v.as_str(), "0" | "no" | "false" | "off")).unwrap_or(d.enabled),
            size_mib: match m.get("SIZE").map(String::as_str) {
                None | Some("ram") | Some("") => 0,
                Some(v) => v.parse().unwrap_or(0),
            },
            algorithm: m.get("ALGORITHM").filter(|a| !a.is_empty()).cloned().unwrap_or(d.algorithm),
        }
    }

    pub fn render(&self) -> String {
        let size = if self.size_mib == 0 { "ram".to_string() } else { self.size_mib.to_string() };
        format!(
            "# Compressed swap in RAM (tb323fu-zram.service); written by tb323fu-helperd\n\
             # ENABLED=1|0, SIZE=ram|MiB (uncompressed), ALGORITHM (zstd, lz4, ...)\n\
             ENABLED={}\nSIZE={size}\nALGORITHM={}\n",
            u8::from(self.enabled),
            self.algorithm
        )
    }

    /// Write the file when its content differs; true when it was written.
    pub fn write(&self) -> io::Result<bool> {
        let p = sys::path(ZRAM_CONF);
        let body = self.render();
        if fs::read_to_string(&p).is_ok_and(|s| s == body) {
            return Ok(false);
        }
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = p.with_extension("conf.tmp");
        fs::write(&tmp, body)?;
        fs::rename(tmp, &p)?;
        Ok(true)
    }
}

pub fn valid_size(mib: u32) -> bool {
    mib == 0 || (SIZE_MIN_MIB..=SIZE_MAX_MIB).contains(&mib)
}

/// What is in effect now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZramStatus {
    /// the zram swap device ("/dev/zram0"), if any
    pub device: Option<String>,
    /// "tb323fu" (the platform service), "other" (e.g. zram-generator), "" none
    pub source: String,
    pub algorithm: String,
    pub disk_size: u64,
    /// uncompressed bytes stored, compressed bytes, memory used in total
    pub orig_data: u64,
    pub compr_data: u64,
    pub mem_used: u64,
    pub swappiness: Option<u32>,
    pub page_cluster: Option<u32>,
}

impl ZramStatus {
    pub fn ratio(&self) -> f64 {
        if self.compr_data == 0 { 0.0 } else { self.orig_data as f64 / self.compr_data as f64 }
    }
}

/// zram devices among the active swaps of a /proc/swaps text.
pub fn zram_swaps(proc_swaps: &str) -> Vec<String> {
    proc_swaps
        .lines()
        .skip(1)
        .filter_map(|l| l.split_whitespace().next())
        .filter(|d| d.strip_prefix("/dev/zram").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())))
        .map(str::to_string)
        .collect()
}

/// (orig_data_size, compr_data_size, mem_used_total) of an mm_stat line.
pub fn parse_mm_stat(s: &str) -> Option<(u64, u64, u64)> {
    let v: Vec<u64> = s.split_whitespace().take(3).map(|x| x.parse().ok()).collect::<Option<_>>()?;
    (v.len() == 3).then(|| (v[0], v[1], v[2]))
}

pub fn status() -> ZramStatus {
    let swaps = fs::read_to_string(sys::path("/proc/swaps")).unwrap_or_default();
    let mut st = ZramStatus {
        swappiness: sys::read_i64(&sys::path("/proc/sys/vm/swappiness")).map(|v| v as u32),
        page_cluster: sys::read_i64(&sys::path("/proc/sys/vm/page-cluster")).map(|v| v as u32),
        ..Default::default()
    };
    let ours = sys::read_opt(&sys::path(ZRAM_STATE));
    let devs = zram_swaps(&swaps);
    let Some(dev) = devs.iter().find(|d| Some(d.as_str()) == ours.as_deref()).or(devs.first()).cloned() else {
        return st;
    };
    st.source = if Some(&dev) == ours.as_ref() { "tb323fu" } else { "other" }.into();
    let blk = sys::path(&format!("/sys/block/{}", dev.trim_start_matches("/dev/")));
    st.algorithm = sys::read_opt(&blk.join("comp_algorithm")).and_then(|a| sys::selected(&a).or(Some(a))).unwrap_or_default();
    st.disk_size = sys::read_i64(&blk.join("disksize")).unwrap_or(0) as u64;
    if let Some((o, c, m)) = sys::read_opt(&blk.join("mm_stat")).as_deref().and_then(parse_mm_stat) {
        (st.orig_data, st.compr_data, st.mem_used) = (o, c, m);
    }
    st.device = Some(dev);
    st
}

/// NixOS sets zram up from its configuration (`services.tb323fu.zram`).
pub fn declarative() -> bool {
    sys::path("/etc/NIXOS").exists()
}

/// The platform unit is installed.
pub fn unit_installed() -> bool {
    ["/usr/lib/systemd/system", "/lib/systemd/system", "/etc/systemd/system"]
        .iter()
        .any(|d| sys::path(&format!("{d}/{ZRAM_UNIT}")).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps() {
        let s = "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n\
                 /swapfile                               file\t\t8388604\t\t0\t\t-1\n\
                 /dev/zram0                              partition\t15487392\t0\t\t100\n\
                 /dev/zram                               partition\t1\t0\t\t1\n";
        assert_eq!(zram_swaps(s), vec!["/dev/zram0"]);
        assert!(zram_swaps("Filename Type Size Used Priority\n").is_empty());
    }

    #[test]
    fn mm_stat() {
        assert_eq!(parse_mm_stat("  121896960 27118593 29143040        0 29143040     1543        0        0        0\n"),
            Some((121896960, 27118593, 29143040)));
        assert_eq!(parse_mm_stat("1 2"), None);
        assert_eq!(parse_mm_stat(""), None);
    }

    #[test]
    fn conf_and_status() {
        let _g = sys::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("tb323fu-zram-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("TB323FU_SYSFS_ROOT", &root);

        assert_eq!(ZramConf::read(), ZramConf::default());
        let c = ZramConf { enabled: false, size_mib: 4096, algorithm: "lz4".into() };
        assert!(c.write().unwrap());
        assert!(!c.write().unwrap());
        assert_eq!(ZramConf::read(), c);
        fs::write(root.join("etc/tb323fu/zram.conf"), "ENABLED=yes\nSIZE=ram\n").unwrap();
        assert_eq!(ZramConf::read(), ZramConf::default());

        assert_eq!(status().device, None);
        let w = |p: &str, s: &str| {
            let p = root.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, s).unwrap();
        };
        w("proc/swaps", "Filename Type Size Used Priority\n/dev/zram1 partition 1048572 0 100\n");
        w("sys/block/zram1/comp_algorithm", "lzo lz4 [zstd]\n");
        w("sys/block/zram1/disksize", "1073741824\n");
        w("sys/block/zram1/mm_stat", "4500 1000 1200 0 1200 0 0 0 0\n");
        w("proc/sys/vm/swappiness", "100\n");
        let s = status();
        assert_eq!((s.device.as_deref(), s.source.as_str(), s.algorithm.as_str()), (Some("/dev/zram1"), "other", "zstd"));
        assert_eq!((s.disk_size, s.orig_data, s.compr_data, s.swappiness, s.page_cluster), (1073741824, 4500, 1000, Some(100), None));
        assert!((s.ratio() - 4.5).abs() < 1e-9);
        w("run/tb323fu-zram/device", "/dev/zram1\n");
        assert_eq!(status().source, "tb323fu");

        std::env::remove_var("TB323FU_SYSFS_ROOT");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sizes() {
        assert!(valid_size(0) && valid_size(256) && valid_size(16384));
        assert!(!valid_size(100) && !valid_size(70000));
    }
}
