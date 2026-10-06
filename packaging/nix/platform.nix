# SPDX-License-Identifier: MIT
# Platform files (layer 1). install.sh rewrites /usr/libexec/tb323fu to the
# store path when PREFIX is not /usr; configuration stays under /etc/tb323fu
# (mutable -- the helper writes there), so the defaults are shipped under
# share/tb323fu/etc and copied by the NixOS module when absent. NixOS has no
# /lib/firmware: the units name /run/current-system/firmware instead, which
# the module links to hardware.firmware.
{ lib, stdenv, busybox, src }:
stdenv.mkDerivation {
  pname = "tb323fu-platform";
  version = "0.4.0";
  inherit src;
  dontConfigure = true;
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    stage=$TMPDIR/stage
    DESTDIR=$stage PREFIX=$out SYSCONFDIR=/etc CC=$CC sh userspace/platform/install.sh
    mkdir -p $out
    cp -a $stage$out/. $out/
    mkdir -p $out/share/tb323fu
    cp -a $stage/etc $out/share/tb323fu/etc
    for u in $out/lib/systemd/system/*.service; do
      substituteInPlace "$u" --replace-quiet /lib/firmware/ /run/current-system/firmware/
    done
    # back-to-android is also the initramfs' copy: it runs under busybox sh
    # (not busybox in buildInputs -- its find/xargs would shadow stdenv's)
    substituteInPlace $out/sbin/back-to-android --replace-fail '#!/bin/sh' '#!${busybox}/bin/busybox sh'
    patchShebangs $out/libexec $out/sbin $out/lib/systemd/system-sleep
    runHook postInstall
  '';
  meta = with lib; {
    description = "Lenovo Legion Tab Gen 5 (TB323FU) platform files";
    license = licenses.mit;
    platforms = [ "aarch64-linux" ];
  };
}
