# Firmware

This directory describes how the firmware is obtained. Rooting Android (needed for the extraction) is in
[docs/rooting.md](../docs/rooting.md); what a root filesystem needs besides firmware is in [docs/distros.md](../docs/distros.md#what-every-root-needs).

**No firmware files are stored in this repository.** The DSPs, GPU, video codec, Wi-Fi, Bluetooth, touch controller and speaker
amplifiers need Lenovo/Qualcomm binaries that may not be redistributed. Every one of them is already on your tablet, in Android's
`/vendor`, so you extract them from your own device.

## Files

| File | What it is |
|---|---|
| `manifest.tsv` | every firmware file the mainline kernel loads: target path, sha256, where it comes from on Android |
| `extract-on-device.sh` | runs on Android as root; copies/merges every manifest entry into an output directory laid out like the Linux root filesystem |
| `pil-squash.py` | PC-side equivalent of the merge step (split `.mdt` + `.bNN` → `.mbn`), for extracting from a firmware dump instead of the device |

## Manifest columns

- **target** — path under the Linux root filesystem (e.g. `lib/firmware/qcom/kaanapali/lenovo/baldur/adsp.mbn`).
- **sha256** — the hash of that file as extracted from Android build `TB323FU_ROW 18.0.12.104`.
- **source** — the file on Android. `X.mdt + .bNN` is a split PIL image: Android stores the ELF header and program headers in the
  `.mdt` and each segment in a numbered `.bNN` file; the script writes the segments back at their offsets to form one `.mbn`,
  the form the mainline kernel loads. "(renamed)" means the Linux driver expects a different file name.

A **hash mismatch is a warning, not an error**: another firmware version (an Android OTA) gives other hashes and usually still works.
A **missing** file is an error.

## Extracting

On the tablet in Android, with root (KernelSU or Magisk):

```sh
adb push manifest.tsv extract-on-device.sh /data/local/tmp/
adb shell su -c 'sh /data/local/tmp/extract-on-device.sh /data/local/tmp/tb323fu-firmware'
```

It needs only toybox/busybox tools (`od`, `dd`, `head`, `sha256sum`). The result is a tree such as
`/data/local/tmp/tb323fu-firmware/lib/firmware/...`; copy it into the Linux root filesystem at `/` so the files end up in
`/lib/firmware/...`.

Where the sources live on Android: `/vendor/firmware`, `/vendor/firmware_mnt/image` (the `modem` partition; Wi-Fi files are in
its `peach/` subdirectory), `/vendor/soccp_firmware/image` and `/vendor/bt_firmware/image` (the `bluetooth` partition).

From a firmware dump on a PC instead: mount/extract those partitions and run `python3 pil-squash.py SRC_DIR OUT_DIR` for the split
images; copy the plain files by hand following `manifest.tsv`.
