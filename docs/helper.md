# Open Device Helper

Open Device Helper (`tb323fu-helper` in this repository; the name is not tied to one device so the parts that are
not TB323FU-specific can move to other ports later) owns the TB323FU's user-facing knobs — charge limit, bypass and charging schedule, restart into Android, torch, RGB-ring
lighting, vibration strength, idle refresh policy, the performance profile (CPU/GPU limits, thermal profile, panel heat limit), USB wakeup, emergency key settings, multiboot selection, kernel updates,
diagnostics — and exposes them over **D-Bus** to a CLI, a GNOME quick-settings tile and a settings app.
It needs only systemd, D-Bus and polkit, and works without any particular desktop. Code and build instructions:
[`helper/`](../helper/README.md). The developer reference (every feature with its kernel interface, the D-Bus API,
packaging, tests): [helper-reference.md](helper-reference.md).

| Part | State |
|---|---|
| `tb323fu-helperd` (system D-Bus service) and `tb323fu-ctl` (CLI) | implemented |
| GNOME Shell quick-settings extension | implemented |
| GTK4 + libadwaita settings app (`tb323fu-settings`) | implemented |
| KDE Plasma applet | planned |

What it does not do:

- **Booting never depends on the helper.** Everything needed to boot and to have working audio, input, sensors and
  the emergency way back to Android stays in plain files and services (layer 1 below).
