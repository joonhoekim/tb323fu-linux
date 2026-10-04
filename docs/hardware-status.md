# Hardware status

Lenovo Legion Tab Gen 5 / Legion Y700 5th Gen (TB323FU), mainline Linux 7.3-rc4 plus this project's patch series
(release kernel `kernel-t40`; most entries were checked on the development kernels before it, with the same patches).
Tested mostly with Debian 13 and GNOME 48 on the device, the guided install with Ubuntu 26.04 and GNOME 50, and booted
with five other distributions ([distros.md](distros.md)); the kernel itself does not depend on any of them.
Last updated 2026-10-04.

## How to read this page

A driver that probes, a device node that exists, or a media element that shows up is **not** evidence that a feature works.
Every row carries one of these evidence levels:

| Evidence | Meaning |
|---|---|
| **measured** | checked on the device with logs, counters or numbers (current, fps, CRC, frame counters, events, bit-exact output) |
| **observed** | a person saw, heard or felt it (screen, sound, LEDs, vibration) |
| **user-reported** | a person reported it working, but no log or numbers were kept; will be re-checked with a record |
| **probe only** | driver, node or element present; never actually used |
| **untested** | never tried |

Status: ✅ works · 🟡 partial or conditional · ❌ does not work · ❓ not verified · — not applicable / not implemented.

A feature that needs a human to judge (display, sound, LEDs, vibration) is never marked ✅ without an observation.
Repeated or probabilistic results (suspend, mode sets) are counted as N/M.

### Labels on other pages

Other pages say in their own words how far something was tried. This is how those words relate to the evidence
levels above:

| Page | Label | Means | Closest level here |
|---|---|---|---|
| [install.md](install.md) and the pages for each PC | **followed end to end** | a person ran the guide as written against a rooted tablet; how far (up to the first start, or further) and how often is said next to it | measured / observed |
| [install-manual.md](install-manual.md), [custom-kernel.md](custom-kernel.md) | **[verified]** | done on the development tablet with the command or tool shown | measured / observed |
| | **[from records]** | done on the development tablet, but the commands are reconstructed from notes and were not re-run in this form | user-reported |
| | **[untested]** | not done on any tablet; follows from how the tools work | untested |
| [distros.md](distros.md) | **checked on the device** | seen on the device with that system, not "should work" | observed |
| [distribution modules](../tools/install/distros/README.md) | **verified** | the guided install with that module was followed end to end on a tablet | measured / observed |
| | **experimental** | tried on a tablet, with known gaps listed next to it; the installer warns before using it | 🟡 |
| | **custom** | your own module; nothing is known about it | untested |
| this page | ✅ 🟡 ❌ ❓ | the status of a feature; every row also carries one of the evidence levels above | — |

<details>
<summary>How a check is done</summary>

Each ✅ comes from reproducing the feature on the device the same way every time: a scripted run that measures something
(frame counters and CRCs for display modes, bit-exact comparison against a software decoder for video, battery current and
temperatures for charging and thermal, repeated suspend cycles counted as N/M, traffic soaks for Wi-Fi), or a person watching or listening while a
script drives the feature. A kernel change resets confidence: after a large rebase the whole table is re-checked.

</details>

## Display, graphics, video

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| Internal panel (CSOT, Novatek NT36536 TDDI, dual DSI) | ✅ | measured + observed | used daily |
| DSC 1.2, 10 bpc | ✅ | measured | |
| 120 Hz | ✅ | measured + observed | default mode |
| Idle refresh 60 / 30 Hz | ✅ | measured + observed | done in the kernel: the mode stays 120 Hz and only the vertical porch is stretched, so the desktop sees no mode change. No flicker; back to 120 Hz before suspend and display off |
| 90 Hz | ✅ | measured + observed | vendor timing; screen and touch fine, no underruns |
| 164 Hz (instead of 165) | ✅ | measured + observed | 120 Hz horizontal timing; screen and touch fine, no underruns. SteamOS picks it by default; long sessions not yet watched |
| 165 Hz (vendor timing) | ❌ | measured + observed | horizontal blanking too short → display underruns every frame. Not exposed by default |
| 144 Hz | ❌ | measured + observed | vendor timing underruns; a variant with the 120 Hz horizontal timing breaks touch. Not exposed by default |
| Backlight | ✅ | measured + observed | |
| Display off / on (with and without an external display) | ✅ | measured | |
| GPU OpenGL 4.6 / GLES 3.2 (Adreno 840, freedreno) | ✅ | measured + observed | up to 1200 MHz |
| GPU idle power collapse (IFPC) | ✅ | measured | |
| Vulkan (turnip) | ✅ | measured | Vulkan 1.4; SteamOS's gamescope runs on it; heavy workloads not tried yet |
| Video decode H.264 / HEVC / VP9 / AV1 (iris) | ✅ | measured | output bit-exact against software decoders, 1080p and 4K |
| Video decode 10-bit (HEVC Main10, VP9 profile 2, AV1 Main 10-bit) | ✅ | measured | bit-exact, 1080p and 4K; needs this project's iris line-buffer fix |
| Video encode H.264 / HEVC (8-bit) | ✅ | measured | NV12 input |
| AV1 in applications | — | untested | the driver works, but Debian's GStreamer 1.26 and FFmpeg 7.1 have no stateful V4L2 AV1 decoder; newer distributions not tried yet |
| Browser hardware video decode | — | untested | Firefox does not use V4L2 stateful decoders |

