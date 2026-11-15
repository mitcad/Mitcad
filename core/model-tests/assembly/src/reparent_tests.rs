// SPDX-License-Identifier: MIT
//! Moving an occurrence to another parent (mitcad#219) with the mock
//! kernel: to the root and into another component under translated and
//! rotated parents, with what is placed in it; repeated instances; links,
//! joints and Isolate following it; refusals that change nothing; undo
//! and redo as one step; the query the destination picker reads. The
//! features that place it go with it (mitcad#301): joints, as-built
//! joints, constraint rows, rigid groups, Moves and Capture Positions,
//! split when they place other occurrences too, or refused with the
//! reason when they could not keep placing it.

use std::f64::consts::FRAC_PI_2;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::test_support::{def, extrude, num};
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

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

fn reparent(occurrence: &str, parent: &str) -> Value {
    json!({"cmd": "reparent_occurrence", "occurrence": occurrence, "parent": parent})
}

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

fn turn(axis: [f64; 3], angle: f64) -> Transform {
    Transform::rotation([0.0; 3], axis, angle).unwrap()
}

/// A 10 x 10 x 5 block in the active component: its extrude, body and top
/// face (z = 5 in the component).
fn block(doc: &mut Document<MockKernel>) -> (FeatureUid, BodyUid, String) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (uid, BodyUid::new(uid, 0), format!("{uid}:end({region})"))
}

/// Every placed body's box in the design, in a stable order.
fn world(doc: &Document<MockKernel>) -> Vec<(BodyUid, [f64; 3], [f64; 3])> {
    let mut out: Vec<_> = doc
        .instances()
        .into_iter()
        .map(|i| {
            let shape = doc.body_shape(i.body).unwrap();
            let placed = doc
                .kernel()
                .transform_shape(shape, &i.transform, None)
                .unwrap();
            let bounds = placed.bounds.unwrap();
            (i.body, bounds.min, bounds.max)
        })
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn assert_same_world(
    before: &[(BodyUid, [f64; 3], [f64; 3])],
    after: &[(BodyUid, [f64; 3], [f64; 3])],
) {
    assert_eq!(before.len(), after.len(), "{before:?} != {after:?}");
    for (a, b) in before.iter().zip(after) {
        assert_eq!(a.0, b.0);
        assert!(
            (0..3).all(|i| (a.1[i] - b.1[i]).abs() < 1e-9 && (a.2[i] - b.2[i]).abs() < 1e-9),
            "{before:?} != {after:?}"
        );
    }
}

fn parent_of(doc: &Document<MockKernel>, uid: OccurrenceUid) -> ComponentUid {
    doc.assembly().occurrence(uid).unwrap().parent
}

/// Assembly:1 (O1) at (100, 0, 0) turned a quarter about Z, holding Part:1
/// (O2) at (0, 0, 5) turned a quarter about X with a block, and Pin:1 (O3)
/// in Part at (2, 0, 0) with a block. Root active.
fn nested() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(
        Some("Assembly"),
        shift(100.0, 0.0, 0.0).after(&turn([0.0, 0.0, 1.0], FRAC_PI_2)),
        true,
    )
    .unwrap();
    doc.create_component(
        Some("Part"),
        shift(0.0, 0.0, 5.0).after(&turn([1.0, 0.0, 0.0], FRAC_PI_2)),
        true,
    )
    .unwrap();
    block(&mut doc);
    doc.create_component(Some("Pin"), shift(2.0, 0.0, 0.0), true)
        .unwrap();
    block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc
}

#[test]
fn a_component_moves_to_the_root_where_it_is_with_what_is_in_it() {
    let mut doc = nested();
    let before = world(&doc);
    let features = doc.features().count();
    let (part, pin) = (OccurrenceUid(2), OccurrenceUid(3));
    let part_component = doc.assembly().occurrence(part).unwrap().component;

    let result = command(&mut doc, reparent("Assembly:1/Part:1", ""));
    assert_eq!(result["path"], "O2");
    assert_eq!(result["path_name"], "Part:1");
    assert_eq!(result["parent"], "C0");
    assert_eq!(doc.undo_label(), Some("Move Part:1 to Root"));
    // The same occurrence and component, now in the root beside Assembly,
    // with its own occurrence still in it; nothing moved in the design.
    assert_eq!(parent_of(&doc, part), ComponentUid::ROOT);
    assert_eq!(
        doc.assembly().occurrence(part).unwrap().component,
        part_component
    );
    assert_eq!(parent_of(&doc, pin), part_component);
    assert_eq!(doc.assembly().occurrence_name(part), "Part:1");
    assert_eq!(doc.features().count(), features);
    assert_same_world(&before, &world(&doc));
    let paths: Vec<String> = doc.instances().into_iter().map(|i| i.path_name).collect();
    assert!(paths.contains(&"Part:1/Pin:1".to_owned()), "{paths:?}");
    // Its placement in the root is the one Assembly gave it.
    let placed = doc.assembly().occurrence(part).unwrap().transform;
    let expected = shift(100.0, 0.0, 0.0)
        .after(&turn([0.0, 0.0, 1.0], FRAC_PI_2))
        .after(&shift(0.0, 0.0, 5.0))
        .after(&turn([1.0, 0.0, 0.0], FRAC_PI_2));
    for r in 0..3 {
        assert!((placed.translation[r] - expected.translation[r]).abs() < 1e-9);
        for c in 0..3 {
            assert!((placed.linear[r][c] - expected.linear[r][c]).abs() < 1e-9);
        }
    }

    // One undo step takes all of it back; redo moves it again.
    assert_eq!(doc.undo().as_deref(), Some("Move Part:1 to Root"));
    assert_eq!(parent_of(&doc, part), ComponentUid(1));
    assert_same_world(&before, &world(&doc));
    doc.redo();
    assert_eq!(parent_of(&doc, part), ComponentUid::ROOT);
    assert_same_world(&before, &world(&doc));

    // The design file keeps the new hierarchy.
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(parent_of(&loaded, part), ComponentUid::ROOT);
    assert_same_world(&before, &world(&loaded));
}

