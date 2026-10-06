// SPDX-License-Identifier: MIT
//! What the application's view and file commands use (U5, U6): named
//! views saved in the document, and a DXF drawing inserted into a sketch.

use serde_json::{Value, json};

use crate::Document;
use crate::testing::MockKernel;

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
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn view_names(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, json!({"query": "named_views"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect()
}

fn camera(name: &str, height: f64) -> Value {
    json!({"cmd": "add_named_view", "name": name, "eye": [100, -100, 100],
           "target": [0, 0, 0], "up": [0, 0, 1], "height": height})
}

#[test]
fn named_views_are_added_renamed_deleted_with_undo_and_saved() {
    let mut doc = Document::new(MockKernel::default());
    assert_eq!(command(&mut doc, camera("Detail", 80.0))["name"], "Detail");
    // Without a name: the next free NamedView<n>.
    let mut unnamed = camera("", 50.0);
    unnamed["perspective"] = json!(true);
    assert_eq!(command(&mut doc, unnamed)["name"], "NamedView1");
    assert_eq!(view_names(&doc), ["Detail", "NamedView1"]);
    let views = query(&doc, json!({"query": "named_views"}));
    assert_eq!(views[1]["perspective"], true);
    assert_eq!(views[0].get("perspective"), None);
    assert_eq!(views[0]["height"], 80.0);
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Add Named View NamedView1"
    );

    // A name in use is refused unless the view replaces it.
    assert!(refused(&mut doc, camera("Detail", 10.0)).contains("a named view 'Detail' exists"));
    let mut replace = camera("Detail", 10.0);
    replace["replace"] = json!(true);
    command(&mut doc, replace);
    assert_eq!(
        query(&doc, json!({"query": "named_views"}))[0]["height"],
        10.0
    );
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Change Named View Detail"
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        query(&doc, json!({"query": "named_views"}))[0]["height"],
        80.0
    );

    command(
        &mut doc,
        json!({"cmd": "rename_named_view", "name": "NamedView1", "new_name": "Home"}),
    );
    assert_eq!(view_names(&doc), ["Detail", "Home"]);
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "rename_named_view", "name": "Home", "new_name": "Detail"})
        )
        .contains("exists")
    );

    // Saved and loaded with the document; views change no geometry.
    let saved = doc.to_json();
    assert!(saved.contains("\"views\""));
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(view_names(&loaded), ["Detail", "Home"]);
    assert_eq!(loaded.to_json(), saved);

    command(
        &mut doc,
        json!({"cmd": "delete_named_view", "name": "Detail"}),
    );
    assert_eq!(view_names(&doc), ["Home"]);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(view_names(&doc), ["Detail", "Home"]);
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "delete_named_view", "name": "Nope"})
        )
        .contains("there is no named view 'Nope'")
    );
    assert_eq!(
        command(&mut doc, json!({"cmd": "recompute"}))["recomputed"],
        0,
        "nothing to recompute"
    );
}

#[test]
fn named_views_must_be_cameras() {
    let mut doc = Document::new(MockKernel::default());
    let mut along = camera("Bad", 10.0);
    along["up"] = json!([-1, 1, -1]);
    assert!(refused(&mut doc, along).contains("up must not be along the line of sight"));
    let mut same = camera("Bad", 10.0);
    same["eye"] = json!([0, 0, 0]);
    assert!(refused(&mut doc, same).contains("the eye must differ from the target"));
    assert!(refused(&mut doc, camera("Bad", 0.0)).contains("the height must be positive"));
    assert!(view_names(&doc).is_empty());
    // A file without views has no "views" field; a broken one is refused.
    assert!(!doc.to_json().contains("\"views\""));
    let broken = doc.to_json().replacen(
        '{',
        "{\"views\": [{\"name\": \"A\", \"eye\": [0, 0, 1]}],",
        1,
    );
    let Err(error) = Document::from_json(&broken, MockKernel::default()) else {
        panic!("a view without a target was loaded");
    };
    assert!(error.to_string().contains("views[0]"), "{error}");
}

