#!/bin/sh
# SPDX-License-Identifier: MIT
# install.sh -- install tb323fu-settings after `cargo build --release`.
#   PREFIX=/usr DESTDIR=... ./install.sh [--uninstall]
set -e
PREFIX=${PREFIX:-/usr/local}
D=${DESTDIR:-}
here=$(cd "$(dirname "$0")" && pwd)
id=io.github.joonhoekim.tb323fu.Settings
files="$PREFIX/bin/tb323fu-settings
$PREFIX/share/applications/$id.desktop
$PREFIX/share/icons/hicolor/scalable/apps/$id.svg
$PREFIX/share/metainfo/$id.metainfo.xml"
if [ "${1:-}" = --uninstall ]; then
	for f in $files; do rm -f "$D$f"; done
	echo "removed tb323fu-settings from $D$PREFIX"
	exit 0
fi
install -Dm755 "$here/target/release/tb323fu-settings" "$D$PREFIX/bin/tb323fu-settings"
install -Dm644 "$here/data/$id.desktop" "$D$PREFIX/share/applications/$id.desktop"
install -Dm644 "$here/data/$id.svg" "$D$PREFIX/share/icons/hicolor/scalable/apps/$id.svg"
install -Dm644 "$here/data/$id.metainfo.xml" "$D$PREFIX/share/metainfo/$id.metainfo.xml"
if [ -z "$D" ]; then
	command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
	command -v update-desktop-database >/dev/null && update-desktop-database -q "$PREFIX/share/applications" 2>/dev/null || true
fi
echo "installed tb323fu-settings into $D$PREFIX"
