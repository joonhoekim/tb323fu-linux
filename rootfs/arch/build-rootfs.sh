#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- an Arch Linux ARM (aarch64) root filesystem for the
# TB323FU, built natively on an arm64 host (for example the tablet itself
# running Linux) into a mounted, empty ext4 partition.
#
#   sh build-rootfs.sh TARGET_DIR
#
# Environment (all optional; same names as rootfs/ubuntu/build-rootfs.sh):
#   ROOT_PARTLABEL=tb323fu-arch     GPT name of the target partition (fstab)
#   HOSTNAME_NEW=tb323fu-arch       hostname of the new system
#   DESKTOP=gnome                   gnome (minimal GNOME + GDM) or none
#   TARBALL_URL=http://os.archlinuxarm.org/os/ArchLinuxARM-aarch64-latest.tar.gz
#   MODULES_FROM=                   empty (default): none -- the boot image carries its
#                                   modules and its initramfs mounts them on
#                                   /lib/modules/<release> (shared modules). A directory
#                                   (lib/modules/<release>): copy it in, for a root that
#                                   opts out ("own" in /etc/tb323fu/modules)
#   FIRMWARE_FROM=/lib/firmware     copy qcom/ ath12k/ qca/ novatek/ aw882xx_acf.bin from here
#   PKGS_FROM=DIR                   install the tb323fu-*.pkg.tar.* packages found here
#                                   (build them with packaging/arch/PKGBUILD)
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf,
#                                   emergency-key.conf when present (device-specific,
#                                   never put them in git)
#   DEV_SSH_KEYS=                   with DEV_ACCESS=1: an authorized_keys file installed for root
#   HEXAGONRPCD_FROM=               a root filesystem with hexagonrpcd installed (e.g. the running
#                                   Debian, /): hexagonrpcd is not packaged for Arch; its binary,
#                                   library, units, udev rule and the qcom sensor tree are copied
#   DEV_ACCESS=0                    1 = developer access: usb0 gadget network
#                                   (192.168.7.2/24), root autologin on ttyGS0,
#                                   sshd root login allowed
#   BALDUR_DEV_PASSWORD=            dev-only password for root and the user; without it
#                                   root is locked and the user has no password
#   DEV_USER=                       create this user (wheel/sudo); with DESKTOP=gnome it
#                                   is also logged in automatically
#
# The Arch Linux ARM kernel (linux-aarch64) and mkinitcpio are removed: this
# device boots its own kernel and initramfs, which switch_root into /sbin/init
# of the partition. Needs: curl, bsdtar (libarchive-tools), gpg, chroot.
# Re-running is safe: an existing base system is not extracted again.
set -eu

T=${1:?usage: build-rootfs.sh TARGET_DIR}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-tb323fu-arch}
HOSTNAME_NEW=${HOSTNAME_NEW:-tb323fu-arch}
DESKTOP=${DESKTOP:-gnome}
TARBALL_URL=${TARBALL_URL:-http://os.archlinuxarm.org/os/ArchLinuxARM-aarch64-latest.tar.gz}
MODULES_FROM=${MODULES_FROM:-}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
PKGS_FROM=${PKGS_FROM:-}
CONFIG_FROM=${CONFIG_FROM:-/etc/tb323fu}
DEV_ACCESS=${DEV_ACCESS:-0}
DEV_SSH_KEYS=${DEV_SSH_KEYS:-}
HEXAGONRPCD_FROM=${HEXAGONRPCD_FROM:-}
BALDUR_DEV_PASSWORD=${BALDUR_DEV_PASSWORD:-}
DEV_USER=${DEV_USER:-}
# Arch Linux ARM Build System <builder@archlinuxarm.org>
ALARM_KEY=68B3537F39A313B3E574D06777193F152BDBE6A6
here=$(cd "$(dirname "$0")/../.." && pwd)

mountpoint -q "$T" || { echo "$T is not a mount point"; exit 1; }

# run a shell command inside the target (arch-chroot equivalent)
ch() {
	for m in proc sys dev run; do
		mountpoint -q "$T/$m" && continue
		case $m in
		proc) mount -t proc proc "$T/proc" ;;
		sys) mount --rbind /sys "$T/sys" ;;
		dev) mount --rbind /dev "$T/dev" ;;
		run) mount -t tmpfs tmpfs "$T/run" ;;
		esac
	done
	nice -n 19 chroot "$T" /usr/bin/env -i HOME=/root TERM=xterm \
		PATH=/usr/local/sbin:/usr/local/bin:/usr/bin LANG=C.UTF-8 /bin/bash -c "$*"
}
cleanup() {
	for m in run dev sys proc; do umount -R "$T/$m" 2>/dev/null || true; done
	# restore the image's resolv.conf (a systemd-resolved link)
	[ -e "$T/etc/resolv.conf.alarm" ] && mv -f "$T/etc/resolv.conf.alarm" "$T/etc/resolv.conf" || true
}
trap cleanup EXIT

