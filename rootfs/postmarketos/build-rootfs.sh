#!/bin/sh
# SPDX-License-Identifier: MIT
# build-rootfs.sh -- a postmarketOS (edge / master) root filesystem with Phosh
# and systemd for the TB323FU, built natively on an arm64 host (for example
# the tablet itself running Linux) into a mounted, empty ext4 partition or image.
#
#   sh build-rootfs.sh TARGET_DIR
#
# Environment:
#   ROOT_PARTLABEL=baldur-root-sd   GPT name of the target partition (fstab)
#   HOSTNAME_NEW=tb323fu-pmos       hostname of the new system
#   DESKTOP=phosh                   phosh (touch-friendly GNOME/Wayland mobile shell)
#   TIMEZONE=                       e.g. Europe/Kyiv; default: host's timezone or UTC
#   FIRMWARE_FROM=/lib/firmware     copy qcom/ ath12k/ qca/ novatek/ aw882xx_acf.bin
#   CONFIG_FROM=/etc/tb323fu        copy bt-address, android-boot.sha256, audio.conf, ...
#   HEXAGONRPCD_FROM=/              copy hexagonrpcd and sensor tree
#   LADSPA_FROM=/usr/lib/ladspa     copy sc4_1882.so for speaker protection
#   DEV_USER=sotiomake              default user with sudo
#   BALDUR_DEV_PASSWORD=2141        password for the user and root
set -eu

T=${1:?usage: build-rootfs.sh TARGET_DIR}
ROOT_PARTLABEL=${ROOT_PARTLABEL:-baldur-root-sd}
HOSTNAME_NEW=${HOSTNAME_NEW:-tb323fu-pmos}
DESKTOP=${DESKTOP:-phosh}
TIMEZONE=${TIMEZONE:-$(readlink /etc/localtime 2>/dev/null | sed -n "s|.*/zoneinfo/||p")}
TIMEZONE=${TIMEZONE:-UTC}
FIRMWARE_FROM=${FIRMWARE_FROM:-/lib/firmware}
CONFIG_FROM=${CONFIG_FROM:-/etc/tb323fu}
HEXAGONRPCD_FROM=${HEXAGONRPCD_FROM:-/}
LADSPA_FROM=${LADSPA_FROM:-/usr/lib/ladspa}
DEV_USER=${DEV_USER:-sotiomake}
BALDUR_DEV_PASSWORD=${BALDUR_DEV_PASSWORD:-2141}

here=$(cd "$(dirname "$0")/../.." && pwd)
say() { printf '== %s\n' "$*"; }

mountpoint -q "$T" || { echo "$T is not a mount point"; exit 1; }

# run a shell command inside the target chroot
ch() {
	for m in proc sys dev run; do
		mountpoint -q "$T/$m" && continue
		case $m in
		proc) mount -t proc proc "$T/proc" ;;
		sys) mount --rbind /sys "$T/sys" && mount --make-rslave "$T/sys" ;;
		dev) mount --rbind /dev "$T/dev" && mount --make-rslave "$T/dev" ;;
		run) mount -t tmpfs tmpfs "$T/run" ;;
		esac
	done
	nice -n 19 chroot "$T" /usr/bin/env -i HOME=/root TERM=xterm \
		PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin LANG=C.UTF-8 /bin/sh -c "$*"
}

cleanup() {
	for m in run dev sys proc; do umount -R "$T/$m" 2>/dev/null || true; done
}
trap cleanup EXIT

