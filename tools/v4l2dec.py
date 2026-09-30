#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""v4l2dec.py -- minimal V4L2 stateful (mem2mem) decoder client, stdlib only,
for codecs no userspace framework drives yet (iris AV1: GStreamer 1.26 has no
stateful AV1 element, Debian ffmpeg 7.1 no av1_v4l2m2m).

    v4l2dec.py IN.ivf OUT.yuv [--dev /dev/video0] [--fourcc AV01|VP90]
               [--cap NV12|P010] [--outbufs 8] [--timeout 20]

Input: IVF (one temporal unit / frame per IVF frame). Output: raw frames
cropped to the visible rectangle (G_SELECTION COMPOSE), NV12 or P010
(single-plane buffers: Y = bytesperline x format height, CbCr after it).
Follows the kernel's "Memory-to-Memory Stateful Video Decoder Interface":
S_FMT OUTPUT -> REQBUFS/mmap -> SUBSCRIBE SOURCE_CHANGE/EOS -> STREAMON OUTPUT
-> feed -> SOURCE_CHANGE -> G_FMT/S_FMT CAPTURE -> REQBUFS -> STREAMON CAPTURE
-> decode -> DECODER_CMD STOP -> buffer with FLAG_LAST. Prints per-frame
timing; exits 1 on error or timeout. Needs root or video group.
"""
import argparse, fcntl, mmap, os, select, struct, sys, time

def IOC(d, nr, size): return (d << 30) | (size << 16) | (ord('V') << 8) | nr
IOW, IOR, IOWR = 1, 2, 3
FMT_SZ, REQ_SZ, BUF_SZ, PLANE_SZ, SUB_SZ, EV_SZ, DCMD_SZ, SEL_SZ, CTRL_SZ = 208, 20, 88, 64, 32, 136, 72, 64, 8
VIDIOC_G_FMT = IOC(IOWR, 4, FMT_SZ); VIDIOC_S_FMT = IOC(IOWR, 5, FMT_SZ)
VIDIOC_REQBUFS = IOC(IOWR, 8, REQ_SZ); VIDIOC_QUERYBUF = IOC(IOWR, 9, BUF_SZ)
VIDIOC_QBUF = IOC(IOWR, 15, BUF_SZ); VIDIOC_DQBUF = IOC(IOWR, 17, BUF_SZ)
VIDIOC_STREAMON = IOC(IOW, 18, 4); VIDIOC_STREAMOFF = IOC(IOW, 19, 4)
VIDIOC_G_CTRL = IOC(IOWR, 27, CTRL_SZ)
VIDIOC_DQEVENT = IOC(IOR, 89, EV_SZ); VIDIOC_SUBSCRIBE_EVENT = IOC(IOW, 90, SUB_SZ)
VIDIOC_G_SELECTION = IOC(IOWR, 94, SEL_SZ); VIDIOC_DECODER_CMD = IOC(IOWR, 96, DCMD_SZ)
CAP, OUT, MMAP = 9, 10, 1           # VIDEO_CAPTURE_MPLANE, VIDEO_OUTPUT_MPLANE
EV_EOS, EV_SRC = 2, 5
FLAG_LAST, FLAG_ERROR = 0x00100000, 0x40
CID_MIN_BUFS_CAP = 0x00980927
SEL_COMPOSE = 0x0100
def fcc(s): return struct.unpack('<I', s.encode())[0]
def fcc_s(v): return struct.pack('<I', v).decode(errors='replace')


def ivf_frames(path):
    d = open(path, 'rb').read()
    if d[:4] != b'DKIF': raise SystemExit("not an IVF file")
    hl = struct.unpack_from('<H', d, 6)[0]
    w, h = struct.unpack_from('<HH', d, 12)
    p, frames = hl, []
    while p + 12 <= len(d):
        n = struct.unpack_from('<I', d, p)[0]
        frames.append(d[p + 12:p + 12 + n]); p += 12 + n
    return d[8:12].decode(), w, h, frames


class Dev:
    def __init__(self, path): self.fd = os.open(path, os.O_RDWR | os.O_NONBLOCK)
    def ioctl(self, req, buf):
        b = bytearray(buf); fcntl.ioctl(self.fd, req, b, True); return b

    def s_fmt(self, typ, fourcc, w, h, sizeimage=0):
        b = bytearray(FMT_SZ)
        struct.pack_into('<IIIII', b, 8, w, h, fourcc, 1, 0)   # width height pix field colorspace
        struct.pack_into('<II', b, 8 + 20, sizeimage, 0)          # plane_fmt[0]
        struct.pack_into('<I', b, 0, typ); b[8 + 180] = 1         # num_planes
        return self.parse_fmt(self.ioctl(VIDIOC_S_FMT, b))

    def g_fmt(self, typ):
        b = bytearray(FMT_SZ); struct.pack_into('<I', b, 0, typ)
        return self.parse_fmt(self.ioctl(VIDIOC_G_FMT, b))

    @staticmethod
    def parse_fmt(b):
        w, h, pf = struct.unpack_from('<III', b, 8)
        si, bpl = struct.unpack_from('<II', b, 28)
        return dict(w=w, h=h, pf=pf, sizeimage=si, bpl=bpl, planes=b[188])

    def reqbufs(self, typ, n):
        b = bytearray(REQ_SZ); struct.pack_into('<III', b, 0, n, typ, MMAP)
        return struct.unpack_from('<I', self.ioctl(VIDIOC_REQBUFS, b), 0)[0]

    def buf(self, typ, idx=0, bytesused=0, flags=0):
        pb = bytearray(PLANE_SZ)                                  # one v4l2_plane
        struct.pack_into('<I', pb, 0, bytesused)
        addr = _addr(pb)
        b = bytearray(BUF_SZ)
        struct.pack_into('<IIII', b, 0, idx, typ, bytesused, flags)
        struct.pack_into('<I', b, 16, 1)                          # field NONE
        struct.pack_into('<I', b, 60, MMAP)
        struct.pack_into('<Q', b, 64, addr); struct.pack_into('<I', b, 72, 1)  # m.planes, length=1
        return b, pb

    def querybuf(self, typ, idx):
        b, pb = self.buf(typ, idx)
        fcntl.ioctl(self.fd, VIDIOC_QUERYBUF, b, True)
        length, off = struct.unpack_from('<II', pb, 4)
        return length, off

    def qbuf(self, typ, idx, bytesused=0, ts=0):
        b, pb = self.buf(typ, idx, bytesused)
        struct.pack_into('<qq', b, 24, ts, 0)                     # timestamp (frame index)
        fcntl.ioctl(self.fd, VIDIOC_QBUF, b, True)

    def dqbuf(self, typ):
        b, pb = self.buf(typ)
        try: fcntl.ioctl(self.fd, VIDIOC_DQBUF, b, True)
        except OSError as e:
            if e.errno in (11,): return None          # EAGAIN
            if e.errno == 32: return 'EPIPE'          # after last buffer
            raise
        idx, _, _, flags = struct.unpack_from('<IIII', b, 0)
        ts = struct.unpack_from('<q', b, 24)[0]
        used = struct.unpack_from('<I', pb, 0)[0]
        return idx, flags, used, ts

    def stream(self, typ, on=True):
        fcntl.ioctl(self.fd, VIDIOC_STREAMON if on else VIDIOC_STREAMOFF, struct.pack('<I', typ))

    def subscribe(self, ev):
        b = bytearray(SUB_SZ); struct.pack_into('<I', b, 0, ev); self.ioctl(VIDIOC_SUBSCRIBE_EVENT, b)

    def dqevent(self):
        try: b = self.ioctl(VIDIOC_DQEVENT, bytearray(EV_SZ))
        except OSError: return None
        return struct.unpack_from('<I', b, 0)[0], struct.unpack_from('<I', b, 8)[0]

    def dec_cmd(self, cmd):
        b = bytearray(DCMD_SZ); struct.pack_into('<I', b, 0, cmd); self.ioctl(VIDIOC_DECODER_CMD, b)

    def compose(self):
        b = bytearray(SEL_SZ); struct.pack_into('<II', b, 0, CAP, SEL_COMPOSE)
        try: b = self.ioctl(VIDIOC_G_SELECTION, b)
        except OSError: return None
        return struct.unpack_from('<iiII', b, 12)

    def min_cap_bufs(self):
        b = bytearray(CTRL_SZ); struct.pack_into('<I', b, 0, CID_MIN_BUFS_CAP)
        try: return struct.unpack_from('<i', self.ioctl(VIDIOC_G_CTRL, b), 4)[0]
        except OSError: return 4


import ctypes
_keep = []
def _addr(bb):
    """address of a bytearray's buffer for v4l2_buffer.m.planes; the last few
    are kept referenced so the pointer stays valid across the ioctl"""
    _keep.append(bb); del _keep[:-16]
    return ctypes.addressof((ctypes.c_char * len(bb)).from_buffer(bb))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("inp"); ap.add_argument("out")
    ap.add_argument("--dev", default="/dev/video0")
    ap.add_argument("--fourcc", help="default from the IVF header (AV01, VP90)")
    ap.add_argument("--cap", help="capture format NV12|P010 (default: driver's choice)")
    ap.add_argument("--outbufs", type=int, default=8)
    ap.add_argument("--timeout", type=float, default=20)
    a = ap.parse_args()
    cc, w, h, frames = ivf_frames(a.inp)
    fourcc = fcc(a.fourcc or {'AV01': 'AV01', 'VP90': 'VP90', 'VP80': 'VP80'}.get(cc, cc))
    d = Dev(a.dev)
    of = d.s_fmt(OUT, fourcc, w, h, max(2 << 20, max(len(f) for f in frames) + 4096))
    print(f"OUTPUT {fcc_s(of['pf'])} {of['w']}x{of['h']} sizeimage {of['sizeimage']}, {len(frames)} frames")
    n_out = d.reqbufs(OUT, a.outbufs)
    obufs = []
    for i in range(n_out):
        ln, off = d.querybuf(OUT, i)
        obufs.append(mmap.mmap(d.fd, ln, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE, offset=off))
    d.subscribe(EV_SRC); d.subscribe(EV_EOS)
    d.stream(OUT)
    free_out = list(range(n_out)); nxt = 0; stop_sent = False
    cbufs = []; cfmt = None; crop = None; written = 0; t0 = time.time(); tq = {}
    fo = open(a.out, 'wb')
    poll = select.poll(); poll.register(d.fd, select.POLLIN | select.POLLOUT | select.POLLPRI | select.POLLERR)

    def setup_capture():
        nonlocal cbufs, cfmt, crop
        f = d.g_fmt(CAP)
        if a.cap: f = d.s_fmt(CAP, fcc(a.cap), f['w'], f['h'])
        cfmt = f
        crop = d.compose() or (0, 0, f['w'], f['h'])
        nb = d.min_cap_bufs() + 2
        n = d.reqbufs(CAP, nb)
        cbufs = []
        for i in range(n):
            ln, off = d.querybuf(CAP, i)
            cbufs.append(mmap.mmap(d.fd, ln, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE, offset=off))
            d.qbuf(CAP, i)
        d.stream(CAP)
        print(f"CAPTURE {fcc_s(f['pf'])} {f['w']}x{f['h']} bpl {f['bpl']} sizeimage {f['sizeimage']}, "
              f"visible {crop}, {n} buffers")

    def write_frame(i, used):
        m = cbufs[i]; f = cfmt; x, y, cw, ch = crop
        bpp = 2 if fcc_s(f['pf']) == 'P010' else 1
        rowb = cw * bpp
        for plane_off, rows, row0 in ((0, ch, y), (f['bpl'] * f['h'], ch // 2, y // 2)):
            for r in range(rows):
                s = plane_off + (row0 + r) * f['bpl'] + x * bpp
                fo.write(m[s:s + rowb])

    last = False; deadline = time.time() + a.timeout
    while not last:
        if time.time() > deadline:
            print(f"TIMEOUT: {written}/{len(frames)} frames"); return 1
        # feed
        while free_out and nxt < len(frames):
            i = free_out.pop(0); f = frames[nxt]
            obufs[i][:len(f)] = f; d.qbuf(OUT, i, len(f), nxt); tq[nxt] = time.time(); nxt += 1
        if nxt == len(frames) and not stop_sent and cbufs:
            d.dec_cmd(1); stop_sent = True                        # V4L2_DEC_CMD_STOP
        ev = poll.poll(200)
        for _, m in ev:
            if m & select.POLLPRI:
                while (e := d.dqevent()):
                    if e[0] == EV_SRC and not cbufs: setup_capture()
                    elif e[0] == EV_SRC: print(f"source change again (changes {e[1]:#x}) -- not handled"); return 1
                    elif e[0] == EV_EOS: pass
            if m & select.POLLERR and not cbufs: pass
        while (r := d.dqbuf(OUT)) not in (None, 'EPIPE'):
            free_out.append(r[0])
        if cbufs:
            while (r := d.dqbuf(CAP)) is not None:
                if r == 'EPIPE': last = True; break
                i, flags, used, ts = r
                if used and not (flags & FLAG_ERROR):
                    write_frame(i, used); written += 1
                    print(f"frame {written:3d} ts {ts:3d} +{(time.time() - tq.get(ts, t0)) * 1000:6.1f} ms"
                          + (" LAST" if flags & FLAG_LAST else ""))
                elif flags & FLAG_ERROR:
                    print(f"frame ts {ts} ERROR flag")
                if flags & FLAG_LAST: last = True; break
                d.qbuf(CAP, i)
    d.stream(CAP, False); d.stream(OUT, False)
    print(f"done: {written}/{len(frames)} frames in {time.time() - t0:.2f} s")
    return 0 if written == len(frames) else 1


if __name__ == "__main__":
    sys.exit(main())
