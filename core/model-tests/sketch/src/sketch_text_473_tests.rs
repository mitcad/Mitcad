// SPDX-License-Identifier: MIT
//! Sketch text from font files, shaped in both directions, SVG drawings and
//! QR codes (mitcad#473), with the mock kernel.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

/// A test font of `core/model/data/fonts`.
fn font(name: &str) -> String {
    format!(
        "{}/../../model/data/fonts/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// The regions of the sketch: keys and areas, sorted by key.
fn regions(s: &Value) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["key"].as_str().unwrap().to_owned(),
                r["area"].as_f64().unwrap(),
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn a_text_in_a_font_file_keeps_its_outlines_without_the_file() {
    let mut d = with_sketch();
    let file = font("LiberationSans-Regular.ttf");
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "MIT", "at": [0, 0],
               "height": 10, "font_file": file}),
    );
    let s = sketch(&d);
    let text = &s["texts"][0];
    assert_eq!(text["font"], "Liberation Sans", "{text}");
    assert_eq!(text["family"], "Liberation Sans");
    assert_eq!(text["fallback"], false);
    assert_eq!(text["font_file"]["path"], file.as_str());
    let licence = text["font_file"]["licence"].as_str().unwrap();
    assert!(
        licence.contains("Red Hat") && licence.contains("Open Font License"),
        "{licence}"
    );
    assert_eq!(text["has_glyphs"], true);
    // M, I and T: a region each, the M about 0.69 em high.
    let before = regions(&s);
    assert_eq!(before.len(), 3, "{before:?}");
    let ys: Vec<f64> = text["outline"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["curves"].as_array().unwrap().clone())
        .flat_map(|c| {
            ["start", "end"]
                .iter()
                .filter_map(|k| c.get(k).and_then(|p| p[1].as_f64()))
                .collect::<Vec<_>>()
        })
        .collect();
    let top = ys.iter().copied().fold(f64::MIN, f64::max);
    assert!((top - 6.88).abs() < 0.05, "{top}");

    // The design opens the same where the file is not. The path as the JSON
    // holds it: escaped (a Windows path's backslashes are doubled there).
    let quoted = serde_json::to_string(&file).unwrap();
    let json = d.to_json().replace(
        &quoted[1..quoted.len() - 1],
        "/no/such/folder/LiberationSans-Regular.ttf",
    );
    let mut reopened = Document::from_json(&json, MockKernel::default()).unwrap();
    reopened.recompute();
    let s = sketch(&reopened);
    assert_eq!(regions(&s), before);
    assert_eq!(s["texts"][0]["stored_outlines"], true);
    assert_eq!(s["texts"][0]["outline"], text["outline"]);
    // Its position and height still change; a new string needs the file.
    command(
        &mut reopened,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t1", "height": 20}),
    );
    let areas: Vec<f64> = regions(&sketch(&reopened)).iter().map(|r| r.1).collect();
    for (a, b) in areas.iter().zip(&before) {
        assert!((a - 4.0 * b.1).abs() < 1e-6 * a, "{a} {}", b.1);
    }
    let why = rejected(
        &mut reopened,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t1", "text": "MITs"}),
    );
    assert!(why.contains("choose the font file again"), "{why}");
    // An installed font instead leaves the file.
    command(
        &mut reopened,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t1", "font": "Mock Sans"}),
    );
    let text = &sketch(&reopened)["texts"][0];
    assert!(
        text.get("font_file").is_none() && text["family"] == "Mock Sans",
        "{text}"
    );
}

/// The x extents of each character's outlines (by its index), from the
/// sketch query's `outline`.
fn ink_x(text: &Value) -> std::collections::BTreeMap<usize, (f64, f64)> {
    let mut out = std::collections::BTreeMap::new();
    for contour in text["outline"].as_array().unwrap() {
        let id = contour["id"].as_str().unwrap();
        let k: usize = id.split('.').nth(1).unwrap()[1..].parse().unwrap();
        for curve in contour["curves"].as_array().unwrap() {
            let points: Vec<&Value> = match curve.get("control") {
                Some(control) => control.as_array().unwrap().iter().collect(),
                None => vec![&curve["start"], &curve["end"]],
            };
            for p in points {
                let x = p[0].as_f64().unwrap();
                let e = out.entry(k).or_insert((x, x));
                e.0 = f64::min(e.0, x);
                e.1 = f64::max(e.1, x);
            }
        }
    }
    out
}

