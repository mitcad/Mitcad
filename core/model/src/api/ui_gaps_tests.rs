// SPDX-License-Identifier: MIT
//! What the application's small UI gaps (P9) need of the model: warnings
//! of features that succeeded with caveats (yellow in the timeline).

use serde_json::{Value, json};

use crate::Document;
use crate::document_tests::{block, corner};
use crate::testing::MockKernel;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn timeline_item(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no {name} in the timeline"))
}

#[test]
fn a_fillet_built_smaller_is_a_warning() {
    let mut b = block();
    const NOTE: &str = "the fillet is built 0.1 % smaller than asked";
    b.doc
        .kernel()
        .notes
        .borrow_mut()
        .insert("fillet", NOTE.to_owned());
    let fillet = json!({"type": "fillet", "body": "F2.b0",
                        "edges": [corner(b.extrude, 1, 0)], "radius": 2});
    // The preview succeeds and tells.
    let preview: Value = serde_json::from_str(
        &b.doc
            .preview(&json!({"cmd": "add_feature", "def": fillet}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(preview["status"], "ok");
    assert_eq!(preview["warnings"], json!([NOTE]));
    command(&mut b.doc, json!({"cmd": "add_feature", "def": fillet}));

    // Yellow in the timeline: the status `warning`, the note as its message.
    let item = timeline_item(&b.doc, "Fillet1");
    assert_eq!(item["status"], "warning");
    assert_eq!(item["error"], NOTE);
    let feature = query(&b.doc, json!({"query": "feature", "uid": "F3"}));
    assert_eq!(feature["status"], "warning");
    assert_eq!(feature["warnings"], json!([NOTE]));
    // The body is there: the feature succeeded.
    assert_eq!(b.doc.bodies().len(), 1);
    assert!(b.doc.stats().error.is_none());
    // Scripts can expect it.
    let script = json!([{"expect": {"feature": "Fillet1", "status": "warning",
                                    "error_contains": "0.1 %"}}]);
    b.doc.run_script(&script.to_string()).unwrap();

    // Other features are not affected; a chamfer without a note is ok.
    assert_eq!(timeline_item(&b.doc, "Extrude1")["status"], "ok");
    let chamfer = json!({"type": "chamfer", "body": "F2.b0",
                         "edges": [corner(b.extrude, 1, 2)],
                         "size": {"type": "equal_distance", "distance": 1}});
    command(&mut b.doc, json!({"cmd": "add_feature", "def": chamfer}));
    let item = timeline_item(&b.doc, "Chamfer1");
    assert_eq!(item["status"], "ok");
    assert_eq!(item["error"], Value::Null);
}

/// A 60 x 40 rectangle on layer Outline, a circle on the frozen layer
/// Holes and a text on a layer the table does not list, in millimetres.
const LAYERED: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n\
0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n2\n\
0\nLAYER\n2\nOutline\n70\n0\n62\n7\n6\nCONTINUOUS\n\
0\nLAYER\n2\nHoles\n70\n1\n62\n-1\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\nOutline\n10\n0\n20\n0\n11\n60\n21\n0\n\
0\nLINE\n8\nOutline\n10\n60\n20\n0\n11\n60\n21\n40\n\
0\nLINE\n8\nOutline\n10\n60\n20\n40\n11\n0\n21\n40\n\
0\nLINE\n8\nOutline\n10\n0\n20\n40\n11\n0\n21\n0\n\
0\nCIRCLE\n8\nHoles\n10\n30\n20\n20\n40\n5\n\
0\nTEXT\n8\nNotes\n10\n0\n20\n-5\n40\n2\n1\nPLATE\n\
0\nENDSEC\n0\nEOF\n";

#[test]
fn a_dxf_drawing_tells_its_layers_and_takes_some_of_them() {
    let dir = std::env::temp_dir().join(format!("mitcad-dxf-layers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plate.dxf");
    std::fs::write(&path, LAYERED).unwrap();
    let path = path.to_str().unwrap();

    let mut doc = Document::new(MockKernel::default());
    let info = query(&doc, json!({"query": "dxf_info", "path": path}));
    assert_eq!(info["unit"], "mm");
    assert_eq!(info["unit_mm"], 1.0);
    assert_eq!(info["entities"], 6);
    let layers: Vec<(&str, u64, bool)> = info["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["name"].as_str().unwrap(),
                l["entities"].as_u64().unwrap(),
                l["visible"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        layers,
        [
            ("Outline", 4, true),
            ("Holes", 1, false),
            ("Notes", 1, true)
        ]
    );
    assert_eq!(
        info["layers"][1]["bounds"],
        json!({"min": [25.0, 15.0], "max": [35.0, 25.0]})
    );
    // The text reaches below the rectangle, its height above its insertion.
    assert_eq!(info["bounds"]["min"], json!([0.0, -5.0]));
    assert_eq!(info["bounds"]["max"], json!([60.0, 40.0]));

    // Only the outline, its lower left corner at (10, 5).
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let result = command(
        &mut doc,
        json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path,
               "at": [10, 5], "layers": ["Outline"]}),
    );
    assert_eq!(result["curves"], 4, "{result}");
    assert_eq!(result["text_count"], 0);
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    let regions = sketch["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 1, "{sketch}");
    assert!((regions[0]["area"].as_f64().unwrap() - 2400.0).abs() < 1e-6);
    assert!(
        sketch["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "point" && e["at"] == json!([70.0, 45.0])),
        "{sketch}"
    );
    // A layer the drawing does not use is refused.
    let refused = doc
        .command(
            &json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path,
                    "layers": ["Dimensions"]})
            .to_string(),
        )
        .unwrap_err();
    assert!(
        refused.0.contains("nothing on a layer Dimensions"),
        "{refused}"
    );
    // A missing file is refused by the query too.
    assert!(
        doc.query(&json!({"query": "dxf_info", "path": dir.join("none.dxf")}).to_string())
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn origin_isolation_and_favourites_are_saved_with_undo() {
    let mut b = block();
    let doc = &mut b.doc;
    let display =
        |doc: &Document<MockKernel>| query(doc, json!({"query": "document"}))["display"].clone();
    assert_eq!(display(doc), json!({"origin": false, "isolated": []}));

    command(doc, json!({"cmd": "set_origin_visible", "visible": true}));
    assert_eq!(
        query(doc, json!({"query": "document"}))["undo"],
        "Show Origin"
    );
    command(
        doc,
        json!({"cmd": "create_component", "name": "Bracket", "activate": false}),
    );
    command(
        doc,
        json!({"cmd": "set_isolation", "items": [{"body": "F2.b0"}, {"occurrence": "O1"}]}),
    );
    assert_eq!(
        display(doc),
        json!({"origin": true, "isolated": [{"occurrence": "O1"}, {"body": "F2.b0"}]})
    );
    command(
        doc,
        json!({"cmd": "set_parameter", "name": "d1", "favorite": true}),
    );
    assert_eq!(
        query(doc, json!({"query": "document"}))["undo"],
        "Add Favorite d1"
    );
    let params = query(doc, json!({"query": "parameters"}));
    assert_eq!(params[0]["favorite"], true);
    assert_eq!(params[1]["favorite"], false);

    // Saved and read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["display"],
        json!({"origin": true, "isolated": [{"occurrence": "O1"}, {"body": "F2.b0"}]})
    );
    assert_eq!(file["parameters"][0]["favorite"], true);
    assert_eq!(file["parameters"][1].get("favorite"), None);
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(display(&opened), display(doc));
    assert!(opened.is_favorite_parameter("d1"));
    // A renamed favourite stays one.
    command(
        &mut opened,
        json!({"cmd": "rename_parameter", "name": "d1", "new_name": "width"}),
    );
    assert!(opened.is_favorite_parameter("width"));

    // Undo takes each back; an isolated body that is gone isolates nothing.
    for label in ["Add Favorite d1", "Isolate 2 item(s)"] {
        assert_eq!(command(doc, json!({"cmd": "undo"}))["label"], label);
    }
    assert_eq!(display(doc), json!({"origin": true, "isolated": []}));
    command(
        doc,
        json!({"cmd": "set_isolation", "items": [{"body": "F2.b0"}]}),
    );
    command(doc, json!({"cmd": "set_marker", "position": 0}));
    assert_eq!(display(doc)["isolated"], json!([]));
    // Nothing to isolate, or an occurrence that is not there, is refused.
    for items in [
        json!([{}]),
        json!([{"occurrence": "O9"}]),
        json!([{"occurrence": "x"}]),
    ] {
        assert!(
            doc.command(&json!({"cmd": "set_isolation", "items": items}).to_string())
                .is_err(),
            "{items}"
        );
    }
}

#[test]
fn timeline_groups_stay_runs_of_features() {
    let mut b = block();
    let doc = &mut b.doc;
    // Sketch1 F1, Extrude1 F2, then three planes F3, F4, F5.
    for distance in [10, 20, 30] {
        command(
            doc,
            json!({"cmd": "add_feature", "def": {"type": "construction_plane",
                   "definition": {"type": "offset", "plane": "xy", "distance": distance}}}),
        );
    }
    let groups =
        |doc: &Document<MockKernel>| query(doc, json!({"query": "timeline"}))["groups"].clone();
    assert_eq!(groups(doc), Value::Null);
    // Not a run: refused; a run in any order: Group1.
    assert!(
        doc.command(&json!({"cmd": "group_features", "features": ["F1", "F3"]}).to_string())
            .is_err()
    );
    let made = command(
        doc,
        json!({"cmd": "group_features", "features": ["F4", "F3"]}),
    );
    assert_eq!(made["name"], "Group1");
    assert_eq!(
        query(doc, json!({"query": "document"}))["undo"],
        "Create Group Group1"
    );
    assert_eq!(
        groups(doc),
        json!([{"name": "Group1", "features": ["F3", "F4"]}])
    );
    // A feature in a group cannot be grouped again.
    assert!(
        doc.command(&json!({"cmd": "group_features", "features": ["F4", "F5"]}).to_string())
            .is_err()
    );
    // Moved between members: it joins; moved away: it leaves.
    command(
        doc,
        json!({"cmd": "reorder_feature", "uid": "F5", "index": 3}),
    );
    assert_eq!(groups(doc)[0]["features"], json!(["F3", "F5", "F4"]));
    command(
        doc,
        json!({"cmd": "reorder_feature", "uid": "F3", "index": 0}),
    );
    assert_eq!(groups(doc)[0]["features"], json!(["F5", "F4"]));
    // Deleted: it leaves; renamed; saved and read back.
    command(doc, json!({"cmd": "delete_feature", "uid": "F4"}));
    assert_eq!(groups(doc)[0]["features"], json!(["F5"]));
    command(
        doc,
        json!({"cmd": "rename_group", "name": "Group1", "new_name": "Planes"}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["groups"],
        json!([{"name": "Planes", "features": ["F5"]}])
    );
    let opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(groups(&opened), groups(doc));
    // A group whose only member goes is gone; undo brings it back.
    command(doc, json!({"cmd": "delete_feature", "uid": "F5"}));
    assert_eq!(groups(doc), Value::Null);
    command(doc, json!({"cmd": "undo"}));
    assert_eq!(groups(doc)[0]["name"], "Planes");
    command(doc, json!({"cmd": "ungroup", "name": "Planes"}));
    assert_eq!(groups(doc), Value::Null);
    assert_eq!(
        query(doc, json!({"query": "document"}))["undo"],
        "Ungroup Planes"
    );
}

#[test]
fn a_pattern_preview_lists_its_elements_also_the_suppressed_ones() {
    let mut b = block();
    let pattern = |suppressed: Value| {
        json!({"cmd": "add_feature", "def": {"type": "rectangular_pattern",
               "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "direction1": {"axis": "x", "quantity": 3, "distance": 100},
               "distance_type": "spacing", "suppressed_elements": suppressed}})
    };
    let preview = |doc: &mut Document<MockKernel>, command: Value| -> Value {
        serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap()
    };
    let all = preview(&mut b.doc, pattern(json!([])));
    let elements = all["elements"].as_array().unwrap();
    assert_eq!(elements.len(), 3, "{all}");
    // Rows of each element's transform: element 2 is 200 mm along X.
    assert_eq!(
        elements[0],
        json!([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0]
        ])
    );
    assert_eq!(elements[2][0][3], 200.0);
    assert_eq!(all["bodies"].as_array().unwrap().len(), 3);
    // Suppressed, an element is not made but still listed (its toggle).
    let some = preview(&mut b.doc, pattern(json!([2])));
    assert_eq!(some["elements"].as_array().unwrap().len(), 3);
    assert_eq!(some["bodies"].as_array().unwrap().len(), 2);
    // Other features list none.
    let fillet = json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
                        "edges": [corner(b.extrude, 1, 0)], "radius": 2}});
    assert_eq!(preview(&mut b.doc, fillet).get("elements"), None);
}

#[test]
fn a_text_in_a_missing_font_is_a_warning() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "AB", "at": [0, 0],
               "height": 10, "font": "No Such Font"}),
    );
    let item = timeline_item(&doc, "Sketch1");
    assert_eq!(item["status"], "warning", "{item}");
    let message = item["error"].as_str().unwrap();
    assert!(
        message.contains("No Such Font is not installed; Mock Sans is used"),
        "{message}"
    );
}
