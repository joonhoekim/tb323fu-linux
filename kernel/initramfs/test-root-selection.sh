#!/bin/sh
# SPDX-License-Identifier: MIT
# test-root-selection.sh -- run the root selection of init (the part between
# ">>> root selection" and "<<< root selection") offline, against fake
# partitions: a fake /sys/class/block, fake disks (directories) and stubs for
# mount, umount, switch_root, chroot, kill. Nothing real is mounted. Also the
# modules-image move into the chosen root and modload (deps by modules.dep).
#
#   sh kernel/initramfs/test-root-selection.sh [SHELL ...]
#
# Default shells: "busybox sh" (with busybox's grep, sort, sed, ... first in
# PATH, as in the initramfs) and dash, whichever exist. Needs a Linux host.
set -u
here=$(cd "$(dirname "$0")" && pwd)
INIT=$here/init
[ $# -gt 0 ] || {
	command -v busybox >/dev/null && set -- "busybox sh"
	command -v dash >/dev/null && set -- "$@" dash
}
[ $# -gt 0 ] || { echo "no shell to test with (busybox, dash)"; exit 2; }

fails=0; runs=0
ok() { runs=$((runs + 1)); echo "  ok   $*"; }
bad() { runs=$((runs + 1)); fails=$((fails + 1)); echo "  FAIL $*"; }

# the section, with its absolute paths moved under $T
section() {
	sed -n '/^# >>> root selection/,/^# <<< root selection/p' "$INIT" | sed \
		-e 's#/sys/class/block#$T/sys/class/block#g' \
		-e 's#/sys/kernel/debug/gpio#$T/gpio#g' \
		-e 's#/sel#$T/sel#g' \
		-e 's#/newroot#$T/newroot#g' \
		-e 's#/tmp/#$T/tmp/#g' \
		-e 's#c 64 /etc/android-boot#c 64 $T/etc/android-boot#g' \
		-e 's#/dev/tty1#/dev/null#g' \
		-e 's#/proc/version#$T/proc/version#g' \
		-e 's#/proc/sys/vm/drop_caches#$T/drop_caches#g' \
		-e 's#kill -\(TERM\|KILL\) -1#: &#' \
		-e 's#exec switch_root#exec $T/bin/switch_root#'
}
# kill -1 here would kill every process of the user (the shells' kill is a
# builtin, no PATH stub reaches it): refuse to run if the rewrite missed one
section | grep 'kill -[A-Z]* -1' | grep -v ': kill' | grep -q . && { echo "kill -1 left in the section; not running"; exit 2; }
grep -q '^# >>> root selection' "$INIT" && grep -q '^# <<< root selection' "$INIT" ||
	{ echo "no root selection markers in $INIT"; exit 2; }

# new case: a fresh tree
setup() {
	M=
	T=$(mktemp -d /tmp/tb323fu-rootsel.XXXXXX)
	mkdir -p "$T/sys/class/block" "$T/disk" "$T/bin" "$T/tmp" "$T/etc" "$T/proc"
	echo "Linux version 7.3.0-rc4-tb323fu-t99 (u@h) (clang) #1 SMP" > "$T/proc/version"
	# stubs (mount understands what init does with it)
	cat > "$T/bin/mount" <<EOF
#!/bin/sh
T=$T
# the modules image: log where it goes (\$T/movefail: the move fails)
[ "\$1" = --move ] && [ "\$2" = "\$T/modimg" ] && { echo "MOVE \${3#\$T}" >> "\$T/log"; [ ! -e "\$T/movefail" ]; exit; }
case " \$* " in *" --move "*|*remount*) exit 0 ;; esac
eval dir=\\\${\$#}; eval dev=\\\${\$((\$# - 1))}
n=\${dev#/dev/}
[ -d "\$T/disk/\$n" ] && [ ! -e "\$T/disk/\$n.nomount" ] || exit 32
rmdir "\$dir" 2>/dev/null; rm -f "\$dir"; ln -s "\$T/disk/\$n" "\$dir"
echo "mount \$n \${dir#\$T}" >> "\$T/log"
EOF
	cat > "$T/bin/umount" <<EOF
#!/bin/sh
T=$T
for a; do d=\$a; done
[ -L "\$d" ] && rm -f "\$d" && echo "umount \${d#\$T}" >> "\$T/log"
[ "\$d" = "\$T/modimg" ] && echo "umount /modimg" >> "\$T/log"
exit 0
EOF
	cat > "$T/bin/switch_root" <<EOF
#!/bin/sh
echo "SWITCH \$(basename "\$(readlink "\$1")") \$2" >> "$T/log"
EOF
	printf '#!/bin/sh\nexit 1\n' > "$T/bin/chroot"
	for s in kill sync usleep; do printf '#!/bin/sh\nexit 0\n' > "$T/bin/$s"; done
	chmod +x "$T/bin/"*
	: > "$T/log"
}
# part DEV NAME [init] [nomount]: a partition with a disk behind it
part() {
	mkdir -p "$T/sys/class/block/$1" "$T/disk/$1"
	printf 'MAJOR=8\nDEVNAME=%s\nDEVTYPE=partition\nPARTNAME=%s\n' "$1" "$2" > "$T/sys/class/block/$1/uevent"
	dev=$1; shift 2
	for o; do
		case $o in
		init) mkdir -p "$T/disk/$dev/sbin"; printf '#!/bin/sh\n' > "$T/disk/$dev/sbin/init"; chmod +x "$T/disk/$dev/sbin/init" ;;
		nomount) : > "$T/disk/$dev.nomount" ;;
		esac
	done
}
# file DEV PATH CONTENT: a file on that disk
file() { mkdir -p "$(dirname "$T/disk/$1/$2")"; printf '%s\n' "$3" > "$T/disk/$1/$2"; }