# 1. Base system: download static apk-tools and keys, then bootstrap
if [ ! -x "$T/sbin/init" ]; then
	say "Bootstrapping postmarketOS with apk.static"
	w=$(mktemp -d)
	mkdir -p "$T/etc/apk/keys" "$T/var/cache/apk" "$T/proc" "$T/sys" "$T/dev" "$T/run"

	# Download apk.static
	curl -fsSL -o "$w/apk.apk" "https://dl-cdn.alpinelinux.org/alpine/v3.21/main/aarch64/apk-tools-static-2.14.13-r0.apk"
	tar -xzf "$w/apk.apk" -C "$w" sbin/apk.static
	APK="$w/sbin/apk.static"
	chmod +x "$APK"

	# Download alpine and postmarketos keys
	curl -fsSL -o "$w/alpine-keys.apk" "https://dl-cdn.alpinelinux.org/alpine/v3.21/main/aarch64/alpine-keys-2.5-r0.apk"
	tar -xzf "$w/alpine-keys.apk" -C "$T" etc/apk/keys/
	curl -fsSL -o "$w/pmos-keys.apk" "https://mirror.postmarketos.org/postmarketos/master/aarch64/postmarketos-keys-1-r1.apk"
	tar -xzf "$w/pmos-keys.apk" -C "$T" etc/apk/keys/

	# Repositories
	cat <<-EOF > "$T/etc/apk/repositories"
		https://dl-cdn.alpinelinux.org/alpine/edge/main
		https://dl-cdn.alpinelinux.org/alpine/edge/community
		https://dl-cdn.alpinelinux.org/alpine/edge/testing
		https://mirror.postmarketos.org/postmarketos/master
		https://mirror.postmarketos.org/postmarketos/extra-repos/systemd/master
	EOF

	# Initialize database and install packages
	"$APK" --root "$T" --initdb add alpine-keys
	"$APK" --root "$T" update

	pk="alpine-baselayout busybox musl-utils coreutils
	    systemd postmarketos-base-systemd
	    postmarketos-ui-phosh postmarketos-ui-phosh-systemd
	    phosh phoc greetd-phrog greetd-phrog-systemd
	    phosh-mobile-settings squeekboard squeekboard-systemd
	    mesa mesa-vulkan-freedreno libdisplay-info
	    xdg-desktop-portal-gtk xdg-desktop-portal-phosh
	    pipewire pipewire-pulse-systemd wireplumber wireplumber-systemd
	    pipewire-filter-graph-builtin pipewire-filter-graph-ladspa pipewire-spa-bluez
	    alsa-ucm-conf alsa-utils
	    networkmanager networkmanager-systemd networkmanager-wifi wpa_supplicant
	    bluez bluez-systemd wireless-regdb
	    iio-sensor-proxy-systemd postmarketos-base-ui-iio-sensor-proxy-systemd
	    feedbackd feedbackd-systemd qrtr
	    sudo shadow bash dbus polkit ca-certificates gcompat
	    alpine-appstream-downloader gnome-software gnome-software-systemd
	    gnome-software-plugin-flatpak gnome-software-plugin-apk2
	    flatpak flatpak-systemd flatpak-polkit postmarketos-base-ui-flatpak
	    python3 py3-gobject3 py3-dbus"

	say "Installing packages into rootfs..."
	# shellcheck disable=SC2086
	"$APK" --root "$T" add $pk
	rm -rf "$w"
fi

# Ensure /etc/machine-id exists
ch "systemd-machine-id-setup"

# DNS for chroot steps
cp -L /etc/resolv.conf "$T/etc/resolv.conf" 2>/dev/null || true

# 2. Systemd PATH & compatibility symlinks (Alpine does not usr-merge /bin and /sbin)
say "Setting up systemd environment and compatibility symlinks"
mkdir -p "$T/etc/systemd/system.conf.d" "$T/etc/systemd/user.conf.d"

cat <<-EOF > "$T/etc/systemd/system.conf.d/00-env.conf"
	[Manager]
	DefaultEnvironment="PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
EOF

cat <<-EOF > "$T/etc/systemd/user.conf.d/00-env.conf"
	[Manager]
	DefaultEnvironment="PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" "XDG_CURRENT_DESKTOP=Phosh:GNOME" "XDG_SESSION_DESKTOP=phosh"
EOF

cat <<-EOF >> "$T/etc/environment"
	XDG_CURRENT_DESKTOP=Phosh:GNOME
	XDG_SESSION_DESKTOP=phosh
EOF

# Compatibility symlinks from /bin and /sbin to /usr/bin and /usr/sbin
ch '/bin/sh -c '\''
for f in /bin/*; do b=$(basename "$f"); [ -e "/usr/bin/$b" ] || ln -s "/bin/$b" "/usr/bin/$b"; done
for f in /sbin/*; do b=$(basename "$f"); [ -e "/usr/sbin/$b" ] || ln -s "/sbin/$b" "/usr/sbin/$b"; done
'\'''

# 3. Device firmware and audio DSP topology
say "Installing device firmware and audio topology"
mkdir -p "$T/usr/lib/firmware"
fw=$(cd "$FIRMWARE_FROM" && ls -d ath12k qcom qca novatek aw882xx_acf.bin 2>/dev/null) || true
if [ -n "$fw" ]; then
	( cd "$FIRMWARE_FROM" && tar cf - $fw ) | tar xpf - -C "$T/usr/lib/firmware"
fi
tplg=qcom/kaanapali/LENOVO-TB323FU-tplg.bin
if [ -f "$here/firmware/audio/${tplg##*/}" ]; then
	install -Dm644 "$here/firmware/audio/${tplg##*/}" "$T/usr/lib/firmware/$tplg"
