// SPDX-License-Identifier: MIT
//! Parameters in a drawing's texts (mitcad#2483) on the mock kernel: a
//! text note on the sheet, a leader note and a text in a view, and a title
//! block field refer to the design's parameters as `{param:Name}`; the
//! sheet and its PDF, SVG, DXF and DWG exports show their values and follow
//! Edit Parameters; the references are kept in the design file and in a
//! template saved from the sheet, follow a rename, and stand as written,
//! with a warning, where the design lacks the parameter.

use serde_json::{Value, json};

use mitcad_dxf::Geometry;

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
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

/// A 40 x 30 x 20 block (F2.b0) and nothing else; its first edge is the
/// upright one at the origin (a leader note picks it, [`side_edge`]).
fn bare_block() -> Document<MockKernel> {
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
    doc.kernel().lists_geometry.set(true);
    let edge = side_edge(&doc);
    doc.kernel().edge_curves.borrow_mut().insert(
        edge,
        CurveGeometry::Line {
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 20.0],
        },
    );
    doc
}

/// The name of the block's first edge, the upright one at the origin.
fn side_edge(doc: &Document<MockKernel>) -> String {
    let body: BodyUid = "F2.b0".parse().unwrap();
    doc.kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .find_map(|e| e.name)
        .expect("an edge of the block")
}

/// The block with the parameters Length (120 mm), Maker (`ACME`, a text)
/// and Tilt (15°).
fn block() -> Document<MockKernel> {
    let mut doc = bare_block();
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "Length", "expression": "120 mm"},
            {"cmd": "add_parameter", "name": "Maker", "expression": "'ACME'"},
            {"cmd": "add_parameter", "name": "Tilt", "expression": "15 deg"},
        ]),
    );
    doc
}

/// A drawing of the block with the ISO template and a front view; its id.
fn add_drawing(doc: &mut Document<MockKernel>) -> String {
    let added = command(doc, json!({"cmd": "add_drawing", "template": "iso"}));
    let id = added["drawing"].as_str().unwrap().to_owned();
    command(
        doc,
        json!({"cmd": "add_drawing_view", "drawing": id, "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    id
}

fn sheet(doc: &Document<MockKernel>, id: &str) -> Value {
    query(doc, json!({"query": "drawing", "drawing": id}))["sheets"][0].clone()
}

/// A sheet's annotation entry of this id.
fn entry(sheet: &Value, id: u64) -> Value {
    sheet["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}: {sheet}"))
}

/// The texts drawn on a sheet.
fn texts(sheet: &Value) -> Vec<String> {
    sheet["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

/// A title block field's value, by its label.
fn field(sheet: &Value, label: &str) -> String {
    sheet["title_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"] == label)
        .unwrap_or_else(|| panic!("no field {label}: {}", sheet["title_fields"]))["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The sheet's title block with the TITLE field's text replaced, as the
/// title block editor sends it.
fn title_block_with(sheet: &Value, text: &str) -> Value {
    let mut layout = sheet["title_block_layout"].clone();
    for f in layout["fields"].as_array_mut().unwrap() {
        if f["label"] == "TITLE" {
            f["text"] = json!(text);
        }
    }
    layout
}

/// The text note (1), the leader note (2) and the text in the view (3) of
/// a drawing, and its TITLE field, each with its references.
fn annotate(doc: &mut Document<MockKernel>, id: &str) {
    command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": id, "type": "text", "at": [30, 60],
               "text": "LENGTH {param:Length} BY {param:Maker}"}),
    );
    let edge = side_edge(doc);
    command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": id, "view": 1, "type": "note",
               "picks": [{"body": "F2.b0", "edge": edge}],
               "text": "MADE BY {param:Maker}\nTILT {param:Tilt}"}),
    );
    command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": id, "view": 1, "type": "text",
               "at": [60, 90], "text": "{param:Length} {title}"}),
    );
    let layout = title_block_with(&sheet(doc, id), "{param:Maker} L={param:Length}");
    command(
        doc,
        json!({"cmd": "edit_drawing_title_block", "drawing": id, "title_block": layout}),
    );
}

struct Exported {
    svg: String,
    pdf: String,
    dxf_texts: Vec<String>,
    dwg_texts: Vec<String>,
}

