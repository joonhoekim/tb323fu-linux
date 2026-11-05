#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# cycle.sh -- boot a Linux image on the tablet without EDL, through Android:
#   (if Linux is running: back-to-android over SSH) -> Android on adb ->
#   write IMG to boot_a with root, check its hash -> reboot -> wait for SSH.
#
#   ANDROID_BOOT_SHA256=<hash> tools/cycle.sh IMG
#   ANDROID_BOOT_SHA256=<hash> tools/cycle.sh --android      # only back to Android
#
# ANDROID_BOOT_SHA256: sha256 of your stock Android boot image, the copy kept
# in boot_b (android/README.md); back-to-android refuses to write otherwise.
# TB323FU_HOST: the tablet's address in Linux (default: USB gadget 192.168.7.2).
# adb: USB, or wireless with ANDROID_SERIAL=IP:5555. Android needs root (su).
# Every wait is bounded.
set -uo pipefail
usage="usage: cycle.sh IMG | --android"
[ $# = 1 ] || { echo "$usage" >&2; exit 2; }
H=${TB323FU_HOST:-192.168.7.2}
want_android=${ANDROID_BOOT_SHA256:?set ANDROID_BOOT_SHA256 (see android/README.md)}
if [ "$1" != --android ]; then
	IMG=$1
	[ -f "$IMG" ] || { echo "no such image: $IMG" >&2; exit 2; }
	want=$(sha256sum "$IMG" | cut -c1-64)
	size=$(stat -c %s "$IMG")
fi
ssh_() { ssh -o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 root@$H "$@"; }

if ssh_ true 2>/dev/null; then
	ssh_ "back-to-android $want_android" | tail -2
fi
timeout 180 adb wait-for-device || { echo "no adb within 180 s"; exit 1; }
booted=0
for _ in $(seq 1 60); do
	[ "$(timeout 10 adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && { booted=1; break; }
	sleep 2
done
[ $booted = 1 ] || { echo "Android did not finish booting within about 2 min"; exit 1; }
[ "$1" = --android ] && { echo "android $(date +%H:%M:%S)"; exit 0; }

src=$(cygpath -m "$IMG" 2>/dev/null || echo "$IMG")
MSYS_NO_PATHCONV=1 adb push "$src" /data/local/tmp/next-boot.img > /dev/null || exit 1
MSYS_NO_PATHCONV=1 adb shell "su -c '
	dd if=/data/local/tmp/next-boot.img of=/dev/block/by-name/boot_a bs=1M conv=fsync status=none
	sync; echo 3 > /proc/sys/vm/drop_caches; rm /data/local/tmp/next-boot.img'"
# right after Android has booted the read-back has come back empty once: read again before giving up
got=
for _ in 1 2 3; do
	got=$(MSYS_NO_PATHCONV=1 adb shell "su -c 'head -c $size /dev/block/by-name/boot_a | sha256sum'" | cut -c1-64)
	[ -z "$got" ] || break
	sleep 3
done
if [ "$got" != "$want" ]; then
	echo "boot_a read back as ${got:-nothing}, not $want -- not rebooting" >&2
	echo "boot_a now holds an unchecked image; before the next restart, in adb shell with su:" >&2
	echo "  dd if=/dev/block/by-name/boot_b of=/dev/block/by-name/boot_a bs=1M conv=fsync   (Android's boot image back)" >&2
	exit 1
fi
echo "boot_a=${want:0:16}; rebooting $(date +%H:%M:%S)"
adb reboot
for _ in $(seq 1 60); do
	ssh_ true 2>/dev/null && { echo "Linux up at $H $(date +%H:%M:%S)"; exit 0; }
	sleep 3
done
echo "no SSH at $H after 60 tries (about 8 min)"
exit 1
