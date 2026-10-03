// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! tb323fu-ctl: command-line client for tb323fu-helperd.

use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

const BUS: &str = "io.github.joonhoekim.OpenDeviceHelper1";
const ROOT: &str = "/io/github/joonhoekim/OpenDeviceHelper1";
const OBJECTS: [&str; 12] = ["Battery", "Android", "Torch", "LedRing", "Refresh", "Gpu", "Usb", "EmergencyKey", "Diagnostics", "Boot", "Thermal", "Kernel"];

const USAGE: &str = "usage: tb323fu-ctl [--json] [--session] COMMAND

  status                         all features
  battery                        battery, charger and charge limit
  charge-limit [PERCENT]         show or set the charge limit (20..100)
  bypass on|off                  hold the battery at its current charge on external power
  android [--yes]                show, or with --yes restart into Android now
  android require-auth on|off    ask for authentication before switching (admin)
  torch [on|off|toggle|level N]
  ledring [charge|off|brightness N|low PERCENT]
  refresh [auto|off|manual [HZ]|idle MS60 MS30|preset power-saver|balanced|smooth]
  gpu [profile NAME|follow on|off|limits PROFILE MIN_MHZ MAX_MHZ|boost on|off]
                                 the performance profile (power-saver, balanced, performance); also `perf`
  gpu cpu-limits PROFILE LITTLE_MIN LITTLE_MAX BIG_MIN BIG_MAX
                                 CPU limits of a profile, MHz (the top of the range keeps boost)
  gpu wifi-low-latency on|off    Wi-Fi power saving off in the performance profile
  usb [wake on|off|dev on|off]
  emergency-key [on|off|hold SECONDS]
  diagnostics [export]
  boot [list|next NAME|clear|default NAME|reboot NAME|rescan]
                                 installed systems (multiboot): one-shot next boot, default, restart into;
                                 list also names what a system lacks for this kernel (modules, firmware)
  thermal                        temperatures (surface, CPU, GPU, board sensors), throttling, profile
  thermal profile quiet|default|performance
                                 board-temperature profile (performance: up to 58 °C, asks once)
  thermal follow on|off          the thermal profile follows the performance profile
  thermal bypass on|off          Bypass charging while the thermal profile is performance
  thermal panel-limit on|off     dim the panel at 55 °C (off asks for authentication)
  kernel [status]                kernel updates: running, trial, last good, available release
  kernel check|list|notes TAG    look for a newer kernel in the channel / show it / its release notes
  kernel download TAG            download and verify a release
  kernel install TAG [--reboot]  repack it into your stock boot image and write boot_a (admin);
                                 it is tried on the next start and confirmed after 90 s (stable)
  kernel update [--reboot]       check, download and install the newest release
  kernel inspect PATH            look at a kernel file (Image, Image.gz or boot image): release, modules, warnings
  kernel install-local PATH [--trial|--keep] [--name NAME] [--reboot] [--yes]
                                 install a kernel from a file the same safe way (admin password each time):
                                 --trial (default) kept only with `kernel keep`; --keep kept by itself
                                 once a system has run 90 s; otherwise back after its third start
  kernel keep                    keep the running trial kernel (testing channel, local files)
  kernel rollback [--reboot]     write the last good kernel (linux-good.img) back (admin)
  kernel channel stable|testing  release channel (admin; the source is kernel.source in helper.toml)
  kernel auto-check on|off       daily check for a new kernel (admin)
  kernel helper-notify on|off    show when a newer helper is published (admin)
  kernel dismiss                 hide the notice after an automatic rollback
  versions                       helper, kernel, series, firmware state
  reload                         re-read /etc/tb323fu/helper.toml (admin)";

struct Ctl {
    conn: Connection,
    json: bool,
}

fn path(obj: &str) -> String {
    if obj.is_empty() { ROOT.to_string() } else { format!("{ROOT}/{obj}") }
}
fn iface(obj: &str) -> String {
    if obj.is_empty() { BUS.to_string() } else { format!("{BUS}.{obj}") }
}

