// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! Minimal blocking D-Bus client for tb323fu-helperd: GetAll per object and
//! method calls. Everything the UI shows comes from property snapshots.

use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

pub const BUS: &str = "io.github.joonhoekim.OpenDeviceHelper1";
pub const ROOT: &str = "/io/github/joonhoekim/OpenDeviceHelper1";

pub type Props = HashMap<String, OwnedValue>;

#[derive(Clone, Default)]
pub struct Client {
    conn: Option<Connection>,
}

fn path(obj: &str) -> String {
    if obj.is_empty() { ROOT.to_string() } else { format!("{ROOT}/{obj}") }
}
fn iface(obj: &str) -> String {
    if obj.is_empty() { BUS.to_string() } else { format!("{BUS}.{obj}") }
}

impl Client {
    pub fn connect() -> Self {
        Self { conn: Connection::system().ok() }
    }

    pub fn connected(&self) -> bool {
        self.conn.is_some()
    }

    /// All properties of one object, or None when the daemon or the object
    /// (feature not present on this kernel) is missing.
    pub fn get_all(&self, obj: &str) -> Option<Props> {
        let c = self.conn.as_ref()?;
        let reply = c
            .call_method(Some(BUS), path(obj).as_str(), Some("org.freedesktop.DBus.Properties"), "GetAll", &(iface(obj).as_str(),))
            .ok()?;
        reply.body().deserialize().ok()
    }

