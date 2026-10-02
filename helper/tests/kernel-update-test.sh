#!/bin/sh
# SPDX-License-Identifier: MIT
# kernel-update-test.sh -- the kernel update flow end to end, without a tablet:
# releases prepared with tools/kernel-release.py and published on a local
# stand-in of the GitHub REST API (a file:// tree), the daemon on a private
# session bus (no polkit) against a fake device tree -- boot_a/boot_b as
# files, the state root as a directory -- downloads through
# data/tb323fu-kernel-fetch (curl, file://), driven with tb323fu-ctl. The
# repacked boot_a is compared with what tools/boot-repack-kernel.py makes from
# the same stock image and kernel. Then a kernel from a local file
# (inspect, install-local) and the optional minisign check.
#
#   dbus-run-session -- sh tests/kernel-update-test.sh [BINDIR]     # default target/release
# Needs: dbus-run-session, python3, curl, minisign.
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
BIN=${1:-target/release}
R=$(mktemp -d /tmp/tb323fu-kupd.XXXXXX)
API=$R/api
export TB323FU_SYSFS_ROOT=$R TB323FU_CONFIG=$R/etc/tb323fu/helper.toml TB323FU_CURRENT_ROOT=baldur-root
export TB323FU_KERNEL_FETCH=$here/../data/tb323fu-kernel-fetch
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
mk() { mkdir -p "$(dirname "$R$1")"; printf '%s\n' "$2" > "$R$1"; }
sha() { sha256sum "$1" | cut -c1-64; }
kget() { sed -n "s/^$1=//p" "$R/var/lib/tb323fu/kernel-state"; }
C="$BIN/tb323fu-ctl --session"
K="python3 $repo/tools/kernel-release.py"
D=
start() {
	$BIN/tb323fu-helperd --session --no-polkit >> "$R/daemon.log" 2>&1 &
	D=$!
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do $C versions > /dev/null 2>&1 && break; sleep 0.3; done
	sleep 0.5
}
stop() { [ -n "$D" ] && kill $D 2>/dev/null && wait $D 2>/dev/null; D=; }
trap 'stop; rm -rf "$R"' EXIT
cfg() { # CHANNEL [extra lines]
	printf '[kernel]\nchannel = "%s"\nsource = "github:o/r"\napi_url = "file://%s"\nauto_check = false\n%s\n' "$1" "$API" "${2:-}" > "$R/etc/tb323fu/helper.toml"
}

V27="Linux version 7.3.0-rc4-tb323fu-t27 (u@h) (clang 19) #1 SMP PREEMPT Wed Oct  1 10:00:00 UTC 2026"
V28="Linux version 7.3.0-rc4-tb323fu-t28 (u@h) (clang 19) #1 SMP PREEMPT Thu Oct  2 10:00:00 UTC 2026"
V29="Linux version 7.3.0-rc4-tb323fu-t29 (u@h) (clang 19) #1 SMP PREEMPT Fri Oct  3 10:00:00 UTC 2026"
VL="Linux version 7.3.0-rc4-tb323fu-t28 (me@pc) (clang 19) #5 SMP PREEMPT Fri Oct  3 12:00:00 UTC 2026"
# fake kernels (VL: a self-built t28 with an initramfs carrying the shared
# modules image) and a stock-shaped boot image (header v4, GKI signature,
# vbmeta, AVB footer), 2 MiB
python3 - "$R" "$V27" "$V28" "$V29" "$VL" <<'EOF'
import gzip, struct, sys
R, v27, v28, v29, vl = sys.argv[1:6]
def kernel(n, banner, initramfs=None):
    k = bytearray((i * 7 % 251) | 1 for i in range(n))
    k = k.replace(b"\n", b".")
    k[56:60] = b"ARM\x64"
    if " #" in banner:  # a real Image has the placeholder of init/version.o first
        ph = (banner.split(" #")[0] + " # SMP PREEMPT \n").encode()
        k[300:300 + len(ph)] = ph
    line = (banner + "\n").encode()
    k[1000:1000 + len(line)] = line
    k[1000 + len(line)] = 0
    if initramfs:
        cpio = b""
        for name in initramfs + ["TRAILER!!!"]:
            nm = name.encode() + b"\0"
            cpio += b"070701" + b"%08X" % 1 + b"0" * 80 + b"%08X" % len(nm) + b"0" * 8 + nm
            cpio += b"\0" * (-len(cpio) % 4)
        gz = gzip.compress(cpio + b"\0" * 100000, 6)
        k[8192:8192 + len(gz)] = gz
    return bytes(k)
