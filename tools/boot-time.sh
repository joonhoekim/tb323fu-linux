#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# boot-time.sh -- PC: boot-time measurement of one boot image over N boots
#   TB323FU_HOST=192.168.7.2 tools/boot-time.sh IMG [N=3] --restore BASE.img
#   tools/boot-time.sh --no-flash - [N]      # measure the running kernel
# Logs go to $LOGDIR (default ./logs).
# Flashes IMG with flash-boot.sh (that reboot is boot 1), then reboots
# N-1 more times with `systemctl reboot` (not reflashing). Per boot:
#   wall     s from the reboot command to ssh answering (PC clock)
#   kernel   systemd-analyze "kernel" -- includes our initramfs /init, since
#            systemd only starts after switch_root
#   user     systemd-analyze "userspace" (to graphical.target)
#   init     kernel timestamp of "Run /init as init process" (kernel-only part)
#   sroot    kernel timestamp of systemd's first line (initramfs /init done)
#   gdm      monotonic s of the first gnome-shell journal line (greeter up)
#   con      kernel log lines at or below the console loglevel (what the panel
#            printed; the init summary is KERN_ALERT, so counted as well)
# plus `systemd-analyze blame | head -5` once per boot into the log file.
# Always ends by flashing --restore BASE.img (required when IMG is flashed).
# Every wait is bounded (240 s); a timeout stops the run.
set -u
here=$(cd "$(dirname "$0")" && pwd)
H=${TB323FU_HOST:-192.168.7.2}
restore=
flash=1
args=()
while [ $# -gt 0 ]; do
	case "$1" in
	--restore) restore=$2; shift 2 ;;
	--no-flash) flash=0; shift ;;
	*) args+=("$1"); shift ;;
	esac
done
img=${args[0]:--}
n=${args[1]:-3}
log="${LOGDIR:-./logs}/$(date +%Y-%m-%d)-boot-time-$(basename "${img%.img}").txt"
mkdir -p "$(dirname "$log")"
s() { ssh -o ConnectTimeout=3 -o BatchMode=yes root@$H "$@"; }
bootid() { s 'cat /proc/sys/kernel/random/boot_id' 2>/dev/null; }
wait_up() { # old boot id -> seconds until ssh answers with a new boot id
	local t0=$1 old=$2 id
	for i in $(seq 1 120); do
		id=$(bootid)
		if [ -n "$id" ] && [ "$id" != "$old" ]; then
			# the user session / greeter needs a moment; systemd-analyze fails until graphical.target
			for j in $(seq 1 60); do s 'systemd-analyze >/dev/null 2>&1' && break; sleep 2; done
			echo $(( $(date +%s) - t0 )); return 0
		fi
		sleep 2
	done
	return 1
}
measure() { # boot# wall
	local r
	r=$(s 'a=$(systemd-analyze 2>/dev/null | head -1)
		k=$(echo "$a" | sed -n "s/.*in \([0-9.]*\)s (kernel).*/\1/p")
		u=$(echo "$a" | sed -n "s/.*+ \([0-9.min ]*\)s (userspace).*/\1/p")
		i=$(dmesg | sed -n "s/^\[ *\([0-9.]*\)\] Run \/init as init process.*/\1/p" | head -1)
		r=$(dmesg | sed -n "s/^\[ *\([0-9.]*\)\] systemd\[1\]: .*/\1/p" | head -1)
		g=$(journalctl -b -o short-monotonic _COMM=gnome-shell --no-pager 2>/dev/null | sed -n "s/^\[ *\([0-9.]*\)\].*/\1/p" | head -1)
		cl=$(cut -f1 /proc/sys/kernel/printk)
		c=$(dmesg -r | awk -v L=$cl "{ if (match(\$0, /^<[0-9]+>/)) { p = substr(\$0, 2, RSTART + RLENGTH - 3) % 8; if (p < L) n++ } } END { print n + 0 }")
		# "-" for a missing value so the columns never shift (e.g. a cleared dmesg)
		echo "${k:--} ${u:--} ${i:--} ${r:--} ${g:--} ${c:--}"
		echo "cmdline: $(cat /proc/cmdline)" >&2
		systemd-analyze blame 2>/dev/null | head -5 >&2' 2>>"$log")
	echo "$1 $2 $r" | tee -a "$log"
}
echo "boot-time $img N=$n host $H $(date)" | tee "$log"
echo "boot wall kernel user init sroot gdm con" | tee -a "$log"
[ $flash = 0 ] || [ -n "$restore" ] || { echo "--restore BASE.img is required when flashing"; exit 2; }
old=$(bootid)
[ -n "$old" ] || { echo "tablet not reachable at $H" | tee -a "$log"; exit 1; }
if [ $flash = 1 ]; then
	t0=$(date +%s)
	TB323FU_HOST=$H "$here/flash-boot.sh" "$img" 2>&1 | tail -2 | tee -a "$log"
	w=$(wait_up $t0 "$old") || { echo "TIMEOUT waiting for boot 1" | tee -a "$log"; exit 1; }
else
	w=-
fi
measure 1 "$w"
for b in $(seq 2 "$n"); do
	old=$(bootid); t0=$(date +%s)
	s 'systemctl reboot' 2>/dev/null
	w=$(wait_up $t0 "$old") || { echo "TIMEOUT waiting for boot $b" | tee -a "$log"; break; }
	measure "$b" "$w"
done
echo "== mean/min/max" | tee -a "$log"
awk '$1 ~ /^[0-9]+$/ { for (c = 2; c <= 8; c++) if ($c ~ /^[0-9.]+$/) { v[c] += $c; k[c]++; if (!(c in mn) || $c < mn[c]) mn[c] = $c; if ($c > mx[c]) mx[c] = $c } }
	END { split("boot wall kernel user init sroot gdm con", h, " "); for (c = 2; c <= 8; c++) if (k[c]) printf "%-7s %8.2f %8.2f %8.2f\n", h[c], v[c]/k[c], mn[c], mx[c] }' "$log" | tee -a "$log"
if [ $flash = 1 ] && [ -n "$restore" ]; then
	echo "restoring $restore" | tee -a "$log"
	old=$(bootid); t0=$(date +%s)
	TB323FU_HOST=$H "$here/flash-boot.sh" "$restore" 2>&1 | tail -1 | tee -a "$log"
	wait_up $t0 "$old" > /dev/null || echo "TIMEOUT waiting for the restored boot" | tee -a "$log"
fi
echo "log: $log"
