// SPDX-License-Identifier: MIT
//! tb323fu-ctl: command-line client for tb323fu-helperd.

use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

const BUS: &str = "io.github.joonhoekim.tb323fu.Helper";
const ROOT: &str = "/io/github/joonhoekim/tb323fu/Helper";
const OBJECTS: [&str; 11] = ["Battery", "Android", "Torch", "LedRing", "Refresh", "Gpu", "Usb", "EmergencyKey", "Diagnostics", "Boot", "Thermal"];

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
  gpu [profile NAME|follow on|off|limits PROFILE MIN_MHZ MAX_MHZ]
  usb [wake on|off|dev on|off]
  emergency-key [on|off|hold SECONDS]
  diagnostics [export]
  boot [list|next NAME|clear|default NAME|reboot NAME|rescan]
                                 installed systems (multiboot): one-shot next boot, default, restart into;
                                 list also names what a system lacks for this kernel (modules, firmware)
  thermal                        temperatures (surface, CPU, GPU, board sensors) and throttling
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
        if let Some(serde_json::Value::Object(z)) = p.get("Zones") {
            for (k, v) in z {
                println!("  {k:<10}{}", deg(Some(v)));
            }
        }
        0
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
        "gpu" => match rest.first().copied() {
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
        "thermal" => c.thermal(),
        "reload" => c.call("", "Reload", &()),
        _ => usage(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&args));
}