fi
if [ -d "$T/lib/firmware" ] && [ ! -L "$T/lib/firmware" ]; then
	( cd "$T/lib/firmware" && ln -sf ../../usr/lib/firmware/* . 2>/dev/null || true )
fi

# 4. Platform files & udev rules
say "Installing platform files and services"
env DESTDIR="$T" CC=true sh "$here/userspace/platform/install.sh"

# Resilient tb323fu-dsp.service with shell read and explicit PATH
cat <<-EOF > "$T/usr/lib/systemd/system/tb323fu-dsp.service"
	# SPDX-License-Identifier: MIT
	[Unit]
	Description=Start the audio and compute DSPs once their firmware is reachable
	DefaultDependencies=no
	After=local-fs.target
	Before=sound.target
	ConditionPathExists=/lib/firmware/qcom/kaanapali/lenovo/baldur/adsp.mbn

	[Service]
	Type=oneshot
	RemainAfterExit=yes
	Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
	ExecStart=/bin/sh -c 'fw=/lib/firmware/qcom/kaanapali/lenovo/baldur; rc=0; for want in adsp cdsp; do [ -f \$fw/\$want.mbn ] || { echo "\$want: no firmware, skipped"; continue; }; f=; for r in /sys/class/remoteproc/remoteproc*; do [ -f \$r/name ] || continue; read -r name < \$r/name; [ "\$name" = \$want ] || continue; f=1; read -r state < \$r/state; [ "\$state" = running ] && break; echo start > \$r/state || rc=1; done; [ -n "\$f" ] || echo "no \$want remoteproc"; done; exit \$rc'

	[Install]
	WantedBy=sysinit.target
EOF

# Mask modem services (rmtfs crashes without cellular/GNSS antenna and resets SoC)
ch "systemctl mask rmtfs tqftpserv >/dev/null 2>&1 || true"
ch "systemctl mask bootmac-bluetooth.service >/dev/null 2>&1 || true"

# Disable interactive boot-menu timeout fallback (panel binds at ~19s)
if [ -f "$T/etc/tb323fu/boot-menu" ]; then
	mv "$T/etc/tb323fu/boot-menu" "$T/etc/tb323fu/boot-menu.disabled"
fi

# 5. Sensors (Hexagon FastRPC & QRTR nameserver)
say "Installing sensors and fastrpc infrastructure"
if [ -d /var/lib/tb323fu/sensors ]; then
	mkdir -p "$T/var/lib/tb323fu"
	cp -a /var/lib/tb323fu/sensors "$T/var/lib/tb323fu/"
fi

# Build and install qrtr-ns natively
if [ ! -x "$T/usr/bin/qrtr-ns" ]; then
	say "Building qrtr-ns"
	ch "apk add --no-cache build-base meson ninja"
	w=$(mktemp -d "$T/tmp/qrtr-build.XXXXXX")
	wb="${w#$T}"
	git clone --depth 1 -b v1.1 https://github.com/linux-msm/qrtr.git "$w/src"
	ch "cd $wb/src && meson setup build -Dqrtr-ns=enabled && ninja -C build install"
	cp -a "$T/usr/local/bin/qrtr-ns" "$T/usr/bin/qrtr-ns"
	cp -a "$T/usr/local/lib"/libqrtr* "$T/usr/lib/" 2>/dev/null || true
	rm -rf "$w"
	ch "apk del meson ninja"
fi

cat <<-EOF > "$T/usr/lib/systemd/system/qrtr-ns.service"
	[Unit]
	Description=QIPCRTR Name Service

	[Service]
	ExecStart=/usr/bin/qrtr-ns -f 1
	Restart=always
	RestartSec=1

	[Install]
	WantedBy=multi-user.target
EOF

# hexagonrpcd wrapper and service drop-in
if [ -n "$HEXAGONRPCD_FROM" ] && [ -x "$HEXAGONRPCD_FROM/usr/libexec/hexagonrpc/hexagonrpcd" ]; then
	say "Installing hexagonrpcd sensors helper"
	H=$HEXAGONRPCD_FROM
	for f in usr/bin/hexagonrpcd usr/libexec/hexagonrpc usr/lib/systemd/system/hexagonrpcd.service \
		usr/lib/systemd/system/hexagonrpcd-*.service usr/lib/udev/rules.d/60-hexagonrpcd.rules; do
		for g in "$H"/$f; do [ -e "$g" ] || continue; mkdir -p "$T/$(dirname "${g#$H/}")"; cp -a "$g" "$T/${g#$H/}"; done
	done
	for l in "$H"/usr/lib/*/libhexagonrpc* "$H"/usr/lib/libhexagonrpc* "$H"/usr/lib64/libhexagonrpc*; do
		[ -e "$l" ] && cp -a "$l" "$T/usr/lib/" || true
	done
	ch "id fastrpc >/dev/null 2>&1 || useradd -r -s /bin/false -d /var/lib/fastrpc fastrpc"
fi

mkdir -p "$T/usr/lib/systemd/system/hexagonrpcd.service.d"
cat <<-EOF > "$T/usr/lib/systemd/system/hexagonrpcd.service.d/tb323fu.conf"
	# SPDX-License-Identifier: MIT
	[Unit]
	After=tb323fu-dsp.service tb323fu-sensors-tree.service qrtr-ns.service
	Wants=tb323fu-dsp.service tb323fu-sensors-tree.service qrtr-ns.service

	[Service]
	Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
	Environment=hexagonrpcd_device=/dev/fastrpc-adsp-secure
	Environment=hexagonrpcd_fw_dir=/var/lib/tb323fu/sensors
	Restart=always
	RestartSec=2
	ExecStartPre=/bin/sh -c 'for i in \$(seq 1 30); do [ -e /dev/fastrpc-adsp-secure ] && exit 0; sleep 0.5; done; exit 1'
EOF

# IIO sensor proxy drop-in (AF_QIPCRTR socket support)
mkdir -p "$T/etc/systemd/system/iio-sensor-proxy.service.d"
cat <<-EOF > "$T/etc/systemd/system/iio-sensor-proxy.service.d/tb323fu-libssc.conf"
	[Unit]
	StartLimitIntervalSec=0
	After=hexagonrpcd.service
	Wants=hexagonrpcd.service

	[Service]
	RestrictAddressFamilies=AF_UNIX AF_LOCAL AF_NETLINK AF_QIPCRTR
	Restart=always
	RestartSec=3
	TimeoutStopSec=3
EOF

# FastRPC udev rules
cat <<-EOF > "$T/etc/udev/rules.d/60-hexagonrpcd.rules"
	SUBSYSTEM=="misc", KERNEL=="fastrpc-*", OWNER="fastrpc", GROUP="fastrpc", MODE="660"
EOF

cat <<-EOF > "$T/etc/udev/rules.d/99-fastrpc.rules"
	KERNEL=="fastrpc-*", MODE="0666"
	SUBSYSTEM=="dma_heap", KERNEL=="*", MODE="0666"
EOF

# 6. Audio LADSPA plugins & PipeWire services
say "Installing audio protection plugins"
if [ -f "$LADSPA_FROM/sc4_1882.so" ]; then
	mkdir -p "$T/usr/lib/ladspa"
	cp -a "$LADSPA_FROM/sc4_1882.so" "$T/usr/lib/ladspa/"
	[ -f "$LADSPA_FROM/fast_lookahead_limiter_1913.so" ] && cp -a "$LADSPA_FROM/fast_lookahead_limiter_1913.so" "$T/usr/lib/ladspa/"
fi

# Enable user audio units
mkdir -p "$T/etc/systemd/user/sockets.target.wants" "$T/etc/systemd/user/default.target.wants"
ln -sf /usr/lib/systemd/user/pipewire.socket "$T/etc/systemd/user/sockets.target.wants/pipewire.socket"
ln -sf /usr/lib/systemd/user/pipewire-pulse.socket "$T/etc/systemd/user/sockets.target.wants/pipewire-pulse.socket"
ln -sf /usr/lib/systemd/user/pipewire.service "$T/etc/systemd/user/default.target.wants/pipewire.service"
ln -sf /usr/lib/systemd/user/wireplumber.service "$T/etc/systemd/user/default.target.wants/wireplumber.service"
mkdir -p "$T/etc/systemd/user/pipewire.service.wants"
ln -sf /usr/lib/systemd/user/tb323fu-speaker-gain.service "$T/etc/systemd/user/pipewire.service.wants/tb323fu-speaker-gain.service"

# 7. Feedback & Haptics (AW86927 dual linear resonant actuators)
say "Setting up haptics and feedbackd"
mkdir -p "$T/etc/feedbackd/themes"
cat <<-EOF > "$T/etc/feedbackd/themes/lenovo,baldur.json"
	{
	  "name" : "lenovo,baldur",
	  "parent-name": "default",
	  "profiles" : [
	    {
	      "name" : "full",
	      "feedbacks" : [
	        { "event-name" : "button-pressed", "type" : "VibraPattern", "magnitudes" : [ 1.0 ], "durations" : [ 50 ] },
	        { "event-name" : "button-released", "type" : "VibraRumble", "duration" : 25 },
	        { "event-name" : "key-pressed", "type" : "VibraPattern", "magnitudes" : [ 1.0 ], "durations" : [ 50 ] },
	        { "event-name" : "key-released", "type" : "VibraRumble", "duration" : 25 },
	        { "event-name" : "message-new-instant", "type" : "VibraRumble", "duration" : 250 },
	        { "event-name" : "message-new-sms", "type" : "VibraRumble", "duration" : 450, "count" : 2, "pause" : 100 },
	        { "event-name" : "notification-new-generic", "type" : "VibraPattern", "magnitudes" : [ 1.0, 0.0, 1.0 ], "durations" : [ 100, 100, 100 ] },
	        { "event-name" : "phone-incoming-call", "type" : "VibraRumble", "duration" : 1000, "count" : 3, "pause" : 500 }
	      ]
	    },
	    {
	      "name" : "quiet",
	      "feedbacks" : [
	        { "event-name" : "button-pressed", "type" : "VibraPattern", "magnitudes" : [ 1.0 ], "durations" : [ 50 ] },
	        { "event-name" : "button-released", "type" : "VibraRumble", "duration" : 25 },
	        { "event-name" : "key-pressed", "type" : "VibraPattern", "magnitudes" : [ 1.0 ], "durations" : [ 50 ] },
	        { "event-name" : "key-released", "type" : "VibraRumble", "duration" : 25 },
	        { "event-name" : "message-new-instant", "type" : "VibraRumble", "duration" : 250 },
	        { "event-name" : "message-new-sms", "type" : "VibraRumble", "duration" : 450, "count" : 2, "pause" : 100 },
	        { "event-name" : "notification-new-generic", "type" : "VibraPattern", "magnitudes" : [ 1.0, 0.0, 1.0 ], "durations" : [ 100, 100, 100 ] },
	        { "event-name" : "phone-incoming-call", "type" : "VibraRumble", "duration" : 1000, "count" : 3, "pause" : 500 }
	      ]
	    }
	  ]
	}
EOF

mkdir -p "$T/etc/modules-load.d"
echo "aw86927" >> "$T/etc/modules-load.d/tb323fu.conf"

# 8. Polkit Desktop Rules (NetworkManager, Flatpak, UDisks2)
say "Installing polkit rules"
mkdir -p "$T/etc/polkit-1/rules.d"
cat <<-EOF > "$T/etc/polkit-1/rules.d/50-desktop.rules"
	/* Allow active local users to manage NetworkManager, Flatpak, UDisks2 and PowerProfiles without password */
	polkit.addRule(function(action, subject) {
	    if (action.id.indexOf("org.freedesktop.NetworkManager.") === 0 &&
	        subject.local && subject.active) {
	        return polkit.Result.YES;
	    }
	    if (action.id.indexOf("org.freedesktop.Flatpak.") === 0 &&
	        subject.local && subject.active) {
	        return polkit.Result.YES;
	    }
	    if (action.id.indexOf("org.freedesktop.UDisks2.filesystem-mount") === 0 &&
	        subject.local && subject.active) {
	        return polkit.Result.YES;
	    }
	    if (action.id.indexOf("net.hadess.PowerProfiles.") === 0 &&
	        subject.local && subject.active) {
	        return polkit.Result.YES;
	    }
	});
EOF

# 9. GSettings overrides (auto-rotation, onscreen keyboard)
say "Installing GSettings overrides"
cat <<-EOF > "$T/usr/share/glib-2.0/schemas/99-tb323fu.gschema.override"
	[org.gnome.settings-daemon.peripherals.touchscreen]
	orientation-lock=false

	[org.gnome.settings-daemon.peripherals.touchscreen:Phosh]
	orientation-lock=false

	[org.gnome.desktop.a11y.applications]
	screen-keyboard-enabled=true

	[org.gnome.desktop.a11y.applications:Phosh]
	screen-keyboard-enabled=true
EOF
ch "glib-compile-schemas /usr/share/glib-2.0/schemas 2>/dev/null || true"

# 10. Phosh launcher & desktop entries compatibility
say "Configuring Phosh and GNOME settings wrappers"
if [ -f "$T/usr/bin/gnome-control-center" ] && [ ! -f "$T/usr/bin/gnome-control-center.real" ]; then
	mv "$T/usr/bin/gnome-control-center" "$T/usr/bin/gnome-control-center.real"
	cat <<-EOF > "$T/usr/bin/gnome-control-center"
		#!/bin/sh
		case ":\$XDG_CURRENT_DESKTOP:" in
		  *:GNOME:*) ;;
		  *) export XDG_CURRENT_DESKTOP="\${XDG_CURRENT_DESKTOP:-Phosh}:GNOME" ;;
		esac
		exec /usr/bin/gnome-control-center.real "\$@"
	EOF
	chmod +x "$T/usr/bin/gnome-control-center"
