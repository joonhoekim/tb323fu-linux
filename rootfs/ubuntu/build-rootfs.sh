#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- an Ubuntu arm64 root filesystem for the TB323FU, built
# natively on an arm64 host (for example the tablet itself running Linux)
# into a mounted, empty ext4 partition.
#
#   sh build-rootfs.sh TARGET_DIR [RELEASE]          # default RELEASE resolute (26.04 LTS)
#
# Environment (all optional):
#   ROOT_PARTLABEL=tb323fu-ubuntu   GPT name of the target partition (fstab)
#   HOSTNAME_NEW=tb323fu-ubuntu     hostname of the new system
#   DESKTOP=gnome                   gnome (minimal GNOME + GDM) or none
#   MIRROR=http://ports.ubuntu.com/ubuntu-ports
#   MODULES_FROM=/lib/modules/$(uname -r)   kernel modules of the kernel that will boot it
#   FIRMWARE_FROM=/lib/firmware     copy qcom/ ath12k/ qca/ novatek/ (and aw882xx_acf.bin) from here
#   DEBS_FROM=DIR                   install the tb323fu-*.deb packages found here
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf,
#                                   emergency-key.conf when present (device-specific,
#                                   never put them in git)
#   DEV_ACCESS=0                    1 = developer access: usb0 gadget network
#                                   (192.168.7.2/24), root autologin on ttyGS0 and tty1,
#                                   sshd root login allowed
#   BALDUR_DEV_PASSWORD=            dev-only password for root and the user; without it
#                                   root is locked and the user has no password
#   DEV_USER=                       create this user (sudo); with DESKTOP=gnome it is
#                                   also logged in automatically
# Needs: debootstrap, ubuntu-keyring (ubuntu-archive-keyring.gpg). Re-running is
# safe: an existing base system is not bootstrapped again.
set -eu
T=${1:?usage: build-rootfs.sh TARGET_DIR [RELEASE]}
REL=${2:-resolute}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-tb323fu-ubuntu}
HOSTNAME_NEW=${HOSTNAME_NEW:-tb323fu-ubuntu}
DESKTOP=${DESKTOP:-gnome}
MIRROR=${MIRROR:-http://ports.ubuntu.com/ubuntu-ports}
MODULES_FROM=${MODULES_FROM:-/lib/modules/$(uname -r)}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
CONFIG_FROM=${CONFIG_FROM:-/etc/tb323fu}
DEV_ACCESS=${DEV_ACCESS:-0}
DEV_USER=${DEV_USER:-}
KEYRING=/usr/share/keyrings/ubuntu-archive-keyring.gpg
NICE="nice -n 19 ionice -c3"
say() { printf '== %s\n' "$*"; }

mountpoint -q "$T" || { echo "$T is not a mount point"; exit 1; }
[ "$(uname -m)" = aarch64 ] || { echo "run this on an arm64 host"; exit 1; }

# 1. base system
if [ ! -e "$T/etc/os-release" ]; then
	S=/usr/share/debootstrap/scripts
	[ -e $S/$REL ] || ln -s gutsy $S/$REL     # newer debootstrap knows it; older ones don't
	say "debootstrap $REL"
	$NICE debootstrap --arch=arm64 --keyring=$KEYRING --components=main,universe \
		--include=ca-certificates "$REL" "$T" "$MIRROR"
fi

# chroot plumbing (undone on exit)
for m in proc sys dev dev/pts run; do mountpoint -q "$T/$m" || mount --bind /$m "$T/$m"; done
trap 'for m in run dev/pts dev sys proc; do umount -l "$T/$m" 2>/dev/null || true; done' EXIT
# a real resolv.conf while building (the target's is a systemd-resolved link)
rm -f "$T/etc/resolv.conf"; cp -L /etc/resolv.conf "$T/etc/resolv.conf"
ch() { chroot "$T" /usr/bin/env DEBIAN_FRONTEND=noninteractive LC_ALL=C.UTF-8 "$@"; }

# 2. archive: release + updates + security, main/universe; no snaps
cat > "$T/etc/apt/sources.list.d/ubuntu.sources" <<EOF
Types: deb
URIs: $MIRROR
Suites: $REL $REL-updates $REL-security
Components: main universe restricted multiverse
Signed-By: $KEYRING
EOF
[ -e "$T/etc/apt/sources.list" ] && : > "$T/etc/apt/sources.list"
mkdir -p "$T/etc/apt/preferences.d"
printf 'Package: snapd\nPin: release a=*\nPin-Priority: -10\n' > "$T/etc/apt/preferences.d/no-snapd"
say "apt update + upgrade"
ch $NICE apt-get update -q
ch $NICE apt-get -y -q dist-upgrade

# 3. packages
base="linux-firmware- systemd-resolved network-manager openssh-server sudo bluez pipewire \
	pipewire-pulse wireplumber alsa-ucm-conf alsa-utils iio-sensor-proxy dbus-user-session \
	polkitd rmtfs tqftpserv qrtr-tools rfkill locales tzdata vim-tiny less \n	wpasupplicant wireless-regdb swh-plugins"
case $DESKTOP in
gnome) desk="gdm3 gnome-shell gnome-session gnome-control-center gnome-terminal nautilus \
	gnome-text-editor gnome-shell-extension-prefs gnome-initial-setup- mesa-vulkan-drivers \
	libgl1-mesa-dri plymouth plymouth-theme-spinner fonts-noto-core" ;;
