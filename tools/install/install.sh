#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# install.sh -- guided install of mainline Linux on a rooted TB323FU.
#
# PROTOTYPE -- NOT TESTED ON REAL HARDWARE. It walks through docs/install.md
# and calls the repository's own scripts for the real work. Read
# docs/install.md first; use --dry-run to see every command before running it.
#
#   tools/install/install.sh [--dry-run] [STEP ...]
#
# Steps (default: all of them in order, asking before each):
#   check      host tools, the tablet in rooted Android, your backups
#   firmware   extract the firmware from Android (firmware/extract-on-device.sh)
#   wayback    Android boot image into boot_b, "Switch to Linux" module (android/install-module.sh)
#   bootimg    how to build the Linux boot image (prints the commands; needs a kernel tree)
#   sdcard     partition a microSD card in a card reader (Linux host)  -- WIPES THE CARD
#   rootfs     build a root filesystem into a partition (rootfs/<distro>/build-rootfs.sh; arm64 Linux host)
#   boot       write boot_a through Android (stage + Switch to Linux, or tools/cycle.sh)
#   firstboot  wait for Linux and run the first-boot checks (needs DEV_ACCESS=1 in the root)
#   ufs        Linux root on the internal storage -- only prints the manual steps
#
# Environment:
#   WORK=./tb323fu-install      working directory (firmware, config, images)
#   DISTRO=ubuntu               rootfs builder: ubuntu, arch, fedora, nixos, steamos
#   DUMP_DIR=                   your LTBox partition dump (checked in "check")
#   LINUX_BOOT_IMG=$WORK/linux-boot.img   the boot image from "bootimg"
#   MODULES_FROM=               lib/modules/<release> of that kernel (for "rootfs")
#   SD_DEV=                     the card in a reader, e.g. /dev/sdX (for "sdcard")
#   SD_LAYOUT="baldur-root:64G" partitions to create, NAME:SIZE ...; the last may be NAME:0 (rest)
#   ROOT_PART=                  partition the root goes into, default /dev/disk/by-partlabel/$ROOT_PARTLABEL
#   ROOT_PARTLABEL=baldur-root  its GPT name
#   TB323FU_HOST=192.168.7.2    the tablet in Linux (USB network, DEV_ACCESS=1 roots)
#
# Host support: Linux first (every step). macOS: check, firmware, wayback, boot
# (adb only; sdcard/rootfs need Linux). Windows: Git Bash or WSL for the adb
# steps; WSL has no direct USB access without usbipd-win. None of it was run
# against a tablet. Every destructive step explains itself, asks you to type a
# confirmation word and prints how to undo it.
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
DRY=0
steps=()
for a in "$@"; do
	case $a in
	--dry-run|-n) DRY=1 ;;
	-h|--help) sed -n '3,38p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
	check|firmware|wayback|bootimg|sdcard|rootfs|boot|firstboot|ufs) steps+=("$a") ;;
	*) echo "unknown argument: $a (see --help)" >&2; exit 2 ;;
	esac
