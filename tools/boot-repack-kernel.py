#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Put a different kernel into a stock boot image, keeping the stock vbmeta blob.

The bootloader on this device (ABL patched in memory by the GBL in `efisp`)
boots a `boot` whose content digest no longer matches its vbmeta hash
descriptor, but it still wants the authentication data at the end of the image
to be the stock, stock-key-signed blob. That was confirmed on the device on
2026-09-22 with a test image, so this tool keeps that blob byte-for-byte
instead of re-signing anything.

Everything except the kernel is copied from the stock image:

    [0, 4096)             header v4, only `kernel_size` changed
    [4096, …)             the new kernel, zero-padded to a page
    [payload_end, +16K)   the stock GKI boot signature, copied verbatim
    [vbmeta_offset, …)    the stock vbmeta blob, copied verbatim
    …zeros…               up to the stock image size
    [end - 64, end)       AVB footer, offsets moved to match the new layout

The GKI boot signature block covers the stock kernel, so with another kernel it
is stale — as is the vbmeta hash descriptor. Both are kept because the point is
to change one thing (the kernel) and leave the shape of the image alone.
`--drop-boot-signature` leaves it out and sets the header's `boot_signature_size`
to 0, if that ever needs comparing.

Only header v4 images with an uncompressed arm64 Image and a ramdisk-less
layout (this device's `boot.img`) are supported.

usage:
  boot-repack-kernel.py STOCK_BOOT NEW_KERNEL OUT_BOOT
  boot-repack-kernel.py STOCK_BOOT --self-test        # rebuild with the stock
                                                      # kernel, expect identity
"""

import argparse
import hashlib
import struct
import sys

PAGE = 4096
FOOTER_SIZE = 64
BOOT_MAGIC = b"ANDROID!"
# boot_img_hdr_v4: magic[8], kernel_size, ramdisk_size, os_version,
# header_size, reserved[4], header_version, cmdline[1536], signature_size
OFF_KERNEL_SIZE = 8
OFF_RAMDISK_SIZE = 12
OFF_HEADER_VERSION = 40
OFF_BOOT_SIG_SIZE = 1580


def pad(n):
    return (n + PAGE - 1) // PAGE * PAGE


class StockBoot:
    """The parts of a stock boot image this tool copies or replaces."""

    def __init__(self, data):
        self.data = data
        if data[:8] != BOOT_MAGIC:
            sys.exit("not an Android boot image")
        self.header_version = struct.unpack_from("<I", data, OFF_HEADER_VERSION)[0]
        if self.header_version != 4:
            sys.exit(f"header version {self.header_version} not supported (want 4)")
        self.kernel_size = struct.unpack_from("<I", data, OFF_KERNEL_SIZE)[0]
        ramdisk_size = struct.unpack_from("<I", data, OFF_RAMDISK_SIZE)[0]
        if ramdisk_size:
            sys.exit(f"image has a {ramdisk_size}-byte ramdisk; not supported")
        self.boot_sig_size = struct.unpack_from("<I", data, OFF_BOOT_SIG_SIZE)[0]

        self.kernel = data[PAGE:PAGE + self.kernel_size]
        if self.kernel[56:60] != b"ARM\x64":
            sys.exit("kernel is not an uncompressed arm64 Image")

        footer_at = len(data) - FOOTER_SIZE
        if data[footer_at:footer_at + 4] != b"AVBf":
            sys.exit("no AVB footer at the end of the image")
        self.footer = data[footer_at:]
        (self.footer_major, self.footer_minor, self.original_image_size,
         self.vbmeta_offset, self.vbmeta_size) = struct.unpack_from(">IIQQQ", self.footer, 4)
        self.vbmeta = data[self.vbmeta_offset:self.vbmeta_offset + self.vbmeta_size]
        if self.vbmeta[:4] != b"AVB0":
            sys.exit("vbmeta blob does not start with AVB0")

        # payload = header + kernel + boot signature, each page-aligned.
        # This image carries a GKI boot signature although the header's
        # `signature_size` field is 0, so its size comes from the footer: the
        # AVB hash descriptor covers the payload, signature included.
        self.payload_end = pad(PAGE + self.kernel_size)
        if self.original_image_size != self.vbmeta_offset:
            sys.exit(
                f"footer original_image_size={self.original_image_size} is not "
                f"vbmeta_offset={self.vbmeta_offset}; layout not supported"
            )
        sig_size = self.original_image_size - self.payload_end
        if sig_size < 0:
            sys.exit(f"payload ends at {self.payload_end}, past the vbmeta blob")
        self.boot_signature = data[self.payload_end:self.original_image_size]
        if sig_size and self.boot_signature[:4] != b"AVB0":
            sys.exit(f"the {sig_size} bytes after the kernel are not an AVB0 block")

    def repack(self, kernel, keep_boot_signature=True):
        """Return a full-size image with `kernel` in place of the stock one."""
        sig = self.boot_signature if keep_boot_signature else b""
        out = bytearray(self.data[:PAGE])
        struct.pack_into("<I", out, OFF_KERNEL_SIZE, len(kernel))
        out += kernel
        out += b"\0" * (pad(len(out)) - len(out))
        out += sig
        out += b"\0" * (pad(len(out)) - len(out))

        vbmeta_offset = len(out)
        out += self.vbmeta
        if len(out) > len(self.data) - FOOTER_SIZE:
            sys.exit(
                f"repacked image ({len(out)} bytes + footer) does not fit the "
                f"{len(self.data)}-byte partition"
            )
        out += b"\0" * (len(self.data) - FOOTER_SIZE - len(out))
        out += struct.pack(">4sIIQQQ", b"AVBf", self.footer_major, self.footer_minor,
                           vbmeta_offset, vbmeta_offset, len(self.vbmeta))
        out += self.footer[36:]  # the footer's 28 reserved bytes, as they are
        return bytes(out)


def describe(stock):
    print(f"stock: header v{stock.header_version} (signature_size field "
          f"{stock.boot_sig_size}), kernel {stock.kernel_size}, "
          f"boot signature {len(stock.boot_signature)} @ {stock.payload_end}, "
          f"vbmeta {stock.vbmeta_size} @ {stock.vbmeta_offset}, "
          f"image {len(stock.data)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("stock")
    ap.add_argument("kernel", nargs="?")
    ap.add_argument("out", nargs="?")
    ap.add_argument("--self-test", action="store_true",
                    help="repack with the stock kernel; the result must equal the input")
    ap.add_argument("--drop-boot-signature", action="store_true")
    args = ap.parse_args()

    stock = StockBoot(open(args.stock, "rb").read())
    describe(stock)

    if args.self_test:
        out = stock.repack(stock.kernel)
        if out == stock.data:
            print("self-test: repacked image is byte-for-byte the stock image")
            return
        if len(out) != len(stock.data):
            sys.exit(f"self-test FAILED: repacked {len(out)} bytes, stock {len(stock.data)}")
        first = next(i for i, (a, b) in enumerate(zip(out, stock.data)) if a != b)
        sys.exit(f"self-test FAILED: differs at offset {first}")

    if not args.kernel or not args.out:
        ap.error("need NEW_KERNEL and OUT_BOOT (or --self-test)")

    kernel = open(args.kernel, "rb").read()
    if kernel[56:60] != b"ARM\x64":
        sys.exit("new kernel is not an uncompressed arm64 Image")
    out = stock.repack(kernel, keep_boot_signature=not args.drop_boot_signature)
    open(args.out, "wb").write(out)

    grew = len(kernel) - stock.kernel_size
    print(f"kernel: {stock.kernel_size} -> {len(kernel)} ({grew:+d})")
    print(f"vbmeta blob moved to {struct.unpack_from('>Q', out, len(out) - FOOTER_SIZE + 20)[0]} "
          f"(stock {stock.vbmeta_offset})")
    print("stock sha256:", hashlib.sha256(stock.data).hexdigest())
    print("out   sha256:", hashlib.sha256(out).hexdigest())


if __name__ == "__main__":
    main()
