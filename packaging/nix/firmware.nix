# SPDX-License-Identifier: MIT
# The device firmware for hardware.firmware, taken from a local directory --
# firmware is never part of this repository (it is extracted from the user's
# own device, see firmware/README.md). The layout is the one under /lib/firmware
# of an installed system: qcom/ (DSPs, GPU, ...), ath12k/ (Wi-Fi), qca/
# (Bluetooth), novatek/ (touch) and aw882xx_acf.bin (speaker amplifiers).
#
#   pkgs.callPackage ./firmware.nix { firmware = /path/to/lib/firmware; }
#
# Keep hardware.firmwareCompression = "none" (the NixOS module does): the DSP
# service tests for adsp.mbn by name and remoteproc loads split .bNN segments.
{ lib, stdenvNoCC, firmware
, dirs ? [ "qcom" "ath12k" "qca" "novatek" ]
, files ? [ "aw882xx_acf.bin" ]
}:
stdenvNoCC.mkDerivation {
  pname = "tb323fu-firmware";
  version = "local";
  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;
  installPhase = ''
    runHook preInstall
    mkdir -p $out/lib/firmware
    for d in ${lib.escapeShellArgs dirs}; do
      [ -d ${firmware}/$d ] && cp -r --no-preserve=mode ${firmware}/$d $out/lib/firmware/
    done
    for f in ${lib.escapeShellArgs files}; do
      [ -e ${firmware}/$f ] && cp --no-preserve=mode ${firmware}/$f $out/lib/firmware/
    done
    runHook postInstall
  '';
  meta = {
    description = "TB323FU firmware from a local copy (not redistributable)";
    platforms = [ "aarch64-linux" ];
  };
}
