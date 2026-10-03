# Recovery

How to get a TB323FU back when Linux, Android or a write goes wrong. It assumes the setup from
[Rooting and dual boot setup](rooting.md): a full EDL dump of your own tablet, LTBox, the firmware package with its
EDL loader, and Android's boot image kept in `boot_b`.

Start with the smallest step: rebooting → fixing `boot_a` from Android (`adb` + `dd`) → unroot → one partition over
EDL → full firmware with data kept → factory reset. Check every file's hash before writing it.

LTBox itself (Flash Partitions, Unroot, Flash Firmware) has builds for Windows, macOS and Linux; host setup is in
[rooting.md → Host computer](rooting.md#host-computer). Only the Windows build was used for the recovery steps here.

| Symptom | Go to |
|---|---|
| Linux does not boot | [Linux does not boot](#linux-does-not-boot) |
| Android does not boot after rooting | [Android does not boot after rooting](#android-does-not-boot-after-rooting) |
| LTBox stopped during a write | [LTBox stopped in the middle of a write](#ltbox-stopped-in-the-middle-of-a-write) |
| Red "Your device is corrupt" screen, slot will not boot after restoring | [Slot marked unbootable](#slot-marked-unbootable) |
| Stuck in a crash/dump loop (900E) | [Getting into EDL](#getting-into-edl), case 2 |

## Modes and how to reach them

| Mode | USB ID | How to get there | Notes |
|---|---|---|---|
| Android (adb) | `17ef:…` | — | `adb reboot edl` goes to EDL (checked) |
| **EDL** | `05c6:9008` "Qualcomm HS-USB QDLoader 9008" | see below | the real recovery mode: partitions can be read and written with a loader |
| Fastboot | — | power + volume up from off (not checked); also shown after a verified-boot rejection | **read-only on this bootloader** (`getvar`, `download`). `START` boots normally. **Do not choose "Boot to Alternate Slot"** — slot `_b` cannot boot |
| Recovery | — | volume down + volume up, then power, from off (checked) | stock AOSP recovery, not TWRP; has "Apply update from ADB"; the default entry is "Enter fastboot" |
| **Crash dump** | `05c6:900E` "Qualcomm HS-USB Diagnostics" | the SoC falls into it after a crash | **not a recovery mode** — nothing can be written. A dump tool gets one Sahara session; afterwards the tablet resets |

In mainline Linux the power button alone does not force the tablet off; hold **power + volume down**.

### Getting into EDL

All three ways were checked. What matters is that volume up is held while ABL reads the keys twice, about 5 s apart,
with USB connected.

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

## Linux does not boot

- If the initramfs comes up (boot summary on the panel, USB serial/network): hold volume up + volume down 10 s, or run
  `back-to-android <hash>` on the serial shell.
- If it hangs before that: EDL → LTBox → Advanced → EDL Operations → **Flash Partitions** → `boot_a` ← your Android
  boot image (the copy in `boot_b`, your dump's `boot_a`, or the package's `boot.img` for the same firmware version);
  all other rows **Skip** → write → Reboot Device → System. Checked several times.
- Afterwards confirm `adb shell getprop ro.boot.slot_suffix` is `_a`.

## Android does not boot after rooting

- If adb works (e.g. from recovery): LTBox → **Unroot Device** (method Magisk / LKM, the backup folder from
  [rooting step 2](rooting.md#2-root-without-unlocking-ltbox)). Unroot can only start from adb or fastboot, not from EDL.
- EDL only: Flash Partitions → `init_boot_a` ← the backed-up `init_boot.img` (or the package's `image/init_boot.img`
  if it is the same version); everything else Skip.

These two paths come from the LTBox source; they were not needed on the development unit.

## LTBox stopped in the middle of a write

Leave the cable and LTBox as they are, save the message and the Work History. The `Flashed …` lines show how far it
got. If `efisp` was written but `init_boot_a` failed, write the stock `init_boot.img` to `init_boot_a` with Flash
Partitions; the GBL in `efisp` does not prevent a stock boot. (From the LTBox source; not experienced here.)

## Slot marked unbootable

A partition that fails verified boot makes ABL mark the **whole slot** unbootable in the GPT attribute bits, and the
mark stays after the partition is restored. Fastboot cannot clear it and LTBox does not show the GPT.

What worked (checked once): the open-source [`edl`](https://github.com/bkerler/edl) tool with the loader `.xml`, after
switching the 9008 device to the **WinUSB** driver with Zadig, rewriting the first 6 sectors of the affected LUN from a
known-good copy (`edl.py rs` to read, compare byte by byte, `edl.py ws` to write, read back, `edl.py reset`). Do not
flip the bit by hand — the GPT header CRC covers it.

<details>
<summary>Notes from that session (Windows drivers)</summary>

- The serial (COM) driver path never got past the Sahara handshake; the libusb-win32 driver crashed.
- Set `PYTHONIOENCODING=utf-8` on Windows, otherwise the progress bar can crash the tool mid-write.
- After the driver switch LTBox no longer sees the device until the driver is removed in Device Manager.
- The `edl` README now recommends the Qualcomm 9008 serial driver plus UsbDk on Windows instead of Zadig/WinUSB; that
  combination was not tried here.

</details>

The `edl` tool on macOS and Linux (not tested here): [Installing from macOS](install-macos.md#4-recovery-tools),
[Installing from Linux](install-linux.md#4-recovery-tools). The commands are the same on every host.

## Full reinstall

- **Same version, keep data:** LTBox → Flash Firmware → matching region → **Keep Data**, from a fresh copy of the
  unpacked package (LTBox writes decrypted files into the folder). Per the LTBox source and the package's XML this
  skips `userdata`, `metadata`, `persist`, `efisp` and the modem calibration partitions and writes slot `_a` only;
  root is lost (`init_boot` is stock again), `efisp` stays. Not run on the development unit.
- **Do not use Advanced → Simple Firmware Flasher** to keep data: it writes the package's `userdata` and `metadata`
  images as they are and wipes the device.
- **Factory state:** LTBox Flash Firmware → **Wipe Data** (also erases `efisp`), or Software Fix → Rescue (installs
  the newest firmware). Both wipe the tablet.
- Device-specific partitions (`persist`, `modemst*`, `fsg`, …) are restored only from **your own** dump, one partition
  at a time, and only when the symptom clearly points at them. Never use the package's `persist.img`.

## Undoing the root

LTBox → Unroot Device restores `init_boot` only; it leaves the GBL in `efisp`. To clear `efisp` too, use Flash
Partitions with `efisp` set to Erase (or write your all-zero `efisp.img` from the dump) — after making sure
`init_boot_a` is stock. Not done on the development unit.

## Updating Android firmware later

Not done on the development unit since rooting. The order prepared from the LTBox source:

1. Confirm the new firmware's ABL still loads `efisp` (otherwise you cannot root again, and the update cannot be undone).
2. Restore the stock `boot_a` and unroot.
3. Enable the OTA apps and update; disable them again.
4. Root again (LTBox then writes only `init_boot`).
5. Redo `prepare-boot-b` / `install` ([rooting step 4](rooting.md#set-up-the-way-back)) and update the hash on the Linux side.
