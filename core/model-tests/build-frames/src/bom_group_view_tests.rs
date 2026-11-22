// SPDX-License-Identifier: MIT
//! Group identical parts in the Bill of Materials' parts-only and
//! structured views (mitcad#2215), with BUILD's test library: each placed
//! profile is a component of its own, yet equal profiles (measured
//! geometry and material, as the list of every component compares them)
//! are one row; a profile made by hand to the same size joins them. The
//! structured view combines siblings only, an assembly row never with a
//! part's.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ValueInput;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::BodyUid;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

/// A 20 mm square bar `length` long in a new component `name` of the
/// active one, made by hand as the library's profile is (a section on XY
/// extruded); the active component again after. Its body.
fn hand_profile(doc: &mut Document<MockKernel>, name: &str, length: f64) -> BodyUid {
    let parent = doc.active_component();
    command(doc, json!({"cmd": "create_component", "name": name})).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [-10.0, -10.0],
            &ValueInput::Number(20.0),
            &ValueInput::Number(20.0),
        )
        .unwrap()
        .region;
    let extrude = doc
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": length}, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    command(
        doc,
        json!({"cmd": "activate_component", "component": parent.to_string()}),
    )
    .unwrap();
    BodyUid::new(extrude, 0)
}

/// A view's rows as `item xquantity`, indented by level.
fn view_rows(answer: &Value) -> Vec<String> {
    answer["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let indent = "  ".repeat(r["level"].as_u64().unwrap() as usize - 1);
            format!(
                "{indent}{} x{}",
                r["item"].as_str().unwrap_or("-"),
                r["quantity"]
            )
        })
        .collect()
}

fn view(doc: &Document<MockKernel>, view: &str, grouped: bool) -> Value {
    query(
        doc,
        json!({"query": "parts_list", "view": view, "group_identical": grouped}),
    )
}

fn group_names(row: &Value) -> Vec<String> {
    row["group_names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn equal_library_profiles_and_a_hand_made_one_are_one_row_when_grouped() {
    let mut doc = design();
    doc.kernel().measures.set(true);
    // Four library profiles: three 600 mm, one 400 mm (placed second).
    let first_placed = first(&mut doc, "p20", [0.0; 3], 600.0);
    first(&mut doc, "p20", [50.0, 0.0, 0.0], 400.0);
    first(&mut doc, "p20", [100.0, 0.0, 0.0], 600.0);
    first(&mut doc, "p20", [150.0, 0.0, 0.0], 600.0);
    let hand = hand_profile(&mut doc, "Hand", 600.0);
    all_ok(&doc);
    // The hand-made bar of the library profile's material, should the
    // library give it one.
    let library_body = BodyUid::new(derived(&doc, component(&first_placed)), 0);
    if let Some(material) = doc
        .body_material_named(library_body, None)
        .map(|m| m.id.clone())
    {
        command(
            &mut doc,
            json!({"cmd": "set_body_material", "uid": hand.to_string(), "material": material}),
        )
        .unwrap();
    }

    // Parts only: five rows of one each, or two grouped (4 and 1).
    let separate = view(&doc, "parts", false);
    assert_eq!(
        view_rows(&separate),
        ["1 x1", "2 x1", "3 x1", "4 x1", "5 x1"],
        "{separate}"
    );
    let grouped = view(&doc, "parts", true);
    assert_eq!(grouped["group_identical"], true);
    assert_eq!(view_rows(&grouped), ["1 x4", "2 x1"], "{grouped}");
    assert_eq!(grouped["rows"][0]["component"], first_placed["component"]);
    let names = group_names(&grouped["rows"][0]);
    assert_eq!(names.len(), 4, "{grouped}");
    assert!(names.contains(&"Hand".to_owned()), "{names:?}");
    assert_eq!(group_names(&grouped["rows"][1]).len(), 1, "{grouped}");
    // The totals follow the rows; the mass of the design is the same.
    assert_eq!(grouped["totals"]["quantity"], 5, "{grouped}");
    assert_eq!(separate["totals"]["quantity"], 5, "{separate}");
    assert_eq!(grouped["totals"]["volume"], separate["totals"]["volume"]);

    // An assembly holding two more equal bars: in the structured view they
    // are combined below it, as siblings, but not with the design's own
    // profiles; the assembly itself is no part to combine with them.
    let sub = command(&mut doc, json!({"cmd": "create_component", "name": "Sub"})).unwrap();
    hand_profile(&mut doc, "Inner A", 600.0);
    hand_profile(&mut doc, "Inner B", 600.0);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    )
    .unwrap();
    all_ok(&doc);
    let structured = view(&doc, "structured", false);
    assert_eq!(
        view_rows(&structured),
        [
            "1 x1", "2 x1", "3 x1", "4 x1", "5 x1", "6 x1", "  6.1 x1", "  6.2 x1"
        ],
        "{structured}"
    );
    let structured = view(&doc, "structured", true);
    assert_eq!(
        view_rows(&structured),
        ["1 x4", "2 x1", "3 x1", "  3.1 x2"],
        "{structured}"
    );
    assert_eq!(structured["rows"][2]["component"], sub["component"]);
    assert_eq!(
        group_names(&structured["rows"][3]),
        ["Inner A", "Inner B"],
        "{structured}"
    );
    // Parts only: the bars in the assembly are parts of the design too.
    let grouped = view(&doc, "parts", true);
    assert_eq!(view_rows(&grouped), ["1 x6", "2 x1"], "{grouped}");

    // A Reference is never combined with what is counted.
    let hand_component = doc
        .assembly()
        .components
        .iter()
        .find(|c| c.name == "Hand")
        .unwrap()
        .uid;
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": hand_component.to_string(),
               "set": "Bill of Materials", "key": "BOM Structure", "value": "Reference"}),
    )
    .unwrap();
    let grouped = view(&doc, "parts", true);
    assert_eq!(view_rows(&grouped), ["1 x5", "2 x1", "- x0"], "{grouped}");
}
