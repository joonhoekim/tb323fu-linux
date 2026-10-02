# Provenance

Where every patch in `patches/` comes from. The series applies with `git am` on the base below.
Imported patches keep their original authors and `Signed-off-by` lines. Where `From:` names the
importer instead of the original authors (squashed imports: 0024, 0030, 0039-0041), the origin
column names the source; see that tree's history for the individual authors.

## Base

| What | Where |
|---|---|
| upstream base | torvalds/linux `v7.3-rc4` = `93f51579e7df248780214094418f205253383cc5` |
| qcom kaanapali GPU DT | qcom `arm64-for-7.4` (patches 0001-0006) |
| linux-next picks | `next-20260925` (patches 0012-0018) |
| community tree | [kaanapali-mainline/linux](https://github.com/kaanapali-mainline/linux) `testing` at `37a76066` (patches 0007-0011) |
| community tree | [infiniti-mainline/linux](https://github.com/infiniti-mainline/linux) `oneplus-15` / `master` (patches 0024, 0029, 0030, 0070-0077, 0084-0094, 0096, 0099-0102) |
| touch driver | [map220v/sm8850-mainline](https://github.com/map220v/sm8850-mainline) `iceland-7.2` (0025) |

## Patches

`sha256` is the first 16 hex digits of the SHA-256 of the patch file as stored here.
0111-0114 and 0116 carry no `Signed-off-by` yet; the author adds it before sending them upstream.

| # | Subject | Author (`From:`) | Origin | Status | sha256 |
|---|---|---|---|---|---|
| `0001` | arm64: dts: qcom: kaanapali: add the GPU SMMU node | Qingqing Zhou | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `ea84290779bebe3c` |
| `0002` | arm64: dts: qcom: kaanapali: Add QFPROM node | Jingyi Wang | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `0ce87440b99f68d8` |
| `0003` | arm64: dts: qcom: Add GPU support for Kaanapali | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `6089dc88850d1987` |
| `0004` | arm64: dts: qcom: kaanapali: Add GPU cooling | Gaurav Kohli | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `1674574f14593262` |
| `0005` | arm64: dts: qcom: kaanapali-mtp: Enable GPU | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `3541b3db577bac30` |
| `0006` | arm64: dts: qcom: kaanapali-qrd: Enable GPU | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `01c93e8d2c936217` |
| `0007` | drm/msm: DPU, DSI and MDSS fixes for AA601 cmd-mode DSC | idusergod | community [kaanapali-mainline](https://github.com/kaanapali-mainline/linux) testing `b6eab7f0e8` (partial) | community, not upstream | `c90ec49a2c3462de` |
| `0008` | drm/msm: cmd-mode DSC 1.2 fixes for DPU 13 (Kaanapali) | EYC | community kaanapali-mainline `144a400716` | community, not upstream | `e70b468bd1be7d7f` |
| `0009` | clk: qcom: dispcc-kaanapali: solve display artifacts at start | Nazar Kompanets | community kaanapali-mainline `6ba50855c6` | community, not upstream | `f3110b92e9f6f3a0` |
| `0010` | drm/msm/adreno: A840: drop IFPC quirk for Infiniti bring-up | idusergod | community kaanapali-mainline `6a4127a8cd` (reverted by 0080) | community; net no-op with 0080 | `675b0d64259a293b` |
| `0011` | arm64: configs: add kaanapali-oneplus-infiniti_defconfig fragment | idusergod | community kaanapali-mainline `37a7606647` (defconfig fragment) | community; base of the baldur fragment | `97d2680233290042` |
| `0012` | scsi: ufs: ufs-qcom: Enable only lane clocks in lane clock APIs | Nitin Rawat | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `f3343cbe58f236e9` |
| `0013` | phy: qcom-mipi-csi2: Add a CSI2 MIPI DPHY driver | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `8e7938167b0d106d` |
| `0014` | phy: core: Fix race-condition between _of_phy_get() and try_module_get() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `2f0487131f392c19` |
| `0015` | phy: core: Add phy_get_by_of_node() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `c550554888abf161` |
| `0016` | phy: core: Add devm_phy_get_by_of_node() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `644f6ad80e7c4e50` |
| `0017` | phy: core: Add missing kerneldoc colon in two locations | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `5cfc4b0ebbd9695b` |
| `0018` | dt-bindings: phy: qcom: Add CSI2 C-PHY/DPHY schema | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `67214dbf7d621bf0` |
| `0019` | arm64: dts: qcom: kaanapali: add the second DSI controller and PHY | joonhoekim | this project | local | `e14bf5ad358920d6` |
| `0020` | drm/panel: nt36523: add DSC, and the Lenovo TB323FU CSOT panel | joonhoekim | this project | local | `2148317b62e444e4` |
| `0021` | arm64: carry a devicetree inside the kernel image | joonhoekim | this project | local (built-in DTB; bootloader workaround) | `57ed051b50008aab` |
| `0022` | arm64: paint boot progress marks into the bootloader's framebuffer | joonhoekim | this project | local (debug: boot progress marks) | `9fd58c712789e4ff` |
| `0023` | drm/msm: video-mode DSC at 10 bpc -- make DSI and the DPU INTF agree on the line | joonhoekim | this project | local | `b86bd2894d196d5f` |
| `0024` | wifi: ath12k + PCI/pwrctrl: WCN7860 ("peach", 17cb:110e) | joonhoekim | [infiniti-mainline](https://github.com/infiniti-mainline/linux) `oneplus-15`, several authors, imported squashed (From: is the importer) | community, not upstream | `5e6d4dd8b6e312b2` |
| `0025` | Input: touchscreen: import NT36536 host-download SPI driver (Novatek) | joonhoekim | [map220v/sm8850-mainline](https://github.com/map220v/sm8850-mainline) `iceland-7.2` (Novatek vendor driver, verbatim) | out of tree, not upstreamable | `3230420d70d5de7d` |
| `0026` | input: touchscreen: NT36536 host-download SPI driver (Novatek) | joonhoekim | this project | local | `f282333bdf1ec787` |
| `0027` | input: nt36536: fixes for TB323FU | joonhoekim | this project | local | `e9c247c0b6b5ad05` |
| `0028` | backlight: aw99706: replay board registers after the HWEN reset | joonhoekim | this project | local | `669cd8efd24602c5` |
| `0029` | iommu: arm-smmu-qcom: kaanapali MDSS identity domain | EYC | community / infiniti-mainline (EYC) | community, not upstream | `a85b5d13de911cd1` |
| `0030` | Bluetooth: qca: WCN7860 (BRAHMA) on the WCN7850 flow | joonhoekim | infiniti-mainline `fecd7925d` (EYC; From: is the importer) | community, not upstream | `bd8ea9dd50ad74c9` |
| `0031` | arm64: dts: qcom: kaanapali: CPU6-7 in a PSCI cluster domain of their own | joonhoekim | this project | local | `842ef62d7a54d5e3` |
| `0032` | drm/msm/dsi: phy: prepare the iface clock only while runtime active | joonhoekim | this project | local | `431db9c93144e5da` |
| `0033` | power: supply: qcom_battmgr: let the host set the USB input current limit | joonhoekim | this project | local | `87439660137fe8db` |
| `0034` | ASoC: qcom: sc8280xp: optional S32_LE on MI2S backends | joonhoekim | this project | local | `ce8b6c3a99752681` |
| `0035` | ASoC: codecs: lpass-tx-macro: board tuning of the decimator filter block | joonhoekim | this project | local | `68be27c31735db78` |
| `0036` | usb: gadget: f_ncm: restart the TX timer when the freelist is empty | joonhoekim | this project | local | `0bd6b91e27f5d8a5` |
| `0037` | ASoC: codecs: wcd939x: read the ADC/DMIC switches per channel | joonhoekim | this project | local | `bd8da5b429e03dd4` |
| `0038` | Input: aw86927 - accept the AW86937 | joonhoekim | this project | local | `739a07ea6f1ceb4b` |
| `0039` | pmic5-gen4-adc: mailing-list series (squashed) | joonhoekim | mailing list: PMIC5 Gen4 ADC series (Jishnu Prakash), squashed | pending upstream | `49a72289bdcd2f2b` |
| `0040` | media-qcom-camss-kaanapali-v16: mailing-list series (squashed) | joonhoekim | mailing list: CAMSS kaanapali v16 (Hangxiang Ma), squashed | pending upstream | `7b2d13ae583fc167` |
| `0041` | media-i2c-s5kjn5-v4: mailing-list series (squashed) | joonhoekim | mailing list: S5KJN5 sensor v4 (Wenmeng Liu), squashed | pending upstream | `7d45c64a7d3e1c93` |
| `0042` | media: qcom: camss: Add support for PHY API devices | Bryan O'Donoghue | mailing list: camss PHY API series 4/5 (Bryan O'Donoghue) | pending upstream | `b10dc4cce7742860` |
| `0043` | media: qcom: camss: Use data-lanes starting at 1 for new CSIPHY mode | Bryan O'Donoghue | mailing list: camss PHY API series 5/5 (Bryan O'Donoghue) | pending upstream | `36389bd9fd734fa3` |
| `0044` | pinctrl: qcom: kaanapali: add the I2C strong pull-up bit | joonhoekim | this project | local | `df563b612fd3824d` |
| `0045` | media: i2c: add Samsung S5KJNS and GalaxyCore GC08A8 sensor drivers | joonhoekim | this project | local | `81b1eb36b303f423` |
| `0046` | clk: qcom: gdsc: keep the clock controller active while a GDSC is on | joonhoekim | this project | local | `d4558bee2c14e249` |
| `0047` | media: i2c: dw9719: add Giantec GT9764, optional vio supply | joonhoekim | this project | local | `f18c081a941a47d0` |
| `0048` | leds: rgb: add Awinic AW22127 driver | joonhoekim | this project | local | `324050349617a56f` |
| `0049` | PCI: qcom: don't advertise a hot-plug slot on the Root Port | joonhoekim | this project | local | `52b07f4d961447bc` |
| `0050` | drm/msm/dsi: phy: runtime suspend the PHY over system sleep | joonhoekim | this project | local | `c590cf05bcfac08e` |
| `0051` | drm/msm/dp: add kaanapali (DP0 descriptor) | joonhoekim | this project | local | `d330b85b114b644b` |
| `0052` | drm/msm/dpu: add the SSPP rec0/rec1 blocks to the snapshot | joonhoekim | this project | local | `e5db396f09712be3` |
| `0053` | input: nt36536: make the pen a tablet libinput accepts | joonhoekim | this project | local | `be8f34f155cef71c` |
| `0054` | arm64: dts: qcom: add Lenovo Legion Tab Y700 gen5 (TB323FU, baldur) board DTs | joonhoekim | this project | local | `36465acde4d25341` |
| `0055` | arm64: configs: add the baldur config fragments | joonhoekim | this project | local | `7f671cda50be4092` |
| `0056` | PCI: qcom: parse iommu-map with the target #iommu-cells | joonhoekim | this project | local; superseded by linux-next (`qcom_pcie_config_sid_1_9_0`) | `2b19a0f18b871aac` |
| `0057` | drm/msm/dp: retrain the link on a quick replug while streaming | joonhoekim | this project | local | `f04dc08d8a8e1e8b` |
| `0058` | wifi: ath12k: don't wake the device over MHI from the panic notifier | joonhoekim | this project | local | `22f3f256562a2773` |
| `0059` | clk: qcom: gcc-kaanapali: keep the USB GDSCs in retention | joonhoekim | this project | local | `6abbc1ff47ae183d` |
| `0060` | soc: qcom: pmic_glink_altmode: handle notifications on the freezable workqueue | joonhoekim | this project | local; superseded upstream (`7d0767c5cd87`, freezable workqueue) | `20799fff702f8818` |
| `0061` | usb: typec: ucsi: run connector change handling on the freezable workqueue | joonhoekim | this project | local | `1f05de01804165ce` |
| `0062` | usb: dwc3: qcom: arm the eUSB2 line interrupts for wakeup | joonhoekim | this project | local | `4a070f5e740ed9f1` |
| `0063` | media: i2c: s5kjns: the colour filter order is GRBG, not GBRG | joonhoekim | this project | local | `b13f5eb90421ba9a` |
| `0064` | arm64: dts: qcom: baldur: camera orientation and rotation | joonhoekim | this project | local | `c36243a1aecf8cae` |
| `0065` | drm/msm/dpu+dsi: change the vertical front porch in place | joonhoekim | this project | local | `a219a264ec2cf4b6` |
| `0066` | drm/panel: nt36523: baldur CSOT 60 and 30 Hz modes | joonhoekim | this project | local | `b223e515733e45f7` |
| `0067` | remoteproc: qcom: q6v5_pas: Don't enable handover IRQ on attach | Shawn Guo | upstream fix `34b8b2d78b62` | upstream | `f2fb167212f57bb6` |
| `0068` | remoteproc: qcom_q6v5_pas: Fix error masking in qcom_pas_stop() | Vignesh Viswanathan | upstream fix `9db31edf92dd` | upstream | `83e78cd75d11594a` |
| `0069` | thermal: gov_step_wise: Fix stale mitigation vote with non-zero lower bounds | Manaf Meethalavalappu Pallikunhi | upstream fix `ec0d89150a93` | upstream | `34d6b208c1d26fd2` |
| `0070` | clk: qcom: gcc-kaanapali: Fix always-enabling PCIE_RSCC clocks | Luca Weiss | qcom clk-fixes `0bfb542fc336` (via infiniti-mainline) | upstream | `e57a889ec9ea4bd7` |
| `0071` | clk: qcom: gpucc-kaanapali: Mark the GPU CX GDSC as votable | Taniya Das | qcom clk-fixes `55981579b27b` (via infiniti-mainline) | upstream | `b197d959b736973b` |
| `0072` | clk: qcom: gcc: Set FORCE_MEM_CORE_ON/FORCE_PERIPH_CORE_ON for PCIe pipe clocks | Qiang Yu | mailing list v1 (Qiang Yu), via infiniti-mainline | pending upstream | `d76ccb2d9a1a8ad4` |
| `0073` | clk: qcom: branch: Turn off a branch that failed to enable | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `b93730c4e02756b5` |
| `0074` | clk: qcom: Fix camera rivian PLL configuration settings | Jagadeesh Kona | upstream (Jagadeesh Kona), via infiniti-mainline | upstream | `9c5950781293f3ab` |
| `0075` | soc: qcom: apr: Register the GPR callback before probing | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `c76df9a3bf98568f` |
| `0076` | ASoC: qcom: q6apm: Send commands from the APM's GPR service | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `7789da4acf28039c` |
| `0077` | arm64: dts: qcom: kaanapali: Advertise a 1024-byte GPR intent | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `73b9f0bdde0cfe68` |
| `0078` | arm64: dts: qcom: baldur: WCN7860 WAKE# is active low | joonhoekim | this project | local | `8227e828656be95a` |
| `0079` | drm/msm/dpu: keep the full modeset when colour management changed | joonhoekim | this project | local | `a063357f486852b7` |
| `0080` | Revert "drm/msm/adreno: A840: drop IFPC quirk for Infiniti bring-up" | joonhoekim | this project | local | `684183ea86fec4c1` |
| `0081` | dt-bindings: display: msm: Add INT2 GDSC to Kaanapali DPU | Yongxing Mou | mailing list: Kaanapali DPU INT2 GDSC v2 (Yongxing Mou) | pending upstream | `05bc7c71eddb1d7a` |
| `0082` | drm/msm/dpu: Attach INT2 power domain alongside MMCX on Kaanapali | Yongxing Mou | mailing list: Kaanapali DPU INT2 GDSC v2 (Yongxing Mou) | pending upstream | `31729280365dc880` |
| `0083` | arm64: dts: qcom: Add INT2 GDSC to Kaanapali DPU | Yongxing Mou | mailing list: Kaanapali DPU INT2 GDSC v2 (Yongxing Mou) | pending upstream | `45cfa63c72e2fe66` |
| `0084` | media: dt-bindings: qcom-kaanapali-iris: Add kaanapali video codec binding | Vikash Garodia | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `deb75403f41251a6` |
| `0085` | media: iris: add AV1 decode buffer size support for vpu4x | Wangao Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `96a651253eabe1a6` |
| `0086` | media: iris: Add platform data for kaanapali | Vikash Garodia | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `7466f36f854e31b3` |
| `0087` | media: iris: add iris4 specific H265 line buffer calculation | Vikash Garodia | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `6c4c88f63b074a62` |
| `0088` | media: iris: Release the MVP NoC low-power request on vpu4x power off | Victor Fuentes | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `a2af9355e24bad3d` |
| `0089` | arm64: dts: qcom: kaanapali: Add iris video node | Jingyi Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `d0ffd538b11194b0` |
| `0090` | media: iris: Fix bus_info prefix in VIDIOC_QUERYCAP | Vishnu Reddy | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `ee6ef74c845c351d` |
| `0091` | media: iris: fix VPSS line buffer width and height order | Wangao Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `784c0378f174f5c4` |
| `0092` | media: iris: fix VPU4x encoder line buffer size for rotation | Wangao Wang | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `6c1fe0df9e2b58f9` |
| `0093` | media: iris: Only allow hierarchical B-frames on kaanapali | Victor Fuentes | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `952692bbb64f8691` |
| `0094` | media: iris: Don't report the picture state on decoder input buffers | Victor Fuentes | infiniti-mainline (iris kaanapali; authors as in From:) | community / pending upstream | `9b5d6b188c52580b` |
| `0095` | arm64: dts: qcom: baldur: enable the iris video codec | joonhoekim | this project | local | `33721e3c71d41c40` |
| `0096` | phy: qcom: qmp-combo: Add Kaanapali USB3+DP PHY | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `1e0e578ff21fb961` |
| `0097` | drm/msm/dpu: stop all video interfaces before cleaning up a split encoder | joonhoekim | this project | sent upstream 2026-09-29, under review | `9fdec9fce3a03776` |
| `0098` | drm/msm/dp: hold one runtime PM reference per plugged state | joonhoekim | this project | sent upstream 2026-09-29, under review | `814b709cea246edc` |
| `0099` | phy: qcom: qmp-combo: Drop the stale err_disable_pipe_clk teardown | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `4cc9500eb779c89d` |
| `0100` | phy: qcom: qmp-combo: Keep the DP lanes of a USB-capable DP sink | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `6f508562ddb6e8ec` |
| `0101` | drm/msm/dp: check the PHY power-on and DPCD link status returns | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `4711f13cff69ad11` |
| `0102` | drm/msm/dp: reset the link caps on every DPCD read | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `bc3e46a9718d4891` |
| `0103` | clk: qcom: gdsc: drop the controller reference only if it was taken | joonhoekim | this project | local | `ed38bddd35bf3659` |
| `0104` | usb: dwc3: tell xHCI that it lost its state when the core is powered off | joonhoekim | this project | sent upstream 2026-09-29, under review | `2a447d4ba9a36679` |
| `0105` | drm/msm/dpu: compute the CRTC bandwidth from the state being checked | joonhoekim | this project | sent upstream 2026-09-29, under review | `8eb9e32dfaa88e5d` |
| `0106` | cpuidle: psci-domain: baldur: use the system domain state only in system suspend | Joonhoe Kim | this project | local workaround (cpuidle system domain state only in suspend) | `09a5acaa07543da4` |
| `0107` | power: supply: qcom_battmgr: fix the battery current sign on Kaanapali | Joonhoe Kim | this project | local | `0a838c98a286a567` |
| `0108` | arm64: dts: qcom: baldur: throttle on the board temperature like Android | Joonhoe Kim | this project | local (board DT) | `21a147ededcac540` |
| `0109` | drm/panel: nt36523: baldur: add 90 Hz and 164 Hz modes | joonhoekim | this project | local (panel modes) | `78d95ed7c8c0aec7` |
| `0110` | drm/msm/dpu: baldur: lower the refresh rate in the kernel when idle | joonhoekim | this project | local, not intended for upstream | `754925f03dd026ce` |
| `0111` | media: iris: vpu4x: size the decoder OPB line buffer for 10-bit output | Joonhoe Kim | this project | upstream candidate; Signed-off-by to be added before submission | `60f0f363b5542ed9` |
| `0112` | media: iris: don't wait for the threaded IRQ handler under core->lock | Joonhoe Kim | this project | upstream candidate; Signed-off-by to be added before submission | `e7c29a4c0e69ba15` |
| `0113` | wifi: ath12k: keep the RX refill ring from running dry | Joonhoe Kim | this project | upstream candidate; Signed-off-by to be added before submission | `0250a3b8b9b44fb9` |
| `0114` | net: qrtr: ns: retry announcements the new node is not ready for yet | Joonhoe Kim | this project | upstream candidate; Signed-off-by to be added before submission | `6826715ed1ed1eb2` |
| `0116` | media: qcom: camss: hold the bandwidth vote while TITAN_TOP is powered | Joonhoe Kim | this project | upstream candidate; Signed-off-by to be added before submission (0115 is a debugging aid kept out of the series) | `3e5a2e77a2018c0d` |

## Out-of-tree

Sources built as external modules (not part of the patch series).

| Directory | What | Origin | License |
|---|---|---|---|
| [`out-of-tree/aw882xx/`](out-of-tree/aw882xx/) | Awinic smart amplifier driver `snd-soc-aw882xx` (speakers, chip ID `0x2308`), vendor driver `v1.15.0` with 7.x API fixes marked `y705` | AWINIC Technology Co., Ltd.; copy from [rockchip-linux/kernel](https://github.com/rockchip-linux/kernel) `develop-6.1` `sound/soc/codecs/aw882xx/` at `1feee0d9c0b20750eef52b06b9211a4a3a353895` | `GPL-2.0` (original headers kept; the C files' notice text adds "or any later version", see its README) |

The amplifier's run-time parameter file `aw882xx_acf.bin` is vendor data and is not included (see
[`firmware/`](../firmware/)).