open(f"{R}/Image-t27", "wb").write(kernel(300000, v27))
open(f"{R}/Image-t28", "wb").write(kernel(500000, v28))
open(f"{R}/Image-t29", "wb").write(kernel(520000, v29))
open(f"{R}/Image-local", "wb").write(kernel(480000, vl, ["init", "lib/modules/7.3.0-rc4-tb323fu-t28", "lib/modules/7.3.0-rc4-tb323fu-t28.sqfs"]))
open(f"{R}/Image-nomods", "wb").write(kernel(470000, vl.replace("#5", "#6"), ["init"]))
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
RP="python3 $repo/tools/boot-repack-kernel.py"
$RP "$R/stock.img" --self-test > /dev/null && ok "fake stock image passes the repack tool's self-test" || bad "fake stock image"
$RP "$R/stock.img" "$R/Image-t27" "$R/boot-t27.img" > /dev/null

# releases: kernel-t28 (release), helper-v9.9.0, kernel-t29 (pre-release)
$K assets "$R/rel28" "$R/Image-t28" --gzip > /dev/null && $K check "$R/rel28" > /dev/null && ok "release assets t28 (kernel-release.py assets + check)" || bad "kernel-release.py assets t28"
$K assets "$R/rel29" "$R/Image-t29" --gzip > /dev/null && ok "release assets t29" || bad "assets t29"
$K assets "$R/bad" "$R/Image-t28" --tag t30 > /dev/null 2>&1 && bad "a t28 kernel accepted as t30" || ok "assets: a kernel whose release does not end in the tag is refused"
$RP "$R/stock.img" "$R/rel28/Image-tb323fu-t28.gz" "$R/boot-t28.img" > /dev/null
printf '## t28\n- the speaker fix\n<!-- tb323fu: min_helper=0.1.0 -->\n' > "$R/notes28.md"
$K fake-api "$API" --repo o/r --dir "$R/rel28" --tag kernel-t28 --title "tb323fu-linux t28" --notes "$R/notes28.md" > /dev/null &&
	$K fake-api "$API" --repo o/r --helper 9.9.0 > /dev/null &&
	$K fake-api "$API" --repo o/r --dir "$R/rel29" --tag kernel-t29 --prerelease > /dev/null && ok "local GitHub API stand-in (fake-api)" || bad "fake-api"

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
cfg stable

start
$C kernel | grep -q "^running    7.3.0-rc4-tb323fu-t27" && ok "status: running t27" || { bad "status"; $C kernel; }
$C kernel | grep -q "from github:o/r" && ok "  status names the source" || bad "  source"
$C kernel | grep -q "nothing newer" && ok "  nothing available before a check" || bad "  available before a check"
$C kernel check | grep -q "kernel-t28 available: tb323fu-linux t28" && ok "check (stable): t28 available, not the t29 pre-release" || { bad "check"; $C kernel check; cat "$R/var/cache/tb323fu-kernel/fetch.log"; }
$C --json kernel | grep -q '"kernel-t28"' && ok "  Available lists it" || bad "  Available"
$C kernel | grep -q "helper     9.9.0 available: sudo apt update" && ok "  newer helper noticed (helper-v9.9.0 release, Debian command)" || bad "  helper notice"
$C kernel notes kernel-t28 | grep -q "the speaker fix" && ok "notes = the release body" || bad "notes"
$C kernel install kernel-t28 2>&1 | grep -q "not downloaded" && ok "install before download refused" || bad "install before download"
$C kernel download kernel-t28 | grep -q "downloaded and checked" && ok "download + check (SHA256SUMS, digest, banner)" || { bad "download"; cat "$R/var/cache/tb323fu-kernel/fetch.log"; }
[ -s "$R/var/lib/tb323fu/kernel/staged/kernel-t28/Image-tb323fu-t28.gz" ] && ok "  staged: the .gz" || bad "  not staged"
$C kernel | grep -q "kernel-t28 tb323fu-linux t28 (downloaded)" && ok "  shown as downloaded" || bad "  downloaded state"
$C --json kernel | grep -q '"State": "ready"' && ok "  state ready" || bad "  state ready"

