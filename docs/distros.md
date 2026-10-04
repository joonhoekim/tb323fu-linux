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
| SteamOS (arm64, community handheld port⁵) | `tb323fu-spare` | `rootfs/steamos` | 14.1 s | Gaming Mode (landscape, seen on the panel; touch lands where tapped after a calibration matrix), Wi-Fi, speakers (protection filter), helper; Desktop Mode (KDE) starts and switches back |

¹ `systemd-analyze` "Startup finished" (kernel + userspace), with automatic login.\
² graphical.target; "Startup finished" is 15 s while first-boot timer jobs (plocate, fstrim) still run. iSCSI and plymouth are masked (see the builder).\
³ Needs the netfilter set in [kernel/config/baldur-netfilter.fragment](../kernel/config/).\
⁴ Steam's own client, run with `FEXBash -c "~/steam-launcher/steam -no-cef-sandbox"`; games not tried yet.\
⁵ [SteamOS ARM for handhelds](https://github.com/hashtagbasit/SteamOS-ARM-Handhelds) v1.3-odin3-beta1; unofficial, not affiliated with Valve. See [SteamOS notes](#steamos-notes).

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
| NixOS | `rootfs/nixos/build-rootfs.sh` (a flake: `flake.nix`, `packaging/nix/`) | an arm64 host with Nix | Nix packages from this repository (`services.tb323fu`) |
| SteamOS (community port) | `rootfs/steamos/build-rootfs.sh` | **arm64 only** (it compiles and copies host files) | `userspace/platform/install.sh`, the helper copied from a built tree (`HELPER_FROM=`); no settings app |

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
- **NixOS:** no initrd and no bootloader (our initramfs starts stage 2); a kernel stub so the boot image's modules
  are used; no getty on tty1 next to GDM's automatic login; firmware uncompressed (the DSP loader asks for the file
  by name).
- **SteamOS:** only its root and home partitions are used; its kernel modules, A/B and offload mounts,
  `systemd-repart` and device services for other handhelds masked; module lists trimmed to this kernel; the LADSPA
  plugins PipeWire needs copied in; Gaming Mode's rotation matched with a touch calibration; a power-button service
  that does not wait for SteamVR.

### Known problems

- **The USB gadget sometimes does not attach after boot** (seen once each on Arch and NixOS): no USB network or
  serial shell. `echo connect > /sys/class/udc/*/soft_connect` as root, or unplug and replug the cable.
- **NixOS:** after a rebuild `/` was once left with mode 777, and sshd then refuses every key: `chmod 755 /`.
- **Fedora:** firewalld is turned off unless the build was given the kernel's modules (`MODULES_FROM=`): the check for
  `nft_compat` cannot see the boot image's modules at build time.
- **Fedora, NixOS, SteamOS:** whether GNOME/KDE asks for the Wi-Fi password on first use was not checked after the
  Ubuntu fix above (Fedora and NixOS install `gnome-keyring` with their GNOME); each was set up with a saved
  connection (`NM_CONNECTIONS_FROM=`).
- No SELinux in the kernel (Fedora runs with the config set to permissive).
- Fedora: x86 binaries go through FEX only if `qemu-user-static-x86` is not installed (its binfmt entry wins; the
  builder excludes it).

## Not tried yet

Nothing here has been booted on the tablet. What is known:

| System | Notes |
|---|---|
| Debian 13 | the development root is Debian, but it was set up by hand before the builders existed; `rootfs/ubuntu` with a Debian mirror and release is the closest starting point |
| [Armada](https://github.com/armada-os/armada) | a gaming distribution for ARM handhelds (Fedora bootc, Steam, FEX, KDE), shipped as one disk image (`armada-YYYYMMDD.img.gz`); its device list goes up to SM8750, not this SoC. Its root is an ostree deployment: our initramfs does not start ostree, so it would need flattening into a plain root (losing Armada's own updates) or ostree support in the initramfs |
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
