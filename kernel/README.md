# Kernel

A patch series on top of a fixed upstream base. There is no kernel fork: clone the base, apply `patches/` in order, build.

## Base

- **Upstream:** torvalds/linux `v7.3-rc4` (`93f51579e7df248780214094418f205253383cc5`).
- **Carried on top, as patches in this series:** the qcom `arm64-for-7.4` kaanapali GPU device tree (0001-0006), picks from the
  [kaanapali-mainline](https://github.com/kaanapali-mainline/linux) community tree (0007-0011), linux-next `next-20260925` picks (0012-0018),
  and further community / mailing-list work — every patch's origin is listed in [PROVENANCE.md](PROVENANCE.md).

## Reproduce the tree

```sh
git clone --depth 1 -b v7.3-rc4 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git linux-tb323fu
cd linux-tb323fu
git am /path/to/tb323fu-linux/kernel/patches/*.patch        # 0001 ... 0118, in order (no 0115)
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
  | `cpuidle_psci_domain.allow_cluster_off=0` | **stopgap, not a fix** (patch 0118): no CPU cluster idle states in runtime idle, which avoids the rare idle crash (3 h without a crash vs 2 in 78 min, same display-off power); s2idle still uses them. `1` = upstream behaviour; can be switched at run time in `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off` |

  </details>

## The kernel on the development tablet

The kernel running on the development tablet (config: `config/reference.config`) is this series (0001-0118, no 0115) **plus test-only changes that are not part of the series**:

- a knob exposing experimental panel modes (vendor 144/165 Hz and a 143 Hz variant; only 90 and 164 Hz are in the series, 0109),
- an idle-state flight recorder for crash analysis, and a knob that adds a delay in the cluster idle path (to test the race theory of the idle crash),
- modem (MPSS) device tree nodes, and remoteproc knobs used to investigate GNSS (the tablet appears to have no usable GNSS antenna),
- a q6apm change that polls for the audio framework instead of a fixed 5 s wait (an upstream candidate, not yet in the series).

Default knobs in the series: the idle refresh rate policy is automatic (`msm.idle_refresh_policy=2`, patch 0110); the CPU cluster idle states are refused in runtime idle (`cpuidle_psci_domain.allow_cluster_off=0` on the built-in command line, patch 0118) — a stopgap that avoids the idle crash, not a fix of its cause; set `/sys/module/cpuidle_psci_domain/parameters/allow_cluster_off` to 1 to use them again.
All of it is built from one tree (no out-of-tree rebuilds of in-tree drivers any more); the only out-of-tree module is the aw882xx speaker amplifier driver, installed in `/lib/modules/<version>/extra/` (source and build: [out-of-tree/aw882xx](out-of-tree/aw882xx/)).

## Licensing

Patches and kernel sources are `GPL-2.0-only` (device trees keep their own SPDX identifiers). Imported patches keep their authors and `Signed-off-by` lines.