done
[ ${#steps[@]} -gt 0 ] || steps=(check firmware wayback bootimg sdcard rootfs boot firstboot ufs)

WORK=${WORK:-$PWD/tb323fu-install}
DISTRO=${DISTRO:-ubuntu}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-baldur-root}
LINUX_BOOT_IMG=${LINUX_BOOT_IMG:-$WORK/linux-boot.img}
H=${TB323FU_HOST:-192.168.7.2}

# ---- helpers ---------------------------------------------------------------
b= r=
[ -t 1 ] && { b=$(tput bold 2>/dev/null || true); r=$(tput sgr0 2>/dev/null || true); }
say()  { printf '%s\n' "$*"; }
head_() { printf '\n%s== %s ==%s\n' "$b" "$*" "$r"; }
warn() { printf '%s!! %s%s\n' "$b" "$*" "$r" >&2; }
die()  { warn "$*"; exit 1; }
# run CMD...: execute, or print it in --dry-run
run() {
	if [ $DRY = 1 ]; then printf '  [dry-run] %s\n' "$*"; return 0; fi
	printf '  + %s\n' "$*"; "$@"
}
# sh_ 'shell string': the same for pipelines
sh_() {
	if [ $DRY = 1 ]; then printf '  [dry-run] %s\n' "$1"; return 0; fi
	printf '  + %s\n' "$1"; bash -c "$1"
}
# adb shell as root (prints the command in --dry-run, returns empty output)
su_() {
	if [ $DRY = 1 ]; then printf '  [dry-run] adb shell su -c %q\n' "$1" >&2; return 0; fi
	MSYS_NO_PATHCONV=1 adb shell "su -c '$1'" | tr -d '\r'
}
# ask QUESTION DEFAULT -> answer on stdout (the default in --dry-run)
ask() {
	local a
	if [ $DRY = 1 ]; then echo "$2"; return; fi
	read -r -p "$1 [$2] " a </dev/tty || a=
	echo "${a:-$2}"
}
# yesno QUESTION: 0 = yes (always yes in --dry-run, so the whole path is shown)
yesno() {
	local a
	[ $DRY = 1 ] && { say "  [dry-run] $1 -> yes"; return 0; }
	read -r -p "$1 [y/N] " a </dev/tty || a=
	case $a in y|Y|yes) return 0 ;; *) return 1 ;; esac
}
# confirm WORD: the user must type WORD exactly
confirm() {
	local a
	if [ $DRY = 1 ]; then say "  [dry-run] would ask you to type: $1"; return 0; fi
	read -r -p "Type ${b}$1${r} to continue, anything else skips: " a </dev/tty || a=
	[ "$a" = "$1" ] || { say "  skipped"; return 1; }
}
undo() { printf '  %sUndo:%s %s\n' "$b" "$r" "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
sha() { if have sha256sum; then sha256sum "$1" | cut -c1-64; else shasum -a 256 "$1" | cut -c1-64; fi; }
hostos() {
	case $(uname -s) in
	Linux) grep -qi microsoft /proc/version 2>/dev/null && echo wsl || echo linux ;;
	Darwin) echo macos ;;
	MINGW*|MSYS*|CYGWIN*) echo windows ;;
	*) uname -s ;;
	esac
}
need_linux() { [ "$(hostos)" = linux ] || { warn "this step needs a Linux host (this is $(hostos)) -- see docs/install.md"; return 1; }; }
android_hash() { [ -s "$WORK/config/android-boot.sha256" ] && head -c64 "$WORK/config/android-boot.sha256"; }

# ---- steps -----------------------------------------------------------------
step_check() {
	head_ "check: host, tablet, backups"
	say "host: $(hostos) $(uname -m)   repository: $repo   work dir: $WORK"
	local t miss=
	for t in adb tar gzip; do have $t || miss="$miss $t"; done
	have sha256sum || have shasum || miss="$miss sha256sum"
	[ -z "$miss" ] && say "required tools: ok" || warn "missing:$miss (adb: Android platform-tools)"
	for t in sgdisk mkfs.ext4 lsblk ssh scp python3 clang make; do
		have $t && say "  optional $t: yes" || say "  optional $t: no"
	done
	run mkdir -p "$WORK/config"

	say "tablet (rooted Android, USB debugging, Shell allowed in KernelSU):"
	if [ $DRY = 0 ]; then
		[ "$(adb get-state 2>/dev/null | tr -d '\r')" = device ] || { warn "no adb device -- connect the tablet in Android with USB debugging on"; return 1; }
		local model slot vb id
		model=$(adb shell getprop ro.product.model | tr -d '\r')
		slot=$(adb shell getprop ro.boot.slot_suffix | tr -d '\r')
		vb=$(adb shell getprop ro.boot.verifiedbootstate | tr -d '\r')
		id=$(su_ id)
		say "  model $model, slot $slot, verified boot $vb, root: ${id%% *}"
		case $model in *TB323FU*) ;; *) warn "this is not a TB323FU ($model)"; return 1 ;; esac
		[ "$slot" = _a ] || { warn "slot is $slot; everything here assumes slot _a"; return 1; }
		case $id in uid=0*) ;; *) warn "no root for adb shell: allow Shell in the KernelSU manager"; return 1 ;; esac
	else
		say "  [dry-run] adb get-state; getprop ro.product.model / ro.boot.slot_suffix / ro.boot.verifiedbootstate; su -c id"
	fi

	say "backups (docs/rooting.md, step 1):"
	local d=${DUMP_DIR:-}
	[ -n "$d" ] || d=$(ask "folder of your LTBox partition dump" "")
	local p ok=1
	if [ -z "$d" ] && [ $DRY = 1 ]; then say "  [dry-run] would check DUMP_DIR for persist.img devinfo.img oemowninfo.img modemst1.img modemst2.img fsg.img boot_a.img init_boot_a.img efisp.img"
	else
		for p in persist devinfo oemowninfo modemst1 modemst2 fsg boot_a init_boot_a efisp; do
			[ -s "$d/$p.img" ] || { warn "  missing in the dump: $p.img"; ok=0; }
		done
		[ $ok = 1 ] && say "  dump: the device-specific partitions are there ($d)" ||
			{ warn "no complete dump -- make one first (docs/rooting.md, '1. Back up') and keep a copy off this PC"; return 1; }
	fi
	say "  Also keep LTBox's root backup folder (init_boot.img + manifest.json) next to the dump."
}

