// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Joonhoe Kim
//! A small Markdown renderer for release notes: headings, paragraphs, lists,
//! block quotes, code blocks, tables and inline bold/italic/code/links, as GTK
//! labels with Pango markup. Anything else stays as text.

use gtk::glib;
use gtk::prelude::*;

fn esc(s: &str) -> String {
    glib::markup_escape_text(s).to_string()
}

/// Inline Markdown to Pango markup: `code`, [text](url), **bold**, *italic*.
fn inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(ch) = rest.chars().next() {
        let after = &rest[ch.len_utf8()..];
        let done = match ch {
            '`' => after.find('`').map(|j| {
                out += &format!("<tt>{}</tt>", esc(&after[..j]));
                &after[j + 1..]
            }),
            '[' => after.find("](").and_then(|k| after[k + 2..].find(')').map(|e| (k, e))).map(|(k, e)| {
                out += &format!("<a href=\"{}\">{}</a>", esc(&after[k + 2..k + 2 + e]), inline(&after[..k]));
                &after[k + 2 + e + 1..]
            }),
            '*' | '_' => {
                let strong = after.starts_with(ch);
                let mark = if strong { &rest[..2] } else { &rest[..1] };
                let body = &rest[mark.len()..];
                // `_` inside a word (snake_case) is not emphasis
                let in_word = ch == '_' && out.chars().last().is_some_and(|c| c.is_alphanumeric());
                match body.find(mark) {
                    Some(j) if j > 0 && !in_word => {
                        let tag = if strong { "b" } else { "i" };
                        out += &format!("<{tag}>{}</{tag}>", inline(&body[..j]));
                        Some(&body[j + mark.len()..])
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        match done {
            Some(r) => rest = r,
            None => {
                out += &esc(&rest[..ch.len_utf8()]);
                rest = after;
            }
        }
    }
    out
}

fn label(markup: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(None);
    l.set_markup(markup);
    l.set_wrap(true);
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    l.set_xalign(0.0);
    l.set_selectable(true);
    l.set_focusable(false);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

fn table(rows: &[Vec<String>]) -> gtk::Grid {
    let g = gtk::Grid::builder().column_spacing(16).row_spacing(6).build();
    for (r, cells) in rows.iter().enumerate() {
        for (k, cell) in cells.iter().enumerate() {
            let m = if r == 0 { format!("<b>{}</b>", inline(cell)) } else { inline(cell) };
            let l = label(&m, &[]);
            l.set_hexpand(k + 1 == cells.len());
            l.set_valign(gtk::Align::Start);
            g.attach(&l, k as i32, r as i32, 1, 1);
        }
    }
    g
}

fn cells(line: &str) -> Vec<String> {
    line.trim().trim_matches('|').split('|').map(|s| s.trim().to_string()).collect()
}

/// Renders `md` into a vertical box.
pub fn render(md: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let mut text = md.to_string();
    // HTML comments (the release notes carry `<!-- tb323fu: min_helper=… -->`)
    while let Some(s) = text.find("<!--") {
        match text[s..].find("-->") {
            Some(e) => text.replace_range(s..s + e + 3, ""),
            None => break,
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, b: &gtk::Box| {
        if !para.is_empty() {
            b.append(&label(&inline(&para.join(" ")), &[]));
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_start();
        if t.starts_with("```") {
            flush(&mut para, &b);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            let l = label(&format!("<tt>{}</tt>", esc(&code.join("\n"))), &["card"]);
            l.set_wrap(false);
            l.set_margin_start(4);
            let sw = gtk::ScrolledWindow::builder().vscrollbar_policy(gtk::PolicyType::Never).child(&l).build();
            b.append(&sw);
        } else if t.is_empty() {
            flush(&mut para, &b);
        } else if let Some(h) = t.strip_prefix('#') {
            flush(&mut para, &b);
            let level = 1 + h.chars().take_while(|&ch| ch == '#').count();
            let txt = h.trim_start_matches('#').trim();
            let size = match level { 1 => "x-large", 2 => "large", _ => "medium" };
            let l = label(&format!("<span size=\"{size}\" weight=\"bold\">{}</span>", inline(txt)), &[]);
            l.set_margin_top(if level <= 2 { 8 } else { 4 });
            b.append(&l);
        } else if t.starts_with('|') && i + 1 < lines.len() && lines[i + 1].trim_start().starts_with('|') && lines[i + 1].contains("---") {
            flush(&mut para, &b);
            let mut rows = vec![cells(t)];
            i += 2;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(cells(lines[i]));
                i += 1;
            }
            b.append(&table(&rows));
            continue;
        } else if t.starts_with("- ") || t.starts_with("* ") || t.chars().take_while(|ch| ch.is_ascii_digit()).count() > 0 && t.contains(". ") && t.split_once(". ").map(|(n, _)| n.chars().all(|ch| ch.is_ascii_digit())).unwrap_or(false) {
            flush(&mut para, &b);
            let indent = (line.len() - t.len()) as i32;
            let (bullet, rest) = if t.starts_with("- ") || t.starts_with("* ") {
                ("•".to_string(), &t[2..])
            } else {
                let (n, r) = t.split_once(". ").unwrap();
                (format!("{n}."), r)
            };
            // continuation lines of the item
            let mut item = rest.to_string();
            while i + 1 < lines.len() {
                let n = lines[i + 1];
                let nt = n.trim_start();
                if nt.is_empty() || nt.starts_with("- ") || nt.starts_with("* ") || nt.starts_with('#') || nt.starts_with('|') || nt.starts_with("```")
                    || (n.len() - nt.len()) == 0 || nt.split_once(". ").map(|(k, _)| !k.is_empty() && k.chars().all(|ch| ch.is_ascii_digit())).unwrap_or(false) {
                    break;
                }
                item.push(' ');
                item += nt;
                i += 1;
            }
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.set_margin_start(8 + indent * 6);
            let bl = gtk::Label::new(Some(&bullet));
            bl.set_valign(gtk::Align::Start);
            row.append(&bl);
            let l = label(&inline(&item), &[]);
            l.set_hexpand(true);
            row.append(&l);
            b.append(&row);
        } else if let Some(q) = t.strip_prefix('>') {
            flush(&mut para, &b);
            let l = label(&inline(q.trim()), &["dim-label"]);
            l.set_margin_start(16);
            b.append(&l);
        } else if t.chars().all(|ch| ch == '-' || ch == '*' || ch == '_') && t.len() >= 3 {
            flush(&mut para, &b);
            b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        } else {
            para.push(t.trim_end().to_string());
        }
        i += 1;
    }
    flush(&mut para, &b);
    b
}
