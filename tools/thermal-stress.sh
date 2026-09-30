#!/bin/sh
# SPDX-License-Identifier: MIT
# thermal-stress.sh — on the tablet (root): load all 8 CPUs for SECONDS and log
# once a second what the thermal framework does about it: both clusters'
# current frequency, the hottest
# CPU zone, the board NTC quiet-thermal (Android's skin sensor), each cooling
# device's state, battery current. Aborts when the hottest CPU zone passes
# LIMIT_C or quiet-thermal passes SKIN_LIMIT_C; the load is always killed.
#   thermal-stress.sh [SECONDS]        # default 600
#   thermal-stress.sh emul             # no load: step quiet-thermal's emul_temp
#                                      # 40..50..40 C and show the pinned states
#   LIMIT_C=115 SKIN_LIMIT_C=52 OUT=/tmp/thermal-stress.log thermal-stress.sh 900
# LIMIT_C defaults to 115: the first second of full load spikes a CPU zone past
# 100 C (103-107 C measured) before LMh/the 95 C passive trip catch it;
# the zones' own hot/critical trips are 120/125 C.
# USB may stay plugged in (battery current then shows the balance, not the load).
# Pass (skin build): quiet-thermal settles near 43-47 C with cdev states at the
# pinned values, the CPU zones stay under 95 C, nothing aborts.
S=${1:-600}; LIMIT_C=${LIMIT_C:-115}; SKIN_LIMIT_C=${SKIN_LIMIT_C:-52}
OUT=${OUT:-/tmp/thermal-stress-$(date +%m%d-%H%M).log}
T=/sys/class/thermal
zone() { for z in $T/thermal_zone*; do [ "$(cat $z/type)" = "$1" ] && { cat $z/temp; return; }; done; echo 0; }
cdev() { for c in $T/cooling_device*; do [ "$(cat $c/type)" = "$1" ] && { cat $c/cur_state; return; }; done; echo -; }
hottest() { h=0; for z in $T/thermal_zone*; do case $(cat $z/type) in cpu-*|cpullc-*) t=$(cat $z/temp); [ "$t" -gt "$h" ] && h=$t;; esac; done; echo $h; }

if [ "$1" = emul ]; then
	# no load: fake quiet-thermal through emul_temp (CONFIG_THERMAL_EMULATION)
	# and read what each cooling device is pinned to; always cleared
	for z in $T/thermal_zone*; do [ "$(cat $z/type)" = quiet-thermal ] && Q=$z; done
	trap 'echo 0 > $Q/emul_temp' INT TERM EXIT
	echo "emul C  f0_kHz f6_kHz  cdev_cpu0 cdev_cpu6 cdev_gpu  gpu_max_Hz"
	for c in 40 43 44 45 46 47 48 49 50 46 40; do
		echo ${c}000 > $Q/emul_temp; sleep 4
		echo "$c  $(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq) $(cat /sys/devices/system/cpu/cpufreq/policy6/scaling_max_freq)  $(cdev cpufreq-cpu0) $(cdev cpufreq-cpu6) $(cdev devfreq-3d00000.gpu)  $(cat /sys/class/devfreq/3d00000.gpu/max_freq)"
	done
	exit
fi

pids=""
stop() { [ -n "$pids" ] && kill $pids 2>/dev/null; wait 2>/dev/null; pids=""; }
trap 'stop; echo "load stopped" | tee -a "$OUT"; exit' INT TERM EXIT

if command -v stress-ng >/dev/null; then
	stress-ng --cpu 8 --timeout ${S}s >/dev/null 2>&1 & pids=$!
else
	i=0; while [ $i -lt 8 ]; do (while :; do :; done) & pids="$pids $!"; i=$((i + 1)); done
fi
echo "thermal-stress ${S}s limit ${LIMIT_C} C skin ${SKIN_LIMIT_C} C, $(uname -v | cut -d' ' -f1), load: $(command -v stress-ng >/dev/null && echo stress-ng || echo busy loops)" | tee "$OUT"
echo "t  f0_kHz f6_kHz  cpu_max_mC quiet_mC  cdev_cpu0 cdev_cpu6 cdev_gpu  bat_uA" | tee -a "$OUT"
t=0
while [ $t -lt $S ]; do
	f0=$(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq)
	f6=$(cat /sys/devices/system/cpu/cpufreq/policy6/scaling_cur_freq)
	h=$(hottest); q=$(zone quiet-thermal)
	b=$(cat /sys/class/power_supply/*-bat/current_now 2>/dev/null | head -1)
	echo "$t $f0 $f6  $h $q  $(cdev cpufreq-cpu0) $(cdev cpufreq-cpu6) $(cdev devfreq-3d00000.gpu)  $b" | tee -a "$OUT"
	if [ "$h" -gt $((LIMIT_C * 1000)) ]; then echo "ABORT: CPU zone $h mC > $LIMIT_C C" | tee -a "$OUT"; break; fi
	if [ "$q" -gt $((SKIN_LIMIT_C * 1000)) ]; then echo "ABORT: quiet-thermal $q mC > $SKIN_LIMIT_C C" | tee -a "$OUT"; break; fi
	sleep 1; t=$((t + 1))
done
stop
# cool-down: 60 s more without load, to see the caps released
i=0; while [ $i -lt 60 ]; do
	[ $((i % 5)) -eq 0 ] && echo "cool+$i $(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq) $(cat /sys/devices/system/cpu/cpufreq/policy6/scaling_cur_freq)  $(hottest) $(zone quiet-thermal)  $(cdev cpufreq-cpu0) $(cdev cpufreq-cpu6) $(cdev devfreq-3d00000.gpu)" | tee -a "$OUT"
	sleep 1; i=$((i + 1))
done
echo "log: $OUT"
