// SPDX-License-Identifier: MIT
//! An edit's preview with the joints after the edited one (mitcad#1860).
//!
//! An editor rolls the marker back to just before the feature it edits,
//! so the joints and rigid groups after it are rolled back too: without
//! them the occurrences they place stand where their own placements put
//! them, and an assembly came apart in the view while one of its joints
//! was edited. A preview given `through` (the marker the editor rolled
//! back from) computes the features right after the edited one that only
//! place occurrences, so they hold in the preview as in the design.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

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

/// The feature's place in the timeline.
fn position(doc: &Document<MockKernel>, uid: FeatureUid) -> usize {
    doc.features().position(|f| f.uid == uid).unwrap()
}

fn run(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    let result: Value = match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    };
    assert_eq!(result["error"], Value::Null, "{result}");
    result
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

/// A grounded plate (`Plate:1`), a pin on it (`Pin:1`, Joint1) and a
/// second pin on the first (`Pin:2`, a later joint), the two pins in a
/// rigid group after it; `between` (a command) is added between the two
/// joints.
struct Stack {
    doc: Document<MockKernel>,
    o1: OccurrenceUid,
    o2: OccurrenceUid,
    o3: OccurrenceUid,
    plate_body: BodyUid,
    pin_body: BodyUid,
    joint1: FeatureUid,
}

fn stack(between: Option<Value>) -> Stack {
    let mut doc = Document::new(MockKernel::default());
    let (_, o1) = doc
        .create_component(Some("Plate"), Transform::IDENTITY, true)
        .unwrap();
    let plate_body = block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, o2) = doc
        .create_component(Some("Pin"), Transform::translation([100.0, 0.0, 0.0]), true)
        .unwrap();
    let pin_body = block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let o3 = doc
        .copy_occurrence(o2, Some(Transform::translation([200.0, 0.0, 0.0])))
        .unwrap();
    doc.set_occurrence_grounded(o1, true).unwrap();
    let pin_bottom = face(&doc, pin_body, "start");
    let pin_top = face(&doc, pin_body, "end");
    let plate_top = face(&doc, plate_body, "end");
    let joint1 = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid", "flip": true,
               "a": {"occurrence": "Pin:1", "geometry": {"body": pin_body, "face": pin_bottom}},
               "b": {"occurrence": "Plate:1", "geometry": {"body": plate_body, "face": plate_top}}}),
    )["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    if let Some(command) = between {
        run(&mut doc, command);
    }
    run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid", "flip": true,
               "a": {"occurrence": "Pin:2", "geometry": {"body": pin_body, "face": pin_bottom}},
               "b": {"occurrence": "Pin:1", "geometry": {"body": pin_body, "face": pin_top}}}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["Pin:1", "Pin:2"]}),
    );
    Stack {
        doc,
        o1,
        o2,
        o3,
        plate_body,
        pin_body,
        joint1,
    }
}

impl Stack {
    /// Joint1 with its Offset set to `offset`, as the Joint panel sends it.
    fn joint1_def(&self, offset: f64) -> Value {
        json!({"type": "joint", "kind": "rigid", "flip": true, "offset": offset,
               "a": {"occurrence": self.o2.to_string(),
                     "geometry": {"body": self.pin_body, "face": face(&self.doc, self.pin_body, "start")}},
               "b": {"occurrence": self.o1.to_string(),
                     "geometry": {"body": self.plate_body, "face": face(&self.doc, self.plate_body, "end")}}})
    }

    /// The marker rolled back to Joint1, as its editor does; the marker it
    /// was at.
    fn edit_joint1(&mut self) -> usize {
        let marker = self.doc.marker();
        let at = position(&self.doc, self.joint1);
        self.doc.set_temporary_marker(at).unwrap();
        marker
    }

    fn preview(&mut self, offset: f64, through: Option<usize>) -> Value {
        let (uid, def) = (self.joint1.to_string(), self.joint1_def(offset));
        let mut command = json!({"cmd": "edit_feature", "uid": uid, "def": def});
        if let Some(through) = through {
            command["through"] = json!(through);
        }
        let report: Value =
            serde_json::from_str(&self.doc.preview(&command.to_string()).unwrap()).unwrap();
        assert_eq!(report["status"], "ok", "{report}");
        report
    }

