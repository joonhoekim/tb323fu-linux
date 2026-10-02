#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- a Fedora (aarch64) Workstation root filesystem for the
# TB323FU, with FEX (x86/x86-64 emulation) for Steam, built natively on an
# arm64 host (for example the tablet itself running Linux) into a mounted,
# empty ext4 partition. The host needs no dnf: the official Fedora container
# base image (OCI) is unpacked into the target and dnf runs in a chroot.
#
#   sh build-rootfs.sh TARGET_DIR [RELEASE]          # default RELEASE 44
#
# Environment (all optional; same names as rootfs/ubuntu and rootfs/arch):
#   ROOT_PARTLABEL=tb323fu-fedora   GPT name of the target partition (fstab)
#   HOSTNAME_NEW=tb323fu-fedora     hostname of the new system
#   DESKTOP=gnome                   gnome (Fedora Workstation environment) or none
#   FEX=1                           1 = fex-emu, its Fedora x86-64 RootFS and thunks, and the
#                                   Steam bootstrap staged in the DEV_USER home
#   TIMEZONE=UTC                    e.g. Asia/Seoul
#   PROXY=                          proxy for every download (host curl, dnf, cargo), e.g.
#                                   socks5h://127.0.0.1:10800 through `ssh -R 10800` from a PC
#   IMAGE_DIR=https://dl.fedoraproject.org/pub/fedora/linux/releases/$RELEASE/Container/aarch64/images
#   FEDORA_KEY_FPR=                 fingerprint of the key that signs the image CHECKSUM
#                                   (defaults to the Fedora 44 key for RELEASE=44)
#   MODULES_FROM=/lib/modules/$(uname -r)   kernel modules of the kernel that will boot it
#   FIRMWARE_FROM=/lib/firmware     copy qcom/ ath12k/ qca/ novatek/ aw882xx_acf.bin from here
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf,
#                                   emergency-key.conf when present (device-specific,
#                                   never put them in git)
#   HEXAGONRPCD_FROM=               a root filesystem with hexagonrpcd installed (e.g. the running
#                                   Debian, /): hexagonrpcd is not packaged for Fedora; its binary,
#                                   library, units, udev rule and the qcom sensor tree are copied
#   IIO_SSC_FROM=                   a root filesystem with an iio-sensor-proxy >= 3.8 built with
#                                   libssc in /usr/local (Fedora's 3.8 has no SSC backend and libssc
#                                   is not packaged): /usr/local/libexec/iio-sensor-proxy and
#                                   libssc.so.* are copied, with the platform's libssc unit drop-in
#   NM_CONNECTIONS_FROM=            copy these NetworkManager keyfiles (e.g. the host's Wi-Fi,
#                                   /etc/NetworkManager/system-connections) -- never from git
#   DEV_ACCESS=0                    1 = developer access: usb0 gadget network
#                                   (192.168.7.2/24), root autologin on ttyGS0,
#                                   sshd root login allowed
#   DEV_SSH_KEYS=                   with DEV_ACCESS=1: an authorized_keys file installed for root
#   BALDUR_DEV_PASSWORD=            dev-only password for root and the user; without it
#                                   root is locked and the user has no password
#   DEV_USER=                       create this user (wheel); with DESKTOP=gnome it is
#                                   also logged in automatically
#
# The platform files and the helper (tb323fu-helperd, tb323fu-ctl, tb323fu-settings,
# the GNOME extension) are built and installed from this repository inside the
# chroot (cargo; needs network). No Fedora kernel, grub, shim or linux-firmware is
# installed (dnf excludepkgs, kept for later updates): this device boots its own
# kernel and initramfs, which switch_root into /sbin/init of the partition.
# Needs: curl, tar, xz, gpg, jq, chroot. Re-running is safe: an existing base
# system is not unpacked again.
set -eu
T=${1:?usage: build-rootfs.sh TARGET_DIR [RELEASE]}
REL=${2:-44}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-tb323fu-fedora}
HOSTNAME_NEW=${HOSTNAME_NEW:-tb323fu-fedora}
DESKTOP=${DESKTOP:-gnome}
FEX=${FEX:-1}
TIMEZONE=${TIMEZONE:-UTC}
PROXY=${PROXY:-}
IMAGE_DIR=${IMAGE_DIR:-https://dl.fedoraproject.org/pub/fedora/linux/releases/$REL/Container/aarch64/images}
[ "$REL" = 44 ] && FEDORA_KEY_FPR=${FEDORA_KEY_FPR:-36F612DCF27F7D1A48A835E4DBFCF71C6D9F90A6}
FEDORA_KEY_FPR=${FEDORA_KEY_FPR:?set FEDORA_KEY_FPR for Fedora $REL}
MODULES_FROM=${MODULES_FROM:-/lib/modules/$(uname -r)}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
CONFIG_FROM=${CONFIG_FROM:-/etc/tb323fu}
HEXAGONRPCD_FROM=${HEXAGONRPCD_FROM:-}
IIO_SSC_FROM=${IIO_SSC_FROM:-}
NM_CONNECTIONS_FROM=${NM_CONNECTIONS_FROM:-}
DEV_ACCESS=${DEV_ACCESS:-0}
DEV_SSH_KEYS=${DEV_SSH_KEYS:-}
BALDUR_DEV_PASSWORD=${BALDUR_DEV_PASSWORD:-}
DEV_USER=${DEV_USER:-}
here=$(cd "$(dirname "$0")/../.." && pwd)
NICE="nice -n 19 ionice -c3"
say() { printf '== %s\n' "$*"; }

mountpoint -q "$T" || { echo "$T is not a mount point"; exit 1; }
[ "$(uname -m)" = aarch64 ] || { echo "run this on an arm64 host"; exit 1; }

# 1. base system: the official container base image, CHECKSUM signed by Fedora
if [ ! -x "$T/usr/bin/dnf5" ]; then
	w=$(mktemp -d)
	list=$(curl -fsSL ${PROXY:+-x $PROXY} "$IMAGE_DIR/")
	img=$(echo "$list" | grep -o 'Fedora-Container-Base-Generic-[0-9.-]*\.aarch64\.oci\.tar\.xz' | head -n1)
	sum=$(echo "$list" | grep -o 'Fedora-Container-[0-9.-]*-aarch64-CHECKSUM' | head -n1)
	say "base image $img"
	curl -fsSL ${PROXY:+-x $PROXY} -o "$w/$sum" "$IMAGE_DIR/$sum"
	curl -fsSL ${PROXY:+-x $PROXY} -o "$w/$img" "$IMAGE_DIR/$img"
	export GNUPGHOME="$w/gnupg"; mkdir -m700 "$GNUPGHOME"
	curl -fsSL ${PROXY:+-x $PROXY} https://fedoraproject.org/fedora.gpg | gpg --batch -q --import
	gpg --batch --status-fd 1 --verify "$w/$sum" 2>/dev/null | grep -q "VALIDSIG $FEDORA_KEY_FPR" ||
		{ echo "CHECKSUM signature is not from $FEDORA_KEY_FPR"; exit 1; }
	# only the signed part of the clearsigned CHECKSUM counts
	(cd "$w" && gpg --batch -q --decrypt "$sum" 2>/dev/null | grep "^SHA256 ($img) = " | sha256sum -c -)
	mkdir "$w/oci"; tar xJf "$w/$img" -C "$w/oci"
	man=$(jq -r '.manifests[0].digest' "$w/oci/index.json" | cut -d: -f2)
	for l in $(jq -r '.layers[].digest' "$w/oci/blobs/sha256/$man" | cut -d: -f2); do
		$NICE tar xzpf "$w/oci/blobs/sha256/$l" -C "$T" --numeric-owner --xattrs --xattrs-include='*'
	done
	rm -rf "$w"
fi

# chroot plumbing (undone on exit; processes left inside are killed first so
# nothing keeps the partition busy)
for m in proc sys dev dev/pts run; do
	mountpoint -q "$T/$m" && continue
	case $m in
	proc) mount -t proc proc "$T/proc" ;;
	run) mount -t tmpfs tmpfs "$T/run" ;;
	*) mount --bind "/$m" "$T/$m" ;;
	esac
