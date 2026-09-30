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
| community tree | [kaanapali-mainline/linux](https://github.com/kaanapali-mainline/linux) `testing` at `37a76066` (patches 0007-0011, carried; the full classification of its 92 commits was done in the development notes) |
| community tree | [infiniti-mainline/linux](https://github.com/infiniti-mainline/linux) `oneplus-15` / `master` (patches 0024, 0029, 0030, 0070-0077, 0084-0094, 0096, 0099-0102) |
| touch driver | [map220v/sm8850-mainline](https://github.com/map220v/sm8850-mainline) `iceland-7.2` (0025) |

## Patches

`sha256` is the first 16 hex digits of the SHA-256 of the patch file as stored here.
0111 and 0112 carry no `Signed-off-by` yet; the author adds it before sending them upstream.

| # | Subject | Author (`From:`) | Origin | Status | sha256 |
|---|---|---|---|---|---|
| `0001` | arm64: dts: qcom: kaanapali: add the GPU SMMU node | Qingqing Zhou | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `ea84290779bebe3c` |
| `0002` | arm64: dts: qcom: kaanapali: Add QFPROM node | Jingyi Wang | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `0ce87440b99f68d8` |
| `0003` | arm64: dts: qcom: Add GPU support for Kaanapali | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `6089dc88850d1987` |
| `0004` | arm64: dts: qcom: kaanapali: Add GPU cooling | Gaurav Kohli | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `1674574f14593262` |
| `0005` | arm64: dts: qcom: kaanapali-mtp: Enable GPU | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `3541b3db577bac30` |
| `0006` | arm64: dts: qcom: kaanapali-qrd: Enable GPU | Akhil P Oommen | qcom tree `arm64-for-7.4` (kaanapali GPU DT v3, Link: in patch) | upstream (queued for v7.4) | `01c93e8d2c936217` |
| `0007` | drm/msm: DPU, DSI and MDSS fixes for AA601 cmd-mode DSC | idusergod | community [kaanapali-mainline](https://github.com/kaanapali-mainline/linux) testing `b6eab7f0e8` (partial) | community, not upstream | `9ac8edf8c34ced85` |
| `0008` | drm/msm: cmd-mode DSC 1.2 fixes for DPU 13 (Kaanapali) | EYC | community kaanapali-mainline `144a400716` | community, not upstream | `25ac8cf324780678` |
| `0009` | clk: qcom: dispcc-kaanapali: solve display artifacts at start | Nazar Kompanets | community kaanapali-mainline `6ba50855c6` | community, not upstream | `8bce12997f57965c` |
| `0010` | drm/msm/adreno: A840: drop IFPC quirk for Infiniti bring-up | idusergod | community kaanapali-mainline `6a4127a8cd` (reverted by 0080) | community; net no-op with 0080 | `931fe187f8ad3cb8` |
| `0011` | arm64: configs: add kaanapali-oneplus-infiniti_defconfig fragment | idusergod | community kaanapali-mainline `37a7606647` (defconfig fragment) | community; base of the baldur fragment | `4a8291cd05182f6c` |
| `0012` | scsi: ufs: ufs-qcom: Enable only lane clocks in lane clock APIs | Nitin Rawat | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `f3343cbe58f236e9` |
| `0013` | phy: qcom-mipi-csi2: Add a CSI2 MIPI DPHY driver | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `8e7938167b0d106d` |
| `0014` | phy: core: Fix race-condition between _of_phy_get() and try_module_get() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `2f0487131f392c19` |
| `0015` | phy: core: Add phy_get_by_of_node() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `c550554888abf161` |
| `0016` | phy: core: Add devm_phy_get_by_of_node() | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `644f6ad80e7c4e50` |
| `0017` | phy: core: Add missing kerneldoc colon in two locations | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `5cfc4b0ebbd9695b` |
| `0018` | dt-bindings: phy: qcom: Add CSI2 C-PHY/DPHY schema | Bryan O'Donoghue | linux-next next-20260925 (Link: in patch) | upstream (linux-next) | `88335aa812c5c110` |
| `0019` | arm64: dts: qcom: kaanapali: add the second DSI controller and PHY | joonhoekim | this project | local | `0e0257ecfc17d2b2` |
| `0020` | drm/panel: nt36523: add DSC, and the Lenovo TB323FU CSOT panel | joonhoekim | this project | local | `9e032179c68c97da` |
| `0021` | arm64: carry a devicetree inside the kernel image | joonhoekim | this project | local (built-in DTB; bootloader workaround) | `86368c4d49c32ff4` |
| `0022` | arm64: paint boot progress marks into the bootloader's framebuffer | joonhoekim | this project | local (debug: boot progress marks) | `835b18addc777b85` |
| `0023` | drm/msm: video-mode DSC at 10 bpc -- make DSI and the DPU INTF agree on the line | joonhoekim | this project | local | `25e21c5aacaabedb` |
| `0024` | wifi: ath12k + PCI/pwrctrl: WCN7860 ("peach", 17cb:110e) | joonhoekim | [infiniti-mainline](https://github.com/infiniti-mainline/linux) `oneplus-15`, several authors, imported squashed (From: is the importer) | community, not upstream | `1412ff954c31d01e` |
| `0025` | Input: touchscreen: import NT36536 host-download SPI driver (Novatek) | joonhoekim | [map220v/sm8850-mainline](https://github.com/map220v/sm8850-mainline) `iceland-7.2` (Novatek vendor driver, verbatim) | out of tree, not upstreamable | `82b01bcce08b5a8d` |
| `0026` | input: touchscreen: NT36536 host-download SPI driver (Novatek) | joonhoekim | this project | local | `f82d278351321f12` |
| `0027` | input: nt36536: fixes for TB323FU (y705-mainline-plan.md 9-27, 9-28) | joonhoekim | this project | local | `a571a9fb49b267d5` |
| `0028` | backlight: aw99706: replay board registers after the HWEN reset | joonhoekim | this project | local | `3775c2851b5dfa8b` |
| `0029` | iommu: arm-smmu-qcom: kaanapali MDSS identity domain | EYC | community / infiniti-mainline (EYC) | community, not upstream | `98325a0a17611918` |
| `0030` | Bluetooth: qca: WCN7860 (BRAHMA) on the WCN7850 flow | joonhoekim | infiniti-mainline `fecd7925d` (EYC; From: is the importer) | community, not upstream | `f14da6306a25551d` |
| `0031` | arm64: dts: qcom: kaanapali: CPU6-7 in a PSCI cluster domain of their own | joonhoekim | this project | local | `a1931f6821e96cbe` |
| `0032` | drm/msm/dsi: phy: prepare the iface clock only while runtime active | joonhoekim | this project | local | `8745d6508e84045c` |
| `0033` | power: supply: qcom_battmgr: let the host set the USB input current limit | joonhoekim | this project | local | `862af7b91be29771` |
| `0034` | ASoC: qcom: sc8280xp: optional S32_LE on MI2S backends | joonhoekim | this project | local | `6e1d08e2565be329` |
| `0035` | ASoC: codecs: lpass-tx-macro: board tuning of the decimator filter block | joonhoekim | this project | local | `3b9d8798267578f7` |
| `0036` | usb: gadget: f_ncm: restart the TX timer when the freelist is empty | joonhoekim | this project | local | `0617f0c53da4dcb8` |
| `0037` | ASoC: codecs: wcd939x: read the ADC/DMIC switches per channel | joonhoekim | this project | local | `d071182c45dd5f3a` |
| `0038` | Input: aw86927 - accept the AW86937 | joonhoekim | this project | local | `09ed9c3b8d8772c8` |
| `0039` | y705 0025-pmic5-gen4-adc: mailing-list series (squashed) | joonhoekim | mailing list: PMIC5 Gen4 ADC series (Jishnu Prakash), squashed | pending upstream | `334c82260a9ba2b2` |
| `0040` | y705 0027-media-qcom-camss-kaanapali-v16: mailing-list series (squashed) | joonhoekim | mailing list: CAMSS kaanapali v16 (Hangxiang Ma), squashed | pending upstream | `efb27f723d4158bc` |
| `0041` | y705 0028-media-i2c-s5kjn5-v4: mailing-list series (squashed) | joonhoekim | mailing list: S5KJN5 sensor v4 (Wenmeng Liu), squashed | pending upstream | `f63d8a5a32647955` |
| `0042` | media: qcom: camss: Add support for PHY API devices | Bryan O'Donoghue | mailing list: camss PHY API series 4/5 (Bryan O'Donoghue) | pending upstream | `b10dc4cce7742860` |
| `0043` | media: qcom: camss: Use data-lanes starting at 1 for new CSIPHY mode | Bryan O'Donoghue | mailing list: camss PHY API series 5/5 (Bryan O'Donoghue) | pending upstream | `36389bd9fd734fa3` |
| `0044` | pinctrl: qcom: kaanapali: add the I2C strong pull-up bit | joonhoekim | this project | local | `167056b94c92cc33` |
| `0045` | media: i2c: add Samsung S5KJNS and GalaxyCore GC08A8 sensor drivers | joonhoekim | this project | local | `b80d9950a5008849` |
| `0046` | clk: qcom: gdsc: keep the clock controller active while a GDSC is on | joonhoekim | this project | local | `cd1a0421542d34b9` |
| `0047` | media: i2c: dw9719: add Giantec GT9764, optional vio supply | joonhoekim | this project | local | `fb434838c377c614` |
| `0048` | leds: rgb: add Awinic AW22127 driver | joonhoekim | this project | local | `61caa5d6d7dcb083` |
| `0049` | PCI: qcom: don't advertise a hot-plug slot on the Root Port | joonhoekim | this project | local | `27005b0184777fc3` |
| `0050` | drm/msm/dsi: phy: runtime suspend the PHY over system sleep | joonhoekim | this project | local | `57a30beaa65c52b7` |
| `0051` | drm/msm/dp: add kaanapali (DP0 descriptor) | joonhoekim | this project | local | `bae5d3bbb069f3ac` |
| `0052` | drm/msm/dpu: add the SSPP rec0/rec1 blocks to the snapshot | joonhoekim | this project | local | `a8f52fe395fba5c1` |
| `0053` | input: nt36536: make the pen a tablet libinput accepts | joonhoekim | this project | local | `f4703f6905169ca5` |
| `0054` | arm64: dts: qcom: add Lenovo Legion Tab Y700 gen5 (TB323FU, baldur) board DTs | joonhoekim | this project | local | `9247be805bbdc9f1` |
| `0055` | arm64: configs: add the baldur config fragments (y705 mainline/kernel) | joonhoekim | this project | local | `07c38f610992f2c0` |
| `0056` | PCI: qcom: parse iommu-map with the target #iommu-cells (y705 0041) | joonhoekim | this project | local; superseded by linux-next (`qcom_pcie_config_sid_1_9_0`) | `98e27357a1b066ca` |
| `0057` | drm/msm/dp: retrain the link on a quick replug while streaming (y705 0042) | joonhoekim | this project | local | `f508d95246d9f750` |
| `0058` | wifi: ath12k: don't wake the device over MHI from the panic notifier (y705 0043) | joonhoekim | this project | local | `b4c7cf5c3af95df7` |
| `0059` | clk: qcom: gcc-kaanapali: keep the USB GDSCs in retention (y705 0044) | joonhoekim | this project | local | `21297da8b406b40d` |
| `0060` | soc: qcom: pmic_glink_altmode: handle notifications on the freezable workqueue (y705 0045) | joonhoekim | this project | local; superseded upstream (`7d0767c5cd87`, freezable workqueue) | `fe479f7206778a8b` |
| `0061` | usb: typec: ucsi: run connector change handling on the freezable workqueue (y705 0046) | joonhoekim | this project | local | `87f68909e7dc7ebd` |
| `0062` | usb: dwc3: qcom: arm the eUSB2 line interrupts for wakeup (y705 0048) | joonhoekim | this project | local | `8fe0f725cbc0faac` |
| `0063` | media: i2c: s5kjns: the colour filter order is GRBG, not GBRG | joonhoekim | this project | local | `9478195eba96551c` |
| `0064` | arm64: dts: qcom: baldur: camera orientation and rotation | joonhoekim | this project | local | `52ab71b74ba880c1` |
| `0065` | drm/msm/dpu+dsi: change the vertical front porch in place (y705 0051) | joonhoekim | this project | local | `36e758e3ac884b44` |
| `0066` | drm/panel: nt36523: baldur CSOT 60 and 30 Hz modes (y705 0052) | joonhoekim | this project | local | `74ff9392396e2a2a` |
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
| `0078` | arm64: dts: qcom: baldur: WCN7860 WAKE# is active low (y705 0053) | joonhoekim | this project | local | `43399c31eba0721e` |
| `0079` | drm/msm/dpu: keep the full modeset when colour management changed (y705 0051 fixup) | joonhoekim | this project | local | `07b1c970eaa518ad` |
| `0080` | Revert "drm/msm/adreno: A840: drop IFPC quirk for Infiniti bring-up" (y705 0080) | joonhoekim | this project | local | `bf34522b7d02d857` |
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
| `0095` | arm64: dts: qcom: baldur: enable the iris video codec (y705 0054) | joonhoekim | this project | local | `b468f29fef780b89` |
| `0096` | phy: qcom: qmp-combo: Add Kaanapali USB3+DP PHY | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `1e0e578ff21fb961` |
| `0097` | drm/msm/dpu: stop all video interfaces before cleaning up a split encoder | joonhoekim | this project | sent upstream 2026-09-29, under review | `709ac802705c96bf` |
| `0098` | drm/msm/dp: hold one runtime PM reference per plugged state | joonhoekim | this project | sent upstream 2026-09-29, under review | `acb641feb39fe870` |
| `0099` | phy: qcom: qmp-combo: Drop the stale err_disable_pipe_clk teardown | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `4cc9500eb779c89d` |
| `0100` | phy: qcom: qmp-combo: Keep the DP lanes of a USB-capable DP sink | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `6f508562ddb6e8ec` |
| `0101` | drm/msm/dp: check the PHY power-on and DPCD link status returns | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `4711f13cff69ad11` |
| `0102` | drm/msm/dp: reset the link caps on every DPCD read | Victor Fuentes | infiniti-mainline (Victor Fuentes) | community, not upstream | `bc3e46a9718d4891` |
| `0103` | clk: qcom: gdsc: drop the controller reference only if it was taken | joonhoekim | this project | local | `d8f8d6675cba9077` |
| `0104` | usb: dwc3: tell xHCI that it lost its state when the core is powered off | joonhoekim | this project | sent upstream 2026-09-29, under review | `2a447d4ba9a36679` |
| `0105` | drm/msm/dpu: compute the CRTC bandwidth from the state being checked | joonhoekim | this project | sent upstream 2026-09-29, under review | `8eb9e32dfaa88e5d` |
| `0106` | cpuidle: psci-domain: baldur: use the system domain state only in system suspend | Joonhoe Kim | this project | local workaround (cpuidle system domain state only in suspend) | `09a5acaa07543da4` |
| `0107` | power: supply: qcom_battmgr: fix the battery current sign on Kaanapali | Joonhoe Kim | this project | local | `13bf1790ca49e953` |
| `0108` | arm64: dts: qcom: baldur: throttle on the board temperature like Android | Joonhoe Kim | this project | local (board DT) | `14085b89a9b601e6` |
| `0109` | drm/panel: nt36523: baldur: add 90 Hz and 164 Hz modes | joonhoekim | this project | local (panel modes) | `ffb468930d448bff` |
| `0110` | drm/msm/dpu: baldur: lower the refresh rate in the kernel when idle | joonhoekim | this project | local, not intended for upstream | `0f1118cc66ceb91c` |
| `0111` | media: iris: vpu4x: size the decoder OPB line buffer for 10-bit output | Joonhoe Kim | this project | upstream candidate; Signed-off-by to be added before submission | `60f0f363b5542ed9` |
| `0112` | media: iris: don't wait for the threaded IRQ handler under core->lock | Joonhoe Kim | this project | upstream candidate; Signed-off-by to be added before submission | `e7c29a4c0e69ba15` |