fn to_json(v: &Value) -> serde_json::Value {
    use serde_json::json;
    match v {
        Value::Bool(b) => json!(b),
        Value::U8(n) => json!(n),
        Value::U16(n) => json!(n),
        Value::U32(n) => json!(n),
        Value::U64(n) => json!(n),
        Value::I16(n) => json!(n),
        Value::I32(n) => json!(n),
        Value::I64(n) => json!(n),
        Value::F64(n) => if n.is_finite() { json!(n) } else { serde_json::Value::Null },
        Value::Str(s) => json!(s.as_str()),
        Value::ObjectPath(p) => json!(p.as_str()),
        Value::Value(b) => to_json(b),
        Value::Array(a) => serde_json::Value::Array(a.iter().map(to_json).collect()),
        Value::Structure(s) => serde_json::Value::Array(s.fields().iter().map(to_json).collect()),
        Value::Dict(d) => {
            let mut m = serde_json::Map::new();
            for (k, val) in d.iter() {
                let key = match to_json(k) {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                m.insert(key, to_json(val));
            }
            serde_json::Value::Object(m)
        }
        other => json!(format!("{other:?}")),
    }
}

fn human(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(m) => {
            if m.is_empty() {
                return "(none)".into();
            }
            m.iter().map(|(k, v)| format!("\n      {k}: {}", human(v))).collect()
        }
        serde_json::Value::Array(a) => a.iter().map(human).collect::<Vec<_>>().join(" "),
        serde_json::Value::Null => "unknown".into(),
        other => other.to_string(),
    }
}

impl Ctl {
    fn props(&self, obj: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
        let reply = self
            .conn
            .call_method(Some(BUS), path(obj).as_str(), Some("org.freedesktop.DBus.Properties"), "GetAll", &(iface(obj).as_str(),))
            .ok()?;
        let all: HashMap<String, OwnedValue> = reply.body().deserialize().ok()?;
        let mut keys: Vec<_> = all.keys().cloned().collect();
        keys.sort();
        let mut m = serde_json::Map::new();
        for k in keys {
            m.insert(k.clone(), to_json(&all[&k]));
        }
        Some(m)
    }

    fn show(&self, objs: &[&str]) -> i32 {
        let mut out = serde_json::Map::new();
        for o in objs {
            if let Some(p) = self.props(o) {
                out.insert(if o.is_empty() { "Helper".into() } else { o.to_string() }, serde_json::Value::Object(p));
            }
        }
        if out.is_empty() {
            eprintln!("tb323fu-ctl: tb323fu-helperd not reachable (or feature not present)");
            return 1;
        }
        if self.json {
            println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap());
        } else {
            for (o, p) in out {
                println!("{o}:");
                if let serde_json::Value::Object(p) = p {
                    for (k, v) in p {
                        println!("  {k}: {}", human(&v));
                    }
                }
            }
        }
        0
    }

    fn call<B>(&self, obj: &str, method: &str, body: &B) -> i32
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        match self.conn.call_method(Some(BUS), path(obj).as_str(), Some(iface(obj).as_str()), method, body) {
            Ok(reply) => {
                if let Ok(s) = reply.body().deserialize::<String>() {
                    println!("{s}");
                }
                0
            }
            Err(e) => {
                eprintln!("tb323fu-ctl: {obj}.{method}: {e}");
                1
            }
        }
    }
}

