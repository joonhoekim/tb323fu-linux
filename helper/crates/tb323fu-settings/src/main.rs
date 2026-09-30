// SPDX-License-Identifier: MIT
//! tb323fu-settings: GTK4/libadwaita settings app for the TB323FU helper.
//!
//! A thin front-end: every value comes from tb323fu-helperd over the system
//! D-Bus (property snapshots polled every 2 s), every change is a method call.
//! Pages whose object the daemon does not export (feature absent on this
//! kernel) are hidden; with no daemon at all a status page explains it.
//! Privileges are decided by the daemon through polkit, not here.

mod dbus;

use adw::prelude::*;
use dbus::{Client, Props};
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const APP_ID: &str = "io.github.joonhoekim.tb323fu.Settings";
const REFRESH_PROFILES: [&str; 3] = ["off", "auto", "manual"];
const REFRESH_PRESETS: [&str; 3] = ["power-saver", "balanced", "smooth"];
const GPU_PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];
const LED_MODES: [&str; 2] = ["charge", "off"];

// ---------------------------------------------------------------- widgets --

fn group(page: &adw::PreferencesPage, title: &str, desc: &str) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::builder().title(title).build();
    if !desc.is_empty() {
        g.set_description(Some(desc));
    }
    page.add(&g);
    g
}

fn info(g: &adw::PreferencesGroup, title: &str) -> gtk::Label {
    let row = adw::ActionRow::builder().title(title).build();
    let l = gtk::Label::new(Some("…"));
    l.add_css_class("dim-label");
    l.set_selectable(true);
    row.add_suffix(&l);
    g.add(&row);
    l
}

fn spin(g: &adw::PreferencesGroup, title: &str, subtitle: &str, lo: f64, hi: f64, step: f64) -> adw::SpinRow {
    let r = adw::SpinRow::with_range(lo, hi, step);
    r.set_title(title);
    if !subtitle.is_empty() {
        r.set_subtitle(subtitle);
    }
    g.add(&r);
    r
}

fn switch(g: &adw::PreferencesGroup, title: &str, subtitle: &str) -> adw::SwitchRow {
    let r = adw::SwitchRow::builder().title(title).build();
    if !subtitle.is_empty() {
        r.set_subtitle(subtitle);
    }
    g.add(&r);
    r
}

fn combo(g: &adw::PreferencesGroup, title: &str, items: &[&str]) -> adw::ComboRow {
    let r = adw::ComboRow::builder().title(title).model(&gtk::StringList::new(items)).build();
    g.add(&r);
    r
}

fn button(g: &adw::PreferencesGroup, title: &str, subtitle: &str, label: &str) -> gtk::Button {
    let row = adw::ActionRow::builder().title(title).build();
    if !subtitle.is_empty() {
        row.set_subtitle(subtitle);
    }
    let b = gtk::Button::with_label(label);
    b.set_valign(gtk::Align::Center);
    row.add_suffix(&b);
    row.set_activatable_widget(Some(&b));
    g.add(&row);
    b
}

fn set_text(l: &gtk::Label, t: &str) {
    if l.text() != t {
        l.set_text(t);
    }
}
fn set_spin(r: &adw::SpinRow, v: Option<u32>) {
    if let Some(v) = v {
        if (r.value() - v as f64).abs() > 0.5 {
            r.set_value(v as f64);
        }
    }
}
fn set_switch(r: &adw::SwitchRow, v: Option<bool>) {
    if let Some(v) = v {
        if r.is_active() != v {
            r.set_active(v);
        }
    }
}
fn set_combo(r: &adw::ComboRow, items: &[&str], v: Option<String>) {
    if let Some(i) = v.and_then(|v| items.iter().position(|x| *x == v)) {
        if r.selected() != i as u32 {
            r.set_selected(i as u32);
        }
    }
}
fn unknown_i(v: Option<i32>, unit: &str) -> String {
    match v {
        Some(x) if x >= 0 => format!("{x}{unit}"),
        _ => "unknown".into(),
    }
}

// --------------------------------------------------------------------- UI --

struct Page {
    objects: &'static [&'static str],
    row: gtk::ListBoxRow,
    nav: adw::NavigationPage,
}

struct Ui {
    client: RefCell<Client>,
    updating: Cell<bool>,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    split: adw::NavigationSplitView,
    sidebar: gtk::ListBox,
    pages: Vec<Page>,
    absent: adw::NavigationPage,
    daemon_up: Cell<Option<bool>>,

