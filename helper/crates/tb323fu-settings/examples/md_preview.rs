// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! Shows a Markdown file the way the app shows release notes:
//!   cargo run --release --example md_preview -- NOTES.md
use adw::prelude::*;
use gtk::glib;

#[path = "../src/md.rs"]
mod md;

fn main() -> glib::ExitCode {
    let file = std::env::args().nth(1).expect("usage: md_preview FILE.md");
    let text = std::fs::read_to_string(&file).expect("read");
    let app = adw::Application::builder().application_id("io.github.joonhoekim.MdPreview").build();
    app.connect_activate(move |app| {
        let body = md::render(&text);
        body.set_margin_top(12);
        body.set_margin_bottom(24);
        body.set_margin_start(24);
        body.set_margin_end(24);
        let sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&body).build();
        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&adw::HeaderBar::new());
        tv.set_content(Some(&sw));
        adw::ApplicationWindow::builder().application(app).title("Release notes").default_width(760).default_height(900).content(&tv).build().present();
    });
    app.run_with_args::<&str>(&[])
}
