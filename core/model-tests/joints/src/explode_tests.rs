// SPDX-License-Identifier: MIT
//! Exploded views of assemblies (mitcad#2113) with the mock kernel: a view
//! of steps that move and turn occurrences, the offsets summed step after
//! step, the trail lines' ends, Auto Explode along a slider joint's axis
//! and along the directions from the assembly's centre, an occurrence
//! deleted under a view (dropped with a warning in words), the design's
//! placements never changed, and the design file.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin, their
//! boxes their bounding boxes. The plate (Plate:1) is grounded at the
//! origin; the pin (Pin:1) slides along x on the plate's top (x from 0 to
//! 10, z from 5 to 10); the wall (Wall:1, grounded) stands at x = 22, z =
//! 5; the lid (Lid:1) floats at z = 30.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

/// A 10 x 10 x 5 block in the active component.
fn block_in(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

fn face(doc: &Document<MockKernel>, body: BodyUid, role: &str) -> String {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == role)
        .unwrap()
        .to_string()
}

struct Rig {
    doc: Document<MockKernel>,
    occurrences: Vec<OccurrenceUid>,
}

impl Rig {
    fn run(&mut self, cmd: Value) -> Value {
        command(&mut self.doc, cmd)
    }

    fn placements(&self) -> Vec<Transform> {
        self.occurrences
            .iter()
            .map(|o| self.doc.placement(*o).unwrap())
            .collect()
    }
}

fn rig() -> Rig {
    let mut doc = Document::new(MockKernel::default());
    let part = |doc: &mut Document<MockKernel>, name: &str, at: [f64; 3]| {
        let (_, o) = doc
            .create_component(Some(name), Transform::translation(at), true)
            .unwrap();
        let body = block_in(doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        (o, body)
    };
    let (plate, plate_body) = part(&mut doc, "Plate", [0.0; 3]);
    let (pin, pin_body) = part(&mut doc, "Pin", [100.0, 0.0, 0.0]);
    let (wall, _) = part(&mut doc, "Wall", [22.0, 0.0, 5.0]);
    let (lid, _) = part(&mut doc, "Lid", [0.0, 0.0, 30.0]);
    doc.set_occurrence_grounded(plate, true).unwrap();
    doc.set_occurrence_grounded(wall, true).unwrap();
    let top = json!({"occurrence": "Plate:1",
                     "geometry": {"body": plate_body, "face": face(&doc, plate_body, "end")}});
    let bottom = json!({"occurrence": "Pin:1",
                        "geometry": {"body": pin_body, "face": face(&doc, pin_body, "start")}});
    command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x", "flip": true,
               "a": bottom, "b": top, "position": {"tx": 0}}),
    );
    assert!(doc.placement(pin).unwrap().translation[0].abs() < 1e-9);
    Rig {
        doc,
        occurrences: vec![plate, pin, wall, lid],
    }
}

fn near(a: &Value, b: [f64; 3]) -> bool {
    let a: Vec<f64> = a
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    a.len() == 3 && (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

fn part<'a>(explode: &'a Value, name: &str) -> &'a Value {
    explode["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no part {name} in {explode}"))
}

