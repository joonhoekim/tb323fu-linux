#!/bin/sh
# SPDX-License-Identifier: MIT
# test-kernel-confirm.sh -- run libexec/tb323fu-kernel-confirm against a fake
# tablet: fake /sys/class/block, boot_a as a file, the state root as a
# directory (TB323FU_ROOT). Nothing is mounted (the state root is the current
# root, or a mount stub links it). Needs sha256sum, od, dd, gzip.
#
#   sh userspace/platform/test-kernel-confirm.sh [SHELL ...]     (default: sh)
set -u
here=$(cd "$(dirname "$0")" && pwd)
C=$here/libexec/tb323fu-kernel-confirm
[ $# -gt 0 ] || set -- sh
fails=0; runs=0
ok() { runs=$((runs + 1)); echo "  ok   $*"; }
bad() { runs=$((runs + 1)); fails=$((fails + 1)); echo "  FAIL $*"; }

V27="Linux version 7.3.0-rc4-tb323fu-t27 (u@h) (clang) #1 SMP PREEMPT Wed Oct 1"
V28="Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang) #1 SMP PREEMPT Thu Oct 2"
le32() { n=$1; printf "\\$(printf %03o $((n & 255)))\\$(printf %03o $((n >> 8 & 255)))\\$(printf %03o $((n >> 16 & 255)))\\$(printf %03o $((n >> 24 & 255)))"; }
# bootimg OUT VERSION [gzip]: a boot image whose kernel carries VERSION
bootimg() {
	k=$T/k.bin
	{ head -c 3000 /dev/zero | tr '\0' 'x'; printf '\n%s\n' "$2"; head -c 5000 /dev/zero | tr '\0' 'y'; } > "$k"
	[ "${3:-}" = gzip ] && { gzip -9 -c "$k" > "$k.gz"; mv "$k.gz" "$k"; }
	ks=$(wc -c < "$k")
	{ printf 'ANDROID!'; le32 "$ks"; head -c $((4096 - 12)) /dev/zero; cat "$k"; head -c 20000 /dev/zero; } > "$1"
}
sha() { sha256sum "$1" | cut -c1-64; }
setup() { # RUNNING_VERSION
	T=$(mktemp -d /tmp/tb323fu-confirm.XXXXXX)
	mkdir -p "$T/sys/class/block/sda5" "$T/sys/class/block/sda17" "$T/dev" "$T/proc/sys/kernel" "$T/var/lib/tb323fu" "$T/bin"
	printf 'DEVNAME=sda5\nPARTNAME=boot_a\n' > "$T/sys/class/block/sda5/uevent"
	printf 'DEVNAME=sda17\nPARTNAME=baldur-root\n' > "$T/sys/class/block/sda17/uevent"
	echo "$1" > "$T/proc/version"
	echo "$1" | cut -d' ' -f3 > "$T/proc/sys/kernel/osrelease"
	S=$T/var/lib/tb323fu
}
state() { printf '%s\n' "$@" > "$S/kernel-state"; }
kget() { sed -n "s/^$1=//p" "$S/kernel-state"; }
run() { TB323FU_ROOT=$T TB323FU_CURRENT_ROOT=baldur-root PATH=$T/bin:$PATH $SH "$C" > "$T/out" 2>&1; echo $? > "$T/rc"; }
said() { grep -q -- "$1" "$T/out" && ok "  said: $1" || { bad "  no '$1'"; sed 's/^/        /' "$T/out"; }; }

for SH in "$@"; do
echo "== $SH"

setup "$V28"; bootimg "$T/dev/sda5" "$V28"; a=$(sha "$T/dev/sda5")
printf old > "$S/linux-good.img"
state good=7.3.0-rc4-tb323fu-t27 good_sha256=x "good_version=$V27" good_serial=27 trial=7.3.0-rc4-tb323fu-t28 \
	"trial_sha256=$a" "trial_version=$V28" trial_serial=28 trial_channel=stable tries=1 max=2 failed=7.3.0-rc4-tb323fu-t28 failed_seen=7.3.0-rc4-tb323fu-t28
run
[ "$(kget good)" = 7.3.0-rc4-tb323fu-t28 ] && [ "$(kget good_sha256)" = "$a" ] && [ "$(kget good_serial)" = 28 ] && [ "$(kget good_version)" = "$V28" ] &&
	ok "stable trial running: confirmed" || { bad "stable trial: $(cat "$S/kernel-state")"; cat "$T/out"; }
[ -z "$(kget trial)$(kget tries)$(kget trial_sha256)$(kget trial_channel)" ] && ok "  trial cleared" || bad "  trial left: $(cat "$S/kernel-state")"
[ -z "$(kget failed)$(kget failed_seen)" ] && ok "  failed= (and failed_seen=) for the same release cleared" || bad "  failed kept"
[ "$(sha "$S/linux-good.img")" = "$a" ] && [ "$(cat "$S/linux-good.img.sha256")" = "$a" ] && ok "  linux-good.img = boot_a" || bad "  linux-good.img"
[ "$(kget max)" = 2 ] && ok "  max kept" || bad "  max lost"
rm -rf "$T"

setup "$V28"; bootimg "$T/dev/sda5" "$V28"; a=$(sha "$T/dev/sda5")
state good=7.3.0-rc4-tb323fu-t27 trial=7.3.0-rc4-tb323fu-t28 "trial_sha256=$a" "trial_version=$V28" trial_channel=testing tries=1
run; [ "$(kget trial)" = 7.3.0-rc4-tb323fu-t28 ] && [ ! -e "$S/linux-good.img" ] && ok "testing trial: left for Keep" || bad "testing trial confirmed"
said "waits for Keep"
rm -rf "$T"

setup "$V28"; bootimg "$T/dev/sda5" "$V28"; a=$(sha "$T/dev/sda5")
state good=7.3.0-rc4-tb323fu-t27 trial=7.3.0-rc4-tb323fu-t28 "trial_sha256=$a" "trial_version=$V28" trial_channel=local trial_keep=1 trial_label=mine tries=1
run; [ "$(kget trial)" = 7.3.0-rc4-tb323fu-t28 ] && [ ! -e "$S/linux-good.img" ] && ok "local trial with trial_keep=1: left for Keep" || bad "trial_keep=1 confirmed"
rm -rf "$T"

setup "$V28"; bootimg "$T/dev/sda5" "$V28"; a=$(sha "$T/dev/sda5")
state good=7.3.0-rc4-tb323fu-t27 good_label=old trial=7.3.0-rc4-tb323fu-t28 "trial_sha256=$a" "trial_version=$V28" trial_channel=local "trial_label=my build" tries=1
run; [ "$(kget good)" = 7.3.0-rc4-tb323fu-t28 ] && [ "$(kget good_label)" = "my build" ] && [ -z "$(kget trial_label)$(kget trial_keep)$(kget trial_channel)" ] &&
	ok "local trial installed with --keep: confirmed, its label kept as good_label" || { bad "local --keep: $(cat "$S/kernel-state")"; cat "$T/out"; }
rm -rf "$T"

setup "$V28"; bootimg "$T/dev/sda5" "$V28"
state good=7.3.0-rc4-tb323fu-t27 trial=7.3.0-rc4-tb323fu-t28 "trial_sha256=$(printf 'e%.0s' $(seq 64))" "trial_version=$V28" trial_channel=stable tries=1
run; [ "$(kget trial)" = 7.3.0-rc4-tb323fu-t28 ] && [ "$(cat "$T/rc")" = 1 ] && ok "boot_a changed under the trial: not confirmed (exit 1)" || bad "confirmed a changed boot_a"
said "no longer holds the trial image"
rm -rf "$T"

setup "$V27"; bootimg "$T/dev/sda5" "$V28"; a=$(sha "$T/dev/sda5")
state good=7.3.0-rc4-tb323fu-t27 "good_version=$V27" trial=7.3.0-rc4-tb323fu-t28 "trial_sha256=$a" "trial_version=$V28" trial_channel=stable
run; [ "$(kget trial)" = 7.3.0-rc4-tb323fu-t28 ] && ok "installed, not started yet: left alone" || bad "pending trial touched"
said "waits for a restart"
rm -rf "$T"

setup "$V27"; bootimg "$T/dev/sda5" "$V27"; a=$(sha "$T/dev/sda5")
state good=7.3.0-rc4-tb323fu-t27 "good_version=$V27" trial=7.3.0-rc4-tb323fu-t28 "trial_sha256=$(printf 'f%.0s' $(seq 64))" "trial_version=$V28"
run; [ -z "$(kget trial)" ] && [ "$(kget good_sha256)" = "$a" ] && [ "$(sha "$S/linux-good.img")" = "$a" ] &&
	ok "stale trial (boot_a holds the good kernel again): dropped, running kernel saved" || { bad "stale trial: $(cat "$S/kernel-state")"; cat "$T/out"; }
rm -rf "$T"

setup "$V27"; bootimg "$T/dev/sda5" "$V27"; a=$(sha "$T/dev/sda5")
run; [ "$(kget good)" = 7.3.0-rc4-tb323fu-t27 ] && [ "$(kget good_serial)" = 27 ] && [ "$(kget max)" = 2 ] && [ "$(sha "$S/linux-good.img")" = "$a" ] &&
	ok "no record yet: the running kernel (in boot_a) becomes the good one" || { bad "first record: $(cat "$S/kernel-state" 2>&1)"; cat "$T/out"; }
rm -rf "$T"

setup "$V27"; bootimg "$T/dev/sda5" "$V27" gzip; a=$(sha "$T/dev/sda5")
run; [ "$(kget good_sha256)" = "$a" ] && ok "gzip kernel in boot_a: recognised" || { bad "gzip kernel"; cat "$T/out"; }
rm -rf "$T"

setup "$V27"; bootimg "$T/dev/sda5" "$V28"
run; [ ! -e "$S/kernel-state" ] && [ ! -e "$S/linux-good.img" ] && ok "boot_a holds another kernel: nothing recorded" || bad "recorded a kernel that is not running"
said "does not hold the running kernel"
rm -rf "$T"

setup "$V27"; bootimg "$T/dev/sda5" "$V27"; a=$(sha "$T/dev/sda5"); cp "$T/dev/sda5" "$S/linux-good.img"
state good=7.3.0-rc4-tb323fu-t27 "good_sha256=$a" "good_version=$V27" good_serial=27 max=2 future=1
cp "$S/kernel-state" "$T/before"
run; cmp -s "$T/before" "$S/kernel-state" && [ ! -s "$T/out" ] && ok "already the good kernel: nothing to do, quietly" || { bad "changed a confirmed state"; cat "$T/out"; }
rm -rf "$T"

# running from another root: the state root is mounted (stub) for the change
setup "$V28"; bootimg "$T/dev/sda5" "$V28"; a=$(sha "$T/dev/sda5")
mkdir -p "$T/sys/class/block/mmcblk0p2" "$T/stateroot/var/lib/tb323fu"
printf 'DEVNAME=mmcblk0p2\nPARTNAME=tb323fu-ubuntu\n' > "$T/sys/class/block/mmcblk0p2/uevent"
printf '%s\n' "trial=7.3.0-rc4-tb323fu-t28" "trial_sha256=$a" "trial_version=$V28" trial_channel=stable > "$T/stateroot/var/lib/tb323fu/kernel-state"
printf '#!/bin/sh\nfor a; do d=$a; done; rmdir "$d"; ln -s %s/stateroot "$d"; echo "mount $*" >> %s/mounts\n' "$T" "$T" > "$T/bin/mount"
printf '#!/bin/sh\nrm -f "$1"; echo "umount $1" >> %s/mounts\n' "$T" > "$T/bin/umount"
chmod +x "$T/bin/mount" "$T/bin/umount"
TB323FU_ROOT=$T TB323FU_CURRENT_ROOT=tb323fu-ubuntu PATH=$T/bin:$PATH $SH "$C" > "$T/out" 2>&1
grep -q "mount -t ext4 $T/dev/sda17 $T/run/tb323fu/confirm-state" "$T/mounts" && grep -q umount "$T/mounts" && ok "another root: the state root is mounted and unmounted" || { bad "state root mount"; cat "$T/mounts" "$T/out"; }
[ "$(sed -n 's/^good=//p' "$T/stateroot/var/lib/tb323fu/kernel-state")" = 7.3.0-rc4-tb323fu-t28 ] && [ -e "$T/stateroot/var/lib/tb323fu/linux-good.img" ] &&
	ok "  confirmed on the state root" || bad "  not confirmed on the state root"
rm -rf "$T"
done
echo "$((runs - fails))/$runs passed"
[ $fails -eq 0 ]