done
cleanup() {
	for p in /proc/[0-9]*; do
		[ "$(readlink "$p/root" 2>/dev/null)" = "$T" ] && kill -9 "${p#/proc/}" 2>/dev/null
	done
	for m in run dev/pts dev sys proc; do umount "$T/$m" 2>/dev/null || umount -l "$T/$m" 2>/dev/null || true; done
}
trap cleanup EXIT
rm -f "$T/etc/resolv.conf"; cp -L /etc/resolv.conf "$T/etc/resolv.conf"
px=""; [ -n "$PROXY" ] && px="https_proxy=$PROXY http_proxy=$PROXY"
ch() { $NICE chroot "$T" /usr/bin/env -i HOME=/root TERM=xterm LANG=C.UTF-8 \
	$px PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin "$@"; }
# retried: a Wi-Fi drop in the middle of ~1700 downloads is not rare; what was
# downloaded stays in dnf's cache until the transaction succeeds
dnf() {
	for i in 1 2 3; do
		ch dnf -y --setopt=install_weak_deps=True ${PROXY:+--setopt=proxy=$PROXY} "$@" && return 0
		[ $i = 3 ] || { echo "dnf failed, retrying in 60 s"; sleep 60; }
	done
	return 1
}

# 2. dnf: documentation back on (the container image turns it off), no boot
# path packages -- ever, also on later updates
cat > "$T/etc/dnf/dnf.conf" <<'EOF'
# see `man dnf.conf` for defaults and possible options

