// SPDX-License-Identifier: MIT
//! Replace Component (mitcad#2640) with the mock kernel: a linked Bracket
//! (two blocks, A and B) held by two joints, one on a face of each block,
//! replaced through `replace_component`:
//!
//! - by an edited copy whose faces have the same names (block B taller):
//!   both joints stay, on the replacement's faces of the same names, and
//!   the replacement takes the name Bracket;
//! - by a design without block A: the joint on B finds B's face at the
//!   same place, the one on A finds nothing, stays (marked) and is
//!   reported;
//! - in all instances (two occurrences, both replaced, the old component
//!   gone) and in one (the other keeps the old component);
//! - undone and redone as one step;
//! - by another component of the design, and refused while that one is
//!   made after the joints.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::{FeatureDef, GeomRef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

fn run(doc: &mut Document<MockKernel>, cmd: Value) -> Value {
    let text = cmd.to_string();
    command(doc, cmd).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// A block of 10 × 10 × `height` with its corner at (`x`, 0, 0) in the
/// active component: its body.
fn block(doc: &mut Document<MockKernel>, x: f64, height: f64) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], height, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// A bracket design: block A at the origin (unless left out) and block B,
/// `height` high, at x = 20.
fn bracket(height: f64, with_a: bool) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    if with_a {
        block(&mut doc, 0.0, 5.0);
    }
    block(&mut doc, 20.0, height);
    doc
}

/// Bracket.mitcad, and Bracket_v2.mitcad as `v2`, in a folder of their
/// own.
fn saved(name: &str, v2: &Document<MockKernel>) -> PathBuf {
    let dir = mitcad_model::testing::unique_temp_dir(name);
    std::fs::write(dir.join("Bracket.mitcad"), bracket(5.0, true).to_json()).unwrap();
    std::fs::write(dir.join("Bracket_v2.mitcad"), v2.to_json()).unwrap();
    dir
}

fn base(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

/// Bracket.mitcad inserted linked: its component and occurrence.
fn insert(doc: &mut Document<MockKernel>, dir: &Path) -> (ComponentUid, OccurrenceUid) {
    let reply = run(
        doc,
        json!({"cmd": "insert_component", "path": "Bracket.mitcad", "base": base(dir)}),
    );
    (
        reply["component"].as_str().unwrap().parse().unwrap(),
        reply["occurrence"].as_str().unwrap().parse().unwrap(),
    )
}

/// The bodies of a component in order.
fn bodies(doc: &Document<MockKernel>, component: ComponentUid) -> Vec<BodyUid> {
    let mut bodies = doc.component_bodies(component);
    bodies.sort();
    bodies
}

/// The first face of a body, given the surface `surface` in the mock.
fn first_face(doc: &Document<MockKernel>, body: BodyUid, surface: SurfaceGeometry) -> String {
    let face = doc.body_shape(body).unwrap().faces[0].to_string();
    doc.kernel()
        .surfaces
        .borrow_mut()
        .insert(face.clone(), surface);
    face
}

/// The surface the mock gives `F<n>:import(<index>)` for every feature
/// after `after`: the faces of a replacement inserted later.
fn later_surface(
    doc: &Document<MockKernel>,
    after: FeatureUid,
    index: u32,
    surface: SurfaceGeometry,
) {
    let mut surfaces = doc.kernel().surfaces.borrow_mut();
    for n in after.0 + 1..after.0 + 80 {
        surfaces.insert(format!("F{n}:import({index})"), surface.clone());
    }
}

fn plane(origin: [f64; 3], normal: [f64; 3]) -> SurfaceGeometry {
    SurfaceGeometry::Plane { origin, normal }
}

/// A rigid joint of the root from a face of `occurrence` to a point.
fn joint(
    doc: &mut Document<MockKernel>,
    occurrence: OccurrenceUid,
    body: BodyUid,
    face: &str,
    point: [f64; 3],
) -> FeatureUid {
    let reply = run(
        doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": occurrence.to_string(),
                     "geometry": {"body": body, "face": face}},
               "b": {"geometry": {"point": point}}}),
    );
    reply["uid"].as_str().unwrap().parse().unwrap()
}

