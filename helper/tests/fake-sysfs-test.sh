#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Joonhoe Kim
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
# thermal: quiet-thermal (hot 80 + passive 43..50), a chip zone, battery, panel; backlight
T=/sys/class/thermal
Q=$T/thermal_zone65
mk $Q/type quiet-thermal; mk $Q/temp 30000; mk $Q/trip_point_0_type hot; mk $Q/trip_point_0_temp 80000
for i in 1 2 3 4 5 6 7 8; do mk $Q/trip_point_${i}_type passive; mk $Q/trip_point_${i}_temp $((42000 + i * 1000)); done
mk $T/thermal_zone1/type cpu-0-0-0-thermal; mk $T/thermal_zone1/temp 35000
mk $T/thermal_zone1/trip_point_0_type passive; mk $T/thermal_zone1/trip_point_0_temp 95000
mk $T/thermal_zone66/type batt-thermal; mk $T/thermal_zone66/temp 30000
mk $T/thermal_zone58/type lcm-thermal; mk $T/thermal_zone58/temp 31000
BL=/sys/class/backlight/aw99706-backlight
mk $BL/brightness 4000; mk $BL/max_brightness 4095; mk $BL/actual_brightness 4000; mk $BL/bl_power 0
mk /proc/sys/kernel/random/boot_id fake-boot
# CPU clusters
C0=/sys/devices/system/cpu/cpufreq/policy0; C6=/sys/devices/system/cpu/cpufreq/policy6
mk $C0/scaling_available_frequencies "384000 1996800 2496000 3628800"; mk $C0/scaling_min_freq 384000
mk $C0/scaling_max_freq 3628800; mk $C0/cpuinfo_max_freq 3628800
mk $C6/scaling_available_frequencies "768000 2880000 4396800"; mk $C6/scaling_min_freq 768000
mk $C6/scaling_max_freq 4608000; mk $C6/cpuinfo_max_freq 4608000
mk /sys/devices/system/cpu/cpufreq/boost 1
# a Wi-Fi interface and a stand-in for iw (power save kept in a file)
mkdir -p $R/sys/class/net/wlan0/wireless; echo on > $R/iw-ps
mkdir -p $R/usr/sbin
cat > $R/usr/sbin/iw <<EOF
#!/bin/sh
case "\$3" in get) echo "Power save: \$(cat $R/iw-ps)";; set) echo "\$5" > $R/iw-ps;; esac
EOF
chmod +x $R/usr/sbin/iw
# wakeup sources of the power supplies; a USB-C port with a partner
mk $B/power/wakeup enabled; mk $U/power/wakeup enabled
mk /sys/class/typec/port0/data_role "host [device]"; mk /sys/class/typec/port0/power_role "source [sink]"
mkdir -p $R/sys/class/typec/port0-partner
# vibration motors (event nodes are plain files here: each FF_GAIN write appends an input_event)
for n in 5 6; do mk /sys/class/input/event$n/device/name aw86927-haptics; mkdir -p $R/dev/input; : > $R/dev/input/event$n; done
# wait until a file has the value (the 5 s poller)
waitfor() { for i in $(seq 1 30); do [ "$(cat "$R$1" 2>/dev/null)" = "$2" ] && return 0; sleep 0.3; done; return 1; }

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

