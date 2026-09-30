# SPDX-License-Identifier: MIT
# The TB323FU kernel as NixOS sees it. The kernel is built outside Nix (see
# kernel/README.md) and boots from the boot image together with its built-in
# initramfs, which then hands over to NixOS stage 2 -- so NixOS never loads
# this kernel. What it does need is the modules tree of the kernel that boots
# (system.modulesTree -> /run/booted-system/kernel-modules/lib/modules/<ver>,
# where NixOS's kmod looks) and a modDirVersion that matches `uname -r`. The
# kernel configuration is optional (kernel.config for modules that query it).
#
#   pkgs.callPackage ./prebuilt-kernel.nix {
#     modules = /path/to/lib/modules/<version>;   # a copy of /lib/modules/$(uname -r)
#     configfile = /path/to/config;               # zcat /proc/config.gz (optional)
#     image = null;                               # the Image (optional, not used to boot)
#     kernelPatches = [ ];                        # explicitly: callPackage would pass pkgs.kernelPatches
#   }
#
# Use it with boot.kernelPackages = pkgs.linuxPackagesFor <this>.
{ lib, stdenvNoCC, modules, configfile ? null, image ? null
, modDirVersion ? baseNameOf (toString modules)
# passed by NixOS (boot.kernelPackages applies kernel.override with these);
# a prebuilt kernel cannot take patches, so they must stay empty
, kernelPatches ? [ ], features ? { }, randstructSeed ? ""
}:
let
  # "7.3.0-rc4-oneplus-infiniti+" -> "7.3.0-rc4"
  m = builtins.match "([0-9]+\\.[0-9]+(\\.[0-9]+)?(-rc[0-9]+)?).*" modDirVersion;
  version = if m == null then modDirVersion else lib.head m;
  # the same attribute set linuxManualConfig makes from a .config
  parsed =
    if configfile == null then { }
    else lib.listToAttrs (map (kv: lib.nameValuePair (lib.elemAt kv 0) (lib.elemAt kv 1))
      (lib.filter (kv: kv != null)
        (map (builtins.match "(CONFIG_[A-Za-z0-9_]+)=\"?([^\"]*)\"?")
          (lib.splitString "\n" (builtins.readFile configfile)))));
  config = {
    isSet = attr: parsed ? "CONFIG_${attr}";
    getValue = attr: parsed."CONFIG_${attr}" or null;
    isYes = attr: config.getValue attr == "y";
    isNo = attr: config.getValue attr == "n";
    isModule = attr: config.getValue attr == "m";
    isEnabled = attr: config.isModule attr || config.isYes attr;
    isDisabled = attr: !(config.isSet attr) || config.isNo attr;
  } // parsed;
in
assert lib.assertMsg (kernelPatches == [ ]) "prebuilt-kernel.nix: boot.kernelPatches cannot apply to a prebuilt kernel";
stdenvNoCC.mkDerivation {
  pname = "linux-tb323fu-prebuilt";
  inherit version;
  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;   # prebuilt modules: no stripping, no patching
  installPhase = ''
    runHook preInstall
    mkdir -p $out/lib/modules
    cp -r --no-preserve=mode ${modules} $out/lib/modules/${modDirVersion}
    # links into the build tree of the machine that compiled it
    rm -f $out/lib/modules/${modDirVersion}/build $out/lib/modules/${modDirVersion}/source
    ${if image != null then "cp ${image} $out/Image"
      else "echo 'placeholder: this kernel boots from the boot image' > $out/Image"}
    ${lib.optionalString (configfile != null) "cp ${configfile} $out/config"}
    runHook postInstall
  '';
  passthru = {
    # `features` also makes NixOS skip its requiredKernelConfig assertions (it
    # assumes a Nix-built kernel); `config` is still there for modules that ask
    inherit modDirVersion config features;
    kernelOlder = lib.versionOlder version;
    kernelAtLeast = lib.versionAtLeast version;
    isHardened = false;
    isLibre = false;
    isZen = false;
    buildDTBs = false;   # the device tree is in the boot image too
    target = "Image";    # boot.loader.kernelFile (the placeholder above)
    # sysctl.nix reads CONFIG_ARCH_MMAP_RND_BITS_MAX from it
    configfile = if configfile != null then configfile else builtins.toFile "config" "";
  };
  meta = {
    description = "Prebuilt TB323FU kernel modules (the kernel itself boots from the boot image)";
    license = lib.licenses.gpl2Only;
    platforms = [ "aarch64-linux" ];
  };
}
