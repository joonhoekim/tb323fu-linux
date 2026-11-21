# After installing

What to know once Linux runs on the tablet: the first start, switching between Android and Linux, updates, more
systems on the card, removing Linux, and where to look when something is wrong. The install itself is in
[Installing Linux](install.md).

## First start

The Lenovo logo, then a text summary on the panel, then the desktop. The first start takes longer (the root grows to
fill the partition). Boot times per system: [Distributions](distros.md). GNOME logs you in automatically.

**Wi-Fi:** top right → Wi-Fi → pick your network → a notification asks for the password. Until then the clock is wrong
(Android keeps the real time where Linux cannot read it); it sets itself once the network is up. The time zone comes
from the PC you built on. A Wi-Fi icon with `?` means the internet check failed; it usually clears within a minute.

In GNOME's Terminal:

```sh
tb323fu-check                           # each part PASS / WARN / FAIL, with a hint for each failure
cat /etc/tb323fu/android-boot.sha256    # the hash the script printed in the wayback step
```

`tb323fu-check` comes with platform 0.4.5 or newer; on older installs use `systemctl --failed`. If something fails and
the hint does not help, open an issue and paste `tb323fu-check --markdown`: it holds no device IDs, addresses or
network names, but look it over before posting.

Then try the way back to Android and back to Linux once, while you are at the desk (next section).

In mainline Linux the power button alone does not force the tablet off; hold **power + volume down**.

## Switching between Android and Linux

The tablet boots whatever is in `boot_a`. Android's boot image is kept in `boot_b`; switching copies one or the other
into `boot_a` and restarts. Every way below checks hashes before and after writing and stops without restarting on a
mismatch. How it works: [android/README.md](../android/README.md).

| Direction | How | Checked |
|---|---|---|
| Linux → Android | Open Device Helper → **Android** → Restart into Android, or the Android tile in the quick settings ([helper](helper.md)); in a terminal `back-to-android <hash>` as root | verified |
| Linux → Android, emergency | hold **volume up + volume down for 10 s**; works with a frozen desktop as long as the kernel runs; letting go earlier cancels | verified (Debian; Ubuntu through the guided installer) |
| Android → Linux | KernelSU app → Modules → **Switch to Linux** → Action. Writes the last Linux image that ran (or the staged one) to `boot_a`, verifies it, restarts; on a bad write it copies Android back from `boot_b` first | verified |

A switch takes under a minute. If the Linux image that ran last is a kernel still on trial (below) or one that failed,
Switch to Linux writes the last confirmed kernel instead and says so.

<details>
<summary>From a PC</summary>

| Direction | How | Checked |
|---|---|---|
| Linux running | [`tools/flash-boot.sh`](../tools/flash-boot.sh) over SSH (USB network: tablet `192.168.7.2`, PC `192.168.7.1`) | verified |
| through Android | [`tools/cycle.sh`](../tools/cycle.sh): back to Android if needed, write `boot_a` over adb, reboot, wait for SSH | verified |

</details>

If neither way works: [Recovery](recovery.md) (EDL with your own backup from [rooting](rooting.md)).

## Updates

