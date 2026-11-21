#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- a postmarketOS (Nura) root filesystem with systemd and a
# postmarketOS UI (Phosh by default) for the TB323FU, built on an arm64 host (for example the tablet itself)
# or through qemu-user on another PC, into a mounted, empty ext4 partition.
#
#   sh build-rootfs.sh TARGET_DIR
#
# Environment (all optional; same names as the other builders in rootfs/):
#   ROOT_PARTLABEL=tb323fu-pmos     GPT name of the target partition (fstab)
#   HOSTNAME_NEW=tb323fu-pmos       hostname of the new system
#   DESKTOP=phosh                   phosh, plasma-mobile, gnome-mobile or none
#   CHANNEL=v26.06                  postmarketOS release, or edge
#   MIRROR=https://mirror.nura.eco/postmarketos
#   ALPINE_MIRROR=https://dl-cdn.alpinelinux.org/alpine
#   FIRMWARE_FROM=/lib/firmware     copy qcom/ ath12k/ qca/ novatek/ aw882xx_acf.bin from here
#   TIMEZONE=                       e.g. Europe/Berlin; default: the build host's, else UTC
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf,
#                                   emergency-key.conf when present (device-specific,
#                                   never put them in git)
#   DEV_ACCESS=0                    1 = developer access: usb0 gadget network
#                                   (192.168.7.2/24), root autologin on ttyGS0,
#                                   sshd root login allowed
#   DEV_SSH_KEYS=                   with DEV_ACCESS=1: an authorized_keys file installed for root
#   BALDUR_DEV_PASSWORD=            dev-only password for root and the user; without it
#                                   root is locked and the user has no password
#   DEV_USER=                       create this user (wheel); with a desktop it is also
#                                   logged in automatically
#
# No device package and no kernel are installed: this device boots its own
# kernel and initramfs, which switch_root into /sbin/init of the partition.
# apk.static is checked against the keys in keys/ (as pmbootstrap does), apk
# checks every package against the same keys. The platform files and the helper
# are built from this repository inside the target (musl).
# Needs: curl, tar, openssl, chroot.
set -eu

T=${1:?usage: build-rootfs.sh TARGET_DIR}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-tb323fu-pmos}
HOSTNAME_NEW=${HOSTNAME_NEW:-tb323fu-pmos}
DESKTOP=${DESKTOP:-phosh}
CHANNEL=${CHANNEL:-v26.06}
MIRROR=${MIRROR:-https://mirror.nura.eco/postmarketos}
ALPINE_MIRROR=${ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
TIMEZONE=${TIMEZONE:-$(readlink /etc/localtime 2>/dev/null | sed -n "s|.*/zoneinfo/||p")}
CONFIG_FROM=${CONFIG_FROM:-/etc/tb323fu}
DEV_ACCESS=${DEV_ACCESS:-0}
DEV_SSH_KEYS=${DEV_SSH_KEYS:-}
BALDUR_DEV_PASSWORD=${BALDUR_DEV_PASSWORD:-}
DEV_USER=${DEV_USER:-}
SWH_VERSION=0.4.17
SWH_SHA256=d1b090feec4c5e8f9605334b47faaad72db7cc18fe91d792b9161a9e3b821ce7
here=$(cd "$(dirname "$0")/../.." && pwd)
keys=$here/rootfs/postmarketos/keys
say() { echo "== $*"; }

# channels.cfg of pmaports: the Alpine release under each postmarketOS release
case $CHANNEL in
edge) pmdir=main; alpdir=edge ;;
v26.06) pmdir=v26.06; alpdir=v3.24 ;;
*) echo "unknown CHANNEL $CHANNEL (edge, v26.06)"; exit 1 ;;
esac

# the postmarketOS UI packages; their -systemd parts come in by install_if
case $DESKTOP in
phosh) ui="postmarketos-ui-phosh gnome-console snapshot libcamera-gstreamer" ;;
plasma-mobile) ui="postmarketos-ui-plasma-mobile qmlkonsole plasma-camera" ;;
gnome-mobile) ui="postmarketos-ui-gnome-mobile gnome-console snapshot libcamera-gstreamer py3-gobject3" ;;
none) ui= ;;
*) echo "unknown DESKTOP $DESKTOP (phosh, plasma-mobile, gnome-mobile, none)"; exit 1 ;;
esac

