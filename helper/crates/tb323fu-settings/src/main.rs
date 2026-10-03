// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! tb323fu-settings: GTK4/libadwaita settings app for the TB323FU helper.
//!
//! A thin front-end: every value comes from tb323fu-helperd over the system
//! D-Bus (property snapshots polled every 2 s), every change is a method call.
//! Pages whose object the daemon does not export (feature absent on this
//! kernel) are hidden; with no daemon at all a status page explains it.
//! Privileges are decided by the daemon through polkit, not here.
//!
//! Layout rules (the panel is 1904x3040, 952x1520 sp at 200 %): short
//! values sit on one line at the end of a row and ellipsize; long values
//! (kernel, paths, hashes) go under the row title with a copy button; group
//! descriptions are one short sentence, longer help sits behind a "?" popover.

mod dbus;

use adw::prelude::*;
use dbus::{Client, Props};
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

const APP_ID: &str = "io.github.joonhoekim.OpenDeviceHelper";
const REPO: &str = "https://github.com/joonhoekim/tb323fu-linux";
/// D-Bus ids and the labels shown for them (same order).
const REFRESH_POLICIES: [&str; 3] = ["off", "auto", "manual"];
const REFRESH_POLICY_LABELS: [&str; 3] = ["Always 120 Hz", "Adaptive", "Fixed Rate"];
/// (id, label, ms before 60 Hz, ms before 30 Hz) as in docs/helper.md.
const TIMINGS: [(&str, &str, u32, u32); 3] =
    [("power-saver", "Power Saver", 500, 2000), ("balanced", "Balanced", 1000, 5000), ("smooth", "Smooth", 3000, 15000)];
const GPU_PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];
const GPU_PROFILE_LABELS: [&str; 3] = ["Power Saver", "Balanced", "Performance"];
/// Every object the helper can export (for "N of M available").
const KNOWN_FEATURES: usize = 12;
const DEBOUNCE: Duration = Duration::from_millis(400);
/// The objects refresh() polls (the root object is polled first).
const OBJECTS: [&str; 12] = ["Battery", "Refresh", "Gpu", "Torch", "LedRing", "Usb", "EmergencyKey", "Android", "Diagnostics", "Boot", "Thermal", "Kernel"];

const CSS: &str = "
.tag { font-size: smaller; font-weight: bold; padding: 2px 8px; border-radius: 999px;
       background-color: alpha(currentColor, 0.12); }
";

// ---------------------------------------------------------------- widgets --

/// A page body: scrolls vertically, rows clamped to 760 px (wider than
/// AdwPreferencesPage's 600 px, so values and titles share one line).
fn page_box() -> (gtk::ScrolledWindow, gtk::Box) {
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 24);
    bx.set_margin_top(24);
    bx.set_margin_bottom(24);
    bx.set_margin_start(12);
    bx.set_margin_end(12);
    let clamp = adw::Clamp::builder().maximum_size(760).tightening_threshold(600).child(&bx).build();
    let sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&clamp).vexpand(true).build();
    (sw, bx)
}

fn group(page: &gtk::Box, title: &str, desc: &str) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::new();
    if !title.is_empty() {
        g.set_title(title);
    }
    if !desc.is_empty() {
        g.set_description(Some(desc));
    }
    page.append(&g);
    g
}

/// A "?" button in the group header whose popover holds the longer text.
fn group_help(g: &adw::PreferencesGroup, text: &str) {
    let l = gtk::Label::new(Some(text));
    l.set_wrap(true);
    l.set_max_width_chars(40);
    l.set_xalign(0.0);
    l.set_margin_top(6);
    l.set_margin_bottom(6);
    l.set_margin_start(6);
    l.set_margin_end(6);
    let pop = gtk::Popover::builder().child(&l).build();
    let b = gtk::MenuButton::builder().icon_name("help-about-symbolic").popover(&pop).valign(gtk::Align::Center).build();
    b.add_css_class("flat");
    b.set_tooltip_text(Some("More information"));
    g.set_header_suffix(Some(&b));
}

/// A short read-only value at the end of the row: one line, ellipsized, the
/// full text in the tooltip (set_text keeps it in sync).
fn info(g: &adw::PreferencesGroup, title: &str) -> gtk::Label {
    let row = adw::ActionRow::builder().title(title).build();
    let l = gtk::Label::new(Some("…"));
    l.add_css_class("dim-label");
    l.set_wrap(false);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l.set_xalign(1.0);
    l.set_valign(gtk::Align::Center);
    row.add_suffix(&l);
    g.add(&row);
    l
}

/// A long read-only value under the row title (property style, at most two
/// lines) with a copy button that copies the full text.
#[derive(Clone)]
struct LongInfo {
    row: adw::ActionRow,
    full: Rc<RefCell<String>>,
}

impl LongInfo {
    fn set(&self, shown: &str, full: &str) {
        if self.row.subtitle().as_deref() != Some(shown) {
            self.row.set_subtitle(shown);
        }
        if *self.full.borrow() != full {
            *self.full.borrow_mut() = full.to_string();
            self.row.set_tooltip_text(Some(full));
        }
    }
}

fn info_long(g: &adw::PreferencesGroup, title: &str, toasts: &adw::ToastOverlay) -> LongInfo {
    let row = adw::ActionRow::builder().title(title).subtitle("…").build();
    row.add_css_class("property");
    row.set_subtitle_lines(2);
    // Not selectable: a selectable label grabs focus and opens highlighted;
    // the copy button covers the use.
    let full = Rc::new(RefCell::new(String::new()));
    let b = gtk::Button::from_icon_name("edit-copy-symbolic");
    b.add_css_class("flat");
    b.set_valign(gtk::Align::Center);
    b.set_tooltip_text(Some("Copy"));
    b.update_property(&[gtk::accessible::Property::Label(&format!("Copy {title}"))]);
    let (f2, t2) = (full.clone(), toasts.clone());
    b.connect_clicked(move |b| {
        b.clipboard().set_text(&f2.borrow());
        t2.add_toast(adw::Toast::builder().title("Copied").timeout(2).build());
    });
    row.add_suffix(&b);
    g.add(&row);
    LongInfo { row, full }
}

/// Touch first: the number changes with the -/+ buttons only. An editable
/// entry took the focus on a tap (in Gaming Mode on SteamOS the on-screen
/// keyboard opened and focus could not leave the field).
fn touch_spin(r: &adw::SpinRow) {
    r.set_editable(false);
    r.set_focus_on_click(false);
}