    /// Call a method; Ok(Some(text)) when the method returns a string. Errors
    /// come back as short text for a person (no D-Bus error names).
    pub fn call<B>(&self, obj: &str, method: &str, body: &B) -> Result<Option<String>, String>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        let c = self.conn.as_ref().ok_or_else(|| NOT_RUNNING.to_string())?;
        match c.call_method(Some(BUS), path(obj).as_str(), Some(iface(obj).as_str()), method, body) {
            Ok(reply) => Ok(reply.body().deserialize::<String>().ok()),
            Err(zbus::Error::MethodError(name, msg, _)) => Err(human_error(name.as_str(), msg.as_deref())),
            Err(zbus::Error::InputOutput(_)) => Err(NOT_RUNNING.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// What Kernel.InspectLocal found in a kernel file: release, banner, format,
/// shared modules inside, warnings.
pub type LocalKernel = (String, String, String, bool, Vec<String>);

impl Client {
    /// Kernel.InspectLocal on a file opened by the app (the daemon reads only
    /// what this user can read).
    pub fn inspect_local(&self, f: &std::fs::File) -> Result<LocalKernel, String> {
        let c = self.conn.as_ref().ok_or_else(|| NOT_RUNNING.to_string())?;
        match c.call_method(Some(BUS), path("Kernel").as_str(), Some(iface("Kernel").as_str()), "InspectLocal", &(zbus::zvariant::Fd::from(f),)) {
            Ok(reply) => reply.body().deserialize().map_err(|e| e.to_string()),
            Err(zbus::Error::MethodError(name, msg, _)) => Err(human_error(name.as_str(), msg.as_deref())),
            Err(zbus::Error::InputOutput(_)) => Err(NOT_RUNNING.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

const NOT_RUNNING: &str = "The helper service is not running";

/// D-Bus error name + message -> text for a toast.
pub fn human_error(name: &str, msg: Option<&str>) -> String {
    let short = name.rsplit('.').next().unwrap_or(name);
    match short {
        "AccessDenied" | "AuthFailed" | "InteractiveAuthorizationRequired" => "Authentication was cancelled or denied".into(),
        "ServiceUnknown" | "NoReply" | "NameHasNoOwner" | "UnknownObject" | "Disconnected" => NOT_RUNNING.into(),
        _ => match msg {
            Some(m) if !m.is_empty() => {
                let mut c = m.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            }
            _ => "Something went wrong".into(),
        },
    }
}

/// The setting a method changes, for error toasts.
pub fn setting_name(method: &str) -> &'static str {
    match method {
        "SetChargeLimit" => "Charge limit",
        "SetBypass" => "Bypass charging",
        "SetPolicy" => "Refresh rate",
        "SetRate" => "Fixed rate",
        "SetIdle" | "ApplyPreset" => "Refresh timing",
        "SetProfile" => "GPU profile",
        "SetFollowPowerProfiles" => "Follow power mode",
        "SetLimits" => "GPU limits",
        "Set" => "Torch",
        "SetLevel" => "Torch brightness",
        "SetMode" => "Charge indicator",
        "SetBrightness" => "LED ring brightness",
        "SetLowPercent" => "Low battery colour",
        "SetWake" => "USB wake",
        "SetDevMode" => "Developer mode",
        "SetEnabled" => "Emergency key",
        "SetHoldSeconds" => "Hold time",
        "SetRequireAuth" => "Android authentication",
        "SwitchToAndroid" => "Restart into Android",
        "SetNext" => "Next restart",
        "ClearNext" => "Next restart",
        "SetDefault" => "Default system",
        "RebootInto" => "Restart",
        "Rescan" => "Rescan",
        "Export" => "Export",
        "Check" => "Kernel check",
        "Download" => "Kernel download",
        "Install" => "Kernel install",
        "InstallLocal" | "InspectLocal" => "Kernel from file",
        "Keep" => "Keep kernel",
        "Rollback" => "Previous kernel",
        "Dismiss" => "Kernel notice",
        "SetChannel" => "Kernel channel",
        "SetAutoCheck" => "Daily check",
        "Notes" => "Release notes",
        _ => "Settings",
    }
}

fn val<'a>(p: &'a Props, k: &str) -> Option<&'a Value<'static>> {
    p.get(k).map(|v| &**v)
}

fn str_of(v: &Value<'_>) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.to_string()),
        Value::Value(b) => str_of(b),
        _ => None,
    }
}

fn uu_of(v: &Value<'_>) -> Option<(u32, u32)> {
    match v {
        Value::Structure(st) => match (st.fields().first(), st.fields().get(1)) {
            (Some(Value::U32(x)), Some(Value::U32(y))) => Some((*x, *y)),
            _ => None,
        },
        Value::Value(b) => uu_of(b),
        _ => None,
    }
}

pub fn b(p: &Props, k: &str) -> Option<bool> {
    match val(p, k)? {
        Value::Bool(x) => Some(*x),
        _ => None,
    }
}

pub fn u(p: &Props, k: &str) -> Option<u32> {
    match val(p, k)? {
        Value::U32(x) => Some(*x),
        Value::I32(x) if *x >= 0 => Some(*x as u32),
        Value::U64(x) => Some(*x as u32),
        _ => None,
    }
}

pub fn i(p: &Props, k: &str) -> Option<i32> {
    match val(p, k)? {
        Value::I32(x) => Some(*x),
        Value::U32(x) => Some(*x as i32),
        _ => None,
    }
}

pub fn f(p: &Props, k: &str) -> Option<f64> {
    match val(p, k)? {
        Value::F64(x) => Some(*x),
        _ => None,
    }
}

pub fn s(p: &Props, k: &str) -> Option<String> {
    match val(p, k)? {
        Value::Str(x) => Some(x.to_string()),
        _ => None,
    }
}

pub fn strs(p: &Props, k: &str) -> Vec<String> {
    match val(p, k) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(str_of)
            .collect(),
        _ => Vec::new(),
    }
}

/// a{ss} as sorted pairs.
pub fn dict_ss(p: &Props, k: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(Value::Dict(d)) = val(p, k) {
        for (kk, vv) in d.iter() {
            if let (Some(a), Some(b)) = (str_of(kk), str_of(vv)) {
                out.push((a, b));
            }
        }
    }
    out.sort();
    out
}

/// a{s(uu)} as (key, a, b), sorted by key.
pub fn dict_suu(p: &Props, k: &str) -> Vec<(String, u32, u32)> {
    let mut out = Vec::new();
    if let Some(Value::Dict(d)) = val(p, k) {
        for (kk, vv) in d.iter() {
            if let (Some(a), Some((x, y))) = (str_of(kk), uu_of(vv)) {
                out.push((a, x, y));
            }
        }
    }
    out.sort();
    out
}

/// a{sd} as sorted pairs (NaN values kept).
pub fn dict_sd(p: &Props, k: &str) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    if let Some(Value::Dict(d)) = val(p, k) {
        for (kk, vv) in d.iter() {
            let v = match vv {
                Value::Value(b) => &**b,
                o => o,
            };
            if let (Some(a), Value::F64(x)) = (str_of(kk), v) {
                out.push((a, *x));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// a{sas}: key -> list of strings.
pub fn dict_sas(p: &Props, k: &str) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    if let Some(Value::Dict(d)) = val(p, k) {
        for (kk, vv) in d.iter() {
            let v = match vv {
                Value::Value(b) => &**b,
                o => o,
            };
            if let (Some(a), Value::Array(arr)) = (str_of(kk), v) {
                out.insert(a, arr.iter().filter_map(str_of).collect());
            }
        }
    }
    out
}

/// a(ssbs): the helper's Boot.Roots (name, label, present, init kind).
pub fn roots(p: &Props, k: &str) -> Vec<(String, String, bool, String)> {
    let mut out = Vec::new();
    if let Some(Value::Array(a)) = val(p, k) {
        for v in a.iter() {
            let v = match v {
                Value::Value(b) => &**b,
                other => other,
            };
            if let Value::Structure(st) = v {
                let f = st.fields();
                if f.len() == 4 {
                    let b = matches!(&f[2], Value::Bool(true));
                    if let (Some(n), Some(l), Some(i)) = (str_of(&f[0]), str_of(&f[1]), str_of(&f[3])) {
                        out.push((n, l, b, i));
                    }
                }
            }
        }
    }
    out
}

pub fn t(p: &Props, k: &str) -> Option<u64> {
    match val(p, k)? {
        Value::U64(x) => Some(*x),
        Value::U32(x) => Some(u64::from(*x)),
        _ => None,
    }
}

/// a(sssu): the helper's Kernel.Available (tag, release, notes URL, serial).
pub fn releases(p: &Props, k: &str) -> Vec<(String, String, String, u32)> {
    let mut out = Vec::new();
    if let Some(Value::Array(a)) = val(p, k) {
        for v in a.iter() {
            let v = match v {
                Value::Value(b) => &**b,
                other => other,
            };
            if let Value::Structure(st) = v {
                let f = st.fields();
                if let (4, Some(t), Some(r), Some(n), Some(Value::U32(s))) = (f.len(), f.first().and_then(str_of), f.get(1).and_then(str_of), f.get(2).and_then(str_of), f.get(3)) {
                    out.push((t, r, n, *s));
                }
            }
        }
    }
    out
}