/// What the font's GSUB makes of a glyph by the single substitutions of a
/// feature (`init`, `medi`, `fina`), as the font's own tables say.
fn gsub_single(font: &skrifa::FontRef<'_>, glyph: u32, feature: &[u8; 4]) -> u32 {
    use skrifa::raw::TableProvider;
    use skrifa::raw::tables::gsub::{SingleSubst, SubstitutionSubtables};
    let gsub = font.gsub().unwrap();
    let features = gsub.feature_list().unwrap();
    let lookups = gsub.lookup_list().unwrap();
    let mut glyph = glyph;
    for record in features.feature_records() {
        if record.feature_tag().to_be_bytes() != *feature {
            continue;
        }
        let feature = record.feature(features.offset_data()).unwrap();
        for index in feature.lookup_list_indices() {
            let lookup = lookups.lookups().get(index.get() as usize).unwrap();
            if let Ok(SubstitutionSubtables::Single(subtables)) = lookup.subtables() {
                for subtable in subtables.iter().flatten() {
                    let id = skrifa::GlyphId::new(glyph);
                    let replaced = match &subtable {
                        SingleSubst::Format1(s) => s
                            .coverage()
                            .unwrap()
                            .get(id)
                            .map(|_| (glyph as i32 + i32::from(s.delta_glyph_id())) as u32),
                        SingleSubst::Format2(s) => s.coverage().unwrap().get(id).map(|i| {
                            u32::from(s.substitute_glyph_ids()[usize::from(i)].get().to_u16())
                        }),
                    };
                    if let Some(replaced) = replaced {
                        glyph = replaced;
                        break;
                    }
                }
            }
        }
        // One record per script and language system: the first does.
        break;
    }
    glyph
}

#[test]
fn an_arabic_word_is_shaped_into_its_joined_forms_and_runs_right_to_left() {
    use mitcad_model::sketch::fonts::FontData;
    use skrifa::MetadataProvider;
    let file = font("NotoSansArabic-Regular.ttf");
    // Kaf, teh, beh: initial, medial and final forms.
    let word = "\u{0643}\u{062A}\u{0628}";
    let data = FontData::read(std::path::Path::new(&file), 0).unwrap();
    let shaped = data.shape(word, false);
    let bytes = std::fs::read(&file).unwrap();
    let face = skrifa::FontRef::new(&bytes).unwrap();
    let cmap = |c: char| face.charmap().map(c).unwrap().to_u32();
    let expected: Vec<Vec<u32>> = word
        .chars()
        .zip([b"init", b"medi", b"fina"])
        .map(|(c, feature)| vec![gsub_single(&face, cmap(c), feature)])
        .collect();
    assert_eq!(shaped.ids, expected);
    // The forms are not the letters' own glyphs.
    for (ids, c) in shaped.ids.iter().zip(word.chars()) {
        assert_ne!(ids[0], cmap(c), "{c}");
    }
    // In a sketch the pen runs from right to left.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": word, "at": [0, 0],
               "height": 10, "font_file": file}),
    );
    let ink = ink_x(&sketch(&d)["texts"][0]);
    assert_eq!(ink.len(), 3, "{ink:?}");
    assert!(ink[&0].0 > ink[&1].0 && ink[&1].0 > ink[&2].0, "{ink:?}");
    // The joined letters touch: each one's left end meets the next one's
    // right end (within the font's overlap).
    assert!(
        ink[&0].0 < ink[&1].1 + 0.05 && ink[&1].0 < ink[&2].1 + 0.05,
        "{ink:?}"
    );
}

