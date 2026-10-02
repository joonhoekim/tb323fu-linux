# Guided install (prototype)

> **Prototype — not tested on real hardware.** `install.sh` has only been checked for syntax and run in its
> `--dry-run` mode. It has never been pointed at a tablet. Read [docs/install.md](../../docs/install.md) first; the
> script is a walk-through of that guide, not a replacement for it.

[`install.sh`](install.sh) asks before every step and calls the repository's own scripts for the real work:

| Step | What it does | Calls | Destructive? |
|---|---|---|---|
| `check` | host tools, the tablet in rooted Android (model, slot `_a`, root for Shell), your EDL dump | `adb` | no |
| `firmware` | extracts the firmware from Android and copies it to the PC | [`firmware/extract-on-device.sh`](../../firmware/) | no |
| `wayback` | Android's boot image into `boot_b`, the "Switch to Linux" module, saves the hash and the stock boot image | [`android/install-module.sh`](../../android/) | writes `boot_b` (never booted) — asks you to type `WRITE BOOT_B` |
| `bootimg` | prints the kernel / initramfs / boot image commands with your paths filled in | — | no |
| `sdcard` | GPT and ext4 partitions on a microSD card in a card reader | `sgdisk`, `mkfs.ext4` | **wipes the card** — asks you to type the device name |
| `rootfs` | builds a distribution into a partition | [`rootfs/<distro>/build-rootfs.sh`](../../rootfs/) | writes the (empty) partition — asks you to type `BUILD` |
| `boot` | stages the boot image for the Switch to Linux module, or writes `boot_a` with `tools/cycle.sh` | `install-module.sh stage`, [`tools/cycle.sh`](../cycle.sh) | `cycle.sh` writes `boot_a` — asks you to type `FLASH BOOT_A` |
| `firstboot` | waits (at most 180 s) for SSH and runs the first-boot checks | `ssh` | no |
| `ufs` | **only prints** the pointer to the manual procedure: a root on internal storage wipes Android's data | — | — |

Every destructive step explains what it changes, requires the typed confirmation and prints how to undo it.
Nothing is written to the tablet's partition tables by this script.

```sh
tools/install/install.sh --dry-run            # print every command, execute nothing
tools/install/install.sh check firmware       # run only these steps
WORK=~/tb323fu DISTRO=arch tools/install/install.sh
```

Options are environment variables, listed by `install.sh --help` (`WORK`, `DISTRO`, `DUMP_DIR`, `LINUX_BOOT_IMG`,
`MODULES_FROM`, `SD_DEV`, `SD_LAYOUT`, `ROOT_PART`, `ROOT_PARTLABEL`, `TB323FU_HOST`).

## Hosts

| Host | Steps | Status |
|---|---|---|
| Linux | all (`rootfs` only on arm64, as root) | dry run only |
| macOS | `check`, `firmware`, `wayback`, `bootimg`, `boot`, `firstboot` (adb/ssh steps); `sdcard` and `rootfs` need Linux | not run |
| Windows, Git Bash | the adb/ssh steps, as on macOS | dry run only |
| Windows, WSL2 | like Linux, but USB devices (adb, card readers) reach WSL only through usbipd-win | not run |

## Known gaps

- No release images: the boot image still has to be built by hand (`bootimg` prints the commands).
- The rootfs builders need an arm64 Linux host; see [docs/install.md, step 5](../../docs/install.md#5-put-a-root-filesystem-on-it).
- The speaker amplifier driver (aw882xx) is not in the repository yet.
- `sdcard` uses a card reader on the host; the from-Android variant in the guide is not scripted.
