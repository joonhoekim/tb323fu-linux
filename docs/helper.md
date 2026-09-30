# TB323FU helper — design

Status: **design, not implemented.** This document describes a small helper suite for the device-specific features of the
Lenovo Legion Tab Gen 5 / Y700 5th Gen (TB323FU) that today live in a handful of shell scripts and GNOME Shell extensions.
The goal is to make them work on any systemd distribution and any desktop.

## Goals and non-goals

Goals
- One place that owns the device's user-facing knobs (sysfs, kernel module parameters) and exposes them over **D-Bus**.
- Works on Debian, Ubuntu, Arch Linux ARM and NixOS; depends only on systemd, D-Bus and polkit.
- Works without any particular desktop: every feature is reachable from a CLI; desktop front-ends are thin.
- Settings persist across reboots and are applied at boot by the daemon.
- Privileged actions go through polkit with sensible defaults.

Non-goals
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

## Feature inventory

"Today" names the current implementation in this project's development root filesystem; those files move to the listed layer.

| Feature | Today | Kernel / sysfs interface | Privilege | Layer | Notes and hazards |
|---|---|---|---|---|---|
| Charge limit | udev `61-…-battery.rules` sets `CHARGE_LIMIT=70,80` for UPower/GNOME "preserve battery health" | `/sys/class/power_supply/qcom-battmgr-bat/charge_control_end_threshold` | root | 2 (+1 udev hint kept) | **Decided: the helper is the only owner.** The `CHARGE_LIMIT` udev hint is dropped so UPower/GNOME's "preserve battery health" switch no longer writes the threshold (two writers would overwrite each other); the helper's app and quick settings replace it |
| Switch to Android | `baldur-to-android` (zenity dialog → `pkexec back-to-android <sha256>`), GNOME tile `baldur-android@…`, `.desktop` entry | writes the Android boot image kept in `boot_b` back into `boot_a` and reboots; refuses unless `boot_b` matches the recorded SHA-256 | root, **auth required** | 2 (tool in 1) | `back-to-android` itself stays a layer-1 tool (also used by the emergency chord). The daemon method only wraps it: checks the hash file exists, emits a signal, runs it. Irreversible until the user switches back from Android |
| Emergency chord (volume up+down 10 s → Android) | `baldur-voldown` + `keyhold` | evdev on `pmic_resin` and `gpio-keys` | root | **1** | must work with no desktop and no helper; stays a plain service |
| Torch | `baldur-torch` + GNOME tile `baldur-torch@…` | `/sys/class/leds/white:flash/brightness` (torch level ~96; never the flash strobe) | `video` group via udev `74-…-leds.rules` | 2 | daemon exposes on/off/level with a safe maximum; front-ends no longer write sysfs |
| LED ring (charge indicator / effects) | `baldur-ledring` (amber charging, green full or held, red ≤ LOW %), `/etc/baldur/ledring.conf` | `/sys/class/leds/aw22127:rgb:indicator/{brightness,multi_intensity}` | `video` group | 2 | fold the charge-indicator loop into the daemon (it already watches power-supply uevents). Vendor lighting effects need register-level programming (not the LED class) — out of scope for phase 1 |
| Idle refresh rate | kernel (series patch 0110), default auto; the retired user daemon had a quick-settings menu: on/off, presets, "calm before 60 Hz / 30 Hz" sliders | `/sys/module/msm/parameters/y705_idle_{policy,hz,ms60,ms30,min_hz,input,selfflush,state}` | root (parameters are 0644 root) | 2 | brings back the quick-settings menu lost when the policy moved into the kernel. The kernel always restores 120 Hz before suspend or display-off, so the daemon needs no sleep handling. `selfflush` is a debug knob — not exposed. Parameter names still carry the development prefix; renaming them is a kernel-series task |
| Panel modes (90 / 120 / 164 Hz) | compositor (mutter/gdctl, KWin): normal display settings | DRM modes from the panel driver | user (compositor) | 3 (no daemon) | handled by each desktop's display settings; the helper does not switch modes |
| GPU performance floor/cap per power profile | `baldur-gpu-profile` follows power-profiles-daemon; `/etc/baldur/gpu.conf` | `/sys/class/devfreq/3d00000.gpu/{min,max}_freq` | root | 2 | keep following power-profiles-daemon when present; otherwise the daemon's own profile property. Widen before narrowing (min ≤ max at every step) |
| Speaker level (protected mode) | user unit `baldur-speaker-gain` (amp attenuation 21 while the PipeWire protection filter runs, 159 otherwise), `/etc/baldur/audio.conf` | ALSA controls `aw_dev_0_volume`, `aw_dev_1_volume` | `audio` group | 1 (unchanged) | **safety-relevant**: the loud value is only safe behind the filter chain. Do not move into the daemon; do not expose a free slider. Never restart PipeWire during playback (it stalls the audio DSP) |
| Microphone gain | `baldur-audio-defaults` (`MIC_GAIN`) | ALSA `ADC1/ADC3 Volume` | `audio` | 1 | boot default only |
| Auto brightness | GNOME tile `baldur-autobrightness@…` bound to `org.gnome.settings-daemon.plugins.power ambient-enabled` | desktop setting | user | 3 | desktop-owned; the GNOME extension keeps binding the GSettings key, no daemon involvement |
| Haptics test / strength | test script only (evdev `FF_RUMBLE` on the two AW86937 inputs) | evdev | user (input group) | later | on-screen-keyboard haptics belong to the desktop; a test action in the settings app is optional |
| Board-temperature throttling mode | kernel DT (43–50 °C steps, Android default policy) | none at runtime | — | — | a "game mode" with higher trips would need a runtime knob in the kernel first — noted, not planned |