fn export(doc: &mut Document<MockKernel>, id: &str, folder: &str) -> Exported {
    let dir = own_folder(folder);
    let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
    for name in ["sheet.svg", "sheet.pdf", "sheet.dxf", "sheet.dwg"] {
        command(
            doc,
            json!({"cmd": "export_drawing", "drawing": id, "path": path(name)}),
        );
    }
    let svg = std::fs::read_to_string(path("sheet.svg")).unwrap();
    let pdf = String::from_utf8_lossy(&std::fs::read(path("sheet.pdf")).unwrap()).into_owned();
    let words = |entities: &[mitcad_dxf::Entity]| -> Vec<String> {
        entities
            .iter()
            .filter_map(|e| match &e.geometry {
                Geometry::Text(t) => Some(t.content.clone()),
                _ => None,
            })
            .collect()
    };
    let dxf_texts = words(&mitcad_dxf::read_file(path("sheet.dxf")).unwrap().entities);
    let dwg = mitcad_dwg::read(&std::fs::read(path("sheet.dwg")).unwrap()).unwrap();
    let dwg_texts = words(&mitcad_dwg::flat::drawing(&dwg).entities);
    std::fs::remove_dir_all(&dir).ok();
    Exported {
        svg,
        pdf,
        dxf_texts,
        dwg_texts,
    }
}

#[test]
fn notes_texts_and_title_fields_show_their_parameters_and_follow_them() {
    let mut doc = block();
    let id = add_drawing(&mut doc);
    annotate(&mut doc, &id);
    let s = sheet(&doc, &id);
    // Literal words, a text parameter and a number with its unit.
    let text = entry(&s, 1);
    assert_eq!(text["label"], "LENGTH 120 mm BY ACME", "{text}");
    // The entry keeps the text as written, for its editor.
    assert_eq!(text["text"], "LENGTH {param:Length} BY {param:Maker}");
    let note = entry(&s, 2);
    assert_eq!(note["label"], "MADE BY ACME / TILT 15°", "{note}");
    assert_eq!(note["text"], "MADE BY {param:Maker}\nTILT {param:Tilt}");
    // A text's other braces are its own words.
    let in_view = entry(&s, 3);
    assert_eq!(in_view["label"], "120 mm {title}", "{in_view}");
    assert_eq!(field(&s, "TITLE"), "ACME L=120 mm");
    let drawn = texts(&s);
    for expected in [
        "LENGTH 120 mm BY ACME",
        "MADE BY ACME",
        "TILT 15°",
        "120 mm {title}",
        "ACME L=120 mm",
    ] {
        assert!(drawn.iter().any(|t| t == expected), "{expected}: {drawn:?}");
    }
    assert!(
        !s["warnings"].to_string().contains("no parameter"),
        "{}",
        s["warnings"]
    );

    // Edit Parameters: the sheet follows.
    command(
        &mut doc,
        json!([
            {"cmd": "set_parameter", "name": "Length", "expression": "150 mm"},
            {"cmd": "set_parameter", "name": "Maker", "expression": "'Globex'"},
        ]),
    );
    let s = sheet(&doc, &id);
    assert_eq!(entry(&s, 1)["label"], "LENGTH 150 mm BY Globex");
    assert_eq!(entry(&s, 2)["label"], "MADE BY Globex / TILT 15°");
    assert_eq!(entry(&s, 3)["label"], "150 mm {title}");
    assert_eq!(field(&s, "TITLE"), "Globex L=150 mm");

    // And so do its exports.
    let files = export(&mut doc, &id, "parameter-export");
    for expected in [
        "LENGTH 150 mm BY Globex",
        "Globex L=150 mm",
        "MADE BY Globex",
    ] {
        assert!(files.svg.contains(expected), "the SVG's {expected}");
        assert!(
            files.pdf.contains(&format!("({expected}) Tj")),
            "the PDF's {expected}"
        );
        assert!(
            files.dxf_texts.iter().any(|t| t.contains(expected)),
            "the DXF's {expected}: {:?}",
            files.dxf_texts
        );
        assert!(
            files.dwg_texts.iter().any(|t| t.contains(expected)),
            "the DWG's {expected}: {:?}",
            files.dwg_texts
        );
    }
    assert!(!files.svg.contains("{param:"), "no reference left");

    // A renamed parameter takes its references along.
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "Maker", "new_name": "Vendor"}),
    );
    let s = sheet(&doc, &id);
    assert_eq!(
        entry(&s, 1)["text"],
        "LENGTH {param:Length} BY {param:Vendor}"
    );
    assert_eq!(
        entry(&s, 2)["text"],
        "MADE BY {param:Vendor}\nTILT {param:Tilt}"
    );
    assert_eq!(entry(&s, 1)["label"], "LENGTH 150 mm BY Globex");
    let kept = s["title_block_layout"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"] == "TITLE")
        .unwrap()["text"]
        .clone();
    assert_eq!(kept, "{param:Vendor} L={param:Length}");
    assert_eq!(field(&s, "TITLE"), "Globex L=150 mm");
}

