#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- a NixOS (unstable, aarch64) root filesystem for the TB323FU,
# installed natively on an arm64 host with Nix (for example the tablet itself
# running Debian with nix-setup-systemd) into a mounted, empty ext4 partition.
#
#   sh build-rootfs.sh TARGET_DIR
#
# It writes a device-local flake to WORK -- outside this repository, because it
# holds this device's firmware, sensor files, SSH keys and
# password hash -- that combines this repository's nixosModules.rootfs
# (configuration.nix next to this file) with a generated local.nix, builds the
# system and runs `nixos-install --no-bootloader` from it. WORK stays usable
# afterwards: on the booted NixOS, `nixos-rebuild switch --flake WORK#NAME`
# (copy WORK over first) -- the newest generation is what the initramfs boots.
#
# Environment (all optional):
#   ROOT_PARTLABEL=tb323fu-nixos    GPT name of the target partition (fstab)
#   HOSTNAME_NEW=$ROOT_PARTLABEL    hostname (also the nixosConfigurations name)
#   DESKTOP=gnome                   gnome (GNOME + GDM) or none
#   WORK=/root/nixos-tb323fu        the device-local flake
#   REPO=<this checkout>            this repository; the flake input is git+file:
#                                   (committed files only) for a git checkout, else path:
#   REPO_URL=                       override the flake URL of this repository
#   MODULES_FROM=                   empty (default): shared modules -- the boot image's
#                                   initramfs mounts its modules on /lib/modules/<release>
#                                   and the system holds a kernel stub, so kernel updates
#                                   need no rebuild. A directory (e.g. /lib/modules/$(uname -r)):
#                                   copy those modules into the system instead, for a root
#                                   with "own" in /etc/tb323fu/modules (rebuild per kernel)
#   KCONFIG_FROM=/proc/config.gz    that kernel's configuration (gzip or plain)
#   FIRMWARE_FROM=/lib/firmware     qcom/ ath12k/ qca/ novatek/ (and aw882xx_acf.bin) from here
#   SENSORS_FROM=                   sensor hub files for hexagonrpcd (dsp/ sensors/ socinfo/,
#                                   e.g. /usr/share/qcom/kaanapali/Lenovo/TB323FU); without
#                                   it there are no sensors (rotation, light)
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf,
#                                   emergency-key.conf when present (device-specific)
#   NM_CONNECTIONS_FROM=            NetworkManager keyfiles (*.nmconnection) copied into the
#                                   target's /etc/NetworkManager/system-connections (mutable
#                                   state, mode 600 -- never into the Nix configuration)
#   DEV_ACCESS=0                    1 = developer access: usb0 gadget network
#                                   (192.168.7.2/24), root autologin on ttyGS0,
#                                   sshd root login allowed
#   DEV_SSH_KEYS=                   with DEV_ACCESS=1: an authorized_keys file for root
#   BALDUR_DEV_PASSWORD=            dev-only password for root and the user (its hash ends
#                                   up in the store); without it root is locked and the
#                                   user has no password
#   DEV_USER=                       create this user (wheel); with DESKTOP=gnome it is
#                                   also logged in automatically
# Needs: nix (flakes are enabled per command), openssl, gzip. The first build
# downloads a few GB from cache.nixos.org into the host's /nix/store and copies
# the closure into TARGET_DIR/nix/store.
set -eu
T=${1:?usage: build-rootfs.sh TARGET_DIR}
here=$(cd "$(dirname "$0")/../.." && pwd)
ROOT_PARTLABEL=${ROOT_PARTLABEL:-tb323fu-nixos}
HOSTNAME_NEW=${HOSTNAME_NEW:-$ROOT_PARTLABEL}
DESKTOP=${DESKTOP:-gnome}
WORK=${WORK:-/root/nixos-tb323fu}
REPO=${REPO:-$here}
MODULES_FROM=${MODULES_FROM:-}
KCONFIG_FROM=${KCONFIG_FROM:-/proc/config.gz}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
SENSORS_FROM=${SENSORS_FROM:-}
CONFIG_FROM=${CONFIG_FROM:-/etc/tb323fu}
NM_CONNECTIONS_FROM=${NM_CONNECTIONS_FROM:-}
DEV_ACCESS=${DEV_ACCESS:-0}
DEV_SSH_KEYS=${DEV_SSH_KEYS:-}
BALDUR_DEV_PASSWORD=${BALDUR_DEV_PASSWORD:-}
DEV_USER=${DEV_USER:-}
if [ -z "${REPO_URL:-}" ]; then
	if [ -d "$REPO/.git" ]; then REPO_URL=git+file://$REPO; else REPO_URL=path:$REPO; fi
