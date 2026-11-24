// SPDX-License-Identifier: MIT
//! Extrude with a planar body face as the profile (mitcad#535), with the
//! mock kernel: what reaches the kernel (the face as found, its key, the
//! direction and the sides), Join, Cut and New Body, a sketch-free imported
//! body, the face followed through upstream edits, undo, redo and a saved
//! design, the automatic operation, and the refusals: a face that is gone,
//! not planar or of a mesh body fails the feature without another profile
//! taken, and options a face's extrusion has not are refused. The extents
//! of sketch profiles (mitcad#2033): up to an object (a face of the same
//! body or another, a plane) with an offset, and through all.

use serde_json::{Value, json};

use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::exchange::{BaseInput, ImportBody};
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{Block, block, def, extrude, num};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn last_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default()
}

fn history(doc: &Document<MockKernel>, body: BodyUid) -> String {
    doc.body_shape(body)
        .map_or_else(|| "none".to_owned(), |s| s.history.clone())
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .map(|s| s.faces.iter().map(ToString::to_string).collect())
        .unwrap_or_default()
}

/// An extrusion of `face` of `body` with `extent` and `operation`.
fn face_extrusion(body: BodyUid, face: &str, extent: Value, operation: &str) -> Value {
    json!({"type": "extrude", "profile_face": {"body": body, "face": face},
           "extent": extent, "operation": operation})
}

fn distance(d: f64) -> Value {
    json!({"type": "distance", "distance": d})
}

/// The block's top face, `F2:end(<region>)`.
fn top(b: &Block) -> String {
    format!("F2:end({})", b.region)
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

/// A body imported without a history: six faces `F1:import(0)` to
/// `F1:import(5)`, from the base feature "part" (`history` "mesh" makes it
/// a mesh body).
fn imported(doc: &mut Document<MockKernel>, history: &str) -> BodyUid {
    let imported = doc
        .add_base_feature(BaseInput {
            name: Some("part".to_owned()),
            ..BaseInput::new(vec![ImportBody {
                name: Some("Part".to_owned()),
                color: None,
                shape: MockShape::imported(history, 6),
                face_colors: Vec::new(),
            }])
        })
        .unwrap();
    imported.bodies[0].uid
}

#[test]
fn a_box_face_extrudes_without_a_sketch_and_joins() {
    let mut b = block();
    let top = top(&b);
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &top, distance(5.0), "join"),
    );
    assert_eq!(uid, FeatureUid(3));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: 5")
    );
    assert_eq!(
        history(&b.doc, b.body),
        format!("join(prism(F2:0..20),face_prism(F3:{top}:0..5))")
    );
    // The caps are keyed by the face, the sides by the face's edges.
    let names = faces(&b.doc, b.body);
    for name in [
        format!("F3:start({top})"),
        format!("F3:end({top})"),
        format!("F3:side(E{{{top}|F2:side(c1[c4,c2])}})"),
        format!("F3:side(E{{{top}|F2:side(c3[c2,c4])}})"),
    ] {
        assert!(names.contains(&name), "{name} not in {names:?}");
    }
    // No sketch region went into it.
    assert_eq!(b.doc.kernel().count("extrude_face"), 1);
}

#[test]
fn flip_and_negative_distances_go_into_the_body_for_cut_and_new_body() {
    let mut b = block();
    let top = top(&b);
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &top, distance(-5.0), "cut"),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} reversed from 0: 5")
    );
    assert_eq!(
        history(&b.doc, b.body),
        format!("cut(prism(F2:0..20),face_prism(F3:{top}:-0..5))")
    );
    // Flipped, a positive distance goes in too.
    let mut flipped = face_extrusion(b.body, &top, distance(5.0), "cut");
    flipped["flip"] = json!(true);
    b.doc.edit_feature(uid, &def(flipped)).unwrap();
    assert_eq!(
        history(&b.doc, b.body),
        format!("cut(prism(F2:0..20),face_prism(F3:{top}:-0..5))")
    );
    // A new body of the face's prism; the block stays.
    b.doc
        .edit_feature(
            uid,
            &def(face_extrusion(b.body, &top, distance(5.0), "new_body")),
        )
        .unwrap();
    assert_eq!(history(&b.doc, b.body), "prism(F2:0..20)");
    assert_eq!(
        history(&b.doc, BodyUid::new(uid, 0)),
        format!("face_prism(F3:{top}:0..5)")
    );
}