[main]
# TB323FU: the kernel, its modules and firmware come from the device's boot
# image (tb323fu-linux); a distribution kernel, bootloader or linux-firmware
# would only take space or shadow the device firmware in /usr/lib/firmware.
excludepkgs=kernel,kernel-core,kernel-modules,kernel-modules-core,kernel-modules-extra,kernel-uki-virt,grub2-*,shim-*,linux-firmware,qcom-firmware,atheros-firmware,qcom-wwan-firmware,gnome-initial-setup,rmtfs,tqftpserv,qemu-user-static-x86
EOF
say "dnf upgrade"
dnf upgrade --refresh

# 3. packages
base="systemd systemd-networkd systemd-resolved NetworkManager NetworkManager-wifi wpa_supplicant
	wireless-regdb openssh-server sudo bluez /usr/bin/btmgmt pipewire pipewire-pulseaudio wireplumber
	ladspa-swh-plugins alsa-ucm alsa-utils iio-sensor-proxy mesa-dri-drivers mesa-vulkan-drivers
	vulkan-tools libgpiod-utils qrtr libcamera libcamera-ipa polkit kmod gcc
	glibc-langpack-en less vim-minimal passwd"
case $DESKTOP in
gnome) desk="@workstation-product-environment fedora-release-workstation gdm gnome-shell gnome-session
	gnome-control-center ptyxis nautilus gnome-extensions-app plymouth" ;;
*) desk="" ;;
esac
fexp=""; [ "$FEX" = 1 ] && fexp="fex-emu fex-emu-utils fex-emu-thunks fex-emu-rootfs-fedora erofs-fuse fuse3 steam-devices"
# libssc (copied, see IIO_SSC_FROM) links against these
ssc=""; [ -n "$IIO_SSC_FROM" ] && ssc="libqmi libqrtr-glib protobuf-c libmbim"
say "installing packages"
# shellcheck disable=SC2086
dnf install --allowerasing --skip-unavailable $(echo $base $desk $fexp $ssc)
# the container base image carries the "Container Image" identity (os-release)
[ "$DESKTOP" = gnome ] && dnf swap fedora-release-identity-container fedora-release-identity-workstation

# rmtfs `-s` STARTS THE MODEM: started at boot the modem crashed about a minute
# later (watchdog, cause not yet known) and a modem crash resets the whole SoC
# (900E). They are excluded above; mask them in case they come in anyway, and
# the Mobian helpers that hold the boot or poke the Android A/B slots.
ch systemctl mask rmtfs.service tqftpserv.service droid-juicer.service qbootctl.service \
	bootmac-bluetooth.service ModemManager.service >/dev/null 2>&1 || true
