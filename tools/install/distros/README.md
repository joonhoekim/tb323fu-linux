# Distribution modules

[`install.sh`](../install.sh) does the same steps for every system: firmware, the way back, the kernel, the card,
writing the image, `boot_a`. Only the `rootfs` step depends on the distribution, and that part lives in a **module**:
a small shell file that `install.sh` sources.

```sh
tools/install/install.sh --distros              # the built-in modules
DISTRO=arch tools/install/install.sh            # a built-in module by name
DISTRO=~/my-distro.sh tools/install/install.sh  # your own module, by path
```

| Module | System | Status |
|---|---|---|
| [`ubuntu.sh`](ubuntu.sh) | Ubuntu 26.04, GNOME | verified |
| [`arch.sh`](arch.sh) | Arch Linux ARM, GNOME | verified |
| [`nixos.sh`](nixos.sh) | NixOS (unstable), GNOME | experimental |
| [`steamos.sh`](steamos.sh) | SteamOS, community port | experimental |
| [`armada.sh`](armada.sh) | Armada (Fedora bootc) | experimental |

The status is the module's `DISTRO_STATUS`. What was checked on the tablet for each system, and what does not work
yet: [docs/distros.md](../../../docs/distros.md).
What the status words mean next to those of other pages:
[hardware-status.md → Labels on other pages](../../../docs/hardware-status.md#labels-on-other-pages).

Changing `DISTRO` after a `rootfs` step builds the root again from scratch (the image holds one system). The
other steps are not repeated.

## Writing a module

A module is sourced by bash after `install.sh` has set its variables. It sets three variables and defines
`distro_build`; everything else is optional.

```sh
DISTRO_TITLE="My Linux 1.0"        # shown in the banner and the rootfs step
DISTRO_STATUS=experimental         # verified | experimental | custom (anything but verified prints a warning)
DISTRO_MINUTES="30 min"            # shown in the rootfs step's heading

distro_build() {   # $1 = the mounted, empty ext4 image, $2 = the user name the person chose
	run_builder "$repo/rootfs/mylinux/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" DESKTOP="$DESKTOP" DEV_USER="$2" DEV_ACCESS="$DEV_ACCESS" \
		FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" DEBS_FROM="$WORK/debs"
}

distro_about() { echo "What the build does, in two or three lines."; }      # optional
distro_host_packages() { echo debootstrap; }                                 # optional: extra Debian/Ubuntu packages on the PC
distro_set_password() { sudo chroot "$1" passwd "$2" </dev/tty; }            # optional: this is the default
distro_growroot() { growroot_into "$1"; }                                     # optional: this is the default
DISTRO_KERNEL_ASSETS="config-tb323fu-t*"   # optional: more files of the kernel release, fetched into $WORK/kernel
DISTRO_USER=steamos                        # optional: a fixed user; the person is not asked for a name
```

`distro_growroot` adds what grows the root to its partition on the first start; the default installs a small
systemd service into `/etc`, a system whose `/etc` is generated (NixOS) does it in its own configuration instead. A
module may also raise `IMG_SIZE` (the image before shrinking, 24G) when its root is larger (SteamOS: 40G).

`run_builder SCRIPT MNT VAR=VALUE…` runs a builder as root, without a controlling terminal, from `$WORK`, and only
prints it under `--dry-run`. A module may also run its own commands instead; `$SUDO` is `sudo` or empty, and
`say`, `warn` and `run` print like the rest of the script.

What the module gets:

| Variable | What |
|---|---|
| `$1` | the root, an ext4 image mounted at `$WORK/mnt` (label `$ROOT_PARTLABEL`) |
| `$2` | the user name; give it a password with `distro_set_password`, which runs after the build |
| `$FW` | the firmware copied from Android, laid out as `lib/firmware` (with the audio topology added) |
| `$WORK/config/android-boot.sha256` | the hash of Android's boot image in `boot_b`, for the way back |
| `$WORK/debs/` | the `tb323fu-*.deb` files of the helper release (platform files, helper, app, extension) |
| `$ROOT_PARTLABEL`, `$DESKTOP`, `$DEV_ACCESS` | the partition's GPT name (for `/etc/fstab`), `gnome` or `none`, developer access |
| `$repo`, `$WORK`, `$DRY` | the repository, the work directory, 1 under `--dry-run` |

What the root must contain to boot and work on this tablet is in
[docs/distros.md → What every root needs](../../../docs/distros.md#what-every-root-needs); the builders in
[`rootfs/`](../../../rootfs/) are examples. After `distro_build`, `install.sh` adds the service that grows the root
to its partition on the first start, sets the password, shrinks and packs the image, and writes it to the card.
