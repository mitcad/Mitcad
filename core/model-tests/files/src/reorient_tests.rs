// SPDX-License-Identifier: MIT
//! Reorient Design (mitcad#502) on the mock kernel: a design with a sketch
//! on XY, an extrusion, a construction plane offset from XZ with a sketch
//! and an extrusion of its own, a fillet, a hole and an imported body,
//! turned by quarter turns about each axis: the same bodies turned, face
//! names unchanged, references to origin planes rewritten with their
//! sense, one undo step that restores the design exactly.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::BoundingBox;
use mitcad_model::reorient::{Turn, TurnAxis};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, GeomRef, OriginDatum};
use mitcad_model_api::DocumentApi;

const RECT: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

fn run(doc: &mut Document<MockKernel>, value: Value) -> Value {
    command(doc, value.clone()).unwrap_or_else(|e| panic!("{value}: {e}"))
}

fn uid(text: &str) -> FeatureUid {
    text.parse().unwrap()
}

fn body(text: &str) -> BodyUid {
    text.parse().unwrap()
}

fn entry<'a>(doc: &'a Document<MockKernel>, f: &str) -> &'a mitcad_model::FeatureEntry {
    doc.features().find(|e| e.uid == uid(f)).unwrap()
}

/// Sketch1 (F1) on XY: 60 x 40 at the origin, Extrude1 (F2) 20 up; Plane1
/// (F3) offset 40 from XZ, Sketch2 (F4) on it with a 20 x 10 rectangle,
/// Extrude2 (F5) 15 joined to the block; Fillet1 (F6) on a vertical edge;
/// Hole1 (F7) through the top; Base1 (F8), an imported body.
fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": RECT}],
            "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
            "definition": {"type": "offset", "plane": "xz", "distance": 40}}}),
    );
    run(&mut doc, json!({"cmd": "sketch.create", "plane": "F3"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [20, -15], "width": 20, "height": 10}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F4", "region": RECT}],
            "extent": {"type": "distance", "distance": 15}, "operation": "join",
            "participants": ["F2.b0"]}}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
            "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 5}}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "hole",
            "placement": {"type": "face", "body": "F2.b0", "face": format!("F2:end({RECT})"),
                          "points": [[15, 20, 20]]},
            "diameter": 8, "extent": {"type": "through_all"}}}),
    );
    let brep = serde_json::to_value(mitcad_model::features::Brep::new(b"6 box".to_vec())).unwrap();
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "base", "bodies": [{"brep": brep, "name": "Imported"}]}}),
    );
    for f in ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8"] {
        assert_eq!(doc.status(uid(f)), Some(&FeatureStatus::Ok), "{f}");
    }
    doc
}

fn boxes(doc: &Document<MockKernel>) -> BTreeMap<BodyUid, BoundingBox> {
    [body("F2.b0"), body("F8.b0")]
        .into_iter()
        .map(|b| (b, doc.body_shape(b).unwrap().bounds.unwrap()))
        .collect()
}

fn faces(doc: &Document<MockKernel>, b: &str) -> Vec<String> {
    let mut names: Vec<String> = doc
        .body_shape(body(b))
        .unwrap()
        .faces
        .iter()
        .map(|f| f.to_string())
        .collect();
    names.sort();
    names
}

fn turned_box(turn: Turn, b: &BoundingBox) -> BoundingBox {
    let (p, q) = (turn.vector(b.min), turn.vector(b.max));
    BoundingBox {
        min: std::array::from_fn(|i| p[i].min(q[i])),
        max: std::array::from_fn(|i| p[i].max(q[i])),
    }
}

fn near(a: &BoundingBox, b: &BoundingBox) -> bool {
    (0..3).all(|i| (a.min[i] - b.min[i]).abs() < 1e-9 && (a.max[i] - b.max[i]).abs() < 1e-9)
}

fn sketch_frame(doc: &Document<MockKernel>, f: &str) -> mitcad_model::SketchFrame {
    doc.sketch_output(uid(f)).unwrap().frame
}

fn same_frame(a: &mitcad_model::SketchFrame, b: &mitcad_model::SketchFrame) -> bool {
    let near = |u: [f64; 3], v: [f64; 3]| (0..3).all(|i| (u[i] - v[i]).abs() < 1e-12);
    near(a.origin, b.origin) && near(a.x_axis, b.x_axis) && near(a.y_axis, b.y_axis)
}

