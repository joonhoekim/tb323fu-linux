// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! NetworkManager queries: whether a Wi-Fi interface's power saving is set by
//! its connection profile (then NetworkManager owns it and the helper leaves
//! it alone).

use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const NM: &str = "org.freedesktop.NetworkManager";

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

/// `802-11-wireless.powersave` of the connection applied to `iface` is
/// `disable` (2) or `enable` (3). False without NetworkManager, without an
/// active connection, or when it is `default`/`ignore` (0/1: NetworkManager
/// does not touch the interface's setting).
pub async fn owns_power_save(conn: &zbus::Connection, iface: &str) -> bool {
    let Ok(m) = conn.call_method(Some(NM), "/org/freedesktop/NetworkManager", Some(NM), "GetDeviceByIpIface", &(iface,)).await else {
        return false;
    };
    let Ok(dev) = m.body().deserialize::<OwnedObjectPath>() else { return false };
    let Ok(m) = conn
        .call_method(Some(NM), dev.as_str(), Some("org.freedesktop.NetworkManager.Device"), "GetAppliedConnection", &(0u32,))
        .await
    else {
        return false;
    };
    let Ok((settings, _)) = m.body().deserialize::<(Settings, u64)>() else { return false };
    let ps = settings.get("802-11-wireless").and_then(|w| w.get("powersave")).and_then(|v| u32::try_from(v.clone()).ok());
    matches!(ps, Some(2) | Some(3))
}
