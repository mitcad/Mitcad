// SPDX-License-Identifier: MIT
//! Sheets as one PDF document, a page per sheet: vector lines in
//! millimetres (the pages' user space scaled from points), texts in the
//! standard Helvetica font with WinAnsi encoding, uncompressed content; a
//! title block's PNG and JPEG images as image objects shared by the pages
//! (mitcad#1625, [`super::pdf_image`]), an SVG image as vector paths with
//! its colours and opacities (mitcad#1627, [`super::svg_image`]), an image
//! that cannot be read as its outline.

use std::f64::consts::{FRAC_PI_2, TAU};
use std::fmt::Write as _;

use super::P2;
use super::pdf_image::{self, PdfImage};
use super::sheet::{Anchor, Item, Pen, Sheet, font_size, text_width};
use super::svg_image::{self, Cap, Join, Segment, Shape, Subpath, SvgImage};
use super::template::ImageKind;
use super::trim_number;
use mitcad_model::appearance::ImageData;

/// What a PDF holds of an image.
enum Form {
    /// An image object (PNG, JPEG).
    Raster(PdfImage),
    /// Vector paths (SVG, mitcad#1627).
    Vector(SvgImage),
    /// Its outline: a file that cannot be read.
    Outline,
}

/// The images of a document: each file once, by its base64, with its PDF
/// form.
type Images = Vec<(ImageData, Form)>;

/// The index of an image in `images`, added when it is not there.
fn image_index(images: &mut Images, data: &ImageData, kind: ImageKind) -> usize {
    if let Some(i) = images.iter().position(|(d, _)| d == data) {
        return i;
    }
    let form = match mitcad_model::library::decode_base64(data.base64()) {
        Err(_) => Form::Outline,
        Ok(bytes) => match kind {
            ImageKind::Svg => svg_image::read(&bytes).map_or(Form::Outline, Form::Vector),
            ImageKind::Png | ImageKind::Jpeg => {
                pdf_image::of(&bytes, kind).map_or(Form::Outline, Form::Raster)
            }
        },
    };
    images.push((data.clone(), form));
    images.len() - 1
}

/// A path of subpaths (the operator that paints or clips it follows).
fn subpaths_path(content: &mut String, subpaths: &[Subpath]) {
    for s in subpaths {
        let _ = writeln!(content, "{} {} m", n(s.start[0]), n(s.start[1]));
        for segment in &s.segments {
            match segment {
                Segment::Line(p) => {
                    let _ = writeln!(content, "{} {} l", n(p[0]), n(p[1]));
                }
                Segment::Cubic(a, b, c) => {
                    let _ = writeln!(
                        content,
                        "{} {} {} {} {} {} c",
                        n(a[0]),
                        n(a[1]),
                        n(b[0]),
                        n(b[1]),
                        n(c[0]),
                        n(c[1])
                    );
                }
            }
        }
        if s.closed {
            let _ = writeln!(content, "h");
        }
    }
}

/// The graphics state of an opacity below one: `/Gf<thousandths>` for
/// fills, `/Gs<thousandths>` for strokes, which the page's resources
/// define ([`opacity_states`]).
fn set_opacity(content: &mut String, prefix: &str, alpha: f64) {
    let thousandths = (alpha * 1000.0).round().clamp(0.0, 1000.0) as u32;
    if thousandths < 1000 {
        let _ = writeln!(content, "/{prefix}{thousandths} gs");
    }
}