/// A joint's side a: its occurrence path and its geometry.
fn side_a(doc: &Document<MockKernel>, uid: FeatureUid) -> (String, GeomRef) {
    let entry = doc
        .features()
        .find(|f| f.uid == uid)
        .unwrap_or_else(|| panic!("{uid} was deleted"));
    match &entry.def {
        FeatureDef::Joint(joint) => (joint.a.occurrence.to_string(), joint.a.geometry.clone()),
        other => panic!("{uid} is no joint: {other:?}"),
    }
}

fn face_of(geometry: &GeomRef) -> (BodyUid, String) {
    match geometry {
        GeomRef::Face { body, face } => (*body, face.to_string()),
        other => panic!("not a face: {other:?}"),
    }
}

/// The linked base feature of a component.
fn link_feature(doc: &Document<MockKernel>, component: ComponentUid) -> FeatureUid {
    doc.assembly()
        .component(component)
        .and_then(|c| c.link.as_ref())
        .map(|l| l.feature)
        .expect("a linked component")
}

fn component_of(reply: &Value) -> ComponentUid {
    reply["component"].as_str().unwrap().parse().unwrap()
}

#[test]
fn an_edited_copy_with_the_same_face_names_keeps_both_joints() {
    let dir = saved("2640-edited", &bracket(8.0, true));
    let mut doc = Document::new(MockKernel::default());
    let (old, occurrence) = insert(&mut doc, &dir);
    let [a, b] = bodies(&doc, old)[..] else {
        panic!("two bodies");
    };
    let face_a = first_face(&doc, a, plane([5.0, 5.0, 0.0], [0.0, 0.0, -1.0]));
    let face_b = first_face(&doc, b, plane([25.0, 5.0, 5.0], [0.0, 0.0, 1.0]));
    let first = joint(&mut doc, occurrence, a, &face_a, [5.0, 5.0, 0.0]);
    let second = joint(&mut doc, occurrence, b, &face_b, [25.0, 5.0, 5.0]);
    let old_feature = link_feature(&doc, old);

    let reply = run(
        &mut doc,
        json!({"cmd": "replace_component", "occurrence": occurrence.to_string(),
               "path": "Bracket_v2.mitcad", "base": base(&dir)}),
    );
    assert_eq!(reply["unmatched"], 0, "{reply}");
    let references = reply["references"].as_array().unwrap();
    assert_eq!(references.len(), 2, "{reply}");
    for r in references {
        assert_eq!(r["matched"], "name", "{reply}");
    }
    // The replacement of the same kind takes the old one's name; the old
    // component went.
    let new = component_of(&reply);
    assert_eq!(reply["name"], "Bracket");
    assert_eq!(reply["removed"], true);
    assert!(!doc.assembly().exists(old));
    assert_eq!(
        doc.assembly().occurrence(occurrence).unwrap().component,
        new
    );
    // Both joints stay, on the faces of the same names.
    let new_feature = link_feature(&doc, new);
    let renamed = |face: &str| face.replace(&format!("{old_feature}:"), &format!("{new_feature}:"));
    for (uid, face, index) in [(first, &face_a, 0), (second, &face_b, 1)] {
        let (path, geometry) = side_a(&doc, uid);
        assert_eq!(path, occurrence.to_string());
        assert_eq!(
            face_of(&geometry),
            (BodyUid::new(new_feature, index), renamed(face))
        );
    }
}

