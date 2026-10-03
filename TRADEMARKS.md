# Name and trademark policy

The licenses (see [NOTICE](NOTICE)) cover the code and the documentation. They do not cover the project's name,
its icons, its signing keys or its update channel. This page says how those may be used. Its purpose is to keep
users from mistaking someone else's build for the official one; it does not restrict what the licenses allow you
to do with the code.

"The project name" below means `tb323fu-linux` and the names of its components as published here
(Open Device Helper, the `tb323fu-*` packages). "The icons" means the site icon and the
Open Device Helper icon.

## What you may do

1. **Redistribute unmodified official releases under the project name**, for free or for a fee,
   as long as you say where they come from and do not suggest that you are the project.
2. **Say that your work is based on this project**: "based on tb323fu-linux", "uses the tb323fu-linux kernel
   patches" and similar factual statements are fine.
3. **Link to the project and write about it**, including reviews and tutorials.

## What needs a different name

4. **Modified builds, builds with different defaults, and forks must use their own name and icons.** Do not call
   them "tb323fu-linux", "official", "tb323fu-linux Pro" or anything that reads as the official project.
5. **Forks must change the identifiers that point at this project**: the application and D-Bus IDs
   (`io.github.joonhoekim.*`), the GNOME Shell extension UUID (`tb323fu@joonhoekim.github.io`), the kernel update
   signing key and the update URL. The official signing keys and the official update URL are for official
   releases only; a fork that ships its own kernels signs them with its own key and serves them from its own URL.
6. **Images that bundle firmware blobs may not use the project name at all**, modified or not. The Lenovo and
   Qualcomm firmware this device needs may not be redistributed; this project never ships it and does not want
   its name on anything that does (see [firmware/README.md](firmware/README.md)).

## Other companies' names

7. This project is **not affiliated with, endorsed by or sponsored by Lenovo, Qualcomm or Valve.** "Lenovo",
   "Legion", "Snapdragon", "Qualcomm", "Steam" and "SteamOS" are trademarks of their owners, and "TB323FU" is
   Lenovo's model number. The project uses these names only to identify the device and the software it works
   with, and so should anyone using this project's work. Do not use their logos with it.

If you are unsure whether a use is fine, open an issue and ask.