#[test]
fn ninety_degrees_about_x_turns_the_bodies_and_rewrites_the_origin_references() {
    let mut doc = design();
    let before = boxes(&doc);
    let frames = [sketch_frame(&doc, "F1"), sketch_frame(&doc, "F4")];
    let names = [faces(&doc, "F2.b0"), faces(&doc, "F8.b0")];
    let reply = run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    );
    assert_eq!(reply["turn"], "90 degrees about X");
    assert_eq!(reply["moves"], json!(["Move1"]));
    let turn = Turn::about(TurnAxis::X, 1);
    // The block (with its boss, fillet and hole) and the imported body,
    // where the turn takes them, with the same face names.
    let after = boxes(&doc);
    for (b, old) in &before {
        assert!(
            near(&after[b], &turned_box(turn, old)),
            "{b}: {:?}",
            after[b]
        );
    }
    assert_eq!(
        after[&body("F2.b0")].min,
        [0.0, -20.0, 0.0],
        "the block hangs below XZ"
    );
    assert_eq!(after[&body("F2.b0")].max, [60.0, 0.0, 55.0]);
    assert_eq!(faces(&doc, "F2.b0"), names[0]);
    assert_eq!(faces(&doc, "F8.b0"), names[1]);
    // Sketch1 lies on XZ, facing -Y (XY's +Z turned), not mirrored: its
    // frame is its old one turned.
    let FeatureDef::Sketch(sketch) = &entry(&doc, "F1").def else {
        panic!("a sketch");
    };
    assert_eq!(sketch.plane, SketchPlane::Xz);
    assert!(sketch.frame.is_some());
    assert!(same_frame(
        &sketch_frame(&doc, "F1"),
        &turn.frame(&frames[0])
    ));
    assert_eq!(sketch_frame(&doc, "F1").normal(), [0.0, -1.0, 0.0]);
    // Plane1 was offset from XZ, which the turn takes onto XY exactly.
    let FeatureDef::ConstructionPlane(plane) = &entry(&doc, "F3").def else {
        panic!("a plane");
    };
    assert_eq!(
        plane.definition,
        mitcad_model::features::construction::PlaneDefinition::Offset {
            plane: GeomRef::Origin(OriginDatum::Xy),
            distance: plane_distance(&plane.definition),
        }
    );
    assert!(same_frame(
        &sketch_frame(&doc, "F4"),
        &turn.frame(&frames[1])
    ));
    // Every feature still builds; the Move follows the base feature.
    for f in ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8"] {
        assert_eq!(doc.status(uid(f)), Some(&FeatureStatus::Ok), "{f}");
    }
    let timeline: Vec<String> = doc.features().map(|f| f.name.clone()).collect();
    assert_eq!(timeline.last().map(String::as_str), Some("Move1"));
    assert_eq!(
        timeline[timeline.len() - 2],
        "Base1",
        "the Move comes right after the imported body"
    );
}

fn plane_distance(
    definition: &mitcad_model::features::construction::PlaneDefinition,
) -> mitcad_model::parameters::ParamId {
    match definition {
        mitcad_model::features::construction::PlaneDefinition::Offset { distance, .. } => *distance,
        _ => panic!("an offset plane"),
    }
}

#[test]
fn every_axis_and_quarter_turn_turns_the_same_bodies() {
    for axis in ["x", "y", "z"] {
        for degrees in [90, 180, 270] {
            let mut doc = design();
            let before = boxes(&doc);
            let names = faces(&doc, "F2.b0");
            let frames = [sketch_frame(&doc, "F1"), sketch_frame(&doc, "F4")];
            run(
                &mut doc,
                json!({"cmd": "reorient_design", "axis": axis, "degrees": degrees}),
            );
            let turn = Turn::about(TurnAxis::parse(axis).unwrap(), degrees / 90);
            let after = boxes(&doc);
            for (b, old) in &before {
                assert!(
                    near(&after[b], &turned_box(turn, old)),
                    "{axis} {degrees}: {b}: {:?}",
                    after[b]
                );
            }
            assert_eq!(faces(&doc, "F2.b0"), names, "{axis} {degrees}");
            for (f, frame) in ["F1", "F4"].iter().zip(&frames) {
                assert!(
                    same_frame(&sketch_frame(&doc, f), &turn.frame(frame)),
                    "{axis} {degrees}: {f}"
                );
            }
        }
    }
}

