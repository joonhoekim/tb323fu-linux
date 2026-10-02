# SPDX-License-Identifier: MIT
# The TB323FU kernel as NixOS sees it. The kernel is built outside Nix (see
# kernel/README.md) and boots from the boot image together with its built-in
# initramfs, which then hands over to NixOS stage 2 -- so NixOS never loads
# this kernel. modDirVersion and the modules tree decide where NixOS's kmod
# finds modules: it tries <dir>/lib/modules/<uname -r> for
# /run/booted-system/kernel-modules, /run/current-system/kernel-modules, then
# plain /lib/modules -- the first directory that exists wins.
#
# Two modes:
#
# - Shared modules (modules = null, the default): a stub. The boot image
#   carries all modules of its kernel and the initramfs mounts them on
#   /lib/modules/<release> of the root it boots (kernel/initramfs/init). The
#   generation holds only an empty lib/modules/0-tb323fu-shared, so kmod finds
#   no directory for the running release there and falls through to
#   /lib/modules/<release> = the initramfs' mount. A kernel update then needs
#   no NixOS rebuild. boot.extraModulePackages cannot work in this mode (they
#   would be built against the stub).
# - Own modules (modules = a copy of /lib/modules/<version>): the tree goes
#   into the store (system.modulesTree ->
#   /run/booted-system/kernel-modules/lib/modules/<version>) and modDirVersion
#   must match `uname -r`; every kernel change needs a rebuild. For a NixOS
#   root marked "own" (/etc/tb323fu/modules) or a boot image without the
#   modules image.
#
# The kernel configuration is optional in both (kernel.config for NixOS
# modules that query it, e.g. sysctl.nix; in stub mode its header line also
# gives the version).
#
#   pkgs.callPackage ./prebuilt-kernel.nix {
#     modules = null;                             # or /path/to/lib/modules/<version>
#     configfile = /path/to/config;               # zcat /proc/config.gz (optional)
#     image = null;                               # the Image (optional, not used to boot)
#     kernelPatches = [ ];                        # explicitly: callPackage would pass pkgs.kernelPatches
#   }
#
# Use it with boot.kernelPackages = pkgs.linuxPackagesFor <this>.
{ lib, stdenvNoCC, modules ? null, configfile ? null, image ? null
, modDirVersion ? if modules == null then "0-tb323fu-shared" else baseNameOf (toString modules)
# passed by NixOS (boot.kernelPackages applies kernel.override with these);
# a prebuilt kernel cannot take patches, so they must stay empty
, kernelPatches ? [ ], features ? { }, randstructSeed ? ""
}:
let
  shared = modules == null;
  # "# Linux/arm64 7.3.0-rc4 Kernel Configuration" (the .config header)
  cfgLines = if configfile == null then [ ] else lib.splitString "\n" (builtins.readFile configfile);
  cfgHeader = lib.findFirst (l: builtins.match "# Linux/.* Kernel Configuration" l != null) null cfgLines;
  cfgMatch = if cfgHeader == null then null
    else builtins.match "# Linux/[^ ]+ ([0-9]+\\.[0-9]+(\\.[0-9]+)?(-rc[0-9]+)?)[^ ]* Kernel Configuration" cfgHeader;
  cfgVersion = if cfgMatch == null then null else lib.head cfgMatch;
  # "7.3.0-rc4-oneplus-infiniti+" -> "7.3.0-rc4"
  m = builtins.match "([0-9]+\\.[0-9]+(\\.[0-9]+)?(-rc[0-9]+)?).*" modDirVersion;
  version =
    if shared then (if cfgVersion != null then cfgVersion else "7.3.0")
    else if m == null then modDirVersion else lib.head m;
  # the same attribute set linuxManualConfig makes from a .config
  parsed =
    if configfile == null then { }
    else lib.listToAttrs (map (kv: lib.nameValuePair (lib.elemAt kv 0) (lib.elemAt kv 1))
      (lib.filter (kv: kv != null)
        (map (builtins.match "(CONFIG_[A-Za-z0-9_]+)=\"?([^\"]*)\"?")
          cfgLines)));
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
  pname = if shared then "linux-tb323fu-shared-modules" else "linux-tb323fu-prebuilt";
  inherit version;
  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;   # prebuilt modules: no stripping, no patching
  installPhase = ''
    runHook preInstall
    mkdir -p $out/lib/modules
    ${if shared then ''
      # the stub: no modules; empty index files so the directory exists in
      # the aggregated tree (depmod runs on it at build time)
      mkdir -p $out/lib/modules/${modDirVersion}
      : > $out/lib/modules/${modDirVersion}/modules.order
      : > $out/lib/modules/${modDirVersion}/modules.builtin
    '' else ''
      cp -r --no-preserve=mode ${modules} $out/lib/modules/${modDirVersion}
      # links into the build tree of the machine that compiled it
      rm -f $out/lib/modules/${modDirVersion}/build $out/lib/modules/${modDirVersion}/source
    ''}
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
    sharedModules = shared;
  };
  meta = {
    description = if shared
      then "TB323FU kernel stub: modules come from the boot image (shared modules mount)"
      else "Prebuilt TB323FU kernel modules (the kernel itself boots from the boot image)";
    license = lib.licenses.gpl2Only;
    platforms = [ "aarch64-linux" ];
  };
}
