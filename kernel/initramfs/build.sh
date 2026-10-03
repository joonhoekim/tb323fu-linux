#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# build.sh -- assemble the TB323FU initramfs (the one the kernel Image carries
# built in, CONFIG_INITRAMFS_SOURCE) from this directory, a kernel build and a
# few third-party files that are not stored in this repository.
#
#   kernel/initramfs/build.sh -k KBUILD_OUT -b BUSYBOX [-m MODULES_DIR] [-f FIRMWARE_ROOT] [-u] [options] OUT.cpio.gz
#
#   -k  kernel build directory (make O=...): usr/gen_init_cpio and (without
#       -m) the early modules come from it (build the modules first: make modules)
#   -m  the kernel's whole installed modules tree, MODULES_DIR =
#       <INSTALL_MOD_PATH>/lib/modules/<release> (make modules_install
#       INSTALL_MOD_STRIP=1, out-of-tree modules in extra/, depmod -b run:
#       modules.dep must be there). It goes into the image as one squashfs,
#       /lib/modules/<release>.sqfs (xz; needs mksquashfs), which init mounts
#       and moves into the chosen root: the roots then need no modules of
#       their own (shared modules, docs/notes/kernel-updates-design.md).
#       Without -m only the early modules are copied, flat into /lib/modules,
#       and every root needs its own /lib/modules/<release>.
#   -b  a STATIC aarch64 busybox (e.g. Debian's busybox-static, /bin/busybox;
#       the tested one is BusyBox 1.36.1). Needs the applets init uses:
#       sh, mount, insmod, setfont, watchdog, telnetd, usleep, switch_root, ...
#   -f  firmware root laid out like /lib/firmware's parent, i.e. the output of
#       firmware/extract-on-device.sh (FIRMWARE_ROOT/lib/firmware/...). The
#       Adreno, touch, Bluetooth and Wi-Fi firmware must be in the initramfs:
#       those drivers are built in and probe before any root is mounted.
#       Without -f the image carries no vendor firmware at all (only
#       regulatory.db) -- the release images are built that way, since the
#       firmware may not be redistributed; see README.md for what that costs.
#   -r  directory holding regulatory.db + regulatory.db.p7s (wireless-regdb;
#       default /lib/firmware of the build machine)
#   -a  file with the sha256 of YOUR Android boot image in boot_b (enables the
#       volume up+down emergency way back to Android; see android/README.md).
#       Without it init reads the hash from the root partition
#       (/etc/tb323fu/android-boot.sha256 on the state root: baldur-root, else
#       baldur-root-sd, else the first tb323fu-*);
#       with neither, the chord does nothing in the initramfs (the rootfs's own
#       tb323fu-emergency-key service still works).
#   -F  console font, PSF (default: console-setup's Lat15-Terminus28x14,
#       /usr/share/consolefonts/Lat15-Terminus28x14.psf.gz, unpacked)
#   -c  C compiler for the two small static helpers (default: aarch64-linux-gnu-gcc,
#       or cc on an aarch64 host). keyhold is required, gpu-probe optional.
#   -l  also write the gen_init_cpio list here (absolute paths into the
#       staging dir), for tools/build-boot.sh -i
#   -u  development image: a root shell WITHOUT PASSWORD on the USB serial
#       port (ttyGS0) and telnet on USB networking (192.168.7.2) from early
#       boot on, and whenever init stays in the initramfs. Anyone with a cable
#       gets root. Without -u (release images) the USB gadget is still set up,
#       but no shell; tb323fu.usbshell=1 or =0 on the kernel command line
#       overrides the image either way.
#
# Nothing downloaded here; nothing proprietary is ever written into this repo.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
kout= busybox= moddir= fwroot= regdb=/lib/firmware android= font= cc= list= usbshell=
while getopts k:b:m:f:r:a:F:c:l:u o; do case $o in
	k) kout=$OPTARG ;; b) busybox=$OPTARG ;; m) moddir=$OPTARG ;; f) fwroot=$OPTARG ;; r) regdb=$OPTARG ;;
	a) android=$OPTARG ;; F) font=$OPTARG ;; c) cc=$OPTARG ;; l) list=$OPTARG ;; u) usbshell=1 ;;
	*) sed -n '4,51p' "$0"; exit 2 ;; esac; done
