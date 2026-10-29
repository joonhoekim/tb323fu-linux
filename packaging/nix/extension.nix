# SPDX-License-Identifier: MIT
{ lib, stdenvNoCC, src }:
let
  uuid = "tb323fu@joonhoekim.github.io";
in
stdenvNoCC.mkDerivation {
  pname = "gnome-shell-extension-tb323fu";
  version = (lib.importTOML "${src}/helper/Cargo.toml").workspace.package.version;
  src = "${src}/userspace/desktop/gnome/extension/${uuid}";
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
