#!/bin/sh
# SPDX-License-Identifier: MIT
# kernel-update-test.sh -- the kernel update flow end to end, without a tablet:
# a local release channel built and signed with tools/kernel-channel.py (a
# throw-away minisign key), the daemon on a private session bus (no polkit)
# against a fake device tree -- boot_a/boot_b as files, the state root as a
# directory -- downloads through data/tb323fu-kernel-fetch (curl, file://),
# driven with tb323fu-ctl. The repacked boot_a is compared with what
# tools/boot-repack-kernel.py makes from the same stock image and kernel.
#
#   dbus-run-session -- sh tests/kernel-update-test.sh [BINDIR]     # default target/release
# Needs: dbus-run-session, minisign, python3, curl.
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
BIN=${1:-target/release}
R=$(mktemp -d /tmp/tb323fu-kupd.XXXXXX)
CH=$R/channel
export TB323FU_SYSFS_ROOT=$R TB323FU_CONFIG=$R/etc/tb323fu/helper.toml TB323FU_CURRENT_ROOT=baldur-root
export TB323FU_KERNEL_FETCH=$here/../data/tb323fu-kernel-fetch
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
mk() { mkdir -p "$(dirname "$R$1")"; printf '%s\n' "$2" > "$R$1"; }
sha() { sha256sum "$1" | cut -c1-64; }
kget() { sed -n "s/^$1=//p" "$R/var/lib/tb323fu/kernel-state"; }
C="$BIN/tb323fu-ctl --session"
D=
start() {
	$BIN/tb323fu-helperd --session --no-polkit >> "$R/daemon.log" 2>&1 &
	D=$!
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do $C versions > /dev/null 2>&1 && break; sleep 0.3; done
	sleep 0.5
}
stop() { [ -n "$D" ] && kill $D 2>/dev/null && wait $D 2>/dev/null; D=; }
trap 'stop; rm -rf "$R"' EXIT

V27="Linux version 7.3.0-rc4-tb323fu-t27 (u@h) (clang 19) #1 SMP PREEMPT Wed Oct  1 10:00:00 UTC 2026"
V28="Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang 19) #1 SMP PREEMPT Thu Oct  2 10:00:00 UTC 2026"
# fake kernels and a stock-shaped boot image (header v4, GKI signature, vbmeta, AVB footer), 2 MiB
python3 - "$R" "$V27" "$V28" <<'EOF'
import gzip, struct, sys
R, v27, v28 = sys.argv[1:4]
def kernel(n, banner):
    k = bytearray((i * 7 % 251) | 1 for i in range(n))
    k = k.replace(b"\n", b".")
    k[56:60] = b"ARM\x64"
    if " #" in banner:  # a real Image has the placeholder of init/version.o first
        ph = (banner.split(" #")[0] + " # SMP PREEMPT \n").encode()
        k[300:300 + len(ph)] = ph
    line = (banner + "\n").encode()
    k[1000:1000 + len(line)] = line
    k[1000 + len(line)] = 0
    return bytes(k)
open(f"{R}/Image-t27", "wb").write(kernel(300000, v27))
open(f"{R}/Image-tb323fu-t28.gz", "wb").write(gzip.compress(kernel(500000, v28), 6))
stock_k = kernel(200000, "Linux version 6.6.0-android (b@h) (clang) #1 SMP PREEMPT")
size, page = 2 << 20, 4096
hdr = bytearray(page); hdr[0:8] = b"ANDROID!"
struct.pack_into("<I", hdr, 8, len(stock_k)); struct.pack_into("<I", hdr, 40, 4)
d = bytearray(hdr) + stock_k
d += b"\0" * (-len(d) % page)
d += b"AVB0" + b"\x5a" * (16384 - 4)
vb_off = len(d)
vb = b"AVB0" + b"\xa5" * 2044
d += vb
d += b"\0" * (size - 64 - len(d))
d += struct.pack(">4sIIQQQ", b"AVBf", 1, 0, vb_off, vb_off, len(vb)) + b"\0" * 28
open(f"{R}/stock.img", "wb").write(d)
EOF
python3 "$repo/tools/boot-repack-kernel.py" "$R/stock.img" --self-test > /dev/null && ok "fake stock image passes the repack tool's self-test" || bad "fake stock image"
python3 "$repo/tools/boot-repack-kernel.py" "$R/stock.img" "$R/Image-t27" "$R/boot-t27.img" > /dev/null
python3 "$repo/tools/boot-repack-kernel.py" "$R/stock.img" "$R/Image-tb323fu-t28.gz" "$R/boot-t28.img" > /dev/null