run() { # SHELL
	{
		echo "T=$T"
		echo 'say() { echo "SAY $*" >> $T/log; }'
		echo 'end=hold'
		# the shared modules image (init mounts it at the top): with M=1
		echo 'rel=7.3.0-rc4-tb323fu-t99'
		if [ -n "$M" ]; then mkdir -p "$T/modimg"; echo "modimg=$T/modimg"; else echo 'modimg='; fi
		# functions win over builtins and busybox applets alike
		for s in mount umount chroot kill sync usleep; do echo "$s() { $T/bin/$s \"\$@\"; }"; done
		# kernel trial: no restart, uname from a file, boot_a under $T/dev
		echo "reboot() { echo REBOOT >> $T/log; }; uname() { cat $T/uname; }; TB323FU_DEVDIR=$T/dev"
		grep -E '^(hash_ok|waitfor)\(\)' "$INIT"
		section
	} > "$T/run.sh"
	p=$T/bin:$PATH
	case $1 in
	busybox*)
		mkdir -p "$T/bb"
		for a in $(busybox --list); do [ -e "$T/bin/$a" ] || ln -s "$(command -v busybox)" "$T/bb/$a"; done
		p=$T/bin:$T/bb:$PATH ;;
	esac
	PATH=$p $1 "$T/run.sh" > "$T/out" 2>&1
}
switched() { sed -n 's/^SWITCH \([^ ]*\) .*/\1/p' "$T/log"; }
expect_switch() { # DEV WHY DESC
	got=$(switched)
	if [ "$got" = "$1" ] && grep -q "^SAY  switching to root .* ($2), init" "$T/log"; then ok "$3 -> $1 ($2)"
	else bad "$3: want $1 ($2), got '${got:-none}'"; sed 's/^/        /' "$T/log" "$T/out"; fi
}
expect_log() { # PATTERN DESC
	grep -q -- "$1" "$T/log" && ok "$2" || { bad "$2 (no '$1')"; sed 's/^/        /' "$T/log"; }
}
H1=$(printf 'a%.0s' $(seq 64)); H2=$(printf 'b%.0s' $(seq 64))

for SH in "$@"; do
echo "== $SH"

