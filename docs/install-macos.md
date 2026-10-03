# Installing from macOS

> **Not verified.** Nothing on this page has been tried from a Mac. The rooting and recovery tools are described from
> their own documentation; the install runs the same script that was followed end to end from Windows
> ([Installing from Windows](install-windows.md)), here inside a Linux virtual machine, which is also untested. If
> something does not match, stop and open an issue.

The same path as on Windows ([Installing Linux](install.md)): root the tablet, then one script takes it to Ubuntu
with GNOME on the microSD card. Rooting works from macOS itself. The install script needs a Linux system, so on a
Mac it runs in a Linux virtual machine that can see the tablet over USB.

## 1. Tools for rooting

Rooting follows [Rooting and dual boot setup](rooting.md); its LTBox screens should be the same on every PC. On macOS:

1. Install [LTBox](https://github.com/miner7222/LTBox): `brew tap miner7222/tap`, `brew trust miner7222/tap`,
   `brew install --cask ltbox`, or unpack the `macos_universal` tarball into `/Applications`. Needs macOS 11 or later.
2. The app is only ad-hoc signed, so Gatekeeper blocks the first start. LTBox's documentation gives three ways:
   `xattr -dr com.apple.quarantine /Applications/LTBox.app`, right-click → Open, or System Settings → Privacy &
   Security → Open Anyway.
3. No USB driver is needed: LTBox bundles libusb. Allow the accessory when macOS asks whether the USB device may
   connect (Apple-silicon Macs ask for new accessories).
4. adb for the rooting steps: `brew install --cask android-platform-tools`.
5. The firmware package: Lenovo's Software Fix does not run on macOS, see
   [Getting the firmware package without Windows](rooting.md#getting-the-firmware-package-without-windows).
6. Where LTBox keeps its backup and log folders on macOS was not checked; find the root backup folder after rooting
   (see [rooting step 2](rooting.md#2-root-without-unlocking-ltbox)) and copy it next to your dump.

## 2. A Linux virtual machine

The install script builds the tablet's root filesystem with Linux tools and talks to the tablet with `adb`; both
happen inside the VM.

- **Apple silicon:** [UTM](https://mac.getutm.app/) (free) with **Ubuntu 24.04 or newer for arm64** (server or
  desktop). The VM is arm64 like the tablet, so the build runs natively, without qemu, and is faster than on an
  x86-64 PC.
- **Intel Mac:** an x86-64 Ubuntu VM (UTM, VirtualBox, VMware); the build then runs arm64 programs through qemu, as on
  any x86-64 Linux.
- Give it **at least 40 GB of disk**, 4 CPU cores and 8 GB of memory.
- **USB:** the tablet has to be connected to the VM, not to macOS. In UTM: the VM's settings → enable USB sharing,
  then the USB icon in the VM's toolbar → pick the tablet. The tablet reconnects whenever it restarts or switches
  USB mode (for example after you allow USB debugging), and you may have to pick it again. Stop adb on macOS first
  (`adb kill-server`), or macOS may hold on to the device.

## 3. Run the installer

In the VM, follow [Installing from Linux](install-linux.md#2-adb) from step 2: install `adb`, check that
`adb devices` lists the tablet, clone the repository and run `tools/install/install.sh`. The steps and timings are in
[Installing from Windows, step 5](install-windows.md#5-what-the-script-asks); the first start and the way back in
[step 6](install-windows.md#6-first-start-and-the-way-back).

If USB passthrough does not work for you, the PC-side steps (`host`, `download`, `bootimg`, `rootfs`) do not need the
tablet. The steps that do can be done from macOS with its own `adb`, with the commands in
[Installing by hand](install-manual.md) (steps 1, 2 and 4–6), writing the image built in the VM.

## 4. Recovery tools

The one recovery case that needs the separate [`edl`](https://github.com/bkerler/edl) tool
([Slot marked unbootable](recovery.md#slot-marked-unbootable)) runs on macOS itself: `brew install libusb git`, then
`pip3 install .` from the clone; no driver. The commands (`edl.py rs` / `ws` / `reset` with the loader `.xml`) are
the same as on Windows.
