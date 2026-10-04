# SPDX-License-Identifier: MIT
# NixOS for the TB323FU, installed on a GPT partition named tb323fu-* by
# build-rootfs.sh (next to this file). There is no NixOS bootloader and no
# NixOS stage 1: the boot image carries the kernel and its initramfs
# (kernel/initramfs/init), which mounts the partition and hands over to
# /nix/var/nix/profiles/system/init -- NixOS stage 2 -- so the newest system
# generation is the one that boots (roll back with
# `nix-env -p /nix/var/nix/profiles/system --rollback`).
#
# Kernel modules come from the boot image: its initramfs mounts them on
# /lib/modules/<release> (shared modules), and the kernel package here is a
# stub without modules (packaging/nix/prebuilt-kernel.nix), so a kernel update
# needs no rebuild. Everything device-specific (the kernel's configuration,
# optionally its modules for an "own"-mode root, firmware,
# sensor files, SSH keys, password hashes) is set through the tb323fu.rootfs.*
# options by a local.nix that build-rootfs.sh writes outside this repository.
{ config, lib, pkgs, ... }:
let
  cfg = config.tb323fu.rootfs;
  inherit (lib) mkOption mkIf types;
  kernel = pkgs.callPackage ../../packaging/nix/prebuilt-kernel.nix {
    modules = cfg.kernel.modules;
    configfile = cfg.kernel.config;
    # (callPackage would fill this one with pkgs.kernelPatches, an attribute set)
    kernelPatches = [ ];
  };
  firmware = pkgs.callPackage ../../packaging/nix/firmware.nix { firmware = cfg.firmware; };
  gnome = cfg.desktop == "gnome";
