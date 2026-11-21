# Installing from Linux

The same path as on Windows ([Installing Linux](install.md)): root the tablet, then one script on the PC takes it to
Ubuntu with GNOME on the microSD card. On Linux the tools are installed differently and the script uses your own `adb`.

> **Status: verified up to the first start** ([labels](hardware-status.md#evidence-labels)) from NixOS unstable (x86-64), with the script in an Ubuntu 26.04 container ([NixOS](#nixos)).[^tested]
> A Debian, Ubuntu, Arch or Fedora PC, rooting with LTBox and the `edl` recovery tool are **untested** from Linux; those steps follow their own documentation.
> If something does not match, stop and open an issue.

## 1. Tools for rooting

Rooting follows [Rooting](rooting.md); its LTBox screens should be the same on every PC. On Linux:

1. Install [LTBox](https://github.com/miner7222/LTBox): on Debian/Ubuntu from LTBox's APT repository or the `.deb`, on
   Fedora from its DNF repository or the `.rpm`, on Arch from the AUR (`ltbox-bin`, a community package), or unpack
   the tarball. The repositories and their keys are in [LTBox's documentation](https://miner7222.github.io/ltbox/en/index.html).
2. USB access without root: with the tarball run `sudo ./ltbox --install-udev` once (or the **Install udev rules**
   button in LTBox), then **unplug and replug** the tablet. The packages are expected to ship the rule; check.[^udev]
3. Run LTBox as your normal user (not with `sudo`). It needs libusb-1.0, libudev, xkbcommon, Wayland or X11 libraries
   and fontconfig, usually already present on a desktop.
4. **ModemManager** can disturb the EDL session by probing the 9008 serial device. LTBox's udev rule keeps it away;
   without that rule, or for the `edl` tool, stop it for the session (`sudo systemctl stop ModemManager`).
5. While an `adb` server runs (the one used later), LTBox shows "An external ADB server is preventing device access"
   with a **Kill Server** button; use it, or `adb kill-server`. Afterwards `adb devices` starts the server again.
6. The firmware package: Lenovo's Software Fix runs only on Windows, see
   [Getting the firmware package without Windows](rooting.md#getting-the-firmware-package-without-windows).

## 2. adb

Your distribution's `android-tools` (Arch, Fedora) or `adb` (Debian, Ubuntu) package. With systemd 258 or newer adb
access works through systemd's built-in rule; on older systems install `android-udev-rules` (or your distribution's
equivalent). Then [prepare the tablet](install.md#prepare-the-tablet).

## 3. Run the installer

```sh
git clone https://github.com/joonhoekim/tb323fu-linux.git
cd tb323fu-linux
tools/install/install.sh
```

The steps, what they ask and how long they take: [What the script asks](install.md#what-the-script-asks). Differences on Linux:

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

The install script does not run on NixOS itself.[^nixos-fails] Run it in an **Ubuntu container** (**verified** with Docker; Podman is
**untested**). The container sees the tablet through the PC's own `adb` server, as the Mac's VM does in
[Installing from macOS](install-macos.md#3-run-the-installer); no USB passthrough.

1. On the PC: `pkgs.android-tools` in the system packages and `virtualisation.docker.enable = true;`. [Prepare the
   tablet](install.md#prepare-the-tablet); `adb devices` on the PC then also starts the adb server the container uses.
2. Start the container, privileged (loop-mounted image, binfmt) and on the host network (the adb server on
   `localhost:5037`). The build needs about 30 GB in Docker's storage.

   ```sh
   docker run -it --name tb323fu --privileged --network host -v /dev:/dev ubuntu:26.04 bash
   ```
3. In the container (you are root there; there is no `sudo`):

   ```sh
   apt-get update && apt-get install -y git adb
   adb devices            # the tablet, through the PC's server
   git clone https://github.com/joonhoekim/tb323fu-linux.git
   cd tb323fu-linux
   tools/install/install.sh
   ```

   The container's `adb` and the PC's must speak the same server version ([why](install.md#prepare-the-tablet)).
4. Differences from an Ubuntu PC, in the order the script meets them:
   - `host`: installing the packages asks for a **time zone** (`tzdata`); it becomes the tablet's time zone.
   - `host`: "arm64 programs are not registered". Answer `y`: the script mounts `binfmt_misc`, then fails to restart
     `systemd-binfmt` (there is no systemd in the container). Register qemu by hand and run the script again:

     ```sh
     /usr/lib/systemd/systemd-binfmt /usr/lib/binfmt.d/qemu-aarch64.conf
     cat /proc/sys/fs/binfmt_misc/qemu-aarch64      # enabled, flags: POF
     tools/install/install.sh
     ```

     The registration is the PC kernel's and lasts until the PC restarts. Nothing changes in the NixOS configuration.
   - `rootfs`: the user name suggested is `root`, which the script refuses; type your own.
   - The work directory is `/root/tb323fu-install` in the container. `docker start -ai tb323fu` takes you back in
     after an exit; `docker rm tb323fu` frees the space once the tablet runs Linux.
5. `rootfs` builds through qemu and takes the longest, as on Windows ([what the script asks](install.md#what-the-script-asks)).

Rooting and recovery tools on NixOS:

- **LTBox** has no nixpkgs package and no AppImage. The Linux tarball is a dynamically linked binary that also loads
  Wayland or X11, Vulkan or OpenGL and libusb at run time. With [`nix-ld`](https://github.com/nix-community/nix-ld)
  alone it stops with `NoWaylandLib`; it started (GUI on Wayland, Vulkan) with these libraries:

  ```nix
  programs.nix-ld.enable = true;
  programs.nix-ld.libraries = with pkgs; [
    wayland libxkbcommon libusb1 fontconfig freetype libGL vulkan-loader systemdLibs
    xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr
  ];
  ```

  `steam-run ./ltbox` is the other way (**untested**).
- **udev rule.** `ltbox --install-udev` writes into `/etc/udev/rules.d`, which NixOS does not manage. Put the rule into
  the configuration under LTBox's file name (with `services.udev.extraRules` it ends up in `99-local.rules` and LTBox
  keeps warning):

  ```nix
  services.udev.packages = [
    (pkgs.writeTextDir "lib/udev/rules.d/51-ltbox-qcom.rules" ''
      SUBSYSTEM=="usb", ATTR{idVendor}=="05c6", ATTR{idProduct}=="9008", ENV{ID_MM_DEVICE_IGNORE}="1", TAG+="uaccess"
      SUBSYSTEM=="tty", ATTRS{idVendor}=="05c6", ATTRS{idProduct}=="9008", ENV{ID_MM_DEVICE_IGNORE}="1", TAG+="uaccess"
      SUBSYSTEM=="usb", ATTR{idVendor}=="18d1", TAG+="uaccess"
      SUBSYSTEM=="usb", ATTR{idVendor}=="17ef", TAG+="uaccess"
    '')
  ];
  ```

  These are LTBox 3.3.3's rules without the `plugdev` group, which NixOS does not have. LTBox may still call the rule
  "stale" because it differs from its own text. Switching to such a configuration and LTBox's reaction are **untested**.
- **adb:** `pkgs.android-tools` in the system packages. The `programs.adb.enable` option and the `adbusers` group were
  removed from nixpkgs (25.11) because systemd's own rule covers adb.
- **edl:** `pkgs.edl` is the tool for the recovery case above. nixpkgs marks it unfree: allow it
  (`nixpkgs.config.allowUnfreePredicate`, or `NIXPKGS_ALLOW_UNFREE=1 nix shell --impure nixpkgs#edl`). It ships
  `51-edl.rules` (9008 with `uaccess`), so `services.udev.packages = [ pkgs.edl ];` also gives USB access.

[^tested]: Run on 2026-10-04 from NixOS unstable (x86-64).
[^udev]: The rule is written to `/etc/udev/rules.d/51-ltbox-qcom.rules` and lets the desktop session open the Qualcomm 9008, Google and Lenovo USB devices. LTBox shows "USB device access not set up" as long as no file of that name exists.
[^nixos-fails]: Checked 2026-10-04 on NixOS unstable with nixpkgs' `debootstrap` 1.0.140. nixpkgs has no Ubuntu archive keyring for `/usr/share/keyrings/ubuntu-archive-keyring.gpg`. Its `debootstrap` has no script for Ubuntu 26.04 (`resolute`), and the builder's fallback writes into `/usr/share/debootstrap/scripts`, which NixOS lacks. `boot.binfmt.emulatedSystems = [ "aarch64-linux" ]` registers a dynamically linked qemu as `aarch64-linux` without the `F` flag, so the builder's chroot cannot start arm64 programs and the script's `qemu-aarch64` check fails. `boot.binfmt.preferStaticEmulators = true;` and `boot.binfmt.registrations.aarch64-linux.fixBinary = true;` fix the flag and the qemu, not the name.
