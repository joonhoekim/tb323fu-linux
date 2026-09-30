# GNOME (optional)

| File | What |
|---|---|
| `90_tb323fu-integer-scale.gschema.override` | keeps GNOME on integer UI scales; with fractional scaling on, mutter picks 2.2857 on this panel and then refuses every display change |

Install: copy to `/usr/share/glib-2.0/schemas/` and run `glib-compile-schemas /usr/share/glib-2.0/schemas`.
The quick-settings toggles (torch, Android switch, charge limit, refresh rate) come from the tb323fu helper's GNOME extension.
