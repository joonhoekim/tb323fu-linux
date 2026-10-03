#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# install.sh -- guided install of Ubuntu (GNOME) on the microSD card of a rooted TB323FU.
#
#   tools/install/install.sh [--dry-run] [STEP ...]
#
# Start: rooting done (docs/rooting.md steps 1-3: your own EDL dump kept off the PC,
# KernelSU with Shell allowed, OTA apps off) and the tablet in Android on USB.
# End: Linux in boot_a, Ubuntu with GNOME on the microSD card, Android kept as the
# way back. On Windows run it in a WSL2 Ubuntu terminal (docs/install-windows.md);
# it uses the Windows adb.exe for USB. A native Linux PC works the same way.
#
# Steps, in this order (all by default; each explains itself and asks first):
#   host       packages (apt), arm64 programs through qemu, disk space, adb      1-5 min
#   tablet     the tablet in rooted Android: model, slot _a, root, microSD card  1 min
#   firmware   copy the firmware from Android's /vendor (firmware/)              1 min
#   wayback    Android's boot image in boot_b, Switch to Linux module, hash      1 min
#   download   newest kernel and helper-v releases, checked against SHA256SUMS   1-3 min
#   bootimg    the release kernel packed into YOUR stock boot image              seconds
#   sdcard     GPT on the microSD card, written from Android -- WIPES THE CARD   1 min
#   rootfs     build Ubuntu into an image file here (qemu: 1-2 h with GNOME)     1-2 h
#   write      push the image and write it into the card's partition             10-20 min
#   boot       write the Linux boot image to boot_a, reboot into Linux           1 min
#   firstboot  what a good first boot looks like, the way back                   -
# Finished steps are remembered in $WORK/state and skipped on the next run, so
# after any failure just run it again. Naming steps runs exactly those, again.
# --dry-run prints every command and runs none (it needs no tablet).
#
# Environment (all optional):
#   WORK=~/tb323fu-install     everything goes here; must be on a Linux file system
#                              (in WSL: not under /mnt/c). Needs ~30 GB free
#   ROOT_PARTLABEL=baldur-root-sd   GPT name of the root partition on the card
#   ROOT_SIZE=                 its size (e.g. 64G; 0 = the rest of the card). Asked if unset
#   DEV_USER=                  your user on the tablet (asked; default: your user here)
#   DESKTOP=gnome              gnome or none (none: console only, much faster build)
#   DEV_ACCESS=0               1: developer access in the root (USB network, root shell
#                              on the USB serial port) -- for debugging only
#   STOCK_BOOT=                your stock boot image (e.g. boot_a.img of your EDL dump);
#                              default: read from boot_b. Must match boot_b's hash
#   KERNEL_TAG= HELPER_TAG=    pin releases (default: the newest kernel-t* / helper-v*)
#   GITHUB_TOKEN=              for a private repository (else `gh auth token` is used,
#                              else the public API, else downloads from the browser)
#   ADB=                       adb (Linux) or the Windows adb.exe (WSL) to use
#   ANDROID_SERIAL=            pick one device when several are connected
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
DRY=0
steps=()
all_steps=(host tablet firmware wayback download bootimg sdcard rootfs write boot firstboot)
for a in "$@"; do
	case $a in
	--dry-run|-n) DRY=1 ;;
	-h|--help) sed -n '3,46p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
	*) case " ${all_steps[*]} " in *" $a "*) steps+=("$a") ;; *) echo "unknown argument: $a (see --help)" >&2; exit 2 ;; esac ;;
	esac
done
explicit=0
if [ ${#steps[@]} -gt 0 ]; then explicit=1; else steps=("${all_steps[@]}"); fi

WORK=${WORK:-$HOME/tb323fu-install}
STATE=$WORK/state
REPO_SLUG=${REPO_SLUG:-joonhoekim/tb323fu-linux}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-baldur-root-sd}
DESKTOP=${DESKTOP:-gnome}
DEV_ACCESS=${DEV_ACCESS:-0}
IMG_SIZE=${IMG_SIZE:-24G}
FW=$WORK/fw/tb323fu-firmware/lib/firmware
LINUX_BOOT_IMG=$WORK/linux-boot.img
T=/data/local/tmp

# ---- output and questions ---------------------------------------------------
b='' r=''
[ -t 1 ] && { b=$(tput bold 2>/dev/null || true); r=$(tput sgr0 2>/dev/null || true); }
say()   { printf '%s\n' "$*"; }
head_() { printf '\n%s== %s ==%s\n' "$b" "$*" "$r"; }
warn()  { printf '%s!! %s%s\n' "$b" "$*" "$r" >&2; }
undo()  { printf '  %sUndo:%s %s\n' "$b" "$r" "$*"; }
have()  { command -v "$1" >/dev/null 2>&1; }
run() {
	if [ $DRY = 1 ]; then printf '  [dry-run] %s\n' "$*"; return 0; fi
	printf '  + %s\n' "$*"; "$@"
}
ask() {
	local a
	if [ $DRY = 1 ]; then printf '%s\n' "$2"; return; fi
	read -r -p "$1 [$2] " a </dev/tty || a=
	printf '%s\n' "${a:-$2}"
}
yesno() {
	local a
	[ $DRY = 1 ] && { say "  [dry-run] $1 -> yes"; return 0; }
	read -r -p "$1 [y/N] " a </dev/tty || a=
	case $a in y|Y|yes) return 0 ;; *) return 1 ;; esac
}
confirm() {
	local a
	if [ $DRY = 1 ]; then say "  [dry-run] would ask you to type: $1"; return 0; fi
	read -r -p "Type ${b}$1${r} to go on (anything else stops): " a </dev/tty || a=
	[ "$a" = "$1" ] || { say "  stopped, nothing written"; return 1; }
}
pause() { [ $DRY = 1 ] && return 0; read -r -p "${1:-Press Enter to check again, Ctrl-C to quit} " _ </dev/tty; }
sha() { sha256sum "$1" | cut -c1-64; }
bytes() { numfmt --to=iec-i --suffix=B "$1" 2>/dev/null || echo "$1 bytes"; }

st_get() { [ -f "$STATE" ] && sed -n "s/^$1=//p" "$STATE" | tail -1; }
st_set() {
	[ $DRY = 1 ] && return 0
	mkdir -p "$WORK"
	{ grep -v "^$1=" "$STATE" 2>/dev/null; printf '%s=%s\n' "$1" "$2"; } > "$STATE.new" && mv "$STATE.new" "$STATE"
}

is_wsl() { [ -e /proc/sys/fs/binfmt_misc/WSLInterop ] || grep -qi microsoft /proc/version 2>/dev/null; }
SUDO=sudo; [ "$(id -u)" = 0 ] && SUDO=

