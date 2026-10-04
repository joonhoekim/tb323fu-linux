# Installing from Windows

From a rooted TB323FU to Ubuntu with GNOME on the microSD card, using one Ubuntu terminal on Windows and one script.
Everything happens in that terminal; the script tells you what it is doing, asks before each step and can be run again
after any failure. [Installing Linux](install.md) shows where this fits; [Installing by hand](install-manual.md) has
the individual commands.

> **Status:** followed end to end on 2026-10-04: Windows 11, WSL2 Ubuntu 26.04, a rooted TB323FU with an empty
> 64 GB partition on the card, kernel release `kernel-t38`. Every step below ran, and the first start, Wi-Fi, the
> speakers, both ways back to Android and the way back to Linux were checked on the tablet. That run found five bugs,
> which are fixed: no sound card, Wi-Fi asking for no password, the way back to Android failing on Ubuntu, a
> script that was not executable, and firmware files owned by the PC user. If something does not match what you
> see, stop and open an issue.

## What you need

- The tablet **rooted**: [rooting.md](rooting.md) steps 1–3 done. That means your own **EDL dump kept somewhere other
  than this PC**, KernelSU with **Shell** allowed, and the OTA apps disabled.
- A **microSD card** of 64 GB or more in the tablet. **It will be wiped.**
- A USB-C **data** cable, battery above 50 %.
- About **30 GB free** on the Windows drive that holds WSL (normally C:).
- About **1–2 hours**, most of it an unattended build on the PC (the test run took a little over an hour; the build
  was 22 minutes of it).

## 1. Install Ubuntu on Windows (WSL2)

In **PowerShell as administrator**:

```powershell
wsl --install -d Ubuntu
```

Restart Windows if asked. Open **Ubuntu** from the Start menu and choose a user name and password (this is your
Linux user on the PC; you will type the password when the script installs packages). Check in PowerShell that it is
WSL **2**:

```powershell
wsl -l -v          # VERSION must be 2
```

Ubuntu 24.04 or newer works; 26.04 was used here.

## 2. Install adb on Windows

WSL cannot see USB devices; the script uses the Windows `adb.exe` from Android platform-tools. If you used `adb`
while rooting, you already have it. Otherwise, in PowerShell:

```powershell
winget install Google.PlatformTools
```

