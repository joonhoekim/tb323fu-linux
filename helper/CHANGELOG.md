# Open Device Helper changelog

## 0.3.1 — 2026-10-04

- The GNOME quick settings tile, its menu header and the settings app's sidebar are labeled **Device** (was "Tablet",
  which did not fit the tile); the extension is listed as "Open Device Helper".
- American spelling in the app, `tb323fu-ctl` help and error messages ("color", "canceled").

## 0.3.0 — 2026-10-04

- **The helper updates itself** from `helper-vX.Y.Z` releases on GitHub Releases (the same channel and checks as
  kernel updates): `tb323fu-ctl helper check | download | install | update | rollback`, About → Helper Updates in
  the settings app, a new D-Bus object `HelperUpdate`, polkit action `helper-update`. The swap runs in a transient
  unit, keeps the previous version, restarts the helper and goes back by itself when the new one does not answer
  within 30 s. Only where nobody else owns the helper: when dpkg, pacman, rpm or NixOS installed it, the helper
  says what to do instead (the commands it showed before pointed at repositories that do not exist).
- `tools/helper-release.py` makes and checks the release asset set; `install.sh` takes `BUILD_DIR`.
- **tb323fu-platform:** `back-to-android` is installed with `#!/bin/sh`. It started with `#!/bin/busybox sh`, so on
  a root without busybox (Ubuntu) "Restart into Android" and the volume-key way back failed.

## 0.2.0 — 2026-10-03

- Renamed from "Tablet Settings" to **Open Device Helper** (application and D-Bus IDs `io.github.joonhoekim.OpenDeviceHelper*`).
- **Kernel updates** from this repository's GitHub Releases (testing / stable channel) and **kernels from a file**:
  repacked into your stock boot image, `linux-good.img` kept, written with read-back, tried on the next start and
  rolled back after failed starts; Keep / rollback in the app and `tb323fu-ctl kernel`. Release notes rendered from Markdown.
- **Performance profile**: CPU and GPU limits per profile, CPU boost switch, thermal profiles (quiet / default /
  performance), panel heat protection (dims at 55 °C), low-latency Wi-Fi in the performance profile.
- **Charging**: recharge gap, "full by" schedule (charge to 100 % before a set time), battery health.
- **Lights & Vibration**: LED ring solid color (a palette of nine in the app), hardware breathing on kernels with the
  pattern trigger, notification pulse; vibration strength and test.
- **USB**: wake on charger plug, USB-C port roles shown.
- Multiboot (Systems page, `tb323fu-ctl boot`), read-only thermal readings, per-root health checks.
- Fixes: polkit policy that registered no actions after a restart, -/+ buttons, fewer password prompts, the daemon
  restarts on upgrade, LED brightness lost after a color change.

## 0.1.0 — 2026-10-01

First version: charge limit and bypass, Switch to Android, torch, idle refresh rate, GPU profile, USB wake and developer
mode, emergency key, diagnostics export; `tb323fu-helperd`, `tb323fu-ctl`, the settings app and the GNOME Shell extension.
