#!/bin/sh
# SPDX-License-Identifier: MIT
# install.sh -- install the TB323FU platform files (layer 1) into a root
# filesystem. No package manager is called; distribution packages/recipes can
# run this with DESTDIR set, or copy the same files themselves.
#
#   sh install.sh                          # into / (as root)
#   DESTDIR=/mnt/root sh install.sh        # into a mounted root filesystem
#   PREFIX=/usr SYSCONFDIR=/etc CC=cc      # defaults
#   OPTIONAL="logind debian-iio image-growroot" sh install.sh
#
# Existing files under $SYSCONFDIR/tb323fu are never overwritten (your settings,
# your Bluetooth address). keyhold is built from src/keyhold.c when a C compiler
# is available (CC); otherwise build it yourself and put it in
# $PREFIX/libexec/tb323fu/keyhold.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
DESTDIR=${DESTDIR:-}
PREFIX=${PREFIX:-/usr}
SYSCONFDIR=${SYSCONFDIR:-/etc}
CC=${CC:-cc}
OPTIONAL=${OPTIONAL:-}
libexec=$PREFIX/libexec/tb323fu

# Installed files name /usr/libexec/tb323fu and /etc/tb323fu; rewrite those
# when installing somewhere else (e.g. a NixOS store path).
fix() {
	[ "$PREFIX" = /usr ] && [ "$SYSCONFDIR" = /etc ] && return 0
	sed -i -e "s|/usr/libexec/tb323fu|$libexec|g" -e "s|/etc/tb323fu|$SYSCONFDIR/tb323fu|g" "$1"
}
put() { # mode src dst
	install -D -m "$1" "$here/$2" "$DESTDIR$3"
	fix "$DESTDIR$3"
}
putdir() { # mode srcdir dstdir
	for f in "$here/$2"/*; do [ -f "$f" ] && put "$1" "$2/$(basename "$f")" "$3/$(basename "$f")"; done
}

# udev
putdir 644 udev "$PREFIX/lib/udev/rules.d"
put 644 udev-override/90-feedbackd.rules "$SYSCONFDIR/udev/rules.d/90-feedbackd.rules"

# systemd
for u in "$here"/systemd/system/*.service; do put 644 "systemd/system/$(basename "$u")" "$PREFIX/lib/systemd/system/$(basename "$u")"; done
putdir 644 systemd/system/hexagonrpcd.service.d "$PREFIX/lib/systemd/system/hexagonrpcd.service.d"
putdir 644 systemd/system/iio-sensor-proxy.service.d "$PREFIX/lib/systemd/system/iio-sensor-proxy.service.d"
putdir 644 systemd/system.conf.d "$PREFIX/lib/systemd/system.conf.d"
putdir 755 systemd/system-sleep "$PREFIX/lib/systemd/system-sleep"
putdir 644 systemd/user "$PREFIX/lib/systemd/user"

# helpers
putdir 755 libexec "$libexec"
if command -v "$CC" > /dev/null 2>&1; then
	mkdir -p "$DESTDIR$libexec"
	"$CC" -O2 -Wall -o "$DESTDIR$libexec/keyhold" "$here/src/keyhold.c"
else
	echo "note: no C compiler ($CC): build src/keyhold.c and install it as $libexec/keyhold" >&2
fi

# back-to-android: the Linux side of the Android<->Linux switch (android/); the
# emergency key and the helper run it
install -D -m 755 "$here/../../android/back-to-android" "$DESTDIR$PREFIX/sbin/back-to-android"
fix "$DESTDIR$PREFIX/sbin/back-to-android"

# audio, camera
put 644 alsa/ucm2/conf.d/kaanapali/LENOVO-TB323FU.conf "$PREFIX/share/alsa/ucm2/conf.d/kaanapali/LENOVO-TB323FU.conf"
put 644 alsa/ucm2/Lenovo/TB323FU/HiFi.conf "$PREFIX/share/alsa/ucm2/Lenovo/TB323FU/HiFi.conf"
putdir 644 pipewire/pipewire.conf.d "$PREFIX/share/pipewire/pipewire.conf.d"
putdir 644 wireplumber/wireplumber.conf.d "$PREFIX/share/wireplumber/wireplumber.conf.d"
putdir 644 libcamera/ipa/simple "$PREFIX/share/libcamera/ipa/simple"

# configuration: only when absent
for c in "$here"/etc/tb323fu/*; do
	d=$DESTDIR$SYSCONFDIR/tb323fu/$(basename "$c")
	[ -e "$d" ] || install -D -m 644 "$c" "$d"
done

# optional pieces
for o in $OPTIONAL; do
	case $o in
	logind) put 644 optional/logind.conf.d/tb323fu-powerkey.conf "$PREFIX/lib/systemd/logind.conf.d/tb323fu-powerkey.conf" ;;
	debian-iio) put 644 optional/debian/iio-sensor-proxy.service.d/tb323fu-libssc.conf "$PREFIX/lib/systemd/system/iio-sensor-proxy.service.d/tb323fu-libssc.conf" ;;
	image-growroot) put 644 optional/image/tb323fu-growroot.service "$PREFIX/lib/systemd/system/tb323fu-growroot.service" ;;
	upower-charge-limit) put 644 optional/udev/61-tb323fu-battery.rules "$PREFIX/lib/udev/rules.d/61-tb323fu-battery.rules" ;;
	led-group) put 644 optional/udev/74-tb323fu-leds.rules "$PREFIX/lib/udev/rules.d/74-tb323fu-leds.rules" ;;
	*) echo "unknown optional piece: $o" >&2; exit 2 ;;
	esac
done

cat <<EOF
Installed the TB323FU platform files under ${DESTDIR:-/}.
Enable the services (in the target system):
  systemctl enable tb323fu-gen-ids.service tb323fu-btaddr.service tb323fu-dsp.service \\
                   tb323fu-audio.service tb323fu-usb-port.service tb323fu-emergency-key.service
  systemctl --global enable tb323fu-speaker-gain.service
  udevadm control --reload
The emergency key needs $SYSCONFDIR/tb323fu/android-boot.sha256 (the hash printed when
preparing boot_b, see android/README.md); without it the service does not start.
EOF
