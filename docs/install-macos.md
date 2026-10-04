# Installing from macOS

> **Followed end to end once, up to the first start.** On 2026-10-04, from an Apple-silicon Mac (macOS 26.6) with
> Lima and Ubuntu 26.04 arm64, every step of the install script ran against a rooted tablet (Ubuntu with GNOME built
> in 7 min, the card written in about 4 min), and the tablet then started from the card into the GNOME desktop
> (seen by a person). Sound, Wi-Fi and the way back to Android were not checked again from the Mac; they are the same
> on the tablet whichever PC installed it ([Installing from Windows](install-windows.md)). LTBox 3.3.3 (the
> `macos_universal` tarball) started and recognized the tablet over adb; rooting with it and the `edl` recovery tool
> were not tried on a Mac and are described from their own documentation. If something does not match, stop and open
> an issue.

The same path as on Windows ([Installing Linux](install.md)): root the tablet, then one script takes it to Ubuntu
with GNOME on the microSD card. Rooting works from macOS itself. The install script needs a Linux system, so on a
Mac it runs in a Linux virtual machine; the tablet stays connected to macOS, and the VM reaches it through the Mac's
own `adb` server. No USB passthrough is needed.

## 1. Tools for rooting

Rooting follows [Rooting and dual boot setup](rooting.md); its LTBox screens should be the same on every PC. On macOS:

1. Install [LTBox](https://github.com/miner7222/LTBox): `brew tap miner7222/tap`, `brew trust miner7222/tap`,
   `brew install --cask ltbox`, or unpack the `macos_universal` tarball into `/Applications`. Needs macOS 11 or later.
2. The app is only ad-hoc signed, so Gatekeeper blocks the first start. LTBox's documentation gives three ways:
   `xattr -dr com.apple.quarantine /Applications/LTBox.app`, right-click → Open, or System Settings → Privacy &
   Security → Open Anyway.
3. No USB driver is needed: LTBox bundles libusb. Allow the accessory when macOS asks whether the USB device may
   connect (Apple-silicon Macs ask for new accessories).
4. adb: `brew install --cask android-platform-tools`. It is used for rooting and, in step 3 below, by the VM.
5. The firmware package: Lenovo's Software Fix does not run on macOS, see
   [Getting the firmware package without Windows](rooting.md#getting-the-firmware-package-without-windows).
6. LTBox keeps its settings, its own adb key and its logs in `~/Library/Application Support/ltbox/`. Where it puts the
   root backup on macOS was not checked; find that folder after rooting
   (see [rooting step 2](rooting.md#2-root-without-unlocking-ltbox)) and copy it next to your dump.
7. LTBox talks to the tablet over USB itself, and **starting it stops the Mac's adb server**. Quit LTBox before
   step 3 below, then run `adb devices` once on the Mac to start the server again.

## 2. A Linux virtual machine

The install script builds the tablet's root filesystem with Linux tools (debootstrap, a loop-mounted ext4 image)
inside the VM. [Lima](https://lima-vm.io/) is used here because it lets the VM reach services on the Mac by name:

```sh
brew install lima
limactl start --name=tb323fu --cpus=4 --memory=8 --disk=60 template:ubuntu-26.04
limactl shell tb323fu
```

- On Apple silicon the VM is arm64 like the tablet, so the build runs natively, without qemu (7 min for Ubuntu
  with GNOME on an M4 Pro, against 22 min through qemu on the Windows test PC). On an Intel Mac the
  same template gives an x86-64 VM; the build then runs arm64 programs through qemu, as on any x86-64 Linux
  (the `host` step installs and checks that). The Intel case was not tried.
- The VM needs **about 30 GB free** for the build; the disk above is 60 GB and grows only as it is used. The
  script's work directory (`~/tb323fu-install`) is in the VM's own home, not in the Mac's home folder that Lima
  shares into the VM (read-only).
- Other VMs (UTM, VMware, Parallels) should work too, but the address of the Mac differs and the Mac's adb server
  then has to listen on the network, not only on `localhost` (`adb kill-server; adb -a start-server`); this was
  not tried. Lima reaches the Mac's `localhost` as `host.lima.internal`, so nothing on the Mac is opened to the
  network.

## 3. Run the installer

The tablet stays on the Mac's USB. On the Mac, prepare it as in
[Installing from Windows, step 3](install-windows.md#3-prepare-the-tablet) and allow USB debugging for the Mac;
`adb devices` then lists it as `device`. The adb server that this starts on the Mac is the one the VM uses.

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

`ADB_SERVER_SOCKET` makes the VM's `adb` a client of the Mac's server: commands go to the tablet through the Mac,
and files for `adb push` and `adb pull` are read and written in the VM. The VM's `adb` and the Mac's must speak the
same server version (`adb version`, first line: `1.0.41` for both on 2026-10-04); if they do not, the VM's `adb`
tries to restart the server and fails.

The steps, what they ask and how long they take are in
[Installing from Windows, step 5](install-windows.md#5-what-the-script-asks); the first start and the way back in
[After installing](after-install.md#first-start). Differences on a Mac:

- The `host` step installs its packages with `apt` in the VM; there is no `adb.exe` wrapper as under WSL.
- Keep the Mac awake and the tablet connected while the script talks to the tablet. If the Mac's adb server is
  restarted (`adb kill-server`, a reboot), run `adb devices` on the Mac once to start it again.
- Transfers run at the speed of the tablet's USB link; through the VM they were as fast as from macOS directly
  (35 MB/s on a USB 2 link).

## 4. Recovery tools

The one recovery case that needs the separate [`edl`](https://github.com/bkerler/edl) tool
([Slot marked unbootable](recovery.md#slot-marked-unbootable)) runs on macOS itself: `brew install libusb git`, then
`pip3 install .` from the clone; no driver. The commands (`edl.py rs` / `ws` / `reset` with the loader `.xml`) are
the same as on Windows. Not tried on a Mac.
