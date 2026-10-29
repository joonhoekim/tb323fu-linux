# Guided install

[`install.sh`](install.sh) takes a rooted TB323FU ([rooting.md](../../docs/rooting.md) steps 1–3) to Ubuntu with GNOME
on the microSD card, with Android kept as the way back. On Windows it runs in a WSL2 Ubuntu terminal and uses the
Windows `adb.exe`; a native Linux PC works the same way with its own `adb`; on a Mac it runs in a Linux VM whose
`adb` uses the Mac's adb server (`ADB_SERVER_SOCKET`, [install-macos.md](../../docs/install-macos.md)). The pages
to follow start at [docs/install.md](../../docs/install.md) (one per PC); the individual commands are in
[docs/install-manual.md](../../docs/install-manual.md).

> **Status** per PC: [docs/install.md → Your PC](../../docs/install.md#your-pc); per system: [distros/](distros/README.md).

```sh
tools/install/install.sh              # every step, asking before each
tools/install/install.sh --dry-run    # print every command, run none (no tablet needed)
tools/install/install.sh rootfs       # run one step again
tools/install/install.sh --answers FILE   # no questions (below)
```

| Step | What it does | Calls | Writes |
|---|---|---|---|
| `host` | apt packages, arm64 through qemu (binfmt with the `F` flag), ~30 GB free, finds `adb.exe` and writes the `adb` wrapper | `apt-get` | the PC only |
| `tablet` | `adb devices`, TB323FU, slot `_a`, root for Shell, the microSD card, battery; explains each failure (unauthorized, no root, tablet in Linux) | `adb` | — |
| `firmware` | firmware from Android's `/vendor` to `$WORK/fw` | [`firmware/extract-on-device.sh`](../../firmware/) | `/data/local/tmp` |
| `wayback` | `boot_a` == `boot_b`, else copies it; installs the Switch to Linux module if missing; saves the hash and the stock boot image (from `boot_b`, or `STOCK_BOOT` checked against it) | [`android/install-module.sh`](../../android/) | `boot_b` — typed `WRITE BOOT_B` |
| `download` | newest `kernel-t*` and `helper-v*` releases, checked against `SHA256SUMS`; the newest stable release (a pre-release only when there is none); the public API (`GITHUB_TOKEN` optional, for its rate limit), else files downloaded in the browser are picked up from Downloads | `curl`, GitHub API | the PC only |
| `bootimg` | the release kernel into your stock boot image | [`tools/boot-repack-kernel.py`](../boot-repack-kernel.py) | the PC only |
| `sdcard` | GPT built with `sgdisk` in a file of the card's size, first 34 and last 33 sectors written to the card from Android (install-manual.md step 4); one partition `baldur-root-sd` | `sgdisk`, `adb` | **the card is wiped** — typed `ERASE` |
| `rootfs` | the chosen distribution (a [module](distros/README.md), default Ubuntu) into a sparse ext4 image (`IMG_SIZE`, at most the partition), adds `tb323fu-growroot.service`, sets the user's password, shrinks the image to its contents + 2 GiB, gzip | the module's builder, e.g. [`rootfs/ubuntu/build-rootfs.sh`](../../rootfs/ubuntu/build-rootfs.sh) | the PC only |
| `write` | pushes the image, `zcat \| dd` into the partition, reads it back and compares | `adb` | the partition — typed `WRITE` |
| `boot` | stages the boot image for the module, writes `boot_a` from the staged copy, checks the hash (on a mismatch: `boot_b` back into `boot_a`, no reboot), reboots | `install-module.sh stage`, `adb` | `boot_a` — typed `FLASH BOOT_A` |
| `firstboot` | what to expect, checks, the way back | — | — |

State (finished steps, hashes, partition, release tags) is kept in `$WORK/state`; a finished step is skipped on the
next run, a named step always runs. Options are environment variables, listed by `install.sh --help` (`WORK`,
`ROOT_PARTLABEL`, `ROOT_SIZE`, `DEV_USER`, `DESKTOP`, `DISTRO`, `DEV_ACCESS`, `STOCK_BOOT`, `KERNEL_TAG`, `HELPER_TAG`,
`GITHUB_TOKEN`, `ADB`, `ANDROID_SERIAL`).

## Without questions

```sh
cp tools/install/answers.example my-answers     # then edit it
tools/install/install.sh --answers my-answers
```

With `--answers FILE` (or `NONINTERACTIVE=1`) the script asks nothing: yes/no questions are answered yes and the others
with their default. What cannot be undone is not covered by that: the EDL dump question needs `HAVE_EDL_DUMP=yes`, each
writing step needs its word in `CONFIRM` (`WRITE BOOT_B`, `ERASE`, `WRITE`, `FLASH BOOT_A`), erasing the card also needs
`ERASE_CARD` to name that card (`DEV:SECTORS`, printed by the `tablet` step), and the `rootfs` step needs
`USER_PASSWORD` or `USER_PASSWORD_FILE`. Where the script would wait for you (the tablet not found, no root,
downloads by hand) it stops instead; fix it and run it again. `apt` still needs `sudo`: run `sudo -v` first if yours
asks for a password. A password written in a command line can end up in the shell history; a file avoids that.

## adb in WSL

WSL sees no USB devices. `host` finds the Windows `adb.exe` (on the Windows `PATH`, WinGet's and Android Studio's
folders, `platform-tools` in the user folder, or a path you type) and writes `$WORK/bin/adb`, which the script and the
repository's scripts it calls (`install-module.sh`) use as `adb`. The wrapper turns local file names of `push`, `pull`
and `install` into Windows paths (`\\wsl.localhost\…`); if `adb.exe` cannot read one, it copies the file through
`%TEMP%\tb323fu-adb`. For `adb shell CMD` it strips `\r` and does not pass the terminal on. `ANDROID_SERIAL` is turned
into `-s`, since Windows programs do not see WSL's environment.

## Not covered

Distributions without a module ([distros/](distros/README.md)), a card reader on the PC,
a kernel you build yourself, and a root on the internal storage (which wipes Android's data) are manual:
[docs/install-manual.md](../../docs/install-manual.md).