#[test]
fn an_origin_plane_becomes_xy() {
    let mut doc = design();
    let before = boxes(&doc);
    let reply = run(&mut doc, json!({"cmd": "reorient_design", "plane": "xz"}));
    assert_eq!(reply["turn"], "XZ becomes XY");
    let turn = Turn::about(TurnAxis::X, 1);
    for (b, old) in &before {
        assert!(near(&boxes(&doc)[b], &turned_box(turn, old)), "{b}");
    }
    let mut doc = design();
    run(&mut doc, json!({"cmd": "reorient_design", "plane": "yz"}));
    let turn = Turn::about(TurnAxis::Y, 3);
    for (b, old) in &before {
        assert!(near(&boxes(&doc)[b], &turned_box(turn, old)), "{b}");
    }
}

#[test]
fn undo_restores_the_design_exactly() {
    let mut doc = design();
    let file = doc.to_json();
    let before = boxes(&doc);
    run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "y", "degrees": 270}),
    );
    assert_ne!(doc.to_json(), file);
    let undone = run(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(undone["label"], "Reorient Design");
    assert_eq!(doc.to_json(), file);
    for (b, old) in &before {
        assert!(near(&boxes(&doc)[b], old), "{b}");
    }
}

#[test]
fn moves_turn_in_turned_coordinates() {
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": RECT}],
            "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "move", "bodies": ["F2.b0"],
            "transform": {"type": "translate_xyz", "x": 10, "y": 20, "z": 30}}}),
    );
    let before = boxes_of(&doc, "F2.b0");
    run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    );
    let turn = Turn::about(TurnAxis::X, 1);
    assert!(near(&boxes_of(&doc, "F2.b0"), &turned_box(turn, &before)));
    // The y value (20) now runs along +Z; the z value (30) along -Y.
    let values: BTreeMap<String, f64> = doc
        .parameters()
        .iter()
        .map(|p| (p.name().to_owned(), p.value()))
        .collect();
    let FeatureDef::Move(m) = &entry(&doc, "F3").def else {
        panic!("a move");
    };
    let mitcad_model::features::moves::MoveSpec::TranslateXyz { x, y, z } = &m.transform else {
        panic!("along the axes");
    };
    let value = |id: &mitcad_model::parameters::ParamId| values[&doc.parameters().name(*id)];
    assert_eq!([value(x), value(y), value(z)], [10.0, -30.0, 20.0]);
}

fn boxes_of(doc: &Document<MockKernel>, b: &str) -> BoundingBox {
    doc.body_shape(body(b)).unwrap().bounds.unwrap()
}

#[test]
fn a_design_turns_with_its_components() {
    // A component placed at (100, 0, 0) with a block of its own (made in
    // it), and the root's block: both turn, and the component's own
    // sketch is rewritten in its coordinates.
    let mut doc = Document::new(MockKernel::default());
    run(
        &mut doc,
        json!({"cmd": "create_component", "name": "Part", "transform": {"translation": [100, 0, 0]}}),
    );
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 20}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": RECT}],
            "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );
    let instance = |doc: &Document<MockKernel>| {
        doc.instances()
            .into_iter()
            .find(|i| i.body == body("F2.b0"))
            .unwrap()
            .transform
    };
    let local = boxes_of(&doc, "F2.b0");
    run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "z", "degrees": 90}),
    );
    let turn = Turn::about(TurnAxis::Z, 1);
    // In the component's turned coordinates, and placed at (0, 100, 0).
    assert!(near(&boxes_of(&doc, "F2.b0"), &turned_box(turn, &local)));
    assert_eq!(instance(&doc).translation, [0.0, 100.0, 0.0]);
    assert_eq!(
        instance(&doc).linear,
        mitcad_model::Transform::IDENTITY.linear
    );
}