setup; part sda17 baldur-root init; part mmcblk0p1 baldur-root-sd init
part mmcblk0p2 tb323fu-ubuntu init
run "$SH"; expect_switch sda17 default "UFS layout, no selection"
expect_log "^mount sda17 /sel" "  selection read from baldur-root"
rm -rf "$T"

setup; part sda17 baldur-root init; part mmcblk0p1 baldur-root-sd init
part mmcblk0p2 tb323fu-ubuntu init
file sda17 etc/tb323fu/boot-next tb323fu-ubuntu; file sda17 etc/tb323fu/boot-default baldur-root-sd
run "$SH"; expect_switch mmcblk0p2 boot-next "UFS layout, boot-next"
[ ! -e "$T/disk/sda17/etc/tb323fu/boot-next" ] && ok "  boot-next consumed" || bad "  boot-next not consumed"
rm -rf "$T"

setup; part sda17 baldur-root init; part mmcblk0p1 baldur-root-sd init
part mmcblk0p2 tb323fu-ubuntu init
file sda17 etc/tb323fu/boot-default tb323fu-ubuntu; file mmcblk0p1 etc/tb323fu/boot-next baldur-root-sd
run "$SH"; expect_switch mmcblk0p2 boot-default "UFS layout, boot-default (a boot-next on the SD root is ignored)"
rm -rf "$T"

setup; part sda17 baldur-root init; part mmcblk0p1 baldur-root-sd init
file sda17 etc/tb323fu/boot-next tb323fu-gone
run "$SH"; expect_switch sda17 default "boot-next names a missing partition"
expect_log "^SAY  root tb323fu-gone: no such partition" "  said so"
rm -rf "$T"

setup; part mmcblk0p1 baldur-root-sd init; part mmcblk0p2 tb323fu-arch init
file mmcblk0p1 etc/tb323fu/boot-default tb323fu-arch
run "$SH"; expect_switch mmcblk0p2 boot-default "no baldur-root: state root baldur-root-sd"
rm -rf "$T"

setup; part mmcblk0p1 tb323fu-ubuntu init; part mmcblk0p2 tb323fu-arch init
part mmcblk0p3 tb323fu-fedora init
run "$SH"; expect_switch mmcblk0p2 default "SD only, tb323fu-* only, no selection: first sorted (arch)"
rm -rf "$T"

setup; part mmcblk0p1 tb323fu-ubuntu init; part mmcblk0p2 tb323fu-arch init
part mmcblk0p3 tb323fu-fedora init
file mmcblk0p2 etc/tb323fu/boot-default tb323fu-fedora; file mmcblk0p1 etc/tb323fu/boot-default tb323fu-ubuntu
file mmcblk0p2 etc/tb323fu/boot-next tb323fu-ubuntu
run "$SH"; expect_switch mmcblk0p1 boot-next "SD only, boot-next on the state root (arch)"
[ ! -e "$T/disk/mmcblk0p2/etc/tb323fu/boot-next" ] && ok "  boot-next consumed on arch" || bad "  boot-next not consumed on arch"
rm -rf "$T"

setup; part mmcblk0p1 tb323fu-ubuntu init; part mmcblk0p2 tb323fu-arch init
part mmcblk0p3 tb323fu-fedora init
file mmcblk0p2 etc/tb323fu/boot-default tb323fu-fedora
run "$SH"; expect_switch mmcblk0p3 boot-default "SD only, boot-default on the state root"
rm -rf "$T"

setup; part sda17 baldur-root; part mmcblk0p1 baldur-root-sd init nomount
part mmcblk0p2 tb323fu-arch; part mmcblk0p3 tb323fu-ubuntu init
run "$SH"; expect_switch mmcblk0p3 fallback "fallback past no init / no mount / no init"
expect_log "^SAY  root baldur-root (/dev/sda17): no init" "  baldur-root: no init"
expect_log "^SAY  root baldur-root-sd (/dev/mmcblk0p1): does not mount" "  baldur-root-sd: does not mount"
expect_log "^SAY  root tb323fu-arch (/dev/mmcblk0p2): no init" "  tb323fu-arch: no init"
rm -rf "$T"

