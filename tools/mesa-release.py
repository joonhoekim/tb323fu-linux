#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""mesa-release.py -- the asset set of a Mesa release on GitHub Releases, as the helper's
Mesa channel expects it (docs/notes/mesa-channel-design.md).

A Mesa release (a GitHub Release, tag mesa-VERSION, VERSION a date like 2026.10.06) carries:

    tb323fu-mesa-VERSION-aarch64.tar.gz   MANIFEST + tree/<relative path>: the drivers under lib/,
                                          licence texts under share/licenses/
    SHA256SUMS                            sha256sum of every file of the release
    the release body                      the notes (Markdown: what changed against upstream Mesa,
                                          with links to the merge requests); a line
                                          <!-- tb323fu: min_helper=X --> states needs

MANIFEST: format=1, version=, arch=aarch64, commit= (the port's Mesa commit), base= (the
upstream commit it is based on), mesa_version=, min_glibc= (the newest GLIBC_x.y symbol the
drivers use), drivers=vulkan,opencl, then one "file SHA256 MODE SIZE PATH" per file, PATH
relative (lib/... or share/...). The helper unpacks the tree into its own version directory
under /var/lib/tb323fu/mesa; nothing of the distribution is touched.

Build with tools/agent/mesa-dist-build.sh of the development repository (or the same meson
options: Turnip and rusticl, LLVM/clang/SPIR-V translator static, static-libclc=all, no GL) on
the Debian 13 tablet, the oldest glibc of the supported roots.

  mesa-release.py assets OUT --stage DIR [--version V] [--commit SHA] [--base SHA]
        [--mesa-version X] [--license NAME=FILE ...] [--no-strip]
        take the drivers from a DESTDIR install (DIR/usr/lib/...), strip them, write MANIFEST,
        the tarball and OUT/SHA256SUMS. Version default: today's date.
  mesa-release.py check OUT          what the helper checks: tarball name, SHA256SUMS, MANIFEST
                                     rules, every file's hash