#[test]
fn two_drawings_follow_their_own_parameters_and_keep_them_in_the_file() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Width", "expression": "80 mm"}),
    );
    let first = add_drawing(&mut doc);
    let second = add_drawing(&mut doc);
    for (id, name) in [(&first, "Length"), (&second, "Width")] {
        command(
            &mut doc,
            json!({"cmd": "add_drawing_annotation", "drawing": id, "type": "text",
                   "at": [30, 60], "text": format!("SIZE {{param:{name}}}")}),
        );
    }
    let label = |doc: &Document<MockKernel>, id: &str| entry(&sheet(doc, id), 1)["label"].clone();
    assert_eq!(label(&doc, &first), "SIZE 120 mm");
    assert_eq!(label(&doc, &second), "SIZE 80 mm");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "expression": "90 mm"}),
    );
    assert_eq!(label(&doc, &first), "SIZE 120 mm");
    assert_eq!(label(&doc, &second), "SIZE 90 mm");

    // Saved and opened again: the references as written, the same values.
    let saved = doc.to_json();
    assert!(saved.contains("SIZE {param:Width}"), "kept as written");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(label(&loaded, &first), "SIZE 120 mm");
    assert_eq!(label(&loaded, &second), "SIZE 90 mm");
}

#[test]
fn a_template_keeps_its_references_and_shows_a_missing_parameter() {
    let mut doc = block();
    let id = add_drawing(&mut doc);
    annotate(&mut doc, &id);
    // Save as Template: the field's references go with it.
    let project = own_folder("parameter-template");
    let folder = project.to_string_lossy().into_owned();
    let saved = command(
        &mut doc,
        json!({"cmd": "save_drawing_template", "drawing": id, "library": "project",
               "name": "With Parameters", "project": folder}),
    );
    let listed = query(
        &doc,
        json!({"query": "drawing_templates", "project": folder}),
    );
    let mut template = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["choice"] == saved["choice"])
        .unwrap_or_else(|| panic!("{listed}"))
        .clone();
    template.as_object_mut().unwrap().remove("choice");
    assert!(
        template["title_block"]["fields"]
            .to_string()
            .contains("{param:Maker} L={param:Length}"),
        "{template}"
    );

    // In a design that has Length but not Maker: the drawing opens, the
    // missing reference stands as written and the sheet says why.
    let mut other = bare_block();
    command(
        &mut other,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "75 mm"}),
    );
    let added = command(
        &mut other,
        json!({"cmd": "add_drawing", "template": template}),
    );
    let other_id = added["drawing"].as_str().unwrap().to_owned();
    let s = sheet(&other, &other_id);
    assert_eq!(field(&s, "TITLE"), "{param:Maker} L=75 mm");
    let warnings = s["warnings"].to_string();
    assert!(
        warnings.contains("title block: the design has no parameter Maker"),
        "{warnings}"
    );
    // It can still be edited.
    let layout = title_block_with(&s, "{param:Length} ONLY");
    command(
        &mut other,
        json!({"cmd": "edit_drawing_title_block", "drawing": other_id, "title_block": layout}),
    );
    let s = sheet(&other, &other_id);
    assert_eq!(field(&s, "TITLE"), "75 mm ONLY");
    assert!(
        !s["warnings"].to_string().contains("no parameter"),
        "{}",
        s["warnings"]
    );

    // A note of a parameter the design lacks: its reference as written and
    // a warning on it.
    command(
        &mut other,
        json!({"cmd": "add_drawing_annotation", "drawing": other_id, "type": "text",
               "at": [30, 60], "text": "BY {param:Maker}, {param:Finish}"}),
    );
    let s = sheet(&other, &other_id);
    let note = s["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["type"] == "text")
        .unwrap()
        .clone();
    assert_eq!(note["label"], "BY {param:Maker}, {param:Finish}");
    assert_eq!(note["missing_parameters"], json!(["Maker", "Finish"]));
    assert_eq!(note["level"], "warning", "{note}");
    assert!(
        note["warning"]
            .as_str()
            .unwrap()
            .contains("no parameters Maker, Finish"),
        "{note}"
    );
    std::fs::remove_dir_all(&project).ok();
}
