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
set -o pipefail
H=${TB323FU_HOST:-192.168.7.2}
want_android=${ANDROID_BOOT_SHA256:?set ANDROID_BOOT_SHA256 (see android/README.md)}
ssh_() { ssh -o BatchMode=yes -o ConnectTimeout=5 root@$H "$@"; }
if ssh_ true 2>/dev/null; then
	ssh_ "back-to-android $want_android" | tail -2
fi
timeout 180 adb wait-for-device || { echo "no adb within 180 s"; exit 1; }
for i in $(seq 1 60); do
	[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break
	sleep 2
done
[ "$1" = --android ] && { echo "android $(date +%H:%M:%S)"; exit 0; }

IMG=${1:?usage: cycle.sh IMG | --android}
want=$(sha256sum "$IMG" | cut -c1-64)
src=$(cygpath -m "$IMG" 2>/dev/null || echo "$IMG")
MSYS_NO_PATHCONV=1 adb push "$src" /data/local/tmp/next-boot.img > /dev/null || exit 1
got=$(MSYS_NO_PATHCONV=1 adb shell "su -c 'dd if=/data/local/tmp/next-boot.img of=/dev/block/by-name/boot_a bs=1M conv=fsync 2>/dev/null; sync; sha256sum /dev/block/by-name/boot_a; rm /data/local/tmp/next-boot.img'" | cut -c1-64)
[ "$got" = "$want" ] || { echo "boot_a mismatch $got -- not rebooting"; exit 1; }
echo "boot_a=${want:0:16}; rebooting $(date +%H:%M:%S)"
adb reboot
for i in $(seq 1 60); do
	ssh_ true 2>/dev/null && { echo "Linux up at $H $(date +%H:%M:%S)"; exit 0; }
	sleep 3
done
echo "no SSH at $H after 180 s"
exit 1
