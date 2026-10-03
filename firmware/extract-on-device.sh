#!/system/bin/sh
# SPDX-License-Identifier: MIT
# extract-on-device.sh -- collect the firmware the mainline kernel needs from
# the tablet's own Android /vendor, so no firmware is ever redistributed.
# Run on Android as root (KernelSU/Magisk su);
# POSIX sh, needs only toybox/busybox: od, dd, head, sha256sum, mkdir, cp.
#
#   sh extract-on-device.sh [OUTDIR]        # default /data/local/tmp/tb323fu-firmware
#
# For every line of manifest.tsv (next to this script, or $MANIFEST):
#   - a plain source file is copied,
#   - a split PIL image (".mdt + .bNN") is merged back into one .mbn, the same
#     way pil-squash.py does it: the .mdt holds the ELF header and the
#     program headers; each segment i with p_filesz > 0 is .b<i> (or, for the
#     hash segment, already inside the .mdt) and is written at its p_offset,
#   - the result lands at OUTDIR/<target path> and its sha256 is compared with
#     the manifest. A mismatch is a warning, not an error: another firmware
#     version (OTA) gives other hashes and is usually still fine.
# Sources on Android: /vendor/firmware, /vendor/firmware_mnt/image (modem_a;
# Wi-Fi under peach/, copied to the ath12k names in the target column),
# /vendor/soccp_firmware/image, and /vendor/bt_firmware/image (the bluetooth
# partition, Bluetooth). Every file the mainline image needs comes from the
# device; none is taken from linux-firmware.
# Exit status: 0 if every file was produced, 1 if any is missing.
#
# Testing off-device: FW_SRCROOT=/some/dir prefixes every source path
# (so /vendor/firmware_mnt/image/x.mdt is read from $FW_SRCROOT/vendor/...).
set -u
OUT=${1:-/data/local/tmp/tb323fu-firmware}
d=$(dirname "$0")
M=$d/manifest.tsv
MANIFEST=${MANIFEST:-$M}
R=${FW_SRCROOT:-}
[ -r "$MANIFEST" ] || { echo "no manifest: $MANIFEST" >&2; exit 1; }

u8()  { od -An -tu1 -j "$2" -N1 "$1" | tr -d ' \n'; }
u16() { od -An -tu2 -j "$2" -N2 "$1" | tr -d ' \n'; }
u32() { od -An -tu4 -j "$2" -N4 "$1" | tr -d ' \n'; }
u64() { lo=$(u32 "$1" "$2"); hi=$(u32 "$1" $(($2 + 4))); echo $((hi * 4294967296 + lo)); }
# largest power of two (<= 64 KiB) dividing every argument: dd block size
blk() { b=65536; for n in "$@"; do while [ $b -gt 1 ] && [ $((n % b)) -ne 0 ]; do b=$((b / 2)); done; done; echo $b; }

# squash MDT OUT -> 0 ok, 1 error (message on stderr)
squash() {
	mdt=$1; out=$2; base=${mdt%.mdt}
	[ "$(od -An -tu1 -N4 "$mdt" | tr -s ' \n' '  ')" = ' 127 69 76 70 ' ] || { echo "not an ELF" >&2; return 1; }
	msize=$(wc -c < "$mdt")
	if [ "$(u8 "$mdt" 4)" = 2 ]; then
		phoff=$(u64 "$mdt" 32); phent=$(u16 "$mdt" 54); phnum=$(u16 "$mdt" 56); w=64
	else
		phoff=$(u32 "$mdt" 28); phent=$(u16 "$mdt" 42); phnum=$(u16 "$mdt" 44); w=32
	fi
	head -c $((phoff + phnum * phent)) "$mdt" > "$out" || return 1
	i=0
	while [ $i -lt "$phnum" ]; do
		ph=$((phoff + i * phent))
		if [ $w = 64 ]; then off=$(u64 "$mdt" $((ph + 8))); sz=$(u64 "$mdt" $((ph + 32)))
		else off=$(u32 "$mdt" $((ph + 4))); sz=$(u32 "$mdt" $((ph + 16))); fi
		if [ "$sz" -gt 0 ]; then
			seg=$(printf '%s.b%02d' "$base" $i)
			if [ -f "$seg" ]; then
				[ "$(wc -c < "$seg")" -eq "$sz" ] || { echo "$(basename "$seg") size != p_filesz $sz" >&2; return 1; }
				b=$(blk "$off")
				dd if="$seg" of="$out" bs=$b seek=$((off / b)) conv=notrunc 2>/dev/null || return 1
			elif [ $((off + sz)) -le "$msize" ]; then
				# the hash segment usually lives inside the .mdt already
				b=$(blk "$off" "$sz")
				dd if="$mdt" of="$out" bs=$b skip=$((off / b)) seek=$((off / b)) count=$((sz / b)) conv=notrunc 2>/dev/null || return 1
			else
				echo "missing $(basename "$seg")" >&2; return 1
			fi
		fi
		i=$((i + 1))
	done
}

ok=0; warn=0; fail=0
TAB=$(printf '\t')
while IFS="$TAB" read -r target sha src; do
	case "$target" in ''|'#'*) continue ;; esac
	src=${src%% *}                       # drop "+ .bNN" and trailing comments
	dst=$OUT/$target
	mkdir -p "$(dirname "$dst")"
	case "$src" in
	*.mdt) err=$(squash "$R$src" "$dst" 2>&1) || { rm -f "$dst"; echo "FAIL  $target ($err)"; fail=$((fail + 1)); continue; } ;;
	*)     cp "$R$src" "$dst" 2>/dev/null || { echo "FAIL  $target (no $src)"; fail=$((fail + 1)); continue; } ;;
	esac
	got=$(sha256sum < "$dst" | cut -d' ' -f1)   # stdin: no "\" prefix for odd paths
	if [ "$got" = "$sha" ]; then echo "ok    $target"; ok=$((ok + 1))
	else echo "WARN  $target (sha256 differs from manifest: other firmware version?)"; warn=$((warn + 1)); fi
done < "$MANIFEST"
echo "== $ok ok, $warn hash mismatch, $fail missing -> $OUT"
[ $fail -eq 0 ]
