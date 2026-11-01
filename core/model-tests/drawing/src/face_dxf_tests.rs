// SPDX-License-Identifier: MIT
//! Export Face as DXF (mitcad#1049, `export_face_dxf`) on the mock kernel,
//! whose projections are the bodies' boxes: a plate's outline seen along
//! its top face and along a side, at 1:1 from the origin, read back from
//! the file; a face that is not the body's planar face is refused.

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

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// A plate 100 x 60 x 3 (F1.b0) at (20, 10), a box whose faces the mock
/// kernel measures.
fn plate() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [20, 10],
               "length": 100, "width": 60, "height": 3, "operation": "new_body"}}),
    );
    doc
}

#[test]
fn a_face_s_outline_is_written_at_full_size_from_the_origin() {
    let mut doc = plate();
    let dir = mitcad_model::testing::unique_temp_dir("face-dxf");
    let path = dir.join("plate.dxf").to_string_lossy().into_owned();
    let written = command(
        &mut doc,
        json!({"cmd": "export_face_dxf", "body": "F1.b0", "face": "F1:top", "path": path}),
    );
    assert_eq!(written["face"], "F1:top", "{written}");
    assert_eq!(
        (written["outer"].as_u64(), written["inner"].as_u64()),
        (Some(1), Some(0))
    );
    assert!(
        near(written["width"].as_f64().unwrap(), 100.0)
            && near(written["height"].as_f64().unwrap(), 60.0),
        "{written}"
    );
    let back = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(back.units, mitcad_dxf::Units::Millimeters);
    let outer = back.on_layers(&["CUT_OUTER".to_owned()]);
    assert_eq!(outer.entities.len(), 4);
    let (min, max) = outer.bounds().unwrap();
    assert!(
        near(min.x, 0.0) && near(min.y, 0.0) && near(max.x, 100.0) && near(max.y, 60.0),
        "{min:?} {max:?}"
    );
    // Seen along a side: the plate's edge, 3 mm thick.
    let side = dir.join("side.dxf").to_string_lossy().into_owned();
    let written = command(
        &mut doc,
        json!({"cmd": "export_face_dxf", "body": "F1.b0", "face": "F1:side1", "path": side}),
    );
    let (w, h) = (
        written["width"].as_f64().unwrap(),
        written["height"].as_f64().unwrap(),
    );
    assert!(near(w.min(h), 3.0), "{written}");
    // Nothing is added to the design: no drawing.
    let drawings: Value =
        serde_json::from_str(&doc.query(r#"{"query": "drawings"}"#).unwrap()).unwrap();
    assert_eq!(drawings, json!([]));
    // A face the body does not have is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "export_face_dxf", "body": "F1.b0", "face": "F7:top",
               "path": dir.join("none.dxf").to_string_lossy()}),
    );
    assert!(error.contains("F7:top"), "{error}");
}
