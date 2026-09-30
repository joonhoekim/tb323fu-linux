# Platform files (layer 1)

Static configuration and small boot-time services that make the TB323FU hardware work: audio, sensors,
Bluetooth, USB-C routing, the emergency way back to Android. They are distribution- and desktop-neutral
(systemd, udev, ALSA/PipeWire only), always installed, and **work without the tb323fu helper and without a
desktop**. The helper (layer 2, `helper/`) adds user-facing settings on top; nothing here depends on it.
See [docs/helper.md](../../docs/helper.md) for the layer model.

Install with [`install.sh`](install.sh) (`DESTDIR`, `PREFIX`, `SYSCONFDIR`, `CC`, `OPTIONAL`; no package manager
calls), then enable the services it prints. Distribution recipes can call it or copy the same files.

> Work in progress: these files come from the development image (Debian 13) and were re-packaged for this
> repository; the renamed set has not been booted as a whole yet.

## Files

### udev rules → `/usr/lib/udev/rules.d`

| File | Why |
|---|---|
| `60-tb323fu-usb-input.rules` | on a PC port (USB SDP) raise the input current limit from the firmware's 500 mA to 900 mA, as the vendor kernel does |
| `70-tb323fu-dma-heap.rules` | give the `video` group the system dma-buf heap; libcamera's software ISP needs it, otherwise camera apps find no camera |
| `70-tb323fu-usb-port.rules` | re-route the single USB controller whenever a USB-C partner appears or leaves (runs `tb323fu-usb-port`) |
| `71-tb323fu-gpu-devfreq.rules` | boot-time GPU frequency floor (160 MHz); without it compositing at 120 Hz stutters until the clock ramps |
| `72-tb323fu-pcie-nowake.rules` | the Wi-Fi PCIe root port is not a wakeup source (no Wake-on-WLAN) |
| `73-tb323fu-usb-nowake.rules` | USB wakeup off by default: with it on, a USB 2 hub wakes the tablet right after suspending (the helper can turn it on) |
| `81-tb323fu-sensors.rules` | SSC sensor types and the accelerometer mount matrix for iio-sensor-proxy, and start the proxy for this device |
| `91-tb323fu-audio.rules` | speaker route and safe amplifier attenuation as soon as the sound card appears |
| `udev-override/90-feedbackd.rules` → `/etc/udev/rules.d` | replaces feedbackd's packaged rules: drops its LED rules (they would take over the torch and RGB ring), adds the two haptic motors as vibra devices (LGPL-2.1+, from feedbackd) |

### systemd → `/usr/lib/systemd/…`

| File | Why |
|---|---|
| `tb323fu-gen-ids.service` + `tb323fu-gen-ids` | once per install: derive a Bluetooth address (`/etc/tb323fu/bt-address`) and a fixed Wi-Fi MAC (`/etc/systemd/network/10-tb323fu-wlan-mac.link`) from `/etc/machine-id`; the Wi-Fi firmware otherwise invents a new MAC every boot. Existing files are kept |
| `tb323fu-btaddr.service` | the Bluetooth controller starts without an address; give it `/etc/tb323fu/bt-address` before bluetoothd starts |
| `tb323fu-dsp.service` | start the audio and compute DSPs once the root filesystem (with their firmware) is mounted |
| `tb323fu-audio.service` + `tb323fu-audio-defaults` | speaker/microphone mixer defaults again after `alsa-restore`, so a saved mixer state never leaves the amplifiers unattenuated |
| `tb323fu-usb-port.service` + `tb323fu-usb-port` | route USB to the connector in use at boot (bottom USB 3 + DP or side USB 2; one controller, a GPIO switches the lines), and recover a stuck gadget |
| `tb323fu-emergency-key.service` + `tb323fu-emergency-key` | hold volume up + down (10 s by default) → back to Android via `back-to-android`, even with a frozen desktop. Config `/etc/tb323fu/emergency-key.conf` (`ENABLED`, `HOLD_SECONDS`), Android image hash `/etc/tb323fu/android-boot.sha256`; needs `keyhold` (`src/keyhold.c`) and `back-to-android` (`android/`) |
| `hexagonrpcd.service.d/tb323fu.conf` | point hexagonrpcd at the secure ADSP fastrpc node and the sensor file tree; order it after the DSPs |
| `iio-sensor-proxy.service.d/tb323fu-stop-timeout.conf` | kill the proxy after 3 s on stop (it once ignored SIGTERM and held a suspend for 90 s) |
| `system-sleep/tb323fu-sensors` | stop the sensor stream across suspend; the streaming sensor hub otherwise keeps the SoC out of its deepest idle |
| `system.conf.d/tb323fu-watchdog.conf` | the firmware leaves the SoC watchdog running: systemd keeps feeding it (30 s) |
| `user/tb323fu-speaker-gain.service` + `tb323fu-speaker-gain` | open the speaker amplifiers up only while PipeWire's protecting filter chain runs; safe attenuation otherwise (safety: there is no DSP speaker protection) |