in {
  options.tb323fu.rootfs = {
    partlabel = mkOption {
      type = types.str;
      default = "tb323fu-nixos";
      description = "GPT partition name of the root filesystem.";
    };
    kernel.modules = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        null (default): shared modules -- the boot image's initramfs mounts its
        modules on /lib/modules/<release> and NixOS's kmod falls through to it
        (the generation holds a stub). A path: a copy of /lib/modules/<version>
        of the kernel in the boot image (its name is the version), for a root
        with "own" in /etc/tb323fu/modules or an image without the modules
        squashfs; then every kernel change needs a rebuild.
      '';
    };
    kernel.config = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "That kernel's configuration (zcat /proc/config.gz), for NixOS modules that query boot.kernelPackages.kernel.config.";
    };
    firmware = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "A directory laid out like /lib/firmware with qcom/, ath12k/, qca/, novatek/, aw882xx_acf.bin.";
    };
    desktop = mkOption {
      type = types.enum [ "gnome" "none" ];
      default = "gnome";
      description = "gnome: GNOME with GDM (automatic login for `user`); none: console only.";
    };
    user = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "The desktop user (wheel); logged in automatically with GNOME.";
    };
    initialHashedPassword = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Initial password hash for root and `user` (a development password; the hash
        ends up in the world-readable store). Without it root is locked and the user
        has no password.
      '';
    };
    devAccess = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Developer access: usb0 gadget network 192.168.7.2/24 (gateway 192.168.7.1),
        root autologin on the USB serial console ttyGS0, sshd root login.
      '';
    };
    rootAuthorizedKeys = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = "SSH public keys for root (with devAccess).";
    };
  };

  config = {
    services.tb323fu.enable = true;

    # --- boot: our boot image and initramfs, NixOS stage 2 only
    boot.initrd.enable = false;
    boot.loader.grub.enable = false;
    boot.loader.external = {
      enable = true;
      installHook = pkgs.writeShellScript "tb323fu-install-bootloader" ''
        echo "tb323fu: the boot image's initramfs starts /nix/var/nix/profiles/system/init; nothing to install"
      '';
    };
    boot.kernelPackages = pkgs.linuxPackagesFor kernel;
    hardware.firmware = lib.optional (cfg.firmware != null) firmware;
    fileSystems."/" = {
      device = "/dev/disk/by-partlabel/${cfg.partlabel}";
      fsType = "ext4";
      options = [ "noatime" "x-systemd.growfs" ];   # an image written into a larger partition
    };

    # --- system
    networking.hostName = lib.mkDefault cfg.partlabel;
    time.timeZone = lib.mkDefault "UTC";
    i18n.defaultLocale = "en_US.UTF-8";
    nix.settings.experimental-features = [ "nix-command" "flakes" ];
    networking.networkmanager.enable = true;   # Wi-Fi (wpa_supplicant backend)
    # no modem here (it stays off; see rmtfs below) -- NetworkManager would enable it
    networking.modemmanager.enable = false;
    hardware.bluetooth.enable = true;
    environment.systemPackages = with pkgs; [ vim git usbutils pciutils alsa-utils iw ];
    # rmtfs/tqftpserv are deliberately absent: `rmtfs -s` starts the modem, whose
    # watchdog then resets the SoC (see rootfs/ubuntu/build-rootfs.sh)

    users.users = {
      root = {
        initialHashedPassword = mkIf (cfg.initialHashedPassword != null) cfg.initialHashedPassword;
        openssh.authorizedKeys.keys = mkIf cfg.devAccess cfg.rootAuthorizedKeys;
      };
    } // lib.optionalAttrs (cfg.user != null) {
      ${cfg.user} = {
        isNormalUser = true;
        extraGroups = [ "wheel" "video" "audio" "input" "render" "networkmanager" ];
        initialHashedPassword = cfg.initialHashedPassword;
      };
    };

    # --- desktop
    services.displayManager.gdm.enable = gnome;
    services.desktopManager.gnome.enable = gnome;
    # GNOME Web's address bar does not bring up the on-screen keyboard; Firefox does
    environment.gnome.excludePackages = mkIf gnome [ pkgs.epiphany ];
    programs.firefox.enable = gnome;
    services.displayManager.autoLogin = mkIf (gnome && cfg.user != null) {
      enable = true;
      user = cfg.user;
    };
    # GDM automatic login and a getty on tty1 fight over the VT
    systemd.services."getty@tty1".enable = mkIf (gnome && cfg.user != null) false;
    systemd.services."autovt@tty1".enable = mkIf (gnome && cfg.user != null) false;
    # the TB323FU quick-settings extension, on by default for every user
    programs.dconf.profiles.user.databases = mkIf gnome [{
      settings = {
        "org/gnome/shell".enabled-extensions = [ "tb323fu@joonhoekim.github.io" ];
      } // lib.optionalAttrs cfg.devAccess {
        # GNOME suspends after 15 min idle, and s2idle turns USB and Wi-Fi off:
        # the tablet vanishes from the PC. Never suspend on idle.
        "org/gnome/settings-daemon/plugins/power" = {
          sleep-inactive-ac-type = "nothing";
          sleep-inactive-battery-type = "nothing";
        };
      };
    }];

    # --- developer access
    services.openssh.enable = true;
    services.openssh.settings.PermitRootLogin = if cfg.devAccess then "yes" else "prohibit-password";
    # usb0 (CDC-NCM, set up by the initramfs) keeps its fixed address; NetworkManager
    # leaves it alone and systemd-networkd owns it
    networking.networkmanager.unmanaged = mkIf cfg.devAccess [ "interface-name:usb0" ];
    systemd.network.enable = cfg.devAccess;
    systemd.network.wait-online.enable = false;
    systemd.network.networks."50-usb0" = mkIf cfg.devAccess {
      matchConfig.Name = "usb0";
      address = [ "192.168.7.2/24" ];
      dns = [ "1.1.1.1" ];
      routes = [{ Gateway = "192.168.7.1"; Metric = 1024; }];
      linkConfig.RequiredForOnline = "no";
    };
    # a root shell on the USB serial console (ttyGS0)
    systemd.services."serial-getty@ttyGS0" = mkIf cfg.devAccess {
      # a drop-in on the template's instance (Restart=always, BindsTo=dev-ttyGS0.device
      # stay), not a standalone unit
      overrideStrategy = "asDropin";
      enable = true;
      wantedBy = [ "getty.target" ];
      serviceConfig.ExecStart = [
        ""
        "@${pkgs.util-linux}/sbin/agetty agetty --login-program ${config.services.getty.loginProgram} --autologin root --keep-baud %I 115200,57600,38400,9600 $TERM"
      ];
      serviceConfig.TimeoutStopSec = 5;
    };

    system.stateVersion = "26.11";
  };
}
