# Tools

The scripts used while porting. The manual setup is in [docs/rooting.md](../docs/rooting.md) and [docs/install.md](../docs/install.md);
[`install/`](install/README.md) is a guided install script (**prototype, untested on real hardware**).

Every script starts with its usage. Hosts are never hard-coded except the tablet's USB network default: the root filesystem sets up a
USB network gadget in which the tablet is **192.168.7.2** and the PC is 192.168.7.1. Set `TB323FU_HOST` to use another address
(e.g. Wi-Fi).

## Build and flash (PC)

| Script | What it does |
|---|---|
| `build-boot.sh` | build the kernel (`dtbs`, then `Image`) and pack it into a boot image, optionally regenerating the initramfs first |
| `boot-repack-kernel.py` | put a new kernel into a stock Android boot image (header v4), keeping the stock boot signature, vbmeta blob and AVB footer layout |
| `flash-boot.sh` | write a boot image to `boot_a` from the running Linux over SSH, verify by read-back, reboot |
| `cycle.sh` | the same through Android: back to Android, write `boot_a` with root over adb, reboot, wait for Linux |

## Diagnostics and tests

| Script | Runs on | What it does |
|---|---|---|
| `post-crash.sh` | PC | after an unexpected reboot: collect pstore (also systemd-pstore's archive), the previous boot's journal tail, this boot's early kernel log |
| `boot-time.sh` | PC | boot-time measurement over N boots (systemd-analyze, time to the greeter, lines printed to the console); mean/min/max |
| `codec-check.sh` | tablet | iris hardware codecs via GStreamer, bit-exact against software decoders: H.264/HEVC/VP9 decode, H.264/HEVC encode |
| `v4l2dec.py` | tablet | minimal V4L2 stateful (memory-to-memory) decoder client, Python standard library only; IVF input, NV12/P010 output |
| `av1-check.sh` | tablet | AV1 (8/10-bit) hardware decode with `v4l2dec.py`, bit-exact against libdav1d; VP9 as a control |
| `thermal-stress.sh` | tablet | CPU load with per-second clocks, temperatures and cooling states; `emul` steps the board sensor's emulated temperature |

## Safety notes

- Writing `boot_a` is only safe when the way back exists: the stock Android boot image in `boot_b` (see [android/README.md](../android/README.md)).
- Do not `rmmod qcom_iris` on kernels without the iris remove-path fix: it can deadlock and reboot the tablet.