#[test]
fn a_hebrew_word_in_an_installed_font_runs_right_to_left() {
    use mitcad_model::sketch::fonts::FontData;
    use skrifa::MetadataProvider;
    let file = font("LiberationSans-Regular.ttf");
    let word = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}";
    let data = FontData::read(std::path::Path::new(&file), 0).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    let face = skrifa::FontRef::new(&bytes).unwrap();
    let ids: Vec<Vec<u32>> = word
        .chars()
        .map(|c| vec![face.charmap().map(c).unwrap().to_u32()])
        .collect();
    assert_eq!(data.shape(word, false).ids, ids);
    // Liberation Sans as an installed font (the mock kernel's file of it):
    // shaped by the model, the pen from right to left.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": word, "at": [0, 0],
               "height": 10, "font": "Liberation Sans"}),
    );
    let text = &sketch(&d)["texts"][0];
    assert_eq!(
        (&text["family"], &text["fallback"]),
        (&json!("Liberation Sans"), &json!(false))
    );
    let ink = ink_x(text);
    let starts: Vec<f64> = ink.values().map(|e| e.0).collect();
    assert_eq!(starts.len(), 4);
    assert!(starts.windows(2).all(|w| w[0] > w[1]), "{starts:?}");
    // The line starts at the anchor, as a left-aligned line does.
    assert!(starts[3] >= 0.0 && starts[3] < 1.0, "{starts:?}");
}

#[test]
fn a_mixed_line_is_in_visual_order() {
    // Latin, Hebrew and digits in a left-to-right line: "abc", then the
    // Hebrew run with its number, the number reading left to right.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1",
               "text": "abc \u{05E9}\u{05DC}\u{05D5}\u{05DD} 123", "at": [0, 0],
               "height": 10, "font_file": font("LiberationSans-Regular.ttf")}),
    );
    let ink = ink_x(&sketch(&d)["texts"][0]);
    let mut order: Vec<(f64, usize)> = ink.iter().map(|(k, e)| (e.0, *k)).collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let order: Vec<usize> = order.into_iter().map(|(_, k)| k).collect();
    assert_eq!(order, vec![0, 1, 2, 9, 10, 11, 7, 6, 5, 4]);
    // A line of Latin only keeps its order (and its outlines).
    assert_eq!(
        mitcad_model::sketch::text::visual_lines("ab\ncd", &[vec![0, 1], vec![3, 4]]),
        vec![vec![0, 1], vec![3, 4]]
    );
}

#[test]
fn a_font_file_that_is_not_a_font_is_refused() {
    let mut d = with_sketch();
    let why = rejected(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "A", "at": [0, 0],
               "height": 10, "font_file": font("README.md")}),
    );
    assert!(why.contains("not a font"), "{why}");
    let why = rejected(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "A", "at": [0, 0],
               "height": 10, "font_file": font("missing.ttf")}),
    );
    assert!(why.contains("cannot be read"), "{why}");
}

/// The design JSON of a sketch text in a font file, with its font file's
/// path set to `path` and, unless `glyphs`, its stored outlines left out.
fn font_file_design(d: &Document<MockKernel>, path: &str, glyphs: bool) -> String {
    let mut value: Value = serde_json::from_str(&d.to_json()).unwrap();
    let text = &mut value["features"][0]["texts"][0];
    assert!(text["glyphs"].is_object(), "{text}");
    text["font_file"]["path"] = json!(path);
    if !glyphs {
        text.as_object_mut().unwrap().remove("glyphs");
    }
    value.to_string()
}