fi

if [ -f "$T/usr/share/applications/org.gnome.Settings.desktop" ]; then
	sed -i 's/^OnlyShowIn=GNOME;$/OnlyShowIn=GNOME;Phosh;/' "$T/usr/share/applications/org.gnome.Settings.desktop"
fi

if [ -f "$T/usr/share/applications/sm.puri.OSK0.desktop" ]; then
	sed -i 's/^OnlyShowIn=.*$/OnlyShowIn=GNOME;Phosh;Phrog;/' "$T/usr/share/applications/sm.puri.OSK0.desktop"
fi

if [ -f "$T/usr/share/applications/mobi.phosh.MobileSettings.nobuiltins.desktop" ]; then
	echo "NoDisplay=true" >> "$T/usr/share/applications/mobi.phosh.MobileSettings.nobuiltins.desktop"
fi

# Export desktop env in phosh-session
if [ -f "$T/usr/bin/phosh-session" ]; then
	sed -i '/exec \$SYSTEMD_CAT/i export XDG_CURRENT_DESKTOP="Phosh:GNOME"\nexport XDG_SESSION_DESKTOP="phosh"\n' "$T/usr/bin/phosh-session"
fi

# Enable Squeekboard user service
mkdir -p "$T/etc/systemd/user/gnome-session-initialized.target.wants"
ln -sf /usr/lib/systemd/user/mobi.phosh.OSK.service "$T/etc/systemd/user/gnome-session-initialized.target.wants/mobi.phosh.OSK.service"