#[test]
fn steps_move_and_turn_one_after_another() {
    let mut rig = rig();
    let before = rig.placements();
    let added = rig.run(json!({"cmd": "add_explode_view", "steps": [
        {"occurrences": ["Lid:1"], "distance": 40, "direction": "z"},
        {"name": "Out", "occurrences": ["Lid:1", "Wall:1"], "distance": "10 mm", "direction": [1, 0, 0]},
        {"occurrences": ["Wall:1"], "angle": 90, "axis": "z"},
    ]}));
    assert_eq!(added["name"], "Explode1");
    assert_eq!(rig.doc.undo_label(), Some("Add Explode1"));

    let explode = query(
        &rig.doc,
        json!({"query": "explode_view", "name": "Explode1"}),
    );
    assert_eq!(explode["steps"].as_array().unwrap().len(), 3);
    assert_eq!(explode["steps"][0]["name"], "Step 1");
    assert_eq!(explode["steps"][1]["name"], "Out");
    assert!(near(&explode["steps"][0]["translation"], [0.0, 0.0, 40.0]));
    assert_eq!(
        explode["steps"][2]["angle_degrees"]
            .as_f64()
            .unwrap()
            .round(),
        90.0
    );
    // The lid moved in two steps ends at their sum.
    assert!(near(
        &part(&explode, "Lid:1")["translation"],
        [10.0, 0.0, 40.0]
    ));
    // The wall turned about its own centre where step 2 left it: its
    // centre (27, 5, 7.5) is at (37, 5, 7.5), turned a quarter about z.
    let wall = part(&explode, "Wall:1");
    let m: Vec<Vec<f64>> = serde_json::from_value(wall["transform"].clone()).unwrap();
    let centre = [27.0, 5.0, 7.5];
    let moved: Vec<f64> = (0..3)
        .map(|r| (0..3).map(|c| m[r][c] * centre[c]).sum::<f64>() + m[r][3])
        .collect();
    assert!(
        (moved[0] - 37.0).abs() < 1e-6 && (moved[1] - 5.0).abs() < 1e-6,
        "{moved:?}"
    );
    assert!(
        (m[0][1] + 1.0).abs() < 1e-9 && (m[1][0] - 1.0).abs() < 1e-9,
        "{m:?}"
    );
    // Trail lines: the lid's centre up, then both along x; the turn about
    // the wall's own centre leaves no line.
    let trails = explode["trails"].as_array().unwrap();
    assert_eq!(trails.len(), 3, "{trails:?}");
    assert_eq!(trails[0]["name"], "Lid:1");
    assert!(near(&trails[0]["from"], [5.0, 5.0, 32.5]));
    assert!(near(&trails[0]["to"], [5.0, 5.0, 72.5]));
    assert!(near(&trails[1]["from"], [5.0, 5.0, 72.5]));
    assert!(near(&trails[1]["to"], [15.0, 5.0, 72.5]));
    assert!(near(&trails[2]["from"], [27.0, 5.0, 7.5]));
    assert!(near(&trails[2]["to"], [37.0, 5.0, 7.5]));
    assert!(explode["warnings"].as_array().unwrap().is_empty());

    // Halfway into the animation (1.5 of its 3 seconds): step 1 done, step
    // 2 half way, step 3 not begun.
    let half = query(
        &rig.doc,
        json!({"query": "explode_view", "name": "Explode1", "time": 1.5, "placements": true}),
    );
    assert_eq!(half["seconds"], 3.0);
    assert!(near(&part(&half, "Lid:1")["translation"], [5.0, 0.0, 40.0]));
    // The design's placements of the lid: assembled at z 30, shown at 70.
    let lid = rig.occurrences[3].to_string();
    let placed = half["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["path"] == lid)
        .unwrap();
    assert!((placed["transform"][2][3].as_f64().unwrap() - 70.0).abs() < 1e-9);
    assert_eq!(half["design_trails"].as_array().unwrap().len(), 3);

    // Trails off, a step moved to the front and renamed: undo steps.
    rig.run(json!({"cmd": "edit_explode_view", "name": "Explode1", "trails": false}));
    let off = query(
        &rig.doc,
        json!({"query": "explode_view", "name": "Explode1"}),
    );
    assert!(off["trails"].as_array().unwrap().is_empty());
    rig.run(json!({"cmd": "edit_explode_view", "name": "Explode1", "move_step": 1, "to": 0}));
    rig.run(
        json!({"cmd": "edit_explode_view", "name": "Explode1", "step": 1,
                   "set": {"distance": "2 * 20 mm"}}),
    );
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    assert_eq!(views[0]["steps"][0]["name"], "Out");
    assert_eq!(views[0]["steps"][1]["distance"], "2 * 20 mm");
    rig.run(json!({"cmd": "edit_explode_view", "name": "Explode1", "new_name": "Apart"}));
    assert_eq!(rig.doc.undo_label(), Some("Rename Explode1"));
    rig.doc.undo();
    assert_eq!(
        query(&rig.doc, json!({"query": "explode_views"}))[0]["name"],
        "Explode1"
    );

    // Refused in words: a step that moves nowhere, an occurrence not in
    // the design, a distance that is not a length.
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_explode_view", "steps": [{"occurrences": ["Lid:1"]}]}),
    );
    assert!(
        error.contains("moves along a direction, turns about an axis, or both"),
        "{error}"
    );
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_explode_view", "steps": [{"occurrences": ["Gone:1"], "distance": 5, "direction": "x"}]}),
    );
    assert!(error.contains("no occurrence Gone:1"), "{error}");
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_explode_view", "steps": [{"occurrences": ["Lid:1"], "distance": "90 deg", "direction": "x"}]}),
    );
    assert!(error.contains("distance"), "{error}");

    // The design's placements never moved.
    assert_eq!(rig.placements(), before);
}

