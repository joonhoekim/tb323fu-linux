# Installing Linux

From a stock Lenovo Legion Tab Gen 5 (TB323FU) to mainline Linux on the microSD card, with Android kept on the
tablet as the way back. This page is the whole path in order; the steps on your PC are on the page for its system.

## The path

| | What | Where it happens | Guide |
|---|---|---|---|
| 1 | **Back up** the whole tablet (an EDL dump), **root** Android without unlocking (LTBox, KernelSU) | Android + a PC tool | [Rooting](rooting.md) |
| 2 | **Install**: firmware copied from Android, the way back set up, the release kernel packed into your own boot image, the card partitioned, a root filesystem built on the PC and written to the card, Linux written to `boot_a` | one script on the PC, talking to rooted Android | the page for your PC, below |
| 3 | **First start**, then switch between the systems | the tablet | [After installing](after-install.md#first-start) |
| 4 | Keep it up to date: kernel releases, the helper; remove Linux again if you want | Linux, Open Device Helper | [After installing](after-install.md#updates) |

If something goes wrong on the way: [Recovery](recovery.md). The tablet always keeps Android's boot image in `boot_b`:
holding **volume up + volume down for 10 s** in Linux puts it back and starts Android.

## Your PC

Step 2 is one script, [`tools/install/install.sh`](../tools/install/README.md). It needs a Linux system on the PC
that can run arm64 programs (to build the tablet's root filesystem) and `adb` to reach the tablet.

| PC | How the script runs | Status ([labels](hardware-status.md#labels-on-other-pages)) |
|---|---|---|
| **Windows 10/11** | in Ubuntu under WSL2, with the Windows `adb.exe` | **followed end to end** on a tablet (2026-10-04) → [Installing from Windows](install-windows.md) |
| **Linux** | directly, with the system's `adb`; on NixOS in an Ubuntu container | **followed end to end** once from NixOS through the container, up to the first start (2026-10-04); a plain Debian/Ubuntu PC not tried → [Installing from Linux](install-linux.md) |
| **macOS** | in an arm64 Linux virtual machine (Lima), using the Mac's `adb` server; no USB passthrough | **followed end to end** once, up to the first start (2026-10-04) → [Installing from macOS](install-macos.md) |

The result is the same on every PC: Ubuntu 26.04 with GNOME (unless you [pick another system](#which-system)) on a
partition `baldur-root-sd` of the microSD card, the release kernel in `boot_a`, Android's boot image in `boot_b`, and
Open Device Helper installed. Allow 1–2 hours, most of it an unattended build.

## Which system

The tablet is not tied to one distribution: the kernel and the boot chain are the same for all, and each system lives
in its own partition. [Distributions](distros.md) lists what has been tried, how each one is built and what its
builder takes care of.

| | System | How |
|---|---|---|
| **Start here** | Ubuntu 26.04 with GNOME | the guided script, as above — followed end to end |
| | Arch Linux ARM with GNOME | the same script with `DISTRO=arch` — followed end to end once; no rotation sensor |
| | NixOS with GNOME | the same script with `DISTRO=nixos` — followed end to end once; its configuration is editable on the tablet ([how](distros.md#nixos-your-own-configuration)) |
| | SteamOS (community port) | the same script with `DISTRO=steamos` — experimental: Gaming Mode works on kernel t39 or later, Switch to Desktop does not yet |
| | Fedora (with FEX for x86 games) | built on the tablet itself, from a first Linux root: [Distributions → Installing each one](distros.md#installing-each-one) |
| | Armada (experimental, `DISTRO=armada`), anything else (postmarketOS, Debian, your own) | Armada boots into Gaming Mode; the others are not tried yet; what a root needs and how to plug your own builder into the script: [Distributions → Your own distribution](distros.md#your-own-distribution) |

Several systems can share the card; you pick one with Open Device Helper → Systems or `tb323fu-ctl boot`.

## Beyond the script

[Installing by hand](install-manual.md) lists every step as commands, with what each one changes and how far it
was tested. If you know your way around Linux, it is the recommended way: every step is visible and yours to change.
Use it for a distribution the script does not build (Fedora, or your own: [distros.md](distros.md)), a kernel
you build yourself ([custom-kernel.md](custom-kernel.md)), several systems side by side on the card, or a root on the
internal storage (which costs Android its data).
