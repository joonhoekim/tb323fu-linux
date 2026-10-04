# SPDX-License-Identifier: MIT
{ lib, rustPlatform, src }:
let
  # only this tree, so edits elsewhere in the repository do not rebuild it
  tree = builtins.path { path = "${src}/helper"; name = "tb323fu-helper-src"; };
in
rustPlatform.buildRustPackage {
  pname = "tb323fu-helper";
  version = "0.3.0";
  src = tree;
  cargoLock.lockFile = "${tree}/Cargo.lock";
  # tests start a private D-Bus daemon; run them outside the sandbox
  doCheck = false;
  postInstall = ''
    mkdir -p $out/libexec
    mv $out/bin/tb323fu-helperd $out/libexec/
    sed "s|@LIBEXECDIR@|$out/libexec|" data/tb323fu-helperd.service > unit
    install -Dm644 unit $out/lib/systemd/system/tb323fu-helperd.service
    install -Dm755 data/tb323fu-kernel-fetch $out/libexec/tb323fu-kernel-fetch
    sed "s|@LIBEXECDIR@|$out/libexec|" data/tb323fu-kernel-fetch.service > unit
    install -Dm644 unit $out/lib/systemd/system/tb323fu-kernel-fetch.service
    install -Dm644 data/io.github.joonhoekim.OpenDeviceHelper1.conf $out/share/dbus-1/system.d/io.github.joonhoekim.OpenDeviceHelper1.conf
    install -Dm644 data/io.github.joonhoekim.OpenDeviceHelper1.service $out/share/dbus-1/system-services/io.github.joonhoekim.OpenDeviceHelper1.service
    install -Dm644 data/io.github.joonhoekim.opendevicehelper.policy $out/share/polkit-1/actions/io.github.joonhoekim.opendevicehelper.policy
    install -Dm644 data/helper.toml.example $out/share/doc/tb323fu-helper/helper.toml.example
  '';
  meta = with lib; {
    description = "TB323FU device helper daemon (D-Bus + polkit) and CLI";
    license = licenses.gpl3Plus;
    platforms = platforms.linux;
    mainProgram = "tb323fu-ctl";
  };
}
