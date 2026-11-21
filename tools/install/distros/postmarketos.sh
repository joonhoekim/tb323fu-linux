# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: postmarketOS (Nura) (see README.md).
DISTRO_TITLE="postmarketOS v26.06 (Nura)"
DISTRO_STATUS=verified
DISTRO_MINUTES="30-45 min with Phosh"

distro_host_packages() { echo "curl tar openssl"; }

distro_about() {
	cat <<EOF2
rootfs/postmarketos/build-rootfs.sh bootstraps postmarketOS with systemd from its package
repositories (apk.static checked against the keys in rootfs/postmarketos/keys), installs
Phosh (or Plasma Mobile, GNOME Mobile with DESKTOP=plasma-mobile, gnome-mobile; none: console
only), the firmware, your boot_b hash, and compiles/installs the tablet's platform files and
helper from this repository.
EOF2
}

distro_build() { # MNT USER
	local d=phosh
	case $DESKTOP in none|phosh|plasma-mobile|gnome-mobile) d=$DESKTOP ;; esac
	run_builder "$repo/rootfs/postmarketos/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW=tb323fu-pmos DESKTOP="$d" DEV_USER="$2" \
		DEV_ACCESS="$DEV_ACCESS" FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config"
}
