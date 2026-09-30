# SPDX-License-Identifier: MIT
{ lib, stdenvNoCC, src }:
stdenvNoCC.mkDerivation rec {
  pname = "gnome-shell-extension-tb323fu";
  version = "0.1.0";
  uuid = "tb323fu@joonhoekim.github.io";
  src = "${src}/userspace/desktop/gnome/extension/tb323fu@joonhoekim.github.io";
  dontBuild = true;
  installPhase = ''
    install -Dm644 -t $out/share/gnome-shell/extensions/${uuid} extension.js metadata.json
  '';
  passthru.extensionUuid = uuid;
  meta = with lib; {
    description = "GNOME Shell quick settings for the TB323FU helper";
    license = licenses.gpl2Plus;
    platforms = platforms.linux;
  };
}