# ---- adb ----------------------------------------------------------------------
# su_ 'cmd': run cmd as root on the tablet (KernelSU must allow Shell). The command
# must not contain single quotes.
su_() {
	if [ $DRY = 1 ]; then printf '  [dry-run] adb shell su -c %q\n' "$1" >&2; return 0; fi
	adb shell "su -c '$1'" </dev/null | tr -d '\r'
}
adb_() {
	if [ $DRY = 1 ]; then printf '  [dry-run] adb %s\n' "$*" >&2; return 0; fi
	adb "$@" </dev/null | tr -d '\r'
}
winenv() { (cd /mnt/c 2>/dev/null && cmd.exe /c "echo %$1%" 2>/dev/null | tr -d '\r'); }
win2wsl() { case $1 in [A-Za-z]:\\*|[A-Za-z]:/*) wslpath -u "$1" 2>/dev/null ;; *) printf '%s\n' "$1" ;; esac; }

find_adb_exe() {
	local c u l
	for c in "${ADB:-}" "$(st_get adb_exe)" "$(command -v adb.exe 2>/dev/null)"; do
		[ -n "$c" ] && [ -x "$(win2wsl "$c")" ] && { win2wsl "$c"; return 0; }
	done
	u=$(win2wsl "$(winenv USERPROFILE)"); l=$(win2wsl "$(winenv LOCALAPPDATA)")
	for c in "$l"/Microsoft/WinGet/Links/adb.exe "$l"/Microsoft/WinGet/Packages/Google.PlatformTools*/platform-tools/adb.exe \
		"$l"/Android/Sdk/platform-tools/adb.exe "$u"/platform-tools/adb.exe "$u"/Downloads/platform-tools/adb.exe \
		"$u"/Desktop/platform-tools/adb.exe /mnt/c/platform-tools/adb.exe /mnt/c/adb/adb.exe /mnt/c/adb/platform-tools/adb.exe; do
		[ -n "$u" ] && [ -f "$c" ] && { printf '%s\n' "$c"; return 0; }
	done
	return 1
}

# A small adb in $WORK/bin that runs the Windows adb.exe: WSL itself sees no USB
# devices, Windows does. Local file names (push/pull/install) are turned into
# Windows paths; if adb.exe cannot read a path inside WSL, the file goes through
# a folder in the Windows temp directory instead.
write_adb_wrapper() {
	local exe=$1 stage
	stage=$(win2wsl "$(winenv TEMP)")/tb323fu-adb
	mkdir -p "$WORK/bin"
	cat > "$WORK/bin/adb" <<EOF
#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# adb for WSL (written by tools/install/install.sh): runs the Windows adb.exe.
ADB_EXE='$exe'
STAGE='$stage'
EOF
	cat >> "$WORK/bin/adb" <<'EOF'
set -o pipefail
winpath() {
	local p=$1 w
	case $p in /*) ;; *) p=$PWD/$p ;; esac
	w=$(wslpath -w "$p" 2>/dev/null)
	if [ -z "$w" ]; then
		case $p in /mnt/[a-z]/*) w="${p:5:1}:${p:6}" ;; *) w="//wsl.localhost/$WSL_DISTRO_NAME$p" ;; esac
		w=${w//\//\\}
	fi
	printf '%s' "$w"
}
pre=()
[ -n "${ANDROID_SERIAL:-}" ] && pre=(-s "$ANDROID_SERIAL")
while [ $# -gt 0 ]; do
	case $1 in -s|-t|-H|-P|-L) pre+=("$1" "$2"); shift 2 ;; -*) pre+=("$1"); shift ;; *) break ;; esac
done
cmd=${1:-}; [ $# -gt 0 ] && shift
opts=() args=()
for a; do case $a in -*) opts+=("$a") ;; *) args+=("$a") ;; esac; done
online() { [ "$("$ADB_EXE" "${pre[@]}" get-state 2>/dev/null | tr -d '\r')" = device ]; }
case $cmd in
push)
	n=${#args[@]}; [ "$n" -ge 2 ] || exec "$ADB_EXE" "${pre[@]}" push "$@"
	dst=${args[n-1]}; src=("${args[@]:0:n-1}"); w=()
	for s in "${src[@]}"; do w+=("$(winpath "$s")"); done
	"$ADB_EXE" "${pre[@]}" push "${opts[@]}" "${w[@]}" "$dst" && exit 0
	rc=$?; online || exit $rc
	echo "adb: retrying through $STAGE" >&2
	mkdir -p "$STAGE" || exit $rc; w=()
	for s in "${src[@]}"; do cp -a "$s" "$STAGE/" || exit $rc; w+=("$(winpath "$STAGE/${s##*/}")"); done
	"$ADB_EXE" "${pre[@]}" push "${opts[@]}" "${w[@]}" "$dst"; rc=$?
	for s in "${src[@]}"; do rm -rf "${STAGE:?}/${s##*/}"; done
	exit $rc ;;
