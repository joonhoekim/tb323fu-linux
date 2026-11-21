# Distributions

The kernel does not depend on a distribution, and **this page is not a list of what is allowed**: it records what has
been tried. Any arm64 Linux distribution can run on the tablet if its root has what
[every root needs](#what-every-root-needs). The systems below were built with the scripts in [`rootfs/`](../rootfs/)
and booted on a TB323FU, each from its own partition through the initramfs's root selection
([kernel/initramfs/README.md](../kernel/initramfs/README.md#root-partitions-and-multiboot), `tb323fu-ctl boot …`,
[helper.md](helper.md#multiboot)). Several can live side by side on the card.

Everything in the "Works" column is **verified** on a tablet ([labels](hardware-status.md#evidence-labels)):
seen or heard, or measured.

| System | Partition (example) | Builder | Boot to desktop[^boot-time] | Works | Not working |
|---|---|---|---|---|---|
| Debian 13 (trixie) | `baldur-root` (UFS) | the original development image | 5.8 s | everything in [hardware-status.md](hardware-status.md) | – |
| Debian 13 (trixie) | `tb323fu-ufs` (UFS) | `rootfs/debian` | 4.5 s | GNOME 48 autologin, Wi-Fi password prompt, speakers, touch, pen, sensors, cameras, Bluetooth, suspend, to Android and back[^ufs-installer] | – |
| Ubuntu 26.04 LTS | `tb323fu-ubuntu` | `rootfs/ubuntu` | 8.3 s | GNOME 50 autologin, touch, Wi-Fi, speakers, helper and GNOME extension, sensors (4), both cameras, suspend, to Android and back[^ubuntu-sensors] | – |
| Arch Linux ARM | `tb323fu-arch` | `rootfs/arch` | 7.5 s | autologin, touch, Wi-Fi, speakers, helper, both cameras, suspend, to Android and back | sensors through the installer (needs `HEXAGONRPCD_FROM=`) |
| Fedora 44 Workstation + FEX | `tb323fu-fedora` | `rootfs/fedora` | 7.5 s[^fedora-time] | GNOME 50 autologin, Wi-Fi, speakers, firewalld[^netfilter], FEX x86-64, Steam client starts[^steam-client], suspend, to Android and back | sensors through the installer (needs `HEXAGONRPCD_FROM=`) |
| NixOS unstable (2026-10-01) | `tb323fu-nixos` | `rootfs/nixos` + `flake.nix` | 11.2 s | stage 2 straight from our initramfs, GNOME autologin, Wi-Fi, speakers, helper, suspend, to Android and back | sensors through the installer |
| SteamOS (arm64, community handheld port[^steamos-port]) | `tb323fu-spare` | `rootfs/steamos` | 14.1 s | Gaming Mode (landscape; touch lands where tapped after a calibration matrix), Wi-Fi, speakers, helper, suspend, to Android and back | Switch to Desktop ([known problems](#known-problems)); sensors and cameras (the image has no libcamera) |
| Armada 20260926 (Fedora bootc, experimental[^armada-port]) | `baldur-root-sd` | `rootfs/armada` | – | boots (a long first-start splash), Gaming Mode with touch after a calibration matrix, Steam's first-start setup, suspend, to Android and back; x86 runtime for FEX mounts on kernel t40 ([Armada notes](#armada-notes)) | cameras until `libcamera-ipa pipewire-plugin-libcamera` are installed |
| postmarketOS v26.06 (Nura), Phosh, systemd | `tb323fu-pmos` | `rootfs/postmarketos` | –[^pmos-time] | Phosh autologin, touch and rotation, Wi-Fi, speakers, microphone, both cameras, sensors (accelerometer, light, proximity), torch, LED ring, helper, suspend, to Android and back; through the guided installer end to end (43 min from WSL2, `tb323fu-check` passes)[^pmos-installer] | compass |
| postmarketOS v26.06 (Nura), Plasma Mobile, systemd (experimental) | `tb323fu-pmos-plasma` | `rootfs/postmarketos` (`DESKTOP=plasma-mobile`) | – | Plasma Mobile autologin, touch and rotation, Wi-Fi, speakers, microphone, both cameras, suspend, to Android and back | not yet through the installer; Plasma Camera's preview does not follow the rotation |
| postmarketOS v26.06 (Nura), GNOME Mobile, systemd (experimental) | `tb323fu-pmos-gnome-mobile` | `rootfs/postmarketos` (`DESKTOP=gnome-mobile`) | – | GDM autologin, touch, rotation (through `tb323fu-autorotate`; the shell does not turn the panel by itself), Wi-Fi, speakers, microphone, both cameras, suspend, to Android and back | not yet through the installer |

## Installing each one

The guided installer ([Installing Linux](install.md)) builds one system on a PC and writes it to the card; which one
is a [distribution module](../tools/install/distros/README.md). The other builders run on an arm64 Linux machine
(normally the tablet itself, running a first Linux root) into a mounted, empty partition, as in
[Installing by hand, step 5](install-manual.md#5-put-a-root-filesystem-on-it). Each builder lists its options at the
top of the file.

| System | How | Built where | Platform files and helper come from |
|---|---|---|---|
| Ubuntu 26.04 | `tools/install/install.sh` (default), or `rootfs/ubuntu/build-rootfs.sh` | any Linux; x86-64 through qemu (WSL2 works) | the `.deb` files of a `helper-v*` release (`DEBS_FROM=`) |
| Debian 13 | `DISTRO=debian tools/install/install.sh`, or `rootfs/debian/build-rootfs.sh` (the Ubuntu builder with `OS=debian`) | any Linux with `debian-archive-keyring`; x86-64 through qemu | the `.deb` files of a `helper-v*` release (`DEBS_FROM=`) |
| Arch Linux ARM | `DISTRO=arch tools/install/install.sh`, or `rootfs/arch/build-rootfs.sh` | any Linux; x86-64 through qemu | the release's `.deb` files, unpacked (`DEBS_FROM=`), or your own packages from `packaging/arch/PKGBUILD` (`PKGS_FROM=`) |
| Fedora 44 | `DISTRO=fedora tools/install/install.sh`, or `rootfs/fedora/build-rootfs.sh` | any Linux; x86-64 through qemu (WSL2 works, about 25 min with GNOME) | built from this repository inside the root (needs network) |
| NixOS | `DISTRO=nixos tools/install/install.sh`, or `rootfs/nixos/build-rootfs.sh` (a flake: `flake.nix`, `packaging/nix/`) | any Linux with Nix; x86-64 through qemu (`extra-platforms = aarch64-linux`), where the helper is compiled slowly | Nix packages from this repository (`services.tb323fu`) |
| SteamOS (community port) | `DISTRO=steamos tools/install/install.sh` (experimental), or `rootfs/steamos/build-rootfs.sh` | any Linux; off arm64 it needs the kernel modules, LADSPA plugins and platform packages as arm64 files (the module fetches them: the kernel release's modules bundle, Ubuntu 24.04's `swh-plugins`, the helper release's `.deb` files) | the release's `.deb` files (`DEBS_FROM=`), or `userspace/platform/install.sh` and `HELPER_FROM=` on arm64; no settings app. Needs kernel t39 or later (tracefs): on t38 the builder leaves the SteamOS manager off, Gaming Mode works, Switch to Desktop does not |
| postmarketOS | `DISTRO=postmarketos tools/install/install.sh`, or `rootfs/postmarketos/build-rootfs.sh` | any Linux with `openssl`; x86-64 through qemu | built from this repository inside the root (needs network) |
| Armada | `DISTRO=armada tools/install/install.sh` (experimental), or `rootfs/armada/build-rootfs.sh` | any Linux whose kernel mounts btrfs (the image's root); off arm64 it needs the same arm64 inputs as SteamOS (the module fetches them) | the release's `.deb` files (`DEBS_FROM=`); no settings app |

### What the builders take care of

Nothing has to be done by hand after a build. The list below is for reference, besides
[What every root needs](#what-every-root-needs).

<details>
<summary>What each builder does for its distribution</summary>

- **Ubuntu:** snapd pinned away; `droid-juicer` and `qbootctl` masked; libssc and an iio-sensor-proxy with the SSC
  backend built from pinned sources (a few minutes); `gnome-keyring` (without it the Wi-Fi password
  prompt never appears); the GDM hook that ends the boot splash after automatic login; `ssh` enabled for developer
  access (Ubuntu leaves it off).
- **Arch Linux ARM:** the distribution kernel and mkinitcpio removed; the platform files and helper unpacked from the
  release's `.deb` files (the same files as on Ubuntu); pacman's download sandbox off while building
  through qemu (no Landlock there); the image's default user removed; `bootmac-bluetooth` masked; the platform units
  and the helper enabled. Sensors need `HEXAGONRPCD_FROM=` (a root with `hexagonrpcd`, which Arch does not package);
  without it the root has no rotation sensor.
- **Fedora:** kernel, bootloader, `linux-firmware` and Qualcomm firmware packages excluded from dnf (they would shadow
  the device firmware); `snd-pcm` loaded without `snd-seq` (the distribution's ALSA config otherwise leaves no
  sound card); iSCSI and plymouth masked; SELinux permissive (not in this kernel);
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
- **postmarketOS:** bootstrapped with `apk.static` (signature checked against `rootfs/postmarketos/keys`) from the
  postmarketOS and Alpine repositories, without a device package or kernel; `/usr` merged as pmbootstrap does; the
  swh LADSPA plugins built from a pinned source (not packaged for Alpine); a `hexagonrpcd.service` for the secure ADSP
  node and a `fastrpc` user; Alpine's `72-feedbackd.rules` masked (it gives the LED ring and torch to feedbackd);
  `rmtfs`, `tqftpserv` and `postmarketos-zram-swap` masked.
- **Armada:** the OSTree deployment and its `/var` copied out of the image's btrfs as a plain root; the mounts bootc
  wrote for the image's disk removed; OSTree, bootc and rpm-ostree units, `bootc-generic-growpart`, Armada's ABL and
  boot-image plumbing, its installer (it writes to the internal storage) and its MTP gadget (it would take the USB
  controller from the initramfs' gadget) masked; a device profile for Armada's `device-env` and the Gaming Mode
  orientation and a touch calibration matrix (as on SteamOS); its handheld controller setup masked; LADSPA plugins;
  SELinux permissive; developer access through a NetworkManager profile, with `usb0` taken out of NetworkManager's
  "unmanaged USB gadget" rule (Armada has no systemd-networkd).

</details>

### NixOS: your own configuration

The NixOS root carries the flake it was built from in `/etc/nixos`. Only `configuration.nix` there is yours; it
starts with NixOS's defaults (a few examples, commented out). `local.nix` (this tablet's partition, user, Android
hash) and `tb323fu-linux/` (a copy of this repository: kernel stub, firmware, services, the desktop) are the
tablet's part. After a change, on the tablet:

```sh
sudo nixos-rebuild switch --flake /etc/nixos#tb323fu-nixos
```

The first rebuild downloads nixpkgs (needs the network). The rebuilt system is what the next start boots.

**Your own copy, e.g. `~/nixos-config`** (verified):

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

- **The USB gadget sometimes does not attach after boot** (seen on Arch and NixOS): no USB network or
  serial shell. `echo connect > /sys/class/udc/*/soft_connect` as root, or unplug and replug the cable.
- **NixOS:** after a rebuild `/` was once left with mode 777, and sshd then refuses every key: `chmod 755 /`.
- **Fedora:** the builder keeps firewalld on for release kernels (they have `nft_compat` and `nft_fib_ipv6`) and checks
  a kernel given with `MODULES_FROM=`. A root built with the current builder has not been booted yet (untested).
- **Fedora, NixOS, SteamOS:** whether GNOME/KDE asks for the Wi-Fi password on first use was not checked after the
  Ubuntu `gnome-keyring` fix (Fedora and NixOS install `gnome-keyring` with their GNOME); each was set up with a saved
  connection (`NM_CONNECTIONS_FROM=`).
- **SteamOS (port release v1.3-8elite-beta2):** Steam's power menu has no Switch to Desktop, and
  `steamosctl switch-to-desktop-mode` brings Gaming Mode back: SDDM logs in again without taking the temporary
  Plasma session the manager writes. The older release (v1.3-odin3-beta1, withdrawn) switched both ways on the
  development tablet.
- **SteamOS, touch in games that only take a mouse:** the port's session starts gamescope with
  `--default-touch-mode 4` (passthrough), so a game under Proton shows a cursor that follows the finger but gets no
  click. Setting `STEAM_TOUCH_CLICK_MODE` to 1 on the root window makes taps click, but touch in Steam's own interface
  then behaves like a mouse until the session restarts. A controller avoids it (reported in
  [#57](https://github.com/joonhoekim/tb323fu-linux/discussions/57)).
- No SELinux in the kernel (Fedora runs with the config set to permissive).
- Fedora: x86 binaries go through FEX only if `qemu-user-static-x86` is not installed (its binfmt entry wins; the
  builder excludes it).

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
- **rmtfs and tqftpserv masked.** `rmtfs -s` starts the modem, which then crashes and resets the whole SoC.
  The tablet has no GNSS antenna and no cellular, so the modem has no use.
- **Mobile-distribution helpers masked** when present: `droid-juicer` (holds the boot forever), `qbootctl`,
  `bootmac-bluetooth`, ModemManager.
- **Sensors:** `hexagonrpcd` with a `fastrpc` system user and its udev rule, and an `iio-sensor-proxy` built with
  the SSC backend (libssc) — Ubuntu's and Fedora's have none: the Ubuntu builder builds one, the Fedora builder
  copies one. The sensor hub's own files are made on the tablet at first start (`tb323fu-sensors-tree` in the
  platform files).
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
- First start: Armada's own splash for a while, then Gaming Mode; Steam's first-start setup (language) comes up; the sound card and the helper run (verified; touch needs the calibration matrix).
- x86 games: Armada presents its x86-64 runtime for FEX from a squashfs image compressed with zstd: kernel t40 mounts
  it (`CONFIG_SQUASHFS_ZSTD`), and Decky's plugin loader then starts; a game itself is untested. On t39 both fail.
- Not checked yet: Desktop Mode, sound by ear, suspend through Armada's own `suspend-dispatch`, Armada's power
  daemon next to the helper (both set CPU and GPU limits). No sensors (Fedora's `iio-sensor-proxy` has no SSC
  backend).
- Open: `armada-flatpak-setup` fails ("GPG: Unable to export keys"), as does `flatpak remotes --system`: the system
  Flatpak repository copied from the image is not usable as it is. With developer access, Steam's first-start network
  step disconnects `usb0` (NetworkManager then leaves it down until the next start).

</details>

[^boot-time]: `systemd-analyze` "Startup finished" (kernel + userspace), with automatic login. The time changes with the kernel; these values are not kept up to date.
[^ufs-installer]: Also built on the tablet itself, and through the guided installer onto `tb323fu-ufs` with `TARGET=ufs`, built in WSL2. The other systems were also installed onto `tb323fu-ufs` this way; their `tb323fu-check` passes except where "Not working" says otherwise.
[^ubuntu-sensors]: The first start builds the sensor hub's files; sensors also work after suspend. Speakers run through the protection filter on both amplifiers. Installed with `TARGET=ufs` the desktop comes up in 4.6 s.
[^fedora-time]: Time to graphical.target; "Startup finished" is 15 s while first-boot timer jobs (plocate, fstrim) still run. iSCSI and plymouth are masked (see the builder). Through the installer onto `tb323fu-ufs`: 3.7 s.
[^netfilter]: Needs the netfilter set in [kernel/config/baldur-netfilter.fragment](../kernel/config/).
[^steam-client]: Steam's own client, run with `FEXBash -c "~/steam-launcher/steam -no-cef-sandbox"`; games untested.
[^steamos-port]: [SteamOS ARM for handhelds](https://github.com/hashtagbasit/SteamOS-ARM-Handhelds); unofficial, not affiliated with Valve. The builder pins v1.3-8elite-beta2. The earlier v1.3-odin3-beta1 (since withdrawn) also switched to Desktop Mode (KDE) and back. See [SteamOS notes](#steamos-notes).
[^pmos-installer]: All steps from an empty work directory onto an existing `tb323fu-pmos` partition (`REPARTITION_CARD` unset, so the card kept its other systems); the root grew to its 64 GB partition on the first start.
[^pmos-time]: `systemd-time-wait-sync` keeps the boot unfinished until the clock syncs over the network; graphical.target is reached at 6.8 s, Phosh is ready at 12 s.
[^armada-port]: [Armada](https://github.com/armada-os/armada), a gaming distribution for ARM handhelds; its device list ends at SM8750, so this SoC is not one of its targets. See [Armada notes](#armada-notes).
