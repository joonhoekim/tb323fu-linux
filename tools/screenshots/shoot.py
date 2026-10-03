#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""shoot.py -- drive tb323fu-settings through AT-SPI and take screenshots with
GNOME Shell's Screenshot API. Runs inside the session shoot.sh starts (it is
not useful on its own).

  shoot.py --app PATH --out DIR [--scheme light|dark|both] [--only NAME,...]

Pictures: <scheme>/<page>.png for every page, and <scheme>/kernel-file-*.png
for "Install Kernel from File" (the dialog is cancelled, nothing is installed).
"""
import argparse
import os
import subprocess
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib  # noqa: E402

PAGES = ["battery", "display", "performance", "lights", "usb", "emergency-key", "systems", "android", "diagnostics", "about"]
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
USER_BUS = f"/run/user/{os.getuid()}/bus"
UNIT = "tb323fu-shots-app"


def user_env():
    """The environment that reaches the user's own systemd (not this session)."""
    return {**os.environ, "XDG_RUNTIME_DIR": os.path.dirname(USER_BUS), "DBUS_SESSION_BUS_ADDRESS": f"unix:path={USER_BUS}"}


def call(dest, path, iface, method, args):
    return bus.call_sync(dest, path, iface, method, args, None, 0, -1, None)


def own_screenshot_name():
    # The Screenshot API answers only senders owning one of a few names (GNOME
    # Screenshot, the portals); this private session has neither.
    bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName",
                  GLib.Variant("(su)", ("org.gnome.Screenshot", 4)), None, 0, -1, None)


def shot(path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if overview_active():
        set_overview(False)
        settle(1.0)
    ok, _ = call("org.gnome.Shell.Screenshot", "/org/gnome/Shell/Screenshot", "org.gnome.Shell.Screenshot", "Screenshot",
                 GLib.Variant("(bbs)", (False, False, path))).unpack()
    if not ok:
        sys.exit(f"shoot.py: screenshot {path} failed")
    print("  ", path)


def set_overview(active):
    call("org.gnome.Shell", "/org/gnome/Shell", "org.freedesktop.DBus.Properties", "Set",
         GLib.Variant("(ssv)", ("org.gnome.Shell", "OverviewActive", GLib.Variant("b", active))))


def overview_active():
    return call("org.gnome.Shell", "/org/gnome/Shell", "org.freedesktop.DBus.Properties", "Get",
                GLib.Variant("(ss)", ("org.gnome.Shell", "OverviewActive"))).unpack()[0]


def scheme(name):
    subprocess.run(["gsettings", "set", "org.gnome.desktop.interface", "color-scheme",
                    "prefer-dark" if name == "dark" else "default"], check=True)


def walk(o, depth=0):
    if o is None or depth > 80:
        return
    yield o
    try:
        n = o.get_child_count()
    except GLib.Error:
        return
    for i in range(n):
        yield from walk(o.get_child_at_index(i), depth + 1)


def app_named(name):
    d = Atspi.get_desktop(0)
    for i in range(d.get_child_count()):
        a = d.get_child_at_index(i)
        if a is not None and a.get_name() == name:
            return a
    return None


def find(root, role=None, name=None, timeout=10):
    end = time.time() + timeout
    while time.time() < end:
        r = root() if callable(root) else root
        for o in walk(r):
            try:
                if (role is None or o.get_role_name() == role) and (name is None or o.get_name() == name):
                    return o
            except GLib.Error:
                continue
        time.sleep(0.3)
    r = root() if callable(root) else root
    if r is None:
        d = Atspi.get_desktop(0)
        apps = [d.get_child_at_index(i).get_name() for i in range(d.get_child_count())]
        sys.exit(f"shoot.py: no {role or 'object'} {name!r}: the application is not there; there are: {apps}")
    seen = sorted({f"{o.get_role_name()}:{o.get_name()!r}" for o in walk(r) if role is None or o.get_role_name() == role})
    sys.exit(f"shoot.py: no {role or 'object'} {name!r}; there are: {', '.join(seen)}")


def click(o):
    o.do_action(0)


def select(o):
    """Select o in its list or table (the Selection interface of a parent)."""
    child, p = o, o.get_parent()
    while p is not None:
        sel = p.get_selection_iface() if hasattr(p, "get_selection_iface") else None
        if sel is not None:
            sel.select_child(child.get_index_in_parent())
            return
        child, p = p, p.get_parent()
    sys.exit(f"shoot.py: nothing selects {o.get_name()!r}")


def settle(s=1.2):
    time.sleep(s)


class App:
    def __init__(self, path):
        self.path = path
        self.proc = None

    def open(self, page):
        self.close()
        cmd = [self.path, "--page", page, "--maximized"]
        if os.path.exists(USER_BUS):
            # Under the user's systemd, polkit counts the app as part of the
            # user's active session (allow_active), like the app on the panel;
            # from this private session it would be refused.
            env = [f"--setenv={k}={os.environ[k]}" for k in ("WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "HOME", "LANG")]
            subprocess.run(["systemd-run", "--user", "--quiet", "--collect", f"--unit={UNIT}", *env, *cmd], env=user_env(), check=True)
            self.proc = UNIT
        else:
            self.proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        find(lambda: app_named("tb323fu-settings"), role="frame", timeout=15)
        settle(2.0)

    def root(self):
        return app_named("tb323fu-settings")

    def close(self):
        if self.proc == UNIT:
            subprocess.run(["systemctl", "--user", "stop", UNIT], env=user_env())
        elif self.proc:
            self.proc.terminate()
            self.proc.wait()
        if self.proc:
            self.proc = None
            settle(0.5)


def kernel_file(app, out, kernel_name):
    app.open("about")
    btn = find(app.root, role="button", name="Install Kernel from File")
    settle()
    shot(f"{out}/kernel-file-1-about.png")
    click(btn)
    settle(3.0)
    # the file chooser is Nautilus (GNOME portal) or the GTK portal's
    portal = lambda: app_named("nautilus") or app_named("org.gnome.Nautilus") or app_named("xdg-desktop-portal-gtk")
    f = find(portal, name=kernel_name, timeout=15)
    shot(f"{out}/kernel-file-2-chooser.png")
    select(f)
    settle()
    click(find(portal, role="button", name="Open"))
    find(app.root, name="Install", timeout=20)
    settle(1.5)
    shot(f"{out}/kernel-file-3-confirm.png")
    click(find(app.root, role="button", name="Cancel"))
    settle()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--app", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--scheme", default="light", choices=["light", "dark", "both"])
    p.add_argument("--kernel-name", default="", help="the file in Recent to pick")
    p.add_argument("--only", default="", help="comma-separated pages, or kernel-file")
    a = p.parse_args()
    only = [x for x in a.only.split(",") if x]
    own_screenshot_name()
    set_overview(False)
    app = App(a.app)
    for sch in (["light", "dark"] if a.scheme == "both" else [a.scheme]):
        scheme(sch)
        out = os.path.join(a.out, sch)
        print(sch)
        for page in PAGES:
            if not only or page in only:
                app.open(page)
                shot(f"{out}/{page}.png")
        if a.kernel_name and (not only or "kernel-file" in only):
            kernel_file(app, out, a.kernel_name)
    app.close()


if __name__ == "__main__":
    main()