#[test]
fn a_joint_to_the_origin_turns_with_the_design() {
    // Pin (a component of its own) jointed rigidly by its XY plane to the
    // root's XZ plane: after the turn the joint names the turned XZ (XY,
    // exactly) and the pin is where the turn takes it.
    let mut doc = Document::new(MockKernel::default());
    run(
        &mut doc,
        json!({"cmd": "create_component", "name": "Pin", "transform": {"translation": [100, 0, 0]}}),
    );
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 20}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": RECT}],
            "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );
    run(
        &mut doc,
        json!({"cmd": "activate_component", "component": "Root"}),
    );
    let joint = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid",
            "a": {"occurrence": "Pin:1", "geometry": "xy"}, "b": {"geometry": "xz"}}),
    );
    assert_eq!(joint["error"], Value::Null, "{joint}");
    let placed = |doc: &Document<MockKernel>| {
        doc.instances()
            .into_iter()
            .find(|i| i.body == body("F2.b0"))
            .unwrap()
            .transform
    };
    let before = placed(&doc);
    run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    );
    let turn = Turn::about(TurnAxis::X, 1);
    let expected = turn.conjugate(&before);
    let now = placed(&doc);
    for r in 0..3 {
        assert!(
            (now.translation[r] - expected.translation[r]).abs() < 1e-9,
            "{now:?}"
        );
        for c in 0..3 {
            assert!(
                (now.linear[r][c] - expected.linear[r][c]).abs() < 1e-9,
                "{now:?}"
            );
        }
    }
    let joint = doc
        .features()
        .find(|f| matches!(f.def, FeatureDef::Joint(_)))
        .unwrap();
    let FeatureDef::Joint(def) = &joint.def else {
        unreachable!()
    };
    assert_eq!(def.b.geometry, GeomRef::Origin(OriginDatum::Xy));
    // The pin's own XY turned faces -Y in its coordinates: a fixed plane.
    assert!(matches!(def.a.geometry, GeomRef::FixedPlane { .. }));
}

#[test]
fn a_sketch_on_a_face_keeps_its_place_on_the_face() {
    // A face's plane takes its in-plane x axis from the model's X: turned
    // about Z the top face faces +Z as before, so its frame alone would
    // turn the sketch back by the quarter turn. The sketch's own frame
    // keeps it turned with the design.
    let mut doc = design();
    run(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": format!("F2:end({RECT})"), "body": "F2.b0"}}),
    );
    let sketch = "F9";
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [40, 25], "width": 10, "height": 8}),
    );
    let frame = sketch_frame(&doc, sketch);
    run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "z", "degrees": 90}),
    );
    let turn = Turn::about(TurnAxis::Z, 1);
    assert!(same_frame(&sketch_frame(&doc, sketch), &turn.frame(&frame)));
    let FeatureDef::Sketch(def) = &entry(&doc, sketch).def else {
        panic!("a sketch");
    };
    assert!(matches!(def.plane, SketchPlane::Face { .. }));
    assert!(
        def.frame.is_some(),
        "an own frame turns it back onto the design"
    );
    let features: Vec<String> = doc.features().map(|f| f.name.clone()).collect();
    assert!(features.contains(&"Sketch3".to_owned()));
}

#[test]
fn a_base_feature_that_joins_is_refused() {
    let mut doc = design();
    let brep = serde_json::to_value(mitcad_model::features::Brep::new(b"6 box".to_vec())).unwrap();
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "base", "bodies": [{"brep": brep}],
            "operation": "join", "participants": ["F2.b0"]}}),
    );
    let file = doc.to_json();
    let error = command(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    )
    .unwrap_err();
    assert!(error.contains("Base2"), "{error}");
    assert_eq!(doc.to_json(), file, "nothing changed");
}

#[test]
fn identity_and_bad_turns_are_refused() {
    let mut doc = design();
    for (value, expected) in [
        (
            json!({"cmd": "reorient_design", "plane": "xy"}),
            "leaves the design",
        ),
        (
            json!({"cmd": "reorient_design", "axis": "x", "degrees": 45}),
            "90, 180 or 270",
        ),
        (
            json!({"cmd": "reorient_design", "axis": "w", "degrees": 90}),
            "unknown axis",
        ),
        (json!({"cmd": "reorient_design", "axis": "x"}), "takes"),
        (
            json!({"cmd": "reorient_design", "plane": "x"}),
            "unknown origin plane",
        ),
    ] {
        let error = command(&mut doc, value.clone()).unwrap_err();
        assert!(error.contains(expected), "{value}: {error}");
    }
}

