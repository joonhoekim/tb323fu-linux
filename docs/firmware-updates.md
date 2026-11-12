# Updating Android firmware

The kernel is tested on **ZUI 18.0.12.104**; what is known about other versions is in the
[table below](#firmware-versions). Nothing in this project stops you from updating, and as long as LTBox can root
the new firmware, the rest is a question of what changed in it. This page has the order, the way back when Linux
does not start, how we matched the kernel to .104 and how to check a new firmware the same way. If you are on
another firmware, please tell us ([below](#tell-us)), even if you only ran the check: you are then the first person
who can.

A firmware update **cannot be undone**: it raises the rollback index, so the older firmware will not boot again.

## Firmware versions

| Firmware | Region | Linux | Memory map | Report |
|---|---|---|---|---|
| 18.0.12.104 | ROW | works (the development tablet) | matches ([map](../firmware/memory-map-zui-18.0.12.104.txt)) | — |
| 18.0.12.187 | ROW | hangs early in boot | not checked yet | [#9](https://github.com/joonhoekim/tb323fu-linux/issues/9) |
| ZUXOS 2.0.10.026 | PRC (24 GB) | boots to the desktop (`kernel-t61`, Arch Linux ARM) | matches; only `/memory` differs, as expected with 24 GB | [#52](https://github.com/joonhoekim/tb323fu-linux/issues/52) |

Every other version is untested. All reports:
[issues labelled `firmware`](https://github.com/joonhoekim/tb323fu-linux/issues?q=is%3Aissue+label%3Afirmware).

## Before you update

- Your **EDL dump** is kept somewhere other than the tablet ([Rooting](rooting.md)).
- **LTBox's check passes for the new firmware's bootloader:** it must still load `efisp`, otherwise you cannot root
  again.
- You can reach **EDL** and know how to write `boot_a` there ([Recovery](recovery.md#linux-does-not-boot)): if the
  kernel hangs before the boot summary, that is the way back to Android.

## The order

Prepared from the LTBox source; not done on the development tablet since rooting.

1. Restore the stock `boot_a` and unroot ([Undoing the root](recovery.md#undoing-the-root)).
2. Enable the OTA apps and update; disable them again.
3. Root again (LTBox then writes only `init_boot`).
4. Install again, every step: the guided installer warns about the firmware and asks whether to go on (on a PC
   where it ran before, start with an empty `~/tb323fu-install`, or the finished steps are skipped), or by hand from
   [step 1](install-manual.md#1-extract-the-firmware). The firmware files Linux loads, the Linux boot image and the
   hash of Android's boot image all have to come from the new firmware, and the root on the card is built again. To
   keep a root you already have, see [the way back](install-manual.md#2-the-way-back-androids-boot-image-in-boot_b)
   for the hash it records.

If Linux starts, check what you use ([First start](after-install.md#first-start)) and tell us. If it does not: put
Android back ([Linux does not boot](recovery.md#linux-does-not-boot)), then read on.

## Why a firmware update can break the kernel

The bootloader normally hands the kernel a devicetree, a command line and a ramdisk. This kernel brings all three
itself (the devicetree is built into the `Image`), and takes **only `/memory`** from the bootloader: the list of RAM
ranges, which the bootloader computes at boot from the DRAM setup and the memory the firmware keeps for itself.

That memory is described a second time in the kernel's own devicetree, as `/reserved-memory`: the regions the DSPs,
the modem, the display and others run in. Those addresses were taken from 18.0.12.104. A firmware that moves a
region, makes one larger or adds one can leave the two descriptions disagreeing:

- a firmware region **inside** `/memory` that the kernel does not reserve becomes ordinary RAM for Linux, which then
  overwrites the firmware's data: a hang or a reset with no message, often early;
- a region the kernel reserves at the **old** address no longer matches the firmware: a DSP loads into the wrong
  place or fails to start.

This is the leading guess for #9, not a confirmed cause. Other things also change with a firmware (the bootloader
itself, the firmware of the always-on and security processors), and the check below does not see them.

## How the memory map matches on 18.0.12.104

`tools/fw-memory-check.py` compares the devicetree Android boots with (the bootloader's `/memory` and Android's
`/reserved-memory`) with the one built into a kernel `Image`. Run on 18.0.12.104 and the t58 kernel, shortened:

```text
Android: Qualcomm Technologies, Inc. Canoe QRD + ST54L + Pandeiro (msm-id <0x295 0x20000 0x294 0x10000 …>, board-id <0x2000b 0x0>)
kernel:  Lenovo Legion Tab Y700 5th Gen (msm-id <0x295 0x20000>, board-id <0x2000b 0x0>)
/memory from the bootloader: 20 ranges, 15607 MiB
kernel /reserved-memory: 24 regions with a fixed address
not compared (placed at boot, no fixed address): 16 in Android, 0 in the kernel

Android region                               range                     flags     in Linux
gunyah_hyp_region@80000000                   0x080000000+e00000    no-map    outside /memory
...
qdss_apps_region@82800000                    0x082800000+2000000   reusable  RAM: 32.00 MiB not reserved
mpss_region@8aa00000                         0x08aa00000+eb00000   no-map    outside /memory
adspslpi_region@a1400000                     0x0a1400000+4600000   no-map    outside /memory
tz_merged_region@d8000000                    0x0d8000000+800000    no-map    outside /memory
trust_ui_vm_region@f3800000                  0x0f3800000+4000000   reusable  RAM: 64.00 MiB not reserved
oem_vm_region@f7c00000                       0x0f7c00000+4c00000   reusable  RAM: 76.00 MiB not reserved
splash_region                                0x0fc800000+2b00000             reserved by the kernel too
lcp_dare_region                              0x880000000+6000000   reusable  RAM: 96.00 MiB not reserved

kernel region                                range                     in Android
pdp@81300000                                 0x081300000+100000    within cpucp_pdp_region@81200000
mpss@8aa00000                                0x08aa00000+eb00000   same as mpss_region@8aa00000
adspslpi@a1400000                            0x0a1400000+4600000   same as adspslpi_region@a1400000
rmtfs@d7c00000                               0x0d7c00000+400000    not in the map, inside /memory
splash_region@fc800000                       0x0fc800000+2b00000   same as splash_region
...
```

What this says, and why .104 boots:

- **The bootloader already cuts every fixed firmware region out of `/memory`** (20 ranges with holes in them). All
  `no-map` regions are "outside /memory": Linux never gets them as RAM, whatever its devicetree says.
- **Four regions inside `/memory` are not reserved** by the kernel, and all four are `reusable`: memory pools that
  Android also uses as normal RAM until it lends them to a virtual machine. Starting from the bootloader, nothing is
  lent yet, so Linux may use them.
- **Every region the kernel reserves matches Android**: the same address and size, or (`pdp`) a part of a larger
  Android region. `rmtfs` is a buffer the kernel itself provides to the modem, at the end of a `/memory` range.
  `splash_region` keeps the boot screen's buffer.
- **One of them had to be changed to match.** Upstream `kaanapali.dtsi` reserves `adspslpi` with `0x4c00000` bytes;
  Android has `0x4600000`, and the bootloader hands out the memory after it (from `0xa5a00000`) as RAM. The audio DSP
  then failed to start (`request_mem_region` returned `-EBUSY`). `kernel/dts/baldur-audio.dtsi` overrides it:

  ```dts
  &adspslpi_mem {
  	reg = <0x0 0xa1400000 0x0 0x4600000>;
  };
  ```

The full lists for .104 are in [`firmware/memory-map-zui-18.0.12.104.txt`](../firmware/memory-map-zui-18.0.12.104.txt).

## Checking a new firmware

On the updated, rooted Android:

```sh
adb shell "su -c 'cp /sys/firmware/fdt /sdcard/fdt.dtb'"
adb pull /sdcard/fdt.dtb
adb shell rm /sdcard/fdt.dtb
adb shell getprop ro.product.model
adb shell getprop ro.build.display.id
```

`ro.product.model` must contain `TB323FU`: with more than one device connected, `adb` may have reached another one.

The kernel: `Image-tb323fu-tNN.gz` from [Releases](https://github.com/joonhoekim/tb323fu-linux/releases) (the one
the installer used), unpacked with `gunzip`. Then, on any PC with Python 3:

```sh
tools/fw-memory-check.py fdt.dtb Image-tb323fu-tNN --extract memory-map.txt
diff firmware/memory-map-zui-18.0.12.104.txt memory-map.txt
```

The `diff` shows what the firmware changed; the check shows what that means for the kernel. Look for:

| Line | Meaning |
|---|---|
| `no-map … RAM: … not reserved` | firmware memory Linux would use as RAM: reserve it in the kernel's devicetree |
| `reusable … RAM: … not reserved` | a pool Android lends out later; usually harmless at boot (as on .104) |
| `DIFFERS: overlaps …` | a region the kernel reserves at an address the firmware no longer uses that way: move or resize it |
| `/memory` ranges differ | expected when regions move; the check above covers what matters |

The first two lines show what was compared. Android's devicetree carries the name of Qualcomm's reference board, not
Lenovo's; what has to match is the SoC id (the first number of a `qcom,msm-id` pair, `0x295`). A devicetree from
another device stops the script with exit status 2 (`--force` compares anyway); a different `board-id` only warns,
and so does a devicetree that looks like the kernel's own (taken under Linux instead of Android).

The script exits with 1 when it finds a `no-map` region exposed or a kernel region that differs. A text map works
in place of `fdt.dtb`: `tools/fw-memory-check.py firmware/memory-map-zui-18.0.12.104.txt Image-tb323fu-tNN` checks a
kernel you built against .104.

## Changing the devicetree

The board devicetree is in [`kernel/dts/`](../kernel/dts/); the SoC regions come from upstream `kaanapali.dtsi`
in the kernel tree and are changed by label from the board files, as `adspslpi` above:

```dts
/* a region the firmware moved or resized */
&mpss_mem {
	reg = <0x0 0x8ab00000 0x0 0xeb00000>;
};

/* a firmware region the kernel did not reserve before */
&{/reserved-memory} {
	example_fw_mem: example-fw@9a000000 {
		reg = <0x0 0x9a000000 0x0 0x100000>;
		no-map;
	};
};
```

(The addresses here are made up.) Put the change in the `baldur-*.dtsi` file it belongs to, build
([Building your own kernel](custom-kernel.md)), run the check against the new `Image` until it reports nothing, and
try it. A kernel that hangs before the boot summary needs the EDL way back each time.

## Tell us

If your firmware already has an issue ([`firmware` label](https://github.com/joonhoekim/tb323fu-linux/issues?q=is%3Aissue+label%3Afirmware)),
add a comment there; otherwise open one with the
[Firmware report](https://github.com/joonhoekim/tb323fu-linux/issues/new?template=firmware-report.yml) form. Either
way, with:

- the firmware (`ro.build.display.id`) and the kernel release you checked or tried;
- `memory-map.txt` and the output of the check (they hold addresses and the SoC and board ids only; do not attach `fdt.dtb` itself, it can
  contain your device's identifiers);
- what happened: how far the boot got, what the panel showed.

A devicetree change that boots is welcome as a pull request
([CONTRIBUTING](../CONTRIBUTING.md)); a new `firmware/memory-map-zui-<version>.txt` with it helps the next person.