#[test]
fn a_replacement_without_the_matching_block_breaks_one_joint_and_reports_it() {
    let dir = saved("2640-without", &bracket(5.0, false));
    let mut doc = Document::new(MockKernel::default());
    let (old, occurrence) = insert(&mut doc, &dir);
    let [a, b] = bodies(&doc, old)[..] else {
        panic!("two bodies");
    };
    let face_a = first_face(&doc, a, plane([5.0, 5.0, 0.0], [0.0, 0.0, -1.0]));
    let top_b = plane([25.0, 5.0, 5.0], [0.0, 0.0, 1.0]);
    let face_b = first_face(&doc, b, top_b.clone());
    // The replacement's one body is block B: its first face at B's place.
    later_surface(&doc, link_feature(&doc, old), 0, top_b);
    let on_a = joint(&mut doc, occurrence, a, &face_a, [5.0, 5.0, 0.0]);
    let on_b = joint(&mut doc, occurrence, b, &face_b, [25.0, 5.0, 5.0]);

    let reply = run(
        &mut doc,
        json!({"cmd": "replace_component", "occurrence": occurrence.to_string(),
               "path": "Bracket_v2.mitcad", "base": base(&dir)}),
    );
    assert_eq!(reply["unmatched"], 1, "{reply}");
    let references = reply["references"].as_array().unwrap();
    let row = |uid: FeatureUid| {
        references
            .iter()
            .find(|r| r["feature"] == uid.to_string())
            .unwrap_or_else(|| panic!("{uid} not reported: {reply}"))
    };
    assert_eq!(row(on_b)["matched"], "geometry", "{reply}");
    assert_eq!(row(on_a)["matched"], Value::Null, "{reply}");
    let message = row(on_a)["message"].as_str().unwrap();
    assert!(message.contains("no face that matches it"), "{message}");
    // In words: no ids in the message.
    assert!(!message.contains("import("), "{message}");
    let new = component_of(&reply);
    let new_feature = link_feature(&doc, new);
    // The joint on B is on the replacement's face at B's place.
    let (_, geometry) = side_a(&doc, on_b);
    assert_eq!(
        face_of(&geometry),
        (
            BodyUid::new(new_feature, 0),
            format!("{new_feature}:import(0)")
        )
    );
    // The joint on A stays, marked: on a face no face of the replacement
    // is, which names the face it had.
    let (path, geometry) = side_a(&doc, on_a);
    assert_eq!(path, occurrence.to_string());
    let (body, face) = face_of(&geometry);
    assert_eq!(body, BodyUid::new(new_feature, 0));
    assert_eq!(face, format!("{new_feature}:replaced({face_a})"));
}

#[test]
fn all_instances_replaces_both_occurrences_and_this_one_only_its_own() {
    let dir = saved("2640-instances", &bracket(8.0, true));
    let mut doc = Document::new(MockKernel::default());
    let (old, first) = insert(&mut doc, &dir);
    let copied = run(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": first.to_string(),
               "transform": {"translation": [0.0, 50.0, 0.0]}}),
    );
    let second: OccurrenceUid = copied["occurrence"].as_str().unwrap().parse().unwrap();
    let placed = |doc: &Document<MockKernel>, o: OccurrenceUid| {
        doc.assembly().occurrence(o).unwrap().transform
    };
    let at_second = placed(&doc, second);

    // This One: the other keeps the old component.
    let mut one = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    let reply = run(
        &mut one,
        json!({"cmd": "replace_component", "occurrence": second.to_string(), "all": false,
               "path": "Bracket_v2.mitcad", "base": base(&dir)}),
    );
    let new = component_of(&reply);
    assert_eq!(reply["removed"], false);
    assert_eq!(reply["name"], "Bracket_v2");
    assert_eq!(reply["occurrences"], json!([second.to_string()]));
    assert_eq!(one.assembly().occurrence(first).unwrap().component, old);
    assert_eq!(one.assembly().occurrence(second).unwrap().component, new);
    assert_eq!(one.assembly().occurrence_name(second), "Bracket_v2:1");
    assert_eq!(placed(&one, second), at_second);

    // All Instances: both, at their places; the old component goes.
    let reply = run(
        &mut doc,
        json!({"cmd": "replace_component", "occurrence": second.to_string(),
               "path": "Bracket_v2.mitcad", "base": base(&dir)}),
    );
    let new = component_of(&reply);
    assert_eq!(reply["removed"], true);
    assert_eq!(
        reply["occurrences"],
        json!([first.to_string(), second.to_string()])
    );
    for o in [first, second] {
        assert_eq!(doc.assembly().occurrence(o).unwrap().component, new);
    }
    assert_eq!(placed(&doc, second), at_second);
    assert!(!doc.assembly().exists(old));
    assert_eq!(doc.assembly().occurrences_of(new).count(), 2);
}

