# SPDX-License-Identifier: MIT
# Platform files (layer 1). install.sh rewrites /usr/libexec/tb323fu to the
# store path when PREFIX is not /usr; configuration stays under /etc/tb323fu
# (mutable -- the helper writes there), so the defaults are shipped under
# share/tb323fu/etc and copied by the NixOS module when absent.
{ lib, stdenv, src }:
stdenv.mkDerivation {
  pname = "tb323fu-platform";
  version = "0.1.0";
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
    patchShebangs $out/libexec $out/sbin $out/lib/systemd/system-sleep
    runHook postInstall
  '';
  meta = with lib; {
    description = "Lenovo Legion Tab Gen 5 (TB323FU) platform files";
    license = licenses.mit;
    platforms = [ "aarch64-linux" ];
  };
}
