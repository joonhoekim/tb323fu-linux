# SPDX-License-Identifier: MIT
{ lib, rustPlatform, src }:
rustPlatform.buildRustPackage {
  pname = "tb323fu-helper";
  version = "0.1.0";
  src = "${src}/helper";
  cargoLock.lockFile = "${src}/helper/Cargo.lock";
  # tests start a private D-Bus daemon; run them outside the sandbox
  doCheck = false;
  postInstall = ''
    mkdir -p $out/libexec
    mv $out/bin/tb323fu-helperd $out/libexec/
    sed "s|@LIBEXECDIR@|$out/libexec|" data/tb323fu-helperd.service > unit
    install -Dm644 unit $out/lib/systemd/system/tb323fu-helperd.service
    install -Dm644 data/io.github.joonhoekim.tb323fu.Helper.conf $out/share/dbus-1/system.d/io.github.joonhoekim.tb323fu.Helper.conf
    install -Dm644 data/io.github.joonhoekim.tb323fu.Helper.service $out/share/dbus-1/system-services/io.github.joonhoekim.tb323fu.Helper.service
    install -Dm644 data/io.github.joonhoekim.tb323fu.helper.policy $out/share/polkit-1/actions/io.github.joonhoekim.tb323fu.helper.policy
    install -Dm644 data/helper.toml.example $out/share/doc/tb323fu-helper/helper.toml.example
  '';
  meta = with lib; {
    description = "TB323FU device helper daemon (D-Bus + polkit) and CLI";
    license = licenses.mit;
    platforms = platforms.linux;
    mainProgram = "tb323fu-ctl";
  };
}