/// A title block's SVG image as vector paths (mitcad#1627), each shape in a
/// graphics state of its own (its clip paths, colours, opacities and
/// stroke), so that the sheet's pens and colours are kept around it.
fn vector_image(content: &mut String, shapes: &[Shape]) {
    for shape in shapes {
        // Clipped to nothing: not drawn.
        if shape.clips.iter().any(|c| c.subpaths.is_empty()) {
            continue;
        }
        let _ = writeln!(content, "q");
        for clip in &shape.clips {
            subpaths_path(content, &clip.subpaths);
            let _ = writeln!(content, "{} n", if clip.even_odd { "W*" } else { "W" });
        }
        let fill_op = shape.fill.map(|fill| {
            let [r, g, b] = fill.colour.rgb;
            let _ = writeln!(content, "{} {} {} rg", n(r), n(g), n(b));
            set_opacity(content, "Gf", fill.colour.alpha);
            if fill.even_odd { "f*" } else { "f" }
        });
        if let Some(stroke) = &shape.stroke {
            let [r, g, b] = stroke.colour.rgb;
            let dashes: Vec<String> = stroke.dashes.iter().map(|d| n(*d)).collect();
            let _ = writeln!(
                content,
                "{} {} {} RG {} w {} J {} j {} M [{}] {} d",
                n(r),
                n(g),
                n(b),
                n(stroke.width),
                match stroke.cap {
                    Cap::Butt => 0,
                    Cap::Round => 1,
                    Cap::Square => 2,
                },
                match stroke.join {
                    Join::Miter => 0,
                    Join::Round => 1,
                    Join::Bevel => 2,
                },
                n(stroke.miter_limit.max(1.0)),
                dashes.join(" "),
                n(stroke.dash_offset)
            );
            set_opacity(content, "Gs", stroke.colour.alpha);
        }
        match (fill_op, shape.stroke.is_some()) {
            (Some(fill), true) if shape.stroke_first => {
                subpaths_path(content, &shape.subpaths);
                let _ = writeln!(content, "S");
                subpaths_path(content, &shape.subpaths);
                let _ = writeln!(content, "{fill}");
            }
            (Some(fill), true) => {
                subpaths_path(content, &shape.subpaths);
                let _ = writeln!(content, "{}", if fill == "f*" { "B*" } else { "B" });
            }
            (Some(fill), false) => {
                subpaths_path(content, &shape.subpaths);
                let _ = writeln!(content, "{fill}");
            }
            (None, true) => {
                subpaths_path(content, &shape.subpaths);
                let _ = writeln!(content, "S");
            }
            (None, false) => {}
        }
        let _ = writeln!(content, "Q");
    }
}

