#!/system/bin/sh
# SPDX-License-Identifier: MIT
# Switch to Linux: the KernelSU "Action" button. Writes the Linux boot image
# to boot_a and reboots into it -- what tools/cycle.sh does from a PC, done on
# the tablet.
#
# Which image:
#   1. the Linux that was running last: back-to-android saves boot_a to
#      /var/lib/tb323fu/linux-current.img (+ .sha256) on the Linux root
#      (GPT name baldur-root), mounted here read-only (ro,noload -- no
#      journal replay, nothing written);
#   2. otherwise the staged fallback (android/install-module.sh stage IMG):
#      /data/adb/tb323fu/linux.img (+ .sha256).
# The way back is back-to-android in Linux (volume up+down held 10 s, or the
# "Android" quick-settings tile), which copies boot_b into boot_a -- so this
# refuses to write unless boot_b holds the known Android image
# (android.sha256 in this module, written by install-module.sh from boot_b).
# Every step is checked; on any doubt it stops without rebooting.
MOD=${0%/*}
D=/data/adb/tb323fu
img=$D/linux.img
a=/dev/block/by-name/boot_a
b=/dev/block/by-name/boot_b
fail() { echo "! $*"; echo "! stopped, still in Android"; exit 1; }
h() { sha256sum "$1" | cut -c1-64; }

android=$(cut -c1-64 "$MOD/android.sha256" 2>/dev/null)
[ ${#android} -eq 64 ] || fail "module has no android.sha256"
M=$D/root
cleanup() { umount "$M" 2>/dev/null; }
trap cleanup EXIT
img= want= src=
root=/dev/block/by-name/baldur-root
if [ -b "$root" ]; then
	mkdir -p "$M" && mount -t ext4 -o ro,noload "$root" "$M" 2>/dev/null &&
		[ -f "$M/var/lib/tb323fu/linux-current.img" ] && {
		img=$M/var/lib/tb323fu/linux-current.img
		want=$(cut -c1-64 "$img.sha256" 2>/dev/null)
		src="last running Linux (root partition baldur-root)"
	}
fi
if [ ${#want} -ne 64 ]; then
	cleanup
	img=$D/linux.img
	want=$(cut -c1-64 "$img.sha256" 2>/dev/null)
	src="staged $img"
fi
[ ${#want} -eq 64 ] && [ -f "$img" ] || fail "no Linux image (none saved by back-to-android, none staged)"
echo "- image: $src"
[ "$(getprop ro.boot.slot_suffix)" = _a ] || fail "running slot is not _a"
[ -b "$a" ] && [ -b "$b" ] || fail "no boot_a/boot_b"
size=$(blockdev --getsize64 "$a")
[ "$(stat -c %s "$img")" = "$size" ] || fail "image is not $size bytes (boot_a size)"

echo "- checking the image"
if [ "$(h "$img")" != "$want" ]; then
	# a half-written save on baldur-root: fall back to the staged one
	[ "$img" = "$D/linux.img" ] && fail "image hash mismatch"
	echo "- $src does not match its hash; using the staged image"
	cleanup; img=$D/linux.img; want=$(cut -c1-64 "$img.sha256" 2>/dev/null)
	[ ${#want} -eq 64 ] && [ -f "$img" ] || fail "no staged image either"
	[ "$(stat -c %s "$img")" = "$size" ] || fail "staged image is not $size bytes"
	[ "$(h "$img")" = "$want" ] || fail "staged image hash mismatch"
fi
echo "- checking boot_b (the way back)"
[ "$(h "$b")" = "$android" ] || fail "boot_b is not the Android image -- there would be no way back"
[ "$(h "$a")" = "$android" ] || echo "- note: boot_a was not the stock Android image"

echo "- writing boot_a"
dd if="$img" of="$a" bs=1M conv=fsync 2>/dev/null
sync
cleanup
echo 3 > /proc/sys/vm/drop_caches
if [ "$(h "$a")" != "$want" ]; then
	echo "! boot_a mismatch after the write; restoring Android from boot_b"
	dd if="$b" of="$a" bs=1M conv=fsync 2>/dev/null; sync
	echo 3 > /proc/sys/vm/drop_caches
	[ "$(h "$a")" = "$android" ] && fail "boot_a is Android again" ||
		fail "boot_a is NEITHER image -- do not reboot; rerun or use EDL"
fi
echo "- boot_a = Linux $(echo $want | cut -c1-16)"
echo "- rebooting into Linux in 3 s (back: volume up+down held 10 s)"
sleep 3
reboot