step_firmware() {
	head_ "firmware: extract from Android (nothing on the tablet changes)"
	say "Copies every file of firmware/manifest.tsv from Android's /vendor into a tree laid out like /lib/firmware."
	yesno "extract now?" || return 0
	run adb push "$repo/firmware/manifest.tsv" "$repo/firmware/extract-on-device.sh" /data/local/tmp/
	su_ 'sh /data/local/tmp/extract-on-device.sh /data/local/tmp/tb323fu-firmware' | tail -5
	su_ 'tar -C /data/local/tmp -cf /data/local/tmp/tb323fu-firmware.tar tb323fu-firmware'
	run mkdir -p "$WORK/fw"
	run adb pull /data/local/tmp/tb323fu-firmware.tar "$WORK/fw/"
	run tar -C "$WORK/fw" -xf "$WORK/fw/tb323fu-firmware.tar"
	say "firmware: $WORK/fw/tb323fu-firmware/lib/firmware (FIRMWARE_FROM for the rootfs builders)"
}

step_wayback() {
	head_ "wayback: Android's boot image in boot_b"
	say "boot_b is never booted; it keeps a copy of your Android boot image so every tool can put Android back."
	local a bb
	a=$(su_ 'sha256sum /dev/block/by-name/boot_a' | cut -c1-64)
	bb=$(su_ 'sha256sum /dev/block/by-name/boot_b' | cut -c1-64)
	say "  boot_a ${a:-?}   boot_b ${bb:-?}"
	[ $DRY = 1 ] || [ -n "$a" ] || { warn "cannot read boot_a over adb (root for Shell?)"; return 1; }
	if [ $DRY = 1 ] || [ "$a" != "$bb" ]; then
		say "boot_b differs from the running Android boot image: copy boot_a into boot_b."
		warn "WRITES boot_b (slot _b cannot boot on this tablet, so nothing that boots changes)"
		undo "nothing to undo for booting; a stock boot image is also in your dump (boot_a.img) and the firmware package"
		if confirm "WRITE BOOT_B"; then run "$repo/android/install-module.sh" prepare-boot-b || return 1
		else return 0; fi
	fi
	say "Install the 'Switch to Linux' KernelSU module (refuses unless boot_a == boot_b)."
	yesno "install the module?" && run "$repo/android/install-module.sh" install
	bb=$(su_ 'sha256sum /dev/block/by-name/boot_b' | cut -c1-64)
	[ $DRY = 1 ] && bb='<sha256 of boot_b>'
	sh_ "mkdir -p '$WORK/config' && echo '$bb' > '$WORK/config/android-boot.sha256'"
	say "Android boot hash -> $WORK/config/android-boot.sha256 (CONFIG_FROM for the builders, -a for the initramfs)"
	su_ 'dd if=/dev/block/by-name/boot_b of=/data/local/tmp/stock-boot.img 2>/dev/null'
	run adb pull /data/local/tmp/stock-boot.img "$WORK/stock-boot.img"
	say "stock boot image -> $WORK/stock-boot.img (tools/build-boot.sh -s)"
}

step_bootimg() {
	head_ "bootimg: build the Linux boot image (on a Linux PC; not automated here)"
	cat <<EOF
No release images exist yet. Build kernel, modules, initramfs and boot image as in
docs/install.md, step 3 -- in short (KERNEL = your kernel tree with kernel/patches applied):

  make ARCH=arm64 LLVM=1 O=out -j\$(nproc) dtbs modules
  make ARCH=arm64 LLVM=1 O=out INSTALL_MOD_PATH=\$PWD/mods modules_install
  $repo/kernel/initramfs/build.sh -k out -b BUSYBOX_STATIC \\
      -f $WORK/fw/tb323fu-firmware -a $WORK/config/android-boot.sha256 -l initramfs.list initramfs.cpio.gz
  $repo/tools/build-boot.sh -k . -o out -s $WORK/stock-boot.img -i initramfs.list $LINUX_BOOT_IMG

Then set MODULES_FROM=\$KERNEL/mods/lib/modules/<release> for the rootfs step.
EOF
	if [ -s "$LINUX_BOOT_IMG" ]; then say "found $LINUX_BOOT_IMG ($(sha "$LINUX_BOOT_IMG" | cut -c1-16)…)"
	else say "not found yet: $LINUX_BOOT_IMG"; fi
}

