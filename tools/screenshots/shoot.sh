#!/bin/sh
# SPDX-License-Identifier: MIT
# shoot.sh -- screenshots of the settings app for the docs, taken on the tablet
# in a headless GNOME Shell with a 1600x2560 virtual monitor at 200 % (like the
# panel). The desktop on the panel is not touched: the shell runs on its own
# D-Bus session, runtime directory and an empty home, so it starts from GNOME's
# defaults. Run as root on a root with GNOME Shell 48 and python3-gi:
#
#   sh shoot.sh OUTDIR [--app PATH] [--kernel FILE] [shoot.py options]
#
#   --app PATH     the tb323fu-settings binary (default: the one in PATH)
#   --kernel FILE  copied to ~/Downloads of the test home, for the
#                  "Install Kernel from File" pictures
#
# Needs: gnome-shell, gdctl, gir1.2-atspi-2.0, python3-gi. Environment:
# DESKTOP_USER (default: the user with uid 1000).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
[ $# -ge 1 ] || { sed -n '3,16p' "$0"; exit 2; }
out=$(realpath -m "$1"); shift
app=$(command -v tb323fu-settings || true)
kernel=
while [ $# -gt 0 ]; do
	case $1 in
	--app) app=$(realpath "$2"); shift 2 ;;
	--kernel) kernel=$(realpath "$2"); shift 2 ;;
	*) break ;;
	esac
done
[ -x "$app" ] || { echo "shoot.sh: no tb323fu-settings (--app)" >&2; exit 1; }
u=${DESKTOP_USER:-$(id -nu 1000)}
w=$(mktemp -d /tmp/tb323fu-shots.XXXXXX)
cleanup() {
	for m in "$w/run/doc" "$w/run/gvfs"; do
		mountpoint -q "$m" && umount -l "$m"
	done
	rm -rf "$w" 2>/dev/null || true
}
trap cleanup EXIT
mkdir -p "$w/home/Downloads" "$w/run" "$w/out" "$out"
chmod 700 "$w/run"
cp "$here/shoot.py" "$app" "$w/"
if [ -n "$kernel" ]; then
	# in the home folder, where the file chooser opens (and in Recent)
	k="$w/home/$(basename "$kernel")"
	cp "$kernel" "$k"
	mkdir -p "$w/home/.local/share"
	now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
	cat >"$w/home/.local/share/recently-used.xbel" <<-XBEL
	<?xml version="1.0" encoding="UTF-8"?>
	<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks" xmlns:mime="http://www.freedesktop.org/standards/shared-mime-info">
	<bookmark href="file://$k" added="$now" modified="$now" visited="$now"><info><metadata owner="http://freedesktop.org">
	<mime:mime-type type="application/gzip"/><bookmark:applications><bookmark:application name="Files" exec="&apos;nautilus %u&apos;" modified="$now" count="1"/></bookmark:applications>
	</metadata></info></bookmark>
	</xbel>
	XBEL
fi
chown -R "$u" "$w"
runuser -u "$u" -- env -i PATH=/usr/local/bin:/usr/bin:/bin HOME="$w/home" USER="$u" \
	XDG_RUNTIME_DIR="$w/run" XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=GNOME LANG=C.UTF-8 \
	dbus-run-session -- sh -c '
	dbus-update-activation-environment WAYLAND_DISPLAY=shots-0 XDG_RUNTIME_DIR XDG_SESSION_TYPE XDG_CURRENT_DESKTOP
	gsettings set org.gnome.shell welcome-dialog-last-shown-version 999
	gnome-shell --headless --wayland --no-x11 --wayland-display shots-0 \
		--virtual-monitor 1600x2560 --sm-disable >"$1/shell.log" 2>&1 &
	shell=$!
	gdbus wait --session --timeout 30 org.gnome.Shell.Screenshot || { tail -n 20 "$1/shell.log" >&2; kill $shell; exit 1; }
	export WAYLAND_DISPLAY=shots-0
	gdctl set --logical-monitor --primary --monitor Meta-0 --scale 2 >/dev/null
	shift
	python3 "$0/shoot.py" "$@" 2>&1; rc=$?
	kill $shell; wait $shell 2>/dev/null
	exit $rc' "$w" "$w" --app "$w/$(basename "$app")" --out "$w/out" ${kernel:+--kernel-name "$(basename "$kernel")"} "$@" 2>"$w/session.log" || {
	cp -r "$w/out/." "$w/session.log" "$out/"
	echo "shoot.sh: failed; the session's log ($out/session.log) ends with:" >&2
	tail -n 15 "$w/session.log" >&2
	exit 1
}
cp -r "$w/out/." "$out/"
echo "shoot.sh: pictures in $out"