- No package-manager calls. The helper updates itself from the project's GitHub Releases only where nobody else
  owns it (an `install.sh` install); where a package manager or NixOS installed it, it only says what to do
  ([Helper updates](#helper-updates)). **Kernels** are updated through the helper too, from the same releases or a
  file you built ([Kernel updates](#kernel-updates)).
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
- **GNOME Shell extension** (quick settings, `userspace/desktop/gnome/extension/`), one tile, labeled "Device" in the panel; it talks to the daemon over D-Bus only.
  - **Tile:** the subtitle shows charge and live refresh rate (`80% · 60 Hz`); a tap opens the settings app. Without the daemon the tile still opens the app.
  - **Arrow:** opens the menu. It stays open while choices are made and scrolls on a landscape screen; without the daemon it says the helper is not running.
  - **Menu header:** battery charge and state (`80% · Charging`), plus the charger when plugged in: the contract, or type and measured power (`99% · Bypass · PPS · ~40 W`).
  - **Charge Limit:** a heading with 60% / 80% / 100%; a limit set in the app (e.g. 75%) shows in the heading (`Charge Limit · 75%`). Below it, Bypass Charging.
  - **Refresh policy:** Adaptive, Fixed (N Hz), Always 120 Hz.
  - **Torch:** on/off with a brightness slider.
  - **Open Device Helper…:** opens the settings app.
  - **Errors:** one notification per failed call (no D-Bus names); a canceled authentication shows nothing.
  - Auto Brightness stays a separate GSettings tile.
- **Settings app** (Rust, gtk-rs, GTK4 + libadwaita; `tb323fu-settings [--page NAME]` opens a page directly). A list of
  pages on the left, the page on the right; below 860 sp it becomes list → page navigation. Short values stay on one
  line, long ones (kernel, hashes, paths) sit under the row title with a copy button. The pages:
  - **Battery:** charge limit and recharge gap, Bypass Charging, "charge to 100 % by" a time (days of the week), the
    battery's state, power, health and cycle count, the charger's contract and measured input.
  - **Display:** refresh policy (adaptive, fixed, always 120 Hz), the idle timing presets or custom idle times, panel
    heat protection.
  - **Performance:** the profile (power-saver / balanced / performance) with CPU and GPU limits per profile, low-latency
    Wi-Fi in the performance profile, the thermal profile, CPU boost, and the temperatures.
  - **Lights & Vibration:** torch, LED ring mode and color (a palette of nine; `tb323fu-ctl` takes any `#rrggbb`),
    breathing, notification pulse, vibration strength with a test.
  - **USB:** wake sources, USB-C port roles, developer mode.
  - **Emergency Key:** on/off and how long to hold volume up + volume down.
  - **Systems:** every system the boot loader can see; tap a row for restart into / next start / default. The subtitle
    names the partition and any problem in short (e.g. `baldur-root-sd · No sound or Wi-Fi`).
  - **Android:** restart into Android, and whether that asks for authentication.
  - **Diagnostics:** an export of logs and state for bug reports.
  - **About:** versions and firmware; **Kernel Updates** (the running, trial and good kernels, the channel, check /
    download / install, Keep after a trial, rollback, "Install Kernel from File"; see
    [Kernel updates](#kernel-updates)); **Helper Updates** ([below](#helper-updates)); an About dialog with copyable
    debug information.

  The daemon is polled on a worker thread, so a slow answer (a rescan of the systems mounts SD roots) never freezes
  the window. Adwaita fits GNOME; on other desktops it still runs as a plain app.
- **KDE Plasma applet** (later): QML plasmoid over the same D-Bus API.

## Multiboot

The kernel's initramfs can boot any of these root filesystems (GPT partition names): `baldur-root` (usually UFS), `baldur-root-sd`
and `tb323fu-*` (for example `tb323fu-ubuntu`, `tb323fu-arch`, `tb323fu-nixos` on the SD card). None is required. The selection lives on
the **state root**: the first present of `baldur-root`, `baldur-root-sd`, then the `tb323fu-*` partitions in sorted order (the helper
mounts it under `/run/tb323fu/state` when running from another root):

- `/etc/tb323fu/boot-next` — one-shot: the initramfs reads it, **deletes it before trying the root**, then boots it. A system that hangs
  later therefore boots the default on the next restart.
- `/etc/tb323fu/boot-default` — persistent default (absent = the state root).
- Order tried: next, default, the state root, then every other candidate in the same order; a root that is missing, does not mount or has
  no init is skipped.
- Init per root: `/sbin/init` (Debian, Ubuntu, Arch, Fedora) or NixOS's `/nix/var/nix/profiles/system/init`.
- `/etc/tb323fu/boot-menu` (any content) enables a menu at boot: volume-up moves to the next system, 5 s without a press boots it.
- The modules come with the boot image (shared modules, mounted on the chosen root's `/lib/modules/$(uname -r)` by the initramfs); a root with `/etc/tb323fu/modules` = `own` carries its own tree. Every root needs the firmware files.

The helper's `Boot` object ([D-Bus API](helper-reference.md#d-bus-api)) lists the roots (reading each one's `os-release`, mounted read-only without journal replay, cached) and writes
the selection; when it runs from another root it mounts the state root under `/run/tb323fu/state` for that. CLI: `tb323fu-ctl boot`
(`list`, `next NAME`, `clear`, `default NAME`, `reboot NAME`, `rescan`); the settings app has a "Systems" page.
While reading each root the helper also checks it against the running kernel (`RootHealth`): the audio DSP and Wi-Fi
firmware (also `.zst`/`.xz`), and — only when the running boot image has no shared modules, or for a root in `own` mode —
the modules directory with `modules.dep` and its `extra/` (the out-of-tree aw882xx amplifier driver); an `own`-mode root
with a tree shows "own modules (not updated with the kernel)".
`tb323fu-ctl boot list` prints the problems under the root, and the app warns on the row and in the restart dialog.


## Kernel updates

The kernel (with its device tree, initramfs and — with shared modules — all its modules) is updated through the
helper, from the project's **GitHub Releases** or from a **file you built yourself**
([custom-kernel.md](custom-kernel.md)). How it works in detail (release assets, every check, the state files):
[reference → Kernel updates](helper-reference.md#kernel-updates).

**What you install.** A release carries only a kernel `Image`, never a boot image: the boot image must come from
your own tablet (Lenovo's header, GKI signature and vbmeta blobs). The helper packs the new kernel into **your own
stock boot image** — the copy in `boot_b`, checked against the recorded Android hash first — and writes the result
to `boot_a`.

**Channels.** Releases are numbered (`kernel-tNN`); a release is offered when its number is higher than the running
kernel's. `stable` (default) offers the newest release that is not a pre-release; `testing` offers the newest of
all, pre-releases included. A fork can publish its own releases; its users set `[kernel] source =
"github:OWNER/REPO"` in `/etc/tb323fu/helper.toml`.

**What happens.**

1. **Check**: daily (only checking, never downloading; can be switched off) or on request.
2. **Download**: the helper checks the file against the release's checksums.
3. **Install** (administrator password, reused for a short while): needs battery ≥ 30 % or a charger. The kernel
   in `boot_a` is saved first as the good kernel (`linux-good.img` on the state root), then the new one is written
   to `boot_a` and read back.
4. **Trial**: the new kernel starts on trial. On the stable channel it is kept automatically once the system has
   reached `graphical.target` (or `multi-user.target`) and run 90 s more. Testing-channel kernels, pre-releases and
   kernels from a file are kept only when you press **Keep**.
5. **Going back**: the **third** start of a kernel that was never kept writes the good kernel back into `boot_a`
   and restarts. **Go Back** (app) or `tb323fu-ctl kernel rollback` does the same by hand.

A kernel that dies **before** the initramfs runs cannot be caught (nothing of ours runs). Fastboot is read-only on
this bootloader, so the way back is then EDL, writing `boot_a` from a PC — see
[recovery](recovery.md#linux-does-not-boot). Android's Switch to Linux restores the good kernel instead of a kernel
still on trial or one that failed ([android/README.md](../android/README.md)).

**A kernel from a file.** `tb323fu-ctl kernel install-local PATH` or **Install Kernel from File…** in the app
installs an `Image`, `Image.gz` or a boot image (only its kernel is used) the same way as a release. First the helper
shows what is in the file: release, `/proc/version` banner, whether it carries the shared modules (without them every
root needs its own modules for that release, or it starts without them), and warnings. By default such a kernel is
kept only when you press **Keep** (`--trial`); `--keep` keeps it after 90 s like a stable release. A local install
asks for the administrator's password **every time**: a file passed no checks but the choice of the person
installing it.

**Trust.** The helper checks size, checksums and the kernel's own version banner. The checksums come from the same
place as the kernel, so they catch a damaged download, **not** a changed release: whoever can publish a release in
that repository decides what you install. Optional minisign signatures and the account rules behind the project's
releases: [reference → Kernel updates](helper-reference.md#kernel-updates).

CLI: `tb323fu-ctl kernel [status] | check | notes TAG | download TAG | install TAG [--reboot] | update [--reboot] |
inspect PATH | install-local PATH [--trial|--keep] [--name NAME] [--reboot] [--yes] | keep | rollback [--reboot] |
channel stable|testing | auto-check on|off | helper-notify on|off | dismiss`. Settings app: **About** → Kernel Updates
(status, "Kernel tNN available" with Notes and Download → Install… → Restart Now, Check Now, channel, daily check,
Install Kernel from File…, Go Back while a kernel is on trial), a banner for
Keep or after an automatic rollback; **Systems** shows a line while a kernel is on trial.

## Helper updates

The helper (daemon, CLI, settings app, GNOME extension and their data files) updates **itself** from the same
GitHub Releases as the kernel: a release tagged `helper-vX.Y.Z`. There are no apt or pacman repositories. Release
assets, the allowed paths and how an update is applied: [reference → Helper updates](helper-reference.md#helper-updates).

**Who updates the helper.** Decided by who owns the installed helper, not by the distribution's name:

| Installed by | What happens |
|---|---|
| `install.sh`, the Fedora and SteamOS builders (no package owns it) | the helper updates itself |
| a Debian package | refused; "install the new version's .deb files (`apt install ./tb323fu-helper_*.deb`)" |
| an Arch package | refused; build `packaging/arch/PKGBUILD` and `pacman -U` it |
| an RPM package | refused; update the package |
| NixOS | refused; `nix flake update tb323fu-linux && sudo nixos-rebuild switch` |

**What happens.** The daily kernel check also looks for a newer helper (while "helper notify" is on). Download
needs no password; the helper checks the files and unpacks them. Install asks for the administrator's password,
replaces the files, restarts the daemon and checks that the new one answers within 30 s; if it does not, the old
version goes back by itself. **Go Back…** puts the previous version back (and can itself be undone the same way).
The settings app needs a restart to show its new version, and the GNOME extension's new code loads at the next
login (Wayland). Front-ends and daemons of different versions keep working together.

**Trust.** The same as for kernel releases: whoever can publish releases in the configured repository decides what
runs as root on the tablets that update.

CLI: `tb323fu-ctl helper [status] | check | notes [VERSION] | download [VERSION] | install [VERSION] |
update [VERSION] | rollback` (`update`, `install` and `rollback` wait for the restarted helper and print the
result). Settings app: **About** → Helper Updates (status, "Helper X Available" with Notes and Download →
Update…, or the command when a package manager updates the helper; Go Back… to the previous version).

## Graphics drivers (Mesa)

The project's Mesa build for the Adreno 840 — Turnip (Vulkan) and rusticl (OpenCL) — comes from the same GitHub
Releases, tagged `mesa-<date>`; it is provided as a binary release only. It is installed under
`/var/lib/tb323fu/mesa/` next to the distribution's Mesa, which stays installed and is what every application uses
while the channel is off. One build for every distribution (LLVM linked in; needs glibc 2.38 or newer). GL (the
desktop itself) still comes from the distribution.

**Switching on** writes the Vulkan and OpenCL loader variables (`VK_DRIVER_FILES`, `OCL_ICD_VENDORS`,
`RUSTICL_ENABLE`) to `/var/lib/tb323fu/mesa/env.conf`, which the session reads through
`/etc/environment.d/60-tb323fu-mesa.conf` and login shells through `/etc/profile.d/tb323fu-mesa.sh`; applications
started after the next login use it. A version switched on is **on trial**: it counts the starts, and without
**Keep** the third start switches back to the distribution's Mesa (and says which version failed). **Rollback**
goes back to the previously installed version.

**One command** with either driver, whatever the session setting: `tb323fu-mesa run COMMAND` (the port's) and
`tb323fu-mesa distro COMMAND` (the distribution's) — for comparing, or for one application that misbehaves.

**Without the helper**, a release can be used directly (any directory; nothing is installed system-wide):

```sh
sha256sum -c SHA256SUMS                         # next to tb323fu-mesa-<version>-aarch64.tar.gz
tar xf tb323fu-mesa-<version>-aarch64.tar.gz    # MANIFEST and tree/lib/…
D=$PWD/tree/lib
printf '{"file_format_version": "1.0.1", "ICD": {"library_path": "%s/libvulkan_freedreno.so", "api_version": "1.4.0"}}\n' "$D" > turnip.json
mkdir -p opencl && echo "$D/libRusticlOpenCL.so.1" > opencl/rusticl.icd
VK_DRIVER_FILES=$PWD/turnip.json OCL_ICD_VENDORS=$PWD/opencl RUSTICL_ENABLE=freedreno COMMAND
```

CLI: `tb323fu-ctl mesa [status] | check | notes [VERSION] | download [VERSION] | install [VERSION] | update |
on | off | keep | rollback | run COMMAND… | distro COMMAND…`. Settings app: **Performance** → Graphics Drivers
(status, "Use the Project's Mesa", "Mesa X Available" with Notes and Download → Install…, Check Now, Go Back…; a
Keep banner on the page while a version is on trial). D-Bus: [reference → D-Bus API](helper-reference.md#d-bus-api).

## Persistence

- `android.require_auth` (bool, default `false`): whether switching to Android asks for authentication (polkit `android-switch-auth` instead of `android-switch`). Settable from the app and `tb323fu-ctl`; changing it itself requires authentication.
- `/etc/tb323fu/helper.toml` — written by the daemon when a setting changes, read at start; defaults when the file or a key is absent. A file that does not parse is copied to `helper.toml.bad` before the daemon starts from the defaults.
- Applied once at daemon start (after the relevant devices exist; the daemon waits on udev for the LED/power-supply/devfreq devices, bounded).
- Settings that the kernel already defaults sensibly (idle refresh auto) are only written when the user changes them. The charge limit is always applied by the helper at start (default 80 %; with Bypass on, the battery is held at its capacity at start).
- The state of the automatic rules is kept there too, so a restart picks up where it left off: Bypass switched on or declined during the performance thermal profile (`thermal.bypass_auto`, `thermal.bypass_declined`), the brightness the panel heat limit will give back (`thermal.panel_saved`), and the interfaces whose Wi-Fi power saving the helper switched off (`wifi.power_save_off`).
- The legacy `/etc/baldur/*.conf` files are read once on first start to migrate values, then left untouched.

## Not exposed

Interfaces the kernel offers (often as 0644 files) that the helper deliberately leaves alone, and features that were
considered and skipped.

| What | Why |
|---|---|
| Chip protection trips (CPU 95, GPU 105, `hot` 120 / `critical` 125 °C, PMIC 95/115/145 °C), every `hot` trip, zone `mode` / `policy` / `emul_temp`, cooling-device `cur_state` | the last line of defense; writable only because mainline marks every DT trip writable. The thermal profile writes `quiet-thermal` passive trips only, below a fixed 58 °C ceiling |
| Speaker amplifier volume, profile and monitor controls (`aw_dev_*`), bypassing the protection filter | safe only behind the PipeWire protection filter; can damage the speakers (layer 1) |
| Charge current and USB input current (`input_current_limit`) | ignored by the firmware on PD/PPS chargers — a switch would only pretend to work |
| Charge limit below 20 %, recharge threshold above the limit | deep storage charge is bad for the battery |
| Backlight current, brightness above `max_brightness`, flash strobe (2 A) | panel/LED lifetime, heat; the torch stays at torch level |
| 144/165 Hz panel modes, DSI timings, bandwidth votes | underrun every frame on this kernel; bandwidth votes are a TrustZone/RPMh hazard |
| cpuidle state `disable`, CPU hotplug, cpufreq governor, `mem_sleep` deep | no gain, and they would interfere with the idle-state settings of the kernel; deep sleep saves no power here |
| `msm` and `ath12k` debug parameters | debugging only (GPU ACD off changes voltage margins) |
| Wi-Fi transmit power, regulatory country | radio regulations; the system's regulatory database decides |
| remoteproc restart, NPU, `/dev/mem` register writes (vendor LED-ring effects, amplifier registers) | a stopped remoteproc kills the SoC; TrustZone kills the system on secure registers; the driver state would diverge |
| USB-C power role ("don't power connected devices") | `typec/port*/power_role` is writable, but with UCSI it is a per-connection PR swap, refused without a partner (`EIO` on the device); a swap hands the source role to the other device (a phone would then charge the tablet) rather than switching power off, and a failed swap needs a replug. Nothing persistent to switch, and not tried on the device (port 0 carries the development link). The roles are shown read-only (`Usb.Ports`) |
| Wake from a Bluetooth keyboard | the Bluetooth UART (`1994000.serial`) has no wakeup interrupt in the device tree (no `power/wakeup` on the serial device; only the serdev controller's flag, which routes nothing) — needs a device-tree change first |
| Panel color modes (sRGB / P3) | the mainline DPU offers no gamma/3D LUT on this panel (`GAMMA_LUT` size 0), so only an ICC profile in the compositor could do it. The vendor 17³ calibration LUTs (`/vendor/etc/display/panel_baldur_*_C3D_cali_{srgb,p3}.txt`) are per-panel vendor data that cannot be shipped, and converting them into ICC profiles is untested; a layer-1 tool would have to build them from the user's own vendor partition first |
| Touch sampling rate, game / palm / glove / edge modes | the mainline `nt36536_ts` driver has no sysfs or proc nodes for them; the firmware commands must be worked out from the vendor driver first |

## Reference

Every feature with its kernel interface, privileges and hazards, the D-Bus API with its polkit actions, packaging
and tests: [Open Device Helper: reference](helper-reference.md).
