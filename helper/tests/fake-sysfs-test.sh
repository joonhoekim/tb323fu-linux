#!/bin/sh
# SPDX-License-Identifier: MIT
# fake-sysfs-test.sh -- run tb323fu-helperd against a fake device tree on a
# private D-Bus session (no polkit) and drive it with tb323fu-ctl.
#   dbus-run-session -- sh tests/fake-sysfs-test.sh [BINDIR]    # default target/release
set -u
BIN=${1:-target/release}
R=$(mktemp -d /tmp/tb323fu-fake.XXXXXX)
export TB323FU_SYSFS_ROOT=$R TB323FU_CONFIG=$R/etc/tb323fu/helper.toml
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
check() { # desc file expected
	got=$(cat "$R$2" 2>/dev/null)
	[ "$got" = "$3" ] && ok "$1 ($2 = $3)" || bad "$1 ($2 = '$got', want '$3')"
}
mk() { mkdir -p "$(dirname "$R$1")"; printf '%s\n' "$2" > "$R$1"; }

# battery + charger
B=/sys/class/power_supply/qcom-battmgr-bat
mk $B/type Battery; mk $B/status Charging; mk $B/capacity 62; mk $B/current_now 1500000
mk $B/voltage_now 4100000; mk $B/temp 305; mk $B/health Good; mk $B/cycle_count 5
mk $B/charge_control_end_threshold 100; mk $B/charge_control_start_threshold 95
U=/sys/class/power_supply/ucsi-source-psy-pmic_glink.ucsi.01
mk $U/type USB; mk $U/online 1; mk $U/usb_type "C [PD] PD_PPS"; mk $U/voltage_now 9000000; mk $U/current_max 3000000
# LEDs
mk /sys/class/leds/white:flash/brightness 0; mk /sys/class/leds/white:flash/max_brightness 255
L=/sys/class/leds/aw22127:rgb:indicator
mk $L/brightness 0; mk $L/multi_intensity "0 0 0"; mk $L/max_brightness 255
# idle refresh knobs
P=/sys/module/msm/parameters
for k in "policy 2" "hz 60" "ms60 1000" "ms30 5000" "min_hz 30" "input 1" "selfflush 3"; do mk $P/idle_refresh_${k% *} ${k#* }; done
mk $P/idle_refresh_state "policy 2 enabled 1 suspending 0 base 3400 live 13600 hz 30 idle_ms 9000"
# GPU
G=/sys/class/devfreq/3d00000.gpu
mk $G/min_freq 160000000; mk $G/max_freq 1200000000
mk $G/available_frequencies "160000000 461000000 726000000 1200000000"
# USB
mk /sys/bus/platform/devices/a600000.usb/power/wakeup disabled
mk /sys/kernel/config/usb_gadget/g1/UDC a600000.usb; mkdir -p $R/sys/class/udc/a600000.usb
# Android
mk /etc/android-boot.sha256 "$(printf '%064d' 7)"
mkdir -p $R/usr/local/sbin && printf '#!/bin/sh\necho "fake back-to-android $1" > %s/android-called\n' "$R" > $R/usr/local/sbin/back-to-android && chmod +x $R/usr/local/sbin/back-to-android
# legacy config to migrate
mk /etc/baldur/ledring.conf "BRIGHTNESS=33
LOW=12"
mk /proc/uptime "100.0 100.0"; mk /proc/version "Linux version 7.3.0-test"

$BIN/tb323fu-helperd --session --no-polkit > $R/daemon.log 2>&1 &
D=$!
trap 'kill $D 2>/dev/null; rm -rf "$R"' EXIT
for i in 1 2 3 4 5 6 7 8 9 10; do $BIN/tb323fu-ctl --session versions >/dev/null 2>&1 && break; sleep 0.3; done
C="$BIN/tb323fu-ctl --session"

# start-up: charge limit applied from defaults (80), legacy LED values migrated
check "start applies default charge limit" $B/charge_control_end_threshold 80
check "start threshold end-10" $B/charge_control_start_threshold 70
grep -q "brightness = 33" $R/etc/tb323fu/helper.toml && ok "legacy ledring.conf migrated" || bad "legacy ledring.conf not migrated"
$C --json status | grep -q '"ChargerContract": "PD 9.0 V 3.00 A"' && ok "charger contract" || bad "charger contract"
$C --json battery | grep -q '"State": "charging"' && ok "state charging" || bad "state"

$C charge-limit 60 && check "set charge limit 60" $B/charge_control_end_threshold 60
check "start lowered first" $B/charge_control_start_threshold 50
$C charge-limit 10 2>/dev/null && bad "charge limit 10 accepted" || ok "charge limit 10 refused"
$C bypass on && check "bypass holds at capacity" $B/charge_control_end_threshold 62
$C --json battery | grep -q '"State": "charging"' && ok "bypass on, 1.5 A still flowing: charging" || bad "bypass transition state"
mk $B/current_now 185000	# firmware still says Charging (t26)
$C --json battery | grep -q '"State": "bypass"' && ok "bypass on, 185 mA: bypass" || bad "bypass state"
mk $B/current_now 1500000
$C bypass off && check "bypass off restores" $B/charge_control_end_threshold 60
# PD/PPS charger: UCSI reports no contract, battmgr measures the input
mk $U/voltage_now 0; mk $U/current_max 0
UB=/sys/class/power_supply/qcom-battmgr-usb
mk $UB/type USB; mk $UB/online 1; mk $UB/usb_type "Unknown SDP DCP CDP ACA C PD PD_DRP [PD_PPS] BrickID"
mk $UB/voltage_now 9192000; mk $UB/current_now 4310000
$C --json battery | grep -q '"ChargerContract": "PPS · 9.2 V in · ~40 W"' && ok "measured contract" || bad "measured contract"
$C --json battery | grep -q '"ChargerAdapter": "PD_PPS"' && ok "adapter PPS" || bad "adapter"
$C --json battery | grep -q '"InputVoltageMv": 9192' && ok "input voltage" || bad "input voltage"
mk $U/voltage_now 9000000; mk $U/current_max 3000000

$C torch on && check "torch on at default level" /sys/class/leds/white:flash/brightness 96
$C torch level 40 && check "torch level while on" /sys/class/leds/white:flash/brightness 40
$C torch off && check "torch off" /sys/class/leds/white:flash/brightness 0

sleep 0.5
check "LED ring amber while charging" $L/multi_intensity "255 90 0"
check "LED ring brightness (migrated)" $L/brightness 33
$C ledring off && check "LED ring off" $L/brightness 0

$C refresh manual 60 && check "manual policy" $P/idle_refresh_policy 1
check "manual rate" $P/idle_refresh_hz 60
$C refresh preset smooth && check "preset ms60" $P/idle_refresh_ms60 3000
check "preset ms30" $P/idle_refresh_ms30 15000
$C refresh auto && check "auto policy" $P/idle_refresh_policy 2
$C --json refresh | grep -q '"LiveRate": 30' && ok "live rate from state" || bad "live rate"

$C gpu profile power-saver && check "gpu cap power-saver" $G/max_freq 726000000
check "gpu floor power-saver" $G/min_freq 160000000
$C gpu limits power-saver 461 726 && check "gpu limits applied" $G/min_freq 461000000
$C gpu limits power-saver 900 100 2>/dev/null && bad "min>max accepted" || ok "min>max refused"

$C usb wake on && check "usb wake" /sys/bus/platform/devices/a600000.usb/power/wakeup enabled
$C usb dev off && check "dev mode off unbinds" /sys/kernel/config/usb_gadget/g1/UDC ""
$C usb dev on && check "dev mode on binds" /sys/kernel/config/usb_gadget/g1/UDC a600000.usb

$C emergency-key hold 15 && grep -q HOLD_SECONDS=15 $R/etc/tb323fu/emergency-key.conf && ok "emergency hold" || bad "emergency hold"
$C emergency-key off && grep -q ENABLED=0 $R/etc/tb323fu/emergency-key.conf && ok "emergency off" || bad "emergency off"

$C android --yes && sleep 0.5 && grep -q "fake back-to-android $(printf '%064d' 7)" $R/android-called && ok "android switch runs back-to-android with the hash" || bad "android switch"

grep -q "charge_limit = 60" $R/etc/tb323fu/helper.toml && ok "config persisted" || bad "config not persisted"
kill $D; wait $D 2>/dev/null
# restart: persisted settings re-applied
echo 100 > $R$B/charge_control_end_threshold
$BIN/tb323fu-helperd --session --no-polkit >> $R/daemon.log 2>&1 &
D=$!
for i in 1 2 3 4 5 6 7 8 9 10; do $BIN/tb323fu-ctl --session versions >/dev/null 2>&1 && break; sleep 0.3; done
check "restart re-applies charge limit" $B/charge_control_end_threshold 60
check "restart re-applies usb wake" /sys/bus/platform/devices/a600000.usb/power/wakeup enabled

[ $fail = 0 ] && echo "ALL PASSED" || { echo "SOME FAILED"; cat $R/daemon.log; }
exit $fail
