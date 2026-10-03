# Audio topology

`LENOVO-TB323FU-tplg.bin` tells the audio DSP's driver (`q6apm`) which graphs exist: the speaker path Android uses
(LPASS primary MI2S, both aw882xx amplifiers on SD1) and the microphones. Without it the sound card does not appear
(`qcom-apm … tplg firmware loading qcom/kaanapali/LENOVO-TB323FU-tplg.bin failed -2`).

Unlike the rest of the firmware it is not Lenovo's: it is built here from
[linux-msm/audioreach-topology](https://github.com/linux-msm/audioreach-topology) (BSD-3-Clause) and the two `.m4`
files in this directory, so it is part of the repository. It belongs in the root filesystem at
`/lib/firmware/qcom/kaanapali/LENOVO-TB323FU-tplg.bin`. `tools/install/install.sh` puts it into the firmware
directory it builds the root from; by hand, see [install.md](../../docs/install.md#1-extract-the-firmware).

To build it again (needs `m4` and `alsatplg` from alsa-utils):

```sh
git clone https://github.com/linux-msm/audioreach-topology
cp firmware/audio/LENOVO-TB323FU.m4 audioreach-topology/
cp firmware/audio/subgraph-device-i2s-mfc-playback.m4 audioreach-topology/audioreach/
cd audioreach-topology
m4 -I . LENOVO-TB323FU.m4 > tb323fu-tplg.conf
alsatplg -c tb323fu-tplg.conf -o LENOVO-TB323FU-tplg.bin
```
