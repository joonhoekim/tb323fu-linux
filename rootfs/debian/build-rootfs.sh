#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- a Debian arm64 root filesystem for the TB323FU. Debian and
# Ubuntu share one builder; this runs it with OS=debian.
#
#   sh build-rootfs.sh TARGET_DIR [RELEASE]          # default RELEASE trixie (Debian 13)
#
# The environment is that of rootfs/ubuntu/build-rootfs.sh. Needs: debootstrap,
# debian-archive-keyring.
OS=debian exec sh "$(dirname "$0")/../ubuntu/build-rootfs.sh" "$@"
