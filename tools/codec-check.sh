#!/bin/sh
# SPDX-License-Identifier: MIT
# codec-check.sh — on the tablet (root or user): iris hardware codecs via
# GStreamer V4L2 stateful elements, judged against software codecs, not by
# "the element exists" (hardware-status: probe != working).
#   decode: software-encode a reference clip (x264/x265/vp9enc), decode it with
#           the hardware decoder AND the software decoder, convert both to I420,
#           compare frame count and per-stream MD5 (decoders must be bit-exact).
#   encode: hardware-encode, then decode with the software decoder; frame count.
#   codec-check.sh [W H FRAMES]      # default 1920 1080 60
# Prints one PASS/FAIL line per test. Needs x264enc, x265enc, vp9enc, avdec_*.
W=${1:-1920}; H=${2:-1080}; N=${3:-60}
d=$(mktemp -d /tmp/codec-XXXX); trap 'rm -rf $d' EXIT
# colorimetry=bt709: videotestsrc's default is not in the iris caps
# I420 for the software reference encoders (4:2:0 8-bit profiles), NV12 for the
# iris encoders (they refuse I420)
SRC="videotestsrc num-buffers=$N pattern=smpte ! video/x-raw,format=I420,width=$W,height=$H,framerate=30/1,colorimetry=bt709"
HWSRC=$(echo "$SRC" | sed s/I420/NV12/)
RAW="videoconvert ! video/x-raw,format=I420,width=$W,height=$H"
g() { timeout 60 gst-launch-1.0 -q "$@" >/dev/null 2>&1; }
frames() { echo $(( $(stat -c %s "$1" 2>/dev/null || echo 0) / (W * H * 3 / 2) )); }

dec() { # name swenc parse ext mux demux swdec hwdec
	n=$1; ref=$d/$n.$4
	g $SRC ! $2 ! $3 ! $5 ! filesink location=$ref || { echo "decode $n: SKIP (no reference encoder)"; return; }
	g filesrc location=$ref ! $6 ! $3 ! $7 ! $RAW ! filesink location=$d/$n.sw.yuv
	g filesrc location=$ref ! $6 ! $3 ! $8 ! $RAW ! filesink location=$d/$n.hw.yuv
	fs=$(frames $d/$n.sw.yuv); fh=$(frames $d/$n.hw.yuv)
	ms=$(md5sum < $d/$n.sw.yuv | cut -c1-12); mh=$(md5sum < $d/$n.hw.yuv | cut -c1-12)
	if [ "$fh" = "$N" ] && [ "$ms" = "$mh" ]; then r=PASS; else r=FAIL; fi
	echo "decode $n: $r (hw $fh/$N frames md5 $mh, sw $fs frames md5 $ms)"
}
enc() { # name hwenc parse swdec
	n=$1; out=$d/$n.enc
	g $HWSRC ! $2 ! $3 ! filesink location=$out
	g filesrc location=$out ! $3 ! $4 ! $RAW ! filesink location=$d/$n.enc.yuv
	f=$(frames $d/$n.enc.yuv)
	if [ "$f" = "$N" ]; then r=PASS; else r=FAIL; fi
	echo "encode $n: $r ($(stat -c %s $out 2>/dev/null || echo 0) bytes, sw decode $f/$N frames)"
}

echo "codec-check ${W}x$H, $N frames, $(uname -r | sed 's/.*+//') $(uname -v | cut -d' ' -f1)"
#   name  sw encoder                            parse            ext  mux                         demux        sw decoder  hw decoder
# x264enc prefers AVC framing, which a raw file cannot carry: force byte-stream
dec h264 "x264enc speed-preset=ultrafast"       "h264parse ! video/x-h264,stream-format=byte-stream" h264 "identity"                   "identity"   avdec_h264  v4l2h264dec
dec hevc "x265enc speed-preset=ultrafast"       "h265parse"      h265 "identity"                   "identity"   avdec_h265  v4l2h265dec
# vp9: in a WebM container (a raw VP9 stream has no framing)
g $SRC ! vp9enc deadline=1 cpu-used=8 ! webmmux ! filesink location=$d/vp9.webm && {
	g filesrc location=$d/vp9.webm ! matroskademux ! vp9dec ! $RAW ! filesink location=$d/vp9.sw.yuv
	g filesrc location=$d/vp9.webm ! matroskademux ! vp9parse ! v4l2vp9dec ! $RAW ! filesink location=$d/vp9.hw.yuv
	fs=$(frames $d/vp9.sw.yuv); fh=$(frames $d/vp9.hw.yuv)
	ms=$(md5sum < $d/vp9.sw.yuv | cut -c1-12); mh=$(md5sum < $d/vp9.hw.yuv | cut -c1-12)
	if [ "$fh" = "$N" ] && [ "$ms" = "$mh" ]; then r=PASS; else r=FAIL; fi
	echo "decode vp9: $r (hw $fh/$N frames md5 $mh, sw $fs frames md5 $ms)"
} || echo "decode vp9: SKIP (no reference encoder)"
enc h264 v4l2h264enc h264parse avdec_h264
enc hevc v4l2h265enc h265parse avdec_h265
gst-inspect-1.0 2>/dev/null | grep -qi 'v4l2av1dec' && echo "av1: hw element present (not tested here)" || echo "av1: no hw element"
