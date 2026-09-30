// SPDX-License-Identifier: MIT
//! Minimal blocking D-Bus client for tb323fu-helperd: GetAll per object and
//! method calls. Everything the UI shows comes from property snapshots.

use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

pub const BUS: &str = "io.github.joonhoekim.tb323fu.Helper";
pub const ROOT: &str = "/io/github/joonhoekim/tb323fu/Helper";

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

    /// Call a method; Ok(Some(text)) when the method returns a string.
    pub fn call<B>(&self, obj: &str, method: &str, body: &B) -> Result<Option<String>, String>
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType,
    {
        let c = self.conn.as_ref().ok_or_else(|| "helper service not reachable".to_string())?;
        match c.call_method(Some(BUS), path(obj).as_str(), Some(iface(obj).as_str()), method, body) {
            Ok(reply) => Ok(reply.body().deserialize::<String>().ok()),
            Err(zbus::Error::MethodError(name, msg, _)) => {
                let n = name.as_str();
                let short = n.rsplit('.').next().unwrap_or(n);
                Err(match msg {
                    Some(m) if !m.is_empty() => format!("{short}: {m}"),
                    _ => short.to_string(),
                })
            }
            Err(e) => Err(e.to_string()),
        }
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
