# SPDX-License-Identifier: MIT
# NixOS module: services.tb323fu. Untested (written without a Nix system).
# Usage in a flake: modules = [ tb323fu-linux.nixosModules.default { services.tb323fu.enable = true; } ];
self:
{ config, lib, pkgs, ... }:
let
  cfg = config.services.tb323fu;
  sys = pkgs.stdenv.hostPlatform.system;
  pk = self.packages.${sys};
  platform = pk.tb323fu-platform;
  helper = pk.tb323fu-helper;
  settings = pk.tb323fu-settings;
  extension = pk.tb323fu-helper-gnome;
  # tools the platform scripts and the helper call through PATH
  toolPath = with pkgs; [ coreutils gnugrep gnused gawk findutils util-linux procps kmod bluez alsa-utils systemd busybox ];
  platformUnits = [ "tb323fu-gen-ids" "tb323fu-btaddr" "tb323fu-dsp" "tb323fu-audio" "tb323fu-usb-port" "tb323fu-emergency-key" ];
  ucm2 = pkgs.symlinkJoin {
    name = "alsa-ucm2-tb323fu";
    paths = [ "${pkgs.alsa-ucm-conf}/share/alsa/ucm2" "${platform}/share/alsa/ucm2" ];
  };
  initialHelperToml = pkgs.writeText "helper.toml" ''
    [android]
    require_auth = ${lib.boolToString cfg.android.requireAuth}
  '';
in {
  options.services.tb323fu = {
    enable = lib.mkEnableOption "Lenovo Legion Tab Gen 5 (TB323FU) platform files and helper";
    helper.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Run tb323fu-helperd (charge limit, refresh, torch, LED ring, GPU, ...) and install tb323fu-ctl.";
    };
    settingsApp.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Install the GTK4/libadwaita settings app.";
    };
    gnome.enable = lib.mkOption {
      type = lib.types.bool;
      default = config.services.desktopManager.gnome.enable or false;
      defaultText = lib.literalExpression "config.services.desktopManager.gnome.enable";
      description = "Install the GNOME Shell quick-settings extension (enable it per user).";
    };
    android.requireAuth = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Initial value of android.require_auth in /etc/tb323fu/helper.toml
        (written only when the file does not exist; the helper owns the file).
      '';
    };
    androidBootSha256 = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "SHA-256 of the Android boot image kept in boot_b; arms the emergency key (volume up+down 10 s).";
    };
  };

  config = lib.mkIf cfg.enable (lib.mkMerge [
    {
      # layer 1: udev rules, systemd units, audio, sensors, emergency key
      services.udev.packages = [ platform ];
      systemd.packages = [ platform ];
      environment.systemPackages = [ platform pkgs.ladspaPlugins ];
      # the speaker protection filter-chain uses the swh LADSPA compressor (sc4);
      # without it the mandatory filter-chain module fails and PipeWire does not start
      systemd.user.services.pipewire.environment.LADSPA_PATH = "${pkgs.ladspaPlugins}/lib/ladspa";
      hardware.wirelessRegulatoryDatabase = true;
      systemd.services = lib.genAttrs platformUnits (_: {
        wantedBy = [ "multi-user.target" ];
        path = toolPath ++ [ platform ];
      });
      systemd.user.services.tb323fu-speaker-gain = {
        wantedBy = [ "pipewire.service" ];
        path = toolPath;
      };
      # configuration is mutable (the helper writes it): copy the defaults once
      systemd.tmpfiles.rules = [
        "d /etc/tb323fu 0755 root root -"
        "C /etc/tb323fu/audio.conf - - - - ${platform}/share/tb323fu/etc/tb323fu/audio.conf"
        "C /etc/tb323fu/emergency-key.conf - - - - ${platform}/share/tb323fu/etc/tb323fu/emergency-key.conf"
      ] ++ lib.optional (cfg.androidBootSha256 != null)
        "f /etc/tb323fu/android-boot.sha256 0644 root root - ${cfg.androidBootSha256}";
      # audio: UCM next to alsa-ucm-conf, PipeWire/WirePlumber speaker protection
      environment.variables.ALSA_CONFIG_UCM2 = "${ucm2}";
      systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2 = "${ucm2}";
      systemd.user.services.wireplumber.environment.ALSA_CONFIG_UCM2 = "${ucm2}";
      services.pipewire.configPackages = [ platform ];
      services.pipewire.wireplumber.configPackages = [ platform ];
      # camera tuning
      environment.variables.LIBCAMERA_IPA_CONFIG_PATH = "${platform}/share/libcamera/ipa";
    }
    (lib.mkIf cfg.helper.enable {
      environment.systemPackages = [ helper ];
      systemd.packages = [ helper ];
      services.dbus.packages = [ helper ];
      security.polkit.enable = true;
      systemd.services.tb323fu-helperd = {
        wantedBy = [ "multi-user.target" ];
        path = toolPath ++ [ platform ];
      };
      systemd.tmpfiles.rules = [ "C /etc/tb323fu/helper.toml - - - - ${initialHelperToml}" ];
    })
    (lib.mkIf cfg.settingsApp.enable { environment.systemPackages = [ settings ]; })
    (lib.mkIf cfg.gnome.enable { environment.systemPackages = [ extension ]; })
  ]);
}