#[test]
fn a_font_file_named_by_a_design_is_not_read() {
    // mitcad#929: a font file path that came with a design file is never
    // read; the text needs its stored outlines or the file chosen again.
    let mut d = with_sketch();
    let file = font("LiberationSans-Regular.ttf");
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "MIT", "at": [0, 0],
               "height": 10, "font_file": file}),
    );
    // Chosen in this session: a new string reads the file again.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t1", "text": "MITs"}),
    );
    assert_eq!(sketch(&d)["texts"][0]["has_glyphs"], true);
    command(
        &mut d,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t1", "text": "MIT"}),
    );

    // A missing file and no outlines: the outlines are named, no read
    // error (nothing was read).
    let json = font_file_design(&d, "/nonexistent/x.ttf", false);
    let mut opened = Document::from_json(&json, MockKernel::default()).unwrap();
    opened.recompute();
    let error = opened
        .status(mitcad_model::ids::FeatureUid(1))
        .and_then(|s| s.error())
        .unwrap_or_default()
        .to_owned();
    assert!(
        error.contains("t1: the text has no outlines stored for its font file x.ttf")
            && error.contains("choose the font file again")
            && !error.contains("cannot be read"),
        "{error}"
    );
    // The same for a font file that is there: the design's path is not
    // read either.
    let json = font_file_design(&d, &file, false);
    let mut opened = Document::from_json(&json, MockKernel::default()).unwrap();
    opened.recompute();
    let error = opened
        .status(mitcad_model::ids::FeatureUid(1))
        .and_then(|s| s.error())
        .unwrap_or_default()
        .to_owned();
    assert!(
        error.contains("no outlines stored for its font file LiberationSans-Regular.ttf"),
        "{error}"
    );

    // With its outlines the text opens; a new string needs the file chosen
    // again, even where the design's path names one.
    let json = font_file_design(&d, &file, true);
    let mut opened = Document::from_json(&json, MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(sketch(&opened)["texts"][0]["stored_outlines"], true);
    let why = rejected(
        &mut opened,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t1", "text": "MITs"}),
    );
    assert!(
        why.contains("a font file named by the design is not read")
            && why.contains("choose the font file again"),
        "{why}"
    );
    // Chosen again, it is read.
    command(
        &mut opened,
        json!({"cmd": "sketch.edit_text", "sketch": "F1", "id": "t1", "text": "MITs",
               "font_file": file}),
    );
    let text = &sketch(&opened)["texts"][0];
    assert_eq!(text["has_glyphs"], true, "{text}");
}

/// A region of the sketch query by key: area, centroid and loop count.
fn region_of(s: &Value, key: &str) -> (f64, [f64; 2], usize) {
    let r = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == key)
        .unwrap_or_else(|| panic!("no region {key}"));
    let c = &r["centroid"];
    (
        r["area"].as_f64().unwrap(),
        [c[0].as_f64().unwrap(), c[1].as_f64().unwrap()],
        r["loops"].as_array().unwrap().len(),
    )
}

