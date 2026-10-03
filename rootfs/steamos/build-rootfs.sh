#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- SteamOS (arm64) for the TB323FU, from the unofficial
# community port "SteamOS ARM for handhelds" (Valve's Steam Frame SteamOS,
# repackaged for Snapdragon handhelds), built on an arm64 Linux host (for
# example the tablet itself running Linux) into a mounted, empty ext4 partition.
#
#   sh build-rootfs.sh TARGET_DIR
#
# THIRD-PARTY IMAGE: this downloads a release image of
#   https://github.com/hashtagbasit/SteamOS-ARM-Handhelds
# (not affiliated with Valve; Valve's userspace and Steam client inside are
# Valve's). Only its root and home partitions are used: no kernel, device tree,
# bootloader (ROCKNIX ABL) or initramfs of it -- this device boots its own
# kernel and initramfs, which switch_root into /sbin/init of the partition.
# The image is a plain one-system image (BOOT vfat + root ext4 + home ext4, no
# A/B, no RAUC), so its root and home are copied into one ext4 partition.
#
# Environment (all optional; same names as the other rootfs/ builders):
#   ROOT_PARTLABEL=tb323fu-steamos  GPT name of the target partition (fstab)
#   HOSTNAME_NEW=tb323fu-steamos    hostname of the new system
#   RELEASE=v1.3-odin3-beta1        release tag of the port (SM8750 image: the
#                                   newest chip it supports, Adreno 830/840 Mesa)
#   IMAGE_NAME=steamos-arm-handhelds-sm8750-$RELEASE
#   PARTS="7z.001 7z.002 7z.003"    the split 7-Zip archive holding $IMAGE_NAME.img
#   SUMS_SHA256=90b67933...         sha256 of the release's SHA256SUMS file (pins
#                                   the release; the parts are checked against it)
#   WORK_DIR=/var/tmp/steamos-arm   download and unpack here (~25 GB for this release)
#   MODULES_FROM=/lib/modules/$(uname -r)   kernel modules of the kernel that will boot it
#                                   (with extra/ -- the speaker amplifier driver); used to
#                                   trim the image's modules-load.d lists. With a boot image
#                                   that carries its modules (shared modules) the copy is
#                                   hidden by the initramfs' mount of the same release
#   FIRMWARE_FROM=/lib/firmware     copy qcom/ ath12k/ qca/ novatek/ aw882xx_acf.bin from here
#   LADSPA_FROM=/usr/lib/ladspa     sc4_1882.so and fast_lookahead_limiter_1913.so (swh-plugins)
#                                   for the speaker protection filter; SteamOS has none
#   HELPER_FROM=                    a helper/ tree with target/release built (tb323fu-helperd,
#                                   tb323fu-ctl; glibc <= the image's 2.39), installed with
#                                   PREFIX=/usr; without it no helper
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf,
#                                   emergency-key.conf when present (device-specific,
#                                   never put them in git)
#   NM_CONNECTIONS_FROM=            copy these NetworkManager keyfiles (e.g. the host's Wi-Fi,
#                                   /etc/NetworkManager/system-connections) -- never from git
#   ORIENTATION=right               gamescope --force-orientation for the portrait panel
#                                   (left|right|normal|upsidedown); Desktop Mode (KWin)
#                                   guesses Rotated270 itself
#   TIMEZONE=                       e.g. Asia/Seoul (the image says America/Los_Angeles)
#   DEV_ACCESS=0                    1 = developer access: usb0 gadget network
#                                   (192.168.7.2/24), root autologin on ttyGS0,
#                                   sshd root login allowed
#   DEV_SSH_KEYS=                   with DEV_ACCESS=1: an authorized_keys file installed for root
#   BALDUR_DEV_PASSWORD=            dev-only password for root; without it root stays locked
#
# The image's user is `steamos` (uid 1000, no password, passwordless sudo); SDDM
# logs it in automatically into Gaming Mode (gamescope + Steam), Desktop Mode
# is KDE Plasma. Needs: curl, sha256sum, 7z (7zip), losetup, rsync, chroot.
set -eu