# Fedora's dist-alsa.conf: `install snd-pcm ... && modprobe snd-seq`. This
# kernel has no snd-seq, the install command fails, and udev's modprobe of the
# LPASS codec macros (which need snd-pcm) fails with it: no sound card at boot.
cat > "$T/etc/modprobe.d/tb323fu-no-snd-seq.conf" <<'EOM'
# tb323fu: this kernel has no snd-seq (CONFIG_SND_SEQUENCER); dist-alsa.conf's
# install rule then fails and snd-pcm dependents (LPASS codecs) do not load at boot.
install snd-pcm /sbin/modprobe --ignore-install snd-pcm
EOM
# Boot time (graphical.target 22.9 s -> 7.5 s):
# - iscsi.service orders itself After=network-online.target and Before=
#   remote-fs.target even when its condition skips it, so GDM (after
#   systemd-user-sessions, after remote-fs) waited for NetworkManager-wait-online.
# - plymouth: with automatic login Fedora's GDM never quits it (the PostLogin
#   script that works on Debian does not run here), plymouth-quit-wait held
#   multi-user.target for 26 s. The default here is the boot log, not a splash.
ch systemctl mask iscsi.service iscsid.service iscsiuio.service iscsid.socket iscsiuio.socket \
	plymouth-start.service plymouth-quit-wait.service plymouth-read-write.service >/dev/null 2>&1 || true
# Fedora's presets (what a first boot with an empty machine-id also applies)
ch systemctl preset-all >/dev/null 2>&1 || true
# firewalld's stock nftables ruleset needs nft_compat and friends: in the
# t24 kernel they are the separate netfilter module set
# (kernel/config/baldur-netfilter.fragment); without them firewalld fails at
# boot, so it is only kept when the copied modules have nft_compat. IPv6
# rpfilter needs NFT_FIB_IPV6, which needs a new Image -- off until then.
if find "$T/usr/lib/modules" -name 'nft_compat.ko*' | grep -q .; then
	if ! find "$T/usr/lib/modules" -name 'nft_fib_ipv6.ko*' | grep -q .; then
		sed -i 's/^IPv6_rpfilter=.*/IPv6_rpfilter=no/' "$T/etc/firewalld/firewalld.conf"
		grep -q '^IPv6_rpfilter=' "$T/etc/firewalld/firewalld.conf" || echo 'IPv6_rpfilter=no' >> "$T/etc/firewalld/firewalld.conf"
	fi
else
	ch systemctl disable firewalld.service >/dev/null 2>&1 || true
fi

# 4. kernel modules and firmware of the kernel that boots it
if [ -d "$MODULES_FROM" ]; then
	v=$(basename "$MODULES_FROM")
	say "modules $v"
	mkdir -p "$T/usr/lib/modules"
	rm -rf "$T/usr/lib/modules/$v"; cp -a "$MODULES_FROM" "$T/usr/lib/modules/$v"
	ch depmod -a "$v"
fi
mkdir -p "$T/usr/lib/firmware"
for d in qcom ath12k qca novatek; do   # novatek: the touch controller firmware
	[ -d "$FIRMWARE_FROM/$d" ] || continue
	mkdir -p "$T/usr/lib/firmware/$d"; cp -a "$FIRMWARE_FROM/$d/." "$T/usr/lib/firmware/$d/"
done
[ -e "$FIRMWARE_FROM/aw882xx_acf.bin" ] && cp -a "$FIRMWARE_FROM/aw882xx_acf.bin" "$T/usr/lib/firmware/"
for f in regulatory.db regulatory.db.p7s; do   # wireless-regdb normally provides these
	[ -e "$T/usr/lib/firmware/$f" ] || { [ -e "$FIRMWARE_FROM/$f" ] && cp -L "$FIRMWARE_FROM/$f" "$T/usr/lib/firmware/$f"; }
done

