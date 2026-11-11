// SPDX-License-Identifier: MIT
//! Drawing templates of a company's own (mitcad#1625) on the mock kernel:
//! sheets of a custom size, images (a logo) in the title block, and the
//! title block edited on the sheet (`edit_drawing_title_block`); each kept
//! in the design file, saved as a template and exported to PDF, SVG and
//! DXF.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// A fresh folder of a test's own beside the test program in the build
/// tree, not in the system's temporary folder.
fn own_folder(name: &str) -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("the test program's path");
    let dir = exe
        .parent()
        .expect("the test program's folder")
        .join("test-folders")
        .join(format!("{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn drawing(doc: &Document<MockKernel>, id: &str) -> Value {
    query(
        doc,
        json!({"query": "drawing", "drawing": id, "title": "Bracket"}),
    )
}

/// The items of a sheet's group of this class.
fn items(answer: &Value, class: &str) -> Vec<Value> {
    answer["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["class"] == class)
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .collect()
}

/// A 40 x 30 x 20 block (F2.b0) and nothing else.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

/// CRC-32 (ISO 3309) of a PNG chunk's type and data.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for b in bytes {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A PNG file of `width` x `height` RGBA pixels of one colour, its data an
/// uncompressed ("stored") deflate block.
fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut raw = Vec::new();
    for _ in 0..height {
        // Each row starts with its filter type, 0 (none).
        raw.extend([0u8]);
        for _ in 0..width {
            raw.extend_from_slice(&rgba);
        }
    }
    assert!(raw.len() < 0xffff, "one stored block");
    let mut zlib = vec![0x78, 0x01, 0x01];
    let len = raw.len() as u16;
    zlib.extend_from_slice(&len.to_le_bytes());
    zlib.extend_from_slice(&(!len).to_le_bytes());
    zlib.extend_from_slice(&raw);
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut typed = kind.to_vec();
        typed.extend_from_slice(body);
        out.extend_from_slice(&typed);
        out.extend_from_slice(&crc32(&typed).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    out
}

fn base64(bytes: &[u8]) -> String {
    mitcad_model::library::encode_base64(bytes)
}

/// The ISO template's data as `drawing_templates` lists it, made a user's
/// template of a 500 x 300 mm sheet with a 40 x 20 logo in its block's
/// lower left corner.
fn company_template(doc: &Document<MockKernel>) -> Value {
    let mut t = query(doc, json!({"query": "drawing_templates"}))["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["choice"] == "iso")
        .unwrap()
        .clone();
    let fields = t.as_object_mut().unwrap();
    fields.remove("choice");
    t["id"] = json!("company");
    t["name"] = json!("Company");
    t["library"] = json!("user");
    t["size"] = json!({"width": 500, "height": 300});
    t["title_block"]["images"] =
        json!([{"at": [1, 1], "size": [40, 20], "data": base64(&png(4, 2, [200, 20, 20, 255]))}]);
    t
}

#[test]
fn a_custom_sheet_size_is_kept_and_exported_at_that_size() {
    let mut doc = block();
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "size": {"width": 500, "height": 300}}),
    );
    let id = added["drawing"].as_str().unwrap().to_owned();
    let answer = drawing(&doc, &id);
    let sheet = &answer["sheets"][0];
    assert_eq!(sheet["size"], "custom", "{sheet}");
    assert_eq!(
        (sheet["width"].as_f64(), sheet["height"].as_f64()),
        (Some(500.0), Some(300.0))
    );
    let listed = query(&doc, json!({"query": "drawings"}));
    assert_eq!(listed[0]["sheets"][0]["size"], "500 x 300 mm", "{listed}");
    // A size of its own is as given: the orientation does not turn it.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": id, "orientation": "portrait"}),
    );
    let sheet = drawing(&doc, &id)["sheets"][0].clone();
    assert_eq!(
        (sheet["width"].as_f64(), sheet["height"].as_f64()),
        (Some(500.0), Some(300.0))
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": id, "size": {"width": 250, "height": 400}}),
    );
    let sheet = drawing(&doc, &id)["sheets"][0].clone();
    assert_eq!(
        (sheet["width"].as_f64(), sheet["height"].as_f64()),
        (Some(250.0), Some(400.0))
    );
    // Kept in the file as given.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["drawings"][0]["sheets"][0]["size"],
        json!({"width": 250.0, "height": 400.0})
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // Exported at that size: the SVG's and the PDF's page.
    let dir = own_folder("custom-size");
    let svg = dir.join("sheet.svg");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": svg.to_string_lossy()}),
    );
    assert_eq!(exported["sheet"], "250 x 400 mm");
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(
        text.contains(r#"width="250mm" height="400mm""#),
        "{}",
        &text[..300]
    );
    let pdf = dir.join("sheet.pdf");
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": pdf.to_string_lossy()}),
    );
    let bytes = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
    // 250 and 400 mm in points.
    assert!(
        bytes.contains("/MediaBox [0 0 708.6614 1133.8583]"),
        "the PDF's page"
    );
    // Refused: a side out of range, a field it has not, a size it has not.
    for (size, message) in [
        (json!({"width": 5, "height": 300}), "10 to 10000 mm"),
        (json!({"width": 300}), "its height is missing"),
        (
            json!({"width": 300, "height": 200, "depth": 1}),
            "no field depth",
        ),
        (json!("A5"), "not A5"),
    ] {
        let error = refused(
            &mut doc,
            json!({"cmd": "edit_drawing_sheet", "drawing": id, "size": size}),
        );
        assert!(error.contains(message), "{error}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_template_with_a_custom_size_and_a_logo_round_trips_and_exports_it() {
    let mut doc = block();
    let template = company_template(&doc);
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "template": template}),
    );
    let id = added["drawing"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": id, "type": "base", "direction": "front",
               "at": [150, 150], "scale": "1:1"}),
    );
    let answer = drawing(&doc, &id);
    let sheet = &answer["sheets"][0];
    // The template's size of its own.
    assert_eq!(
        (sheet["width"].as_f64(), sheet["height"].as_f64()),
        (Some(500.0), Some(300.0))
    );
    // The logo in the title block, under its lines: fitted in its 40 x 20
    // box at its proportions (4 x 2 pixels), from the block's corner.
    let block_items = items(&answer, "title-block");
    let image = block_items
        .iter()
        .find(|i| i.get("image").is_some())
        .expect("the logo on the sheet");
    assert_eq!(image["format"], "png");
    let (lo, hi) = (sheet["title_box"][0].clone(), sheet["title_box"][1].clone());
    let corner = [lo[0].as_f64().unwrap(), lo[1].as_f64().unwrap()];
    assert!(hi[0].as_f64().unwrap() > corner[0]);
    assert_eq!(image["at"], json!([corner[0] + 1.0, corner[1] + 1.0]));
    assert_eq!(image["size"], json!([40.0, 20.0]));
    // The layout the title block editor edits.
    assert_eq!(
        sheet["title_block_layout"]["images"][0]["size"],
        json!([40.0, 20.0])
    );
    // Kept in the design file with the sheet's template.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kept = &file["drawings"][0]["sheets"][0]["template"];
    assert_eq!(kept["size"], json!({"width": 500.0, "height": 300.0}));
    assert_eq!(
        kept["title_block"]["images"][0]["data"],
        template["title_block"]["images"][0]["data"]
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // Saved to a library as a template and listed with its size and logo.
    let project = own_folder("logo-project");
    let folder = project.to_string_lossy().into_owned();
    let saved_template = command(
        &mut doc,
        json!({"cmd": "save_drawing_template", "drawing": id, "library": "project",
               "name": "Company Logo", "project": folder}),
    );
    let listed = query(
        &doc,
        json!({"query": "drawing_templates", "project": folder}),
    );
    let ours = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["choice"] == saved_template["choice"])
        .unwrap_or_else(|| panic!("{listed}"))
        .clone();
    assert_eq!(ours["size"], json!({"width": 500.0, "height": 300.0}));
    assert_eq!(ours["title_block"]["images"].as_array().unwrap().len(), 1);
    // Exported with the logo: an image object in the PDF, an <image> in
    // the SVG; the DXF has its outline, and says so.
    let dir = own_folder("logo-export");
    let pdf = dir.join("sheet.pdf");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": pdf.to_string_lossy()}),
    );
    assert!(
        !exported["warnings"].to_string().contains("outline"),
        "{exported}"
    );
    let bytes = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
    assert!(
        bytes.contains("/Subtype /Image /Width 4 /Height 2"),
        "the PDF's image"
    );
    assert!(
        bytes.contains("/XObject << /Im0 "),
        "the page's image resource"
    );
    assert!(bytes.contains("/Im0 Do"), "the image drawn");
    assert!(
        bytes.contains("/MediaBox [0 0 1417.3228 850.3937]"),
        "the PDF's page"
    );
    let svg = dir.join("sheet.svg");
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": svg.to_string_lossy()}),
    );
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains(r#"width="500mm" height="300mm""#));
    assert!(text.contains("<image "), "the SVG's image");
    assert!(
        text.contains("data:image/png;base64,"),
        "the SVG's image data"
    );
    let dxf = dir.join("sheet.dxf");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": dxf.to_string_lossy()}),
    );
    let warnings = exported["warnings"].to_string();
    assert!(warnings.contains("holds no raster images"), "{warnings}");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&project).ok();
}

