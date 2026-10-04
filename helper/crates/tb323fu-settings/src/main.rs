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
mod md;

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
/// (id, label, ms before 60 Hz, ms before 30 Hz) as in docs/helper-reference.md.
const TIMINGS: [(&str, &str, u32, u32); 3] =
    [("power-saver", "Power Saver", 500, 2000), ("balanced", "Balanced", 1000, 5000), ("smooth", "Smooth", 3000, 15000)];
const GPU_PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];
const GPU_PROFILE_LABELS: [&str; 3] = ["Power Saver", "Balanced", "Performance"];
const THERMAL_PROFILES: [&str; 3] = ["quiet", "default", "performance"];
const THERMAL_LABELS: [&str; 3] = ["Quiet", "Default", "Performance"];
const LED_MODES: [&str; 4] = ["off", "charge", "solid", "breathe"];
const LED_MODE_LABELS: [&str; 4] = ["Off", "Charge Indicator", "Solid Color", "Breathing"];
/// Colors that look like themselves on the ring (dark ones read as off,
/// brown as yellow).
const LED_PALETTE: [(&str, &str); 9] = [("Red", "#ff0000"), ("Orange", "#ff6000"), ("Yellow", "#ffd000"), ("Green", "#00ff00"), ("Cyan", "#00ffff"), ("Blue", "#0000ff"), ("Purple", "#a000ff"), ("Pink", "#ff40a0"), ("White", "#ffffff")];
const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const DAY_LETTERS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
/// Every object the helper can export (for "N of M available").
const KNOWN_FEATURES: usize = 13;
const DEBOUNCE: Duration = Duration::from_millis(400);
/// The objects refresh() polls (the root object is polled first).
const OBJECTS: [&str; 14] = ["Battery", "Refresh", "Gpu", "Torch", "LedRing", "Haptics", "Usb", "EmergencyKey", "Android", "Diagnostics", "Boot", "Thermal", "Kernel", "HelperUpdate"];

const CSS: &str = "
.tag { font-size: smaller; font-weight: bold; padding: 2px 8px; border-radius: 999px;
       background-color: alpha(currentColor, 0.12); }
.swatch { min-width: 28px; min-height: 28px; padding: 0; border-radius: 999px;
          box-shadow: inset 0 0 0 1px alpha(black, 0.2); }
