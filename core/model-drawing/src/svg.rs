// SPDX-License-Identifier: MIT
//! A sheet as an SVG drawing in millimetres: one `<g class="…">` per group
//! (`view` with the view's `id`, `dimension`, `hole-row`, …), black lines
//! of ISO 128's widths, texts in Helvetica (or the viewer's sans serif),
//! a title block's images (mitcad#1625) as `<image>`s with their files as
//! data URLs.

use std::f64::consts::{PI, TAU};
use std::fmt::Write as _;

use super::sheet::{Anchor, Item, Pen, Sheet, font_size};
use super::trim_number;

fn n(v: f64) -> String {
    trim_number(v, 3)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn stroke(pen: Pen) -> String {
    let mut s = format!(
        r#" fill="none" stroke="black" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""#,
        n(pen.width())
    );
    if !pen.dashes().is_empty() {
        let dashes: Vec<String> = pen.dashes().iter().map(|d| n(*d)).collect();
        let _ = write!(s, r#" stroke-dasharray="{}""#, dashes.join(" "));
    }
    s
}

/// The sheet as an SVG document.
pub fn write(sheet: &Sheet) -> String {
    let h = sheet.height;
    // SVG's y runs down.
    let p = |x: f64, y: f64| format!("{} {}", n(x), n(h - y));
    let mut out = String::new();
    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}mm" height="{hh}mm" viewBox="0 0 {w} {hh}">"#,
        w = n(sheet.width),
        hh = n(h)
    );
    let _ = writeln!(out, "<title>{}</title>", escape(&sheet.title));
    let _ = writeln!(
        out,
        r#"<rect x="0" y="0" width="{}" height="{}" fill="white"/>"#,
        n(sheet.width),
        n(h)
    );
    for group in &sheet.groups {
        match &group.id {
            Some(id) => {
                let _ = writeln!(out, r#"<g class="{}" id="{}">"#, group.class, escape(id));
            }
            None => {
                let _ = writeln!(out, r#"<g class="{}">"#, group.class);
            }
        }
        for item in &group.items {
            match item {
                Item::Line { pen, a, b } => {
                    let _ = writeln!(
                        out,
                        r#"<line x1="{}" y1="{}" x2="{}" y2="{}"{}/>"#,
                        n(a[0]),
                        n(h - a[1]),
                        n(b[0]),
                        n(h - b[1]),
                        stroke(*pen)
                    );
                }
                Item::Arc {
                    pen,
                    center,
                    radius,
                    start,
                    end,
                } => {
                    if end - start >= TAU - 1e-9 {
                        let _ = writeln!(
                            out,
                            r#"<circle cx="{}" cy="{}" r="{}"{}/>"#,
                            n(center[0]),
                            n(h - center[1]),
                            n(*radius),
                            stroke(*pen)
                        );
                    } else {
                        let at =
                            |a: f64| p(center[0] + radius * a.cos(), center[1] + radius * a.sin());
                        // Counter-clockwise on paper is SVG's negative
                        // sweep, its y running down.
                        let large = i32::from(end - start > PI);
                        let _ = writeln!(
                            out,
                            r#"<path d="M {} A {r} {r} 0 {large} 0 {}"{}/>"#,
                            at(*start),
                            at(*end),
                            stroke(*pen),
                            r = n(*radius)
                        );
                    }
                }
                Item::Polyline {
                    pen,
                    points,
                    closed,
                } => {
                    let list: Vec<String> = points
                        .iter()
                        .map(|q| format!("{},{}", n(q[0]), n(h - q[1])))
                        .collect();
                    let tag = if *closed { "polygon" } else { "polyline" };
                    let _ = writeln!(
                        out,
                        r#"<{tag} points="{}"{}/>"#,
                        list.join(" "),
                        stroke(*pen)
                    );
                }
                Item::Fill { points } => {
                    let list: Vec<String> = points
                        .iter()
                        .map(|q| format!("{},{}", n(q[0]), n(h - q[1])))
                        .collect();
                    let _ = writeln!(
                        out,
                        r#"<polygon points="{}" fill="black" stroke="none"/>"#,
                        list.join(" ")
                    );
                }
                Item::Shade { points, grey } => {
                    let list: Vec<String> = points
                        .iter()
                        .map(|q| format!("{},{}", n(q[0]), n(h - q[1])))
                        .collect();
                    let level = (grey.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let _ = writeln!(
                        out,
                        r#"<polygon points="{}" fill="rgb({level},{level},{level})" stroke="none"/>"#,
                        list.join(" ")
                    );
                }
                Item::Image {
                    at,
                    size,
                    kind,
                    data,
                } => {
                    // Stretched over its box, which has its proportions
                    // already; `xlink:href` for the readers of SVG 1.1.
                    let _ = writeln!(
                        out,
                        r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" xlink:href="data:{};base64,{}"/>"#,
                        n(at[0]),
                        n(h - at[1] - size[1]),
                        n(size[0]),
                        n(size[1]),
                        kind.media_type(),
                        data.base64()
                    );
                }
                Item::Text {
                    at,
                    height,
                    angle,
                    anchor,
                    text,
                } => {
                    let anchor = match anchor {
                        Anchor::Start => "start",
                        Anchor::Middle => "middle",
                        Anchor::End => "end",
                    };
                    let (x, y) = (n(at[0]), n(h - at[1]));
                    let turn = if angle.abs() > 1e-12 {
                        format!(r#" transform="rotate({} {x} {y})""#, n(-angle.to_degrees()))
                    } else {
                        String::new()
                    };
                    let _ = writeln!(
                        out,
                        r#"<text x="{x}" y="{y}" font-family="Helvetica, Arial, sans-serif" font-size="{}" text-anchor="{anchor}"{turn}>{}</text>"#,
                        n(font_size(*height)),
                        escape(text)
                    );
                }
            }
        }
        let _ = writeln!(out, "</g>");
    }
    let _ = writeln!(out, "</svg>");
    out
}
