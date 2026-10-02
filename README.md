# tb323fu-linux

Mainline Linux on the **Lenovo Legion Tab Gen 5 / Legion Y700 5th Gen** (model **TB323FU**, Snapdragon 8 Elite Gen 5, SM8850 "kaanapali", board "baldur").
The same model number is used in China and elsewhere; the firmware region (PRC / ROW) differs, not the hardware.

> **Status: work in progress — not ready for everyday use.** There are no release images yet; the [install guide](docs/install.md)
> is reconstructed from the development records and has not been followed end to end by anyone else.
> Everything here is published so the kernel work can be reviewed and reused.

## What this is

The main product is the **kernel port**: a patch series on top of a fixed upstream base, the board device tree, and the kernel config.
Distribution and desktop integration (a Debian/GNOME image, services, udev rules) are provided as an optional add-on, not a requirement —
the kernel does not assume a particular distribution or desktop.

## Where to start

| You want to | Read |
|---|---|
| know what works | [Hardware status](docs/hardware-status.md) — a feature is marked as working only after it was checked on the device (measured, or seen/heard by a person); "the driver probes" does not count |
| see which distributions boot | [Distributions](docs/distros.md) |
| understand how the tablet was set up for dual boot | [Rooting and dual boot setup](docs/rooting.md) (host setup per OS, backup, LTBox, KernelSU, switching) and [Recovery](docs/recovery.md) |
| install Linux after rooting | [Installing Linux](docs/install.md) (firmware, boot image, root partition, first boot, multiboot; each step marked verified / from records / untested) and the guided [install script prototype](tools/install/README.md) |
| build or review the kernel | [kernel/](kernel/README.md), [PROVENANCE.md](kernel/PROVENANCE.md), [initramfs](kernel/initramfs/README.md) |
| put together a root filesystem | [platform files](userspace/platform/README.md), [firmware](firmware/README.md), [`rootfs/`](rootfs/) builders |
| use the tablet settings (charge limit, refresh rate, multiboot) | [Device helper](docs/helper.md) |

## Layout

| Path | Contents |
|---|---|
| `kernel/` | patch series, base commit, config, board DT, [`PROVENANCE.md`](kernel/PROVENANCE.md) (where every imported patch came from), [`initramfs/`](kernel/initramfs/) (the built-in initramfs: USB way in, boot summary, multiboot root selection) |
| `firmware/` | manifest (file, sha256, source on the device) and a script that extracts the firmware from your own tablet — no firmware files are stored here |
| `rootfs/` | optional: scripts that build root filesystems for the multiboot partitions (Ubuntu, Arch Linux ARM, Fedora, NixOS) — status in [docs/distros.md](docs/distros.md) |
| `userspace/` | `platform/`: distribution-neutral platform files every install needs (udev, systemd units, audio/UCM, sensors, emergency key) with `install.sh`; `desktop/`: optional desktop extras |
| `helper/` | device helper: `tb323fu-helperd` (system D-Bus), `tb323fu-ctl`, the `tb323fu-settings` app ([docs/helper.md](docs/helper.md)) |
| `packaging/` | Debian (`build-debs.sh`), Arch (`PKGBUILD`) and Nix (`flake.nix` at the root) recipes |
| `android/` | Android-side helpers (switching between Android and Linux; setup in [docs/rooting.md](docs/rooting.md)) |
| `tools/` | build and flash scripts |
| `docs/` | documentation (see [Where to start](#where-to-start)) |

## License

Every file's SPDX header takes precedence; files without one are covered by [`REUSE.toml`](REUSE.toml), otherwise by the MIT [`LICENSE`](LICENSE).
[`NOTICE`](NOTICE) has the full map, the copyright holders and the third-party code; the license texts are in [`LICENSES/`](LICENSES/).

| Part | License |
|---|---|
| helper (daemon, CLI, helper core, Tablet Settings app and their data files) | `GPL-3.0-or-later` |
| kernel patches and config fragments | `GPL-2.0-only`, as the kernel; imported patches keep their original authors and `Signed-off-by` lines (see `kernel/PROVENANCE.md`) |
| board device trees (`kernel/dts/`) | `BSD-3-Clause`, as upstream qcom device trees |
| out-of-tree `aw882xx` driver | `GPL-2.0` (AWINIC's headers kept) |
| GNOME Shell extension | `GPL-2.0-or-later`, as GNOME Shell |
| platform files, root filesystem builders, tools, packaging, site code | `MIT` (a few platform files follow their upstream: UCM2 `BSD-3-Clause`, libcamera tuning `CC0-1.0`, feedbackd rule `LGPL-2.1-or-later`) |
| documentation text and screenshots | `CC-BY-SA-4.0`; code snippets and commands in the documentation are also available under `MIT` |

Firmware is not redistributed, and neither are Valve's Steam client, the SteamOS-ARM image or distribution base images: the builders download them on your machine.
The project name, icons, signing keys and update URL are covered by the [name and trademark policy](TRADEMARKS.md); forks and modified builds use their own.
Contributions are taken under the license of the file they change, with a DCO sign-off ([CONTRIBUTING.md](CONTRIBUTING.md)).
This is an independent community project, not affiliated with Lenovo, Qualcomm or Valve; their names are used only to identify the device and the software it works with.

## Acknowledgements

Built on the community [kaanapali-mainline](https://github.com/kaanapali-mainline) tree and the work referenced in `kernel/PROVENANCE.md`.
This project was developed with the help of AI coding assistants (Claude Code); every change was tested on the device before being marked as working.
