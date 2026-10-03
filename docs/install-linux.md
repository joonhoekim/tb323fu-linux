# Installing from Linux

> **Not verified separately.** Nothing on this page has been tried from a Linux PC. The installer script, the root
> filesystem builder and the commands that talk to the tablet are the ones that were followed end to end from
> Windows ([Installing from Windows](install-windows.md)); there they ran in an Ubuntu system under WSL2, so a
> Debian or Ubuntu PC should behave the same. The rooting and recovery tools below are described from their own
> documentation. If something does not match, stop and open an issue.

The same path as on Windows ([Installing Linux](install.md)): root the tablet, then one script on the PC takes it to
Ubuntu with GNOME on the microSD card. What differs on Linux is how the tools are installed and that the script uses
your own `adb`.

## 1. Tools for rooting

Rooting follows [Rooting and dual boot setup](rooting.md); its LTBox screens should be the same on every PC. On Linux:

1. Install [LTBox](https://github.com/miner7222/LTBox): on Debian/Ubuntu from LTBox's APT repository or the `.deb`, on
   Fedora from its DNF repository or the `.rpm`, on Arch from the AUR (`ltbox-bin`, a community package), or unpack
   the tarball. The repositories and their keys are in [LTBox's documentation](https://miner7222.github.io/ltbox/en/index.html).
2. USB access without root: with the tarball run `sudo ./ltbox --install-udev` once, then **unplug and replug** the
   tablet (the packages are expected to ship the rule; check). It lets the desktop session open the Qualcomm 9008
   and Lenovo USB devices.
3. Run LTBox as your normal user (not with `sudo`). It needs libusb-1.0, libudev, xkbcommon, Wayland or X11 libraries
   and fontconfig, usually already present on a desktop.
4. If **ModemManager** is running, it may probe the 9008 serial device and disturb the EDL session; the `edl` tool's
   README says to stop it (`sudo systemctl stop ModemManager`). Whether LTBox is affected is not known; stopping it
   for the session costs nothing.
5. The firmware package: Lenovo's Software Fix runs only on Windows, see
   [Getting the firmware package without Windows](rooting.md#getting-the-firmware-package-without-windows).

## 2. adb

Your distribution's `android-tools` (Arch, Fedora) or `adb` (Debian, Ubuntu) package. With systemd 258 or newer adb
access works through systemd's built-in rule; on older systems install `android-udev-rules` (or your distribution's
equivalent). Prepare the tablet as in [Installing from Windows, step 3](install-windows.md#3-prepare-the-tablet); then
`adb devices` lists it as `device`.

## 3. Run the installer

```sh
git clone https://github.com/joonhoekim/tb323fu-linux.git
cd tb323fu-linux
tools/install/install.sh
```

The steps, what they ask and how long they take are in
[Installing from Windows, step 5](install-windows.md#5-what-the-script-asks); the first start and the way back in
[step 6](install-windows.md#6-first-start-and-the-way-back). Differences on Linux:

- **Packages.** On Debian and Ubuntu the `host` step installs what it needs with `apt` (`debootstrap`,
  `ubuntu-keyring`, `qemu-user-binfmt`, `gdisk`, `e2fsprogs`, `pigz`, …). Elsewhere it lists them and you install
  them yourself; the names differ. The builder needs `debootstrap` and Ubuntu's archive key at
  `/usr/share/keyrings/ubuntu-archive-keyring.gpg` (Arch: `debootstrap` `ubuntu-keyring`).
- **arm64 programs.** On an x86-64 PC the root is built through qemu: `qemu-aarch64` must be registered with
  binfmt_misc **with the `F` flag** (`cat /proc/sys/fs/binfmt_misc/qemu-aarch64`). Debian/Ubuntu's
  `qemu-user-binfmt` does that; on Arch `qemu-user-static` + `qemu-user-static-binfmt`, on Fedora
  `qemu-user-static-aarch64`. On an arm64 PC nothing is needed and the build is faster.
- **adb** is the system's (`ADB=/path/to/adb` to pick another); there is no wrapper as under WSL.
- `sudo` is used for the packages, the loop-mounted image and the build.

## 4. Recovery tools

The one recovery case that needs the separate [`edl`](https://github.com/bkerler/edl) tool
([Slot marked unbootable](recovery.md#slot-marked-unbootable)): stop ModemManager for the session, run the
repository's `install-linux-edl-drivers.sh` (udev rules for 9008, blacklists `qcserial`), rebuild the initramfs and
reboot; then `pip3 install .` from the clone. The commands (`edl.py rs` / `ws` / `reset` with the loader `.xml`)
are the same as on Windows.

## NixOS

- LTBox has no nixpkgs package and no AppImage (checked October 2026). The Linux tarball is a dynamically linked
  binary, so it needs [`nix-ld`](https://github.com/nix-community/nix-ld) (`programs.nix-ld.enable = true;` plus the
  libraries above in `programs.nix-ld.libraries`) or `steam-run ./ltbox`.
- `ltbox --install-udev` writes into `/etc/udev/rules.d`, which NixOS does not manage. Put an equivalent rule into
  the configuration instead, for example

  ```nix
  services.udev.extraRules = ''
    # Qualcomm EDL (9008) and Lenovo devices, for the logged-in user
    SUBSYSTEM=="usb", ATTRS{idVendor}=="05c6", ATTRS{idProduct}=="9008", TAG+="uaccess"
    SUBSYSTEM=="usb", ATTRS{idVendor}=="17ef", TAG+="uaccess"
  '';
  ```

  or `services.udev.packages = [ pkgs.edl ];` (nixpkgs' `edl` ships `51-edl.rules` for 9008).
- adb: add `pkgs.android-tools` to the system packages. The old `programs.adb.enable` option and the `adbusers`
  group were removed from nixpkgs (25.11) because systemd's own rule now covers adb.
- `pkgs.edl` is the `edl` tool for the recovery case above.
- The installer expects Debian-style tools (`debootstrap`, the Ubuntu keyring) and qemu registered with binfmt
  (`boot.binfmt.emulatedSystems = [ "aarch64-linux" ];` registers it, but check for the `F` flag the script asks
  for). Running it inside an Ubuntu container or VM avoids both questions.
