# Open Device Helper changelog

## 0.2.0 — 2026-10-03

- Renamed from "Tablet Settings" to **Open Device Helper** (application and D-Bus IDs `io.github.joonhoekim.OpenDeviceHelper*`).
- **Kernel updates** from this repository's GitHub Releases (testing / stable channel) and **kernels from a file**:
  repacked into your stock boot image, `linux-good.img` kept, written with read-back, tried on the next start and
  rolled back after failed starts; Keep / rollback in the app and `tb323fu-ctl kernel`. Release notes rendered from Markdown.
- **Performance profile**: CPU and GPU limits per profile, CPU boost switch, thermal profiles (quiet / default /
  performance), panel heat protection (dims at 55 °C), low-latency Wi-Fi in the performance profile.
- **Charging**: recharge gap, "full by" schedule (charge to 100 % before a set time), battery health.
- **Lights & Vibration**: LED ring solid colour (a palette of nine in the app), hardware breathing on kernels with the
  pattern trigger, notification pulse; vibration strength and test.
- **USB**: wake on charger plug, USB-C port roles shown.
- Multiboot (Systems page, `tb323fu-ctl boot`), read-only thermal readings, per-root health checks.
- Fixes: polkit policy that registered no actions after a restart, -/+ buttons, fewer password prompts, the daemon
  restarts on upgrade, LED brightness lost after a colour change.

## 0.1.0 — 2026-10-01

First version: charge limit and bypass, Switch to Android, torch, idle refresh rate, GPU profile, USB wake and developer
mode, emergency key, diagnostics export; `tb323fu-helperd`, `tb323fu-ctl`, the settings app and the GNOME Shell extension.
