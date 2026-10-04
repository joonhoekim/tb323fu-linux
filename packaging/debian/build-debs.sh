#!/bin/sh
# SPDX-License-Identifier: MIT
# build-debs.sh -- build Debian/Ubuntu binary packages from this repository
# with dpkg-deb (no debhelper needed). Run on the target architecture
# (arm64 on the tablet, or an arm64 builder) after building the Rust parts:
#   (cd helper && cargo build --release --locked)
#   (cd helper/crates/tb323fu-settings && cargo build --release --locked)
#   sh packaging/debian/build-debs.sh [OUTDIR]          # default ./out/deb
# Environment: VERSION (default 0.3.1), ARCH (default: dpkg --print-architecture).
#
# Packages:
#   tb323fu-platform      layer 1: udev rules, systemd units, audio/UCM, sensors,
#                         emergency key, back-to-android (arch-specific: keyhold)
#   tb323fu-helper        tb323fu-helperd (system D-Bus + polkit) and tb323fu-ctl
#   tb323fu-settings      the GTK4/libadwaita settings app
#   tb323fu-helper-gnome  GNOME Shell quick-settings extension (all)
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${1:-$root/out/deb}
VERSION=${VERSION:-0.3.1}
ARCH=${ARCH:-$(dpkg --print-architecture)}
MAINT="Joonhoe Kim <26rote@gmail.com>"
HOME_URL=https://github.com/joonhoekim/tb323fu-linux
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$OUT"

# copyright NAME LICENSE -- /usr/share/doc/NAME/copyright (machine-readable, DEP-5);
# GPL texts point at /usr/share/common-licenses, MIT is spelled out
copyright() {
	d=$work/$1/usr/share/doc/$1
	mkdir -p "$d"
	{
		echo "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
		echo "Upstream-Name: tb323fu-linux"
		echo "Source: $HOME_URL"
		echo
		echo "Files: *"
		echo "Copyright: 2026 Joonhoe Kim"
		echo "License: $2"
		case $2 in
		MIT) sed '1,4d; s/^$/./; s/^/ /' "$root/LICENSE" ;;
		GPL-3.0-or-later) echo " On Debian systems the full text is in /usr/share/common-licenses/GPL-3." ;;
		GPL-2.0-or-later) echo " On Debian systems the full text is in /usr/share/common-licenses/GPL-2." ;;
		esac
		echo "Comment: a file's own SPDX header takes precedence; see NOTICE in the source."
	} > "$d/copyright"
}

# control NAME ARCH DEPENDS RECOMMENDS DESCRIPTION-LINE [LONG]
control() {
	d=$work/$1/DEBIAN
	mkdir -p "$d"
	{
		echo "Package: $1"
		echo "Version: $VERSION"
		echo "Architecture: $2"
		echo "Maintainer: $MAINT"
		echo "Section: admin"
		echo "Priority: optional"
		echo "Homepage: $HOME_URL"
		[ -n "$3" ] && echo "Depends: $3"
		[ -n "$4" ] && echo "Recommends: $4"
		echo "Installed-Size: $(du -sk "$work/$1" | cut -f1)"
		echo "Description: $5"
		[ -n "${6:-}" ] && echo "$6" | sed 's/^/ /'
	} > "$d/control"
	# every file under etc/ is a conffile
	if [ -d "$work/$1/etc" ]; then
		(cd "$work/$1" && find etc -type f | sed 's|^|/|') > "$d/conffiles"
		[ -s "$d/conffiles" ] || rm -f "$d/conffiles"
	fi
}
script() { # NAME maintscript body
	printf '#!/bin/sh\nset -e\n%s\n' "$3" > "$work/$1/DEBIAN/$2"
	chmod 755 "$work/$1/DEBIAN/$2"
}
build() {
	find "$work/$1" -type d -exec chmod 755 {} +
	dpkg-deb --root-owner-group --build "$work/$1" "$OUT/${1}_${VERSION}_$2.deb" > /dev/null
	echo "$OUT/${1}_${VERSION}_$2.deb"
}

# ---- tb323fu-platform
DESTDIR=$work/tb323fu-platform PREFIX=/usr SYSCONFDIR=/etc VERSION=$VERSION sh "$root/userspace/platform/install.sh" > /dev/null
copyright tb323fu-platform MIT
control tb323fu-platform "$ARCH" "systemd, udev, bluez, swh-plugins, wireless-regdb" \
	"pipewire, wireplumber, alsa-ucm-conf, iio-sensor-proxy, hexagonrpcd, qrtr-tools, rmtfs, tqftpserv, tb323fu-helper" \
	"Lenovo Legion Tab Gen 5 (TB323FU) platform files" \
	"udev rules, systemd units (Bluetooth address, DSP start, audio defaults, USB port