    fn z(&self, o: OccurrenceUid) -> f64 {
        self.doc.placement(o).unwrap().translation[2]
    }
}

/// The height the preview places `o` at, None when it does not place it.
fn placed_z(report: &Value, o: OccurrenceUid) -> Option<f64> {
    report["placements"]
        .as_array()?
        .iter()
        .find(|p| p["path"] == o.to_string())
        .map(|p| p["transform"][2][3].as_f64().unwrap())
}

fn assert_close(value: Option<f64>, expected: f64, report: &Value) {
    let value = value.unwrap_or_else(|| panic!("not placed: {report}"));
    assert!(
        (value - expected).abs() < 1e-6,
        "{value} != {expected}: {report}"
    );
}

#[test]
fn an_edit_preview_keeps_the_later_joints_and_rigid_groups() {
    let mut stack = stack(None);
    // The design: the pin on the plate's top, the second pin on the pin.
    assert!((stack.z(stack.o2) - 5.0).abs() < 1e-6);
    assert!((stack.z(stack.o3) - 10.0).abs() < 1e-6);
    let marker = stack.edit_joint1();
    // Rolled back to Joint1, both pins are where their own placements put
    // them: the stack comes apart.
    assert!(stack.z(stack.o2).abs() < 1e-6);
    assert!(stack.z(stack.o3).abs() < 1e-6);

    // Without `through` the later joint stays rolled back (mitcad#309):
    // the second pin is left where it is.
    let report = stack.preview(3.0, None);
    assert_close(placed_z(&report, stack.o2), 8.0, &report);
    assert_eq!(placed_z(&report, stack.o3), None, "{report}");

    // With it, the later joint holds: the second pin follows the pin,
    // 3 mm higher, as it will after OK.
    let report = stack.preview(3.0, Some(marker));
    assert_close(placed_z(&report, stack.o2), 8.0, &report);
    assert_close(placed_z(&report, stack.o3), 13.0, &report);
    // The same Offset as before: the stack as the design has it.
    let report = stack.preview(0.0, Some(marker));
    assert_close(placed_z(&report, stack.o2), 5.0, &report);
    assert_close(placed_z(&report, stack.o3), 10.0, &report);
    // Nothing committed: the document stays rolled back.
    assert!(stack.z(stack.o3).abs() < 1e-6);
    assert_eq!(stack.doc.marker(), position(&stack.doc, stack.joint1));
}

#[test]
fn an_edit_preview_stops_at_the_first_feature_that_places_nothing() {
    // A construction plane between the joints: the preview computes no
    // feature from it on, so the later joint stays rolled back.
    let plane = json!({"cmd": "add_feature", "def": {"type": "construction_plane",
        "definition": {"type": "offset", "plane": "xy", "distance": 5}}});
    let mut stack = stack(Some(plane));
    assert!((stack.z(stack.o3) - 10.0).abs() < 1e-6);
    let marker = stack.edit_joint1();
    let report = stack.preview(3.0, Some(marker));
    assert_close(placed_z(&report, stack.o2), 8.0, &report);
    assert_eq!(placed_z(&report, stack.o3), None, "{report}");
    // Nor past the marker the editor rolled back from.
    let mut stack = stack_without_marker_end();
    let at = position(&stack.doc, stack.joint1);
    let report = stack.preview(3.0, Some(at + 1));
    assert_close(placed_z(&report, stack.o2), 8.0, &report);
    assert_eq!(placed_z(&report, stack.o3), None, "{report}");
}

/// The stack with the marker moved back to just after Joint1 by hand and
/// then rolled back to Joint1 by an editor: the later joint was rolled
/// back before the edit began.
fn stack_without_marker_end() -> Stack {
    let mut stack = stack(None);
    let at = position(&stack.doc, stack.joint1);
    stack.doc.set_marker(at + 1).unwrap();
    stack.doc.set_temporary_marker(at).unwrap();
    stack
}