## Input, audio, camera, indicators

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| Touchscreen | ✅ | measured + observed | occasional firmware-reset message after resume. SteamOS Gaming Mode rotates only the picture, so the builder adds a touch calibration matrix |
| Pen (AES) | ❓ | probe only | no pen available for testing |
| Speakers (2 × Awinic aw882xx) | ✅ | measured + observed | left/right heard clean; both amplifiers start on every distribution in [distros.md](distros.md); speaker protection is a PipeWire filter, not Android's DSP protection |
| Microphones | ✅ | measured | the internal microphone records speaker playback (noise floor −62 dBFS, playback peaks −26 dBFS) |
| USB-C analog audio (headset adapter) | — | probe only | no audio route yet |
| Rear camera (Samsung S5KJNS) | ✅ | measured + observed | 1080p via libcamera; auto-exposure is imprecise |
| Front camera (GalaxyCore GC08A8) | ✅ | measured + observed | |
| Rear camera focus (VCM) | 🟡 | measured | manual focus works; no autofocus in libcamera |
| Camera streaming with the screen off | ✅ | measured | restarting a capture with the display off used to hang the system; fixed by patch 0116 (repeated starts with the display off pass) |
| Torch | ✅ | observed | also from the helper's quick settings |
| Flash strobe | ❓ | probe only | |
| RGB ring light | ✅ | observed + measured | charge indicator, solid colours (a nine-colour palette in the helper; colours are gamma-corrected because the LEDs are linear) and breathing run by the chip itself (`hw_pattern`) watched on 2026-10-03. Known limitation: every brightness or colour change goes dark for about 45 ms while the chip's program restarts (no update command found in the vendor material) |
| Haptics (2 motors) | ✅ | observed | |
| Volume and power keys | ✅ | observed | power key suspends and resumes (seen in SteamOS) |
| Emergency key (volume up + down, 10 s → back to Android) | ✅ | measured + observed | Android restore starts 10 s after the keys are detected and Android boots (1/1); switching back to Linux from Android works |