# refusals before anything is written
a0=$(sha "$R/dev/sda5")
mk $B/capacity 12
$C kernel install kernel-t28 2>&1 | grep -q "connect a charger" && ok "install refused at 12 % without a charger" || bad "low battery install"
mk $B/capacity 62
cp "$R/etc/tb323fu/android-boot.sha256" "$R/hash"; mk /etc/tb323fu/android-boot.sha256 "$(printf '0%.0s' $(seq 64))"
$C kernel install kernel-t28 2>&1 | grep -q "boot_b is not the recorded Android image" && ok "install refused when boot_b is not the recorded image" || bad "boot_b check"
cp "$R/hash" "$R/etc/tb323fu/android-boot.sha256"
printf 'x' | dd of="$R/var/lib/tb323fu/kernel/staged/kernel-t28/Image-tb323fu-t28.gz" bs=1 seek=300 conv=notrunc 2>/dev/null
$C kernel install kernel-t28 2>&1 | grep -q "does not match SHA256SUMS" && ok "a staged file changed after the download: refused at install" || bad "changed staged file installed"
[ "$(sha "$R/dev/sda5")" = "$a0" ] && ok "  boot_a untouched" || bad "  boot_a changed"
$C kernel download kernel-t28 > /dev/null

$C kernel install kernel-t28 | grep -q "t28 installed in boot_a" && ok "install" || { bad "install"; tail "$R/daemon.log"; }
cmp -s "$R/dev/sda5" "$R/boot-t28.img" && ok "  boot_a = boot-repack-kernel.py's image, byte for byte" || bad "  boot_a differs from the Python repack"
cmp -s "$R/var/lib/tb323fu/linux-good.img" "$R/boot-t27.img" && ok "  linux-good.img = the t27 image" || bad "  linux-good.img"
[ "$(kget trial)" = 7.3.0-rc4-tb323fu-t28 ] && [ "$(kget trial_version)" = "$V28" ] && [ "$(kget trial_channel)" = stable ] && [ -z "$(kget trial_keep)" ] && [ "$(kget good)" = 7.3.0-rc4-tb323fu-t27 ] && ok "  kernel-state: trial t28 (stable, no Keep needed), good t27" || bad "  kernel-state: $(cat "$R/var/lib/tb323fu/kernel-state")"
$C --json kernel | grep -q '"State": "pending-reboot"' && ok "  state pending-reboot" || bad "  pending-reboot"
stop