#[test]
fn auto_explode_slides_along_a_joint_and_out_from_the_centre() {
    let mut rig = rig();
    let before = rig.placements();
    let made = rig.run(json!({"cmd": "auto_explode"}));
    assert_eq!(made["name"], "Explode1");
    assert_eq!(made["steps"], 4);
    assert_eq!(rig.doc.undo_label(), Some("Auto Explode Explode1"));
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    let steps = views[0]["steps"].as_array().unwrap();
    let by_name = |name: &str| {
        steps
            .iter()
            .find(|s| s["name"] == name)
            .unwrap_or_else(|| panic!("no step {name}: {steps:?}"))
    };
    // The whole is x 0 to 32, z 0 to 35: its centre (16, 5, 17.5). The
    // plate below and the lid above go along z, the outer ones first; the
    // wall to the right goes along x; the pin slides along its joint's
    // axis, out of the centre's side.
    assert!(near(
        &by_name("Plate:1")["direction"]["vector"],
        [0.0, 0.0, -1.0]
    ));
    assert!(near(
        &by_name("Lid:1")["direction"]["vector"],
        [0.0, 0.0, 1.0]
    ));
    assert!(near(
        &by_name("Wall:1")["direction"]["vector"],
        [1.0, 0.0, 0.0]
    ));
    assert!(near(
        &by_name("Pin:1")["direction"]["vector"],
        [-1.0, 0.0, 0.0]
    ));
    // By its size: a 10 x 10 x 5 block's diagonal is 15.
    assert_eq!(by_name("Pin:1")["distance"], "15 mm");
    let order: Vec<&str> = steps.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(order, ["Plate:1", "Lid:1", "Pin:1", "Wall:1"]);

    // Again with a factor and only the lid, after the steps it has.
    rig.run(
        json!({"cmd": "auto_explode", "name": "Explode1", "occurrences": ["Lid:1"],
                   "factor": 2, "append": true}),
    );
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    assert_eq!(views[0]["steps"].as_array().unwrap().len(), 5);
    assert_eq!(views[0]["steps"][4]["distance"], "30 mm");
    let explode = query(
        &rig.doc,
        json!({"query": "explode_view", "name": "Explode1"}),
    );
    assert!(near(
        &part(&explode, "Lid:1")["translation"],
        [0.0, 0.0, 45.0]
    ));
    let error = refused(&mut rig.doc, json!({"cmd": "auto_explode", "factor": 0}));
    assert!(
        error.contains("the factor must be a number more than 0"),
        "{error}"
    );
    assert_eq!(rig.placements(), before);
}