.swatch:checked { box-shadow: inset 0 0 0 1px alpha(black, 0.2), 0 0 0 3px @accent_color; }
.swatch-ff0000 { background: #ff0000; }
.swatch-ff6000 { background: #ff6000; }
.swatch-ffd000 { background: #ffd000; }
.swatch-00ff00 { background: #00ff00; }
.swatch-00ffff { background: #00ffff; }
.swatch-0000ff { background: #0000ff; }
.swatch-a000ff { background: #a000ff; }
.swatch-ff40a0 { background: #ff40a0; }
.swatch-ffffff { background: #ffffff; }
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

/// GTK makes the -/+ buttons of a non-editable spin row insensitive, so the
/// row stays editable. In SteamOS Gaming Mode a tap on the number opened the
/// on-screen keyboard and focus could not leave the field: there the number
/// itself takes no focus, and only the buttons change it.
fn touch_spin(r: &adw::SpinRow) {
    r.set_focus_on_click(false);
    if in_gamescope() {
        if let Some(t) = descendant_text(r.upcast_ref()) {
            t.set_focusable(false);
            t.set_can_target(false);
        }
    }
}

fn in_gamescope() -> bool {
    std::env::var_os("GAMESCOPE_WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_lowercase().contains("gamescope"))
}

fn descendant_text(w: &gtk::Widget) -> Option<gtk::Widget> {
    let mut c = w.first_child();
    while let Some(ch) = c {
        if ch.is::<gtk::Text>() {
            return Some(ch);
        }
        if let Some(t) = descendant_text(&ch) {
            return Some(t);
        }
        c = ch.next_sibling();
    }
    None
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
    bat_gap: adw::SpinRow,
    bat_fullby: adw::ExpanderRow,
    bat_fb_hour: adw::SpinRow,
    bat_fb_min: adw::SpinRow,
    bat_fb_days: Vec<gtk::ToggleButton>,
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
    panel_group: adw::PreferencesGroup,
    panel_limit: adw::SwitchRow,
    // Performance (the Gpu object is the performance profile)
    gpu_groups: [adw::PreferencesGroup; 3],
    gpu_profile: adw::ComboRow,
    gpu_follow: adw::SwitchRow,
    perf_wifi: adw::SwitchRow,
    cpu_boost: adw::SwitchRow,
    limits: Vec<LimitRows>,
    limits_seen: RefCell<(Vec<(String, u32, u32)>, Vec<(String, [u32; 4])>)>,
    th_prof_group: adw::PreferencesGroup,
    th_profile: adw::ComboRow,
    th_follow: adw::SwitchRow,
    th_bypass: adw::SwitchRow,
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
    led_mode: adw::ComboRow,
    led_color_row: adw::ActionRow,
    led_color: Vec<(&'static str, gtk::ToggleButton)>,
    led_speed: adw::SpinRow,
    led_override: adw::SwitchRow,
    led_bright: adw::SpinRow,
    led_low: adw::SpinRow,
    led_notify: adw::SwitchRow,
    // Haptics
    hap_group: adw::PreferencesGroup,
    hap_strength: adw::SpinRow,
    // Usb
    usb_wake: adw::SwitchRow,
    usb_charger: adw::SwitchRow,
    usb_dev: adw::SwitchRow,
    usb_ports_group: adw::PreferencesGroup,
    usb_ports_rows: RefCell<Vec<adw::ActionRow>>,
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
    // Helper updates (About)
    hu: HelperUi,
}

/// Limits of one performance profile: GPU min/max and CPU little/big min/max.
struct LimitRows {
    profile: &'static str,
    exp: adw::ExpanderRow,
    gpu: [adw::SpinRow; 2],
    cpu: [adw::SpinRow; 4],
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

/// The helper-update widgets (About): status, the newer release with Notes and one
/// action (Download -> Update…), the command when a package manager updates the
/// helper, Go Back to the version before the last update.
struct HelperUi {
    group: adw::PreferencesGroup,
    state: gtk::Label,
    avail: adw::ActionRow,
    notes: gtk::Button,
    action: gtk::Button,
    command: LongInfo,
    back: adw::ActionRow,
    back_btn: gtk::Button,
    /// what the action button does now: (verb, version)
    next: RefCell<(&'static str, String)>,
    busy: Cell<bool>,
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
        let bat_gap = spin(&g, "Recharge Gap (%)", "", 3.0, 20.0, 1.0);
        let bat_bypass = switch(&g, "Bypass Charging", "Run from the charger, battery idle");
        let g = group(&b, "Scheduled Charging", "");
        group_help(&g, "The limit goes up to 100% early enough to be full at this time, estimated from the charge left and the charger. Two hours later, or when you unplug after it, the limit comes back.");
        let bat_fullby = adw::ExpanderRow::builder().title("Charge to 100% by").show_enable_switch(true).enable_expansion(false).build();
        let bat_fb_hour = adw::SpinRow::with_range(0.0, 23.0, 1.0);
        bat_fb_hour.set_title("Hour");
        let bat_fb_min = adw::SpinRow::with_range(0.0, 55.0, 5.0);
        bat_fb_min.set_title("Minute");
        touch_spin(&bat_fb_hour);
        touch_spin(&bat_fb_min);
        bat_fullby.add_row(&bat_fb_hour);
        bat_fullby.add_row(&bat_fb_min);
        let days_row = adw::ActionRow::builder().title("Days").build();
        let days_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        days_box.set_valign(gtk::Align::Center);
        let mut bat_fb_days = Vec::new();
        for (d, l) in DAYS.iter().zip(DAY_LETTERS) {
            let t = gtk::ToggleButton::with_label(l);
            t.add_css_class("circular");
            t.set_tooltip_text(Some(&capitalize(d)));
            t.update_property(&[gtk::accessible::Property::Label(&capitalize(d))]);
            days_box.append(&t);
            bat_fb_days.push(t);
        }
        days_row.add_suffix(&days_box);
        bat_fullby.add_row(&days_row);
        g.add(&bat_fullby);
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
        let panel_group = group(&b, "Panel", "");
        group_help(&panel_group, "As on Android: from 55 °C panel temperature the brightness is held at 70% until it cools below 52 °C.");
        let panel_limit = switch(&panel_group, "Dim When the Panel Is Hot", "70% brightness from 55 °C");

        // Performance
        let (p_gpu, b) = page_box();
        let g = group(&b, "Profile", "Sets the CPU and GPU limits below.");
        group_help(&g, "Power Saver, Balanced and Performance each have their own CPU and GPU limits. Following the power mode takes the desktop's choice (it offers Power Saver and Balanced on this tablet). The thermal profile and Wi-Fi power saving can follow the profile too.");
        let gpu_group = g.clone();
        let gpu_profile = combo(&g, "Profile", &GPU_PROFILE_LABELS);
        let gpu_follow = switch(&g, "Follow Power Mode", "");
        let perf_wifi = switch(&g, "Low-Latency Wi-Fi in Performance", "Wi-Fi power saving off");
        let th_prof_group = group(&b, "Thermal", "");
        group_help(&th_prof_group, "When the back of the tablet gets warm the kernel lowers the CPU and GPU clocks step by step (43–50 °C). Quiet starts 3 °C earlier; Performance 8 °C later (never above 58 °C), so long games keep their speed but the back and the battery get warmer. Performance ends by itself if a battery reaches 45 °C. The chips' own protection limits never change.");
        let th_profile = combo(&th_prof_group, "Thermal Profile", &THERMAL_LABELS);
        let th_follow = switch(&th_prof_group, "Follow the Profile", "Power Saver: Quiet · Performance: Performance");
        let th_bypass = switch(&th_prof_group, "Bypass Charging in Performance", "Keeps the battery cooler on a charger");
        let g = group(&b, "CPU", "");
        let cpu_boost = switch(&g, "CPU Boost", "Fast cores up to 4.6 GHz; warmer under load");
        let cpu_group = g.clone();
        let g = group(&b, "Frequency Limits", "");
        let gpu_groups = [gpu_group, g.clone(), cpu_group];
        group_help(&g, "Lowest and highest CPU and GPU clocks for each profile. The top of the CPU range keeps CPU Boost. Changes take effect when you press Apply.");
        let mut limits = Vec::new();
        let mut gpu_buttons = Vec::new();
        for (p, label) in GPU_PROFILES.into_iter().zip(GPU_PROFILE_LABELS) {
            let exp = adw::ExpanderRow::builder().title(label).subtitle("…").build();
            let mk = |title: &str, lo: f64, hi: f64, step: f64| {
                let r = adw::SpinRow::with_range(lo, hi, step);
                r.set_title(title);
                touch_spin(&r);
                exp.add_row(&r);
                r
            };
            let lo = mk("GPU Minimum (MHz)", 100.0, 2000.0, 1.0);
            let hi = mk("GPU Maximum (MHz)", 100.0, 2000.0, 1.0);
            let cpu = [
                mk("Efficiency Cores Minimum (MHz)", 300.0, 2000.0, 100.0),
                mk("Efficiency Cores Maximum (MHz)", 300.0, 5000.0, 100.0),
                mk("Fast Cores Minimum (MHz)", 300.0, 2900.0, 100.0),
                mk("Fast Cores Maximum (MHz)", 300.0, 5000.0, 100.0),
            ];
            let apply = adw::ActionRow::builder().title("Apply Limits").build();
            let bt = gtk::Button::with_label("Apply");
            bt.set_valign(gtk::Align::Center);
            bt.set_tooltip_text(Some(&format!("Apply {label} limits")));
            bt.update_property(&[gtk::accessible::Property::Label(&format!("Apply {label} limits"))]);
            apply.add_suffix(&bt);
            apply.set_activatable_widget(Some(&bt));
            exp.add_row(&apply);
            g.add(&exp);
            limits.push(LimitRows { profile: p, exp, gpu: [lo, hi], cpu });
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
        group_help(&led_group, "The RGB ring on the back. Charge indicator: amber while charging, green when full or held at the limit, red when low. Breathing pauses while the screen is off.");
        let led_mode = combo(&led_group, "Mode", &LED_MODE_LABELS);
        let swatches = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).max_children_per_line(9)
            .min_children_per_line(5).column_spacing(6).row_spacing(6).valign(gtk::Align::Center).build();
        let mut led_color: Vec<(&'static str, gtk::ToggleButton)> = Vec::new();
        for (name, hex) in LED_PALETTE {
            let t = gtk::ToggleButton::builder().tooltip_text(name).valign(gtk::Align::Center).build();
            t.add_css_class("swatch");
            t.add_css_class(&format!("swatch-{}", &hex[1..]));
            t.update_property(&[gtk::accessible::Property::Label(name)]);
            if let Some((_, first)) = led_color.first() {
                t.set_group(Some(first));
            }
            swatches.insert(&t, -1);
            led_color.push((hex, t));
        }
        let led_color_row = adw::ActionRow::builder().title("Color").build();
        led_color_row.add_suffix(&swatches);
        led_group.add(&led_color_row);
        let led_speed = spin(&led_group, "Breathing Cycle (s)", "", 1.0, 20.0, 0.5);
        led_speed.set_digits(1);
        let led_override = switch(&led_group, "Charge Colors Take Over", "While charging, held at the limit or low");
        let led_bright = spin(&led_group, "Brightness", "", 1.0, 255.0, 1.0);
        let led_low = spin(&led_group, "Red Below (%)", "", 5.0, 50.0, 1.0);
        let led_notify = switch(&led_group, "Pulse for Notifications", "Needs the Device quick settings tile (GNOME extension)");
        let (_, led_pulse) = button(&led_group, "Try a Pulse", "", "Pulse");
        let hap_group = group(&b, "Vibration", "");
        group_help(&hap_group, "The strength applies to everything that vibrates: the on-screen keyboard, games, notifications.");
        let hap_strength = spin(&hap_group, "Strength (%)", "", 0.0, 100.0, 10.0);
        let hap_row = adw::ActionRow::builder().title("Test").build();
        let mut hap_buttons = Vec::new();
        for (m, l) in [("left", "Left"), ("right", "Right")] {
            let bt = gtk::Button::with_label(l);
            bt.set_valign(gtk::Align::Center);
            bt.update_property(&[gtk::accessible::Property::Label(&format!("Test the {m} motor"))]);
            hap_row.add_suffix(&bt);
            hap_buttons.push((m, bt));
        }
        hap_group.add(&hap_row);

        // USB
        let (p_usb, b) = page_box();
        let g = group(&b, "Wake", "");
        let usb_wake = switch(&g, "Wake from USB Devices", "Keyboard or mouse on USB-C wakes it");
        let usb_charger = switch(&g, "Wake When a Charger Is Plugged In", "Or unplugged");
        let usb_ports_group = group(&b, "USB-C Ports", "");
        let g = group(&b, "Development", "Network link and root console over the USB cable.");
        group_help(&g, "Only turn this on for development: anyone with a cable gets a root console. Turning it on asks for authentication.");
        let usb_dev = switch(&g, "USB Developer Mode", "");

        // Emergency key
        let (p_ek, b) = page_box();
        let g = group(&b, "", "Hold both volume keys to restart into Android.");
        group_help(&g, "Works even when the desktop is frozen.");
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
        let kn_banner = adw::Banner::new("");
        let hu_group = group(&b, "Helper Updates", "New versions of Open Device Helper from the project's GitHub releases.");
        group_help(&hu_group, "The helper restarts with the new version. If it does not answer within 30 seconds, the version before \
            comes back by itself. Restart this app to use its new version; the GNOME extension's new version loads at the next login.");
        let hu_state = info(&hu_group, "Status");
        let hu_avail = adw::ActionRow::builder().title("New Version").build();
        hu_avail.set_subtitle_lines(1);
        let hu_notes = gtk::Button::with_label("Notes");
        hu_notes.set_valign(gtk::Align::Center);
        hu_notes.add_css_class("flat");
        let hu_action = gtk::Button::with_label("Download");
        hu_action.set_valign(gtk::Align::Center);
        hu_action.add_css_class("suggested-action");
        hu_avail.add_suffix(&hu_notes);
        hu_avail.add_suffix(&hu_action);
        hu_avail.set_visible(false);
        hu_group.add(&hu_avail);
        let hu_command = info_long(&hu_group, "New Version", &toasts);
        hu_command.row.set_visible(false);
        let (hu_back, hu_back_btn) = button(&hu_group, "Previous Version", "", "Go Back…");
        hu_back_btn.add_css_class("destructive-action");
        hu_back.set_visible(false);
        hu_group.set_visible(false);

        // Navigation
        let sidebar = gtk::ListBox::new();
        sidebar.add_css_class("navigation-sidebar");
        sidebar.set_selection_mode(gtk::SelectionMode::Single);
        let defs: [(&str, &str, &'static [&'static str], &gtk::ScrolledWindow, Option<&adw::Banner>); 10] = [
            ("Battery", "battery-good-symbolic", &["Battery"], &p_bat, None),
            ("Display", "video-display-symbolic", &["Refresh", "Thermal"], &p_ref, None),
            ("Performance", "power-profile-balanced-symbolic", &["Gpu", "Thermal"], &p_gpu, None),
            ("Lights & Vibration", "display-brightness-symbolic", &["Torch", "LedRing", "Haptics"], &p_led, None),
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
        let side_page = adw::NavigationPage::builder().title("Device").child(&side_tv).build();

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
            bat_gap,
            bat_fullby,
            bat_fb_hour,
            bat_fb_min,
            bat_fb_days,
            ref_group,
            ref_policy,
            ref_rate,
            ref_timing,
            ref_custom: Cell::new(false),
            ref_s60,
            ref_s30,
            ref_min,
            ref_input,
            panel_group,
            panel_limit,
            gpu_groups,
            gpu_profile,
            gpu_follow,
            perf_wifi,
            cpu_boost,
            limits,
            limits_seen: RefCell::new((Vec::new(), Vec::new())),
            th_prof_group,
            th_profile,
            th_follow,
            th_bypass,
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
            led_mode,
            led_color_row,
            led_color,
            led_speed,
            led_override,
            led_bright,
            led_low,
            led_notify,
            hap_group,
            hap_strength,
            usb_wake,
            usb_charger,
            usb_dev,
            usb_ports_group,
            usb_ports_rows: RefCell::new(Vec::new()),
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
                banner: kn_banner,
                banner_kind: RefCell::new(""),
                next: RefCell::new(("", String::new())),
                busy: Cell::new(false),
                local: kn_local_btn,
                sys_group: kn_sys_group,
                sys_row: kn_sys_row,
            },
            hu: HelperUi {
                group: hu_group,
                state: hu_state,
                avail: hu_avail,
                notes: hu_notes,
                action: hu_action,
                command: hu_command,
                back: hu_back,
                back_btn: hu_back_btn,
                next: RefCell::new(("", String::new())),
                busy: Cell::new(false),
            },
        });
        ui.connect(gpu_buttons, rescan, diag_open, retry);
        ui.connect_more(led_pulse, hap_buttons);
        ui.connect_kernel(kn_back_btn);
        ui.connect_helper();
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

    /// Ask before a risky choice; the object stays busy while the dialog is
    /// open. `cancel` puts the widget back.
    #[allow(clippy::too_many_arguments)]
    fn confirm(self: &Rc<Self>, obj: &'static str, heading: &str, body: &str, verb: &str, go: impl Fn(&Rc<Ui>) + 'static,
        cancel: impl Fn(&Rc<Ui>) + 'static) {
        self.hold(obj);
        let d = adw::AlertDialog::new(Some(heading), Some(body));
        d.add_response("cancel", "Cancel");
        d.add_response("go", verb);
        d.set_response_appearance("go", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let ui = self.clone();
        d.connect_response(None, move |_, resp| {
            ui.release(obj);
            if resp == "go" { go(&ui) } else { cancel(&ui) }
        });
        d.present(Some(&self.window));
    }

    /// Pulse and vibration test buttons.
    fn connect_more(self: &Rc<Self>, led_pulse: gtk::Button, hap: Vec<(&'static str, gtk::Button)>) {
        let ui = self.clone();
        led_pulse.connect_clicked(move |_| ui.call_simple("LedRing", "Pulse", (String::new(), 2u32)));
        for (m, b) in hap {
            let ui = self.clone();
            b.connect_clicked(move |b| {
                b.set_sensitive(false);
                let b2 = b.clone();
                ui.call_then("Haptics", "Test", (m.to_string(),), move |_, _| b2.set_sensitive(true));
            });
        }
    }

    /// Send the "full by" time and days (debounced): "" when switched off,
    /// no days when all seven are on.
    fn send_full_by(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        let on = self.bat_fullby.enables_expansion();
        let time = if on { format!("{:02}:{:02}", self.bat_fb_hour.value() as u32, self.bat_fb_min.value() as u32) } else { String::new() };
        let mut days: Vec<String> = DAYS.iter().zip(&self.bat_fb_days).filter(|(_, t)| t.is_active()).map(|(d, _)| d.to_string()).collect();
        if days.len() == DAYS.len() {
            days.clear();
        }
        if on && days.is_empty() && self.bat_fb_days.iter().all(|t| !t.is_active()) {
            return; // no day chosen yet
        }
        self.debounce("fullby", "Battery", move |ui| ui.call_simple("Battery", "SetFullBy", (time, days)));
    }

    /// Show the LED rows that apply to the mode.
    fn sync_led_rows(&self) {
        let m = LED_MODES[self.led_mode.selected() as usize % LED_MODES.len()];
        let own = m == "solid" || m == "breathe";
        self.led_color_row.set_visible(own);
        self.led_speed.set_visible(m == "breathe");
        self.led_override.set_visible(own);
        self.led_low.set_visible(m == "charge" || (own && self.led_override.is_active()));
        self.led_bright.set_visible(m != "off");
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
        self.switch_sends(&self.cpu_boost, "Gpu", "SetCpuBoost");
        self.switch_sends(&self.perf_wifi, "Gpu", "SetWifiLowLatency");
        for (i, b) in gpu_buttons.into_iter().enumerate() {
            let l = &self.limits[i];
            // keep minimum <= maximum while editing
            // (the daemon refuses CPU floors above the first thermal step)
            for (lo, hi, cap) in [(&l.gpu[0], &l.gpu[1], 2000.0), (&l.cpu[0], &l.cpu[1], 1996.0), (&l.cpu[2], &l.cpu[3], 2880.0)] {
                let hi2 = hi.clone();
                lo.connect_value_notify(move |lo| hi2.adjustment().set_lower(lo.value()));
                let lo2 = lo.clone();
                hi.connect_value_notify(move |hi| lo2.adjustment().set_upper(hi.value().min(cap)));
            }
            let ui = self.clone();
            b.connect_clicked(move |_| {
                let l = &ui.limits[i];
                let v = |r: &adw::SpinRow| r.value().round() as u32;
                ui.call_simple("Gpu", "SetLimits", (l.profile.to_string(), v(&l.gpu[0]), v(&l.gpu[1])));
                if l.cpu[0].is_visible() {
                    ui.call_simple("Gpu", "SetCpuLimits", (l.profile.to_string(), v(&l.cpu[0]), v(&l.cpu[1]), v(&l.cpu[2]), v(&l.cpu[3])));
                }
            });
        }

        // Thermal
        let ui = self.clone();
        self.th_profile.connect_selected_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            let p = THERMAL_PROFILES[r.selected() as usize % THERMAL_PROFILES.len()];
            if p != "performance" {
                ui.call_simple("Thermal", "SetProfile", (p.to_string(),));
                return;
            }
            ui.confirm("Thermal", "Let the Tablet Run Hotter?",
                "Clocks stay high 8 °C longer: the back and the battery get noticeably warmer. It ends by itself if a battery reaches 45 °C.",
                "Run Hotter", |ui| ui.call_simple("Thermal", "SetProfile", ("performance".to_string(),)), |ui| ui.refresh());
        });
        self.switch_sends(&self.th_follow, "Thermal", "SetFollowPerformance");
        self.switch_sends(&self.th_bypass, "Thermal", "SetPerformanceBypass");
        let ui = self.clone();
        self.panel_limit.connect_active_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            if r.is_active() {
                ui.call_simple("Thermal", "SetPanelLimit", (true,));
                return;
            }
            ui.confirm_switch(r, "Thermal", "Turn Off Panel Heat Protection?",
                "The panel then stays at full brightness when it is hot, which ages it faster.",
                "Turn Off", |ui| ui.call_simple("Thermal", "SetPanelLimit", (false,)));
        });

        // Torch / LED
        self.switch_sends(&self.torch_on, "Torch", "Set");
        self.spin_sends(&self.torch_level, "torch", "Torch", "SetLevel");
        let ui = self.clone();
        self.led_mode.connect_selected_notify(move |r| {
            if !ui.updating.get() {
                let m = LED_MODES[r.selected() as usize % LED_MODES.len()];
                ui.call_simple("LedRing", "SetMode", (m.to_string(),));
                ui.sync_led_rows();
            }
        });
        for (hex, t) in &self.led_color {
            let ui = self.clone();
            let hex = *hex;
            t.connect_toggled(move |t| {
                if ui.updating.get() || !t.is_active() {
                    return;
                }
                ui.debounce("ledc", "LedRing", move |ui| ui.call_simple("LedRing", "SetColor", (hex.to_string(),)));
            });
        }
        let ui = self.clone();
        self.led_speed.connect_value_notify(move |r| {
            if ui.updating.get() {
                return;
            }
            let ms = (r.value() * 1000.0).round() as u32;
            ui.debounce("leds", "LedRing", move |ui| ui.call_simple("LedRing", "SetSpeed", (ms,)));
        });
        let ui = self.clone();
        self.led_override.connect_active_notify(move |r| {
            if !ui.updating.get() {
                ui.call_simple("LedRing", "SetChargeOverride", (r.is_active(),));
                ui.sync_led_rows();
            }
        });
        self.switch_sends(&self.led_notify, "LedRing", "SetNotifyPulse");
        self.spin_sends(&self.led_bright, "ledb", "LedRing", "SetBrightness");
        self.spin_sends(&self.led_low, "ledlow", "LedRing", "SetLowPercent");
        self.spin_sends(&self.hap_strength, "hap", "Haptics", "SetStrength");

        // Battery: recharge gap, full by
        self.spin_sends(&self.bat_gap, "gap", "Battery", "SetRechargeGap");
        let ui = self.clone();
        self.bat_fullby.connect_enable_expansion_notify(move |_| ui.send_full_by());
        for r in [&self.bat_fb_hour, &self.bat_fb_min] {
            let ui = self.clone();
            r.connect_value_notify(move |_| ui.send_full_by());
        }
        for t in &self.bat_fb_days {
            let ui = self.clone();
            t.connect_toggled(move |_| ui.send_full_by());
        }
        self.switch_sends(&self.usb_charger, "Usb", "SetChargerWake");

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
        self.ref_group.set_visible(present("Refresh"));
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
        self.panel_group.set_visible(present("Thermal"));
        self.th_prof_group.set_visible(present("Thermal"));
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
            self.update_led(l);
        }
        self.hap_group.set_visible(present("Haptics"));
        if let Some(h) = p("Haptics") {
            set_spin(&self.hap_strength, dbus::u(h, "Strength").map(f64::from));
        }
        if let Some(u) = p("Usb") {
            set_switch(&self.usb_wake, dbus::b(u, "WakeEnabled"));
            set_switch(&self.usb_dev, dbus::b(u, "DevMode"));
            set_switch(&self.usb_charger, dbus::b(u, "ChargerWake"));
            self.update_ports(&dbus::ports(u, "Ports"));
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
        match props.get("HelperUpdate") {
            Some(Some(h)) => {
                self.hu.group.set_visible(true);
                self.update_helper(h);
            }
            // an older helper without self-update
            Some(None) if !self.hu.busy.get() => self.hu.group.set_visible(false),
            _ => {}
        }
        self.updating.set(false);
    }

    fn update_helper(&self, p: &Props) {
        let s = |k: &str| dbus::s(p, k).unwrap_or_default();
        let (avail, downloaded, prev, state) = (s("Available"), s("Downloaded"), s("Previous"), s("State"));
        let own = s("Method") == "self";
        let lu: HashMap<String, String> = dbus::dict_ss(p, "LastUpdate").into_iter().collect();
        let g = |k: &str| lu.get(k).cloned().unwrap_or_default();
        let label = match state.as_str() {
            "checking" => "Checking…".to_string(),
            "downloading" => format!("Downloading… {} %", dbus::u(p, "Progress").unwrap_or(0)),
            "verifying" => "Verifying…".into(),
            "installing" | "rolling-back" => "Restarting the helper…".into(),
            "interrupted" => "An update was interrupted: go back to the previous version".into(),
            _ => match g("result").as_str() {
                "rolled-back" if g("to") != s("Version") && !avail.is_empty() => format!("{} did not start; back on {}", g("to"), g("from")),
                _ if !avail.is_empty() => format!("Version {} (version {avail} is available)", s("Version")),
                _ => format!("Version {} (up to date)", s("Version")),
            },
        };
        set_text(&self.hu.state, &label);
        set_class(&self.hu.state, "warning", state == "interrupted");

        let next: (&'static str, String) = if avail.is_empty() || !own || !dbus::b(p, "Installable").unwrap_or(false) {
            ("", String::new())
        } else if downloaded == avail {
            ("install", avail.clone())
        } else {
            ("download", avail.clone())
        };
        self.hu.avail.set_visible(!next.0.is_empty());
        if !next.0.is_empty() {
            let title = format!("Helper {avail} Available");
            if self.hu.avail.title() != title {
                self.hu.avail.set_title(&title);
            }
            let btn = if next.0 == "install" { "Update…" } else { "Download" };
            if self.hu.action.label().as_deref() != Some(btn) {
                self.hu.action.set_label(btn);
            }
        }
        *self.hu.next.borrow_mut() = next;
        let cmd_row = !avail.is_empty() && !own;
        self.hu.command.row.set_visible(cmd_row);
        if cmd_row {
            self.hu.command.row.set_title(&format!("Helper {avail} Available"));
            let cmd = s("UpdateCommand");
            self.hu.command.set(&cmd, &cmd);
        }
        let back = own && !prev.is_empty();
        self.hu.back.set_visible(back);
        if back {
            let sub = format!("Back to {prev}");
            if self.hu.back.subtitle().as_deref() != Some(sub.as_str()) {
                self.hu.back.set_subtitle(&sub);
            }
        }
    }

    /// A long HelperUpdate call on a worker thread (like kernel_call). The
    /// helper restarts during Install and Rollback; the poll picks it up again.
    fn helper_call<B>(self: &Rc<Self>, method: &'static str, body: B)
    where
        B: serde::ser::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    {
        if self.hu.busy.replace(true) {
            return;
        }
        for b in [&self.hu.action, &self.hu.back_btn] {
            b.set_sensitive(false);
        }
        let client = self.client.borrow().clone();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(move || client.call("HelperUpdate", method, &body))
                .await
                .unwrap_or_else(|_| Err("Something went wrong".into()));
            ui.hu.busy.set(false);
            for b in [&ui.hu.action, &ui.hu.back_btn] {
                b.set_sensitive(true);
            }
            match &res {
                Ok(Some(m)) if !m.is_empty() => ui.toast(m),
                Err(e) => ui.toast(&format!("{}: {e}", if method == "Rollback" { "Previous helper" } else { "Helper update" })),
                _ => {}
            }
            ui.refresh();
        });
    }

    fn connect_helper(self: &Rc<Self>) {
        let ui = self.clone();
        self.hu.action.connect_clicked(move |_| {
            let (verb, v) = ui.hu.next.borrow().clone();
            match verb {
                "download" => ui.helper_call("Download", (v,)),
                "install" => {
                    let d = adw::AlertDialog::new(Some(&format!("Update Open Device Helper to {v}?")),
                        Some("The helper restarts with the new version. If it does not answer within 30 seconds, the current version \
                            comes back by itself."));
                    d.add_response("cancel", "Cancel");
                    d.add_response("go", "Update");
                    d.set_response_appearance("go", adw::ResponseAppearance::Suggested);
                    d.set_default_response(Some("go"));
                    d.set_close_response("cancel");
                    let ui2 = ui.clone();
                    d.connect_response(None, move |_, r| {
                        if r == "go" {
                            ui2.helper_call("Install", (v.clone(),));
                        }
                    });
                    d.present(Some(&ui.window));
                }
                _ => {}
            }
        });
        let ui = self.clone();
        self.hu.notes.connect_clicked(move |_| {
            let v = ui.hu.next.borrow().1.clone();
            ui.notes_dialog("HelperUpdate", &v, format!("Open Device Helper {v}"));
        });
        let ui = self.clone();
        self.hu.back_btn.connect_clicked(move |_| {
            let d = adw::AlertDialog::new(Some("Go Back to the Previous Helper?"),
                Some("The files from before the last helper update are put back and the helper restarts."));
            d.add_response("cancel", "Cancel");
            d.add_response("go", "Go Back");
            d.set_response_appearance("go", adw::ResponseAppearance::Destructive);
            d.set_default_response(Some("cancel"));
            d.set_close_response("cancel");
            let ui2 = ui.clone();
            d.connect_response(None, move |_, r| {
                if r == "go" {
                    ui2.helper_call("Rollback", ());
                }
            });
            d.present(Some(&ui.window));
        });
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
            system up twice, the previous kernel comes back. A kernel that hangs before its start-up screen is not counted and can only be undone over EDL (see the recovery guide).", true);
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

    /// The release notes (Markdown, the release body on GitHub), rendered, in
    /// a dialog that can be large (a bottom sheet on a narrow window).
    fn show_notes(self: &Rc<Self>, tag: &str) {
        self.notes_dialog("Kernel", tag, format!("Kernel {}", short_tag(tag)));
    }

    fn notes_dialog(self: &Rc<Self>, obj: &'static str, arg: &str, title: String) {
        self.call_then(obj, "Notes", (arg.to_string(),), move |ui, res| {
            let Ok(Some(text)) = res else { return };
            let body = if text.trim().is_empty() {
                let l = gtk::Label::new(Some("No release notes."));
                l.add_css_class("dim-label");
                let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
                b.append(&l);
                b
            } else {
                md::render(&text)
            };
            body.set_margin_top(12);
            body.set_margin_bottom(24);
            body.set_margin_start(24);
            body.set_margin_end(24);
            let sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&body).build();
            let tv = adw::ToolbarView::new();
            tv.add_top_bar(&adw::HeaderBar::new());
            tv.set_content(Some(&sw));
            let d = adw::Dialog::builder().title(title).content_width(760).content_height(900).child(&tv).build();
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
        let gap = dbus::u(p, "RechargeGap");
        set_spin(&self.bat_gap, gap.map(f64::from));
        let sub = match (dbus::u(p, "ChargeLimit"), gap) {
            (Some(l), Some(g)) => format!("Charging resumes below {}%", l.saturating_sub(g)),
            _ => String::new(),
        };
        if self.bat_gap.subtitle().as_deref().unwrap_or("") != sub {
            self.bat_gap.set_subtitle(&sub);
        }
        let full_by = dbus::s(p, "FullBy").unwrap_or_default();
        let days = dbus::strs(p, "FullByDays");
        if self.bat_fullby.enables_expansion() != !full_by.is_empty() {
            self.bat_fullby.set_enable_expansion(!full_by.is_empty());
        }
        if let Some((h, m)) = full_by.split_once(':').and_then(|(h, m)| Some((h.parse::<f64>().ok()?, m.parse::<f64>().ok()?))) {
            set_spin(&self.bat_fb_hour, Some(h));
            set_spin(&self.bat_fb_min, Some(m));
        } else if full_by.is_empty() && !self.busy("Battery") && self.bat_fb_hour.value() == 0.0 {
            set_spin(&self.bat_fb_hour, Some(7.0));
        }
        for (d, t) in DAYS.iter().zip(&self.bat_fb_days) {
            let on = days.is_empty() || days.iter().any(|x| x == d);
            if t.is_active() != on {
                t.set_active(on);
            }
        }
        let when = match days.len() {
            0 => "every day".to_string(),
            5 if !days.iter().any(|d| d == "sat" || d == "sun") => "weekdays".to_string(),
            _ => days.iter().map(|d| capitalize(d)).collect::<Vec<_>>().join(", "),
        };
        let sub = if dbus::b(p, "FullByActive") == Some(true) {
            "Charging to 100% now".to_string()
        } else if full_by.is_empty() {
            "Off".to_string()
        } else {
            format!("{full_by}, {when}")
        };
        if self.bat_fullby.subtitle().as_str() != sub {
            self.bat_fullby.set_subtitle(&sub);
        }
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
        let health = dbus::s(p, "Health").map(|h| capitalize(&h)).unwrap_or_else(|| "Unknown".into());
        set_text(&self.bat_health, &match dbus::i(p, "StateOfHealth").filter(|s| *s >= 0) {
            Some(s) => format!("{health} · {s}% of new"),
            None => health,
        });
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
        set_switch(&self.cpu_boost, dbus::b(p, "CpuBoost"));
        let following = follow == Some(true);
        self.gpu_profile.set_sensitive(!following);
        let sub = if following { "Set by the power mode" } else { "" };
        if self.gpu_profile.subtitle().as_deref().unwrap_or("") != sub {
            self.gpu_profile.set_subtitle(sub);
        }
        self.perf_wifi.set_visible(dbus::b(p, "WifiAvailable") == Some(true));
        set_switch(&self.perf_wifi, dbus::b(p, "WifiLowLatency"));
        let floors = dbus::dict_suu(p, "Floors");
        let cpu = dbus::dict_s4u(p, "CpuLimits");
        let range = dbus::dict_suu(p, "CpuRange");
        // only when the daemon's values change: never undo an edit before Apply
        if *self.limits_seen.borrow() == (floors.clone(), cpu.clone()) {
            return;
        }
        let ghz = |m: u32| format!("{:.1}", m as f64 / 1000.0);
        for l in &self.limits {
            let mut sub = Vec::new();
            if let Some((_, a, b)) = floors.iter().find(|(n, _, _)| n == l.profile) {
                sub.push(format!("GPU {a}–{b} MHz"));
                l.gpu[0].adjustment().set_upper(2000.0);
                l.gpu[1].adjustment().set_lower(100.0);
                l.gpu[1].set_value(*b as f64);
                l.gpu[0].set_value(*a as f64);
            }
            let c = cpu.iter().find(|(n, _)| n == l.profile).map(|x| x.1);
            for r in &l.cpu {
                r.set_visible(c.is_some());
            }
            if let Some(c) = c {
                sub.push(format!("CPU ≤ {} / {} GHz", ghz(c[1]), ghz(c[3])));
                for (k, cl) in ["little", "big"].iter().enumerate() {
                    let (lo, hi) = range.iter().find(|(n, _, _)| n == cl).map(|x| (x.1 as f64, x.2 as f64)).unwrap_or((300.0, 5000.0));
                    let cap: f64 = if *cl == "big" { 2880.0 } else { 1996.0 };
                    let (mn, mx) = (&l.cpu[2 * k], &l.cpu[2 * k + 1]);
                    mn.adjustment().set_lower(lo);
                    mn.adjustment().set_upper(cap.min(hi));
                    mx.adjustment().set_lower(lo);
                    mx.adjustment().set_upper(hi);
                    mx.set_value(c[2 * k + 1] as f64);
                    mn.set_value(c[2 * k] as f64);
                }
            }
            l.exp.set_subtitle(&sub.join(" · "));
        }
        *self.limits_seen.borrow_mut() = (floors, cpu);
    }

    fn update_led(&self, l: &Props) {
        set_combo(&self.led_mode, &LED_MODES, dbus::s(l, "Mode"));
        set_spin(&self.led_bright, dbus::u(l, "Brightness").map(f64::from));
        set_spin(&self.led_low, dbus::u(l, "LowPercent").map(f64::from));
        set_spin(&self.led_speed, dbus::u(l, "Speed").map(|ms| ms as f64 / 1000.0));
        set_switch(&self.led_override, dbus::b(l, "ChargeOverride"));
        set_switch(&self.led_notify, dbus::b(l, "NotifyPulse"));
        if let Some(c) = dbus::s(l, "Color") {
            for (hex, t) in &self.led_color {
                let on = hex.eq_ignore_ascii_case(&c);
                if t.is_active() != on {
                    t.set_active(on);
                }
            }
        }
        self.sync_led_rows();
    }

    /// One row per USB-C port: what is attached and which way power flows.
    fn update_ports(&self, ports: &[(String, String, String, bool)]) {
        let mut rows = self.usb_ports_rows.borrow_mut();
        while rows.len() > ports.len() {
            if let Some(r) = rows.pop() {
                self.usb_ports_group.remove(&r);
            }
        }
        while rows.len() < ports.len() {
            let r = adw::ActionRow::new();
            self.usb_ports_group.add(&r);
            rows.push(r);
        }
        for (r, (name, data, power, partner)) in rows.iter().zip(ports) {
            let n: u32 = name.trim_start_matches("port").parse().unwrap_or(0);
            r.set_title(&format!("Port {}", n + 1));
            let sub = if !partner {
                "Nothing connected".to_string()
            } else {
                let d = if data == "host" { "USB devices attached" } else { "Connected to a computer or charger" };
                let pw = if power == "source" { "supplying power" } else { "receiving power" };
                format!("{d} · {pw}")
            };
            if r.subtitle().as_deref() != Some(sub.as_str()) {
                r.set_subtitle(&sub);
            }
        }
        self.usb_ports_group.set_visible(!ports.is_empty());
    }

    fn update_thermal(&self, p: &Props) {
        let profiles = dbus::strs(p, "Profiles");
        self.th_prof_group.set_visible(!profiles.is_empty());
        let follow = dbus::b(p, "FollowPerformance");
        set_combo(&self.th_profile, &THERMAL_PROFILES, dbus::s(p, "Profile"));
        set_switch(&self.th_follow, follow);
        set_switch(&self.th_bypass, dbus::b(p, "PerformanceBypass"));
        self.th_profile.set_sensitive(follow != Some(true));
        let trips = dbus::doubles(p, "Trips");
        let sub = match (trips.first(), trips.last(), follow) {
            (_, _, Some(true)) => "Set by the profile".to_string(),
            (Some(a), Some(b), _) => format!("Slows down from {a:.0} °C, in steps up to {b:.0} °C"),
            _ => String::new(),
        };
        if self.th_profile.subtitle().as_deref().unwrap_or("") != sub {
            self.th_profile.set_subtitle(&sub);
        }
        set_switch(&self.panel_limit, dbus::b(p, "PanelLimit"));
        let sub = if dbus::b(p, "PanelLimited") == Some(true) { "Dimmed now: the panel is hot" } else { "70% brightness from 55 °C" };
        if self.panel_limit.subtitle().as_deref().unwrap_or("") != sub {
            self.panel_limit.set_subtitle(sub);
        }
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
