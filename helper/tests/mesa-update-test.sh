#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Joonhoe Kim
# mesa-update-test.sh -- the helper's Mesa channel end to end, without a tablet: releases are made
# with tools/mesa-release.py from a fake stage (small files standing in for the drivers) and
# published on a file:// stand-in of the GitHub API (kernel-release.py fake-api); the daemon runs
# on a private session bus (no polkit) against a fake root.
# Covers: check, download, a damaged download, install, on/off and env.conf, the tb323fu-mesa
# wrapper, the trial (boots counted once each, switched off after two starts), keep, a second
# version, rollback.
#
#   dbus-run-session -- sh tests/mesa-update-test.sh [BINDIR]     # default target/release
# Needs: dbus-run-session, python3, curl.
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
BIN=$(cd "${1:-target/release}" && pwd)
R=$(mktemp -d /tmp/tb323fu-mesa.XXXXXX)
API=$R/api
export TB323FU_SYSFS_ROOT=$R TB323FU_CONFIG=$R/etc/tb323fu/helper.toml TB323FU_CURRENT_ROOT=baldur-root
export TB323FU_KERNEL_FETCH=$here/../data/tb323fu-kernel-fetch TB323FU_BOOT_ID=boot-0
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
C="$BIN/tb323fu-ctl --session"
K="python3 $repo/tools/kernel-release.py"
M="python3 $repo/tools/mesa-release.py"
MD=$R/var/lib/tb323fu/mesa
WRAP="env TB323FU_MESA_DIR=$MD sh $here/../data/tb323fu-mesa"

start() {
	"$BIN/tb323fu-helperd" --session --no-polkit >> "$R/daemon.log" 2>&1 &
	echo $! > "$R/daemon.pid"
	for i in $(seq 30); do $C versions > /dev/null 2>&1 && break; sleep 0.3; done
}
stop() { kill "$(cat "$R/daemon.pid" 2>/dev/null)" 2>/dev/null; sleep 0.3; }
trap 'stop; rm -rf "$R"' EXIT
prop() { $C --json mesa 2>/dev/null | sed -n "s/.*\"$1\": \"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\}\$/\1/p" | head -1; }

release() { # VERSION
	S=$R/stage-$1
	mkdir -p "$S/usr/lib"
	printf 'vulkan %s\n' "$1" > "$S/usr/lib/libvulkan_freedreno.so"
	printf 'opencl %s\n' "$1" > "$S/usr/lib/libRusticlOpenCL.so.1.0.0"
	printf 'MIT\n' > "$R/LICENSE"
	$M assets "$R/rel-$1" --stage "$S" --version "$1" --commit c0ffee --base base$1 --mesa-version 26.3.0-devel \
		--license mesa-license.txt="$R/LICENSE" --no-strip > /dev/null && $M check "$R/rel-$1" > /dev/null &&
		$K fake-api "$API" --repo o/r --tag "mesa-$1" --dir "$R/rel-$1" > /dev/null
}

mkdir -p "$R/etc/tb323fu" "$R/var/cache/tb323fu-kernel"
printf '[kernel]\nsource = "github:o/r"\napi_url = "file://%s"\nauto_check = false\n' "$API" > "$R/etc/tb323fu/helper.toml"
release 2026.10.06 && ok "mesa-release.py assets + check, fake-api mesa-2026.10.06" || bad "release"

start
$C mesa check | grep -q "Mesa 2026.10.06 available" && ok "check: 2026.10.06 available" || { bad "check"; $C mesa check; tail "$R/daemon.log"; }
[ "$(prop Installed)" = "" ] && ok "  nothing installed yet" || bad "  Installed"
$C mesa on 2>&1 | grep -q "no Mesa version is installed" && ok "  on refused before an install" || bad "  on without install"

# a damaged tarball: the asset's bytes changed after SHA256SUMS
cp -r "$API/assets" "$R/assets.bak"
id=$(python3 -c 'import json,sys; print([x["id"] for r in json.load(open(sys.argv[1])) for x in r["assets"] if x["name"].endswith(".tar.gz")][0])' "$API/repos/o/r/releases")
printf 'X' | dd of="$API/assets/$id" bs=1 seek=100 conv=notrunc 2>/dev/null
$C mesa download 2>&1 | grep -qi "does not match" && ok "damaged download refused" || bad "damaged download"
[ -z "$(ls "$MD/versions" 2>/dev/null)" ] && ok "  nothing unpacked" || bad "  unpacked after a damaged download"
rm -rf "$API/assets"; cp -r "$R/assets.bak" "$API/assets"

