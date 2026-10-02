# SPDX-License-Identifier: MIT
{ lib, rustPlatform, pkg-config, wrapGAppsHook4, gtk4, libadwaita, src }:
rustPlatform.buildRustPackage {
  pname = "tb323fu-settings";
  version = "0.1.0";
  src = builtins.path { path = "${src}/helper/crates/tb323fu-settings"; name = "tb323fu-settings-src"; };
  cargoLock.lockFile = "${src}/helper/crates/tb323fu-settings/Cargo.lock";
  nativeBuildInputs = [ pkg-config wrapGAppsHook4 ];
  buildInputs = [ gtk4 libadwaita ];
  postInstall = ''
    id=io.github.joonhoekim.tb323fu.Settings
    install -Dm644 data/$id.desktop $out/share/applications/$id.desktop
    install -Dm644 data/$id.svg $out/share/icons/hicolor/scalable/apps/$id.svg
    install -Dm644 data/$id.metainfo.xml $out/share/metainfo/$id.metainfo.xml
  '';
  meta = with lib; {
    description = "Settings app for the TB323FU helper (GTK4/libadwaita)";
    license = licenses.gpl3Plus;
    platforms = platforms.linux;
    mainProgram = "tb323fu-settings";
  };
}