#[test]
fn a_component_moves_into_another_one_under_turned_parents() {
    let mut doc = nested();
    // Bracket:1 (O4) in the root at (0, 50, 0), turned about X and Y.
    doc.create_component(
        Some("Bracket"),
        shift(0.0, 50.0, 0.0)
            .after(&turn([1.0, 0.0, 0.0], 0.3))
            .after(&turn([0.0, 1.0, 0.0], -0.7)),
        true,
    )
    .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let before = world(&doc);
    let bracket = doc
        .assembly()
        .occurrence(OccurrenceUid(4))
        .unwrap()
        .component;

    command(&mut doc, reparent("O1/O2", "O4"));
    assert_eq!(parent_of(&doc, OccurrenceUid(2)), bracket);
    assert_same_world(&before, &world(&doc));
    // And back into Assembly: its first placement again.
    command(&mut doc, reparent("O4/O2", "O1"));
    assert_eq!(parent_of(&doc, OccurrenceUid(2)), ComponentUid(1));
    let placed = doc
        .assembly()
        .occurrence(OccurrenceUid(2))
        .unwrap()
        .transform;
    let first = shift(0.0, 0.0, 5.0).after(&turn([1.0, 0.0, 0.0], FRAC_PI_2));
    assert!((0..3).all(|r| (placed.translation[r] - first.translation[r]).abs() < 1e-9));
    assert_same_world(&before, &world(&doc));
    // Two steps, each undone on its own.
    doc.undo();
    assert_eq!(parent_of(&doc, OccurrenceUid(2)), bracket);
    doc.undo();
    assert_eq!(parent_of(&doc, OccurrenceUid(2)), ComponentUid(1));
}

#[test]
fn cycles_the_current_parent_and_the_root_are_refused_without_change() {
    let mut doc = nested();
    let saved = doc.to_json();
    let depth = doc.undo_depth();
    let refusals = [
        (reparent("", ""), "the root component cannot be moved"),
        (reparent("O1/O2", "O1"), "already in Assembly"),
        (reparent("O1/O2", "O1/O2"), "its own component"),
        (reparent("O1/O2", "O1/O2/O3"), "placed in it"),
        (reparent("O1", "O1/O2"), "placed in it"),
        (reparent("O1/O9", ""), "no occurrence path"),
    ];
    for (cmd, expected) in refusals {
        let error = rejected(&mut doc, cmd.clone());
        assert!(error.contains(expected), "{cmd}: {error}");
        assert_eq!(doc.to_json(), saved, "{cmd} changed the design");
        assert_eq!(doc.undo_depth(), depth);
    }
}

#[test]
fn repeated_instances_are_kept_or_refused() {
    let mut doc = nested();
    // A second Part in the root, of the same component: moving the nested
    // one leaves it as it is.
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1/O2", "transform": {"translation": [0, -40, 0]}}),
    );
    let copy = OccurrenceUid(4);
    let copy_before = doc.assembly().occurrence(copy).unwrap().clone();
    let before = world(&doc);
    command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(doc.assembly().occurrence(copy).unwrap(), &copy_before);
    assert_same_world(&before, &world(&doc));
    doc.undo();

    // Assembly placed twice: Part is in both, so it cannot leave one.
    doc.activate_component(ComponentUid::ROOT).unwrap();
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1", "transform": {"translation": [0, 0, 80]}}),
    );
    let saved = doc.to_json();
    let error = rejected(&mut doc, reparent("O1/O2", ""));
    assert!(error.contains("Assembly is placed 2 times"), "{error}");
    // Nor can anything go into Assembly: it would appear twice.
    let error = rejected(&mut doc, reparent("O4", "O1"));
    assert!(error.contains("Assembly is placed 2 times"), "{error}");
    assert_eq!(doc.to_json(), saved);
}

#[test]
fn a_move_along_turned_axes_cannot_go_with_it() {
    // Move1 moves Part:1 3 along Assembly's y axis; Assembly is turned a
    // quarter about Z, so in the root those values would move it along x.
    let mut doc = nested();
    doc.activate_component(ComponentUid(1)).unwrap();
    doc.add_feature(
        &def(json!({"type": "move_occurrence", "occurrences": ["O2"],
                    "transform": {"type": "translate_xyz", "x": 0, "y": 3, "z": 0}})),
        None,
    )
    .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let saved = doc.to_json();
    let error = rejected(&mut doc, reparent("O1/O2", ""));
    assert!(
        error.contains("Move1: it moves Part:1 along the x, y and z axes of Assembly"),
        "{error}"
    );
    assert_eq!(doc.to_json(), saved);
    // The occurrence itself can move (mitcad#301); the picker's query says
    // why the root cannot take this Move.
    let answer = query(
        &doc,
        json!({"query": "reparent_occurrence", "occurrence": "O1/O2"}),
    );
    assert_eq!(answer["problems"], json!([]), "{answer}");
    let root = answer["destinations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["occurrence"] == "")
        .unwrap()
        .clone();
    assert_eq!(root["eligible"], false, "{answer}");
    assert!(
        root["reason"].as_str().unwrap().contains("Move1"),
        "{answer}"
    );
}

/// Root with Assembly:1 (O1) 10 up holding Part:1 (O2) with a block, and
/// Cover:1 (O3) beside it with a sketch on Part's top face, linked through
/// O1/O2, with a linked projection of that face.
fn linked() -> (Document<MockKernel>, Value) {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Assembly"), shift(0.0, 0.0, 10.0), true)
        .unwrap();
    doc.create_component(Some("Part"), shift(5.0, 0.0, 0.0), true)
        .unwrap();
    let (_, body, face) = block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(Some("Cover"), shift(0.0, 50.0, -3.0), true)
        .unwrap();
    // Cover's sketch on Part's face (mitcad#220: both sides allow it).
    mitcad_model::test_support::allow_attachments(&mut doc);
    let sketch = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": body},
               "occurrence": "O1/O2"}),
    )["uid"]
        .clone();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    (doc, sketch)
}

