#!/bin/sh
# SPDX-License-Identifier: MIT
# install.sh -- install tb323fu-helperd and tb323fu-ctl (after `cargo build --release`).
#   ./install.sh [--uninstall]
#   PREFIX=/usr DESTDIR=/tmp/pkg ./install.sh          # packaging
# Defaults: PREFIX=/usr/local, LIBEXECDIR=$PREFIX/libexec, units and D-Bus /
# polkit files under $PREFIX/lib, $PREFIX/share (systemd, dbus and polkit read
# /usr/local/... only for some of these -- see below).
set -eu
PREFIX=${PREFIX:-/usr/local}
DESTDIR=${DESTDIR:-}
LIBEXECDIR=${LIBEXECDIR:-$PREFIX/libexec}
BINDIR=${BINDIR:-$PREFIX/bin}
# systemd looks in /etc/systemd/system and /usr/lib/systemd/system (not
# /usr/local/lib); dbus-daemon reads /etc/dbus-1/system.d and /usr/share/dbus-1;
# polkit reads /usr/share/polkit-1/actions. So a /usr/local install puts these
# into /etc and /usr/share; a PREFIX=/usr package uses the /usr paths.
if [ "$PREFIX" = /usr ]; then
	UNITDIR=${UNITDIR:-/usr/lib/systemd/system}
	DBUSCONF=${DBUSCONF:-/usr/share/dbus-1/system.d}
else
	UNITDIR=${UNITDIR:-/etc/systemd/system}
	DBUSCONF=${DBUSCONF:-/etc/dbus-1/system.d}
fi
DBUSSVC=${DBUSSVC:-/usr/share/dbus-1/system-services}
POLKIT=${POLKIT:-/usr/share/polkit-1/actions}
here=$(cd "$(dirname "$0")" && pwd)
B=$here/target/release

files() {
	echo "$LIBEXECDIR/tb323fu-helperd $BINDIR/tb323fu-ctl $UNITDIR/tb323fu-helperd.service"
	echo "$DBUSCONF/io.github.joonhoekim.tb323fu.Helper.conf $DBUSSVC/io.github.joonhoekim.tb323fu.Helper.service"
	echo "$POLKIT/io.github.joonhoekim.tb323fu.helper.policy"
}

if [ "${1:-}" = --uninstall ]; then
	[ -z "$DESTDIR" ] && systemctl disable --now tb323fu-helperd.service 2>/dev/null || true
	for f in $(files); do rm -f "$DESTDIR$f"; done
	[ -z "$DESTDIR" ] && systemctl daemon-reload || true
	echo "removed (settings kept in /etc/tb323fu)"
	exit 0
fi

[ -x "$B/tb323fu-helperd" ] && [ -s "$B/tb323fu-helperd" ] || { echo "build first: cargo build --release" >&2; exit 1; }
install -Dm755 "$B/tb323fu-helperd" "$DESTDIR$LIBEXECDIR/tb323fu-helperd"
install -Dm755 "$B/tb323fu-ctl" "$DESTDIR$BINDIR/tb323fu-ctl"
sed "s|@LIBEXECDIR@|$LIBEXECDIR|" "$here/data/tb323fu-helperd.service" > "$here/data/.unit"
install -Dm644 "$here/data/.unit" "$DESTDIR$UNITDIR/tb323fu-helperd.service"
rm -f "$here/data/.unit"
install -Dm644 "$here/data/io.github.joonhoekim.tb323fu.Helper.conf" "$DESTDIR$DBUSCONF/io.github.joonhoekim.tb323fu.Helper.conf"
install -Dm644 "$here/data/io.github.joonhoekim.tb323fu.Helper.service" "$DESTDIR$DBUSSVC/io.github.joonhoekim.tb323fu.Helper.service"
install -Dm644 "$here/data/io.github.joonhoekim.tb323fu.helper.policy" "$DESTDIR$POLKIT/io.github.joonhoekim.tb323fu.helper.policy"
install -Dm644 "$here/data/helper.toml.example" "$DESTDIR$PREFIX/share/doc/tb323fu-helper/helper.toml.example"
if [ -z "$DESTDIR" ]; then
	sync
	systemctl daemon-reload
	systemctl reload dbus 2>/dev/null || true
	systemctl enable --now tb323fu-helperd.service
	echo "installed; tb323fu-ctl status"
fi
