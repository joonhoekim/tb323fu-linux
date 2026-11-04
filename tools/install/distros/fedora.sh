# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: Fedora 44 Workstation (see README.md).
DISTRO_TITLE="Fedora 44 Workstation"
DISTRO_STATUS=verified
DISTRO_MINUTES="20-50 min with GNOME"

distro_host_packages() { echo "jq curl tar xz-utils gpg"; }

distro_about() {
	cat <<EOF2
rootfs/fedora/build-rootfs.sh unpacks the official Fedora 44 container base image (checked
against its signature), installs Fedora Workstation (GNOME), FEX-emu (x86/x86-64 emulation),
the firmware, your boot_b hash, and compiles/installs the tablet's platform files, helper and
GNOME extension from this repository.
EOF2
}

distro_build() { # MNT USER
	run_builder "$repo/rootfs/fedora/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW=tb323fu-fedora DESKTOP="$DESKTOP" DEV_USER="$2" \
		DEV_ACCESS="$DEV_ACCESS" FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config"
}
