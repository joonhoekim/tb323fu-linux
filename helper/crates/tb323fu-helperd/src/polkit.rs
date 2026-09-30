// SPDX-License-Identifier: MIT
//! polkit authorization of the D-Bus caller (CheckAuthorization with the
//! caller's unique bus name as subject, interaction allowed).

use std::collections::HashMap;
use zbus::fdo;
use zbus::message::Header;
use zbus::zvariant::Value;

pub const PREFIX: &str = "io.github.joonhoekim.tb323fu.helper.";

pub async fn check(conn: &zbus::Connection, hdr: &Header<'_>, action: &str, no_polkit: bool) -> fdo::Result<()> {
    if no_polkit {
        return Ok(());
    }
    let sender = hdr
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))?
        .to_string();
    let action_id = format!("{PREFIX}{action}");
    let mut subject_details: HashMap<&str, Value> = HashMap::new();
    subject_details.insert("name", Value::from(sender.as_str()));
    let subject = ("system-bus-name", subject_details);
    let details: HashMap<&str, &str> = HashMap::new();
    let reply = conn
        .call_method(
            Some("org.freedesktop.PolicyKit1"),
            "/org/freedesktop/PolicyKit1/Authority",
            Some("org.freedesktop.PolicyKit1.Authority"),
            "CheckAuthorization",
            &(subject, action_id.as_str(), details, 1u32, ""),
        )
        .await
        .map_err(|e| fdo::Error::Failed(format!("polkit: {e}")))?;
    let (authorized, _challenge, _d): (bool, bool, HashMap<String, String>) = reply
        .body()
        .deserialize()
        .map_err(|e| fdo::Error::Failed(format!("polkit reply: {e}")))?;
    if authorized {
        Ok(())
    } else {
        Err(fdo::Error::AccessDenied(format!("not authorized: {action_id}")))
    }
}
