#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""helper-release.py -- the asset set of an Open Device Helper release on GitHub
Releases, as the helper's self-update expects it (docs/helper.md "Helper updates").

A helper release (a GitHub Release, tag helper-vX.Y.Z) carries:

    tb323fu-helper-X.Y.Z-aarch64.tar.gz   MANIFEST + root/<path>: what helper/install.sh,
                                          crates/tb323fu-settings/install.sh and the GNOME
                                          extension install with PREFIX=/usr/local, plus
                                          COPYING and THIRD-PARTY-LICENSES (Rust crates,
                                          tools/third-party-licenses.py) in share/doc
    SHA256SUMS                            sha256sum of every file of the release
    SHA256SUMS.minisig                    optional (helpers with kernel.require_signature)
    the release body                      the notes (Markdown); a line
                                          <!-- tb323fu: min_kernel=tNN min_platform=X --> states needs
    anything else                         ignored by the helper (e.g. the Debian packages)

MANIFEST: format=1, version=, arch=aarch64, prefix=/usr/local, min_glibc= (the newest
GLIBC_x.y symbol the binaries use), commit=, then one "file SHA256 MODE SIZE PATH" per
file. The helper installs only the paths this script allows (the same list as the
helper's helperupdate.rs).

Build on the oldest system the release is for -- the Debian 13 (trixie, glibc 2.41)
tablet: binaries built against a newer glibc do not start on an older one (the
helper refuses a release whose min_glibc is newer than the system's).

  helper-release.py assets OUT [--version V] [--build-dir D] [--settings-build-dir D]
        [--no-settings] [--no-extension] [--stage DIR] [--commit SHA] [--any-arch]
        stage the install (after `cargo build --release` in helper/ and in
        helper/crates/tb323fu-settings/) into a temporary DESTDIR, write MANIFEST, the
        tarball and OUT/SHA256SUMS (over every file in OUT). --stage DIR takes an
        already staged tree instead. Version default: helper/Cargo.toml.
  helper-release.py check OUT         what the helper checks: tarball name, SHA256SUMS,
                                      MANIFEST, allowed paths, every file's hash

Publishing (maintainer):
  gh release create helper-vX.Y.Z OUT/* --title "Open Device Helper X.Y.Z" --notes-file NOTES
Test stand-in for the GitHub API: kernel-release.py fake-api ROOT --repo O/R --helper X.Y.Z --dir OUT
"""
import argparse
import gzip
import hashlib
import io
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
ARCH = "aarch64"
PREFIX = "/usr/local"
UUID = "tb323fu@joonhoekim.github.io"
ALLOWED_FILES = {
    "/usr/local/libexec/tb323fu-helperd",
    "/usr/local/libexec/tb323fu-kernel-fetch",
    "/usr/local/bin/tb323fu-ctl",
    "/usr/local/bin/tb323fu-settings",
    "/etc/systemd/system/tb323fu-helperd.service",
    "/etc/systemd/system/tb323fu-kernel-fetch.service",
    "/etc/dbus-1/system.d/io.github.joonhoekim.OpenDeviceHelper1.conf",
    "/usr/share/dbus-1/system-services/io.github.joonhoekim.OpenDeviceHelper1.service",
    "/usr/share/polkit-1/actions/io.github.joonhoekim.opendevicehelper.policy",
    "/usr/local/share/applications/io.github.joonhoekim.OpenDeviceHelper.desktop",
    "/usr/local/share/icons/hicolor/scalable/apps/io.github.joonhoekim.OpenDeviceHelper.svg",
    "/usr/local/share/metainfo/io.github.joonhoekim.OpenDeviceHelper.metainfo.xml",
}
ALLOWED_DIRS = ("/usr/local/share/doc/tb323fu-helper/", f"/usr/local/share/gnome-shell/extensions/{UUID}/")
REQUIRED = ("/usr/local/libexec/tb323fu-helperd", "/usr/local/bin/tb323fu-ctl", "/etc/systemd/system/tb323fu-helperd.service")
COMPONENT = re.compile(r"^[A-Za-z0-9._@+-]{1,100}$")
VERSION = re.compile(r"^[0-9][A-Za-z0-9.+-]{0,39}$")


def allowed(p):
    parts = p[1:].split("/")
    if not p.startswith("/") or len(p) > 240 or any(c in ("", ".", "..") or not COMPONENT.match(c) for c in parts):
        return False
    return p in ALLOWED_FILES or any(p.startswith(d) and len(p) > len(d) for d in ALLOWED_DIRS)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def tarball_name(v):
    return f"tb323fu-helper-{v}-{ARCH}.tar.gz"


def cargo_version():
    txt = open(os.path.join(REPO, "helper", "Cargo.toml"), encoding="utf-8").read()
    m = re.search(r'\[workspace\.package\][^\[]*?\nversion\s*=\s*"([^"]+)"', txt)
    if not m:
        sys.exit("no [workspace.package] version in helper/Cargo.toml")
    return m.group(1)


def glibc_of(data):
    best = None
    for m in re.finditer(rb"GLIBC_(2\.\d+(?:\.\d+)?)\x00", data):
        v = tuple(int(x) for x in m.group(1).decode().split("."))
        best = max(best or v, v)
    return best


def stage(a, dest):
    env = dict(os.environ, DESTDIR=dest, PREFIX=PREFIX)
    if a.build_dir:
        env["BUILD_DIR"] = os.path.abspath(a.build_dir)
    subprocess.run(["sh", os.path.join(REPO, "helper", "install.sh")], env=env, check=True, stdout=subprocess.DEVNULL)
    if not a.no_settings:
        env.pop("BUILD_DIR", None)
        if a.settings_build_dir:
            env["BUILD_DIR"] = os.path.abspath(a.settings_build_dir)
        subprocess.run(["sh", os.path.join(REPO, "helper", "crates", "tb323fu-settings", "install.sh")], env=env, check=True, stdout=subprocess.DEVNULL)
    doc = os.path.join(dest, "usr", "local", "share", "doc", "tb323fu-helper")
    os.makedirs(doc, exist_ok=True)
    shutil.copyfile(os.path.join(REPO, "LICENSES", "GPL-3.0-or-later.txt"), os.path.join(doc, "COPYING"))
    workspaces = [os.path.join(REPO, "helper")]
    if not a.no_settings:
        workspaces.append(os.path.join(REPO, "helper", "crates", "tb323fu-settings"))
    subprocess.run([sys.executable, os.path.join(HERE, "third-party-licenses.py"), os.path.join(doc, "THIRD-PARTY-LICENSES"), *workspaces],
                   check=True, stdout=subprocess.DEVNULL)
    for n in ("COPYING", "THIRD-PARTY-LICENSES"):
        os.chmod(os.path.join(doc, n), 0o644)
    if not a.no_extension:
        src = os.path.join(REPO, "userspace", "desktop", "gnome", "extension", UUID)
        dst = os.path.join(dest, "usr", "local", "share", "gnome-shell", "extensions", UUID)
        os.makedirs(dst, exist_ok=True)
        for n in sorted(os.listdir(src)):
            if os.path.isfile(os.path.join(src, n)):
                shutil.copyfile(os.path.join(src, n), os.path.join(dst, n))
                os.chmod(os.path.join(dst, n), 0o644)


def collect(tree, any_arch):
    files, glibc = [], None
    for d, dirs, names in os.walk(tree):
        dirs.sort()
        for n in sorted(names):
            full = os.path.join(d, n)
            p = "/" + os.path.relpath(full, tree).replace(os.sep, "/")
            if os.path.islink(full) or not os.path.isfile(full):
                sys.exit(f"{p}: not a regular file")
            if not allowed(p):
                sys.exit(f"{p}: a helper release may not install this path (helperupdate.rs ALLOWED_FILES/ALLOWED_DIRS)")
            data = open(full, "rb").read()
            mode = 0o755 if os.stat(full).st_mode & 0o111 else 0o644
            if data[:4] == b"\x7fELF":
                machine = int.from_bytes(data[18:20], "little")
                if machine != 183 and not any_arch:
                    sys.exit(f"{p}: not an aarch64 binary (ELF machine {machine})")
                g = glibc_of(data)
                glibc = max(glibc or g, g) if g else glibc
            files.append((p, data, mode))
    files.sort()
    for r in REQUIRED:
        if r not in [f[0] for f in files]:
            sys.exit(f"{r} is missing from the staged tree (build first: cargo build --release)")
    return files, glibc


def manifest(version, files, glibc, commit):
    o = f"# tb323fu-helper release manifest\nformat=1\nversion={version}\narch={ARCH}\nprefix={PREFIX}\n"
    if glibc:
        o += "min_glibc=" + ".".join(map(str, glibc)) + "\n"
    if commit:
        o += f"commit={commit}\n"
    for p, data, mode in files:
        o += f"file {sha(data)} {mode:o} {len(data)} {p}\n"
    return o


def tar_gz(mtext, files):
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as t:
        def add(name, data, mode, typ=tarfile.REGTYPE):
            ti = tarfile.TarInfo(name)
            ti.type, ti.mode, ti.mtime, ti.uid, ti.gid, ti.uname, ti.gname = typ, mode, epoch, 0, 0, "root", "root"
            ti.size = len(data) if typ == tarfile.REGTYPE else 0
            t.addfile(ti, io.BytesIO(data) if typ == tarfile.REGTYPE else None)
        add("MANIFEST", mtext.encode(), 0o644)
        add("root", b"", 0o755, tarfile.DIRTYPE)
        for p, data, mode in files:
            add("root" + p, data, mode)
    out = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", compresslevel=9, fileobj=out, mtime=0) as g:
        g.write(raw.getvalue())
    return out.getvalue()


def write_sums(out):
    names = sorted(n for n in os.listdir(out) if os.path.isfile(os.path.join(out, n)) and not n.startswith("SHA256SUMS"))
    with open(os.path.join(out, "SHA256SUMS"), "w") as f:
        for n in names:
            f.write(f"{sha(open(os.path.join(out, n), 'rb').read())}  {n}\n")
    sig = os.path.join(out, "SHA256SUMS.minisig")
    if os.path.exists(sig):
        os.remove(sig)
        print("SHA256SUMS changed: removed the old SHA256SUMS.minisig (sign again if you sign)")


def cmd_assets(a):
    version = a.version or cargo_version()
    if not VERSION.match(version):
        sys.exit(f"{version}: not a version")
    if a.version and a.version != cargo_version():
        print(f"note: --version {a.version}, helper/Cargo.toml says {cargo_version()} (the daemon reports the Cargo version)")
    commit = a.commit
    if commit is None:
        try:
            r = subprocess.run(["git", "-C", REPO, "rev-parse", "HEAD"], capture_output=True, text=True)
            commit = r.stdout.strip() if r.returncode == 0 else ""
        except FileNotFoundError:
            commit = ""
    with tempfile.TemporaryDirectory() as tmp:
        tree = a.stage or os.path.join(tmp, "stage")
        if not a.stage:
            stage(a, tree)
        files, glibc = collect(tree, a.any_arch)
    mtext = manifest(version, files, glibc, commit)
    os.makedirs(a.out, exist_ok=True)
    for n in os.listdir(a.out):
        if re.match(r"^tb323fu-helper-.*-aarch64\.tar\.gz$", n):
            os.remove(os.path.join(a.out, n))
    name = tarball_name(version)
    open(os.path.join(a.out, name), "wb").write(tar_gz(mtext, files))
    write_sums(a.out)
    print(f"{name}: {len(files)} files, min_glibc {'.'.join(map(str, glibc)) if glibc else '-'}")
    print(f"publish: gh release create helper-v{version} {a.out}/* --title \"Open Device Helper {version}\" --notes-file NOTES")


def cmd_check(a):
    bad = 0
    tbs = [n for n in sorted(os.listdir(a.out)) if re.match(r"^tb323fu-helper-(.+)-aarch64\.tar\.gz$", n)]
    if len(tbs) != 1:
        sys.exit(f"want one tb323fu-helper-X.Y.Z-aarch64.tar.gz, found {tbs or 'none'}")
    name = tbs[0]
    version = re.match(r"^tb323fu-helper-(.+)-aarch64\.tar\.gz$", name).group(1)
    data = open(os.path.join(a.out, name), "rb").read()
    sums = {}
    try:
        for l in open(os.path.join(a.out, "SHA256SUMS")):
            h, _, n = l.rstrip("\n").partition(" ")
            sums[n.lstrip(" *")] = h
    except FileNotFoundError:
        sys.exit("no SHA256SUMS")
    if sums.get(name) != sha(data):
        print(f"FAIL {name}: missing from SHA256SUMS or a different hash"); bad = 1
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as t:
        members = t.getmembers()
        if any(m.pax_headers for m in members):
            print("FAIL the tarball is not plain ustar"); bad = 1
        content = {}
        for m in members:
            if m.isdir():
                continue
            if not m.isfile():
                print(f"FAIL {m.name}: not a regular file"); bad = 1
                continue
            content[m.name] = t.extractfile(m).read()
    mtext = content.pop("MANIFEST", None)
    if mtext is None:
        sys.exit("FAIL no MANIFEST in the tarball")
    head, listed = {}, {}
    for l in mtext.decode().splitlines():
        if not l or l.startswith("#"):
            continue
        if "=" in l.split(" ")[0]:
            k, _, v = l.partition("=")
            head[k] = v
            continue
        f = l.split(" ")
        if len(f) != 5 or f[0] != "file":
            print(f"FAIL MANIFEST line: {l}"); bad = 1
            continue
        listed[f[4]] = (f[1], int(f[2], 8), int(f[3]))
    if head.get("format") != "1" or head.get("version") != version or head.get("arch") != ARCH or head.get("prefix") != PREFIX:
        print(f"FAIL MANIFEST header {head} (want format=1, version={version}, arch={ARCH}, prefix={PREFIX})"); bad = 1
    for r in REQUIRED:
        if r not in listed:
            print(f"FAIL {r} is not in the release"); bad = 1
    have = {"/" + n[len("root/"):]: d for n, d in content.items() if n.startswith("root/")}
    for n in content:
        if not n.startswith("root/"):
            print(f"FAIL {n}: outside root/"); bad = 1
    for p in sorted(set(listed) | set(have)):
        if not allowed(p):
            print(f"FAIL {p}: not a path a helper release may install"); bad = 1
        elif p not in have or p not in listed:
            print(f"FAIL {p}: in {'MANIFEST' if p in listed else 'the tarball'} only"); bad = 1
        elif sha(have[p]) != listed[p][0] or len(have[p]) != listed[p][2] or listed[p][1] not in (0o644, 0o755):
            print(f"FAIL {p}: does not match MANIFEST"); bad = 1
    print(f"{'ok  ' if not bad else 'FAIL'} {name}: helper {version}, {len(listed)} files, min_glibc {head.get('min_glibc', '-')}, commit {head.get('commit', '-')[:12]}")
    sys.exit(bad)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    s = p.add_subparsers(dest="cmd", required=True)
    x = s.add_parser("assets"); x.add_argument("out"); x.add_argument("--version"); x.add_argument("--build-dir"); x.add_argument("--settings-build-dir")
    x.add_argument("--no-settings", action="store_true"); x.add_argument("--no-extension", action="store_true"); x.add_argument("--stage")
    x.add_argument("--commit"); x.add_argument("--any-arch", action="store_true", help="tests: binaries of another architecture")
    x = s.add_parser("check"); x.add_argument("out")
    a = p.parse_args()
    {"assets": cmd_assets, "check": cmd_check}[a.cmd](a)


if __name__ == "__main__":
    main()
