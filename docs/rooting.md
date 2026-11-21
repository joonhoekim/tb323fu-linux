# Rooting

**Step 1 of [Installing Linux](install.md).** Every install page starts from a rooted tablet: your own full backup,
root on Android **without unlocking the bootloader** (LTBox), and KernelSU with root for `adb shell`. Android stays
as it is. After this page, the install script does the rest. When something goes wrong, see [Recovery](recovery.md).

Done on one unit ([Verified on](#verified-on)). Steps marked **verified** were done on a tablet; anything else is
marked **untested**.

## Overview

| Step | What changes on the tablet | Reversible? |
|---|---|---|
| 1. Back up | nothing (read-only EDL dump) | — |
| 2. Root with LTBox | `efisp` gets a patched GBL EFI app, `init_boot_a` gets the KernelSU loader | yes: unroot restores `init_boot`; `efisp` can be erased (see [Undoing the root](recovery.md#undoing-the-root)) |
| 3. KernelSU | manager app, root for `adb shell`, OTA apps turned off | yes |
| [Next](#next-the-install-script-does-the-rest): install | done by the install script: `boot_b` ← copy of your Android boot image, a Linux root partition on the microSD card, `boot_a` ← Linux | yes ([removing Linux](after-install.md#removing-linux)) |

The bootloader stays **locked** the whole time (`ro.boot.flash.locked=1`, verified boot state `green`).

## Risks — read first

- **You can brick the tablet.** Every write in this guide goes to a boot partition or a partition table. The recovery
  path is Qualcomm EDL with your own backup; make the backup before anything else and keep it off the PC.
- **Warranty.** Modifying boot partitions is outside what Lenovo supports. Assume the warranty is affected.
- **Data loss.** Rooting itself (step 2) did not wipe data. A **classic bootloader unlock** would (this guide does not
  do one). A Linux root partition on the internal UFS storage requires shrinking `userdata`, which means a
  **factory reset of Android** ([Installing by hand, step 8](install-manual.md#8-optional-a-linux-root-on-the-internal-storage)).
- **Region (PRC / ROW).** Same hardware, different firmware. This guide was done on a **ROW** unit. LTBox picks the GBL
  EFI file by the region recorded on the device; PRC units are listed as supported by LTBox but are **untested**
  here. Do not cross-flash regions.
- **Never write a GBL EFI file to `efisp` by hand.** LTBox's troubleshooting page warns that flashing the upstream
  `superturtlee/gbl_root_canoe` build to `efisp` on a locked bootloader makes Google stop issuing remote keys to the
  device (Play Integrity fails completely, Widevine drops to L3), and that this survives unrooting.
  Let LTBox choose and verify the file.
- **Only `boot_a` is used for Linux.** The patched bootloader relaxes verified boot for `boot` only. A modified
  `recovery` image was rejected (red "Your device is corrupt" screen) and the whole slot was marked unbootable; it
  took an EDL partition-table rewrite to recover ([Slot marked unbootable](recovery.md#slot-marked-unbootable)).
  Do not put experiments in `recovery`, `dtbo`, `vbmeta`, `vendor_boot` or anything in `super`.
- **Slot `_b` is not a fallback.** The tablet uses virtual A/B: `super` holds only `_a` system partitions, so slot `_b`
  has no system to boot. That is why `boot_b` can store the Android boot image.
- **OTA updates.** An OTA overwrites `init_boot` (root is lost) and can overwrite `boot_b` (your way back from
  Linux). Step 3 disables the OTA apps; a firmware update cannot be undone: [Updating Android firmware](firmware-updates.md).

## Verified on

| Item | Version |
|---|---|
| Device | TB323FU, 16 GB / 512 GB, ROW |
| Firmware | `TB323FU_ROW_OPEN_USER_Q00020.0_A16_ZUI_18.0.12.104_ST_260711` (Android 16, ZUI 18.0.12.104) |
| Kernel at rooting | `6.12.30-android16-5` (KMI `android16-6.12`) |
| LTBox | v3.3.1 for rooting, v3.3.2 for a later dump (Windows 11) |
| KernelSU | v3.3.0, LKM mode |
| GBL EFI written by LTBox | `miner7222/gbl_root_baldur` release `5.3.120-mod5`, `generic_superfastboot_row.efi`, 135,168 bytes, SHA-256 `90b16cacc4f2f6aded2c5bf0eed7d20b6a294115f6cf09dab555b4a4496e2628` |

Rooting was done on `18.0.12.104`; other firmware versions are untested ([Updating Android firmware](firmware-updates.md#firmware-versions)).

## What you need

- A host computer: **Windows** (verified), or macOS / Linux (**untested**) — see [Host computer](#host-computer).
- [LTBox](https://github.com/miner7222/LTBox) ([documentation](https://miner7222.github.io/ltbox/en/index.html)).
- [Android platform-tools](https://developer.android.com/tools/releases/platform-tools) (`adb`).
- The firmware package for your device from [Lenovo Software Fix](https://pcsupport.lenovo.com/rescue-and-smart-assistant)
  (ROW). Software Fix runs only on Windows; for other hosts see
  [Getting the firmware package without Windows](#getting-the-firmware-package-without-windows).
  PRC firmware: LTBox's dashboard offers the download for PRC units (untested).
- A USB-C data cable, battery above 50 %, and about 60 GB free disk space (firmware package plus a full dump).
- A second place to keep the backup (external disk, NAS).

## Host computer

LTBox is a native desktop application with release builds for Windows, macOS and Linux. It brings its own adb, Sahara
and Firehose code, so no separate EDL tool is needed. The LTBox screens in the steps below should be the same on every
host (untested off Windows).

| Host | Status | LTBox package | EDL (9008) access |
|---|---|---|---|
| Windows 11 x86-64 | **verified** (LTBox v3.3.1 / v3.3.2) | zip or Scoop | Qualcomm USB driver, installed from LTBox |
| macOS 11+ | starts and sees the tablet over adb (v3.3.3, macOS 26.6); EDL and rooting **untested** | Homebrew cask or tarball (universal) | bundled libusb, no driver |
| Linux x86-64 / arm64 | **untested** (instructions come from LTBox's own documentation) | `.deb`, `.rpm`, tarball, AUR `ltbox-bin` | udev rule (`ltbox --install-udev`) |
| NixOS | starts (v3.3.3 tarball through nix-ld, NixOS unstable); device access, EDL and rooting **untested** | tarball | udev rule in the NixOS configuration |

Releases and `.sha256` files: [LTBox's releases page](https://github.com/miner7222/LTBox/releases). Install
instructions per OS: [LTBox's documentation](https://miner7222.github.io/ltbox/en/index.html); the driver part:
[Connecting a device](https://miner7222.github.io/ltbox/en/connecting-a-device.html).

### Windows (verified)

1. Unpack the LTBox zip to a path **without spaces or special characters** and run `ltbox.exe` (or install it with
   Scoop; not used here).
2. On first start the dashboard offers to install the **Qualcomm USB driver**. Install it and **reboot the PC**.
   Afterwards the tablet in EDL shows up as "Qualcomm HS-USB QDLoader 9008". If a scan in EDL fails with
   `Sahara HELLO timed out … Received unsuccessful End of Image packet`, set LTBox → Settings → Qualcomm USB driver
   to Userspace and try again (needed on one PC,
   [#57](https://github.com/joonhoekim/tb323fu-linux/discussions/57)).
3. Install platform-tools (`adb`) and put it on `PATH`.
4. Get the firmware package with Lenovo Software Fix ([below](#get-the-firmware-package)) and close Software Fix
   completely before starting LTBox.

<details>
<summary>Windows notes: two GPUs, LTBox's folders</summary>

If LTBox crashes or shows an empty window on a laptop with two GPUs, start it with `$env:ICED_BACKEND = "tiny-skia"`
in PowerShell (from LTBox's troubleshooting page; not needed here).

LTBox's backup folder: `%LOCALAPPDATA%\ltbox\backup\`; log folder: `%APPDATA%\ltbox\logs\`.

</details>

### macOS and Linux (untested)

Installing LTBox, USB access and adb: [Installing from macOS](install-macos.md#1-tools-for-rooting),
[Installing from Linux](install-linux.md#1-tools-for-rooting) (with NixOS). The firmware package needs a way around
Software Fix, below.

### Getting the firmware package without Windows

Lenovo distributes ROW firmware for this tablet only through Software Fix (Rescue and Smart Assistant), a Windows
application. LTBox does **not** download firmware; it expects a folder you already have.

| Option | Notes | Status |
|---|---|---|
| Software Fix on any Windows PC | download only, then copy the whole unpacked package to your Mac/Linux machine. Connecting the tablet is preferable to entering its S/N: Software Fix then checks the exact device and firmware | **verified** |
| Software Fix in a Windows virtual machine | VirtualBox, VMware, UTM (Apple silicon: Windows 11 on Arm), QEMU/KVM. Entering the S/N should need no USB passthrough. Do not flash from a VM | untested |
| [LenovoMotoFirmwareDownloader](https://github.com/enigma550/LenovoMotoFirmwareDownloader) | unofficial, cross-platform, needs your Lenovo account login; TB323FU support not stated | untested; third-party code handling your credentials |
| Firmware mirror sites | no way to check integrity | not recommended |

The package must contain `image/qsahara_device_programmer.x` and the images it lists. Keep the downloaded archive
unchanged as a reference copy: LTBox writes decrypted files into the folder it flashes from.

## 1. Back up

### Get the firmware package

In Software Fix, select the tablet (preferably connected over USB; entering its S/N also works) and let it
**download** the firmware. **Do not press Rescue / Flash**: that installs the firmware and wipes the tablet. Close
Software Fix completely afterwards (including the tray icon); while it runs it holds the USB connection and LTBox
cannot reach the device.

The package contains the **EDL loader**: `image/qsahara_device_programmer.x` plus the eight images it lists. Keep the
whole `image/` folder together. The loader only has to match the model, not the firmware version or region.

The package holds nothing device-specific (its `persist.img` etc. are factory defaults), and its stock images only fit
the version it was built for. It does not replace a dump of your own device.

<details>
<summary>LTBox v3.3.1 accepted only an <code>.xml</code> loader</summary>

LTBox v3.3.1's loader picker accepted only `.xml` for this model, while the package ships the encrypted `.x`.
It was decrypted to `qsahara_device_programmer.xml` with the method LTBox's own code uses
(`crates/ltbox-core/src/crypto.rs`), and the eight images were copied next to it. Newer LTBox versions accepting the
`.x` directly: untested.

</details>

### Dump every partition

Before connecting LTBox, stop any other adb server (`adb kill-server`); LTBox talks to the device directly and
conflicts with it.

LTBox → **Advanced → Dump Partitions**:

1. Select the EDL loader (the `.xml`). **Scan** reboots the tablet into EDL and reads the partition tables of LUN 0–5.
2. Select **every partition except `userdata`** (it is file-based encrypted and useless as an image). Include `super`
   (20 GiB): it is the only copy of your current system.
3. Choose the output folder. In v3.3.2 the dump starts as soon as the folder is chosen, without another confirmation.

**LTBox v3.3.1/v3.3.2 shows no completion message**; it returns to the first screen and the file log does not record
the dump. Expect about 5 minutes and 26 GiB (142 files); the tablet boots back to Android by itself. Verify the
result:

<details>
<summary>How to verify the dump</summary>

Compare each `<name>.img` size with the partition sizes in the package's `gpt_main0.bin` … `gpt_main5.bin`
(4096-byte sectors). Expected differences: `userdata` (excluded) and `last_parti` (size 0 in the package GPT, present
on LUN 1–5 under the same name, so only one file is left).

</details>

Write a SHA-256 list of the dump and copy dump, list and firmware package to a second place.

The partitions that cannot be recreated from any firmware are `persist` (sensor calibration), `devinfo`,
`oemowninfo`, `modemst1`, `modemst2`, `fsg`, `fsc`, `fs_bkup`, `keystore`, `secdata`, `uefivarstore`. LTBox treats a
failed dump of `devinfo`, `persist` or `oemowninfo` as fatal. **A dump is specific to one device and contains its
identifiers: never share it, never flash it to another tablet.**

Also check that `efisp.img` is all zeros before rooting: LTBox only writes its GBL file to an empty `efisp`.

## 2. Root without unlocking (LTBox)

LTBox writes through **EDL (Qualcomm emergency download, USB `05c6:9008`)** and puts a patched **GBL** EFI
application into the `efisp` partition. The bootloader (ABL) loads it, and it patches ABL in memory so that ABL
treats itself as unlocked while still reporting `locked` / `green`. This depends on the firmware's ABL loading
`efisp`; LTBox checks the device's `abl_a` first and stops without writing if the check fails.

<details>
<summary>Why not a classic bootloader unlock</summary>

Unlocking wipes the device, relocking is effectively blocked, and this bootloader's fastboot cannot flash (no
`fastboot boot`; `flash` and `set_active` answer `unknown command`). The older Lenovo "AOSP test key" method from
earlier Y700 generations does not apply to this model.

</details>

<details>
<summary>What LTBox writes, step by step</summary>

From the LTBox v3.3.1 source, confirmed by its log:

1. Over adb: reads the active slot and kernel version, downloads the KernelSU manager, `kernelsu.ko` for the kernel's
   KMI and `ksuinit` from the KernelSU GitHub releases, installs the manager.
2. `adb reboot edl`, uploads the loader, dumps `init_boot_a`, `abl_a`, `efisp`, `vendor_boot_a`; checks that ABL loads
   `efisp`; if `efisp` is empty, picks the GBL file by the region in `vendor_boot` and verifies its fixed SHA-256;
   backs up the stock `init_boot`.
3. Repacks `init_boot`: the original `init` becomes `init.real`, `ksuinit` becomes `init`, `kernelsu.ko` is added.
   No AVB re-signing, no `vbmeta` change.
4. Writes **`efisp`** (only if it was empty) and **`init_boot_a`**. Nothing else: not `vbmeta`, not slot `_b`, not
   `userdata`.
5. Reboots (two or three times).

</details>

If a write fails after another one succeeded, LTBox **leaves the device in EDL on purpose**: do not force a reboot
(see [LTBox stopped in the middle of a write](recovery.md#ltbox-stopped-in-the-middle-of-a-write)).

### Steps (verified)

Before: enable **USB debugging** in Developer options, close Software Fix, run `adb kill-server`, battery above 50 %.
*OEM unlocking* does not need to be on.

1. Connect the tablet, start LTBox, allow USB debugging on the tablet. The dashboard shows model, slot and firmware.
2. Sidebar → **Root Device**.
3. Provider: **KernelSU Variants → LKM → KernelSU → Stable**. (GKI-mode rooting was not supported for this model in
   v3.3.1; LTBox's documentation recommends this choice for most users.)
4. Loader: the `qsahara_device_programmer.xml` from step 1.
5. Confirm → Start, and leave the cable alone until the tablet is back in Android.

Result: LTBox logs `Flashed efisp` then `Flashed init_boot_a`; after reboot `kernelsu` is in `/proc/modules`, the
bootloader still reports `locked` / `green`, slot `_a`.

**Keep LTBox's backup folder** (on Windows `%LOCALAPPDATA%\ltbox\backup\root\TB323FU_<date_time>\`, containing
`init_boot.img` and `manifest.json`; location on macOS and Linux untested) and copy it next to your dump. Unroot needs
it. The backed-up `init_boot.img` was identical to `image/init_boot.img` of the firmware package for the same version.

Save LTBox's **Work History** with its Save button if you want a record; the file log under `%APPDATA%\ltbox\logs\`
only keeps errors.

## 3. Root on Android (KernelSU)

LTBox installs the KernelSU manager (`me.weishu.kernelsu`). Root is in `init_boot`, not in the kernel (LKM mode), so a
different kernel in `boot` keeps root as long as it has the same KMI.

- KernelSU gives `su` only to apps you allow. The helpers in [`android/`](../android/) and
  [`firmware/`](../firmware/) run `adb shell su -c …`, so allow **Shell (`com.android.shell`)** in the manager's
  superuser list. Turn it off again when you do not need it.
- Check:

  ```sh
  adb shell getprop ro.boot.verifiedbootstate   # green
  adb shell getprop ro.boot.flash.locked        # 1
  adb shell su -c id                            # uid=0(root)
  ```

- **Disable the OTA apps.** On this unit:

  ```sh
  adb shell pm disable-user --user 0 com.lenovo.ota
  adb shell pm disable-user --user 0 com.tblenovo.center
  ```

  This setting lives in `/data`: a factory reset turns OTA back on, so repeat it after every reset.
- Play Integrity after rooting: Basic and Device pass, Strong fails.[^integrity]

The tablet is now ready for the install. Keep KernelSU's **Shell** permission on until the install is done.

## Next: the install script does the rest

With steps 1–3 done, go to **[Installing Linux](install.md)** and pick the page for your PC. One script then
copies the firmware Linux needs from Android (none is distributed by this project, see
[`firmware/README.md`](../firmware/README.md)), copies your Android boot image into `boot_b` as the way back and
installs the **Switch to Linux** KernelSU module, packs the release kernel into your own stock boot image, partitions
the microSD card, writes a root filesystem to it and writes Linux to `boot_a`.
[Installing by hand](install-manual.md) has the same steps as commands, plus a root on the internal storage.
Switching between the systems afterwards: [After installing](after-install.md#switching-between-android-and-linux).

<details>
<summary>How the dual boot works</summary>

The model (details in [`android/README.md`](../android/README.md)):

- The tablet always boots slot `_a`. `boot_b` is never booted, so it holds a **copy of your Android boot image**: the
  way back. Its SHA-256 is recorded once, and every tool refuses to write unless `boot_b` still matches it.
- Switching to Linux writes the Linux boot image into `boot_a`; switching back copies `boot_b` into `boot_a`.
  Only `boot_a` changes. Partitions are found by GPT name, never by number.

**The Linux boot image** is built from **your own stock boot image**: [`tools/build-boot.sh`](../tools/build-boot.sh)
uses [`tools/boot-repack-kernel.py`](../tools/boot-repack-kernel.py) to put the mainline kernel into it while keeping
the header, the stock boot signature, the stock vbmeta blob and the AVB footer layout. Nothing is re-signed; the
patched bootloader accepts the changed digest for `boot`. An image packed with plain `mkbootimg` loses that blob and
is not accepted. The kernel carries its device tree, command line and initramfs inside the image; see
[`kernel/initramfs/README.md`](../kernel/initramfs/README.md).

**A root partition for Linux.** The initramfs looks for root filesystems by **GPT partition name**: `baldur-root`
(default; on the internal UFS storage after `userdata`), `baldur-root-sd` (microSD fallback)
and `tb323fu-*` partitions for more systems ([multiboot](../kernel/initramfs/README.md#root-partitions-and-multiboot)).

| Where | What it costs | Notes |
|---|---|---|
| microSD (new GPT, ext4) | nothing on the tablet | what the install script does. Android then reports the card as unsupported; that is expected |
| internal UFS, after `userdata` | **a factory reset of Android** | `userdata` is f2fs (cannot shrink) and encrypted. Steps, and what to redo in Android afterwards: [the manual install, step 8](install-manual.md#8-optional-a-linux-root-on-the-internal-storage) |

</details>

## Terms

ABL, GBL / `efisp`, AVB, EDL / 9008, 900E, `boot` / `init_boot`, virtual A/B, LUN and the other terms used
across the documentation: [Glossary](glossary.md).

## Links

- LTBox: [repository](https://github.com/miner7222/LTBox), documentation —
  [Root a Device](https://miner7222.github.io/ltbox/en/root-a-device.html),
  [Unroot a Device](https://miner7222.github.io/ltbox/en/unroot-a-device.html),
  [Troubleshooting](https://miner7222.github.io/ltbox/en/troubleshooting.html),
  [How It Works](https://miner7222.github.io/ltbox/en/how-it-works.html)
- [KernelSU installation guide](https://kernelsu.org/guide/installation.html)
- [gbl_root_baldur](https://github.com/miner7222/gbl_root_baldur) (the GBL build LTBox pins for this model)
- [bkerler/edl](https://github.com/bkerler/edl)
- [XDA thread for the TB323FU](https://xdaforums.com/t/gen-5-lenovo-legion-tab-5-global-tb323fu-how-to-root-and-bootloader-unlock.4800204/)
- In this repository: [install](install.md), [after installing](after-install.md), [android/](../android/README.md), [firmware/](../firmware/README.md),
  [kernel/initramfs/](../kernel/initramfs/README.md), [tools/](../tools/README.md), [distros](distros.md),
  [helper](helper.md), [hardware status](hardware-status.md), [recovery](recovery.md)

**Next:** [Installing Linux](install.md) — from a rooted tablet to Linux on the microSD card.

[^integrity]: The likely cause is the old vendor security patch level of this firmware (2025-06-05), not root. It was not measured before rooting, so there is no comparison.