fn origin(doc: &Document<MockKernel>, sketch: &Value) -> Vec<f64> {
    let s = query(doc, json!({"query": "sketch", "uid": sketch}));
    s["frame"]["origin"]
        .as_array()
        .unwrap_or_else(|| panic!("no frame: {s}"))
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

#[test]
fn links_and_isolate_through_the_occurrence_follow_it() {
    let (mut doc, sketch) = linked();
    let uid: FeatureUid = sketch.as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let at = origin(&doc, &sketch);
    command(
        &mut doc,
        json!({"cmd": "set_isolation", "items": [{"occurrence": "O1/O2"}]}),
    );

    let result = command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(
        result["updated"],
        json!([doc.feature(uid).unwrap().name, "Isolate"])
    );
    let s = query(&doc, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(s["plane_link"], json!({"source": "O2", "target": "O3"}));
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let moved = origin(&doc, &sketch);
    assert!(
        at.iter().zip(&moved).all(|(a, b)| (a - b).abs() < 1e-9),
        "{at:?} != {moved:?}"
    );
    let display = query(&doc, json!({"query": "document"}))["display"].clone();
    assert_eq!(display["isolated"], json!([{"occurrence": "O2"}]));

    // Undo restores the hierarchy, the link and Isolate together.
    doc.undo();
    let s = query(&doc, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(s["plane_link"], json!({"source": "O1/O2", "target": "O3"}));
    let display = query(&doc, json!({"query": "document"}))["display"].clone();
    assert_eq!(display["isolated"], json!([{"occurrence": "O1/O2"}]));
}

#[test]
fn links_across_a_parent_the_timeline_moves_are_refused() {
    let (mut doc, sketch) = linked();
    // Assembly is moved by the timeline after the sketch: the sketch sees
    // Part where Assembly is at its point of the timeline.
    doc.add_feature(
        &def(json!({"type": "move_occurrence", "occurrences": ["O1"],
                    "transform": {"type": "translate_xyz", "x": 0, "y": 0, "z": 7}})),
        None,
    )
    .unwrap();
    let name = doc
        .feature(sketch.as_str().unwrap().parse().unwrap())
        .unwrap()
        .name
        .clone();
    let saved = doc.to_json();
    let error = rejected(&mut doc, reparent("O1/O2", ""));
    assert!(
        error.contains(&name) && error.contains("Assembly:1 is placed by Move1"),
        "{error}"
    );
    assert_eq!(doc.to_json(), saved);
}

#[test]
fn joints_follow_when_the_occurrence_they_move_stays() {
    // Asm:1 (O1) holding Sub:1 (O2) holding Part:1 (O3) with a block, and
    // Plate:1 (O4) with a block, joined where they are in the root.
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Asm"), Transform::IDENTITY, true)
        .unwrap();
    doc.create_component(Some("Sub"), shift(10.0, 0.0, 0.0), true)
        .unwrap();
    doc.create_component(Some("Part"), shift(0.0, 10.0, 0.0), true)
        .unwrap();
    block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(Some("Plate"), shift(0.0, 0.0, -20.0), true)
        .unwrap();
    block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let joint = command(
        &mut doc,
        json!({"cmd": "add_as_built_joint", "kind": "rigid", "a": "O1/O2/O3", "b": "O4"}),
    );
    let uid: FeatureUid = joint["uid"].as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let name = doc.feature(uid).unwrap().name.clone();

    // Out of Sub into Asm: the joint still moves Asm:1, through the new path.
    let before = world(&doc);
    let result = command(&mut doc, reparent("O1/O2/O3", "O1"));
    assert_eq!(result["updated"], json!([name]));
    let def = query(&doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(def["a"], "O1/O3");
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_same_world(&before, &world(&doc));
    doc.undo();

    // To the root it would leave Asm:1, which the joint moves: refused,
    // naming the joint.
    let saved = doc.to_json();
    let error = rejected(&mut doc, reparent("O1/O2/O3", ""));
    assert!(error.contains(&name) && error.contains("Asm:1"), "{error}");
    assert_eq!(doc.to_json(), saved);
}

#[test]
fn the_query_lists_destinations_contents_and_the_plan() {
    let mut doc = nested();
    doc.create_component(Some("Bracket"), shift(0.0, 50.0, 0.0), true)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let depth = doc.undo_depth();
    let answer = query(
        &doc,
        json!({"query": "reparent_occurrence", "occurrence": "O1/O2", "parent": ""}),
    );
    assert_eq!(answer["name"], "Part:1");
    assert_eq!(answer["path_name"], "Root/Assembly:1/Part:1");
    assert_eq!(answer["parent_path"], "Root/Assembly:1");
    assert_eq!(answer["contents"]["occurrences"], json!(["Pin:1"]));
    assert_eq!(answer["contents"]["components"], 2);
    assert_eq!(answer["contents"]["bodies"], 2);
    assert_eq!(answer["problems"], json!([]));
    let row = |path: &str| -> Value {
        answer["destinations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["occurrence"] == path)
            .unwrap_or_else(|| panic!("no destination {path}: {answer}"))
            .clone()
    };
    assert_eq!(row("")["eligible"], true);
    assert_eq!(row("")["path"], "Root");
    assert_eq!(row("O4")["eligible"], true);
    assert_eq!(row("O4")["path"], "Root/Bracket:1");
    let current = row("O1");
    assert_eq!(current["current"], true);
    assert_eq!(current["eligible"], false);
    assert!(current["reason"].as_str().unwrap().contains("already in"));
    assert!(
        row("O1/O2")["reason"]
            .as_str()
            .unwrap()
            .contains("its own component")
    );
    assert!(
        row("O1/O2/O3")["reason"]
            .as_str()
            .unwrap()
            .contains("placed in it")
    );
    assert_eq!(answer["plan"]["eligible"], true, "{answer}");
    assert_eq!(answer["plan"]["path"], "O2");
    assert_eq!(answer["plan"]["parent_path"], "Root");
    assert_eq!(answer["plan"]["from_path"], "Root/Assembly:1");
    let x = answer["plan"]["transform"][0][3].as_f64().unwrap();
    assert!((x - 100.0).abs() < 1e-9, "{answer}");
    // Queries change nothing.
    assert_eq!(doc.undo_depth(), depth);
    assert_eq!(parent_of(&doc, OccurrenceUid(2)), ComponentUid(1));
    let refused = query(
        &doc,
        json!({"query": "reparent_occurrence", "occurrence": "O1/O2", "parent": "O1/O2/O3"}),
    );
    assert_eq!(refused["plan"]["eligible"], false);
    assert!(
        refused["plan"]["problems"][0]
            .as_str()
            .unwrap()
            .contains("placed in it")
    );
}

// The features that place the moved occurrence go with it (mitcad#301).

/// Box:1 (O1) at (20, 0, 0) turned a quarter about Z with a block of its
/// own (Extrude1 F2), holding Part:1 (O2) at (0, 30, 0), Pin:1 (O3) at
/// (0, -30, 0) and Sub:1 (O4) at (50, 0, 0), each with a block; Lid:1
/// (O5) in the root at (0, 80, 0). Box is active.
struct Boxed {
    doc: Document<MockKernel>,
    own: BodyUid,
    part: BodyUid,
    pin: BodyUid,
    sub: BodyUid,
}

fn boxed() -> Boxed {
    let mut doc = Document::new(MockKernel::default());
    let in_box = ComponentUid(1);
    doc.create_component(
        Some("Box"),
        shift(20.0, 0.0, 0.0).after(&turn([0.0, 0.0, 1.0], FRAC_PI_2)),
        true,
    )
    .unwrap();
    let (_, own, _) = block(&mut doc);
    let child = |doc: &mut Document<MockKernel>, name: &str, at: Transform| {
        doc.activate_component(in_box).unwrap();
        doc.create_component(Some(name), at, true).unwrap();
        block(doc).1
    };
    let part = child(&mut doc, "Part", shift(0.0, 30.0, 0.0));
    let pin = child(&mut doc, "Pin", shift(0.0, -30.0, 0.0));
    let sub = child(&mut doc, "Sub", shift(50.0, 0.0, 0.0));
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(Some("Lid"), shift(0.0, 80.0, 0.0), true)
        .unwrap();
    doc.activate_component(in_box).unwrap();
    Boxed {
        doc,
        own,
        part,
        pin,
        sub,
    }
}

/// A joint or constraint side: a face of `role` of a block, through the
/// occurrence path `occurrence` ("" for the component's own block).
fn side(doc: &Document<MockKernel>, occurrence: &str, body: BodyUid, role: &str) -> Value {
    let face = doc
        .body_shape(body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == role)
        .unwrap()
        .to_string();
    json!({"occurrence": occurrence, "geometry": {"body": body, "face": face}})
}

/// A rigid joint of the active component: a's face turned over onto b's.
fn joint(doc: &mut Document<MockKernel>, a: Value, b: Value) -> FeatureUid {
    let result = command(
        doc,
        json!({"cmd": "add_joint", "kind": "rigid", "flip": true, "a": a, "b": b}),
    );
    let uid: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    uid
}

fn def_of(doc: &Document<MockKernel>, uid: FeatureUid) -> Value {
    query(doc, json!({"query": "feature", "uid": uid}))["def"].clone()
}

fn component_of(doc: &Document<MockKernel>, uid: FeatureUid) -> ComponentUid {
    doc.feature(uid).unwrap().component
}

fn feature_named(doc: &Document<MockKernel>, name: &str) -> FeatureUid {
    doc.features()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no feature {name}"))
        .uid
}

fn reopened(doc: &Document<MockKernel>) -> Document<MockKernel> {
    let mut loaded = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    loaded.recompute();
    loaded
}

#[test]
fn a_joint_goes_to_the_root_with_the_occurrence_it_places() {
    let Boxed {
        mut doc, own, part, ..
    } = boxed();
    // Joint1 of Box puts Part:1's bottom onto the top of Box's own block.
    let a = side(&doc, "O2", part, "start");
    let b = side(&doc, "", own, "end");
    let uid = joint(&mut doc, a, b);
    let before = world(&doc);
    let own_before = doc
        .assembly()
        .occurrence(OccurrenceUid(2))
        .unwrap()
        .transform;
    // The joint places it: its own placement is only where solving starts.
    let solved = doc.placement(OccurrenceUid(2)).unwrap();
    assert!((own_before.translation[2] - solved.translation[2]).abs() > 1.0);

    // The picker: Part:1 can move, the root takes it with its joint.
    let answer = query(
        &doc,
        json!({"query": "reparent_occurrence", "occurrence": "O1/O2", "parent": ""}),
    );
    assert_eq!(answer["problems"], json!([]), "{answer}");
    assert_eq!(answer["plan"]["eligible"], true, "{answer}");
    assert_eq!(
        answer["plan"]["features"],
        json!([{"name": "Joint1", "from": "C1", "from_name": "Box",
                "to": "C0", "to_name": "Root", "split": null}])
    );

    let result = command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(result["features"][0]["name"], "Joint1", "{result}");
    assert_eq!(result["features"][0]["to_name"], "Root");
    // A root joint now, its paths from the root: Box's block is Box:1's.
    assert_eq!(component_of(&doc, uid), ComponentUid::ROOT);
    let def = def_of(&doc, uid);
    assert_eq!(def["a"]["occurrence"], "O2");
    assert_eq!(def["b"]["occurrence"], "O1");
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(
        query(&doc, json!({"query": "joints"}))["joints"][0]["state"],
        "placed"
    );
    assert_same_world(&before, &world(&doc));
    // It keeps its place in the timeline.
    assert_eq!(doc.features().last().unwrap().uid, uid);

    // One undo step takes the joint back with the occurrence.
    doc.undo();
    assert_eq!(parent_of(&doc, OccurrenceUid(2)), ComponentUid(1));
    assert_eq!(component_of(&doc, uid), ComponentUid(1));
    let def = def_of(&doc, uid);
    assert_eq!(def["a"]["occurrence"], "O2");
    assert_eq!(def["b"]["occurrence"], Value::Null);
    assert_same_world(&before, &world(&doc));
    doc.redo();
    assert_eq!(component_of(&doc, uid), ComponentUid::ROOT);
    assert_same_world(&before, &world(&doc));

    // The design file keeps it.
    let loaded = reopened(&doc);
    assert_eq!(component_of(&loaded, uid), ComponentUid::ROOT);
    assert_eq!(def_of(&loaded, uid)["b"]["occurrence"], "O1");
    assert!(loaded.status(uid).unwrap().is_ok());
    assert_same_world(&before, &world(&loaded));
}

#[test]
fn a_joint_to_geometry_outside_the_new_parent_keeps_the_occurrence_there() {
    let Boxed {
        mut doc, own, part, ..
    } = boxed();
    let a = side(&doc, "O2", part, "start");
    let b = side(&doc, "", own, "end");
    joint(&mut doc, a, b);
    let saved = doc.to_json();
    // In a sibling (Lid) or deeper (Sub), Part:1 would not be placed
    // directly in the component that holds both sides: the joint there
    // would move Lid:1 or Sub:1 instead.
    for (parent, name) in [("O5", "Lid"), ("O1/O4", "Sub")] {
        let error = rejected(&mut doc, reparent("O1/O2", parent));
        assert!(
            error.contains(&format!(
                "Joint1: it joins Part:1 to Box:1, which is not in {name}"
            )),
            "{error}"
        );
        // Box:1 holds Part:1 itself: the reason says only to join it again
        // (mitcad#1847).
        assert!(
            error.contains(&format!("Delete it and join Part:1 again inside {name}"))
                && !error.contains("as well"),
            "{error}"
        );
        assert_eq!(doc.to_json(), saved);
    }
}

#[test]
fn a_joint_to_geometry_of_the_new_parent_goes_into_it() {
    let Boxed {
        mut doc, part, sub, ..
    } = boxed();
    // Joint1 of Box puts Part:1 onto Sub:1's block.
    let a = side(&doc, "O2", part, "start");
    let b = side(&doc, "O4", sub, "end");
    let uid = joint(&mut doc, a, b);
    let before = world(&doc);

    // Into Sub: a joint of Sub, Sub's own block on its side b.
    let result = command(&mut doc, reparent("O1/O2", "O1/O4"));
    assert_eq!(result["features"][0]["to_name"], "Sub", "{result}");
    assert_eq!(component_of(&doc, uid), ComponentUid(4));
    let def = def_of(&doc, uid);
    assert_eq!(def["a"]["occurrence"], "O2");
    assert_eq!(def["b"]["occurrence"], Value::Null);
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_same_world(&before, &world(&doc));
    let loaded = reopened(&doc);
    assert_eq!(component_of(&loaded, uid), ComponentUid(4));
    assert_same_world(&before, &world(&loaded));
    doc.undo();
    assert_eq!(component_of(&doc, uid), ComponentUid(1));
    assert_eq!(def_of(&doc, uid)["b"]["occurrence"], "O4");

    // To the root: Sub's block through Box:1.
    command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(component_of(&doc, uid), ComponentUid::ROOT);
    assert_eq!(def_of(&doc, uid)["b"]["occurrence"], "O1/O4");
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_same_world(&before, &world(&doc));
}

#[test]
fn a_move_of_several_occurrences_is_split() {
    let Boxed { mut doc, .. } = boxed();
    // Move1 turns Part:1 and Pin:1 0.3 about Box's Z and shifts them.
    let by = shift(5.0, 2.0, 1.0).after(&turn([0.0, 0.0, 1.0], 0.3));
    let matrix = mitcad_model::assembly::matrix_rows(&by);
    let uid = doc
        .add_feature(
            &def(
                json!({"type": "move_occurrence", "occurrences": ["O2", "O3"],
                        "transform": {"type": "free", "matrix": matrix}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let before = world(&doc);
    let count = doc.features().count();

    let result = command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(
        result["features"],
        json!([{"name": "Move1", "from": "C1", "from_name": "Box",
                "to": "C0", "to_name": "Root", "split": "Move2"}])
    );
    // Move1 still moves Pin:1 in Box; Move2, right after it, moves Part:1
    // in the root by the same motion in the root's coordinates.
    assert_eq!(doc.features().count(), count + 1);
    assert_eq!(def_of(&doc, uid)["occurrences"], json!(["O3"]));
    assert_eq!(component_of(&doc, uid), ComponentUid(1));
    let split = feature_named(&doc, "Move2");
    assert_eq!(component_of(&doc, split), ComponentUid::ROOT);
    assert_eq!(def_of(&doc, split)["occurrences"], json!(["O2"]));
    let order: Vec<FeatureUid> = doc.features().map(|f| f.uid).collect();
    assert_eq!(order[order.len() - 2..], [uid, split]);
    assert!(
        doc.status(split).unwrap().is_ok(),
        "{:?}",
        doc.status(split)
    );
    assert_same_world(&before, &world(&doc));

    // Undo takes the split back too.
    doc.undo();
    assert_eq!(doc.features().count(), count);
    assert_eq!(def_of(&doc, uid)["occurrences"], json!(["O2", "O3"]));
    assert_same_world(&before, &world(&doc));
    doc.redo();
    assert_eq!(doc.features().count(), count + 1);
    let loaded = reopened(&doc);
    assert_eq!(
        component_of(&loaded, feature_named(&loaded, "Move2")),
        ComponentUid::ROOT
    );
    assert_same_world(&before, &world(&loaded));

    // A Move by values cannot be split: its parameters are its own.
    let mut doc = boxed().doc;
    doc.add_feature(
        &def(
            json!({"type": "move_occurrence", "occurrences": ["O2", "O3"],
                    "transform": {"type": "translate_xyz", "x": 0, "y": 3, "z": 0}}),
        ),
        None,
    )
    .unwrap();
    let error = rejected(&mut doc, reparent("O1/O2", "O1/O4"));
    assert!(
        error.contains("Move1: it also moves Pin:1; a Move by values cannot be split"),
        "{error}"
    );
}

#[test]
fn a_move_by_values_goes_whole_when_the_axes_agree() {
    // Sub:1 is not turned in Box: a move along Box's axes is the same
    // motion along Sub's, so Move1 goes into Sub unchanged.
    let Boxed { mut doc, .. } = boxed();
    let uid = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": ["O2"],
                        "transform": {"type": "translate_xyz", "x": 0, "y": 3, "z": 0}})),
            None,
        )
        .unwrap()
        .uid;
    let before = world(&doc);
    let def_before = def_of(&doc, uid);
    command(&mut doc, reparent("O1/O2", "O1/O4"));
    assert_eq!(component_of(&doc, uid), ComponentUid(4));
    assert_eq!(def_of(&doc, uid), def_before);
    assert_same_world(&before, &world(&doc));
}

#[test]
fn a_captured_position_goes_with_it() {
    let Boxed { mut doc, .. } = boxed();
    let at = shift(3.0, 40.0, 2.0).after(&turn([1.0, 0.0, 0.0], 0.4));
    let uid = doc
        .capture_positions(&[(OccurrenceUid(2), at)])
        .unwrap()
        .uid;
    assert_eq!(component_of(&doc, uid), ComponentUid(1));
    let before = world(&doc);
    let result = command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(result["features"][0]["name"], "Position1", "{result}");
    assert_eq!(component_of(&doc, uid), ComponentUid::ROOT);
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_same_world(&before, &world(&doc));
    doc.undo();
    assert_eq!(component_of(&doc, uid), ComponentUid(1));
    assert_same_world(&before, &world(&doc));
    doc.redo();
    let loaded = reopened(&doc);
    assert_eq!(component_of(&loaded, uid), ComponentUid::ROOT);
    assert_same_world(&before, &world(&loaded));
}

#[test]
fn a_rigid_group_holds_it_to_its_old_parent() {
    let Boxed { mut doc, part, .. } = boxed();
    let result = command(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["O2", "O3"]}),
    );
    let uid: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    let before = world(&doc);
    let result = command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(result["features"][0]["split"], Value::Null, "{result}");
    // Pin:1 stays in Box: in the root Part:1 is held to Box:1.
    assert_eq!(component_of(&doc, uid), ComponentUid::ROOT);
    assert_eq!(def_of(&doc, uid)["occurrences"], json!(["O2", "O1"]));
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_same_world(&before, &world(&doc));
    let loaded = reopened(&doc);
    assert_eq!(def_of(&loaded, uid)["occurrences"], json!(["O2", "O1"]));
    assert_same_world(&before, &world(&loaded));

    // A later move of Box:1 takes Part:1 along, as when it was in Box.
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.add_feature(
        &def(json!({"type": "move_occurrence", "occurrences": ["O1"],
                    "transform": {"type": "translate_xyz", "x": 0, "y": 0, "z": 10}})),
        None,
    )
    .unwrap();
    let moved = world(&doc);
    let z = |w: &[(BodyUid, [f64; 3], [f64; 3])]| w.iter().find(|b| b.0 == part).unwrap().1[2];
    assert!((z(&moved) - z(&before) - 10.0).abs() < 1e-9, "{moved:?}");
    while doc
        .undo()
        .is_some_and(|label| label != "Move Part:1 to Root")
    {}
    assert_eq!(component_of(&doc, uid), ComponentUid(1));
    assert_eq!(def_of(&doc, uid)["occurrences"], json!(["O2", "O3"]));
}

#[test]
fn constraint_rows_of_the_occurrence_are_split_off() {
    let Boxed {
        mut doc,
        own,
        part,
        pin,
        ..
    } = boxed();
    // Constraint1 of Box: Part:1's bottom 2 above Box's top, Pin:1's on it.
    let rows = json!([
        {"type": "align", "a": side(&doc, "O2", part, "start"), "b": side(&doc, "", own, "end"),
         "offset": 2},
        {"type": "align", "a": side(&doc, "O3", pin, "start"), "b": side(&doc, "", own, "end")}
    ]);
    let result = command(&mut doc, json!({"cmd": "add_constraint_set", "rows": rows}));
    let uid: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let owner = |doc: &Document<MockKernel>| -> Vec<Value> {
        query(doc, json!({"query": "parameters"}))
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| {
                p["comment"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("Constraint1")
            })
            .map(|p| p["owner"].clone())
            .collect()
    };
    assert_eq!(owner(&doc), vec![json!(uid.to_string())]);
    let before = world(&doc);

    let result = command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(result["features"][0]["split"], "Constraint2", "{result}");
    let split = feature_named(&doc, "Constraint2");
    assert_eq!(component_of(&doc, uid), ComponentUid(1));
    assert_eq!(component_of(&doc, split), ComponentUid::ROOT);
    let kept = def_of(&doc, uid);
    assert_eq!(kept["rows"].as_array().unwrap().len(), 1);
    assert_eq!(kept["rows"][0]["a"]["occurrence"], "O3");
    let moved = def_of(&doc, split);
    assert_eq!(moved["rows"][0]["a"]["occurrence"], "O2");
    assert_eq!(moved["rows"][0]["b"]["occurrence"], "O1");
    // The row's offset goes with it.
    assert_eq!(owner(&doc), vec![json!(split.to_string())]);
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert!(
        doc.status(split).unwrap().is_ok(),
        "{:?}",
        doc.status(split)
    );
    assert_same_world(&before, &world(&doc));
    let loaded = reopened(&doc);
    assert_eq!(
        component_of(&loaded, feature_named(&loaded, "Constraint2")),
        ComponentUid::ROOT
    );
    assert_same_world(&before, &world(&loaded));

    doc.undo();
    assert!(doc.features().all(|f| f.name != "Constraint2"));
    assert_eq!(def_of(&doc, uid)["rows"].as_array().unwrap().len(), 2);
    assert_eq!(owner(&doc), vec![json!(uid.to_string())]);
}

#[test]
fn what_the_joint_holds_it_to_must_stay_after_it() {
    let Boxed {
        mut doc, part, pin, ..
    } = boxed();
    // Joint1 puts Part:1 onto Pin:1; a later Move of Box moves Pin:1,
    // which Part:1 in the root would no longer follow.
    let a = side(&doc, "O2", part, "start");
    let b = side(&doc, "O3", pin, "end");
    joint(&mut doc, a, b);
    doc.add_feature(
        &def(json!({"type": "move_occurrence", "occurrences": ["O3"],
                    "transform": {"type": "translate_xyz", "x": 0, "y": 0, "z": 4}})),
        None,
    )
    .unwrap();
    let saved = doc.to_json();
    let error = rejected(&mut doc, reparent("O1/O2", ""));
    assert!(
        error.contains("Joint1: Pin:1 is placed by Move1 later in the timeline"),
        "{error}"
    );
    assert_eq!(doc.to_json(), saved);
}

#[test]
fn an_as_built_joint_and_a_group_of_three_go_with_it() {
    let Boxed { mut doc, .. } = boxed();
    // Part:1 held where it is to Box's own geometry, and grouped with Pin:1
    // and Sub:1.
    let result = command(
        &mut doc,
        json!({"cmd": "add_as_built_joint", "kind": "rigid", "a": "O2", "b": ""}),
    );
    let joint: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    let result = command(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["O3", "O2", "O4"]}),
    );
    let group: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    let before = world(&doc);
    let result = command(&mut doc, reparent("O1/O2", ""));
    assert_eq!(result["features"][0]["name"], "AsBuiltJoint1", "{result}");
    assert_eq!(result["features"][1]["split"], "RigidGroup2", "{result}");
    assert_eq!(component_of(&doc, joint), ComponentUid::ROOT);
    let def = def_of(&doc, joint);
    assert_eq!(
        (def["a"].clone(), def["b"].clone()),
        (json!("O2"), json!("O1"))
    );
    assert_eq!(def_of(&doc, group)["occurrences"], json!(["O3", "O4"]));
    let split = feature_named(&doc, "RigidGroup2");
    assert_eq!(component_of(&doc, split), ComponentUid::ROOT);
    assert_eq!(def_of(&doc, split)["occurrences"], json!(["O2", "O1"]));
    for uid in [joint, group, split] {
        assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    }
    assert_same_world(&before, &world(&doc));
    doc.undo();
    assert_eq!(
        def_of(&doc, group)["occurrences"],
        json!(["O3", "O2", "O4"])
    );
    assert!(doc.features().all(|f| f.name != "RigidGroup2"));
}

#[test]
fn a_joint_that_would_move_the_old_parent_instead_is_refused() {
    let Boxed {
        mut doc, own, part, ..
    } = boxed();
    // Side a is Box's own block, which cannot move in Box: Part:1 (b)
    // moves. In the root a is Box:1, free, and moves first.
    let a = side(&doc, "", own, "end");
    let b = side(&doc, "O2", part, "start");
    joint(&mut doc, a, b);
    let saved = doc.to_json();
    let depth = doc.undo_depth();
    let error = rejected(&mut doc, reparent("O1/O2", ""));
    assert!(
        error.contains("the move would change where")
            && error.contains("Joint1 would place the occurrences differently in Root"),
        "{error}"
    );
    assert_eq!(doc.to_json(), saved);
    assert_eq!(doc.undo_depth(), depth);
    // With Box:1 grounded it stays, and Part:1 moves onto it as before.
    doc.set_occurrence_grounded(OccurrenceUid(1), true).unwrap();
    let before = world(&doc);
    command(&mut doc, reparent("O1/O2", ""));
    assert_same_world(&before, &world(&doc));
}

// Several occurrences of one parent move together (mitcad#1847).

/// A roller on a shaft, all in the root: Shaft:1 (O1), Bearing:1 (O2) and
/// Tube:1 (O3), each with a block; Bearing:1 and Tube:1 in RigidGroup1,
/// Bearing:1 joined onto Shaft:1 by Joint1; Holder:1 (O4), empty, beside
/// them. The root is active.
fn roller() -> (Document<MockKernel>, FeatureUid, FeatureUid) {
    let mut doc = Document::new(MockKernel::default());
    let part = |doc: &mut Document<MockKernel>, name: &str, at: Transform| {
        doc.activate_component(ComponentUid::ROOT).unwrap();
        doc.create_component(Some(name), at, true).unwrap();
        block(doc).1
    };
    let shaft = part(&mut doc, "Shaft", shift(0.0, 0.0, 0.0));
    let bearing = part(&mut doc, "Bearing", shift(20.0, 0.0, 0.0));
    part(&mut doc, "Tube", shift(40.0, 0.0, 0.0));
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(Some("Holder"), shift(0.0, 80.0, 0.0), true)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let result = command(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["O2", "O3"]}),
    );
    let group: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    let a = side(&doc, "O2", bearing, "start");
    let b = side(&doc, "O1", shaft, "end");
    let joint = joint(&mut doc, a, b);
    (doc, group, joint)
}

