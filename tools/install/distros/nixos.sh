# SPDX-License-Identifier: MIT
# Distribution module for tools/install/install.sh: NixOS (see README.md).
DISTRO_TITLE="NixOS (unstable)"
DISTRO_STATUS=experimental
DISTRO_MINUTES="1-2 h the first time"
DISTRO_KERNEL_ASSETS="config-tb323fu-t*"

distro_host_packages() { echo nix-bin openssl; }

distro_about() {
	cat <<EOF2
rootfs/nixos/build-rootfs.sh builds NixOS from this repository's flake (flake.nix, packaging/nix/) with
Nix on this PC: most of it comes prebuilt from cache.nixos.org (a few GB), the tablet's own packages
(helper, settings app) are compiled for arm64 through qemu, which is the slow part. Nix must accept
arm64 builds: "extra-platforms = aarch64-linux" in /etc/nix/nix.conf.
EOF2
}

distro_build() { # MNT USER
	local cfg; cfg=$(cd "$WORK/kernel" 2>/dev/null && ls config-tb323fu-t* 2>/dev/null | sort -V | tail -1)
	[ $DRY = 1 ] || [ -n "$cfg" ] || { warn "no kernel config (download step)"; return 1; }
	if [ $DRY = 0 ] && ! grep -qs '^extra-platforms.*aarch64-linux' /etc/nix/nix.conf && [ "$(uname -m)" != aarch64 ]; then
		warn "add 'extra-platforms = aarch64-linux' to /etc/nix/nix.conf"; return 1
	fi
	# nixos-install wants every directory above the root to be mode 755 ($HOME often is not)
	local r=/run/tb323fu-nixos-root rc
	# private: mounts the build makes under it must not appear under $1 as well
	if [ $DRY = 0 ]; then $SUDO mkdir -p $r && $SUDO mount --bind "$1" $r && $SUDO mount --make-private $r || return 1; fi
	run_builder "$repo/rootfs/nixos/build-rootfs.sh" "$r" \
		ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW=tb323fu-nixos DESKTOP="$DESKTOP" DEV_USER="$2" \
		DEV_ACCESS="$DEV_ACCESS" FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" \
		KCONFIG_FROM="$WORK/kernel/$cfg" WORK="$WORK/nixos-flake"
	rc=$?
	[ $DRY = 1 ] && return $rc
	$SUDO umount -R $r || { warn "$r is still mounted: the image must not be packed while it is"; return 1; }
	return $rc
}

# /etc is NixOS's own: the user's password goes through its passwd, the root grows by
# x-systemd.growfs (configuration.nix) instead of install.sh's service
distro_set_password() { $SUDO chroot "$1" /nix/var/nix/profiles/system/sw/bin/passwd "$2" </dev/tty; }
distro_growroot() { :; }
