# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: postmarketOS (Phosh + systemd).
DISTRO_TITLE="postmarketOS (Phosh)"
DISTRO_STATUS=experimental
DISTRO_MINUTES="10-25 min"

distro_host_packages() { echo "curl tar xz-utils"; }

distro_about() {
	cat <<EOF2
rootfs/postmarketos/build-rootfs.sh bootstraps postmarketOS (Alpine edge) with Phosh (touch-friendly
Wayland shell) and systemd, installs hardware firmware, audio topology, platform files,
sensor hub daemons (hexagonrpcd, qrtr-ns), Flatpak/Flathub support, and builds the helper
daemon and settings GUI natively.
EOF2
}

distro_build() { # MNT USER
	run_builder "$repo/rootfs/postmarketos/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW=tb323fu-pmos DESKTOP="phosh" DEV_USER="$2" \
		FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config"
}