/// The numbers of a PDF content line's operands.
fn operands(line: &str) -> Vec<f64> {
    line.split_whitespace()
        .filter_map(|w| w.parse::<f64>().ok())
        .collect()
}

/// The lines of the first graphics state a PDF draws in colour (an SVG
/// image's shape: `q`, its colours, its path, its painting, `Q`).
fn coloured_shapes(pdf: &str) -> Vec<Vec<String>> {
    let lines: Vec<&str> = pdf.lines().collect();
    let mut shapes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i] == "q"
            && lines[i + 1..]
                .iter()
                .take_while(|l| **l != "Q")
                .any(|l| l.ends_with(" rg") || l.contains(" RG "))
        {
            let shape: Vec<String> = lines[i + 1..]
                .iter()
                .take_while(|l| **l != "Q")
                .map(|l| (*l).to_owned())
                .collect();
            i += shape.len() + 2;
            shapes.push(shape);
        } else {
            i += 1;
        }
    }
    shapes
}

#[test]
fn an_svg_logo_is_vector_paths_in_a_pdf_and_lines_in_a_dxf() {
    // mitcad#1627: a navy band, a red triangle with a green edge and a
    // black ring, 2:1.
    let mut doc = block();
    let mut template = company_template(&doc);
    let svg = r##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
        <rect width="100" height="50" fill="#000080"/>
        <path d="M10 40 L50 10 L90 40 Z" fill="#ff0000" stroke="#008000" stroke-width="4"/>
        <circle cx="50" cy="25" r="8" fill="none" stroke="#000000" stroke-width="2"/>
    </svg>"##;
    template["title_block"]["images"] =
        json!([{"at": [0, 0], "size": [30, 30], "data": base64(svg.as_bytes())}]);
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "template": template}),
    );
    let id = added["drawing"].as_str().unwrap().to_owned();
    let answer = drawing(&doc, &id);
    let image = items(&answer, "title-block")
        .into_iter()
        .find(|i| i.get("image").is_some())
        .unwrap();
    assert_eq!(image["format"], "svg");
    // 2:1 in a 30 x 30 box: 30 x 15 in its middle, 0.3 mm per unit.
    assert_eq!(image["size"], json!([30.0, 15.0]));
    let lo = answer["sheets"][0]["title_box"][0].clone();
    let (x0, y0) = (lo[0].as_f64().unwrap(), lo[1].as_f64().unwrap() + 7.5);
    assert_eq!(image["at"][1].as_f64(), Some(y0));
    let inside = |x: f64, y: f64| {
        (x0 - 1e-3..=x0 + 30.0 + 1e-3).contains(&x) && (y0 - 1e-3..=y0 + 15.0 + 1e-3).contains(&y)
    };
    let no_image_warning = |exported: &Value| {
        let warnings = exported["warnings"].to_string();
        assert!(
            !warnings.contains("title block") && !warnings.contains("outline"),
            "{warnings}"
        );
    };
    let dir = own_folder("svg-logo");

    // The PDF: each shape a path of its own in its colours, inside the
    // box, the band's top left corner at the box's top left.
    let pdf = dir.join("sheet.pdf");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": pdf.to_string_lossy()}),
    );
    no_image_warning(&exported);
    let text = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
    assert!(!text.contains("/XObject"), "no image object");
    let shapes = coloured_shapes(&text);
    assert_eq!(shapes.len(), 3, "{shapes:?}");
    for shape in &shapes {
        for line in shape {
            if line.ends_with(" m") || line.ends_with(" l") || line.ends_with(" c") {
                let v = operands(line);
                for p in v.chunks(2) {
                    assert!(inside(p[0], p[1]), "{line} outside the logo's box");
                }
            }
        }
    }
    let band = &shapes[0];
    assert_eq!(band[0], "0 0 0.502 rg");
    let start = operands(&band[1]);
    assert!((start[0] - x0).abs() < 1e-3 && (start[1] - (y0 + 15.0)).abs() < 1e-3);
    assert_eq!(band.last().unwrap(), "f");
    let triangle = &shapes[1];
    assert_eq!(triangle[0], "1 0 0 rg");
    let stroke = operands(&triangle[1]);
    assert_eq!(&stroke[..3], &[0.0, 0.502, 0.0], "{}", triangle[1]);
    assert!((stroke[3] - 1.2).abs() < 1e-3, "4 units wide: 1.2 mm");
    assert_eq!(triangle.last().unwrap(), "B");
    let ring = &shapes[2];
    assert!(ring[0].starts_with("0 0 0 RG 0.6 w"), "{}", ring[0]);
    assert!(ring.iter().any(|l| l.ends_with(" c")), "the ring's curves");
    assert_eq!(ring.last().unwrap(), "S");

    // The DXF: the shapes' lines on a layer per colour and weight, inside
    // the box; the ring black (7), 0.6 mm wide, its points on its circle.
    let dxf = dir.join("sheet.dxf");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": dxf.to_string_lossy()}),
    );
    no_image_warning(&exported);
    let back = mitcad_dxf::read_file(&dxf).unwrap();
    let logo: Vec<&mitcad_dxf::Entity> = back
        .entities
        .iter()
        .filter(|e| e.layer.starts_with("IMAGE_"))
        .collect();
    assert!(logo.len() > 20, "{} lines", logo.len());
    let mut layers: Vec<&str> = logo.iter().map(|e| e.layer.as_str()).collect();
    layers.sort_unstable();
    layers.dedup();
    assert_eq!(layers.len(), 4, "{layers:?}");
    assert!(layers.contains(&"IMAGE_7_60"), "{layers:?}");
    assert_eq!(back.layer("IMAGE_7_60").map(|l| l.color), Some(7));
    for e in &logo {
        let mitcad_dxf::Geometry::Line { start, end } = &e.geometry else {
            panic!("{:?}", e.geometry);
        };
        for p in [start, end] {
            assert!(inside(p.x, p.y), "{p:?} outside the logo's box");
            if e.layer == "IMAGE_7_60" {
                let r = (p.x - (x0 + 15.0)).hypot(p.y - (y0 + 7.5));
                assert!((r - 2.4).abs() < 0.02, "{p:?} {r} off the ring");
            }
        }
    }

    // The DWG as the DXF, without a warning.
    let dwg = dir.join("sheet.dwg");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": dwg.to_string_lossy()}),
    );
    no_image_warning(&exported);

    // The SVG keeps the file as it is.
    let svg_file = dir.join("sheet.svg");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": svg_file.to_string_lossy()}),
    );
    no_image_warning(&exported);
    assert!(
        std::fs::read_to_string(&svg_file)
            .unwrap()
            .contains("data:image/svg+xml;base64,")
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_svg_logo_s_text_is_its_glyphs_outlines_in_a_pdf_and_a_dxf() {
    // mitcad#1637: a line of red text in the bundled font, 24 units high,
    // its baseline 35 units down a 100 x 50 logo.
    let mut doc = block();
    let mut template = company_template(&doc);
    let svg = r##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
        <text x="5" y="35" font-family="Droid Sans" font-size="24" fill="#ff0000">ACME</text>
    </svg>"##;
    template["title_block"]["images"] =
        json!([{"at": [0, 0], "size": [30, 30], "data": base64(svg.as_bytes())}]);
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "template": template}),
    );
    let id = added["drawing"].as_str().unwrap().to_owned();
    let answer = drawing(&doc, &id);
    // 2:1 in a 30 x 30 box: 30 x 15 in its middle, 0.3 mm per unit; the
    // baseline 4.5 mm above the box's bottom, the capitals' tops about
    // 0.71 of 24 units above it.
    let lo = answer["sheets"][0]["title_box"][0].clone();
    let (x0, y0) = (lo[0].as_f64().unwrap(), lo[1].as_f64().unwrap() + 7.5);
    let baseline = y0 + 15.0 - 35.0 * 0.3;
    let inside = |x: f64, y: f64| {
        (x0 - 1e-3..=x0 + 30.0 + 1e-3).contains(&x)
            && (baseline - 0.3..=y0 + 15.0 + 1e-3).contains(&y)
    };
    let no_image_warning = |exported: &Value| {
        let warnings = exported["warnings"].to_string();
        assert!(
            !warnings.contains("title block") && !warnings.contains("outline"),
            "{warnings}"
        );
    };
    // The text's extent: its left within the first letter, its right past
    // the third, its top near the capitals' height.
    let extent = |points: &[(f64, f64)]| {
        let left = points.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let right = points.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let top = points.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        assert!(left > x0 + 1.0 && left < x0 + 3.0, "{left} from {x0}");
        assert!(right > x0 + 12.0, "{right} from {x0}");
        assert!(
            (top - (baseline + 0.71 * 24.0 * 0.3)).abs() < 0.5,
            "{top} above {baseline}"
        );
    };
    let dir = own_folder("svg-text");

    // The PDF: the glyphs' outlines as filled vector paths in red, inside
    // the box, with curves, no warning.
    let pdf = dir.join("sheet.pdf");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": pdf.to_string_lossy()}),
    );
    no_image_warning(&exported);
    let text = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
    let shapes = coloured_shapes(&text);
    assert!(!shapes.is_empty(), "the text's paths");
    let mut points = Vec::new();
    let mut outlines = 0;
    let mut curves = 0;
    for shape in &shapes {
        assert_eq!(shape[0], "1 0 0 rg", "{shape:?}");
        assert_eq!(shape.last().unwrap(), "f", "filled, not stroked");
        for line in shape {
            if line.ends_with(" m") || line.ends_with(" l") || line.ends_with(" c") {
                outlines += usize::from(line.ends_with(" m"));
                curves += usize::from(line.ends_with(" c"));
                let v = operands(line);
                for p in v.chunks(2) {
                    assert!(inside(p[0], p[1]), "{line} outside the logo's box");
                    points.push((p[0], p[1]));
                }
            }
        }
    }
    // A, its counter, C, M, E.
    assert!(outlines >= 5, "{outlines} outlines");
    assert!(curves > 0, "C's curves");
    extent(&points);

    // The DXF: the outlines as lines on the red fill's layer, inside the
    // box, no warning.
    let dxf = dir.join("sheet.dxf");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": dxf.to_string_lossy()}),
    );
    no_image_warning(&exported);
    let back = mitcad_dxf::read_file(&dxf).unwrap();
    let logo: Vec<&mitcad_dxf::Entity> = back
        .entities
        .iter()
        .filter(|e| e.layer.starts_with("IMAGE_"))
        .collect();
    assert!(logo.len() > 20, "{} lines", logo.len());
    let mut points = Vec::new();
    for e in &logo {
        assert!(e.layer.starts_with("IMAGE_1_"), "red: {}", e.layer);
        let mitcad_dxf::Geometry::Line { start, end } = &e.geometry else {
            panic!("{:?}", e.geometry);
        };
        for p in [start, end] {
            assert!(inside(p.x, p.y), "{p:?} outside the logo's box");
            points.push((p.x, p.y));
        }
    }
    extent(&points);

    // The DWG as the DXF, without a warning.
    let dwg = dir.join("sheet.dwg");
    let exported = command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": id, "path": dwg.to_string_lossy()}),
    );
    no_image_warning(&exported);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_svg_logo_that_cannot_be_read_or_names_a_missing_font_is_warned_of() {
    let export = |svg: &str, name: &str| {
        let mut doc = block();
        let mut template = company_template(&doc);
        template["title_block"]["images"] =
            json!([{"at": [0, 0], "size": [30, 30], "data": base64(svg.as_bytes())}]);
        let added = command(
            &mut doc,
            json!({"cmd": "add_drawing", "template": template}),
        );
        let id = added["drawing"].as_str().unwrap().to_owned();
        let dir = own_folder(name);
        let pdf = dir.join("sheet.pdf");
        let exported = command(
            &mut doc,
            json!({"cmd": "export_drawing", "drawing": id, "path": pdf.to_string_lossy()}),
        );
        std::fs::remove_dir_all(&dir).ok();
        exported["warnings"].to_string()
    };
    // Its root element, but no SVG usvg reads: its outline.
    let broken = export(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect"#,
        "svg-broken",
    );
    assert!(broken.contains("cannot be read"), "{broken}");
    // Text in a font no computer has: drawn in the bundled font, and the
    // warning names both.
    let text = export(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 10"><rect width="5" height="5"/><text x="8" y="8" font-family="No Such Font Of MitCad Tests">ACME</text></svg>"#,
        "svg-missing-font",
    );
    assert!(
        text.contains("fonts this computer does not have (No Such Font Of MitCad Tests)"),
        "{text}"
    );
    assert!(text.contains("draws their text in Droid Sans"), "{text}");
    // Text that names no font is in the bundled one: no warning.
    let plain = export(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 10"><rect width="5" height="5"/><text x="8" y="8">ACME</text></svg>"#,
        "svg-plain-text",
    );
    assert!(!plain.contains("title block"), "{plain}");
}

