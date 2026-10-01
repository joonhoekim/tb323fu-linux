// SPDX-License-Identifier: GPL-2.0-or-later
// TB323FU quick settings: a thin front-end for the tb323fu-helperd system
// service (docs/helper.md). Everything goes through the system D-Bus; the
// extension never touches sysfs or runs commands.

import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as MessageTray from 'resource:///org/gnome/shell/ui/messageTray.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';
import {QuickMenuToggle, SystemIndicator} from 'resource:///org/gnome/shell/ui/quickSettings.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS = 'io.github.joonhoekim.tb323fu.Helper';
const ROOT = '/io/github/joonhoekim/tb323fu/Helper';
const OBJ = name => ({path: `${ROOT}/${name}`, iface: `${BUS}.${name}`});
const SETTINGS_DESKTOP_ID = 'io.github.joonhoekim.tb323fu.Settings.desktop';

// Labels for the daemon's ids (the settings app uses the same words).
const STATE_LABELS = {
    charging: 'Charging', discharging: 'Discharging', full: 'Full',
    'not-charging': 'Not Charging', bypass: 'Held at Limit',
};
// What a failed call was about, for the notification title.
const METHOD_TITLES = {
    SetChargeLimit: 'Charge limit', SetBypass: 'Bypass charging', SetPolicy: 'Refresh rate',
    Set: 'Torch', SetLevel: 'Torch brightness',
};

// One notification source for the extension, reused (created on demand).
let notifySource = null;
function notify(title, body) {
    if (!notifySource) {
        notifySource = new MessageTray.Source({title: 'Tablet', iconName: 'computer-symbolic'});
        notifySource.connect('destroy', () => {
            notifySource = null;
        });
        Main.messageTray.add(notifySource);
    }
    const n = new MessageTray.Notification({source: notifySource, title, body});
    notifySource.addNotification(n);
}

// ---- small D-Bus layer ---------------------------------------------------