impl Ctl {
    /// Installed roots as a table (or the raw properties with --json).
    fn boot_list(&self) -> i32 {
        if self.json {
            return self.show(&["Boot"]);
        }
        let Some(p) = self.props("Boot") else {
            eprintln!("tb323fu-ctl: no Boot object (helper not running, or no root partitions)");
            return 1;
        };
        let get = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let (cur, def, next) = (get("Current"), get("Default"), get("Next"));
        println!("{:<3} {:<18} {:<8} {}", "", "PARTITION", "INIT", "SYSTEM");
        if let Some(serde_json::Value::Array(rs)) = p.get("Roots") {
            for r in rs {
                let f = |i: usize| r.get(i).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                let name = f(0);
                let mut mark = String::new();
                if name == cur { mark.push('*'); }
                if name == def { mark.push('d'); }
                if name == next { mark.push('n'); }
                let label = f(1);
                println!("{:<3} {:<18} {:<8} {}", mark, name, f(3), if !label.is_empty() { label } else if f(3) == "unknown" { "(not readable)".into() } else { "(empty)".into() });
                if let Some(serde_json::Value::Array(ps)) = p.get("RootHealth").and_then(|h| h.get(&name)) {
                    for x in ps.iter().filter_map(|v| v.as_str()) {
                        println!("{:<3} {:<18} ! {x}", "", "");
                    }
                }
            }
        }
        println!("* running  d default ({def})  n next boot ({})", if next.is_empty() { "none" } else { next.as_str() });
        0
    }
}

impl Ctl {
    /// Temperatures as a short table (or the raw properties with --json).
    fn thermal(&self) -> i32 {
        if self.json {
            return self.show(&["Thermal"]);
        }
        let Some(p) = self.props("Thermal") else {
            eprintln!("tb323fu-ctl: no Thermal object (helper not running, or no thermal zones)");
            return 1;
        };
        let deg = |v: Option<&serde_json::Value>| match v.and_then(|v| v.as_f64()) {
            Some(t) => format!("{t:.1} °C"),
            None => "unknown".into(),
        };
        println!("surface     {}", deg(p.get("Surface")));
        println!("cpu (max)   {}", deg(p.get("CpuMax")));
        println!("gpu (max)   {}", deg(p.get("GpuMax")));
        println!("throttling  {}", if p.get("Throttling").and_then(|v| v.as_bool()).unwrap_or(false) { "yes" } else { "no" });
        if let Some(pr) = p.get("Profile").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            let follow = p.get("FollowPerformance").and_then(|v| v.as_bool()).unwrap_or(false);
            let trips: Vec<String> = p.get("Trips").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|t| t.as_f64()).map(|t| format!("{t:.0}")).collect()).unwrap_or_default();
            println!("profile     {pr}{} (board steps {} °C)", if follow { ", follows the performance profile" } else { "" }, trips.join(" "));
        }
        if p.get("PanelLimited").and_then(|v| v.as_bool()).unwrap_or(false) {
            println!("panel       hot: brightness held at 70 %");
        }
        if let Some(serde_json::Value::Object(z)) = p.get("Zones") {
            for (k, v) in z {
                println!("  {k:<10}{}", deg(Some(v)));
            }
        }
        0
    }
}

/// "2026-10-02 12:00 UTC" for unix seconds (0: never).
fn utc(t: u64) -> String {
    if t == 0 {
        return "never".into();
    }
    let (days, secs) = ((t / 86400) as i64, t % 86400);
    // civil from days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", secs / 3600, secs % 3600 / 60)
}

