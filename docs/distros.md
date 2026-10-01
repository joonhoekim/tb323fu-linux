# Distributions

The kernel does not depend on a distribution. These are the systems that have been built with the scripts in
[`rootfs/`](../rootfs/) and booted on a TB323FU, each from its own partition through the initramfs's root selection
([kernel/initramfs/README.md](../kernel/initramfs/README.md#root-partitions-and-multiboot), `tb323fu-ctl boot …`,
[helper.md](helper.md#multiboot)). "Checked" means seen on the device, not "should work".

| System | Partition (example) | Builder | Boot to desktop¹ | Checked on the device |
|---|---|---|---|---|
| Debian 13 (trixie) | `baldur-root` (UFS) | the original development image | 5.8 s | everything in [hardware-status.md](hardware-status.md) |
| Ubuntu 26.04 LTS | `tb323fu-ubuntu` | `rootfs/ubuntu` | 8.3 s | GNOME 50 autologin, touch, Wi-Fi, speakers (protection filter, both amplifiers), helper + GNOME extension |
| Arch Linux ARM | `tb323fu-arch` | `rootfs/arch` | 7.5 s | autologin, touch, Wi-Fi, speakers, sensors (4), helper |
| Fedora 44 Workstation + FEX | `tb323fu-fedora` | `rootfs/fedora` | 7.5 s² | GNOME 50 autologin, Wi-Fi, speakers, sensors (4), firewalld³, FEX x86-64, Steam client starts⁴ |
| NixOS 26.11 (unstable) | `tb323fu-nixos` | `rootfs/nixos` + `flake.nix` | 11.2 s | stage 2 straight from our initramfs, GNOME autologin, Wi-Fi, speakers, sensors, helper |
| SteamOS (arm64, community handheld port⁵) | `tb323fu-spare` | `rootfs/steamos` | 14.1 s | Gaming Mode (landscape, seen on the panel; touch lands where tapped after a calibration matrix), Wi-Fi, speakers (protection filter), helper; Desktop Mode (KDE) starts and switches back |

¹ `systemd-analyze` "Startup finished" (kernel + userspace), with automatic login.\
² graphical.target; "Startup finished" is 15 s while first-boot timer jobs (plocate, fstrim) still run. iSCSI and plymouth are masked (see the builder).\
³ Needs the netfilter set in [kernel/config/baldur-netfilter.fragment](../kernel/config/).\
⁴ Steam's own client, run with `FEXBash -c "~/steam-launcher/steam -no-cef-sandbox"`; games not tried yet.\
⁵ [SteamOS ARM for handhelds](https://github.com/hashtagbasit/SteamOS-ARM-Handhelds) v1.3-odin3-beta1; unofficial, not affiliated with Valve. See [SteamOS notes](#steamos-notes).

## What every root needs

The builders in [`rootfs/`](../rootfs/) do all of it.

- **Kernel modules** of the boot image's kernel in `/lib/modules/<release>` (NixOS: a derivation wrapping them,
  `packaging/nix/prebuilt-kernel.nix`). No distribution kernel, bootloader or initramfs is used.
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

## Known gaps

- No SELinux in the kernel (Fedora runs with the config set to permissive).
- Fedora: x86 binaries go through FEX only if `qemu-user-static-x86` is not installed (its binfmt entry wins).

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
