#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- Armada (arm64 gaming distribution for handhelds, Fedora
# bootc based) for the TB323FU, from an Armada release image, into a mounted,
# empty ext4 partition.
#
#   sh build-rootfs.sh TARGET_DIR
#
# THIRD-PARTY IMAGE: this downloads a release image of
#   https://github.com/armada-os/armada
# Only its root partition is used: no kernel, device tree, ABL or initramfs of
# it -- this device boots its own kernel and initramfs, which switch_root into
# /sbin/init of the partition and know nothing of OSTree. The image's root is a
# btrfs holding an OSTree (bootc) deployment; the booted deployment and its
# /var are copied out as a plain root. Armada's own updates (bootc) do not work
# on the result.
#
# Environment (all optional; same names as the other rootfs/ builders):
#   ROOT_PARTLABEL=tb323fu-armada   GPT name of the target partition (fstab)
#   HOSTNAME_NEW=tb323fu-armada     hostname of the new system
#   RELEASE=20260926                release of Armada
#   IMAGE_SHA256=2213cc70...        sha256 of armada-$RELEASE.img.gz (from the release page)
#   WORK_DIR=/var/tmp/armada        download and unpack here (~33 GB for this release)
#   MODULES_FROM=/lib/modules/$(uname -r)   kernel modules of the kernel that will boot it
#                                   (with extra/); used to trim the image's modules-load.d
#                                   lists. With a boot image that carries its modules (shared
#                                   modules) the copy is hidden by the initramfs' mount
#   FIRMWARE_FROM=/lib/firmware     copy qcom/ ath12k/ qca/ novatek/ aw882xx_acf.bin from here
#   LADSPA_FROM=/usr/lib/ladspa     sc4_1882.so and fast_lookahead_limiter_1913.so (swh-plugins)
#                                   for the speaker protection filter; Armada has none
#   KCONFIG_FROM=/proc/config.gz    the configuration of the kernel that will boot it
#   DEBS_FROM=                      the tb323fu-*.deb files of a helper-v* release: platform files
#                                   and helper unpacked from them instead of install.sh and HELPER_FROM
#   HELPER_FROM=                    a helper/ tree with target/release built, installed with
#                                   PREFIX=/usr; without it no helper
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf,
#                                   emergency-key.conf when present (device-specific,
#                                   never put them in git)
#   NM_CONNECTIONS_FROM=            copy these NetworkManager keyfiles -- never from git
#   ORIENTATION=right               Gaming Mode orientation of the portrait panel
#                                   (left|right|normal|upsidedown)
#   TIMEZONE=                       e.g. Asia/Seoul
#   DEV_ACCESS=0                    1 = developer access: usb0 gadget network
#                                   (192.168.7.2/24), root autologin on ttyGS0,
#                                   sshd root login allowed
#   DEV_SSH_KEYS=                   with DEV_ACCESS=1: an authorized_keys file installed for root
#   BALDUR_DEV_PASSWORD=            dev-only password for root; without it root stays locked
#
# The image's user is `armada` (uid 1000, password "armada" unless changed); SDDM
# logs it in automatically into Gaming Mode (gamescope + Steam), Desktop Mode is
# KDE Plasma. Needs: curl, sha256sum, gzip (or pigz), losetup, btrfs in the
# running kernel, rsync, chroot (and bsdtar with DEBS_FROM).
set -eu

T=${1:?usage: build-rootfs.sh TARGET_DIR}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-tb323fu-armada}
HOSTNAME_NEW=${HOSTNAME_NEW:-tb323fu-armada}
RELEASE=${RELEASE:-20260926}
[ "$RELEASE" = 20260926 ] &&
	IMAGE_SHA256=${IMAGE_SHA256:-2213cc709bca39c977f7d0810150810436f82115df2e13275b0557fd571a3c7a}
