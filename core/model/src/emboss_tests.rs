// SPDX-License-Identifier: MIT
//! Emboss (mitcad#458) with the mock kernel: what reaches the kernel for a
//! planar face, the sketch plane and a cylindrical face, raised and sunk;
//! the participants; the faces of the result; refusals; the definition's
//! form.

use serde_json::json;

use crate::features::{EmbossDef, EmbossMode, FeatureDef, SketchPlane};
use crate::ids::{BodyUid, FeatureUid};
use crate::profile::SketchFrame;
use crate::test_support::{block, def, extrude, num};
use crate::testing::MockKernel;
use crate::topo::RegionKey;
use crate::{Document, FeatureStatus};

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message,
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

fn emboss(
    sketch: FeatureUid,
    region: &RegionKey,
    face: Option<(BodyUid, &str)>,
    depth: f64,
    mode: &str,
) -> serde_json::Value {
    let mut value = json!({"type": "emboss", "profiles": [{"sketch": sketch, "region": region}],
        "depth": depth, "mode": mode});
    if let Some((body, face)) = face {
        value["face"] = json!({"body": body, "face": face});
    }
    value
}

/// A sketch with a rectangle; its uid and region.
fn rectangle(
    doc: &mut Document<MockKernel>,
    plane: SketchPlane,
    corner: [f64; 2],
    size: [f64; 2],
) -> (FeatureUid, RegionKey) {
    let sketch = doc.add_sketch(plane).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, corner, &num(size[0]), &num(size[1]))
        .unwrap()
        .region;
    (sketch, region)
}

#[test]
fn a_planar_face_raises_out_of_the_material_and_sinks_into_it() {
    let mut b = block();
    let top = format!("F2:end({})", b.region);
    // Sketch2 (F3) on XY, 20 below the top face it is embossed on.
    let (sketch, region) = rectangle(&mut b.doc, SketchPlane::Xy, [10.0, 10.0], [10.0, 5.0]);
    let uid = b
        .doc
        .add_feature(
            &def(emboss(sketch, &region, Some((b.body, &top)), 2.0, "raise")),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // From the face's plane (z = 20), along the sketch normal: out.
    assert_eq!(
        last_spec(&b.doc),
        "extrude F4 along [0.0, 0.0, 1.0] from offset 20: distance 2 taper 0"
    );
    assert_eq!(
        history(&b.doc, b.body),
        "join(prism(F2:0..20),prism(F4:20..22))"
    );
    let named = faces(&b.doc, b.body);
    assert!(named.contains(&format!("F4:end({region})")), "{named:?}");

    // Sunk 3 into the block: down from the top face, cut.
    b.doc
        .edit_feature(
            uid,
            &def(emboss(sketch, &region, Some((b.body, &top)), 3.0, "sink")),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "extrude F4 along [-0.0, -0.0, -1.0] from offset 20: distance 3 taper 0"
    );
    assert_eq!(
        history(&b.doc, b.body),
        "cut(prism(F2:0..20),prism(F4:-20..-17))"
    );

    // On the bottom face (normal down) the same sketch raises downwards.
    let bottom = format!("F2:start({})", b.region);
    b.doc
        .edit_feature(
            uid,
            &def(emboss(
                sketch,
                &region,
                Some((b.body, &bottom)),
                1.0,
                "raise",
            )),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "extrude F4 along [-0.0, -0.0, -1.0] from offset 0: distance 1 taper 0"
    );
}

#[test]
fn without_a_face_the_sketch_plane_is_the_start() {
    let mut b = block();
    let (sketch, region) = rectangle(&mut b.doc, SketchPlane::Xy, [10.0, 10.0], [10.0, 5.0]);
    let uid = b
        .doc
        .add_feature(&def(emboss(sketch, &region, None, 4.0, "sink")), None)
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "extrude F4 along [-0.0, -0.0, -1.0] from offset 0: distance 4 taper 0"
    );
    assert!(history(&b.doc, b.body).starts_with("cut(prism(F2:0..20),"));
    // The default mode is left out of the definition.
    let FeatureDef::Emboss(made) = &b.doc.feature(uid).unwrap().def else {
        panic!("an emboss");
    };
    assert_eq!(made.mode, EmbossMode::Sink);
    let text = serde_json::to_value(FeatureDef::<String>::Emboss(EmbossDef {
        profiles: made.profiles.clone(),
        face: None,
        depth: "d9".to_owned(),
        mode: EmbossMode::Raise,
        participants: Vec::new(),
        projection: false,
        direction: None,
        along_axis: None,
        around_angle: None,
        rotation_angle: None,
    }))
    .unwrap();
    assert_eq!(
        text,
        json!({"type": "emboss", "profiles": [{"sketch": "F3", "region": region}], "depth": "d9"})
    );
}