# 1. base system from the official tarball (md5 + signature checked)
if [ ! -x "$T/usr/bin/pacman" ]; then
	w=$(mktemp -d)
	for s in "" .md5 .sig; do curl -fsSL -o "$w/alarm.tar.gz$s" "$TARBALL_URL$s"; done
	(cd "$w" && sed "s|  .*|  alarm.tar.gz|" alarm.tar.gz.md5 | md5sum -c -)
	export GNUPGHOME="$w/gnupg"; mkdir -m700 "$GNUPGHOME"
	gpg --batch --keyserver hkps://keyserver.ubuntu.com --recv-keys $ALARM_KEY
	gpg --batch --verify "$w/alarm.tar.gz.sig" "$w/alarm.tar.gz"
	nice -n 19 bsdtar -xpf "$w/alarm.tar.gz" -C "$T"
	rm -rf "$w"
fi
[ -e "$T/etc/resolv.conf.alarm" ] || mv "$T/etc/resolv.conf" "$T/etc/resolv.conf.alarm"
cp -L /etc/resolv.conf "$T/etc/resolv.conf"

# 2. keyring, drop the distribution kernel, update, packages
ch "pacman-key --init >/dev/null && pacman-key --populate archlinuxarm >/dev/null"
ch "pacman -Q linux-aarch64 >/dev/null 2>&1 && pacman -Rns --noconfirm linux-aarch64 mkinitcpio mkinitcpio-busybox || true"
ch "pacman -Syu --noconfirm"
pk="networkmanager pipewire pipewire-pulse pipewire-alsa wireplumber bluez bluez-utils
    iio-sensor-proxy libssc openssh sudo mesa vulkan-freedreno alsa-ucm-conf alsa-utils
    polkit dbus qrtr-git rmtfs-git swh-plugins wireless-regdb"
[ "$DESKTOP" = gnome ] && pk="$pk gnome-shell gdm gnome-control-center gnome-terminal nautilus
    gnome-settings-daemon gnome-session gnome-keyring xdg-user-dirs-gtk libadwaita gtk4"
ch "pacman -S --noconfirm --needed $(echo $pk)"
ch "systemctl mask rmtfs tqftpserv >/dev/null 2>&1 || true"   # rmtfs -s starts the modem; a modem crash resets the SoC

# 3. kernel modules and firmware of the device
if [ -n "$MODULES_FROM" ]; then   # own mode only; normally the boot image's modules are mounted
	k=$(basename "$MODULES_FROM")
	mkdir -p "$T/usr/lib/modules"
	cp -a "$MODULES_FROM" "$T/usr/lib/modules/"
	ch "depmod $k"
fi
( cd "$FIRMWARE_FROM" && tar cf - $(ls -d ath12k qcom qca novatek aw882xx_acf.bin 2>/dev/null) ) |
	tar xpf - -C "$T/usr/lib/firmware"

# 4. system configuration
echo "PARTLABEL=$ROOT_PARTLABEL / ext4 defaults,noatime 0 1" > "$T/etc/fstab"
echo "$HOSTNAME_NEW" > "$T/etc/hostname"
printf '127.0.0.1 localhost\n::1 localhost\n127.0.1.1 %s\n' "$HOSTNAME_NEW" > "$T/etc/hosts"
sed -i 's/^#en_US.UTF-8/en_US.UTF-8/' "$T/etc/locale.gen"
ch "locale-gen >/dev/null"; echo LANG=en_US.UTF-8 > "$T/etc/locale.conf"
mkdir -p "$T/etc/tb323fu"
for f in bt-address android-boot.sha256 audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/"
done
ch "systemctl mask bootmac-bluetooth.service >/dev/null 2>&1 || true"

# 5. users
ch "id alarm >/dev/null 2>&1 && userdel -r alarm 2>/dev/null || true"   # image default user
if [ -n "$DEV_USER" ]; then
	ch "id $DEV_USER >/dev/null 2>&1 || useradd -m -G wheel,video,audio,input -s /bin/bash $DEV_USER"
	echo "%wheel ALL=(ALL:ALL) ALL" > "$T/etc/sudoers.d/10-wheel"; chmod 440 "$T/etc/sudoers.d/10-wheel"
fi
if [ -n "$BALDUR_DEV_PASSWORD" ]; then
	echo "root:$BALDUR_DEV_PASSWORD" | ch chpasswd
	[ -n "$DEV_USER" ] && echo "$DEV_USER:$BALDUR_DEV_PASSWORD" | ch chpasswd
else
	ch "passwd -l root >/dev/null"
fi

