# Device helper

`tb323fu-helper` owns the TB323FU's user-facing knobs — charge limit and bypass, restart into Android, torch, RGB-ring
charge indicator, idle refresh policy, GPU limits, USB wakeup, emergency key settings, multiboot selection,
diagnostics — and exposes them over **D-Bus** to a CLI, a GNOME quick-settings tile and a settings app.
It needs only systemd, D-Bus and polkit, and works without any particular desktop. Code and build instructions:
[`helper/`](../helper/README.md).

| Part | State |
|---|---|
| `tb323fu-helperd` (system D-Bus service) and `tb323fu-ctl` (CLI) | implemented |
| GNOME Shell quick-settings extension | implemented |
| GTK4 + libadwaita settings app (`tb323fu-settings`) | implemented |
| KDE Plasma applet | planned |

What it does not do:

- **Booting never depends on the helper.** Everything needed to boot and to have working audio, input, sensors and
  the emergency way back to Android stays in plain files and services (layer 1 below).
- No self-updater and no package-manager calls. Each distribution's packaging updates it.
- No new hardware drivers; the helper only drives interfaces the kernel already exposes.

## Layers

| Layer | What | Examples | Depends on |
|---|---|---|---|
| **1. Platform files** | static configuration and boot-critical services; always installed; work with no helper and no desktop | udev rules (`60-…`–`91-…`), ALSA UCM for the TB323FU card, PipeWire speaker-protection filter chain and WirePlumber rule, libcamera sensor tuning, services: Bluetooth address, DSP start, audio defaults, USB port role, emergency volume-up+down chord, per-install ID generation | systemd, udev |
| **2. Helper daemon** | `tb323fu-helperd`: one system service, owns the knobs below, D-Bus API, polkit checks, persistence | charge limit, Android switch, torch, LED ring, idle refresh policy, GPU profile, speaker level | systemd, D-Bus, polkit |
| **3. Front-ends** | talk only to the daemon's D-Bus API (and, for desktop-owned settings, the desktop's own settings store) | `tb323fu-ctl` CLI (always shipped), GNOME Shell quick-settings extension, GTK4 + libadwaita settings app (gtk-rs), KDE Plasma applet (later) | layer 2 |

