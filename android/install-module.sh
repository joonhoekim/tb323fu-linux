#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# install-module.sh -- PC side of the Android -> Linux switch. Needs the tablet
# running Android, rooted with KernelSU, on adb (USB, or ANDROID_SERIAL=IP:5555).
#
#   install-module.sh prepare-boot-b   copy the running Android boot image
#                                      (boot_a) into boot_b once -- the way back
#   install-module.sh install          build the switch-to-linux module with
#                                      android.sha256 = hash of boot_b, install it
#                                      with ksud (active after the next reboot)
#   install-module.sh stage IMG        push a Linux boot image as the fallback
#                                      /data/adb/tb323fu/linux.img (+ .sha256)
#   install-module.sh status           boot_a/boot_b hashes, staged image, module
#
# prepare-boot-b and install only proceed while Android runs from slot _a and
# boot_a is what Android booted from; install refuses unless boot_a == boot_b,
# so the recorded hash is really your Android kernel. Print the hash with
# `status` and keep it: back-to-android and tools/cycle.sh need it.
set -eo pipefail
here=$(cd "$(dirname "$0")" && pwd)
su_() { MSYS_NO_PATHCONV=1 adb shell "su -c '$*'" | tr -d '\r'; }
slot_a() { [ "$(adb shell getprop ro.boot.slot_suffix | tr -d '\r')" = _a ] || { echo "Android is not running from slot _a"; exit 1; }; }
hash_of() { su_ "sha256sum /dev/block/by-name/$1" | cut -c1-64; }
case $1 in
prepare-boot-b)
	slot_a
	a=$(hash_of boot_a); b=$(hash_of boot_b)
	echo "boot_a $a"; echo "boot_b $b"
	[ "$a" = "$b" ] && { echo "boot_b already holds the running Android boot image"; exit 0; }
	read -r -p "Copy boot_a (the Android kernel running now) over boot_b? [yes/N] " ok
	[ "$ok" = yes ] || exit 1
	su_ "dd if=/dev/block/by-name/boot_a of=/dev/block/by-name/boot_b bs=1M conv=fsync 2>/dev/null; sync"
	[ "$(hash_of boot_b)" = "$a" ] && echo "boot_b = $a" || { echo "boot_b does not match after the copy"; exit 1; } ;;
install)
	slot_a
	a=$(hash_of boot_a); b=$(hash_of boot_b)
	[ ${#b} -eq 64 ] && [ "$a" = "$b" ] || { echo "boot_a ($a) != boot_b ($b): run prepare-boot-b first (in Android)"; exit 1; }
	tmp=$(mktemp -d)
	cp -r "$here/switch-to-linux" "$tmp/m"
	echo "$b" > "$tmp/m/android.sha256"
	sed -i 's/\r$//' "$tmp"/m/*
	(cd "$tmp/m" && python3 -c "
import zipfile, os
with zipfile.ZipFile('../switch-to-linux.zip', 'w', zipfile.ZIP_DEFLATED) as z:
    for f in sorted(os.listdir('.')):
        i = zipfile.ZipInfo(f); i.external_attr = (0o755 if f.endswith('.sh') else 0o644) << 16
        z.writestr(i, open(f, 'rb').read())")
	zip=$(cygpath -m "$tmp/switch-to-linux.zip" 2>/dev/null || echo "$tmp/switch-to-linux.zip")
	MSYS_NO_PATHCONV=1 adb push "$zip" /data/local/tmp/ > /dev/null
	su_ "ksud module install /data/local/tmp/switch-to-linux.zip" | tail -3
	echo "android.sha256 = $b (keep it: back-to-android needs it)"
	rm -rf "$tmp" ;;
stage)
	IMG=${2:?usage: $0 stage IMG}
	want=$(sha256sum "$IMG" | cut -c1-64)
	src=$(cygpath -m "$IMG" 2>/dev/null || echo "$IMG")
	MSYS_NO_PATHCONV=1 adb push "$src" /data/local/tmp/linux.img > /dev/null
	su_ "mkdir -p /data/adb/tb323fu && mv /data/local/tmp/linux.img /data/adb/tb323fu/linux.img && echo $want > /data/adb/tb323fu/linux.img.sha256"
	got=$(su_ "sha256sum /data/adb/tb323fu/linux.img" | cut -c1-64)
	[ "$got" = "$want" ] && echo "staged ${want:0:16}" || { echo "staged copy mismatch $got"; exit 1; } ;;
status)
	echo "slot:   $(adb shell getprop ro.boot.slot_suffix | tr -d '\r')"
	echo "boot_a: $(hash_of boot_a)"
	echo "boot_b: $(hash_of boot_b)"
	echo "staged: $(su_ "cat /data/adb/tb323fu/linux.img.sha256 2>/dev/null" | cut -c1-16)"
	su_ "ksud module list" | grep -A3 -i tb323fu-switch || echo "module not installed" ;;
*) sed -n '4,19p' "$0"; exit 2 ;;
esac
