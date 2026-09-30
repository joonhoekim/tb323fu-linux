# TB323FU Tablet — GNOME Shell quick settings

A "Tablet" tile in GNOME's quick settings (GNOME 48/49). It is only a front-end for the `tb323fu-helperd` system service
(`helper/`, design in `docs/helper.md`): every action is a call on the system D-Bus (`io.github.joonhoekim.tb323fu.Helper`),
and permissions are decided by the helper's polkit policy. Without the helper the tile shows "helper not running".

| In the menu | Helper object |
|---|---|
| tile on/off = adaptive refresh (lower the rate when idle); subtitle = current Hz | `Refresh.SetPolicy` |
| battery %, state (charging / bypass / …), charger contract; charge limit 60 / 80 / 100 %; bypass charging | `Battery` |
| refresh policy (adaptive / fixed / always 120 Hz) and idle timing presets | `Refresh` |
| torch on/off and level | `Torch` |
| GPU follows the power mode | `Gpu.SetFollowPowerProfiles` |
| wake from USB devices | `Usb.SetWake` |
| Switch to Android… (asks for confirmation, then the tablet restarts) | `Android.SwitchToAndroid` |

Items whose helper object is missing (the helper exports only what the running kernel supports) are hidden.

## Install

Per user:

```sh
cp -r tb323fu@joonhoekim.github.io ~/.local/share/gnome-shell/extensions/
# log out and back in (Wayland), then
gnome-extensions enable tb323fu@joonhoekim.github.io
```

System-wide: copy the directory to `/usr/share/gnome-shell/extensions/` instead.

## License

GPL-2.0-or-later (GNOME Shell extensions run inside and import GNOME Shell, which is GPL); the rest of the repository is MIT.