T=${1:?usage: build-rootfs.sh TARGET_DIR}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-tb323fu-steamos}
HOSTNAME_NEW=${HOSTNAME_NEW:-tb323fu-steamos}
RELEASE=${RELEASE:-v1.3-odin3-beta1}
IMAGE_NAME=${IMAGE_NAME:-steamos-arm-handhelds-sm8750-$RELEASE}
PARTS=${PARTS:-7z.001 7z.002 7z.003}
[ "$RELEASE" = v1.3-odin3-beta1 ] &&
	SUMS_SHA256=${SUMS_SHA256:-90b67933b8de1b93b5a51ce0a92da07f3e7035e92e5570e492e8ef2d0f8cb9af}
SUMS_SHA256=${SUMS_SHA256:?set SUMS_SHA256 (sha256 of the SHA256SUMS of release $RELEASE)}
URL=https://github.com/hashtagbasit/SteamOS-ARM-Handhelds/releases/download/$RELEASE
WORK_DIR=${WORK_DIR:-/var/tmp/steamos-arm}
MODULES_FROM=${MODULES_FROM:-/lib/modules/$(uname -r)}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
LADSPA_FROM=${LADSPA_FROM:-/usr/lib/ladspa}
HELPER_FROM=${HELPER_FROM:-}
CONFIG_FROM=${CONFIG_FROM:-/etc/tb323fu}
NM_CONNECTIONS_FROM=${NM_CONNECTIONS_FROM:-}
ORIENTATION=${ORIENTATION:-right}
TIMEZONE=${TIMEZONE:-}
DEV_ACCESS=${DEV_ACCESS:-0}
DEV_SSH_KEYS=${DEV_SSH_KEYS:-}
BALDUR_DEV_PASSWORD=${BALDUR_DEV_PASSWORD:-}
here=$(cd "$(dirname "$0")/../.." && pwd)
say() { printf '== %s\n' "$*"; }

mountpoint -q "$T" || { echo "$T is not a mount point"; exit 1; }
# arm64, or another architecture running arm64 programs through qemu-user
# (binfmt with the F flag, e.g. Debian/Ubuntu's qemu-user-binfmt; WSL2 works)
case $(uname -m) in
aarch64) ;;
*) b=/proc/sys/fs/binfmt_misc/qemu-aarch64
	{ grep -qx enabled $b && grep -q '^flags:.*F' $b; } 2>/dev/null ||
		{ echo "run this on an arm64 host, or install qemu-user-binfmt (arm64 programs through qemu)"; exit 1; } ;;
esac

# 1. the release image: SHA256SUMS pinned, parts checked, unpacked once
mkdir -p "$WORK_DIR"; W=$WORK_DIR
img=$W/$IMAGE_NAME.img
if [ ! -s "$img" ] || [ ! -e "$img.ok" ]; then
	curl -fsSL -o "$W/SHA256SUMS" "$URL/SHA256SUMS"
	echo "$SUMS_SHA256  $W/SHA256SUMS" | sha256sum -c - >/dev/null || { echo "SHA256SUMS of $RELEASE changed"; exit 1; }
	for p in $PARTS; do
		f=$IMAGE_NAME.$p
		(cd "$W" && grep -q " $f\$" SHA256SUMS && grep " $f\$" SHA256SUMS | sha256sum -c - >/dev/null 2>&1) && continue
		say "download $f"
		curl -fL -C - -o "$W/$f" "$URL/$f" || curl -fL -o "$W/$f" "$URL/$f"
		(cd "$W" && grep " $f\$" SHA256SUMS | sha256sum -c -)
	done
	say "unpack $IMAGE_NAME.img"
	(cd "$W" && nice -n 19 7z x -y "$IMAGE_NAME.$(echo $PARTS | cut -d' ' -f1)" >/dev/null)
	touch "$img.ok"
fi