#[test]
fn two_sides_symmetric_and_a_start_offset() {
    let mut b = block();
    let top = top(&b);
    let new_body = BodyUid::new(FeatureUid(3), 0);
    let uid = add(
        &mut b.doc,
        face_extrusion(
            b.body,
            &top,
            json!({"type": "symmetric", "distance": 4}),
            "new_body",
        ),
    );
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: 4; 4")
    );
    assert_eq!(
        history(&b.doc, new_body),
        format!("face_prism(F3:{top}:-4..4)")
    );
    let cases = [
        (
            json!({"type": "symmetric", "distance": 4, "full_length": true}),
            None,
            "from 0: 2; 2",
        ),
        (
            json!({"type": "two_sides", "side1": {"type": "distance", "distance": 5},
                   "side2": {"type": "distance", "distance": 3}}),
            None,
            "from 0: 5; 3",
        ),
        // A side of length zero leaves the other side alone.
        (
            json!({"type": "two_sides", "side1": {"type": "distance", "distance": 0},
                   "side2": {"type": "distance", "distance": 3}}),
            None,
            "reversed from 0: 3",
        ),
        (
            distance(5.0),
            Some(json!({"type": "offset", "offset": 2})),
            "from 2: 5",
        ),
    ];
    for (extent, start, spec) in cases {
        let mut value = face_extrusion(b.body, &top, extent, "new_body");
        if let Some(start) = start {
            value["start"] = start;
        }
        b.doc.edit_feature(uid, &def(value)).unwrap();
        assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
        assert_eq!(
            last_spec(&b.doc),
            format!("extrude_face F3 {top} key {top} {spec}")
        );
    }
    assert_eq!(
        history(&b.doc, new_body),
        format!("face_prism(F3:{top}:2..7)")
    );
}

/// Signed two-sided distances (mitcad#765) on a body face: a negative side
/// ends on the other side's side, the solid lies between the two ends
/// (5 and -3: from 3 to 5 above the face); ends that meet are refused;
/// without `signed` the sizes count.
#[test]
fn signed_two_sides_of_a_face_lie_between_their_ends() {
    let mut b = block();
    let top = top(&b);
    let new_body = BodyUid::new(FeatureUid(3), 0);
    let two_sides = |d1: f64, d2: f64, signed: bool| {
        json!({"type": "two_sides", "signed": signed,
               "side1": {"type": "distance", "distance": d1},
               "side2": {"type": "distance", "distance": d2}})
    };
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &top, two_sides(5.0, -3.0, true), "new_body"),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: 5; -3")
    );
    // From the top 3 to 5 up.
    assert_eq!(
        history(&b.doc, new_body),
        format!("face_prism(F3:{top}:3..5)")
    );
    // Mirrored, with tapers: into the block, 3 to 5 below the top.
    let mut mirrored = two_sides(-3.0, 5.0, true);
    mirrored["side1"]["taper"] = json!(0.1);
    mirrored["side2"]["taper"] = json!(-0.05);
    b.doc
        .edit_feature(
            uid,
            &def(face_extrusion(b.body, &top, mirrored, "new_body")),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: -3; 5 taper 0.1 -0.05")
    );
    // Ends that meet are refused.
    let error = b
        .doc
        .edit_feature(
            uid,
            &def(face_extrusion(
                b.body,
                &top,
                two_sides(5.0, -5.0, true),
                "new_body",
            )),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("the two sides overlap"), "{error}");
    // Without the flag each side goes its own way by its size.
    b.doc
        .edit_feature(
            uid,
            &def(face_extrusion(
                b.body,
                &top,
                two_sides(5.0, -3.0, false),
                "new_body",
            )),
        )
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: 5; 3")
    );
}

#[test]
fn a_face_of_an_imported_body_extrudes_in_a_design_without_sketches() {
    let mut doc = Document::new(MockKernel::default());
    let body = imported(&mut doc, "brep");
    assert_eq!(body.to_string(), "F1.b0");
    let uid = add(
        &mut doc,
        face_extrusion(body, "F1:import(2)", distance(5.0), "join"),
    );
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(doc.timeline().len(), 2, "a base feature and the extrusion");
    let joined = history(&doc, body);
    assert!(
        joined.starts_with("join(") && joined.ends_with(",face_prism(F2:F1:import(2):0..5))"),
        "{joined}"
    );
    let names = faces(&doc, body);
    assert!(
        names.contains(&"F2:side(E{F1:import(1)|F1:import(2)})".to_owned()),
        "{names:?}"
    );
}

