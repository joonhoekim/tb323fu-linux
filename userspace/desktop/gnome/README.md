# GNOME (optional)

| File | What |
|---|---|
| `90_tb323fu-integer-scale.gschema.override` | keeps GNOME on integer UI scales; with fractional scaling on, mutter picks 2.2857 on this panel and then refuses every display change |

Install: copy to `/usr/share/glib-2.0/schemas/` and run `glib-compile-schemas /usr/share/glib-2.0/schemas`.
| `extension/` | the "Tablet" quick-settings tile — a front-end for the tb323fu helper (charge limit and bypass, refresh policy, torch, GPU, USB wake, switch to Android); see [extension/README.md](extension/README.md) |