# Display scale for Phosh (1904x3040 panel)
mkdir -p "$T/etc/phosh"
cat <<-EOF > "$T/etc/phosh/phoc.ini"
	[output:DSI-1]
	scale = 2
EOF

# 11. Flathub & AppStream store setup
say "Setting up Flathub remote and AppStream cache"
ch "flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo 2>/dev/null || true"
ch "alpine-appstream-downloader >/dev/null 2>&1 || true"

# 12. Build and install tb323fu-helperd, tb323fu-ctl and keyhold natively
if [ ! -x "$T/usr/bin/tb323fu-ctl" ] || [ ! -x "$T/usr/bin/tb323fu-settings" ]; then
	say "Building helper daemon, CLI and native binaries"
	S="/var/tmp/tb323fu-src"
	rm -rf "$T$S"
	mkdir -p "$T$S"
	cp -a "$here/helper" "$T$S/"
	cp -a "$here/userspace/platform" "$T$S/"

	ch "apk add --no-cache rust cargo build-base pkgconf gtk4.0-dev libadwaita-dev"

	# Compile keyhold natively with musl gcc
	ch "gcc -O2 -Wall -Wextra -o /usr/libexec/tb323fu/keyhold $S/platform/src/keyhold.c"

	# Build helper daemon and CLI
	ch "env CARGO_HOME=/var/tmp/tb323fu-cargo sh -c 'cd $S/helper && cargo build --release --locked -q'"
	ch "env PREFIX=/usr LIBEXECDIR=/usr/libexec/tb323fu sh $S/helper/install.sh >/dev/null"

	# Build settings GUI
	ch "env CARGO_HOME=/var/tmp/tb323fu-cargo sh -c 'cd $S/helper/crates/tb323fu-settings && cargo build --release --locked -q'"
	ch "env PREFIX=/usr sh -c 'cd $S/helper/crates/tb323fu-settings && sh ./install.sh' >/dev/null"

	ch "apk del rust cargo build-base pkgconf gtk4.0-dev libadwaita-dev"
	rm -rf "$T$S" "$T/var/tmp/tb323fu-cargo"