# 6. desktop
if [ "$DESKTOP" = gnome ]; then
	ch "systemctl enable gdm >/dev/null"
	if [ -n "$DEV_USER" ]; then
		printf '[daemon]\nAutomaticLoginEnable=True\nAutomaticLogin=%s\n' "$DEV_USER" > "$T/etc/gdm/custom.conf"
		# automatic login never tells plymouth to quit (see userspace/desktop/gnome)
		install -Dm755 "$here/userspace/desktop/gnome/gdm/PostLogin-Default" "$T/etc/gdm/PostLogin/Default"
	fi
fi
ch "systemctl enable NetworkManager bluetooth sshd >/dev/null"

# 7. developer access over the USB cable (the initramfs creates the gadget)
if [ "$DEV_ACCESS" = 1 ]; then
	mkdir -p "$T/etc/systemd/network" "$T/etc/NetworkManager/conf.d" \
		"$T/etc/systemd/system/serial-getty@ttyGS0.service.d" "$T/etc/ssh/sshd_config.d"
	printf '[Match]\nName=usb0\n\n[Network]\nAddress=192.168.7.2/24\nDNS=1.1.1.1\n\n[Route]\nGateway=192.168.7.1\nMetric=1024\n' \
		> "$T/etc/systemd/network/50-usb0.network"
	printf '[keyfile]\nunmanaged-devices=interface-name:usb0\n' > "$T/etc/NetworkManager/conf.d/tb323fu-usb0.conf"
	printf '[Service]\nExecStart=\nExecStart=-/usr/bin/agetty --autologin root --keep-baud 115200,57600,38400,9600 - $TERM\n' \
		> "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/autologin.conf"
	printf '[Service]\nTimeoutStopSec=5\n' > "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/stop-timeout.conf"
	echo "PermitRootLogin yes" > "$T/etc/ssh/sshd_config.d/10-tb323fu-dev.conf"
	if [ -n "$DEV_SSH_KEYS" ] && [ -f "$DEV_SSH_KEYS" ]; then
		install -d -m 700 "$T/root/.ssh"; install -m 600 "$DEV_SSH_KEYS" "$T/root/.ssh/authorized_keys"
	fi
	ch "systemctl enable systemd-networkd serial-getty@ttyGS0.service >/dev/null"
	# GNOME suspends after 15 min idle, and s2idle turns USB and Wi-Fi off: the
	# tablet vanishes from the PC. Never suspend on idle (a dconf system default;
	# a user's own setting still wins).
	mkdir -p "$T/etc/dconf/profile" "$T/etc/dconf/db/local.d"
	[ -e "$T/etc/dconf/profile/user" ] || printf 'user-db:user\n' > "$T/etc/dconf/profile/user"
	grep -qx 'system-db:local' "$T/etc/dconf/profile/user" || echo 'system-db:local' >> "$T/etc/dconf/profile/user"
	printf "[org/gnome/settings-daemon/plugins/power]\nsleep-inactive-ac-type='nothing'\nsleep-inactive-battery-type='nothing'\n" \
		> "$T/etc/dconf/db/local.d/00-tb323fu-dev"
	ch "dconf update 2>/dev/null || true"
fi

# 7b. sensors: hexagonrpcd (SSC over fastrpc) is not packaged for Arch Linux ARM
if [ -n "$HEXAGONRPCD_FROM" ]; then
	H=$HEXAGONRPCD_FROM
	for f in usr/bin/hexagonrpcd usr/libexec/hexagonrpc usr/lib/systemd/system/hexagonrpcd.service 		usr/lib/systemd/system/hexagonrpcd-*.service usr/lib/udev/rules.d/60-hexagonrpcd.rules usr/share/qcom; do
		for g in $H/$f; do [ -e "$g" ] || continue; mkdir -p "$T/$(dirname "${g#$H/}")"; cp -a "$g" "$T/${g#$H/}"; done
	done
	for l in "$H"/usr/lib/*/libhexagonrpc* "$H"/usr/lib/libhexagonrpc*; do [ -e "$l" ] && cp -a "$l" "$T/usr/lib/"; done
	# the service runs as user fastrpc and the udev rule gives /dev/fastrpc-* to group fastrpc
	mkdir -p "$T/etc/sysusers.d"
	printf 'u fastrpc - "FastRPC (Hexagon DSP sensors)" /var/lib/fastrpc
' > "$T/etc/sysusers.d/hexagonrpcd.conf"
	ch "systemd-sysusers >/dev/null && systemctl enable hexagonrpcd >/dev/null 2>&1 || true"
fi

# 8. platform files + helper (packages built from packaging/arch/PKGBUILD)
if [ -n "$PKGS_FROM" ]; then
	mkdir -p "$T/var/cache/tb323fu-pkgs"
	cp "$PKGS_FROM"/tb323fu-*.pkg.tar.* "$T/var/cache/tb323fu-pkgs/"
	ch "pacman -U --noconfirm --needed /var/cache/tb323fu-pkgs/*.pkg.tar.*"
fi
sync
echo "done: $T ($(du -sh "$T" 2>/dev/null | cut -f1))"
