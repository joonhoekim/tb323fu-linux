# Recovery

How to get a TB323FU back when Linux, Android or a write goes wrong. It assumes the setup from
[Rooting](rooting.md): a full EDL dump of your own tablet, LTBox, the firmware package with its
EDL loader, and Android's boot image kept in `boot_b`.

Start with the smallest step: rebooting → copying `boot_b` back into `boot_a` from Android, if it still runs (first bullet of [Linux does not boot](#linux-does-not-boot)) → unroot → one partition over
EDL → full firmware with data kept → factory reset. Check every file's hash before writing it.

LTBox (Flash Partitions, Unroot, Flash Firmware) has builds for Windows, macOS and Linux; host setup is in
[rooting.md → Host computer](rooting.md#host-computer). Only the Windows build was used for the recovery steps here.

| Symptom | Go to |
|---|---|
| Linux does not boot | [Linux does not boot](#linux-does-not-boot) |
| Android does not boot after rooting | [Android does not boot after rooting](#android-does-not-boot-after-rooting) |
| LTBox stopped during a write | [LTBox stopped in the middle of a write](#ltbox-stopped-in-the-middle-of-a-write) |
| Red "Your device is corrupt" screen, slot will not boot after restoring | [Slot marked unbootable](#slot-marked-unbootable) |
| Only the fastboot screen; START and Recovery mode come back to it | [Slot marked unbootable](#slot-marked-unbootable) |
| Stuck in a crash/dump loop (900E) | [Getting into EDL](#getting-into-edl), case 2 |

## Modes and how to reach them

| Mode | USB ID | How to get there | Notes |
|---|---|---|---|
| Android (adb) | `17ef:…` | — | `adb reboot edl` goes to EDL (verified) |
| **EDL** | `05c6:9008` "Qualcomm HS-USB QDLoader 9008" | see below | the real recovery mode: partitions can be read and written with a loader |
| Fastboot | — | power + volume up from off (untested); also shown after a verified-boot rejection | **read-only on this bootloader** (`getvar`, `download`). `START` boots normally. **Do not choose "Boot to Alternate Slot"**: slot `_b` cannot boot |
| Recovery | — | volume down + volume up, then power, from off (verified) | stock AOSP recovery, not TWRP; has "Apply update from ADB"; the default entry is "Enter fastboot" |
| **Crash dump** | `05c6:900E` "Qualcomm HS-USB Diagnostics" | the SoC falls into it after a crash | **not a recovery mode**: nothing can be written. A dump tool gets one Sahara session; afterwards the tablet resets |

In mainline Linux the power button alone does not force the tablet off; hold **power + volume down**.

### Getting into EDL

All three ways are verified. Volume up must be held while ABL reads the keys twice, about 5 s apart, with USB
connected.

1. **Tablet off:** hold **volume up**, plug in USB, keep holding about 10 s → logo → black screen, 9008 appears.
2. **Stuck in a crash/dump loop (900E):** keep USB plugged in, force a restart (long power, or **power + volume down**
   if power alone does nothing), and hold **volume up as soon as the restart begins**. The same works from the
   fastboot screen.
3. **If 2 does not work:** unplug USB, hold **power + volume down** to force it off, then hold **volume up** (only the
   backlight comes on); it enumerates as 9008.

Leave EDL with LTBox → Reboot Device → **System** (needs the loader). EDL survives unplugging and replugging the
cable.

Do not let a failing image reboot over and over: repeated boot failures can make ABL switch to slot `_b`, which does
not boot. Go to EDL and restore `boot_a` instead. [`tools/flash-boot.sh`](../tools/flash-boot.sh) switches Qualcomm
download mode off before writing, so a crash on the way reboots instead of stopping in 900E.

## Linux does not boot

- If the initramfs comes up (boot summary on the panel, USB serial/network): hold volume up + volume down 10 s, or run
  `back-to-android <hash>` on the serial shell.
- If Android still runs after a bad write of `boot_a` (before the restart): in `adb shell` with `su`, copy Android's
  boot image back: `dd if=/dev/block/by-name/boot_b of=/dev/block/by-name/boot_a bs=1M conv=fsync` (the command
  [`tools/cycle.sh`](../tools/cycle.sh) prints when its read-back check fails).
- If it hangs before that: EDL → LTBox → Advanced → EDL Operations → **Flash Partitions** → `boot_a` ← your Android
  boot image (the copy in `boot_b`, your dump's `boot_a`, or the package's `boot.img` for the same firmware version);
  all other rows **Skip** → write → Reboot Device → System (verified).
- Afterwards confirm `adb shell getprop ro.boot.slot_suffix` is `_a`.

## Android does not boot after rooting

Both paths are from the LTBox source (untested; not needed so far).

- If adb works (e.g. from recovery): LTBox → **Unroot Device** (method Magisk / LKM, the backup folder from
  [rooting step 2](rooting.md#2-root-without-unlocking-ltbox)). Unroot can only start from adb or fastboot, not from EDL.
- EDL only: Flash Partitions → `init_boot_a` ← the backed-up `init_boot.img` (or the package's `image/init_boot.img`
  if it is the same version); everything else Skip.

## LTBox stopped in the middle of a write

Leave the cable and LTBox as they are, save the message and the Work History. The `Flashed …` lines show how far it
got. If `efisp` was written but `init_boot_a` failed, write the stock `init_boot.img` to `init_boot_a` with Flash
Partitions; the GBL in `efisp` does not prevent a stock boot. (From the LTBox source; untested.)

## Slot marked unbootable

A partition that fails verified boot makes ABL mark the **whole slot** unbootable in the GPT attribute bits, and the
mark stays after the partition is restored. Fastboot cannot clear it and LTBox does not show the GPT.

Signs: `fastboot getvar slot-unbootable:a` says `yes`; the tablet boots only into fastboot. What worked (verified
twice): the open-source [`edl`](https://github.com/bkerler/edl) tool with the loader `.xml`, after
switching the 9008 device to the **WinUSB** driver with Zadig, rewriting the first 6 sectors of the affected LUN from a
known-good copy (`edl.py rs` to read, compare byte by byte, `edl.py ws` to write, read back, `edl.py reset`; `reset`
takes no `--memory`). The only bytes that change are bit 55 of `boot_a`'s attributes (`0xf7` → `0x77` in its top byte)
and the two CRCs in the header. Flipping the bit alone, without recomputing both CRCs, invalidates the table.

<details>
<summary>Notes from that session (Windows drivers)</summary>

- The serial (COM) driver path never got past the Sahara handshake; the libusb-win32 driver crashed.
- Set `PYTHONIOENCODING=utf-8` on Windows, otherwise the progress bar can crash the tool mid-write.
- After the driver switch LTBox no longer sees the device until the driver is removed in Device Manager.
  Windows keeps the WinUSB binding per USB port, so there can be several entries (`QUSB_BULK…`, USB ID `05C6 9008`),
  hidden while the tablet is not in EDL. To go back (verified): in an administrator PowerShell, list them with
  `pnputil /enum-devices /disconnected` and remove each with `pnputil /remove-device "<instance ID>"`. The next EDL
  entry gets the Qualcomm serial driver again (`QDLoader 9008 (COMn)`) and LTBox sees it; the driver packages Zadig
  installed could stay. If it still comes up as `QUSB_BULK`, delete those packages too (`pnputil /enum-drivers`,
  then `pnputil /delete-driver oemNN.inf`).
- The `edl` README recommends the Qualcomm 9008 serial driver plus UsbDk on Windows instead of Zadig/WinUSB; that
  combination is untested here.

</details>

The `edl` tool on macOS and Linux (untested): [Installing from macOS](install-macos.md#4-recovery-tools),
[Installing from Linux](install-linux.md#4-recovery-tools). The commands are the same on every host.

## Full reinstall

All options here are untested on a tablet; they follow the LTBox source and the package's XML.

- **Same version, keep data:** LTBox → Flash Firmware → matching region → **Keep Data**, from a fresh copy of the
  unpacked package (LTBox writes decrypted files into the folder). This skips `userdata`, `metadata`, `persist`,
  `efisp` and the modem calibration partitions and writes slot `_a` only; root is lost (`init_boot` is stock again),
  `efisp` stays.
- **Do not use Advanced → Simple Firmware Flasher** to keep data: it writes the package's `userdata` and `metadata`
  images as they are and wipes the device.
- **Factory state:** LTBox Flash Firmware → **Wipe Data** (also erases `efisp`), or Software Fix → Rescue (installs
  the newest firmware). Both wipe the tablet.
- Device-specific partitions (`persist`, `modemst*`, `fsg`, …) are restored only from **your own** dump, one partition
  at a time, and only when the symptom clearly points at them. Never use the package's `persist.img`.

## Undoing the root

LTBox → Unroot Device → **Magisk / LKM** (KernelSU in LKM mode lives in `init_boot`), with the backup folder from
rooting and the same loader. It writes only `init_boot_a` from the backup and restarts; Android comes up `green` and
locked, without `su` (verified 2026-11-21, LTBox v3.3.2). The KernelSU manager app stays; uninstall it in Android.

Unroot leaves the GBL in `efisp`. **Erase `efisp` only if `boot_a` holds Lenovo's signed boot image** (same hash as
`image/boot.img` in the package). Without the GBL, ABL verifies `boot_a` itself; if it rejects the image it marks the
slot unbootable and the tablet stops in fastboot, where START and Recovery mode lead back to fastboot →
[Slot marked unbootable](#slot-marked-unbootable). That is what happened here with a self-built kernel in `boot_a`.
Erasing with a stock `boot_a` is untested.

To erase: EDL → Flash Partitions → tick `efisp` twice (first click = Write, second = Erase) → Start. LTBox writes
3 MiB of zeros (verified).

## Updating Android firmware

The order, the way back if Linux does not start afterwards and how to check a new firmware against the kernel:
[Updating Android firmware](firmware-updates.md).
