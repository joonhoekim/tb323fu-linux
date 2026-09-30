#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# flash-boot.sh -- write a boot image to boot_a from the running Linux on the
# tablet, over SSH, verify it and reboot into it.
#
#   TB323FU_HOST=192.168.7.2 tools/flash-boot.sh IMG [--no-reboot]
#
# TB323FU_HOST: the tablet's address. The default, 192.168.7.2, is the USB
# network gadget address the root filesystem configures (the PC side of the
# cable is 192.168.7.1); use the Wi-Fi address instead if you connect that way.
# Needs your SSH key in the tablet's /root/.ssh/authorized_keys.
#
# Checks the copy's sha256, that boot_a exists exactly once (found by its GPT
# name, never by number) and is big enough, writes, reads back and compares
# before rebooting. Qualcomm download mode is switched off first, so a crash on
# the way reboots instead of hanging in dump mode.
set -euo pipefail
img=$(cygpath -u "$1" 2>/dev/null || echo "$1")   # Git Bash: scp reads "C:/..." as host "C"
reboot=1
[ "${2:-}" = --no-reboot ] && reboot=0
host=root@${TB323FU_HOST:-192.168.7.2}
ssh_=(ssh -o BatchMode=yes -o ConnectTimeout=5 "$host")
want=$(sha256sum "$img" | cut -d' ' -f1)
size=$(stat -c %s "$img")
echo "image $want ($size bytes)"
scp -q -o BatchMode=yes "$img" "$host:/root/next-boot.img"
"${ssh_[@]}" "set -e
echo off > /sys/module/qcom_scm/parameters/download_mode 2>/dev/null || true
[ \"\$(sha256sum /root/next-boot.img | cut -d' ' -f1)\" = $want ] || { echo 'copy corrupt'; exit 1; }
a=\$(for u in /sys/class/block/*/uevent; do grep -q '^PARTNAME=boot_a\$' \$u && echo /dev/\$(grep '^DEVNAME=' \$u | cut -d= -f2); done; true)
[ \$(echo \"\$a\" | wc -w) = 1 ] || { echo \"boot_a not unique: \$a\"; exit 1; }
[ \$(blockdev --getsize64 \$a) -ge $size ] || { echo 'boot_a too small'; exit 1; }
dd if=/root/next-boot.img of=\$a bs=1M conv=fsync status=none
sync; echo 3 > /proc/sys/vm/drop_caches
got=\$(head -c $size \$a | sha256sum | cut -d' ' -f1)
[ \"\$got\" = $want ] || { echo \"boot_a readback \$got does not match -- do NOT reboot\"; exit 1; }
echo \"boot_a (\$a) = ${want:0:16}\"
rm /root/next-boot.img"
if [ $reboot = 1 ]; then
	echo "rebooting $(date +%T)"
	"${ssh_[@]}" 'systemctl reboot' || true
fi
