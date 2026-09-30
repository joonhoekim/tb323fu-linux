#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Merge Qualcomm split PIL images (.mdt + .bNN) into a single .mbn.

Same job as Linaro's pil-squasher: the .mdt is the ELF header plus program
headers (and usually the hash segment); each loadable segment lives in a
separate .bNN file named by its program-header index. The .mbn is that ELF
with every segment written back at its p_offset.
"""
import os, struct, sys

def squash(mdt_path, out_path):
    with open(mdt_path, "rb") as f:
        mdt = f.read()
    if mdt[:4] != b"\x7fELF":
        return None, "not an ELF"
    is64 = mdt[4] == 2
    if is64:
        e_phoff, = struct.unpack_from("<Q", mdt, 0x20)
        e_phentsize, e_phnum = struct.unpack_from("<HH", mdt, 0x36)
    else:
        e_phoff, = struct.unpack_from("<I", mdt, 0x1c)
        e_phentsize, e_phnum = struct.unpack_from("<HH", mdt, 0x2a)

    base = mdt_path[:-4]  # strip ".mdt"
    out = bytearray(mdt[:e_phoff + e_phnum * e_phentsize])
    written = 0
    for i in range(e_phnum):
        ph = e_phoff + i * e_phentsize
        if is64:
            p_offset, = struct.unpack_from("<Q", mdt, ph + 0x08)
            p_filesz, = struct.unpack_from("<Q", mdt, ph + 0x20)
        else:
            p_offset, = struct.unpack_from("<I", mdt, ph + 0x04)
            p_filesz, = struct.unpack_from("<I", mdt, ph + 0x10)
        if p_filesz == 0:
            continue
        seg = "%s.b%02d" % (base, i)
        if not os.path.exists(seg):
            # the hash segment is often already inside the .mdt
            if p_offset + p_filesz <= len(mdt):
                data = mdt[p_offset:p_offset + p_filesz]
            else:
                return None, "missing %s" % os.path.basename(seg)
        else:
            with open(seg, "rb") as f:
                data = f.read()
        if len(data) != p_filesz:
            return None, "%s is %d bytes, phdr says %d" % (
                os.path.basename(seg), len(data), p_filesz)
        if len(out) < p_offset:
            out += b"\0" * (p_offset - len(out))
        out[p_offset:p_offset + p_filesz] = data
        written += 1
    with open(out_path, "wb") as f:
        f.write(out)
    return (len(out), written), None

if __name__ == "__main__":
    src, dst = sys.argv[1], sys.argv[2]
    os.makedirs(dst, exist_ok=True)
    for name in sorted(os.listdir(src)):
        if not name.endswith(".mdt"):
            continue
        out = os.path.join(dst, name[:-4] + ".mbn")
        res, err = squash(os.path.join(src, name), out)
        if err:
            print("%-20s SKIP: %s" % (name, err))
            if os.path.exists(out):
                os.unlink(out)
        else:
            size, nseg = res
            print("%-20s -> %-20s %10d bytes, %d segments" %
                  (name, os.path.basename(out), size, nseg))
