# Provenance

Where every patch in `patches/` comes from. The series applies with `git am` on the base below.
Imported patches keep their original authors and `Signed-off-by` lines. Where `From:` names the
importer instead of the original authors (squashed imports: 0024, 0030, 0039-0041), the origin
column names the source; see that tree's history for the individual authors.

## Base

| What | Where |
|---|---|
| upstream base | torvalds/linux `v7.3` = `c03c1a14507dfaf8b856c2608f9ac216021fc222` |
| qcom kaanapali GPU DT | qcom `arm64-for-7.4` (patches 0001-0006) |
| linux-next picks | `next-20260925` (patches 0012-0018) |
| community tree | [kaanapali-mainline/linux](https://github.com/kaanapali-mainline/linux) `testing` at `37a76066` (patches 0007-0011) |
| community tree | [infiniti-mainline/linux](https://github.com/infiniti-mainline/linux) `oneplus-15` / `master` (patches 0024, 0029, 0030, 0072-0077, 0084-0094, 0096, 0099-0102) |
| touch driver | [map220v/sm8850-mainline](https://github.com/map220v/sm8850-mainline) `iceland-7.2` (0025) |

## Patches

Numbers 0067-0071 and 0140 are free: those patches are in v7.3 and were dropped when the base moved from v7.3-rc4 to v7.3.

`sha256` is the first 16 hex digits of the SHA-256 of the patch file as stored here.
0111-0114, 0116-0119, 0128-0139, 0141-0156 and 0200-0203 and 0205 are this project's own patches, signed off by the author (`Signed-off-by: Joonhoe Kim`).

| # | Subject | Author (`From:`) | Origin | Status | sha256 |
|---|---|---|---|---|---|
| `0001` | arm64: dts: qcom: kaanapali: add the GPU SMMU node | Qingqing Zhou | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `42401966c62ac534` |
| `0002` | arm64: dts: qcom: kaanapali: Add QFPROM node | Jingyi Wang | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `722ffe8a82b93f4f` |
| `0003` | arm64: dts: qcom: Add GPU support for Kaanapali | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `58f5b9c2ab7984ce` |
| `0004` | arm64: dts: qcom: kaanapali: Add GPU cooling | Gaurav Kohli | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `a30993812a2e1f58` |
| `0005` | arm64: dts: qcom: kaanapali-mtp: Enable GPU | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `bccf2aea2cb354a8` |
| `0006` | arm64: dts: qcom: kaanapali-qrd: Enable GPU | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `63e30b41fa15bff6` |
| `0007` | drm/msm: DPU, DSI and MDSS fixes for AA601 cmd-mode DSC | idusergod | community [kaanapali-mainline](https://github.com/kaanapali-mainline/linux) testing `b6eab7f0e8` (partial) | community, not upstream | `fa8c875792a5ca27` |
| `0008` | drm/msm: cmd-mode DSC 1.2 fixes for DPU 13 (Kaanapali) | EYC | community kaanapali-mainline `144a400716` | community, not upstream | `b9125ca15316235e` |
| `0009` | clk: qcom: dispcc-kaanapali: solve display artifacts at start | Nazar Kompanets | community kaanapali-mainline `6ba50855c6` | community, not upstream | `c406142c60f2855d` |
| `0010` | drm/msm/adreno: A840: drop IFPC quirk for Infiniti bring-up | idusergod | community kaanapali-mainline `6a4127a8cd` (reverted by 0080) | community; net no-op with 0080 | `b76f63ca32f93327` |
| `0011` | arm64: configs: add kaanapali-oneplus-infiniti_defconfig fragment | idusergod | community kaanapali-mainline `37a7606647` (defconfig fragment) | community; base of the baldur fragment | `77d96d7fca1ff66c` |
| `0012` | scsi: ufs: ufs-qcom: Enable only lane clocks in lane clock APIs | Nitin Rawat | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `e2b5d24935893131` |
| `0013` | phy: qcom-mipi-csi2: Add a CSI2 MIPI DPHY driver | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `0e0fb25f69658143` |
| `0014` | phy: core: Fix race-condition between _of_phy_get() and try_module_get() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `cf59f474def9a7f6` |
| `0015` | phy: core: Add phy_get_by_of_node() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `c4d580da2049f373` |
| `0016` | phy: core: Add devm_phy_get_by_of_node() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `817f8719483fe113` |
| `0017` | phy: core: Add missing kerneldoc colon in two locations | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `b8cbeb9396891401` |
| `0018` | dt-bindings: phy: qcom: Add CSI2 C-PHY/DPHY schema | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `8aa24a69edcaf633` |
| `0019` | arm64: dts: qcom: kaanapali: add the second DSI controller and PHY | Joonhoe Kim | this project | local | `c769986050891505` |
| `0020` | drm/panel: nt36523: add DSC, and the Lenovo TB323FU CSOT panel | Joonhoe Kim | this project | local | `c3acca2bc457c0a3` |
| `0021` | arm64: carry a devicetree inside the kernel image | Joonhoe Kim | this project | local (built-in DTB; bootloader workaround) | `1128b471f7e3048a` |
| `0022` | arm64: paint boot progress marks into the bootloader's framebuffer | Joonhoe Kim | this project | local (debug: boot progress marks) | `24cfc04be4e0288f` |
| `0023` | drm/msm: video-mode DSC at 10 bpc -- make DSI and the DPU INTF agree on the line | Joonhoe Kim | this project | local; its DPU part applied to msm-next 2026-10-06 as "drm/msm/dpu: round up the compressed INTF width" (`6b93e8b453fa`, [lore](https://lore.kernel.org/all/20261004102758.84436-1-26rote@gmail.com/)) | `08b85820784bba6e` |
| `0024` | wifi: ath12k + PCI/pwrctrl: WCN7860 ("peach", 17cb:110e) | Joonhoe Kim | [infiniti-mainline](https://github.com/infiniti-mainline/linux) `oneplus-15`, several authors, imported squashed (From: is the importer) | community, not upstream | `99cb62ac3a92f820` |
| `0025` | Input: touchscreen: import NT36536 host-download SPI driver (Novatek) | Joonhoe Kim | [map220v/sm8850-mainline](https://github.com/map220v/sm8850-mainline) `iceland-7.2` (Novatek vendor driver, verbatim) | out of tree, not upstreamable | `8050b5ef880e4283` |
| `0026` | input: touchscreen: NT36536 host-download SPI driver (Novatek) | Joonhoe Kim | this project | local | `96c5007c163182ff` |
| `0027` | input: nt36536: fixes for TB323FU | Joonhoe Kim | this project | local | `2ce1506e72ecb90e` |
| `0028` | backlight: aw99706: replay board registers after the HWEN reset | Joonhoe Kim | this project | local | `ddeb5f76b5aed5a4` |
| `0029` | iommu: arm-smmu-qcom: kaanapali MDSS identity domain | EYC | community / infiniti-mainline (EYC) | community, not upstream | `4ae8cd543e5d551c` |
| `0030` | Bluetooth: qca: WCN7860 (BRAHMA) on the WCN7850 flow | Joonhoe Kim | infiniti-mainline `fecd7925d` (EYC; From: is the importer) | community, not upstream | `a41176230396e02d` |
| `0031` | arm64: dts: qcom: kaanapali: CPU6-7 in a PSCI cluster domain of their own | Joonhoe Kim | this project | local | `e5f9ac398dc793d0` |
| `0032` | drm/msm/dsi: phy: prepare the iface clock only while runtime active | Joonhoe Kim | this project | local | `89fb07030a452127` |
| `0033` | power: supply: qcom_battmgr: let the host set the USB input current limit | Joonhoe Kim | this project | sent upstream 2026-10-07, under review ([lore](https://lore.kernel.org/all/20261007123939.1679-1-26rote@gmail.com/)) | `15d76e4beb9e0e67` |
| `0034` | ASoC: qcom: sc8280xp: optional S32_LE on MI2S backends | Joonhoe Kim | this project | local | `0a0ea44be2cff789` |
| `0035` | ASoC: codecs: lpass-tx-macro: board tuning of the decimator filter block | Joonhoe Kim | this project | local | `a2c42b9833425881` |
| `0036` | usb: gadget: f_ncm: restart the TX timer when the freelist is empty | Joonhoe Kim | this project | local | `a14d12dc20d08fb1` |
| `0037` | ASoC: codecs: wcd939x: read the ADC/DMIC switches per channel | Joonhoe Kim | this project | applied upstream 2026-10-12 (ASoC `for-7.4`, `e7860dc5d8d4`, [lore](https://lore.kernel.org/all/20261007123954.1709-1-26rote@gmail.com/)) | `532cb19ba893beee` |
| `0038` | Input: aw86927 - accept the AW86937 | Joonhoe Kim | this project | local | `6b7c1b306e3d1ad6` |
| `0039` | pmic5-gen4-adc: mailing-list series (squashed) | Joonhoe Kim | mailing list: PMIC5 Gen4 ADC series (Jishnu Prakash), squashed | pending upstream | `6f8b4db43443da58` |
| `0040` | media-qcom-camss-kaanapali-v16: mailing-list series (squashed) | Joonhoe Kim | mailing list: CAMSS kaanapali v16 (Hangxiang Ma), squashed | pending upstream | `022134299180050c` |
| `0041` | media-i2c-s5kjn5-v4: mailing-list series (squashed) | Joonhoe Kim | mailing list: S5KJN5 sensor v4 (Wenmeng Liu), squashed | pending upstream | `e800e4554ddb6dd9` |
| `0042` | media: qcom: camss: Add support for PHY API devices | Bryan O'Donoghue | mailing list: camss PHY API series 4/5 (Bryan O'Donoghue) | pending upstream | `7ee2ef5e09abcf35` |
| `0043` | media: qcom: camss: Use data-lanes starting at 1 for new CSIPHY mode | Bryan O'Donoghue | mailing list: camss PHY API series 5/5 (Bryan O'Donoghue) | pending upstream | `ed38f859a99a4158` |
| `0044` | pinctrl: qcom: kaanapali: add the I2C strong pull-up bit | Joonhoe Kim | this project | applied upstream 2026-10-10 (`1df7f369075d`, pinctrl tree; not in v7.3; [lore](https://lore.kernel.org/all/20261007124001.1724-1-26rote@gmail.com/)) | `6b27df1a5ef5a398` |
| `0045` | media: i2c: add Samsung S5KJNS and GalaxyCore GC08A8 sensor drivers | Joonhoe Kim | this project | local | `fdcb38768c250fb8` |
| `0046` | clk: qcom: gdsc: keep the clock controller active while a GDSC is on | Joonhoe Kim | this project | local | `9bb2a14bb5459c3f` |
| `0047` | media: i2c: dw9719: add Giantec GT9764, optional vio supply | Joonhoe Kim | this project | local | `a176107bcbe864d2` |
| `0048` | leds: rgb: add Awinic AW22127 driver | Joonhoe Kim | this project | local | `a7d3dffb6dae6a0d` |
| `0049` | PCI: qcom: don't advertise a hot-plug slot on the Root Port | Joonhoe Kim | this project | local | `be149d22c826a2c4` |
| `0050` | drm/msm/dsi: phy: runtime suspend the PHY over system sleep | Joonhoe Kim | this project | local | `7cf82cedd7d85a15` |
| `0051` | drm/msm/dp: add kaanapali (DP0 descriptor) | Joonhoe Kim | this project | local | `2322d7fe7058b5b6` |
| `0052` | drm/msm/dpu: add the SSPP rec0/rec1 blocks to the snapshot | Joonhoe Kim | this project | sent upstream 2026-10-07, reviewed ([lore](https://lore.kernel.org/all/20261007123945.1694-1-26rote@gmail.com/)) | `7928b4b5ebef95d8` |
| `0053` | input: nt36536: make the pen a tablet libinput accepts | Joonhoe Kim | this project | local | `8b0e4c4efaaecc8c` |
| `0054` | arm64: dts: qcom: add Lenovo Legion Tab Y700 gen5 (TB323FU, baldur) board DTs | Joonhoe Kim | this project | local | `beb900dd9a382dc3` |
| `0055` | arm64: configs: add the baldur config fragments | Joonhoe Kim | this project | local | `ad5a02f65cb534fe` |
| `0056` | PCI: qcom: parse iommu-map with the target #iommu-cells | Joonhoe Kim | this project | local; superseded by linux-next (`qcom_pcie_config_sid_1_9_0`) | `ab5de4888eaeef17` |
| `0057` | drm/msm/dp: retrain the link on a quick replug while streaming | Joonhoe Kim | this project | local | `9649c3d9940a0dff` |
| `0058` | wifi: ath12k: don't wake the device over MHI from the panic notifier | Joonhoe Kim | this project | local | `e1c0e92d685b4eab` |
| `0059` | clk: qcom: gcc-kaanapali: keep the USB GDSCs in retention | Joonhoe Kim | this project | local | `5b1e7c9ecc20d714` |
| `0060` | soc: qcom: pmic_glink_altmode: handle notifications on the freezable workqueue | Joonhoe Kim | this project | local; superseded upstream (`7d0767c5cd87`, freezable workqueue) | `e0d41b43de28a1b5` |
| `0061` | usb: typec: ucsi: run connector change handling on the freezable workqueue | Joonhoe Kim | this project | local | `d56dd5766fa29a9f` |
| `0062` | usb: dwc3: qcom: arm the eUSB2 line interrupts for wakeup | Joonhoe Kim | this project | local | `8e5c98291d12bfcc` |
| `0063` | media: i2c: s5kjns: the colour filter order is GRBG, not GBRG | Joonhoe Kim | this project | local | `873a6f4f757173d1` |
| `0064` | arm64: dts: qcom: baldur: camera orientation and rotation | Joonhoe Kim | this project | local | `d861f43508423bc2` |
| `0065` | drm/msm/dpu+dsi: change the vertical front porch in place | Joonhoe Kim | this project | local | `ac04a5c64afaa68f` |
| `0066` | drm/panel: nt36523: baldur CSOT 60 and 30 Hz modes | Joonhoe Kim | this project | local | `41d9e5bb28f8772d` |
| `0072` | clk: qcom: gcc: Set FORCE_MEM_CORE_ON/FORCE_PERIPH_CORE_ON for PCIe pipe clocks | Qiang Yu | mailing list v1 (Qiang Yu), via infiniti-mainline | pending upstream | `544092ad8ddcd01c` |
| `0073` | clk: qcom: branch: Turn off a branch that failed to enable | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `3a5923403a112fd3` |
| `0074` | clk: qcom: Fix camera rivian PLL configuration settings | Jagadeesh Kona | upstream (Jagadeesh Kona), via infiniti-mainline | upstream | `cece4e2ff6b399ef` |
| `0075` | soc: qcom: apr: Register the GPR callback before probing | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `254078b0be2a4faf` |
| `0076` | ASoC: qcom: q6apm: Send commands from the APM's GPR service | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `a2ad4a083e555cd7` |
| `0077` | arm64: dts: qcom: kaanapali: Advertise a 1024-byte GPR intent | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `79d4aab5ea4cdc9b` |
| `0078` | arm64: dts: qcom: baldur: WCN7860 WAKE# is active low | Joonhoe Kim | this project | local | `887de4b5c27c101a` |
| `0079` | drm/msm/dpu: keep the full modeset when colour management changed | Joonhoe Kim | this project | local | `8d246df497d58c28` |
| `0080` | Revert "drm/msm/adreno: A840: drop IFPC quirk for Infiniti bring-up" | Joonhoe Kim | this project | local | `8f7e5c4abe7923c5` |
| `0081` | dt-bindings: display: msm: Add INT2 GDSC to Kaanapali DPU | Yongxing Mou | mailing list: Kaanapali DPU INT2 GDSC v2 (Yongxing Mou) | pending upstream | `13b935a8078915fd` |
| `0082` | drm/msm/dpu: Attach INT2 power domain alongside MMCX on Kaanapali | Yongxing Mou | mailing list: Kaanapali DPU INT2 GDSC v2 (Yongxing Mou) | pending upstream | `601381c34ae6c0ee` |
| `0083` | arm64: dts: qcom: Add INT2 GDSC to Kaanapali DPU | Yongxing Mou | mailing list: Kaanapali DPU INT2 GDSC v2 (Yongxing Mou) | pending upstream | `9de0029dfb625d0c` |
| `0084` | media: dt-bindings: qcom-kaanapali-iris: Add kaanapali video codec binding | Vikash Garodia | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `12c4fb1b80a7e345` |
| `0085` | media: iris: add AV1 decode buffer size support for vpu4x | Wangao Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `16574150664b5a9b` |
| `0086` | media: iris: Add platform data for kaanapali | Vikash Garodia | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `1f3b365e662c472c` |
| `0087` | media: iris: add iris4 specific H265 line buffer calculation | Vikash Garodia | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `76b6511386377d5c` |
| `0088` | media: iris: Release the MVP NoC low-power request on vpu4x power off | Victor Fuentes | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `8a506a98a671395e` |
| `0089` | arm64: dts: qcom: kaanapali: Add iris video node | Jingyi Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `4685a36c167e1d12` |
| `0090` | media: iris: Fix bus_info prefix in VIDIOC_QUERYCAP | Vishnu Reddy | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `78b925b1f4546694` |
| `0091` | media: iris: fix VPSS line buffer width and height order | Wangao Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `08cea84bc1a8bf27` |
| `0092` | media: iris: fix VPU4x encoder line buffer size for rotation | Wangao Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `8fa80d8ae62592ca` |
| `0093` | media: iris: Only allow hierarchical B-frames on kaanapali | Victor Fuentes | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `29dc45b4eabf0c44` |
| `0094` | media: iris: Don't report the picture state on decoder input buffers | Victor Fuentes | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `86cfcbd0cf24fbec` |
| `0095` | arm64: dts: qcom: baldur: enable the iris video codec | Joonhoe Kim | this project | local | `ea202ed02a3e9193` |
| `0096` | phy: qcom: qmp-combo: Add Kaanapali USB3+DP PHY | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `1e0e578ff21fb961` |
| `0097` | drm/msm/dpu: disable the slave encoder before the master | Joonhoe Kim | this project | applied to msm-next 2026-10-06 (`32d04280ddeb`, [lore](https://lore.kernel.org/all/20261002130712.50612-1-26rote@gmail.com/)) | `412d487ac73e0de4` |
| `0098` | drm/msm/dp: hold one runtime PM reference per plugged state | Joonhoe Kim | this project | upstream v2 2026-10-08, under review ([lore](https://lore.kernel.org/all/20261007155848.106-1-26rote@gmail.com/)) | `c40d7ae1222c1a85` |
| `0099` | phy: qcom: qmp-combo: Drop the stale err_disable_pipe_clk teardown | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `c38d35be6e577f86` |
| `0100` | phy: qcom: qmp-combo: Keep the DP lanes of a USB-capable DP sink | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `bc25054239f26b93` |
| `0101` | drm/msm/dp: check the PHY power-on and DPCD link status returns | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `7dcd497771e0649d` |
| `0102` | drm/msm/dp: reset the link caps on every DPCD read | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `e8b2f54c35c9e802` |
| `0103` | clk: qcom: gdsc: drop the controller reference only if it was taken | Joonhoe Kim | this project | local | `cac3ca5aa1071443` |
| `0104` | usb: dwc3: tell xHCI that it lost its state when the core is powered off | Joonhoe Kim | this project | sent upstream 2026-09-29, under review | `241ae504c37a9442` |
| `0105` | drm/msm/dpu: compute the CRTC bandwidth from the state being checked | Joonhoe Kim | this project | applied to msm-next 2026-10-06 (`480ba24aa340`, [lore](https://lore.kernel.org/all/20261002130656.50577-1-26rote@gmail.com/)) | `d308e4ba73faca78` |
| `0106` | cpuidle: psci-domain: baldur: use the system domain state only in system suspend | Joonhoe Kim | this project | local workaround (cpuidle system domain state only in suspend) | `e5fe440e8cc5bdfb` |
| `0107` | power: supply: qcom_battmgr: fix the battery current sign on Kaanapali | Joonhoe Kim | this project | sent upstream 2026-10-16 (1/2 with 0132, [lore](https://lore.kernel.org/all/20261016124806.1789-1-26rote@gmail.com/)) | `0a838c98a286a567` |
| `0108` | arm64: dts: qcom: baldur: throttle on the board temperature like Android | Joonhoe Kim | this project | local (board DT) | `13f0e85d57cb3439` |
| `0109` | drm/panel: nt36523: baldur: add 90 Hz and 164 Hz modes | Joonhoe Kim | this project | local (panel modes) | `f07f63856a202e8b` |
| `0110` | drm/msm/dpu: baldur: lower the refresh rate in the kernel when idle | Joonhoe Kim | this project | local, not intended for upstream | `5000b52c101ab3d9` |
| `0111` | media: iris: vpu4x: size the decoder OPB line buffer for 10-bit output | Joonhoe Kim | this project | upstream candidate | `6658d64632accd9e` |
| `0112` | media: iris: don't wait for the threaded IRQ handler under core->lock | Joonhoe Kim | this project | upstream candidate | `09cb4d1e2ba7c805` |
| `0113` | wifi: ath12k: keep the RX refill ring from running dry | Joonhoe Kim | this project | its RX buffer leak fix sent upstream 2026-10-16 as "wifi: ath12k: replenish the RX buffers reaped without a complete MSDU" ([lore](https://lore.kernel.org/all/20261016124620.76-1-26rote@gmail.com/)); the rest local | `b7e380ed216b2ebe` |
| `0114` | net: qrtr: announce local services again once our HELLO is sent | Joonhoe Kim | this project | sent upstream (v3, 2026-10-29, [lore](https://lore.kernel.org/all/20261029105142.2503-1-26rote@gmail.com/)) | `caf9d42e6df663d2` |
| `0116` | media: qcom: camss: hold the bandwidth vote while TITAN_TOP is powered | Joonhoe Kim | this project | upstream candidate (0115 is a debugging aid kept out of the series) | `3b8ea94fd19e5fc5` |
| `0117` | arm64: dts: qcom: kaanapali: add CPU capacity-dmips-mhz | Joonhoe Kim | this project | upstream candidate | `cde3fbe0e37a205a` |
| `0118` | cpuidle: psci-domain: add allow_cluster_off to refuse cluster idle states | Joonhoe Kim | this project | local workaround: no cluster idle state in runtime idle on baldur, where its power-down reset the SoC when idle (set by the config fragment's command line; the cause is not fixed) | `cd890a6b3497f17e` |
| `0119` | arm64: dts: qcom: kaanapali: describe the UFS MCQ registers | Joonhoe Kim | this project | upstream candidate | `28e15806d26509c5` |
| `0120` | firmware: arm_scmi: Add SCMI QCOM Generic Extension Protocol documentation | Pragnesh Papaniya | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (1/10) | pending upstream (RFC) | `541a3ead9d4abaeb` |
| `0121` | dt-bindings: firmware: arm,scmi: Add Qualcomm Generic Extension Protocol | Pragnesh Papaniya | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (2/10) | pending upstream (RFC) | `af24d91cd6e10cce` |
| `0122` | firmware: arm_scmi: vendors: Add QCOM SCMI Generic Extensions | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (3/10) | pending upstream (RFC) | `e6d0c2af8acd1bd0` |
| `0123` | PM / devfreq: Add new target_freq attribute flag for governors | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (4/10) | pending upstream (RFC) | `355c4dd2ff7a7db3` |
| `0124` | PM / devfreq: Add new track_remote flag for governors | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (5/10) | pending upstream (RFC) | `b5f80c36ac1792c6` |
| `0125` | PM / devfreq: Add a governor for tracking remote device frequencies | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (6/10) | pending upstream (RFC) | `aacfce78d0823416` |
| `0126` | PM / devfreq: Introduce the QCOM SCMI Memlat devfreq driver | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (7/10) | pending upstream (RFC) | `04130953d6e121a0` |
| `0127` | arm64: dts: qcom: kaanapali: Enable LLCC/DDR/DDR_QOS DVFS | Jia Yang | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (10/10) | pending upstream (RFC) | `9adfa16d38f8f769` |
| `0128` | clk: qcom: gcc-kaanapali: Enable FORCE_MEM_CORE_ON for UFS AXI PHY clock | Joonhoe Kim | this project | sent upstream 2026-10-07, reviewed by Abel Vesa and Taniya Das ([lore](https://lore.kernel.org/all/20261007143716.2161-1-26rote@gmail.com/)) | `339344f78c87f8df` |
| `0129` | scsi: ufs: core: Fix a deadlock between clock scaling and device commands | Joonhoe Kim | this project | upstream (queued for v7.4, scsi `7.4/scsi-queue`; the applied version also releases the locks in reverse order, [lore](https://lore.kernel.org/all/20261016124608.46-1-26rote@gmail.com/)) | `b36fb8b36b0b0baf` |
| `0130` | Bluetooth: qca: check the device address on every setup | Joonhoe Kim | this project | sent upstream 2026-10-16 ([lore](https://lore.kernel.org/all/20261016124615.61-1-26rote@gmail.com/)) | `48c51137866a015b` |
| `0131` | drm/msm/dpu: only force a modeset for colour management when a DSPP is missing | Joonhoe Kim | this project | not sent: Dmitry Baryshkov's "drm/msm: fix SMMU fault dumps" v4 7/10 and 9/10 do the same; Tested-by sent 2026-10-16 ([lore](https://lore.kernel.org/all/20261016124055.1232-1-26rote@gmail.com/)) | `2ced778647db0a8e` |
| `0132` | power: supply: qcom_battmgr: expose the charge current limit | Joonhoe Kim | this project | sent upstream 2026-10-16 (2/2 with 0107; the posted version names the properties charge_control_limit and charge_control_limit_max, [lore](https://lore.kernel.org/all/20261016124806.1789-2-26rote@gmail.com/)) | `4d91da7fe124553f` |
| `0133` | phy: qcom: qmp-combo: Leave the USB PCS alone in DP-only mode | Joonhoe Kim | this project | sent upstream (v3 for phy-next, 2026-11-03, [lore](https://lore.kernel.org/all/20261103135051.1668-1-26rote@gmail.com/); this file is the v7.3 version) | `479f6b426bba70d1` |
| `0134` | usb: typec: mux: fsa4480: keep the USB 2.0 switch on in DP-only modes | Joonhoe Kim | this project | upstream (usb `usb-linus`, [lore](https://lore.kernel.org/all/20261022165143.1327-1-26rote@gmail.com/)) | `fbeb2dee06db7008` |
| `0135` | dt-bindings: usb: qcom,snps-dwc3: Add mode-switch | Joonhoe Kim | this project | upstream candidate (with 0136 and the kaanapali.dtsi `mode-switch` on the controller) | `c20c71bcef2d89f7` |
| `0136` | usb: dwc3: qcom: Use the UTMI clock as PIPE clock in DisplayPort-only modes | Joonhoe Kim | this project | upstream candidate | `fda21dc98c794d1d` |
| `0137` | drm/msm/dp: Don't redo the plug handling for a sink that is set up | Joonhoe Kim | this project | local: overlaps DisplayPort series under review upstream | `819ebe5e8eb4019d` |
| `0138` | arm64: dts: qcom: kaanapali: Mark the USB3-DP PHY as a Type-C mode switch | Joonhoe Kim | this project | local: Balaji Kalkonda's "arm64: dts: qcom: kaanapali: Add DisplayPort controller" v2 3/5 adds the same property | `939a0aefc12d2cd4` |
| `0139` | arm64: dts: qcom: baldur: Four-lane HBR3 DisplayPort | Joonhoe Kim | this project | local (board device tree: four-lane HBR3 DisplayPort) | `71840cf5f71c19fb` |
| `0141` | scsi: ufs: qcom: Reprogram ESI after a host controller reset | Joonhoe Kim | this project | sent upstream 2026-10-23 ([lore](https://lore.kernel.org/all/20261022165006.1275-1-26rote@gmail.com/)) | `032cb5247e104b7e` |
| `0142` | drm/panel: nt36523: baldur: add a 144 Hz mode | Joonhoe Kim | this project | local (panel modes) | `cbd2e18aa32c711c` |
| `0143` | input: nt36536: accept pen report format 0x02 | Joonhoe Kim | this project | local (Lenovo Tab Pen Plus, issue #16) | `548f89b3ad2ac757` |
| `0144` | drm/panel: nt36523: baldur: report the panel physical size | Joonhoe Kim | this project | local (panel; issue #16) | `57c3723e43a47e88` |
| `0145` | input: nt36536: ignore pen ID reports and reports outside the axes | Joonhoe Kim | this project | local (Lenovo Tab Pen Plus, issue #16) | `b2b5cfdb8e160700` |
| `0146` | input: nt36536: report the pen pressure range 0..4095 | Joonhoe Kim | this project | local (Lenovo Tab Pen Plus, issue #16) | `d6a9beb22c7d422b` |
| `0147` | usb: typec: ucsi: Leave the mux to the altmode driver for alt mode partners | Joonhoe Kim | this project | upstream candidate | `7353ccb93231c5ec` |
| `0148` | arm64: configs: baldur: drop no_console_suspend from the command line | Joonhoe Kim | this project | local | `bd9ca58e00dadbfa` |
| `0149` | usb: dwc3: qcom: Leave DP-only mode from the mux callback | Joonhoe Kim | this project | on top of 0136; same behaviour as v2 sent upstream (2026-11-03, [lore](https://lore.kernel.org/all/20261103135057.1682-1-26rote@gmail.com/)) | `fb3f6ce16e9f81a3` |
| `0150` | drm/msm/dp: Track the PHY power and link clock state | Joonhoe Kim | this project, based on Xilin Wu's msm-dp-hdr10 v2 08/20 and part of 09/20 ([lore](https://lore.kernel.org/all/20261009-msm-dp-hdr10-v2-8-1835d4966da3@radxa.com/), not merged) | local | `03f6d92dd65c8f0f` |
| `0151` | drm/msm/dp: Release the PHY init when the link is not powered | Joonhoe Kim | this project | upstream candidate | `3c06f42319c69716` |
| `0152` | mmc: sd: Keep the card when its extension registers fail | Joonhoe Kim | this project, fix suggested by the reporter of [#26](https://github.com/joonhoekim/tb323fu-linux/issues/26) | upstream candidate | `5fc19deafa742e07` |
| `0153` | input: nt36536: report the pen tilt in the screen's axes | Joonhoe Kim | this project, fix by the reporter of [#16](https://github.com/joonhoekim/tb323fu-linux/issues/16) | local | `82e75995b4cb964c` |
| `0154` | leds: rgb: aw22127: Add color effects and zones, set the LED current | Joonhoe Kim | this project; the rotation command first found by the author of [#34](https://github.com/joonhoekim/tb323fu-linux/pull/34) | local | `fd6c7b935aff0cbc` |
| `0155` | drm/bridge: aux-hpd: Report the last HPD state when HPD is enabled | Joonhoe Kim | this project; same approach as Yongxing Mou's patch under review ([lore](https://lore.kernel.org/all/20260803-drm-usbdp-preboot-v1-1-2539b362be00@oss.qualcomm.com/)) | local until that patch lands | `ab4567bdb49bafe3` |
| `0156` | arm64: configs: baldur: Build the Qualcomm dwc3 glue in | Joonhoe Kim | this project | local (config) | `2778019d75138991` |
| `0157` | drm/connector: report out-of-band IRQ_HPD events | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 1/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged; adapted: v7.3 context in `drm_connector.h` | `8549a45a81e46920` |
| `0158` | drm/connector: pass down IRQ_HPD to the drivers | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 2/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged; adapted: `drm_bridge_connector_detect()` (v7.3) instead of `detect_ctx` | `add484e5b1920035` |
| `0159` | drm/bridge: pass down IRQ_HPD to the drivers | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 3/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged; adapted: the omapdrm hdmi5 hunk dropped (no `hpd_notify` there in v7.3) | `b05ceb87e5be5078` |
| `0160` | drm/bridge: pass extra events to the HPD callback | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 4/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged; adapted: v7.3 context in `drm_bridge.h` | `0dc8f3332e6564a4` |
| `0161` | drm/bridge: aux-hpd: let drivers pass IRQ_HPD events | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 5/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged; adapted: merged with 0155 (the stored status is kept and replayed) | `0519abf7a443f251` |
| `0162` | drm/msm/dp: handle the IRQ_HPD events reported by USB-C | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 7/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged | `f9dbc5fa8c6002b4` |
| `0163` | soc: qcom: pmic-glink-altmode: pass down HPD_IRQ events | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 8/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged | `32da84b09c6c4fdd` |
| `0164` | usb: typec: ucsi: huawei-gaokun: pass down HPD_IRQ events | Dmitry Baryshkov | mailing list: [hpd-irq-events v8](https://lore.kernel.org/all/20261024-hpd-irq-events-v8-0-6ee949e7a384@oss.qualcomm.com/) 9/9 (Dmitry Baryshkov; 6/9 not taken, 0098/0137 cover it) | under review upstream; local until merged | `0803c0a9962191ad` |
| `0165` | drm/msm/dp: move link-level teardown from display_disable to display_unprepare | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 1/35 (Yongxing Mou) | applied to msm-next (`1bda647ce61b`..`5e4a3f7b2620`); in v7.4 | `01eff33dd9710268` |
| `0166` | drm/msm/dp: Pass panel to msm_dp_ctrl_config_msa() | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 2/35 (Yongxing Mou) | applied to msm-next (`1bda647ce61b`..`5e4a3f7b2620`); in v7.4 | `7272311fd8365041` |
| `0167` | drm/msm/dp: drop redundant config_ctrl_link() from msm_dp_ctrl_on_stream() | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 3/35 (Yongxing Mou) | applied to msm-next (`1bda647ce61b`..`5e4a3f7b2620`); in v7.4 | `28336e830a02bce9` |
| `0168` | drm/msm/dp: introduce stream_id for each DP panel | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 4/35 (Yongxing Mou) | applied to msm-next (`1bda647ce61b`..`5e4a3f7b2620`); in v7.4; adapted: context of 0150 in `dp_ctrl.c` | `278cedf21a626f8f` |
| `0169` | drm/msm/dp: add support for programming p1/p2/p3 register blocks | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 5/35 (Yongxing Mou) | applied to msm-next (`1bda647ce61b`..`5e4a3f7b2620`); in v7.4 | `783b389d56543692` |
| `0170` | drm/msm/dp: add MST stream register definitions | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 6/35 (Yongxing Mou) | applied to msm-next (`1bda647ce61b`..`5e4a3f7b2620`); in v7.4 | `9fcc82451eacdff1` |
| `0171` | drm/msm/dp: add stream-aware link register accessors | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 7/35 (Yongxing Mou) | applied to msm-next (`1bda647ce61b`..`5e4a3f7b2620`); in v7.4 | `24b76e6baa5e3794` |
| `0172` | drm/msm/dp: add support to send ACT packets for MST | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 8/35 (Yongxing Mou) | under review upstream; local until merged; adapted: context of 0150 in `dp_ctrl.c` | `eb889ca5aeb84876` |
| `0173` | drm/msm/dp: add support to enable MST in mainlink control | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 9/35 (Yongxing Mou) | under review upstream; local until merged | `7991d428a2275c05` |
| `0174` | drm/msm/dp: no need to update tu calculation for mst | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 10/35 (Yongxing Mou) | under review upstream; local until merged | `9df78d780db53049` |
| `0175` | drm/msm/dp: always program MST_FIFO_CONSTANT_FILL for MST use cases | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 11/35 (Yongxing Mou) | under review upstream; local until merged | `e76d49e7bc3b5dce` |
| `0176` | drm/msm/dp: add support for sending VCPF packets in DP controller | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 12/35 (Yongxing Mou) | under review upstream; local until merged | `7c405a4ae8a4f4b3` |
| `0177` | drm/msm/dp: add support for MST channel slot allocation | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 13/35 (Yongxing Mou) | under review upstream; local until merged; adapted: context of 0150 in `dp_ctrl.c` | `4e20787209e31139` |
| `0178` | drm/msm/dp: replace power_on with active_stream_cnt | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 14/35 (Yongxing Mou) | under review upstream; local until merged; adapted: the `power_on` uses of 0057, 0098, 0151 also moved to `active_stream_cnt` | `135acc5d18d24d52` |
| `0179` | drm/msm/dp: factor out _helper variants of bridge ops accepting a panel | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 15/35 (Yongxing Mou) | under review upstream; local until merged; adapted: context of 0178 | `288237aece9751b3` |
| `0180` | drm/msm/dp: add link_ready to manage link-level operations | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 16/35 (Yongxing Mou) | under review upstream; local until merged | `6d1f750850792c69` |
| `0181` | drm/msm/dp: add msm_dp_display_get_panel() to initialize DP panel | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 17/35 (Yongxing Mou) | under review upstream; local until merged | `9d93ad1c9bf0d8cb` |
| `0182` | drm/msm/dp: introduce dp_mst_drm module | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 18/35 (Yongxing Mou) | under review upstream; local until merged | `d5be95a17efa5af4` |
| `0183` | drm/msm/dp: add MST connector creation and topology callbacks | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 19/35 (Yongxing Mou) | under review upstream; local until merged | `4371f91fcf4dadee` |
| `0184` | drm/msm/dpu: pass msm_display_info to dpu_encoder_get_intf() | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 20/35 (Yongxing Mou) | under review upstream; local until merged | `4cb91ad71edf1fe7` |
| `0185` | drm/msm/dpu: use stream_id to select MST interfaces | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 21/35 (Yongxing Mou) | under review upstream; local until merged | `a70296b8c6134381` |
| `0186` | drm/msm/dpu: add per-stream MST encoders | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 22/35 (Yongxing Mou) | under review upstream; local until merged | `9529733ee7e751c2` |
| `0187` | drm/msm/dp: move link capabilities to dp_link | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 23/35 (Yongxing Mou) | under review upstream; local until merged; adapted: context of 0101 (link caps cleared on DPCD read) | `e0a2828c7dc26129` |
| `0188` | drm/msm/dp: add atomic stream handling for MST | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 24/35 (Yongxing Mou) | under review upstream; local until merged | `b310aff45f3e5829` |
| `0189` | drm/bridge_connector: suppress hotplug for IRQ_HPD without status changes | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 25/35 (Yongxing Mou) | under review upstream; local until merged | `fe892685f6c93f91` |
| `0190` | drm/bridge_connector: avoid detect-based HPD notifications for DisplayPort | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 26/35 (Yongxing Mou) | under review upstream; local until merged | `bb929b6b4b4c589b` |
| `0191` | drm/msm/dp: add HPD callback for dp MST | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 27/35 (Yongxing Mou) | under review upstream; local until merged; adapted: MST guard added before the 0098/0137 plug guard | `1c449da8d5d2c209` |
| `0192` | drm/msm/dp: wire MST helpers into atomic check and commit paths | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 28/35 (Yongxing Mou) | under review upstream; local until merged | `731452c1f761e947` |
| `0193` | drm/msm/dp: mark the SST bridge disconnected when mst is active | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 29/35 (Yongxing Mou) | under review upstream; local until merged | `b474fe8896b692ed` |
| `0194` | drm/msm/dp: enable MST on capable sinks | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 30/35 (Yongxing Mou) | under review upstream; local until merged; adapted: 0137: an MST sink counts as configured | `b3775dd526b2b38d` |
| `0195` | drm/msm/dp: mark the SST bridge disconnected when an MST-capable sink is present | Abhinav Kumar | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 31/35 (Yongxing Mou) | under review upstream; local until merged | `84a35ccfbce9f0bb` |
| `0196` | drm/msm/dp: mark the SST connector disconnected when MST is enabled | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 32/35 (Yongxing Mou) | under review upstream; local until merged | `ae557d1f0e590ffb` |
| `0197` | drm/msm/dp: wake threaded handler for HPD IRQs | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 33/35 (Yongxing Mou) | under review upstream; local until merged | `610f2449fda0375f` |
| `0198` | drm/msm/dp: order IRQ HPD handling with plug state changes | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 34/35 (Yongxing Mou) | under review upstream; local until merged; adapted: second hunk dropped: with hpd v8 the plug handler already runs before IRQ_HPD handling | `4a7542657c729ad8` |
| `0199` | soc: qcom: pmic-glink-altmode: skip retimer reset on DP IRQ | Yongxing Mou | mailing list: [MST v7](https://lore.kernel.org/all/20260929-msm-dp-mst-v7-0-4b81473185e7@oss.qualcomm.com/) 35/35 (Yongxing Mou) | under review upstream; local until merged | `c2f7e3f9d8b99d26` |
| `0200` | drm/msm/dp: don't take mode_config.mutex under plugged_lock when enabling MST | Joonhoe Kim | this project (on top of the MST v7 series) | local; for the MST v7 thread | `b9ee12afabdc80b1` |
| `0201` | drm/msm/dpu: with colour management, use one LM up to the mixer width | Joonhoe Kim | this project (on top of the MST v7 series) | local; for the MST v7 thread | `45371e91d2f88fd5` |
| `0202` | drm/msm/dp: suspend and resume the MST topology across system sleep | Joonhoe Kim | this project (on top of the MST v7 series) | local; for the MST v7 thread (the series has no topology suspend/resume) | `9cb9614d698c0978` |
| `0203` | drm/msm/dp: keep the PHY when an MST stream is disabled with none left | Joonhoe Kim | this project (on top of the MST v7 series) | local; on top of 0151 and the MST v7 series | `664ae18708ab8bd4` |
| `0205` | drm/msm/dp: ignore sink requests from IRQ_HPD while the link is down | Joonhoe Kim | this project (on top of the IRQ_HPD events v8 and MST v7 series) | local; fix for IRQ_HPD events v8, to report upstream | `743476e880631af8` |
| `0206` | media: iris: Fix iova allocation from restrict region | Vishnu Reddy | upstream `200473c198b2` ([lore](https://lore.kernel.org/all/20260818-reserve_iova_in_driver-v2-1-5005a1154408@oss.qualcomm.com/)), from the 7.3-stable queue | backport; drop when 7.3.y has it | `b504fe3eed372b11` |
| `0207` | arm64: dts: qcom: kaanapali-lenovo-baldur: Add DisplayPort audio | sotiomake | PR #49, `[jk: …]` fix by this project | local | `8326550d389fe943` |

## Out-of-tree

Sources built as external modules (not part of the patch series).

| Directory | What | Origin | License |
|---|---|---|---|
| [`out-of-tree/aw882xx/`](out-of-tree/aw882xx/) | Awinic smart amplifier driver `snd-soc-aw882xx` (speakers, chip ID `0x2308`), vendor driver `v1.15.0` with 7.x API fixes marked `tb323fu:` | AWINIC Technology Co., Ltd.; copy from [rockchip-linux/kernel](https://github.com/rockchip-linux/kernel) `develop-6.1` `sound/soc/codecs/aw882xx/` at `1feee0d9c0b20750eef52b06b9211a4a3a353895` | `GPL-2.0` (original headers kept; the C files' notice text adds "or any later version", see its README) |

The amplifier's run-time parameter file `aw882xx_acf.bin` is vendor data and is not included (see
[`firmware/`](../firmware/)).
