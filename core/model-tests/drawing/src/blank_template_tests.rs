// SPDX-License-Identifier: MIT
//! Sheets without a frame or a title block (mitcad#2325) on the mock
//! kernel: the built-in Blank template and a template with only
//! `show_frame` false draw and export (PDF, DXF, SVG) without the groups
//! they leave out, their views using the room; the title block's values
//! stay in the design; `edit_drawing_title_block` refuses a sheet without
//! a block unless it puts the block back; an old template reads as
//! before.

use serde_json::{Value, json};

use mitcad_dxf::Geometry;
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

/// The classes of the first sheet's groups.
fn classes(doc: &Document<MockKernel>, id: &str) -> Vec<String> {
    drawing(doc, id)["sheets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["class"].as_str().unwrap().to_owned())
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

/// The middle of a new top view's box at 1:1, placed where the sheet's
/// views' area has its middle.
fn view_middle(doc: &mut Document<MockKernel>, id: &str) -> [f64; 2] {
    let view = command(
        doc,
        json!({"cmd": "add_drawing_view", "drawing": id, "type": "base", "direction": "top",
               "scale": "1:1"}),
    )["view"]
        .as_u64()
        .unwrap();
    let views = drawing(doc, id)["sheets"][0]["views"].clone();
    let shown = views
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == view)
        .cloned()
        .unwrap_or_else(|| panic!("no view {view}: {views}"));
    let b = &shown["box"];
    let at = |i: usize, k: usize| b[i][k].as_f64().unwrap();
    [(at(0, 0) + at(1, 0)) / 2.0, (at(0, 1) + at(1, 1)) / 2.0]
}

/// What the exported files have: the SVG's group classes, whether the
/// PDF draws the ISO frame's rectangle on an A3 sheet, the bounds of the
/// DXF's `FRAME` layer (none without anything on it) and whether a text
/// is in the PDF and in the DXF.
struct Exported {
    svg: String,
    pdf: String,
    frame_layer: Option<([f64; 2], [f64; 2])>,
    dxf_texts: Vec<String>,
}

fn export(doc: &mut Document<MockKernel>, id: &str, folder: &str) -> Exported {
    let dir = own_folder(folder);
    let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
    for name in ["sheet.svg", "sheet.pdf", "sheet.dxf"] {
        command(
            doc,
            json!({"cmd": "export_drawing", "drawing": id, "path": path(name)}),
        );
    }
    let svg = std::fs::read_to_string(path("sheet.svg")).unwrap();
    let pdf = String::from_utf8_lossy(&std::fs::read(path("sheet.pdf")).unwrap()).into_owned();
    let dxf = mitcad_dxf::read_file(path("sheet.dxf")).unwrap();
    let frame_layer = dxf
        .on_layers(&["FRAME".to_owned()])
        .bounds()
        .map(|(lo, hi)| ([lo.x, lo.y], [hi.x, hi.y]));
    let dxf_texts = dxf
        .entities
        .iter()
        .filter_map(|e| match &e.geometry {
            Geometry::Text(t) => Some(t.content.clone()),
            _ => None,
        })
        .collect();
    std::fs::remove_dir_all(&dir).ok();
    Exported {
        svg,
        pdf,
        frame_layer,
        dxf_texts,
    }
}

/// The ISO frame's rectangle on an A3 sheet as the PDF draws it (mm).
const ISO_FRAME: &str = "20 10 m 410 10 l 410 287 l 20 287 l s";

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

#[test]
fn a_blank_sheet_draws_and_exports_no_frame_and_no_title_block() {
    let mut doc = block();
    // Blank is listed with the built-in ones, after ISO and ANSI.
    let listed = query(&doc, json!({"query": "drawing_templates"}));
    let choices: Vec<&str> = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["library"] == "builtin")
        .map(|t| t["choice"].as_str().unwrap())
        .collect();
    assert_eq!(choices, ["iso", "ansi", "blank"], "{listed}");
    let blank = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["choice"] == "blank")
        .unwrap();
    assert_eq!(blank["name"], "Blank");
    assert_eq!(blank["show_frame"], false);
    assert_eq!(blank["show_title_block"], false);

    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing", "size": "A3", "template": "blank",
               "title_block": {"title": "Kept", "number": "N-1"}}),
    );
    let id = added["drawing"].as_str().unwrap().to_owned();
    let answer = drawing(&doc, &id);
    let sheet = &answer["sheets"][0];
    assert_eq!(sheet["template"]["choice"], "blank", "{sheet}");
    assert_eq!(sheet["template"]["show_frame"], false);
    assert_eq!(sheet["template"]["show_title_block"], false);
    // No fields drawn; the values given stay in the design.
    assert_eq!(sheet["title_fields"], json!([]), "{sheet}");
    assert_eq!(sheet["title_block"]["title"], "Kept");
    assert_eq!(sheet["title_block"]["number"], "N-1");
    // A revision draws no table either.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_revision", "drawing": id, "description": "First"}),
    );
    // The views' area is the whole sheet within Blank's 10 mm margins:
    // 10 to 287 high, its middle at 148.5.
    let middle = view_middle(&mut doc, &id);
    assert!((middle[1] - 148.5).abs() < 1e-6, "{middle:?}");
    let drawn = classes(&doc, &id);
    for left_out in ["frame", "title-block", "revision-table"] {
        assert!(
            !drawn.iter().any(|c| c == left_out),
            "{left_out}: {drawn:?}"
        );
    }
    assert!(drawn.iter().any(|c| c == "view"), "{drawn:?}");

    let files = export(&mut doc, &id, "blank-export");
    assert!(
        !files.svg.contains(r#"<g class="frame""#),
        "the SVG's frame"
    );
    assert!(!files.svg.contains(r#"<g class="title-block""#));
    assert!(!files.svg.contains(r#"<g class="revision-table""#));
    assert!(files.svg.contains(r#"<g class="view""#), "the SVG's view");
    assert!(!files.pdf.contains(ISO_FRAME), "the PDF's frame");
    // Texts drawn on the page (`(…) Tj`): the sheet's title is also the
    // PDF's `/Title` in its document information, which stays.
    assert!(!files.pdf.contains("(TITLE) Tj"), "the PDF's title block");
    assert!(!files.pdf.contains("(Kept) Tj"), "the PDF's title");
    assert_eq!(files.frame_layer, None, "nothing on the DXF's FRAME layer");
    assert!(
        !files.dxf_texts.iter().any(|t| t == "Kept" || t == "TITLE"),
        "{:?}",
        files.dxf_texts
    );

    // Back to ISO: the frame, the block and the values kept.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": id, "template": "iso"}),
    );
    let drawn = classes(&doc, &id);
    assert!(drawn.iter().any(|c| c == "frame"), "{drawn:?}");
    assert!(drawn.iter().any(|c| c == "title-block"), "{drawn:?}");
    assert!(drawn.iter().any(|c| c == "revision-table"), "{drawn:?}");
    let fields = drawing(&doc, &id)["sheets"][0]["title_fields"].clone();
    assert!(
        fields
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["label"] == "TITLE" && f["text"] == "Kept"),
        "{fields}"
    );
    let files = export(&mut doc, &id, "blank-iso-export");
    assert!(files.svg.contains(r#"<g class="frame""#));
    assert!(files.pdf.contains(ISO_FRAME), "the PDF's frame");
    assert!(files.pdf.contains("(Kept) Tj"), "the PDF's title");
    let (lo, hi) = files.frame_layer.expect("the DXF's frame");
    assert!(
        near(lo, [20.0, 10.0]) && near(hi, [410.0, 287.0]),
        "{lo:?} {hi:?}"
    );
}

#[test]
fn a_template_without_its_frame_draws_and_exports_its_block_only() {
    let mut doc = block();
    // ISO as a user's template without its frame.
    let mut t = query(&doc, json!({"query": "drawing_templates"}))["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["choice"] == "iso")
        .unwrap()
        .clone();
    t.as_object_mut().unwrap().remove("choice");
    t["id"] = json!("no-frame");
    t["name"] = json!("No Frame");
    t["library"] = json!("user");
    t["show_frame"] = json!(false);
    let added = command(&mut doc, json!({"cmd": "add_drawing", "template": t}));
    let id = added["drawing"].as_str().unwrap().to_owned();
    let answer = drawing(&doc, &id);
    let sheet = &answer["sheets"][0];
    assert_eq!(sheet["template"]["show_frame"], false, "{sheet}");
    assert_eq!(sheet["template"]["show_title_block"], true);
    assert!(
        !sheet["title_fields"].as_array().unwrap().is_empty(),
        "{sheet}"
    );
    // The views' area: up to the margin at the top (no frame line to keep
    // off), above the block and the revision table's room below: 75 to
    // 287 on A3, its middle at 181.
    let middle = view_middle(&mut doc, &id);
    assert!((middle[1] - 181.0).abs() < 1e-6, "{middle:?}");
    let drawn = classes(&doc, &id);
    assert!(!drawn.iter().any(|c| c == "frame"), "{drawn:?}");
    assert!(drawn.iter().any(|c| c == "title-block"), "{drawn:?}");

    let files = export(&mut doc, &id, "no-frame-export");
    assert!(
        !files.svg.contains(r#"<g class="frame""#),
        "the SVG's frame"
    );
    assert!(files.svg.contains(r#"<g class="title-block""#));
    assert!(!files.pdf.contains(ISO_FRAME), "the PDF's frame");
    assert!(
        files.pdf.contains("(DRAWING NO.) Tj"),
        "the PDF's title block"
    );
    // The DXF's FRAME layer: the block's outline only.
    let (lo, hi) = files.frame_layer.expect("the block's outline");
    assert!(
        near(lo, [230.0, 10.0]) && near(hi, [410.0, 46.0]),
        "{lo:?} {hi:?}"
    );
    assert!(
        files.dxf_texts.iter().any(|t| t == "DRAWING NO."),
        "{:?}",
        files.dxf_texts
    );
}

#[test]
fn the_title_block_editor_turns_the_block_and_the_frame_on_and_off() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_drawing", "size": "A3", "template": "blank"}),
    );
    let layout = drawing(&doc, "D1")["sheets"][0]["title_block_layout"].clone();
    // Blank keeps an empty block of the default size for the editor.
    assert_eq!(layout["width"], 180.0, "{layout}");
    assert_eq!(layout["height"], 36.0, "{layout}");
    // A sheet without a title block has none to edit…
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": layout}),
    );
    assert!(error.contains("has no title block"), "{error}");
    // …unless the command puts it back.
    let mut with_field = layout.clone();
    with_field["fields"] =
        json!([{"label": "TITLE", "text": "{title}", "at": [0, 0], "size": [180, 36]}]);
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": with_field,
               "show_title_block": true}),
    );
    let sheet = drawing(&doc, "D1")["sheets"][0].clone();
    assert_eq!(sheet["template"]["show_title_block"], true, "{sheet}");
    assert_eq!(sheet["template"]["show_frame"], false, "{sheet}");
    let drawn = classes(&doc, "D1");
    assert!(drawn.iter().any(|c| c == "title-block"), "{drawn:?}");
    assert!(!drawn.iter().any(|c| c == "frame"), "{drawn:?}");
    // The frame on, the block off again: its layout kept.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": "D1", "title_block": with_field,
               "show_frame": true, "show_title_block": false}),
    );
    let sheet = drawing(&doc, "D1")["sheets"][0].clone();
    assert_eq!(sheet["template"]["show_frame"], true, "{sheet}");
    assert_eq!(sheet["template"]["show_title_block"], false, "{sheet}");
    assert_eq!(
        sheet["title_block_layout"]["fields"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let drawn = classes(&doc, "D1");
    assert!(drawn.iter().any(|c| c == "frame"), "{drawn:?}");
    assert!(!drawn.iter().any(|c| c == "title-block"), "{drawn:?}");
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Edit title block of Sheet1 of Drawing1"
    );
}

#[test]
fn an_old_template_reads_and_draws_as_before() {
    // A design whose sheets have the ISO template as every design before
    // mitcad#2325 kept it: no `show_frame`, no `show_title_block`.
    let mut doc = block();
    command(&mut doc, json!({"cmd": "add_drawing", "template": "iso"}));
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let template = &file["drawings"][0]["sheets"][0]["template"];
    assert_eq!(template["id"], "iso", "{template}");
    assert!(template.get("show_frame").is_none(), "{template}");
    assert!(template.get("show_title_block").is_none(), "{template}");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    let drawn = classes(&loaded, "D1");
    assert!(drawn.iter().any(|c| c == "frame"), "{drawn:?}");
    assert!(drawn.iter().any(|c| c == "title-block"), "{drawn:?}");
    let sheet = drawing(&loaded, "D1")["sheets"][0].clone();
    assert_eq!(sheet["template"]["show_frame"], true, "{sheet}");
    assert_eq!(sheet["template"]["show_title_block"], true, "{sheet}");
}