setup; part sda17 baldur-root init; part mmcblk0p5 baldur-root init
run "$SH"; expect_switch sda17 default "the same name twice: UFS (sd*) wins"
rm -rf "$T"

setup; part sda1 persist init
run "$SH"; expect_log "^SAY  no root partition (baldur-root, baldur-root-sd, tb323fu-\*) after 10 s" "no candidate: said so"
expect_log "^SAY  no usable root; staying in the initramfs" "  stays in the initramfs"
[ -z "$(switched)" ] && ok "  no switch" || bad "  switched to $(switched)"
rm -rf "$T"

# the emergency key's hash
setup; part sda17 baldur-root init; part mmcblk0p1 baldur-root-sd init
file sda17 etc/tb323fu/android-boot.sha256 "$H1"; file mmcblk0p1 etc/tb323fu/android-boot.sha256 "$H2"
run "$SH"; [ "$(cat "$T/tmp/android-boot.sha256" 2>/dev/null)" = "$H1" ] && ok "hash from the state root" || bad "hash from the state root: '$(cat "$T/tmp/android-boot.sha256" 2>/dev/null)'"
expect_log "^SAY  emergency key: Android image hash from the root (aaaaaaaaaaaa...)" "  said so"
[ -e "$T/tmp/android-hash-done" ] && ok "  android-hash-done" || bad "  no android-hash-done"
rm -rf "$T"

setup; part sda17 baldur-root init nomount; part mmcblk0p1 baldur-root-sd init
file mmcblk0p1 etc/tb323fu/android-boot.sha256 "$H2"; file mmcblk0p1 etc/tb323fu/boot-default tb323fu-x
run "$SH"; [ "$(cat "$T/tmp/android-boot.sha256" 2>/dev/null)" = "$H2" ] && ok "state root does not mount: hash from the next root" || bad "hash fallback: '$(cat "$T/tmp/android-boot.sha256" 2>/dev/null)'"
expect_switch mmcblk0p1 fallback "  ... and its boot-default is not read"
rm -rf "$T"

setup; part mmcblk0p1 tb323fu-ubuntu init; part mmcblk0p2 tb323fu-arch init
file mmcblk0p2 etc/android-boot.sha256 "$H1"
run "$SH"; [ "$(cat "$T/tmp/android-boot.sha256" 2>/dev/null)" = "$H1" ] && ok "SD only: hash (old path) from the state root" || bad "SD only hash"
rm -rf "$T"

setup; part sda17 baldur-root init; file sda17 etc/tb323fu/android-boot.sha256 "$H1"
printf '%s\n' "$H2" > "$T/etc/android-boot.sha256"
run "$SH"; [ ! -e "$T/tmp/android-boot.sha256" ] || [ "$(cat "$T/tmp/android-boot.sha256")" = "$H1" ] && ok "image hash present: no message" || bad "image hash"
grep -q "emergency key" "$T/log" && bad "  emergency key message with an image hash" || ok "  no emergency key message"
rm -rf "$T"

setup; part sda17 baldur-root init
run "$SH"; expect_log "^SAY  emergency key off: no Android image hash" "no hash anywhere: emergency key off"
rm -rf "$T"

# the volume-up menu (5 s, nothing pressed): boots what was current
setup; part mmcblk0p1 tb323fu-ubuntu init; part mmcblk0p2 tb323fu-arch init
file mmcblk0p2 etc/tb323fu/boot-menu ""; file mmcblk0p2 etc/tb323fu/boot-default tb323fu-ubuntu
run "$SH"; expect_switch mmcblk0p1 boot-next "menu on the state root, idle: the default"
expect_log "^SAY  boot menu: tb323fu-ubuntu" "  menu said tb323fu-ubuntu"
rm -rf "$T"

