# GNOME (optional)

| File | What |
|---|---|
| `90_tb323fu-integer-scale.gschema.override` | keeps GNOME on integer UI scales; with fractional scaling on, mutter picks 2.2857 on this panel and then refuses every display change |
| `extension/` | the "Device" quick-settings tile — a front-end for the tb323fu helper (charge limit and bypass, refresh policy, torch, GPU, USB wake, switch to Android); see [extension/README.md](extension/README.md) |
| `autorotate/` | turns the panel with the tablet while a keyboard or mouse is connected (GNOME rotates by itself only when none is); the GNOME orientation lock still applies. Off unless installed |

Install the schema override: copy it to `/usr/share/glib-2.0/schemas/` and run `glib-compile-schemas /usr/share/glib-2.0/schemas`.

## Automatic login

With GDM automatic login, plymouth is never told to quit and boot waits ~21 s in `plymouth-quit-wait`.
Install [`gdm/PostLogin-Default`](gdm/PostLogin-Default) as `/etc/gdm3/PostLogin/Default` (Debian/Ubuntu; `/etc/gdm/PostLogin/Default` on Arch/NixOS) — measured on the device: userspace boot 23 s → 2.6 s.

## Rotation with a keyboard or mouse connected

GNOME rotates the screen only in touch mode, which ends while a keyboard or mouse is connected (a docked tablet is
used like a laptop). [`autorotate/`](autorotate/) rotates the built-in panel with the accelerometer anyway, through
GNOME's own display settings, so touch and pen follow. It needs iio-sensor-proxy with a working accelerometer
(`tb323fu-check`, line `sensors`) and `python3-gi`.

```sh
sudo install -m 755 autorotate/tb323fu-autorotate /usr/local/bin/
sudo install -m 644 autorotate/tb323fu-autorotate.service /etc/systemd/user/
systemctl --user enable --now tb323fu-autorotate
```

Undo: `systemctl --user disable --now tb323fu-autorotate`. Idea: vitalo233 ([#17](https://github.com/joonhoekim/tb323fu-linux/issues/17)).