mountpoint -q "$T" || { echo "$T is not a mount point"; exit 1; }
case $(uname -m) in
aarch64) ;;
*) b=/proc/sys/fs/binfmt_misc/qemu-aarch64
	{ grep -qx enabled $b && grep -q '^flags:.*F' $b; } 2>/dev/null ||
		{ echo "run this on an arm64 host, or install qemu-user-binfmt (arm64 programs through qemu)"; exit 1; } ;;
esac
for c in curl tar openssl chroot; do command -v $c >/dev/null || { echo "needs $c"; exit 1; }; done
get() { curl -fsSL --proto '=https' --proto-redir '=https' -o "$1" "$2"; }

ch() {
	for m in proc sys dev run; do
		mountpoint -q "$T/$m" && continue
		mkdir -p "$T/$m"
		case $m in
		proc) mount -t proc proc "$T/proc" ;;
		sys) mount --rbind /sys "$T/sys" && mount --make-rslave "$T/sys" ;;
		dev) mount --rbind /dev "$T/dev" && mount --make-rslave "$T/dev" ;;
		run) mount -t tmpfs tmpfs "$T/run" ;;
		esac
	done
	nice -n 19 chroot "$T" /usr/bin/env -i HOME=/root TERM=xterm \
		PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin LANG=C.UTF-8 /bin/sh -c "$*"
}
w=$(mktemp -d)
cleanup() {
	for m in run dev sys proc; do umount -R "$T/$m" 2>/dev/null || true; done
	rm -rf "$w"
}
trap cleanup EXIT

# 1. apk.static from Alpine main, its signature checked with openssl
say "apk.static ($CHANNEL: postmarketOS $pmdir, Alpine $alpdir)"
get "$w/APKINDEX.tar.gz" "$ALPINE_MIRROR/$alpdir/main/aarch64/APKINDEX.tar.gz"
ver=$(tar -xzOf "$w/APKINDEX.tar.gz" APKINDEX 2>/dev/null |
	awk 'BEGIN{RS="";FS="\n"} /(^|\n)P:apk-tools-static(\n|$)/ {for(i=1;i<=NF;i++) if($i ~ /^V:/) print substr($i,3)}')
[ -n "$ver" ] || { echo "no apk-tools-static in the Alpine index"; exit 1; }
get "$w/apk.apk" "$ALPINE_MIRROR/$alpdir/main/aarch64/apk-tools-static-$ver.apk"
mkdir "$w/apk"; tar -xzf "$w/apk.apk" -C "$w/apk" 2>/dev/null
sig=$(cd "$w/apk/sbin" && ls apk.static.SIGN.RSA.sha256.* 2>/dev/null | head -n1)
key=${sig#apk.static.SIGN.RSA.sha256.}
[ -n "$sig" ] && [ -f "$keys/$key" ] || { echo "apk.static: no signature by a known key ($sig)"; exit 1; }
openssl dgst -sha256 -verify "$keys/$key" -signature "$w/apk/sbin/$sig" "$w/apk/sbin/apk.static" >/dev/null ||
	{ echo "apk.static: bad signature"; exit 1; }
APK=$w/apk/sbin/apk.static
case $($APK --version 2>/dev/null) in "apk-tools $ver,"*) ;;
*) echo "apk.static reports another version than $ver"; exit 1 ;; esac

