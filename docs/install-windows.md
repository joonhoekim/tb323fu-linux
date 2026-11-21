# Installing from Windows

From a rooted TB323FU to Ubuntu with GNOME on the microSD card, with one script run in a WSL terminal.
The build uses a WSL distribution of its own, which you remove afterwards. The script asks before each step and can be
run again after any failure. [Installing Linux](install.md) shows where this fits; [Installing by hand](install-manual.md)
has the individual commands.

> **Status: verified** ([labels](hardware-status.md#evidence-labels)), including first start, Wi-Fi, SSH, switching to Android and `wsl --unregister tb323fu`.[^tested]
> If something does not match what you see, stop and open an issue.

## What you need

- The tablet **rooted**: [rooting.md](rooting.md) steps 1–3 done (your own **EDL dump kept somewhere other than this
  PC**, KernelSU with **Shell** allowed, OTA apps disabled).
- A **microSD card** of 64 GB or more in the tablet. **It will be wiped.**
- A USB-C **data** cable, battery above 50 %.
- About **30 GB free** on the Windows drive that holds WSL (normally C:).
- About **1–2 hours**, most of it an unattended build on the PC.

## 1. A WSL distribution for the installer

In **PowerShell** (as administrator if WSL is not installed yet):

```powershell
wsl --install Ubuntu-26.04 --name tb323fu
```

Restart Windows if asked. The new Ubuntu asks for a user name and password (you type the password when the script
installs packages). Later, open it with `wsl -d tb323fu` in PowerShell. Check that it is WSL **2**:

```powershell
wsl -l -v          # tb323fu, VERSION 2
```

It holds the script's packages and about 30 GB of build files; you remove it in one command when Linux runs
([step 6](#6-first-start)). A WSL distribution you already use also works, but then keeps those packages and files.

## 2. Install adb on Windows

WSL cannot see USB devices, so the script uses the Windows `adb.exe` from Android platform-tools. If you used `adb`
while rooting, you have it. Otherwise, in PowerShell:

```powershell
winget install Google.PlatformTools
```

(or unzip [platform-tools](https://developer.android.com/tools/releases/platform-tools) somewhere; the script asks for
the path of `adb.exe` if it does not find it). Then **close all terminals**, run `wsl --shutdown` in PowerShell, and
reopen `wsl -d tb323fu` so that it sees the new program.

## 3. Prepare the tablet

Android running, USB debugging allowed for this PC, **Shell** allowed in KernelSU: [Prepare the tablet](install.md#prepare-the-tablet).
Check in PowerShell that `adb devices` lists it as `device`.

## 4. Run the installer

In the **tb323fu** terminal (`wsl -d tb323fu`, then `cd`):

```sh
sudo apt update && sudo apt install -y git
git clone https://github.com/joonhoekim/tb323fu-linux.git
cd tb323fu-linux
tools/install/install.sh
```

Keep the clone in your home directory there, not under `/mnt/c`. Another system instead of Ubuntu:
`DISTRO=arch tools/install/install.sh` (or `debian`, `fedora`, `nixos`, `steamos`; [which system](install.md#which-system)).
`tools/install/install.sh --dry-run` shows what the script would do without doing it.

## 5. What the script asks

The steps, what you type and how long each takes: [What the script asks](install.md#what-the-script-asks).
Here the state is in `~/tb323fu-install/state` of the `tb323fu` distribution.

## 6. First start

The script restarts the tablet into Linux. What a good first start looks like, Wi-Fi, the checks and both ways back to
Android are in **[After installing](after-install.md)**; try the way back once while you are at the desk.

The script copies the files worth keeping (`stock-boot.img`, `linux-boot.img`, `config/`) to
`%USERPROFILE%\tb323fu-install` on Windows and cleans up the tablet itself. When you no longer need the installer's
distribution, remove it with everything in it, in PowerShell:

```powershell
wsl --unregister tb323fu
```

If you ran the script in a distribution you keep, delete `~/tb323fu-install` instead.[^sparse]

## Troubleshooting

| Problem | What to do |
|---|---|
| **`adb.exe` not found** | Install platform-tools (step 2), then `wsl --shutdown` in PowerShell and reopen it (`wsl -d tb323fu`). Or type the path of `adb.exe` when the script asks (`C:\…\platform-tools\adb.exe` is fine), or start it as `ADB=/mnt/c/…/adb.exe tools/install/install.sh`. |
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

Once Linux runs: [After installing](after-install.md) (switching, updates, removing Linux). On a Linux PC:
[Installing from Linux](install-linux.md); on a Mac: [Installing from macOS](install-macos.md). Other distributions, your
own kernel and a root on the internal storage are in [Installing by hand](install-manual.md).

[^tested]: Last full run 2026-11-05 on Windows 11 with a fresh `tb323fu` distribution and a wiped card (kernel t58, helper 0.4.6). Speakers, sensors and both cameras were checked on 2026-10-29.
[^sparse]: A WSL disk does not shrink when files are deleted. Sparse mode returns only space freed after it was turned on, so turn it on first (`wsl --shutdown`, then `wsl --manage <name> --set-sparse true`), then delete. It is still marked experimental ([WSL settings](https://learn.microsoft.com/en-us/windows/wsl/wsl-config)), and the space shows up as free space on `C:`, not as a smaller `ext4.vhdx`.