/// The graphics states of opacity a page's content uses ([`set_opacity`]),
/// each once, as entries of its `/ExtGState`.
fn opacity_states(content: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in content.split(|b| *b == b'\n') {
        let Some(name) = line
            .strip_suffix(b" gs")
            .and_then(|l| l.strip_prefix(b"/"))
            .and_then(|l| std::str::from_utf8(l).ok())
        else {
            continue;
        };
        let (key, value) = if let Some(v) = name.strip_prefix("Gf") {
            ("ca", v)
        } else if let Some(v) = name.strip_prefix("Gs") {
            ("CA", v)
        } else {
            continue;
        };
        let Ok(thousandths) = value.parse::<u32>() else {
            continue;
        };
        let entry = format!(
            "/{name} << /{key} {} >>",
            n(f64::from(thousandths) / 1000.0)
        );
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    out
}

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

/// The page's content stream; its images are `/Im<index in images>`.
fn content(sheet: &Sheet, images: &mut Images) -> Vec<u8> {
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
                Item::Image {
                    at,
                    size,
                    kind,
                    data,
                } => {
                    let i = image_index(images, data, *kind);
                    match &images[i].1 {
                        Form::Raster(_) => {
                            // The image's unit square onto its box.
                            let _ = writeln!(
                                text,
                                "q {} 0 0 {} {} {} cm /Im{i} Do Q",
                                n(size[0]),
                                n(size[1]),
                                n(at[0]),
                                n(at[1])
                            );
                        }
                        Form::Vector(svg) => vector_image(&mut text, &svg.placed(*at, *size)),
                        Form::Outline => {
                            // A file that cannot be read: its outline.
                            set_pen(&mut text, Pen::Thin, &mut pen);
                            let _ = writeln!(
                                text,
                                "{} {} {} {} re S",
                                n(at[0]),
                                n(at[1]),
                                n(size[0]),
                                n(size[1])
                            );
                        }
                    }
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
    // The pages' contents first: they find the images (mitcad#1625).
    let mut images: Images = Vec::new();
    let contents: Vec<Vec<u8>> = sheets.iter().map(|s| content(s, &mut images)).collect();
    // The images' objects after the pages': each image's number, and its
    // soft mask's right after it.
    let first_image = 5 + 2 * pages;
    let mut numbers: Vec<Option<usize>> = Vec::new();
    let mut next = first_image;
    for (_, form) in &images {
        match form {
            Form::Raster(pdf) => {
                numbers.push(Some(next));
                next += if pdf.mask.is_some() { 2 } else { 1 };
            }
            Form::Vector(_) | Form::Outline => numbers.push(None),
        }
    }
    for (i, sheet) in sheets.iter().enumerate() {
        // The images this page draws.
        let used: Vec<String> = numbers
            .iter()
            .enumerate()
            .filter_map(|(k, number)| {
                let number = (*number)?;
                let name = format!("/Im{k} ");
                contents[i]
                    .windows(name.len())
                    .any(|w| w == name.as_bytes())
                    .then(|| format!("/Im{k} {number} 0 R"))
            })
            .collect();
        let x_objects = if used.is_empty() {
            String::new()
        } else {
            format!(" /XObject << {} >>", used.join(" "))
        };
        // The opacities of its SVG images' paths (mitcad#1627).
        let opacities = opacity_states(&contents[i]);
        let states = if opacities.is_empty() {
            String::new()
        } else {
            format!(" /ExtGState << {} >>", opacities.join(" "))
        };
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >>{x_objects}{states} >> /Contents {} 0 R >>",
                n(sheet.width * PT),
                n(sheet.height * PT),
                6 + 2 * i
            )
            .into_bytes(),
        );
        let stream = &contents[i];
        let mut object = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        object.extend_from_slice(stream);
        object.extend_from_slice(b"\nendstream");
        objects.push(object);
    }
    for (number, (_, form)) in numbers.iter().zip(&images) {
        let (Some(number), Form::Raster(pdf)) = (number, form) else {
            continue;
        };
        let stream_object = |entries: &str, stream: &[u8], mask: Option<usize>| {
            let mask = mask.map_or_else(String::new, |m| format!(" /SMask {m} 0 R"));
            let mut object =
                format!("<< {entries}{mask} /Length {} >>\nstream\n", stream.len()).into_bytes();
            object.extend_from_slice(stream);
            object.extend_from_slice(b"\nendstream");
            object
        };
        let mask_number = pdf.mask.as_ref().map(|_| number + 1);
        objects.push(stream_object(&pdf.entries, &pdf.stream, mask_number));
        if let Some((entries, stream)) = &pdf.mask {
            objects.push(stream_object(entries, stream, None));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_shapes_are_paths_with_their_paint_and_opacity() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <rect width="20" height="10" fill="#ff0000" fill-opacity="0.5" stroke="#0000ff" stroke-width="2"/>
            <clipPath id="c"><rect width="10" height="10"/></clipPath>
            <path d="M0 0 C5 10 15 10 20 0" fill="none" stroke="#000000" clip-path="url(#c)"
                  stroke-linecap="round" stroke-dasharray="2 1"/>
        </svg>"##;
        let image = svg_image::read(svg).unwrap();
        let mut content = String::new();
        // 1 mm per unit, the image's top left at (0, 10).
        vector_image(&mut content, &image.placed([0.0, 0.0], [20.0, 10.0]));
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines[0], "q");
        assert_eq!(lines[1], "1 0 0 rg");
        assert_eq!(lines[2], "/Gf500 gs");
        assert_eq!(lines[3], "0 0 1 RG 2 w 0 J 0 j 4 M [] 0 d");
        assert_eq!(lines[4], "0 10 m");
        assert!(lines.contains(&"B"), "{content}");
        // The curve clipped, its stroke round and dashed.
        assert!(lines.contains(&"W n"), "{content}");
        assert!(
            lines.contains(&"0 0 0 RG 1 w 1 J 0 j 4 M [2 1] 0 d"),
            "{content}"
        );
        assert!(lines.contains(&"5 0 15 0 20 10 c"), "{content}");
        assert_eq!(lines.iter().filter(|l| **l == "Q").count(), 2);
        assert_eq!(
            opacity_states(content.as_bytes()),
            vec!["/Gf500 << /ca 0.5 >>".to_owned()]
        );
    }
}
