# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: SteamOS, community port (see README.md).
DISTRO_TITLE="SteamOS (community port)"
DISTRO_STATUS=experimental
DISTRO_MINUTES="15-30 min after a 4.4 GB download"
DISTRO_KERNEL_ASSETS="modules-tb323fu-t*.tar.gz"
DISTRO_USER=steamos
[ "$IMG_SIZE" = 24G ] && IMG_SIZE=40G

distro_host_packages() { echo 7zip rsync; }

distro_about() {
	cat <<EOF2
rootfs/steamos/build-rootfs.sh downloads the SM8750 image of "SteamOS ARM for handhelds"
(github.com/hashtagbasit/SteamOS-ARM-Handhelds, unofficial, not affiliated with Valve; pinned by its
SHA256SUMS), copies its root and home into the image, and adds this tablet's firmware, kernel modules
lists, platform files and helper. Gaming Mode starts on its own; its user is "steamos".
EOF2
}

# swh-plugins from Ubuntu 24.04: its sc4 needs glibc 2.39, the image's (newer builds need 2.40)
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
		rm -rf "$WORK/steamos-modules" && mkdir -p "$WORK/steamos-modules" && tar -C "$WORK/steamos-modules" -xzf "$WORK/kernel/$mt" || return 1
		rel=$(cd "$WORK/steamos-modules" && find . -mindepth 3 -maxdepth 4 -type d -path '*/lib/modules/*' -name '[0-9]*' | head -1)
		[ -n "$rel" ] || { warn "no lib/modules/<release> in $mt"; return 1; }
		ladspa_from_ubuntu "$WORK/steamos-ladspa" || return 1
	fi
	run_builder "$repo/rootfs/steamos/build-rootfs.sh" "$1" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW=tb323fu-steamos DEV_ACCESS="$DEV_ACCESS" \
		FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" DEBS_FROM="$WORK/debs" \
		MODULES_FROM="$WORK/steamos-modules/${rel#./}" LADSPA_FROM="$WORK/steamos-ladspa/usr/lib/ladspa" \
		TIMEZONE="$(readlink /etc/localtime 2>/dev/null | sed -n 's|.*/zoneinfo/||p')" WORK_DIR="$WORK/steamos-image"
}
