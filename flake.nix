# SPDX-License-Identifier: MIT
# Nix packages and a NixOS module for the TB323FU (Lenovo Legion Tab Gen 5)
# userspace: platform files, helper daemon + CLI, settings app, GNOME
# extension. The kernel is not built here (see kernel/README.md).
# Untested: no Nix installation was available when this was written.
{
  description = "Lenovo Legion Tab Gen 5 (TB323FU) userspace: platform files and helper";

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

      nixosModules.default = import ./packaging/nix/module.nix self;
    };
}