# "restart": t28 runs, the initramfs counted a start
mk /proc/version "$V28"; mk /proc/sys/kernel/osrelease 7.3.0-rc4-tb323fu-t28
sed -i 's/^tries=.*//' "$R/var/lib/tb323fu/kernel-state"; echo tries=1 >> "$R/var/lib/tb323fu/kernel-state"
start
$C --json kernel | grep -q '"State": "trial"' && ok "after the restart: state trial" || bad "state trial"
$C --json kernel | grep -q '"KeepPending": false' && ok "  stable: no Keep needed (the confirm unit keeps it)" || bad "  KeepPending"
$C kernel | grep -q "start 1 of 2" && ok "  status shows the start count" || bad "  start count"
$C kernel | grep -q "nothing newer" && ok "  t28 itself is not offered again" || bad "  t28 offered again"
$C kernel install kernel-t28 2>&1 | grep -q "not newer" && ok "  installing it again refused" || bad "  reinstall"
$C kernel keep | grep -q "kept 7.3.0-rc4-tb323fu-t28" && ok "keep (what the confirm unit does after 90 s)" || bad "keep"
[ "$(kget good)" = 7.3.0-rc4-tb323fu-t28 ] && [ -z "$(kget trial)" ] && [ -z "$(kget tries)" ] && ok "  good t28, no trial" || bad "  state after keep: $(cat "$R/var/lib/tb323fu/kernel-state")"
cmp -s "$R/var/lib/tb323fu/linux-good.img" "$R/boot-t28.img" && ok "  linux-good.img = the t28 image" || bad "  linux-good after keep"
[ ! -e "$R/var/lib/tb323fu/kernel/staged/kernel-t28" ] && ok "  staged download cleaned up" || bad "  staged left"
$C kernel check | grep -q "up to date (stable channel: kernel-t28)" && ok "check when running the newest release: up to date" || bad "up to date"

# testing channel: the pre-release
$C kernel channel testing && grep -q 'channel = "testing"' "$R/etc/tb323fu/helper.toml" && ok "channel testing persisted" || bad "channel"
$C kernel check | grep -q "kernel-t29 available" && ok "check (testing): the t29 pre-release" || bad "testing check"
cp "$API/assets/$(python3 -c "import json;r=json.load(open('$API/repos/o/r/releases'));print([a['id'] for a in r[0]['assets'] if a['name'].endswith('.gz')][0])")" "$R/asset.bak"
gzf=$(python3 -c "import json;r=json.load(open('$API/repos/o/r/releases'));print([a['id'] for a in r[0]['assets'] if a['name'].endswith('.gz')][0])")
printf 'x' | dd of="$API/assets/$gzf" bs=1 seek=200 conv=notrunc 2>/dev/null
$C kernel download kernel-t29 2>&1 | grep -q "does not match SHA256SUMS" && ok "a damaged download is refused (SHA256SUMS)" || bad "damaged download accepted"
cp "$R/asset.bak" "$API/assets/$gzf"

# optional signatures: off by default; on, they need a key and SHA256SUMS.minisig
cfg testing 'require_signature = true'
$C reload > /dev/null 2>&1
$C kernel download kernel-t29 2>&1 | grep -q "SHA256SUMS.minisig" && ok "require_signature: a release without SHA256SUMS.minisig is refused" || bad "unsigned release accepted"
if minisign -G -W -p "$R/test.pub" -s "$R/test.key" > /dev/null 2>&1; then
	minisign -S -s "$R/test.key" -m "$R/rel29/SHA256SUMS" > /dev/null 2>&1
	$K fake-api "$API" --repo o/r --dir "$R/rel29" --tag kernel-t29 --prerelease > /dev/null
	$C kernel check > /dev/null
	$C kernel download kernel-t29 2>&1 | grep -q "no public key is configured" && ok "  signed, but no key configured: refused" || bad "  no key"
	cp "$R/test.pub" "$R/etc/tb323fu/keys/kernel-test.pub"
	$C kernel download kernel-t29 | grep -q "downloaded and checked" && ok "  signed with a configured key: accepted" || bad "  signed release"
	minisign -G -W -p "$R/other.pub" -s "$R/other.key" > /dev/null 2>&1; cp "$R/other.pub" "$R/etc/tb323fu/keys/kernel-test.pub"
	$C kernel download kernel-t29 2>&1 | grep -q "trusted key" && ok "  signed with another key: refused" || bad "  other key"
	rm -f "$R/etc/tb323fu/keys/kernel-test.pub"
else
	bad "minisign missing"
fi
cfg testing
$C reload > /dev/null 2>&1

