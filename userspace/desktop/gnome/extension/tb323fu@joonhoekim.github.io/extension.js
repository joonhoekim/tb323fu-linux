// SPDX-License-Identifier: GPL-2.0-or-later
// TB323FU quick settings: a thin front-end for the tb323fu-helperd system
// service (docs/helper.md). Everything goes through the system D-Bus; the
// extension never touches sysfs or runs commands.

import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as ModalDialog from 'resource:///org/gnome/shell/ui/modalDialog.js';
import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';
import {QuickMenuToggle, SystemIndicator} from 'resource:///org/gnome/shell/ui/quickSettings.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS = 'io.github.joonhoekim.tb323fu.Helper';
const ROOT = '/io/github/joonhoekim/tb323fu/Helper';
const OBJ = name => ({path: `${ROOT}/${name}`, iface: `${BUS}.${name}`});

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
                    Main.notify('Tablet', `${method}: ${e.message}`);
                }
                this.refresh(name);
            });
    }
}

// ---- confirmation dialog for the Android switch --------------------------

const ConfirmDialog = GObject.registerClass(
class ConfirmDialog extends ModalDialog.ModalDialog {
    _init(onConfirm) {
        super._init({styleClass: 'modal-dialog'});
        const box = new St.BoxLayout({vertical: true, style: 'spacing: 12px; padding: 12px;'});
        box.add_child(new St.Label({text: 'Switch to Android?', style: 'font-weight: bold; font-size: 1.2em;'}));
        box.add_child(new St.Label({text: 'The tablet restarts into Android now. Unsaved work in Linux is lost.'}));
        this.contentLayout.add_child(box);
        this.addButton({label: 'Cancel', action: () => this.close(), key: 0xff1b /* Escape */});
        this.addButton({label: 'Restart into Android', action: () => { this.close(); onConfirm(); }, default: true});
    }
});

// ---- the quick-settings tile ---------------------------------------------