# 2. base system
mkdir -p "$T/etc/apk/keys"
cp "$keys"/*.rsa.pub "$T/etc/apk/keys/"
for SSL_CERT_FILE in /etc/ssl/certs/ca-certificates.crt /etc/ssl/cert.pem /etc/pki/tls/certs/ca-bundle.crt; do
	[ -e "$SSL_CERT_FILE" ] && break
done
export SSL_CERT_FILE
cat > "$T/etc/apk/repositories" <<EOF
$MIRROR/extra-repos/systemd/$pmdir
$MIRROR/$pmdir
$ALPINE_MIRROR/$alpdir/main
$ALPINE_MIRROR/$alpdir/community
EOF
pk="postmarketos-base postmarketos-base-systemd networkmanager networkmanager-wifi bluez bluez-btmgmt
    pipewire wireplumber pipewire-pulse pipewire-alsa alsa-ucm-conf alsa-utils
    pipewire-filter-graph pipewire-filter-graph-builtin pipewire-filter-graph-ladspa
    hexagonrpcd libssc iio-sensor-proxy feedbackd qrtr rmtfs mesa-dri-gallium mesa-vulkan-freedreno
    openssh-server sudo shadow wireless-regdb curl e2fsprogs-extra"
pk="$pk $ui"
say "packages"
# shellcheck disable=SC2086
$APK --root "$T" --arch aarch64 --initdb --update-cache add $(echo $pk)
# pmbootstrap starts from a /usr-merged root; postmarketos-base merges only on upgrade
[ -L "$T/bin" ] || ch "merge-usr"
[ -e "$T/etc/resolv.conf.tb323fu-build" ] || mv "$T/etc/resolv.conf" "$T/etc/resolv.conf.tb323fu-build" 2>/dev/null ||
	: > "$T/etc/resolv.conf.tb323fu-build"
rm -f "$T/etc/resolv.conf"; cp -L /etc/resolv.conf "$T/etc/resolv.conf"

# 3. firmware of the device
fw=$(cd "$FIRMWARE_FROM" && ls -d ath12k qcom qca novatek aw882xx_acf.bin 2>/dev/null) || true
[ -n "$fw" ] || { echo "no device firmware (qcom/, ath12k/, ...) in $FIRMWARE_FROM: see docs/install-manual.md step 1" >&2; exit 1; }
mkdir -p "$T/lib/firmware"
# shellcheck disable=SC2086
( cd "$FIRMWARE_FROM" && tar chf - $fw ) | tar xpf - -C "$T/lib/firmware"
tplg=qcom/kaanapali/LENOVO-TB323FU-tplg.bin
[ -e "$T/lib/firmware/$tplg" ] || install -Dm644 "$here/firmware/audio/${tplg##*/}" "$T/lib/firmware/$tplg"

# 4. platform files and helper, built from this repository inside the target
say "platform files and helper"
S=/var/tmp/tb323fu-src
rm -rf "$T$S"; mkdir -p "$T$S"
(cd "$here" && tar cf - --exclude=./helper/target --exclude=./helper/crates/tb323fu-settings/target --exclude=./site .) |
	tar xf - -C "$T$S"