#[test]
fn the_face_reference_follows_edits_undo_redo_and_a_saved_design() {
    let mut b = block();
    let top = top(&b);
    let before = b.doc.to_json();
    assert!(!before.contains("profile_face"), "{before}");
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &top, distance(5.0), "join"),
    );
    // The block 30 deep: the face is found where it is now.
    b.doc.set_parameter("d3", 30.0).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        history(&b.doc, b.body),
        format!("join(prism(F2:0..30),face_prism(F3:{top}:0..5))")
    );
    b.doc.undo();
    assert_eq!(
        history(&b.doc, b.body),
        format!("join(prism(F2:0..20),face_prism(F3:{top}:0..5))")
    );
    b.doc.redo();
    assert_eq!(
        history(&b.doc, b.body),
        format!("join(prism(F2:0..30),face_prism(F3:{top}:0..5))")
    );
    // The distance edited.
    b.doc
        .edit_feature(
            uid,
            &def(face_extrusion(b.body, &top, distance(8.0), "join")),
        )
        .unwrap();
    assert_eq!(
        history(&b.doc, b.body),
        format!("join(prism(F2:0..30),face_prism(F3:{top}:0..8))")
    );
    // Saved and opened again: the same face and result.
    let saved = b.doc.to_json();
    assert!(saved.contains("\"profile_face\""), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(status(&loaded, uid), FeatureStatus::Ok);
    assert_eq!(history(&loaded, b.body), history(&b.doc, b.body));
}

#[test]
fn a_new_extrusion_of_a_face_joins_going_out_and_cuts_going_in() {
    let mut b = block();
    let top = top(&b);
    let preview = |doc: &mut Document<MockKernel>, d: f64| -> Value {
        let command = json!({"cmd": "add_feature",
                             "def": face_extrusion(b.body, &top, distance(d), "new_body"),
                             "auto_operation": true});
        let report: Value =
            serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap();
        assert_eq!(report["status"], "ok", "{report}");
        report
    };
    let out = preview(&mut b.doc, 5.0);
    assert_eq!(
        out["auto_operation"],
        json!({"operation": "join", "objects": [b.body.to_string()]}),
        "{out}"
    );
    let into = preview(&mut b.doc, -5.0);
    assert_eq!(
        into["auto_operation"],
        json!({"operation": "cut", "objects": [b.body.to_string()]}),
        "{into}"
    );
}

#[test]
fn a_face_that_is_gone_fails_without_taking_another_profile() {
    let mut b = block();
    // An unused sketch profile beside it, which must not stand in.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    b.doc
        .add_rectangle(sketch, [100.0, 0.0], &num(10.0), &num(10.0))
        .unwrap();
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, "F2:top", distance(5.0), "join"),
    );
    let why = error(&b.doc, uid);
    assert!(why.contains("F2:top"), "{why}");
    assert_eq!(b.doc.kernel().count("extrude_face"), 0);
    assert_eq!(history(&b.doc, b.body), "prism(F2:0..20)");
}

#[test]
fn faces_that_are_not_planar_or_of_mesh_bodies_are_refused() {
    let mut b = block();
    let side = "F2:side(c1[c4,c2])";
    b.doc.kernel().surfaces.borrow_mut().insert(
        side.to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, side, distance(5.0), "join"),
    );
    let why = error(&b.doc, uid);
    assert!(why.contains("is not planar"), "{why}");
    assert_eq!(history(&b.doc, b.body), "prism(F2:0..20)");

    let mesh = imported(&mut b.doc, "mesh");
    let face = format!("{}:import(0)", mesh.feature);
    let uid = add(
        &mut b.doc,
        face_extrusion(mesh, &face, distance(5.0), "new_body"),
    );
    let why = error(&b.doc, uid);
    assert!(
        why.contains("is a mesh body") && why.contains("Extrude cannot use"),
        "{why}"
    );
}

