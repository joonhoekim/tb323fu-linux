# After installing

What to know once Linux runs on the tablet: the first start, switching between Android and Linux, updates, more
systems on the card, how to remove Linux again, and where to look when something is wrong. The install itself is in
[Installing Linux](install.md).

## First start

The Lenovo logo, then a text summary on the panel, then the desktop. The first start takes a little longer (the root
grows to fill the partition); after that the desktop is up about 12 s after the kernel starts. GNOME logs you in
automatically.

**Wi-Fi:** top right → Wi-Fi → pick your network → a notification asks for the password. Until then the clock is
wrong (Android keeps the real time in a place Linux cannot read); it sets itself once the network is up. The time
zone comes from the PC you built on. A Wi-Fi icon with a `?` means the internet check failed; it usually clears
within a minute.

In GNOME's Terminal:

```sh
uname -r                                # the release kernel
systemctl is-system-running             # running, or degraded (see systemctl --failed)
cat /etc/tb323fu/android-boot.sha256    # the hash the script printed in the wayback step
```

Then try the way back to Android and back to Linux once, while you are at the desk (next section).

In mainline Linux the power button alone does not force the tablet off; hold **power + volume down**.

## Switching between Android and Linux

The tablet boots whatever is in `boot_a`. Android's boot image is kept in `boot_b`; switching copies one or the other
into `boot_a` and restarts. Every way below checks hashes before and after writing and stops without restarting on a
mismatch. How it works: [android/README.md](../android/README.md).

| Direction | How | Checked |
|---|---|---|
| Linux → Android | Open Device Helper → **Android** → Restart into Android, or the Android tile in the quick settings ([helper](helper.md)); in a terminal `back-to-android <hash>` as root | yes |
| Linux → Android, emergency | hold **volume up + volume down for 10 s**; works with a frozen desktop as long as the kernel runs; letting go earlier cancels | yes (Debian; Ubuntu through the guided installer) |
| Android → Linux | KernelSU app → Modules → **Switch to Linux** → Action. Writes the last Linux image that ran (or the staged one) to `boot_a`, verifies it, restarts; on a bad write it copies Android back from `boot_b` first | yes |

A switch takes about 40 s. If the Linux image that ran last is a kernel still on trial (below) or one that failed,
the Switch to Linux module writes the last confirmed kernel instead and says so.

<details>
<summary>From a PC</summary>

| Direction | How | Checked |
|---|---|---|
| from a PC, Linux running | [`tools/flash-boot.sh`](../tools/flash-boot.sh) over SSH (USB network: tablet `192.168.7.2`, PC `192.168.7.1`) | yes |
| from a PC, through Android | [`tools/cycle.sh`](../tools/cycle.sh): back to Android if needed, write `boot_a` over adb, reboot, wait for SSH | yes, many times |

</details>

If neither way works: [Recovery](recovery.md) (EDL with your own backup from [rooting](rooting.md)).

## Updates

**Kernel.** Open Device Helper → **About** → Kernel Updates, or `tb323fu-ctl kernel update`. The helper checks the
project's GitHub Releases once a day (it only checks; downloading and installing are your choice) and packs the new
kernel into your own stock boot image from `boot_b`. The channel is **stable** by default: it offers the newest
release that is not a pre-release (`kernel-t40` at the time of writing); **testing** offers pre-releases too. A kernel
from a file you built: **Install Kernel from File…** ([custom-kernel.md](custom-kernel.md)).

