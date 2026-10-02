# Switching between Android and Linux

This directory describes the switching mechanism. Rooting, setting it up and recovery are in
[docs/rooting.md](../docs/rooting.md) and [docs/recovery.md](../docs/recovery.md).

Android stays installed. Linux boots from the same `boot` partition slot Android uses, and each side can hand the tablet back to the other
without a PC.

## The model

- The tablet always boots **slot `_a`**. Slot `_b`'s `boot_b` partition is never booted, so it is used as storage.
- **`boot_b` keeps a copy of your stock Android boot image** — the way back. Its sha256 is recorded once and every tool checks it before
  writing anything: if `boot_b` is not that image, nothing is written.
- **Linux runs by writing its boot image into `boot_a`**; **Android comes back by copying `boot_b` into `boot_a`**. Only `boot_a` changes.
  Partitions are always found by GPT name (`boot_a`, `boot_b`), never by number — the same storage unit holds the bootloaders.

| Direction | How | Where the code runs |
|---|---|---|
| Android → Linux | KernelSU module **Switch to Linux**: its Action button writes the Linux image to `boot_a`, verifies it and reboots | Android (`switch-to-linux/action.sh`) |
| Linux → Android | `back-to-android <hash>`: copies `boot_b` into `boot_a`, verifies, reboots. Also saves the running Linux image so the Action button boots the same one back | Linux (`back-to-android`, installed as `/usr/local/sbin/back-to-android`) |
| Linux → Android, emergency | **hold volume up + volume down together for 10 s** — runs `back-to-android` even if the desktop is frozen (as long as the kernel runs) | Linux root filesystem service |
| from a PC | `tools/cycle.sh` (through Android and adb) or `tools/flash-boot.sh` (from the running Linux over SSH) | PC |

Which image the Action button writes: the Linux image that ran last (saved by `back-to-android` to `/var/lib/tb323fu/linux-current.img` on
the Linux state root, mounted read-only from Android), else a staged fallback in `/data/adb/tb323fu/linux.img`. The state root is the
first present of `baldur-root`, `baldur-root-sd`, then the `tb323fu-*` partitions in sorted order — the root the initramfs reads the boot
selection from ([kernel/initramfs](../kernel/initramfs/README.md#root-partitions-and-multiboot)); `back-to-android` mounts it when
running from another root. `test-state-root.sh` checks both scripts' choice offline.
If the written `boot_a` does not verify, it copies Android back from `boot_b` before giving up.

If both directions fail, the tablet can still be recovered with Qualcomm EDL and a full backup — make one before trying any of this
([recovery](../docs/recovery.md)).

## Files

| File | What it is |
|---|---|
| `switch-to-linux/` | the KernelSU module (`module.prop`, `action.sh`); `android.sha256` is added at install time |
| `install-module.sh` | PC helper: `prepare-boot-b` (copy the running Android boot image into `boot_b` once), `install` (build the module with the hash of `boot_b` and install it), `stage IMG`, `status` |
| `back-to-android` | the Linux-side script (busybox sh) |

## Setting it up

`prepare-boot-b`, then `install` (note the printed hash — the Linux side needs it), then `stage linux-boot.img`.
The full steps are in [docs/rooting.md](../docs/rooting.md#set-up-the-way-back).

Nothing here contains device-specific data; the Android image hash is computed from your own `boot_b`.
