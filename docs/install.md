# Installing Linux

From a stock Lenovo Legion Tab Gen 5 (TB323FU) to mainline Linux on the microSD card, with Android kept on the
tablet as the way back. This page is the whole path in order; the steps on your PC are on the page for its system.

## The path

| | What | Where it happens | Guide |
|---|---|---|---|
| 1 | **Back up** the whole tablet (an EDL dump), **root** Android without unlocking (LTBox, KernelSU) | Android + a PC tool | [Rooting and dual boot setup](rooting.md) |
| 2 | **Install**: firmware copied from Android, the way back set up, the release kernel packed into your own boot image, the card partitioned, a root filesystem built on the PC and written to the card, Linux written to `boot_a` | one script on the PC, talking to rooted Android | the page for your PC, below |
| 3 | **First start**, then switch between the systems | the tablet | [first start](install-windows.md#6-first-start-and-the-way-back) |
| 4 | Keep it up to date: kernel releases, the helper | Linux, Open Device Helper | [helper.md](helper.md) |

If something goes wrong on the way: [Recovery](recovery.md). The tablet always keeps Android's boot image in `boot_b`:
holding **volume up + volume down for 10 s** in Linux puts it back and starts Android.

## Your PC

Step 2 is one script, [`tools/install/install.sh`](../tools/install/README.md). It needs a Linux system on the PC
that can run arm64 programs (to build the tablet's root filesystem) and `adb` to reach the tablet.

| PC | How the script runs | Status |
|---|---|---|
| **Windows 10/11** | in Ubuntu under WSL2, with the Windows `adb.exe` | **followed end to end** on a tablet (2026-10-04) → [Installing from Windows](install-windows.md) |
| **Linux** | directly, with the system's `adb` | **not verified separately**: the same script and builders → [Installing from Linux](install-linux.md) |
| **macOS** | in an arm64 Linux virtual machine, with the tablet passed through over USB | **not verified** → [Installing from macOS](install-macos.md) |

The result is the same on every PC: Ubuntu 26.04 with GNOME on a partition `baldur-root-sd` of the microSD card,
the release kernel in `boot_a`, Android's boot image in `boot_b`, and Open Device Helper installed. Allow 1–2 hours,
most of it an unattended build.

## Beyond the script

[Installing by hand](install-manual.md) lists every step as commands, with what each one changes and how far it
was tested. Use it for another distribution ([distros.md](distros.md): Arch, Fedora, NixOS, SteamOS), a kernel
you build yourself ([custom-kernel.md](custom-kernel.md)), several systems side by side on the card, or a root on the
internal storage (which costs Android its data).