**Trial and Keep.** A new kernel is first on trial. Before writing it, the helper saves the running one as
`linux-good.img`. On the stable channel the kernel is confirmed by itself once the system has been up for 90 s. On the
testing channel, for pre-releases and for kernels from a file, only **Keep** (in the app, or `tb323fu-ctl kernel
keep`) confirms it. A kernel that is not confirmed by its third start is replaced by `linux-good.img` and the tablet
restarts. A kernel that dies before the initramfs runs cannot be caught this way; then the way back is EDL
([recovery](recovery.md#linux-does-not-boot)). Details: [helper.md → Kernel updates](helper.md#kernel-updates).

**Helper.** Releases tagged `helper-vX.Y.Z` (`helper-v0.4.0` is the current stable one) show in **About** → Helper
Updates and in `tb323fu-ctl helper`. Who installs them depends on who installed the helper
([helper.md → Helper updates](helper.md#helper-updates)):

| System (guided script) | How the helper was installed | Updating it |
|---|---|---|
| Ubuntu | the release's `.deb` packages, owned by `apt` | the helper does not replace itself; it shows the new version and the command: install the new release's `.deb` files (`sudo apt install ./tb323fu-helper_*.deb` …) |
| Arch Linux ARM | the release's `.deb` files, unpacked without a package manager | nobody owns the files, which helper.md counts as `self`: the helper updates itself |
| NixOS | Nix packages from this repository | `nix flake update tb323fu-linux && sudo nixos-rebuild switch` |

On other systems Open Device Helper shows the command when a package manager owns the helper.

**The distribution** updates the usual way (`apt`, `pacman`, `nixos-rebuild`, …). The kernel and its modules come
with the boot image, not from the distribution's packages.

## Optional: the project's Mesa for Vulkan and OpenCL

After the install, every application uses the distribution's Mesa, and nothing changes that by itself. The project
also offers its own Mesa build for this GPU — Turnip (Vulkan) and rusticl (OpenCL) — which is much faster for GPU
compute (OpenCL programs, Vulkan compute; on this tablet Geekbench 7 OpenCL about 20000 instead of about 7500).
OpenGL and the desktop itself keep using the distribution's Mesa either way. It is optional and comes as a binary
release only.

> The scores were measured on one tablet (the author's) and are for orientation only; yours may differ.

- **Try it:** Open Device Helper → **Performance** → Graphics Drivers → **Check Now**, then Download and Install…, and
  switch on **Use the Project's Mesa**; or `tb323fu-ctl mesa update` and `tb323fu-ctl mesa on`. It is used by
  applications started after the next login. The distribution's Mesa stays installed next to it.
- **On trial:** a version you switch on has to be confirmed with **Keep** (the banner on the Performance page, or
  `tb323fu-ctl mesa keep`). Without Keep, the third start switches back to the distribution's Mesa by itself.
- **Going back:** switch it off (`tb323fu-ctl mesa off`, used from the next login), or **Go Back…** to the previously
  installed version.
- **One program only:** `tb323fu-mesa run COMMAND` runs one program with the project's Mesa and `tb323fu-mesa distro
  COMMAND` with the distribution's, whatever the setting — handy for comparing, or when one application misbehaves.

Details, and using a release without the helper: [helper.md → Graphics drivers](helper.md#graphics-drivers-mesa).
Measured results: [Performance → GPU](performance.md#gpu).

## Another system on the card

Each system lives in its own partition, and all of them share the one kernel in `boot_a`. The guided script puts one
system on the card; more are added by hand: a partition named `tb323fu-<name>` in the card's free space and a root
built into it ([Installing by hand, steps 4–5](install-manual.md#4-partition-a-microsd-card)). Which systems have
been tried and how each one is built: [Distributions](distros.md). Picking one: Open Device Helper → **Systems**, or
`tb323fu-ctl boot` ([Installing by hand → Multiboot](install-manual.md#multiboot)).

## Removing Linux

Linux never replaces Android: Android's data, apps and system stay as they were (unless you moved the Linux root to
the internal storage, last row). Going back to the stock tablet, in this order:

| Step | How | Status |
|---|---|---|
| 1. Android back in `boot_a` | any Linux → Android way above, or EDL → LTBox Flash Partitions → `boot_a` ← your Android boot image ([recovery](recovery.md#linux-does-not-boot)) | verified |
| 2. Remove the Switch to Linux module | KernelSU app → Modules → Switch to Linux → uninstall, restart. Its staged image stays in `/data/adb/tb323fu`: `adb shell su -c 'rm -rf /data/adb/tb323fu'` | untested |
| 3. The card | Android: Settings → Storage → the card → format; or delete the partition (`sgdisk -d`) on a PC | untested |
| 4. `boot_b` | nothing to do: it is never booted. A stock copy is also in your dump and in the firmware package (same version) | — |
| 5. The root (KernelSU, `efisp`) | [Recovery → Undoing the root](recovery.md#undoing-the-root) | from LTBox's source |
| 6. The OTA apps | turned off while [rooting](rooting.md#3-root-on-android-kernelsu); turn them on again: `adb shell pm enable --user 0 com.lenovo.ota`, the same for `com.tblenovo.center`. Before a firmware update, see [Recovery → Updating Android firmware later](recovery.md#updating-android-firmware-later) | untested |
| a Linux root on the internal storage | write the saved table back (`sgdisk -l lun0-gpt.bak $D`) and factory-reset Android again so `userdata` is formatted at full size (Android: Settings → Reset, or LTBox Flash Firmware → Wipe Data) | untested |

Never restore a partition table backup taken before a firmware update over a newer layout, and never restore another
tablet's table: the backup contains your device's unique GUIDs. A complete factory state (which wipes the tablet):
[Recovery → Full reinstall](recovery.md#full-reinstall).

## Something is wrong

| Symptom | Where to look |
|---|---|
| the install script stops or fails | [Installing from Windows → Troubleshooting](install-windows.md#troubleshooting) (most entries apply on every PC) |
| Linux does not start, or hangs before the desktop | hold volume up + volume down 10 s for Android; then [Recovery → Linux does not boot](recovery.md#linux-does-not-boot) |
| Android does not start, red "Your device is corrupt" screen, LTBox stopped half-way | [Recovery](recovery.md) (table at the top) |
| black screen, no reaction to buttons, no USB network | possibly a crash dump state: [Recovery → Getting into EDL](recovery.md#getting-into-edl) |
| no USB network or serial shell after boot, problems of one distribution | [Distributions → Known problems](distros.md#known-problems) |
| a feature does not work (display modes, USB devices, GPS, …) | [Hardware status](hardware-status.md) and its [Known issues](hardware-status.md#known-issues) |
| a kernel update did not come up | it goes back by itself after the third start; [helper.md → Kernel updates](helper.md#kernel-updates) |

Anything else: open an issue on GitHub with what you did and what you saw.
