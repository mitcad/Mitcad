// SPDX-License-Identifier: MIT
//! A drawing's own hole callout (mitcad#2054) in the exported files: its
//! hole's lines with its own texts before and after them are in the PDF,
//! the SVG, the DXF and the DWG, as the sheet draws them.

use serde_json::{Value, json};

use mitcad_dxf::Geometry;
use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
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

/// A 20 x 20 x 20 block (F2.b0) with a counterbored 6.6 mm hole through it
/// at (10, 10) (F3, its wall a cylinder along Z); a drawing with the top
/// view at 1:1 and on it a hole callout on the hole, `TYP` before its
/// lines and `2 PLACES` after them.
fn called_out() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "add_feature", "def": {"type": "hole",
              "placement": {"type": "face", "body": "F2.b0", "face": format!("F2:end({R})"),
                            "points": [[10, 10, 20]]},
              "diameter": 6.6, "extent": {"type": "through_all"},
              "kind": {"type": "counterbore", "diameter": 11, "depth": 6.8}}},
        ]),
    );
    doc.kernel().lists_geometry.set(true);
    doc.kernel().surfaces.borrow_mut().insert(
        "F3:hole0.wall".to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [10.0, 10.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 3.3,
        },
    );
    // The hole's rim at the block's top a circle, as a kernel gives it: the
    // drawing sees the round wall end on by it.
    let body: BodyUid = "F2.b0".parse().unwrap();
    let rim = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .find(|n| n.contains("F3:hole0.wall"))
        .expect("an edge of the hole's wall");
    doc.kernel().edge_curves.borrow_mut().insert(
        rim,
        CurveGeometry::Circle {
            center: [10.0, 10.0, 20.0],
            normal: [0.0, 0.0, 1.0],
            radius: 3.3,
            start: [13.3, 10.0, 20.0],
            end: [13.3, 10.0, 20.0],
        },
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "hole_callout",
               "picks": [{"body": "F2.b0", "face": "F3:hole0.wall"}], "at": [185, 185],
               "format": {"before": "TYP", "after": "2 PLACES"}}),
    );
    let sheet = &query(&doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0];
    let callout = &sheet["annotations"][0];
    assert_eq!(callout["broken"], false, "{callout}");
    assert_eq!(
        callout["label"], "TYP Ø6.6 THRU / ⌴ Ø11 ↧ 6.8 2 PLACES",
        "{callout}"
    );
    doc
}

#[test]
fn a_hole_callout_is_exported_with_its_own_texts() {
    let mut doc = called_out();
    let dir = own_folder("hole-callout-export");
    let export = |doc: &mut Document<MockKernel>, name: &str| {
        let path = dir.join(name);
        command(
            doc,
            json!({"cmd": "export_drawing", "drawing": "D1", "path": path.to_string_lossy()}),
        );
        path
    };

    // The PDF: each line a text of its own, in WinAnsi (Ø as 0xD8).
    let pdf = std::fs::read(export(&mut doc, "sheet.pdf")).unwrap();
    let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"(TYP \xd86.6 THRU)"), "the first line in the PDF");
    assert!(has(b" 6.8 2 PLACES)"), "the last line in the PDF");

    // The SVG as the sheet writes it.
    let svg = std::fs::read_to_string(export(&mut doc, "sheet.svg")).unwrap();
    assert!(svg.contains("TYP Ø6.6 THRU"), "the first line in the SVG");
    assert!(svg.contains("6.8 2 PLACES"), "the last line in the SVG");

    // The DXF and the DWG: texts with the lines.
    let dxf = mitcad_dxf::read_file(export(&mut doc, "sheet.dxf")).unwrap();
    let dwg_bytes = std::fs::read(export(&mut doc, "sheet.dwg")).unwrap();
    let dwg_file = mitcad_dwg::read(&dwg_bytes).unwrap();
    let dwg = mitcad_dwg::flat::drawing(&dwg_file);
    for (what, drawing) in [("DXF", &dxf), ("DWG", &dwg)] {
        let texts: Vec<&str> = drawing
            .entities
            .iter()
            .filter_map(|e| match &e.geometry {
                Geometry::Text(t) => Some(t.content.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|t| t.contains("TYP") && t.contains("6.6 THRU")),
            "the first line in the {what}: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("6.8 2 PLACES")),
            "the last line in the {what}: {texts:?}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}