step_sdcard() {
	head_ "sdcard: partition a microSD card in a card reader -- WIPES THE CARD"
	need_linux || [ $DRY = 1 ] || return 0
	have sgdisk && have mkfs.ext4 || { warn "needs sgdisk (gdisk) and mkfs.ext4 (e2fsprogs)"; [ $DRY = 1 ] || return 0; }
	local layout=${SD_LAYOUT:-baldur-root:64G} dev=${SD_DEV:-}
	say "Partition names matter: the initramfs boots baldur-root, baldur-root-sd and tb323fu-* partitions."
	say "The first present of those (in that order, tb323fu-* sorted) holds the boot selection and is the default."
	lsblk -d -o NAME,SIZE,MODEL,TRAN,RM 2>/dev/null
	[ -n "$dev" ] || dev=$(ask "card device (whole disk, e.g. /dev/sdX)" "/dev/sdX")
	if [ $DRY = 0 ]; then
		[ -b "$dev" ] || { warn "$dev is not a block device"; return 1; }
		[ "$(cat /sys/block/"$(basename "$dev")"/removable 2>/dev/null)" = 1 ] ||
			warn "$dev does not report itself as removable -- make very sure it is the card"
		if lsblk -no MOUNTPOINT "$dev" | grep -q .; then warn "$dev has mounted partitions; unmount them first"; return 1; fi
	fi
	say "Plan for $dev: $layout (ext4 each, label = GPT name)"
	warn "EVERYTHING ON $dev WILL BE ERASED"
	undo "none for the old card contents. To give the card back to Android: format it in Android's storage settings"
	confirm "$(basename "$dev")" || return 0
	run sgdisk --zap-all "$dev"
	local i=1 e name size args=()
	for e in $layout; do
		name=${e%%:*}; size=${e#*:}
		if [ "$size" = 0 ]; then args+=(-n "$i:0:0"); else args+=(-n "$i:0:+$size"); fi
		args+=(-t "$i:8300" -c "$i:$name"); i=$((i + 1))
	done
	run sgdisk "${args[@]}" "$dev"
	run partprobe "$dev"
	i=1
	for e in $layout; do
		name=${e%%:*}
		local part="$dev$i"; case $dev in *[0-9]) part="${dev}p$i" ;; esac
		run mkfs.ext4 -F -L "$name" "$part"; i=$((i + 1))
	done
	run sgdisk -p "$dev"
}

step_rootfs() {
	head_ "rootfs: build $DISTRO into a partition (arm64 Linux host, as root)"
	local builder="$repo/rootfs/$DISTRO/build-rootfs.sh"
	[ -f "$builder" ] || { warn "no builder $builder"; return 1; }
	need_linux || [ $DRY = 1 ] || return 0
	[ "$(uname -m)" = aarch64 ] || { warn "the builders run only on an arm64 host (this is $(uname -m)); see docs/install.md, step 5"; [ $DRY = 1 ] || return 0; }
	[ "$(id -u)" = 0 ] || { warn "run this step as root (sudo -E)"; [ $DRY = 1 ] || return 0; }
	local part=${ROOT_PART:-/dev/disk/by-partlabel/$ROOT_PARTLABEL} mnt="$WORK/mnt"
	local mods=${MODULES_FROM:-}
	[ -n "$mods" ] || mods=$(ask "kernel modules of your boot image (…/lib/modules/<release>)" "")
	[ $DRY = 1 ] || [ -d "$mods" ] || { warn "no modules directory: '$mods'"; return 1; }
	say "Target: $part (GPT name $ROOT_PARTLABEL), mounted at $mnt; must be an empty ext4 filesystem."
	say "Builder options are documented at the top of $builder (DESKTOP, DEV_USER, DEV_ACCESS, ...)."
	run mkdir -p "$mnt"
	run mount "$part" "$mnt" || return 1
	if [ $DRY = 0 ] && [ -n "$(ls -A "$mnt" | grep -v '^lost+found$')" ]; then
		warn "$part is not empty -- refusing (re-running a builder on its own result is allowed by the builders, but not from here)"
		run umount "$mnt"; return 1
	fi
	warn "WRITES a whole system into $part"
	undo "mkfs.ext4 -F -L $ROOT_PARTLABEL $part (or delete the partition)"
	confirm "BUILD" || { run umount "$mnt"; return 0; }
	run env ROOT_PARTLABEL="$ROOT_PARTLABEL" HOSTNAME_NEW="${HOSTNAME_NEW:-$ROOT_PARTLABEL}" \
		MODULES_FROM="$mods" FIRMWARE_FROM="$WORK/fw/tb323fu-firmware/lib/firmware" CONFIG_FROM="$WORK/config" \
		sh "$builder" "$mnt"
	run umount "$mnt"
}