fn spin(g: &adw::PreferencesGroup, title: &str, subtitle: &str, lo: f64, hi: f64, step: f64) -> adw::SpinRow {
    let r = adw::SpinRow::with_range(lo, hi, step);
    touch_spin(&r);
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

fn combo(g: &adw::PreferencesGroup, title: &str, labels: &[&str]) -> adw::ComboRow {
    let r = adw::ComboRow::builder().title(title).model(&gtk::StringList::new(labels)).build();
    g.add(&r);
    r
}

fn button(g: &adw::PreferencesGroup, title: &str, subtitle: &str, label: &str) -> (adw::ActionRow, gtk::Button) {
    let row = adw::ActionRow::builder().title(title).build();
    if !subtitle.is_empty() {
        row.set_subtitle(subtitle);
    }
    let b = gtk::Button::with_label(label);
    b.set_valign(gtk::Align::Center);
    row.add_suffix(&b);
    row.set_activatable_widget(Some(&b));
    g.add(&row);
    (row, b)
}

/// A small rounded state label ("Running", "Default", ...).
fn tag(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("tag");
    l.add_css_class(class);
    l.set_valign(gtk::Align::Center);
    l
}

fn row_of(w: &impl IsA<gtk::Widget>) -> Option<gtk::Widget> {
    w.ancestor(adw::ActionRow::static_type())
}

fn set_text(l: &gtk::Label, t: &str) {
    if l.text() != t {
        l.set_text(t);
        l.set_tooltip_text(Some(t));
    }
}
fn set_class(w: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if w.has_css_class(class) != on {
        if on {
            w.add_css_class(class);
        } else {
            w.remove_css_class(class);
        }
    }
}
/// The spin row (its text entry) has keyboard focus: the user is typing.
fn editing(r: &adw::SpinRow) -> bool {
    r.root().and_then(|w| w.focus()).is_some_and(|f| f.is_ancestor(r))
}
fn set_spin(r: &adw::SpinRow, v: Option<f64>) {
    if let Some(v) = v {
        if !editing(r) && (r.value() - v).abs() > 1e-6 {
            r.set_value(v);
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
fn set_combo(r: &adw::ComboRow, ids: &[&str], v: Option<String>) {
    if let Some(i) = v.and_then(|v| ids.iter().position(|x| *x == v)) {
        if r.selected() != i as u32 {
            r.set_selected(i as u32);
        }
    }
}
fn yes_no(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "Yes",
        Some(false) => "No",
        None => "Unknown",
    }
}
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
fn degrees(v: Option<f64>) -> String {
    match v {
        Some(t) if t.is_finite() => format!("{t:.1} °C"),
        _ => "Unknown".into(),
    }
}

/// A root problem in a few words for the row subtitle: the part in
/// parentheses when there is one ("missing modules for 7.3.0-... (no sound
/// or Wi-Fi)" -> "No sound or Wi-Fi"); the full text is in the tooltip.
fn short_problem(p: &str) -> String {
    match (p.rfind('('), p.ends_with(')')) {
        (Some(i), true) => capitalize(&p[i + 1..p.len() - 1]),
        _ => capitalize(p),
    }
}

/// One poll of the daemon: the root object's properties and each object's.
struct Fetched {
    root: Option<Props>,
    props: HashMap<&'static str, Option<Props>>,
}

/// GetAll for the root object and every object not in `skip` (blocking).
fn fetch(c: &Client, skip: &[&'static str]) -> Fetched {
    let root = c.get_all("");
    let props = if root.is_some() {
        OBJECTS.iter().filter(|o| !skip.contains(o)).map(|o| (*o, c.get_all(o))).collect()
    } else {
        HashMap::new()
    };
    Fetched { root, props }
}

/// The release from /proc/version ("Linux version 7.3.0 (...)" -> "7.3.0").
fn kernel_release(full: &str) -> String {
    full.split_whitespace().nth(2).unwrap_or(full).to_string()
}

/// Board sensor names (Thermal.Zones keys) for people.
fn zone_label(z: &str) -> String {
    match z {
        "skin" => "Back Cover".into(),
        "quiet" => "Board".into(),
        "batt" => "Battery".into(),
        "batt2" => "Battery 2".into(),
        "usb" => "USB".into(),
        "usb2-conn" => "USB Connector".into(),
        "lcm" => "Display Panel".into(),
        "wlan" => "Wi-Fi".into(),
        "ddr" => "Memory".into(),
        "ufs" => "Storage".into(),
        "xo" => "Clock Crystal".into(),
        "rear-cam" => "Rear Camera".into(),
        "fcam" => "Front Camera".into(),
        "wls" => "Wireless".into(),
        other => capitalize(other),
    }
}

// --------------------------------------------------------------------- UI --

struct Page {
    objects: &'static [&'static str],
    row: gtk::ListBoxRow,
    nav: adw::NavigationPage,
}

/// The last Boot values the rows were built from.
#[derive(PartialEq, Default, Clone)]
struct BootState {
    roots: Vec<(String, String, bool, String)>,
    cur: String,
    def: String,
    next: String,
    health: Vec<(String, Vec<String>)>,
}

struct Ui {
    client: RefCell<Client>,
    updating: Cell<bool>,
    /// a poll is running on a worker thread / another one was asked for
    polling: Cell<bool>,
    poll_again: Cell<bool>,
    /// calls in flight (and open confirmation dialogs / pending debounces) per
    /// object: refresh() leaves that object's widgets alone meanwhile
    pending: RefCell<HashMap<&'static str, u32>>,
    timers: RefCell<HashMap<&'static str, glib::SourceId>>,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    split: adw::NavigationSplitView,
    sidebar: gtk::ListBox,
    pages: Vec<Page>,
    selected: Cell<Option<usize>>,
    daemon_up: Cell<Option<bool>>,

    // Battery
    bat_limit: adw::SpinRow,
    bat_bypass: adw::SwitchRow,
    bat_state: gtk::Label,
    bat_cap: gtk::Label,
    bat_power: gtk::Label,
    bat_cur: gtk::Label,
    bat_volt: gtk::Label,
    bat_temp: gtk::Label,
    bat_health: gtk::Label,
    bat_cycles: gtk::Label,
    bat_design: gtk::Label,
    bat_charger: gtk::Label,
    bat_input: gtk::Label,
    // Refresh
    ref_group: adw::PreferencesGroup,
    ref_policy: adw::ComboRow,
    ref_rate: adw::SpinRow,
    ref_timing: adw::ComboRow,
    ref_custom: Cell<bool>,
    ref_s60: adw::SpinRow,
    ref_s30: adw::SpinRow,
    ref_min: gtk::Label,
    ref_input: gtk::Label,
    // Gpu
    gpu_groups: [adw::PreferencesGroup; 2],
    gpu_profile: adw::ComboRow,
    gpu_follow: adw::SwitchRow,
    gpu_limits: Vec<(&'static str, adw::ExpanderRow, adw::SpinRow, adw::SpinRow)>,
    gpu_seen: RefCell<Vec<(String, u32, u32)>>,
    // Thermal (on the Performance page)
    th_group: adw::PreferencesGroup,
    th_surface: gtk::Label,
    th_cpu: gtk::Label,
    th_gpu: gtk::Label,
    th_throttle: gtk::Label,
    th_all: adw::ExpanderRow,
    th_rows: RefCell<Vec<(String, adw::ActionRow, gtk::Label)>>,
    // Torch / LED
    torch_group: adw::PreferencesGroup,
    torch_on: adw::SwitchRow,
    torch_level: adw::SpinRow,
    led_group: adw::PreferencesGroup,
    led_charge: adw::SwitchRow,
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
    and_hash: LongInfo,
    and_auth: adw::SwitchRow,
    and_row: adw::ActionRow,
    and_switch: gtk::Button,
    // Systems (multiboot)
    boot_group: adw::PreferencesGroup,
    boot_rows: RefCell<Vec<adw::ActionRow>>,
    boot_last: RefCell<BootState>,
    boot_banner: adw::Banner,
    // Diagnostics
    diag_crash: gtk::Label,
    diag_clean: gtk::Label,
    diag_export: gtk::Button,
    diag_path: LongInfo,
    diag_open_row: adw::ActionRow,
    diag_last: RefCell<Option<String>>,
    // About
    ab_version: gtk::Label,
    ab_kernel: LongInfo,
    ab_series: LongInfo,
    ab_features: LongInfo,
    ab_fw: adw::ExpanderRow,
    ab_fw_rows: RefCell<Vec<adw::ActionRow>>,
    ab_fw_last: RefCell<Option<Vec<(String, String)>>>,
    about_debug: RefCell<String>,
    // Kernel updates (About; a trial line on Systems)
    kn: KernelUi,
}

/// The kernel-update widgets: status, the available release with one action
/// button (Download -> Install… -> Restart Now), channel, daily check, a
/// banner for Keep (testing channel) or after an automatic rollback.
struct KernelUi {
    group: adw::PreferencesGroup,
    modules: gtk::Label,
    state: gtk::Label,
    avail: adw::ActionRow,
    notes: gtk::Button,
    action: gtk::Button,
    check: adw::ActionRow,
    check_btn: gtk::Button,
    channel: adw::ComboRow,
    auto: adw::SwitchRow,
    back: adw::ActionRow,
    helper: LongInfo,
    banner: adw::Banner,
    /// what the banner button does: "keep" or "dismiss"
    banner_kind: RefCell<&'static str>,
    /// what the action button does now: (verb, tag)
    next: RefCell<(&'static str, String)>,
    /// a long call is running (the poll keeps showing State and Progress)
    busy: Cell<bool>,
    local: gtk::Button,
    sys_group: adw::PreferencesGroup,
    sys_row: adw::ActionRow,
}

const CHANNELS: [&str; 2] = ["stable", "testing"];
const CHANNEL_LABELS: [&str; 2] = ["Stable", "Testing"];
const CHANNEL_NOTES: [&str; 2] = ["Builds the project marked stable", "Every build that passed the device checks"];

/// "kernel-t28" -> "t28" (the tag without the project prefix).
fn short_tag(t: &str) -> &str {
    t.strip_prefix("kernel-").unwrap_or(t)
}

fn content_page(title: &str, child: &impl IsA<gtk::Widget>, banner: Option<&adw::Banner>) -> adw::NavigationPage {
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    if let Some(b) = banner {
        tv.add_top_bar(b);
    }
    tv.set_content(Some(child));
    adw::NavigationPage::builder().title(title).tag(title).child(&tv).build()
}

impl Ui {
    fn new(app: &adw::Application) -> Rc<Self> {
        let toasts = adw::ToastOverlay::new();

        // Battery
        let (p_bat, b) = page_box();
        let g = group(&b, "Charging", "");
        let bat_limit = spin(&g, "Charge Limit (%)", "80% keeps the battery healthier", 20.0, 100.0, 5.0);
        let bat_bypass = switch(&g, "Bypass Charging", "Run from the charger, battery idle");
        let g = group(&b, "Status", "");
        let bat_state = info(&g, "State");
        let bat_cap = info(&g, "Charge");
        let bat_power = info(&g, "Power");
        let bat_cur = info(&g, "Current");
        let bat_volt = info(&g, "Voltage");
        let bat_temp = info(&g, "Temperature");
        let bat_health = info(&g, "Health");
        let bat_cycles = info(&g, "Charge Cycles");
        let bat_design = info(&g, "Design Capacity");
        let g = group(&b, "Charger", "");
        let bat_charger = info(&g, "Contract");
        let bat_input = info(&g, "Input (Measured)");

        // Display
        let (p_ref, b) = page_box();
        // the description follows the mode (sync_refresh_rows)
        let g = group(&b, "Refresh Rate", "");
        group_help(&g, "Adaptive: the panel stays in its 120 Hz mode. When nothing changes on screen the kernel lowers the rate, and any update or touch brings 120 Hz back at once.");
        let ref_group = g.clone();
        let ref_policy = combo(&g, "Mode", &REFRESH_POLICY_LABELS);
        let ref_rate = spin(&g, "Fixed Rate (Hz)", "", 30.0, 120.0, 30.0);
        let mut timing_labels: Vec<&str> = TIMINGS.iter().map(|t| t.1).collect();
        timing_labels.push("Custom");
        let ref_timing = combo(&g, "Timing", &timing_labels);
        let ref_s60 = spin(&g, "Before 60 Hz (s)", "", 0.1, 120.0, 0.1);
        ref_s60.set_digits(1);
        let ref_s30 = spin(&g, "Before 30 Hz (s)", "", 0.1, 120.0, 0.1);
        ref_s30.set_digits(1);
        let details = adw::ExpanderRow::builder().title("Details").build();
        let ref_min = {
            let row = adw::ActionRow::builder().title("Lowest Rate").build();
            let l = gtk::Label::new(Some("…"));
            l.add_css_class("dim-label");
            row.add_suffix(&l);
            details.add_row(&row);
            l
        };
        let ref_input = {
            let row = adw::ActionRow::builder().title("Input Wakes to 120 Hz").build();
            let l = gtk::Label::new(Some("…"));
            l.add_css_class("dim-label");
            row.add_suffix(&l);
            details.add_row(&row);
            l
        };
        g.add(&details);

        // Performance
        let (p_gpu, b) = page_box();
        let g = group(&b, "GPU", "");
        let gpu_group = g.clone();
        let gpu_profile = combo(&g, "GPU Profile", &GPU_PROFILE_LABELS);
        let gpu_follow = switch(&g, "Follow Power Mode", "");
        let g = group(&b, "Frequency Limits", "");
        let gpu_groups = [gpu_group, g.clone()];
        group_help(&g, "Lowest and highest GPU clock for each profile. Changes take effect when you press Apply.");
        let mut gpu_limits = Vec::new();
        let mut gpu_buttons = Vec::new();
        for (p, label) in GPU_PROFILES.into_iter().zip(GPU_PROFILE_LABELS) {
            let exp = adw::ExpanderRow::builder().title(label).subtitle("…").build();
            let lo = adw::SpinRow::with_range(100.0, 2000.0, 1.0);
            lo.set_title("Minimum (MHz)");
            let hi = adw::SpinRow::with_range(100.0, 2000.0, 1.0);
            hi.set_title("Maximum (MHz)");
            touch_spin(&lo);
            touch_spin(&hi);
            exp.add_row(&lo);
            exp.add_row(&hi);
            let apply = adw::ActionRow::builder().title("Apply Limits").build();
            let bt = gtk::Button::with_label("Apply");
            bt.set_valign(gtk::Align::Center);
            bt.set_tooltip_text(Some(&format!("Apply {label} limits")));
            bt.update_property(&[gtk::accessible::Property::Label(&format!("Apply {label} limits"))]);
            apply.add_suffix(&bt);
            apply.set_activatable_widget(Some(&bt));
            exp.add_row(&apply);
            g.add(&exp);
            gpu_limits.push((p, exp, lo, hi));
            gpu_buttons.push(bt);
        }
        let th_group = group(&b, "Temperature", "");
        group_help(&th_group, "Read-only. Throttling means the kernel is lowering the CPU or GPU clock to cool down.");
        let th_surface = info(&th_group, "Surface");
        let th_cpu = info(&th_group, "CPU");
        let th_gpu = info(&th_group, "GPU");
        let th_throttle = info(&th_group, "Throttling");
        let th_all = adw::ExpanderRow::builder().title("All Sensors").build();
        th_group.add(&th_all);

        // Torch & LED ring
        let (p_led, b) = page_box();
        let torch_group = group(&b, "Torch", "");
        group_help(&torch_group, "The rear camera light at torch brightness, never the flash.");
        let torch_on = switch(&torch_group, "Torch", "");
        let torch_level = spin(&torch_group, "Brightness", "", 1.0, 255.0, 1.0);
        let led_group = group(&b, "LED Ring", "");
        group_help(&led_group, "The RGB ring on the back: amber while charging, green when full or held at the limit, red when low.");
        let led_charge = switch(&led_group, "Charge Indicator", "");
        let led_bright = spin(&led_group, "Brightness", "", 1.0, 255.0, 1.0);
        let led_low = spin(&led_group, "Red Below (%)", "", 5.0, 50.0, 1.0);

        // USB
        let (p_usb, b) = page_box();
        let g = group(&b, "Wake", "");
        let usb_wake = switch(&g, "Wake from USB Devices", "Keyboard or mouse on USB-C wakes it");
        let g = group(&b, "Development", "Network link and root console over the USB cable.");
        group_help(&g, "Only turn this on for development: anyone with a cable gets a root console. Turning it on asks for authentication.");
        let usb_dev = switch(&g, "USB Developer Mode", "");

        // Emergency key
        let (p_ek, b) = page_box();
        let g = group(&b, "", "Hold both volume keys to restart into Android.");
        group_help(&g, "Works even when the desktop is frozen. Turning it off asks for authentication.");
        let ek_enabled = switch(&g, "Restart into Android with Volume Keys", "");
        let ek_hold = spin(&g, "Hold Time (s)", "", 3.0, 30.0, 1.0);

        // Android
        let (p_and, b) = page_box();
        let g = group(&b, "", "Android stays installed; Linux takes its boot slot.");
        group_help(&g, "Restarting into Android writes the Android boot image back into the boot slot. Linux returns when you flash it again.");
        let and_avail = info(&g, "Android Image");
        let and_hash = info_long(&g, "Image SHA-256", &toasts);
        let and_auth = switch(&g, "Require Authentication", "");
        let g = group(&b, "", "");
        let (and_row, and_switch) = button(&g, "Restart into Android", "", "Restart…");
        and_switch.add_css_class("destructive-action");

        // Systems (multiboot)
        let (p_boot, b) = page_box();
        // a kernel on trial: confirmed once a system runs 90 s (or Keep)
        let kn_sys_group = group(&b, "", "");
        let kn_sys_row = adw::ActionRow::builder().title("Trying a New Kernel").build();
        kn_sys_row.set_subtitle_lines(2);
        kn_sys_row.add_prefix(&gtk::Image::from_icon_name("software-update-available-symbolic"));
        kn_sys_group.add(&kn_sys_row);
        kn_sys_group.set_visible(false);
        let boot_group = group(&b, "", "Restarting into one boots it once.");
        let rescan = gtk::Button::from_icon_name("view-refresh-symbolic");
        rescan.add_css_class("flat");
        rescan.set_valign(gtk::Align::Center);
        rescan.set_tooltip_text(Some("Look for systems again"));
        boot_group.set_header_suffix(Some(&rescan));
        let boot_banner = adw::Banner::new("");
        boot_banner.set_button_label(Some("Cancel"));

        // Diagnostics
        let (p_diag, b) = page_box();
        let g = group(&b, "Crash Records", "");
        let diag_crash = info(&g, "Stored Records");
        let diag_clean = info(&g, "Last Boot");
        let g = group(&b, "Export", "Crash records, the last boot's log and versions, personal data removed.");
        let (_, diag_export) = button(&g, "Export Diagnostics", "", "Export");
        let diag_path = info_long(&g, "Archive", &toasts);
        diag_path.row.set_subtitle_lines(1);
        diag_path.row.set_visible(false);
        let (diag_open_row, diag_open) = button(&g, "Show in Files", "", "Open Folder");
        diag_open_row.set_visible(false);

        // About
        let (p_about, b) = page_box();
        let g = group(&b, "Versions", "");
        let ab_version = info(&g, "Helper");
        let ab_app = info(&g, "App");
        set_text(&ab_app, env!("CARGO_PKG_VERSION"));
        let ab_kernel = info_long(&g, "Kernel", &toasts);
        let ab_series = info_long(&g, "Patch Series", &toasts);
        let ab_features = info_long(&g, "Features", &toasts);
        let g = group(&b, "Firmware", "Files from your tablet, checked against the manifest.");
        let ab_fw = adw::ExpanderRow::builder().title("Firmware Files").build();
        g.add(&ab_fw);
        let kn_group = group(&b, "Kernel Updates", "Releases of the project on GitHub, packed into your own boot image.");
        group_help(&kn_group, "A new kernel is tried on the next start. Once a system has run with it for 90 seconds it is kept \
            (testing channel and kernels from a file: when you press Keep). If it does not get that far twice, the previous \
            kernel comes back by itself. A kernel from a file takes the same way, but nothing checks who built it.");
        let kn_modules = info(&kn_group, "Kernel Modules");
        let kn_state = info(&kn_group, "Status");
        let kn_avail = adw::ActionRow::builder().title("New Kernel").build();
        kn_avail.set_subtitle_lines(1);
        let kn_notes = gtk::Button::with_label("Notes");
        kn_notes.set_valign(gtk::Align::Center);
        kn_notes.add_css_class("flat");
        let kn_action = gtk::Button::with_label("Download");
        kn_action.set_valign(gtk::Align::Center);
        kn_action.add_css_class("suggested-action");
        kn_avail.add_suffix(&kn_notes);
        kn_avail.add_suffix(&kn_action);
        kn_avail.set_visible(false);
        kn_group.add(&kn_avail);
        let (kn_check, kn_check_btn) = button(&kn_group, "Last Check", "", "Check Now");
        let kn_channel = combo(&kn_group, "Channel", &CHANNEL_LABELS);
        kn_channel.set_subtitle(CHANNEL_NOTES[0]);
        let kn_auto = switch(&kn_group, "Check Daily", "Never downloads by itself");
        let (_, kn_local_btn) = button(&kn_group, "Install Kernel from File", "An Image, Image.gz or boot image you built", "Choose…");
        let (kn_back, kn_back_btn) = button(&kn_group, "Previous Kernel", "", "Go Back…");
        kn_back_btn.add_css_class("destructive-action");
        kn_back.set_visible(false);
        let kn_helper = info_long(&kn_group, "Helper Update", &toasts);
        kn_helper.row.set_visible(false);
        let kn_banner = adw::Banner::new("");

        // Navigation
        let sidebar = gtk::ListBox::new();
        sidebar.add_css_class("navigation-sidebar");
        sidebar.set_selection_mode(gtk::SelectionMode::Single);
        let defs: [(&str, &str, &'static [&'static str], &gtk::ScrolledWindow, Option<&adw::Banner>); 10] = [
            ("Battery", "battery-good-symbolic", &["Battery"], &p_bat, None),
            ("Display", "video-display-symbolic", &["Refresh"], &p_ref, None),
            ("Performance", "power-profile-balanced-symbolic", &["Gpu", "Thermal"], &p_gpu, None),
            ("Torch & LED Ring", "display-brightness-symbolic", &["Torch", "LedRing"], &p_led, None),
            ("USB", "media-removable-symbolic", &["Usb"], &p_usb, None),
            ("Emergency Key", "dialog-warning-symbolic", &["EmergencyKey"], &p_ek, None),
            ("Systems", "drive-multidisk-symbolic", &["Boot"], &p_boot, Some(&boot_banner)),
            ("Android", "system-reboot-symbolic", &["Android"], &p_and, None),
            ("Diagnostics", "preferences-system-details-symbolic", &["Diagnostics"], &p_diag, None),
            ("About", "help-about-symbolic", &[""], &p_about, Some(&kn_banner)),
        ];
        let mut pages = Vec::new();
        for (title, icon, objects, page, banner) in defs {
            let bx = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            bx.append(&gtk::Image::from_icon_name(icon));
            let l = gtk::Label::new(Some(title));
            l.set_xalign(0.0);
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            bx.append(&l);
            let row = gtk::ListBoxRow::builder().child(&bx).build();
            sidebar.append(&row);
            pages.push(Page { objects, row, nav: content_page(title, page, banner) });
        }
        let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&sidebar).build();
        let menu = gio::Menu::new();
        menu.append(Some("Keyboard Shortcuts"), Some("app.shortcuts"));
        menu.append(Some("About Open Device Helper"), Some("app.about"));
        let menu_btn = gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).primary(true).build();
        menu_btn.set_tooltip_text(Some("Main Menu"));
        let side_header = adw::HeaderBar::new();
        side_header.pack_end(&menu_btn);
        let side_tv = adw::ToolbarView::new();
        side_tv.add_top_bar(&side_header);
        side_tv.set_content(Some(&scroller));
        let side_page = adw::NavigationPage::builder().title("Tablet").child(&side_tv).build();

        let split = adw::NavigationSplitView::new();
        split.set_sidebar(Some(&side_page));
        split.set_content(Some(&pages[0].nav));
        split.set_min_sidebar_width(200.0);
        split.set_max_sidebar_width(240.0);
        split.set_sidebar_width_fraction(0.22);

        // No daemon: the whole window says so (not a blank sidebar).
        let retry = gtk::Button::with_label("Try Again");
        retry.add_css_class("pill");
        retry.add_css_class("suggested-action");
        retry.set_halign(gtk::Align::Center);
        let status = adw::StatusPage::builder()
            .icon_name("dialog-warning-symbolic")
            .title("Helper Service Not Running")
            .description("This app needs tb323fu-helperd. Start it with <tt>systemctl start tb323fu-helperd</tt>")
            .child(&retry)
            .build();
        let absent_tv = adw::ToolbarView::new();
        absent_tv.add_top_bar(&adw::HeaderBar::new());
        absent_tv.set_content(Some(&status));

        let stack = gtk::Stack::new();
        stack.add_named(&split, Some("main"));
        stack.add_named(&absent_tv, Some("absent"));
        toasts.set_child(Some(&stack));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Open Device Helper")
            .default_width(900)
            .default_height(760)
            .content(&toasts)
            .build();
        // Narrow windows (below 860 sp: half of a landscape screen, or
        // portrait above 200 %) get list -> page navigation; a full portrait
        // window at 200 % (952 sp) and landscape keep the split view.
        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 860sp").unwrap());
        bp.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(bp);

        let ui = Rc::new(Ui {
            client: RefCell::new(Client::connect()),
            updating: Cell::new(false),
            polling: Cell::new(false),
            poll_again: Cell::new(false),
            pending: RefCell::new(HashMap::new()),
            timers: RefCell::new(HashMap::new()),
            window,
            toasts,
            stack,
            split,
            sidebar,
            pages,
            selected: Cell::new(None),
            daemon_up: Cell::new(None),
            bat_limit,
            bat_bypass,
            bat_state,
            bat_cap,
            bat_power,
            bat_cur,
            bat_volt,
            bat_temp,
            bat_health,
            bat_cycles,
            bat_design,
            bat_charger,
            bat_input,
            ref_group,
            ref_policy,
            ref_rate,
            ref_timing,
            ref_custom: Cell::new(false),
            ref_s60,
            ref_s30,
            ref_min,
            ref_input,
            gpu_groups,
            gpu_profile,
            gpu_follow,
            gpu_limits,
            gpu_seen: RefCell::new(Vec::new()),
            th_group,
            th_surface,
            th_cpu,
            th_gpu,
            th_throttle,
            th_all,
            th_rows: RefCell::new(Vec::new()),
            torch_group,
            torch_on,
            torch_level,
            led_group,
            led_charge,
            led_bright,
            led_low,
            usb_wake,
            usb_dev,
            ek_enabled,
            ek_hold,
            and_avail,
            and_hash,
            and_auth,
            and_row,
            and_switch,
            boot_group,
            boot_rows: RefCell::new(Vec::new()),
            boot_last: RefCell::new(BootState::default()),
            boot_banner,
            diag_crash,
            diag_clean,
            diag_export,
            diag_path,
            diag_open_row,
            diag_last: RefCell::new(None),
            ab_version,
            ab_kernel,
            ab_series,
            ab_features,
            ab_fw,
            ab_fw_rows: RefCell::new(Vec::new()),
            ab_fw_last: RefCell::new(None),
            about_debug: RefCell::new(String::new()),
            kn: KernelUi {
                group: kn_group,
                modules: kn_modules,
                state: kn_state,
                avail: kn_avail,
                notes: kn_notes,
                action: kn_action,
                check: kn_check,
                check_btn: kn_check_btn,
                channel: kn_channel,
                auto: kn_auto,
                back: kn_back,
                helper: kn_helper,
                banner: kn_banner,
                banner_kind: RefCell::new(""),
                next: RefCell::new(("", String::new())),
                busy: Cell::new(false),
                local: kn_local_btn,
                sys_group: kn_sys_group,
                sys_row: kn_sys_row,
            },
        });
        ui.connect(gpu_buttons, rescan, diag_open, retry);
        ui.connect_kernel(kn_back_btn);
        ui
    }

    fn toast(&self, msg: &str) {
        self.toasts.add_toast(adw::Toast::new(msg));
    }

    fn hold(&self, obj: &'static str) {
        *self.pending.borrow_mut().entry(obj).or_insert(0) += 1;
    }
    fn release(&self, obj: &'static str) {
        let mut p = self.pending.borrow_mut();
        if let Some(n) = p.get_mut(obj) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                p.remove(obj);
            }
        }
    }
    fn busy(&self, obj: &str) -> bool {
        self.pending.borrow().get(obj).is_some_and(|n| *n > 0)
    }

    /// Run a method call off the main thread; toast errors (with the setting's
    /// name), hand the result to `done`, then refresh.
    fn call_then<B, F>(self: &Rc<Self>, obj: &'static str, method: &'static str, body: B, done: F)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
        F: FnOnce(&Rc<Ui>, Result<Option<String>, String>) + 'static,
    {
        let client = self.client.borrow().clone();
        let ui = self.clone();
        self.hold(obj);
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(move || client.call(obj, method, &body))
                .await
                .unwrap_or_else(|_| Err("Something went wrong".into()));
            ui.release(obj);
            if let Err(e) = &res {
                ui.toast(&format!("{}: {e}", dbus::setting_name(method)));
            }
            done(&ui, res);
            ui.refresh();
        });
    }

    fn call_simple<B>(self: &Rc<Self>, obj: &'static str, method: &'static str, body: B)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    {
        self.call_then(obj, method, body, |_, _| {});
    }

    /// Run `f` 400 ms after the last change under `key` (holding a spin
    /// button's "+" sends one call). The object counts as busy meanwhile.
    fn debounce(self: &Rc<Self>, key: &'static str, obj: &'static str, f: impl FnOnce(&Rc<Ui>) + 'static) {
        let mut timers = self.timers.borrow_mut();
        if let Some(id) = timers.remove(key) {
            id.remove();
        } else {
            self.hold(obj);
        }
        let ui = self.clone();
        let id = glib::timeout_add_local_once(DEBOUNCE, move || {
            ui.timers.borrow_mut().remove(key);
            ui.release(obj);
            f(&ui);
        });
        timers.insert(key, id);
    }

    /// A spin row whose value (as u32) goes to `method` after the debounce.
    fn spin_sends(self: &Rc<Self>, r: &adw::SpinRow, key: &'static str, obj: &'static str, method: &'static str) {
        let ui = self.clone();
        r.connect_value_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            let v = r.value().round() as u32;
            ui.debounce(key, obj, move |ui| ui.call_simple(obj, method, (v,)));
        });
    }

    fn switch_sends(self: &Rc<Self>, r: &adw::SwitchRow, obj: &'static str, method: &'static str) {
        let ui = self.clone();
        r.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple(obj, method, (r.is_active(),));
            }
        });
    }

    /// Ask before a risky switch change; the object stays busy while the
    /// dialog is open, and Cancel puts the switch back.
    #[allow(clippy::too_many_arguments)]
    fn confirm_switch(self: &Rc<Self>, r: &adw::SwitchRow, obj: &'static str, heading: &str, body: &str, verb: &str,
        go: impl Fn(&Rc<Ui>) + 'static) {
        self.hold(obj);
        let d = adw::AlertDialog::new(Some(heading), Some(body));
        d.add_response("cancel", "Cancel");
        d.add_response("go", verb);
        d.set_response_appearance("go", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let ui = self.clone();
        let r = r.clone();
        d.connect_response(None, move |_, resp| {
            ui.release(obj);
            if resp == "go" {
                go(&ui);
            } else {
                ui.updating.set(true);
                r.set_active(!r.is_active());
                ui.updating.set(false);
            }
        });
        d.present(Some(&self.window));
    }

    fn connect(self: &Rc<Self>, gpu_buttons: Vec<gtk::Button>, rescan: gtk::Button, diag_open: gtk::Button, retry: gtk::Button) {
        // Navigation
        // The selected row decides the page, so the highlight always matches
        // what is shown; activating a row (a tap, also in collapsed mode)
        // selects it and brings the content forward.
        let ui = self.clone();
        self.sidebar.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let i = row.index();
            if i >= 0 {
                if let Some(p) = ui.pages.get(i as usize) {
                    ui.split.set_content(Some(&p.nav));
                    ui.selected.set(Some(i as usize));
                }
            }
        });
        let ui = self.clone();
        self.sidebar.connect_row_activated(move |lb, row| {
            if lb.selected_row().as_ref() != Some(row) {
                lb.select_row(Some(row));
            }
            ui.split.set_show_content(true);
        });
        let ui = self.clone();
        retry.connect_clicked(move |_| {
            *ui.client.borrow_mut() = Client::connect();
            ui.refresh_now();
            if ui.daemon_up.get() != Some(true) {
                ui.toast("Still not running");
            }
        });

        // Battery
        self.spin_sends(&self.bat_limit, "limit", "Battery", "SetChargeLimit");
        self.switch_sends(&self.bat_bypass, "Battery", "SetBypass");

        // Refresh
        let ui = self.clone();
        self.ref_policy.connect_selected_notify(move |r| {
            if !ui.updating.get() {
                let p = REFRESH_POLICIES[r.selected() as usize % REFRESH_POLICIES.len()];
                ui.call_simple("Refresh", "SetPolicy", (p.to_string(),));
            }
        });
        self.spin_sends(&self.ref_rate, "rate", "Refresh", "SetRate");
        let ui = self.clone();
        self.ref_timing.connect_selected_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            match TIMINGS.get(r.selected() as usize) {
                Some((id, ..)) => {
                    ui.ref_custom.set(false);
                    ui.call_simple("Refresh", "ApplyPreset", (id.to_string(),));
                }
                None => ui.ref_custom.set(true),
            }
            ui.sync_refresh_rows();
        });
        for spin in [&self.ref_s60, &self.ref_s30] {
            let ui = self.clone();
            spin.connect_value_notify(move |_| {
                // before-30 can never be shorter than before-60
                ui.ref_s30.adjustment().set_lower(ui.ref_s60.value());
                if ui.updating.get() {
                    return;
                }
                let ms = |v: f64| (v * 1000.0).round() as u32;
                let (a, b) = (ms(ui.ref_s60.value()), ms(ui.ref_s30.value()));
                ui.debounce("idle", "Refresh", move |ui| ui.call_simple("Refresh", "SetIdle", (a, b)));
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
        self.switch_sends(&self.gpu_follow, "Gpu", "SetFollowPowerProfiles");
        for (i, b) in gpu_buttons.into_iter().enumerate() {
            let (_, _, lo, hi) = &self.gpu_limits[i];
            // keep minimum <= maximum while editing
            let hi2 = hi.clone();
            lo.connect_value_notify(move |lo| hi2.adjustment().set_lower(lo.value()));
            let lo2 = lo.clone();
            hi.connect_value_notify(move |hi| lo2.adjustment().set_upper(hi.value()));
            let ui = self.clone();
            b.connect_clicked(move |_| {
                let (p, _, lo, hi) = &ui.gpu_limits[i];
                ui.call_simple("Gpu", "SetLimits", (p.to_string(), lo.value() as u32, hi.value() as u32));
            });
        }

        // Torch / LED
        self.switch_sends(&self.torch_on, "Torch", "Set");
        self.spin_sends(&self.torch_level, "torch", "Torch", "SetLevel");
        let ui = self.clone();
        self.led_charge.connect_active_notify(move |r| {
            if !ui.updating.get() {
                let m = if r.is_active() { "charge" } else { "off" };
                ui.call_simple("LedRing", "SetMode", (m.to_string(),));
            }
        });
        self.spin_sends(&self.led_bright, "ledb", "LedRing", "SetBrightness");
        self.spin_sends(&self.led_low, "ledlow", "LedRing", "SetLowPercent");

        // USB
        self.switch_sends(&self.usb_wake, "Usb", "SetWake");
        let ui = self.clone();
        self.usb_dev.connect_active_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            if !r.is_active() {
                ui.call_simple("Usb", "SetDevMode", (false,));
                return;
            }
            ui.confirm_switch(r, "Usb", "Turn On USB Developer Mode?",
                "Anyone with a USB cable to this tablet gets a network link and a root console. Only turn this on for development.",
                "Turn On", |ui| ui.call_simple("Usb", "SetDevMode", (true,)));
        });

        // Emergency key
        let ui = self.clone();
        self.ek_enabled.connect_active_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            if r.is_active() {
                ui.call_simple("EmergencyKey", "SetEnabled", (true,));
                return;
            }
            ui.confirm_switch(r, "EmergencyKey", "Turn Off the Emergency Key?",
                "If the desktop freezes you will need a forced restart (volume down + power).",
                "Turn Off", |ui| ui.call_simple("EmergencyKey", "SetEnabled", (false,)));
        });
        self.spin_sends(&self.ek_hold, "hold", "EmergencyKey", "SetHoldSeconds");

        // Android
        self.switch_sends(&self.and_auth, "Android", "SetRequireAuth");
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

        // Systems
        let ui = self.clone();
        self.boot_banner.connect_button_clicked(move |_| ui.call_simple("Boot", "ClearNext", ()));
        let ui = self.clone();
        rescan.connect_clicked(move |b| {
            b.set_sensitive(false);
            let b2 = b.clone();
            ui.call_then("Boot", "Rescan", (), move |_, _| b2.set_sensitive(true));
        });

        // Diagnostics
        let ui = self.clone();
        self.diag_export.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label("Exporting…");
            ui.call_then("Diagnostics", "Export", (), |ui, res| {
                ui.diag_export.set_sensitive(true);
                ui.diag_export.set_label("Export");
                if let Ok(Some(path)) = res {
                    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(path.clone());
                    ui.diag_path.set(&name, &path);
                    ui.diag_path.row.set_visible(true);
                    ui.diag_open_row.set_visible(true);
                    *ui.diag_last.borrow_mut() = Some(path);
                    let t = adw::Toast::builder().title("Diagnostics exported").button_label("Open Folder").build();
                    let ui2 = ui.clone();
                    t.connect_button_clicked(move |_| ui2.open_export());
                    ui.toasts.add_toast(t);
                }
            });
        });
        let ui = self.clone();
        diag_open.connect_clicked(move |_| ui.open_export());
    }

    fn open_export(self: &Rc<Self>) {
        if let Some(path) = self.diag_last.borrow().clone() {
            let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(&path)));
            let ui = self.clone();
            launcher.open_containing_folder(Some(&self.window), None::<&gio::Cancellable>, move |r| {
                if let Err(e) = r {
                    ui.toast(&format!("Could not open the folder: {e}"));
                }
            });
        }
    }

    fn about(self: &Rc<Self>) {
        let d = adw::AboutDialog::builder()
            .application_name("Open Device Helper")
            .application_icon(APP_ID)
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name("Joonhoe Kim")
            .license_type(gtk::License::MitX11)
            .website(REPO)
            .issue_url(format!("{REPO}/issues"))
            .comments("Settings for the Lenovo Legion Tab Gen 5 (TB323FU), through the tb323fu-helperd service.")
            .debug_info(self.about_debug.borrow().as_str())
            .build();
        d.present(Some(&self.window));
    }

    fn shortcuts(self: &Rc<Self>) {
        let d = adw::AlertDialog::new(Some("Keyboard Shortcuts"), Some("Ctrl+Q  Quit\nCtrl+W  Close the window"));
        d.add_response("ok", "OK");
        d.present(Some(&self.window));
    }

    fn poll_client(&self) -> (Client, Vec<&'static str>) {
        if !self.client.borrow().connected() {
            *self.client.borrow_mut() = Client::connect();
        }
        let skip = OBJECTS.iter().copied().filter(|o| self.busy(o)).collect();
        (self.client.borrow().clone(), skip)
    }

    /// Poll the daemon on a worker thread, then update the widgets. The GTK
    /// thread never waits for the daemon (a Boot rescan can take seconds);
    /// asks while a poll runs fold into one more poll after it.
    fn refresh(self: &Rc<Self>) {
        if self.polling.replace(true) {
            self.poll_again.set(true);
            return;
        }
        let (c, skip) = self.poll_client();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let f = gio::spawn_blocking(move || fetch(&c, &skip)).await;
            ui.polling.set(false);
            if let Ok(f) = f {
                ui.apply(f);
            }
            if ui.poll_again.replace(false) {
                ui.refresh();
            }
        });
    }

    /// Poll on the GTK thread (start-up and Try Again, where the answer is
    /// needed at once and nothing slow is running).
    fn refresh_now(self: &Rc<Self>) {
        let (c, skip) = self.poll_client();
        self.apply(fetch(&c, &skip));
    }

    /// Update every widget from a poll without triggering the change
    /// handlers. Objects with a call in flight are left alone.
    fn apply(self: &Rc<Self>, f: Fetched) {
        let Fetched { root, props } = f;
        let up = root.is_some();
        if self.daemon_up.get() != Some(up) {
            self.daemon_up.set(Some(up));
            self.stack.set_visible_child_name(if up { "main" } else { "absent" });
        }
        if !up {
            return;
        }
        self.updating.set(true);
        let features = root.as_ref().map(|r| dbus::strs(r, "Features")).unwrap_or_default();
        // presence from the Features list, so a busy object keeps its page
        let present = |o: &str| o.is_empty() || features.iter().any(|f| f == o);
        let had_selection = self.sidebar.selected_row().is_some_and(|r| r.is_visible());
        for p in &self.pages {
            p.row.set_visible(p.objects.iter().any(|o| present(o)));
        }
        if !had_selection {
            // first refresh or reconnect: back to the page the user was on
            let want = self.selected.get().and_then(|i| self.pages.get(i)).filter(|p| p.row.is_visible());
            if let Some(p) = want.or_else(|| self.pages.iter().find(|p| p.row.is_visible())) {
                self.sidebar.select_row(Some(&p.row));
            }
        }
        // an object that became busy while the poll ran keeps its widgets
        let p = |o: &str| if self.busy(o) { None } else { props.get(o).and_then(|x| x.as_ref()) };
        if let Some(b) = p("Battery") {
            self.update_battery(b);
        }
        if let Some(r) = p("Refresh") {
            self.update_refresh(r);
        }
        for g in &self.gpu_groups {
            g.set_visible(present("Gpu"));
        }
        if let Some(g) = p("Gpu") {
            self.update_gpu(g);
        }
        self.th_group.set_visible(present("Thermal"));
        if let Some(t) = p("Thermal") {
            self.update_thermal(t);
        }
        self.torch_group.set_visible(present("Torch"));
        if let Some(t) = p("Torch") {
            set_switch(&self.torch_on, dbus::b(t, "On"));
            if let Some(m) = dbus::u(t, "MaxLevel") {
                if m > 0 && (self.torch_level.adjustment().upper() - m as f64).abs() > 0.5 {
                    self.torch_level.adjustment().set_upper(m as f64);
                }
            }
            set_spin(&self.torch_level, dbus::u(t, "Level").map(f64::from));
        }
        self.led_group.set_visible(present("LedRing"));
        if let Some(l) = p("LedRing") {
            set_switch(&self.led_charge, dbus::s(l, "Mode").map(|m| m == "charge"));
            set_spin(&self.led_bright, dbus::u(l, "Brightness").map(f64::from));
            set_spin(&self.led_low, dbus::u(l, "LowPercent").map(f64::from));
        }
        if let Some(u) = p("Usb") {
            set_switch(&self.usb_wake, dbus::b(u, "WakeEnabled"));
            set_switch(&self.usb_dev, dbus::b(u, "DevMode"));
        }
        if let Some(e) = p("EmergencyKey") {
            set_switch(&self.ek_enabled, dbus::b(e, "Enabled"));
            set_spin(&self.ek_hold, dbus::u(e, "HoldSeconds").map(f64::from));
        }
        if let Some(a) = p("Android") {
            let avail = dbus::b(a, "Available").unwrap_or(false);
            set_text(&self.and_avail, if avail { "Ready" } else { "Not Set Up" });
            let h = dbus::s(a, "ImageSha256").unwrap_or_default();
            let short = if h.len() > 12 { format!("{}…", &h[..12]) } else if h.is_empty() { "Unknown".into() } else { h.clone() };
            self.and_hash.set(&short, &h);
            set_switch(&self.and_auth, dbus::b(a, "RequireAuth"));
            self.and_switch.set_visible(avail);
            let sub = if avail { "Restarts now" } else { "Android image not set up" };
            if self.and_row.subtitle().as_deref() != Some(sub) {
                self.and_row.set_subtitle(sub);
            }
        }
        if let Some(d) = p("Diagnostics") {
            set_text(&self.diag_crash, &dbus::u(d, "CrashRecords").map(|n| n.to_string()).unwrap_or_else(|| "Unknown".into()));
            let clean = dbus::b(d, "LastBootClean");
            set_text(&self.diag_clean, match clean {
                Some(true) => "Clean",
                Some(false) => "Crashed",
                None => "Unknown",
            });
            set_class(&self.diag_clean, "warning", clean == Some(false));
            set_class(&self.diag_clean, "dim-label", clean != Some(false));
        }
        if let Some(b) = p("Boot") {
            self.update_boot(b);
        }
        if let Some(r) = &root {
            self.update_about(r);
        }
        self.kn.group.set_visible(present("Kernel"));
        if let Some(k) = props.get("Kernel").and_then(|x| x.as_ref()) {
            self.update_kernel(k);
        } else if !present("Kernel") {
            self.kn.banner.set_revealed(false);
            self.kn.sys_group.set_visible(false);
        }
        self.updating.set(false);
    }

    fn update_boot(self: &Rc<Self>, p: &Props) {
        let roots = dbus::roots(p, "Roots");
        let mut health: Vec<(String, Vec<String>)> = dbus::dict_sas(p, "RootHealth").into_iter().collect();
        health.sort();
        let state = BootState {
            cur: dbus::s(p, "Current").unwrap_or_default(),
            def: dbus::s(p, "Default").unwrap_or_default(),
            next: dbus::s(p, "Next").unwrap_or_default(),
            roots,
            health,
        };
        let name_of = |n: &str| {
            state.roots.iter().find(|r| r.0 == n).map(|r| if r.1.is_empty() { r.0.clone() } else { r.1.clone() }).unwrap_or(n.to_string())
        };
        let show_banner = !state.next.is_empty();
        if show_banner {
            let t = format!("Next restart starts {}", name_of(&state.next));
            if self.boot_banner.title() != t {
                self.boot_banner.set_title(&t);
            }
        }
        if self.boot_banner.is_revealed() != show_banner {
            self.boot_banner.set_revealed(show_banner);
        }
        if *self.boot_last.borrow() == state {
            return;
        }
        for r in self.boot_rows.borrow_mut().drain(..) {
            self.boot_group.remove(&r);
        }
        for (name, label, _present, init) in &state.roots {
            let title = if label.is_empty() { name.clone() } else { label.clone() };
            let problems = state.health.iter().find(|h| h.0 == *name).map(|h| h.1.clone()).unwrap_or_default();
            let row = adw::ActionRow::builder().title(title.as_str()).build();
            row.set_title_lines(1);
            row.set_subtitle_lines(1);
            let installed = init != "none";
            if !installed {
                row.set_subtitle("No system installed");
                row.add_css_class("dim-label");
            } else if !problems.is_empty() {
                // the partition name stays first: two roots can share a label
                let mut short: Vec<String> = Vec::new();
                for p in &problems {
                    let s = short_problem(p);
                    if !short.contains(&s) {
                        short.push(s);
                    }
                }
                row.set_subtitle(&format!("{name} · {}", short.join(", ")));
                let warn = gtk::Image::from_icon_name("dialog-warning-symbolic");
                warn.add_css_class("warning");
                row.add_prefix(&warn);
                row.set_tooltip_text(Some(&format!("{name}\n{}", problems.iter().map(|p| capitalize(p)).collect::<Vec<_>>().join("\n"))));
            } else {
                row.set_subtitle(name);
            }
            let (running, is_def, is_next) = (*name == state.cur, *name == state.def, *name == state.next);
            if running {
                row.add_suffix(&tag("Running", "success"));
            }
            if is_def {
                row.add_suffix(&tag("Default", "accent"));
            }
            if is_next {
                row.add_suffix(&tag("Next", "accent"));
            }
            if installed {
                let menu = gio::Menu::new();
                let actions = gio::SimpleActionGroup::new();
                if !running {
                    menu.append(Some("Restart Now…"), Some("row.restart"));
                    let a = gio::SimpleAction::new("restart", None);
                    let (ui, n, t, pr) = (self.clone(), name.clone(), title.clone(), problems.clone());
                    a.connect_activate(move |_, _| ui.confirm_restart(&n, &t, &pr));
                    actions.add_action(&a);
                    if !is_next {
                        menu.append(Some("Start on Next Restart"), Some("row.next"));
                        let a = gio::SimpleAction::new("next", None);
                        let (ui, n) = (self.clone(), name.clone());
                        a.connect_activate(move |_, _| ui.call_simple("Boot", "SetNext", (n.clone(),)));
                        actions.add_action(&a);
                    }
                }
                if !is_def {
                    menu.append(Some("Make Default"), Some("row.default"));
                    let a = gio::SimpleAction::new("default", None);
                    let (ui, n) = (self.clone(), name.clone());
                    a.connect_activate(move |_, _| ui.call_simple("Boot", "SetDefault", (n.clone(),)));
                    actions.add_action(&a);
                }
                if menu.n_items() > 0 {
                    let mb = gtk::MenuButton::builder().icon_name("view-more-symbolic").menu_model(&menu).valign(gtk::Align::Center).build();
                    mb.add_css_class("flat");
                    mb.set_tooltip_text(Some("Actions"));
                    mb.update_property(&[gtk::accessible::Property::Label(&format!("Actions for {title}"))]);
                    row.insert_action_group("row", Some(&actions));
                    row.add_suffix(&mb);
                    // a tap anywhere on a system that is not running opens
                    // its menu (touch); the running row only has the button
                    if !running {
                        row.set_activatable(true);
                        let mb2 = mb.clone();
                        row.connect_activated(move |_| mb2.popup());
                    }
                }
            }
            self.boot_group.add(&row);
            self.boot_rows.borrow_mut().push(row);
        }
        *self.boot_last.borrow_mut() = state;
    }

    fn confirm_restart(self: &Rc<Self>, name: &str, title: &str, problems: &[String]) {
        let mut body = String::from("The tablet restarts now and boots this system once. Unsaved work is lost.");
        if !problems.is_empty() {
            body.push_str("\n\nThis system has problems:\n");
            body.push_str(&problems.iter().map(|p| format!("• {}", capitalize(p))).collect::<Vec<_>>().join("\n"));
        }
        let d = adw::AlertDialog::new(Some(&format!("Restart into {title}?")), Some(&body));
        d.add_response("cancel", "Cancel");
        d.add_response("go", "Restart");
        d.set_response_appearance("go", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let ui = self.clone();
        let n = name.to_string();
        d.connect_response(None, move |_, resp| {
            if resp == "go" {
                ui.call_simple("Boot", "RebootInto", (n.clone(),));
            }
        });
        d.present(Some(&self.window));
    }

    /// A long Kernel call (check, download, install, ...) on a worker thread.
    /// The object is not held busy, so the poll keeps showing State and
    /// Progress meanwhile; the buttons are off instead.
    fn kernel_call<B>(self: &Rc<Self>, method: &'static str, body: B, done: impl FnOnce(&Rc<Ui>, &Result<Option<String>, String>) + 'static)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    {
        if self.kn.busy.replace(true) {
            return;
        }
        self.kernel_sensitive(false);
        let client = self.client.borrow().clone();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(move || client.call("Kernel", method, &body))
                .await
                .unwrap_or_else(|_| Err("Something went wrong".into()));
            ui.kn.busy.set(false);
            ui.kernel_sensitive(true);
            match &res {
                Ok(Some(m)) if !m.is_empty() => ui.toast(m),
                Err(e) => ui.toast(&format!("{}: {e}", dbus::setting_name(method))),
                _ => {}
            }
            done(&ui, &res);
            ui.refresh();
        });
    }

    fn kernel_sensitive(&self, on: bool) {
        for b in [&self.kn.action, &self.kn.check_btn, &self.kn.local] {
            b.set_sensitive(on);
        }
        self.kn.banner.set_sensitive(on);
    }

    fn connect_kernel(self: &Rc<Self>, back_btn: gtk::Button) {
        let ui = self.clone();
        self.kn.check_btn.connect_clicked(move |_| ui.kernel_call("Check", (), |_, _| {}));
        let ui = self.clone();
        self.kn.action.connect_clicked(move |_| {
            let (verb, tag) = ui.kn.next.borrow().clone();
            match verb {
                "download" => ui.kernel_call("Download", (tag,), |_, _| {}),
                "install" => ui.confirm_install(&tag),
                "restart" => {
                    let cur = ui.boot_last.borrow().cur.clone();
                    let d = adw::AlertDialog::new(Some("Restart Now?"),
                        Some("The tablet restarts with the new kernel. Unsaved work is lost."));
                    d.add_response("cancel", "Cancel");
                    d.add_response("go", "Restart");
                    d.set_response_appearance("go", adw::ResponseAppearance::Suggested);
                    d.set_default_response(Some("go"));
                    d.set_close_response("cancel");
                    let ui2 = ui.clone();
                    d.connect_response(None, move |_, r| {
                        if r == "go" && !cur.is_empty() {
                            ui2.call_simple("Boot", "RebootInto", (cur.clone(),));
                        } else if r == "go" {
                            ui2.toast("Restart the tablet to use the new kernel");
                        }
                    });
                    d.present(Some(&ui.window));
                }
                _ => {}
            }
        });
        let ui = self.clone();
        self.kn.notes.connect_clicked(move |_| {
            let tag = ui.kn.next.borrow().1.clone();
            ui.show_notes(&tag);
        });
        let ui = self.clone();
        self.kn.channel.connect_selected_notify(move |r| {
            r.set_subtitle(CHANNEL_NOTES[r.selected() as usize % CHANNEL_NOTES.len()]);
            if !ui.updating.get() {
                let c = CHANNELS[r.selected() as usize % CHANNELS.len()];
                ui.call_simple("Kernel", "SetChannel", (c.to_string(),));
            }
        });
        self.switch_sends(&self.kn.auto, "Kernel", "SetAutoCheck");
        let ui = self.clone();
        self.kn.banner.connect_button_clicked(move |_| {
            let kind = *ui.kn.banner_kind.borrow();
            match kind {
                "keep" => ui.kernel_call("Keep", (), |_, _| {}),
                "dismiss" => ui.kernel_call("Dismiss", (), |_, _| {}),
                _ => {}
            }
        });
        let ui = self.clone();
        self.kn.local.connect_clicked(move |_| ui.choose_kernel_file());
        let ui = self.clone();
        back_btn.connect_clicked(move |_| {
            let d = adw::AlertDialog::new(Some("Go Back to the Previous Kernel?"),
                Some("The last kernel that was kept goes back into the boot slot; the new one is recorded as failed. It is used from the next start."));
            d.add_response("cancel", "Cancel");
            d.add_response("go", "Go Back");
            d.add_response("reboot", "Go Back and Restart");
            d.set_response_appearance("reboot", adw::ResponseAppearance::Destructive);
            d.set_default_response(Some("cancel"));
            d.set_close_response("cancel");
            let ui2 = ui.clone();
            d.connect_response(None, move |_, r| match r {
                "go" => ui2.kernel_call("Rollback", (false,), |_, _| {}),
                "reboot" => ui2.kernel_call("Rollback", (true,), |_, _| {}),
                _ => {}
            });
            d.present(Some(&ui.window));
        });
    }

    fn confirm_install(self: &Rc<Self>, tag: &str) {
        let d = adw::AlertDialog::new(Some(&format!("Install Kernel {}?", short_tag(tag))),
            Some("It is packed into your own stock boot image and written to the boot slot, then tried on the next start. \
                If it does not bring a system up twice, the previous kernel comes back by itself."));
        d.add_response("cancel", "Cancel");
        d.add_response("install", "Install");
        d.add_response("reboot", "Install and Restart");
        d.set_response_appearance("reboot", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("reboot"));
        d.set_close_response("cancel");
        let (ui, t) = (self.clone(), tag.to_string());
        d.connect_response(None, move |_, r| match r {
            "install" => ui.kernel_call("Install", (t.clone(), false), |_, _| {}),
            "reboot" => ui.kernel_call("Install", (t.clone(), true), |_, _| {}),
            _ => {}
        });
        d.present(Some(&self.window));
    }

    /// "Install Kernel from File…": pick a file, let the daemon look at it,
    /// show what it found, then install it (the administrator's password).
    fn choose_kernel_file(self: &Rc<Self>) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Kernels and boot images"));
        for p in ["Image*", "*.gz", "*.img", "vmlinuz*"] {
            filter.add_pattern(p);
        }
        let all = gtk::FileFilter::new();
        all.set_name(Some("All files"));
        all.add_pattern("*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        filters.append(&all);
        let d = gtk::FileDialog::builder().title("Choose a Kernel").modal(true).filters(&filters).build();
        let ui = self.clone();
        d.open(Some(&self.window), gio::Cancellable::NONE, move |res| {
            let Ok(file) = res else { return };
            match file.path() {
                Some(p) => ui.inspect_kernel_file(p),
                None => ui.toast("Choose a file on this tablet"),
            }
        });
    }

    fn inspect_kernel_file(self: &Rc<Self>, path: std::path::PathBuf) {
        let client = self.client.borrow().clone();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let p = path.clone();
            let res = gio::spawn_blocking(move || {
                let f = std::fs::File::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                client.inspect_local(&f)
            })
            .await
            .unwrap_or_else(|_| Err("Something went wrong".into()));
            match res {
                Ok(info) => ui.confirm_local(path, info),
                Err(e) => ui.toast(&format!("Kernel from file: {e}")),
            }
        });
    }

    fn confirm_local(self: &Rc<Self>, path: std::path::PathBuf, info: dbus::LocalKernel) {
        let (release, banner, format, shared, warnings) = info;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let line = |t: &str, dim: bool| {
            let l = gtk::Label::new(Some(t));
            l.set_wrap(true);
            l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            l.set_xalign(0.0);
            l.set_selectable(true);
            l.set_focusable(false);
            if dim {
                l.add_css_class("dim-label");
            }
            body.append(&l);
        };
        line(&format!("{name} ({format})"), true);
        line(&banner, true);
        line(if shared { "Carries its modules: every system gets them." } else { "Carries no modules." }, false);
        for w in &warnings {
            line(&format!("⚠ {w}"), false);
        }
        line("Not from the project's releases: nothing checks who built it. It is tried like an update — if it does not bring a \
            system up twice, the previous kernel comes back. A kernel that stops before its own start-up screen needs fastboot or EDL.", true);
        let auto = gtk::CheckButton::with_label("Keep it by itself once a system has run 90 seconds");
        body.append(&auto);
        let sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).max_content_height(420)
            .propagate_natural_height(true).child(&body).build();
        let d = adw::AlertDialog::new(Some(&format!("Install Kernel {release}?")), None);
        d.set_extra_child(Some(&sw));
        d.add_response("cancel", "Cancel");
        d.add_response("install", "Install");
        d.add_response("reboot", "Install and Restart");
        d.set_response_appearance("reboot", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let ui = self.clone();
        d.connect_response(None, move |_, r| {
            if r == "install" || r == "reboot" {
                ui.install_local(path.clone(), auto.is_active(), r == "reboot");
            }
        });
        d.present(Some(&self.window));
    }

    fn install_local(self: &Rc<Self>, path: std::path::PathBuf, auto_confirm: bool, reboot: bool) {
        if self.kn.busy.replace(true) {
            return;
        }
        self.kernel_sensitive(false);
        let client = self.client.borrow().clone();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(move || {
                let f = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                client.call("Kernel", "InstallLocal", &(zbus::zvariant::Fd::from(&f), name.as_str(), auto_confirm, reboot))
            })
            .await
            .unwrap_or_else(|_| Err("Something went wrong".into()));
            ui.kn.busy.set(false);
            ui.kernel_sensitive(true);
            match &res {
                Ok(Some(m)) if !m.is_empty() => ui.toast(m),
                Err(e) => ui.toast(&format!("Kernel from file: {e}")),
                _ => {}
            }
            ui.refresh();
        });
    }

    /// The release notes (Markdown, the release body on GitHub), as text.
    fn show_notes(self: &Rc<Self>, tag: &str) {
        let t = tag.to_string();
        self.call_then("Kernel", "Notes", (t.clone(),), move |ui, res| {
            let Ok(Some(text)) = res else { return };
            let l = gtk::Label::new(Some(if text.is_empty() { "No release notes." } else { text.as_str() }));
            l.set_wrap(true);
            l.set_xalign(0.0);
            l.set_yalign(0.0);
            l.set_selectable(true);
            let sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).min_content_height(240)
                .max_content_height(480).propagate_natural_height(true).child(&l).build();
            let d = adw::AlertDialog::new(Some(&format!("Kernel {}", short_tag(&t))), None);
            d.set_extra_child(Some(&sw));
            d.add_response("ok", "Close");
            d.present(Some(&ui.window));
        });
    }

    fn update_kernel(&self, p: &Props) {
        let s = |k: &str| dbus::s(p, k).unwrap_or_default();
        let state = s("State");
        let (trial, good, failed) = (s("Trial"), s("Good"), s("LastFailed"));
        let avail = dbus::releases(p, "Available");
        let downloaded = dbus::strs(p, "Downloaded");
        let keep = dbus::b(p, "KeepPending").unwrap_or(false);
        let (tries, max) = (dbus::u(p, "Tries").unwrap_or(0), dbus::u(p, "MaxTries").unwrap_or(2));
        set_text(&self.kn.modules, if dbus::b(p, "SharedModules") == Some(true) { "From the boot image" } else { "From this system" });
        let label = match state.as_str() {
            "checking" => "Checking…".to_string(),
            "downloading" => format!("Downloading… {} %", dbus::u(p, "Progress").unwrap_or(0)),
            "verifying" => "Verifying…".into(),
            "installing" => "Installing…".into(),
            "rolling-back" => "Going back…".into(),
            "keeping" => "Keeping…".into(),
            "ready" => "Ready to install".into(),
            "pending-reboot" => format!("Restart to try {trial}"),
            "trial" => format!("Trying {trial}, start {tries} of {max}"),
            "rolled-back" => format!("{failed} did not start"),
            _ if dbus::b(p, "IndexExpired") == Some(true) => "Release list expired".into(),
            _ if !avail.is_empty() => "Update available".into(),
            _ if dbus::t(p, "LastCheck").unwrap_or(0) == 0 => "Not checked yet".into(),
            _ => "Up to date".into(),
        };
        set_text(&self.kn.state, &label);
        set_class(&self.kn.state, "warning", state == "rolled-back" || state == "trial");

        // the action: download, install, restart
        let next: (&'static str, String) = if state == "pending-reboot" {
            ("restart", String::new())
        } else if let Some(a) = avail.first() {
            (if downloaded.contains(&a.0) { "install" } else { "download" }, a.0.clone())
        } else {
            ("", String::new())
        };
        let show = !next.0.is_empty();
        if self.kn.avail.is_visible() != show {
            self.kn.avail.set_visible(show);
        }
        if show {
            let (title, sub) = match (next.0, avail.first()) {
                ("restart", _) => ("Kernel Installed".to_string(), format!("{trial} is tried on the next start")),
                (_, Some(a)) => (format!("Kernel {} Available", short_tag(&a.0)), a.1.clone()),
                _ => (String::new(), String::new()),
            };
            if self.kn.avail.title() != title {
                self.kn.avail.set_title(&title);
            }
            if self.kn.avail.subtitle().as_deref() != Some(sub.as_str()) {
                self.kn.avail.set_subtitle(&sub);
            }
            let btn = match next.0 {
                "download" => "Download",
                "install" => "Install…",
                _ => "Restart Now",
            };
            if self.kn.action.label().as_deref() != Some(btn) {
                self.kn.action.set_label(btn);
            }
            self.kn.notes.set_visible(next.0 != "restart");
        }
        *self.kn.next.borrow_mut() = next;

        let last = dbus::t(p, "LastCheck").unwrap_or(0);
        let when = if last == 0 {
            "Never".to_string()
        } else {
            glib::DateTime::from_unix_local(last as i64).ok().and_then(|d| d.format("%Y-%m-%d %H:%M").ok()).map(|g| g.to_string()).unwrap_or_default()
        };
        if self.kn.check.subtitle().as_deref() != Some(when.as_str()) {
            self.kn.check.set_subtitle(&when);
        }
        if !self.busy("Kernel") {
            set_combo(&self.kn.channel, &CHANNELS, dbus::s(p, "Channel"));
            set_switch(&self.kn.auto, dbus::b(p, "AutoCheck"));
        }
        let back = !trial.is_empty() && !good.is_empty();
        self.kn.back.set_visible(back);
        if back {
            let sub = format!("Back to {good}");
            if self.kn.back.subtitle().as_deref() != Some(sub.as_str()) {
                self.kn.back.set_subtitle(&sub);
            }
        }
        let hl = s("HelperLatest");
        self.kn.helper.row.set_visible(!hl.is_empty());
        if !hl.is_empty() {
            let cmd = s("HelperUpdateCommand");
            self.kn.helper.row.set_title(&format!("Helper {hl} Available"));
            self.kn.helper.set(&cmd, &cmd);
        }

        // banner on About: Keep (testing channel) or the rollback notice
        let (kind, text, btn): (&'static str, String, &str) = if keep {
            ("keep", format!("Trying kernel {trial}: keep it if everything works"), "Keep")
        } else if state == "rolled-back" {
            ("dismiss", format!("Kernel {failed} did not start twice; back on {good}"), "Dismiss")
        } else {
            ("", String::new(), "")
        };
        *self.kn.banner_kind.borrow_mut() = kind;
        if !kind.is_empty() {
            if self.kn.banner.title() != text {
                self.kn.banner.set_title(&text);
            }
            self.kn.banner.set_button_label(Some(btn));
        }
        if self.kn.banner.is_revealed() != !kind.is_empty() {
            self.kn.banner.set_revealed(!kind.is_empty());
        }

        // Systems: a line on top while a kernel is on trial
        let on_trial = state == "trial";
        self.kn.sys_group.set_visible(on_trial);
        if on_trial {
            let sub = if keep {
                format!("{trial}: kept when you press Keep (About), else back to {good} after {max} starts")
            } else {
                format!("{trial}: kept once a system has run 90 s with it")
            };
            if self.kn.sys_row.subtitle().as_deref() != Some(sub.as_str()) {
                self.kn.sys_row.set_subtitle(&sub);
            }
        }
    }

    fn update_battery(&self, p: &Props) {
        set_spin(&self.bat_limit, dbus::u(p, "ChargeLimit").map(f64::from));
        let bypass = dbus::b(p, "Bypass");
        set_switch(&self.bat_bypass, bypass);
        let state = dbus::s(p, "State").unwrap_or_default();
        let st = match state.as_str() {
            "charging" => "Charging",
            "discharging" => "Discharging",
            "bypass" if bypass == Some(true) => "Bypass",
            "bypass" => "Held at Limit",
            "full" => "Full",
            "not-charging" => "Not Charging",
            _ => "Unknown",
        };
        let ma = dbus::i(p, "CurrentMa");
        // plugged in but drawing more than the charger gives: say so, so
        // State and Current never disagree
        let st = if state == "charging" && ma.is_some_and(|c| c < 0) { "Plugged In, Draining" } else { st };
        set_text(&self.bat_state, st);
        set_text(&self.bat_cap, &dbus::u(p, "Capacity").map(|c| format!("{c}%")).unwrap_or_else(|| "Unknown".into()));
        let mv = dbus::u(p, "VoltageMv");
        set_text(&self.bat_cur, &match ma {
            Some(0) => "0 mA".into(),
            Some(c) => {
                let dir = if c > 0 { "into battery" } else { "from battery" };
                let a = c.unsigned_abs();
                if a >= 1000 { format!("{:.1} A {dir}", a as f64 / 1000.0) } else { format!("{a} mA {dir}") }
            }
            None => "Unknown".into(),
        });
        set_text(&self.bat_power, &match (ma, mv) {
            (Some(c), Some(v)) => format!("{:.1} W", (c as f64 * v as f64 / 1e6).abs()),
            _ => "Unknown".into(),
        });
        set_text(&self.bat_volt, &mv.map(|v| format!("{:.2} V", v as f64 / 1000.0)).unwrap_or_else(|| "Unknown".into()));
        set_text(&self.bat_temp, &degrees(dbus::f(p, "TemperatureC")));
        set_text(&self.bat_health, &dbus::s(p, "Health").map(|h| capitalize(&h)).unwrap_or_else(|| "Unknown".into()));
        // unknown values (-1) hide their row instead of saying "unknown"
        for (l, v, unit) in [(&self.bat_cycles, dbus::i(p, "CycleCount"), ""), (&self.bat_design, dbus::i(p, "DesignCapacityMah"), " mAh")] {
            let known = v.is_some_and(|x| x >= 0);
            if let Some(r) = row_of(l) {
                r.set_visible(known);
            }
            if let Some(x) = v.filter(|x| *x >= 0) {
                set_text(l, &format!("{x}{unit}"));
            }
        }
        let ty = dbus::s(p, "ChargerType").unwrap_or_default();
        let contract = dbus::s(p, "ChargerContract").unwrap_or_default();
        let none = |s: &str| s.is_empty() || s == "none";
        set_text(&self.bat_charger, if none(&contract) && none(&ty) {
            "Not Connected"
        } else if none(&contract) || contract == "unknown" {
            ty.as_str()
        } else {
            contract.as_str()
        });
        // measured charger input (battmgr), also when UCSI reports no contract
        let (imv, ima) = (dbus::u(p, "InputVoltageMv").unwrap_or(0), dbus::u(p, "InputCurrentMa").unwrap_or(0));
        if let Some(r) = row_of(&self.bat_input) {
            r.set_visible(imv > 0);
        }
        if imv > 0 {
            let (v, a) = (imv as f64 / 1000.0, ima as f64 / 1000.0);
            set_text(&self.bat_input, &format!("{v:.2} V · {a:.2} A · {:.1} W", v * a));
        }
    }

    /// Show only the rows that apply to the current policy and timing.
    fn sync_refresh_rows(&self) {
        let pol = self.ref_policy.selected();
        let desc = match pol {
            1 => "Slows to 60, then 30 Hz while the screen is still.".to_string(),
            2 => format!("Always {} Hz.", self.ref_rate.value().round()),
            _ => String::new(), // "Always 120 Hz" is the mode's own name
        };
        if self.ref_group.description().as_deref().unwrap_or("") != desc {
            self.ref_group.set_description(if desc.is_empty() { None } else { Some(&desc) });
        }
        self.ref_rate.set_visible(pol == 2);
        self.ref_timing.set_visible(pol == 1);
        let custom = pol == 1 && self.ref_timing.selected() as usize == TIMINGS.len();
        self.ref_s60.set_visible(custom);
        self.ref_s30.set_visible(custom);
    }

    fn update_refresh(&self, p: &Props) {
        set_combo(&self.ref_policy, &REFRESH_POLICIES, dbus::s(p, "Policy"));
        let live = dbus::u(p, "LiveRate").map(|h| format!("Now {h} Hz")).unwrap_or_default();
        if self.ref_policy.subtitle().as_deref().unwrap_or("") != live {
            self.ref_policy.set_subtitle(&live);
        }
        set_spin(&self.ref_rate, dbus::u(p, "Rate").map(f64::from));
        let (a, b) = (dbus::u(p, "IdleMs60"), dbus::u(p, "IdleMs30"));
        let preset = TIMINGS.iter().position(|t| Some(t.2) == a && Some(t.3) == b);
        let sel = if self.ref_custom.get() { None } else { preset };
        let want = sel.unwrap_or(TIMINGS.len()) as u32;
        if self.ref_timing.selected() != want {
            self.ref_timing.set_selected(want);
        }
        // bounds first, so 30's lower bound follows 60
        set_spin(&self.ref_s60, a.map(|v| v as f64 / 1000.0));
        set_spin(&self.ref_s30, b.map(|v| v as f64 / 1000.0));
        set_text(&self.ref_min, &dbus::u(p, "MinHz").map(|h| format!("{h} Hz")).unwrap_or_else(|| "Unknown".into()));
        set_text(&self.ref_input, yes_no(dbus::b(p, "InputWakes")));
        self.sync_refresh_rows();
    }

    fn update_gpu(&self, p: &Props) {
        set_combo(&self.gpu_profile, &GPU_PROFILES, dbus::s(p, "Profile"));
        let follow = dbus::b(p, "FollowPowerProfiles");
        set_switch(&self.gpu_follow, follow);
        let following = follow == Some(true);
        self.gpu_profile.set_sensitive(!following);
        let sub = if following { "Set by the power mode" } else { "" };
        if self.gpu_profile.subtitle().as_deref().unwrap_or("") != sub {
            self.gpu_profile.set_subtitle(sub);
        }
        let floors = dbus::dict_suu(p, "Floors");
        // only when the daemon's values change: never undo an edit before Apply
        if *self.gpu_seen.borrow() == floors {
            return;
        }
        for (name, exp, lo, hi) in &self.gpu_limits {
            if let Some((_, a, b)) = floors.iter().find(|(n, _, _)| n == name) {
                exp.set_subtitle(&format!("{a}–{b} MHz"));
                lo.adjustment().set_upper(2000.0);
                hi.adjustment().set_lower(100.0);
                hi.set_value(*b as f64);
                lo.set_value(*a as f64);
            }
        }
        *self.gpu_seen.borrow_mut() = floors;
    }

    fn update_thermal(&self, p: &Props) {
        set_text(&self.th_surface, &degrees(dbus::f(p, "Surface")));
        set_text(&self.th_cpu, &degrees(dbus::f(p, "CpuMax")));
        set_text(&self.th_gpu, &degrees(dbus::f(p, "GpuMax")));
        let thr = dbus::b(p, "Throttling");
        set_text(&self.th_throttle, yes_no(thr));
        set_class(&self.th_throttle, "warning", thr == Some(true));
        let zones = dbus::dict_sd(p, "Zones");
        let same = {
            let rows = self.th_rows.borrow();
            rows.len() == zones.len() && rows.iter().zip(&zones).all(|(r, z)| r.0 == z.0)
        };
        if !same {
            for (_, r, _) in self.th_rows.borrow_mut().drain(..) {
                self.th_all.remove(&r);
            }
            for (z, _) in &zones {
                let row = adw::ActionRow::builder().title(zone_label(z)).build();
                let l = gtk::Label::new(None);
                l.add_css_class("dim-label");
                l.set_valign(gtk::Align::Center);
                row.add_suffix(&l);
                self.th_all.add_row(&row);
                self.th_rows.borrow_mut().push((z.clone(), row, l));
            }
        }
        for ((_, _, l), (_, v)) in self.th_rows.borrow().iter().zip(&zones) {
            set_text(l, &degrees(Some(*v)));
        }
    }

    fn update_about(&self, p: &Props) {
        set_text(&self.ab_version, &dbus::s(p, "Version").unwrap_or_else(|| "Unknown".into()));
        let kernel = dbus::s(p, "Kernel").unwrap_or_else(|| "unknown".into());
        let release = kernel_release(&kernel);
        self.ab_kernel.set(&release, &kernel);
        let series = dbus::s(p, "SeriesTag").filter(|s| !s.is_empty() && s != "unknown").unwrap_or_else(|| "Unknown".into());
        self.ab_series.set(&series, &series);
        let feats = dbus::strs(p, "Features");
        self.ab_features.set(&format!("{} of {KNOWN_FEATURES} available", feats.len()), &feats.join(", "));
        let fw = dbus::dict_ss(p, "Firmware");
        let bad = fw.iter().filter(|(_, st)| st != "ok").count();
        let fw_summary = if fw.is_empty() {
            "No manifest installed".to_string()
        } else if bad == 0 {
            format!("{} files, all match", fw.len())
        } else {
            format!("{} files, {bad} differ or are missing", fw.len())
        };
        *self.about_debug.borrow_mut() = format!(
            "Kernel: {kernel}\nPatch series: {series}\nHelper: {}\nApp: {}\nFeatures: {}\nFirmware: {fw_summary}\n{}",
            dbus::s(p, "Version").unwrap_or_default(),
            env!("CARGO_PKG_VERSION"),
            feats.join(" "),
            fw.iter().filter(|(_, st)| st != "ok").map(|(f, st)| format!("  {f}: {st}\n")).collect::<String>()
        );
        if self.ab_fw_last.borrow().as_ref() == Some(&fw) {
            return;
        }
        for r in self.ab_fw_rows.borrow_mut().drain(..) {
            self.ab_fw.remove(&r);
        }
        self.ab_fw.set_subtitle(&fw_summary);
        self.ab_fw.set_enable_expansion(!fw.is_empty());
        // problems first, then by path
        let mut sorted = fw.clone();
        sorted.sort_by_key(|(f, st)| (st == "ok", f.clone()));
        for (file, st) in &sorted {
            let path = std::path::Path::new(file.as_str());
            let base = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(file.clone());
            let dir = path.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
            let row = adw::ActionRow::builder().title(base.as_str()).subtitle(dir.as_str()).build();
            row.set_title_lines(1);
            row.set_subtitle_lines(1);
            row.set_tooltip_text(Some(&format!("{file}: {st}")));
            let (icon, class) = if st == "ok" { ("object-select-symbolic", "success") } else { ("dialog-warning-symbolic", "warning") };
            let img = gtk::Image::from_icon_name(icon);
            img.add_css_class(class);
            img.update_property(&[gtk::accessible::Property::Label(&capitalize(st))]);
            row.add_suffix(&img);
            self.ab_fw.add_row(&row);
            self.ab_fw_rows.borrow_mut().push(row);
        }
        *self.ab_fw_last.borrow_mut() = Some(fw);
    }
}