#[test]
fn an_svg_logo_comes_in_with_its_holes_its_stroke_band_and_its_text() {
    let mut d = with_sketch();
    let path = format!(
        "{}/../../model/data/svg/logo.svg",
        env!("CARGO_MANIFEST_DIR")
    );
    let info: Value = serde_json::from_str(
        &d.query(&json!({"query": "svg_info", "path": path}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(info["size"], json!([100.0, 60.0]), "{info}");
    assert_eq!(
        (&info["filled"], &info["stroked"], &info["texts"]),
        (&json!(2), &json!(1), &json!(1))
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.import_svg", "sketch": "F1", "path": path, "stroke_outline": true}),
    );
    assert_eq!(made["warnings"], json!([]), "{made}");
    assert_eq!(
        (&made["text_count"], &made["bands"]),
        (&json!(1), &json!(1)),
        "{made}"
    );
    let s = sketch(&d);
    // The ring and its hole, the disc and its hole, the band and the
    // three letters of the text (the mock font's boxes).
    let regions = s["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 8, "{}", s["regions"]);
    let filled: Vec<&str> = made["filled"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(filled.len(), 3, "{filled:?}");
    let mut found: Vec<(f64, [f64; 2], usize)> = filled.iter().map(|k| region_of(&s, k)).collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let near = |a: f64, b: f64, tol: f64| (a - b).abs() <= tol;
    // The band: 90 x 4 mm about y = 5 (its stroke 8 units of 0.5 mm).
    let (area, centroid, loops) = found[0];
    assert!(
        near(area, 360.0, 1e-6) && near(centroid[1], 5.0, 1e-9) && loops == 1,
        "{found:?}"
    );
    assert!(near(area / 90.0, 4.0, 1e-9), "the band's width");
    // The disc, its hole kept: pi (15^2 - 7.5^2), sampled.
    let (area, centroid, loops) = found[1];
    let disc = std::f64::consts::PI * (225.0 - 56.25);
    assert!(
        near(area, disc, 1e-3 * disc) && near(centroid[0], 75.0, 1e-6) && loops == 2,
        "{found:?}"
    );
    // The square ring, its hole kept: 40^2 - 20^2.
    let (area, centroid, loops) = found[2];
    assert!(
        near(area, 1200.0, 1e-6) && near(centroid[1], 35.0, 1e-9) && loops == 2,
        "{found:?}"
    );
    // The arcs are circles' arcs, exactly.
    let arcs = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "arc")
        .count();
    assert_eq!(arcs, 4);
    // The text, 5 mm high, centred at (50, 10).
    let text = &s["texts"][0];
    assert_eq!(
        (&text["text"], &text["height"], &text["at"], &text["align"]),
        (
            &json!("MIT"),
            &json!(5.0),
            &json!([50.0, 10.0]),
            &json!("center")
        )
    );
    // Without stroke_outline the stroke is its middle line: no band.
    let mut plain = with_sketch();
    let made = command(
        &mut plain,
        json!({"cmd": "sketch.import_svg", "sketch": "F1", "path": path, "scale": 2, "at": [10, 0]}),
    );
    assert_eq!(
        (&made["bands"], made["filled"].as_array().unwrap().len()),
        (&json!(0), 2)
    );
    assert_eq!(made["size"], json!([200.0, 120.0]));
}

#[test]
fn svg_fill_rules_transforms_and_ellipses() {
    use mitcad_model::sketch::svg::{Segment, SvgOptions, read};
    // Two squares one inside the other, the same way round: nonzero fills
    // the inner one too, evenodd leaves it a hole.
    let squares = |rule: &str| {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
                 <path fill-rule="{rule}" d="M0 0H100V100H0Z M25 25H75V75H25Z"/></svg>"#
        )
    };
    let inner = [50.0, 50.0];
    let nonzero = read(&squares("nonzero"), &SvgOptions::default()).unwrap();
    assert!(nonzero.shapes[0].covers(inner, false));
    let even = read(&squares("evenodd"), &SvgOptions::default()).unwrap();
    assert!(!even.shapes[0].covers(inner, false) && even.shapes[0].covers([10.0, 10.0], false));
    // A circle stretched by the viewport (preserveAspectRatio none) is an
    // ellipse 4 by 2 about (10, 5); a rotated group turns a line.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="20mm" height="10mm"
                     preserveAspectRatio="none">
                   <circle cx="5" cy="5" r="2"/>
                   <g transform="rotate(90 5 5)"><line x1="5" y1="5" x2="7" y2="5" stroke="red"/></g>
                 </svg>"#;
    let drawing = read(svg, &SvgOptions::default()).unwrap();
    let Segment::Conic(c) = &drawing.shapes[0].paths[0].segments[0] else {
        panic!("not a conic");
    };
    let a = c.u[0].hypot(c.u[1]);
    assert!(
        (a - 4.0).abs() < 1e-12 && (c.b - 2.0).abs() < 1e-12 && c.whole,
        "{c:?}"
    );
    assert!((c.center[0] - 10.0).abs() < 1e-12 && (c.center[1] - 5.0).abs() < 1e-12);
    // The line from (5, 5) turned to point down the drawing, so down the
    // sketch: from (10, 5) to (10, 3).
    let Segment::Line(p, q) = drawing.shapes[1].paths[0].segments[0] else {
        panic!("not a line");
    };
    assert!(
        (p[0] - 10.0).abs() < 1e-12 && (p[1] - 5.0).abs() < 1e-12,
        "{p:?}"
    );
    assert!(
        (q[0] - 10.0).abs() < 1e-12 && (q[1] - 3.0).abs() < 1e-12,
        "{q:?}"
    );
    assert!(read("<html/>", &SvgOptions::default()).is_err());
}

#[test]
fn a_qr_code_extrudes_and_reads_back() {
    let mut d = with_sketch();
    let url = "https://mitcad.org";
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_code", "sketch": "F1", "text": url, "at": [10, 20],
               "module": 0.5, "ecc": "Q"}),
    );
    let id = made["texts"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_eq!(
        s["texts"][0]["code"],
        json!({"kind": "qr", "ecc": "Q"}),
        "{}",
        s["texts"][0]
    );
    // The code's material: the regions its contours bound as letters.
    let letters: Vec<String> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["text"] == id.as_str() && r["letter"] == true)
        .map(|r| r["key"].as_str().unwrap().to_owned())
        .collect();
    assert!(!letters.is_empty());
    let profiles: Vec<Value> = letters
        .iter()
        .map(|k| json!({"sketch": "F1", "region": k}))
        .collect();
    let extruded = command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude", "profiles": profiles,
               "extent": {"type": "distance", "distance": 1}, "operation": "new_body"}}),
    );
    let uid: mitcad_model::ids::FeatureUid = extruded["uid"].as_str().unwrap().parse().unwrap();
    assert_eq!(d.status(uid), Some(&mitcad_model::FeatureStatus::Ok));
    // The module grid read back from the extruded regions: a module is
    // dark where its middle lies in one.
    let mitcad_model::FeatureDef::Extrude(def) = &d.feature(uid).unwrap().def else {
        panic!("not an extrusion");
    };
    let output = d.sketch_output("F1".parse().unwrap()).unwrap();
    let regions: Vec<&mitcad_model::sketch::regions::Region> = def
        .profiles
        .iter()
        .map(|p| {
            output
                .region_info
                .iter()
                .find(|r| r.profile.key == p.region)
                .unwrap()
        })
        .collect();
    let level = mitcad_model::sketch::SketchCode {
        kind: mitcad_model::sketch::CodeKind::Qr,
        ecc: mitcad_model::sketch::CodeEcc::Q,
    };
    let (width, modules) = mitcad_model::sketch::code::modules(url, &level).unwrap();
    // The regions cover the dark modules exactly: a quarter of a mm² each.
    let dark = modules.iter().filter(|m| **m).count() as f64 * 0.25;
    let covered: f64 = regions.iter().map(|r| r.area).sum();
    assert!((covered - dark).abs() < 1e-9, "{covered} {dark}");
    let grid = rqrr::SimpleGrid::from_func(width, |x, y| {
        // Row y from the top, at 0.5 mm a module from (10, 20).
        let p = [
            10.0 + (x as f64 + 0.5) * 0.5,
            20.0 + ((width - 1 - y) as f64 + 0.5) * 0.5,
        ];
        regions.iter().any(|r| {
            mitcad_model::sketch::geometry::polygon_contains(&r.outline, p)
                && !r
                    .holes
                    .iter()
                    .any(|h| mitcad_model::sketch::geometry::polygon_contains(h, p))
        })
    });
    let (_, content) = rqrr::Grid::new(grid).decode().unwrap();
    assert_eq!(content, url);

    // Edited: another content and level, turned; a content no code holds
    // is refused.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_code", "sketch": "F1", "id": id, "text": "MIT", "ecc": "H",
               "angle": 0.5}),
    );
    assert_eq!(sketch(&d)["texts"][0]["code"]["ecc"], "H");
    let why = rejected(
        &mut d,
        json!({"cmd": "sketch.edit_code", "sketch": "F1", "id": id, "text": "x".repeat(5000)}),
    );
    assert!(why.contains("no QR code holds"), "{why}");
}