# kernel trial (kernel updates): starts counted on the state root, rollback after max
V27="Linux version 7.3.0-rc4-tb323fu-t27 (u@h) (clang) #1 SMP PREEMPT Wed Oct 1"
V28="Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang) #1 SMP PREEMPT Thu Oct 2"
# trialsetup TRIES [GOOD_SHA]: baldur-root (sda17) with a trial of t28, running t28; boot_a = sda5
trialsetup() {
	setup; part sda17 baldur-root init; part sda5 boot_a
	mkdir -p "$T/dev"; printf 'T28-IMAGE' > "$T/dev/sda5"
	mkdir -p "$T/disk/sda17/var/lib/tb323fu"
	printf 'T27-GOOD-IMAGE' > "$T/disk/sda17/var/lib/tb323fu/linux-good.img"
	gs=${2:-$(printf 'T27-GOOD-IMAGE' | sha256sum | cut -c1-64)}
	ts=$(printf 'T28-IMAGE' | sha256sum | cut -c1-64)
	printf '%s\n' "good=7.3.0-rc4-tb323fu-t27" "good_sha256=$gs" "good_version=$V27" "trial=7.3.0-rc4-tb323fu-t28" \
		"trial_sha256=$ts" "trial_version=$V28" "trial_serial=28" "trial_channel=stable" "tries=$1" "max=2" \
		> "$T/disk/sda17/var/lib/tb323fu/kernel-state"
	echo "$V28" > "$T/proc/version"
}
kstate() { sed -n "s/^$1=//p" "$T/disk/sda17/var/lib/tb323fu/kernel-state"; }

trialsetup 0; run "$SH"
[ "$(kstate tries)" = 1 ] && ok "trial: first start counted (tries 1)" || bad "trial: tries '$(kstate tries)'"
expect_log "^SAY  kernel 7.3.0-rc4-tb323fu-t28 on trial: start 1 of 2 (kept once a system runs 90 s)" "  said so"
expect_switch sda17 default "  boots on"
rm -rf "$T"

trialsetup 1; sed -i 's/^trial_channel=.*/trial_channel=testing/' "$T/disk/sda17/var/lib/tb323fu/kernel-state"; run "$SH"
[ "$(kstate tries)" = 2 ] && ok "trial: second start counted (testing channel)" || bad "trial: tries '$(kstate tries)'"
expect_log "start 2 of 2 (kept when you press Keep)" "  said so (Keep)"
rm -rf "$T"

trialsetup 2; run "$SH"
[ "$(cat "$T/dev/sda5")" = T27-GOOD-IMAGE ] && ok "trial: third start: linux-good.img back in boot_a" || bad "trial: boot_a = '$(cat "$T/dev/sda5")'"
[ "$(kstate failed)" = 7.3.0-rc4-tb323fu-t28 ] && [ -z "$(kstate trial)" ] && [ -z "$(kstate tries)" ] && ok "  failed= recorded, trial cleared" || bad "  state: $(cat "$T/disk/sda17/var/lib/tb323fu/kernel-state")"
[ "$(kstate failed_sha256)" = "$(printf 'T28-IMAGE' | sha256sum | cut -c1-64)" ] && ok "  failed_sha256" || bad "  failed_sha256"
[ "$(kstate good)" = 7.3.0-rc4-tb323fu-t27 ] && ok "  good kept" || bad "  good lost"
expect_log "^REBOOT" "  restarted"
[ -z "$(switched)" ] && ok "  no root booted with the failed kernel" || bad "  switched to $(switched)"
rm -rf "$T"

trialsetup 2 "$(printf 'c%.0s' $(seq 64))"; run "$SH"
[ "$(cat "$T/dev/sda5")" = T28-IMAGE ] && ok "trial: linux-good.img not matching its record: not written" || bad "trial: wrote a bad linux-good"
expect_log "does not match its record" "  said so"
expect_switch sda17 default "  boots the trial kernel on"
rm -rf "$T"

trialsetup 2; rm "$T/dev/sda5"; mkdir "$T/dev/sda5"; run "$SH"
expect_log "does not read back as linux-good.img" "trial: boot_a write fails: said so"
expect_log "^SAY  staying in the initramfs" "  stays in the initramfs"
[ -z "$(switched)" ] && ok "  no switch" || bad "  switched to $(switched)"
rm -rf "$T"