# 5. sensors: hexagonrpcd (SSC over fastrpc) and libssc are not packaged for Fedora
if [ -n "$HEXAGONRPCD_FROM" ]; then
	say "hexagonrpcd from $HEXAGONRPCD_FROM"
	H=$HEXAGONRPCD_FROM
	for f in usr/bin/hexagonrpcd usr/libexec/hexagonrpc usr/lib/systemd/system/hexagonrpcd.service \
		usr/lib/systemd/system/hexagonrpcd-*.service usr/lib/udev/rules.d/60-hexagonrpcd.rules usr/share/qcom; do
		for g in $H/$f; do [ -e "$g" ] || continue; mkdir -p "$T/$(dirname "${g#$H/}")"; cp -a "$g" "$T/${g#$H/}"; done
	done
	for l in "$H"/usr/lib/*/libhexagonrpc* "$H"/usr/lib/libhexagonrpc* "$H"/usr/lib64/libhexagonrpc*; do
		[ -e "$l" ] && cp -a "$l" "$T/usr/lib64/" || true
	done
	# the service runs as user fastrpc and the udev rule gives /dev/fastrpc-* to group fastrpc
	mkdir -p "$T/etc/sysusers.d"
	printf 'u fastrpc - "FastRPC (Hexagon DSP sensors)" /var/lib/fastrpc\n' > "$T/etc/sysusers.d/hexagonrpcd.conf"
	ch systemd-sysusers >/dev/null
	ch sh -c 'ldconfig; for u in hexagonrpcd hexagonrpcd-suspend hexagonrpcd-resume; do
		[ -e /usr/lib/systemd/system/$u.service ] && systemctl enable $u.service >/dev/null 2>&1; done; true'
fi
iio_opt=""
if [ -n "$IIO_SSC_FROM" ] && [ -x "$IIO_SSC_FROM/usr/local/libexec/iio-sensor-proxy" ]; then
	say "iio-sensor-proxy with libssc from $IIO_SSC_FROM"
	install -Dm755 "$IIO_SSC_FROM/usr/local/libexec/iio-sensor-proxy" "$T/usr/local/libexec/iio-sensor-proxy"
	mkdir -p "$T/usr/local/lib64"
	cp -a "$IIO_SSC_FROM"/usr/local/lib/*/libssc.so* "$IIO_SSC_FROM"/usr/local/lib*/libssc.so* "$T/usr/local/lib64/" 2>/dev/null || true
	echo /usr/local/lib64 > "$T/etc/ld.so.conf.d/tb323fu-local.conf"
	ch ldconfig
	iio_opt=debian-iio   # the drop-in running /usr/local/libexec/iio-sensor-proxy (not Debian-specific in content)
fi

# 6. platform files and helper, built from this repository inside the target
say "platform files and helper"
S=/var/tmp/tb323fu-src
rm -rf "$T$S"; mkdir -p "$T$S"
(cd "$here" && tar cf - --exclude=./helper/target --exclude=./helper/crates/tb323fu-settings/target .) | tar xf - -C "$T$S"
mkdir -p "$T/etc/tb323fu"
for f in bt-address android-boot.sha256 audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && [ ! -e "$T/etc/tb323fu/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/$f"
done
ch env OPTIONAL="$iio_opt" CC=gcc sh $S/userspace/platform/install.sh >/dev/null
ch systemctl enable tb323fu-gen-ids.service tb323fu-btaddr.service tb323fu-dsp.service \
	tb323fu-audio.service tb323fu-usb-port.service tb323fu-emergency-key.service >/dev/null 2>&1
ch systemctl --global enable tb323fu-speaker-gain.service >/dev/null 2>&1
# the proxy is started at boot and retried (the SSC sensors appear late), see the drop-in
[ -n "$iio_opt" ] && ch systemctl add-wants multi-user.target iio-sensor-proxy.service >/dev/null 2>&1
rust="cargo rust"; [ "$DESKTOP" = gnome ] && rust="$rust gtk4-devel libadwaita-devel"
# shellcheck disable=SC2086
dnf install $rust
ch env CARGO_HOME=/var/tmp/tb323fu-cargo sh -c "cd $S/helper && cargo build --release --locked -q"
ch env PREFIX=/usr LIBEXECDIR=/usr/libexec/tb323fu sh $S/helper/install.sh >/dev/null
ch systemctl enable tb323fu-helperd.service >/dev/null 2>&1
if [ "$DESKTOP" = gnome ]; then
	ch env CARGO_HOME=/var/tmp/tb323fu-cargo sh -c "cd $S/helper/crates/tb323fu-settings && cargo build --release --locked -q"
	ch env PREFIX=/usr sh -c "cd $S/helper/crates/tb323fu-settings && sh ./install.sh" >/dev/null
	uuid=tb323fu@joonhoekim.github.io
	install -Dm644 -t "$T/usr/share/gnome-shell/extensions/$uuid" \
		"$here/userspace/desktop/gnome/extension/$uuid/extension.js" "$here/userspace/desktop/gnome/extension/$uuid/metadata.json"
	install -Dm644 "$here/userspace/desktop/gnome/90_tb323fu-integer-scale.gschema.override" \
		"$T/usr/share/glib-2.0/schemas/90_tb323fu-integer-scale.gschema.override"
	ch glib-compile-schemas /usr/share/glib-2.0/schemas
	# the "Tablet" quick settings tile on for every user (a default, users can turn it off)
	mkdir -p "$T/etc/dconf/profile" "$T/etc/dconf/db/local.d"
	[ -e "$T/etc/dconf/profile/user" ] || printf 'user-db:user\nsystem-db:local\n' > "$T/etc/dconf/profile/user"
	grep -q '^system-db:local' "$T/etc/dconf/profile/user" || echo system-db:local >> "$T/etc/dconf/profile/user"
	printf "[org/gnome/shell]\nenabled-extensions=['%s']\n" "$uuid" > "$T/etc/dconf/db/local.d/10-tb323fu"
	ch dconf update
fi
rm -rf "$T$S" "$T/var/tmp/tb323fu-cargo"

# 7. system configuration
echo "$HOSTNAME_NEW" > "$T/etc/hostname"
printf '127.0.0.1\tlocalhost\n::1\tlocalhost\n127.0.1.1\t%s\n' "$HOSTNAME_NEW" > "$T/etc/hosts"
printf 'PARTLABEL=%s\t/\text4\tdefaults,noatime\t0\t1\n' "$ROOT_PARTLABEL" > "$T/etc/fstab"
echo LANG=en_US.UTF-8 > "$T/etc/locale.conf"
ln -sf "../usr/share/zoneinfo/$TIMEZONE" "$T/etc/localtime"
# this kernel has no SELinux (CONFIG_LSM without selinux); permissive keeps the
# unlabeled files harmless should a kernel with SELinux ever boot it
[ -e "$T/etc/selinux/config" ] && sed -i 's/^SELINUX=.*/SELINUX=permissive/' "$T/etc/selinux/config"
mkdir -p "$T/etc/NetworkManager/conf.d"
printf '[keyfile]\nunmanaged-devices=interface-name:usb0\n' > "$T/etc/NetworkManager/conf.d/10-tb323fu-usb0.conf"
if [ -n "$NM_CONNECTIONS_FROM" ] && ls "$NM_CONNECTIONS_FROM"/*.nmconnection >/dev/null 2>&1; then
	mkdir -p "$T/etc/NetworkManager/system-connections"
	install -m600 "$NM_CONNECTIONS_FROM"/*.nmconnection "$T/etc/NetworkManager/system-connections/"
fi
ch systemctl enable NetworkManager.service bluetooth.service sshd.service >/dev/null 2>&1

# 8. users and desktop
if [ -n "$DEV_USER" ]; then
	g=wheel,video,audio,input; ch getent group render >/dev/null && g=$g,render
	ch id "$DEV_USER" >/dev/null 2>&1 || ch useradd -m -s /bin/bash -G "$g" "$DEV_USER"
fi
if [ -n "$BALDUR_DEV_PASSWORD" ]; then
	printf 'root:%s\n' "$BALDUR_DEV_PASSWORD" | ch chpasswd
	[ -n "$DEV_USER" ] && printf '%s:%s\n' "$DEV_USER" "$BALDUR_DEV_PASSWORD" | ch chpasswd
else
	ch passwd -l root >/dev/null
fi
if [ "$DESKTOP" = gnome ]; then
	ch systemctl enable gdm.service >/dev/null 2>&1
	ch systemctl set-default graphical.target >/dev/null
	# automatic login never tells plymouth to quit (see userspace/desktop/gnome)
	install -Dm755 "$here/userspace/desktop/gnome/gdm/PostLogin-Default" "$T/etc/gdm/PostLogin/Default"
	if [ -n "$DEV_USER" ]; then
		c=$T/etc/gdm/custom.conf
		[ -e "$c" ] || printf '[daemon]\n' > "$c"
		grep -q '^AutomaticLoginEnable' "$c" || sed -i "s/^\[daemon\]/[daemon]\nAutomaticLoginEnable=True\nAutomaticLogin=$DEV_USER/" "$c"
		# gnome-initial-setup is excluded; also mark it done for the user
		ch su - "$DEV_USER" -c 'mkdir -p ~/.config && touch ~/.config/gnome-initial-setup-done'
	fi
fi

# 9. developer access over the USB cable (the initramfs creates the gadget)
if [ "$DEV_ACCESS" = 1 ]; then
	say "developer access (usb0, ttyGS0 autologin, ssh root)"
	mkdir -p "$T/etc/systemd/network" "$T/etc/systemd/system/serial-getty@ttyGS0.service.d" "$T/etc/ssh/sshd_config.d"
	printf '[Match]\nName=usb0\n\n[Network]\nAddress=192.168.7.2/24\nDNS=1.1.1.1\n\n[Route]\nGateway=192.168.7.1\nMetric=1024\n' \
		> "$T/etc/systemd/network/50-usb0.network"
	printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin root --keep-baud 115200,57600,38400,9600 - $TERM\nTimeoutStopSec=5\n' \
		> "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/autologin.conf"
	echo "PermitRootLogin yes" > "$T/etc/ssh/sshd_config.d/10-tb323fu-dev.conf"
	if [ -n "$DEV_SSH_KEYS" ] && [ -f "$DEV_SSH_KEYS" ]; then
		install -d -m 700 "$T/root/.ssh"; install -m 600 "$DEV_SSH_KEYS" "$T/root/.ssh/authorized_keys"
	fi
	ch systemctl enable systemd-networkd.service serial-getty@ttyGS0.service >/dev/null 2>&1
	# GNOME suspends after 15 min idle, and s2idle turns USB and Wi-Fi off: the
	# tablet vanishes from the PC. Never suspend on idle (a dconf system default;
	# a user's own setting still wins).
	mkdir -p "$T/etc/dconf/profile" "$T/etc/dconf/db/local.d"
	[ -e "$T/etc/dconf/profile/user" ] || printf 'user-db:user\n' > "$T/etc/dconf/profile/user"
	grep -qx 'system-db:local' "$T/etc/dconf/profile/user" || echo 'system-db:local' >> "$T/etc/dconf/profile/user"
	printf "[org/gnome/settings-daemon/plugins/power]\nsleep-inactive-ac-type='nothing'\nsleep-inactive-battery-type='nothing'\n" \
		> "$T/etc/dconf/db/local.d/00-tb323fu-dev"
	ch dconf update >/dev/null 2>&1 || true
fi

# 9b. FEX: use Fedora's x86-64 RootFS (fex-emu-rootfs-fedora, an EROFS image; this
# kernel has no EROFS, FEX mounts it with erofsfuse) unless configured otherwise.
# The page size is 4K, so no muvm is needed. qemu-user-static-x86 is excluded:
# its binfmt entry takes x86 binaries before FEX's does. Steam itself is not
# packaged for aarch64: its bootstrap is staged for the user, run it with
#   FEXBash -c "~/steam-launcher/steam -no-cef-sandbox"
# (steamwebhelper's sandboxed zygote dies under FEX: zygote_host_impl_linux
# "Check failed ... No such file or directory"; Steam also falls back to no
# sandbox by itself after that crash).
if [ "$FEX" = 1 ]; then
	c=$T/usr/share/fex-emu/Config.json   # FEX's global configuration
	[ -e "$c" ] || printf '{\n  "Config": {\n    "RootFS": "default.erofs"\n  }\n}\n' > "$c"
	if [ -n "$DEV_USER" ] && [ ! -d "$T/home/$DEV_USER/steam-launcher" ]; then
		curl -fsSL ${PROXY:+-x $PROXY} https://repo.steampowered.com/steam/archive/stable/steam_latest.tar.gz |
			ch su - "$DEV_USER" -c 'tar xzf - -C ~'
	fi
fi

# 10. finish: first boot generates the machine-id; resolv.conf back to resolved's stub
ch dnf clean all >/dev/null
: > "$T/etc/machine-id"
ln -sf ../run/systemd/resolve/stub-resolv.conf "$T/etc/resolv.conf"
sync
say "done: $(du -sh --exclude=proc --exclude=sys --exclude=dev --exclude=run "$T" 2>/dev/null | cut -f1) in $T (Fedora $REL, desktop $DESKTOP, FEX $FEX)"
