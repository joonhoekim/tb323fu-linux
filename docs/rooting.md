# Rooting and dual boot setup

How the TB323FU used for this project got from a stock tablet to Android and mainline Linux on the same device:
backup, root **without unlocking the bootloader** (LTBox), KernelSU on Android, Android's boot image kept in `boot_b`,
Linux written to `boot_a`, and how to get back when something goes wrong.

This is a record of what was done on one unit, not a polished installer. "Checked" means it was done on the device;
anything else is marked as not checked. Commands are given only where they were actually used.

## Overview

| Step | What changes on the tablet | Reversible? |
|---|---|---|
| 1. Back up | nothing (read-only EDL dump) | — |
| 2. Root with LTBox | `efisp` gets a patched GBL EFI app, `init_boot_a` gets the KernelSU loader | yes: unroot restores `init_boot`; `efisp` can be erased (see [Recovery](#undoing-the-root)) |
| 3. KernelSU | manager app, root for `adb shell` | yes |
| 4. Dual boot | `boot_b` ← copy of your Android boot image; a Linux root partition; `boot_a` ← Linux when you switch | `boot_a`/`boot_b`: yes. Making room for a Linux root on the internal storage **wipes Android's data** |
| 5. First Linux boot | nothing more | — |

The bootloader stays **locked** the whole time (`ro.boot.flash.locked=1`, verified boot state `green`).

## Risks — read first

- **You can brick the tablet.** Every write in this guide goes to a boot partition or a partition table. The recovery
  path is Qualcomm EDL with your own backup; make the backup before anything else and keep it off the PC.
- **Warranty.** Modifying boot partitions is outside what Lenovo supports. Assume the warranty is affected.
- **Data loss.** Rooting itself (step 2) did not wipe data on this unit. A **classic bootloader unlock** would (and
  is not what this guide does). Creating a Linux root partition on the internal UFS storage requires shrinking
  `userdata`, which means a **factory reset of Android** (step 4).
- **Region (PRC / ROW).** The hardware is the same; the firmware differs. This guide was done on a **ROW** unit with
  ROW firmware. LTBox picks the GBL EFI file by the region recorded on the device; PRC units are listed as supported
  by LTBox but were **not checked here**. Do not cross-flash regions as part of this procedure.
- **Never write a GBL EFI file to `efisp` by hand.** LTBox's own troubleshooting page warns that flashing the
  upstream `superturtlee/gbl_root_canoe` build to `efisp` on a locked bootloader makes Google stop issuing remote
  keys to the device (Play Integrity fails completely, Widevine drops to L3) and that this damage survives unrooting.
  Let LTBox choose and verify the file.
- **Only `boot_a` is used for Linux.** The patched bootloader relaxes verified boot for `boot` only. A modified
  `recovery` image was rejected (red "Your device is corrupt" screen) and the whole slot was marked unbootable — it
  took an EDL partition-table rewrite to recover (see [Recovery](#slot-marked-unbootable)). Do not put experiments in
  `recovery`, `dtbo`, `vbmeta`, `vendor_boot` or anything in `super`.
- **Slot `_b` is not a fallback.** The tablet uses virtual A/B: `super` holds only `_a` system partitions, so slot `_b`
  has no system to boot. That is why `boot_b` can be used as storage for the Android boot image.
- **OTA updates.** An OTA overwrites `init_boot` (root is lost) and can overwrite `boot_b` (your way back from
  Linux). This guide disables the OTA apps after rooting. Firmware updates raise the rollback index, so a firmware
  version cannot be downgraded afterwards.

## Verified on

| Item | Version |
|---|---|
| Device | TB323FU, 16 GB / 512 GB, ROW |
| Firmware | `TB323FU_ROW_OPEN_USER_Q00020.0_A16_ZUI_18.0.12.104_ST_260711` (Android 16, ZUI 18.0.12.104) |
| Kernel at rooting | `6.12.30-android16-5` (KMI `android16-6.12`) |
| LTBox | v3.3.1 for rooting, v3.3.2 for a later dump (Windows 11) |
| KernelSU | v3.3.0, LKM mode |
| GBL EFI written by LTBox | `miner7222/gbl_root_baldur` release `5.3.120-mod5`, `generic_superfastboot_row.efi`, 135,168 bytes, SHA-256 `90b16cacc4f2f6aded2c5bf0eed7d20b6a294115f6cf09dab555b4a4496e2628` |

LTBox also checked the bootloader of the earlier firmware `18.0.10.039` (passed), but rooting was done on
`18.0.12.104`. Other firmware versions: not checked.

## What you need

- A Windows PC. LTBox also runs on macOS and Linux according to its documentation, but only Windows was used here;
  ROW firmware is only downloadable through Lenovo's Windows tool.
- [LTBox](https://github.com/miner7222/LTBox) ([documentation](https://miner7222.github.io/ltbox/en/index.html)) and,
  on Windows, the Qualcomm USB driver it offers to install on first run (reboot the PC after installing it).
- [Android platform-tools](https://developer.android.com/tools/releases/platform-tools) (`adb`).
- The firmware package for your device from [Lenovo Software Fix](https://pcsupport.lenovo.com/rescue-and-smart-assistant)
  (ROW). PRC firmware: LTBox's dashboard offers the download for PRC units (not checked here).
- A USB-C data cable, battery above 50 %, and about 60 GB free disk space (firmware package plus a full dump).
- A second place to keep the backup (external disk, NAS).

## 1. Back up

### Get the firmware package

In Software Fix, select the tablet (connect it or enter `TB323FU`) and let it **download** the firmware.
**Do not press Rescue / Flash** — that installs the firmware and wipes the tablet. Close Software Fix completely
afterwards (including the tray icon); while it runs it holds the USB connection and LTBox cannot reach the device.

The package contains the **EDL loader**: `image/qsahara_device_programmer.x` plus the eight images it lists.
On this chipset the loader is not a single file — keep the whole `image/` folder together. The loader only has to
match the model, not the firmware version or region.

LTBox v3.3.1's loader picker accepted only `.xml` for this model, while the package ships the encrypted `.x`.
It was decrypted to `qsahara_device_programmer.xml` with the same method LTBox's own code uses
(`crates/ltbox-core/src/crypto.rs`), and the eight images were copied next to it. Whether newer LTBox versions
accept the `.x` directly: not checked.

The package does not contain anything device-specific (its `persist.img` etc. are factory defaults), and the stock
images in it are only good for the version it was built for. It does not replace a dump of your own device.

### Dump every partition

Before connecting LTBox, stop any other adb server (`adb kill-server`); LTBox talks to the device directly and
conflicts with it.

LTBox → **Advanced → Dump Partitions**:

1. Select the EDL loader (the `.xml`). **Scan** reboots the tablet into EDL and reads the partition tables of LUN 0–5.
2. Select **every partition except `userdata`** (it is file-based encrypted and useless as an image). Include `super`
   (20 GiB) — it is the only copy of your current system.
3. Choose the output folder. In v3.3.2 the dump starts as soon as the folder is chosen, without another confirmation.

Checked results (twice): 142 files, 26.21 GiB, about 5 minutes; the tablet booted back to Android by itself.
**LTBox v3.3.1/v3.3.2 shows no completion message** — it just returns to the first screen, and the file log does not
record the dump. Verify the result instead: compare each `<name>.img` size with the partition sizes in the package's
`gpt_main0.bin` … `gpt_main5.bin` (4096-byte sectors). Expected differences: `userdata` (excluded) and `last_parti`
(size 0 in the package GPT, present on LUN 1–5 under the same name, so only one file is left).

Write a SHA-256 list of the dump and copy dump, list and firmware package to a second place.

The partitions that cannot be recreated from any firmware are `persist` (sensor calibration), `devinfo`,
`oemowninfo`, `modemst1`, `modemst2`, `fsg`, `fsc`, `fs_bkup`, `keystore`, `secdata`, `uefivarstore`. LTBox treats a
failed dump of `devinfo`, `persist` or `oemowninfo` as fatal. **A dump is specific to one device and contains its
identifiers — never share it, never flash it to another tablet.**

Also check that `efisp.img` is all zeros before rooting (it was on this unit): LTBox only writes its GBL file to an
empty `efisp`.

## 2. Root without unlocking (LTBox)

### Why no bootloader unlock

There are two separate choices: *how* a modified boot image gets onto the device, and *which* root manager runs.
The classic way to write images is a bootloader unlock plus `fastboot flash`. On this tablet that path is unattractive:
unlocking wipes the device, relocking is effectively blocked, and the fastboot of this bootloader cannot even flash
(it has no `fastboot boot`, and `flash` and `set_active` answer `unknown command`).

LTBox instead writes through **EDL (Qualcomm emergency download, USB `05c6:9008`)** and puts a patched **GBL** EFI
application into the `efisp` partition. The bootloader (ABL) loads that application, which patches ABL in memory so
that it treats itself as unlocked while still reporting `locked` / `green`. The older Lenovo "AOSP test key" method
described in guides for earlier Y700 generations does not apply to this model.

The method depends on the firmware's ABL loading `efisp`. LTBox checks the device's `abl_a` before writing and stops
without writing anything if the check does not pass.

### What LTBox writes

Read from the LTBox v3.3.1 source and confirmed by its log on this unit:

1. Over adb: reads the active slot and kernel version, downloads the KernelSU manager, `kernelsu.ko` for the kernel's
   KMI and `ksuinit` from the KernelSU GitHub releases, installs the manager.
2. `adb reboot edl`, uploads the loader, dumps `init_boot_a`, `abl_a`, `efisp`, `vendor_boot_a`; checks that ABL loads
   `efisp`; if `efisp` is empty, picks the GBL file by the region in `vendor_boot` and verifies its fixed SHA-256;
   backs up the stock `init_boot`.
3. Repacks `init_boot`: the original `init` becomes `init.real`, `ksuinit` becomes `init`, `kernelsu.ko` is added.
   No AVB re-signing, no `vbmeta` change.
4. Writes **`efisp`** (only if it was empty) and **`init_boot_a`**. Nothing else — not `vbmeta`, not slot `_b`, not
   `userdata`.
5. Reboots (two or three times).

If a write fails after another one succeeded, LTBox **leaves the device in EDL on purpose** — do not force a reboot
then (see [Recovery](#ltbox-stopped-in-the-middle-of-a-write)).

### Steps (checked)

Before: enable **USB debugging** in Developer options, close Software Fix, run `adb kill-server`, battery above 50 %.
Turning *OEM unlocking* on is not needed (it was switched off on this unit before rooting).

1. Connect the tablet, start LTBox, allow USB debugging on the tablet. The dashboard shows model, slot and firmware.
2. Sidebar → **Root Device**.
3. Provider: **KernelSU Variants → LKM → KernelSU → Stable**. (GKI-mode rooting was not supported for this model in
   v3.3.1; LTBox's documentation recommends this choice for most users.)
4. Loader: the `qsahara_device_programmer.xml` from step 1.
5. Confirm → Start, and leave the cable alone until the tablet is back in Android.

Result on this unit: `Flashed efisp` (135,168 bytes) then `Flashed init_boot_a` (8,388,608 bytes); after reboot
`kernelsu` was live in `/proc/modules`, the bootloader still `locked` / `green`, slot `_a`.

**Keep LTBox's backup folder** (on Windows `%LOCALAPPDATA%\ltbox\backup\root\TB323FU_<date_time>\`, containing
`init_boot.img` and `manifest.json`) and copy it next to your dump. Unroot needs it. On this unit the backed-up
`init_boot.img` was identical to `image/init_boot.img` of the firmware package for the same version.

Save LTBox's **Work History** with its Save button if you want a record — the file log under `%APPDATA%\ltbox\logs\`
only keeps errors.

## 3. Root on Android (KernelSU)

The KernelSU manager (`me.weishu.kernelsu`) is installed by LTBox. Root is in `init_boot`, not in the kernel
(LKM mode), so a different kernel in `boot` keeps root as long as it has the same KMI.

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
- Play Integrity after rooting: Basic and Device pass, Strong fails. The likely cause is the old vendor security
  patch level of this firmware (2025-06-05), not root; it was not measured before rooting, so there is no comparison.

### Firmware for Linux

With root you can extract the firmware Linux needs from Android — see [`firmware/README.md`](../firmware/README.md).
No firmware is distributed by this project.

## 4. Android in `boot_b`, Linux in `boot_a`

The model (details in [`android/README.md`](../android/README.md)):

- The tablet always boots slot `_a`. `boot_b` is never booted, so it holds a **copy of your Android boot image** — the
  way back. Its SHA-256 is recorded once, and every tool refuses to write unless `boot_b` still matches it.
- Switching to Linux writes the Linux boot image into `boot_a`; switching back copies `boot_b` into `boot_a`.
  Only `boot_a` changes. Partitions are found by GPT name, never by number.

### Set up the way back (checked)

In rooted Android with USB debugging, from a clone of this repository:

```sh
android/install-module.sh prepare-boot-b      # copies the running Android boot image (boot_a) into boot_b; asks first
android/install-module.sh install             # refuses unless boot_a == boot_b; installs the "Switch to Linux" KernelSU module
android/install-module.sh stage linux-boot.img   # fallback Linux image for the module's Action button
android/install-module.sh status              # slot, boot_a/boot_b hashes, staged image, module
```

**Write down the hash `install` prints.** The Linux side needs it: `back-to-android <hash>`, the emergency key
service and the helper read it from `/etc/tb323fu/android-boot.sha256`; `tools/cycle.sh` takes it as
`ANDROID_BOOT_SHA256`.

If the Android boot image ever changes (an OTA, a different Android kernel), `boot_b` and the recorded hash are stale:
the Linux tools then refuse to write (safe, but there is no way back from Linux until you redo `prepare-boot-b` and
`install` in Android and update the hash on the Linux side).

### The Linux boot image

The Linux boot image is built from **your own stock boot image**: [`tools/build-boot.sh`](../tools/build-boot.sh)
uses [`tools/boot-repack-kernel.py`](../tools/boot-repack-kernel.py) to put the mainline kernel into it while keeping
the header, the stock boot signature, the stock vbmeta blob and the AVB footer layout. Nothing is re-signed; the
patched bootloader accepts the changed digest for `boot`. An image packed with plain `mkbootimg` loses that blob and
is not accepted. Get the stock image in Android with root:

```sh
dd if=/dev/block/by-name/boot_a of=/sdcard/stock-boot.img
```

The kernel carries its device tree, command line and initramfs inside the image; see
[`kernel/initramfs/README.md`](../kernel/initramfs/README.md).

### A root partition for Linux

The initramfs looks for root filesystems by **GPT partition name**: `baldur-root` (default; on this unit on the
internal UFS storage after `userdata`), `baldur-root-sd` (microSD fallback) and `tb323fu-*` partitions for more
systems ([multiboot](../kernel/initramfs/README.md#root-partitions-and-multiboot)).

How this unit was set up:

- First, a microSD card with a new GPT and an ext4 root. Android then reports the card as unsupported; that is
  expected.
- Later, `baldur-root` on UFS: `userdata` (the last partition of LUN 0) was cut to 128 GiB and a ~322 GiB ext4
  partition named `baldur-root` was added after it. `userdata` is f2fs (cannot shrink) and encrypted, so this
  **is a factory reset**: `userdata` and `metadata` were cleared, and Android formatted them on its next boot and
  started at the setup wizard. The GPT was edited from Linux with `sgdisk`, after saving a GPT backup (`sgdisk -b`);
  Android's own `sgdisk` cannot do this. A step-by-step procedure for this is not written yet.
- After that reset: `/data/adb` (KernelSU's userspace and allow list) was gone while root in `init_boot` stayed; the
  KernelSU manager APK of the same version was reinstalled with adb and Shell allowed again; the OTA apps had to be
  disabled again.

`boot_a` and `boot_b` were unchanged after the reset, slot still `_a` (checked).

### Switching

| Direction | How | Checked |
|---|---|---|
| Android → Linux | KernelSU manager → module **Switch to Linux** → Action button. Writes the last Linux image that ran (saved by `back-to-android` on `baldur-root`) or the staged one to `boot_a`, verifies it, reboots | yes |
| Linux → Android | `back-to-android <hash>` as root, or the **Android** tile / "Switch to Android" in the desktop ([helper](helper.md)) | yes |
| Linux → Android, emergency | hold **volume up + volume down for 10 s** — works with a frozen desktop as long as the kernel runs; letting go earlier cancels | yes (Debian) |
| from a PC, Linux running | [`tools/flash-boot.sh`](../tools/flash-boot.sh) over SSH (USB network: tablet `192.168.7.2`, PC `192.168.7.1`) | yes |
| from a PC, through Android | [`tools/cycle.sh`](../tools/cycle.sh): back to Android if needed, write `boot_a` over adb, reboot, wait for SSH | yes, many times |

Every one of these checks hashes before and after writing and stops without rebooting on any mismatch. If the Switch
to Linux module finds a bad write, it copies Android back from `boot_b` before giving up.

## 5. First boot of Linux

- Root filesystems: [docs/distros.md](distros.md) lists the systems built with [`rootfs/`](../rootfs/) and booted
  on the device, and what every root needs (kernel modules, firmware, masked services).
- What works: [docs/hardware-status.md](hardware-status.md).
- The initramfs brings up a USB serial shell and USB network (`192.168.7.2`) before anything else, shows a boot
  summary on the panel, then switches to the selected root. Holding **volume up** during the summary stays in the
  initramfs.
- In mainline Linux the power button alone does not force the tablet off; hold **power + volume down**.

## Recovery

Start with the smallest step: rebooting → fixing `boot_a` from Android (`adb` + `dd`) → unroot → one partition over
EDL → full firmware with data kept → factory reset. Check every file's hash before writing it.

### Modes and how to reach them

| Mode | USB ID | How to get there | Notes |
|---|---|---|---|
| Android (adb) | `17ef:…` | — | `adb reboot edl` goes to EDL (checked) |
| **EDL** | `05c6:9008` "Qualcomm HS-USB QDLoader 9008" | see below | the real recovery mode: partitions can be read and written with a loader |
| Fastboot | — | power + volume up from off (not checked); also shown after a verified-boot rejection | **read-only on this bootloader** (`getvar`, `download`). `START` boots normally. **Do not choose "Boot to Alternate Slot"** — slot `_b` cannot boot |
| Recovery | — | volume down + volume up, then power, from off (checked) | stock AOSP recovery, not TWRP; has "Apply update from ADB"; the default entry is "Enter fastboot" |
| **Crash dump** | `05c6:900E` "Qualcomm HS-USB Diagnostics" | the SoC falls into it after a crash | **not a recovery mode** — nothing can be written. A dump tool gets one Sahara session; afterwards the tablet resets |

Getting into EDL (all three checked; what matters is that volume up is held while ABL reads the keys twice, about
5 s apart, with USB connected):

1. **Tablet off:** hold **volume up**, plug in USB, keep holding about 10 s → logo → black screen, 9008 appears.
2. **Stuck in a crash/dump loop (900E):** keep USB plugged in, force a restart (long power, or **power + volume down**
   if power alone does nothing), and hold **volume up from the moment the splash appears**.
3. **If 2 does not work:** unplug USB, hold **power + volume down** to force it off, then hold **volume up** (only the
   backlight comes on); it enumerates as 9008.

Leave EDL with LTBox → Reboot Device → **System** (needs the loader). EDL survives unplugging and replugging the
cable.

Do not let a failing image reboot over and over: repeated boot failures can make ABL switch to slot `_b`, which does
not boot. Go to EDL and restore `boot_a` instead. [`tools/flash-boot.sh`](../tools/flash-boot.sh) switches Qualcomm
download mode off before writing, so a crash on the way reboots instead of stopping in 900E.

### Linux does not boot

- If the initramfs comes up (boot summary on the panel, USB serial/network): hold volume up + volume down 10 s, or run
  `back-to-android <hash>` on the serial shell.
- If it hangs before that: EDL → LTBox → Advanced → EDL Operations → **Flash Partitions** → `boot_a` ← your Android
  boot image (the copy in `boot_b`, your dump's `boot_a`, or the package's `boot.img` for the same firmware version);
  all other rows **Skip** → write → Reboot Device → System. Checked several times.
- Afterwards confirm `adb shell getprop ro.boot.slot_suffix` is `_a`.

### Android does not boot after rooting

- If adb works (e.g. from recovery): LTBox → **Unroot Device** (method Magisk / LKM, the backup folder from step 2).
  Unroot can only start from adb or fastboot, not from EDL.
- EDL only: Flash Partitions → `init_boot_a` ← the backed-up `init_boot.img` (or the package's `image/init_boot.img`
  if it is the same version); everything else Skip.

These two paths come from the LTBox source; they were not needed on this unit.

### LTBox stopped in the middle of a write

Leave the cable and LTBox as they are, save the message and the Work History. The `Flashed …` lines show how far it
got. If `efisp` was written but `init_boot_a` failed, write the stock `init_boot.img` to `init_boot_a` with Flash
Partitions; the GBL in `efisp` does not prevent a stock boot. (From the LTBox source; not experienced here.)

### Slot marked unbootable

A partition that fails verified boot makes ABL mark the **whole slot** unbootable in the GPT attribute bits, and the
mark stays after the partition is restored. Fastboot cannot clear it and LTBox does not show the GPT. What worked
(checked once): the open-source [`edl`](https://github.com/bkerler/edl) tool with the loader `.xml`, after switching
the 9008 device to the **WinUSB** driver with Zadig, rewriting the first 6 sectors of the affected LUN from a known-good
copy (`edl.py rs` to read, compare byte by byte, `edl.py ws` to write, read back, `edl.py reset`). Do not flip the bit
by hand — the GPT header CRC covers it. Notes from that session: the serial (COM) driver path never got past the
Sahara handshake; the libusb-win32 driver crashed; set `PYTHONIOENCODING=utf-8` on Windows, otherwise the progress bar
can crash the tool mid-write; after the driver switch LTBox no longer sees the device until the driver is removed in
Device Manager.

### Full reinstall

- **Same version, keep data:** LTBox → Flash Firmware → matching region → **Keep Data**, from a fresh copy of the
  unpacked package (LTBox writes decrypted files into the folder). Per the LTBox source and the package's XML this
  skips `userdata`, `metadata`, `persist`, `efisp` and the modem calibration partitions and writes slot `_a` only;
  root is lost (`init_boot` is stock again), `efisp` stays. Not run on this unit.
- **Do not use Advanced → Simple Firmware Flasher** to keep data: it writes the package's `userdata` and `metadata`
  images as they are and wipes the device.
- **Factory state:** LTBox Flash Firmware → **Wipe Data** (also erases `efisp`), or Software Fix → Rescue (installs
  the newest firmware). Both wipe the tablet.
- Device-specific partitions (`persist`, `modemst*`, `fsg`, …) are restored only from **your own** dump, one partition
  at a time, and only when the symptom clearly points at them. Never use the package's `persist.img`.

### Undoing the root

LTBox → Unroot Device restores `init_boot` only; it leaves the GBL in `efisp`. To clear `efisp` too, use Flash
Partitions with `efisp` set to Erase (or write your all-zero `efisp.img` from the dump) — after making sure
`init_boot_a` is stock. Not done on this unit.

### Updating Android firmware later

Not done on this unit since rooting. The order prepared from the LTBox source: confirm the new firmware's ABL still
loads `efisp` (otherwise you cannot root again, and the update cannot be undone) → restore the stock `boot_a` and
unroot → enable the OTA apps and update → disable them → root again (LTBox then writes only `init_boot`) → redo
`prepare-boot-b` / `install` and update the hash on the Linux side.

## Terms

| Term | Meaning here |
|---|---|
| ABL | Android bootloader (Qualcomm LinuxLoader); draws the fastboot screen and verifies `boot`/`init_boot` |
| GBL / `efisp` | an EFI application ABL loads from the `efisp` partition; the patched one makes ABL treat itself as unlocked |
| AVB | Android Verified Boot; `green` = locked and verified, `red` = rejected |
| EDL / 9008 | Qualcomm emergency download mode; Sahara uploads the loader, Firehose reads and writes partitions |
| 900E | crash dump mode; one-shot dump session, not a way to write |
| `boot` / `init_boot` | kernel / first-stage ramdisk (where KernelSU LKM lives) |
| virtual A/B | small partitions exist twice, `super` only once; slot `_b` cannot boot here |
| LUN | a UFS logical unit; most boot partitions are on LUN 4, `userdata` on LUN 0 |

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
- In this repository: [android/](../android/README.md), [firmware/](../firmware/README.md),
  [kernel/initramfs/](../kernel/initramfs/README.md), [tools/](../tools/README.md), [distros](distros.md),
  [helper](helper.md), [hardware status](hardware-status.md)
