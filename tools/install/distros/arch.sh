# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: Arch Linux ARM (see README.md).
DISTRO_TITLE="Arch Linux ARM"
DISTRO_STATUS=verified
DISTRO_MINUTES="10-30 min with GNOME"

distro_about() {
	cat <<EOF2
rootfs/arch/build-rootfs.sh unpacks the official Arch Linux ARM tarball (checked against its
signature), updates it, installs $DESKTOP, the firmware, your boot_b hash, and the tablet's
platform files and helper unpacked from the helper-v release's .deb files. The sensors
(hexagonrpcd is not packaged for Arch) are left out.
EOF2
}

distro_build() { # MNT USER
	run_builder "$repo/rootfs/arch/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW=tb323fu-arch DESKTOP="$DESKTOP" DEV_USER="$2" \
		DEV_ACCESS="$DEV_ACCESS" FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" DEBS_FROM="$WORK/debs"
}