pull)
	n=${#args[@]}; [ "$n" -ge 1 ] || exec "$ADB_EXE" "${pre[@]}" pull "$@"
	if [ "$n" -ge 2 ]; then dst=${args[n-1]}; src=("${args[@]:0:n-1}"); else dst=.; src=("${args[@]}"); fi
	"$ADB_EXE" "${pre[@]}" pull "${opts[@]}" "${src[@]}" "$(winpath "$dst")" && exit 0
	rc=$?; online || exit $rc
	echo "adb: retrying through $STAGE" >&2
	rm -rf "$STAGE/pull"; mkdir -p "$STAGE/pull" || exit $rc
	"$ADB_EXE" "${pre[@]}" pull "${opts[@]}" "${src[@]}" "$(winpath "$STAGE/pull")" || exit
	if [ "$n" -ge 2 ] && [ ! -d "$dst" ] && [ "${#src[@]}" = 1 ]; then mv "$STAGE/pull/"* "$dst"; else mv "$STAGE/pull/"* "$dst/"; fi
	rc=$?; rm -rf "$STAGE/pull"; exit $rc ;;
install)
	n=${#args[@]}; [ "$n" -ge 1 ] || exec "$ADB_EXE" "${pre[@]}" install "$@"
	exec "$ADB_EXE" "${pre[@]}" install "${opts[@]}" "$(winpath "${args[n-1]}")" ;;
shell)
	# a command without a terminal: no pty, no \r, keep our stdin
	if [ ${#args[@]} -gt 0 ] && [ -t 0 ]; then
		"$ADB_EXE" "${pre[@]}" shell -T "$@" </dev/null | tr -d '\r'; exit
	elif [ ${#args[@]} -gt 0 ]; then
		"$ADB_EXE" "${pre[@]}" shell "$@" | tr -d '\r'; exit
	fi
	exec "$ADB_EXE" "${pre[@]}" shell "$@" ;;
devices|get-state|version)
	"$ADB_EXE" "${pre[@]}" "$cmd" "$@" | tr -d '\r'; exit ;;
*)
	exec "$ADB_EXE" "${pre[@]}" ${cmd:+"$cmd"} "$@" ;;
esac
EOF
	chmod 755 "$WORK/bin/adb"
}

setup_adb() {
	if is_wsl; then
		local exe
		[ -x "$WORK/bin/adb" ] && [ -x "$(st_get adb_exe)" ] && { export PATH="$WORK/bin:$PATH"; return 0; }
		exe=$(find_adb_exe) || exe=
		while [ -z "$exe" ]; do
			[ $DRY = 1 ] && { say "  [dry-run] adb.exe not found; would ask for its path"; return 0; }
			cat <<EOF
WSL cannot see USB devices; this script uses the Windows adb.exe from Android platform-tools.
It was not found on the Windows PATH or in the usual folders. Either
  - install it in PowerShell:  winget install Google.PlatformTools
    then close this terminal, run "wsl --shutdown" in PowerShell and open Ubuntu again, or
  - unzip platform-tools (developer.android.com/tools/releases/platform-tools) and give its path,
    for example C:\\platform-tools\\adb.exe
EOF
			exe=$(win2wsl "$(ask "path of adb.exe (empty: quit)" "")")
			[ -n "$exe" ] || return 1
			[ -f "$exe" ] || { warn "no file $exe"; exe=; }
		done
		say "  adb: $exe (Windows), through $WORK/bin/adb"
		[ $DRY = 1 ] && return 0
		write_adb_wrapper "$exe"
		st_set adb_exe "$exe"
		export PATH="$WORK/bin:$PATH"
	else
		[ -n "${ADB:-}" ] && { mkdir -p "$WORK/bin"; ln -sf "$ADB" "$WORK/bin/adb"; export PATH="$WORK/bin:$PATH"; }
		have adb || { warn "no adb: install your distribution's android-tools / adb package"; [ $DRY = 1 ] || return 1; }
	fi
	[ $DRY = 1 ] || adb version 2>/dev/null | head -2 | sed 's/^/  /'
}

# ---- steps ----------------------------------------------------------------------
step_host() {
	head_ "host: packages, arm64 through qemu, disk space, adb (1-5 min)"
	local os=linux pkgs miss=() p avail
	is_wsl && os=wsl
	say "This PC: $os, $(uname -m). Repository: $repo"
	say "Work directory: $WORK"
	case $WORK in /mnt/[a-z]/*) warn "WORK is on a Windows drive; the root image needs a Linux file system. Use the default (~/tb323fu-install)."; return 1 ;; esac
	run mkdir -p "$WORK"

	pkgs=(debootstrap ubuntu-keyring libarchive-tools gpg curl ca-certificates e2fsprogs gdisk python3 gzip pigz coreutils)
	[ "$(uname -m)" = aarch64 ] || pkgs+=(qemu-user-binfmt)
	if have dpkg-query; then
		for p in "${pkgs[@]}"; do dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q 'ok installed' || miss+=("$p"); done
		if [ ${#miss[@]} -gt 0 ]; then
			say "Missing packages: ${miss[*]}"
			say "  debootstrap, ubuntu-keyring: build Ubuntu; qemu-user-binfmt: run arm64 programs on this PC;"
			say "  gdisk: the card's partition table; e2fsprogs: the root image; curl, gpg, python3, pigz: downloads, checks, packing."
			if yesno "install them now with apt (asks for your password)?"; then
				run $SUDO apt-get update || return 1
				run $SUDO apt-get install -y "${miss[@]}" || return 1
			else warn "cannot go on without them"; return 1; fi
		else say "packages: all there"; fi
	else
		warn "no dpkg here: install by hand: ${pkgs[*]} (names differ per distribution)"
	fi

	if [ "$(uname -m)" != aarch64 ]; then
		local bf=/proc/sys/fs/binfmt_misc/qemu-aarch64
		if [ $DRY = 0 ] && ! { grep -qx enabled $bf && grep -q '^flags:.*F' $bf; } 2>/dev/null; then
			warn "arm64 programs are not registered with the kernel (binfmt_misc qemu-aarch64)"
			say "  Usually fixed by: sudo systemctl restart systemd-binfmt"
			say "  (WSL without systemd: add [boot] systemd=true to /etc/wsl.conf, then wsl --shutdown in PowerShell)"
			if yesno "try that now?"; then
				[ -e /proc/sys/fs/binfmt_misc/register ] || run $SUDO mount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc
				run $SUDO systemctl restart systemd-binfmt
			fi
			{ grep -qx enabled $bf && grep -q '^flags:.*F' $bf; } 2>/dev/null || { warn "still not registered"; return 1; }
		fi
		say "arm64 through qemu: registered ($(sed -n 's/^interpreter //p' $bf 2>/dev/null))"
	fi

	local d=$WORK; while [ ! -d "$d" ]; do d=$(dirname "$d"); done
	avail=$(df -Pk "$d" 2>/dev/null | awk 'NR==2 {print $4 * 1024}')
	say "free space in $WORK: $(bytes "${avail:-0}") (needs about 30 GB)"
	[ $DRY = 1 ] || [ "${avail:-0}" -ge $((30 * 1024 * 1024 * 1024)) ] || warn "less than 30 GB free -- the build may run out of space"

	setup_adb || return 1
	return 0
}

adb_state() {
	adb devices 2>/dev/null | tr -d '\r' | awk -v s="${ANDROID_SERIAL:-}" 'NR>1 && NF>=2 && (s=="" || $1==s) {print $2}' | sort | uniq -c | awk '{print $2 ($1>1 ? "+" : "")}' | head -1
}

step_tablet() {
	head_ "tablet: rooted Android on USB (1 min)"
	[ $DRY = 1 ] && { say "  [dry-run] adb devices; getprop ro.product.model / ro.boot.slot_suffix; su -c id; the microSD card"; return 0; }
	local st model slot vb id lvl sd
	while :; do
		st=$(adb_state)
		case $st in
		device) break ;;
		device+) warn "several Android devices are connected: unplug the others (or set ANDROID_SERIAL)" ;;
		unauthorized) say "The tablet asks whether to allow USB debugging from this computer: tick \"Always allow\" and tap Allow." ;;
		offline) say "The tablet is offline for adb: unplug and replug the cable." ;;
		*) cat <<'EOF'
No tablet on adb. Check:
  - The tablet runs Android. If it runs this project's Linux: open Open Device Helper -> Android ->
    Switch to Android (or the Android tile in the quick settings, or in a terminal
    sudo back-to-android $(cat /etc/tb323fu/android-boot.sha256)); wait for Android to start.
  - USB debugging is on: Settings -> About tablet -> tap the build number 7 times; then
    Settings -> System -> Developer options -> USB debugging.
  - A USB-C data cable to this PC (some cables only charge).
  - On Windows, `adb devices` in PowerShell should list it too; if not, it is a Windows driver
    or cable problem, not WSL.
EOF
			;;
		esac
		pause || return 1
	done
	model=$(adb_ shell getprop ro.product.model)
	slot=$(adb_ shell getprop ro.boot.slot_suffix)
	vb=$(adb_ shell getprop ro.boot.verifiedbootstate)
	say "  model $model, slot $slot, verified boot $vb"
	case $model in *TB323FU*) ;; *) warn "this is not a TB323FU ($model)"; return 1 ;; esac
	[ "$slot" = _a ] || { warn "Android runs from slot $slot; everything here assumes slot _a (docs/recovery.md)"; return 1; }
	while :; do
		id=$(su_ id)
		case $id in uid=0*) break ;; esac
		say "No root for adb's shell. In the KernelSU app: Superuser -> Shell (com.android.shell) -> allow."
		say "(The tablet may also show a KernelSU prompt right now: allow it.)"
		pause || return 1
	done
	say "  root: yes"
	lvl=$(adb_ shell dumpsys battery | sed -n 's/^ *level: //p')
	[ -n "$lvl" ] && say "  battery: $lvl %"
	[ -n "$lvl" ] && [ "$lvl" -lt 50 ] && warn "battery below 50 %: charge it before writing anything"
	# shellcheck disable=SC2016
	sd=$(su_ 'for b in /sys/block/mmcblk*; do [ "$(cat $b/device/type 2>/dev/null)" = SD ] && echo ${b##*/} $(cat $b/size); done' | head -1)
	if [ -n "$sd" ]; then
		say "  microSD card: ${sd%% *}, $(bytes $(( ${sd##* } * 512 )))"
		st_set sd_dev "${sd%% *}"
	else warn "no microSD card found -- put one in (64 GB or more) before the sdcard step"; fi
	[ "$(st_get done_tablet)" = 1 ] && return 0
	say "Backups: this script assumes you have your own EDL dump (docs/rooting.md, step 1) stored off this PC."
	yesno "Do you have it?" || { warn "make the dump first (docs/rooting.md, '1. Back up')"; return 1; }
}