# recharge gap, battery health, "full by"
$C battery recharge-gap 5 && check "recharge gap 5: start 55" $B/charge_control_start_threshold 55
$C battery recharge-gap 30 2>/dev/null && bad "recharge gap 30 accepted" || ok "recharge gap 30 refused"
$C battery recharge-gap 10 >/dev/null
mk $B/state_of_health 97
$C --json battery | grep -q '"StateOfHealth": 97' && ok "state of health" || bad "state of health"
mk /run/tb323fu-fake-clock "mon 06:30"
$C battery full-by 07:00 mon,tue && waitfor $B/charge_control_end_threshold 100 && ok "full by 07:00 at 06:30: limit 100" || bad "full by did not raise the limit"
$C --json battery | grep -q '"ChargeLimit": 60' && ok "ChargeLimit still shows the configured 60" || bad "ChargeLimit during full by"
$C --json battery | grep -q '"FullByActive": true' && ok "FullByActive" || bad "FullByActive"
mk /run/tb323fu-fake-clock "mon 09:30"
waitfor $B/charge_control_end_threshold 60 && ok "two hours after the target: limit back to 60" || bad "full by did not end"
mk /run/tb323fu-fake-clock "wed 06:30"
sleep 6; check "not on Wednesday" $B/charge_control_end_threshold 60
$C battery full-by 7:75 2>/dev/null && bad "bad time accepted" || ok "bad time refused"
$C battery full-by off && grep -q 'full_by = ""' $R/etc/tb323fu/helper.toml && ok "full by off persisted" || bad "full by off"

$C torch on && check "torch on at default level" /sys/class/leds/white:flash/brightness 96
$C torch level 40 && check "torch level while on" /sys/class/leds/white:flash/brightness 40
$C torch off && check "torch off" /sys/class/leds/white:flash/brightness 0