# 2. its root (p2) and home (p3), read-only, copied into the target
loop=$(losetup -P -r -f --show "$img")
M=$(mktemp -d)
mkdir "$M/root" "$M/home"
cleanup() {
	for p in /proc/[0-9]*; do
		[ "$(readlink "$p/root" 2>/dev/null)" = "$T" ] && kill -9 "${p#/proc/}" 2>/dev/null
	done
	for m in run dev/pts dev sys proc; do umount "$T/$m" 2>/dev/null || true; done
	umount "$M/root" "$M/home" 2>/dev/null || true
	losetup -d "$loop" 2>/dev/null || true
	rmdir "$M/root" "$M/home" "$M" 2>/dev/null || true
}
trap cleanup EXIT
i=0; while [ ! -b "${loop}p3" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
mount -o ro,noload "${loop}p2" "$M/root"
mount -o ro,noload "${loop}p3" "$M/home"
[ -x "$M/root/usr/lib/systemd/systemd" ] || { echo "no systemd in the image's root partition"; exit 1; }
say "copy root and home ($(du -sh "$M/root" | cut -f1) + $(du -sh "$M/home" | cut -f1))"
nice -n 19 rsync -aHAX --numeric-ids --exclude=/lost+found --exclude='/boot/KERNEL*' "$M/root/" "$T/"
mkdir -p "$T/home"
nice -n 19 rsync -aHAX --numeric-ids --exclude=/lost+found "$M/home/" "$T/home/"
umount "$M/root" "$M/home"

# chroot plumbing
for m in proc sys dev dev/pts run; do
	mountpoint -q "$T/$m" && continue
	case $m in
	proc) mount -t proc proc "$T/proc" ;;
	run) mount -t tmpfs tmpfs "$T/run" ;;
	*) mount --bind "/$m" "$T/$m" ;;
	esac
done
ch() { chroot "$T" /usr/bin/env -i HOME=/root TERM=xterm LANG=C.UTF-8 \
	PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin "$@"; }
# systemctl in a chroot: enable/mask only, no manager to talk to
sc() { ch systemctl "$@" >/dev/null 2>&1 || true; }