step_firmware() {
	head_ "firmware: copy it from Android (1 min, nothing on the tablet changes)"
	say "Linux needs the tablet's firmware (Wi-Fi, GPU, audio DSP, touch ...). It is not distributed;"
	say "firmware/extract-on-device.sh copies the files listed in firmware/manifest.tsv from Android's /vendor."
	if [ $DRY = 0 ] && [ -d "$FW/qcom" ] && [ "$explicit" = 0 ]; then say "already here: $FW"; return 0; fi
	yesno "copy the firmware now?" || return 1
	local out rc
	run adb push "$repo/firmware/manifest.tsv" "$repo/firmware/extract-on-device.sh" $T/ || return 1
	out=$(su_ "sh $T/extract-on-device.sh $T/tb323fu-firmware > $T/tb323fu-firmware.log 2>&1; echo rc=\$?; tail -4 $T/tb323fu-firmware.log")
	say "$out" | sed 's/^/  /'
	rc=${out#*rc=}; rc=${rc%%[!0-9]*}
	[ $DRY = 1 ] || [ "$rc" = 0 ] || { warn "some files are missing (log on the tablet: $T/tb323fu-firmware.log); see firmware/README.md"; return 1; }
	su_ "tar -C $T -cf $T/tb323fu-firmware.tar tb323fu-firmware && chmod 644 $T/tb323fu-firmware.tar"
	run mkdir -p "$WORK/fw"
	run adb pull $T/tb323fu-firmware.tar "$WORK/fw/tb323fu-firmware.tar" || return 1
	run tar -C "$WORK/fw" -xf "$WORK/fw/tb323fu-firmware.tar" || return 1
	su_ "rm -rf $T/tb323fu-firmware $T/tb323fu-firmware.tar"
	say "firmware: $FW"
}

module_installed() { su_ 'ksud module list 2>/dev/null' | grep -qi tb323fu-switch; }

step_wayback() {
	head_ "wayback: Android's boot image in boot_b (1 min)"
	cat <<'EOF'
The tablet always boots slot _a. boot_b is never booted, so it keeps a copy of your Android boot
image; switching back to Android copies it into boot_a. Its SHA-256 goes into the Linux system,
and every tool refuses to write unless boot_b still matches it.
EOF
	local a bb
	a=$(su_ 'sha256sum /dev/block/by-name/boot_a' | cut -c1-64)
	bb=$(su_ 'sha256sum /dev/block/by-name/boot_b' | cut -c1-64)
	say "  boot_a ${a:-<hash>}"; say "  boot_b ${bb:-<hash>}"
	[ $DRY = 1 ] || [ ${#a} = 64 ] || { warn "cannot read boot_a (root for Shell?)"; return 1; }
	if [ $DRY = 1 ] || [ "$a" != "$bb" ]; then
		say "boot_b does not hold the Android boot image running now: copy boot_a into boot_b."
		warn "this WRITES boot_b (never booted; nothing that boots changes)"
		undo "not needed; your EDL dump also has boot_a.img and boot_b.img"
		confirm "WRITE BOOT_B" || return 1
		say "  (android/install-module.sh asks once more: type yes)"
		run "$repo/android/install-module.sh" prepare-boot-b || return 1
		[ $DRY = 1 ] || bb=$(su_ 'sha256sum /dev/block/by-name/boot_b' | cut -c1-64)
	else say "boot_b = boot_a: the way back is in place."; fi
	[ $DRY = 1 ] && bb='<sha256 of boot_b>'
	if [ $DRY = 1 ] || ! module_installed; then
		say "The 'Switch to Linux' KernelSU module brings you from Android back to Linux later (active after"
		say "Android's next start). It only writes boot_a, and only an image whose hash it checks."
		yesno "install the module?" && { run "$repo/android/install-module.sh" install || return 1; }
	else say "Switch to Linux module: installed."; fi
	run mkdir -p "$WORK/config"
	[ $DRY = 1 ] || printf '%s\n' "$bb" > "$WORK/config/android-boot.sha256"
	st_set android_sha "$bb"
	say "Android boot hash -> $WORK/config/android-boot.sha256"

	local stock=${STOCK_BOOT:-}
	if [ -n "$stock" ]; then
		stock=$(win2wsl "$stock")
		[ $DRY = 1 ] || [ "$(sha "$stock")" = "$bb" ] || { warn "$stock is not the image in boot_b -- leave STOCK_BOOT empty to read boot_b"; return 1; }
		run cp "$stock" "$WORK/stock-boot.img"
	elif [ $DRY = 1 ] || [ ! -s "$WORK/stock-boot.img" ] || [ "$(sha "$WORK/stock-boot.img")" != "$bb" ]; then
		su_ "dd if=/dev/block/by-name/boot_b of=$T/stock-boot.img bs=1048576 2>/dev/null && chmod 644 $T/stock-boot.img"
		run adb pull $T/stock-boot.img "$WORK/stock-boot.img" || return 1
		su_ "rm -f $T/stock-boot.img"
	fi
	[ $DRY = 1 ] || [ "$(sha "$WORK/stock-boot.img")" = "$bb" ] || { warn "the copied stock boot image does not match boot_b"; return 1; }
	say "your stock boot image -> $WORK/stock-boot.img (keep it with your backup)"
}

# GitHub: the token comes from GITHUB_TOKEN, else gh / gh.exe; none for a public repository.
gh_auth_file() {
	local t='' g f=$WORK/.gh-auth
	[ $DRY = 1 ] && return 1
	t=${GITHUB_TOKEN:-}
	if [ -z "$t" ]; then
		for g in gh gh.exe; do have $g || continue; t=$($g auth token 2>/dev/null | tr -d '\r\n'); [ -n "$t" ] && break; done
	fi
	rm -f "$f"
	[ -n "$t" ] || return 1
	(umask 077; printf 'Authorization: Bearer %s\n' "$t" > "$f")
	printf '%s\n' "$f"
}
gh_api() {
	local auth=$1; shift
	curl -fsSL --retry 2 -H 'X-GitHub-Api-Version: 2022-11-28' ${auth:+-H "@$auth"} "$@"
}
# pick_release JSON PREFIX [TAG] -> "tag" then "name<TAB>url" per asset
pick_release() {
	python3 - "$@" <<'EOF'
import json, sys
rels = json.load(open(sys.argv[1]))
prefix, want = sys.argv[2], (sys.argv[3] if len(sys.argv) > 3 else "")
rs = [r for r in rels if r["tag_name"].startswith(prefix) and not r.get("draft")]
if want:
    rs = [r for r in rs if r["tag_name"] == want]
rs.sort(key=lambda r: r.get("published_at") or r["created_at"], reverse=True)
if not rs:
    sys.exit(1)
print(rs[0]["tag_name"])
for a in rs[0]["assets"]:
    print(a["name"] + "\t" + a["url"])
EOF
}
# check_sums DIR FILE...: every FILE listed in DIR/SHA256SUMS and matching
check_sums() {
	local d=$1 f want; shift
	[ -f "$d/SHA256SUMS" ] || { warn "no $d/SHA256SUMS"; return 1; }
	for f; do
		want=$(awk -v f="$f" '$2 == f || $2 == "*" f {print $1}' "$d/SHA256SUMS" | head -1)
		[ -n "$want" ] || { warn "$f is not listed in $d/SHA256SUMS"; return 1; }
		[ -f "$d/$f" ] || { warn "missing $d/$f"; return 1; }
		[ "$(sha "$d/$f")" = "$want" ] || { warn "$f: checksum does not match (damaged download?)"; return 1; }
	done
}
kernel_files() { local i; i=$(cd "$WORK/kernel" 2>/dev/null && ls Image-tb323fu-t* 2>/dev/null | grep -v '\.gz$' | sort -V | tail -1); [ -n "$i" ] && echo "$i"; }
deb_files() { [ -f "$WORK/debs/SHA256SUMS" ] && awk '{sub(/^\*/, "", $2); print $2}' "$WORK/debs/SHA256SUMS" | grep '^tb323fu-.*\.deb$'; }
downloads_ok() {
	local k d
	k=$(kernel_files) && check_sums "$WORK/kernel" "$k" 2>/dev/null || return 1
	d=$(deb_files) && [ -n "$d" ] || return 1
	# shellcheck disable=SC2086
	check_sums "$WORK/debs" $d 2>/dev/null
}

fetch_api() {
	local auth=$1 json=$WORK/.releases.json kind prefix tag dir line name url want lines
	if [ $DRY = 1 ]; then
		say "  [dry-run] curl https://api.github.com/repos/$REPO_SLUG/releases (with your GitHub login if there is one)"
		say "  [dry-run] download Image-tb323fu-tNN + SHA256SUMS of the newest kernel-t* into $WORK/kernel"
		say "  [dry-run] download tb323fu-*.deb + SHA256SUMS of the newest helper-v* into $WORK/debs"
		return 0
	fi
	gh_api "$auth" -o "$json" "https://api.github.com/repos/$REPO_SLUG/releases?per_page=50" || return 1
	for kind in kernel helper; do
		if [ $kind = kernel ]; then prefix=kernel-t; tag=${KERNEL_TAG:-}; dir=$WORK/kernel
		else prefix=helper-v; tag=${HELPER_TAG:-}; dir=$WORK/debs; fi
		mapfile -t lines < <(pick_release "$json" "$prefix" "$tag")
		[ ${#lines[@]} -gt 0 ] || { warn "no $prefix* release in $REPO_SLUG${tag:+ (wanted $tag)}"; return 1; }
		tag=${lines[0]}; say "  $kind release: $tag"; st_set ${kind}_tag "$tag"
		run mkdir -p "$dir"
		for line in "${lines[@]:1}"; do
			name=${line%%$'\t'*}; url=${line#*$'\t'}
			case $kind:$name in
			kernel:SHA256SUMS|kernel:Image-tb323fu-t*[0-9]) want=1 ;;
			helper:SHA256SUMS|helper:tb323fu-*.deb) want=1 ;;
			*) want=0 ;;
			esac
			[ $want = 1 ] || continue
			printf '  %s ... ' "$name"
			gh_api "$auth" -H 'Accept: application/octet-stream' -o "$dir/$name.part" "$url" && mv "$dir/$name.part" "$dir/$name" && echo ok ||
				{ echo failed; return 1; }
		done
	done
	rm -f "$json"
}

fetch_browser() {
	local dl c k ks hs
	if is_wsl; then dl=$(win2wsl "$(winenv USERPROFILE)")/Downloads; else dl=${XDG_DOWNLOAD_DIR:-$HOME/Downloads}; fi
	cat <<EOF
Download these files in your browser (you must be able to see the repository):
  https://github.com/$REPO_SLUG/releases
  - from the newest "kernel-t.." release: Image-tb323fu-tNN (not the .gz) and SHA256SUMS
  - from the newest "helper-v.." release: every tb323fu-*.deb and its SHA256SUMS
    (the browser may call the second one "SHA256SUMS (1)"; that is fine)
They are looked for in $dl.
EOF
	[ $DRY = 1 ] && return 0
	while :; do
		pause "Press Enter when the downloads are finished (Ctrl-C to quit)" || return 1
		k=$(cd "$dl" 2>/dev/null && ls -t Image-tb323fu-t* 2>/dev/null | grep -v '\.gz$' | head -1)
		ks='' hs=''
		for c in "$dl"/SHA256SUMS*; do
			[ -f "$c" ] || continue
			[ -z "$ks" ] && [ -n "$k" ] && grep -q " \*\{0,1\}$k\$" "$c" && ks=$c
			[ -z "$hs" ] && grep -q 'tb323fu-platform_' "$c" && hs=$c
		done
		[ -n "$k" ] && [ -n "$ks" ] && [ -n "$hs" ] && break
		c=''
		[ -n "$k" ] || c="$c Image-tb323fu-tNN,"
		[ -n "$ks" ] || c="$c the kernel's SHA256SUMS,"
		[ -n "$hs" ] || c="$c the helper's SHA256SUMS,"
		warn "not found yet in $dl:${c%,}"
	done
	mkdir -p "$WORK/kernel" "$WORK/debs"
	cp "$dl/$k" "$WORK/kernel/" && cp "$ks" "$WORK/kernel/SHA256SUMS" && cp "$hs" "$WORK/debs/SHA256SUMS" || return 1
	for c in $(deb_files); do
		[ -f "$dl/$c" ] || { warn "missing $dl/$c"; return 1; }
		cp "$dl/$c" "$WORK/debs/"
	done
	st_set kernel_tag "kernel-${k#Image-tb323fu-}"
}

step_download() {
	head_ "download: kernel and platform packages from GitHub Releases (1-3 min)"
	cat <<'EOF'
A kernel release is a kernel Image (its modules are inside) -- never a ready-made boot.img: the
boot image is made from YOUR stock one in the next step. The helper-v release has the Ubuntu
packages for the tablet (audio, sensors, emergency key, Open Device Helper).
SHA256SUMS catches a damaged download; it is not a signature.
EOF
	if [ $DRY = 0 ] && [ "$explicit" = 0 ] && downloads_ok; then say "already here and checked: $WORK/kernel/$(kernel_files), $(deb_files | wc -l) packages"; return 0; fi
	local auth=''
	auth=$(gh_auth_file) && say "  (GitHub login found: using it)"
	if ! fetch_api "$auth"; then
		[ -n "$auth" ] && warn "the GitHub API did not work with your login"
		[ -z "$auth" ] && say "  The GitHub API gave nothing without a login (private repository?)."
		say "  Alternatives: set GITHUB_TOKEN, or 'gh auth login' (Linux gh, or gh.exe on Windows), or the browser."
		fetch_browser || { rm -f "$WORK/.gh-auth"; return 1; }
	fi
	rm -f "$WORK/.gh-auth"
	[ $DRY = 1 ] && return 0
	downloads_ok || { check_sums "$WORK/kernel" "$(kernel_files)"; warn "the downloads do not check out; delete $WORK/kernel and $WORK/debs and run this step again"; return 1; }
	say "checked: $WORK/kernel/$(kernel_files); packages: $(deb_files | tr '\n' ' ')"
}

step_bootimg() {
	head_ "bootimg: pack the kernel into your stock boot image (seconds)"
	say "Only the kernel is replaced; header, Lenovo's signature blobs and the AVB footer stay your own."
	local k stock=$WORK/stock-boot.img hash
	hash=$(st_get android_sha)
	k=$(kernel_files) || k=''
	[ $DRY = 1 ] && { k=${k:-Image-tb323fu-tNN}; hash=${hash:-x}; }
	[ -n "$k" ] || { warn "no kernel in $WORK/kernel (download step)"; return 1; }
	[ $DRY = 1 ] || [ -s "$stock" ] || { warn "no $stock (wayback step)"; return 1; }
	[ $DRY = 1 ] || [ "$(sha "$stock")" = "$hash" ] || { warn "$stock does not match boot_b's hash -- run the wayback step again"; return 1; }
	run python3 "$repo/tools/boot-repack-kernel.py" "$stock" "$WORK/kernel/$k" "$LINUX_BOOT_IMG" || return 1
	[ $DRY = 1 ] || st_set linux_boot_sha "$(sha "$LINUX_BOOT_IMG")"
	say "Linux boot image -> $LINUX_BOOT_IMG"
}

# sizes in bytes from 64G / 512M / plain numbers
to_bytes() { numfmt --from=iec "${1%B}" 2>/dev/null; }

step_sdcard() {
	head_ "sdcard: a partition table on the microSD card -- WIPES THE CARD (1 min)"
	cat <<EOF
Android's own tools cannot create partitions, so the table is built here, in an empty file the
size of the card, and only its first 34 and last 33 sectors are written to the card from Android.
One ext4-ready partition named $ROOT_PARTLABEL is created; the rest of the card stays free for
more systems later (docs/install.md, Multiboot). Android will call the card "unsupported"; expected.
EOF
	local dev n part_n part_sz first last size size_b img=$WORK/sd-gpt.img
	dev=$(st_get sd_dev); dev=${dev:-mmcblk1}
	if [ $DRY = 1 ]; then n=250085376
	else
		n=$(su_ "cat /sys/block/$dev/size")
		[ "$(su_ "cat /sys/block/$dev/device/type")" = SD ] && [ "${n:-0}" -gt 0 ] ||
			{ warn "no microSD card at $dev (tablet step)"; return 1; }
		[ "$(su_ "cat /sys/block/$dev/queue/logical_block_size")" = 512 ] || { warn "the card does not use 512-byte sectors; not supported here"; return 1; }
		# already done: a partition with our name
		part_n=$(su_ "grep -l \"^PARTNAME=$ROOT_PARTLABEL\$\" /sys/block/$dev/${dev}p*/uevent 2>/dev/null" | head -1)
		if [ -n "$part_n" ]; then
			part_n=${part_n%/uevent}; part_n=${part_n##*p}
			part_sz=$(su_ "cat /sys/block/$dev/${dev}p$part_n/size")
			say "The card already has partition $part_n named $ROOT_PARTLABEL ($(bytes $((part_sz * 512))))."
			if [ "$explicit" = 0 ] || ! yesno "partition the card again anyway (WIPES IT)?"; then
				st_set part_dev "/dev/block/${dev}p$part_n"; st_set part_bytes $((part_sz * 512)); return 0
			fi
		fi
	fi
	say "  card: /dev/block/$dev, $(bytes $((n * 512))) ($n sectors)"
	if [ $DRY = 0 ]; then
		say "  what Android sees on it now:"
		su_ "sm list-volumes 2>/dev/null; ls /dev/block/${dev}p* 2>/dev/null" | sed 's/^/    /'
	fi
	size=${ROOT_SIZE:-}
	if [ -z "$size" ]; then
		if [ $((n * 512)) -gt $((100 * 1024 * 1024 * 1024)) ]; then size=64G; else size=0; fi
		size=$(ask "size of the Linux partition (e.g. 64G; 0 = the whole card)" "$size")
	fi
	if [ "$size" != 0 ]; then
		size_b=$(to_bytes "$size") || { warn "cannot read the size $size"; return 1; }
		[ "$size_b" -ge $((16 * 1024 * 1024 * 1024)) ] || { warn "at least 16G please (GNOME needs about 10 GB)"; return 1; }
		[ "$size_b" -lt $((n * 512 - 2 * 1024 * 1024)) ] || size=0
	fi
	run rm -f "$img"
	run truncate -s $((n * 512)) "$img" || return 1
	if [ "$size" = 0 ]; then run sgdisk -n 1:0:0 -t 1:8300 -c "1:$ROOT_PARTLABEL" "$img" || return 1
	else run sgdisk -n "1:0:+$size" -t 1:8300 -c "1:$ROOT_PARTLABEL" "$img" || return 1; fi
	if [ $DRY = 1 ]; then first=2048 last=134219775
	else
		first=$(sgdisk -i 1 "$img" | sed -n 's/^First sector: \([0-9]*\).*/\1/p')
		last=$(sgdisk -i 1 "$img" | sed -n 's/^Last sector: \([0-9]*\).*/\1/p')
		sgdisk -p "$img" | sed -n '/^Number/,$p' | sed 's/^/    /'
	fi
	run dd if="$img" of="$WORK/gpt-head.bin" bs=512 count=34 status=none
	run dd if="$img" of="$WORK/gpt-tail.bin" bs=512 skip=$((n - 33)) count=33 status=none
	run rm -f "$img"
	[ $DRY = 1 ] || cat > "$WORK/sd-write.sh" <<EOF
# written by install.sh, run on the tablet as root
D=/dev/block/$dev
[ "\$(cat /sys/block/$dev/size)" = $n ] || { echo "the card changed (size)"; exit 1; }
for v in \$(sm list-volumes public 2>/dev/null | grep -o 'public:179,[0-9]*'); do sm unmount \$v; done
sleep 1
if grep -q -e "^\$D" -e 'vold/public:179' /proc/mounts; then echo "the card is still mounted"; exit 1; fi
command -v sgdisk >/dev/null && sgdisk --zap-all \$D >/dev/null 2>&1
dd if=$T/gpt-head.bin of=\$D bs=512 conv=fsync 2>/dev/null || exit 1
dd if=$T/gpt-tail.bin of=\$D bs=512 seek=$((n - 33)) conv=fsync 2>/dev/null || exit 1
blockdev --rereadpt \$D
sleep 2
cat /sys/block/$dev/${dev}p1/size
grep '^PARTNAME=' /sys/block/$dev/${dev}p1/uevent
EOF
	warn "EVERYTHING ON THE CARD (/dev/block/$dev, $(bytes $((n * 512)))) WILL BE ERASED"
	undo "the old contents are gone for good. To give the card back to Android: Settings -> Storage -> the card -> Format"
	confirm "ERASE" || return 1
	run adb push "$WORK/gpt-head.bin" "$WORK/gpt-tail.bin" "$WORK/sd-write.sh" $T/ || return 1
	local out
	out=$(su_ "sh $T/sd-write.sh; rm -f $T/sd-write.sh $T/gpt-head.bin $T/gpt-tail.bin")
	[ $DRY = 1 ] && out="$((last - first + 1))
PARTNAME=$ROOT_PARTLABEL"
	say "$out" | sed 's/^/  /'
	case $out in *"PARTNAME=$ROOT_PARTLABEL"*) ;; *) warn "the new partition did not show up; see the lines above"; return 1 ;; esac
	[ "$(printf '%s\n' "$out" | head -1)" = $((last - first + 1)) ] || { warn "the partition has an unexpected size"; return 1; }
	st_set part_dev "/dev/block/${dev}p1"
	st_set part_bytes $(((last - first + 1) * 512))
	say "partition: /dev/block/${dev}p1 ($ROOT_PARTLABEL, $(bytes $(((last - first + 1) * 512)))); in Linux it is /dev/mmcblk0p1"
}

growroot_into() {
	local m=$1
	run $SUDO install -m 644 "$repo/userspace/platform/optional/image/tb323fu-growroot.service" "$m/etc/systemd/system/"
	run $SUDO mkdir -p "$m/etc/systemd/system/multi-user.target.wants"
	run $SUDO ln -sf ../tb323fu-growroot.service "$m/etc/systemd/system/multi-user.target.wants/tb323fu-growroot.service"
}

step_rootfs() {
	head_ "rootfs: build Ubuntu ($DESKTOP) into an image file on this PC (1-2 h with GNOME)"
	local img=$WORK/root.img mnt=$WORK/mnt part_b size_b user bs minb tgt t0
	part_b=$(st_get part_bytes)
	[ $DRY = 1 ] && part_b=${part_b:-68719476736}
	[ -n "$part_b" ] || { warn "partition size unknown (sdcard step first)"; return 1; }
	[ $DRY = 1 ] || [ -d "$FW/qcom" ] || { warn "no firmware (firmware step)"; return 1; }
	[ $DRY = 1 ] || [ -s "$WORK/config/android-boot.sha256" ] || { warn "no Android boot hash (wayback step)"; return 1; }
	[ $DRY = 1 ] || [ -n "$(deb_files)" ] || { warn "no platform packages (download step)"; return 1; }
	size_b=$(to_bytes "$IMG_SIZE"); [ "$size_b" -gt "$part_b" ] && size_b=$part_b
	cat <<EOF
rootfs/ubuntu/build-rootfs.sh installs Ubuntu 26.04 with debootstrap into an ext4 image of
$(bytes "$size_b"), with the firmware, your boot_b hash and the tablet packages. On an x86-64 PC
every arm64 program runs through qemu: about 20 min without a desktop, 1-2 h with GNOME
(the PC stays usable; the tablet is not needed meanwhile). Afterwards the image is shrunk to what
it holds; on the tablet's first start it grows to fill the partition by itself.
EOF
	user=${DEV_USER:-$(st_get dev_user)}
	[ -n "$user" ] || user=$(ask "your user name on the tablet" "$(id -un)")
	case $user in ''|root|*[!a-z0-9_-]*) warn "user name: lower-case letters, digits, - and _ only, not root"; return 1 ;; esac
	st_set dev_user "$user"
	yesno "build now?" || return 1
	[ $DRY = 1 ] || $SUDO true || return 1

	if [ $DRY = 0 ] && findmnt -n "$mnt" >/dev/null 2>&1; then say "  $mnt is mounted already (an earlier run): building on"
	else
		if [ $DRY = 0 ] && [ -f "$img" ]; then
			say "  $img exists (an earlier run): building on in it"
			run truncate -s "$size_b" "$img" && run e2fsck -fy "$img" >/dev/null; run resize2fs "$img" >/dev/null 2>&1
		else
			run truncate -s "$size_b" "$img" || return 1
			run mkfs.ext4 -q -F -L "$ROOT_PARTLABEL" "$img" || return 1
		fi
		run mkdir -p "$mnt"
		run $SUDO mount -o loop "$img" "$mnt" || return 1
	fi
	t0=$(date +%s)
	say "  log: $WORK/rootfs-build.log"
	if [ $DRY = 1 ]; then
		say "  [dry-run] $SUDO setsid -w env ROOT_PARTLABEL=$ROOT_PARTLABEL DESKTOP=$DESKTOP DEV_USER=$user DEV_ACCESS=$DEV_ACCESS FIRMWARE_FROM=$FW CONFIG_FROM=$WORK/config DEBS_FROM=$WORK/debs sh $repo/rootfs/ubuntu/build-rootfs.sh $mnt"
	# setsid: no controlling terminal for the build; on sudo-rs's pty, job control can stop apt-get under qemu
	elif ! (cd "$WORK" && $SUDO setsid -w env ROOT_PARTLABEL="$ROOT_PARTLABEL" DESKTOP="$DESKTOP" DEV_USER="$user" DEV_ACCESS="$DEV_ACCESS" \
		FIRMWARE_FROM="$FW" CONFIG_FROM="$WORK/config" DEBS_FROM="$WORK/debs" \
		sh "$repo/rootfs/ubuntu/build-rootfs.sh" "$mnt") 2>&1 </dev/null | tee "$WORK/rootfs-build.log"; then
		warn "the build failed (log: $WORK/rootfs-build.log). Often a network hiccup: run this step again, it goes on where it stopped."
		say "  To start over instead: sudo umount $mnt; rm $img"
		return 1
	fi
	[ $DRY = 1 ] || say "  built in $(( ($(date +%s) - t0) / 60 )) min"
	growroot_into "$mnt"
	say "Set a password for $user (for sudo and the lock screen; root stays locked):"
	run $SUDO mount --bind /dev "$mnt/dev"; run $SUDO mount -t proc proc "$mnt/proc"
	local try=0
	if [ $DRY = 1 ]; then say "  [dry-run] $SUDO chroot $mnt passwd $user"
	else
		until $SUDO chroot "$mnt" passwd "$user" </dev/tty; do
			try=$((try + 1)); [ $try -ge 3 ] && { warn "no password set: $user cannot use sudo until you set one (install.sh rootfs again)"; break; }
			warn "try again"
		done
	fi
	run $SUDO umount "$mnt/proc" "$mnt/dev"
	run $SUDO umount "$mnt" || return 1

	say "Shrinking the image to its contents plus 2 GiB ..."
	run e2fsck -fy "$img" >/dev/null
	if [ $DRY = 0 ]; then
		bs=$(dumpe2fs -h "$img" 2>/dev/null | sed -n 's/^Block size: *//p')
		minb=$(resize2fs -P "$img" 2>/dev/null | sed -n 's/.*: *\([0-9]*\)$/\1/p')
		tgt=$((minb + 2 * 1024 * 1024 * 1024 / bs))
		[ $((tgt * bs)) -lt "$size_b" ] && { resize2fs "$img" "$tgt" >/dev/null 2>&1 && truncate -s $((tgt * bs)) "$img" || return 1; }
	fi
	say "Packing it for the transfer (a few minutes) ..."
	local z=gzip; have pigz && z=pigz
	run sh -c "$z -1 -c '$img' > '$img.gz.part' && mv '$img.gz.part' '$img.gz'" || return 1
	if [ $DRY = 0 ]; then
		st_set root_img_bytes "$(stat -c %s "$img")"
		st_set root_img_sha "$(sha "$img")"
		st_set root_gz_sha "$(sha "$img.gz")"
		say "image: $(bytes "$(stat -c %s "$img")"), packed $(bytes "$(stat -c %s "$img.gz")")"
	fi
}

step_write() {
	head_ "write: put the root image into the card's partition, from Android (10-20 min)"
	local part gz=$WORK/root.img.gz isha ibytes gsha free got out
	part=$(st_get part_dev); isha=$(st_get root_img_sha); ibytes=$(st_get root_img_bytes); gsha=$(st_get root_gz_sha)
	[ $DRY = 1 ] && { part=${part:-/dev/block/mmcblk1p1}; ibytes=${ibytes:-1}; }
	[ -n "$part" ] || { warn "no partition (sdcard step)"; return 1; }
	[ $DRY = 1 ] || { [ -s "$gz" ] && [ -n "$isha" ]; } || { warn "no root image (rootfs step)"; return 1; }
	if [ $DRY = 0 ] && [ "$explicit" = 0 ] && [ "$(st_get written_sha)" = "$isha" ]; then say "this image is on the card already"; return 0; fi
	if [ $DRY = 0 ]; then
		[ "$ibytes" -le "$(st_get part_bytes)" ] || { warn "the image is larger than the partition"; return 1; }
		free=$(adb_ shell df -k /data | awk 'NR==2 {print $4 * 1024}')
		[ "${free:-0}" -gt $(( $(stat -c %s "$gz") + 1024 * 1024 * 1024 )) ] ||
			{ warn "not enough free space in Android for $(bytes "$(stat -c %s "$gz")") (free: $(bytes "${free:-0}"))"; return 1; }
	fi
	say "1) copy the packed image to the tablet ($T, a few minutes)"
	run adb push "$gz" $T/tb323fu-root.img.gz || return 1
	got=$(su_ "sha256sum $T/tb323fu-root.img.gz" | cut -c1-64)
	[ $DRY = 1 ] || [ "$got" = "$gsha" ] || { warn "the copy on the tablet is damaged; run this step again"; return 1; }
	say "2) unpack it into $part ($ROOT_PARTLABEL); 5-15 min, depending on the card"
	warn "this OVERWRITES $part (the partition made in the sdcard step)"
	undo "nothing on the tablet itself changes; to clear the card: Android Settings -> Storage -> Format"
	confirm "WRITE" || return 1
	out=$(su_ "zcat $T/tb323fu-root.img.gz | dd of=$part bs=4194304 conv=fsync 2>&1 | tail -1")
	say "  $out"
	say "3) read it back and compare (a few minutes)"
	got=$(su_ "head -c $ibytes $part | sha256sum" | cut -c1-64)
	[ $DRY = 1 ] || [ "$got" = "$isha" ] || { warn "what is on the card differs from the image; run this step again (or try another card)"; return 1; }
	su_ "rm -f $T/tb323fu-root.img.gz"
	st_set written_sha "$isha"
	say "the root is on the card."
}

step_boot() {
	head_ "boot: write the Linux boot image to boot_a and start Linux (1 min)"
	local hash img=$LINUX_BOOT_IMG want bb got
	hash=$(st_get android_sha); want=$(st_get linux_boot_sha)
	[ $DRY = 1 ] && { hash=${hash:-'<sha256 of boot_b>'}; want=${want:-x}; }
	[ -n "$hash" ] || { warn "no Android boot hash (wayback step)"; return 1; }
	[ $DRY = 1 ] || { [ -s "$img" ] && [ "$(sha "$img")" = "$want" ]; } || { warn "no $img (bootimg step)"; return 1; }
	[ $DRY = 1 ] || [ "$(st_get written_sha)" = "$(st_get root_img_sha)" ] || warn "the root image has not been written to the card (write step)"
	bb=$(su_ 'sha256sum /dev/block/by-name/boot_b' | cut -c1-64)
	[ $DRY = 1 ] || [ "$bb" = "$hash" ] || { warn "boot_b no longer holds the recorded Android image -- run the wayback step"; return 1; }
	say "Copying the image to the tablet and staging it for the Switch to Linux module (only /data/adb changes) ..."
	run "$repo/android/install-module.sh" stage "$img" || return 1
	cat <<EOF
Now boot_a gets the Linux boot image; then the tablet restarts into Linux. Android stays in boot_b:
  back to Android: Open Device Helper -> Android, the Android tile in the quick settings,
                   or hold volume up + volume down for 10 s (also when the desktop hangs)
  back to Linux:   KernelSU app -> Modules -> Switch to Linux -> Action
EOF
	undo "if Linux does not start: hold volume up + volume down 10 s; if nothing reacts: power + volume down to force it off, then docs/recovery.md (EDL, LTBox Flash Partitions boot_a <- your Android boot image)"
	confirm "FLASH BOOT_A" || return 1
	got=$(su_ "dd if=/data/adb/tb323fu/linux.img of=/dev/block/by-name/boot_a bs=1048576 conv=fsync 2>/dev/null; sync; sha256sum /dev/block/by-name/boot_a" | cut -c1-64)
	if [ $DRY = 0 ] && [ "$got" != "$want" ]; then
		warn "boot_a does not match after writing -- putting Android back from boot_b, not rebooting"
		got=$(su_ "dd if=/dev/block/by-name/boot_b of=/dev/block/by-name/boot_a bs=1048576 conv=fsync 2>/dev/null; sync; sha256sum /dev/block/by-name/boot_a" | cut -c1-64)
		[ "$got" = "$hash" ] && say "boot_a holds Android again" || warn "boot_a is neither image: do NOT reboot; get help (docs/recovery.md)"
		return 1
	fi
	st_set boot_written "$want"
	say "boot_a = Linux (${want:0:16}...)"
	yesno "restart into Linux now?" && run adb reboot
	return 0
}

step_firstboot() {
	head_ "firstboot: what to expect"
	cat <<EOF
1. Lenovo logo, then a text summary on the panel (kernel, CPUs, storage, battery) for a few seconds.
2. "switching to root $ROOT_PARTLABEL ..." and Ubuntu starts. The first start takes longer: the root
   grows to fill its partition (tb323fu-growroot) and the card is slower than internal storage.
3. GNOME logs you in automatically (user $(st_get dev_user || true)).

Then, in GNOME's Terminal:
  uname -r                                  # the release kernel ($(st_get kernel_tag || true))
  systemctl is-system-running               # running (or degraded: systemctl --failed)
  df -h /                                   # about the size of the partition
  cat /etc/tb323fu/android-boot.sha256      # $(st_get android_sha || true)
  tb323fu-ctl boot list                     # the systems the boot loader can see

Try the way back once while you are at the desk: Open Device Helper -> Android (or hold volume up +
volume down for 10 s). Android must come up; back to Linux with KernelSU -> Modules -> Switch to
Linux -> Action (a newly installed module is active from Android's next start).

If Linux does not come up: docs/recovery.md#linux-does-not-boot. Holding volume up during the
summary keeps the tablet in the initramfs. Power + volume down forces it off.
Space on this PC: $WORK/root.img and root.img.gz can go now; keep stock-boot.img, linux-boot.img
and config/ (the WSL disk does not shrink by itself after deleting).
EOF
}

say "${b}TB323FU: Ubuntu on the microSD card, guided${r}$([ $DRY = 1 ] && echo ' (dry run: nothing is executed)')"
say "Guide: docs/install-windows.md (Windows) and docs/install.md (reference). State: $STATE"
[ $DRY = 1 ] || mkdir -p "$WORK"
if is_wsl && [ -x "$WORK/bin/adb" ]; then export PATH="$WORK/bin:$PATH"; fi
for s in "${steps[@]}"; do
	if [ $explicit = 0 ] && [ $DRY = 0 ] && [ "$(st_get "done_$s")" = 1 ] && [ "$s" != firstboot ]; then
		say "-- $s: done earlier (run 'install.sh $s' to do it again)"; continue
	fi
	case $s in tablet|firmware|wayback|sdcard|write|boot)
		[ $DRY = 1 ] || have adb || setup_adb || exit 1
		[ $s = tablet ] || [ $DRY = 1 ] || [ "$(adb_state)" = device ] || step_tablet || exit 1 ;;
	esac
	if "step_$s"; then st_set "done_$s" 1
	else warn "step '$s' stopped. Fix what it says and run the script again; finished steps are skipped."; exit 1; fi
done
say ""
say "done."