#[test]
fn code_boundaries_keep_islands_and_corner_contacts_apart() {
    use mitcad_model::sketch::code::boundaries;
    // A ring of eight dark modules round a light one, and a module that
    // touches the ring's corner only.
    let dark = [
        true, true, true, false, //
        true, false, true, false, //
        true, true, true, false, //
        false, false, false, true,
    ];
    let loops = boundaries(4, &dark);
    let area = |l: &Vec<[i64; 2]>| {
        let n = l.len();
        (0..n)
            .map(|i| l[i][0] * l[(i + 1) % n][1] - l[(i + 1) % n][0] * l[i][1])
            .sum::<i64>()
            / 2
    };
    let mut areas: Vec<i64> = loops.iter().map(area).collect();
    areas.sort_unstable();
    // The ring's outside (9, counter-clockwise), its island (-1,
    // clockwise) and the lone module (1).
    assert_eq!(areas, vec![-1, 1, 9], "{loops:?}");
    // A C whose ends touch at a corner: its outside and the light module it
    // almost encloses are two loops that touch there, not one loop that
    // passes the corner twice.
    let c_shape = [
        true, true, false, //
        true, false, true, //
        true, true, true,
    ];
    let loops = boundaries(3, &c_shape);
    let mut areas: Vec<i64> = loops.iter().map(area).collect();
    areas.sort_unstable();
    assert_eq!(areas, vec![-1, 8], "{loops:?}");
    for l in &loops {
        let distinct: std::collections::BTreeSet<[i64; 2]> = l.iter().copied().collect();
        assert_eq!(distinct.len(), l.len(), "{l:?}");
    }
    // As sketch profiles: the C (7 modules) and the light module, each a
    // region of its own.
    use mitcad_model::sketch::geometry::Curve2;
    let curves: Vec<mitcad_model::sketch::regions::RegionCurve> = loops
        .iter()
        .enumerate()
        .map(|(j, l)| {
            let k = l.len();
            let parts: Vec<Curve2> = (0..k)
                .map(|a| Curve2::Line {
                    a: [l[a][0] as f64, l[a][1] as f64],
                    b: [l[(a + 1) % k][0] as f64, l[(a + 1) % k][1] as f64],
                })
                .collect();
            mitcad_model::sketch::regions::RegionCurve {
                uid: mitcad_model::topo::CurveId::Glyph {
                    text: mitcad_model::ids::EntityUid(1),
                    glyph: 0,
                    contour: j as u32,
                },
                curve: mitcad_model::sketch::text::composite(&parts).unwrap(),
                parts,
            }
        })
        .collect();
    let mut found: Vec<f64> = mitcad_model::sketch::regions::regions(&curves)
        .iter()
        .map(|r| r.area)
        .collect();
    found.sort_by(f64::total_cmp);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        (found[0] - 1.0).abs() < 1e-9 && (found[1] - 7.0).abs() < 1e-9,
        "{found:?}"
    );
    // Contours whose corners touch where a side of each runs on in one
    // line (x = 3 below and above the corner): each is a region, as the
    // sides leaving the corner go (the derivative of a composite curve on
    // the edge's own side of a knot).
    let a: Vec<[i64; 2]> = vec![[0, 8], [3, 8], [3, 9], [0, 9]];
    let b: Vec<[i64; 2]> = vec![
        [0, 10],
        [2, 10],
        [2, 11],
        [3, 11],
        [3, 9],
        [4, 9],
        [4, 13],
        [0, 13],
    ];
    let mk = |j: u32, l: &Vec<[i64; 2]>| {
        let k = l.len();
        let parts: Vec<Curve2> = (0..k)
            .map(|q| Curve2::Line {
                a: [l[q][0] as f64, l[q][1] as f64],
                b: [l[(q + 1) % k][0] as f64, l[(q + 1) % k][1] as f64],
            })
            .collect();
        mitcad_model::sketch::regions::RegionCurve {
            uid: mitcad_model::topo::CurveId::Glyph {
                text: mitcad_model::ids::EntityUid(1),
                glyph: 0,
                contour: j,
            },
            curve: mitcad_model::sketch::text::composite(&parts).unwrap(),
            parts,
        }
    };
    let mut found: Vec<f64> = mitcad_model::sketch::regions::regions(&[mk(0, &a), mk(1, &b)])
        .iter()
        .map(|r| r.area)
        .collect();
    found.sort_by(f64::total_cmp);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        (found[0] - 3.0).abs() < 1e-9 && (found[1] - 12.0).abs() < 1e-9,
        "{found:?}"
    );
}
