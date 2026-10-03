# TB323FU ("baldur") AudioReach topology.
# Built from linux-msm/audioreach-topology's LEMANS-EVK.m4, cut down to the
# speaker path Android uses (resourcemanager: PAL_DEVICE_OUT_SPEAKER ->
# MI2S-LPAIF-RX-PRIMARY, two aw882xx amplifiers on SD1). The backend runs
# S32_LE at 48 kHz as
# Android runs it (the amplifiers' profile expects 32-bit slots); the front
# end stays S16_LE, which is all q6apm-dai offers. Build with
# the commands in README.md; the kernel loads it as
# qcom/kaanapali/LENOVO-TB323FU-tplg.bin (driver name / card model).
# SPDX-License-Identifier: BSD-3-Clause
include(`audioreach/audioreach.m4')
include(`audioreach/stream-subgraph.m4')
include(`audioreach/device-subgraph.m4')
include(`util/route.m4')
include(`util/mixer.m4')
include(`audioreach/tokens.m4')
#
# Stream SubGraph for MultiMedia1 playback
#  [WR_SH] -> [PCM DEC] -> [PCM CONV] -> [VOL] -> [LOG]
#
dnl Playback MultiMedia1
STREAM_SG_PCM_ADD(audioreach/subgraph-stream-vol-playback.m4, FRONTEND_DAI_MULTIMEDIA1,
	`S16_LE', 48000, 48000, 2, 2,
	0x00004001, 0x00004001, 0x00006001, `110000')
dnl
#
# Device SubGraph for the speakers: Mixer -> [LOG] -> [MFC] -> [I2S EP], primary
# MI2S on SD1 (TLMM 124: the amplifiers' data line; on SD0 they get nothing)
# (subgraph-device-i2s-mfc-playback.m4, here: the MFC turns the
# stream's 16 bits into the 32-bit slots)
#
dnl Primary MI2S Playback. The LPAIF type is plain LPAIF (Android: MI2S-LPAIF-RX-PRIMARY);
dnl LEMANS-EVK's LPAIF_INTF_TYPE_SDR makes the DSP refuse the I2S config.
DEVICE_SG_ADD(audioreach/subgraph-device-i2s-mfc-playback.m4, `Primary', PRIMARY_MI2S_RX,
	`S32_LE', 48000, 48000, 2, 2,
	LPAIF_INTF_TYPE_LPAIF, I2S_INTF_TYPE_PRIMARY, SD_LINE_IDX_I2S_SD1, DATA_FORMAT_FIXED_POINT,
	0x00004006, 0x00004006, 0x00006060, `PRIMARY_MI2S_RX')
dnl

STREAM_DEVICE_PLAYBACK_MIXER(PRIMARY_MI2S_RX, ``PRIMARY_MI2S_RX'', ``MultiMedia1'')
STREAM_DEVICE_PLAYBACK_ROUTE(PRIMARY_MI2S_RX, ``PRIMARY_MI2S_RX Audio Mixer'', ``MultiMedia1, stream0.logger1'')

#
# Microphones (9-50): MultiMedia2 capture from the WCD9395's ADCs through the
# TX macro, CODEC_DMA-LPAIF_RXTX-TX-3 as on Android. From SM8550-HDK.m4.
#
dnl Capture MultiMedia2
STREAM_SG_PCM_ADD(audioreach/subgraph-stream-capture.m4, FRONTEND_DAI_MULTIMEDIA2,
	`S16_LE', 48000, 48000, 1, 2,
	0x00004003, 0x00004003, 0x00006020, `110000')
dnl
dnl WCD TX capture
DEVICE_SG_ADD(audioreach/subgraph-device-codec-dma-capture.m4, `TX_CODEC_DMA_TX_3', TX_CODEC_DMA_TX_3,
	`S16_LE', 48000, 48000, 1, 2,
	LPAIF_INTF_TYPE_RXTX, CODEC_INTF_IDX_TX3, 0, DATA_FORMAT_FIXED_POINT,
	0x00004009, 0x00004009, 0x00006090)
dnl

STREAM_DEVICE_CAPTURE_MIXER(FRONTEND_DAI_MULTIMEDIA2, ``TX_CODEC_DMA_TX_3'')
STREAM_DEVICE_CAPTURE_ROUTE(FRONTEND_DAI_MULTIMEDIA2, ``MultiMedia2 Mixer'', ``TX_CODEC_DMA_TX_3, device120.logger1'')