impl Ctl {
    /// A long method of the Kernel object; its message, or the exit code.
    fn kcall<B>(&self, method: &str, body: &B) -> Result<String, i32>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        match self.conn.call_method(Some(BUS), path("Kernel").as_str(), Some(iface("Kernel").as_str()), method, body) {
            Ok(reply) => Ok(reply.body().deserialize::<String>().unwrap_or_default()),
            Err(zbus::Error::MethodError(_, msg, _)) => {
                eprintln!("tb323fu-ctl: kernel {}: {}", method.to_lowercase(), msg.unwrap_or_default());
                Err(1)
            }
            Err(e) => {
                eprintln!("tb323fu-ctl: kernel {}: {e}", method.to_lowercase());
                Err(1)
            }
        }
    }

    fn kernel_status(&self) -> i32 {
        if self.json {
            return self.show(&["Kernel"]);
        }
        let Some(p) = self.props("Kernel") else {
            eprintln!("tb323fu-ctl: no Kernel object (helper not running, or no boot_a/boot_b)");
            return 1;
        };
        let s = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let b = |k: &str| p.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        let n = |k: &str| p.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
        println!("running    {}{}", s("Running"), if b("SharedModules") { " (modules from the boot image)" } else { "" });
        println!("state      {}", s("State"));
        println!("channel    {} from {} (daily check {}, last check {})", s("Channel"), s("Source"), if b("AutoCheck") { "on" } else { "off" },
            utc(n("LastCheck")));
        let label = |k: &str| if s(k).is_empty() { String::new() } else { format!(" \"{}\"", s(k)) };
        println!("good       {}{}", if s("Good").is_empty() { "(none saved yet)".into() } else { s("Good") }, label("GoodLabel"));
        if !s("Trial").is_empty() {
            let from = if s("TrialChannel") == "local" { "a local file".to_string() } else { format!("{} channel", s("TrialChannel")) };
            println!("trial      {}{} ({from}, start {} of {})", s("Trial"), label("TrialLabel"), n("Tries"), n("MaxTries"));
            if b("KeepPending") {
                let left = n("MaxTries").saturating_sub(n("Tries"));
                println!("           kept only with `tb323fu-ctl kernel keep`; without that, {} comes back after {left} more start{}",
                    s("Good"), if left == 1 { "" } else { "s" });
            }
        }
        if !s("LastFailed").is_empty() {
            println!("failed     {} (did not start, or was rolled back; good kernel: {})", s("LastFailed"), s("Good"));
        }
        let downloaded: Vec<String> = p.get("Downloaded").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        match p.get("Available").and_then(|v| v.as_array()) {
            Some(a) if !a.is_empty() => {
                for e in a {
                    let f = |i: usize| e.get(i).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
                    println!("available  {} {}{}", f(0), f(1), if downloaded.contains(&f(0)) { " (downloaded)" } else { "" });
                }
            }
            _ => println!("available  (nothing newer)"),
        }
        if !s("HelperLatest").is_empty() {
            println!("helper     {} available: {}", s("HelperLatest"), s("HelperUpdateCommand"));
        }
        if !s("Message").is_empty() {
            println!("last       {}", s("Message"));
        }
        0
    }

    /// Kernel.InspectLocal on a file opened here (the daemon reads only what
    /// this user can read); prints what it found.
    fn inspect(&self, p: &str) -> Result<bool, i32> {
        let f = std::fs::File::open(p).map_err(|e| {
            eprintln!("tb323fu-ctl: {p}: {e}");
            1
        })?;
        let fd = zbus::zvariant::Fd::from(&f);
        let r = self.conn.call_method(Some(BUS), path("Kernel").as_str(), Some(iface("Kernel").as_str()), "InspectLocal", &(fd,));
        let (release, banner, format, shared, warnings): (String, String, String, bool, Vec<String>) = match r {
            Ok(reply) => reply.body().deserialize().map_err(|e| {
                eprintln!("tb323fu-ctl: kernel inspect: {e}");
                1
            })?,
            Err(zbus::Error::MethodError(_, msg, _)) => {
                eprintln!("tb323fu-ctl: {p}: {}", msg.unwrap_or_default());
                return Err(1);
            }
            Err(e) => {
                eprintln!("tb323fu-ctl: kernel inspect: {e}");
                return Err(1);
            }
        };
        if self.json {
            println!("{}", serde_json::json!({"Release": release, "Banner": banner, "Format": format, "SharedModules": shared, "Warnings": warnings}));
        } else {
            println!("file       {p} ({format})");
            println!("release    {release}");
            println!("build      {banner}");
            println!("modules    {}", if shared { "shared modules image inside (every system gets them)" } else { "NONE inside" });
            for w in &warnings {
                println!("warning    {w}");
            }
        }
        Ok(!warnings.is_empty())
    }

    /// kernel install-local PATH [--trial|--keep] [--name NAME] [--reboot] [--yes]
    fn install_local(&self, args: &[String], reboot: bool) -> i32 {
        let mut it = args.iter().skip_while(|a| a.as_str() != "install-local").skip(1);
        let (mut path, mut name, mut keep, mut trial, mut yes) = (None, String::new(), false, false, false);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--keep" => keep = true,
                "--trial" => trial = true,
                "--yes" => yes = true,
                "--reboot" | "--json" | "--session" => {}
                "--name" => name = it.next().cloned().unwrap_or_default(),
                s if s.starts_with("--name=") => name = s["--name=".len()..].to_string(),
                s if s.starts_with("--") => return usage(),
                s if path.is_none() => path = Some(s.to_string()),
                _ => return usage(),
            }
        }
        let Some(path) = path else { return usage() };
        if keep && trial {
            eprintln!("tb323fu-ctl: --keep or --trial, not both");
            return 2;
        }
        if let Err(c) = self.inspect(&path) {
            return c;
        }
        println!("keep       {}", if keep { "by itself, once a system has run 90 s with it" } else { "only with `tb323fu-ctl kernel keep` (otherwise back after its third start)" });
        if !yes {
            use std::io::{BufRead, IsTerminal, Write};
            if !std::io::stdin().is_terminal() {
                eprintln!("tb323fu-ctl: not asking without a terminal; add --yes to install");
                return 2;
            }
            print!("Install this kernel into boot_a? [y/N] ");
            let _ = std::io::stdout().flush();
            let mut l = String::new();
            let _ = std::io::stdin().lock().read_line(&mut l);
            if !matches!(l.trim(), "y" | "Y" | "yes") {
                println!("not installed");
                return 1;
            }
        }
        let f = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("tb323fu-ctl: {path}: {e}");
                return 1;
            }
        };
        match self.kcall("InstallLocal", &(zbus::zvariant::Fd::from(&f), name.as_str(), keep, reboot)) {
            Ok(m) => {
                println!("{m}");
                0
            }
            Err(c) => c,
        }
    }

    fn kernel(&self, rest: &[&str], args: &[String], reboot: bool) -> i32 {
        let say = |r: Result<String, i32>| match r {
            Ok(m) => {
                println!("{m}");
                0
            }
            Err(c) => c,
        };
        match rest.first().copied() {
            None | Some("status") | Some("list") => self.kernel_status(),
            Some("check") => say(self.kcall("Check", &())),
            Some("notes") => match rest.get(1) {
                Some(t) => say(self.kcall("Notes", &(*t,))),
                None => usage(),
            },
            Some("download") => match rest.get(1) {
                Some(t) => say(self.kcall("Download", &(*t,))),
                None => usage(),
            },
            Some("install") => match rest.get(1) {
                Some(t) => say(self.kcall("Install", &(*t, reboot))),
                None => usage(),
            },
            Some("update") => {
                match self.kcall("Check", &()) {
                    Ok(m) => println!("{m}"),
                    Err(c) => return c,
                }
                let p = self.props("Kernel").unwrap_or_default();
                let Some(tag) = p.get("Available").and_then(|v| v.as_array()).and_then(|a| a.first())
                    .and_then(|e| e.get(0)).and_then(|v| v.as_str()).map(str::to_string) else {
                    println!("nothing to update");
                    return 0;
                };
                let have = p.get("Downloaded").and_then(|v| v.as_array()).is_some_and(|a| a.iter().any(|x| x.as_str() == Some(tag.as_str())));
                if !have {
                    match self.kcall("Download", &(tag.as_str(),)) {
                        Ok(m) => println!("{m}"),
                        Err(c) => return c,
                    }
                }
                say(self.kcall("Install", &(tag.as_str(), reboot)))
            }
            Some("inspect") => match rest.get(1) {
                Some(p) => match self.inspect(p) {
                    Ok(_) => 0,
                    Err(c) => c,
                },
                None => usage(),
            },
            Some("install-local") => self.install_local(args, reboot),
            Some("keep") => say(self.kcall("Keep", &())),
            Some("rollback") => say(self.kcall("Rollback", &(reboot,))),
            Some("dismiss") => say(self.kcall("Dismiss", &())),
            Some("channel") => match rest.get(1).copied() {
                Some(ch @ ("stable" | "testing")) => self.call("Kernel", "SetChannel", &(ch,)),
                _ => usage(),
            },
            Some("auto-check") => match onoff(rest.get(1)) {
                Some(on) => self.call("Kernel", "SetAutoCheck", &(on,)),
                None => usage(),
            },
            Some("helper-notify") => match onoff(rest.get(1)) {
                Some(on) => self.call("Kernel", "SetHelperNotify", &(on,)),
                None => usage(),
            },
            _ => usage(),
        }
    }
}