$C mesa download | grep -q "Mesa 2026.10.06 downloaded and checked (vulkan, opencl, based on upstream base2026.10.06)" && ok "download" || { bad "download"; $C mesa download; }
[ "$(prop Downloaded)" = 2026.10.06 ] && [ "$(prop State)" = ready ] && ok "  Downloaded, State ready" || bad "  Downloaded/State"
$C mesa install | grep -q "installed; switch the channel on" && ok "install" || bad "install"
[ "$(prop Installed)" = 2026.10.06 ] && ok "  Installed 2026.10.06" || bad "  Installed after install"
grep -q "\"library_path\": \"/var/lib/tb323fu/mesa/current/lib/libvulkan_freedreno.so\"" "$MD/icd/turnip.json" && ok "  Vulkan ICD points into current/" || bad "  ICD"
[ ! -s "$MD/env.conf" ] && ok "  env.conf empty while off" || bad "  env.conf while off"

$C mesa on | grep -q "used from the next login" && ok "on" || bad "on"
grep -q "^VK_DRIVER_FILES=/var/lib/tb323fu/mesa/icd/turnip.json$" "$MD/env.conf" && grep -q "^RUSTICL_ENABLE=freedreno$" "$MD/env.conf" &&
	grep -q "^OCL_ICD_VENDORS=/var/lib/tb323fu/mesa/icd/opencl$" "$MD/env.conf" && ok "  env.conf selects the build" || { bad "  env.conf"; cat "$MD/env.conf"; }
[ "$(prop Trial)" = 2026.10.06 ] && ok "  on trial" || bad "  trial"
$WRAP run sh -c 'echo "$VK_DRIVER_FILES $RUSTICL_ENABLE"' | grep -q "$MD/icd/turnip.json freedreno" && ok "wrapper run sets the variables" || bad "wrapper run"
VK_DRIVER_FILES=x $WRAP distro sh -c 'echo "[${VK_DRIVER_FILES:-}]"' | grep -q '^\[\]$' && ok "wrapper distro clears them" || bad "wrapper distro"

# the trial: each boot counted once (daemon start), the third start without keep switches off
stop
for b in boot-1 boot-1 boot-2; do TB323FU_BOOT_ID=$b "$BIN/tb323fu-helperd" --mesa-boot-tick > /dev/null; done
grep -q "^tries=2$" "$MD/state" && ok "trial: boot-1 counted once, boot-2 once (tries=2)" || { bad "tries"; cat "$MD/state"; }
TB323FU_BOOT_ID=boot-3 start
[ "$(prop Enabled)" = false ] && [ "$(prop Failed)" = 2026.10.06 ] && ok "  third start without keep: switched off, Failed set" || { bad "  revert"; $C mesa; }
[ ! -s "$MD/env.conf" ] && ok "  env.conf emptied" || bad "  env.conf after revert"

$C mesa on > /dev/null && $C mesa keep | grep -q "Mesa 2026.10.06 kept" && ok "on again, keep" || bad "keep"
stop
for b in boot-4 boot-5 boot-6; do TB323FU_BOOT_ID=$b "$BIN/tb323fu-helperd" --mesa-boot-tick > /dev/null; done
TB323FU_BOOT_ID=boot-7 start
[ "$(prop Enabled)" = true ] && ok "  kept: stays on over four starts" || bad "  kept but switched off"

# a second version, rollback
release 2026.10.07 && $C mesa update | grep -q "Mesa 2026.10.07 installed and used from the next login" && ok "update to 2026.10.07 (on trial again)" || { bad "update"; $C mesa; }
[ "$(prop Previous)" = 2026.10.06 ] && [ "$(prop Trial)" = 2026.10.07 ] && ok "  Previous 2026.10.06, trial 2026.10.07" || bad "  previous/trial"
$C mesa rollback | grep -q "Mesa 2026.10.06 is current again" && [ "$(prop Installed)" = 2026.10.06 ] && ok "rollback" || bad "rollback"
$WRAP env | grep -q "^OCL_ICD_VENDORS=$MD/icd/opencl$" && ok "wrapper env" || bad "wrapper env"
$C mesa off | grep -q "distribution's Mesa" && [ ! -s "$MD/env.conf" ] && ok "off" || bad "off"

[ $fail = 0 ] && echo "all ok" || { echo "FAILED"; tail -20 "$R/daemon.log"; }
exit $fail
