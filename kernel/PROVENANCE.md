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
0111-0114, 0116-0119 and 0128 are this project's own patches, signed off by the author (`Signed-off-by: Joonhoe Kim`).

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
| `0019` | arm64: dts: qcom: kaanapali: add the second DSI controller and PHY | Joonhoe Kim | this project | local | `f0825a77ba496390` |
| `0020` | drm/panel: nt36523: add DSC, and the Lenovo TB323FU CSOT panel | Joonhoe Kim | this project | local | `fe8e736091745377` |
| `0021` | arm64: carry a devicetree inside the kernel image | Joonhoe Kim | this project | local (built-in DTB; bootloader workaround) | `c7c512f612f4fc9b` |
| `0022` | arm64: paint boot progress marks into the bootloader's framebuffer | Joonhoe Kim | this project | local (debug: boot progress marks) | `4a5fd2e1bdd92b66` |
| `0023` | drm/msm: video-mode DSC at 10 bpc -- make DSI and the DPU INTF agree on the line | Joonhoe Kim | this project | local; its DPU part sent upstream 2026-10-04 as "drm/msm/dpu: round up the compressed INTF width", reviewed ([lore](https://lore.kernel.org/all/20261004102758.84436-1-26rote@gmail.com/)) | `8db55405d1433157` |
| `0024` | wifi: ath12k + PCI/pwrctrl: WCN7860 ("peach", 17cb:110e) | Joonhoe Kim | [infiniti-mainline](https://github.com/infiniti-mainline/linux) `oneplus-15`, several authors, imported squashed (From: is the importer) | community, not upstream | `4a680f653196e4a1` |
| `0025` | Input: touchscreen: import NT36536 host-download SPI driver (Novatek) | Joonhoe Kim | [map220v/sm8850-mainline](https://github.com/map220v/sm8850-mainline) `iceland-7.2` (Novatek vendor driver, verbatim) | out of tree, not upstreamable | `b5aaa9031e3c009c` |
| `0026` | input: touchscreen: NT36536 host-download SPI driver (Novatek) | Joonhoe Kim | this project | local | `309aa6832efaf287` |
| `0027` | input: nt36536: fixes for TB323FU | Joonhoe Kim | this project | local | `c61cbc974e5b1c8b` |
| `0028` | backlight: aw99706: replay board registers after the HWEN reset | Joonhoe Kim | this project | local | `7b3b59550e2f6699` |
| `0029` | iommu: arm-smmu-qcom: kaanapali MDSS identity domain | EYC | community / infiniti-mainline (EYC) | community, not upstream | `a85b5d13de911cd1` |
| `0030` | Bluetooth: qca: WCN7860 (BRAHMA) on the WCN7850 flow | Joonhoe Kim | infiniti-mainline `fecd7925d` (EYC; From: is the importer) | community, not upstream | `a344f714f953876f` |
| `0031` | arm64: dts: qcom: kaanapali: CPU6-7 in a PSCI cluster domain of their own | Joonhoe Kim | this project | local | `8d8b9791e3d22111` |
| `0032` | drm/msm/dsi: phy: prepare the iface clock only while runtime active | Joonhoe Kim | this project | local | `37afecd818f74360` |
| `0033` | power: supply: qcom_battmgr: let the host set the USB input current limit | Joonhoe Kim | this project | local | `f24476a389c0b719` |
| `0034` | ASoC: qcom: sc8280xp: optional S32_LE on MI2S backends | Joonhoe Kim | this project | local | `038b4173d04f1010` |
| `0035` | ASoC: codecs: lpass-tx-macro: board tuning of the decimator filter block | Joonhoe Kim | this project | local | `f0a158037081bfd3` |
| `0036` | usb: gadget: f_ncm: restart the TX timer when the freelist is empty | Joonhoe Kim | this project | local | `28d91effadbce995` |
| `0037` | ASoC: codecs: wcd939x: read the ADC/DMIC switches per channel | Joonhoe Kim | this project | local | `5ce05d3dd63814a5` |
| `0038` | Input: aw86927 - accept the AW86937 | Joonhoe Kim | this project | local | `ea100d2b380e8526` |
| `0039` | pmic5-gen4-adc: mailing-list series (squashed) | Joonhoe Kim | mailing list: PMIC5 Gen4 ADC series (Jishnu Prakash), squashed | pending upstream | `2f6cc13f96ce7e3d` |
| `0040` | media-qcom-camss-kaanapali-v16: mailing-list series (squashed) | Joonhoe Kim | mailing list: CAMSS kaanapali v16 (Hangxiang Ma), squashed | pending upstream | `4dcde6bf785b8443` |
| `0041` | media-i2c-s5kjn5-v4: mailing-list series (squashed) | Joonhoe Kim | mailing list: S5KJN5 sensor v4 (Wenmeng Liu), squashed | pending upstream | `1a3368f0d213631f` |
| `0042` | media: qcom: camss: Add support for PHY API devices | Bryan O'Donoghue | mailing list: camss PHY API series 4/5 (Bryan O'Donoghue) | pending upstream | `b10dc4cce7742860` |
| `0043` | media: qcom: camss: Use data-lanes starting at 1 for new CSIPHY mode | Bryan O'Donoghue | mailing list: camss PHY API series 5/5 (Bryan O'Donoghue) | pending upstream | `36389bd9fd734fa3` |
| `0044` | pinctrl: qcom: kaanapali: add the I2C strong pull-up bit | Joonhoe Kim | this project | local | `35a9002e127b36f7` |
| `0045` | media: i2c: add Samsung S5KJNS and GalaxyCore GC08A8 sensor drivers | Joonhoe Kim | this project | local | `f9ef91a7def5c204` |
| `0046` | clk: qcom: gdsc: keep the clock controller active while a GDSC is on | Joonhoe Kim | this project | local | `6808ab9547be6d0a` |
| `0047` | media: i2c: dw9719: add Giantec GT9764, optional vio supply | Joonhoe Kim | this project | local | `5391af9522093811` |
| `0048` | leds: rgb: add Awinic AW22127 driver | Joonhoe Kim | this project | local | `bf6f5aa9a84218e7` |
| `0049` | PCI: qcom: don't advertise a hot-plug slot on the Root Port | Joonhoe Kim | this project | local | `1b51f9225d09de84` |
| `0050` | drm/msm/dsi: phy: runtime suspend the PHY over system sleep | Joonhoe Kim | this project | local | `8242d4916d41bc66` |
| `0051` | drm/msm/dp: add kaanapali (DP0 descriptor) | Joonhoe Kim | this project | local | `ebd4d33f8ec4d25e` |
| `0052` | drm/msm/dpu: add the SSPP rec0/rec1 blocks to the snapshot | Joonhoe Kim | this project | local | `aae0c80924b48029` |
| `0053` | input: nt36536: make the pen a tablet libinput accepts | Joonhoe Kim | this project | local | `eb96100ff7ed3666` |
| `0054` | arm64: dts: qcom: add Lenovo Legion Tab Y700 gen5 (TB323FU, baldur) board DTs | Joonhoe Kim | this project | local | `a5ccc1bdd79d98ef` |
| `0055` | arm64: configs: add the baldur config fragments | Joonhoe Kim | this project | local | `ab17df9e0b5b9d62` |
| `0056` | PCI: qcom: parse iommu-map with the target #iommu-cells | Joonhoe Kim | this project | local; superseded by linux-next (`qcom_pcie_config_sid_1_9_0`) | `122235033a6f5780` |
| `0057` | drm/msm/dp: retrain the link on a quick replug while streaming | Joonhoe Kim | this project | local | `9649c3d9940a0dff` |
| `0058` | wifi: ath12k: don't wake the device over MHI from the panic notifier | Joonhoe Kim | this project | local | `e1c0e92d685b4eab` |
| `0059` | clk: qcom: gcc-kaanapali: keep the USB GDSCs in retention | Joonhoe Kim | this project | local | `5b1e7c9ecc20d714` |
| `0060` | soc: qcom: pmic_glink_altmode: handle notifications on the freezable workqueue | Joonhoe Kim | this project | local; superseded upstream (`7d0767c5cd87`, freezable workqueue) | `1a95be7c9baf3256` |
| `0061` | usb: typec: ucsi: run connector change handling on the freezable workqueue | Joonhoe Kim | this project | local | `89d5852e26b988e9` |
| `0062` | usb: dwc3: qcom: arm the eUSB2 line interrupts for wakeup | Joonhoe Kim | this project | local | `8e5c98291d12bfcc` |
| `0063` | media: i2c: s5kjns: the colour filter order is GRBG, not GBRG | Joonhoe Kim | this project | local | `e1a92e9a6cb7c1f3` |
| `0064` | arm64: dts: qcom: baldur: camera orientation and rotation | Joonhoe Kim | this project | local | `4e42ca6a5f0cc11c` |
| `0065` | drm/msm/dpu+dsi: change the vertical front porch in place | Joonhoe Kim | this project | local | `dbaab7f50a61bd72` |
| `0066` | drm/panel: nt36523: baldur CSOT 60 and 30 Hz modes | Joonhoe Kim | this project | local | `87a8e139b054a9b0` |
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
| `0078` | arm64: dts: qcom: baldur: WCN7860 WAKE# is active low | Joonhoe Kim | this project | local | `4bc5e51c86263a78` |
| `0079` | drm/msm/dpu: keep the full modeset when colour management changed | Joonhoe Kim | this project | local | `b236cb0a933a6af6` |
| `0080` | Revert "drm/msm/adreno: A840: drop IFPC quirk for Infiniti bring-up" | Joonhoe Kim | this project | local | `8f7e5c4abe7923c5` |
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
| `0095` | arm64: dts: qcom: baldur: enable the iris video codec | Joonhoe Kim | this project | local | `a75b0c32cc1b7873` |
| `0096` | phy: qcom: qmp-combo: Add Kaanapali USB3+DP PHY | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `1e0e578ff21fb961` |
| `0097` | drm/msm/dpu: disable the slave encoder before the master | Joonhoe Kim | this project | upstream v3 2026-10-02, reviewed ([lore](https://lore.kernel.org/all/20261002130712.50612-1-26rote@gmail.com/)) | `412d487ac73e0de4` |
| `0098` | drm/msm/dp: hold one runtime PM reference per plugged state | Joonhoe Kim | this project | sent upstream 2026-09-29, under review | `c40d7ae1222c1a85` |
| `0099` | phy: qcom: qmp-combo: Drop the stale err_disable_pipe_clk teardown | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `4cc9500eb779c89d` |
| `0100` | phy: qcom: qmp-combo: Keep the DP lanes of a USB-capable DP sink | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `6f508562ddb6e8ec` |
| `0101` | drm/msm/dp: check the PHY power-on and DPCD link status returns | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `4711f13cff69ad11` |
| `0102` | drm/msm/dp: reset the link caps on every DPCD read | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `bc3e46a9718d4891` |
| `0103` | clk: qcom: gdsc: drop the controller reference only if it was taken | Joonhoe Kim | this project | local | `cac3ca5aa1071443` |
| `0104` | usb: dwc3: tell xHCI that it lost its state when the core is powered off | Joonhoe Kim | this project | sent upstream 2026-09-29, under review | `241ae504c37a9442` |
| `0105` | drm/msm/dpu: compute the CRTC bandwidth from the state being checked | Joonhoe Kim | this project | upstream v2 2026-10-02, reviewed ([lore](https://lore.kernel.org/all/20261002130656.50577-1-26rote@gmail.com/)) | `d308e4ba73faca78` |
| `0106` | cpuidle: psci-domain: baldur: use the system domain state only in system suspend | Joonhoe Kim | this project | local workaround (cpuidle system domain state only in suspend) | `e5fe440e8cc5bdfb` |
| `0107` | power: supply: qcom_battmgr: fix the battery current sign on Kaanapali | Joonhoe Kim | this project | local | `0a838c98a286a567` |
| `0108` | arm64: dts: qcom: baldur: throttle on the board temperature like Android | Joonhoe Kim | this project | local (board DT) | `7c93f156e9234773` |
| `0109` | drm/panel: nt36523: baldur: add 90 Hz and 164 Hz modes | Joonhoe Kim | this project | local (panel modes) | `8ca0860580fae17d` |
| `0110` | drm/msm/dpu: baldur: lower the refresh rate in the kernel when idle | Joonhoe Kim | this project | local, not intended for upstream | `ac0d9c10b697a395` |
| `0111` | media: iris: vpu4x: size the decoder OPB line buffer for 10-bit output | Joonhoe Kim | this project | upstream candidate | `4e1ca9889fbbc5ea` |
| `0112` | media: iris: don't wait for the threaded IRQ handler under core->lock | Joonhoe Kim | this project | upstream candidate | `09cb4d1e2ba7c805` |
| `0113` | wifi: ath12k: keep the RX refill ring from running dry | Joonhoe Kim | this project | upstream candidate | `b7e380ed216b2ebe` |
| `0114` | net: qrtr: ns: retry announcements the new node is not ready for yet | Joonhoe Kim | this project | upstream candidate | `e1a306db29173484` |
| `0116` | media: qcom: camss: hold the bandwidth vote while TITAN_TOP is powered | Joonhoe Kim | this project | upstream candidate (0115 is a debugging aid kept out of the series) | `3b8ea94fd19e5fc5` |
| `0117` | arm64: dts: qcom: kaanapali: add CPU capacity-dmips-mhz | Joonhoe Kim | this project | upstream candidate | `cde3fbe0e37a205a` |
| `0118` | cpuidle: psci-domain: add allow_cluster_off to refuse cluster idle states | Joonhoe Kim | this project | local workaround: no cluster idle state in runtime idle on baldur, where its power-down reset the SoC when idle (set by the config fragment's command line; the cause is not fixed) | `9c63e2de94b80582` |
| `0119` | arm64: dts: qcom: kaanapali: describe the UFS MCQ registers | Joonhoe Kim | this project | upstream candidate | `28e15806d26509c5` |
| `0120` | firmware: arm_scmi: Add SCMI QCOM Generic Extension Protocol documentation | Pragnesh Papaniya | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (1/10) | pending upstream (RFC) | `82219b2577b9923e` |
| `0121` | dt-bindings: firmware: arm,scmi: Add Qualcomm Generic Extension Protocol | Pragnesh Papaniya | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (2/10) | pending upstream (RFC) | `144faa6e848f2626` |
| `0122` | firmware: arm_scmi: vendors: Add QCOM SCMI Generic Extensions | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (3/10) | pending upstream (RFC) | `327a4791a0e40e80` |
| `0123` | PM / devfreq: Add new target_freq attribute flag for governors | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (4/10) | pending upstream (RFC) | `092d0284962f9e64` |
| `0124` | PM / devfreq: Add new track_remote flag for governors | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (5/10) | pending upstream (RFC) | `96cdf0068418445d` |
| `0125` | PM / devfreq: Add a governor for tracking remote device frequencies | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (6/10) | pending upstream (RFC) | `c66b5228106f6d14` |
| `0126` | PM / devfreq: Introduce the QCOM SCMI Memlat devfreq driver | Sibi Sankar | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (7/10) | pending upstream (RFC) | `b6766bbbfd86a173` |
| `0127` | arm64: dts: qcom: kaanapali: Enable LLCC/DDR/DDR_QOS DVFS | Jia Yang | mailing list: Qualcomm Generic Vendor Extensions RFC v8, 2026-07-24 (Pragnesh Papaniya) (10/10) | pending upstream (RFC) | `756aac29f1b60aad` |
| `0128` | clk: qcom: gcc-kaanapali: Enable FORCE_MEM_CORE_ON for UFS AXI PHY clock | Joonhoe Kim | this project | upstream candidate (fixes s2idle resume with MCQ, 0119) | `339344f78c87f8df` |

## Out-of-tree

Sources built as external modules (not part of the patch series).

| Directory | What | Origin | License |
|---|---|---|---|
| [`out-of-tree/aw882xx/`](out-of-tree/aw882xx/) | Awinic smart amplifier driver `snd-soc-aw882xx` (speakers, chip ID `0x2308`), vendor driver `v1.15.0` with 7.x API fixes marked `tb323fu:` | AWINIC Technology Co., Ltd.; copy from [rockchip-linux/kernel](https://github.com/rockchip-linux/kernel) `develop-6.1` `sound/soc/codecs/aw882xx/` at `1feee0d9c0b20750eef52b06b9211a4a3a353895` | `GPL-2.0` (original headers kept; the C files' notice text adds "or any later version", see its README) |

The amplifier's run-time parameter file `aw882xx_acf.bin` is vendor data and is not included (see
[`firmware/`](../firmware/)).