trialsetup 0; echo "$V27" > "$T/proc/version"; run "$SH"
[ "$(kstate tries)" = 0 ] && ok "trial: another kernel runs (installed, not yet started): not counted" || bad "trial: counted for another kernel"
rm -rf "$T"

trialsetup 0; echo "gpio101 : in  low" > "$T/gpio"; run "$SH"
[ "$(kstate tries)" = 0 ] && ok "trial: volume-up held: not counted" || bad "trial: counted with volume-up"
expect_log "volume-up held, this start is not counted" "  said so"
rm -rf "$T"

trialsetup 0; sed -i '/^trial_version=/d' "$T/disk/sda17/var/lib/tb323fu/kernel-state"; echo 7.3.0-rc4-tb323fu-t28 > "$T/uname"; run "$SH"
[ "$(kstate tries)" = 1 ] && ok "trial: no trial_version: matched by uname -r" || bad "trial: uname match"
rm -rf "$T"

trialsetup 2; rm "$T/disk/sda17/var/lib/tb323fu/linux-good.img"; run "$SH"
expect_log "no linux-good.img to go back to" "trial: no linux-good.img: said so"
expect_switch sda17 default "  boots on"
rm -rf "$T"

# the shared modules image: moved into the chosen root's lib/modules
R=7.3.0-rc4-tb323fu-t99
moved() { sed -n 's/^MOVE //p' "$T/log" | tr '\n' ' ' | sed 's/ $//'; }
expect_move() { # WANT DESC
	[ "$(moved)" = "$1" ] && ok "$2" || { bad "$2: want move '$1', got '$(moved)'"; sed 's/^/        /' "$T/log" "$T/out"; }
}
setup; M=1; part sda17 baldur-root init; ln -s usr/lib "$T/disk/sda17/lib"; mkdir -p "$T/disk/sda17/usr/lib/modules"
run "$SH"; expect_switch sda17 default "modules: merged /usr (lib -> usr/lib)"
expect_move "/newroot/usr/lib/modules/$R" "  image on /usr/lib/modules/<release>"
[ -d "$T/disk/sda17/usr/lib/modules/$R" ] && ok "  mount point created" || bad "  no mount point"
expect_log "^SAY  modules: shared image on /usr/lib/modules/$R" "  said so"
rm -rf "$T"

setup; M=1; part mmcblk0p3 tb323fu-arch init; ln -s /usr/lib "$T/disk/mmcblk0p3/lib"; mkdir -p "$T/disk/mmcblk0p3/usr/lib"
run "$SH"; expect_move "/newroot/usr/lib/modules/$R" "modules: absolute /lib -> /usr/lib resolved inside the root"
[ ! -e "/usr/lib/modules/$R" ] && ok "  nothing created on the host" || bad "  created on the host"
rm -rf "$T"

setup; M=1; part mmcblk0p5 tb323fu-nixos; mkdir -p "$T/disk/mmcblk0p5/nix/var/nix/profiles/system"
printf '#!/bin/sh\n' > "$T/disk/mmcblk0p5/nix/var/nix/profiles/system/init"; chmod +x "$T/disk/mmcblk0p5/nix/var/nix/profiles/system/init"
run "$SH"; expect_move "/newroot/lib/modules/$R" "modules: NixOS (no /lib): /lib/modules/<release> created"
grep -q "^SWITCH mmcblk0p5 /nix/var/nix/profiles/system/init" "$T/log" && ok "  NixOS init" || bad "  NixOS init"
rm -rf "$T"

setup; M=1; part sda17 baldur-root init; file sda17 etc/tb323fu/modules own; file sda17 "lib/modules/$R/modules.dep" ""
run "$SH"; expect_move "" "modules: own mode: not moved"
expect_log "^umount /modimg" "  image unmounted"
expect_log "^SAY  modules: own tree /lib/modules/$R" "  said so"
expect_switch sda17 default "  still boots"
rm -rf "$T"