/// A 60 x 40 x 20 block (Sketch1 F1, Extrude1 F2) with a pocket cut into
/// its front (Sketch2 F3, Extrude2 F4) that splits the front face in two:
/// `#0` at x 15 and `#1` at x 45, next to the pocket (the mock numbers
/// pieces by their points as the OCCT kernel does); Fillet1 (F5) on the top
/// edge of `#1` and Chamfer1 (F6) on an edge of the fillet's face, so that
/// the piece is named in a key too.
fn split_front() -> Document<MockKernel> {
    let kernel = MockKernel::default();
    kernel
        .split_faces
        .borrow_mut()
        .insert("F2:side(c1[c4,c2])".to_owned());
    let mut doc = Document::new(kernel);
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": RECT}],
            "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [40, -5], "width": 10, "height": 25}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F3", "region": RECT}],
            "extent": {"type": "distance", "distance": 20}, "operation": "cut",
            "participants": ["F2.b0"]}}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
            "edges": [piece_edges(1).0], "radius": 3}}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0",
            "edges": [piece_edges(1).1], "size": {"type": "equal_distance", "distance": 1}}}),
    );
    for f in ["F2", "F4", "F5", "F6"] {
        assert_eq!(doc.status(uid(f)), Some(&FeatureStatus::Ok), "{f}");
    }
    doc
}

/// The fillet's and the chamfer's edges on front piece `k`.
fn piece_edges(k: u32) -> (String, String) {
    let side = format!("F2:side(c1[c4,c2])#{k}");
    let fillet = format!("E{{F2:end({RECT})|{side}}}");
    let chamfer = format!("E{{{side}|F5:fillet({fillet})}}");
    (fillet, chamfer)
}

#[test]
fn pieces_of_split_faces_are_renamed_as_the_turned_design_numbers_them() {
    // The pieces are numbered by their points, x first: turns that change
    // which piece comes first renumber them, and the fillet and the
    // chamfer would name the other piece's edges (mitcad#537). They follow
    // the piece next to the pocket through turn after turn.
    let mut doc = split_front();
    // The piece's point (the tool's centre) and the other's.
    let (pocket, other) = ([45.0, 7.5, 10.0], [15.0, 27.5, 10.0]);
    let first = |a: [f64; 3], b: [f64; 3]| {
        a.iter()
            .zip(&b)
            .map(|(x, y)| x.total_cmp(y))
            .find(|o| o.is_ne())
            .is_some_and(|o| o.is_lt())
    };
    let mut total = Turn::IDENTITY;
    for (axis, degrees, (about, quarters)) in [
        ("z", 180, (TurnAxis::Z, 2)),
        ("x", 270, (TurnAxis::X, 3)),
        ("z", 90, (TurnAxis::Z, 1)),
        ("z", 270, (TurnAxis::Z, 3)),
        ("y", 90, (TurnAxis::Y, 1)),
    ] {
        run(
            &mut doc,
            json!({"cmd": "reorient_design", "axis": axis, "degrees": degrees}),
        );
        total = Turn::about(about, quarters).after(&total);
        let k = if first(total.vector(pocket), total.vector(other)) {
            0
        } else {
            1
        };
        let file = doc.to_json();
        let (fillet, chamfer) = piece_edges(k);
        let (other_fillet, other_chamfer) = piece_edges(1 - k);
        assert!(file.contains(&fillet), "{axis} {degrees}: {fillet}");
        assert!(file.contains(&chamfer), "{axis} {degrees}: {chamfer}");
        assert!(!file.contains(&other_fillet), "{axis} {degrees}");
        assert!(!file.contains(&other_chamfer), "{axis} {degrees}");
        for f in ["F2", "F4", "F5", "F6"] {
            assert_eq!(doc.status(uid(f)), Some(&FeatureStatus::Ok), "{f}");
        }
    }
    // Z 180 renumbers them: Fillet1 and Chamfer1 are rewritten.
    let mut again = split_front();
    let reply = run(
        &mut again,
        json!({"cmd": "reorient_design", "axis": "z", "degrees": 180}),
    );
    let features: Vec<&str> = reply["features"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        features.contains(&"Fillet1") && features.contains(&"Chamfer1"),
        "{reply}"
    );
}

