#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""kernel-release.py -- the asset set of a kernel release on GitHub Releases,
as the helper's kernel updates expect it (docs/helper.md "Kernel updates",
docs/custom-kernel.md), and a local stand-in for the GitHub API for tests.

A kernel release (a GitHub Release, tag e.g. kernel-t31) carries:

    Image-tb323fu-tNN          the raw arm64 Image (DT, initramfs, modules built in)
    Image-tb323fu-tNN.gz       the same, gzip -9 (optional; the helper downloads this one)
    SHA256SUMS                 sha256sum of every file of the release
    SHA256SUMS.minisig         optional: a minisign signature over SHA256SUMS, only
                               for helpers with kernel.require_signature = true
    the release body           the release notes (Markdown); a line
                               <!-- tb323fu: min_helper=X min_platform=Y --> states needs

The kernel's release string must end in -tb323fu-tNN (CONFIG_LOCALVERSION); the
helper refuses an asset whose kernel says otherwise. A pre-release is offered
on the testing channel only. SHA256SUMS travels with the files, so it catches
damaged downloads, not a changed release.

  kernel-release.py assets OUT IMAGE [--gzip] [--tag tNN]
        copy IMAGE (an Image or Image.gz) to OUT as Image-tb323fu-tNN (and .gz), then
        rewrite OUT/SHA256SUMS over every file in OUT
  kernel-release.py sums OUT              rewrite SHA256SUMS over every file in OUT
  kernel-release.py check OUT             what the helper will check: one tNN, sums, banners
  kernel-release.py sign OUT --key KEY    optional: minisign -S over SHA256SUMS
  kernel-release.py verify OUT [--pub P]  sha256sum -c (and minisign -V with --pub)
  kernel-release.py fake-api ROOT --repo O/R --dir OUT --tag kernel-tNN [--prerelease]
        [--title T] [--notes FILE] [--base URL] [--no-digest]
  kernel-release.py fake-api ROOT --repo O/R --helper 0.2.0 [--base URL]
        add a release to a local stand-in of the GitHub REST API: ROOT/repos/O/R/releases
        (the release list, newest first) and ROOT/assets/<id>; asset URLs are
        BASE/assets/<id> (BASE default file://ROOT). Point the helper at it with
        [kernel] api_url = "file://ROOT" (or http://HOST:PORT for `python3 -m http.server`
        run in ROOT) and source = "github:O/R".

Publishing (maintainer, or a contributor on a fork):
  gh release create kernel-tNN OUT/* --title "tb323fu-linux tNN" --notes-file NOTES [--prerelease]
"""
import argparse
import datetime
import gzip
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

SAFE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,95}$")
KERNEL = re.compile(r"^Image-tb323fu-t(\d+)(\.gz)?$")


def banner(raw):
    """(release, /proc/version line). An Image carries the banner twice: a
    placeholder from init/version.o with an empty build number ("# SMP ")
    and the real one ("#7 SMP ..."); the numbered one wins."""
    found = []
    i = raw.find(b"Linux version ")
    while i >= 0:
        end = raw.find(b"\n", i)
        line = raw[i:end].decode("ascii", "replace")
        parts = line.split(" ")
        if end > i and len(parts) > 3 and parts[3].startswith("("):
            found.append((parts[2], line))
        i = raw.find(b"Linux version ", i + 1)
    if not found:
        sys.exit("no 'Linux version' banner in the kernel")
    numbered = [f for f in found if re.search(r" #\d", f[1])]
    return (numbered or found)[0]


def raw_image(data):
    if data[56:60] == b"ARM\x64":
        return data
    if data[:2] == b"\x1f\x8b":
        raw = gzip.decompress(data)
        if raw[56:60] != b"ARM\x64":
            sys.exit("the gzip stream is not an arm64 Image")
        return raw
    if data[:4] in (b"\x02\x21\x4c\x18", b"\x04\x22\x4d\x18"):
        sys.exit("LZ4 kernels do not boot on this tablet (the bootloader has gzip only)")
    sys.exit("not a raw arm64 Image or an Image.gz")


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def files(out):
    return sorted(n for n in os.listdir(out) if os.path.isfile(os.path.join(out, n)) and not n.startswith("SHA256SUMS"))


def cmd_sums(a):
    names = files(a.out)
    with open(os.path.join(a.out, "SHA256SUMS"), "w") as f:
        for n in names:
            f.write(f"{sha(os.path.join(a.out, n))}  {n}\n")
    sig = os.path.join(a.out, "SHA256SUMS.minisig")
    if os.path.exists(sig):
        os.remove(sig)
        print("SHA256SUMS changed: removed the old SHA256SUMS.minisig (sign again if you sign)")
    print(f"SHA256SUMS: {len(names)} files")


def cmd_assets(a):
    data = open(a.image, "rb").read()
    raw = raw_image(data)
    release, line = banner(raw)
    tag = a.tag
    if not tag:
        m = re.search(r"-tb323fu-(t\d+)$", release)
        if not m:
            sys.exit(f"the kernel is release {release}: no -tb323fu-tNN at its end; build with CONFIG_LOCALVERSION=-tb323fu-tNN (or give --tag)")
        tag = m.group(1)
    if not re.fullmatch(r"t\d+", tag):
        sys.exit(f"--tag must be tNN, not {tag}")
    if not release.endswith(f"-tb323fu-{tag}"):
        sys.exit(f"the kernel is release {release}, not a -tb323fu-{tag} build: the helper would refuse it")
    os.makedirs(a.out, exist_ok=True)
    base = os.path.join(a.out, f"Image-tb323fu-{tag}")
    with open(base, "wb") as f:
        f.write(raw)
    if a.gzip:
        with open(base + ".gz", "wb") as f:
            # -9, no name, mtime 0: the same Image gives the same file
            with gzip.GzipFile(filename="", mode="wb", compresslevel=9, fileobj=f, mtime=0) as g:
                g.write(raw)
    print(f"release {release}\nbanner  {line}")
    cmd_sums(a)


def cmd_check(a):
    names = files(a.out)
    sums = {}
    try:
        for l in open(os.path.join(a.out, "SHA256SUMS")):
            h, _, n = l.rstrip("\n").partition(" ")
            sums[n.lstrip(" *")] = h
    except FileNotFoundError:
        sys.exit("no SHA256SUMS")
    kernels = [n for n in names if KERNEL.match(n)]
    serials = {KERNEL.match(n).group(1) for n in kernels}
    if len(serials) != 1:
        sys.exit(f"want kernel assets of exactly one tNN, found {sorted(kernels) or 'none'}")
    tag = "t" + serials.pop()
    bad = 0
    for n in names:
        if not SAFE.match(n):
            print(f"FAIL {n}: not a plain file name"); bad = 1
        elif sums.get(n) != sha(os.path.join(a.out, n)):
            print(f"FAIL {n}: missing from SHA256SUMS or a different hash"); bad = 1
    for n in kernels:
        release, line = banner(raw_image(open(os.path.join(a.out, n), "rb").read()))
        if not release.endswith(f"-tb323fu-{tag}"):
            print(f"FAIL {n}: the kernel is release {release}"); bad = 1
        else:
            print(f"ok   {n}: {line}")
    if os.path.exists(os.path.join(a.out, "SHA256SUMS.minisig")):
        print("note SHA256SUMS.minisig present (checked only by helpers with kernel.require_signature)")
    sys.exit(bad)


def cmd_sign(a):
    subprocess.run(["minisign", "-S", "-s", a.key, "-m", os.path.join(a.out, "SHA256SUMS"),
                    "-t", "tb323fu kernel release " + os.path.basename(os.path.abspath(a.out))], check=True)


def cmd_verify(a):
    bad = subprocess.run(["sha256sum", "-c", "--quiet", "SHA256SUMS"], cwd=a.out).returncode
    if a.pub:
        bad |= subprocess.run(["minisign", "-V", "-q", "-p", a.pub, "-m", os.path.join(a.out, "SHA256SUMS")]).returncode
    print("ok" if not bad else "FAILED")
    sys.exit(1 if bad else 0)


def cmd_fake_api(a):
    owner, _, repo = a.repo.partition("/")
    if not (SAFE.match(owner or "") and SAFE.match(repo or "")):
        sys.exit("--repo OWNER/REPO")
    root = os.path.abspath(a.root)
    base = (a.base or "file://" + root).rstrip("/")
    lst = os.path.join(root, "repos", owner, repo, "releases")
    os.makedirs(os.path.dirname(lst), exist_ok=True)
    os.makedirs(os.path.join(root, "assets"), exist_ok=True)
    rels = json.load(open(lst)) if os.path.exists(lst) else []
    nid = 1 + max([0] + [x["id"] for r in rels for x in r["assets"]] + [r["id"] for r in rels])
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    if a.helper:
        tag, assets, body = f"helper-v{a.helper}", [], ""
    else:
        if not (a.dir and a.tag):
            sys.exit("--dir and --tag (or --helper)")
        tag, body, assets = a.tag, open(a.notes, encoding="utf-8").read() if a.notes else "", []
        for n in sorted(os.listdir(a.dir)):
            p = os.path.join(a.dir, n)
            if not os.path.isfile(p):
                continue
            shutil.copyfile(p, os.path.join(root, "assets", str(nid)))
            x = {"id": nid, "name": n, "size": os.path.getsize(p), "url": f"{base}/assets/{nid}",
                 "browser_download_url": f"{base}/assets/{nid}", "content_type": "application/octet-stream"}
            if not a.no_digest:
                x["digest"] = "sha256:" + sha(p)
            assets.append(x)
            nid += 1
    rels = [r for r in rels if r["tag_name"] != tag]
    rels.insert(0, {"id": nid, "tag_name": tag, "name": a.title or tag, "draft": False, "prerelease": bool(a.prerelease),
                    "published_at": now, "created_at": now, "body": body,
                    "html_url": f"https://github.com/{owner}/{repo}/releases/tag/{tag}", "assets": assets})
    with open(lst + ".tmp", "w") as f:
        json.dump(rels, f, indent=1)
    os.replace(lst + ".tmp", lst)
    print(f"{tag}: {len(assets)} assets; release list {lst}")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    s = p.add_subparsers(dest="cmd", required=True)
    x = s.add_parser("assets"); x.add_argument("out"); x.add_argument("image"); x.add_argument("--gzip", action="store_true"); x.add_argument("--tag")
    x = s.add_parser("sums"); x.add_argument("out")
    x = s.add_parser("check"); x.add_argument("out")
    x = s.add_parser("sign"); x.add_argument("out"); x.add_argument("--key", required=True)
    x = s.add_parser("verify"); x.add_argument("out"); x.add_argument("--pub")
    x = s.add_parser("fake-api"); x.add_argument("root"); x.add_argument("--repo", required=True); x.add_argument("--dir"); x.add_argument("--tag")
    x.add_argument("--prerelease", action="store_true"); x.add_argument("--title"); x.add_argument("--notes"); x.add_argument("--base")
    x.add_argument("--helper"); x.add_argument("--no-digest", action="store_true")
    a = p.parse_args()
    {"assets": cmd_assets, "sums": cmd_sums, "check": cmd_check, "sign": cmd_sign, "verify": cmd_verify, "fake-api": cmd_fake_api}[a.cmd](a)


if __name__ == "__main__":
    main()