#[test]
fn options_a_face_extrusion_has_not_are_refused() {
    let mut b = block();
    let top = top(&b);
    let refused = |doc: &mut Document<MockKernel>, value: Value, text: &str| {
        let why = doc.add_feature(&def(value), None).unwrap_err().to_string();
        assert!(why.contains(text), "{text}: {why}");
    };
    let base = face_extrusion(b.body, &top, distance(5.0), "join");
    let mut with_profiles = base.clone();
    with_profiles["profiles"] = json!([{"sketch": b.sketch, "region": b.region}]);
    refused(&mut b.doc, with_profiles, "takes no sketch profiles");
    let mut thin = base.clone();
    thin["thin"] = json!({"location": "side1", "thickness": 1});
    refused(&mut b.doc, thin, "no thin walls");
    // Through all and up to an object are a face's extents too
    // (mitcad#2033); a start object is not: the face is where it starts.
    let mut from_object = base.clone();
    from_object["start"] = json!({"type": "object", "object": {"type": "plane", "plane": "xy"}});
    refused(&mut b.doc, from_object, "starts at the face");
    let mut wrapped = base;
    wrapped["wrap"] = json!({"body": b.body, "face": top});
    refused(&mut b.doc, wrapped, "or wrap");
    // A body that is not there.
    let missing = face_extrusion(
        BodyUid::new(FeatureUid(9), 0),
        "F9:top",
        distance(5.0),
        "join",
    );
    assert!(b.doc.add_feature(&def(missing), None).is_err());
}

// Tapers and directions of a face's extrusion (mitcad#541).

#[test]
fn a_face_extrusion_tapers_and_goes_along_a_direction() {
    let mut b = block();
    let top = top(&b);
    // Tapered on one side, then on two sides each by its own.
    let uid = add(
        &mut b.doc,
        face_extrusion(
            b.body,
            &top,
            json!({"type": "distance", "distance": 5, "taper": 0.1}),
            "new_body",
        ),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: 5 taper 0.1 0")
    );
    let two = face_extrusion(
        b.body,
        &top,
        json!({"type": "two_sides",
               "side1": {"type": "distance", "distance": 5, "taper": 0.1},
               "side2": {"type": "distance", "distance": 2, "taper": -0.05}}),
        "new_body",
    );
    b.doc.edit_feature(uid, &def(two)).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: 5; 2 taper 0.1 -0.05")
    );
    // Along a direction of its own (a unit vector reaches the kernel),
    // reversed by flip.
    let mut along = face_extrusion(b.body, &top, distance(5.0), "new_body");
    along["direction"] = json!([0.0, 0.0, 2.0]);
    along["flip"] = json!(true);
    b.doc.edit_feature(uid, &def(along)).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} reversed from 0: 5 along [0, 0, 1]")
    );
    // A direction without length is refused.
    let mut zero = face_extrusion(b.body, &top, distance(5.0), "new_body");
    zero["direction"] = json!([0.0, 0.0, 0.0]);
    assert!(b.doc.edit_feature(uid, &def(zero)).is_err());
}

#[test]
fn a_curved_face_extrudes_along_a_direction_only() {
    let mut b = block();
    let side = "F2:side(c1[c4,c2])";
    b.doc.kernel().surfaces.borrow_mut().insert(
        side.to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    let mut along = face_extrusion(b.body, side, distance(5.0), "new_body");
    along["direction"] = json!([0.0, -1.0, 0.0]);
    let uid = add(&mut b.doc, along.clone());
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {side} key {side} from 0: 5 along [0, -1, 0]")
    );
    // With a taper it is refused by the kernel.
    along["extent"] = json!({"type": "distance", "distance": 5, "taper": 0.1});
    b.doc.edit_feature(uid, &def(along)).unwrap();
    let why = error(&b.doc, uid);
    assert!(why.contains("is not planar"), "{why}");
}

// The extents of sketch profiles (mitcad#2033).

/// A stepped block: the block 20 high with a step on its right half joined
/// 30 high from the sketch plane. The lower step's top (`top`), the higher
/// step's top (`higher`).
fn stepped() -> (Block, String, String) {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [30.0, 0.0], &num(30.0), &num(40.0))
        .unwrap()
        .region;
    let step = b
        .doc
        .add_feature(&extrude(sketch, &[&region], 30.0, "join", &[b.body]), None)
        .unwrap()
        .uid;
    assert_eq!(step, FeatureUid(4));
    let lower = top(&b);
    (b, lower, format!("{step}:end({region})"))
}

fn to_object(object: Value) -> Value {
    json!({"type": "to_object", "object": object})
}

