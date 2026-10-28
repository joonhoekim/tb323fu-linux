#!/bin/sh
# SPDX-License-Identifier: MIT
# iio-ssc.sh -- run inside the Ubuntu root (chroot) by build-rootfs.sh: build
# libssc and an iio-sensor-proxy with its SSC backend into /usr/local. Ubuntu
# packages neither (its iio-sensor-proxy has no SSC backend), and the sensor
# hub is reachable only through SSC. The packaged iio-sensor-proxy stays for
# its unit, D-Bus policy and polkit rules; a drop-in runs this build instead.
#
#   sh iio-ssc.sh PATCHDIR      # PATCHDIR: rootfs/patches/iio-sensor-proxy
#
# The build dependencies are removed again; the libraries the result links to
# are kept.
set -eu
LIBSSC_URL=https://codeberg.org/DylanVanAssche/libssc.git
LIBSSC_REV=54dd13e36d65c21263cf5b0f4a70c230a77b26bc     # 0.4.4 + 8
IIO_URL=https://gitlab.freedesktop.org/hadess/iio-sensor-proxy.git
IIO_REV=0085ddf8ecb173a1c5fcf2344aa40e561125354f        # 3.9
patches=${1:?usage: iio-ssc.sh PATCHDIR}

deps="git patch meson ninja-build gcc pkg-config libglib2.0-dev libqmi-glib-dev libprotobuf-c-dev
	protobuf-c-compiler protobuf-compiler libgudev-1.0-dev libpolkit-gobject-1-dev libsystemd-dev
	libudev-dev systemd-dev python3-dev"
new=
for p in $deps; do dpkg -s "$p" >/dev/null 2>&1 || new="$new $p"; done
# shellcheck disable=SC2086
[ -z "$new" ] || apt-get -y -q --no-install-recommends install --mark-auto $new

src=$(mktemp -d /var/tmp/iio-ssc.XXXXXX)
trap 'rm -rf "$src"' EXIT
fetch() { # url rev dir
	git init -q "$3"
	git -C "$3" fetch -q --depth 1 "$1" "$2"
	git -C "$3" checkout -q FETCH_HEAD
}
fetch "$LIBSSC_URL" "$LIBSSC_REV" "$src/libssc"
fetch "$IIO_URL" "$IIO_REV" "$src/iio"
for p in "$patches"/*.patch; do patch -d "$src/iio" -p1 < "$p"; done

meson setup "$src/libssc/_build" "$src/libssc" --prefix=/usr/local --buildtype=release
ninja -C "$src/libssc/_build" install
ldconfig
meson setup "$src/iio/_build" "$src/iio" --prefix=/usr/local --buildtype=release -Dssc-support=enabled
ninja -C "$src/iio/_build"
install -Dm755 "$src/iio/_build/src/iio-sensor-proxy" /usr/local/libexec/iio-sensor-proxy

# keep what the result links to, then drop the build dependencies
for f in /usr/local/libexec/iio-sensor-proxy /usr/local/lib/libssc.so.* /usr/local/lib/*/libssc.so.*; do
	[ -e "$f" ] && ldd "$f" | awk '/=> \// { print $3 }'
done | sort -u | while read -r l; do
	case $l in /usr/local/*) continue ;; esac
	dpkg -S "$(readlink -f "$l")" 2>/dev/null || dpkg -S "$l" 2>/dev/null || true
done | cut -d: -f1 | sort -u | xargs -r apt-mark manual >/dev/null
[ -z "$new" ] || apt-get -y -q autoremove --purge
/usr/local/libexec/iio-sensor-proxy --help >/dev/null
echo "iio-ssc: libssc $LIBSSC_REV, iio-sensor-proxy $IIO_REV with SSC"