const USAGE: &str = "usage: tb323fu-settings [--help] [--version] [--page NAME] [--maximized]

  --page NAME   open on this page (battery, display, performance, lights, usb,
                emergency-key, systems, android, diagnostics, about)
  --maximized   open the window maximized

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
    const PAGE_NAMES: [&str; 10] = ["battery", "display", "performance", "lights", "usb", "emergency-key", "systems", "android", "diagnostics", "about"];
    let page = args.iter().position(|a| a == "--page").and_then(|i| args.get(i + 1)).map(|n| n.to_lowercase());
    let page = match page {
        Some(n) => match PAGE_NAMES.iter().position(|p| *p == n) {
            Some(i) => Some(i),
            None => {
                eprintln!("{USAGE}");
                return glib::ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let maximized = args.iter().any(|a| a == "--maximized");
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|app| {
        let css = gtk::CssProvider::new();
        css.load_from_string(CSS);
        if let Some(d) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&d, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        let quit = gio::ActionEntry::builder("quit").activate(|app: &adw::Application, _, _| app.quit()).build();
        app.add_action_entries([quit]);
        app.set_accels_for_action("app.quit", &["<Ctrl>q"]);
        app.set_accels_for_action("window.close", &["<Ctrl>w"]);
    });
    app.connect_activate(move |app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        let ui = Ui::new(app);
        ui.selected.set(page);
        if maximized {
            ui.window.maximize();
        }
        let about = gio::SimpleAction::new("about", None);
        let u = ui.clone();
        about.connect_activate(move |_, _| u.about());
        app.add_action(&about);
        let sc = gio::SimpleAction::new("shortcuts", None);
        let u = ui.clone();
        sc.connect_activate(move |_, _| u.shortcuts());
        app.add_action(&sc);
        ui.refresh_now();
        let ui2 = ui.clone();
        glib::timeout_add_seconds_local(2, move || {
            ui2.refresh();
            glib::ControlFlow::Continue
        });
        ui.window.present();
        // the first focus selects the focused sidebar row, and the collapse
        // breakpoint applies after the first frames: put both on the page we
        // opened on once the window is up
        let ui3 = ui.clone();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            if let Some(p) = page.and_then(|i| ui3.pages.get(i)).filter(|p| p.row.is_visible()) {
                p.row.grab_focus();
                ui3.sidebar.select_row(Some(&p.row));
                ui3.split.set_show_content(true);
            }
        });
    });
    app.run_with_args::<&str>(&[])
}