setup; M=1; part sda17 baldur-root init; file sda17 etc/tb323fu/modules own
run "$SH"; expect_log "^SAY  modules: /etc/tb323fu/modules = own, but baldur-root has no /lib/modules/$R/modules.dep" "modules: own mode without a tree: said so"
rm -rf "$T"

setup; M=1; part sda17 baldur-root init; file sda17 etc/tb323fu/modules overlay
run "$SH"; expect_move "/newroot/lib/modules/$R" "modules: overlay (not implemented): shared"
expect_log "^SAY  modules: /etc/tb323fu/modules = overlay is not supported" "  said so"
rm -rf "$T"

setup; M=1; part sda17 baldur-root init; : > "$T/disk/sda17/lib"
run "$SH"; expect_move "" "modules: mount point cannot be created: not moved"
expect_log "^SAY  modules: cannot create /lib/modules/$R on baldur-root" "  said so"
expect_switch sda17 default "  still boots"
rm -rf "$T"

setup; M=1; part sda17 baldur-root init; : > "$T/movefail"
run "$SH"; expect_log "^SAY  modules: moving the image to /lib/modules/$R failed" "modules: move fails: said so"
expect_switch sda17 default "  still boots"
rm -rf "$T"

setup; M=1; part sda17 baldur-root; part mmcblk0p1 baldur-root-sd init; mkdir -p "$T/disk/mmcblk0p1/usr/lib"; ln -s usr/lib "$T/disk/mmcblk0p1/lib"
run "$SH"; expect_move "/newroot/usr/lib/modules/$R" "modules: only the root that boots gets it (fallback past no init)"
[ ! -e "$T/disk/sda17/lib/modules/$R" ] && ok "  nothing created on the skipped root" || bad "  mount point on the skipped root"
rm -rf "$T"

setup; part sda17 baldur-root init
run "$SH"; expect_move "" "modules: image without a modules squashfs: nothing moved"
grep -q "modules:" "$T/log" && bad "  modules message without an image" || ok "  no modules message"
[ ! -e "$T/disk/sda17/lib" ] && ok "  nothing created" || bad "  created lib/"
rm -rf "$T"
done


# modload (top of init): a module and its dependencies from the modules image,
# by modules.dep -- dependencies first, in reverse order of the list
for SH in "$@"; do
echo "== $SH: modload"
T=$(mktemp -d /tmp/tb323fu-modload.XXXXXX); mkdir -p "$T/m"
d=kernel/drivers
cat > "$T/m/modules.dep" <<EOF
$d/remoteproc/qcom_q6v5_pas.ko: $d/remoteproc/qcom_q6v5.ko $d/remoteproc/qcom_sysmon.ko $d/remoteproc/qcom_common.ko $d/soc/qcom/qcom_pil_info.ko
$d/soc/qcom/qcom_pil_info.ko:
$d/input/touchscreen/nt36536_ts.ko:
EOF
{
	echo "insmod() { echo \"\${1#$T/m/}\" >> $T/log; }"
	echo "modimg=$T/m"
	sed -n '/^modload() {/,/^}/p' "$INIT"
	echo "modload qcom_q6v5_pas; modload q6v5_pas || echo 'not found' >> $T/log; modload nt36536_ts"
} > "$T/run.sh"
PATH=$PATH $SH "$T/run.sh" > "$T/out" 2>&1
want="$d/soc/qcom/qcom_pil_info.ko $d/remoteproc/qcom_common.ko $d/remoteproc/qcom_sysmon.ko $d/remoteproc/qcom_q6v5.ko $d/remoteproc/qcom_q6v5_pas.ko not found $d/input/touchscreen/nt36536_ts.ko"
got=$(tr '\n' ' ' < "$T/log" | sed 's/ $//')
[ "$got" = "$want" ] && ok "deps first, whole names only, no deps" || { bad "modload: got '$got'"; cat "$T/out"; }
rm -rf "$T"
done
echo "$((runs - fails))/$runs passed"
[ $fails -eq 0 ]
