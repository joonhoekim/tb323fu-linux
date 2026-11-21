# Installing from macOS

The same path as on Windows ([Installing Linux](install.md)): root the tablet, then one script takes it to Ubuntu
with GNOME on the microSD card. Rooting works from macOS itself. The install script needs Linux, so on a Mac it runs
in a Linux virtual machine (VM) that reaches the tablet through the Mac's own `adb` server. No USB passthrough is needed.

> **Status: verified up to the first start** ([labels](hardware-status.md#evidence-labels)) on an Apple-silicon Mac with Lima and Ubuntu 26.04 arm64.[^tested]
> Rooting with LTBox and the `edl` recovery tool are **untested** on a Mac; those steps follow their own documentation.
> If something does not match, stop and open an issue.

## 1. Tools for rooting

Rooting follows [Rooting](rooting.md); its LTBox screens should be the same on every PC. On macOS:

1. Install [LTBox](https://github.com/miner7222/LTBox) (macOS 11 or later): `brew tap miner7222/tap`, `brew trust miner7222/tap`,
   `brew install --cask ltbox`, or unpack the `macos_universal` tarball into `/Applications`.
2. The app is only ad-hoc signed, so Gatekeeper blocks the first start. Use
   `xattr -dr com.apple.quarantine /Applications/LTBox.app`, or right-click → Open, or System Settings → Privacy &
   Security → Open Anyway.
3. No USB driver is needed (LTBox bundles libusb). Allow the accessory when macOS asks whether the USB device may
   connect.
4. adb: `brew install --cask android-platform-tools`. It is used for rooting and, in step 3 below, by the VM.
5. The firmware package: Lenovo's Software Fix does not run on macOS, see
   [Getting the firmware package without Windows](rooting.md#getting-the-firmware-package-without-windows).
6. LTBox keeps its settings, adb key and logs in `~/Library/Application Support/ltbox/`. Where it puts the root
   backup on macOS is untested; find that folder after rooting
   (see [rooting step 2](rooting.md#2-root-without-unlocking-ltbox)) and copy it next to your dump.
7. **Starting LTBox stops the Mac's adb server.** Quit LTBox before step 3 below, then run `adb devices` once on the Mac
   to start the server again.

## 2. A Linux virtual machine

The script builds the root filesystem inside the VM (debootstrap, a loop-mounted ext4 image).
[Lima](https://lima-vm.io/) is used here because the VM can reach services on the Mac by name:

```sh
brew install lima
limactl start --name=tb323fu --cpus=4 --memory=8 --disk=60 template:ubuntu-26.04
limactl shell tb323fu
```

- The VM needs **about 30 GB free**; the 60 GB disk above grows only as used. The script's work directory
  (`~/tb323fu-install`) is in the VM's own home, not in the Mac folder Lima shares read-only.
- On Apple silicon the VM is arm64 like the tablet, so the build runs natively and is fast.[^speed] On an Intel Mac
  the VM is x86-64 and the build runs arm64 programs through qemu; the `host` step installs and checks that
  (**untested**).
- Other VMs (UTM, VMware, Parallels) are **untested**. The Mac's adb server then has to listen on the network
  (`adb kill-server; adb -a start-server`), and the Mac's address differs. Lima reaches the Mac's `localhost` as
  `host.lima.internal`, so nothing on the Mac is opened to the network.

## 3. Run the installer

The tablet stays on the Mac's USB. On the Mac,
[prepare it](install.md#prepare-the-tablet); `adb devices` then also starts the adb server the VM uses.

In the VM:

```sh
sudo apt-get update && sudo apt-get install -y adb git
echo 'export ADB_SERVER_SOCKET=tcp:host.lima.internal:5037' >> ~/.bashrc
export ADB_SERVER_SOCKET=tcp:host.lima.internal:5037
adb devices            # the tablet, as on the Mac
git clone https://github.com/joonhoekim/tb323fu-linux.git
cd tb323fu-linux
tools/install/install.sh
```

`ADB_SERVER_SOCKET` makes the VM's `adb` a client of the Mac's server. Files for `adb push` and `adb pull` are read and
written in the VM. The VM's `adb` and the Mac's must speak the same server version ([why](install.md#prepare-the-tablet)).

The steps, what they ask and how long they take: [What the script asks](install.md#what-the-script-asks). Differences on a Mac:

- The `host` step installs its packages with `apt` in the VM; there is no `adb.exe` wrapper as under WSL.
- Keep the Mac awake and the tablet connected while the script talks to the tablet. If the Mac's adb server is
  restarted (`adb kill-server`, a reboot), run `adb devices` on the Mac once to start it again.

## 4. Recovery tools

The one recovery case that needs the separate [`edl`](https://github.com/bkerler/edl) tool
([Slot marked unbootable](recovery.md#slot-marked-unbootable)) runs on macOS itself (**untested**): `brew install libusb git`,
then `pip3 install .` from the clone; no driver. The commands (`edl.py rs` / `ws` / `reset` with the loader `.xml`) are
the same as on Windows.

[^tested]: Run on 2026-10-04 with macOS 26.6.
[^speed]: 7 min for Ubuntu with GNOME on an M4 Pro, against 22 min through qemu on the Windows test PC. Transfers through the VM were as fast as from macOS directly (35 MB/s on a USB 2 link).