## D-Bus API (sketch)

System bus name **`io.github.joonhoekim.tb323fu.Helper`**, one object per feature under `/io/github/joonhoekim/tb323fu/Helper/…`.
Standard `org.freedesktop.DBus.Properties` for properties (with `PropertiesChanged`), plus methods below.

| Object / interface | Properties | Methods | Signals | polkit action (default for the active local user) |
|---|---|---|---|---|
| `/…/Battery` · `io.github.joonhoekim.tb323fu.Helper.Battery` | `ChargeLimit` (u, %), `Status` (s), `Capacity` (u) | `SetChargeLimit(u)` | — | `io.github.joonhoekim.tb323fu.helper.charge-limit` — allow (`yes`) |
| `/…/Android` · `…Android` | `Available` (b: hash file present and `boot_b` recorded), `ImageSha256` (s) | `SwitchToAndroid()` | `SwitchingToAndroid()` | `…android-switch` — allow (`yes`); `…android-switch-auth` — `auth_admin_keep`. The daemon checks the second one only when `android.require_auth = true` in the config (default `false`) |
| `/…/Torch` · `…Torch` | `On` (b), `Level` (u), `MaxLevel` (u) | `Set(b)`, `SetLevel(u)` | — | `…torch` — allow |
| `/…/LedRing` · `…LedRing` | `Mode` (s: `charge` / `off`), `Brightness` (u), `LowPercent` (u) | `SetMode(s)`, `SetBrightness(u)`, `SetLowPercent(u)` | — | `…led-ring` — allow |
| `/…/Refresh` · `…Refresh` | `Policy` (s: `off` / `auto` / `manual`), `Rate` (u, manual Hz), `IdleMs60` (u), `IdleMs30` (u), `MinHz` (u), `InputWakes` (b), `LiveRate` (u, read-only from `state`) | `SetPolicy(s)`, `SetRate(u)`, `SetIdle(u ms60, u ms30)`, `ApplyPreset(s)` | — | `…refresh` — allow |
| `/…/Gpu` · `…Gpu` | `Profile` (s), `FollowPowerProfiles` (b), `Floors` (a{s(uu)}) | `SetProfile(s)`, `SetLimits(s profile, u min_mhz, u max_mhz)` | — | `…gpu` — allow (`yes`) for both `SetProfile` and `SetLimits` |
| `/…` · `…Helper` | `Version` (s), `Features` (as: detected capabilities) | `Reload()` | — | `…admin` — `auth_admin` |

Capability detection: each object is only exported when its sysfs interface exists (for example no `Refresh` object on a kernel without the idle-refresh patch).
Inactive/remote sessions get `no` for everything except reading properties.

## Persistence

- `android.require_auth` (bool, default `false`): whether switching to Android asks for authentication (polkit `android-switch-auth` instead of `android-switch`). Settable from the app and `tb323fu-ctl`; changing it itself requires authentication.
- `/etc/tb323fu/helper.toml` — written by the daemon when a setting changes, read at start; defaults when the file or a key is absent.
- Applied once at daemon start (after the relevant devices exist; the daemon waits on udev for the LED/power-supply/devfreq devices, bounded).
- Settings that the kernel already defaults sensibly (idle refresh auto) are only written when the user changes them. The charge limit is always applied by the helper at start (default 80 %).
- The legacy `/etc/baldur/*.conf` files are read once on first start to migrate values, then left untouched.

## Front-ends

