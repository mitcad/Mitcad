// SPDX-License-Identifier: MIT
//! A laid-out drawing of an assembly (mitcad#2147, GitHub #527): each body
//! drawn at every placement the 3D view shows, a hidden occurrence's left
//! out, and the export's answer counting the placements as well as the
//! bodies.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn run(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&text).unwrap()
}

/// A root block, 40 x 30 x 20 at the origin (F2.b0), and Pin, a 10 x 10 x
/// 10 block (F4.b0), placed at x = 100 (Pin:1), 200 (Pin:2) and 300
/// (Pin:3).
fn pins() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    run(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    let first = run(
        &mut doc,
        json!({"cmd": "create_component", "name": "Pin",
               "transform": {"translation": [100, 0, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    run(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    for x in [200, 300] {
        run(
            &mut doc,
            json!({"cmd": "copy_occurrence", "occurrence": first,
                   "transform": {"translation": [x, 0, 0]}}),
        );
    }
    doc
}

/// The x coordinates on the sheet of the lines of the SVG's view `id`
/// that run along the sheet's y axis, each once, in order.
fn upright_lines(svg: &str, id: &str) -> Vec<f64> {
    let start = svg
        .find(&format!(r#"id="{id}">"#))
        .unwrap_or_else(|| panic!("no {id} in the SVG"));
    let rest = &svg[start..];
    let group = &rest[..rest.find("</g>").unwrap()];
    let attribute = |line: &str, name: &str| -> Option<f64> {
        let key = format!(r#" {name}=""#);
        let at = line.find(&key)? + key.len();
        line[at..].split('"').next()?.parse().ok()
    };
    let mut xs: Vec<f64> = Vec::new();
    for line in group.lines().filter(|l| l.starts_with("<line ")) {
        let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
            attribute(line, "x1"),
            attribute(line, "y1"),
            attribute(line, "x2"),
            attribute(line, "y2"),
        ) else {
            continue;
        };
        if (x1 - x2).abs() < 1e-6
            && (y1 - y2).abs() > 1e-6
            && !xs.iter().any(|x| (x - x1).abs() < 1e-3)
        {
            xs.push(x1);
        }
    }
    xs.sort_by(f64::total_cmp);
    xs
}

/// The sides of the blocks drawn in the top view at 1:10, from the
/// leftmost one, in millimetres on the sheet.
fn sides(xs: &[f64]) -> Vec<f64> {
    let first = xs.first().copied().unwrap_or(0.0);
    xs.iter()
        .map(|x| ((x - first) * 1000.0).round() / 1000.0)
        .collect()
}

#[test]
fn a_laid_out_top_view_draws_every_shown_placement_at_its_place() {
    let mut doc = pins();
    let dir = mitcad_model::testing::unique_temp_dir("drawing-placements");
    let path = dir.join("pins.svg").to_string_lossy().into_owned();
    let export = |doc: &mut Document<MockKernel>| {
        run(
            doc,
            json!({"cmd": "export_drawing", "path": path, "sheet": "A3", "views": ["top"],
                   "scale": "1:10", "dimensions": false, "hole_table": false}),
        )
    };

    // Every placement shown: the block and Pin three times.
    let result = export(&mut doc);
    assert_eq!(result["bodies"], json!(["F2.b0", "F4.b0"]), "{result}");
    assert_eq!(result["placements"], 4, "{result}");
    assert_eq!(result["scale"], "1:10", "{result}");
    let projected = doc.kernel().projected.borrow().last().cloned().unwrap();
    assert_eq!(projected.len(), 4, "{projected:?}");
    let text = std::fs::read_to_string(&path).unwrap();
    // The block from 0 to 40, the pins from 100, 200 and 300, 10 wide.
    let all = sides(&upright_lines(&text, "view-top"));
    assert_eq!(
        all,
        [0.0, 4.0, 10.0, 11.0, 20.0, 21.0, 30.0, 31.0],
        "{text}"
    );

    // Pin:2 hidden: the block and the two pins shown, where they are.
    run(
        &mut doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "Pin:2", "visible": false}),
    );
    let result = export(&mut doc);
    assert_eq!(result["bodies"], json!(["F2.b0", "F4.b0"]), "{result}");
    assert_eq!(result["placements"], 3, "{result}");
    let projected = doc.kernel().projected.borrow().last().cloned().unwrap();
    assert_eq!(projected.len(), 3, "{projected:?}");
    let text = std::fs::read_to_string(&path).unwrap();
    let shown = sides(&upright_lines(&text, "view-top"));
    assert_eq!(shown, [0.0, 4.0, 10.0, 11.0, 30.0, 31.0], "{text}");

    // Pin's body alone: at its two shown placements.
    let result = run(
        &mut doc,
        json!({"cmd": "export_drawing", "path": path, "sheet": "A3", "views": ["top"],
               "scale": "1:10", "dimensions": false, "hole_table": false, "bodies": ["F4.b0"]}),
    );
    assert_eq!(result["bodies"], json!(["F4.b0"]), "{result}");
    assert_eq!(result["placements"], 2, "{result}");
    let text = std::fs::read_to_string(&path).unwrap();
    let pins = sides(&upright_lines(&text, "view-top"));
    assert_eq!(pins, [0.0, 1.0, 20.0, 21.0], "{text}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_kept_drawings_export_counts_its_placements() {
    let mut doc = pins();
    run(
        &mut doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "Pin:3", "visible": false}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_drawing", "layout": {"views": ["top"], "sheet": "A3", "scale": "1:10",
               "dimensions": false, "hole_table": false}}),
    );
    let dir = mitcad_model::testing::unique_temp_dir("drawing-placements-kept");
    let path = dir.join("kept.svg").to_string_lossy().into_owned();
    let result = run(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": "Drawing1", "path": path}),
    );
    assert_eq!(result["bodies"], json!(["F2.b0", "F4.b0"]), "{result}");
    assert_eq!(result["placements"], 3, "{result}");
    std::fs::remove_dir_all(&dir).ok();
}