    // Battery
    bat_limit: adw::SpinRow,
    bat_bypass: adw::SwitchRow,
    bat_state: gtk::Label,
    bat_cap: gtk::Label,
    bat_cur: gtk::Label,
    bat_volt: gtk::Label,
    bat_temp: gtk::Label,
    bat_health: gtk::Label,
    bat_cycles: gtk::Label,
    bat_design: gtk::Label,
    bat_charger: gtk::Label,
    // Refresh
    ref_policy: adw::ComboRow,
    ref_rate: adw::SpinRow,
    ref_preset: adw::ComboRow,
    ref_ms60: adw::SpinRow,
    ref_ms30: adw::SpinRow,
    ref_min: gtk::Label,
    ref_input: gtk::Label,
    ref_live: gtk::Label,
    // Gpu
    gpu_profile: adw::ComboRow,
    gpu_follow: adw::SwitchRow,
    gpu_limits: Vec<(&'static str, adw::SpinRow, adw::SpinRow)>,
    // Torch / LED
    torch_group: adw::PreferencesGroup,
    torch_on: adw::SwitchRow,
    torch_level: adw::SpinRow,
    led_group: adw::PreferencesGroup,
    led_mode: adw::ComboRow,
    led_bright: adw::SpinRow,
    led_low: adw::SpinRow,
    // Usb
    usb_wake: adw::SwitchRow,
    usb_dev: adw::SwitchRow,
    // Emergency key
    ek_enabled: adw::SwitchRow,
    ek_hold: adw::SpinRow,
    // Android
    and_avail: gtk::Label,
    and_hash: gtk::Label,
    and_auth: adw::SwitchRow,
    and_switch: gtk::Button,
    // Diagnostics
    diag_crash: gtk::Label,
    diag_clean: gtk::Label,
    diag_path: gtk::Label,
    diag_open: gtk::Button,
    diag_last: RefCell<Option<String>>,
    // About
    ab_version: gtk::Label,
    ab_kernel: gtk::Label,
    ab_series: gtk::Label,
    ab_features: gtk::Label,
    ab_fw: adw::ExpanderRow,
    ab_fw_rows: RefCell<Vec<adw::ActionRow>>,
    ab_fw_last: RefCell<Vec<(String, String)>>,
}

fn content_page(title: &str, child: &impl IsA<gtk::Widget>) -> adw::NavigationPage {
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(child));
    adw::NavigationPage::builder().title(title).tag(title).child(&tv).build()
}