#[test]
fn a_replacement_is_one_undo_step_and_redoes() {
    let dir = saved("2640-undo", &bracket(8.0, true));
    let mut doc = Document::new(MockKernel::default());
    let (old, occurrence) = insert(&mut doc, &dir);
    let a = bodies(&doc, old)[0];
    let face = first_face(&doc, a, plane([5.0, 5.0, 0.0], [0.0, 0.0, -1.0]));
    let uid = joint(&mut doc, occurrence, a, &face, [5.0, 5.0, 0.0]);
    let depth = doc.undo_depth();
    let reply = run(
        &mut doc,
        json!({"cmd": "replace_component", "occurrence": occurrence.to_string(),
               "path": "Bracket_v2.mitcad", "base": base(&dir)}),
    );
    let new = component_of(&reply);
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.undo_label(), Some("Replace Bracket with Bracket_v2"));
    let replaced = side_a(&doc, uid);

    run(&mut doc, json!({"cmd": "undo"}));
    assert!(doc.assembly().exists(old));
    assert!(!doc.assembly().exists(new));
    assert_eq!(
        doc.assembly().occurrence(occurrence).unwrap().component,
        old
    );
    assert_eq!(face_of(&side_a(&doc, uid).1), (a, face.clone()));

    run(&mut doc, json!({"cmd": "redo"}));
    assert!(!doc.assembly().exists(old));
    assert_eq!(
        doc.assembly().occurrence(occurrence).unwrap().component,
        new
    );
    assert_eq!(side_a(&doc, uid), replaced);
}

/// Old and New, each a block in a component of its own placed in the
/// root, New made before the joint on Old when `new_first`.
fn two_components(new_first: bool) -> (Document<MockKernel>, OccurrenceUid, FeatureUid) {
    let mut doc = Document::new(MockKernel::default());
    let make = |doc: &mut Document<MockKernel>, name: &str| {
        let (_, occurrence) = doc
            .create_component(Some(name), Transform::IDENTITY, true)
            .unwrap();
        let body = block(doc, 0.0, 5.0);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        (occurrence, body)
    };
    let (old, body) = make(&mut doc, "Old");
    if new_first {
        make(&mut doc, "New");
    }
    let face = doc.body_shape(body).unwrap().faces[0].to_string();
    let uid = joint(&mut doc, old, body, &face, [0.0, 0.0, 0.0]);
    if !new_first {
        make(&mut doc, "New");
    }
    (doc, old, uid)
}

#[test]
fn another_component_of_the_design_takes_the_joint_when_it_is_made_before_it() {
    let (mut doc, occurrence, uid) = two_components(true);
    let reply = run(
        &mut doc,
        json!({"cmd": "replace_component", "occurrence": occurrence.to_string(),
               "with": "New"}),
    );
    assert_eq!(reply["name"], "New");
    assert_eq!(reply["unmatched"], 0, "{reply}");
    let new = component_of(&reply);
    assert_eq!(doc.assembly().occurrences_of(new).count(), 2);
    // The joint is on New's body now.
    let (body, _) = face_of(&side_a(&doc, uid).1);
    assert_eq!(
        doc.features()
            .find(|f| f.uid == body.feature)
            .unwrap()
            .component,
        new
    );

    // Made after the joint: refused, nothing changed.
    let (mut late, occurrence, _) = two_components(false);
    let before = late.to_json();
    let error = command(
        &mut late,
        json!({"cmd": "replace_component", "occurrence": occurrence.to_string(),
               "with": "New"}),
    )
    .unwrap_err();
    assert!(error.contains("is made after"), "{error}");
    assert_eq!(late.to_json(), before);
}