IMAGE_SHA256=${IMAGE_SHA256:?set IMAGE_SHA256 (sha256 of armada-$RELEASE.img.gz, from the release page)}
URL=https://downloads.armadaos.dev/release
WORK_DIR=${WORK_DIR:-/var/tmp/armada}
MODULES_FROM_SET=${MODULES_FROM:-}; LADSPA_FROM_SET=${LADSPA_FROM:-}
MODULES_FROM=${MODULES_FROM:-/lib/modules/$(uname -r)}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
LADSPA_FROM=${LADSPA_FROM:-/usr/lib/ladspa}
HELPER_FROM=${HELPER_FROM:-}
DEBS_FROM=${DEBS_FROM:-}
KCONFIG_FROM=${KCONFIG_FROM:-$([ "$(uname -m)" = aarch64 ] && echo /proc/config.gz)}
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
case $(uname -m) in
aarch64) ;;
*) b=/proc/sys/fs/binfmt_misc/qemu-aarch64
	{ grep -qx enabled $b && grep -q '^flags:.*F' $b; } 2>/dev/null ||
		{ echo "run this on an arm64 host, or install qemu-user-binfmt (arm64 programs through qemu)"; exit 1; }
	[ -n "$MODULES_FROM_SET" ] && [ -n "$LADSPA_FROM_SET" ] && [ -n "$DEBS_FROM" ] ||
		{ echo "not on arm64: set MODULES_FROM, LADSPA_FROM and DEBS_FROM to arm64 files (see the top of this file)"; exit 1; } ;;
esac
grep -qw btrfs /proc/filesystems || modprobe btrfs 2>/dev/null || true
grep -qw btrfs /proc/filesystems || { echo "the running kernel cannot mount btrfs (the image's root)"; exit 1; }

# 1. the release image: pinned by sha256, unpacked once
mkdir -p "$WORK_DIR"; W=$WORK_DIR
gz=$W/armada-$RELEASE.img.gz img=$W/armada-$RELEASE.img
if [ ! -s "$img" ] || [ ! -e "$img.ok" ]; then
	if ! echo "$IMAGE_SHA256  $gz" | sha256sum -c - >/dev/null 2>&1; then
		say "download armada-$RELEASE.img.gz"
		curl -fL -C - -o "$gz" "$URL/armada-$RELEASE.img.gz" || curl -fL -o "$gz" "$URL/armada-$RELEASE.img.gz"
		echo "$IMAGE_SHA256  $gz" | sha256sum -c - >/dev/null || { echo "armada-$RELEASE.img.gz: sha256 mismatch"; exit 1; }
	fi
	say "unpack armada-$RELEASE.img"
	z=gzip; command -v pigz >/dev/null && z=pigz
	nice -n 19 $z -dc "$gz" > "$img.part" && mv "$img.part" "$img"
	touch "$img.ok"
fi

