# Open Device Helper changelog

## 0.4.2 — 2026-10-15

- **Bypass keeps the charger at full power:** with a kernel that has the charge current limit (series patch 0132,
  kernel t51), Bypass raises the limit to 100 % and holds the battery with a small charge current instead of lowering
  the limit to the charge, so the USB PD contract stays at 9 V instead of dropping to 5 V (~12 W) — measured 8.97 V in,
  ~85 mA into the battery, and the battery still helps when the load is over what the charger gives. It starts at the
  full current until the charger charges (the firmware does not start under a small limit), and starts a charger that
  stopped (replugged) again. New settings: `battery.bypass_method` (`auto` / `current` / `limit`),
  `battery.bypass_current_ma` (50..1000, default 100); `tb323fu-ctl bypass method|current`; D-Bus `BypassMethod`,
  `BypassMethodActive`, `BypassCurrentMa`, `ChargeCurrentMa`, `SetBypassMethod`, `SetBypassCurrent`; app rows under
  Charging.
- **Stalled charging is restarted:** on a charger, below the start threshold and "Not charging" for 3 minutes (the
  firmware stops by itself at times), the limit is taken below the charge for a moment and back, which starts it
  again; at most every 30 minutes, above 55 %. `battery.stall_rescue` (default on), `tb323fu-ctl battery stall-rescue`,
  D-Bus `StallRescue`, app switch "Restart Stalled Charging".
- **A Bluetooth mouse's battery is no longer taken for the tablet's:** power supplies with `scope` Device are skipped
  (a connected mouse showed its charge, and a 100 % charge limit, as the tablet's).
- About shows the patch series again: `SeriesTag` falls back to the tag of a `-tb323fu-TAG` kernel release (it said
  `unknown`).

## 0.4.1 — 2026-10-06

- **Refresh follows the display mode:** in the 164 Hz panel mode the fixed rate goes up to 164 Hz and Full Rate
  (was "Always 120 Hz") means the mode's rate. New `MaxHz` property on the `Refresh` object, read from the kernel's
  `idle_refresh_state`.

## 0.4.0 — 2026-10-06

- **Mesa channel:** the project's Mesa build for the Adreno 840 (Turnip Vulkan, rusticl OpenCL, binary releases)
  from `mesa-*` releases on GitHub Releases, next to the distribution's Mesa, which stays installed and
  untouched. `tb323fu-ctl mesa check | download | install | update | on | off | keep | rollback`; switching on sets
  the loader variables for the next login (`/etc/environment.d/60-tb323fu-mesa.conf`,
  `/etc/profile.d/tb323fu-mesa.sh`). A version switched on is on trial: without Keep, the third start goes back to
  the distribution's Mesa. `tb323fu-mesa run|distro COMMAND` (also `tb323fu-ctl mesa run|distro`) picks the driver
  for one command whatever the session setting. New D-Bus object `Mesa`, polkit action `mesa-install`. Settings
  app: Performance → Graphics Drivers, with a Keep banner while a version is on trial.
- `tools/mesa-release.py` makes and checks the Mesa release assets.
- **Settings survive a restart:** Bypass turned off during the performance thermal profile stays off
  (`thermal.bypass_declined`); Bypass on is applied again at start; the brightness the panel heat limit lowered and
  the Wi-Fi interfaces whose power saving the helper switched off are given back after a restart; a config file that
  does not parse is kept as `helper.toml.bad` before the defaults are used.
- The app icon is the site's pixel tablet on a blue tile.
- Release tarballs and packages carry the license notices of the Rust crates (`THIRD-PARTY-LICENSES`).

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