## Wireless, sensors, power

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| Wi-Fi (WCN7860, ath12k) | ✅ | measured | used daily; a receive stall under sustained heavy traffic is fixed by this project's ath12k patch (30-minute 2.4 and 5 GHz soaks clean); the PCIe link trains at x2 but runs at x1; 6 GHz / 320 MHz / MLO not checked |
| Bluetooth LE input devices | ✅ | observed | |
| Bluetooth audio (A2DP) | ✅ | observed | the first syllable of a stream can be clipped |
| Bluetooth HFP | ❓ | untested | |
| Accelerometer / auto-rotate | ✅ | measured + observed | via the sensor hub (SSC) |
| Ambient light / auto-brightness | ✅ | measured + observed | |
| Proximity | ✅ | measured | |
| Compass | 🟡 | measured + observed | relative rotation is right (about 90° per quarter turn); absolute heading is about 30° off compared with a phone. Android's map apps are 90–180° off on this device too, so the sensor is uncalibrated on the stock OS as well. GNOME's sensor proxy only lets authorised clients read it |
| Gyroscope, SAR | — | untested | behind the sensor hub, not wired to iio-sensor-proxy |
| Hall sensor (cover) | ❓ | measured | needs the original folio case |
| GNSS / GPS | ❌ | measured | the modem, location service and a location session run, but no satellites are ever seen; Android on the same device gets no GPS fix either — most likely no antenna (Lenovo's spec sheet lists GPS) |
| Modem (MPSS) | 🟡 | measured | boots and stays up when rmtfs and tqftpserv start before it; if its initialisation stalls, the SoC firmware resets the whole tablet and Linux cannot contain it. Kept off (rmtfs and tqftpserv masked): no cellular, no usable GNSS |
| Thermal throttling | ✅ | measured | CPU (95 °C) and GPU (105 °C) chip limits, plus board-temperature steps from 43 °C following Android's policy; checked under a 10-minute full load |
| Battery readings | ✅ | measured | while bypass charging, `status` still reads "Charging" (display only) |
| USB PD charging, PPS | ✅ | measured | about 40 W into the battery on PPS with a 65 W charger (about 9.25 V in). The USB-C controller reports the negotiated voltage and current as 0 |
| Charge limit, bypass charging | ✅ | measured + observed | bypass cuts the battery current (9 A → 185 mA) and it comes back when turned off; the kernel `status` stays "Charging", so the helper derives its State from the current ("Bypass" at ≤ 300 mA; logic tested, not yet re-checked on the charger) |
| Suspend (s2idle) and resume | 🟡 | measured | works (9 of 9 RTC cycles: Wi-Fi, sensors, sound, USB and video decode back each time); suspend still uses the CPU cluster idle state that runtime idle leaves out (next rows) |
| Deep sleep (CX / DDR power collapse) | ✅ | measured | |
| CPU cluster idle state in runtime idle | 🟡 | measured | left out by default since patch 0118 (`cpuidle_psci_domain.allow_cluster_off=0` on the built-in command line; switch at run time with `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off`, 1 = allowed). With it allowed the tablet reset when idle (2 resets in 78 min, no error message); refused, 3 h idle without one, at the same display-off power (658 / 659 / 621 mW allowed / refused / allowed). Why the cluster's power-down resets the SoC is not fixed; s2idle still uses the state (the system state needs it). The release kernels since t38 run with it refused |
| Wake sources | ✅ | measured | power key, RTC; USB wake off by default |

## USB, storage, other

| Feature | Status | Evidence | Notes |
|---|:-:|---|---|
| USB host (SuperSpeed+, hubs) | ✅ | measured | the second port is USB 2.0 only |
| USB low-speed devices | ❌ | measured | enumeration fails |
| USB gadget (network, serial) | ✅ | measured | |
| DisplayPort alt mode | ✅ | measured + observed | 2 lanes HBR2, up to 3840×2160 at 30 Hz. Checked with a TV through a USB-C to HDMI adapter: extended desktop at 4K 30, no flicker on either screen while the internal panel idles down to 30 Hz, unplug/replug 3/3. The adapter's own low-speed USB device fails to enumerate (harmless) |
| DisplayPort MST (daisy chain / "extend" on MST hubs) | ❌ | measured | waiting for upstream MST support |
| UFS storage | ✅ | measured | |
| microSD | ✅ | measured | UHS-I SDR104; also used for the multiboot roots |
| Multiboot (systems on microSD partitions) | ✅ | measured + observed | six systems boot with this kernel, picked by the initramfs (falls back to the internal root); see [distros.md](distros.md) |
| Firewall (nftables, iptables-nft, firewalld) | ✅ | measured | firewalld including IPv6 reverse-path filtering, NixOS's default firewall; legacy iptables tables are not built |
| Audio DSP (ADSP) | ✅ | measured | |
| Compute DSP / NPU | ❓ | probe only | |
| CPU frequency scaling, boost | ✅ | measured | 6 cores up to 3.63 GHz and 2 up to 4.61 GHz (boost), the same maximum frequencies as Android; the helper sets limits per performance profile and can turn boost off |
| CPU capacity, energy-aware scheduling | ✅ | measured | patch 0117 gives the scheduler the two core sizes (`cpu_capacity` 799 / 1024; before it all eight read 1024, so a busy single thread stayed on a small core) and energy-aware scheduling turns on. Geekbench 6 without pinning: single-core 3590 / multi-core 11016 on a development kernel with 0117 and 0118 (before 0117: 2288 / 10359); Android's published results for this tablet are about 3655 / 10758 |
| Switching between Android and Linux | ✅ | measured + observed | about 40 s |
| Boot time | ✅ | measured | about 6 s from kernel start to the desktop (Debian, automatic login); 7–15 s on the other distributions |

## Known issues

- **CPU cluster idle state left out of runtime idle** (patch 0118): with it the tablet reset when idle, without an error message. Leaving it out avoids that at the same idle power; the cause in the cluster's power-down path is not fixed, and suspend still uses the state (knob: `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off`).
- **165 Hz / 144 Hz**: the vendor timings underrun the display controller. 164 Hz (120 Hz horizontal timing) is offered instead; 144 Hz needs a different timing.
- **No DisplayPort MST** yet: monitors that need MST for "extend" only mirror.
- **GNSS**: no satellites on Linux, and no GPS fix on Android either — treated as not usable on this device.
- **Compass**: only relative rotation can be trusted; the absolute heading is off on Android as well.
- **Bypass charging** works; the kernel battery `status` still reads "Charging" while it is on (the helper shows "Bypass" from the current instead).
- **Modem**: a stalled modem start resets the whole SoC, so rmtfs and tqftpserv stay masked on every root (see [distros.md](distros.md)).
- **Low-speed USB devices** (some keyboards, adapters) fail to enumerate.
- **SteamOS (systemd 257.7)** can hang a shutdown when the USB serial console is connected to a PC that is not reading it (the tty reset waits for output to drain, then the watchdog resets the tablet). The SteamOS builder turns that tty reset off; fixed in systemd 257.8 and later; other distributions are not affected.
- Do not force higher display bandwidth votes: it resets the SoC.
- Do not stop the audio DSP at runtime: it takes the SoC down.

## Still to test

In rough order of value. Rows above marked user-reported, ❓ or 🟡, and rows last checked before a relevant kernel change, are here.

| What | How it will be settled | Needs a person |
|---|---|---|
| 164 Hz in long sessions, SteamOS orientation and UI size, touch on Fedora and NixOS | 10+ minutes in Gaming Mode with underrun counters; first taps on each desktop | eyes, hands |
| Bluetooth HFP | headset call profile, microphone loopback | ears, voice |
| Long idle with patch 0118 | hours of idle and display-off time without a reset, against the 78-minute baseline with the cluster state allowed | no (scripted) |
| AV1 in applications | newer GStreamer / FFmpeg on Arch, Fedora or NixOS, compared bit-exact with a software decoder | no (scripted) |
| 144 Hz | another vertical-porch variant, underruns and touch | eyes, hands |
| From the spec sheet | battery design capacity, OpenCL, heavy Vulkan, full-resolution (50 MP) rear capture, touch report rate (up to 480 Hz), HBM (800 nit) and HDR, 8K and film-grain video | mostly no; touch rate needs a finger |
| Flash strobe | trigger the strobe control and watch | eyes |
| Board thermal above 45 °C | 20–30 minutes of CPU + GPU load, steps and frequencies logged, desktop smoothness watched | eyes (optional) |
| Needs hardware not at hand | pen, Bluetooth LE Audio, USB audio adapters, the original folio case, the 68 W charger, a 6 GHz / 320 MHz / MLO access point | hands |

## Spec sheet vs status

<details>
<summary>Lenovo / Qualcomm specifications compared with the status here</summary>

Sources: Lenovo PSREF "Legion Tab (8.8", 5)" and datasheet, Qualcomm Snapdragon 8 Elite Gen 5 product brief.