# 2. the btrfs root partition, read-only; the booted deployment and its /var
loop=$(losetup -P -r -f --show "$img")
M=$(mktemp -d)
cleanup() {
	for p in /proc/[0-9]*; do
		[ "$(readlink "$p/root" 2>/dev/null)" = "$T" ] && kill -9 "${p#/proc/}" 2>/dev/null
	done
	for m in run dev/pts dev sys proc; do umount "$T/$m" 2>/dev/null || true; done
	umount "$M" 2>/dev/null || true
	losetup -d "$loop" 2>/dev/null || true
	rmdir "$M" 2>/dev/null || true
}
trap cleanup EXIT
i=0; while [ ! -b "${loop}p1" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
for p in "$loop"p*; do
	[ "$(blkid -o value -s TYPE "$p" 2>/dev/null)" = btrfs ] || continue
	mount -t btrfs -o ro,rescue=nologreplay "$p" "$M" 2>/dev/null || mount -t btrfs -o ro "$p" "$M"
	break
done
mountpoint -q "$M" || { echo "no btrfs partition in $img"; exit 1; }
sr=$(cd "$M" && ls -d ostree/deploy/* */ostree/deploy/* 2>/dev/null | head -1)
[ -n "$sr" ] || { echo "no ostree/deploy/<stateroot> in the image's root"; exit 1; }
S=$M/$sr
set -- "$S"/deploy/*.[0-9]
[ $# = 1 ] && [ -d "$1" ] || { echo "expected one deployment in $S/deploy, found: $*"; exit 1; }
D=$1
[ -x "$D/usr/lib/systemd/systemd" ] || { echo "no systemd in $D"; exit 1; }
say "copy deployment $(basename "$D") ($(du -sh "$D" | cut -f1)) and /var ($(du -sh "$S/var" | cut -f1))"
nice -n 19 rsync -aHAX --numeric-ids --exclude=/.ostree.cfs --exclude=/ostree --exclude=/sysroot \
	"$D/" "$T/"
nice -n 19 rsync -aHAX --numeric-ids --exclude=/.ostree-selabeled "$S/var/" "$T/var/"
umount "$M"
install -d -m 700 "$T/var/roothome"
mkdir -p "$T/var/home" "$T/var/opt" "$T/var/srv" "$T/var/mnt" "$T/var/usrlocal"

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
sc() { ch systemctl "$@" >/dev/null 2>&1 || true; }

# 3. the image's kernel side goes (its modules, vmlinuz, initramfs, dtbs)
for d in "$T"/usr/lib/modules/*; do
	[ -d "$d" ] && [ "$(basename "$d")" != "$(basename "$MODULES_FROM")" ] && rm -rf "$d"
done

# 4. kernel modules and firmware of the device
v=$(basename "$MODULES_FROM")
say "modules $v"
rm -rf "$T/usr/lib/modules/$v"; cp -a "$MODULES_FROM" "$T/usr/lib/modules/$v"
[ -d "$T/usr/lib/modules/$v/extra" ] || echo "WARNING: no extra/ in $MODULES_FROM (speaker amplifier driver)"
ch depmod -a "$v"
mkdir -p "$T/usr/lib/firmware"
for d in qcom ath12k qca novatek; do
	[ -d "$FIRMWARE_FROM/$d" ] || continue
	mkdir -p "$T/usr/lib/firmware/$d"; cp -a "$FIRMWARE_FROM/$d/." "$T/usr/lib/firmware/$d/"
done
[ -e "$FIRMWARE_FROM/aw882xx_acf.bin" ] && cp -a "$FIRMWARE_FROM/aw882xx_acf.bin" "$T/usr/lib/firmware/"
tplg=qcom/kaanapali/LENOVO-TB323FU-tplg.bin
[ -e "$T/usr/lib/firmware/$tplg" ] || install -Dm644 "$here/firmware/audio/${tplg##*/}" "$T/usr/lib/firmware/$tplg"
for f in regulatory.db regulatory.db.p7s; do
	[ -e "$T/usr/lib/firmware/$f" ] || [ -e "$T/usr/lib/firmware/$f.xz" ] ||
		{ [ -e "$FIRMWARE_FROM/$f" ] && cp -L "$FIRMWARE_FROM/$f" "$T/usr/lib/firmware/$f"; }
done
# a same-named file in /etc without the modules this kernel lacks keeps
# systemd-modules-load from failing
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
# - the mounts bootc wrote for the image's disk (btrfs root by UUID, /boot, the ESP)
for u in -.mount boot.mount boot-efi.mount; do
	rm -f "$T/etc/systemd/system/$u" "$T"/etc/systemd/system/*.wants/"$u"
done
# - OSTree, bootc and rpm-ostree (no /sysroot, no deployments, no bootloader
#   entries); bootc-generic-growpart would grow the partition holding / (it is
#   install.sh's growroot's job here)
# - Armada's boot plumbing for ROCKNIX ABL (boot.img regeneration in the ESP,
#   ABL updates, ESP rename, update reserve) and its installer, which writes
#   Armada to the internal storage
# - armada-mtp: binds its own USB gadget to the UDC the initramfs' gadget holds
# - rmtfs `-s` starts the modem, which crashes and resets the whole SoC, and the
#   mobile-distribution helpers (in case they ever come in)
masked="ostree-remount.service ostree-finalize-staged.service ostree-finalize-staged-hold.service
	ostree-boot-complete.service ostree-prepare-root.service ostree-state-overlay@.service
	bootc-destructive-cleanup.service bootc-fetch-apply-updates.service bootc-fetch-apply-updates.timer
	bootc-finalize-staged.service bootc-generic-growpart.service bootc-publish-rhsm-facts.service
	bootc-root-setup.service bootc-status-updated.path bootc-sysusers-shadow-sync.service
	bootloader-update.service rpm-ostreed.service rpm-ostreed-automatic.service rpm-ostreed-automatic.timer
	rpm-ostree-bootstatus.service rpm-ostree-countme.service rpm-ostree-countme.timer
	rpm-ostree-fix-shadow-mode.service systemd-repart.service
	armada-bootimg-sync.service armada-esp-rename.service armada-update-reserve.service
	armada-installer-visibility.service armada-mtp.service
	rmtfs.service tqftpserv.service ModemManager.service droid-juicer.service qbootctl.service
	bootmac-bluetooth.service"
for u in $masked; do
	rm -f "$T"/etc/systemd/system/*.wants/"$u"
	sc mask "$u"
	[ "$(readlink "$T/etc/systemd/system/$u")" = /dev/null ] || echo "WARNING: $u not masked"
done
[ -e "$T/etc/selinux/config" ] && sed -i 's/^SELINUX=.*/SELINUX=permissive/' "$T/etc/selinux/config"

# 6. LADSPA plugins of the speaker protection filter; without them the
# platform's PipeWire filter-chain fails and PipeWire with it
mkdir -p "$T/usr/lib64/ladspa"
for f in sc4_1882.so fast_lookahead_limiter_1913.so; do
	if [ -e "$LADSPA_FROM/$f" ]; then cp -L "$LADSPA_FROM/$f" "$T/usr/lib64/ladspa/"
	else echo "WARNING: no $LADSPA_FROM/$f -- the speaker filter (and PipeWire) will fail"; fi
done

# 7. platform files and helper
say "platform files"
mkdir -p "$T/etc/tb323fu"
for f in bt-address android-boot.sha256 audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && [ ! -e "$T/etc/tb323fu/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/$f"
done
if [ -n "$DEBS_FROM" ]; then
	for d in "$DEBS_FROM"/tb323fu-platform_*.deb "$DEBS_FROM"/tb323fu-helper_*.deb; do
		x=$(mktemp -d)
		bsdtar -xOf "$d" 'data.tar*' | bsdtar -xpf - -C "$x"
		[ -d "$x/usr" ] && tar -C "$x" -cf - usr | tar -C "$T" -xpf - --keep-directory-symlink
		[ -d "$x/etc" ] && tar -C "$x" -cf - etc | tar -C "$T" -xpf - --keep-directory-symlink --skip-old-files
		rm -rf "$x"
	done
	sc enable tb323fu-helperd.service
else
	CC=${CC:-cc} DESTDIR=$T sh "$here/userspace/platform/install.sh" >/dev/null
fi
chown -R 0:0 "$T/usr/lib/firmware" "$T/etc/tb323fu"
sc enable tb323fu-gen-ids.service tb323fu-btaddr.service tb323fu-dsp.service \
	tb323fu-audio.service tb323fu-usb-port.service tb323fu-emergency-key.service tb323fu-kernel-confirm.service
sc --global enable tb323fu-speaker-gain.service
if [ -z "$DEBS_FROM" ] && [ -n "$HELPER_FROM" ] && [ -x "$HELPER_FROM/target/release/tb323fu-helperd" ]; then
	say "helper from $HELPER_FROM"
	PREFIX=/usr LIBEXECDIR=/usr/libexec/tb323fu DESTDIR=$T sh "$HELPER_FROM/install.sh" >/dev/null
	sc enable tb323fu-helperd.service
fi
if [ -e "$T/usr/libexec/tb323fu/tb323fu-helperd" ]; then
	need=$(strings "$T/usr/libexec/tb323fu/tb323fu-helperd" | grep -oE 'GLIBC_[0-9.]+' | sort -uV | tail -n1)
	have=$(strings "$T/usr/lib64/libc.so.6" | grep -oE 'GLIBC_[0-9.]+' | sort -uV | tail -n1)
	echo "   helper needs $need, the image has $have"
fi

# 8. system configuration
echo "$HOSTNAME_NEW" > "$T/etc/hostname"
printf '127.0.0.1\tlocalhost\n::1\tlocalhost\n127.0.1.1\t%s\n' "$HOSTNAME_NEW" > "$T/etc/hosts"
cat > "$T/etc/fstab" <<EOF
# TB323FU: one ext4 partition found by GPT name; the boot image (kernel +
# initramfs) is the device's own, nothing of the Armada image's ESP or /boot
PARTLABEL=$ROOT_PARTLABEL	/	ext4	defaults,noatime	0	1
EOF
kconfig() { case $KCONFIG_FROM in *.gz) gzip -dc "$KCONFIG_FROM" ;; *) cat "$KCONFIG_FROM" ;; esac 2>/dev/null; }
if [ -r "$KCONFIG_FROM" ]; then
	for o in CONFIG_NTSYNC CONFIG_SCHED_CLASS_EXT CONFIG_USB_CONFIGFS_F_FS; do
		kconfig | grep -qE "^$o=[ym]" || echo "   kernel without $o: Armada's use of it stays off"
	done