step_boot() {
	head_ "boot: write the Linux boot image to boot_a through Android"
	local hash
	hash=$(android_hash) || true
	[ -z "$hash" ] && [ $DRY = 1 ] && hash="<sha256 of boot_b>"
	[ -n "$hash" ] || { warn "no $WORK/config/android-boot.sha256 -- run the wayback step first"; return 1; }
	[ $DRY = 1 ] || [ -s "$LINUX_BOOT_IMG" ] || { warn "no boot image $LINUX_BOOT_IMG (bootimg step)"; return 1; }
	say "a) stage it for the 'Switch to Linux' module (writes only /data/adb), then press its Action button yourself"
	say "b) tools/cycle.sh writes boot_a over adb, checks the hash and reboots"
	local how; how=$(ask "a or b" a)
	warn "boot_a will hold Linux; Android comes back from boot_b"
	undo "in Linux: back-to-android $hash  |  hold volume up + volume down 10 s  |  EDL: LTBox Flash Partitions boot_a <- your Android boot image (docs/recovery.md)"
	case $how in
	a) run "$repo/android/install-module.sh" stage "$LINUX_BOOT_IMG" &&
		say "Now: KernelSU manager -> Modules -> Switch to Linux -> Action." ;;
	b) confirm "FLASH BOOT_A" && run env ANDROID_BOOT_SHA256="$hash" "$repo/tools/cycle.sh" "$LINUX_BOOT_IMG" ;;
	*) say "skipped" ;;
	esac
}

step_firstboot() {
	head_ "firstboot: checks"
	cat <<EOF
On the panel: bootloader logo -> initramfs boot summary -> "switching to root <name> ..." -> your system.
Volume up held during the summary keeps the initramfs (USB serial shell). Power + volume down forces it off.
EOF
	if ! have ssh; then say "no ssh here -- run the checks on the tablet: uname -r; ls /lib/modules/\$(uname -r); systemctl is-system-running"; return 0; fi
	say "Waiting up to 180 s for SSH at root@$H (roots built with DEV_ACCESS=1 and your key) ..."
	local ok=0 i
	if [ $DRY = 1 ]; then say "  [dry-run] ssh root@$H true (every 5 s, 36 tries)"; ok=1
	else for i in $(seq 1 36); do ssh -o BatchMode=yes -o ConnectTimeout=5 root@"$H" true 2>/dev/null && { ok=1; break; }; sleep 5; done; fi
	[ $ok = 1 ] || { say "no SSH -- check on the tablet itself, or docs/recovery.md#linux-does-not-boot"; return 0; }
	run ssh -o BatchMode=yes root@"$H" 'uname -r; ls -d /lib/modules/$(uname -r); systemctl is-system-running; cat /etc/tb323fu/android-boot.sha256; command -v tb323fu-ctl >/dev/null && tb323fu-ctl boot list'
	say "Then test the way back once: back-to-android <hash> (or the 10 s key chord), and Switch to Linux again."
}

step_ufs() {
	head_ "ufs: a Linux root on the internal storage -- not automated"
	cat <<EOF
This step shrinks Android's userdata and WIPES ANDROID'S DATA (a factory reset). The
prototype does not do it for you: follow docs/install.md, step 8, by hand, from Linux
booted from the SD card, and save the partition table first (sgdisk -b).
EOF
}

say "${b}tb323fu guided install -- PROTOTYPE, untested on real hardware${r}$([ $DRY = 1 ] && echo ' (dry run: nothing is executed)')"
say "Guide: $repo/docs/install.md"
for s in "${steps[@]}"; do
	if [ ${#steps[@]} -gt 1 ] && [ $DRY = 0 ]; then yesno "next step: $s -- run it?" || continue; fi
	"step_$s" || { warn "step $s stopped"; [ $DRY = 1 ] || exit 1; }
done
say ""
say "done."
