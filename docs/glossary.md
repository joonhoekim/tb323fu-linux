# Glossary

Terms used throughout the documentation, each with a link to where it is explained.

## Booting and partitions

| Term | Meaning here |
|---|---|
| ABL | Android bootloader (Qualcomm LinuxLoader); draws the fastboot screen and verifies `boot`/`init_boot` ([rooting](rooting.md#2-root-without-unlocking-ltbox)) |
| AVB | Android Verified Boot; `green` = locked and verified, `red` = rejected |
| `boot` / `init_boot` | kernel / first-stage ramdisk (where the KernelSU loader lives) |
| `boot_a` / `boot_b` | the two copies of the `boot` partition. The tablet always boots `boot_a`; Linux runs by writing its boot image there. `boot_b` is never booted and keeps a copy of your stock Android boot image ([android/README.md](../android/README.md#the-model)) |
| the way back | that copy in `boot_b`, with its SHA-256 recorded at install time: copying it into `boot_a` starts Android again. Every tool checks the hash before writing ([install-manual.md, step 2](install-manual.md#2-the-way-back-androids-boot-image-in-boot_b), [switching](after-install.md#switching-between-android-and-linux)) |
| virtual A/B | small partitions exist twice, `super` only once; slot `_b` cannot boot here |
| LUN | a UFS logical unit; most boot partitions are on LUN 4, `userdata` on LUN 0 |
| GBL / `efisp` | an EFI application ABL loads from the `efisp` partition; the patched one makes ABL treat itself as unlocked ([rooting](rooting.md#2-root-without-unlocking-ltbox)) |
| `baldur-root`, `baldur-root-sd`, `tb323fu-*` | GPT partition names the initramfs looks for: `baldur-root` usually on the internal storage (UFS), `baldur-root-sd` on the microSD card, `tb323fu-*` (e.g. `tb323fu-ubuntu`) for more systems on the card. Roots are always found by name, never by number ([initramfs](../kernel/initramfs/README.md#root-partitions-and-multiboot)) |
| state root | the first present of `baldur-root`, `baldur-root-sd`, then the `tb323fu-*` partitions in sorted order. It holds the boot selection (`/etc/tb323fu/boot-next`, `boot-default`) and the kernel-update state (`/var/lib/tb323fu/`) for all systems ([helper.md → Multiboot](helper.md#multiboot)) |
| root selection | the built-in initramfs picks the root to boot: the one-shot next choice, the default, the state root, then every other candidate; optionally a boot menu (volume up) ([initramfs](../kernel/initramfs/README.md#root-partitions-and-multiboot)) |
| shared modules | the kernel's modules travel inside its boot image as one squashfs, which the initramfs mounts on the chosen root's `/lib/modules/<release>`; so a kernel update needs nothing installed in the roots. A root can opt out (`own` mode) ([initramfs](../kernel/initramfs/README.md#kernel-modules-shared)) |

## Kernel updates

| Term | Meaning here |
|---|---|
| trial | a newly installed kernel starts on trial: the initramfs counts its starts, and the third start of a kernel that was never kept writes the good kernel back ([helper.md → Kernel updates](helper.md#kernel-updates)) |
| Keep | confirms a kernel on trial by hand (Open Device Helper or `tb323fu-ctl kernel keep`). Stable-channel kernels are kept automatically after 90 s; testing-channel kernels and kernels from a file wait for Keep |
| good kernel | the last confirmed kernel, saved as `linux-good.img` on the state root; what a failed trial, Go Back and Android's Switch to Linux fall back to |
| release (`kernel-tNN`, `helper-vX.Y.Z`) | the kernels and helper versions published on the project's GitHub Releases; a kernel release is an `Image`, never a boot image ([install-manual.md](install-manual.md#why-there-is-no-ready-made-bootimg)) |

## Android side and recovery

| Term | Meaning here |
|---|---|
| EDL / 9008 | Qualcomm emergency download mode (USB `05c6:9008`); Sahara uploads the loader, Firehose reads and writes partitions. The real recovery mode ([recovery](recovery.md#modes-and-how-to-reach-them)) |
| 900E | crash dump mode (USB `05c6:900E`); one-shot dump session, not a way to write; the tablet resets afterwards |
| LTBox | the PC tool that backs up the tablet over EDL and roots Android without unlocking the bootloader (patched GBL plus KernelSU) ([rooting](rooting.md#2-root-without-unlocking-ltbox)) |
| KernelSU | the root manager on Android, in `init_boot` (LKM mode) ([rooting](rooting.md#3-root-on-android-kernelsu)) |
| Switch to Linux | the KernelSU module whose Action button writes the Linux boot image to `boot_a`, verifies it and restarts ([android/README.md](../android/README.md#the-model)) |
| `back-to-android` | the Linux tool that copies `boot_b` into `boot_a` and restarts into Android; the emergency key (volume up + volume down for 10 s) runs it too ([after-install.md](after-install.md#switching-between-android-and-linux)) |

## Software of this project

| Term | Meaning here |
|---|---|
| Open Device Helper | the system service (`tb323fu-helperd`), CLI (`tb323fu-ctl`), settings app and GNOME quick-settings tile for the tablet's own settings: charging, refresh rate, performance, lights, multiboot, kernel and helper updates ([helper.md](helper.md)) |
| guided installer | `tools/install/install.sh`: one script on the PC that does the whole install against a rooted tablet ([install.md](install.md), [tools/install](../tools/install/README.md)) |
| distribution module | the small shell file that builds one system's root filesystem for the guided installer (`DISTRO=ubuntu`, `arch`, `nixos`, …); your own can be passed by path ([distribution modules](../tools/install/distros/README.md)) |
| platform files | the distribution-neutral files every root needs: udev rules, systemd units, audio and sensor configuration, the emergency key ([userspace/platform](../userspace/platform/README.md)) |
| firmware extraction | the firmware is not shipped; it is copied from your own tablet's Android `/vendor` with a script and checked against a manifest ([firmware](../firmware/README.md), [install-manual.md, step 1](install-manual.md#1-extract-the-firmware)) |
| audio topology | `LENOVO-TB323FU-tplg.bin`, which tells the audio DSP driver which audio paths exist; without it the sound card does not appear. Unlike the rest of the firmware it is built in this repository from open sources ([firmware/audio](../firmware/audio/README.md)) |
