# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: Debian 13 (see README.md).
DISTRO_TITLE="Debian 13"
DISTRO_STATUS=verified
DISTRO_MINUTES="20-60 min with GNOME"

distro_host_packages() { echo debootstrap debian-archive-keyring; }

distro_about() {
	cat <<EOF2
rootfs/debian/build-rootfs.sh installs Debian 13 (trixie) with debootstrap ($DESKTOP), the
firmware, your boot_b hash and the tablet's packages (helper-v release, .deb). It is the
Ubuntu builder with Debian's archive. Through qemu on an x86-64 PC about as long as Ubuntu.
EOF2
}

distro_build() { # MNT USER
	run_builder "$repo/rootfs/debian/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" DESKTOP="$DESKTOP" DEV_USER="$2" DEV_ACCESS="$DEV_ACCESS" \
		FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" DEBS_FROM="$WORK/debs"
}
