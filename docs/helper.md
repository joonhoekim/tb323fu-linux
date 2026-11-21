# Open Device Helper

Open Device Helper (`tb323fu-helper` in this repository) owns the TB323FU's user-facing knobs: charge limit, bypass and
charging schedule, restart into Android, torch, RGB-ring lighting, vibration, idle refresh, performance profile,
USB wakeup, emergency key, multiboot selection, kernel updates and diagnostics. It exposes them over **D-Bus** to a CLI,
a GNOME quick-settings tile and a settings app. It needs only systemd, D-Bus and polkit, and works without any
particular desktop. Code and build: [`helper/`](../helper/README.md). Every feature with its kernel interface, the D-Bus
API, packaging and tests: [helper-reference.md](helper-reference.md).

| Part | State |
|---|---|
| `tb323fu-helperd` (system D-Bus service) and `tb323fu-ctl` (CLI) | implemented |
| GNOME Shell quick-settings extension | implemented |
| GTK4 + libadwaita settings app (`tb323fu-settings`) | implemented |
| KDE Plasma applet | planned |

What it does not do:

- **Booting never depends on the helper.** Everything needed to boot, and for audio, input, sensors and the way back to
  Android, stays in plain files and services (layer 1 below).
- No package-manager calls. It updates itself from the project's GitHub Releases only where nobody else owns it; where a
  package manager owns it, it says what to do ([Helper updates](#helper-updates)). Kernels are updated through it too
  ([Kernel updates](#kernel-updates)).
- No new hardware drivers; it only drives interfaces the kernel already exposes.

## Layers

| Layer | What | Examples | Depends on |
|---|---|---|---|
| **1. Platform files** | static configuration and boot-critical services; always installed; work with no helper and no desktop | udev rules, ALSA UCM, PipeWire speaker-protection filter and WirePlumber rule, libcamera tuning, services (Bluetooth address, DSP start, audio defaults, USB port role, emergency volume chord, per-install ID) | systemd, udev |
| **2. Helper daemon** | `tb323fu-helperd`: one system service; owns the knobs, D-Bus API, polkit checks, persistence | charge limit, Android switch, torch, LED ring, refresh policy, GPU profile | systemd, D-Bus, polkit |
| **3. Front-ends** | talk only to the daemon's D-Bus API (and the desktop's own settings store for desktop-owned settings) | `tb323fu-ctl`, GNOME extension, settings app, KDE applet (later) | layer 2 |

The structure follows the Tab Companion of [ubuntu-galaxy-tab-s9-ultra](https://github.com/agcarbajo/ubuntu-galaxy-tab-s9-ultra)
without its coupling to Ubuntu, GNOME and apt.

## Front-ends

- **`tb323fu-ctl`** (CLI, always installed, the reference for scripting; full list in
  [helper/README.md](../helper/README.md#using-it)): `tb323fu-ctl status`, `charge-limit 80`, `android --yes`,
  `torch on`, `refresh auto|off|manual 60`, `refresh idle 1000 5000`, `gpu profile balanced`.
- **GNOME Shell extension** (`userspace/desktop/gnome/extension/`): one quick-settings tile, "Device". It talks to the
  daemon over D-Bus only. Without the daemon the tile still opens the app and the menu says the helper is not running.
  - **Tile:** subtitle shows charge and refresh rate (`80% · 60 Hz`); a tap opens the settings app. The arrow opens the menu.
  - **Menu:** battery charge and state, plus the charger when plugged in (`99% · Bypass · PPS · ~40 W`); Charge Limit
    (60 / 80 / 100 %, or a value set in the app) with Bypass Charging below; Refresh policy (Adaptive, Fixed 30-164 Hz,
    Full Rate); Torch with a brightness slider; Open Device Helper….
  - Errors show as one notification per failed call; a canceled authentication shows nothing. Auto Brightness stays a
    separate GSettings tile.
- **Settings app** (`tb323fu-settings [--page NAME]`; GTK4 + libadwaita, runs as a plain app on other desktops). Pages:
  - **Battery:** charge limit and recharge gap, Bypass Charging (Charge Current method keeps the charger at full power,
    Charge Limit lets it drop to 5 V), Restart Stalled Charging, "charge to 100 % by" a time and weekdays, battery and
    charger state.
  - **Display:** refresh policy, idle timing presets or custom times, panel heat protection.
  - **Performance:** profile (power-saver / balanced / performance) with CPU and GPU limits, low-latency Wi-Fi, thermal
    profile, CPU boost, compressed swap ([zram](after-install.md#swap-zram)), temperatures, Graphics Drivers
    ([Mesa](#graphics-drivers-mesa)).
  - **Lights & Vibration:** torch, LED ring mode and color (nine-color palette; `tb323fu-ctl` takes any `#rrggbb`),
    breathing, notification pulse, vibration strength with a test.
  - **USB:** wake sources, USB-C port roles, developer mode.
  - **Emergency Key:** on/off and hold time for volume up + volume down.
  - **Systems:** every system the boot loader can see; tap a row for restart into / next start / default. Problems show
    in the subtitle (`baldur-root-sd · No sound or Wi-Fi`).
  - **Android:** restart into Android, and whether that asks for authentication.
  - **Diagnostics:** an export of logs and state for bug reports.
  - **About:** versions and firmware, [Kernel Updates](#kernel-updates), [Helper Updates](#helper-updates), and an About
    dialog with copyable debug information.
- **KDE Plasma applet** (later): QML plasmoid over the same D-Bus API.

## Multiboot

The kernel's initramfs can boot any of these root filesystems (GPT partition names): `baldur-root` (usually UFS),
`baldur-root-sd`, and `tb323fu-*` (e.g. `tb323fu-ubuntu`, `tb323fu-arch`, `tb323fu-nixos` on the SD card). None is
required. The selection lives on the **state root**: the first present of `baldur-root`, `baldur-root-sd`, then the
`tb323fu-*` partitions in sorted order.[^state-mount]

- `/etc/tb323fu/boot-next`: one-shot. The initramfs reads it, **deletes it before trying the root**, then boots it, so
  a system that hangs later boots the default on the next restart.
- `/etc/tb323fu/boot-default`: persistent default (absent = the state root).
- Order tried: next, default, the state root, then every other candidate in the same order. A root that is missing,
  does not mount or has no init is skipped.
- Init per root: `/sbin/init` (Debian, Ubuntu, Arch, Fedora) or NixOS's `/nix/var/nix/profiles/system/init`.
- `/etc/tb323fu/boot-menu` (any content) enables a boot menu: volume-up moves to the next system, 5 s without a press
  boots it.
- Modules come with the boot image (shared modules, mounted on the chosen root's `/lib/modules/$(uname -r)`). A root
  with `/etc/tb323fu/modules` = `own` carries its own tree. Every root needs the firmware files.

CLI: `tb323fu-ctl boot` (`list`, `next NAME`, `clear`, `default NAME`, `reboot NAME`, `rescan`); the app has the
**Systems** page. The helper's `Boot` object ([D-Bus API](helper-reference.md#d-bus-api)) reads each root's `os-release`
(mounted read-only, cached) and checks it against the running kernel (`RootHealth`): audio DSP and Wi-Fi firmware, and,
when the boot image has no shared modules or the root is in `own` mode, the modules directory with `modules.dep` and
`extra/` (the aw882xx amplifier driver). `tb323fu-ctl boot list` prints the problems under the root; the app warns on
the row and in the restart dialog.

## Kernel updates

The kernel (with its device tree, initramfs and, with shared modules, all its modules) is updated through the helper,
from the project's **GitHub Releases** or from a **file you built** ([custom-kernel.md](custom-kernel.md)). Release
assets, every check and the state files: [reference → Kernel updates](helper-reference.md#kernel-updates).

**What you install.** A release carries only a kernel `Image`, never a boot image: that must come from your own tablet
(Lenovo's header, GKI signature and vbmeta blobs). The helper packs the new kernel into **your own stock boot image**
(the copy in `boot_b`, checked against the recorded Android hash first) and writes the result to `boot_a`.

**Channels.** Releases are numbered `kernel-tNN`; one is offered when its number is higher than the running kernel's.
`stable` (default) offers the newest stable release; `testing` offers the newest of all, pre-releases included.
`tb323fu-ctl kernel install kernel-tNN` installs a given release. A fork's users set `[kernel] source =
"github:OWNER/REPO"` in `/etc/tb323fu/helper.toml`.

**What happens.**

1. **Check:** daily (checking only, never downloading; can be switched off) or on request.
2. **Download:** the helper checks the file against the release's checksums.
3. **Install** (administrator password, reused for a short while): needs battery at 30 % or more, or a charger. The
   kernel in `boot_a` is saved first as the good kernel (`linux-good.img` on the state root), then the new one is
   written to `boot_a` and read back.
4. **Trial:** the new kernel starts on trial. On the stable channel it is kept automatically once the system has
   reached `graphical.target` (or `multi-user.target`) and run 90 s more. Testing-channel kernels, pre-releases and
   kernels from a file are kept only when you press **Keep**.
5. **Going back:** the **third** start of a kernel that was never kept writes the good kernel back into `boot_a` and
   restarts. **Go Back** (app) or `tb323fu-ctl kernel rollback` does the same by hand.

A kernel that dies **before** the initramfs runs cannot be caught. Fastboot is read-only on this bootloader, so the way
back is EDL, writing `boot_a` from a PC: [recovery](recovery.md#linux-does-not-boot). Android's Switch to Linux restores
the good kernel instead of one still on trial or one that failed ([android/README.md](../android/README.md)).

**A kernel from a file.** `tb323fu-ctl kernel install-local PATH` or **Install Kernel from File…** installs an `Image`,
`Image.gz` or a boot image (only its kernel is used) the same way. The helper first shows what is in the file: release,
`/proc/version` banner, whether it carries the shared modules (without them every root needs its own modules for that
release), and warnings. Such a kernel is kept only on **Keep** (`--trial`, the default); `--keep` keeps it after 90 s
like a stable release. A local install asks for the administrator's password **every time**.

**Trust.** The checks catch a damaged download, **not** a changed release: whoever can publish a release in the
configured repository decides what you install ([reference → Kernel updates](helper-reference.md#kernel-updates)).

CLI: `tb323fu-ctl kernel [status] | check | notes TAG | download TAG | install TAG [--reboot] | update [--reboot] |
inspect PATH | install-local PATH [--trial|--keep] [--name NAME] [--reboot] [--yes] | keep | rollback [--reboot] |
channel stable|testing | auto-check on|off | helper-notify on|off | dismiss`. App: **About** → Kernel Updates (status,
Notes, Download → Install… → Restart Now, Check Now, channel, daily check, Install Kernel from File…, Go Back while on
trial), with a banner for Keep or after an automatic rollback; **Systems** shows a line while a kernel is on trial.

## Helper updates

The helper (daemon, CLI, settings app, GNOME extension and data files) updates **itself** from the same GitHub Releases,
tagged `helper-vX.Y.Z`. There are no apt or pacman repositories. Release assets, allowed paths and how an update is
applied: [reference → Helper updates](helper-reference.md#helper-updates).

**Who updates the helper** depends on who owns the installed files:

| Installed by | What happens |
|---|---|
| `install.sh`, the Fedora and SteamOS builders, the guided Arch Linux ARM install (no package owns it) | the helper updates itself |
| a Debian package (the guided Ubuntu install) | refused; "install the new version's .deb files (`apt install ./tb323fu-helper_*.deb`)" |
| an Arch package | refused; build `packaging/arch/PKGBUILD` and `pacman -U` it |
| an RPM package | refused; update the package |
| NixOS | refused; `nix flake update tb323fu-linux && sudo nixos-rebuild switch` |

**What happens.** The daily kernel check also looks for a newer helper (while "helper notify" is on). Download needs no
password. Install asks for the administrator's password, replaces the files, restarts the daemon and checks that the new
one answers within 30 s; if not, the old version goes back by itself. **Go Back…** restores the previous version (and can
itself be undone). The settings app needs a restart to show its new version; the GNOME extension loads its new code at
the next login. Front-ends and daemons of different versions keep working together.

**Trust** is the same as for kernel releases, and the helper runs as root.

CLI: `tb323fu-ctl helper [status] | check | notes [VERSION] | download [VERSION] | install [VERSION] |
update [VERSION] | rollback` (`update`, `install` and `rollback` wait for the restarted helper and print the result).
App: **About** → Helper Updates (status, Notes, Download → Update…, or the command when a package manager owns the
helper; Go Back…).

## Graphics drivers (Mesa)

The project's Mesa build for the Adreno 840, Turnip (Vulkan) and rusticl (OpenCL), comes from the same GitHub Releases,
tagged `mesa-<date>`, as a binary release only. It is installed under `/var/lib/tb323fu/mesa/` next to the
distribution's Mesa, which stays installed and is what every application uses while the channel is off. One build serves
every distribution (LLVM linked in; needs glibc 2.38 or newer). GL, including the desktop itself, still comes from the
distribution.

**Switching on** writes the loader variables (`VK_DRIVER_FILES`, `OCL_ICD_VENDORS`, `RUSTICL_ENABLE`) to
`/var/lib/tb323fu/mesa/env.conf`, read by the session through `/etc/environment.d/60-tb323fu-mesa.conf` and by login
shells through `/etc/profile.d/tb323fu-mesa.sh`; applications started after the next login use it. A version switched on
is **on trial**: without **Keep**, the third start switches back to the distribution's Mesa (and says which version
failed). **Rollback** goes back to the previously installed version.

**One command** with either driver, whatever the session setting: `tb323fu-mesa run COMMAND` (the port's) and
`tb323fu-mesa distro COMMAND` (the distribution's), for comparing or for one application that misbehaves.

<details>
<summary>Without the helper</summary>

A release can be used directly from any directory; nothing is installed system-wide:

```sh
sha256sum -c SHA256SUMS                         # next to tb323fu-mesa-<version>-aarch64.tar.gz
tar xf tb323fu-mesa-<version>-aarch64.tar.gz    # MANIFEST and tree/lib/…
D=$PWD/tree/lib
printf '{"file_format_version": "1.0.1", "ICD": {"library_path": "%s/libvulkan_freedreno.so", "api_version": "1.4.0"}}\n' "$D" > turnip.json
mkdir -p opencl && echo "$D/libRusticlOpenCL.so.1" > opencl/rusticl.icd
VK_DRIVER_FILES=$PWD/turnip.json OCL_ICD_VENDORS=$PWD/opencl RUSTICL_ENABLE=freedreno COMMAND
```

</details>

CLI: `tb323fu-ctl mesa [status] | check | notes [VERSION] | download [VERSION] | install [VERSION] | update |
on | off | keep | rollback | run COMMAND… | distro COMMAND…`. App: **Performance** → Graphics Drivers (status, "Use the
Project's Mesa", Notes, Download → Install…, Check Now, Go Back…; a Keep banner while on trial). D-Bus:
[reference → D-Bus API](helper-reference.md#d-bus-api).

## Persistence

- `/etc/tb323fu/helper.toml` is written by the daemon when a setting changes and read at start; defaults apply when the
  file or a key is absent. A file that does not parse is copied to `helper.toml.bad` and the daemon starts from the
  defaults.
- `android.require_auth` (bool, default `false`): whether switching to Android asks for authentication (polkit
  `android-switch-auth` instead of `android-switch`). Changing it itself requires authentication.
- Settings are applied once at daemon start, after the devices exist (the daemon waits on udev, bounded). Kernel
  defaults (e.g. idle refresh auto) are written only when you change them. The charge limit is always applied at start
  (default 80 %; with Bypass on, the battery is held at its capacity at start).
- The state of the automatic rules is kept there too (Bypass switched on or declined during the performance thermal
  profile, the brightness the panel heat limit will give back, interfaces whose Wi-Fi power saving was switched off).
- The legacy `/etc/baldur/*.conf` files are read once on first start to migrate values, then left alone.

## Not exposed

Interfaces the kernel offers that the helper deliberately leaves alone.

<details>
<summary>The list, with reasons</summary>

| What | Why |
|---|---|
| Chip protection trips (CPU 95, GPU 105, `hot` 120 / `critical` 125 °C, PMIC 95/115/145 °C), zone `mode` / `policy` / `emul_temp`, cooling-device `cur_state` | the last line of defense; writable only because mainline marks every DT trip writable. The thermal profile writes `quiet-thermal` passive trips only, below a fixed 58 °C ceiling |
| Speaker amplifier volume, profile and monitor controls (`aw_dev_*`), bypassing the protection filter | safe only behind the PipeWire protection filter; can damage the speakers |
| Charge current as a setting of its own, USB input current (`input_current_limit`) | charge current is used only by Bypass (Charge Current method); the firmware ignores the input current on PD/PPS chargers, so a switch would only pretend to work |
| Charge limit below 20 %, recharge threshold above the limit | deep storage charge is bad for the battery |
| Backlight current, brightness above `max_brightness`, flash strobe (2 A) | panel/LED lifetime, heat; the torch stays at torch level |
| The vendor 144/165 Hz timings, DSI timings, bandwidth votes | the vendor timings underrun every frame (the kernel offers 144 and 164 Hz with its own); bandwidth votes are a TrustZone/RPMh hazard |
| cpuidle state `disable`, CPU hotplug, cpufreq governor, `mem_sleep` deep | no gain; they would interfere with the kernel's idle-state settings; deep sleep saves no power here |
| `msm` and `ath12k` debug parameters | debugging only (GPU ACD off changes voltage margins) |
| Wi-Fi transmit power, regulatory country | radio regulations; the system's regulatory database decides |
| remoteproc restart, NPU, `/dev/mem` register writes | a stopped remoteproc kills the SoC; TrustZone kills the system on secure registers |
| USB-C power role | with UCSI it is a per-connection swap that hands the source role to the other device and needs a replug when it fails;[^power-role] roles are shown read-only (`Usb.Ports`) |
| Wake from a Bluetooth keyboard | the Bluetooth UART (`1994000.serial`) has no wakeup interrupt in the device tree; needs a device-tree change first |
| Panel color modes (sRGB / P3) | the mainline DPU offers no gamma/3D LUT on this panel, so only an ICC profile in the compositor could do it;[^color-lut] not done |
| Touch sampling rate, game / palm / glove / edge modes | the mainline `nt36536_ts` driver has no sysfs or proc nodes for them |

</details>

[^state-mount]: When the helper runs from another root, it mounts the state root under `/run/tb323fu/state`.
[^power-role]: `typec/port*/power_role` is writable but refused without a partner (`EIO`); untested on the device, as port 0 carries the development link.
[^color-lut]: The vendor 17³ calibration LUTs (`/vendor/etc/display/panel_baldur_*_C3D_cali_{srgb,p3}.txt`) are per-panel vendor data that cannot be shipped; converting them into ICC profiles is untested.