/// The case of the ticket: the lower step's top extruded up to the higher
/// step's top (To Selected Face: its surface continued) joins into the
/// block, one level body; with an offset it goes past it; the stored
/// feature recomputes after an upstream change and after a save.
#[test]
fn a_stepped_blocks_lower_top_joins_up_to_the_higher_top() {
    let (mut b, lower, higher) = stepped();
    let face = json!({"type": "face", "body": b.body, "face": higher});
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &lower, to_object(face.clone()), "join"),
    );
    assert_eq!(uid, FeatureUid(5));
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "extrude_face F5 {lower} key {lower} from 0: to face {higher} extend true offset 0"
        )
    );
    // One body: the stepped block with the face's prism joined into it.
    assert_eq!(b.doc.bodies().len(), 1);
    let joined = history(&b.doc, b.body);
    assert!(
        joined.starts_with("join(") && joined.ends_with(&format!(",face_prism(F5:{lower}:0..10))")),
        "{joined}"
    );
    // The caps keyed by the face, as by a distance.
    let names = faces(&b.doc, b.body);
    assert!(names.contains(&format!("F5:end({lower})")), "{names:?}");

    // With an offset (positive goes past the face).
    let mut past = to_object(face.clone());
    past["offset"] = json!(2);
    b.doc
        .edit_feature(uid, &def(face_extrusion(b.body, &lower, past, "join")))
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "extrude_face F5 {lower} key {lower} from 0: to face {higher} extend true offset 2"
        )
    );

    // The block made deeper upstream: the face and its object are found
    // again.
    b.doc.set_parameter("d3", 25.0).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let recomputed = history(&b.doc, b.body);
    assert!(recomputed.contains("prism(F2:0..25)"), "{recomputed}");

    // Saved and opened again: the same feature and result.
    let saved = b.doc.to_json();
    assert!(saved.contains("\"to_object\""), "{saved}");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(status(&loaded, uid), FeatureStatus::Ok);
    assert_eq!(history(&loaded, b.body), history(&b.doc, b.body));
}

/// Up to a plane and up to a body; To Adjacent Faces; with a taper; and on
/// two sides, a side up to an object beside a distance (signed, ending on
/// the object's side too).
#[test]
fn a_face_extrudes_up_to_planes_bodies_and_on_two_sides() {
    let (mut b, lower, higher) = stepped();
    // The stepped block as the extrusion finds it.
    let before = history(&b.doc, b.body);
    let plane = json!({"type": "plane", "plane": {"origin": [0, 0, 30], "normal": [0, 0, 1]}});
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &lower, to_object(plane.clone()), "join"),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "extrude_face F5 {lower} key {lower} from 0: to plane [0.0, 0.0, 30.0] [0.0, 0.0, 1.0] \
             offset 0"
        )
    );
    let cases = [
        (
            to_object(json!({"type": "face", "body": b.body, "face": higher, "chained": true})),
            format!("to face {higher} extend false offset 0"),
        ),
        (
            to_object(json!({"type": "body", "body": b.body, "through": true})),
            format!("to body {before} through true offset 0"),
        ),
        (
            json!({"type": "to_object", "object": plane, "taper": 0.1}),
            "to plane [0.0, 0.0, 30.0] [0.0, 0.0, 1.0] offset 0 taper 0.1 0".to_owned(),
        ),
        (
            json!({"type": "two_sides", "side1": to_object(plane.clone()),
                   "side2": {"type": "distance", "distance": 5}}),
            "to plane [0.0, 0.0, 30.0] [0.0, 0.0, 1.0] offset 0; 5".to_owned(),
        ),
        (
            json!({"type": "two_sides", "signed": true, "side1": to_object(plane.clone()),
                   "side2": {"type": "distance", "distance": -3}}),
            "to plane [0.0, 0.0, 30.0] [0.0, 0.0, 1.0] offset 0; -3".to_owned(),
        ),
    ];
    for (extent, spec) in cases {
        b.doc
            .edit_feature(uid, &def(face_extrusion(b.body, &lower, extent, "join")))
            .unwrap();
        assert_eq!(status(&b.doc, uid), FeatureStatus::Ok, "{spec}");
        assert_eq!(
            last_spec(&b.doc),
            format!("extrude_face F5 {lower} key {lower} from 0: {spec}")
        );
    }
}