/// A 60 x 40 rectangle of lines whose corners meet within rounding, a
/// circle, an arc, an ellipse, a spline and a text, in centimetres.
const DRAWING: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n5\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\n0\n10\n0\n20\n0\n11\n6\n21\n0\n\
0\nLINE\n8\n0\n10\n6.0000000000001\n20\n0\n11\n6\n21\n4\n\
0\nLINE\n8\n0\n10\n6\n20\n4\n11\n0\n21\n4\n\
0\nLINE\n8\n0\n10\n0\n20\n4\n11\n0\n21\n0\n\
0\nCIRCLE\n8\n0\n10\n3\n20\n2\n40\n1\n\
0\nARC\n8\n0\n10\n10\n20\n0\n40\n2\n50\n0\n51\n90\n\
0\nELLIPSE\n8\n0\n10\n20\n20\n0\n11\n2\n21\n0\n40\n0.5\n41\n0\n42\n6.283185307179586\n\
0\nTEXT\n8\n0\n10\n0\n20\n-2\n40\n0.5\n1\nPLATE\n\
0\nENDSEC\n0\nEOF\n";

#[test]
fn a_dxf_drawing_goes_into_a_sketch_as_one_undo_step() {
    let dir = std::env::temp_dir().join(format!("mitcad-dxf-insert-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plate.dxf");
    std::fs::write(&path, DRAWING).unwrap();

    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xz"}));
    let result = command(
        &mut doc,
        json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path.to_str().unwrap(),
               "at": [100, 0]}),
    );
    // Four lines, a circle, an arc and an ellipse. The rectangle's corners
    // are shared: 4 corners, the centres of the circle, the arc and the
    // ellipse, and the arc's ends (the ellipse's axis end is its own).
    assert_eq!(result["curves"], 7, "{result}");
    assert_eq!(result["text_count"], 1);
    assert_eq!(result["points"], 9, "{result}");
    assert_eq!(
        query(&doc, json!({"query": "document"}))["undo"],
        "Insert DXF into Sketch1"
    );

    let sketch = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    let entities = sketch["entities"].as_array().unwrap();
    let circle = entities
        .iter()
        .find(|e| e["geometry"]["radius"] == 10.0 && e["type"] == "circle")
        .unwrap_or_else(|| panic!("no circle of radius 10 mm in {sketch}"));
    assert_eq!(circle["geometry"]["center"], json!([130.0, 20.0]));
    // Profiles (moved 100 mm along x, in mm): the circle, the ellipse
    // (20 x 10 mm radii) and the rectangle around the circle; and the
    // text's letters.
    let regions: Vec<&Value> = sketch["regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["text"].is_null())
        .collect();
    assert_eq!(regions.len(), 3, "{regions:?}");
    assert!(sketch["regions"].as_array().unwrap().len() > 3);
    let mut areas: Vec<f64> = regions
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    areas.sort_by(f64::total_cmp);
    let pi = std::f64::consts::PI;
    assert!((areas[0] - pi * 100.0).abs() < 1e-6, "{areas:?}");
    assert!((areas[1] - pi * 200.0).abs() < 1e-3, "{areas:?}");
    assert!((areas[2] - (2400.0 - pi * 100.0)).abs() < 1e-6, "{areas:?}");
    assert_eq!(sketch["texts"][0]["text"], "PLATE");

    command(&mut doc, json!({"cmd": "undo"}));
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    assert!(sketch["entities"].as_array().unwrap().is_empty());

    // A unitless drawing takes the unit given; a missing file is refused.
    std::fs::write(
        &path,
        DRAWING.replace("$INSUNITS\n70\n5", "$INSUNITS\n70\n0"),
    )
    .unwrap();
    let result = command(
        &mut doc,
        json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path.to_str().unwrap(),
               "unit": "in"}),
    );
    assert_eq!(result["curves"], 7);
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    assert!(
        sketch["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "circle" && e["geometry"]["radius"] == 25.4),
        "{sketch}"
    );
    let missing = dir.join("missing.dxf");
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": missing.to_str().unwrap()})
        )
        .contains("missing.dxf")
    );
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path.to_str().unwrap(),
                   "unit": "furlong"})
        )
        .contains("unknown drawing unit")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
