# Kernel

A patch series on top of a fixed upstream base. There is no kernel fork: clone the base, apply `patches/` in order, build.

## For reviewers

The series by subsystem. Origin, author and status of each patch: [PROVENANCE.md](PROVENANCE.md).

| Area | Patches |
|---|---|
| Display: DPU, DSI, MDSS | 0007, 0008, 0009 (display clocks), 0023, 0029 (MDSS SMMU), 0032, 0050, 0052, 0065, 0079, 0081-0083 (INT2 GDSC), 0097, 0105, 0110 (idle refresh rate) |
| DSI panel and backlight | 0019 (second DSI in the SoC DT), 0020, 0066, 0109 (NT36523 CSOT panel modes), 0028 (AW99706 backlight) |
| DisplayPort and the USB-C combo PHY | 0051, 0057, 0098, 0101, 0102; PHY 0096, 0099, 0100 |
| USB and Type-C | 0036 (NCM gadget), 0059 (USB GDSCs), 0060 (pmic_glink altmode), 0061 (UCSI), 0062, 0104 (dwc3) |
| Power, charging, ADC | 0033, 0107 (qcom_battmgr), 0039 (PMIC5 Gen4 ADC) |
| Audio | 0034, 0035, 0037 (ASoC), 0075-0077 (GPR/APM); the amplifier driver is [out of tree](#out-of-tree-module) |
| Camera | 0013-0018 (CSI2 PHY, linux-next), 0040, 0042, 0043 (CAMSS), 0041, 0045 (sensors), 0044 (CCI pull-up), 0047 (focus motor), 0063, 0064, 0074 (camera PLL), 0116 |
| Video (iris) | 0084-0095, 0111, 0112 |
| GPU | 0001-0006 (GPU DT, queued for v7.4), 0010 and its revert 0080 |
| CPU idle and capacity | 0031, 0106, 0118 (PSCI domains and cluster idle), 0117 (capacity-dmips-mhz) |
| Thermal | 0069, 0108 |
| Storage (UFS) | 0119 (MCQ: multiple I/O queues) |
| Memory bus scaling | 0120-0127 (memlat in CPUCP firmware over the SCMI Qualcomm vendor protocol: DDR, LLCC, DDR_QOS) |
| Clocks and power domains | 0046, 0070-0073, 0103 |
| Wi-Fi, Bluetooth, PCIe | 0024, 0058, 0113 (ath12k), 0030 (Bluetooth), 0049, 0056 (PCIe), 0078 |
| Remoteproc and QRTR | 0067, 0068, 0114 (QRTR name service; the modem) |
| Input, haptics, LEDs | 0025-0027, 0053 (NT36536 touch and pen), 0038 (AW86937 haptics), 0048 (AW22127 LED ring) |
| Storage | 0012 (UFS) |
| Board device tree, config, boot | 0054 (board DTs), 0055 (config fragments), 0011 (defconfig fragment), 0021 (built-in DTB), 0022 (boot progress marks, debug) |

The **Status** column of PROVENANCE.md says where a patch stands upstream:

| Status | Means |
|---|---|
| upstream, upstream (linux-next), upstream (queued for v7.4) | already merged upstream or queued in a maintainer tree; carried until the base includes it |
| pending upstream | someone else's series on the mailing list, not merged yet |
| community, not upstream | from a community tree ([kaanapali-mainline](https://github.com/kaanapali-mainline/linux), [infiniti-mainline](https://github.com/infiniti-mainline/linux)), not posted upstream |
| sent upstream, upstream vN | this project's patch, posted to the mailing list (date, version and review state in the column) |
| upstream candidate | this project's patch, written to be sent but not posted yet |
| local | this project's patch, specific to this board or not ready for upstream |
| local workaround | avoids a problem without fixing its cause (0106, 0118) |
| superseded | a newer upstream or linux-next change (named in the column) does the same |

"0115" is missing on purpose: it is a debugging aid kept out of the series.

## Base

- **Upstream:** torvalds/linux `v7.3-rc4` (`93f51579e7df248780214094418f205253383cc5`).
- **Carried on top, as patches in this series:** the qcom `arm64-for-7.4` kaanapali GPU device tree (0001-0006), picks from the
  [kaanapali-mainline](https://github.com/kaanapali-mainline/linux) community tree (0007-0011), linux-next `next-20260925` picks (0012-0018),
  and further community / mailing-list work — every patch's origin is listed in [PROVENANCE.md](PROVENANCE.md).

## Reproduce the tree

```sh
git clone --depth 1 -b v7.3-rc4 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git linux-tb323fu
cd linux-tb323fu
git am /path/to/tb323fu-linux/kernel/patches/*.patch        # 0001 ... 0127, in order (no 0115)
```

## Configuration

`config/` holds:

| File | What |
|---|---|
| `baldur.fragment` | the board fragment, merged after `arch/arm64/configs/kaanapali-oneplus-infiniti_defconfig` (the OnePlus 15 fragment, added by patch 0011) |
| `baldur-display.fragment` | **required**, merged after `baldur.fragment`: panel driver and backlight built in, the display device tree (`kaanapali-lenovo-baldur-display.dtb`) as the built-in DTB, the touch module — without it the kernel has no panel |
| `baldur-kexec.fragment` | variant for kexec images started from Android (bring-up only) |
| `baldur-netfilter.fragment` | netfilter for distribution firewalls and containers (iptables-nft, firewalld, ipset), merged last; see below |
| `reference.config` | the full `.config` of the kernel on the development tablet ([see below](#the-kernel-on-the-development-tablet)), for comparison |

The same fragments are also added to the tree by patch 0055 (`arch/arm64/configs/`). The copies here have
`CONFIG_INITRAMFS_SOURCE` blanked — set it to your own initramfs (built with [`initramfs/build.sh`](initramfs/)); the version inside patch 0055 still names the path of the original build machine.

<details>
<summary>What <code>baldur-netfilter.fragment</code> enables</summary>

iptables-nft's `nft_compat`, xt matches, REJECT/rpfilter and ipset. The first part is `=m` only and can be added to an
existing build without a new Image. The second part (`NFT_FIB_IPV6`/`NFT_FIB_INET` for firewalld's IPv6 rpfilter,
`NF_CT_NETLINK`, conntrack mark, redirect, the nft bridge and netdev families) changes vmlinux and the core netfilter
modules, so it needs a new Image and the whole matching modules set. Still off: legacy iptables tables, conntrack
zones/labels, ARP tables.

</details>

`kaanapali-oneplus-infiniti_defconfig` is itself a fragment on top of the arm64 `defconfig`, so the order is:

```sh
make ARCH=arm64 LLVM=1 O=out defconfig
C=/path/to/tb323fu-linux/kernel/config
scripts/kconfig/merge_config.sh -m -O out out/.config \
    arch/arm64/configs/kaanapali-oneplus-infiniti_defconfig \
    $C/baldur.fragment $C/baldur-display.fragment $C/baldur-netfilter.fragment
make ARCH=arm64 LLVM=1 O=out olddefconfig
make ARCH=arm64 LLVM=1 O=out -j"$(nproc)" Image dtbs modules
```

## Out-of-tree module

The speakers need the Awinic aw882xx amplifier driver, which is not in upstream Linux. Its GPL-2.0 source is in
[`out-of-tree/aw882xx/`](out-of-tree/aw882xx/); build it against the same kernel build and install it in `extra/`:

```sh
make -C linux-tb323fu O=out ARCH=arm64 LLVM=1 M=$PWD/kernel/out-of-tree/aw882xx CONFIG_SND_SOC_AW882XX=m modules
# -> snd-soc-aw882xx.ko into /lib/modules/<release>/extra/, then depmod
```

## Device tree

`dts/` has the board files as they are after the whole series is applied (they are added by patches 0054, 0064, 0078, 0095 and 0108):
`kaanapali-lenovo-baldur.dts` (the board), `kaanapali-lenovo-baldur-display.dts` (the one the tested kernel uses), `kaanapali-lenovo-baldur-kexec.dts`,
and the `baldur-*.dtsi` pieces they include. The SoC side (`kaanapali.dtsi`) comes from upstream plus the series.

## Built-in device tree and fixed command line

The bootloader on this tablet cannot be made to pass our device tree or command line, so the kernel carries both:

- **Built-in DTB** (patch 0021, `CONFIG_ARM64_BUILTIN_FDT=y`, `CONFIG_ARM64_BUILTIN_FDT_NAME="qcom/kaanapali-lenovo-baldur-display.dtb"`):
  the kernel ignores the DTB the bootloader hands over and uses the one linked into the Image.
- **Fixed command line** (`CONFIG_CMDLINE_FORCE=y`): the command line in the boot image header is ignored. The tested kernel uses:

  ```
  icc-rpmh.qos_disable=1 icc_rpmh.qos_disable=1 fw_devlink.sync_state=timeout fbcon=font:TER16x32 panic=10 oops=panic
  console=tty1 keep_bootcon qcom_scm.download_mode=full baldur.end=hold consoleblank=120 no_console_suspend
  mem_sleep_default=s2idle cpuidle_psci_domain.allow_cluster_off=0 baldur.diag=0
  ```

  <details>
  <summary>Why each option is there</summary>

  | Option | Why |
  |---|---|
  | `icc-rpmh.qos_disable=1`, `icc_rpmh.qos_disable=1` | leave the interconnect QoS settings the bootloader programmed (built-in and module spelling) |
  | `fw_devlink.sync_state=timeout` | let providers finish `sync_state` after a timeout even if a consumer never probes |
  | `fbcon=font:TER16x32` | a readable console font on the 3040x1904 panel |
  | `panic=10`, `oops=panic` | reboot 10 s after any oops/panic instead of hanging |
  | `console=tty1`, `keep_bootcon` | kernel messages on the panel, keeping the early boot console |
  | `qcom_scm.download_mode=full` | on a crash, stop in the Qualcomm dump mode so a full memory dump can be taken |
  | `baldur.end=hold`, `baldur.diag=0` | knobs of the initramfs `/init`: `baldur.end=hold` = normal boot with root selection; `baldur.diag=0` is obsolete and ignored — see [initramfs/README.md](initramfs/README.md) |
  | `consoleblank=120` | blank the text console after 2 minutes |
  | `no_console_suspend` | keep the console alive across suspend (debugging) |
  | `mem_sleep_default=s2idle` | the only system sleep state this platform supports |
  | `cpuidle_psci_domain.allow_cluster_off=0` | patch 0118: no CPU cluster idle state in runtime idle, which avoids the resets it caused when idle (3 h without one vs 2 in 78 min, same display-off power); the cause in the cluster's power-down is not fixed, and s2idle still uses the state. `1` = upstream behavior; can be switched at run time in `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off` |

  </details>

## The kernel on the development tablet

The kernel running on the development tablet (config: `config/reference.config`) is this series (0001-0118, no 0115) **plus test-only changes that are not part of the series**:

- a knob exposing experimental panel modes (vendor 144/165 Hz and a 143 Hz variant; only 90 and 164 Hz are in the series, 0109),
- an idle-state flight recorder for crash analysis, and a knob that adds a delay in the cluster idle path (used to narrow down the idle resets),
- modem (MPSS) device tree nodes, and remoteproc knobs used to investigate GNSS (the tablet appears to have no usable GNSS antenna),
- a q6apm change that polls for the audio framework instead of a fixed 5 s wait (an upstream candidate, not yet in the series).

Default knobs in the series: the idle refresh rate policy is automatic (`msm.idle_refresh_policy=2`, patch 0110); the CPU cluster idle states are refused in runtime idle (`cpuidle_psci_domain.allow_cluster_off=0` on the built-in command line, patch 0118) — it avoids the idle resets that state caused, without fixing their cause; set `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off` to 1 to use them again.
All of it is built from one tree (no out-of-tree rebuilds of in-tree drivers any more); the only out-of-tree module is the aw882xx speaker amplifier driver, installed in `/lib/modules/<version>/extra/` (source and build: [out-of-tree/aw882xx](out-of-tree/aw882xx/)).

## Licensing

Patches and kernel sources are `GPL-2.0-only` (device trees keep their own SPDX identifiers). Imported patches keep their authors and `Signed-off-by` lines.