/// Through all: a cut of the block's top into it reaches past the block's
/// far side (its depth 20, by a quarter and 1 mm more), both ways alike;
/// a curved face along a direction from every point it may start at.
#[test]
fn a_face_cuts_through_all() {
    let mut b = block();
    let top = top(&b);
    let mut into = face_extrusion(b.body, &top, json!({"type": "through_all"}), "cut");
    into["flip"] = json!(true);
    let uid = add(&mut b.doc, into);
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} reversed from 0: 26")
    );
    assert_eq!(
        history(&b.doc, b.body),
        format!("cut(prism(F2:0..20),face_prism(F3:{top}:-0..26))")
    );
    // Both ways.
    b.doc
        .edit_feature(
            uid,
            &def(face_extrusion(
                b.body,
                &top,
                json!({"type": "through_all", "both_sides": true}),
                "cut",
            )),
        )
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {top} key {top} from 0: 26; 26")
    );
    // Out of the block nothing lies in the way.
    b.doc
        .edit_feature(
            uid,
            &def(face_extrusion(
                b.body,
                &top,
                json!({"type": "through_all"}),
                "cut",
            )),
        )
        .unwrap();
    let why = error(&b.doc, uid);
    assert!(why.contains("no bodies in the extrude direction"), "{why}");

    // A curved side face along -y: from the corners of its body's box, 40
    // across.
    let side = "F2:side(c1[c4,c2])";
    b.doc.kernel().surfaces.borrow_mut().insert(
        side.to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        },
    );
    let mut along = face_extrusion(b.body, side, json!({"type": "through_all"}), "new_body");
    along["direction"] = json!([0.0, -1.0, 0.0]);
    b.doc.edit_feature(uid, &def(along)).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("extrude_face F3 {side} key {side} from 0: 51 along [0, -1, 0]")
    );
}

/// Up to a face of another body, as a new body and joined into both.
#[test]
fn a_face_extrudes_up_to_a_face_of_another_body() {
    let mut b = block();
    let top = top(&b);
    // A plate apart from the block: 40 to 45 above the sketch plane.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [0.0, 0.0], &num(60.0), &num(40.0))
        .unwrap()
        .region;
    let plate = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "start": {"type": "offset", "offset": 40},
               "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}),
    );
    let plate_body = BodyUid::new(plate, 0);
    let underside = format!("{plate}:start({region})");
    let object = to_object(json!({"type": "face", "body": plate_body, "face": underside}));
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &top, object.clone(), "new_body"),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "extrude_face {uid} {top} key {top} from 0: to face {underside} extend true offset 0"
        )
    );
    assert_eq!(b.doc.bodies().len(), 3);
    assert_eq!(
        history(&b.doc, BodyUid::new(uid, 0)),
        format!("face_prism({uid}:{top}:0..10)")
    );
    // Joined into the block and the plate: one body.
    let mut join = face_extrusion(b.body, &top, object, "join");
    join["participants"] = json!([b.body, plate_body]);
    b.doc.edit_feature(uid, &def(join)).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(b.doc.bodies().len(), 1);
}

/// A pattern's Adjust builds each copy of a face's extrusion up to an
/// object again, up to the same plane, as for a sketch profile's; Identical
/// moves the finished tool.
#[test]
fn adjust_rebuilds_a_face_extrusion_up_to_an_object_at_each_copy() {
    let mut b = block();
    let top = top(&b);
    let plane = json!({"type": "plane", "plane": {"origin": [0, 0, 30], "normal": [0, 0, 1]}});
    let uid = add(
        &mut b.doc,
        face_extrusion(b.body, &top, to_object(plane), "new_body"),
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let specs = |doc: &Document<MockKernel>| -> Vec<String> {
        doc.kernel()
            .specs
            .borrow()
            .iter()
            .filter(|s| s.starts_with(&format!("extrude_face {uid} ")))
            .cloned()
            .collect()
    };
    assert_eq!(specs(&b.doc).len(), 1);
    let pattern = |compute: &str| {
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [uid]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 3,
                              "distance": 80},
               "distance_type": "spacing", "compute": compute})
    };
    let adjust = add(&mut b.doc, pattern("adjust"));
    assert_eq!(status(&b.doc, adjust), FeatureStatus::Ok);
    let built = specs(&b.doc);
    assert_eq!(built.len(), 3, "{built:?}");
    assert!(
        built[1..]
            .iter()
            .all(|s| s.contains("to plane [0.0, 0.0, 30.0] [0.0, 0.0, 1.0]")),
        "{built:?}"
    );
    b.doc
        .edit_feature(adjust, &def(pattern("identical")))
        .unwrap();
    assert_eq!(specs(&b.doc).len(), 3);
}
