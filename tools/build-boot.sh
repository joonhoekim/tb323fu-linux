#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# build-boot.sh -- build the kernel and pack it into a boot image for boot_a.
#
#   tools/build-boot.sh -k KERNEL_TREE -o OUT_DIR -s STOCK_BOOT.img [-i INITRAMFS_LIST] OUT.img
#
#   -k  kernel source tree with the patch series applied (see kernel/)
#   -o  build directory (make O=...), already configured (.config in it)
#   -s  a stock Android boot image from YOUR tablet (see below)
#   -i  optional: regenerate the initramfs first from a gen_init_cpio list
#       (the file named by CONFIG_INITRAMFS_SOURCE in OUT_DIR/.config is
#       rewritten; gzip -9)
#   -j  make jobs (default: nproc)
#
# The kernel carries its device tree and command line inside the Image (the
# bootloader's are not used), so `make dtbs` runs before `make Image`: in a
# fresh or changed build directory a single parallel make can link the Image
# before the .dtb it embeds exists.
#
# Packing: tools/boot-repack-kernel.py puts the new Image into the stock boot
# image and keeps everything else of it -- header v4, the stock GKI boot
# signature and vbmeta blob, the AVB footer (offsets adjusted). The bootloader
# (with the unlock tooling this project assumes) boots such an image; nothing
# is re-signed. The stock image must come from your own device and firmware
# version: on Android with root,
#     dd if=/dev/block/by-name/boot_a of=/sdcard/stock-boot.img
# (boot_a holds Android's kernel while Android runs; it is the same image this
# project keeps in boot_b as the way back, see android/README.md).
#
# Needs: clang/LLVM, make, python3; gen_init_cpio is built by the kernel
# (OUT_DIR/usr/gen_init_cpio).
set -eo pipefail
here=$(cd "$(dirname "$0")" && pwd)
kt= out= stock= list= jobs=$(nproc 2>/dev/null || echo 4)
while getopts "k:o:s:i:j:" o; do
	case $o in
	k) kt=$OPTARG ;; o) out=$OPTARG ;; s) stock=$OPTARG ;; i) list=$OPTARG ;; j) jobs=$OPTARG ;;
	*) sed -n '4,13p' "$0"; exit 2 ;;
	esac
done
shift $((OPTIND - 1))
img=${1:-}
[ -n "$kt" ] && [ -n "$out" ] && [ -n "$stock" ] && [ -n "$img" ] || { sed -n '4,13p' "$0"; exit 2; }
[ -f "$out/.config" ] || { echo "no $out/.config (configure the build directory first)"; exit 1; }
[ -f "$stock" ] || { echo "no stock boot image $stock"; exit 1; }
out=$(cd "$out" && pwd)

if [ -n "$list" ]; then
	irf=$(sed -n 's/^CONFIG_INITRAMFS_SOURCE="\(.*\)"$/\1/p' "$out/.config")
	[ -n "$irf" ] || { echo "CONFIG_INITRAMFS_SOURCE is not set in $out/.config"; exit 1; }
	[ -x "$out/usr/gen_init_cpio" ] || make -C "$kt" ARCH=arm64 LLVM=1 O="$out" usr/gen_init_cpio
	(cd "$(dirname "$list")" && "$out/usr/gen_init_cpio" "$(basename "$list")") | gzip -9 > "$irf"
	echo "initramfs $irf ($(stat -c %s "$irf") bytes)"
fi

make -C "$kt" ARCH=arm64 LLVM=1 O="$out" -j"$jobs" dtbs
make -C "$kt" ARCH=arm64 LLVM=1 O="$out" -j"$jobs" Image
python3 "$here/boot-repack-kernel.py" "$stock" "$out/arch/arm64/boot/Image" "$img" | tail -3
sha256sum "$img"