fi
NIX="nix --extra-experimental-features nix-command --extra-experimental-features flakes"
say() { printf '== %s\n' "$*"; }
nixstr() { printf '"%s"' "$(printf '%s' "$1" | sed 's/[\\"$]/\\&/g')"; }

mountpoint -q "$T" || { echo "$T is not a mount point"; exit 1; }
# arm64, or another architecture running arm64 programs through qemu-user
# (binfmt with the F flag, e.g. Debian/Ubuntu's qemu-user-binfmt; WSL2 works)
case $(uname -m) in
aarch64) ;;
*) b=/proc/sys/fs/binfmt_misc/qemu-aarch64
	{ grep -qx enabled $b && grep -q '^flags:.*F' $b; } 2>/dev/null ||
		{ echo "run this on an arm64 host, or install qemu-user-binfmt (arm64 programs through qemu)"; exit 1; } ;;
esac
command -v nix > /dev/null || { echo "needs nix"; exit 1; }
[ -z "$MODULES_FROM" ] || [ -d "$MODULES_FROM" ] || { echo "no kernel modules at $MODULES_FROM"; exit 1; }

# 1. the device-local flake: copies of what must not go into git
say "device-local flake in $WORK"
mkdir -p "$WORK/kernel/modules"
rm -rf "$WORK/kernel/modules"/*
kmods=null
if [ -n "$MODULES_FROM" ]; then
	v=$(basename "$MODULES_FROM")
	cp -a "$MODULES_FROM" "$WORK/kernel/modules/$v"
	rm -f "$WORK/kernel/modules/$v/build" "$WORK/kernel/modules/$v/source"
	kmods="./kernel/modules + $(nixstr "/$v")"
fi
kconfig=null
if [ -e "$KCONFIG_FROM" ]; then
	case $KCONFIG_FROM in *.gz) gzip -dc "$KCONFIG_FROM" ;; *) cat "$KCONFIG_FROM" ;; esac > "$WORK/kernel/config"
	kconfig=./kernel/config
fi
fw=null
rm -rf "$WORK/firmware"
for d in qcom ath12k qca novatek; do   # novatek: the touch controller firmware
	[ -d "$FIRMWARE_FROM/$d" ] || continue
	mkdir -p "$WORK/firmware"; cp -a "$FIRMWARE_FROM/$d" "$WORK/firmware/"; fw=./firmware
done
[ -e "$FIRMWARE_FROM/aw882xx_acf.bin" ] && { mkdir -p "$WORK/firmware"; cp -a "$FIRMWARE_FROM/aw882xx_acf.bin" "$WORK/firmware/"; fw=./firmware; }
sensors=null
rm -rf "$WORK/sensors"
if [ -n "$SENSORS_FROM" ] && [ -d "$SENSORS_FROM" ]; then
	cp -a "$SENSORS_FROM" "$WORK/sensors"; sensors=./sensors
fi
keys=
if [ "$DEV_ACCESS" = 1 ] && [ -n "$DEV_SSH_KEYS" ] && [ -e "$DEV_SSH_KEYS" ]; then
	keys=$(grep -v -e '^#' -e '^$' "$DEV_SSH_KEYS" | while read -r k; do printf '      %s\n' "$(nixstr "$k")"; done)
fi
hash=null
[ -n "$BALDUR_DEV_PASSWORD" ] && hash=$(nixstr "$(printf '%s\n' "$BALDUR_DEV_PASSWORD" | openssl passwd -6 -stdin)")
user=null; [ -n "$DEV_USER" ] && user=$(nixstr "$DEV_USER")
androidsha=null
[ -s "$CONFIG_FROM/android-boot.sha256" ] && androidsha=$(nixstr "$(head -c 64 "$CONFIG_FROM/android-boot.sha256")")
dev=false; [ "$DEV_ACCESS" = 1 ] && dev=true

cat > "$WORK/local.nix" <<EOF
# Written by tb323fu-linux rootfs/nixos/build-rootfs.sh -- device-local, not for git.
{ ... }: {
  tb323fu.rootfs = {
    partlabel = $(nixstr "$ROOT_PARTLABEL");
    kernel.modules = $kmods;
    kernel.config = $kconfig;
    firmware = $fw;
    desktop = $(nixstr "$DESKTOP");
    user = $user;
    initialHashedPassword = $hash;
    devAccess = $dev;
    rootAuthorizedKeys = [
$keys
    ];
  };
  networking.hostName = $(nixstr "$HOSTNAME_NEW");
  services.tb323fu.sensors.dataDir = $sensors;
  services.tb323fu.androidBootSha256 = $androidsha;
}
EOF
cat > "$WORK/flake.nix" <<EOF
# Written by tb323fu-linux rootfs/nixos/build-rootfs.sh -- device-local, not for git.
{
  inputs.tb323fu-linux.url = $(nixstr "$REPO_URL");
  inputs.nixpkgs.follows = "tb323fu-linux/nixpkgs";
  outputs = { self, nixpkgs, tb323fu-linux }: {
    nixosConfigurations.$(nixstr "$HOSTNAME_NEW") = nixpkgs.lib.nixosSystem {
      modules = [
        { nixpkgs.hostPlatform = "aarch64-linux"; }
        tb323fu-linux.nixosModules.rootfs
        ./local.nix
      ];
    };
  };
}
EOF
chmod 700 "$WORK"
# pick up the current state of this repository
$NIX flake update --flake "path:$WORK"

# 2. build the system in the host store (errors show up here, with logs), then
#    install it: nixos-install copies the closure into TARGET_DIR/nix/store and
#    creates the system profile the initramfs starts
C="path:$WORK#nixosConfigurations.\"$HOSTNAME_NEW\""
say "building the system"
sys=$(nice -n 19 $NIX build --no-link --print-out-paths -L "$C.config.system.build.toplevel")
# the installer tools from the same nixpkgs
t1=$($NIX build --no-link --print-out-paths "$C.pkgs.nixos-install")
t2=$($NIX build --no-link --print-out-paths "$C.pkgs.nixos-enter")
tools=$t1/bin:$t2/bin:
say "system $sys"
say "nixos-install into $T"
PATH=$tools$PATH nixos-install --root "$T" --no-bootloader --no-root-passwd --no-channel-copy \
	--flake "path:$WORK#$HOSTNAME_NEW"
# First activation now, in a chroot (users, passwords, /etc, /bin/sh), so the
# initramfs finds a complete system; stage 2 activates again at every boot.
say "first activation"
PATH=$tools$PATH nixos-enter --root "$T" -c true
# activation in the chroot wrote to the target's /run, a tmpfs once booted
find "$T/run" -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true

# 3. mutable state: this device's settings, Wi-Fi connections
mkdir -p "$T/etc/tb323fu"
for f in bt-address audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && [ ! -e "$T/etc/tb323fu/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/$f"
done
if [ -n "$NM_CONNECTIONS_FROM" ]; then
	d=$T/etc/NetworkManager/system-connections
	mkdir -p "$d"; chmod 700 "$d"
	for c in "$NM_CONNECTIONS_FROM"/*.nmconnection; do
		[ -e "$c" ] || continue
		install -m 600 "$c" "$d/"
	done
fi
sync
say "done: $(du -sh "$T" 2>/dev/null | cut -f1) in $T ($HOSTNAME_NEW, desktop $DESKTOP)"
