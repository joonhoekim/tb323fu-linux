# SPDX-License-Identifier: MIT
# NixOS module: services.tb323fu -- the platform files, helper, settings app and
# GNOME extension of this repository. Built and installed on the device with
# rootfs/nixos (NixOS unstable, aarch64, 2026-10).
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
  toolPath = with pkgs; [ coreutils gnugrep gnused gawk findutils util-linux procps kmod bluez alsa-utils systemd gzip busybox ];
  # the [Install] sections of packaged units are ignored by NixOS: the same targets here
  platformUnits = {
    tb323fu-gen-ids = "sysinit.target";
    tb323fu-dsp = "sysinit.target";
    tb323fu-audio = "sound.target";
    tb323fu-btaddr = "bluetooth.target";
    tb323fu-usb-port = "multi-user.target";
    tb323fu-emergency-key = "multi-user.target";
    tb323fu-kernel-confirm = "multi-user.target";
  };
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
      description = "Install the GNOME Shell quick-settings extension (enable it per user) and keep GNOME on integer scales.";
    };
    sensors.dataDir = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      example = lib.literalExpression "./sensors/kaanapali/Lenovo/TB323FU";
      description = ''
        The sensor hub's files for hexagonrpcd (dsp/, sensors/, socinfo/ -- built
        from the device's own dsp and persist partitions, never distributed). When
        set, hexagonrpcd serves them on the ADSP's secure fastrpc node and
        iio-sensor-proxy (libssc backend) provides rotation and light.
      '';
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
      # udev RUN+= helpers (usb-port, audio-defaults) get udev's PATH
      services.udev.path = toolPath ++ [ platform ];
      systemd.packages = [ platform ];
      environment.systemPackages = [ platform pkgs.ladspaPlugins ];
      # the firmware leaves the SoC watchdog running; the initramfs feeds it until
      # the hand-over, from then on systemd does (tb323fu-watchdog.conf elsewhere)
      systemd.settings.Manager.RuntimeWatchdogSec = lib.mkDefault "30s";
      # The platform units name /run/current-system/firmware (platform.nix
      # rewrites /lib/firmware; NixOS links hardware.firmware there itself).
      # Uncompressed: tb323fu-dsp tests for adsp.mbn, remoteproc loads .bNN parts.
      hardware.firmwareCompression = lib.mkDefault "none";
      # the speaker protection filter-chain uses the swh LADSPA compressor (sc4);
      # without it the mandatory filter-chain module fails and PipeWire does not start
      # (the pipewire module puts it on the services' LADSPA_PATH)
      services.pipewire.extraLadspaPackages = [ pkgs.ladspaPlugins ];
      hardware.wirelessRegulatoryDatabase = true;
      systemd.services = lib.mapAttrs (_: target: {
        wantedBy = [ target ];
        path = toolPath ++ [ platform ];
      }) platformUnits;
      systemd.user.services.tb323fu-speaker-gain = {
        wantedBy = [ "pipewire.service" ];
        path = toolPath ++ [ pkgs.pipewire ];   # pw-cli, pw-metadata
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
    (lib.mkIf (cfg.sensors.dataDir != null) {
      # hexagonrpcd (nixpkgs: hexagonrpc) answers the sensor hub's file requests;
      # the device's own files, on the ADSP's only (secure) fastrpc node
      users.users.fastrpc = { isSystemUser = true; group = "fastrpc"; description = "FastRPC (Hexagon DSP sensors)"; };
      users.groups.fastrpc = { };
      services.udev.extraRules = ''
        SUBSYSTEM=="misc", KERNEL=="fastrpc-*", OWNER="fastrpc", GROUP="fastrpc", MODE="0660"
      '';
      systemd.services.hexagonrpcd = {
        description = "Hexagon DSP sensors daemon";
        after = [ "tb323fu-dsp.service" ];
        wants = [ "tb323fu-dsp.service" ];
        wantedBy = [ "multi-user.target" ];
        serviceConfig = {
          ExecStart = "${pkgs.hexagonrpc}/bin/hexagonrpcd -f /dev/fastrpc-adsp-secure -d adsp -s -R ${cfg.sensors.dataDir}";
          User = "fastrpc";
          Group = "fastrpc";
          Restart = "on-failure";
          RestartSec = 5;
        };
      };
      # iio-sensor-proxy with its libssc backend (81-tb323fu-sensors.rules starts it)
      hardware.sensor.iio.enable = true;
      systemd.services.iio-sensor-proxy.serviceConfig.TimeoutStopSec = 3;
    })
    (lib.mkIf cfg.helper.enable {
      environment.systemPackages = [ helper ];
      systemd.packages = [ helper ];
      services.dbus.packages = [ helper ];
      security.polkit.enable = true;
      # kernel updates: the daemon starts this download unit (DynamicUser, network)
      systemd.services.tb323fu-kernel-fetch.path = [ pkgs.curl pkgs.coreutils ];
      systemd.services.tb323fu-helperd = {
        wantedBy = [ "multi-user.target" ];
        path = toolPath ++ [ platform ];
      };
      systemd.tmpfiles.rules = [ "C /etc/tb323fu/helper.toml - - - - ${initialHelperToml}" ];
    })
    (lib.mkIf cfg.settingsApp.enable { environment.systemPackages = [ settings ]; })
    (lib.mkIf cfg.gnome.enable {
      environment.systemPackages = [ extension ];
      # userspace/desktop/gnome/90_tb323fu-integer-scale.gschema.override
      services.desktopManager.gnome.extraGSettingsOverrides = ''
        [org.gnome.mutter]
        experimental-features=['xwayland-native-scaling']
      '';
    })
  ]);
}