#[test]
fn every_unavailable_parent_of_a_grouped_roller_says_why() {
    let (doc, ..) = roller();
    let answer = query(
        &doc,
        json!({"query": "reparent_occurrence", "occurrence": "O2"}),
    );
    assert_eq!(answer["problems"], json!([]), "{answer}");
    // Its rigid group is offered to move with it.
    assert_eq!(
        answer["rigid_group"],
        json!([{"occurrence": "O3", "name": "Tube:1", "path": "O3", "path_name": "Root/Tube:1"}])
    );
    let destinations = answer["destinations"].as_array().unwrap();
    assert!(!destinations.is_empty());
    for row in destinations {
        assert_eq!(row["eligible"], false, "{row}");
        let reason = row["reason"].as_str().unwrap();
        assert!(!reason.is_empty(), "{row}");
        // In words: no uids of occurrences, components or features.
        let uid = reason
            .as_bytes()
            .windows(2)
            .any(|w| matches!(w[0], b'O' | b'C' | b'F') && w[1].is_ascii_digit());
        assert!(!uid, "{reason}");
    }
    let holder = destinations
        .iter()
        .find(|d| d["occurrence"] == "O4")
        .unwrap()["reason"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        holder.contains(
            "RigidGroup1: it holds Bearing:1 rigidly to Tube:1, which would not be in Holder"
        ),
        "{holder}"
    );
    assert!(
        holder.contains("Joint1: it joins Bearing:1 to Shaft:1, which is not in Holder"),
        "{holder}"
    );
    // Each blocker also on its own, as the picker lists them under the row.
    let reasons = &destinations
        .iter()
        .find(|d| d["occurrence"] == "O4")
        .unwrap()["reasons"];
    assert_eq!(reasons.as_array().map(Vec::len), Some(2), "{reasons}");
}