fi
[ -n "$TIMEZONE" ] && ln -sf "../usr/share/zoneinfo/$TIMEZONE" "$T/etc/localtime"
: > "$T/etc/machine-id"
mkdir -p "$T/etc/NetworkManager/conf.d"
[ "$DEV_ACCESS" = 1 ] ||
	printf '[keyfile]\nunmanaged-devices=interface-name:usb0\n' > "$T/etc/NetworkManager/conf.d/10-tb323fu-usb0.conf"
if [ -n "$NM_CONNECTIONS_FROM" ] && ls "$NM_CONNECTIONS_FROM"/*.nmconnection >/dev/null 2>&1; then
	mkdir -p "$T/etc/NetworkManager/system-connections"
	install -m600 "$NM_CONNECTIONS_FROM"/*.nmconnection "$T/etc/NetworkManager/system-connections/"
fi

# 9. the device to Armada: a profile for device-env (panel orientation, rotation
# in gamescope's shader -- the DPU inline rotator cannot take this panel's
# size), and Gaming Mode's orientation for gamescope-session-plus
model="Lenovo Legion Tab Y700 5th Gen"
cat > "$T/usr/lib/armada/devices/lenovo-tb323fu.conf" <<EOF
ARMADA_DEVICE_ID=lenovo-tb323fu
ARMADA_DEVICE_NAME='Lenovo Legion Tab Y700 (TB323FU)'
ARMADA_PANEL_ORIENTATION=$ORIENTATION
ARMADA_GAMESCOPE_USE_ROTATION_SHADER=1
EOF
de=$T/usr/libexec/armada/device-env
grep -q 'profile=lenovo-tb323fu' "$de" ||
	sed -i "s|^\(\s*\)\*) profile= ;;|\1\"$model\") profile=lenovo-tb323fu ;;\n&|" "$de"
grep -q 'profile=lenovo-tb323fu' "$de" || echo "WARNING: $de has no profile for $model"
mkdir -p "$T/etc/gamescope-session-plus/sessions.d"
printf 'ORIENTATION=%s\nOUTPUT_CONNECTOR=*,DSI-1\n' "$ORIENTATION" > "$T/etc/gamescope-session-plus/sessions.d/steam"

# 10. developer access over the USB cable (the initramfs creates the gadget)
if [ "$DEV_ACCESS" = 1 ]; then
	say "developer access (usb0, ttyGS0 autologin, ssh root)"
	mkdir -p "$T/etc/NetworkManager/system-connections" "$T/etc/systemd/system/serial-getty@ttyGS0.service.d" "$T/etc/ssh/sshd_config.d"
	cat > "$T/etc/NetworkManager/system-connections/tb323fu-usb0.nmconnection" <<'EOF'
[connection]
id=tb323fu-usb0
type=ethernet
interface-name=usb0
autoconnect=true

[ethernet]

[ipv4]
method=manual
address1=192.168.7.2/24,192.168.7.1
dns=1.1.1.1;
route-metric=1024

[ipv6]
method=disabled
EOF
	chmod 600 "$T/etc/NetworkManager/system-connections/tb323fu-usb0.nmconnection"
	printf '[Service]\nExecStart=\nExecStart=-/usr/bin/agetty --autologin root --keep-baud 115200,57600,38400,9600 - $TERM\nTimeoutStopSec=5\n' \
		> "$T/etc/systemd/system/serial-getty@ttyGS0.service.d/autologin.conf"
	echo "PermitRootLogin yes" > "$T/etc/ssh/sshd_config.d/10-tb323fu-dev.conf"
	if [ -n "$DEV_SSH_KEYS" ] && [ -f "$DEV_SSH_KEYS" ]; then
		install -d -m 700 "$T/var/roothome/.ssh"; install -m 600 "$DEV_SSH_KEYS" "$T/var/roothome/.ssh/authorized_keys"
	fi
	sc enable serial-getty@ttyGS0.service sshd.service
fi
if [ -n "$BALDUR_DEV_PASSWORD" ]; then
	printf 'root:%s\n' "$BALDUR_DEV_PASSWORD" | ch chpasswd
fi

sync
say "done: $T ($(du -sh -x "$T" 2>/dev/null | cut -f1), Armada $RELEASE)"
