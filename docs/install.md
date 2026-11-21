# Installing Linux

From a stock Lenovo Legion Tab Gen 5 (TB323FU) to mainline Linux on the microSD card, with Android kept on the
tablet as the way back. Allow 1–2 hours, most of it an unattended build.

## The path

| | What | Guide |
|---|---|---|
| 1 | **Back up** the whole tablet (an EDL dump) and **root** Android without unlocking (LTBox, KernelSU) | [Rooting](rooting.md) |
| 2 | **Install** with one script on your PC, talking to the rooted tablet: it copies the firmware, sets up the way back, packs the release kernel, partitions the card, builds a root filesystem and writes Linux to `boot_a` | the page for [your PC](#your-pc) |
| 3 | **First start**, then switch between the systems | [After installing](after-install.md#first-start) |
| 4 | Keep it up to date (kernel releases, the helper), or remove Linux | [After installing](after-install.md#updates) |

The result is the same on every PC: Ubuntu 26.04 with GNOME (unless you [pick another system](#which-system)) on the
partition `baldur-root-sd` of the microSD card, the release kernel in `boot_a`, Android's boot image in `boot_b`, and
Open Device Helper installed.

> **Android firmware:** tested on ZUI 18.0.12.104; on 18.0.12.187 Linux hangs during boot
> ([#9](https://github.com/joonhoekim/tb323fu-linux/issues/9)). Keep the OTA apps disabled:
> [Updating Android firmware](firmware-updates.md).

**If something goes wrong:** [Recovery](recovery.md). The tablet keeps Android's boot image in `boot_b`; holding
**volume up + volume down for 10 s** in Linux puts it back and starts Android.

## Your PC

Step 2 is one script, [`tools/install/install.sh`](../tools/install/README.md). It needs a Linux system on the PC that
can run arm64 programs (to build the root filesystem) and `adb` to reach the tablet.

| PC | How the script runs | Guide | Status ([labels](hardware-status.md#evidence-labels)) |
|---|---|---|---|
| **Windows 10/11** | in Ubuntu under WSL2, with the Windows `adb.exe` | [Installing from Windows](install-windows.md) | verified |
| **Linux** | directly, with the system's `adb`; on NixOS in an Ubuntu container | [Installing from Linux](install-linux.md) | verified from NixOS up to the first start; a plain Debian/Ubuntu PC untested |
| **macOS** | in an arm64 Linux virtual machine (Lima), using the Mac's `adb` server; no USB passthrough | [Installing from macOS](install-macos.md) | verified up to the first start |

## Prepare the tablet

The same on every PC, before the script runs:

1. Start the tablet in **Android**. (If it runs this project's Linux: Open Device Helper → **Android** → **Restart
   into Android**.)
2. **USB debugging:** Settings → About tablet → tap the build number seven times; then Developer options →
   **USB debugging** on.
3. Connect it to the PC. On the tablet, allow USB debugging for this computer (tick **Always allow**).
4. In the **KernelSU** app → Superuser: allow **Shell**.

Check on the PC (on Windows in PowerShell, not in WSL): `adb devices` lists the tablet as `device`.

If the script runs in a VM or container that reaches the PC's own `adb` server (macOS, NixOS), the `adb` inside and the
PC's must speak the same server version (`adb version`, first line); otherwise the inner `adb` restarts the server and
loses the tablet.

## What the script asks

Each step explains itself first. Steps that write to the tablet make you **type a word** (`WRITE BOOT_B`, `ERASE`,
`WRITE`, `FLASH BOOT_A`); anything else stops without writing. Finished steps are remembered in
`~/tb323fu-install/state`: after a failure, fix what it says and run `tools/install/install.sh` again.
`tools/install/install.sh STEP` runs one step again.

| Step | What happens | What you do | Time |
|---|---|---|---|
| `host` | installs the packages it needs (`debootstrap`, `qemu-user-binfmt`, `gdisk`, …), checks that arm64 programs run through qemu, finds `adb` (on Windows `adb.exe`) | your password for `sudo` | 1–10 min |
| `tablet` | checks the tablet: TB323FU, slot `_a`, root for Shell, the microSD card, battery | answer the prompts on the tablet; confirm you have your EDL dump | 1 min |
| `firmware` | copies the firmware Linux needs from Android's `/vendor` to the PC (nothing on the tablet changes) | — | 1 min |
| `wayback` | makes sure `boot_b` holds your Android boot image (the way back), installs the **Switch to Linux** KernelSU module, saves the hash and your stock boot image | type `WRITE BOOT_B` only if `boot_b` needs the copy | 1 min |
| `download` | the newest kernel release and the Ubuntu packages for the tablet from GitHub Releases, checked against `SHA256SUMS` | browser download only without a GitHub login | 1–3 min |
| `bootimg` | packs the release kernel into **your own** stock boot image ([why](install-manual.md#why-there-is-no-ready-made-bootimg)) | — | seconds |
| `sdcard` | builds a partition table on the PC and writes it to the card from Android: one partition `baldur-root-sd` (64 GB on a large card, else the whole card) | pick the size; type `ERASE` | 1 min |
| `rootfs` | builds Ubuntu 26.04 with GNOME into an image file on the PC (arm64 through qemu), then shrinks and packs it | your user name for the tablet; a password for it (needed for `sudo`, asked after the build) | **20–60 min** |
| `write` | copies the image to the tablet and writes it into the card's partition, then reads it back and compares | type `WRITE` | 3–10 min |
| `boot` | writes the Linux boot image to `boot_a` (checked; on a bad write it puts Android back first) and restarts into Linux | type `FLASH BOOT_A` | 1 min |
| `firstboot` | prints what a good first start looks like and what to check | — | — |

The tablet can stay connected during the `rootfs` build. After the first start: [After installing](after-install.md#first-start).

## Which system

Each system lives in its own partition; the kernel and boot chain are the same for all. The default is Ubuntu. Another
one is the same script with `DISTRO=<name>`. [Distributions](distros.md) has what works on each and how each is built.

| System | `DISTRO=` | Status |
|---|---|---|
| **Ubuntu 26.04 with GNOME** (start here) | `ubuntu` (default) | verified |
| Debian 13 with GNOME | `debian` | verified |
| Arch Linux ARM with GNOME | `arch` | verified; no rotation sensor |
| Fedora 44 with GNOME, FEX for x86 games | `fedora` | verified; no rotation sensor |
| NixOS with GNOME | `nixos` | verified; no rotation sensor; [editable configuration](distros.md#nixos-your-own-configuration) |
| SteamOS (community port) | `steamos` | experimental: Gaming Mode works on kernel t39 or later, Switch to Desktop does not ([known problems](distros.md#known-problems)) |
| Armada | `armada` | experimental: boots into Gaming Mode |
| postmarketOS v26.06 with Phosh | `postmarketos` | verified; `DESKTOP=plasma-mobile` or `gnome-mobile` experimental |
| your own | – | [Your own distribution](distros.md#your-own-distribution) |

Several systems can share the card; pick one with Open Device Helper → Systems or `tb323fu-ctl boot`. Each of them
also installs into a partition on the internal storage that you made beforehand (`TARGET=ufs`,
[tools/install/README.md](../tools/install/README.md)).

## Beyond the script

[Installing by hand](install-manual.md) lists every step as commands, with what each changes. Use it for a distribution
the script does not build ([distros.md](distros.md)), a kernel you build yourself ([custom-kernel.md](custom-kernel.md)),
several systems side by side on the card, or a root on the internal storage (which costs Android its data).