shift $((OPTIND - 1)); out=${1:?output .cpio.gz}
[ -n "$kout" ] && [ -n "$busybox" ] || { sed -n '4,51p' "$0"; exit 2; }
if [ -z "$cc" ]; then
	if [ "$(uname -m)" = aarch64 ]; then cc=cc; else cc=aarch64-linux-gnu-gcc; fi
fi
strip=${cc%gcc}strip; command -v "$strip" >/dev/null || strip=strip

st=$(mktemp -d); trap 'rm -rf "$st"' EXIT
mkdir -p "$st/modules" "$st/fw"

# ours: init, back-to-android, keyhold, gpu-probe
cp "$here/init" "$st/init"
cp "$repo/android/back-to-android" "$st/back-to-android"
"$cc" -static -O2 -o "$st/keyhold" "$repo/userspace/platform/src/keyhold.c"
"$cc" -static -O2 -I"$kout/usr/include" -o "$st/gpu-probe" "$here/gpu-probe.c" 2>/dev/null \
	|| { echo "gpu-probe not built (needs: make O=$kout headers_install); the summary skips it" >&2; rm -f "$st/gpu-probe"; }

# kernel modules that stay modules on purpose (see README.md): init loads them
early="qcom_pil_info qcom_common qcom_sysmon qcom_q6v5 qcom_q6v5_pas nt36536_ts"
rel=
if [ -n "$moddir" ]; then
	# -m: the whole tree as one squashfs (init mounts it and modloads from it)
	moddir=$(cd "$moddir" && pwd); rel=${moddir##*/}
	[ -s "$moddir/modules.dep" ] || { echo "no $moddir/modules.dep (run depmod -b on the installed tree)" >&2; exit 1; }
	for m in $early; do
		grep -q -E "(^|/)$m\.ko:" "$moddir/modules.dep" || { echo "$m.ko is not in $moddir/modules.dep" >&2; exit 1; }
	done
	for l in build source; do [ -e "$moddir/$l" ] || [ -L "$moddir/$l" ] && { echo "$moddir/$l: remove the build tree links first" >&2; exit 1; }; done
	command -v mksquashfs >/dev/null || { echo "needs mksquashfs (squashfs-tools)" >&2; exit 1; }
	# fixed times: the same tree gives the same image
	mksquashfs "$moddir" "$st/modules.sqfs" -comp xz -all-root -no-xattrs -noappend -quiet \
		-mkfs-time 0 -all-time 0 > /dev/null
	echo "modules image: $rel.sqfs, $(find "$moddir" -name '*.ko' | wc -l) modules, $(stat -c %s "$st/modules.sqfs") bytes" >&2
else
	for m in $early; do
		f=$(find "$kout" -name "$m.ko" -print -quit)
		[ -n "$f" ] || { echo "missing $m.ko in $kout (make modules)" >&2; exit 1; }
		cp "$f" "$st/modules/"; "$strip" --strip-debug "$st/modules/$m.ko" 2>/dev/null || true
	done
fi

# third-party: busybox, font, firmware, regulatory database
cp "$busybox" "$st/busybox"
if [ -n "$font" ]; then cp "$font" "$st/font.psf"
else zcat /usr/share/consolefonts/Lat15-Terminus28x14.psf.gz > "$st/font.psf"; fi
fw=( qcom/gen80200_sqe.fw qcom/gen80200_gmu.bin qcom/gen80200_aqe.fw
     qcom/kaanapali/Lenovo/baldur/gen80200_zap.mbn
     novatek/novatek_ts_fw.bin
     qca/brhbtnv20.bin qca/brhbtfw20.mbn
     ath12k/WCN7860/hw2.0/amss.bin ath12k/WCN7860/hw2.0/m3.bin
     ath12k/WCN7860/hw2.0/aux_ucode.bin ath12k/WCN7860/hw2.0/board.bin
     ath12k/WCN7860/hw2.0/regdb.bin ath12k/WCN7860/hw2.0/qdss.cfg )
if [ -n "$fwroot" ]; then
	for f in "${fw[@]}"; do
		[ -f "$fwroot/lib/firmware/$f" ] || { echo "missing firmware $f under $fwroot/lib/firmware" >&2; exit 1; }
		mkdir -p "$st/fw/$(dirname "$f")"; cp "$fwroot/lib/firmware/$f" "$st/fw/$f"
	done
	# optional: the BT .tlv some firmware versions ask for
	[ -f "$fwroot/lib/firmware/qca/brhbtfw20.tlv" ] && cp "$fwroot/lib/firmware/qca/brhbtfw20.tlv" "$st/fw/qca/"
else
	echo "no -f: no vendor firmware in this initramfs (regulatory.db only)" >&2
fi
cp "$regdb/regulatory.db" "$regdb/regulatory.db.p7s" "$st/fw/"
[ -n "$android" ] && cut -c1-64 "$android" > "$st/android-boot.sha256"
[ -n "$usbshell" ] && : > "$st/usb-shell"

# gen_init_cpio list: see spec.list for the annotated layout
L="$st/initramfs.list"
{
	echo "dir /bin 0755 0 0"
	echo "file /bin/busybox $st/busybox 0755 0 0"
	echo "file /init $st/init 0755 0 0"
	echo "file /bin/back-to-android $st/back-to-android 0755 0 0"
	echo "file /bin/keyhold $st/keyhold 0755 0 0"
	[ -f "$st/gpu-probe" ] && echo "file /bin/gpu-probe $st/gpu-probe 0755 0 0"
	echo "dir /etc 0755 0 0"
	[ -f "$st/android-boot.sha256" ] && echo "file /etc/android-boot.sha256 $st/android-boot.sha256 0644 0 0"
	[ -f "$st/usb-shell" ] && echo "file /etc/usb-shell $st/usb-shell 0644 0 0"
	echo "dir /dev 0755 0 0"
	echo "nod /dev/console 0600 0 0 c 5 1"
	echo "file /font.psf $st/font.psf 0644 0 0"
	for d in /proc /sys /sys/fs /sys/fs/pstore /lib /lib/modules; do echo "dir $d 0755 0 0"; done
	for m in "$st"/modules/*.ko; do [ -e "$m" ] && echo "file /lib/modules/${m##*/} $m 0644 0 0"; done
	[ -n "$rel" ] && echo "dir /lib/modules/$rel 0755 0 0" && echo "file /lib/modules/$rel.sqfs $st/modules.sqfs 0644 0 0"
	(cd "$st/fw" && find . -type d | sed 's|^\.||' | sort) | while read -r d; do echo "dir /lib/firmware$d 0755 0 0"; done
	(cd "$st/fw" && find . -type f | sed 's|^\./||' | sort) | while read -r f; do echo "file /lib/firmware/$f $st/fw/$f 0644 0 0"; done
} > "$L"

gic="$kout/usr/gen_init_cpio"
# the kernel builds it only for list/directory initramfs sources; else build it
# here from the source tree the build directory points to
if [ ! -x "$gic" ] && [ -f "$kout/source/usr/gen_init_cpio.c" ]; then
	gic="$st/gen_init_cpio"; ${HOSTCC:-cc} -O2 -o "$gic" "$kout/source/usr/gen_init_cpio.c"
fi
[ -x "$gic" ] || { echo "no $kout/usr/gen_init_cpio (build the kernel once, or cc -o it from usr/gen_init_cpio.c)" >&2; exit 1; }
"$gic" "$L" | gzip -9 > "$out"
echo "initramfs: $out ($(stat -c %s "$out") bytes, USB root shell: $([ -n "$usbshell" ] && echo on || echo off))"
if [ -n "$list" ]; then
	# keep the staging dir for the list's absolute paths
	keep="${list%.list}.d"; rm -rf "$keep"; cp -a "$st" "$keep"
	sed "s|$st|$keep|g" "$L" > "$list"; echo "list: $list (files in $keep)"
fi