#[test]
fn a_rigid_group_joined_to_a_shaft_outside_is_refused_with_what_to_do() {
    let (mut doc, ..) = roller();
    let saved = doc.to_json();
    let depth = doc.undo_depth();
    // Both members together: the group could go whole, the joint to the
    // shaft in the root cannot.
    let answer = query(
        &doc,
        json!({"query": "reparent_occurrence", "occurrence": "O2", "with": ["O3"],
               "parent": "O4"}),
    );
    assert_eq!(answer["rigid_group"], json!([]), "{answer}");
    assert_eq!(answer["plan"]["eligible"], false, "{answer}");
    let problems = answer["plan"]["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{answer}");
    let problem = problems[0].as_str().unwrap();
    assert!(
        problem.starts_with("Joint1: it joins Bearing:1 to Shaft:1, which is not in Holder")
            && problem.contains(
                "Move Shaft:1 into Holder as well, or delete it and join Bearing:1 again inside \
                 Holder"
            ),
        "{problem}"
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": "O2", "with": ["O3"], "parent": "O4"}),
    );
    assert!(
        error.contains("Move Shaft:1 into Holder as well"),
        "{error}"
    );
    assert_eq!(doc.to_json(), saved);
    assert_eq!(doc.undo_depth(), depth);
}

#[test]
fn a_rigid_group_with_its_shaft_moves_as_one() {
    let (mut doc, group, joint) = roller();
    let before = world(&doc);
    let depth = doc.undo_depth();
    let holder = doc
        .assembly()
        .occurrence(OccurrenceUid(4))
        .unwrap()
        .component;
    let plan = query(
        &doc,
        json!({"query": "reparent_occurrence", "occurrence": "O2", "with": ["O3", "O1"],
               "parent": "O4"}),
    )["plan"]
        .clone();
    assert_eq!(plan["eligible"], true, "{plan}");
    assert_eq!(plan["moved"], json!(["O4/O2", "O4/O3", "O4/O1"]));

    let result = command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": "O2", "with": ["O3", "O1"],
               "parent": "O4"}),
    );
    assert_eq!(result["path"], "O4/O2", "{result}");
    assert_eq!(result["moved"].as_array().unwrap().len(), 3, "{result}");
    let names: Vec<&str> = result["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["RigidGroup1", "Joint1"], "{result}");
    assert!(
        result["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["split"].is_null())
    );
    for uid in [1, 2, 3] {
        assert_eq!(parent_of(&doc, OccurrenceUid(uid)), holder);
    }
    // The group and the joint go whole into Holder, their paths unchanged.
    assert_eq!(component_of(&doc, group), holder);
    assert_eq!(def_of(&doc, group)["occurrences"], json!(["O2", "O3"]));
    assert_eq!(component_of(&doc, joint), holder);
    let def = def_of(&doc, joint);
    assert_eq!(
        (
            def["a"]["occurrence"].clone(),
            def["b"]["occurrence"].clone()
        ),
        (json!("O2"), json!("O1"))
    );
    for uid in [group, joint] {
        assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    }
    assert_same_world(&before, &world(&doc));
    let loaded = reopened(&doc);
    assert_eq!(component_of(&loaded, joint), holder);
    assert_same_world(&before, &world(&loaded));

    // One undo step puts all three and their features back.
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(
        doc.undo().as_deref(),
        Some("Move Bearing:1, Tube:1, Shaft:1 to Holder")
    );
    for uid in [1, 2, 3] {
        assert_eq!(parent_of(&doc, OccurrenceUid(uid)), ComponentUid::ROOT);
    }
    assert_eq!(component_of(&doc, group), ComponentUid::ROOT);
    assert_eq!(component_of(&doc, joint), ComponentUid::ROOT);
    assert_same_world(&before, &world(&doc));
}

#[test]
fn occurrences_of_different_parents_do_not_move_together() {
    let mut doc = nested();
    doc.create_component(Some("Bracket"), shift(0.0, 50.0, 0.0), true)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let saved = doc.to_json();
    let error = rejected(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": "O1/O2", "with": ["O4"],
               "parent": ""}),
    );
    assert!(
        error.contains("Part:1 and Bracket:1 are in different components"),
        "{error}"
    );
    assert_eq!(doc.to_json(), saved);
}
