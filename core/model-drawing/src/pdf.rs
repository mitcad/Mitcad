// SPDX-License-Identifier: MIT
//! Sheets as one PDF document, a page per sheet: vector lines in
//! millimetres (the pages' user space scaled from points), texts in the
//! standard Helvetica font with WinAnsi encoding, uncompressed content.

use std::f64::consts::{FRAC_PI_2, TAU};
use std::fmt::Write as _;

use super::P2;
use super::sheet::{Anchor, Item, Pen, Sheet, font_size, text_width};
use super::trim_number;

/// Points per millimetre.
const PT: f64 = 72.0 / 25.4;

fn n(v: f64) -> String {
    trim_number(v, 4)
}

/// A text as a PDF string in WinAnsi encoding: Latin-1 letters as they are,
/// what the encoding lacks as '?'.
fn pdf_string(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in text.chars() {
        let byte = match c {
            '(' | ')' | '\\' => {
                out.push(b'\\');
                c as u8
            }
            ' '..='~' => c as u8,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            _ => b'?',
        };
        out.push(byte);
    }
    out.push(b')');
    out
}

fn set_pen(content: &mut String, pen: Pen, current: &mut Option<Pen>) {
    if *current == Some(pen) {
        return;
    }
    let dashes: Vec<String> = pen.dashes().iter().map(|d| n(*d)).collect();
    let _ = writeln!(content, "{} w [{}] 0 d", n(pen.width()), dashes.join(" "));
    *current = Some(pen);
}

/// Cubic Bézier pieces of an arc, at most a quarter turn each.
fn arc_path(content: &mut String, center: P2, radius: f64, start: f64, end: f64) {
    let sweep = end - start;
    let pieces = (sweep / FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
    let p0 = at(start);
    let _ = writeln!(content, "{} {} m", n(p0[0]), n(p0[1]));
    for i in 0..pieces {
        let a0 = start + step * i as f64;
        let a1 = a0 + step;
        let (p0, p3) = (at(a0), at(a1));
        let p1 = [p0[0] - k * radius * a0.sin(), p0[1] + k * radius * a0.cos()];
        let p2 = [p3[0] + k * radius * a1.sin(), p3[1] - k * radius * a1.cos()];
        let _ = writeln!(
            content,
            "{} {} {} {} {} {} c",
            n(p1[0]),
            n(p1[1]),
            n(p2[0]),
            n(p2[1]),
            n(p3[0]),
            n(p3[1])
        );
    }
}

/// The page's content stream.
fn content(sheet: &Sheet) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut text = String::new();
    let _ = writeln!(text, "{k} 0 0 {k} 0 0 cm", k = n(PT));
    let _ = writeln!(text, "1 J 1 j 0 G 0 g");
    let mut pen = None;
    for group in &sheet.groups {
        for item in &group.items {
            match item {
                Item::Line { pen: p, a, b } => {
                    set_pen(&mut text, *p, &mut pen);
                    let _ = writeln!(
                        text,
                        "{} {} m {} {} l S",
                        n(a[0]),
                        n(a[1]),
                        n(b[0]),
                        n(b[1])
                    );
                }
                Item::Arc {
                    pen: p,
                    center,
                    radius,
                    start,
                    end,
                } => {
                    set_pen(&mut text, *p, &mut pen);
                    arc_path(&mut text, *center, *radius, *start, *end);
                    let closed = end - start >= TAU - 1e-9;
                    let _ = writeln!(text, "{}", if closed { "s" } else { "S" });
                }
                Item::Polyline {
                    pen: p,
                    points,
                    closed,
                } => {
                    if points.len() < 2 {
                        continue;
                    }
                    set_pen(&mut text, *p, &mut pen);
                    let _ = write!(text, "{} {} m", n(points[0][0]), n(points[0][1]));
                    for q in &points[1..] {
                        let _ = write!(text, " {} {} l", n(q[0]), n(q[1]));
                    }
                    let _ = writeln!(text, " {}", if *closed { "s" } else { "S" });
                }
                Item::Fill { points } => {
                    if points.len() < 3 {
                        continue;
                    }
                    let _ = write!(text, "{} {} m", n(points[0][0]), n(points[0][1]));
                    for q in &points[1..] {
                        let _ = write!(text, " {} {} l", n(q[0]), n(q[1]));
                    }
                    let _ = writeln!(text, " h f");
                }
                Item::Shade { points, grey } => {
                    if points.len() < 3 {
                        continue;
                    }
                    // In its own grey, then black again for the arrowheads.
                    let _ = write!(
                        text,
                        "{} g {} {} m",
                        n(grey.clamp(0.0, 1.0)),
                        n(points[0][0]),
                        n(points[0][1])
                    );
                    for q in &points[1..] {
                        let _ = write!(text, " {} {} l", n(q[0]), n(q[1]));
                    }
                    let _ = writeln!(text, " h f 0 g");
                }
                Item::Text {
                    at,
                    height,
                    angle,
                    anchor,
                    text: value,
                } => {
                    let shift = match anchor {
                        Anchor::Start => 0.0,
                        Anchor::Middle => text_width(value, *height) / 2.0,
                        Anchor::End => text_width(value, *height),
                    };
                    let (c, s) = (angle.cos(), angle.sin());
                    let x = at[0] - shift * c;
                    let y = at[1] - shift * s;
                    let _ = write!(
                        text,
                        "BT /F1 {} Tf {} {} {} {} {} {} Tm ",
                        n(font_size(*height)),
                        n(c),
                        n(s),
                        n(-s),
                        n(c),
                        n(x),
                        n(y)
                    );
                    out.extend_from_slice(text.as_bytes());
                    text.clear();
                    out.extend_from_slice(&pdf_string(value));
                    text.push_str(" Tj ET\n");
                }
            }
        }
    }
    out.extend_from_slice(text.as_bytes());
    out
}

/// The sheets as a PDF document (version 1.4).
pub fn write(sheets: &[Sheet], title: &str) -> Vec<u8> {
    // Objects: 1 catalog, 2 pages, 3 font, 4 info, then a page and its
    // content per sheet.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let pages = sheets.len();
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 5 + 2 * i)).collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {pages} >>",
            kids.join(" ")
        )
        .into_bytes(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    let mut info = b"<< /Producer (MitCad) /Title ".to_vec();
    info.extend_from_slice(&pdf_string(title));
    info.extend_from_slice(b" >>");
    objects.push(info);
    for (i, sheet) in sheets.iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                n(sheet.width * PT),
                n(sheet.height * PT),
                6 + 2 * i
            )
            .into_bytes(),
        );
        let stream = content(sheet);
        let mut object = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        object.extend_from_slice(&stream);
        object.extend_from_slice(b"\nendstream");
        objects.push(object);
    }
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(table, "{offset:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend_from_slice(table.as_bytes());
    out
}