Publishing (maintainer):
  gh release create mesa-VERSION OUT/* --prerelease --title "Mesa VERSION" --notes-file NOTES
"""
import argparse
import datetime
import gzip
import hashlib
import io
import os
import re
import subprocess
import sys
import tarfile
import tempfile

ARCH = "aarch64"
DRIVERS = {  # driver: (file in the DESTDIR install, path in the tree)
    "vulkan": ("usr/lib/libvulkan_freedreno.so", "lib/libvulkan_freedreno.so"),
    "opencl": ("usr/lib/libRusticlOpenCL.so.1.0.0", "lib/libRusticlOpenCL.so.1"),
}
SAFE_REL = re.compile(r"^(lib|share)(/[A-Za-z0-9._@+-]{1,100})+$")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def tarball_name(v):
    return f"tb323fu-mesa-{v}-{ARCH}.tar.gz"


def safe_version(v):
    return bool(re.fullmatch(r"[0-9][A-Za-z0-9.+-]{0,39}", v))


def glibc_of(data):
    best = None
    for m in re.finditer(rb"GLIBC_(2\.\d+(?:\.\d+)?)\x00", data):
        v = tuple(int(x) for x in m.group(1).decode().split("."))
        best = max(best or v, v)
    return best


def stripped(path, strip):
    if not strip:
        return open(path, "rb").read()
    with tempfile.NamedTemporaryFile(delete=False) as t:
        tmp = t.name
    try:
        subprocess.run(["strip", "--strip-unneeded", "-o", tmp, path], check=True)
        return open(tmp, "rb").read()
    finally:
        os.unlink(tmp)


def manifest(a, files, glibc):
    o = f"# tb323fu Mesa release manifest\nformat=1\nversion={a.version}\narch={ARCH}\n"
    for k, v in (("commit", a.commit), ("base", a.base), ("mesa_version", a.mesa_version)):
        if v:
            o += f"{k}={v}\n"
    if glibc:
        o += "min_glibc=" + ".".join(map(str, glibc)) + "\n"
    o += "drivers=" + ",".join(d for d in DRIVERS if any(p == DRIVERS[d][1] for p, _, _ in files)) + "\n"
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
        add("tree", b"", 0o755, tarfile.DIRTYPE)
        for p, data, mode in files:
            add("tree/" + p, data, mode)
    out = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", compresslevel=9, fileobj=out, mtime=0) as g:
        g.write(raw.getvalue())
    return out.getvalue()


def write_sums(out):
    names = sorted(n for n in os.listdir(out) if os.path.isfile(os.path.join(out, n)) and not n.startswith("SHA256SUMS"))
    with open(os.path.join(out, "SHA256SUMS"), "w") as f:
        for n in names:
            f.write(f"{sha(open(os.path.join(out, n), 'rb').read())}  {n}\n")


def cmd_assets(a):
    a.version = a.version or datetime.date.today().strftime("%Y.%m.%d")
    if not safe_version(a.version):
        sys.exit(f"bad version {a.version!r}")
    files = []
    for d, (src, dst) in DRIVERS.items():
        p = os.path.join(a.stage, src)
        if os.path.isfile(p):
            files.append((dst, stripped(p, not a.no_strip), 0o755))
        else:
            print(f"note: no {src} in the stage, release without {d}")
    if not files:
        sys.exit("no driver in the stage")
    for lic in a.license or []:
        name, _, path = lic.partition("=")
        files.append((f"share/licenses/{name}", open(path, "rb").read(), 0o644))
    for p, _, _ in files:
        if not SAFE_REL.match(p):
            sys.exit(f"unsafe path {p}")
    glibc = None
    for p, data, _ in files:
        g = glibc_of(data)
        if g:
            glibc = max(glibc or g, g)
    mtext = manifest(a, files, glibc)
    os.makedirs(a.out, exist_ok=True)
    tgz = tar_gz(mtext, files)
    open(os.path.join(a.out, tarball_name(a.version)), "wb").write(tgz)
    write_sums(a.out)
    print(f"{tarball_name(a.version)}: {len(tgz) / 1e6:.1f} MB, {len(files)} files, min_glibc {'.'.join(map(str, glibc)) if glibc else '-'}")
    print(mtext.split("\nfile ")[0])


def cmd_check(a):
    names = os.listdir(a.out)
    tb = [n for n in names if re.fullmatch(rf"tb323fu-mesa-(.+)-{ARCH}\.tar\.gz", n)]
    if len(tb) != 1:
        sys.exit(f"want exactly one tarball, have {tb}")
    version = re.fullmatch(rf"tb323fu-mesa-(.+)-{ARCH}\.tar\.gz", tb[0]).group(1)
    sums = {}
    for l in open(os.path.join(a.out, "SHA256SUMS")):
        h, _, n = l.strip().partition("  ")
        sums[n] = h
    data = open(os.path.join(a.out, tb[0]), "rb").read()
    if sums.get(tb[0]) != sha(data):
        sys.exit("tarball does not match SHA256SUMS")
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as t:
        mem = {m.name: m for m in t.getmembers()}
        m = t.extractfile("MANIFEST").read().decode()
        if not re.search(rf"^version={re.escape(version)}$", m, re.M) or "format=1" not in m:
            sys.exit("MANIFEST version/format")
        drivers = re.search(r"^drivers=(.*)$", m, re.M).group(1).split(",")
        listed = set()
        for l in m.splitlines():
            if not l.startswith("file "):
                continue
            _, h, mode, size, p = l.split(" ")
            if not SAFE_REL.match(p) or mode not in ("644", "755"):
                sys.exit(f"MANIFEST entry {l}")
            d = t.extractfile("tree/" + p).read()
            if sha(d) != h or len(d) != int(size):
                sys.exit(f"{p} does not match MANIFEST")
            listed.add(p)
        for d in drivers:
            if DRIVERS[d][1] not in listed:
                sys.exit(f"drivers={d} but no {DRIVERS[d][1]}")
        extra = {n[5:] for n, x in mem.items() if n.startswith("tree/") and x.isfile()} - listed
        if extra:
            sys.exit(f"files not in MANIFEST: {sorted(extra)}")
    print(f"ok: Mesa {version}, drivers {','.join(drivers)}, {len(listed)} files")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    s = p.add_subparsers(dest="cmd", required=True)
    x = s.add_parser("assets")
    x.add_argument("out")
    x.add_argument("--stage", required=True)
    x.add_argument("--version")
    x.add_argument("--commit", default="")
    x.add_argument("--base", default="")
    x.add_argument("--mesa-version", default="")
    x.add_argument("--license", action="append")
    x.add_argument("--no-strip", action="store_true")
    x = s.add_parser("check")
    x.add_argument("out")
    a = p.parse_args()
    {"assets": cmd_assets, "check": cmd_check}[a.cmd](a)


if __name__ == "__main__":
    main()
