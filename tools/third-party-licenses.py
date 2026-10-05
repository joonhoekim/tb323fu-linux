#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""third-party-licenses.py -- copyright and license texts of the Rust crates that are
built into a binary release, for the notices the MIT and Apache-2.0 licenses require.

  third-party-licenses.py OUT WORKSPACE_DIR... [--target aarch64-unknown-linux-gnu]

Reads `cargo metadata --locked` of each workspace (the crate sources are fetched into
the cargo registry as needed), follows the normal dependencies of its members for the
target (no dev or build dependencies), and writes OUT: each crate with its version and
license expression, then the license files of its source (LICENSE*, LICENCE*,
COPYING*, NOTICE*, COPYRIGHT*), identical texts printed once.
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT)([-._].*)?$", re.I)


def crates(workspace, target):
    r = subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target],
                       cwd=workspace, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"cargo metadata in {workspace}: {r.stderr.strip()}")
    meta = json.loads(r.stdout)
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen, todo = set(), list(meta["workspace_members"])
    while todo:
        i = todo.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                todo.append(d["pkg"])
    return [pkgs[i] for i in seen if pkgs[i]["source"]]


def license_files(pkg):
    root = os.path.dirname(pkg["manifest_path"])
    out = []
    for n in sorted(os.listdir(root)):
        p = os.path.join(root, n)
        if LICENSE_FILE.match(n) and os.path.isfile(p):
            out.append((n, open(p, encoding="utf-8", errors="replace").read().strip() + "\n"))
    if pkg.get("license_file"):
        p = os.path.join(root, pkg["license_file"])
        if os.path.isfile(p) and all(n != os.path.basename(p) for n, _ in out):
            out.append((pkg["license_file"], open(p, encoding="utf-8", errors="replace").read().strip() + "\n"))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("workspaces", nargs="+")
    ap.add_argument("--target", default="aarch64-unknown-linux-gnu")
    a = ap.parse_args()
    found = {}
    for w in a.workspaces:
        for p in crates(w, a.target):
            found[(p["name"], p["version"])] = p
    texts, body, missing = {}, [], []
    for key in sorted(found):
        p = found[key]
        body.append(f"== {p['name']} {p['version']} ({p.get('license') or 'no license field'})")
        files = license_files(p)
        if not files:
            missing.append(f"{p['name']} {p['version']}")
            body.append("   no license file in the crate source; the license is the expression above\n")
            continue
        for n, t in files:
            h = hashlib.sha256(t.encode()).hexdigest()[:12]
            if h not in texts:
                texts[h] = t
                body.append(f"-- {n} [text {h}]\n\n{t}")
            else:
                body.append(f"-- {n}: identical to [text {h}] above\n")
    with open(a.out, "w", encoding="utf-8") as f:
        f.write("Third-party Rust crates built into these binaries, with the license files of their sources.\n")
        f.write(f"{len(found)} crates (target {a.target}).\n\n")
        f.write("\n".join(body))
    print(f"{a.out}: {len(found)} crates, {len(texts)} distinct license texts")
    for m in missing:
        print(f"note: {m}: no license file in the crate source", file=sys.stderr)


if __name__ == "__main__":
    main()