// Keep the quick-settings menu open when a choice is made: PopupBaseMenuItem's
// activate() emits 'activate', and the menu closes itself via itemActivated.
// These items run their action and update their check mark in place instead.
function stayItem(label, group, key, action) {
    const it = new PopupMenu.PopupMenuItem(label);
    it.activate = () => {
        for (const [k, other] of Object.entries(group))
            other.setOrnament(k === String(key) ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
        action();
    };
    return it;
}

function staySwitch(label, action) {
    const it = new PopupMenu.PopupSwitchMenuItem(label, false);
    // toggle() flips the switch and emits 'toggled'; no super.activate(), so no close
    it.activate = () => it.toggle();
    it.connect('toggled', (_i, on) => action(on));
    return it;
}

class Helper {
    constructor(onChange) {
        this._onChange = onChange;
        this._subs = [];
        this.present = false;
        this.features = [];
        this.props = {};       // name -> {prop: value}
        this._watch = Gio.bus_watch_name(Gio.BusType.SYSTEM, BUS, Gio.BusNameWatcherFlags.NONE,
            () => { this.present = true; this.refreshAll(); },
            () => { this.present = false; this.features = []; this.props = {}; this._onChange(); });
        // PropertiesChanged carries only invalidated names: re-read that object
        this._subs.push(Gio.DBus.system.signal_subscribe(BUS, 'org.freedesktop.DBus.Properties',
            'PropertiesChanged', null, null, Gio.DBusSignalFlags.NONE,
            (_c, _s, path, _i, _sig, params) => {
                const iface = params.deepUnpack()[0];
                const name = iface.startsWith(`${BUS}.`) ? iface.slice(BUS.length + 1) : null;
                if (name && path === `${ROOT}/${name}`)
                    this.refresh(name);
            }));
    }

    destroy() {
        Gio.bus_unwatch_name(this._watch);
        for (const id of this._subs)
            Gio.DBus.system.signal_unsubscribe(id);
        this._subs = [];
    }

    has(name) {
        return this.present && this.features.includes(name);
    }

    _getAll(path, iface) {
        return new Promise((resolve, reject) => {
            Gio.DBus.system.call(BUS, path, 'org.freedesktop.DBus.Properties', 'GetAll',
                new GLib.Variant('(s)', [iface]), GLib.VariantType.new('(a{sv})'),
                Gio.DBusCallFlags.NONE, 3000, null, (conn, res) => {
                    try {
                        resolve(conn.call_finish(res).recursiveUnpack()[0]);
                    } catch (e) {
                        reject(e);
                    }
                });
        });
    }

    async refreshAll() {
        try {
            const root = await this._getAll(ROOT, BUS);
            this.features = root.Features ?? [];
            this.props.Helper = root;
            await Promise.all(this.features.map(n => this.refresh(n, false)));
        } catch (e) {
            this.features = [];
        }
        this._onChange();
    }

    async refresh(name, notify = true) {
        const o = OBJ(name);
        try {
            this.props[name] = await this._getAll(o.path, o.iface);
        } catch (e) {
            // object gone or daemon restarting; keep the last values
        }
        if (notify)
            this._onChange();
    }

    call(name, method, sig = null, args = []) {
        const o = OBJ(name);
        const params = sig ? new GLib.Variant(`(${sig})`, args) : null;
        Gio.DBus.system.call(BUS, o.path, o.iface, method, params, null,
            Gio.DBusCallFlags.ALLOW_INTERACTIVE_AUTHORIZATION, 30000, null, (conn, res) => {
                try {
                    conn.call_finish(res);
                } catch (e) {
                    // a cancelled or refused authentication is the user's choice: no banner
                    const remote = Gio.DBusError.get_remote_error(e) ?? '';
                    if (!remote.endsWith('.AccessDenied')) {
                        Gio.DBusError.strip_remote_error(e);
                        const msg = e.message ? e.message[0].toUpperCase() + e.message.slice(1) : 'Something went wrong';
                        notify(METHOD_TITLES[method] ?? 'Tablet', msg);
                    }
                }
                this.refresh(name);
            });
    }
}

// ---- the quick-settings tile ---------------------------------------------

function launchSettings() {
    const app = Gio.DesktopAppInfo.new(SETTINGS_DESKTOP_ID);
    if (app)
        app.launch([], global.create_app_launch_context(0, -1));
    else
        notify('Tablet Settings', 'The settings app is not installed');
}

const TabletToggle = GObject.registerClass(
class TabletToggle extends QuickMenuToggle {
    _init() {
        super._init({title: 'Tablet', iconName: 'computer-symbolic', toggleMode: false});
        this.menu.setHeader('computer-symbolic', 'Tablet');
        this._helper = new Helper(() => this._sync());

        // the tile opens the settings app; the arrow opens the detailed menu
        // (also without the helper: it then says so and offers the app).
        this.connect('clicked', () => {
            Main.panel.closeQuickSettings?.();
            launchSettings();
        });
        this.menu.connect('open-state-changed', (_m, open) => {
            if (open)
                this._helper.refreshAll();
        });

        this._absent = new PopupMenu.PopupMenuItem('The helper service is not running', {reactive: false});
        this.menu.addMenuItem(this._absent);

        // Everything else sits in one section that is re-parented into a
        // scroll view (below), so a landscape screen can never cut items off.
        this._inner = new PopupMenu.PopupMenuSection();

        // battery (charge and state are in the menu header)
        this._batSection = new PopupMenu.PopupMenuSection();
        this._limitLabel = new PopupMenu.PopupSeparatorMenuItem('Charge Limit');
        this._batSection.addMenuItem(this._limitLabel);
        this._limitItems = {};
        for (const p of [60, 80, 100]) {
            const it = stayItem(`${p}%`, this._limitItems, p,
                () => this._helper.call('Battery', 'SetChargeLimit', 'u', [p]));
            this._limitItems[p] = it;
            this._batSection.addMenuItem(it);
        }
        this._bypass = staySwitch('Bypass Charging',
            on => this._helper.call('Battery', 'SetBypass', 'b', [on]));
        this._batSection.addMenuItem(this._bypass);
        this._inner.addMenuItem(this._batSection);

        // refresh policy (idle timing presets live in the app)
        this._refSection = new PopupMenu.PopupMenuSection();
        this._refSection.addMenuItem(new PopupMenu.PopupSeparatorMenuItem('Refresh Rate'));
        this._policyItems = {};
        for (const [pol, label] of [['auto', 'Adaptive'], ['manual', 'Fixed'], ['off', 'Always 120 Hz']]) {
            const it = stayItem(label, this._policyItems, pol,
                () => this._helper.call('Refresh', 'SetPolicy', 's', [pol]));
            this._policyItems[pol] = it;
            this._refSection.addMenuItem(it);
        }
        this._inner.addMenuItem(this._refSection);

        // torch
        this._torchSection = new PopupMenu.PopupMenuSection();
        this._torchSection.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._torch = staySwitch('Torch', on => this._helper.call('Torch', 'Set', 'b', [on]));
        this._torchSection.addMenuItem(this._torch);
        const sliderItem = new PopupMenu.PopupBaseMenuItem({activate: false});
        this._torchSlider = new Slider(0);
        this._torchSlider.x_expand = true;
        this._sliderSync = false;
        this._torchSlider.connect('drag-end', () => this._sendLevel());
        this._torchSlider.connect('scroll-event', () => GLib.idle_add(GLib.PRIORITY_DEFAULT, () => { this._sendLevel(); return GLib.SOURCE_REMOVE; }));
        this._torchSlider.connect('key-press-event', () => GLib.idle_add(GLib.PRIORITY_DEFAULT, () => { this._sendLevel(); return GLib.SOURCE_REMOVE; }));
        sliderItem.add_child(this._torchSlider);
        this._torchSection.addMenuItem(sliderItem);
        this._inner.addMenuItem(this._torchSection);

        // GPU, USB wake, idle timing, Android switch: in the settings app
        this._inner.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._settings = new PopupMenu.PopupMenuItem('Tablet Settings…');
        this._settings.connect('activate', () => launchSettings());
        this._inner.addMenuItem(this._settings);

        this.menu.addMenuItem(this._inner);
        this._scroll = new St.ScrollView({
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
        });
        this.menu.box.remove_child(this._inner.actor);
        this._scroll.child = this._inner.actor;
        this.menu.box.add_child(this._scroll);
        this.menu.connect('open-state-changed', (_m, open) => {
            if (open)
                this._fitHeight();
        });

        this._sync();
    }

    // Cap the item area to part of the work area so the menu scrolls instead of
    // running off a landscape screen (the tile grid above takes the rest).
    _fitHeight() {
        const wa = Main.layoutManager.getWorkAreaForMonitor(Main.layoutManager.primaryIndex);
        const {scaleFactor} = St.ThemeContext.get_for_stage(global.stage);
        const px = Math.max(160, Math.floor(wa.height / scaleFactor * 0.45));
        this._scroll.style = `max-height: ${px}px;`;
    }

    _sendLevel() {
        if (this._sliderSync || !this._helper.has('Torch'))
            return;
        const max = this._helper.props.Torch?.MaxLevel ?? 1;
        const level = Math.max(1, Math.round(this._torchSlider.value * max));
        this._helper.call('Torch', 'SetLevel', 'u', [level]);
    }

    _sync() {
        const h = this._helper;
        const present = h.present && h.features.length > 0;
        this._absent.visible = !present;
        // the tile stays usable: it opens the app, the arrow says what is wrong
        this.reactive = true;
        this.checked = false;

        const bat = h.props.Battery;
        this._batSection.actor.visible = h.has('Battery');
        if (bat && present) {
            const plugged = bat.ChargerContract && !['none', 'unknown'].includes(bat.ChargerContract);
            const state = bat.State === 'bypass' && bat.Bypass ? 'Bypass' : STATE_LABELS[bat.State] ?? 'Unknown';
            const level = Math.min(100, Math.max(0, Math.round(bat.Capacity / 10) * 10));
            // Adwaita has no battery-level-100-charging icon, only -charged
            const charging = bat.State === 'charging' ? (level === 100 ? '-charged' : '-charging') : '';
            this.menu.setHeader(`battery-level-${level}${charging}-symbolic`, 'Tablet',
                `${bat.Capacity}% · ${state}${plugged ? ` · ${bat.ChargerContract}` : ''}`);
            for (const [p, it] of Object.entries(this._limitItems))
                it.setOrnament(Number(p) === bat.ChargeLimit && !bat.Bypass ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
            // a limit set in the app (75 %) checks no preset: show it in the label
            const preset = bat.ChargeLimit in this._limitItems;
            this._limitLabel.label.text = preset || bat.Bypass ? 'Charge Limit' : `Charge Limit · ${bat.ChargeLimit}%`;
            this._bypass.setToggleState(!!bat.Bypass);
        } else {
            this.menu.setHeader('computer-symbolic', 'Tablet', present ? null : 'Helper not running');
        }

        const ref = h.props.Refresh;
        this._refSection.actor.visible = h.has('Refresh');
        if (ref && present) {
            this._policyItems.manual.label.text = `Fixed (${ref.Rate} Hz)`;
            for (const [pol, it] of Object.entries(this._policyItems))
                it.setOrnament(pol === ref.Policy ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
        }
        if (!present)
            this.subtitle = 'Helper not running';
        else if (bat && ref)
            this.subtitle = `${bat.Capacity}% · ${ref.LiveRate} Hz`;
        else if (bat)
            this.subtitle = `${bat.Capacity}%`;
        else
            this.subtitle = ref ? `${ref.LiveRate} Hz` : null;

        const t = h.props.Torch;
        this._torchSection.actor.visible = h.has('Torch');
        if (t) {
            this._torch.setToggleState(!!t.On);
            this._sliderSync = true;
            this._torchSlider.value = t.MaxLevel ? t.Level / t.MaxLevel : 0;
            this._sliderSync = false;
        }
    }

    destroy() {
        this._helper.destroy();
        super.destroy();
    }
});

const TabletIndicator = GObject.registerClass(
class TabletIndicator extends SystemIndicator {
    _init() {
        super._init();
        this.quickSettingsItems.push(new TabletToggle());
    }

    destroy() {
        this.quickSettingsItems.forEach(i => i.destroy());
        super.destroy();
    }
});

export default class TabletExtension extends Extension {
    enable() {
        this._indicator = new TabletIndicator();
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicator);
    }

    disable() {
        this._indicator?.destroy();
        this._indicator = null;
        notifySource?.destroy();
        notifySource = null;
    }
}