- **`tb323fu-ctl`** (Rust, same crate as the daemon's client library): `tb323fu-ctl status`, `charge-limit 80`, `android --yes`, `torch on`, `refresh auto|off|manual 60`, `refresh idle 1000 5000`, `gpu profile balanced`. Always installed; the reference for scripting.
- **GNOME Shell extension** (quick settings): Flashlight toggle, "Restart into Android" tile, Adaptive Refresh toggle with presets and the two idle sliders (the menu the retired refresh daemon had), Auto Brightness (GSettings, unchanged). Talks to the daemon over D-Bus only.
- **Settings app** (Rust, gtk-rs, GTK4 + libadwaita): pages for Battery, Display (refresh policy), Lights (torch, LED ring), Performance (GPU), System (Android switch, versions). Adwaita fits GNOME; on other desktops it still runs as a plain app.
- **KDE Plasma applet** (later): QML plasmoid over the same D-Bus API.

## Packaging

Files (paths for a normal FHS distribution):

| File | Path |
|---|---|
| daemon | `/usr/libexec/tb323fu-helperd` |
| CLI | `/usr/bin/tb323fu-ctl` |
| systemd unit | `/usr/lib/systemd/system/tb323fu-helperd.service` (`Type=dbus`, `BusName=io.github.joonhoekim.tb323fu.Helper`, `WantedBy=multi-user.target`, hardening: `ProtectSystem=strict`, `ReadWritePaths=/etc/tb323fu /sys`) |
| D-Bus policy | `/usr/share/dbus-1/system.d/io.github.joonhoekim.tb323fu.Helper.conf` (own: root; send: everyone; polkit decides) |
| D-Bus activation | `/usr/share/dbus-1/system-services/io.github.joonhoekim.tb323fu.Helper.service` (`SystemdService=`) |
| polkit | `/usr/share/polkit-1/actions/io.github.joonhoekim.tb323fu.helper.policy` |
| GNOME extension | `/usr/share/gnome-shell/extensions/tb323fu@tb323fu.github.io/` (separate package) |
| settings app | `/usr/bin/tb323fu-settings`, `.desktop`, icons (separate package) |

- **Debian / Ubuntu**: `tb323fu-helper` (daemon + CLI), `tb323fu-helper-gnome`, `tb323fu-settings`; `Depends: dbus, polkitd`. Platform files ship as `tb323fu-platform`.
- **Arch Linux ARM**: one PKGBUILD with split packages (same split); `depends=(dbus polkit)`.
- **NixOS**: a module `services.tb323fu.helper.enable` that adds the package, `systemd.packages`, `services.dbus.packages`, `security.polkit` actions, and optional declarative settings rendered to `/etc/tb323fu/helper.toml` (read-only in that case; the daemon then reports settings as managed and refuses writes).

## Migration from today's pieces

| Today | Becomes |
|---|---|
| `baldur-torch`, `baldur-torch@…` extension | Torch object + GNOME tile |
| `baldur-to-android`, `baldur-android@…` extension, polkit action for `back-to-android` | Android object (wraps `back-to-android`, which stays in layer 1) |
| `baldur-ledring` service + `ledring.conf` | LedRing object (migrates the conf) |
| `baldur-gpu-profile` service + `gpu.conf` | Gpu object (migrates the conf) |
| retired refresh daemon's quick-settings menu | Refresh object + GNOME menu |
| `baldur-autobrightness@…` | stays a GNOME-only tile (GSettings) |
| `baldur-speaker-gain`, `baldur-audio-defaults`, `baldur-voldown`, `baldur-btaddr`, `baldur-usb-port`, `baldur-dsp`, `baldur-gen-ids`, udev/UCM/PipeWire files | layer 1, renamed to the `tb323fu-` prefix in the platform package |

## Prototype plan

1. **Phase 1** — daemon + CLI with three features: charge limit, Android switch, idle refresh policy. `zbus` for D-Bus, `polkit` via `org.freedesktop.PolicyKit1` calls, TOML persistence.
2. **Phase 2** — GNOME Shell extension (Flashlight, Android tile, Adaptive Refresh menu); torch and LED ring objects in the daemon.
3. **Phase 3** — GTK4 + libadwaita settings app; GPU object; migration of the remaining legacy configs.

## Test plan

Without the device
- Run the daemon against a **fake sysfs root** (`TB323FU_SYSFS_ROOT=/tmp/fake-sys` with the relevant files) on a private D-Bus (`dbus-run-session`): property reads, setters write the right values, capability detection hides objects whose files are missing, persistence round-trips, polkit denials return the right D-Bus error (with a test policy).
- CLI golden-output tests against the same fake daemon.

On the device
- Charge limit: set 60/80 with a PD charger attached; threshold file and charging state follow; the UPower hint is absent (GNOME's switch gone).
- Android switch: `Available` false without the hash file; with it, the auth prompt appears and the switch works (then return to Linux).
- Refresh: policy auto/off/manual, idle presets; the live rate follows (frame-counter check), overview stays open, suspend while stretched resumes at 120 Hz.
- Torch and LED ring: visual check (a person watches).
- Daemon killed or not installed: boot, audio, sensors and the emergency chord still work.

## Open questions

- ~~Final D-Bus name~~ decided: `io.github.joonhoekim.tb323fu.Helper`. Neither the Android switch (default no authentication, `android.require_auth`) nor the GPU limits need a password; the daemon runs as root, so this is only polkit policy.
- ~~Charge limit owner~~ decided: the helper (UPower's udev hint removed from the platform files).
- Whether the kernel parameters get distribution-neutral names before the helper depends on them.
- LED ring effects beyond the charge indicator need a small kernel or register-level interface first.
