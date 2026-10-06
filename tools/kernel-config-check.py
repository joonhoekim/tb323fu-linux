#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""kernel-config-check.py -- check a kernel .config against the board fragments.

  kernel-config-check.py CONFIG [FRAGMENT...] [--prev CONFIG]

CONFIG may be a .config, a release's config-tb323fu-tNN or /proc/config.gz.

FAIL when
  - a FRAGMENT line (CONFIG_X=v or "# CONFIG_X is not set"; FRAGMENTs in merge
    order, a later one overrides) is not what CONFIG ended up with
    (merge_config.sh only warns, and olddefconfig can drop or override a value
    whose dependencies are missing)
  - CONFIG enables an option from DENY: debug options that change how drivers
    behave or run, not only what they log. A new kernel can turn one on by
    default (DMABUF_DEBUG defaults to y with DEBUG_KERNEL; it broke every
    dma-buf the GPU imported in t38-t40).
With --prev, also lists the options that differ from an earlier config
(information only).

Exit status 1 on any FAIL.
"""
import argparse
import gzip
import re
import sys

DENY = """
DMABUF_DEBUG DMA_API_DEBUG DEBUG_SG KASAN KCSAN KMSAN UBSAN KFENCE
PROVE_LOCKING DEBUG_ATOMIC_SLEEP DEBUG_PREEMPT DEBUG_SPINLOCK DEBUG_MUTEXES
DEBUG_RT_MUTEXES DEBUG_WW_MUTEX_SLOWPATH DEBUG_IRQFLAGS DEBUG_LIST
DEBUG_PAGEALLOC SLUB_DEBUG_ON PAGE_POISONING PAGE_OWNER DEBUG_VM
DEBUG_OBJECTS DEBUG_KMEMLEAK DEBUG_KOBJECT DEBUG_NOTIFIERS FAULT_INJECTION
DRM_DEBUG_MM DRM_MSM_GPU_SUDO
""".split()

# set by the build scripts, not by the fragments
BUILD_SET = {"CONFIG_INITRAMFS_SOURCE", "CONFIG_LOCALVERSION"}

LINE = re.compile(r"^(CONFIG_\w+)=(.*)$")
UNSET = re.compile(r"^# (CONFIG_\w+) is not set$")


def load(path):
    opener = gzip.open if path.endswith(".gz") else open
    vals = {}
    with opener(path, "rt", encoding="utf-8", errors="replace") as f:
        for l in f:
            l = l.strip()
            m = LINE.match(l)
            if m:
                vals[m.group(1)] = m.group(2)
                continue
            m = UNSET.match(l)
            if m:
                vals[m.group(1)] = "n"
    return vals


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("config")
    p.add_argument("fragments", nargs="*")
    p.add_argument("--prev")
    a = p.parse_args()
    cfg = load(a.config)
    bad = 0
    want = {}
    for frag in a.fragments:
        for k, v in load(frag).items():
            want[k] = (v, frag)
    miss = [(k, v, f) for k, (v, f) in want.items() if k not in BUILD_SET and cfg.get(k, "n") != v]
    for k, v, f in miss:
        print(f"FAIL {f}: {k}={v} requested, the config has {cfg.get(k, 'n')}")
    if a.fragments and not miss:
        print(f"ok   all {len(want)} fragment values in the config (later fragments override earlier ones)")
    bad |= bool(miss)
    on = [k for k in DENY if cfg.get("CONFIG_" + k, "n") != "n"]
    for k in on:
        print(f"FAIL CONFIG_{k}={cfg['CONFIG_' + k]}: a debug option that changes driver behavior")
    if not on:
        print(f"ok   none of the {len(DENY)} denied debug options is set")
    bad |= bool(on)
    if a.prev:
        prev = load(a.prev)
        ch = sorted(k for k in set(cfg) | set(prev) if cfg.get(k, "n") != prev.get(k, "n"))
        print(f"note {len(ch)} options differ from {a.prev}:")
        for k in ch:
            print(f"     {k[7:]} {prev.get(k, '-')} -> {cfg.get(k, '-')}")
    sys.exit(bad)


if __name__ == "__main__":
    main()