fn onoff(s: Option<&&str>) -> Option<bool> {
    match s.copied() {
        Some("on") | Some("true") | Some("1") => Some(true),
        Some("off") | Some("false") | Some("0") => Some(false),
        _ => None,
    }
}

fn num(s: Option<&&str>) -> Option<u32> {
    s.and_then(|v| v.parse().ok())
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    2
}

fn run(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let session = args.iter().any(|a| a == "--session");
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).filter(|s| !s.starts_with("--") || *s == "--yes").collect();
    if a.is_empty() || a[0] == "help" || args.iter().any(|x| x == "--help") {
        println!("{USAGE}");
        return 0;
    }
    let conn = match if session { Connection::session() } else { Connection::system() } {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tb323fu-ctl: D-Bus: {e}");
            return 1;
        }
    };
    let c = Ctl { conn, json };
    let rest = &a[1..];
    match a[0] {
        "status" => c.show(&OBJECTS),
        "versions" => c.show(&[""]),
        "battery" => c.show(&["Battery"]),
        "charge-limit" => match num(rest.first()) {
            Some(p) => c.call("Battery", "SetChargeLimit", &(p,)),
            None if rest.is_empty() => c.show(&["Battery"]),
            None => usage(),
        },
        "bypass" => match onoff(rest.first()) {
            Some(b) => c.call("Battery", "SetBypass", &(b,)),
            None => usage(),
        },
        "android" => match rest.first().copied() {
            None => c.show(&["Android"]),
            Some("--yes") => c.call("Android", "SwitchToAndroid", &()),
            Some("require-auth") => match onoff(rest.get(1)) {
                Some(b) => c.call("Android", "SetRequireAuth", &(b,)),
                None => usage(),
            },
            _ => usage(),
        },
        "torch" => match rest.first().copied() {
            None => c.show(&["Torch"]),
            Some("on") => c.call("Torch", "Set", &(true,)),
            Some("off") => c.call("Torch", "Set", &(false,)),
            Some("toggle") => {
                let on = c.props("Torch").and_then(|p| p.get("On").and_then(|v| v.as_bool())).unwrap_or(false);
                c.call("Torch", "Set", &(!on,))
            }
            Some("level") => match num(rest.get(1)) {
                Some(n) => c.call("Torch", "SetLevel", &(n,)),
                None => usage(),
            },
            _ => usage(),
        },
        "ledring" => match rest.first().copied() {
            None => c.show(&["LedRing"]),
            Some(m @ ("charge" | "off")) => c.call("LedRing", "SetMode", &(m,)),
            Some("brightness") => match num(rest.get(1)) {
                Some(n) => c.call("LedRing", "SetBrightness", &(n,)),
                None => usage(),
            },
            Some("low") => match num(rest.get(1)) {
                Some(n) => c.call("LedRing", "SetLowPercent", &(n,)),
                None => usage(),
            },
            _ => usage(),
        },
        "refresh" => match rest.first().copied() {
            None => c.show(&["Refresh"]),
            Some(p @ ("auto" | "off")) => c.call("Refresh", "SetPolicy", &(p,)),
            Some("manual") => {
                if let Some(hz) = num(rest.get(1)) {
                    let r = c.call("Refresh", "SetRate", &(hz,));
                    if r != 0 {
                        return r;
                    }
                }
                c.call("Refresh", "SetPolicy", &("manual",))
            }
            Some("idle") => match (num(rest.get(1)), num(rest.get(2))) {
                (Some(x), Some(y)) => c.call("Refresh", "SetIdle", &(x, y)),
                _ => usage(),
            },
            Some("preset") => match rest.get(1) {
                Some(n) => c.call("Refresh", "ApplyPreset", &(*n,)),
                None => usage(),
            },
            _ => usage(),
        },
        "gpu" | "perf" => match rest.first().copied() {
            None => c.show(&["Gpu"]),
            Some("profile") => match rest.get(1) {
                Some(p) => c.call("Gpu", "SetProfile", &(*p,)),
                None => usage(),
            },
            Some("follow") => match onoff(rest.get(1)) {
                Some(b) => c.call("Gpu", "SetFollowPowerProfiles", &(b,)),
                None => usage(),
            },
            Some("limits") => match (rest.get(1), num(rest.get(2)), num(rest.get(3))) {
                (Some(p), Some(lo), Some(hi)) => c.call("Gpu", "SetLimits", &(*p, lo, hi)),
                _ => usage(),
            },
            Some("boost") => match onoff(rest.get(1)) {
                Some(b) => c.call("Gpu", "SetCpuBoost", &(b,)),
                None => usage(),
            },
            Some("cpu-limits") => match (rest.get(1), num(rest.get(2)), num(rest.get(3)), num(rest.get(4)), num(rest.get(5))) {
                (Some(p), Some(a), Some(b), Some(x), Some(y)) => c.call("Gpu", "SetCpuLimits", &(*p, a, b, x, y)),
                _ => usage(),
            },
            Some("wifi-low-latency") => match onoff(rest.get(1)) {
                Some(b) => c.call("Gpu", "SetWifiLowLatency", &(b,)),
                None => usage(),
            },
            _ => usage(),
        },
        "usb" => match (rest.first().copied(), onoff(rest.get(1))) {
            (None, _) => c.show(&["Usb"]),
            (Some("wake"), Some(b)) => c.call("Usb", "SetWake", &(b,)),
            (Some("dev"), Some(b)) => c.call("Usb", "SetDevMode", &(b,)),
            _ => usage(),
        },
        "emergency-key" => match rest.first().copied() {
            None => c.show(&["EmergencyKey"]),
            Some("on") => c.call("EmergencyKey", "SetEnabled", &(true,)),
            Some("off") => c.call("EmergencyKey", "SetEnabled", &(false,)),
            Some("hold") => match num(rest.get(1)) {
                Some(n) => c.call("EmergencyKey", "SetHoldSeconds", &(n,)),
                None => usage(),
            },
            _ => usage(),
        },
        "diagnostics" => match rest.first().copied() {
            None => c.show(&["Diagnostics"]),
            Some("export") => c.call("Diagnostics", "Export", &()),
            _ => usage(),
        },
        "boot" => match rest.first().copied() {
            None | Some("list") => c.boot_list(),
            Some("next") => match rest.get(1) {
                Some(n) => c.call("Boot", "SetNext", &(*n,)),
                None => usage(),
            },
            Some("clear") => c.call("Boot", "ClearNext", &()),
            Some("default") => match rest.get(1) {
                Some(n) => c.call("Boot", "SetDefault", &(*n,)),
                None => usage(),
            },
            Some("reboot") => match rest.get(1) {
                Some(n) => c.call("Boot", "RebootInto", &(*n,)),
                None => usage(),
            },
            Some("rescan") => c.call("Boot", "Rescan", &()),
            _ => usage(),
        },
        "thermal" => match (rest.first().copied(), rest.get(1).copied()) {
            (None, _) => c.thermal(),
            (Some("profile"), Some(p)) => c.call("Thermal", "SetProfile", &(p,)),
            (Some("follow"), _) => match onoff(rest.get(1)) {
                Some(b) => c.call("Thermal", "SetFollowPerformance", &(b,)),
                None => usage(),
            },
            (Some("bypass"), _) => match onoff(rest.get(1)) {
                Some(b) => c.call("Thermal", "SetPerformanceBypass", &(b,)),
                None => usage(),
            },
            (Some("panel-limit"), _) => match onoff(rest.get(1)) {
                Some(b) => c.call("Thermal", "SetPanelLimit", &(b,)),
                None => usage(),
            },
            _ => usage(),
        },
        "kernel" => c.kernel(rest, args, args.iter().any(|x| x == "--reboot")),
        "reload" => c.call("", "Reload", &()),
        _ => usage(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&args));
}