mkdir -p "$T/etc/tb323fu"
for f in bt-address android-boot.sha256 audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && [ ! -e "$T/etc/tb323fu/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/$f"
done
chown -R 0:0 "$T/lib/firmware" "$T/etc/tb323fu"
rust="build-base cargo perl-xml-parser"; [ "$DESKTOP" != none ] && rust="$rust gtk4.0-dev libadwaita-dev"
ch "apk add --virtual .tb323fu-build $rust >/dev/null"
# the speaker protection filter's LADSPA plugins (swh-plugins is not packaged for Alpine)
get "$w/swh.tar.gz" "https://github.com/swh/ladspa/archive/refs/tags/v$SWH_VERSION.tar.gz"
echo "$SWH_SHA256  $w/swh.tar.gz" | sha256sum -c - >/dev/null || { echo "swh-plugins: checksum mismatch"; exit 1; }
tar xzf "$w/swh.tar.gz" -C "$T/var/tmp"
ch "cd /var/tmp/ladspa-$SWH_VERSION && echo '#define PACKAGE \"swh-plugins\"' > config.h && mkdir -p /usr/lib/ladspa &&
	perl makestub.pl sc4_1882.xml > sc4_1882.c &&
	gcc -O2 -fPIC -shared -I. -o /usr/lib/ladspa/sc4_1882.so sc4_1882.c util/db.c util/rms.c -lm &&
	perl makestub.pl fast_lookahead_limiter_1913.xml > fast_lookahead_limiter_1913.c &&
	gcc -O2 -fPIC -shared -I. -o /usr/lib/ladspa/fast_lookahead_limiter_1913.so fast_lookahead_limiter_1913.c -lm"
rm -rf "$T/var/tmp/ladspa-$SWH_VERSION"
ch "CC=gcc sh $S/userspace/platform/install.sh >/dev/null"
# Alpine's feedbackd rules are 72-feedbackd.rules, which the platform's 90-feedbackd.rules
# does not shadow: they would hand the LED ring and the torch to feedbackd's pattern trigger
ln -sf /dev/null "$T/etc/udev/rules.d/72-feedbackd.rules"
# Alpine's hexagonrpcd has per-DSP units for the non-secure ADSP node only; the
# platform's hexagonrpcd.service.d drop-in orders this one after the DSPs
mkdir -p "$T/etc/sysusers.d"
printf 'u fastrpc - "FastRPC (Hexagon DSP sensors)" /var/lib/fastrpc\n' > "$T/etc/sysusers.d/tb323fu-fastrpc.conf"
ch "systemd-sysusers >/dev/null"
cat > "$T/etc/systemd/system/hexagonrpcd.service" <<'EOF'
[Unit]
Description=Hexagon DSP sensors daemon (ADSP sensorspd)

[Service]
ExecStart=/usr/bin/hexagonrpcd -f /dev/fastrpc-adsp-secure -d adsp -s -R /var/lib/tb323fu/sensors
User=fastrpc
Group=fastrpc
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
ch "systemctl enable tb323fu-gen-ids tb323fu-btaddr tb323fu-dsp tb323fu-audio tb323fu-usb-port \
	tb323fu-emergency-key tb323fu-kernel-confirm tb323fu-zram hexagonrpcd >/dev/null 2>&1"
ch "systemctl --global enable tb323fu-speaker-gain >/dev/null 2>&1"
ch "systemctl mask rmtfs tqftpserv postmarketos-zram-swap >/dev/null 2>&1 || true"   # rmtfs starts the modem; zram is tb323fu-zram's
ch "export CARGO_HOME=/var/tmp/tb323fu-cargo; cd $S/helper && cargo build --release --locked -q"
ch "PREFIX=/usr LIBEXECDIR=/usr/libexec/tb323fu sh $S/helper/install.sh >/dev/null"
ch "systemctl enable tb323fu-helperd >/dev/null 2>&1"
if [ "$DESKTOP" != none ]; then
	ch "export CARGO_HOME=/var/tmp/tb323fu-cargo; cd $S/helper/crates/tb323fu-settings && cargo build --release --locked -q"
	ch "cd $S/helper/crates/tb323fu-settings && PREFIX=/usr sh ./install.sh >/dev/null"
fi
ch "apk del .tb323fu-build >/dev/null"
rm -rf "$T$S" "$T/var/tmp/tb323fu-cargo"

# 5. system configuration
echo "PARTLABEL=$ROOT_PARTLABEL / ext4 defaults,noatime 0 1" > "$T/etc/fstab"
echo "$HOSTNAME_NEW" > "$T/etc/hostname"
printf '127.0.0.1 localhost\n::1 localhost\n127.0.1.1 %s\n' "$HOSTNAME_NEW" > "$T/etc/hosts"
[ -e "$T/usr/share/zoneinfo/${TIMEZONE:-UTC}" ] && ln -sf "/usr/share/zoneinfo/${TIMEZONE:-UTC}" "$T/etc/localtime"
mkdir -p "$T/etc/NetworkManager/conf.d"
printf '[keyfile]\nunmanaged-devices=interface-name:usb0\n' > "$T/etc/NetworkManager/conf.d/10-tb323fu-usb0.conf"
# postmarketOS's USB tethering reads the device package's deviceinfo; the initramfs owns usb0 here
mkdir -p "$T/etc/NetworkManager/dispatcher.d"
ln -sf /dev/null "$T/etc/NetworkManager/dispatcher.d/50-tethering.sh"
ch "systemctl enable NetworkManager bluetooth sshd >/dev/null 2>&1"

# 6. users and desktop
if [ -n "$DEV_USER" ]; then
	g=wheel; for x in video audio input render feedbackd plugdev netdev; do ch "getent group $x >/dev/null" && g=$g,$x; done
	ch "id $DEV_USER >/dev/null 2>&1 || useradd -m -s /bin/sh -G $g $DEV_USER"
fi
if [ -n "$BALDUR_DEV_PASSWORD" ]; then
	printf 'root:%s\n' "$BALDUR_DEV_PASSWORD" | ch chpasswd
	[ -n "$DEV_USER" ] && printf '%s:%s\n' "$DEV_USER" "$BALDUR_DEV_PASSWORD" | ch chpasswd
else
	ch "passwd -l root >/dev/null"
fi
[ "$DESKTOP" != none ] && ch "systemctl set-default graphical.target >/dev/null"
# pmbootstrap's record of the user; plasma-mobile's generator runs its session as this user
[ -n "$DEV_USER" ] && echo "$DEV_USER" > "$T/etc/default_user"
if [ "$DESKTOP" = phosh ]; then
	ch "systemctl enable greetd >/dev/null 2>&1"
	c=$T/etc/phrog/greetd-config.toml   # greetd.service.d of greetd-phrog points greetd here
	if [ -n "$DEV_USER" ] && [ -e "$c" ] && ! grep -q '^\[initial_session\]' "$c"; then
		# DesktopNames of phosh.desktop; GNOME Settings refuses to start without GNOME in it
		printf '\n[initial_session]\ncommand = "env XDG_CURRENT_DESKTOP=Phosh:GNOME XDG_SESSION_DESKTOP=phosh phosh-session"\nuser = "%s"\n' \
			"$DEV_USER" >> "$c"
	fi
fi
if [ "$DESKTOP" = gnome-mobile ]; then
	ch "systemctl enable gdm >/dev/null 2>&1"
	# gnome-shell-mobile does not rotate the panel by itself on this tablet
	install -Dm755 "$here/userspace/desktop/gnome/autorotate/tb323fu-autorotate" "$T/usr/local/bin/tb323fu-autorotate"
	install -Dm644 "$here/userspace/desktop/gnome/autorotate/tb323fu-autorotate.service" \
		"$T/etc/systemd/user/tb323fu-autorotate.service"
	ch "systemctl --global enable tb323fu-autorotate >/dev/null 2>&1"
	if [ -n "$DEV_USER" ]; then
		mkdir -p "$T/etc/gdm"
		printf '[daemon]\nAutomaticLoginEnable=True\nAutomaticLogin=%s\n' "$DEV_USER" > "$T/etc/gdm/custom.conf"
		ch "su - $DEV_USER -c 'mkdir -p ~/.config && touch ~/.config/gnome-initial-setup-done'"
	fi
fi

# 7. developer access over the USB cable (the initramfs creates the gadget)
if [ "$DEV_ACCESS" = 1 ]; then
	say "developer access (usb0, ttyGS0 autologin, ssh root)"
	ch "apk add systemd-networkd >/dev/null"
	mkdir -p "$T/etc/systemd/network" "$T/etc/systemd/system/serial-getty@ttyGS0.service.d" "$T/etc/ssh/sshd_config.d"
	printf '[Match]\nName=usb0\n\n[Network]\nAddress=192.168.7.2/24\nDNS=1.1.1.1\n\n[Route]\nGateway=192.168.7.1\nMetric=1024\n' \
		> "$T/etc/systemd/network/50-usb0.network"
	printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin root --keep-baud 115200,57600,38400,9600 - $TERM\nTimeoutStopSec=5\n' \
		> "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/autologin.conf"
	echo "PermitRootLogin yes" > "$T/etc/ssh/sshd_config.d/10-tb323fu-dev.conf"
	if [ -n "$DEV_SSH_KEYS" ] && [ -f "$DEV_SSH_KEYS" ]; then
		install -d -m 700 "$T/root/.ssh"; install -m 600 "$DEV_SSH_KEYS" "$T/root/.ssh/authorized_keys"
	fi
	ch "systemctl enable systemd-networkd serial-getty@ttyGS0.service >/dev/null 2>&1"
fi

# 8. finish: first boot generates the machine-id; the system's own resolv.conf back
: > "$T/etc/machine-id"
rm -f "$T/etc/resolv.conf"
if [ -s "$T/etc/resolv.conf.tb323fu-build" ] || [ -L "$T/etc/resolv.conf.tb323fu-build" ]; then
	mv "$T/etc/resolv.conf.tb323fu-build" "$T/etc/resolv.conf"
else
	rm -f "$T/etc/resolv.conf.tb323fu-build"
fi
rm -rf "$T/var/cache/apk"/*
sync
say "done: $(du -sh "$T" 2>/dev/null | cut -f1) in $T (postmarketOS $CHANNEL, desktop $DESKTOP)"