const TabletToggle = GObject.registerClass(
class TabletToggle extends QuickMenuToggle {
    _init() {
        super._init({title: 'Tablet', iconName: 'computer-symbolic', toggleMode: true});
        this.menu.setHeader('computer-symbolic', 'Tablet');
        this._helper = new Helper(() => this._sync());

        // toggle itself = adaptive (auto) refresh on/off
        this.connect('clicked', () => {
            if (!this._helper.has('Refresh'))
                return;
            const auto = this._helper.props.Refresh?.Policy === 'auto';
            this._helper.call('Refresh', 'SetPolicy', 's', [auto ? 'off' : 'auto']);
        });
        this.menu.connect('open-state-changed', (_m, open) => {
            if (open)
                this._helper.refreshAll();
        });

        this._absent = new PopupMenu.PopupMenuItem('Helper service not running', {reactive: false});
        this.menu.addMenuItem(this._absent);

        // battery
        this._batSection = new PopupMenu.PopupMenuSection();
        this._batInfo = new PopupMenu.PopupMenuItem('', {reactive: false});
        this._batSection.addMenuItem(this._batInfo);
        this._limitItems = {};
        for (const p of [60, 80, 100]) {
            const it = stayItem(`Charge limit ${p} %`, this._limitItems, p,
                () => this._helper.call('Battery', 'SetChargeLimit', 'u', [p]));
            this._limitItems[p] = it;
            this._batSection.addMenuItem(it);
        }
        this._bypass = staySwitch('Bypass charging (run from the charger)',
            on => this._helper.call('Battery', 'SetBypass', 'b', [on]));
        this._batSection.addMenuItem(this._bypass);
        this.menu.addMenuItem(this._batSection);

        // refresh
        this._refSection = new PopupMenu.PopupMenuSection();
        this._refSection.addMenuItem(new PopupMenu.PopupSeparatorMenuItem('Refresh rate'));
        this._refInfo = new PopupMenu.PopupMenuItem('', {reactive: false});
        this._refSection.addMenuItem(this._refInfo);
        this._policyItems = {};
        for (const [pol, label] of [['auto', 'Adaptive (lower when idle)'], ['manual', 'Fixed rate'], ['off', 'Always 120 Hz']]) {
            const it = stayItem(label, this._policyItems, pol,
                () => this._helper.call('Refresh', 'SetPolicy', 's', [pol]));
            this._policyItems[pol] = it;
            this._refSection.addMenuItem(it);
        }
        this._presetItems = {};
        for (const [pre, label] of [['power-saver', 'Idle timing: power saver'], ['balanced', 'Idle timing: balanced'], ['smooth', 'Idle timing: smooth']]) {
            const it = stayItem(label, this._presetItems, pre,
                () => this._helper.call('Refresh', 'ApplyPreset', 's', [pre]));
            this._presetItems[pre] = it;
            this._refSection.addMenuItem(it);
        }
        this.menu.addMenuItem(this._refSection);

        // torch
        this._torchSection = new PopupMenu.PopupMenuSection();
        this._torchSection.addMenuItem(new PopupMenu.PopupSeparatorMenuItem('Torch'));
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
        this.menu.addMenuItem(this._torchSection);

        // gpu + usb
        this._miscSection = new PopupMenu.PopupMenuSection();
        this._miscSection.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._gpuFollow = staySwitch('GPU follows the power mode',
            on => this._helper.call('Gpu', 'SetFollowPowerProfiles', 'b', [on]));
        this._miscSection.addMenuItem(this._gpuFollow);
        this._usbWake = staySwitch('Wake from USB devices',
            on => this._helper.call('Usb', 'SetWake', 'b', [on]));
        this._miscSection.addMenuItem(this._usbWake);
        this.menu.addMenuItem(this._miscSection);

        // android
        this._androidSection = new PopupMenu.PopupMenuSection();
        this._androidSection.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._android = new PopupMenu.PopupMenuItem('Switch to Android…');
        this._android.connect('activate', () => {
            new ConfirmDialog(() => this._helper.call('Android', 'SwitchToAndroid')).open();
        });
        this._androidSection.addMenuItem(this._android);
        this.menu.addMenuItem(this._androidSection);

        this._sync();
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
        this.reactive = present;

        const bat = h.props.Battery;
        this._batSection.actor.visible = h.has('Battery');
        if (bat) {
            const contract = bat.ChargerContract ? ` · ${bat.ChargerContract}` : '';
            this._batInfo.label.text = `Battery ${bat.Capacity} % · ${bat.State}${contract}`;
            for (const [p, it] of Object.entries(this._limitItems))
                it.setOrnament(Number(p) === bat.ChargeLimit && !bat.Bypass ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
            this._bypass.setToggleState(!!bat.Bypass);
        }

        const ref = h.props.Refresh;
        this._refSection.actor.visible = h.has('Refresh');
        if (ref) {
            this.checked = ref.Policy === 'auto';
            this._refInfo.label.text = `Now ${ref.LiveRate} Hz`;
            this.subtitle = `${ref.LiveRate} Hz`;
            for (const [pol, it] of Object.entries(this._policyItems))
                it.setOrnament(pol === ref.Policy ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
            const cur = `${ref.IdleMs60}/${ref.IdleMs30}`;
            const presets = {'power-saver': '500/2000', balanced: '1000/5000', smooth: '3000/15000'};
            for (const [pre, it] of Object.entries(this._presetItems)) {
                it.visible = ref.Policy === 'auto';
                it.setOrnament(presets[pre] === cur ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
            }
        } else {
            this.checked = false;
            this.subtitle = present ? null : 'helper not running';
        }

        const t = h.props.Torch;
        this._torchSection.actor.visible = h.has('Torch');
        if (t) {
            this._torch.setToggleState(!!t.On);
            this._sliderSync = true;
            this._torchSlider.value = t.MaxLevel ? t.Level / t.MaxLevel : 0;
            this._sliderSync = false;
        }

        const g = h.props.Gpu;
        this._gpuFollow.visible = h.has('Gpu');
        if (g)
            this._gpuFollow.setToggleState(!!g.FollowPowerProfiles);
        const u = h.props.Usb;
        this._usbWake.visible = h.has('Usb');
        if (u)
            this._usbWake.setToggleState(!!u.WakeEnabled);
        this._miscSection.actor.visible = h.has('Gpu') || h.has('Usb');

        const a = h.props.Android;
        this._androidSection.actor.visible = h.has('Android');
        if (a)
            this._android.setSensitive(!!a.Available);
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
    }
}