# 3. the image's kernel side goes (its ROCKNIX modules, KERNEL copy)
for d in "$T"/usr/lib/modules/*; do
	[ -d "$d" ] && [ "$(basename "$d")" != "$(basename "$MODULES_FROM")" ] && rm -rf "$d"
done
rm -f "$T"/boot/KERNEL*

# 4. kernel modules (including extra/ and updates/) and firmware of the device
v=$(basename "$MODULES_FROM")
say "modules $v"
rm -rf "$T/usr/lib/modules/$v"; cp -a "$MODULES_FROM" "$T/usr/lib/modules/$v"
[ -d "$T/usr/lib/modules/$v/extra" ] || echo "WARNING: no extra/ in $MODULES_FROM (speaker amplifier driver)"
ch depmod -a "$v"
mkdir -p "$T/usr/lib/firmware"
for d in qcom ath12k qca novatek; do   # novatek: the touch controller firmware
	[ -d "$FIRMWARE_FROM/$d" ] || continue
	mkdir -p "$T/usr/lib/firmware/$d"; cp -a "$FIRMWARE_FROM/$d/." "$T/usr/lib/firmware/$d/"
done
[ -e "$FIRMWARE_FROM/aw882xx_acf.bin" ] && cp -a "$FIRMWARE_FROM/aw882xx_acf.bin" "$T/usr/lib/firmware/"
for f in regulatory.db regulatory.db.p7s; do
	[ -e "$T/usr/lib/firmware/$f" ] || { [ -e "$FIRMWARE_FROM/$f" ] && cp -L "$FIRMWARE_FROM/$f" "$T/usr/lib/firmware/$f"; }
done
# the Frame's ath12k option (2 ms RX reorder timeout for its VR link) is not a
# parameter of this ath12k; an empty file of the same name in /etc replaces it
: > "$T/etc/modprobe.d/ath12k.conf"
# modules this kernel does not have (ntsync, ...): a same-named file in /etc
# without them keeps systemd-modules-load from failing
for f in "$T"/usr/lib/modules-load.d/*.conf; do
	[ -e "$T/etc/modules-load.d/$(basename "$f")" ] && continue
	keep=; drop=
	for m in $(grep -v '^[#;]' "$f"); do
		if ch modprobe -n -q -S "$v" "$m" 2>/dev/null; then keep="$keep $m"; else drop="$drop $m"; fi
	done
	[ -n "$drop" ] || continue
	mkdir -p "$T/etc/modules-load.d"
	{ echo "# tb323fu: not in kernel $v:$drop"; for m in $keep; do echo "$m"; done; } > "$T/etc/modules-load.d/$(basename "$f")"
done

# 5. what does not exist here, or must not run here
# - the Steam Frame's and the Odin 3's own hardware services (fan, FPGA, charger
#   LEDs, U-Boot environment, soft AP, its audio mixer setup, video codec links)
# - SteamOS's A/B and partition-set plumbing: /etc overlay, /home and the
#   EFI/ESP mounts by partition set, the offload bind mounts; the image's
#   "grow the home partition" job (it would resize a partition of this disk)
# - rmtfs `-s` starts the modem, which crashes and resets the whole SoC, and the
#   mobile-distribution helpers (in case they ever come in)
masked="odin3d.service sm8750-audio-setup.service deckard-audio-setup.service deckard-fan-control.service
	deckard-fpga-resume.service deckard-charger.service deckard-power-monitor.service
	setubootenv.service softapmanager.service steam-video-codec-setup.service steamos-boot.service
	steamos-arm-bootdebug-file.service steamos-sm8550-expand-home.service
	etc.mount home.mount efi.mount esp.mount steamos-offload.target
	mkinitcpio-generate-shutdown-ramfs.service holo-post-update-shutdown.service
	rmtfs.service tqftpserv.service ModemManager.service droid-juicer.service qbootctl.service
	bootmac-bluetooth.service systemd-repart.service"
# systemd-repart: the image's repart.d adds a home partition to the root disk
# (first test boot: it found no free space on the card and refused -- never let
# it try). The offload bind mounts (/root, /var/log, /var/tmp, ... from
# /home/.steamos/offload) come in through RequiresMountsFor= even with their
# target masked: /root then hid root's .ssh. Root is writable here; mask them.
for u in "$T"/usr/lib/systemd/system/steamos-offload.target.wants/*.mount \
	"$T"/etc/systemd/system/steamos-offload.target.wants/*.mount; do
	[ -e "$u" ] || [ -L "$u" ] && masked="$masked $(basename "$u")"
done
for u in $masked; do
	# a unit file the port wrote into /etc (sm8750-audio-setup) cannot be
	# masked in place: remove it and the links to it first
	if [ -f "$T/etc/systemd/system/$u" ] && [ ! -L "$T/etc/systemd/system/$u" ]; then
		rm -f "$T/etc/systemd/system/$u" "$T"/etc/systemd/system/*.wants/"$u"
	fi
	sc mask "$u"
	[ "$(readlink "$T/etc/systemd/system/$u")" = /dev/null ] || echo "WARNING: $u not masked"
done

# 6. LADSPA plugins of the speaker protection filter (swh-plugins: sc4,
# fastLookaheadLimiter); without them the platform's PipeWire filter-chain
# fails and PipeWire with it
mkdir -p "$T/usr/lib/ladspa"
for f in sc4_1882.so fast_lookahead_limiter_1913.so; do
	if [ -e "$LADSPA_FROM/$f" ]; then cp -L "$LADSPA_FROM/$f" "$T/usr/lib/ladspa/"
	else echo "WARNING: no $LADSPA_FROM/$f -- the speaker filter (and PipeWire) will fail"; fi
done

# 7. platform files and helper
say "platform files"
mkdir -p "$T/etc/tb323fu"
for f in bt-address android-boot.sha256 audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && [ ! -e "$T/etc/tb323fu/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/$f"
done
CC=${CC:-cc} DESTDIR=$T sh "$here/userspace/platform/install.sh" >/dev/null
sc enable tb323fu-gen-ids.service tb323fu-btaddr.service tb323fu-dsp.service \
	tb323fu-audio.service tb323fu-usb-port.service tb323fu-emergency-key.service
sc --global enable tb323fu-speaker-gain.service
if [ -n "$HELPER_FROM" ] && [ -x "$HELPER_FROM/target/release/tb323fu-helperd" ]; then
	say "helper from $HELPER_FROM"
	need=$(objdump -T "$HELPER_FROM/target/release/tb323fu-helperd" | grep -oE 'GLIBC_[0-9.]+' | sort -uV | tail -n1)
	have=$(ls "$T"/usr/lib/libc.so.6 >/dev/null && strings "$T/usr/lib/libc.so.6" | grep -oE 'GLIBC_[0-9.]+' | sort -uV | tail -n1)
	echo "   helper needs $need, the image has $have"
	PREFIX=/usr LIBEXECDIR=/usr/libexec/tb323fu DESTDIR=$T sh "$HELPER_FROM/install.sh" >/dev/null
	sc enable tb323fu-helperd.service
fi

# 8. system configuration
echo "$HOSTNAME_NEW" > "$T/etc/hostname"
printf '127.0.0.1\tlocalhost\n::1\tlocalhost\n127.0.1.1\t%s\n' "$HOSTNAME_NEW" > "$T/etc/hosts"
cat > "$T/etc/fstab" <<EOF
# TB323FU: root and home in one partition, found by GPT name; the boot image
# (kernel + initramfs) is the device's own, nothing of the SteamOS image's BOOT
PARTLABEL=$ROOT_PARTLABEL	/	ext4	defaults,noatime	0	1
EOF
# the port masks the user steamos-manager (its 7.2 kernel had no tracefs); this
# kernel has tracefs, and without the manager steamosctl -- Steam's "Switch to
# Desktop" and back -- fails (tested: both ways work with it running)
[ "$(readlink "$T/etc/systemd/user/steamos-manager.service")" = /dev/null ] &&
	rm -f "$T/etc/systemd/user/steamos-manager.service"
[ -n "$TIMEZONE" ] && ln -sf "../usr/share/zoneinfo/$TIMEZONE" "$T/etc/localtime"
# the port means `steamos` to have passwordless sudo (Steam's brightness slider
# runs `sudo tee .../brightness`), but its 99-steamos-nopasswd is read before
# the image's `wheel` file, and the last match wins: a password was required
printf 'steamos ALL=(ALL) NOPASSWD: ALL\n' > "$T/etc/sudoers.d/zz-tb323fu-steamos"
chmod 440 "$T/etc/sudoers.d/zz-tb323fu-steamos"
mkdir -p "$T/etc/NetworkManager/conf.d"
printf '[keyfile]\nunmanaged-devices=interface-name:usb0\n' > "$T/etc/NetworkManager/conf.d/10-tb323fu-usb0.conf"
if [ -n "$NM_CONNECTIONS_FROM" ] && ls "$NM_CONNECTIONS_FROM"/*.nmconnection >/dev/null 2>&1; then
	mkdir -p "$T/etc/NetworkManager/system-connections"
	install -m600 "$NM_CONNECTIONS_FROM"/*.nmconnection "$T/etc/NetworkManager/system-connections/"
fi

# 9. Gaming Mode on the portrait panel (1904x3040 DSI, no "panel orientation"
# property): the port's session rotates in gamescope's composite shader (the
# DPU's inline rotator cannot take this panel's size -- KMS rotation stays off,
# it is opt-in there) and takes the orientation from GAMESCOPE_ORIENTATION;
# the Odin 3 drop-in in the image (99-odin3.conf) forces "right"; this one
# (99-tb323fu.conf) sorts after it.
mkdir -p "$T/etc/systemd/user/gamescope-session.service.d"
cat > "$T/etc/systemd/user/gamescope-session.service.d/99-tb323fu.conf" <<EOF
# TB323FU: which way Gaming Mode is turned (left|right|normal|upsidedown)
[Service]
Environment=GAMESCOPE_ORIENTATION=$ORIENTATION
EOF
# The rotation shader turns the picture only: touch came in unrotated and landed
# elsewhere. Rotate the touchscreen and pen with a libinput calibration matrix
# (gamescope reads it through wlroots/libinput). Measured for "right" on the
# device: landscape x = native y, landscape y = 1 - native x. The others follow
# from it (not tried on the device).
case $ORIENTATION in
right) cal="0 1 0 -1 0 1" ;;
left) cal="0 -1 1 1 0 0" ;;
upsidedown) cal="-1 0 1 0 -1 1" ;;
*) cal="" ;;
esac
if [ -n "$cal" ]; then
	cat > "$T/etc/udev/rules.d/99-tb323fu-touch-gamescope.rules" <<EOF
# TB323FU on SteamOS: Gaming Mode is turned $ORIENTATION by gamescope's rotation shader,
# which does not rotate touch input; rotate the touchscreen and pen to match.
ATTRS{name}=="NVTCapacitiveTouchScreen|NVTCapacitivePen", ENV{LIBINPUT_CALIBRATION_MATRIX}="$cal"
EOF
fi
# Power button: logind ignores it (HandlePowerKey=ignore) and leaves it to
# steamos-powerbuttond, which sends Steam steam://shortpowerpress (Steam then
# suspends). The Steam Frame's unit only runs next to SteamVR
# (Requisite=steamvr.service + an ExecCondition on the VR device model), so it
# never started here and the button did nothing. A drop-in cannot clear a
# Requisite=: replace the unit, start it with Gaming Mode.
cat > "$T/etc/systemd/user/steamos-powerbuttond.service" <<'EOF'
# TB323FU: replaces /usr/lib/systemd/user/steamos-powerbuttond.service (Steam Frame: SteamVR only).
[Unit]
Description=Power Button daemon for SteamOS
PartOf=gamescope-session.target
After=gamescope-session.service

[Service]
ExecStart=/usr/lib/hwsupport/steamos-powerbuttond
Restart=on-failure
Slice=session.slice

[Install]
WantedBy=gamescope-session.target
EOF
mkdir -p "$T/etc/systemd/user/gamescope-session.target.wants"
ln -sf ../steamos-powerbuttond.service "$T/etc/systemd/user/gamescope-session.target.wants/steamos-powerbuttond.service"

# 10. developer access over the USB cable (the initramfs creates the gadget)
if [ "$DEV_ACCESS" = 1 ]; then
	say "developer access (usb0, ttyGS0 autologin, ssh root)"
	mkdir -p "$T/etc/systemd/network" "$T/etc/systemd/system/serial-getty@ttyGS0.service.d" "$T/etc/ssh/sshd_config.d"
	printf '[Match]\nName=usb0\n\n[Network]\nAddress=192.168.7.2/24\nDNS=1.1.1.1\n\n[Route]\nGateway=192.168.7.1\nMetric=1024\n' \
		> "$T/etc/systemd/network/50-usb0.network"
	printf '[Service]\nExecStart=\nExecStart=-/usr/bin/agetty --autologin root --keep-baud 115200,57600,38400,9600 - $TERM\nTimeoutStopSec=5\n' \
		> "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/autologin.conf"
	# SteamOS ships systemd 257.7: when a unit with TTYReset=yes stops, PID 1 itself asks the terminal
	# for its size (ANSI DSR) after tcsetattr(TCSADRAIN), which waits until the output drains. If the PC
	# is not reading the gadget's serial port nothing drains, PID 1 stops feeding the watchdog, and the
	# reboot ends in a pretimeout panic. systemd 257.8 uses TCSANOW.
	printf '[Service]\nTTYReset=no\n' > "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/tty-no-reset.conf"
	echo "PermitRootLogin yes" > "$T/etc/ssh/sshd_config.d/10-tb323fu-dev.conf"
	if [ -n "$DEV_SSH_KEYS" ] && [ -f "$DEV_SSH_KEYS" ]; then
		install -d -m 700 "$T/root/.ssh"; install -m 600 "$DEV_SSH_KEYS" "$T/root/.ssh/authorized_keys"
	fi
	sc enable systemd-networkd.service serial-getty@ttyGS0.service sshd.service
fi
if [ -n "$BALDUR_DEV_PASSWORD" ]; then
	printf 'root:%s\n' "$BALDUR_DEV_PASSWORD" | ch chpasswd
fi

sync
say "done: $T ($(du -sh -x "$T" 2>/dev/null | cut -f1), SteamOS ARM $RELEASE)"
