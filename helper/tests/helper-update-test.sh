#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Joonhoe Kim
# helper-update-test.sh -- the helper's self-update end to end, without a
# tablet: the "installed" helper is helper/install.sh into a fake root
# (DESTDIR), releases are made with tools/helper-release.py and published on a
# file:// stand-in of the GitHub API (kernel-release.py fake-api), the daemon
# runs on a private session bus (no polkit) from the fake root, and the
# restart is a script that starts whatever daemon the fake root holds now.
# Covers: check, package-manager and NixOS refusal, damaged download, required
# signature, min_platform, update + health check, rollback, a release whose
# daemon does not start (automatic rollback), paths outside the allow-list.
#
#   dbus-run-session -- sh tests/helper-update-test.sh [BINDIR]     # default target/release
# Needs: dbus-run-session, python3, curl.
set -u
here=$(cd "$(dirname "$0")" && pwd)
V0=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/../Cargo.toml" | head -1)   # the helper under test
V1=${V0%.*}.$((${V0##*.} + 1))-test                                         # the release it updates to
V2=${V0%.*}.$((${V0##*.} + 2))-test; V3=${V0%.*}.$((${V0##*.} + 3))-test
repo=$(cd "$here/../.." && pwd)
BIN=$(cd "${1:-target/release}" && pwd)
R=$(mktemp -d /tmp/tb323fu-hupd.XXXXXX)
API=$R/api
export TB323FU_SYSFS_ROOT=$R TB323FU_CONFIG=$R/etc/tb323fu/helper.toml TB323FU_CURRENT_ROOT=baldur-root
export TB323FU_KERNEL_FETCH=$here/../data/tb323fu-kernel-fetch
export TB323FU_HELPER_RESTART="sh $R/restart.sh" TB323FU_HEALTH_SECS=6
unset TB323FU_TEST_VERSION
fail=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fail=1; }
mk() { mkdir -p "$(dirname "$R$1")"; printf '%s\n' "$2" > "$R$1"; }
C="$BIN/tb323fu-ctl --session"
K="python3 $repo/tools/kernel-release.py"
H="python3 $repo/tools/helper-release.py"
HD=/var/lib/tb323fu/helper
D=/usr/local/libexec/tb323fu-helperd

# package managers: a fake dpkg-query that owns the helper while $R/owned exists
mkdir -p "$R/fakebin"
printf '#!/bin/sh\n[ -e %s/owned ] && { echo "tb323fu-helper: $2"; exit 0; }\nexit 1\n' "$R" > "$R/fakebin/dpkg-query"
chmod +x "$R/fakebin/dpkg-query"
export PATH="$R/fakebin:$PATH"

# the restart: stop the running daemon, start the one in the fake root
cat > "$R/restart.sh" <<EOF
old=\$(cat "$R/daemon.pid" 2>/dev/null)
if [ -n "\$old" ]; then
	kill "\$old" 2>/dev/null
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do kill -0 "\$old" 2>/dev/null || break; sleep 0.1; done
fi
env -u TB323FU_TEST_VERSION "$R$D" --session --no-polkit >> "$R/daemon.log" 2>&1 &
echo \$! > "$R/daemon.pid"
EOF
start() {
	sh "$R/restart.sh"
	for i in $(seq 30); do $C versions > /dev/null 2>&1 && break; sleep 0.3; done
}
stop() { kill "$(cat "$R/daemon.pid" 2>/dev/null)" 2>/dev/null; sleep 0.3; }
trap 'stop; rm -rf "$R"' EXIT
cfg() {
	printf '[kernel]\nsource = "github:o/r"\napi_url = "file://%s"\nauto_check = false\n%s\n' "$API" "${1:-}" > "$R/etc/tb323fu/helper.toml"
}
ver() { $C --json versions 2>/dev/null | sed -n 's/.*"Version": "\([^"]*\)".*/\1/p'; }

# a release made from a staged install whose daemon is a wrapper: VERSION, BODY of the wrapper
release() { # VERSION OUT WRAPPER-BODY
	S=$R/stage-$1
	rm -rf "$S"
	DESTDIR=$S PREFIX=/usr/local BUILD_DIR=$BIN sh "$here/../install.sh" > /dev/null
	printf '#!/bin/sh\n%s\n' "$3" > "$S$D"
	$H assets "$2" --stage "$S" --version "$1" --any-arch --commit test > /dev/null
}

# the installed helper $V0 (install.sh, /usr/local), and files that are not the helper's
mkdir -p "$R/etc/tb323fu" "$R/var/cache/tb323fu-kernel"
DESTDIR=$R PREFIX=/usr/local BUILD_DIR=$BIN sh "$here/../install.sh" > /dev/null
mk /usr/local/bin/other-tool "not the helper's"
mk /etc/systemd/system/other.service "[Unit]"
mk /usr/share/polkit-1/actions/other.policy "<policyconfig/>"
cfg
release $V1 "$R/rel" "TB323FU_TEST_VERSION=$V1 exec $BIN/tb323fu-helperd \"\$@\"" &&
	$H check "$R/rel" > "$R/chk" && ok "helper-release.py assets + check: $(cat "$R/chk")" || { bad "helper-release.py"; cat "$R/chk"; }
gzip -dc "$R/rel/tb323fu-helper-$V1-aarch64.tar.gz" | tar -t | grep -q '^root/usr/share/polkit-1/actions/io.github.joonhoekim.opendevicehelper.policy$' &&
	ok "  the tarball carries the polkit policy (install.sh's tree)" || bad "  tarball contents"
cp "$R/rel/SHA256SUMS" "$R/sums.bak"
printf 'deb\n' > "$R/rel/tb323fu-helper_${V1}_arm64.deb"   # an extra asset (packages) is ignored
( cd "$R/rel" && sha256sum tb323fu-helper-$V1-aarch64.tar.gz tb323fu-helper_${V1}_arm64.deb > SHA256SUMS )
printf '## helper %s\n- self-update test\n' "$V1" > "$R/notes.md"
$K fake-api "$API" --repo o/r --helper $V1 --dir "$R/rel" --notes "$R/notes.md" > /dev/null && ok "fake-api: helper release with assets" || bad "fake-api"

start
[ "$(ver)" = $V0 ] && ok "running $V0 from the fake root" || { bad "start: $(ver)"; tail "$R/daemon.log"; }
$C helper | grep -q "^updates    self-update from GitHub Releases" && ok "status: self-update (no package owns the helper)" || { bad "status"; $C helper; }
$C helper check | grep -q "helper $V1 available (tb323fu-ctl helper update)" && ok "check: $V1 available" || { bad "check"; $C helper check; }
$C helper notes | grep -q "self-update test" && ok "notes" || bad "notes"
$C --json helper | grep -q '"Installable": true' && ok "  Installable" || bad "  Installable"

# owned by a package manager, NixOS: notify only
touch "$R/owned"
$C helper check | grep -q "Debian package tb323fu-helper" && ok "owned by dpkg: check names the package" || bad "dpkg check"
$C helper update > "$R/out" 2>&1; rc=$?
[ $rc != 0 ] && grep -q "apt install ./tb323fu-helper_" "$R/out" && ok "  update refused, with the package hint" || { bad "  update when owned"; cat "$R/out"; }
$C helper download > /dev/null 2>&1; $C helper install 2>&1 | grep -q "installed by dpkg (package tb323fu-helper)" && ok "  Install refused by the daemon" || bad "  Install when owned"
rm -f "$R/owned"; mk /etc/NIXOS ""
$C helper check | grep -q "nixos-rebuild" && $C --json helper | grep -q '"Method": "nix"' && ok "NixOS: the flake command, method nix" || bad "nixos"
rm -f "$R/etc/NIXOS"; $C helper check > /dev/null
rm -rf "$R$HD/staged"

# damaged download, required signature
asset=$(python3 -c "import json;r=json.load(open('$API/repos/o/r/releases'));print([a['id'] for a in r[0]['assets'] if a['name'].endswith('.tar.gz')][0])")
cp "$API/assets/$asset" "$R/asset.bak"
printf 'x' | dd of="$API/assets/$asset" bs=1 seek=200 conv=notrunc 2>/dev/null
$C helper download 2>&1 | grep -q "does not match SHA256SUMS" && ok "a damaged download is refused (SHA256SUMS)" || bad "damaged download accepted"
cp "$R/asset.bak" "$API/assets/$asset"
cfg 'require_signature = true'; $C reload > /dev/null 2>&1
$C helper download 2>&1 | grep -q "SHA256SUMS.minisig" && ok "require_signature: refused without SHA256SUMS.minisig" || bad "unsigned accepted"
cfg; $C reload > /dev/null 2>&1

$C helper download | grep -q "helper $V1 downloaded and checked" && ok "download + check (SHA256SUMS, digest, MANIFEST, every file)" || { bad "download"; cat "$R/var/cache/tb323fu-kernel/fetch.log"; }
$C --json helper | grep -q '"State": "ready"' && ok "  state ready" || bad "  state ready"

# min_platform in the notes
printf '## x\n<!-- tb323fu: min_platform=9.9 -->\n' > "$R/notes-min.md"
$K fake-api "$API" --repo o/r --helper $V1 --dir "$R/rel" --notes "$R/notes-min.md" > /dev/null
mk /usr/share/tb323fu/platform-version 0.2.0
$C helper check > /dev/null
$C helper download > /dev/null
$C helper update 2>&1 | grep -q "needs tb323fu-platform 9.9" && ok "min_platform not met: install refused" || bad "min_platform"
[ "$(ver)" = $V0 ] && ok "  still $V0" || bad "  version changed"
$K fake-api "$API" --repo o/r --helper $V1 --dir "$R/rel" --notes "$R/notes.md" > /dev/null
$C helper check > /dev/null

# update, health check
before_other=$(sha256sum "$R/usr/local/bin/other-tool" "$R/etc/systemd/system/other.service" "$R/usr/share/polkit-1/actions/other.policy")
$C helper update > "$R/out" 2>&1 && grep -q "helper $V1 is running" "$R/out" && ok "update: installed, restarted, health check passed" || { bad "update"; cat "$R/out"; tail -20 "$R/daemon.log"; cat "$R$HD/update.log"; }
[ "$(ver)" = $V1 ] && ok "  Version $V1" || bad "  version $(ver)"
cmp -s "$R$D" "$R/stage-$V1$D" && cmp -s "$R/usr/local/bin/tb323fu-ctl" "$R/stage-$V1/usr/local/bin/tb323fu-ctl" && ok "  files = the release tree" || bad "  files"
[ "$before_other" = "$(sha256sum "$R/usr/local/bin/other-tool" "$R/etc/systemd/system/other.service" "$R/usr/share/polkit-1/actions/other.policy")" ] && ok "  files outside the manifest untouched" || bad "  other files changed"
grep -q "^version=$V0$" "$R$HD/prev/MANIFEST" && cmp -s "$R$HD/prev/root$D" "$BIN/tb323fu-helperd" && ok "  previous version kept ($HD/prev, version $V0)" || bad "  prev"
grep -q "^version=$V1$" "$R$HD/current.manifest" && ok "  current.manifest" || bad "  current.manifest"
[ -z "$(find "$R" -name '*.tb323fu-new')" ] && ok "  no staged leftovers" || bad "  leftovers"
$C helper | grep -q "^previous   $V0" && $C helper | grep -q "^last       update $V0 -> $V1: ok" && ok "  status: previous and last update" || { bad "  status"; $C helper; }

# rollback
$C helper rollback > "$R/out" 2>&1 && grep -q "helper $V0 is running" "$R/out" && ok "rollback: back on $V0" || { bad "rollback"; cat "$R/out"; cat "$R$HD/update.log"; }
[ "$(ver)" = $V0 ] && cmp -s "$R$D" "$BIN/tb323fu-helperd" && ok "  Version $V0, the original daemon file" || bad "  after rollback"
grep -q "^version=$V1$" "$R$HD/prev/MANIFEST" && ok "  prev now holds $V1 (rollback is reversible)" || bad "  prev after rollback"

# a release whose daemon does not start: the health check fails, the old one comes back
release $V2 "$R/rel2" "exit 1"
$K fake-api "$API" --repo o/r --helper $V2 --dir "$R/rel2" > /dev/null
$C helper update > "$R/out" 2>&1; rc=$?
[ $rc != 0 ] && grep -q "rolled-back" "$R/out" && ok "broken release: update reports rolled-back" || { bad "broken release"; cat "$R/out"; }
[ "$(ver)" = $V0 ] && cmp -s "$R$D" "$BIN/tb323fu-helperd" && ok "  back on $V0, the original daemon file" || { bad "  not restored: $(ver)"; cat "$R$HD/update.log"; }
$C --json helper | grep -q "\"reason\": \"no answer as helper $V2" && ok "  LastUpdate says why" || { bad "  reason"; $C helper; }
grep -q "^version=$V1$" "$R$HD/prev/MANIFEST" && ok "  prev unchanged by the failed update" || bad "  prev changed"

# paths outside the allow-list
release $V3 "$R/rel3" "exit 0"
mkdir -p "$R/stage-$V3/etc/cron.d"; echo x > "$R/stage-$V3/etc/cron.d/x"
$H assets "$R/rel3b" --stage "$R/stage-$V3" --version $V3 --any-arch 2>&1 | grep -q "may not install" && ok "helper-release.py refuses a path outside the allow-list" || bad "builder allow-list"
python3 - "$R/rel3" "$V3" <<'EOF'
import gzip, hashlib, io, os, sys, tarfile
d = sys.argv[1]
n = f"tb323fu-helper-{sys.argv[2]}-aarch64.tar.gz"
src = tarfile.open(os.path.join(d, n), "r:gz")
out = io.BytesIO()
with tarfile.open(fileobj=out, mode="w", format=tarfile.USTAR_FORMAT) as t:
    for m in src.getmembers():
        data = src.extractfile(m).read() if m.isfile() else None
        if m.name == "MANIFEST":
            data += b"file " + hashlib.sha256(b"x\n").hexdigest().encode() + b" 644 2 /etc/cron.d/x\n"
            m.size = len(data)
        t.addfile(m, io.BytesIO(data) if data is not None else None)
    ti = tarfile.TarInfo("root/etc/cron.d/x"); ti.size = 2
    t.addfile(ti, io.BytesIO(b"x\n"))
open(os.path.join(d, n), "wb").write(gzip.compress(out.getvalue()))
h = hashlib.sha256(open(os.path.join(d, n), "rb").read()).hexdigest()
open(os.path.join(d, "SHA256SUMS"), "w").write(f"{h}  {n}\n")
EOF
$H check "$R/rel3" > /dev/null 2>&1 && bad "check accepted /etc/cron.d" || ok "helper-release.py check refuses it"
$K fake-api "$API" --repo o/r --helper $V3 --dir "$R/rel3" > /dev/null
$C helper check > /dev/null
$C helper download $V3 2>&1 | grep -q "/etc/cron.d/x, which a helper release may not install" && ok "the daemon refuses it at download" || bad "daemon allow-list"
[ ! -e "$R/etc/cron.d/x" ] && ok "  nothing written there" || bad "  /etc/cron.d/x written"

[ $fail = 0 ] && echo "ALL PASSED" || { echo "SOME FAILED"; tail -30 "$R/daemon.log"; }
exit $fail