**Kernel.** Open Device Helper → **About** → Kernel Updates, or `tb323fu-ctl kernel update`. The helper checks the
project's GitHub Releases once a day (it only checks; downloading and installing are your choice). **Stable** is the
default channel ([latest release](https://github.com/joonhoekim/tb323fu-linux/releases/latest)); **Testing** also
offers pre-releases. A new kernel starts on trial and is replaced automatically if it fails to start; channels, **Keep**
and going back: [helper.md → Kernel updates](helper.md#kernel-updates). A kernel you built:
[custom-kernel.md](custom-kernel.md).

**Helper.** Releases tagged `helper-vX.Y.Z` show in **About** → Helper Updates and in `tb323fu-ctl helper`. Whether the
helper updates itself or you update its package depends on how it was installed (Ubuntu: `apt` owns it; Arch Linux ARM:
it updates itself; NixOS: `nix flake update`): [helper.md → Helper updates](helper.md#helper-updates).

**The distribution** updates the usual way (`apt`, `pacman`, `nixos-rebuild`, …). The kernel and its modules come with
the boot image, not from the distribution's packages.

## Optional: the project's Mesa for Vulkan and OpenCL

After the install, every application uses the distribution's Mesa. The project also offers its own Mesa build for this
GPU, Turnip (Vulkan) and rusticl (OpenCL), which is much faster for GPU compute: about 2.7 times the Geekbench 7 OpenCL
score of Debian's Mesa on one tablet, for orientation only ([Performance → GPU](performance.md#gpu)). OpenGL and the
desktop keep using the distribution's Mesa either way. It is optional and comes as a binary release only.

- **Try it:** Open Device Helper → **Performance** → Graphics Drivers → **Check Now**, then Download and Install…, and
  switch on **Use the Project's Mesa**; or `tb323fu-ctl mesa update` and `tb323fu-ctl mesa on`. Applications started
  after the next login use it. The distribution's Mesa stays installed.
- **On trial:** a version you switch on must be confirmed with **Keep** (banner on the Performance page, or
  `tb323fu-ctl mesa keep`). Without Keep, the third start switches back to the distribution's Mesa.
- **Going back:** `tb323fu-ctl mesa off` (from the next login), or **Go Back…** to the previously installed version.
- **One program only:** `tb323fu-mesa run COMMAND` uses the project's Mesa and `tb323fu-mesa distro COMMAND` the
  distribution's, whatever the setting.

Details, and using a release without the helper: [helper.md → Graphics drivers](helper.md#graphics-drivers-mesa).

## Swap (zram)

The root images have no swap partition or file. The platform package sets up compressed swap in RAM at every start
(`tb323fu-zram.service`): a zram device as large as the RAM, zstd, `vm.swappiness=100`, `vm.page-cluster=0`. Idle memory
is compressed, so a working set larger than the RAM keeps running instead of being closed by the out-of-memory
killer.[^zram-test]

```sh
tb323fu-ctl memory zram          # active?, size, stored and compressed, swappiness
tb323fu-ctl memory zram off      # or on; also Open Device Helper → Performance → Compressed Swap
tb323fu-ctl memory zram size 8192   # MiB, or "size ram"
```

Without the helper, edit `/etc/tb323fu/zram.conf` (`ENABLED=0|1`, `SIZE=ram|MiB`, `ALGORITHM=zstd`) and run
`systemctl restart tb323fu-zram`. If the system already has a zram swap (Fedora's zram-generator), the service leaves it
alone. A swap file next to it is fine: zram is used first (priority 100). On NixOS use `services.tb323fu.zram.enable`
(default on) and `services.tb323fu.zram.memoryPercent`.

Other zram sysctls (`vm.watermark_boost_factor=0`, `vm.watermark_scale_factor=125`, `vm.vfs_cache_pressure=50`,
`vm.dirty_background_ratio=5`, `vm.dirty_ratio=10`) showed no gain and are not set.

## Screen rotation

The rotation sensor works on every system that has the sensor support ([Distributions](distros.md)); whether the screen
follows it is the desktop's choice:

- **GNOME** rotates only while no keyboard or mouse is connected, and then also hides the rotation lock tile. To keep
  rotating with one connected: [`userspace/desktop/gnome`](../userspace/desktop/gnome/README.md#rotation-with-a-keyboard-or-mouse-connected).
- **KDE Plasma:** System Settings → Display & Monitor → Orientation **Automatic**; clear "Only when in tablet mode" to
  keep rotating with a keyboard. Needs Qt Sensors (`qt6-sensors` or similar). Untested.
- Window managers without automatic rotation (sway, niri, …): `monitor-sensor` shows the orientation iio-sensor-proxy
  reports, for a script of your own.

## Another system on the card

Each system lives in its own partition, and all share the one kernel in `boot_a`. The guided script puts one system on
the card; add more by hand: a partition named `tb323fu-<name>` in the card's free space and a root built into it
([Installing by hand, steps 4–5](install-manual.md#4-partition-a-microsd-card)). Systems tried and how each is built:
[Distributions](distros.md). To pick one: Open Device Helper → **Systems**, or `tb323fu-ctl boot`
([Installing by hand → Multiboot](install-manual.md#multiboot)).

## Removing Linux

Linux never replaces Android: its data, apps and system stay as they were (unless you moved the Linux root to the
internal storage, last row). To go back to the stock tablet, in this order:

| Step | How | Status |
|---|---|---|
| 1. Android back in `boot_a` | any Linux → Android way above, or EDL → LTBox Flash Partitions → `boot_a` ← your Android boot image ([recovery](recovery.md#linux-does-not-boot)) | verified |
| 2. Remove the Switch to Linux module | KernelSU app → Modules → Switch to Linux → uninstall, restart. Its staged image stays in `/data/adb/tb323fu`: `adb shell su -c 'rm -rf /data/adb/tb323fu'` | untested |
| 3. The card | Android: Settings → Storage → the card → format; or delete the partition (`sgdisk -d`) on a PC | untested |
| 4. `boot_b` | nothing to do: it is never booted. A stock copy is also in your dump and in the firmware package (same version) | — |
| 5. The root (KernelSU, `efisp`) | [Recovery → Undoing the root](recovery.md#undoing-the-root). Erase `efisp` only after step 1 put Lenovo's signed boot back in `boot_a` | Unroot verified; `efisp` erase with stock `boot_a` untested |
| 6. The OTA apps | turned off while [rooting](rooting.md#3-root-on-android-kernelsu); turn them on again: `adb shell pm enable --user 0 com.lenovo.ota`, the same for `com.tblenovo.center`. Before a firmware update, see [Updating Android firmware](firmware-updates.md) | untested |
| a Linux root on the internal storage | write the saved table back (`sgdisk -l lun0-gpt.bak $D`) and factory-reset Android again so `userdata` is formatted at full size (Android: Settings → Reset, or LTBox Flash Firmware → Wipe Data) | untested |

Never restore a partition table backup taken before a firmware update over a newer layout, and never restore another
tablet's table: the backup contains your device's unique GUIDs. A complete factory state (which wipes the tablet):
[Recovery → Full reinstall](recovery.md#full-reinstall).

## Something is wrong

| Symptom | Where to look |
|---|---|
| the install script stops or fails | [Installing from Windows → Troubleshooting](install-windows.md#troubleshooting) (most entries apply on every PC) |
| Linux does not start, or hangs before the desktop | hold volume up + volume down 10 s for Android (once the boot summary is on the screen; a hang before it needs EDL); then [Recovery → Linux does not boot](recovery.md#linux-does-not-boot) |
| Android does not start, red "Your device is corrupt" screen, LTBox stopped half-way | [Recovery](recovery.md) (table at the top) |
| black screen, no reaction to buttons, no USB network | possibly a crash dump state: [Recovery → Getting into EDL](recovery.md#getting-into-edl) |
| no USB network or serial shell after boot, problems of one distribution | [Distributions → Known problems](distros.md#known-problems) |
| a feature does not work (display modes, USB devices, GPS, …) | [Hardware status](hardware-status.md) and its [Known issues](hardware-status.md#known-issues) |
| a kernel update did not come up | it goes back by itself after the third start; [helper.md → Kernel updates](helper.md#kernel-updates) |

Anything else: open an issue on GitHub with what you did and what you saw.

[^zram-test]: In one test, 15–17 GB of memory on the 15 GB tablet was killed every time without swap and kept running with zram (compression about 4.5 times), with no measurable difference in build time or latency.
