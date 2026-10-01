# tb323fu-linux

Mainline Linux on the **Lenovo Legion Tab Gen 5 / Legion Y700 5th Gen** (model **TB323FU**, Snapdragon 8 Elite Gen 5, SM8850 "kaanapali", board "baldur").
The same model number is used in China and elsewhere; the firmware region (PRC / ROW) differs, not the hardware.

> **Status: work in progress — not ready for installation.** There are no release images yet and no install guide.
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
| understand how the tablet was set up for dual boot | [Rooting and dual boot setup](docs/rooting.md) (backup, LTBox, KernelSU, switching) and [Recovery](docs/recovery.md) |
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

The repository is MIT-licensed unless a file says otherwise (SPDX headers take precedence).
Kernel patches and drivers are `GPL-2.0-only`, device trees `GPL-2.0-only OR BSD-3-Clause`, as upstream.
Imported patches keep their original authors and `Signed-off-by` lines; see `kernel/PROVENANCE.md`.
Firmware is not redistributed.

## Acknowledgements

Built on the community [kaanapali-mainline](https://github.com/kaanapali-mainline) tree and the work referenced in `kernel/PROVENANCE.md`.
This project was developed with the help of AI coding assistants (Claude Code); every change was tested on the device before being marked as working.
