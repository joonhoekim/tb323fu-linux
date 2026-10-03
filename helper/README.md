# tb323fu-helper (Open Device Helper)

`tb323fu-helperd` (system D-Bus service) and `tb323fu-ctl` (CLI) for the device-specific features of the
Lenovo Legion Tab Gen 5 / Y700 5th Gen (TB323FU): battery charge limit and bypass, battery/charger details,
restart into Android, flashlight, RGB-ring charge indicator, idle refresh policy, GPU limits per power profile,
USB wakeup and developer USB access, the emergency key combination, and a diagnostics export.
Design and API contract: [docs/helper.md](../docs/helper.md).

The GTK settings app is in [`crates/tb323fu-settings/`](#settings-app-tb323fu-settings); the GNOME quick-settings tile is in
[`userspace/desktop/gnome/extension/`](../userspace/desktop/gnome/extension/README.md).
Booting never depends on this; it only drives interfaces the kernel already exposes.

## Build and install

```sh
cargo build --release          # Rust >= 1.80; crates: zbus, serde, toml, sha2, serde_json
sudo ./install.sh              # /usr/local + /etc/systemd/system, enables tb323fu-helperd.service
tb323fu-ctl status
sudo ./install.sh --uninstall  # settings in /etc/tb323fu are kept
```

Packaging: `PREFIX=/usr DESTDIR=$pkgdir ./install.sh` installs into `/usr/libexec`, `/usr/bin`,
`/usr/lib/systemd/system`, `/usr/share/dbus-1/{system.d,system-services}`, `/usr/share/polkit-1/actions`
without touching the running system.

## Using it

```sh
tb323fu-ctl battery                  # capacity, state (charging/bypass/...), current, voltage, temperature, charger
tb323fu-ctl charge-limit 80          # 20..100
tb323fu-ctl bypass on                # hold the battery where it is while on external power
tb323fu-ctl battery recharge-gap 5   # charging resumes 5 % below the limit
tb323fu-ctl battery full-by 07:00 mon tue wed thu fri   # 100 % by 7:00 on weekdays, then back to the limit
tb323fu-ctl torch on | off | level 60
tb323fu-ctl ledring charge | solid | breathe | off | brightness 40 | low 15
tb323fu-ctl ledring color "#3c78ff" | speed 4000 | charge-override on | notify on | pulse "#00ff00" 3
tb323fu-ctl refresh auto | off | manual 60 | preset power-saver|balanced|smooth | idle 1000 5000
tb323fu-ctl gpu profile power-saver | follow on | limits balanced 160 1200   # the performance profile
tb323fu-ctl gpu cpu-limits power-saver 384 2496 768 2880   # CPU little min/max, big min/max (MHz)
tb323fu-ctl gpu wifi-low-latency on   # Wi-Fi power saving off in the performance profile
tb323fu-ctl thermal profile quiet | default | performance   # board-temperature steps -3 / 0 / +8 °C (max 58 °C)
tb323fu-ctl thermal follow on | bypass on | panel-limit on
tb323fu-ctl usb wake on
tb323fu-ctl android --yes            # restart into Android now
tb323fu-ctl diagnostics export       # sanitized tarball under /var/lib/tb323fu/diagnostics
tb323fu-ctl boot list                # installed systems, and what each lacks for this kernel
tb323fu-ctl thermal                  # surface, CPU, GPU and board temperatures, throttling, thermal profile
tb323fu-ctl kernel                   # kernel updates: running, trial, last good, available release
tb323fu-ctl kernel update --reboot   # check, download, install (admin) and restart; or check / download TAG / install TAG
tb323fu-ctl kernel keep              # keep a testing-channel kernel; rollback [--reboot] writes the last good one back
tb323fu-ctl kernel install-local ~/Image.gz --name "my build"   # a kernel you built: same trial and rollback (admin, every time)
tb323fu-ctl --json status
```

Settings are stored in `/etc/tb323fu/helper.toml` (see `data/helper.toml.example`) and applied at every start.

## Settings app (tb323fu-settings)

A GTK4/libadwaita front-end in `crates/tb323fu-settings/` — a separate Cargo workspace, so the daemon and CLI above
build without GTK development libraries. It reads everything from `tb323fu-helperd` (property poll every 2 s) and
changes settings through its methods; polkit decides what needs authentication, not the app. Pages: Battery, Display
(refresh rate), Performance (GPU, temperatures), Torch & LED ring, USB, Emergency key, Systems, Android, Diagnostics, About (with kernel updates). Pages whose
object the daemon does not export are hidden; without the daemon the app shows a status page.

```sh
# build dependencies: GTK >= 4.12 and libadwaita >= 1.5 development files (Debian: libgtk-4-dev libadwaita-1-dev)
cd crates/tb323fu-settings
cargo build --release
PREFIX=/usr/local ./install.sh      # binary, .desktop, icon, metainfo; --uninstall removes them
```

## Permissions

The daemon runs as root; polkit decides per call. Everyday controls are allowed for the active local
session without a password (charge limit, bypass, Android switch, flashlight, LED ring, refresh, performance profile,
cooler thermal profiles, USB wakeup, emergency-key settings, diagnostics). Authentication is asked for developer USB access,
the performance thermal profile and switching the panel heat protection off (once per session),
disabling the emergency key, `android require-auth`, `reload`, and for kernel updates: installing, going back,
and changing the channel or the daily check (looking for, downloading and keeping a kernel need none); installing a
kernel from a file asks for the administrator's password every time (`auth_admin`, see docs/helper.md).
Switching to Android can be made to ask for authentication with `tb323fu-ctl android require-auth on`. Actions are in
`data/io.github.joonhoekim.opendevicehelper.policy`.

## Tests

```sh
cargo test --release
dbus-run-session -- sh tests/fake-sysfs-test.sh target/release   # daemon on a private bus against a fake device tree
dbus-run-session -- sh tests/kernel-update-test.sh target/release  # kernel updates end to end (python3, curl, minisign)
```

`TB323FU_SYSFS_ROOT` points every device path at a fake tree, `TB323FU_CONFIG` the settings file, and
`tb323fu-helperd --session --no-polkit` runs on the session bus without authorization (tests only).

## Notes

- The charge limit is owned by the helper. If a udev rule still gives UPower a `CHARGE_LIMIT` hint (the platform's
  optional `upower-charge-limit` rule), GNOME's "preserve battery health" switch writes the same threshold: leave that
  rule out when installing the helper.
- The emergency key settings are written to `/etc/tb323fu/emergency-key.conf` (`ENABLED`, `HOLD_SECONDS`), which the
  platform service `tb323fu-emergency-key` reads.
