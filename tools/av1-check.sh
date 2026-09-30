#!/bin/sh
# SPDX-License-Identifier: MIT
# av1-check.sh — on the tablet (root): iris AV1 hardware decode through the
# stateful V4L2 client v4l2dec.py (no GStreamer/ffmpeg support for
# stateful AV1 yet), bit-exact against libdav1d via ffmpeg. Also runs VP9 as
# a control.
#   av1-check.sh [DIR]     # default /tmp/av1t; needs ffmpeg (libsvtav1,
#                          # libaom-av1, libdav1d, libvpx) and v4l2dec.py in DIR
# Per stream: frames decoded, MD5 of hw vs sw raw (NV12 8-bit / P010 10-bit,
# visible area), frames that differ, iris session errors in dmesg.
D=${1:-/tmp/av1t}; cd "$D" || exit 1
E="ffmpeg -hide_banner -loglevel error -y"
mk() { # name size frames pixfmt encoder args...
	n=$1 s=$2 f=$3 p=$4; shift 4
	[ -s $n.ivf ] || $E -f lavfi -i testsrc2=size=$s:rate=30 -frames:v $f -pix_fmt $p "$@" $n.ivf 2>/dev/null
}
mk vp9      1920x1080 30 yuv420p     -c:v libvpx-vp9 -deadline realtime -cpu-used 8
mk svt8     1920x1080 30 yuv420p     -c:v libsvtav1 -preset 12
mk aom8     1920x1080 30 yuv420p     -c:v libaom-av1 -cpu-used 8 -row-mt 1
mk svt10    1920x1080 30 yuv420p10le -c:v libsvtav1 -preset 12
mk svt8-4k  3840x2160 10 yuv420p     -c:v libsvtav1 -preset 12
mk svt10-4k 3840x2160 10 yuv420p10le -c:v libsvtav1 -preset 12
for n in vp9 svt8 aom8 svt10 svt8-4k svt10-4k; do
	case $n in *10*) cap=P010 pf=p010le bpp=2 ;; *) cap=NV12 pf=nv12 bpp=1 ;; esac
	case $n in vp9) sw=libvpx-vp9 ;; *) sw=libdav1d ;; esac
	case $n in *4k) w=3840 h=2160 ;; *) w=1920 h=1080 ;; esac
	dmesg -C
	timeout 60 python3 v4l2dec.py $n.ivf $n.hw.yuv --cap $cap > $n.hw.log 2>&1; rc=$?
	$E -c:v $sw -i $n.ivf -pix_fmt $pf -f rawvideo $n.sw.yuv
	fs=$((w * h * 3 / 2 * bpp))
	hn=$(( $(stat -c %s $n.hw.yuv 2>/dev/null || echo 0) / fs )); sn=$(( $(stat -c %s $n.sw.yuv) / fs ))
	mh=$(md5sum < $n.hw.yuv | cut -c1-12); ms=$(md5sum < $n.sw.yuv | cut -c1-12)
	diff=$(python3 -c "
a=open('$n.hw.yuv','rb').read(); b=open('$n.sw.yuv','rb').read(); fs=$fs
print(sum(1 for i in range(min(len(a),len(b))//fs) if a[i*fs:(i+1)*fs]!=b[i*fs:(i+1)*fs]))")
	se=$(dmesg | grep -c 'session error')
	[ "$mh" = "$ms" ] && [ "$hn" = "$sn" ] && [ $rc = 0 ] && r=PASS || r=FAIL
	echo "$n: $r (hw $hn frames md5 $mh, sw $sn md5 $ms, differing $diff, rc $rc, session errors $se, $(grep -m1 CAPTURE $n.hw.log | cut -d' ' -f2-3))"
	rm -f $n.hw.yuv $n.sw.yuv
done