*) desk="" ;;
esac
say "installing packages"
# shellcheck disable=SC2086
ch $NICE apt-get -y -q --no-install-recommends install $(echo $base $desk | tr ' ' '\n' | grep -v -- '-$')
ch apt-get -y -q purge snapd 2>/dev/null || true


# rmtfs.service runs `rmtfs -r -P -s`, and -s STARTS THE MODEM. Started at boot
# the modem crashed about a minute later (watchdog, cause not yet known -- the
# OEM image was present) and a modem crash resets the whole SoC (900E). The
# modem has no use on this Wi-Fi tablet: mask the services so nothing pulls
# them in.
ch systemctl mask rmtfs.service tqftpserv.service >/dev/null 2>&1 || true
# Mobian-derived helpers pulled in by hexagonrpcd / qcom-phone-utils: droid-juicer
# (extracts firmware from Android partitions) waits forever and holds the boot;
# qbootctl marks Android A/B slots and fails here. Neither applies to this setup.
ch systemctl mask droid-juicer.service qbootctl.service >/dev/null 2>&1 || true

# 4. kernel modules and firmware of the kernel that boots it
if [ -d "$MODULES_FROM" ]; then
	v=$(basename "$MODULES_FROM")
	say "modules $v"
	mkdir -p "$T/lib/modules"
	rm -rf "$T/lib/modules/$v"; cp -a "$MODULES_FROM" "$T/lib/modules/$v"
	ch depmod -a "$v"
fi
for d in qcom ath12k qca novatek; do   # novatek: the touch controller firmware
	[ -d "$FIRMWARE_FROM/$d" ] || continue
	mkdir -p "$T/lib/firmware/$d"; cp -a "$FIRMWARE_FROM/$d/." "$T/lib/firmware/$d/"
done
[ -e "$FIRMWARE_FROM/aw882xx_acf.bin" ] && cp -a "$FIRMWARE_FROM/aw882xx_acf.bin" "$T/lib/firmware/"