(or unzip [platform-tools](https://developer.android.com/tools/releases/platform-tools) somewhere; the script asks for
the path of `adb.exe` if it does not find it). Then **close all terminals**, run `wsl --shutdown` in PowerShell, and
open Ubuntu again, so that it sees the new program.

## 3. Prepare the tablet

1. Start the tablet in **Android**. (If it runs this project's Linux: Open Device Helper → **Android** → **Restart
   into Android**.)
2. **USB debugging:** Settings → About tablet → tap the build number seven times; then Developer options →
   **USB debugging** on.
3. Connect it to the PC. On the tablet, allow USB debugging for this computer (tick **Always allow**).
4. In the **KernelSU** app → Superuser: allow **Shell**.

Check in PowerShell: `adb devices` lists the tablet as `device`.

## 4. Run the installer

In the **Ubuntu** terminal:

```sh
sudo apt update && sudo apt install -y git
git clone https://github.com/joonhoekim/tb323fu-linux.git
cd tb323fu-linux
tools/install/install.sh
```

Keep the clone in your Ubuntu home directory (as above), not under `/mnt/c`. Another system instead of Ubuntu:
`DISTRO=arch tools/install/install.sh` (or `nixos`, `steamos`; [which system](install.md#which-system)).

To see everything the script would do without doing it: `tools/install/install.sh --dry-run`.

## 5. What the script asks

Each step explains itself first. Steps that write to the tablet make you **type a word** (`WRITE BOOT_B`, `ERASE`,
`WRITE`, `FLASH BOOT_A`); anything else stops without writing. Finished steps are remembered (in
`~/tb323fu-install/state`): if a step fails, fix what it says and run `tools/install/install.sh` again; it carries on
where it stopped. `tools/install/install.sh STEP` runs one step again.

| Step | What happens | What you do | Time |
|---|---|---|---|
| `host` | installs the Ubuntu packages it needs (`debootstrap`, `qemu-user-binfmt`, `gdisk`, …), checks that arm64 programs run through qemu, finds `adb.exe` | your Ubuntu password | 1–10 min |
| `tablet` | checks the tablet: TB323FU, slot `_a`, root for Shell, the microSD card, battery | answer the prompts on the tablet; confirm you have your EDL dump | 1 min |
| `firmware` | copies the firmware Linux needs from Android's `/vendor` to the PC (nothing on the tablet changes) | — | 1 min |
| `wayback` | makes sure `boot_b` holds your Android boot image (the way back), installs the **Switch to Linux** KernelSU module, saves the hash and your stock boot image | type `WRITE BOOT_B` only if `boot_b` needs the copy | 1 min |
| `download` | the newest kernel release and the Ubuntu packages for the tablet from GitHub Releases, checked against `SHA256SUMS` | (browser download only without a GitHub login, see above) | 1–3 min |
| `bootimg` | packs the release kernel into **your own** stock boot image ([why](install-manual.md#why-there-is-no-ready-made-bootimg)) | — | seconds |
| `sdcard` | builds a partition table on the PC and writes it to the card from Android: one partition `baldur-root-sd` (64 GB on a large card, else the whole card) | pick the size; type `ERASE` | 1 min |
| `rootfs` | builds Ubuntu 26.04 with GNOME into an image file on the PC (arm64 through qemu), then shrinks and packs it | your user name for the tablet; a password for it (needed for `sudo`, asked after the build) | **20–60 min** |
| `write` | copies the image to the tablet and writes it into the card's partition, then reads it back and compares | type `WRITE` | 3–10 min |
| `boot` | writes the Linux boot image to `boot_a` (checked; on a bad write it puts Android back first) and restarts into Linux | type `FLASH BOOT_A` | 1 min |
| `firstboot` | prints what a good first start looks like and what to check | — | — |

The tablet is not needed during the `rootfs` build; it can stay connected.

## 6. First start and the way back

The Lenovo logo, then a text summary on the panel, then Ubuntu. The first start takes a little longer (the root grows
to fill the partition); after that the desktop is up about 12 s after the kernel starts. GNOME logs you in automatically.

**Wi-Fi:** top right → Wi-Fi → pick your network → a notification asks for the password. Until then the clock is
wrong (Android keeps the real time in a place Linux cannot read); it sets itself once the network is up. The time
zone comes from the PC you built on. A Wi-Fi icon with a `?` means the internet check failed; it usually clears
within a minute.

In GNOME's Terminal:

```sh
uname -r                                # the release kernel
systemctl is-system-running             # running, or degraded (see systemctl --failed)
cat /etc/tb323fu/android-boot.sha256    # the hash the script printed in the wayback step
```

Try the way back once while you are at the desk:

- **To Android:** Open Device Helper → **Android**, or the Android tile in the quick settings, or hold
  **volume up + volume down for 10 s** (works even when the desktop hangs).
- **To Linux:** KernelSU app → Modules → **Switch to Linux** → Action.

Afterwards `~/tb323fu-install/root.img` and `root.img.gz` can be deleted (keep `stock-boot.img`, `linux-boot.img` and
`config/`). The WSL disk does not shrink by itself when files are deleted.

## Troubleshooting

| Problem | What to do |
|---|---|
| **`adb.exe` not found** | Install platform-tools (step 2), then `wsl --shutdown` in PowerShell and reopen Ubuntu. Or type the path of `adb.exe` when the script asks (`C:\…\platform-tools\adb.exe` is fine), or start it as `ADB=/mnt/c/…/adb.exe tools/install/install.sh`. |
| **No tablet** | `adb devices` in **PowerShell** must list it. If PowerShell does not see it either, it is the cable, USB debugging, or the Windows driver — not WSL. If it runs Linux, switch it to Android first. |
| **`unauthorized`** | Unlock the tablet and accept the USB debugging prompt (tick Always allow). No prompt: Developer options → Revoke USB debugging authorizations, replug. |
| **No root for adb's shell** | KernelSU app → Superuser → **Shell** → allow. |
| **arm64 programs not registered** (binfmt) | `sudo systemctl restart systemd-binfmt`. If WSL runs without systemd: add `[boot]` / `systemd=true` to `/etc/wsl.conf`, `wsl --shutdown`, reopen. Check: `cat /proc/sys/fs/binfmt_misc/qemu-aarch64` says `enabled` and its flags contain `F`. |
| **Disk space** | The build needs about 30 GB inside WSL. `df -h ~` shows what is free. |
| **The build stopped** | Usually a network hiccup while downloading packages: run the script again, it continues in the same image. To start the root over: `sudo umount ~/tb323fu-install/mnt; rm ~/tb323fu-install/root.img`, then run `tools/install/install.sh rootfs`. |
| **Downloads fail** | Usually the network, or the GitHub API's limit for anonymous requests: run the step again later, or set `GITHUB_TOKEN=…` (any GitHub token); otherwise the script shows which files to download in the browser. |
| **`adb push` fails** on a file in WSL | The script retries through a folder in your Windows temp directory by itself. |
| **Android says the card is unsupported** | Expected after the `sdcard` step: Android cannot read the Linux partition. |
| **Linux does not start** | Hold volume up + volume down 10 s for Android; if nothing reacts, see [Recovery](recovery.md#linux-does-not-boot). |

On a Linux PC: [Installing from Linux](install-linux.md). Other distributions, your own kernel and a root on the
internal storage are in [Installing by hand](install-manual.md).
