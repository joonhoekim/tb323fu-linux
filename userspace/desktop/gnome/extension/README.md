# TB323FU Tablet — GNOME Shell quick settings

A "Tablet" tile in GNOME's quick settings (GNOME 48/49). It is only a front-end for the `tb323fu-helperd` system service
(`helper/`, design in `docs/helper.md`): every action is a call on the system D-Bus (`io.github.joonhoekim.OpenDeviceHelper1`),
and permissions are decided by the helper's polkit policy. Without the helper the tile shows "helper not running".

**The tile:** pressing it opens Open Device Helper (and closes the panel); the arrow opens the menu below. The subtitle shows
the charge and the live refresh rate (`80% · 60 Hz`). The tile has no on/off state of its own.

| In the menu | Helper object |
|---|---|
| header: battery %, state (charging / bypass / …), charger contract; charge limit 60 / 80 / 100 %; bypass charging | `Battery` |
| refresh policy (adaptive / fixed / always 120 Hz) | `Refresh.SetPolicy` |
| torch on/off and level | `Torch` |
| Open Device Helper… — opens the settings app (`io.github.joonhoekim.OpenDeviceHelper.desktop`) | — |

The menu is kept short so it fits a landscape screen; idle timing presets, GPU-follow, USB wake and the Android switch are in
the settings app. As a fallback the items sit in a scroll view capped to about half the work area height, so nothing can be
pushed off the screen. Choices (charge limit, policy, switches) keep the menu open.

**LED ring notification pulse:** when "Pulse for Notifications" is on in the settings app (`LedRing.NotifyPulse`), a new desktop
notification blinks the ring twice in its colour (`LedRing.Pulse`), at most every 2 s, and not while notification banners
are off (do not disturb).

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
