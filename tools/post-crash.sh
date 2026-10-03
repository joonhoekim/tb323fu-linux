#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# post-crash.sh — PC: right after the tablet came back from an unexpected
# reboot/crash, collect everything the boot logs still hold, in one go:
#   pstore (first; both /sys/fs/pstore and systemd-pstore's
#   archive /var/lib/systemd/pstore), the boot list, the previous boot's journal
#   (tail of everything + kernel, and where it stops vs. the boot's end time),
#   this boot's early kernel log and reset/watchdog/PON lines, cmdline,
#   download mode, uptime.
#   TB323FU_HOST=192.168.7.2 LOGDIR=./logs tools/post-crash.sh TAG   # -> $LOGDIR/<date>-TAG/
# Read-only on the tablet. If the tablet sits in Qualcomm crash-dump mode
# (USB 05c6:900e) instead, it has no network: collect the dump first (one
# Sahara session only), then force-restart.
set -u
H=${TB323FU_HOST:-192.168.7.2}
tag=${1:-crash}
d="${LOGDIR:-./logs}/$(date +%Y-%m-%d)-$tag"
mkdir -p "$d"
s() { ssh -o ConnectTimeout=5 -o BatchMode=yes root@$H "$@"; }
s 'ls /sys/fs/pstore/ 2>/dev/null' > "$d/pstore-list.txt"
if [ -s "$d/pstore-list.txt" ]; then scp -q "root@$H:/sys/fs/pstore/*" "$d/"; fi
# systemd-pstore moves the records here during boot, so /sys/fs/pstore is
# usually empty by the time we look
s 'ls -la --time-style=full-iso /var/lib/systemd/pstore/' > "$d/var-pstore-list.txt"
mkdir -p "$d/var-pstore" && scp -q "root@$H:/var/lib/systemd/pstore/*" "$d/var-pstore/" 2>/dev/null
s 'uptime; uname -v; cat /proc/cmdline; echo "download_mode: $(cat /sys/module/qcom_scm/parameters/download_mode 2>/dev/null)"' > "$d/now.txt"
s 'journalctl --list-boots --no-pager' > "$d/boots.txt"
s 'journalctl -b -1 --no-pager -o short-monotonic | tail -300' > "$d/prev-boot-tail.txt"
s 'journalctl -b -1 -k --no-pager -o short-monotonic | tail -200' > "$d/prev-boot-kernel-tail.txt"
s 'dmesg | head -400' > "$d/this-boot-dmesg-head.txt"
s 'dmesg | grep -iE "reset|reboot|restart|watchdog|wdog|pon|panic|oops|bite|bark|err_fatal|crash|ramoops|pstore"' > "$d/this-boot-reset-lines.txt"
echo "== $d"
echo "pstore files: $(wc -l < "$d/pstore-list.txt") in /sys/fs/pstore, $(ls "$d/var-pstore" | wc -l) in /var/lib/systemd/pstore (newest: $(ls -t "$d/var-pstore" | head -1))"
grep -a -m3 -iE "panic|blocked for|Oops|BUG:" "$d"/var-pstore/console-ramoops-* 2>/dev/null | cut -c1-160
cat "$d/now.txt" | head -1
tail -2 "$d/boots.txt"
echo "prev boot journal ends: $(tail -1 "$d/prev-boot-tail.txt" | cut -c1-160)"
echo "prev boot kernel ends:  $(tail -1 "$d/prev-boot-kernel-tail.txt" | cut -c1-160)"
echo "reset-ish lines this boot: $(wc -l < "$d/this-boot-reset-lines.txt")"
