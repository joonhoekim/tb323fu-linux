# Distributions

The kernel does not depend on a distribution, and **this page is not a list of what is allowed**: it records what has
been tried. Any arm64 Linux distribution can run on the tablet if its root filesystem has what
[every root needs](#what-every-root-needs); the systems below are the ones that have been built with the scripts in
[`rootfs/`](../rootfs/) and booted on a TB323FU, each from its own partition through the initramfs's root selection
([kernel/initramfs/README.md](../kernel/initramfs/README.md#root-partitions-and-multiboot), `tb323fu-ctl boot …`,
[helper.md](helper.md#multiboot)). "Checked" means seen on the device, not "should work". Several can live side by
side on the card.

| System | Partition (example) | Builder | Boot to desktop¹ | Checked on the device |
|---|---|---|---|---|
| Debian 13 (trixie) | `baldur-root` (UFS) | the original development image | 5.8 s | everything in [hardware-status.md](hardware-status.md) |
| Ubuntu 26.04 LTS | `tb323fu-ubuntu` | `rootfs/ubuntu` | 8.3 s | GNOME 50 autologin, touch, Wi-Fi, speakers (protection filter, both amplifiers), helper + GNOME extension |
| Arch Linux ARM | `tb323fu-arch` | `rootfs/arch` | 7.5 s | autologin, touch, Wi-Fi, speakers, sensors (4), helper; through the guided installer (2026-10-04, built in WSL2): GNOME, speakers, Wi-Fi password prompt, helper, both ways to Android and back, no sensors |
| Fedora 44 Workstation + FEX | `tb323fu-fedora` | `rootfs/fedora` | 7.5 s² | GNOME 50 autologin, Wi-Fi, speakers, sensors (4), firewalld³, FEX x86-64, Steam client starts⁴ |
| NixOS 26.11 (unstable) | `tb323fu-nixos` | `rootfs/nixos` + `flake.nix` | 11.2 s | stage 2 straight from our initramfs, GNOME autologin, Wi-Fi, speakers, sensors, helper |
| SteamOS (arm64, community handheld port⁵) | `tb323fu-spare` | `rootfs/steamos` | 14.1 s | Gaming Mode (landscape, seen on the panel; touch lands where tapped after a calibration matrix), Wi-Fi, speakers (protection filter), helper; with the old port release v1.3-odin3-beta1 also Desktop Mode (KDE) both ways. Current release v1.3-8elite-beta2 through the guided installer on kernel t39: Gaming Mode, touch, speakers, both ways to Android and back; Switch to Desktop does not work ([known problems](#known-problems)) |
| Armada 20260926 (Fedora bootc, experimental⁶) | `baldur-root-sd` | `rootfs/armada` | – | not booted yet: built in WSL2 and checked offline only |

¹ `systemd-analyze` "Startup finished" (kernel + userspace), with automatic login.\
² graphical.target; "Startup finished" is 15 s while first-boot timer jobs (plocate, fstrim) still run. iSCSI and plymouth are masked (see the builder).\
³ Needs the netfilter set in [kernel/config/baldur-netfilter.fragment](../kernel/config/).\
⁴ Steam's own client, run with `FEXBash -c "~/steam-launcher/steam -no-cef-sandbox"`; games not tried yet.\
⁵ [SteamOS ARM for handhelds](https://github.com/hashtagbasit/SteamOS-ARM-Handhelds) v1.3-odin3-beta1 (since withdrawn; the builder now pins v1.3-8elite-beta2); unofficial, not affiliated with Valve. See [SteamOS notes](#steamos-notes).\
⁶ [Armada](https://github.com/armada-os/armada), a gaming distribution for ARM handhelds; its device list ends at SM8750, so this SoC is not one of its targets. See [Armada notes](#armada-notes).

## Installing each one

The guided installer ([Installing Linux](install.md)) builds one system on a PC and writes it to the card; which one
is a [distribution module](../tools/install/distros/README.md). The other builders run on an arm64 Linux machine
(normally the tablet itself, running a first Linux root) into a mounted, empty partition, as in
[Installing by hand, step 5](install-manual.md#5-put-a-root-filesystem-on-it). Each builder lists its options at the
top of the file.

| System | How | Built where | Platform files and helper come from |
|---|---|---|---|
| Ubuntu 26.04 | `tools/install/install.sh` (default), or `rootfs/ubuntu/build-rootfs.sh` | any Linux; x86-64 through qemu (WSL2 works) | the `.deb` files of a `helper-v*` release (`DEBS_FROM=`) |
| Arch Linux ARM | `DISTRO=arch tools/install/install.sh`, or `rootfs/arch/build-rootfs.sh` | any Linux; x86-64 through qemu | the release's `.deb` files, unpacked (`DEBS_FROM=`), or your own packages from `packaging/arch/PKGBUILD` (`PKGS_FROM=`) |
| Fedora 44 | `rootfs/fedora/build-rootfs.sh` | arm64 (not tried through qemu; it builds the helper with cargo inside the root, which would take hours there) | built from this repository inside the root (needs network) |
| NixOS | `DISTRO=nixos tools/install/install.sh`, or `rootfs/nixos/build-rootfs.sh` (a flake: `flake.nix`, `packaging/nix/`) | any Linux with Nix; x86-64 through qemu (`extra-platforms = aarch64-linux`), where the helper is compiled slowly | Nix packages from this repository (`services.tb323fu`) |
| SteamOS (community port) | `DISTRO=steamos tools/install/install.sh` (experimental), or `rootfs/steamos/build-rootfs.sh` | any Linux; off arm64 it needs the kernel modules, LADSPA plugins and platform packages as arm64 files (the module fetches them: the kernel release's modules bundle, Ubuntu 24.04's `swh-plugins`, the helper release's `.deb` files) | the release's `.deb` files (`DEBS_FROM=`), or `userspace/platform/install.sh` and `HELPER_FROM=` on arm64; no settings app. Needs kernel t39 or later (tracefs): on t38 the builder leaves the SteamOS manager off, Gaming Mode works, Switch to Desktop does not |
| Armada | `DISTRO=armada tools/install/install.sh` (experimental), or `rootfs/armada/build-rootfs.sh` | any Linux whose kernel mounts btrfs (the image's root); off arm64 it needs the same arm64 inputs as SteamOS (the module fetches them) | the release's `.deb` files (`DEBS_FROM=`); no settings app |

### What the builders take care of

Everything a distribution needed on this tablet so far is done by its builder; nothing has to be done by hand after
the build. Besides the list in [What every root needs](#what-every-root-needs):

- **Ubuntu:** snapd pinned away; `droid-juicer` and `qbootctl` masked; `gnome-keyring` (without it the Wi-Fi password
  prompt never appears); the GDM hook that ends the boot splash after automatic login; `ssh` enabled for developer
  access (Ubuntu leaves it off).
- **Arch Linux ARM:** the distribution kernel and mkinitcpio removed; the platform files and helper unpacked from the
  release's `.deb` files (the same files as on Ubuntu); pacman's download sandbox off while building
  through qemu (no Landlock there); the image's default user removed; `bootmac-bluetooth` masked; the platform units
  and the helper enabled. Sensors need `HEXAGONRPCD_FROM=` (a root with `hexagonrpcd`, which Arch does not package);
  without it the root has no rotation sensor.
- **Fedora:** kernel, bootloader, `linux-firmware` and Qualcomm firmware packages excluded from dnf (they would shadow
  the device firmware); `snd-pcm` loaded without `snd-seq` (the distribution's ALSA config otherwise leaves no
  sound card); iSCSI and plymouth masked (22.9 s → 7.5 s to the desktop); SELinux permissive (not in this kernel);
  `hexagonrpcd` and an `iio-sensor-proxy` with the SSC backend from another root (`*_FROM=`); FEX set up to read its
  x86-64 root through erofsfuse (no EROFS in the kernel).
- **NixOS:** no initrd and no bootloader (our initramfs starts stage 2); the flake copied to `/etc/nixos` with a
  `configuration.nix` of your own; a kernel stub so the boot image's modules
  are used; no getty on tty1 next to GDM's automatic login; firmware uncompressed (the DSP loader asks for the file
  by name).
- **SteamOS:** only its root and home partitions are used; its kernel modules, A/B and offload mounts,
  `systemd-repart` and device services for other handhelds masked; module lists trimmed to this kernel; the LADSPA
  plugins PipeWire needs copied in; Gaming Mode's rotation matched with a touch calibration; a power-button service
  that does not wait for SteamVR.
- **Armada:** the OSTree deployment and its `/var` copied out of the image's btrfs as a plain root; the mounts bootc
  wrote for the image's disk removed; OSTree, bootc and rpm-ostree units, `bootc-generic-growpart`, Armada's ABL and
  boot-image plumbing, its installer (it writes to the internal storage) and its MTP gadget (it would take the USB
  controller from the initramfs' gadget) masked; a device profile for Armada's `device-env` and the Gaming Mode
  orientation; LADSPA plugins; SELinux permissive; developer access through a NetworkManager profile (Armada has no
  systemd-networkd).

### NixOS: your own configuration

The NixOS root carries the flake it was built from in `/etc/nixos`. Only `configuration.nix` there is yours; it
starts with NixOS's defaults (a few examples, commented out). `local.nix` (this tablet's partition, user, Android
hash) and `tb323fu-linux/` (a copy of this repository: kernel stub, firmware, services, the desktop) are the
tablet's part. After a change, on the tablet:

```sh
sudo nixos-rebuild switch --flake /etc/nixos#tb323fu-nixos
```

The first rebuild downloads nixpkgs (needs the network). The rebuilt system is what the next start boots.

**Your own copy, e.g. `~/nixos-config`** (checked on the tablet: a package added, rebuilt in 57 s, still there after a
restart):

```sh
sudo cp -r /etc/nixos ~/nixos-config && sudo chown -R $USER: ~/nixos-config   # some firmware files are root-only
nano ~/nixos-config/configuration.nix
sudo nixos-rebuild switch --flake ~/nixos-config#tb323fu-nixos
```

- **Under git:** a flake in a git repository sees only the files git tracks — `git add` the whole directory before
  rebuilding, or Nix reports `local.nix` or the firmware as missing. Alternatively build with
  `--flake path:$HOME/nixos-config#tb323fu-nixos`, which reads the directory as it is.
- **Do not publish `firmware/`**: it holds Lenovo's and Qualcomm's files from your tablet. Keep such a repository
  private, or keep `firmware/` out of it (in `.gitignore`) and build with the `path:` form above.
- **A newer version of this repository:** replace `tb323fu-linux/` with a newer copy, or point
  `inputs.tb323fu-linux.url` in `flake.nix` at `github:joonhoekim/tb323fu-linux`, then `nix flake update` in the
  directory and rebuild.

### Known problems

- **The USB gadget sometimes does not attach after boot** (seen once each on Arch and NixOS): no USB network or
  serial shell. `echo connect > /sys/class/udc/*/soft_connect` as root, or unplug and replug the cable.
- **NixOS:** after a rebuild `/` was once left with mode 777, and sshd then refuses every key: `chmod 755 /`.
- **Fedora:** firewalld is turned off unless the build was given the kernel's modules (`MODULES_FROM=`): the check for
  `nft_compat` cannot see the boot image's modules at build time.
- **Fedora, NixOS, SteamOS:** whether GNOME/KDE asks for the Wi-Fi password on first use was not checked after the
  Ubuntu fix above (Fedora and NixOS install `gnome-keyring` with their GNOME); each was set up with a saved
  connection (`NM_CONNECTIONS_FROM=`).
- **SteamOS (port release v1.3-8elite-beta2):** Steam's power menu has no Switch to Desktop, and
  `steamosctl switch-to-desktop-mode` brings Gaming Mode back: SDDM logs in again without taking the temporary
  Plasma session the manager writes. The older release (v1.3-odin3-beta1, withdrawn) switched both ways on the
  development tablet.
- No SELinux in the kernel (Fedora runs with the config set to permissive).
- Fedora: x86 binaries go through FEX only if `qemu-user-static-x86` is not installed (its binfmt entry wins; the
  builder excludes it).

## Not tried yet

Not built with the scripts here yet. What is known:

| System | Notes |
|---|---|
| Debian 13 through a builder | the development root is Debian (first row above), but it was set up by hand before the builders existed; `rootfs/ubuntu` with a Debian mirror and release is the closest starting point |
| postmarketOS | aimed at phones and tablets, with its own boot chain; its root would need the same changes as the others |

## Your own distribution

Make the root meet [What every root needs](#what-every-root-needs), name its partition `tb323fu-<something>` (or
`baldur-root-sd` / `baldur-root`), and pick it with `tb323fu-ctl boot`. To use the guided installer for it, write a
[distribution module](../tools/install/distros/README.md) — a short shell file that runs your builder — and start
the installer with `DISTRO=path/to/your-module.sh`.

## What every root needs

The builders in [`rootfs/`](../rootfs/) do all of it.

- **No kernel modules of its own.** The boot image carries all modules of its kernel and the initramfs mounts them
  read-only on `/lib/modules/<release>` of the root it starts (merged `/usr` roots: `/usr/lib/modules/<release>`),
  so a kernel update is just the new boot image ([kernel/initramfs](../kernel/initramfs/README.md#kernel-modules-shared)).
  NixOS holds a kernel stub (`packaging/nix/prebuilt-kernel.nix`) and its kmod falls through to that mount. The root
  only needs a writable `/` for the mount point. `own` in `/etc/tb323fu/modules` opts a root out (it then carries its
  own tree, e.g. for DKMS). No distribution kernel, bootloader or initramfs is used: one installed by a package gets
  its own release directory and is ignored.
- **Firmware** from your own tablet (`firmware/`): `qcom`, `ath12k`, `qca`, `novatek` (touch — without it the
  touchscreen is dead), `aw882xx_acf.bin`, and `regulatory.db` (`wireless-regdb`).
- **wpa_supplicant** next to NetworkManager (without it Wi-Fi shows as unavailable).
- **LADSPA swh plugins** (`swh-plugins` / `ladspa-swh-plugins` / `ladspaPlugins`): the speaker protection filter
  uses `sc4_1882`; without it PipeWire does not start.
- **rmtfs and tqftpserv masked.** `rmtfs -s` starts the modem, which then crashes and currently resets the whole SoC.
  The tablet has no GNSS antenna and no cellular, so the modem has no use.
- **Mobile-distribution helpers masked** when present: `droid-juicer` (holds the boot forever), `qbootctl`,
  `bootmac-bluetooth`, ModemManager.
- **Sensors:** `hexagonrpcd` with a `fastrpc` system user and its udev rule, and an `iio-sensor-proxy` built with
  the SSC backend (libssc) — Fedora's has none, so the builder copies one.
- **Platform files and helper** (`userspace/platform`, `helper/`, or the packages in `packaging/`).
- **A distribution's own disk plumbing masked** when it comes from an image: SteamOS's `systemd-repart` (its `repart.d` wants to add
  a home partition to the root disk), partition-set and offload mounts, its A/B boot registration.

## SteamOS notes

<details>
<summary>How the SteamOS root is put together</summary>

- The image is the SM8750 build of SteamOS ARM for handhelds, with Valve's Steam Frame userspace and the native
  arm64 Steam client (Valve ships a native arm64 Steam client for Linux since May 2026).
- Its root and home are copied into one partition; the device's own kernel boots it.
- Gaming Mode turns the picture with gamescope's rotation shader, which does not turn touch: the builder adds a
  libinput calibration matrix for the touchscreen and pen.
- The power button suspends once `steamos-powerbuttond` runs. The Steam Frame unit only starts next to SteamVR, so
  the builder replaces it.
- Its own disk plumbing is masked (see [What every root needs](#what-every-root-needs)).

</details>

## Armada notes

<details>
<summary>How the Armada root is put together</summary>

- The release image is a raw disk: ESP (ROCKNIX ABL boot image), `/boot` (ext4, OSTree's boot entries) and a btrfs
  root whose subvolume `root` holds an OSTree repository and one bootc deployment (composefs enabled). Our initramfs
  does not start OSTree, so the builder copies the deployment's tree (with its merged `/etc`) and the stateroot's
  `/var` (the `armada` user's home with the Steam bootstrap) into one ext4 partition. Nothing of the ESP or `/boot` is
  used.
- `/home`, `/root`, `/opt` and so on stay symlinks into `/var`, as on Fedora's atomic systems.
- Armada's updates (bootc, Steam's system update button) do not work on the copy; a newer release means building the
  root again.
- The user is `armada` (the image's password is `armada`; the guided installer sets your own). SDDM logs it
  into Gaming Mode; Desktop Mode is KDE Plasma.
- The panel has no orientation property: a profile in Armada's `device-env` (by the device tree's model) turns on
  gamescope's rotation shader, and `/etc/gamescope-session-plus/sessions.d/steam` gives Gaming Mode its orientation
  (`ORIENTATION=`, default `right`, as on SteamOS).
- Not checked yet: that it boots at all, Gaming Mode's picture and whether touch follows the rotation (SteamOS needed
  a calibration matrix), Desktop Mode, sound, suspend through Armada's own `suspend-dispatch`, Armada's power daemon
  next to the helper (both set CPU and GPU limits), Steam's first start. No sensors (Fedora's `iio-sensor-proxy` has
  no SSC backend).

</details>
