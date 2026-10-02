#!/bin/sh
# SPDX-License-Identifier: MIT
# test-state-root.sh -- run the state-root parts of switch-to-linux/action.sh
# and back-to-android (between ">>> state root" and "<<< state root") offline,
# against a fake /dev/block/by-name, /sys/class/block and mountinfo.
#
#   sh android/test-state-root.sh [SHELL ...]     # default: busybox sh, dash, mksh
#
# Block devices are plain files here, so "[ -b" is tested as "[ -e".
set -u
here=$(cd "$(dirname "$0")" && pwd)
[ $# -gt 0 ] || {
	command -v busybox >/dev/null && set -- "busybox sh"
	command -v dash >/dev/null && set -- "$@" dash
	command -v mksh >/dev/null && set -- "$@" mksh
}
[ $# -gt 0 ] || { echo "no shell to test with"; exit 2; }
fails=0; runs=0
ok() { runs=$((runs + 1)); echo "  ok   $*"; }
bad() { runs=$((runs + 1)); fails=$((fails + 1)); echo "  FAIL $*"; }

part_of() { # FILE: its state-root section, paths moved under $T
	sed -n '/^# >>> state root/,/^# <<< state root/p' "$1" | sed \
		-e 's#/dev/block/by-name#$T/by-name#g' \
		-e 's#/sys/class/block#$T/sys/class/block#g' \
		-e 's#/sys/dev/block#$T/sys/dev/block#g' \
		-e 's#/dev/block/\$(#$T/dev/block/$(#g' \
		-e 's#/proc/self/mountinfo#$T/mountinfo#g' \
		-e 's#\[ -b #[ -e #g'
}
for f in "$here/switch-to-linux/action.sh" "$here/back-to-android"; do
	part_of "$f" | grep -q . || { echo "no state root markers in $f"; exit 2; }
done

setup() {
	T=$(mktemp -d /tmp/tb323fu-state.XXXXXX)
	mkdir -p "$T/by-name" "$T/sys/class/block" "$T/sys/dev/block" "$T/dev/block"
	: > "$T/mountinfo"
}
# part DEV NAME MAJ:MIN [byname]: a partition (byname: also in by-name, as UFS ones are)
part() {
	mkdir -p "$T/sys/class/block/$1"
	printf 'MAJOR=%s\nMINOR=%s\nDEVNAME=%s\nDEVTYPE=partition\nPARTNAME=%s\n' "${3%:*}" "${3#*:}" "$1" "$2" > "$T/sys/class/block/$1/uevent"
	ln -s "../../class/block/$1" "$T/sys/dev/block/$3"
	: > "$T/dev/block/$1"
	[ "${4:-}" = byname ] && ln -s "$T/dev/block/$1" "$T/by-name/$2"
	return 0
}
# the running root (back-to-android's cur_root)
rootfs() { echo "24 1 $1 / / rw,noatime shared:1 - ext4 /dev/x rw" > "$T/mountinfo"; }

action() { # SHELL -> "state dev"
	{ echo "T=$T"; part_of "$here/switch-to-linux/action.sh"; echo 'echo "$state $([ -n "$state" ] && partdev "$state")"'; } > "$T/a.sh"
	$1 "$T/a.sh" 2>"$T/a.err"
}
b2a() { # SHELL -> "state current"
	{ echo "T=$T"; part_of "$here/back-to-android"; echo 'echo "$(state_root) $(cur_root)"'; } > "$T/b.sh"
	$1 "$T/b.sh" 2>"$T/b.err"
}
expect() { # GOT WANT DESC
	[ "$1" = "$2" ] && ok "$3: $2" || { bad "$3: want '$2', got '$1'"; cat "$T"/*.err 2>/dev/null | sed 's/^/        /'; }
}

for SH in "$@"; do
echo "== $SH"
setup
part sda17 baldur-root 259:17 byname; part sda5 boot_a 8:5 byname; part mmcblk0p1 baldur-root-sd 179:1
part mmcblk0p2 tb323fu-ubuntu 179:2; rootfs 259:17
expect "$(action "$SH")" "baldur-root $T/by-name/baldur-root" "action, UFS layout"
expect "$(b2a "$SH")" "baldur-root baldur-root" "back-to-android, UFS layout, on baldur-root"
rootfs 179:2
expect "$(b2a "$SH")" "baldur-root tb323fu-ubuntu" "back-to-android, UFS layout, on tb323fu-ubuntu"
rm -rf "$T"

setup
part sda17 baldur-root 259:17 byname
rm -rf "$T/sys/class/block"	# sysfs unreadable: by-name alone, as before
expect "$(action "$SH")" "baldur-root $T/by-name/baldur-root" "action, by-name only"
rm -rf "$T"

setup
part sda5 boot_a 8:5 byname; part mmcblk0p1 baldur-root-sd 179:1; part mmcblk0p2 tb323fu-arch 179:2
rootfs 179:2
expect "$(action "$SH")" "baldur-root-sd $T/dev/block/mmcblk0p1" "action, SD baldur-root-sd (not in by-name)"
expect "$(b2a "$SH")" "baldur-root-sd tb323fu-arch" "back-to-android, SD baldur-root-sd"
rm -rf "$T"

setup
part sda5 boot_a 8:5 byname; part mmcblk0p1 tb323fu-ubuntu 179:1; part mmcblk0p2 tb323fu-arch 179:2
part mmcblk0p3 tb323fu-fedora 179:3; rootfs 179:2
expect "$(action "$SH")" "tb323fu-arch $T/dev/block/mmcblk0p2" "action, SD tb323fu-* only"
expect "$(b2a "$SH")" "tb323fu-arch tb323fu-arch" "back-to-android, SD tb323fu-* only, on arch"
rm -rf "$T"

setup
part sda17 baldur-root 259:17 byname; part mmcblk0p5 baldur-root 179:5
expect "$(action "$SH")" "baldur-root $T/by-name/baldur-root" "action, the same name on UFS and SD: by-name (UFS)"
rm -rf "$T"

setup
part sda5 boot_a 8:5 byname; part sda6 tb323fu 8:6 byname; part sda7 userdata 8:7 byname
expect "$(action "$SH")" " " "action, no candidate (\"tb323fu\" alone is none)"
rm -rf "$T"
done
echo "$((runs - fails))/$runs passed"
[ $fails -eq 0 ]
