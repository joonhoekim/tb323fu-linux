#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Build and sign a kernel release channel for tb323fu-helperd's updates.

A channel is a directory served over https (the project site), http or
file:// (a local test channel):

    index.json (+ .minisig)                   channels -> current release, expiry, helper.latest
    <tag>/tb323fu-<tag>.json (+ .minisig)     the release manifest (docs/helper.md "Kernel updates")
    <tag>/<kernel file>                       the kernel Image (raw or .gz), never a boot image

The manifest carries the kernel file's SHA-256 and size, the release string
and banner read from the Image itself, the serial (monotonic), the channel,
the release notes (Markdown) and the minimum helper version. Signing uses the
minisign tool with the project key (kept offline; a separate test key for
local channels).

usage:
  kernel-channel.py add DIR IMAGE --tag kernel-t28 [--serial 28] [--channel testing]
                    [--notes NOTES.md] [--notes-url URL] [--min-helper 0.1.0] [--min-platform V]
                    [--source-tag TAG] [--source-commit SHA] [--gpl-source URL ...]
  kernel-channel.py index DIR --set stable=kernel-t27 --set testing=kernel-t28
                    [--expires-days 30] [--helper-latest 0.2.0] [--helper-notes URL]
  kernel-channel.py sign DIR --key SECRET.key        (minisign -S for every .json without a fresh .minisig)
  kernel-channel.py verify DIR --pub KEY.pub         (minisign -V for every .json)

Example, a local test channel served from a PC:
  kernel-channel.py add ch Image-tb323fu-t28.gz --tag kernel-t28 --channel testing --notes notes.md
  kernel-channel.py index ch --set testing=kernel-t28 --set stable=kernel-t28
  kernel-channel.py sign ch --key private/signing/kernel-test.key
  (cd ch && python3 -m http.server 8000)
  # on the tablet: /etc/tb323fu/keys/kernel-test.pub, and in /etc/tb323fu/helper.toml
  #   [kernel] index_url = "http://192.168.7.1:8000/index.json"
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


def banner(raw):
    """The kernel's linux_banner: (release, the whole /proc/version line).

    An Image carries it twice: a placeholder from init/version.o with an empty
    build number ("# SMP PREEMPT ") and the real one ("#7 SMP PREEMPT <date>");
    the one with a build number wins."""
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
    return (numbered or found)[-1]


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


def cmd_add(a):
    if not SAFE.match(a.tag):
        sys.exit(f"bad tag {a.tag}")
    data = open(a.image, "rb").read()
    raw = raw_image(data)
    release, line = banner(raw)
    serial = a.serial
    if serial is None:
        m = re.search(r"t(\d+)$", a.tag)
        if not m:
            sys.exit("no --serial and the tag does not end in tNN")
        serial = int(m.group(1))
    fname = os.path.basename(a.image)
    if not SAFE.match(fname) or fname.endswith((".json", ".minisig")):
        sys.exit(f"bad kernel file name {fname}")
    d = os.path.join(a.dir, a.tag)
    os.makedirs(d, exist_ok=True)
    if os.path.abspath(a.image) != os.path.abspath(os.path.join(d, fname)):
        shutil.copyfile(a.image, os.path.join(d, fname))
    notes = open(a.notes, encoding="utf-8").read() if a.notes else ""
    man = {
        "format": 1,
        "tag": a.tag,
        "release": release,
        "build": line,
        "serial": serial,
        "channel": a.channel,
        "kernel": {"file": fname, "sha256": hashlib.sha256(data).hexdigest(), "size": len(data)},
        "image_sha256": hashlib.sha256(raw).hexdigest(),
        "min_helper": a.min_helper,
        "min_platform": a.min_platform or "",
        "notes": notes,
        "notes_url": a.notes_url or "",
        "source_tag": a.source_tag or "",
        "source_commit": a.source_commit or "",
        "gpl_sources": a.gpl_source or [],
    }
    out = os.path.join(d, f"tb323fu-{a.tag}.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(man, f, indent=1, ensure_ascii=False)
        f.write("\n")
    try:
        os.remove(out + ".minisig")
    except FileNotFoundError:
        pass
    print(f"{out}: {release} serial {serial} channel {a.channel}, {fname} {len(data)} bytes")


def cmd_index(a):
    chans = {}
    for s in a.set:
        name, _, tag = s.partition("=")
        if not SAFE.match(name) or not SAFE.match(tag):
            sys.exit(f"bad --set {s}")
        mpath = os.path.join(a.dir, tag, f"tb323fu-{tag}.json")
        try:
            man = json.load(open(mpath, encoding="utf-8"))
        except FileNotFoundError:
            sys.exit(f"no manifest {mpath} (add it first)")
        chans[name] = {"tag": tag, "serial": man["serial"], "manifest": f"{tag}/tb323fu-{tag}.json"}
    now = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0)
    idx = {
        "format": 1,
        "generated": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "expires": (now + datetime.timedelta(days=a.expires_days)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "channels": chans,
        "helper": {"latest": a.helper_latest or "", "notes": a.helper_notes or ""},
    }
    out = os.path.join(a.dir, "index.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(idx, f, indent=1)
        f.write("\n")
    try:
        os.remove(out + ".minisig")
    except FileNotFoundError:
        pass
    desc = ", ".join("%s=%s" % (k, v["tag"]) for k, v in chans.items())
    print(f"{out}: {desc}, expires {idx['expires']}")


def jsons(d):
    for root, _, files in os.walk(d):
        for f in sorted(files):
            if f.endswith(".json"):
                yield os.path.join(root, f)


def cmd_sign(a):
    for j in jsons(a.dir):
        sig = j + ".minisig"
        if os.path.exists(sig) and os.path.getmtime(sig) >= os.path.getmtime(j):
            continue
        subprocess.run(["minisign", "-S", "-s", a.key, "-m", j, "-t", os.path.basename(j)], check=True,
                       stdin=subprocess.DEVNULL)
        print("signed", j)


def cmd_verify(a):
    bad = 0
    for j in jsons(a.dir):
        r = subprocess.run(["minisign", "-V", "-q", "-p", a.pub, "-m", j], stdin=subprocess.DEVNULL)
        print(("ok  " if r.returncode == 0 else "BAD ") + j)
        bad |= r.returncode != 0
    sys.exit(1 if bad else 0)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest="cmd", required=True)
    p = sp.add_parser("add")
    p.add_argument("dir")
    p.add_argument("image")
    p.add_argument("--tag", required=True)
    p.add_argument("--serial", type=int)
    p.add_argument("--channel", default="stable", choices=["stable", "testing"])
    p.add_argument("--notes")
    p.add_argument("--notes-url")
    p.add_argument("--min-helper", default="0.1.0")
    p.add_argument("--min-platform")
    p.add_argument("--source-tag")
    p.add_argument("--source-commit")
    p.add_argument("--gpl-source", action="append")
    p.set_defaults(f=cmd_add)
    p = sp.add_parser("index")
    p.add_argument("dir")
    p.add_argument("--set", action="append", required=True, help="CHANNEL=TAG")
    p.add_argument("--expires-days", type=int, default=30)
    p.add_argument("--helper-latest")
    p.add_argument("--helper-notes")
    p.set_defaults(f=cmd_index)
    p = sp.add_parser("sign")
    p.add_argument("dir")
    p.add_argument("--key", required=True)
    p.set_defaults(f=cmd_sign)
    p = sp.add_parser("verify")
    p.add_argument("dir")
    p.add_argument("--pub", required=True)
    p.set_defaults(f=cmd_verify)
    a = ap.parse_args()
    a.f(a)


if __name__ == "__main__":
    main()