### Audio and camera → `/usr/share/…`

| File | Why |
|---|---|
| `alsa/ucm2/conf.d/kaanapali/LENOVO-TB323FU.conf`, `alsa/ucm2/Lenovo/TB323FU/HiFi.conf` | ALSA UCM profile; without it PipeWire finds no usable device ("Dummy Output"). BSD-3-Clause like alsa-ucm-conf, to allow submitting it there |
| `pipewire/pipewire.conf.d/60-tb323fu-speakers.conf` | the "Speakers" sink: high-pass, compressor, RMS power cap, limiter — the speaker protection Android does in its DSP (needs swh-plugins LADSPA) |
| `wireplumber/wireplumber.conf.d/60-tb323fu-speaker-raw.conf` | name the unprotected raw sink as such and keep it from becoming the default |
| `libcamera/ipa/simple/{s5kjns,gc08a8}.yaml` | black level for the rear and front sensors (libcamera simple pipeline) |

### Configuration → `/etc/tb323fu` (installed only when absent)

| File | What |
|---|---|
| `audio.conf` | `SPEAKER_ATTEN` (safe), `SPEAKER_ATTEN_PROTECTED` (behind the filter), `MIC_GAIN` — read the comments before changing anything |
| `emergency-key.conf` | `ENABLED=1`, `HOLD_SECONDS=10` |

### Optional (`OPTIONAL="…"` in install.sh)

| Name | File | When |
|---|---|---|
| `upower-charge-limit` | `optional/udev/61-tb323fu-battery.rules` | only **without** the helper: lets UPower/GNOME's "Preserve battery health" switch manage the charge limit. With the helper installed the helper owns it (two writers would fight) |
| `led-group` | `optional/udev/74-tb323fu-leds.rules` | only without the helper: `video` group may write the torch and RGB ring directly |
| `logind` | `optional/logind.conf.d/tb323fu-powerkey.conf` | ignore short power-key presses in logind (logind's default powers off); for desktops that handle the key themselves |
| `debian-iio` | `optional/debian/iio-sensor-proxy.service.d/tb323fu-libssc.conf` | Debian 13 only: run a locally built iio-sensor-proxy ≥ 3.8 with libssc from `/usr/local` |
| `image-growroot` | `optional/image/tb323fu-growroot.service` | prebuilt images: grow the root filesystem once |
| — | `optional/debian/cups-browsed/README.md` | how to stop cups-browsed from holding the boot on `network-online.target` (distribution-specific unit copy) |

GNOME-only extras live in [`../desktop/gnome/`](../desktop/gnome/).

## Services to enable

```sh
systemctl enable tb323fu-gen-ids tb323fu-btaddr tb323fu-dsp tb323fu-audio tb323fu-usb-port tb323fu-emergency-key
systemctl --global enable tb323fu-speaker-gain
```

## Dependencies (by capability)

| Capability | Needs |
|---|---|
| audio | alsa-utils (`amixer`, `alsactl`), PipeWire + WirePlumber, swh-plugins (LADSPA `sc4`, `fastLookaheadLimiter`), alsa-ucm-conf base |
| DSPs, firmware | the firmware from your own tablet (`firmware/`), `qrtr` userspace tools, pd-mapper support in the kernel |
| sensors | hexagonrpcd, libssc, iio-sensor-proxy ≥ 3.8 built with libssc |
| Bluetooth | bluez (`btmgmt`) |
| USB-C routing | libgpiod v2 tools (`gpiodetect`, `gpioset`) |
| emergency key | a C compiler for `keyhold` (or a prebuilt binary), `back-to-android` from `android/` |
| per-install IDs | coreutils (`sha256sum`), systemd-networkd `.link` handling (udev) |
| camera | libcamera (simple pipeline, software ISP) |
| modem (optional) | rmtfs, tqftpserv |

## Not here, and why

- **Torch, RGB ring charge indicator, GPU profile, charge limit, refresh-rate policy, switch to Android** —
  user-facing settings owned by the helper (layer 2); the development image's scripts for them are replaced by it.
- **Development conveniences** (serial console and tty autologin, USB network gadget, NetworkManager tweaks,
  hostname, fstab, first-boot script) — belong to a development image, not to the platform.
- **Desktop extensions** — layer 3, provided with the helper.

## Conflicts

- **`bootmac` (qcom-phone-utils)**: its `bootmac-bluetooth.service` generates a random Bluetooth address and brings the controller up through the legacy `hciconfig` path, which hides it from the management interface; `tb323fu-btaddr` then cannot set the real address and Bluetooth ends up with a random address or unusable. Mask it: `systemctl mask bootmac-bluetooth.service`.