impl Ui {
    fn new(app: &adw::Application) -> Rc<Self> {
        // Battery
        let p_bat = adw::PreferencesPage::new();
        let g = group(&p_bat, "Charging", "The helper owns the charge limit; the battery stops charging at the limit.");
        let bat_limit = spin(&g, "Charge limit", "Percent (20–100). 80 keeps the battery healthier.", 20.0, 100.0, 5.0);
        let bat_bypass = switch(&g, "Bypass charging", "Run from the charger and leave the battery idle at its current charge");
        let g = group(&p_bat, "Battery", "");
        let bat_state = info(&g, "State");
        let bat_cap = info(&g, "Charge");
        let bat_cur = info(&g, "Current");
        let bat_volt = info(&g, "Voltage");
        let bat_temp = info(&g, "Temperature");
        let bat_health = info(&g, "Health");
        let bat_cycles = info(&g, "Charge cycles");
        let bat_design = info(&g, "Design capacity");
        let g = group(&p_bat, "Charger", "");
        let bat_charger = info(&g, "Contract");

        // Display
        let p_ref = adw::PreferencesPage::new();
        let g = group(&p_ref, "Idle refresh rate", "The panel stays in its 120 Hz mode; when nothing changes on screen the kernel slows it to 60 and then 30 Hz, and any update brings it back at once.");
        let ref_policy = combo(&g, "Policy", &REFRESH_PROFILES);
        let ref_rate = spin(&g, "Manual rate", "Used when the policy is manual (Hz)", 30.0, 120.0, 30.0);
        let ref_live = info(&g, "Current rate");
        let g = group(&p_ref, "Timing", "");
        let ref_preset = combo(&g, "Preset", &REFRESH_PRESETS);
        let preset_btn = button(&g, "Apply preset", "Sets both idle times below", "Apply");
        let ref_ms60 = spin(&g, "Idle time before 60 Hz", "Milliseconds without screen updates", 100.0, 60000.0, 100.0);
        let ref_ms30 = spin(&g, "Idle time before 30 Hz", "Milliseconds without screen updates", 100.0, 120000.0, 100.0);
        let ref_min = info(&g, "Lowest rate");
        let ref_input = info(&g, "Touch and keys wake to 120 Hz");

        // Performance
        let p_gpu = adw::PreferencesPage::new();
        let g = group(&p_gpu, "GPU", "");
        let gpu_profile = combo(&g, "Profile", &GPU_PROFILES);
        let gpu_follow = switch(&g, "Follow the power profile", "Switch together with the system power mode");
        let g = group(&p_gpu, "Frequency limits", "Lowest and highest GPU clock per profile (MHz).");
        let mut gpu_limits = Vec::new();
        for p in GPU_PROFILES {
            let exp = adw::ExpanderRow::builder().title(p).build();
            let lo = adw::SpinRow::with_range(100.0, 2000.0, 1.0);
            lo.set_title("Minimum");
            let hi = adw::SpinRow::with_range(100.0, 2000.0, 1.0);
            hi.set_title("Maximum");
            exp.add_row(&lo);
            exp.add_row(&hi);
            let apply = adw::ActionRow::builder().title("Apply limits").build();
            let b = gtk::Button::with_label("Apply");
            b.set_valign(gtk::Align::Center);
            apply.add_suffix(&b);
            apply.set_activatable_widget(Some(&b));
            exp.add_row(&apply);
            g.add(&exp);
            gpu_limits.push((p, lo, hi, b));
        }

        // Torch & LED ring
        let p_led = adw::PreferencesPage::new();
        let torch_group = group(&p_led, "Torch", "The rear camera light, at torch brightness (not the flash).");
        let torch_on = switch(&torch_group, "Torch", "");
        let torch_level = spin(&torch_group, "Brightness", "", 1.0, 255.0, 1.0);
        let led_group = group(&p_led, "LED ring", "The RGB ring on the back.");
        let led_mode = combo(&led_group, "Mode", &LED_MODES);
        let led_bright = spin(&led_group, "Brightness", "", 1.0, 255.0, 1.0);
        let led_low = spin(&led_group, "Low battery below", "Percent: the ring turns red below this charge", 5.0, 50.0, 1.0);

        // USB
        let p_usb = adw::PreferencesPage::new();
        let g = group(&p_usb, "USB", "");
        let usb_wake = switch(&g, "Wake from USB devices", "A keyboard or mouse on the USB-C port can wake the tablet");
        let g = group(&p_usb, "Developer mode", "Exposes a network interface and a root serial console over the USB cable. Only turn this on for development.");
        let usb_dev = switch(&g, "USB developer mode", "Asks for authentication");

        // Emergency key
        let p_ek = adw::PreferencesPage::new();
        let g = group(&p_ek, "Emergency key", "Holding volume up and volume down together restarts into Android, even when the desktop is frozen.");
        let ek_enabled = switch(&g, "Enabled", "Turning it off asks for authentication");
        let ek_hold = spin(&g, "Hold time", "Seconds both keys must be held", 3.0, 30.0, 1.0);

        // Android
        let p_and = adw::PreferencesPage::new();
        let g = group(&p_and, "Android", "Android stays installed on the tablet; the Linux image replaces it in the boot slot until you switch back.");
        let and_avail = info(&g, "Android image");
        let and_hash = info(&g, "Image SHA-256");
        let and_auth = switch(&g, "Ask for authentication", "Require a password before switching (changing this asks for authentication)");
        let g = group(&p_and, "", "");
        let and_switch = button(&g, "Restart into Android", "Writes the Android image back and restarts now", "Restart…");
        and_switch.add_css_class("destructive-action");

        // Diagnostics
        let p_diag = adw::PreferencesPage::new();
        let g = group(&p_diag, "Crash records", "");
        let diag_crash = info(&g, "Stored records");
        let diag_clean = info(&g, "Last boot ended cleanly");
        let g = group(&p_diag, "Export", "Collects crash records, the end of the previous boot's log and versions into one archive. Addresses, host and user names are removed.");
        let export_btn = button(&g, "Export diagnostics", "", "Export");
        let diag_path = info(&g, "Archive");
        let diag_open = button(&g, "Show the archive", "", "Open folder");
        diag_open.set_sensitive(false);

        // About
        let p_about = adw::PreferencesPage::new();
        let g = group(&p_about, "Versions", "");
        let ab_version = info(&g, "Helper");
        let ab_kernel = info(&g, "Kernel");
        let ab_series = info(&g, "Patch series");
        let ab_features = info(&g, "Features");
        let g = group(&p_about, "Firmware", "Files from your own tablet, compared with the manifest.");
        let ab_fw = adw::ExpanderRow::builder().title("Firmware files").build();
        g.add(&ab_fw);

        // Navigation
        let sidebar = gtk::ListBox::new();
        sidebar.add_css_class("navigation-sidebar");
        let defs: [(&str, &str, &'static [&'static str], &adw::PreferencesPage); 9] = [
            ("Battery", "battery-good-symbolic", &["Battery"], &p_bat),
            ("Display", "video-display-symbolic", &["Refresh"], &p_ref),
            ("Performance", "power-profile-balanced-symbolic", &["Gpu"], &p_gpu),
            ("Torch & LED ring", "weather-clear-symbolic", &["Torch", "LedRing"], &p_led),
            ("USB", "media-removable-symbolic", &["Usb"], &p_usb),
            ("Emergency key", "dialog-warning-symbolic", &["EmergencyKey"], &p_ek),
            ("Android", "system-reboot-symbolic", &["Android"], &p_and),
            ("Diagnostics", "utilities-system-monitor-symbolic", &["Diagnostics"], &p_diag),
            ("About", "help-about-symbolic", &[""], &p_about),
        ];
        let mut pages = Vec::new();
        for (title, icon, objects, page) in defs {
            let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            bx.append(&gtk::Image::from_icon_name(icon));
            let l = gtk::Label::new(Some(title));
            l.set_xalign(0.0);
            bx.append(&l);
            let row = gtk::ListBoxRow::builder().child(&bx).build();
            sidebar.append(&row);
            pages.push(Page { objects, row, nav: content_page(title, page) });
        }
        let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&sidebar).build();
        let side_tv = adw::ToolbarView::new();
        side_tv.add_top_bar(&adw::HeaderBar::new());
        side_tv.set_content(Some(&scroller));
        let side_page = adw::NavigationPage::builder().title("TB323FU").child(&side_tv).build();

        let status = adw::StatusPage::builder()
            .icon_name("dialog-warning-symbolic")
            .title("Helper service not running")
            .description("This app talks to tb323fu-helperd. Start it with\nsystemctl start tb323fu-helperd")
            .build();
        let absent = content_page("TB323FU", &status);

        let split = adw::NavigationSplitView::new();
        split.set_sidebar(Some(&side_page));
        split.set_content(Some(&absent));
        split.set_min_sidebar_width(200.0);

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Tablet settings")
            .default_width(900)
            .default_height(760)
            .content(&toasts)
            .build();
        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 600sp").unwrap());
        bp.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(bp);

        let gpu_buttons: Vec<gtk::Button> = gpu_limits.iter().map(|(_, _, _, b)| b.clone()).collect();
        let ui = Rc::new(Ui {
            client: RefCell::new(Client::connect()),
            updating: Cell::new(false),
            window,
            toasts,
            split,
            sidebar,
            pages,
            absent,
            daemon_up: Cell::new(None),
            bat_limit,
            bat_bypass,
            bat_state,
            bat_cap,
            bat_cur,
            bat_volt,
            bat_temp,
            bat_health,
            bat_cycles,
            bat_design,
            bat_charger,
            ref_policy,
            ref_rate,
            ref_preset,
            ref_ms60,
            ref_ms30,
            ref_min,
            ref_input,
            ref_live,
            gpu_profile,
            gpu_follow,
            gpu_limits: gpu_limits.into_iter().map(|(p, lo, hi, _)| (p, lo, hi)).collect(),
            torch_group,
            torch_on,
            torch_level,
            led_group,
            led_mode,
            led_bright,
            led_low,
            usb_wake,
            usb_dev,
            ek_enabled,
            ek_hold,
            and_avail,
            and_hash,
            and_auth,
            and_switch,
            diag_crash,
            diag_clean,
            diag_path,
            diag_open,
            diag_last: RefCell::new(None),
            ab_version,
            ab_kernel,
            ab_series,
            ab_features,
            ab_fw,
            ab_fw_rows: RefCell::new(Vec::new()),
            ab_fw_last: RefCell::new(Vec::new()),
        });
        ui.connect(preset_btn, export_btn, gpu_buttons);
        ui
    }

    fn toast(&self, msg: &str) {
        self.toasts.add_toast(adw::Toast::new(msg));
    }

    /// Run a method call off the main thread; toast errors, then refresh.
    fn call<B, F>(self: &Rc<Self>, obj: &'static str, method: &'static str, body: B, on_ok: F)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
        F: FnOnce(&Rc<Ui>, Option<String>) + 'static,
    {
        let client = self.client.borrow().clone();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(move || client.call(obj, method, &body)).await;
            match res {
                Ok(Ok(reply)) => on_ok(&ui, reply),
                Ok(Err(e)) => ui.toast(&format!("{method}: {e}")),
                Err(_) => ui.toast(&format!("{method}: failed")),
            }
            ui.refresh();
        });
    }

    fn call_simple<B>(self: &Rc<Self>, obj: &'static str, method: &'static str, body: B)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    {
        self.call(obj, method, body, |_, _| {});
    }

    fn connect(self: &Rc<Self>, preset_btn: gtk::Button, export_btn: gtk::Button, gpu_buttons: Vec<gtk::Button>) {
        // Navigation
        let ui = self.clone();
        self.sidebar.connect_row_activated(move |_, row| {
            let i = row.index();
            if i >= 0 {
                if let Some(p) = ui.pages.get(i as usize) {
                    ui.split.set_content(Some(&p.nav));
                    ui.split.set_show_content(true);
                }
            }
        });

        // Battery
        let ui = self.clone();
        self.bat_limit.connect_value_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Battery", "SetChargeLimit", (r.value() as u32,));
            }
        });
        let ui = self.clone();
        self.bat_bypass.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Battery", "SetBypass", (r.is_active(),));
            }
        });

        // Refresh
        let ui = self.clone();
        self.ref_policy.connect_selected_notify(move |r| {
            if !ui.updating.get() {
                let p = REFRESH_PROFILES[r.selected() as usize % REFRESH_PROFILES.len()];
                ui.call_simple("Refresh", "SetPolicy", (p.to_string(),));
            }
        });
        let ui = self.clone();
        self.ref_rate.connect_value_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Refresh", "SetRate", (r.value() as u32,));
            }
        });
        let ui = self.clone();
        preset_btn.connect_clicked(move |_| {
            let p = REFRESH_PRESETS[ui.ref_preset.selected() as usize % REFRESH_PRESETS.len()];
            ui.call_simple("Refresh", "ApplyPreset", (p.to_string(),));
        });
        for spin in [&self.ref_ms60, &self.ref_ms30] {
            let ui = self.clone();
            spin.connect_value_notify(move |_| {
                if !ui.updating.get() {
                    let (a, b) = (ui.ref_ms60.value() as u32, ui.ref_ms30.value() as u32);
                    ui.call_simple("Refresh", "SetIdle", (a, b));
                }
            });
        }

        // Gpu
        let ui = self.clone();
        self.gpu_profile.connect_selected_notify(move |r| {
            if !ui.updating.get() {
                let p = GPU_PROFILES[r.selected() as usize % GPU_PROFILES.len()];
                ui.call_simple("Gpu", "SetProfile", (p.to_string(),));
            }
        });
        let ui = self.clone();
        self.gpu_follow.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Gpu", "SetFollowPowerProfiles", (r.is_active(),));
            }
        });
        for (i, b) in gpu_buttons.into_iter().enumerate() {
            let ui = self.clone();
            b.connect_clicked(move |_| {
                let (p, lo, hi) = &ui.gpu_limits[i];
                ui.call_simple("Gpu", "SetLimits", (p.to_string(), lo.value() as u32, hi.value() as u32));
            });
        }

        // Torch / LED
        let ui = self.clone();
        self.torch_on.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Torch", "Set", (r.is_active(),));
            }
        });
        let ui = self.clone();
        self.torch_level.connect_value_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Torch", "SetLevel", (r.value() as u32,));
            }
        });
        let ui = self.clone();
        self.led_mode.connect_selected_notify(move |r| {
            if !ui.updating.get() {
                let m = LED_MODES[r.selected() as usize % LED_MODES.len()];
                ui.call_simple("LedRing", "SetMode", (m.to_string(),));
            }
        });
        let ui = self.clone();
        self.led_bright.connect_value_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("LedRing", "SetBrightness", (r.value() as u32,));
            }
        });
        let ui = self.clone();
        self.led_low.connect_value_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("LedRing", "SetLowPercent", (r.value() as u32,));
            }
        });

        // USB
        let ui = self.clone();
        self.usb_wake.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Usb", "SetWake", (r.is_active(),));
            }
        });
        let ui = self.clone();
        self.usb_dev.connect_active_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            if !r.is_active() {
                ui.call_simple("Usb", "SetDevMode", (false,));
                return;
            }
            let d = adw::AlertDialog::new(
                Some("Turn on USB developer mode?"),
                Some("Anyone with a USB cable to this tablet gets a network link and a root console. Only turn this on for development."),
            );
            d.add_response("cancel", "Cancel");
            d.add_response("on", "Turn on");
            d.set_response_appearance("on", adw::ResponseAppearance::Destructive);
            d.set_default_response(Some("cancel"));
            d.set_close_response("cancel");
            let ui2 = ui.clone();
            d.connect_response(None, move |_, resp| {
                if resp == "on" {
                    ui2.call_simple("Usb", "SetDevMode", (true,));
                } else {
                    ui2.updating.set(true);
                    ui2.usb_dev.set_active(false);
                    ui2.updating.set(false);
                }
            });
            d.present(Some(&ui.window));
        });

        // Emergency key
        let ui = self.clone();
        self.ek_enabled.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("EmergencyKey", "SetEnabled", (r.is_active(),));
            }
        });
        let ui = self.clone();
        self.ek_hold.connect_value_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("EmergencyKey", "SetHoldSeconds", (r.value() as u32,));
            }
        });

        // Android
        let ui = self.clone();
        self.and_auth.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("Android", "SetRequireAuth", (r.is_active(),));
            }
        });
        let ui = self.clone();
        self.and_switch.connect_clicked(move |_| {
            let d = adw::AlertDialog::new(
                Some("Restart into Android?"),
                Some("The Android image goes back into the boot slot and the tablet restarts now. Unsaved work is lost."),
            );
            d.add_response("cancel", "Cancel");
            d.add_response("go", "Restart into Android");
            d.set_response_appearance("go", adw::ResponseAppearance::Destructive);
            d.set_default_response(Some("cancel"));
            d.set_close_response("cancel");
            let ui2 = ui.clone();
            d.connect_response(None, move |_, resp| {
                if resp == "go" {
                    ui2.call_simple("Android", "SwitchToAndroid", ());
                }
            });
            d.present(Some(&ui.window));
        });

        // Diagnostics
        let ui = self.clone();
        export_btn.connect_clicked(move |_| {
            ui.toast("Collecting diagnostics…");
            ui.call("Diagnostics", "Export", (), |ui, reply| {
                if let Some(path) = reply {
                    set_text(&ui.diag_path, &path);
                    ui.diag_open.set_sensitive(true);
                    ui.toast("Diagnostics exported");
                    *ui.diag_last.borrow_mut() = Some(path);
                }
            });
        });
        let ui = self.clone();
        self.diag_open.connect_clicked(move |_| {
            if let Some(path) = ui.diag_last.borrow().clone() {
                let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(&path)));
                let ui2 = ui.clone();
                launcher.open_containing_folder(Some(&ui.window), None::<&gio::Cancellable>, move |r| {
                    if let Err(e) = r {
                        ui2.toast(&format!("Could not open the folder: {e}"));
                    }
                });
            }
        });
    }

    /// Poll the daemon and update every visible widget without triggering
    /// the change handlers.
    fn refresh(self: &Rc<Self>) {
        if !self.client.borrow().connected() {
            *self.client.borrow_mut() = Client::connect();
        }
        let c = self.client.borrow().clone();
        let root = c.get_all("");
        let up = root.is_some();
        if self.daemon_up.get() != Some(up) {
            self.daemon_up.set(Some(up));
            if up {
                if let Some(first) = self.pages.first() {
                    self.sidebar.select_row(Some(&first.row));
                    self.split.set_content(Some(&first.nav));
                }
            } else {
                self.split.set_content(Some(&self.absent));
            }
        }
        if !up {
            for p in &self.pages {
                p.row.set_visible(false);
            }
            return;
        }
        self.updating.set(true);
        let get = |o: &str| c.get_all(o);
        let bat = get("Battery");
        let rf = get("Refresh");
        let gpu = get("Gpu");
        let torch = get("Torch");
        let led = get("LedRing");
        let usb = get("Usb");
        let ek = get("EmergencyKey");
        let and = get("Android");
        let diag = get("Diagnostics");
        let present = |o: &str| match o {
            "" => true,
            "Battery" => bat.is_some(),
            "Refresh" => rf.is_some(),
            "Gpu" => gpu.is_some(),
            "Torch" => torch.is_some(),
            "LedRing" => led.is_some(),
            "Usb" => usb.is_some(),
            "EmergencyKey" => ek.is_some(),
            "Android" => and.is_some(),
            "Diagnostics" => diag.is_some(),
            _ => false,
        };
        for p in &self.pages {
            p.row.set_visible(p.objects.iter().any(|o| present(o)));
        }
        if let Some(p) = &bat {
            self.update_battery(p);
        }
        if let Some(p) = &rf {
            self.update_refresh(p);
        }
        if let Some(p) = &gpu {
            self.update_gpu(p);
        }
        self.torch_group.set_visible(torch.is_some());
        if let Some(p) = &torch {
            set_switch(&self.torch_on, dbus::b(p, "On"));
            if let Some(m) = dbus::u(p, "MaxLevel") {
                if m > 0 && (self.torch_level.adjustment().upper() - m as f64).abs() > 0.5 {
                    self.torch_level.adjustment().set_upper(m as f64);
                }
            }
            set_spin(&self.torch_level, dbus::u(p, "Level"));
        }
        self.led_group.set_visible(led.is_some());
        if let Some(p) = &led {
            set_combo(&self.led_mode, &LED_MODES, dbus::s(p, "Mode"));
            set_spin(&self.led_bright, dbus::u(p, "Brightness"));
            set_spin(&self.led_low, dbus::u(p, "LowPercent"));
        }
        if let Some(p) = &usb {
            set_switch(&self.usb_wake, dbus::b(p, "WakeEnabled"));
            set_switch(&self.usb_dev, dbus::b(p, "DevMode"));
        }
        if let Some(p) = &ek {
            set_switch(&self.ek_enabled, dbus::b(p, "Enabled"));
            set_spin(&self.ek_hold, dbus::u(p, "HoldSeconds"));
        }
        if let Some(p) = &and {
            let avail = dbus::b(p, "Available").unwrap_or(false);
            set_text(&self.and_avail, if avail { "ready" } else { "not set up" });
            let h = dbus::s(p, "ImageSha256").unwrap_or_default();
            set_text(&self.and_hash, if h.len() >= 16 { &h[..16] } else if h.is_empty() { "unknown" } else { h.as_str() });
            set_switch(&self.and_auth, dbus::b(p, "RequireAuth"));
            self.and_switch.set_sensitive(avail);
        }
        if let Some(p) = &diag {
            set_text(&self.diag_crash, &dbus::u(p, "CrashRecords").map(|n| n.to_string()).unwrap_or_else(|| "unknown".into()));
            set_text(
                &self.diag_clean,
                match dbus::b(p, "LastBootClean") {
                    Some(true) => "yes",
                    Some(false) => "no — see the crash records",
                    None => "unknown",
                },
            );
        }
        if let Some(p) = &root {
            self.update_about(p);
        }
        self.updating.set(false);
    }

    fn update_battery(&self, p: &Props) {
        set_spin(&self.bat_limit, dbus::u(p, "ChargeLimit"));
        set_switch(&self.bat_bypass, dbus::b(p, "Bypass"));
        set_text(&self.bat_state, &dbus::s(p, "State").unwrap_or_else(|| "unknown".into()));
        set_text(&self.bat_cap, &dbus::u(p, "Capacity").map(|c| format!("{c} %")).unwrap_or_else(|| "unknown".into()));
        set_text(&self.bat_cur, &dbus::i(p, "CurrentMa").map(|c| format!("{c} mA")).unwrap_or_else(|| "unknown".into()));
        set_text(&self.bat_volt, &dbus::u(p, "VoltageMv").map(|v| format!("{:.2} V", v as f64 / 1000.0)).unwrap_or_else(|| "unknown".into()));
        set_text(&self.bat_temp, &dbus::f(p, "TemperatureC").map(|t| format!("{t:.1} °C")).unwrap_or_else(|| "unknown".into()));
        set_text(&self.bat_health, &dbus::s(p, "Health").unwrap_or_else(|| "unknown".into()));
        set_text(&self.bat_cycles, &unknown_i(dbus::i(p, "CycleCount"), ""));
        set_text(&self.bat_design, &unknown_i(dbus::i(p, "DesignCapacityMah"), " mAh"));
        let ty = dbus::s(p, "ChargerType").unwrap_or_default();
        let contract = dbus::s(p, "ChargerContract").unwrap_or_default();
        set_text(
            &self.bat_charger,
            if contract.is_empty() && ty.is_empty() { "not connected" } else if contract.is_empty() { ty.as_str() } else { contract.as_str() },
        );
    }

    fn update_refresh(&self, p: &Props) {
        set_combo(&self.ref_policy, &REFRESH_PROFILES, dbus::s(p, "Policy"));
        set_spin(&self.ref_rate, dbus::u(p, "Rate"));
        set_spin(&self.ref_ms60, dbus::u(p, "IdleMs60"));
        set_spin(&self.ref_ms30, dbus::u(p, "IdleMs30"));
        set_text(&self.ref_min, &dbus::u(p, "MinHz").map(|h| format!("{h} Hz")).unwrap_or_else(|| "unknown".into()));
        set_text(
            &self.ref_input,
            match dbus::b(p, "InputWakes") {
                Some(true) => "yes",
                Some(false) => "no",
                None => "unknown",
            },
        );
        set_text(&self.ref_live, &dbus::u(p, "LiveRate").map(|h| format!("{h} Hz")).unwrap_or_else(|| "unknown".into()));
        self.ref_rate.set_sensitive(self.ref_policy.selected() == 2);
    }

    fn update_gpu(&self, p: &Props) {
        set_combo(&self.gpu_profile, &GPU_PROFILES, dbus::s(p, "Profile"));
        set_switch(&self.gpu_follow, dbus::b(p, "FollowPowerProfiles"));
        self.gpu_profile.set_sensitive(!self.gpu_follow.is_active());
        let floors = dbus::dict_suu(p, "Floors");
        for (name, lo, hi) in &self.gpu_limits {
            if let Some((_, a, b)) = floors.iter().find(|(n, _, _)| n == name) {
                set_spin(lo, Some(*a));
                set_spin(hi, Some(*b));
            }
        }
    }

    fn update_about(&self, p: &Props) {
        set_text(&self.ab_version, &dbus::s(p, "Version").unwrap_or_else(|| "unknown".into()));
        set_text(&self.ab_kernel, &dbus::s(p, "Kernel").unwrap_or_else(|| "unknown".into()));
        set_text(&self.ab_series, &dbus::s(p, "SeriesTag").filter(|s| !s.is_empty()).unwrap_or_else(|| "unknown".into()));
        set_text(&self.ab_features, &dbus::strs(p, "Features").join(", "));
        let fw = dbus::dict_ss(p, "Firmware");
        if *self.ab_fw_last.borrow() != fw {
            for r in self.ab_fw_rows.borrow_mut().drain(..) {
                self.ab_fw.remove(&r);
            }
            let bad = fw.iter().filter(|(_, st)| st != "ok").count();
            self.ab_fw.set_subtitle(&if fw.is_empty() {
                "no manifest".to_string()
            } else if bad == 0 {
                format!("{} files, all match", fw.len())
            } else {
                format!("{} files, {bad} differ or are missing", fw.len())
            });
            for (file, st) in &fw {
                let row = adw::ActionRow::builder().title(file.as_str()).subtitle(st.as_str()).build();
                row.set_title_lines(1);
                self.ab_fw.add_row(&row);
                self.ab_fw_rows.borrow_mut().push(row);
            }
            *self.ab_fw_last.borrow_mut() = fw;
        }
    }
}

const USAGE: &str = "usage: tb323fu-settings [--help] [--version]

GTK4/libadwaita settings for the Lenovo Legion Tab Gen 5 (TB323FU).
Needs the tb323fu-helperd service on the system bus.";

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return glib::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version") {
        println!("tb323fu-settings {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        let ui = Ui::new(app);
        ui.refresh();
        let ui2 = ui.clone();
        glib::timeout_add_seconds_local(2, move || {
            ui2.refresh();
            glib::ControlFlow::Continue
        });
        ui.window.present();
    });
    app.run_with_args::<&str>(&[])
}
