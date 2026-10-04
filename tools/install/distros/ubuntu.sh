# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: Ubuntu 26.04 (see README.md).
DISTRO_TITLE="Ubuntu 26.04"
DISTRO_STATUS=verified
DISTRO_MINUTES="20-60 min with GNOME"

distro_host_packages() { echo debootstrap ubuntu-keyring; }

distro_about() {
	cat <<EOF2
rootfs/ubuntu/build-rootfs.sh installs Ubuntu 26.04 with debootstrap ($DESKTOP), the firmware,
your boot_b hash and the tablet's packages (helper-v release, .deb). Through qemu on an x86-64
PC about 20 min without a desktop, 20-60 min with GNOME (22 min on the test PC).
EOF2
}

distro_build() { # MNT USER
	run_builder "$repo/rootfs/ubuntu/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" DESKTOP="$DESKTOP" DEV_USER="$2" DEV_ACCESS="$DEV_ACCESS" \
		FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" DEBS_FROM="$WORK/debs"
}