/// A part as the .iam import brings one in with its history: its stored
/// body as a base feature (Base1) and a block extruded after it, copied
/// into the component `leg` of the assembly and placed at (100, 0, 0).
fn leg_assembly() -> (Document<MockKernel>, mitcad_model::ids::ComponentUid) {
    let mut part = Document::new(MockKernel::default());
    let brep = serde_json::to_value(mitcad_model::features::Brep::new(b"6 box".to_vec())).unwrap();
    run(
        &mut part,
        json!({"cmd": "add_feature", "def": {"type": "base", "bodies": [{"brep": brep, "name": "Leg"}]}}),
    );
    run(&mut part, json!({"cmd": "sketch.create"}));
    run(
        &mut part,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [0, 0], "width": 30, "height": 10}),
    );
    run(
        &mut part,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F2", "region": RECT}],
            "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );
    let mut doc = Document::new(MockKernel::default());
    let place = mitcad_model::Transform {
        linear: mitcad_model::Transform::IDENTITY.linear,
        translation: [100.0, 0.0, 0.0],
    };
    let (component, _) = doc
        .add_component_copy(
            &part,
            Some("leg"),
            mitcad_model::ids::ComponentUid::ROOT,
            place,
        )
        .unwrap();
    (doc, component)
}

#[test]
fn a_part_copied_with_its_imported_body_turns() {
    // The structure of the bench assembly's figure part (mitcad#537): a
    // base feature in a copied component with a feature after it.
    let (mut doc, component) = leg_assembly();
    let bodies = doc.component_bodies(component);
    assert_eq!(bodies.len(), 2);
    let before: Vec<BoundingBox> = bodies
        .iter()
        .map(|b| doc.body_shape(*b).unwrap().bounds.unwrap())
        .collect();
    let reply = run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 270}),
    );
    assert_eq!(reply["moves"], json!(["Move1"]), "{reply}");
    let turn = Turn::about(TurnAxis::X, 3);
    for (b, was) in bodies.iter().zip(&before) {
        assert!(near(
            &doc.body_shape(*b).unwrap().bounds.unwrap(),
            &turned_box(turn, was)
        ));
    }
}

#[test]
fn a_move_that_would_fail_is_named_with_its_place_and_error() {
    // The kernel cannot move the imported body (as OCCT's transform
    // refused a body whose shell is not connected): the refusal names the
    // Move, the base feature it follows, its component and the error.
    let (mut doc, _) = leg_assembly();
    doc.kernel().fail.borrow_mut().insert("transform");
    let file = doc.to_json();
    let error = command(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 270}),
    )
    .unwrap_err();
    assert!(
        error.contains("Move1 (after Base1, in leg) would fail: transform failed"),
        "{error}"
    );
    assert_eq!(doc.to_json(), file, "nothing changed");
}

#[test]
fn a_body_that_departs_names_its_component_the_feature_and_how_far() {
    // A cut whose piece stays where it was, whatever the turn (the mock's
    // fixed piece boxes): the block departs from the turn after the cut.
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, json!({"cmd": "create_component", "name": "Box"}));
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": RECT}],
            "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [30, 0], "width": 30, "height": 40}),
    );
    *doc.kernel().piece_boxes.borrow_mut() = Some(vec![BoundingBox {
        min: [0.0, 0.0, 0.0],
        max: [30.0, 40.0, 20.0],
    }]);
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F3", "region": RECT}],
            "extent": {"type": "distance", "distance": 20}, "operation": "cut",
            "participants": ["F2.b0"]}}),
    );
    let error = command(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "z", "degrees": 90}),
    )
    .unwrap_err();
    // Turned 90 about Z the piece would be at x -40..0, y 0..30.
    assert!(
        error.contains(
            "of Box (made by Extrude1) departs from the turn after Extrude2: its box up to \
             40.000 mm from where the turn takes it"
        ),
        "{error}"
    );
}