fi

# Enable systemd services
ch "systemctl enable tb323fu-helperd.service tb323fu-emergency-key.service tb323fu-zram.service >/dev/null 2>&1 || true"
ch "systemctl enable tb323fu-gen-ids.service tb323fu-btaddr.service tb323fu-dsp.service tb323fu-audio.service tb323fu-usb-port.service tb323fu-kernel-confirm.service >/dev/null 2>&1 || true"
ch "systemctl enable qrtr-ns.service hexagonrpcd.service iio-sensor-proxy.service >/dev/null 2>&1 || true"

# 13. System configuration
say "Configuring system files"
echo "$HOSTNAME_NEW" > "$T/etc/hostname"
printf '127.0.0.1\tlocalhost\n::1\tlocalhost\n127.0.1.1\t%s\n' "$HOSTNAME_NEW" > "$T/etc/hosts"
printf 'PARTLABEL=%s\t/\text4\tdefaults,noatime\t0\t1\n' "$ROOT_PARTLABEL" > "$T/etc/fstab"

mkdir -p "$T/etc/tb323fu"
for f in bt-address android-boot.sha256 audio.conf emergency-key.conf; do
	[ -e "$CONFIG_FROM/$f" ] && cp -a "$CONFIG_FROM/$f" "$T/etc/tb323fu/"