# 5. this device's settings (never from git) and our packages
mkdir -p "$T/etc/tb323fu"
for f in bt-address android-boot.sha256 audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && [ ! -e "$T/etc/tb323fu/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/$f"
done
if [ -n "${DEBS_FROM:-}" ] && ls "$DEBS_FROM"/tb323fu-*.deb >/dev/null 2>&1; then
	say "tb323fu packages"
	mkdir -p "$T/tmp/tb323fu-debs"; cp "$DEBS_FROM"/tb323fu-*.deb "$T/tmp/tb323fu-debs/"
	[ "$DESKTOP" = gnome ] || rm -f "$T"/tmp/tb323fu-debs/tb323fu-helper-gnome_*.deb "$T"/tmp/tb323fu-debs/tb323fu-settings_*.deb
	# keep the device settings copied above over the packages' default conffiles
	ch sh -c 'apt-get -y -q -o Dpkg::Options::=--force-confold install /tmp/tb323fu-debs/*.deb'
	rm -rf "$T/tmp/tb323fu-debs"
fi

# 6. system configuration
echo "$HOSTNAME_NEW" > "$T/etc/hostname"
printf '127.0.0.1\tlocalhost\n127.0.1.1\t%s\n' "$HOSTNAME_NEW" > "$T/etc/hosts"
printf 'PARTLABEL=%s\t/\text4\tdefaults,noatime\t0\t1\n' "$ROOT_PARTLABEL" > "$T/etc/fstab"
ch sh -c 'sed -i "s/^# *en_US.UTF-8/en_US.UTF-8/" /etc/locale.gen && locale-gen >/dev/null'
mkdir -p "$T/etc/NetworkManager/conf.d"
printf '[keyfile]\nunmanaged-devices=interface-name:usb0\n' > "$T/etc/NetworkManager/conf.d/10-tb323fu-usb0.conf"
mkdir -p "$T/etc/systemd/system.conf.d"
printf '[Manager]\nRuntimeWatchdogSec=30\n' > "$T/etc/systemd/system.conf.d/watchdog.conf"

if [ -n "$DEV_USER" ]; then
	ch id "$DEV_USER" >/dev/null 2>&1 || ch useradd -m -s /bin/bash -G sudo,video,audio,input,render "$DEV_USER"
fi
if [ -n "${BALDUR_DEV_PASSWORD:-}" ]; then
	printf 'root:%s\n' "$BALDUR_DEV_PASSWORD" | ch chpasswd
	[ -n "$DEV_USER" ] && printf '%s:%s\n' "$DEV_USER" "$BALDUR_DEV_PASSWORD" | ch chpasswd
else
	ch passwd -l root >/dev/null
fi

if [ "$DESKTOP" = gnome ]; then
	mkdir -p "$T/etc/gdm3/PostLogin"
	printf '#!/bin/sh\n# automatic login never tells plymouth to quit (see userspace/desktop/gnome)\nplymouth quit --retain-splash 2>/dev/null\nexit 0\n' > "$T/etc/gdm3/PostLogin/Default"
	chmod 755 "$T/etc/gdm3/PostLogin/Default"
	if [ -n "$DEV_USER" ]; then
		c=$T/etc/gdm3/custom.conf
		[ -e "$c" ] || printf '[daemon]\n' > "$c"
		grep -q '^AutomaticLoginEnable' "$c" || sed -i "s/^\[daemon\]/[daemon]\nAutomaticLoginEnable=true\nAutomaticLogin=$DEV_USER/" "$c"
	fi
fi

if [ "$DEV_ACCESS" = 1 ]; then
	say "developer access (usb0, ttyGS0/tty1 autologin, ssh root)"
	mkdir -p "$T/etc/systemd/network"
	printf '[Match]\nName=usb0\n\n[Network]\nAddress=192.168.7.2/24\nDNS=1.1.1.1\n\n[Route]\nGateway=192.168.7.1\nMetric=1024\n' > "$T/etc/systemd/network/50-usb0.network"
	ch systemctl enable systemd-networkd >/dev/null 2>&1
	for g in serial-getty@ttyGS0 getty@tty1; do
		mkdir -p "$T/etc/systemd/system/$g.service.d"
	done
	printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin root --keep-baud 115200,57600,38400,9600 - $TERM\nTimeoutStopSec=5\n' > "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/autologin.conf"
	printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin root --noclear - $TERM\n' > "$T/etc/systemd/system/getty@tty1.service.d/autologin.conf"
	ch systemctl enable serial-getty@ttyGS0.service >/dev/null 2>&1
	mkdir -p "$T/etc/ssh/sshd_config.d"
	printf 'PermitRootLogin yes\n' > "$T/etc/ssh/sshd_config.d/10-tb323fu-dev.conf"
	ch systemctl enable ssh.service >/dev/null 2>&1   # Ubuntu leaves openssh-server disabled
	# the PC's key, if this host has one for root
	[ -e /root/.ssh/authorized_keys ] && { mkdir -p "$T/root/.ssh"; cp /root/.ssh/authorized_keys "$T/root/.ssh/"; chmod 700 "$T/root/.ssh"; }
	# GNOME suspends after 15 min idle, and s2idle turns USB and Wi-Fi off: the
	# tablet vanishes from the PC. Never suspend on idle (a dconf system default;
	# a user's own setting still wins).
	mkdir -p "$T/etc/dconf/profile" "$T/etc/dconf/db/local.d"
	[ -e "$T/etc/dconf/profile/user" ] || printf 'user-db:user\n' > "$T/etc/dconf/profile/user"
	grep -qx 'system-db:local' "$T/etc/dconf/profile/user" || echo 'system-db:local' >> "$T/etc/dconf/profile/user"
	printf "[org/gnome/settings-daemon/plugins/power]\nsleep-inactive-ac-type='nothing'\nsleep-inactive-battery-type='nothing'\n" \
		> "$T/etc/dconf/db/local.d/00-tb323fu-dev"
	ch dconf update 2>/dev/null || true
fi

ch apt-get -y -q clean
ln -sf ../run/systemd/resolve/stub-resolv.conf "$T/etc/resolv.conf"
say "done: $(du -sh "$T" 2>/dev/null | cut -f1) in $T ($REL, desktop $DESKTOP)"