#[test]
fn the_move_after_a_base_feature_names_its_bodies_by_their_numbers() {
    // As Find Repeated Parts leaves a base feature whose copy B of A was
    // taken out (mitcad#1306): `[A, C]`, C still `F1.b2` with its faces
    // from 12 on. The Move turns `F1.b0` and `F1.b2`, not the gone `F1.b1`.
    let mut doc = Document::new(MockKernel::default());
    let brep = serde_json::to_value(mitcad_model::features::Brep::new(b"6 box".to_vec())).unwrap();
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "base", "bodies": [
            {"brep": brep, "name": "A"},
            {"brep": brep, "name": "C", "number": 2, "first_face": 12}]}}),
    );
    let before: Vec<BoundingBox> = ["F1.b0", "F1.b2"]
        .iter()
        .map(|b| boxes_of(&doc, b))
        .collect();
    let reply = run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    );
    assert_eq!(reply["moves"], json!(["Move1"]), "{reply}");
    let mv: &mitcad_model::FeatureEntry = doc
        .features()
        .find(|f| matches!(f.def, FeatureDef::Move(_)))
        .expect("a Move after the base feature");
    let FeatureDef::Move(def) = &mv.def else {
        panic!("a move");
    };
    assert_eq!(def.bodies, vec![body("F1.b0"), body("F1.b2")]);
    let uids: Vec<FeatureUid> = doc.features().map(|f| f.uid).collect();
    for f in uids {
        assert_eq!(doc.status(f), Some(&FeatureStatus::Ok), "{f}");
    }
    let turn = Turn::about(TurnAxis::X, 1);
    for (b, was) in ["F1.b0", "F1.b2"].iter().zip(&before) {
        assert!(near(&boxes_of(&doc, b), &turned_box(turn, was)), "{b}");
    }
}

/// A 24-bit BMP file of `width` x `height` grey pixels, 4000 pixels per
/// metre (0.25 mm each), as base64.
fn bmp(width: u32, height: u32) -> String {
    let row = (width * 3).div_ceil(4) * 4;
    let pixels = row * height;
    let mut bytes = b"BM".to_vec();
    bytes.extend_from_slice(&(54 + pixels).to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 24, 0]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&pixels.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.resize(bytes.len() + pixels as usize, 0x80);
    mitcad_model::internal::base64::encode(&bytes)
}

fn points(value: &Value) -> Vec<[f64; 3]> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("no points: {value}"))
        .iter()
        .map(|p| std::array::from_fn(|i| p[i].as_f64().unwrap()))
        .collect()
}

fn near_point(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-6)
}

/// A comment pinned to a point and an image reference on the XY plane turn
/// with the design (mitcad#1315): the comment's place is its old place
/// turned, in the query and in the file, and the image lies where the turn
/// takes it.
#[test]
fn point_comments_and_image_references_turn_with_the_design() {
    // A box from x 90 to 100, y -5 to 5, z 0 to 10.
    let mut doc = Document::new(MockKernel::default());
    run(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [90, -5], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    );
    run(
        &mut doc,
        json!({"cmd": "add_comment", "text": "a hole here", "target": {"at": [95, 0, 5]}}),
    );
    run(
        &mut doc,
        json!({"cmd": "insert_image_reference", "data": bmp(40, 20), "plane": "xy",
               "at": [95, 0, 0]}),
    );
    let image = |doc: &Document<MockKernel>| {
        let images: Value =
            serde_json::from_str(&doc.query(r#"{"query": "image_references"}"#).unwrap()).unwrap();
        images[0].clone()
    };
    let comment = |doc: &Document<MockKernel>| {
        let comments: Value =
            serde_json::from_str(&doc.query(r#"{"query": "comments"}"#).unwrap()).unwrap();
        comments["comments"][0].clone()
    };
    let corners = points(&image(&doc)["corners"]);
    assert_eq!(corners.len(), 4);
    assert!(near_point(
        points(&json!([comment(&doc)["place"]]))[0],
        [95.0, 0.0, 5.0]
    ));

    let turn = Turn::about(TurnAxis::Z, 1);
    run(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "z", "degrees": 90}),
    );
    // The point (95, 0, 5) turned a quarter about Z: (0, 95, 5).
    let turned = comment(&doc);
    assert_eq!(turned["orphaned"], false, "{turned}");
    let place = points(&json!([turned["place"]]))[0];
    assert!(near_point(place, [0.0, 95.0, 5.0]), "{turned}");
    let at = points(&json!([turned["target"]["at"]]))[0];
    assert!(near_point(at, [0.0, 95.0, 5.0]), "{turned}");
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let saved = points(&json!([file["comments"][0]["place"]]))[0];
    assert!(
        near_point(saved, [0.0, 95.0, 5.0]),
        "{}",
        file["comments"][0]
    );
    // The image's corners, each the old one turned.
    let now = points(&image(&doc)["corners"]);
    assert_eq!(now.len(), 4);
    for corner in &corners {
        let expected = turn.vector(*corner);
        assert!(
            now.iter().any(|c| near_point(*c, expected)),
            "{expected:?} not in {now:?}"
        );
    }
}