done

# Timezone
if [ -e "$T/usr/share/zoneinfo/$TIMEZONE" ]; then
	ln -sf "/usr/share/zoneinfo/$TIMEZONE" "$T/etc/localtime"
fi

# 14. Users and Greetd / Phosh autologin
say "Setting up user $DEV_USER"
ch "id $DEV_USER >/dev/null 2>&1 || useradd -m -s /bin/bash -G wheel,video,audio,input,feedbackd,fastrpc,netdev,plugdev $DEV_USER"
echo "%wheel ALL=(ALL:ALL) ALL" > "$T/etc/sudoers.d/10-wheel"
chmod 440 "$T/etc/sudoers.d/10-wheel"

if [ -n "$BALDUR_DEV_PASSWORD" ]; then
	echo "root:$BALDUR_DEV_PASSWORD" | ch chpasswd
	echo "$DEV_USER:$BALDUR_DEV_PASSWORD" | ch chpasswd
fi

# Configure greetd autologin to phosh
mkdir -p "$T/etc/phrog" "$T/etc/greetd"
cat <<-EOF > "$T/etc/greetd/config.toml"
	[terminal]
	vt = 1

	[default_session]
	command = "/usr/libexec/phrog-greetd-session"
	user = "greetd"

	[initial_session]
	command = "systemd-cat phosh-session"
	user = "$DEV_USER"
EOF
cp -a "$T/etc/greetd/config.toml" "$T/etc/phrog/greetd-config.toml"
ch "id greetd >/dev/null 2>&1 && adduser greetd input 2>/dev/null || true"
ch "id greetd >/dev/null 2>&1 && adduser greetd video 2>/dev/null || true"

ch "systemctl enable greetd.service NetworkManager.service bluetooth.service >/dev/null 2>&1 || true"

# Clean up resolv.conf to point to systemd-resolved
rm -f "$T/etc/resolv.conf"
ln -sf /run/systemd/resolve/stub-resolv.conf "$T/etc/resolv.conf"

say "postmarketOS rootfs built successfully in $T"