# the device: boot_a (t27 running), boot_b (stock), the state root baldur-root, power
for p in "sda5 boot_a" "sda6 boot_b" "sda17 baldur-root"; do
	mkdir -p "$R/sys/class/block/${p% *}"
	printf 'DEVNAME=%s\nPARTNAME=%s\n' ${p% *} ${p#* } > "$R/sys/class/block/${p% *}/uevent"
done
mkdir -p "$R/dev" "$R/var/cache/tb323fu-kernel" "$R/var/lib/tb323fu" "$R/etc/tb323fu/keys"
cp "$R/boot-t27.img" "$R/dev/sda5"; cp "$R/stock.img" "$R/dev/sda6"
mk /etc/tb323fu/android-boot.sha256 "$(sha "$R/stock.img")"
B=/sys/class/power_supply/qcom-battmgr-bat
mk $B/type Battery; mk $B/status Discharging; mk $B/capacity 62; mk $B/current_now -500000
mk $B/charge_control_end_threshold 80
mk /proc/version "$V27"; mk /proc/sys/kernel/osrelease 7.3.0-rc4-tb323fu-t27; mk /proc/uptime "1000.0 1000.0"
mk /etc/os-release 'ID=debian'

# the channel, signed with a throw-away key the fake system trusts
minisign -G -W -p "$R/test.pub" -s "$R/test.key" > /dev/null 2>&1 || { echo "minisign missing"; exit 2; }
cp "$R/test.pub" "$R/etc/tb323fu/keys/kernel-test.pub"
printf '## t28\n- the speaker fix\n' > "$R/notes.md"
K="python3 $repo/tools/kernel-channel.py"
$K add "$CH" "$R/Image-tb323fu-t28.gz" --tag kernel-t28 --channel stable --notes "$R/notes.md" > /dev/null &&
	$K index "$CH" --set testing=kernel-t28 --set stable=kernel-t28 --helper-latest 9.9.0 > /dev/null &&
	$K sign "$CH" --key "$R/test.key" > /dev/null && $K verify "$CH" --pub "$R/test.pub" > /dev/null &&
	ok "channel built and signed (kernel-channel.py)" || bad "kernel-channel.py"
cat > "$R/etc/tb323fu/helper.toml" <<EOF
[kernel]
channel = "testing"
index_url = "file://$CH/index.json"
auto_check = false
EOF

start
$C kernel | grep -q "^running    7.3.0-rc4-tb323fu-t27" && ok "status: running t27" || { bad "status"; $C kernel; }
$C kernel | grep -q "nothing newer" && ok "  nothing available before a check" || bad "  available before a check"
$C kernel check | grep -q "kernel-t28 available: 7.3.0-rc4-tb323fu-t28" && ok "check: t28 available" || { bad "check"; $C kernel check; }
$C --json kernel | grep -q '"kernel-t28"' && ok "  Available lists it" || bad "  Available"
$C kernel | grep -q "helper     9.9.0 available: sudo apt update" && ok "  newer helper noticed (Debian command)" || bad "  helper notice"
$C kernel notes kernel-t28 | grep -q "the speaker fix" && ok "notes from the signed manifest" || bad "notes"
$C kernel install kernel-t28 2>&1 | grep -q "not downloaded" && ok "install before download refused" || bad "install before download"
$C kernel download kernel-t28 | grep -q "downloaded and verified" && ok "download + verify" || { bad "download"; cat "$R/var/cache/tb323fu-kernel/fetch.log"; }
[ -s "$R/var/lib/tb323fu/kernel/staged/kernel-t28/Image-tb323fu-t28.gz" ] && ok "  staged" || bad "  not staged"
$C kernel | grep -q "kernel-t28 7.3.0-rc4-tb323fu-t28 (downloaded)" && ok "  shown as downloaded" || bad "  downloaded state"
$C --json kernel | grep -q '"State": "ready"' && ok "  state ready" || bad "  state ready"

# refusals before anything is written
a0=$(sha "$R/dev/sda5")
mk $B/capacity 12
$C kernel install kernel-t28 2>&1 | grep -q "connect a charger" && ok "install refused at 12 % without a charger" || bad "low battery install"
mk $B/capacity 62
cp "$R/etc/tb323fu/android-boot.sha256" "$R/hash"; mk /etc/tb323fu/android-boot.sha256 "$(printf '0%.0s' $(seq 64))"
$C kernel install kernel-t28 2>&1 | grep -q "boot_b is not the recorded Android image" && ok "install refused when boot_b is not the recorded image" || bad "boot_b check"
cp "$R/hash" "$R/etc/tb323fu/android-boot.sha256"
[ "$(sha "$R/dev/sda5")" = "$a0" ] && ok "  boot_a untouched" || bad "  boot_a changed"

$C kernel install kernel-t28 | grep -q "t28 installed in boot_a" && ok "install" || { bad "install"; tail "$R/daemon.log"; }
cmp -s "$R/dev/sda5" "$R/boot-t28.img" && ok "  boot_a = boot-repack-kernel.py's image, byte for byte" || bad "  boot_a differs from the Python repack"
cmp -s "$R/var/lib/tb323fu/linux-good.img" "$R/boot-t27.img" && ok "  linux-good.img = the t27 image" || bad "  linux-good.img"
[ "$(kget trial)" = 7.3.0-rc4-tb323fu-t28 ] && [ "$(kget trial_version)" = "$V28" ] && [ "$(kget trial_channel)" = testing ] && [ "$(kget good)" = 7.3.0-rc4-tb323fu-t27 ] && ok "  kernel-state: trial t28 (testing: the configured channel, not the manifest's), good t27" || bad "  kernel-state: $(cat "$R/var/lib/tb323fu/kernel-state")"
$C --json kernel | grep -q '"State": "pending-reboot"' && ok "  state pending-reboot" || bad "  pending-reboot"
stop

# "restart": t28 runs, the initramfs counted a start
mk /proc/version "$V28"; mk /proc/sys/kernel/osrelease 7.3.0-rc4-tb323fu-t28
sed -i 's/^tries=.*//' "$R/var/lib/tb323fu/kernel-state"; echo tries=1 >> "$R/var/lib/tb323fu/kernel-state"
start
$C --json kernel | grep -q '"State": "trial"' && ok "after the restart: state trial" || bad "state trial"
$C --json kernel | grep -q '"KeepPending": true' && ok "  testing channel: KeepPending" || bad "  KeepPending"
$C kernel | grep -q "start 1 of 2" && ok "  status shows the start count" || bad "  start count"
$C kernel | grep -q "nothing newer" && ok "  t28 itself is not offered again" || bad "  t28 offered again"
$C kernel install kernel-t28 2>&1 | grep -q "not newer" && ok "  installing it again refused" || bad "  reinstall"
$C kernel keep | grep -q "kept 7.3.0-rc4-tb323fu-t28" && ok "keep" || bad "keep"
[ "$(kget good)" = 7.3.0-rc4-tb323fu-t28 ] && [ -z "$(kget trial)" ] && [ -z "$(kget tries)" ] && ok "  good t28, no trial" || bad "  state after keep: $(cat "$R/var/lib/tb323fu/kernel-state")"
cmp -s "$R/var/lib/tb323fu/linux-good.img" "$R/boot-t28.img" && ok "  linux-good.img = the t28 image" || bad "  linux-good after keep"
[ ! -e "$R/var/lib/tb323fu/kernel/staged/kernel-t28" ] && ok "  staged download cleaned up" || bad "  staged left"
$C --json kernel | grep -q '"State": "idle"' && ok "  state idle" || bad "  idle"
$C kernel check | grep -q "up to date" && ok "check when running the newest: up to date" || bad "up to date"

# signatures: a changed index, an unknown key, an expired index
cp "$CH/index.json" "$R/index.bak"
sed -i 's/"latest": "9.9.0"/"latest": "9.9.1"/' "$CH/index.json"
$C kernel check 2>&1 | grep -q "signature does not verify" && ok "a changed index is refused" || bad "changed index accepted"
cp "$R/index.bak" "$CH/index.json"
mv "$R/etc/tb323fu/keys/kernel-test.pub" "$R/test.pub.off"
$C kernel check 2>&1 | grep -q "signature does not verify" && ok "an index signed with an unknown key is refused" || bad "unknown key accepted"
mv "$R/test.pub.off" "$R/etc/tb323fu/keys/kernel-test.pub"
$K index "$CH" --set testing=kernel-t28 --expires-days -1 > /dev/null && $K sign "$CH" --key "$R/test.key" > /dev/null
$C kernel check 2>&1 | grep -q "expired" && ok "an expired index is reported, not used" || bad "expired index"
$C --json kernel | grep -q '"IndexExpired": true' && ok "  IndexExpired" || bad "  IndexExpired"

# a manual rollback writes linux-good.img back
printf 'garbage' | dd of="$R/dev/sda5" bs=1 seek=5000 conv=notrunc 2>/dev/null
$C kernel rollback | grep -q "boot_a holds 7.3.0-rc4-tb323fu-t28 again" && cmp -s "$R/dev/sda5" "$R/boot-t28.img" && ok "rollback restores linux-good.img" || bad "rollback"

# settings
$C kernel channel stable && grep -q 'channel = "stable"' "$R/etc/tb323fu/helper.toml" && ok "channel stable persisted" || bad "channel"
$C kernel channel beta 2>/dev/null && bad "channel beta accepted" || ok "channel beta refused"
$C kernel auto-check on && grep -q 'auto_check = true' "$R/etc/tb323fu/helper.toml" && ok "auto-check persisted" || bad "auto-check"

[ $fail = 0 ] && echo "ALL PASSED" || { echo "SOME FAILED"; tail -30 "$R/daemon.log"; }
exit $fail
