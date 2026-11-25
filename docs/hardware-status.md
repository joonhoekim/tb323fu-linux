# Hardware status

Lenovo Legion Tab Gen 5 / Legion Y700 5th Gen (TB323FU), mainline Linux 7.3 plus this project's patch series
(the [newest stable kernel release](https://github.com/joonhoekim/tb323fu-linux/releases/latest); most rows were checked
on development kernels with the same patches).
Tested mostly with Debian 13 and GNOME 48 on the device, the guided install with Ubuntu 26.04 and GNOME 50, and booted
with the other distributions in [distros.md](distros.md); the kernel itself does not depend on any of them.
How fast it is compared with Android: [Performance](performance.md).
Android firmware: everything here is on ZUI 18.0.12.104 ([other versions](firmware-updates.md#firmware-versions)).

## How to read this page

A driver that probes or a device node that exists is not evidence that a feature works. Every row carries one evidence label:

- **verified**: done on a tablet; measured with logs or numbers, or a person saw, heard or felt it (a user's report names the issue).
- **untested**: not tried on a tablet (a present driver or node does not count).

Status: ✅ works · 🟡 partial or conditional · ❌ does not work · ❓ not verified · — not applicable, not implemented,
or a limit of the device itself that Android has as well ([Device limits](#device-limits-same-on-android)).

A feature that needs a human to judge (display, sound, LEDs, vibration) is never marked ✅ without an observation.
Repeated or probabilistic results (suspend, mode sets) are counted as N/M.

### Evidence labels

The other pages use the same two labels: **verified** and **untested**.
The installer modules in [tools/install/distros/](../tools/install/distros/README.md) use their own states, **verified**
(the guided install was followed end to end on a tablet) and **experimental** (tried, with known gaps listed; the installer warns).

<details>
<summary>How a check is done</summary>

Each ✅ comes from reproducing the feature on the device the same way every time: a scripted run that measures something
(frame counters and CRCs for display modes, bit-exact comparison against a software decoder for video, battery current and
temperatures for charging and thermal, repeated suspend cycles for suspend, traffic soaks for Wi-Fi), or a person watching
or listening while a script drives the feature. After a large kernel rebase the whole table is re-checked.

</details>

## Display, graphics, video

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| Internal panel (CSOT, Novatek NT36536 TDDI, dual DSI) | ✅ | verified | used daily |
| DSC 1.2, 10 bpc | ✅ | verified | |
| 120 Hz | ✅ | verified | default mode |
| Idle refresh 60 / 30 Hz | ✅ | verified | the mode stays 120 Hz and only the vertical porch is stretched, so the desktop sees no mode change; back to 120 Hz before suspend and display off. From 164 Hz too: switching between 164 and 30 Hz with an animated external display ran 120 s without display errors on `kernel-t64` (patch 0216) |
| 90 Hz | ✅ | verified | vendor timing |
| 165 Hz (offered as 164 Hz) | ✅ | verified | SteamOS picks it by default; long sessions not watched[^why-164] |
| 144 Hz | ✅ | verified | on `kernel-t53` and later (patch 0142); one of six switches once showed the picture shifted up, not seen again[^why-144] |
| Backlight | ✅ | verified | the top of the range (4095) is already the high-brightness (HBM) level[^hbm]; brightness in nits not measured |
| Display off / on (with and without an external display) | ✅ | verified | |
| GPU OpenGL 4.6 / GLES 3.2 (Adreno 840, freedreno) | ✅ | verified | up to 1200 MHz |
| GPU idle power collapse (IFPC) | ✅ | verified | |
| Vulkan (turnip) | ✅ | verified | Vulkan 1.4; SteamOS's gamescope runs on it; numbers in [Performance](performance.md#gpu) |
| GPU compute, OpenCL (rusticl) | ✅ | verified | needs the project's Mesa build from the helper; Debian's Mesa is much slower. Numbers in [Performance](performance.md#gpu) |
| Video decode H.264 / HEVC / VP9 / AV1 (iris) | ✅ | verified | bit-exact against software decoders at 1080p and 4K; at 8K HEVC, VP9 and AV1 are bit-exact, H.264 stops after 2 of 30 frames |
| Video decode 10-bit (HEVC Main10, VP9 profile 2, AV1 Main 10-bit) | ✅ | verified | bit-exact at 1080p and 4K; needs this project's iris line-buffer fix. At 8K only AV1 10-bit works; 8K HEVC Main10 fails ([Known issues](#known-issues)) |
| Video encode H.264 / HEVC (8-bit) | ✅ | verified | NV12 input, up to 4K (at 8K, H.264 gives broken output and HEVC none). 10-bit is not possible yet: the driver takes no 10-bit input format |
| AV1 in applications | — | untested | the driver works, but Debian's GStreamer 1.26 and FFmpeg 7.1 have no stateful V4L2 AV1 decoder; newer distributions untested |
| Browser hardware video decode | — | untested | Firefox does not use V4L2 stateful decoders |

## Input, audio, camera, indicators

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| Touchscreen | ✅ | verified | occasional firmware-reset message after resume. SteamOS Gaming Mode rotates only the picture, so the builder adds a touch calibration matrix |
| Pen (AES) | ✅ | verified | Lenovo Tab Pen Plus (AP500U): tracking, pressure, both barrel buttons, rotates with the display in GNOME[^pen]. GNOME's Graphics Tablets settings do not list it (libwacom has no SPI devices). At 164 Hz the pen stops tracking while touch keeps working (120 and 60 Hz fine) |
| Speakers (2 × Awinic aw882xx) | ✅ | verified | both amplifiers start on every distribution in [distros.md](distros.md); speaker protection is a PipeWire filter, not Android's DSP protection |
| Microphones | ✅ | verified | |
| USB-C analog audio (headset adapter) | — | untested | no audio route yet |
| Rear camera (Samsung S5KJNS) | ✅ | verified | 1080p and 4072×3072 at 30 fps via libcamera; auto-exposure is imprecise. The sensor driver has only its 4080×3072 (binned, 12.5 MP) mode, not the full 50 MP |
| Front camera (GalaxyCore GC08A8) | ✅ | verified | full resolution (3264×2448) at 30 fps |
| Rear camera focus (VCM) | 🟡 | verified | manual focus works; no autofocus in libcamera |
| Camera streaming with the screen off | ✅ | verified | |
| Torch | ✅ | verified | also from the helper's quick settings |
| Flash strobe | ✅ | verified | `/sys/class/leds/white:flash`; libcamera does not drive the flash yet |
| RGB ring light | ✅ | verified | charge indicator, solid colors (nine in the helper) and breathing run by the chip (`hw_pattern`). Known limitation: every brightness or color change goes dark for about 45 ms while the chip's program restarts |
| Haptics (2 motors) | ✅ | verified | two AW86937; from `kernel-t64` with the posted driver patches for their register layout (0217–0222). Each motor checked by hand, before and after suspend |
| Volume and power keys | ✅ | verified | power key suspends and resumes |
| Emergency key (volume up + down, 10 s → back to Android) | ✅ | verified | switching back to Linux from Android works |

## Wireless, sensors, power

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| Wi-Fi (WCN7860, ath12k) | ✅ | verified | used daily; the PCIe link trains at x2 but runs at x1; 6 GHz / 320 MHz / MLO not checked[^wifi] |
| Bluetooth LE input devices | ✅ | verified | paired devices reconnect after power off/on and rfkill (patch 0130) |
| Bluetooth audio (A2DP) | ✅ | verified | the first syllable of a stream can be clipped |
| Bluetooth HFP | ✅ | verified | earbuds with a microphone: headset profile with mSBC (16 kHz) in PipeWire |
| Accelerometer / auto-rotate | ✅ | verified | via the sensor hub (SSC) |
| Ambient light / auto-brightness | ✅ | verified | |
| Proximity | ✅ | verified | |
| Compass | ✅ | verified | relative rotation is right; the absolute heading is about 30° off compared with a phone, and Android's map apps are off too ([Device limits](#device-limits-same-on-android)). GNOME's sensor proxy only lets authorized clients read it |
| Gyroscope, SAR | — | untested | behind the sensor hub, not wired to iio-sensor-proxy |
| Hall sensor (cover) | ❓ | verified | needs the original folio case |
| GNSS / GPS | — | verified | the modem, location service and a location session run, but no satellites are ever seen; Android gets no GPS fix either, most likely no antenna ([Device limits](#device-limits-same-on-android)) |
| Modem (MPSS) | 🟡 | verified | boots and stays up when rmtfs and tqftpserv start before it; if its initialization stalls, the SoC firmware resets the whole tablet. Kept off (rmtfs and tqftpserv masked): no cellular, no usable GNSS |
| Thermal throttling | ✅ | verified | CPU (95 °C) and GPU (105 °C) chip limits, plus board-temperature steps from 43 °C following Android's policy |
| Battery readings | ✅ | verified | while bypass charging with the Charge Limit method, `status` still reads "Charging" (display only); the firmware does not report the design or full capacity |
| USB PD charging, PPS | ✅ | verified | about 40 W into the battery on PPS with a 65 W charger. The USB-C controller reports the negotiated voltage and current as 0 |
| Charge limit, bypass charging | ✅ | verified | the Charge Current method (patch 0132) holds the charger at about 9 V; under full CPU load the input stops at about 2.4 A (about 21 W). The Charge Limit method lets the charger drop to 5 V and `status` stays "Charging", so the helper derives its State from the current |
| Suspend (s2idle) and resume | 🟡 | verified | works across repeated cycles with Wi-Fi, sensors, sound, USB, storage and video decode back each time[^suspend]; suspend still uses the CPU cluster idle state that runtime idle leaves out (next row) |
| Deep sleep (CX / DDR power collapse) | ✅ | verified | |
| CPU cluster idle state in runtime idle | 🟡 | verified | left out by default since patch 0118, because with it the tablet reset when idle; the cause is not fixed. Switch at run time with `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off` (1 = allowed). Release kernels since t38 run with it refused[^cluster] |
| Wake sources | ✅ | verified | power key, RTC; USB wake off by default |

## USB, storage, other

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| USB host (SuperSpeed+, hubs) | ✅ | verified | the second port is USB 2.0 only |
| USB low-speed devices | ✅ | verified | e.g. the device inside a USB-C to HDMI cable; kernels before t52 sometimes failed with `-71`/`-110` |
| USB gadget (network, serial) | ✅ | verified | |
| DisplayPort alt mode | ✅ | verified | 4 lanes HBR3 with pin assignment C: a 5120×2160 USB-C monitor at 75 Hz with its USB 2.0 hub; a TV through a USB-C to HDMI adapter at 4K 30. With an adapter that keeps USB 3 next to DisplayPort (pin assignment D), USB 3 stays at SuperSpeed (5 Gbit/s); a picture through such an adapter is untested[^dp] |
| DisplayPort MST (daisy chain / "extend" on MST hubs) | 🟡 | verified | from `kernel-t61` (checked again on `kernel-t64`), with patches 0157-0205 (upstream MST v7, under review, and five fixes). A Realtek MST hub with two 1920×1200 panels shows two separate displays next to the built-in one, also after suspend and replugs. With this monitor a replug sometimes leaves one or both screens dark (its hub drops HPD or stops answering; Android shows the same): unplug and plug again, or reconnect the monitor's power. Once, on a test kernel, a replug whose link training failed hung the tablet until the watchdog reset it (not seen in 12 replugs on `kernel-t63`). Daisy chains, 4K and audio over MST untested[^mst] |
| DisplayPort audio | ✅ | verified | on `kernel-t62`, platform 0.4.9: sound on a DisplayPort monitor and on an HDMI monitor through a USB-C to HDMI cable, at the same time as the speakers; the PipeWire device follows plug and unplug. A stream pinned to the display output stops when the display is unplugged until the application is restarted ([#53](https://github.com/joonhoekim/tb323fu-linux/issues/53)). Over MST: untested |
| UFS storage | ✅ | verified | multi-queue mode (patch 0119), kept over suspend (0128); fixes for a deadlock that rebooted the tablet after hours of use (0129) and for recovery after a controller reset (0141 and Linux 7.3). Numbers in [Performance](performance.md#storage) |
| microSD | ✅ | verified | 100 MHz by default: at 202 MHz the slot's BH201 retimer gives some cards CRC and I/O errors until it is calibrated ([#26](https://github.com/joonhoekim/tb323fu-linux/issues/26)); 1 GiB reads at about 47 MB/s. Writing 1 to `/sys/module/sd_retimer_o2/parameters/calibrate` calibrates the retimer at the next card insertion and runs the card at 202 MHz (about 90 MB/s), tried on two cards[^sd-cal]. Also used for the multiboot roots |
| Multiboot (systems on microSD partitions) | ✅ | verified | the systems in [distros.md](distros.md) boot, picked by the initramfs (falls back to the internal root) |
| Firewall (nftables, iptables-nft, firewalld) | ✅ | verified | firewalld including IPv6 reverse-path filtering, NixOS's default firewall; legacy iptables tables are not built |
| Audio DSP (ADSP) | ✅ | verified | |
| Compute DSP / NPU (Hexagon HTP v81) | ✅ | verified | Qualcomm's QNN runtime and llama.cpp's Hexagon backend run with the open fastrpc library (unsigned PD); not packaged by the project. Numbers in [Performance](performance.md#npu) |
| CPU frequency scaling, boost | ✅ | verified | 6 cores up to 3.63 GHz and 2 up to 4.61 GHz (boost), as on Android; the helper sets limits per performance profile and can turn boost off |
| CPU capacity, energy-aware scheduling | ✅ | verified | patch 0117 gives the scheduler the two core sizes, so a busy single thread no longer stays on a small core; Geekbench 6 single-core reaches Android's level ([Performance](performance.md)) |
| Switching between Android and Linux | ✅ | verified | about 40 s |
| Boot time | ✅ | verified | about 6 s from kernel start to the desktop (Debian, automatic login); 7–15 s on the other distributions |

## Known issues

- **CPU cluster idle state left out of runtime idle** (patch 0118): with it the tablet reset when idle, without an error message. Leaving it out avoids that at the same idle power (on `kernel-t64`, about 7 hours idle at the login screen with the screen on passed without a reset); the cause in the cluster's power-down path is not fixed, and suspend still uses the state (knob: `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off`).
- **8K HEVC 10-bit video does not decode**: with `kernel-t60` and older, hardware decoding of an 8K (7680×4320) HEVC Main10 stream resets the tablet; from `kernel-t61` (patch 0206) it fails with a decode error instead. 8K 8-bit HEVC, VP9 and AV1 (also 10-bit) decode fine; H.264 and encoding do not work at 8K.
- **Bypass charging** works; with the Charge Limit method the kernel battery `status` still reads "Charging" and the charger drops to 5 V (the Charge Current method keeps 9 V).
- **Charging can stop below the limit**: the firmware stops at 5 V and resumes only when the limit is moved below the current charge and back; the helper's Restart Stalled Charging does this automatically. Cause not found.
- **Modem**: a stalled modem start resets the whole SoC, so rmtfs and tqftpserv stay masked on every root (see [distros.md](distros.md)).
- **SteamOS (systemd 257.7)** can hang a shutdown when the USB serial console is connected to a PC that is not reading it (the tty reset waits for output to drain, then the watchdog resets the tablet). The SteamOS builder turns that tty reset off; fixed in systemd 257.8 and later; other distributions are not affected.
- Do not force higher display bandwidth votes: it resets the SoC.
- Do not stop the audio DSP at runtime: it takes the SoC down.

## Device limits (same on Android)

These come from the hardware or its firmware; the stock Android on the same tablet behaves the same way, so they are not
gaps in the Linux port.

- **GNSS**: no satellites are seen on Linux, and Android gets no GPS fix either — most likely no antenna.
- **Compass**: relative rotation is right, the absolute heading is off; Android's map apps are off on this device too.
- **Fast USB-C replugs**: replugging a USB-C monitor within about a second sometimes ends without a power-delivery
  contract (no picture, no hub); unplug and plug again. Android shows "Check the charger connection" in the same case.

## Spec sheet vs status

<details>
<summary>Lenovo / Qualcomm specifications compared with the status here</summary>

Sources: Lenovo PSREF "Legion Tab (8.8", 5)" and datasheet, Qualcomm Snapdragon 8 Elite Gen 5 product brief.

| Area | Lenovo / Qualcomm | Status here |
|---|---|---|
| Display | 8.8" 3040×1904, up to 165 Hz, HBM 800 nit, HDR | 120 / 90 / 60 / 30 / 144 Hz and 165 (as 164) ✅; HBM ✅ (the top brightness is the HBM LED current), HDR untested |
| External display | USB-C DisplayPort; SoC up to 4K 120 / 8K 30 | 5120×2160 75 Hz, 4 lanes HBR3 ✅; MST 🟡 (two 1920×1200, development kernel) |
| Video decode | H.264, H.265, VP9, AV1, 10-bit | all ✅ in the driver (1080p and 4K); 8K ✅ for HEVC 8-bit, VP9, AV1 (8- and 10-bit), ❌ for H.264 and HEVC 10-bit |
| Video encode | HEVC 10-bit | H.264 / HEVC 8-bit ✅ up to 4K; 10-bit ❌ (no 10-bit input in the driver) |
| GPU | GLES 3.2, Vulkan 1.3, OpenCL 3.0 | GL ✅, Vulkan ✅, OpenCL ✅ (the project's Mesa build) |
| NPU | Hexagon NPU | QNN and llama.cpp run ✅ |
| Audio | 2 speakers, 2 microphones | ✅ |
| Cameras | 50 MP rear with AF and flash, 8 MP front | ✅ (rear up to 12.5 MP binned, front at its full 8 MP); 50 MP mode not in the driver; AF manual only |
| Sensors | accelerometer + gyro, hall, proximity + light, compass, GPS | accelerometer, light, proximity ✅; compass ✅ (absolute heading off, as on Android); gyro not wired; GPS — (no fix on Android either) |
| Charging | 68 W PD 3.0 / PPS, bypass charging | about 40 W on PPS ✅, bypass ✅ (charger kept at 9 V) |
| USB | USB-C 10 Gbps + DP, second port USB 2.0 | ✅ |
| Storage | UFS 4.1, microSD up to 2 TB | ✅ |
| Wi-Fi / Bluetooth | Wi-Fi 7, Bluetooth 6.0 | Wi-Fi ✅ (2.4 / 5 GHz; 6 GHz untested), BT ✅ (HFP ✅; LE Audio untested) |

</details>

[^why-164]: 164 Hz uses the 120 Hz horizontal timing; screen and touch fine, no underruns. The vendor's 165 Hz timing has too short a horizontal blanking and underruns every frame, so it is not offered.
[^why-144]: The 164 Hz horizontal timing with a longer vertical front porch; screen and touch fine, no underruns. The vendor timing underruns.
[^hbm]: Android's HBM sets the backlight chip's LED current to 28.0 mA (register 0x02 = 0x2e), the value Linux keeps all the time.
[^pen]: Seen by a user on their tablet (issue #16, kernels t54–t56): pressure with 4096 levels (patches 0143, 0146), tracking in all corners, mapped to the built-in display (0144). Not re-checked on the development tablet, which has no pen.
[^wifi]: A receive stall under sustained heavy traffic is fixed by this project's ath12k patch; 30-minute soaks on 2.4 and 5 GHz were clean.
[^suspend]: 9 of 9 RTC cycles, then 15 more with storage checked after patch 0128; with a USB-C monitor connected, 5 power-key resumes.
[^cluster]: With the state allowed the tablet reset twice in 78 minutes of idle, with no error message; refused, 3 hours of idle passed without one, at the same display-off power (658 / 659 / 621 mW allowed / refused / allowed). s2idle still uses the state because the system sleep state needs it.
[^dp]: The 4-lane monitor's USB 2.0 hub kept working after replugs and across suspend. The earlier 2-lane HBR2 run with the TV showed no flicker on either screen while the internal panel idled down to 30 Hz, and unplug/replug passed 3 of 3; the adapter's own low-speed USB device fails to enumerate (harmless).
[^mst]: Suspend passed 3 of 3. Android (ZUI 18.0.12.104) drives both MST streams in the kernel, but its settings use only one as an extended display and mirror the other; Linux extends both.
[^sd-cal]: On the card tested here a known phase passed in about 0.6 s on 10 of 10 re-insertions (simulated by rebinding the slot); on another card a known phase passed in 30-70 % of boots (about 2 s), otherwise a full sweep took 44-52 s. The calibration stops if the card's first block reads back changed. If a calibrated phase stops reading after suspend, the card stays at 100 MHz for that attach and the next, then the phase is tried again (checked with a forced miss and a mounted partition). The setting is lost at reboot. With the root on the microSD it cannot be turned on yet: it takes effect only when the card is attached again, and the release kernel uses its built-in command line, so `sd_retimer_o2.calibrate=1` cannot be passed at boot.