sleep 0.5
check "LED ring amber while charging" $L/multi_intensity "255 26 0"
check "LED ring brightness (migrated)" $L/brightness 33
$C ledring color '#102030' && $C ledring solid && sleep 0.3 && check "solid while charging: charge colour takes over" $L/multi_intensity "255 26 0"
$C ledring charge-override off && sleep 0.3 && check "solid: own colour" $L/multi_intensity "1 3 6"
check "solid: full brightness" $L/brightness 33
$C ledring color red 2>/dev/null && bad "colour 'red' accepted" || ok "colour 'red' refused"
$C ledring speed 1000 && $C ledring breathe && sleep 0.2
bs=""; for i in 1 2 3 4; do bs="$bs $(cat $R$L/brightness)"; sleep 0.13; done
[ $(echo $bs | tr " " "
" | sort -u | wc -l) -gt 1 ] && ok "breathing changes the brightness ($bs)" || bad "breathing ($bs)"
mk $BL/bl_power 4; sleep 0.5; check "screen off: breathing stops at full brightness" $L/brightness 33
mk $BL/bl_power 0
$C ledring pulse '#00ff00' 1 && sleep 0.1 && check "pulse shows its colour" $L/multi_intensity "0 255 0"
sleep 0.6; check "after the pulse: back to the ring colour" $L/multi_intensity "1 3 6"
# a kernel with hardware patterns: breathing runs on the chip
mk $L/trigger "[none] pattern"; mk $L/hw_pattern ""; mk $L/repeat 0
$C ledring speed 2000 && sleep 0.3
check "hw breathing: pattern trigger" $L/trigger pattern
check "hw breathing: eight-point cycle" $L/hw_pattern "3 250 7 250 18 250 29 250 33 250 29 250 18 250 7 250"
check "hw breathing: colour" $L/multi_intensity "1 3 6"
check "hw breathing: repeat forever before the pattern" $L/repeat "-1"
$C ledring solid && sleep 0.3 && check "solid: pattern trigger cleared" $L/trigger none
check "solid after hw breathing: brightness" $L/brightness 33
rm -f $R$L/trigger $R$L/hw_pattern $R$L/repeat
$C ledring charge-override on >/dev/null
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

# CPU limits follow the performance profile (power-saver now)
check "CPU full range by default" $C0/scaling_max_freq 3628800
$C gpu cpu-limits power-saver 384 2500 768 2880 && check "little cap rounded to 2.496 GHz" $C0/scaling_max_freq 2496000
check "big cap" $C6/scaling_max_freq 2880000
$C gpu cpu-limits power-saver 3628 3628 768 4396 2>/dev/null && bad "little floor 3.6 GHz accepted" || ok "little floor above 1996 MHz refused"
$C gpu profile balanced && check "balanced: whole range, boost kept" $C6/scaling_max_freq 4608000
check "balanced: little whole range" $C0/scaling_max_freq 3628800
grep -q "power-saver = \[" $R/etc/tb323fu/helper.toml && ok "CPU limits persisted" || bad "CPU limits not persisted"

# thermal profile: only quiet-thermal passive trips move, never above 58 °C
check "default profile writes nothing" $Q/trip_point_8_temp 50000
$C thermal profile performance && check "performance: top step 58 °C" $Q/trip_point_8_temp 58000
check "performance: first step 51 °C" $Q/trip_point_1_temp 51000
check "hot trip untouched" $Q/trip_point_0_temp 80000
check "chip zone untouched" $T/thermal_zone1/trip_point_0_temp 95000
check "performance on a charger switches Bypass on" $B/charge_control_end_threshold 62
$C thermal profile quiet && check "quiet: first step 40 °C" $Q/trip_point_1_temp 40000
check "leaving performance restores the limit" $B/charge_control_end_threshold 60
$C thermal profile hot 2>/dev/null && bad "unknown thermal profile accepted" || ok "unknown thermal profile refused"
$C thermal profile performance >/dev/null
mk $T/thermal_zone66/temp 45500	# battery at 45.5 °C: performance must end
waitfor $Q/trip_point_8_temp 50000 && ok "hot battery ends performance (back to default)" || bad "hot battery did not end performance"
grep -q 'profile = "default"' $R/etc/tb323fu/helper.toml && ok "fallback persisted" || bad "fallback not persisted"
$C thermal profile performance 2>/dev/null && bad "performance accepted with a hot battery" || ok "performance refused while the battery is hot"
mk $T/thermal_zone66/temp 30000
# linked to the performance profile
$C thermal follow on && $C gpu profile power-saver && check "follow: power-saver -> quiet" $Q/trip_point_1_temp 40000
$C gpu profile balanced && check "follow: balanced -> default" $Q/trip_point_1_temp 43000
$C thermal follow off

# panel heat limit: 55 °C holds the backlight at 178/255, 52 °C gives it back
mk $T/thermal_zone58/temp 56000
waitfor $BL/brightness 2858 && ok "hot panel: backlight held at 2858" || bad "hot panel: backlight $(cat $R$BL/brightness)"
$C --json thermal | grep -q '"PanelLimited": true' && ok "PanelLimited" || bad "PanelLimited"
mk $T/thermal_zone58/temp 50000
waitfor $BL/brightness 4000 && ok "cool panel: brightness restored" || bad "cool panel: backlight $(cat $R$BL/brightness)"

# Wi-Fi power saving off in the performance profile, back on after
$C gpu wifi-low-latency on && $C gpu profile performance && check "performance: Wi-Fi power save off" /iw-ps off
$C gpu profile balanced && check "balanced: Wi-Fi power save back on" /iw-ps on
$C gpu wifi-low-latency off

# vibration strength: FF_GAIN events (type 0x15, code 0x60) on both motors
gain() { od -An -tx1 -v $R/dev/input/event$1 | tr -d ' \n' | tail -c 16; }
[ -s $R/dev/input/event5 ] && ok "start writes the default strength" || bad "no gain written at start"
$C haptics strength 50 && [ "$(gain 5)" = "15006000ff7f0000" ] && [ "$(gain 6)" = "15006000ff7f0000" ] && ok "strength 50 -> gain 0x7fff on both" || bad "strength 50: $(gain 5) $(gain 6)"
$C haptics strength 150 2>/dev/null && bad "strength 150 accepted" || ok "strength 150 refused"
$C --json haptics | grep -q '"left"' && ok "motors listed" || bad "motors"

$C usb wake on && check "usb wake" /sys/bus/platform/devices/a600000.usb/power/wakeup enabled
$C usb charger-wake off && check "charger wake off (battery)" $B/power/wakeup disabled
check "charger wake off (UCSI source)" $U/power/wakeup disabled
$C usb charger-wake on && check "charger wake on" $U/power/wakeup enabled
$C --json usb | grep -q '"port0"' && ok "USB-C ports listed" || bad "USB-C ports"
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