The structure follows the reference project [ubuntu-galaxy-tab-s9-ultra](https://github.com/agcarbajo/ubuntu-galaxy-tab-s9-ultra)'s
Tab Companion (a backend owns sysfs/evdev/uinput, the window uses only D-Bus and GSettings), without its coupling to Ubuntu,
GNOME and apt.

## Front-ends

- **`tb323fu-ctl`** (Rust, same crate as the daemon's client library; full list in [helper/README.md](../helper/README.md#using-it)): `tb323fu-ctl status`, `charge-limit 80`, `android --yes`, `torch on`, `refresh auto|off|manual 60`, `refresh idle 1000 5000`, `gpu profile balanced`. Always installed; the reference for scripting.
- **GNOME Shell extension** (quick settings, `userspace/desktop/gnome/extension/`): one "Tablet" tile whose subtitle shows charge and live refresh rate (`80% · 60 Hz`); a tap opens the settings app, the arrow opens a menu with the battery charge and state in its header (`80% · Charging`, plus the charger when plugged in: the contract, or type and measured power, `99% · Bypass · PPS · ~40 W`), a Charge Limit heading with 60% / 80% / 100% (a limit set in the app, e.g. 75%, shows in the heading: `Charge Limit · 75%`) and Bypass Charging, the refresh policy (Adaptive, Fixed (N Hz), Always 120 Hz), the torch with a brightness slider, and "Tablet Settings…". The menu stays open while choices are made and scrolls on a landscape screen. Without the daemon the tile still opens the app and the menu says the helper is not running. Errors appear as one notification per failed call (no D-Bus names); a cancelled authentication shows nothing. Auto Brightness stays a separate GSettings tile. Talks to the daemon over D-Bus only.
- **Settings app** (Rust, gtk-rs, GTK4 + libadwaita, `tb323fu-settings [--page NAME]`): pages Battery (limit, bypass, state, power, charger contract and measured input), Display (refresh policy, timing presets or custom idle times), Performance (GPU profile and limits, temperatures from `Thermal`), Torch & LED Ring, USB, Emergency Key, Systems (multiboot: restart into / next / default from a row's menu, opened by tapping the row; the subtitle always names the partition, then any problem in short, e.g. `baldur-root-sd · No sound or Wi-Fi`), Android, Diagnostics (export) and About (versions, firmware, About dialog with copyable debug info). Short values stay on one line, long ones (kernel, hashes, paths) sit under the row title with a copy button; the split view collapses into list → page navigation below 860 sp. The property poll runs on a worker thread, so a slow daemon answer (a Boot rescan mounts SD roots; the daemon runs it on a blocking thread too) never freezes the window. Adwaita fits GNOME; on other desktops it still runs as a plain app.
- **KDE Plasma applet** (later): QML plasmoid over the same D-Bus API.

## Multiboot

The kernel's initramfs can boot any of these root filesystems (GPT partition names): `baldur-root` (UFS, the default), `baldur-root-sd`
and `tb323fu-*` (for example `tb323fu-ubuntu`, `tb323fu-arch`, `tb323fu-nixos` on the SD card). The selection lives on the UFS root:

- `/etc/tb323fu/boot-next` — one-shot: the initramfs reads it, **deletes it before trying the root**, then boots it. A system that hangs
  later therefore boots the default on the next restart.
- `/etc/tb323fu/boot-default` — persistent default (absent = `baldur-root`).
- Order tried: next, default, `baldur-root`, `baldur-root-sd`; a root that is missing, does not mount or has no init is skipped.
- Init per root: `/sbin/init` (Debian, Ubuntu, Arch, Fedora) or NixOS's `/nix/var/nix/profiles/system/init`.
- `/etc/tb323fu/boot-menu` (any content) enables a menu at boot: volume-up moves to the next system, 5 s without a press boots it.
- Every root needs the running kernel's modules in `/lib/modules/$(uname -r)` (the kernel is shared) and the firmware files.

The helper's `Boot` object lists the roots (reading each one's `os-release`, mounted read-only without journal replay, cached) and writes
the selection; when it runs from another root it mounts the UFS root under `/run/tb323fu/ufs` for that. CLI: `tb323fu-ctl boot`
(`list`, `next NAME`, `clear`, `default NAME`, `reboot NAME`, `rescan`); the settings app has a "Systems" page.
While reading each root the helper also checks it against the running kernel (`RootHealth`): the modules directory with
`modules.dep`, its `extra/` (the out-of-tree aw882xx amplifier driver), and the audio DSP and Wi-Fi firmware (also `.zst`/`.xz`).
`tb323fu-ctl boot list` prints the problems under the root, and the app warns on the row and in the restart dialog.

## Persistence

- `android.require_auth` (bool, default `false`): whether switching to Android asks for authentication (polkit `android-switch-auth` instead of `android-switch`). Settable from the app and `tb323fu-ctl`; changing it itself requires authentication.
- `/etc/tb323fu/helper.toml` — written by the daemon when a setting changes, read at start; defaults when the file or a key is absent.
- Applied once at daemon start (after the relevant devices exist; the daemon waits on udev for the LED/power-supply/devfreq devices, bounded).
- Settings that the kernel already defaults sensibly (idle refresh auto) are only written when the user changes them. The charge limit is always applied by the helper at start (default 80 %).
- The legacy `/etc/baldur/*.conf` files are read once on first start to migrate values, then left untouched.

## Feature inventory

<details>
<summary>Every feature with its kernel interface, privilege, layer and hazards</summary>

| Feature | Kernel / sysfs interface | Privilege | Layer | Notes and hazards |
|---|---|---|---|---|
| Charge limit | `/sys/class/power_supply/qcom-battmgr-bat/charge_control_end_threshold` | root | 2 | **The helper is the only owner.** The `CHARGE_LIMIT` udev hint is dropped so UPower/GNOME's "preserve battery health" switch no longer writes the threshold (two writers would overwrite each other); the helper's app and quick settings replace it |
| Switch to Android | writes the Android boot image kept in `boot_b` back into `boot_a` and reboots; refuses unless `boot_b` matches the recorded SHA-256 | root; authentication optional (`android.require_auth`) | 2 (tool in 1) | `back-to-android` itself stays a layer-1 tool (also used by the emergency chord). The daemon method only wraps it: checks the hash file exists, emits a signal, runs it. Irreversible until the user switches back from Android |
| Emergency chord (volume up+down 10 s → Android) | evdev on `pmic_resin` and `gpio-keys` | root | **1** | must work with no desktop and no helper; stays a plain service |
| Torch | `/sys/class/leds/white:flash/brightness` (torch level ~96; never the flash strobe) | root (daemon); `video` group via the optional udev `74-…-leds.rules` without the helper | 2 | daemon exposes on/off/level with a safe maximum; front-ends never write sysfs |
| LED ring (charge indicator / effects) | `/sys/class/leds/aw22127:rgb:indicator/{brightness,multi_intensity}` | root (daemon); `video` group via the optional udev `74-…-leds.rules` without the helper | 2 | the charge indicator (amber charging, green full or held, red at or below the low percentage) runs in the daemon. Vendor lighting effects need register-level programming (not the LED class) — not implemented |
| Idle refresh rate | `/sys/module/msm/parameters/idle_refresh_{policy,hz,ms60,ms30,min_hz,input,selfflush,state}` | root (parameters are 0644 root) | 2 | The kernel always restores 120 Hz before suspend or display-off, so the daemon needs no sleep handling. `selfflush` is a debug knob — not exposed. |
| Panel modes (90 / 120 / 164 Hz) | DRM modes from the panel driver | user (compositor) | 3 (no daemon) | handled by each desktop's display settings; the helper does not switch modes |
| GPU performance floor/cap per power profile | `/sys/class/devfreq/3d00000.gpu/{min,max}_freq` | root | 2 | follows power-profiles-daemon when present; otherwise the daemon's own profile property. Widen before narrowing (min ≤ max at every step) |
| Speaker level (protected mode) | ALSA controls `aw_dev_0_volume`, `aw_dev_1_volume` | `audio` group | 1 | user unit `tb323fu-speaker-gain`: amplifier attenuation 21 while the PipeWire protection filter runs, 159 otherwise (`/etc/tb323fu/audio.conf`). **Safety-relevant**: the loud value is only safe behind the filter chain. Do not move into the daemon; do not expose a free slider. Never restart PipeWire during playback (it stalls the audio DSP) |
| Microphone gain | ALSA `ADC1/ADC3 Volume` | `audio` | 1 | boot default only (`MIC_GAIN`, applied by `tb323fu-audio-defaults`) |
| Auto brightness | desktop setting (GNOME: `org.gnome.settings-daemon.plugins.power ambient-enabled`) | user | 3 | desktop-owned; a separate GNOME tile binds the GSettings key, no daemon involvement |
| Haptics test / strength | evdev `FF_RUMBLE` on the two AW86937 inputs | user (input group) | later | on-screen-keyboard haptics belong to the desktop; a test action in the settings app is optional |
| Board-temperature throttling mode | none at runtime (kernel DT: 43–50 °C steps, Android's default policy) | — | — | a "game mode" with higher trips would need a runtime knob in the kernel first — noted, not planned |

</details>

Not implemented (needs kernel work first): a runtime thermal profile (Android's game-mode skin thresholds), LED ring
effects beyond the charge indicator, touch sampling rate. Chip protection trips (95/105 °C) are never exposed, and
speaker limits stay in layer 1.

## D-Bus API

System bus name **`io.github.joonhoekim.tb323fu.Helper`**, one object per feature under `/io/github/joonhoekim/tb323fu/Helper/…`.
Standard `org.freedesktop.DBus.Properties` for properties (with `PropertiesChanged`), plus methods below.

<details>
<summary>Objects, properties, methods and polkit actions</summary>

| Object / interface | Properties | Methods | Signals | polkit action (default for the active local user) |
|---|---|---|---|---|
| `/…/Battery` · `io.github.joonhoekim.tb323fu.Helper.Battery` | `ChargeLimit` (u, %), `Bypass` (b: limit ≤ capacity with external power → battery idle, current ≈ 0), `Status` (s, raw kernel), `State` (s: `charging` / `discharging` / `bypass` / `full` / `not-charging`, derived from status + current + limit + the Bypass switch: the firmware keeps saying "Charging" while the battery is held, so with Bypass on, external power and \|current\| ≤ 300 mA it is `bypass` (185 mA seen in bypass on a 65 W charger); while the current is still larger — just switched on, or the charger cannot carry the load — it stays `charging` / `discharging`; without the switch, "Charging" at or above the limit with \|current\| ≤ 300 mA is `bypass` too, shown as "Held at Limit"), `Capacity` (u), `CurrentMa` (i), `VoltageMv` (u), `TemperatureC` (d), `Health` (s), `CycleCount` (i, −1 unknown), `DesignCapacityMah` (i, −1 unknown), `ChargerType` (s: UCSI `usb_type`, e.g. `C`, `PD`; `USB` without UCSI, `none`), `ChargerContract` (s: the UCSI contract, e.g. `PD 9.0 V 3.00 A`; the firmware reports 0 for PD/PPS chargers, then the measured input marked `in`: `PPS · 9.2 V in · ~40 W`; `unknown` / `none`), `ChargerAdapter` (s: adapter the battery manager detected — `SDP`, `DCP`, `CDP`, `PD`, `PD_PPS`, … — `""` unknown), `InputVoltageMv` (u), `InputCurrentMa` (u: measured charger input from the battmgr USB supply, 0 unplugged) | `SetChargeLimit(u)`, `SetBypass(b)` | `Changed()` | `…charge-limit` — allow (`yes`) |
| `/…/Android` · `…Android` | `Available` (b: hash file present and `boot_b` recorded), `ImageSha256` (s), `RequireAuth` (b) | `SwitchToAndroid()`, `SetRequireAuth(b)` (`…admin`) | `SwitchingToAndroid()` | `…android-switch` — allow (`yes`); `…android-switch-auth` — `auth_admin_keep`. The daemon checks the second one only when `android.require_auth = true` in the config (default `false`) |
| `/…/Torch` · `…Torch` | `On` (b), `Level` (u), `MaxLevel` (u) | `Set(b)`, `SetLevel(u)` | — | `…torch` — allow |
| `/…/LedRing` · `…LedRing` | `Mode` (s: `charge` / `off`), `Brightness` (u), `LowPercent` (u) | `SetMode(s)`, `SetBrightness(u)`, `SetLowPercent(u)` | — | `…led-ring` — allow |
| `/…/Refresh` · `…Refresh` | `Policy` (s: `off` / `auto` / `manual`), `Rate` (u, manual Hz), `IdleMs60` (u), `IdleMs30` (u), `MinHz` (u), `InputWakes` (b), `LiveRate` (u, read-only from `state`) | `SetPolicy(s)`, `SetRate(u)`, `SetIdle(u ms60, u ms30)`, `ApplyPreset(s)` (`power-saver` 0.5 s/2 s, `balanced` 1 s/5 s, `smooth` 3 s/15 s) | — | `…refresh` — allow |
| `/…/Gpu` · `…Gpu` | `Profile` (s), `FollowPowerProfiles` (b), `Floors` (a{s(uu)}) | `SetProfile(s)` (also stops following), `SetFollowPowerProfiles(b)`, `SetLimits(s profile, u min_mhz, u max_mhz)` | — | `…gpu` — allow (`yes`) for both `SetProfile` and `SetLimits` |
| `/…/Usb` · `…Usb` | `WakeEnabled` (b: USB host/port wakeup from suspend), `DevMode` (b: USB gadget network + serial console for developers) | `SetWake(b)`, `SetDevMode(b)` | — | `…usb-wake` — allow; `…dev-mode` — `auth_admin_keep` |
| `/…/EmergencyKey` · `…EmergencyKey` | `Enabled` (b), `HoldSeconds` (u) — writes `/etc/tb323fu/emergency-key.conf` (`ENABLED`, `HOLD_SECONDS`, 3..30 s) that the layer-1 service reads; the service itself stays in layer 1 | `SetEnabled(b)`, `SetHoldSeconds(u)` | — | `…emergency-key` — allow for enabling/changing time, `auth_admin_keep` for disabling |
| `/…/Diagnostics` · `…Diagnostics` | `CrashRecords` (u: pstore archive entries), `LastBootClean` (b: no panic record archived this boot and the previous boot's journal ends with a clean shutdown — a dump-mode crash leaves no pstore record) | `Export() → s` (path of a tarball with pstore, previous-boot journal tail, dmesg head, versions; user names/addresses/serials stripped) | — | `…diagnostics` — allow |
| `/…/Boot` · `…Boot` | `Roots` (a(ssbs): GPT partition name, os-release PRETTY_NAME, present, init kind `systemd`/`nixos`/`none`), `Default` (s), `Next` (s, one-shot), `Current` (s: the partition `/` came from), `RootHealth` (a{sas}: root → problems for the running kernel — no `lib/modules/$(uname -r)/modules.dep`, no `extra/` (aw882xx amplifier driver), no audio DSP or Wi-Fi firmware; roots without problems and NixOS roots are left out) | `SetNext(s)`, `ClearNext()`, `SetDefault(s)`, `RebootInto(s)`, `Rescan()` | — | `…boot-next` — allow; `…reboot-into` — allow; `…boot-default` — `auth_admin_keep` |
| `/…/Thermal` · `…Thermal` (read-only) | `Surface` (d, °C: skin NTC, else quiet), `CpuMax` (d: hottest `cpu-*`/`cpullc-*` tsens zone), `GpuMax` (d: hottest `gpuss-*`), `Throttling` (b: a `cpufreq-*`/`devfreq-*` cooling device above state 0), `Zones` (a{sd}: board sensors by zone type without `-thermal`: skin, quiet, batt, batt2, usb, usb2-conn, lcm, wlan, ddr, ufs, xo, rear-cam, fcam, wls); NaN = absent | — | — | none (no methods; trips and policies are never written) |
| `/…` · `…Helper` | `Version` (s), `Features` (as), `Kernel` (s), `SeriesTag` (s: patch-series identity if the kernel exposes it, else unknown), `Firmware` (a{ss}: file → sha256 match state vs the manifest) | `Reload()` | — | `…admin` — `auth_admin` |

</details>

Implementation notes: property getters read the device live; a 5 s poller runs the LED-ring charge indicator and the GPU profile follow, re-asserts the USB wakeup setting, and emits `PropertiesChanged` (changed properties invalidated) when values change underneath; `Battery.Changed()` fires on state-like battery changes (not on every current sample), `Thermal` on a change of a whole degree.

Capability detection: each object is only exported when its sysfs interface exists (for example no `Refresh` object on a kernel without the idle-refresh patch).
Inactive/remote sessions get `no` for everything except reading properties.

## Packaging

Files (paths for a normal FHS distribution):

| File | Path |
|---|---|
| daemon | `/usr/libexec/tb323fu/tb323fu-helperd` (packages; `/usr/local/libexec/` for a manual install) |
| CLI | `/usr/bin/tb323fu-ctl` |
| systemd unit | `/usr/lib/systemd/system/tb323fu-helperd.service` (`Type=dbus`, `BusName=io.github.joonhoekim.tb323fu.Helper`, `WantedBy=multi-user.target`, hardening: `ProtectSystem=strict`, `ReadWritePaths=/etc/tb323fu /sys`) |
| D-Bus policy | `/usr/share/dbus-1/system.d/io.github.joonhoekim.tb323fu.Helper.conf` (own: root; send: everyone; polkit decides) |
| D-Bus activation | `/usr/share/dbus-1/system-services/io.github.joonhoekim.tb323fu.Helper.service` (`SystemdService=`) |
| polkit | `/usr/share/polkit-1/actions/io.github.joonhoekim.tb323fu.helper.policy` |
| GNOME extension | `/usr/share/gnome-shell/extensions/tb323fu@joonhoekim.github.io/` (separate package) |
| settings app | `/usr/bin/tb323fu-settings`, `.desktop`, icons (separate package) |

Recipes (in `packaging/`, plus `flake.nix` at the repository root):

| Distribution | Recipe | Packages | Status |
|---|---|---|---|
| Debian / Ubuntu | `packaging/debian/build-debs.sh` (dpkg-deb, no debhelper) | `tb323fu-platform`, `tb323fu-helper`, `tb323fu-settings`, `tb323fu-helper-gnome` | built on the device (arm64), contents checked with `dpkg -c`; `tb323fu-settings` install + remove tested |
| Arch Linux ARM | `packaging/arch/PKGBUILD` (split package, aarch64) | same four | syntax only (`bash -n`); no Arch system tried |
| NixOS | `flake.nix` → `packaging/nix/*.nix`, modules `nixosModules.default` (`services.tb323fu`) and `nixosModules.rootfs` ([`rootfs/nixos`](../rootfs/nixos/configuration.nix)) | same four | built on the device (aarch64, nixos-unstable 2026-09-29) and installed with `rootfs/nixos/build-rootfs.sh`; first boot not yet verified |

- Debian: `tb323fu-helper` `Depends: dbus, polkitd | policykit-1, systemd`, enables `tb323fu-helperd`; `tb323fu-platform` `Depends: systemd, udev, bluez`, `Recommends:` PipeWire/WirePlumber, alsa-ucm-conf, iio-sensor-proxy, hexagonrpcd, qrtr-tools, rmtfs, tqftpserv; its postinst enables the platform units and masks `bootmac-bluetooth` when present. `/etc/tb323fu/*` are conffiles.
- Arch installs the daemon under `/usr/lib/tb323fu/` (Arch has no libexec); `.install` files print the enable commands.
- NixOS: see below.
- `back-to-android` ships with the platform package (`/usr/sbin`); the helper looks it up in `PATH` first, then `/usr/local/sbin`, `/usr/sbin`, `/usr/libexec/tb323fu`. The Android image hash is read from `/etc/tb323fu/android-boot.sha256` (older `/etc/android-boot.sha256` as fallback).

<details>
<summary>NixOS module details</summary>

The platform package is installed with `PREFIX=$out` (install.sh rewrites `/usr/libexec/tb323fu` to the store path); `/etc/tb323fu` stays mutable and gets its defaults once through `systemd.tmpfiles` `C` rules. The module wires udev rules, systemd units (with `path` for the tools the scripts call), D-Bus, polkit, PipeWire/WirePlumber `configPackages`, a merged `ALSA_CONFIG_UCM2`, `LIBCAMERA_IPA_CONFIG_PATH`, and options `helper.enable`, `settingsApp.enable`, `gnome.enable`, `android.requireAuth` (initial value), `androidBootSha256`.

Also: `hexagonrpcd` (nixpkgs `hexagonrpc`) + iio-sensor-proxy when `sensors.dataDir` points at the device's sensor files; the swh LADSPA plugins through `services.pipewire.extraLadspaPackages`; `hardware.wirelessRegulatoryDatabase`; `RuntimeWatchdogSec=30s`; the units' `/lib/firmware` is rewritten to `/run/current-system/firmware` (NixOS links `hardware.firmware` there; the module keeps it uncompressed). The `[Install]` sections of packaged units do not apply on NixOS, so the module sets the same `wantedBy` targets.

</details>

## Tests

<details>
<summary>Test plan (without and on the device)</summary>

Without the device
- Run the daemon against a **fake sysfs root** (`TB323FU_SYSFS_ROOT=/tmp/fake-sys` with the relevant files) on a private D-Bus (`dbus-run-session`): property reads, setters write the right values, capability detection hides objects whose files are missing, persistence round-trips, polkit denials return the right D-Bus error (with a test policy).
- CLI golden-output tests against the same fake daemon.

On the device
- Charge limit: set 60/80 with a PD charger attached; threshold file and charging state follow; the UPower hint is absent (GNOME's switch gone).
- Android switch: `Available` false without the hash file; with it, the auth prompt appears and the switch works (then return to Linux).
- Refresh: policy auto/off/manual, idle presets; the live rate follows (frame-counter check), overview stays open, suspend while stretched resumes at 120 Hz.
- Torch and LED ring: visual check (a person watches).
- Daemon killed or not installed: boot, audio, sensors and the emergency chord still work.

</details>