#[test]
fn a_deleted_occurrence_leaves_its_steps_with_a_warning() {
    let mut rig = rig();
    rig.run(
        json!({"cmd": "add_explode_view", "name": "Apart", "steps": [
        {"occurrences": ["Lid:1", "Wall:1"], "distance": 20, "direction": "z"}]}),
    );
    let lid = rig.occurrences[3];
    let result = rig.run(json!({"cmd": "delete_occurrence", "occurrence": lid.to_string()}));
    let warnings = result["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w == "Lid:1 was removed: the explode view Apart no longer moves it"),
        "{result}"
    );
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    let left = views[0]["steps"][0]["occurrences"].as_array().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["name"], "Wall:1");
    // Undo brings it back into the step.
    rig.doc.undo();
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    assert_eq!(
        views[0]["steps"][0]["occurrences"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // The view's own commands say nothing of it.
    let removed = rig.run(json!({"cmd": "remove_explode_view", "name": "Apart"}));
    assert!(removed.get("warnings").is_none(), "{removed}");
    assert_eq!(rig.doc.undo_label(), Some("Delete Apart"));
}

#[test]
fn explode_views_survive_the_design_file() {
    let mut rig = rig();
    let empty = rig.doc.to_json();
    assert!(!empty.contains("explode_views"));
    rig.run(
        json!({"cmd": "add_explode_view", "name": "Apart", "steps": [
        {"occurrences": ["Lid:1"], "distance": "40 mm", "direction": "z", "seconds": 2},
        {"occurrences": ["Wall:1"], "angle": "45 deg", "axis": "-z", "trails": false,
         "point": [27, 5, 7.5]}]}),
    );
    rig.run(json!({"cmd": "auto_explode", "name": "Auto"}));
    let text = rig.doc.to_json();
    assert!(text.contains("\"explode_views\""), "{text}");
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_eq!(
        query(&rig.doc, json!({"query": "explode_views"})),
        query(&copy, json!({"query": "explode_views"}))
    );
    assert_eq!(
        query(&rig.doc, json!({"query": "explode_view", "name": "Apart"})),
        query(&copy, json!({"query": "explode_view", "name": "Apart"}))
    );
    assert_eq!(copy.to_json(), text);

    // A step of an occurrence the file does not have: refused.
    let mut bad: Value = serde_json::from_str(&text).unwrap();
    bad["explode_views"][0]["steps"][0]["occurrences"][0] = json!("O99");
    let error = Document::from_json(&bad.to_string(), MockKernel::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("explode_views[0]"), "{error}");
}

/// Edit Explode View changes the steps once a command (mitcad#2527): two
/// changes together, whose indexes would name steps the other one moved
/// (a panic before), are refused in words, as is an index past the steps;
/// the view and the undo steps stay as they were. One change with the
/// trail lines still goes, the step added where it was asked.
#[test]
fn one_change_of_the_steps_a_command_and_no_index_past_them() {
    let mut rig = rig();
    rig.run(json!({"cmd": "add_explode_view", "steps": [
        {"name": "Up", "occurrences": ["Lid:1"], "distance": 40, "direction": "z"},
        {"name": "Out", "occurrences": ["Wall:1"], "distance": 10, "direction": "x"},
        {"name": "Turn", "occurrences": ["Wall:1"], "angle": 90, "axis": "z"},
    ]}));
    let before = query(&rig.doc, json!({"query": "explode_views"}));
    let depth = rig.doc.undo_depth();
    let added = json!({"name": "New", "occurrences": ["Lid:1"], "distance": 5, "direction": "z"});
    for edit in [
        json!({"cmd": "edit_explode_view", "name": "Explode1", "remove_step": 2,
               "move_step": 2, "to": 0}),
        json!({"cmd": "edit_explode_view", "name": "Explode1",
               "steps": [{"occurrences": ["Lid:1"], "distance": 5, "direction": "z"}],
               "step": 2, "set": {"distance": 20}}),
        json!({"cmd": "edit_explode_view", "name": "Explode1", "add_step": added,
               "at": 0, "remove_step": 0}),
    ] {
        let error = refused(&mut rig.doc, edit.clone());
        assert!(
            error.contains("one change of the steps a command"),
            "{edit}: {error}"
        );
        assert_eq!(
            query(&rig.doc, json!({"query": "explode_views"})),
            before,
            "{edit}"
        );
        assert_eq!(rig.doc.undo_depth(), depth, "{edit}");
    }
    for edit in [
        json!({"cmd": "edit_explode_view", "name": "Explode1", "remove_step": 3}),
        json!({"cmd": "edit_explode_view", "name": "Explode1", "move_step": 3, "to": 0,
               "trails": false}),
        json!({"cmd": "edit_explode_view", "name": "Explode1", "step": 3,
               "set": {"distance": 20}}),
    ] {
        let error = refused(&mut rig.doc, edit.clone());
        assert!(
            error.contains("Explode1 has no step 4 (it has 3)"),
            "{edit}: {error}"
        );
        assert_eq!(
            query(&rig.doc, json!({"query": "explode_views"})),
            before,
            "{edit}"
        );
        assert_eq!(rig.doc.undo_depth(), depth, "{edit}");
    }
    rig.run(
        json!({"cmd": "edit_explode_view", "name": "Explode1", "add_step": added,
                   "at": 0, "trails": false}),
    );
    assert_eq!(rig.doc.undo_depth(), depth + 1);
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    let names: Vec<&str> = views[0]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["New", "Up", "Out", "Turn"]);
}