# a kernel from a file: same release as the running t28, another build
$C kernel inspect "$R/Image-local" > "$R/insp"
grep -q "release    7.3.0-rc4-tb323fu-t28" "$R/insp" && grep -q "shared modules image inside" "$R/insp" && ! grep -q "^warning" "$R/insp" && ok "inspect: release, shared modules, no warnings" || { bad "inspect"; cat "$R/insp"; }
$C kernel inspect "$R/Image-nomods" | grep -q "^warning    No shared modules" && ok "inspect: a kernel without the modules image is warned about" || bad "inspect nomods"
$C kernel inspect "$R/boot-t27.img" | grep -q "boot image" && ok "inspect: a boot image (its kernel field)" || bad "inspect boot image"
$C kernel inspect "$R/notes28.md" 2>&1 | grep -q "neither a raw arm64 Image nor gzip" && ok "inspect: not a kernel: refused" || bad "inspect text file"
$C kernel install-local "$R/Image-local" < /dev/null 2>&1 | grep -q "add --yes" && ok "install-local without a terminal asks for --yes" || bad "install-local without --yes"
$C kernel install-local "$R/Image-local" --name "speaker test" --yes | grep -q "installed in boot_a.*press Keep" && ok "install-local --trial (default)" || { bad "install-local"; tail "$R/daemon.log"; }
[ "$(kget trial_channel)" = local ] && [ "$(kget trial_keep)" = 1 ] && [ "$(kget trial_label)" = "speaker test" ] && [ "$(kget trial_version)" = "$VL" ] && ok "  kernel-state: local, waits for Keep, label, its banner" || bad "  state: $(cat "$R/var/lib/tb323fu/kernel-state")"
$RP "$R/stock.img" "$R/Image-local" "$R/boot-local.img" > /dev/null
cmp -s "$R/dev/sda5" "$R/boot-local.img" && ok "  boot_a = the Python repack of the file" || bad "  boot_a differs"
cmp -s "$R/var/lib/tb323fu/linux-good.img" "$R/boot-t28.img" && ok "  linux-good.img still t28" || bad "  linux-good changed"
stop
mk /proc/version "$VL"
echo tries=1 >> "$R/var/lib/tb323fu/kernel-state"
start
$C --json kernel | grep -q '"KeepPending": true' && ok "  after the restart: KeepPending" || bad "  KeepPending local"
$C kernel | grep -q 'trial      7.3.0-rc4-tb323fu-t28 "speaker test" (a local file, start 1 of 2)' && ok "  status: label and origin" || { bad "  status local"; $C kernel; }
$C kernel install-local "$R/Image-nomods" --yes 2>&1 | grep -q "still on trial" && ok "  another install while on trial: refused" || bad "  install on trial"
$C kernel keep | grep -q kept && [ "$(kget good_label)" = "speaker test" ] && ok "  keep: good, with its label" || bad "  keep local"
$C kernel install-local "$R/Image-nomods" --keep --yes | grep -q "kept once a system has run 90 s.*no shared modules" && [ -z "$(kget trial_keep)" ] && ok "install-local --keep: no Keep needed; warned about modules" || bad "install-local --keep"

# a manual rollback writes linux-good.img back
$C kernel rollback | grep -q "boot_a holds 7.3.0-rc4-tb323fu-t28 again" && cmp -s "$R/dev/sda5" "$R/boot-local.img" && ok "rollback restores linux-good.img (the kept local build)" || bad "rollback"
[ "$(kget failed)" = 7.3.0-rc4-tb323fu-t28 ] && [ -z "$(kget trial)" ] && ok "  the pending install recorded as failed" || bad "  failed"

# settings
$C kernel channel beta 2>/dev/null && bad "channel beta accepted" || ok "channel beta refused"
$C kernel auto-check on && grep -q 'auto_check = true' "$R/etc/tb323fu/helper.toml" && ok "auto-check persisted" || bad "auto-check"
grep -q 'source = "github:o/r"' "$R/etc/tb323fu/helper.toml" && ok "source kept when the daemon writes the file" || bad "source lost"

[ $fail = 0 ] && echo "ALL PASSED" || { echo "SOME FAILED"; tail -30 "$R/daemon.log"; }
exit $fail
