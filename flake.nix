# SPDX-License-Identifier: MIT
# Nix packages and NixOS modules for the TB323FU (Lenovo Legion Tab Gen 5):
# platform files, helper daemon + CLI, settings app, GNOME extension, and the
# NixOS root filesystem (rootfs/nixos). The kernel is not built here (see
# kernel/README.md); packaging/nix/prebuilt-kernel.nix wraps the one that boots.
# Built on the device (aarch64-linux, nixos-unstable, 2026-10).
{
  description = "Lenovo Legion Tab Gen 5 (TB323FU) userspace: platform files, helper, NixOS";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-linux" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAll (pkgs: rec {
        tb323fu-platform = pkgs.callPackage ./packaging/nix/platform.nix { src = self; };
        tb323fu-helper = pkgs.callPackage ./packaging/nix/helper.nix { src = self; };
        tb323fu-settings = pkgs.callPackage ./packaging/nix/settings.nix { src = self; };
        tb323fu-helper-gnome = pkgs.callPackage ./packaging/nix/extension.nix { src = self; };
        default = tb323fu-helper;
      });

      # services.tb323fu (platform files, helper, settings app, extension)
      nixosModules.default = import ./packaging/nix/module.nix self;
      # the whole system for a tb323fu-* partition; needs the device-local
      # tb323fu.rootfs.* settings that rootfs/nixos/build-rootfs.sh writes
      nixosModules.rootfs = {
        imports = [ self.nixosModules.default ./rootfs/nixos/configuration.nix ];
      };
    };
}