routing, emergency key, per-install IDs), ALSA UCM, PipeWire speaker protection,
libcamera tuning and back-to-android. Needed on every install of this device.
.
Mask bootmac-bluetooth.service (qcom-phone-utils) if it is installed: it races
tb323fu-btaddr."
script tb323fu-platform postinst 'if [ "$1" = configure ]; then
	udevadm control --reload 2>/dev/null || true
	systemctl daemon-reload 2>/dev/null || true
	systemctl enable tb323fu-gen-ids.service tb323fu-btaddr.service tb323fu-dsp.service tb323fu-audio.service tb323fu-usb-port.service tb323fu-emergency-key.service tb323fu-kernel-confirm.service 2>/dev/null || true
	systemctl --global enable tb323fu-speaker-gain.service 2>/dev/null || true
	if systemctl list-unit-files bootmac-bluetooth.service >/dev/null 2>&1; then systemctl mask bootmac-bluetooth.service 2>/dev/null || true; fi
	[ -e /etc/tb323fu/android-boot.sha256 ] || echo "tb323fu-platform: put the Android boot image hash in /etc/tb323fu/android-boot.sha256 (see android/README.md) to arm the emergency key"
fi'
script tb323fu-platform prerm 'if [ "$1" = remove ]; then
	systemctl disable tb323fu-gen-ids.service tb323fu-btaddr.service tb323fu-dsp.service tb323fu-audio.service tb323fu-usb-port.service tb323fu-emergency-key.service tb323fu-kernel-confirm.service 2>/dev/null || true
	systemctl --global disable tb323fu-speaker-gain.service 2>/dev/null || true
fi'
build tb323fu-platform "$ARCH"

# ---- tb323fu-helper
DESTDIR=$work/tb323fu-helper PREFIX=/usr LIBEXECDIR=/usr/libexec/tb323fu sh "$root/helper/install.sh" > /dev/null
copyright tb323fu-helper GPL-3.0-or-later
control tb323fu-helper "$ARCH" "dbus, polkitd | policykit-1, systemd, curl" "tb323fu-platform" \
	"TB323FU device helper (charge limit, refresh, torch, LED ring, GPU, ...)" \
	"tb323fu-helperd owns the device knobs behind one system D-Bus service
(io.github.joonhoekim.OpenDeviceHelper1) with polkit checks; tb323fu-ctl is its CLI.
Settings: /etc/tb323fu/helper.toml."
script tb323fu-helper postinst 'if [ "$1" = configure ]; then
	systemctl daemon-reload 2>/dev/null || true
	systemctl reload dbus 2>/dev/null || true
	systemctl enable --now tb323fu-helperd.service 2>/dev/null || true
fi'
script tb323fu-helper prerm 'if [ "$1" = remove ]; then systemctl disable --now tb323fu-helperd.service 2>/dev/null || true; fi'
script tb323fu-helper postrm 'systemctl daemon-reload 2>/dev/null || true'
build tb323fu-helper "$ARCH"

# ---- tb323fu-settings
(cd "$root/helper/crates/tb323fu-settings" && DESTDIR=$work/tb323fu-settings PREFIX=/usr sh ./install.sh > /dev/null)
copyright tb323fu-settings GPL-3.0-or-later
control tb323fu-settings "$ARCH" "libgtk-4-1 (>= 4.12), libadwaita-1-0 (>= 1.5), tb323fu-helper" "" \
	"Settings app for the TB323FU helper (GTK4/libadwaita)"
build tb323fu-settings "$ARCH"

# ---- tb323fu-helper-gnome
uuid=tb323fu@joonhoekim.github.io
mkdir -p "$work/tb323fu-helper-gnome/usr/share/gnome-shell/extensions/$uuid"
cp "$root/userspace/desktop/gnome/extension/$uuid/"* "$work/tb323fu-helper-gnome/usr/share/gnome-shell/extensions/$uuid/"
chmod 644 "$work/tb323fu-helper-gnome/usr/share/gnome-shell/extensions/$uuid/"*
copyright tb323fu-helper-gnome GPL-2.0-or-later
control tb323fu-helper-gnome all "gnome-shell (>= 48), tb323fu-helper" "tb323fu-settings" \
	"GNOME quick settings for the TB323FU helper" \
	"Enable it per user with: gnome-extensions enable $uuid"
build tb323fu-helper-gnome all