/// A rod: a circle of diameter 20 about the origin (Sketch1 F1) extruded
/// 30 up (F2, Body1 F2.b0, its wall F2:side(c1)).
fn rod() -> (Document<MockKernel>, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [0.0, 0.0], &num(20.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 30.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (doc, BodyUid::new(uid, 0))
}

#[test]
fn a_cylindrical_face_takes_the_profiles_wrapped() {
    let (mut doc, rod) = rod();
    // Sketch2 (F3) on XZ, parallel to the rod's axis.
    let (sketch, region) = rectangle(&mut doc, SketchPlane::Xz, [-5.0, 10.0], [10.0, 6.0]);
    let uid = doc
        .add_feature(
            &def(emboss(
                sketch,
                &region,
                Some((rod, "F2:side(c1)")),
                1.5,
                "raise",
            )),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "wrap F4 about 0 0 0 along 0 0 1 radius 10 to 11.5"
    );
    assert_eq!(
        history(&doc, rod),
        "join(prism(F2:0..30),wrap(F4:10..11.5))"
    );
    let named = faces(&doc, rod);
    assert!(named.contains(&format!("F4:end({region})")), "{named:?}");

    // Sunk: radially in.
    doc.edit_feature(
        uid,
        &def(emboss(
            sketch,
            &region,
            Some((rod, "F2:side(c1)")),
            2.0,
            "sink",
        )),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "wrap F4 about 0 0 0 along 0 0 1 radius 10 to 8"
    );
    assert_eq!(history(&doc, rod), "cut(prism(F2:0..30),wrap(F4:10..8))");

    // As deep as the radius reaches the axis.
    doc.edit_feature(
        uid,
        &def(emboss(
            sketch,
            &region,
            Some((rod, "F2:side(c1)")),
            10.0,
            "sink",
        )),
    )
    .unwrap();
    assert!(error(&doc, uid).contains("reaches the axis of face F2:side(c1)"));
}

#[test]
fn a_wrap_needs_a_sketch_along_the_axis() {
    let (mut doc, rod) = rod();
    // On XY the sketch stands across the axis.
    let (sketch, region) = rectangle(&mut doc, SketchPlane::Xy, [-5.0, -5.0], [3.0, 3.0]);
    let uid = doc
        .add_feature(
            &def(emboss(
                sketch,
                &region,
                Some((rod, "F2:side(c1)")),
                1.0,
                "raise",
            )),
            None,
        )
        .unwrap()
        .uid;
    assert!(
        error(&doc, uid).contains("the sketch plane must be parallel to the cylinder's axis"),
        "{}",
        error(&doc, uid)
    );
}

/// The highest point of a body's bounds (the mock's wrapped tool is the
/// profiles' prism in the sketch frame the offsets moved).
fn top(doc: &Document<MockKernel>, body: BodyUid) -> f64 {
    doc.body_shape(body)
        .and_then(|s| s.bounds)
        .map_or(f64::NAN, |b| b.max[2])
}

/// Wrap offsets (mitcad#1259): Along Axis, Around and Rotation move a
/// wrapped rectangle from where the sketch places it; zero offsets are the
/// sketch's wrap; the values take expressions of parameters and are saved;
/// off a cylinder or cone they are refused. (Where exactly the offsets put
/// the profiles: `features::emboss::tests`; the volumes and places on the
/// real kernel: `cli.emboss_wrap_offsets`.)
#[test]
fn a_wrap_moves_along_the_axis_around_it_and_turns_on_the_face() {
    let (mut doc, rod) = rod();
    doc.add_parameter("shift", 15.0, "").unwrap();
    // Sketch2 (F3) on XZ (sketch y along -Z), through the rod's axis: a 4 x
    // 8 rectangle from x = -2 to 2 and z = 2 to 10, its middle at z = 6.
    let (sketch, region) = rectangle(&mut doc, SketchPlane::Xz, [-2.0, -10.0], [4.0, 8.0]);
    let wrap = |offsets: serde_json::Value| {
        let mut value = emboss(sketch, &region, Some((rod, "F2:side(c1)")), 1.0, "raise");
        for (key, slot) in offsets.as_object().unwrap() {
            value[key] = slot.clone();
        }
        def(value)
    };
    let uid = doc.add_feature(&wrap(json!({})), None).unwrap().uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let plain = (last_spec(&doc), history(&doc, rod), top(&doc, rod));
    assert_eq!(plain.2, 30.0, "the rod's top: the tool lies within it");

    // Zero offsets: today's wrap.
    doc.edit_feature(
        uid,
        &wrap(json!({"along_axis": 0, "around_angle": 0, "rotation_angle": 0})),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!((last_spec(&doc), history(&doc, rod), top(&doc, rod)), plain);

    // Along the axis by twice `shift` (30 mm): from z = 32 to 40, over the
    // rod's top; the cylinder and the radii are the same.
    doc.edit_feature(uid, &wrap(json!({"along_axis": "shift * 2"})))
        .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(last_spec(&doc), plain.0);
    assert!((top(&doc, rod) - 40.0).abs() < 1e-9, "{}", top(&doc, rod));

    // And turned a quarter turn on the face about its middle (z = 36): 8
    // across and 4 along, from z = 34 to 38; turned about the axis too.
    doc.edit_feature(
        uid,
        &wrap(json!({"along_axis": "shift * 2", "around_angle": "90 deg",
                     "rotation_angle": "90 deg"})),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!((top(&doc, rod) - 38.0).abs() < 1e-9, "{}", top(&doc, rod));
    let FeatureDef::Emboss(made) = &doc.feature(uid).unwrap().def else {
        panic!("an emboss");
    };
    assert!(
        made.along_axis.is_some() && made.around_angle.is_some() && made.rotation_angle.is_some()
    );

    // The parameter drives it: shift 20, 40 mm along, z = 44 to 48.
    doc.set_parameter("shift", 20.0).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!((top(&doc, rod) - 48.0).abs() < 1e-9, "{}", top(&doc, rod));

    // Saved and opened again: the offsets are kept.
    let saved = doc.to_json();
    for key in ["\"along_axis\"", "\"around_angle\"", "\"rotation_angle\""] {
        assert!(saved.contains(key), "{key} not in {saved}");
    }
    // An opened document is recomputed when asked (as the app does).
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(status(&opened, uid), FeatureStatus::Ok);
    assert!(
        (top(&opened, rod) - 48.0).abs() < 1e-9,
        "{}",
        top(&opened, rod)
    );

    // On the rod's planar end they move nothing and are refused.
    let mut flat = emboss(sketch, &region, Some((rod, "F2:end(r{c1})")), 1.0, "raise");
    flat["along_axis"] = json!(5);
    doc.edit_feature(uid, &def(flat)).unwrap();
    assert!(
        error(&doc, uid).contains("Along Axis, Around and Rotation move only profiles wrapped"),
        "{}",
        error(&doc, uid)
    );
}

#[test]
fn the_faces_body_is_the_participant_unless_bodies_are_listed() {
    let mut b = block();
    // A second body beside the block (F4).
    let (other, other_region) = rectangle(&mut b.doc, SketchPlane::Xy, [100.0, 0.0], [10.0, 10.0]);
    let second = b
        .doc
        .add_feature(
            &extrude(other, &[&other_region], 5.0, "new_body", &[]),
            None,
        )
        .unwrap()
        .uid;
    let second = BodyUid::new(second, 0);
    let top = format!("F2:end({})", b.region);
    let (sketch, region) = rectangle(&mut b.doc, SketchPlane::Xy, [10.0, 10.0], [10.0, 5.0]);
    let uid = b
        .doc
        .add_feature(
            &def(emboss(sketch, &region, Some((b.body, &top)), 2.0, "sink")),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(history(&b.doc, b.body).starts_with("cut("));
    assert_eq!(history(&b.doc, second), "prism(F4:0..5)");

    // Listed, the second body is cut too.
    let mut listed = emboss(sketch, &region, Some((b.body, &top)), 2.0, "sink");
    listed["participants"] = json!([b.body, second]);
    b.doc.edit_feature(uid, &def(listed)).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(history(&b.doc, second).starts_with("cut(prism(F4:0..5),"));
}

#[test]
fn refusals() {
    let mut b = block();
    let (sketch, region) = rectangle(&mut b.doc, SketchPlane::Xy, [10.0, 10.0], [10.0, 5.0]);
    // A depth of zero.
    let zero = b
        .doc
        .add_feature(&def(emboss(sketch, &region, None, 0.0, "raise")), None)
        .unwrap_err()
        .to_string();
    assert!(
        zero.contains("the emboss depth must be greater than zero"),
        "{zero}"
    );
    // A projection without a face to project onto (mitcad#1091).
    let mut unplaced = emboss(sketch, &region, None, 1.0, "raise");
    unplaced["projection"] = json!(true);
    let unplaced = b
        .doc
        .add_feature(&def(unplaced), None)
        .unwrap_err()
        .to_string();
    assert!(
        unplaced.contains("needs the face to project the profiles onto"),
        "{unplaced}"
    );
    // A projection direction in the sketch plane (XY: the X axis).
    let top = format!("F2:end({})", b.region);
    let mut sideways = emboss(sketch, &region, Some((b.body, &top)), 1.0, "raise");
    sideways["direction"] = json!("x");
    let uid = b.doc.add_feature(&def(sideways), None).unwrap().uid;
    assert!(
        error(&b.doc, uid).contains("the projection direction lies in the sketch plane"),
        "{}",
        error(&b.doc, uid)
    );
    // A face of a feature after it.
    let later = b
        .doc
        .add_feature(
            &def(emboss(
                sketch,
                &region,
                Some((b.body, "F9:end(r{c1})")),
                1.0,
                "raise",
            )),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(later.contains("F9"), "{later}");
    // An unknown mode.
    let mut bad = emboss(sketch, &region, None, 1.0, "raise");
    bad["mode"] = json!("lift");
    assert!(
        serde_json::from_value::<FeatureDef<crate::ValueInput>>(bad)
            .unwrap_err()
            .to_string()
            .contains("unknown variant `lift`")
    );
}

/// An extrusion with `wrap` (mitcad#473): the profiles wrapped onto the
/// face as an emboss wraps them, out of the material by the distance, into
/// it flipped; the operation is the extrusion's.
#[test]
fn an_extrusion_wraps_its_profiles_onto_a_cylinder() {
    let (mut doc, rod) = rod();
    let (sketch, region) = rectangle(&mut doc, SketchPlane::Xz, [-5.0, 10.0], [10.0, 6.0]);
    let wrapped = |distance: f64, operation: &str, flip: bool| {
        let profiles = json!([{"sketch": sketch, "region": region}]);
        def(json!({"type": "extrude", "profiles": profiles,
                   "extent": {"type": "distance", "distance": distance},
                   "operation": operation, "flip": flip,
                   "wrap": {"body": rod, "face": "F2:side(c1)"}}))
    };
    let uid = doc
        .add_feature(&wrapped(1.5, "join", false), None)
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "wrap F4 about 0 0 0 along 0 0 1 radius 10 to 11.5"
    );
    assert_eq!(
        history(&doc, rod),
        "join(prism(F2:0..30),wrap(F4:10..11.5))"
    );
    // Engraved: flipped, cut.
    doc.edit_feature(uid, &wrapped(2.0, "cut", true)).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(history(&doc, rod), "cut(prism(F2:0..30),wrap(F4:10..8))");
    // A negative distance goes in too.
    doc.edit_feature(uid, &wrapped(-2.0, "cut", false)).unwrap();
    assert_eq!(history(&doc, rod), "cut(prism(F2:0..30),wrap(F4:10..8))");
    // The definition keeps the face; a taper is refused.
    let saved = doc.to_json();
    assert!(saved.contains("\"wrap\""), "{saved}");
    let mut tapered = json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
        "extent": {"type": "distance", "distance": 1, "taper": 0.1}, "operation": "join",
        "wrap": {"body": rod, "face": "F2:side(c1)"}});
    let why = doc
        .add_feature(&def(tapered.clone()), None)
        .unwrap_err()
        .to_string();
    assert!(why.contains("without a taper"), "{why}");
    // A face that is neither a cylinder nor a cone.
    tapered["extent"] = json!({"type": "distance", "distance": 1});
    tapered["wrap"] = json!({"body": rod, "face": "F2:end(r{c1})"});
    let uid = doc.add_feature(&def(tapered), None).unwrap().uid;
    assert!(
        error(&doc, uid).contains("must be cylindrical or conical"),
        "{}",
        error(&doc, uid)
    );
}

/// A ball: Sphere1 (F1, Body1 F1.b0, its face F1:side0), 40 across about
/// the origin.
fn ball() -> (Document<MockKernel>, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let uid = doc
        .add_feature(
            &def(
                json!({"type": "sphere", "plane": "xy", "center": [0, 0], "diameter": 40,
                "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    (doc, BodyUid::new(uid, 0))
}

/// Projection (mitcad#1091): a face that is neither planar, cylindrical
/// nor conical takes the profiles projected along the sketch normal,
/// raised (a positive depth) or sunk (a negative one); the definition
/// says nothing of it, and the kernel's refusals name the face.
#[test]
fn any_other_face_takes_the_profiles_projected() {
    let (mut doc, ball) = ball();
    // Sketch2 (F2) on XY through the centre: a 10 x 10 square.
    let (sketch, region) = rectangle(&mut doc, SketchPlane::Xy, [-5.0, -5.0], [10.0, 10.0]);
    let on_ball = Some((ball, "F1:side0"));
    let uid = doc
        .add_feature(&def(emboss(sketch, &region, on_ball, 1.0, "raise")), None)
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(last_spec(&doc), "project F3 onto F1:side0 along 0 0 1 by 1");
    let raised = history(&doc, ball);
    assert!(
        raised.starts_with("join(") && raised.ends_with(",project(F3:F1:side0 by 1))"),
        "{raised}"
    );
    let named = faces(&doc, ball);
    assert!(named.contains(&format!("F3:end({region})")), "{named:?}");
    let FeatureDef::Emboss(made) = &doc.feature(uid).unwrap().def else {
        panic!("an emboss");
    };
    assert!(!made.projection && made.direction.is_none());
    let saved = doc.to_json();
    assert!(
        !saved.contains("\"projection\"") && !saved.contains("\"direction\""),
        "{saved}"
    );

    // Sunk 2: into the material.
    doc.edit_feature(uid, &def(emboss(sketch, &region, on_ball, 2.0, "sink")))
        .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "project F3 onto F1:side0 along 0 0 1 by -2"
    );
    assert!(
        history(&doc, ball).starts_with("cut("),
        "{}",
        history(&doc, ball)
    );

    // The kernel's refusal (a profile partly off the face) fails the
    // feature, the face named. A depth not computed before: the recompute
    // would hand out the cached result of raised 1 without the kernel.
    doc.kernel().fail.borrow_mut().insert("projected_emboss");
    doc.edit_feature(uid, &def(emboss(sketch, &region, on_ball, 1.5, "raise")))
        .unwrap();
    assert!(
        error(&doc, uid).contains("to project onto face F1:side0: projected_emboss failed"),
        "{}",
        error(&doc, uid)
    );
}

/// A cylindrical face takes the projection when the definition asks for
/// it, or gives a direction; a planar face when it gives a direction (a
/// face's normal here).
#[test]
fn a_cylinder_or_a_plane_takes_the_projection_when_asked() {
    let (mut doc, rod) = rod();
    // Sketch2 (F3) on XZ (normal +Y), beside the rod.
    let (sketch, region) = rectangle(&mut doc, SketchPlane::Xz, [-5.0, 10.0], [10.0, 6.0]);
    let mut projected = emboss(sketch, &region, Some((rod, "F2:side(c1)")), 1.5, "raise");
    projected["projection"] = json!(true);
    let uid = doc.add_feature(&def(projected.clone()), None).unwrap().uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "project F4 onto F2:side(c1) along 0 1 0 by 1.5"
    );
    assert_eq!(
        history(&doc, rod),
        "join(prism(F2:0..30),project(F4:F2:side(c1) by 1.5))"
    );
    let FeatureDef::Emboss(made) = &doc.feature(uid).unwrap().def else {
        panic!("an emboss");
    };
    assert!(made.projection);

    // A fixed direction (made a unit vector) implies the projection.
    projected.as_object_mut().unwrap().remove("projection");
    projected["direction"] = json!({"origin": [0, 0, 0], "direction": [0, 2, 1]});
    doc.edit_feature(uid, &def(projected)).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "project F4 onto F2:side(c1) along 0 0.894427191 0.447213595 by 1.5"
    );

    // The block's top face, along the normal of that face itself.
    let mut b = block();
    let top = format!("F2:end({})", b.region);
    let (sketch, region) = rectangle(&mut b.doc, SketchPlane::Xy, [10.0, 10.0], [10.0, 5.0]);
    let mut along_face = emboss(sketch, &region, Some((b.body, &top)), 2.0, "raise");
    along_face["direction"] = json!({"body": b.body, "face": top});
    let uid = b.doc.add_feature(&def(along_face), None).unwrap().uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("project F4 onto {top} along 0 0 1 by 2")
    );
    // Without a direction the top face is extruded from, as before.
    b.doc
        .edit_feature(
            uid,
            &def(emboss(sketch, &region, Some((b.body, &top)), 2.0, "raise")),
        )
        .unwrap();
    assert!(
        last_spec(&b.doc).starts_with("extrude F4"),
        "{}",
        last_spec(&b.doc)
    );
}

fn near_frame(a: &SketchFrame, b: &SketchFrame) -> bool {
    let near = |u: [f64; 3], v: [f64; 3]| (0..3).all(|i| (u[i] - v[i]).abs() <= 1e-9);
    near(a.origin, b.origin) && near(a.x_axis, b.x_axis) && near(a.y_axis, b.y_axis)
}

/// Reorient Design keeps a wrap's offsets where the turn takes them
/// (mitcad#1315): Along Axis and Around count along the face's axis made to
/// point towards +Z, else +Y, else +X, so a turn that takes the counted
/// direction to the other sense negates them. A rod along +Z turned so that
/// it lies along -Y counts along +Y; every quarter turn, for axes of every
/// sense and an oblique one, gives the wrapped frame turned.
#[test]
fn a_turn_keeps_a_wraps_offsets_where_it_takes_them() {
    use crate::features::emboss::WrapOffsets;
    use crate::kernel::Axis;
    use crate::reorient::{Turn, TurnAxis, wrap_count_reversed};

    let frame = SketchFrame {
        origin: [0.0, -10.0, 6.0],
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 0.0, -1.0],
    };
    let offsets = WrapOffsets {
        along: 10.0,
        around: 0.3,
        rotation: 0.0,
    };
    // The frame `offsets` place about `axis`, turned, against the frame the
    // turned offsets place about the turned axis.
    let kept = |turn: Turn, direction: [f64; 3]| {
        let axis = Axis {
            origin: [1.0, 2.0, 3.0],
            direction,
        };
        let (was, _) = offsets.placed(frame, &[], &axis);
        let sign = if wrap_count_reversed(turn, direction) {
            -1.0
        } else {
            1.0
        };
        let turned = WrapOffsets {
            along: sign * offsets.along,
            around: sign * offsets.around,
            rotation: 0.0,
        };
        let axis = Axis {
            origin: turn.vector(axis.origin),
            direction: turn.vector(direction),
        };
        let (now, _) = turned.placed(turn.frame(&frame), &[], &axis);
        near_frame(&now, &turn.frame(&was))
    };
    // The rod along +Z turned a quarter about X: along -Y, counted +Y.
    let turn = Turn::about(TurnAxis::X, 1);
    assert_eq!(turn.vector([0.0, 0.0, 1.0]), [0.0, -1.0, 0.0]);
    assert!(wrap_count_reversed(turn, [0.0, 0.0, 1.0]));
    assert!(kept(turn, [0.0, 0.0, 1.0]));
    // Its own axis pointing down (against the extrusion) counts the same.
    assert!(wrap_count_reversed(turn, [0.0, 0.0, -1.0]));
    for axis in [TurnAxis::X, TurnAxis::Y, TurnAxis::Z] {
        for quarters in 1..4 {
            let turn = Turn::about(axis, quarters);
            for direction in [
                [1.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, -1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
                [0.0, 0.6, -0.8],
            ] {
                assert!(kept(turn, direction), "{turn:?} {direction:?}");
            }
        }
    }
}

/// Reorient Design negates an Emboss's Along Axis and Around when the turn
/// reverses the direction they count in (mitcad#1315): the rod along +Z
/// turned to lie along -Y.
#[test]
fn reorient_design_negates_a_wraps_offsets_it_reverses() {
    use crate::reorient::{Turn, TurnAxis};

    let (mut doc, rod) = rod();
    // The rod's wall has its axis along the extrusion, which the turn
    // takes to -Y with the sketch (the mock's is +Z otherwise).
    doc.kernel().extrusion_axes.set(true);
    let (sketch, region) = rectangle(&mut doc, SketchPlane::Xz, [-2.0, -10.0], [4.0, 8.0]);
    let mut value = emboss(sketch, &region, Some((rod, "F2:side(c1)")), 1.0, "raise");
    value["along_axis"] = json!(10);
    value["around_angle"] = json!(0.3);
    let uid = doc.add_feature(&def(value), None).unwrap().uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let FeatureDef::Emboss(made) = &doc.feature(uid).unwrap().def else {
        panic!("an emboss");
    };
    let (along, around) = (made.along_axis.unwrap(), made.around_angle.unwrap());
    doc.reorient_design(Turn::about(TurnAxis::X, 1)).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let value = |id| doc.parameters().get(id).unwrap().value();
    assert!((value(along) + 10.0).abs() < 1e-9, "{}", value(along));
    assert!((value(around) + 0.3).abs() < 1e-9, "{}", value(around));
}