#[test]
fn a_title_block_is_edited_on_the_sheet() {
    let mut doc = block();
    command(&mut doc, json!({"cmd": "add_drawing", "template": "iso"}));
    let mut layout = drawing(&doc, "D1")["sheets"][0]["title_block_layout"].clone();
    let lines = layout["lines"].as_array().unwrap().len();
    let fields = layout["fields"].as_array().unwrap().len();
    // A line, a field of the material and a PNG logo added.
    layout["lines"]
        .as_array_mut()
        .unwrap()
        .push(json!([0, 30, 50, 30]));
    layout["fields"].as_array_mut().unwrap().push(
        json!({"label": "STOCK", "text": "{material}", "at": [0, 30], "size": [50, 6], "height": 2.5}),
    );
    layout["images"] =
        json!([{"at": [2, 31], "size": [10, 4], "data": base64(&png(2, 1, [0, 0, 0, 255]))}]);
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": layout}),
    );
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Edit title block of Sheet1 of Drawing1"
    );
    let answer = drawing(&doc, "D1");
    let sheet = &answer["sheets"][0];
    assert_eq!(
        sheet["title_block_layout"]["lines"]
            .as_array()
            .unwrap()
            .len(),
        lines + 1
    );
    assert_eq!(
        sheet["title_block_layout"]["fields"]
            .as_array()
            .unwrap()
            .len(),
        fields + 1
    );
    // The new field filled in from the design as the others are.
    assert!(
        sheet["title_fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["label"] == "STOCK"),
        "{}",
        sheet["title_fields"]
    );
    assert!(
        items(&answer, "title-block")
            .iter()
            .any(|i| i.get("image").is_some())
    );
    // The sheet keeps its template's choice.
    assert_eq!(sheet["template"]["choice"], "iso");
    // Undone.
    command(&mut doc, json!({"cmd": "undo"}));
    let undone = drawing(&doc, "D1")["sheets"][0]["title_block_layout"].clone();
    assert_eq!(undone["lines"].as_array().unwrap().len(), lines);
    // Refused: a field without a cell, an image that is none, a block
    // larger than the frame; a sheet it has not.
    let mut bad = undone.clone();
    bad["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"text": "x", "at": [0, 0], "size": [0, 5]}));
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": bad}),
    );
    assert!(error.contains("has no cell"), "{error}");
    let mut bad = undone.clone();
    bad["images"] = json!([{"at": [0, 0], "size": [5, 5], "data": base64(b"no image")}]);
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": bad}),
    );
    assert!(error.contains("no PNG, JPEG or SVG image"), "{error}");
    let mut bad = undone.clone();
    bad["width"] = json!(1000);
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": bad}),
    );
    assert!(error.contains("do not fit"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "sheet": 3,
               "title_block": undone}),
    );
    assert!(error.contains("has no sheet 3"), "{error}");
}

#[test]
fn a_simple_title_block_edited_becomes_a_template_of_the_sheets_own() {
    let mut doc = block();
    command(&mut doc, json!({"cmd": "add_drawing"}));
    let answer = drawing(&doc, "D1");
    let sheet = &answer["sheets"][0];
    assert!(sheet["template"].is_null());
    // The editor starts from the ISO template's block.
    let layout = sheet["title_block_layout"].clone();
    assert_eq!(layout["width"], 180.0);
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": layout}),
    );
    let sheet = drawing(&doc, "D1")["sheets"][0].clone();
    assert_eq!(sheet["template"]["id"], "custom");
    assert_eq!(sheet["template"]["name"], "Custom");
    // Now it can be saved as a template.
    let project = own_folder("simple-edited");
    let saved = command(
        &mut doc,
        json!({"cmd": "save_drawing_template", "drawing": "D1", "library": "project",
               "name": "Ours", "project": project.to_string_lossy()}),
    );
    assert_eq!(saved["choice"], "project:ours");
    std::fs::remove_dir_all(&project).ok();
}
