# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: Armada (see README.md).
DISTRO_TITLE="Armada"
DISTRO_STATUS=experimental
DISTRO_MINUTES="15-30 min after a 5.9 GB download"
DISTRO_KERNEL_ASSETS="modules-tb323fu-t*.tar.gz config-tb323fu-t*"
DISTRO_USER=armada
[ "$IMG_SIZE" = 24G ] && IMG_SIZE=40G

distro_host_packages() { echo rsync pigz; }

distro_about() {
	cat <<EOF2
rootfs/armada/build-rootfs.sh downloads an Armada release image (github.com/armada-os/armada, pinned by
sha256; about 33 GB unpacked on this PC), copies its OSTree deployment and /var into the image as a
plain root (Armada's own updates do not work there), and adds this tablet's firmware, module lists,
platform files and helper. Gaming Mode starts on its own; the user is "armada".
EOF2
}

# swh-plugins from Ubuntu 24.04 (glibc 2.39; Armada has 2.43)
ladspa_from_ubuntu() { # DIR
	local d=$1 fn
	[ -e "$d/usr/lib/ladspa/sc4_1882.so" ] && return 0
	fn=$(curl -fsSL http://ports.ubuntu.com/ubuntu-ports/dists/noble/universe/binary-arm64/Packages.gz | gzip -dc |
		awk '/^Package: swh-plugins$/ {f=1} f && /^Filename:/ {print $2; exit}')
	[ -n "$fn" ] || { warn "swh-plugins not found in Ubuntu's arm64 archive"; return 1; }
	mkdir -p "$d" && curl -fsSL -o "$d/swh.deb" "http://ports.ubuntu.com/ubuntu-ports/$fn" && dpkg-deb -x "$d/swh.deb" "$d"
}

distro_build() { # MNT USER
	local mt rel
	mt=$(cd "$WORK/kernel" 2>/dev/null && ls modules-tb323fu-t*.tar.gz 2>/dev/null | sort -V | tail -1)
	if [ $DRY = 0 ]; then
		[ -n "$mt" ] || { warn "no kernel modules bundle (download step)"; return 1; }
		rm -rf "$WORK/armada-modules" && mkdir -p "$WORK/armada-modules" && tar -C "$WORK/armada-modules" -xzf "$WORK/kernel/$mt" || return 1
		rel=$(cd "$WORK/armada-modules" && find . -mindepth 3 -maxdepth 4 -type d -path '*/lib/modules/*' -name '[0-9]*' | head -1)
		[ -n "$rel" ] || { warn "no lib/modules/<release> in $mt"; return 1; }
		ladspa_from_ubuntu "$WORK/armada-ladspa" || return 1
	fi
	run_builder "$repo/rootfs/armada/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW=tb323fu-armada DEV_ACCESS="$DEV_ACCESS" \
		FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" DEBS_FROM="$WORK/debs" \
		MODULES_FROM="$WORK/armada-modules/${rel#./}" LADSPA_FROM="$WORK/armada-ladspa/usr/lib/ladspa" \
		KCONFIG_FROM="$WORK/kernel/$(cd "$WORK/kernel" 2>/dev/null && ls config-tb323fu-t* 2>/dev/null | sort -V | tail -1)" \
		TIMEZONE="$(readlink /etc/localtime 2>/dev/null | sed -n 's|.*/zoneinfo/||p')" WORK_DIR="$WORK/armada-image"
}