| Area | Lenovo / Qualcomm | Status here |
|---|---|---|
| Display | 8.8" 3040×1904, up to 165 Hz, HBM 800 nit, HDR | 120 / 90 / 60 / 30 / 164 Hz ✅; 165 ❌; HBM and HDR untested |
| External display | USB-C DisplayPort; SoC up to 4K 120 / 8K 30 | 4K 30, 2 lanes ✅; MST ❌ |
| Video decode | H.264, H.265, VP9, AV1, 10-bit | all ✅ in the driver (1080p and 4K); 8K untested |
| Video encode | HEVC 10-bit | H.264 / HEVC 8-bit ✅; 10-bit untested |
| GPU | GLES 3.2, Vulkan 1.3, OpenCL 3.0 | GL ✅, Vulkan ✅, OpenCL untested |
| Audio | 2 speakers, 2 microphones | ✅ |
| Cameras | 50 MP rear with AF and flash, 8 MP front | ✅ at 1080p; AF manual only; full resolution untested |
| Sensors | accelerometer + gyro, hall, proximity + light, compass, GPS | accelerometer, light, proximity ✅; compass 🟡 (relative only); gyro not wired; GPS ❌ |
| Charging | 68 W PD 3.0 / PPS, bypass charging | about 40 W on PPS ✅, bypass ✅ |
| USB | USB-C 10 Gbps + DP, second port USB 2.0 | ✅ |
| Storage | UFS 4.1, microSD up to 2 TB | ✅ |
| Wi-Fi / Bluetooth | Wi-Fi 7, Bluetooth 6.0 | Wi-Fi ✅ (2.4 / 5 GHz; 6 GHz untested), BT ✅ (HFP and LE Audio untested) |

</details>
