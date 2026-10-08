// SPDX-License-Identifier: MIT
//! Sweeps, lofts, pipes, coils, ribs and webs (F3) in documents with the
//! mock kernel: paths, what reaches the kernel, checks, unsupported options,
//! unitless values and references.

use serde_json::{Value, json};

use crate::document_tests::{Block, block, def};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Curve3;
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus, ModelError};

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

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

/// A line of a sketch; its curve id.
fn line(doc: &mut Document<MockKernel>, sketch: FeatureUid, a: [f64; 2], b: [f64; 2]) -> String {
    let result = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch, "start": a, "end": b}),
    );
    result["made"][0].as_str().unwrap().to_owned()
}

/// The block (Sketch1 F1, Extrude1 F2, Body1 F2.b0), a path sketch (F3) of
/// three joined lines c1, c4 and c7 listed out of order (the bend first),
/// and a profile sketch on YZ (F4) with a circle: region `r{c1}`.
fn with_path() -> (Block, Vec<String>) {
    let mut b = block();
    let path = b
        .doc
        .add_sketch(crate::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let first = line(&mut b.doc, path, [0.0, 0.0], [40.0, 0.0]);
    let bend = line(&mut b.doc, path, [40.0, 30.0], [40.0, 0.0]);
    let last = line(&mut b.doc, path, [40.0, 30.0], [70.0, 30.0]);
    command(&mut b.doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    command(
        &mut b.doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F4", "center": [0, 0], "diameter": 10}),
    );
    (b, vec![first, bend, last])
}

fn sweep(curves: &[&str], extra: Value) -> Value {
    let mut value = json!({"type": "sweep", "profiles": [{"sketch": "F4", "region": "r{c1}"}],
        "path": {"sketch": "F3", "curves": curves}, "operation": "new_body"});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

#[test]
fn sweeps_follow_their_paths_to_the_kernel() {
    let (mut b, c) = with_path();
    assert_eq!(c, ["c1", "c4", "c7"]);
    // The curves in any order, joined from the first one's free end.
    let uid = add(&mut b.doc, sweep(&["c1", "c7", "c4"], json!({}))).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1,c4,c7 extents 1 1 Perpendicular twist 0 taper 0 rail none"
    );
    let shape = b.doc.body_shape(BodyUid::new(uid, 0)).unwrap();
    let faces: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert_eq!(faces, ["F5:start(r{c1})", "F5:end(r{c1})", "F5:side(c1)"]);

    // A chained selection from the last curve runs from its free end back.
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c7"],
                json!({"path": {"sketch": "F3", "curves": ["c7"], "chain": true},
                       "orientation": "parallel", "flip": true,
                       "extent": {"type": "partial", "fraction": 0.5}}),
            )),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1,c4,c7 extents 0.5 1 Parallel twist 0 taper 0 rail none"
    );

    // Twist and taper; a guide rail overrides them.
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c1", "c4", "c7"],
                json!({"twist_angle": "90 deg", "taper_angle": -0.1}),
            )),
        )
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1,c4,c7 extents 1 1 Perpendicular twist 1.5707963267948966 taper -0.1 rail none"
    );
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c1"],
                json!({"twist_angle": 1.0, "guide_rail": {"sketch": "F3", "curves": ["c7"]},
                       "profile_scaling": "stretch"}),
            )),
        )
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail c7 Stretch"
    );
}

#[test]
fn sweep_paths_and_values_are_checked() {
    let (mut b, _) = with_path();
    for (value, expected) in [
        (
            sweep(&["c1", "c1"], json!({})),
            "path: curve c1 is listed more than once",
        ),
        (sweep(&["c2"], json!({})), "path: Sketch2 has no curve c2"),
        (sweep(&[], json!({})), "path: no curves selected"),
        (
            sweep(
                &["c1"],
                json!({"extent": {"type": "partial", "fraction": 1.5}}),
            ),
            "between 0 and 1, got 1.5",
        ),
        (
            sweep(&["c1"], json!({"taper_angle": 2.0})),
            "less than 90 degrees",
        ),
    ] {
        match add(&mut b.doc, value) {
            Err(e) => assert!(e.to_string().contains(expected), "{e}"),
            Ok(uid) => panic!("{uid} was added; expected {expected}"),
        }
    }

    // A gap or a branch fails the feature.
    let path = FeatureUid(3);
    let far = line(&mut b.doc, path, [100.0, 0.0], [120.0, 0.0]);
    let uid = add(&mut b.doc, sweep(&["c1", &far], json!({}))).unwrap();
    assert!(
        error(&b.doc, uid).contains("path: the path's curves are not connected: c10 does not join"),
        "{}",
        error(&b.doc, uid)
    );
    let branch = line(&mut b.doc, path, [40.0, 0.0], [40.0, -30.0]);
    b.doc
        .edit_feature(uid, &def(sweep(&["c1", "c4", &branch], json!({}))))
        .unwrap();
    assert!(error(&b.doc, uid).contains("branches at (40.000000, 0.000000"));

    // A fraction is a unitless parameter.
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c1"],
                json!({"extent": {"type": "partial", "fraction": 0.25, "fraction2": 0.5}}),
            )),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let fraction = b
        .doc
        .parameters()
        .iter()
        .find(|p| p.comment() == "Sweep1 fraction")
        .map(|p| p.expression().to_owned());
    assert_eq!(fraction.as_deref(), Some("0.25"));
}

#[test]
fn sweep_paths_of_edges_come_from_the_kernel() {
    let (mut b, _) = with_path();
    let line3 = |start: [f64; 3], end: [f64; 3]| Curve3::Line { start, end };
    // Two edges at the block's top; the kernel gives their curves.
    let (extrude, region) = (b.extrude, b.region.clone());
    let edge = |i: usize| crate::document_tests::top_edge(extrude, 1, &region, i).to_string();
    b.doc.kernel().named_curves.borrow_mut().extend([
        (edge(0), line3([0.0, 0.0, 20.0], [60.0, 0.0, 20.0])),
        (edge(1), line3([60.0, 0.0, 20.0], [60.0, 40.0, 20.0])),
    ]);
    let uid = add(
        &mut b.doc,
        sweep(
            &[],
            json!({"path": {"body": "F2.b0", "edges": [edge(1), edge(0)]}}),
        ),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // From the first edge's free end, (60, 40).
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "sweep F5 path {},{} extents 1 1 Perpendicular twist 0 taper 0 rail none",
            edge(1),
            edge(0)
        )
    );
    // The sweep refers to the body's creator.
    assert!(
        b.doc
            .feature(uid)
            .unwrap()
            .def
            .references()
            .features
            .contains(&b.extrude)
    );
}

#[test]
fn lofts_check_sections_conditions_and_rails() {
    let (mut b, _) = with_path();
    let top = crate::topo::FaceName::end(b.extrude, b.region.clone()).to_string();
    let loft = |sections: Value, extra: Value| {
        let mut value = json!({"type": "loft", "sections": sections, "operation": "new_body"});
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    };
    let profile = json!({"type": "profile", "sketch": "F4", "region": "r{c1}"});
    let face = json!({"type": "face", "body": "F2.b0", "face": top});
    let uid = add(
        &mut b.doc,
        loft(
            json!([profile, face, {"type": "point", "point": "origin"}]),
            json!({"ruled": true, "end_condition": {"type": "point_sharp"}}),
        ),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "loft F5 sections region r{{c1}}; face {top}; point [0.0, 0.0, 0.0] ruled true \
             closed false centerline none"
        )
    );
    let shape = b.doc.body_shape(BodyUid::new(uid, 0)).unwrap();
    let faces: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert_eq!(faces, ["F5:side(c1)", "F5:start(r{c1})"]);

    for (value, expected) in [
        (loft(json!([profile]), json!({})), "two or more sections"),
        (
            loft(
                json!([profile, {"type": "point", "point": "origin"}, face]),
                json!({}),
            ),
            "section 2: a point can only be the first or the last section",
        ),
        (
            loft(json!([profile, profile]), json!({})),
            "section 2: it is listed more than once",
        ),
        (
            loft(
                json!([profile, face]),
                json!({"end_condition": {"type": "point_sharp"}}),
            ),
            "the end condition point_sharp does not fit a curve section",
        ),
        (
            loft(
                json!([profile, face]),
                json!({"centerline": {"sketch": "F3", "curves": ["c1"]},
                       "rails": [{"sketch": "F3", "curves": ["c7"]}]}),
            ),
            "a centre line or rails, not both",
        ),
        (
            loft(
                json!([profile, {"type": "point", "point": {"sketch": "F4", "point": "p9"}}]),
                json!({}),
            ),
            "section 2: Sketch3 has no point p9",
        ),
        // Directions at sketch profiles, tangent and smooth ends at faces.
        (
            loft(
                json!([profile, face]),
                json!({"start_condition": {"type": "tangent", "weight": 1}}),
            ),
            "the start condition tangent does not fit a profile section",
        ),
        (
            loft(
                json!([face, profile]),
                json!({"start_condition": {"type": "direction", "angle": 0, "weight": 1}}),
            ),
            "the start condition direction does not fit a face section",
        ),
        (
            loft(
                json!([profile, face]),
                json!({"ruled": true, "rails": [{"sketch": "F3", "curves": ["c7"]}]}),
            ),
            "a ruled loft has no end conditions or rails",
        ),
        (
            loft(
                json!([profile, face]),
                json!({"closed": true, "end_condition": {"type": "smooth", "weight": 1}}),
            ),
            "a closed loft has no end conditions",
        ),
    ] {
        match add(&mut b.doc, value) {
            Err(e) => assert!(e.to_string().contains(expected), "{e}"),
            Ok(uid) => panic!("{uid} was added; expected {expected}"),
        }
    }

    // Rails and end conditions reach the kernel.
    for (sections, extra, expected) in [
        (
            json!([profile, face]),
            json!({"rails": [{"sketch": "F3", "curves": ["c7"]}]}),
            " rails c7",
        ),
        (
            json!([face, profile]),
            json!({"start_condition": {"type": "tangent", "weight": 1}}),
            " start Tangent angle 0 weight 1",
        ),
        (
            json!([profile, face]),
            json!({"start_condition": {"type": "direction", "angle": "30 deg", "weight": 2},
                   "end_condition": {"type": "smooth", "weight": 0.5}}),
            " start Direction angle 0.5235987755982988 weight 2 end Smooth angle 0 weight 0.5",
        ),
    ] {
        let uid = add(&mut b.doc, loft(sections, extra)).unwrap();
        assert_eq!(
            status(&b.doc, uid),
            FeatureStatus::Ok,
            "{}",
            error(&b.doc, uid)
        );
        assert!(
            last_spec(&b.doc).ends_with(expected),
            "{}",
            last_spec(&b.doc)
        );
    }
    // A weight is unitless.
    let weight = b
        .doc
        .parameters()
        .iter()
        .find(|p| p.comment() == "Loft3 weight")
        .map(|p| p.expression().to_owned());
    assert_eq!(weight.as_deref(), Some("1"));
    // Weights from above 0 to 10, angles between -90 and 90 degrees.
    for (condition, expected) in [
        (
            json!({"type": "direction", "angle": 0, "weight": 0}),
            "must be greater than zero",
        ),
        (
            json!({"type": "direction", "angle": 0, "weight": 11}),
            "the start condition's weight must be at most 10, got 11",
        ),
        (
            json!({"type": "direction", "angle": "90 deg", "weight": 1}),
            "the start condition's angle must lie between -90 and 90 degrees",
        ),
    ] {
        let uid = add(
            &mut b.doc,
            loft(
                json!([profile, face]),
                json!({ "start_condition": condition }),
            ),
        )
        .unwrap();
        assert!(
            error(&b.doc, uid).contains(expected),
            "{}",
            error(&b.doc, uid)
        );
    }

    // A centre line reaches the kernel; a region with holes does not.
    let uid = add(
        &mut b.doc,
        loft(
            json!([profile, face]),
            json!({"centerline": {"sketch": "F3", "curves": ["c1"]}}),
        ),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(last_spec(&b.doc).ends_with("centerline c1"));
}

#[test]
fn pipes_and_coils_reach_the_kernel() {
    let (mut b, _) = with_path();
    let uid = add(
        &mut b.doc,
        json!({"type": "pipe", "path": {"sketch": "F3", "curves": ["c1"], "chain": true},
               "section": "square", "size": 8, "thickness": 1, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "pipe F5 path c1,c4,c7 extents 1 0 Square size 8 thickness Some(1.0)"
    );
    // A wall that fills the section; a second fraction on an open path.
    let error_of = |value: Value, b: &mut Block| add(&mut b.doc, value).unwrap_err().to_string();
    let message = error_of(
        json!({"type": "pipe", "path": {"sketch": "F3", "curves": ["c1"]}, "section": "triangular",
               "size": 8, "thickness": 2, "operation": "new_body"}),
        &mut b,
    );
    assert!(
        message.contains("fills the section; it must be less than 2"),
        "{message}"
    );
    let uid = add(
        &mut b.doc,
        json!({"type": "pipe", "path": {"sketch": "F3", "curves": ["c1"]}, "size": 8,
               "extent": {"type": "partial", "fraction": 0.5, "fraction2": 0.2},
               "operation": "new_body"}),
    )
    .unwrap();
    assert!(error(&b.doc, uid).contains("only a pipe along a closed path has a second fraction"));

    // Coils: the pitch from the height; the centre in the plane's frame.
    let uid = add(
        &mut b.doc,
        json!({"type": "coil", "plane": "xz", "center": [10, 5], "diameter": 40,
               "helix": {"type": "revolutions_and_height", "revolutions": 4, "height": 30},
               "section": "triangular_internal", "section_position": "outside",
               "section_size": 4, "clockwise": true, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "coil F7 at [10.0, 0.0, -5.0] axis [-0.0, 1.0, 0.0] diameter 40 revolutions 4 pitch 7.5 \
         angle 0 spiral false clockwise true TriangularInternal Outside size 4"
    );
    let revolutions = b
        .doc
        .parameters()
        .iter()
        .find(|p| p.comment() == "Coil1 revolutions")
        .map(|p| p.expression().to_owned());
    assert_eq!(revolutions.as_deref(), Some("4"));
    let message = error_of(
        json!({"type": "coil", "plane": "xy", "diameter": 40, "angle": 0.1,
               "helix": {"type": "spiral", "revolutions": 2, "pitch": 5},
               "section_size": 2, "operation": "new_body"}),
        &mut b,
    );
    assert!(message.contains("a spiral coil has no angle"), "{message}");
    let message = error_of(
        json!({"type": "coil", "plane": "x", "diameter": 40,
               "helix": {"type": "height_and_pitch", "height": 20, "pitch": 5},
               "section_size": 2, "operation": "new_body"}),
        &mut b,
    );
    assert!(
        message.contains("is not a plane or a planar face"),
        "{message}"
    );
}

#[test]
fn ribs_and_webs_join_the_bodies() {
    let (mut b, _) = with_path();
    let uid = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": "F3", "curves": ["c4", "c1"]},
               "thickness": 4, "extent": {"type": "to_next"}, "flip": true}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "rib F5 chains c4,c1 thickness 4 Symmetric depth None flip true bodies 1"
    );
    // Joined to the block (the mock joins every touched target).
    assert!(
        b.doc
            .body_shape(b.body)
            .unwrap()
            .history
            .starts_with("join(")
    );
    let uid = add(
        &mut b.doc,
        json!({"type": "web", "curves": {"sketch": "F3", "curves": ["c1", "c7"]},
               "thickness": 3, "thickness_location": "side2",
               "extent": {"type": "depth", "depth": 10}, "participants": ["F2.b0"]}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "web F6 chains c1; c7 thickness 3 Side2 depth Some(10.0) flip false bodies 1"
    );
    // A rib needs one open chain.
    let uid = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": "F3", "curves": ["c1", "c7"]},
               "thickness": 4, "extent": {"type": "to_next"}}),
    )
    .unwrap();
    assert!(error(&b.doc, uid).contains("c7 does not join the others"));
    let message = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": "F3", "curves": ["c1"]},
               "thickness": 0, "extent": {"type": "to_next"}}),
    )
    .unwrap_err()
    .to_string();
    assert!(
        message.contains("the rib's thickness must be greater than zero"),
        "{message}"
    );
}

#[test]
fn definitions_round_trip_through_json() {
    let (mut b, _) = with_path();
    let uid = add(
        &mut b.doc,
        sweep(
            &["c1"],
            json!({"twist_angle": 0.5, "extent": {"type": "partial", "fraction": 0.5}}),
        ),
    )
    .unwrap();
    let answer: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    let def = &answer["def"];
    assert_eq!(def["type"], "sweep");
    assert_eq!(def["path"], json!({"sketch": "F3", "curves": ["c1"]}));
    assert_eq!(def["extent"]["type"], "partial");
    assert!(def.get("orientation").is_none());
    // A path is curves of a sketch or edges of a body.
    let error = serde_json::from_value::<crate::features::FeatureDef<String>>(json!({
        "type": "sweep", "profiles": [], "path": {"edges": []}, "operation": "new_body"}))
    .unwrap_err();
    assert!(error.to_string().contains("needs \"body\""), "{error}");
}

#[test]
fn helices_turn_their_profiles_about_an_axis() {
    // FreeCAD's helices (mitcad#4): the profile on YZ (F4) turned about
    // the Y axis, 3 turns of 5; against the axis left-handed.
    let (mut b, _) = with_path();
    let helix = |extra: Value| {
        let mut value = json!({"type": "helix", "profiles": [{"sketch": "F4", "region": "r{c1}"}],
            "axis": "y", "pitch": 5, "revolutions": 3, "operation": "new_body"});
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    };
    let uid = add(&mut b.doc, helix(json!({}))).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "helix F5 axis [0.0, 0.0, 0.0] [0.0, 1.0, 0.0] pitch 5 revolutions 3 left false"
    );
    let shape = b.doc.body_shape(BodyUid::new(uid, 0)).unwrap();
    let faces: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert_eq!(faces, ["F5:start(r{c1})", "F5:end(r{c1})", "F5:side(c1)"]);
    b.doc
        .edit_feature(uid, &def(helix(json!({"flip": true, "left_handed": true}))))
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        "helix F5 axis [0.0, 0.0, 0.0] [-0.0, -1.0, -0.0] pitch 5 revolutions 3 left true"
    );
    let feature_def = |doc: &Document<MockKernel>| -> Value {
        let answer: Value = serde_json::from_str(
            &doc.query(&json!({"query": "feature", "uid": uid}).to_string())
                .unwrap(),
        )
        .unwrap();
        answer["def"].clone()
    };
    assert_eq!(feature_def(&b.doc)["flip"], true);
    // A growth widens it (mitcad#59): passed with the direction's flip, by
    // Mitcad's construction, which a command without one means and the
    // stored definition then names (mitcad#83).
    b.doc
        .edit_feature(uid, &def(helix(json!({"flip": true, "growth": 1.5}))))
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        "helix F5 axis [0.0, 0.0, 0.0] [-0.0, -1.0, -0.0] pitch 5 revolutions 3 left false \
         growth 1.5 flip true"
    );
    assert!(feature_def(&b.doc).get("growth").is_some());
    assert_eq!(feature_def(&b.doc)["construction"], "mitcad");
    // FreeCAD's construction (the FreeCAD import's) is kept by an edit
    // without one, and by saving and opening.
    b.doc
        .edit_feature(
            uid,
            &def(helix(json!({"growth": 1.5, "construction": "freecad"}))),
        )
        .unwrap();
    assert!(last_spec(&b.doc).ends_with("growth 1.5 flip false freecad"));
    b.doc
        .edit_feature(uid, &def(helix(json!({"growth": 2}))))
        .unwrap();
    assert!(last_spec(&b.doc).ends_with("growth 2 flip false freecad"));
    assert_eq!(feature_def(&b.doc)["construction"], "freecad");
    let reopened = Document::from_json(&b.doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(feature_def(&reopened)["construction"], "freecad");
    b.doc
        .edit_feature(
            uid,
            &def(helix(json!({"growth": 2, "construction": "mitcad"}))),
        )
        .unwrap();
    assert!(last_spec(&b.doc).ends_with("growth 2 flip false"));
    let reopened = Document::from_json(&b.doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(feature_def(&reopened)["construction"], "mitcad");
    // A growing helix of a file written before the construction existed
    // came from the FreeCAD import: it keeps FreeCAD's construction.
    let mut file: Value = serde_json::from_str(&b.doc.to_json()).unwrap();
    for feature in file["features"].as_array_mut().unwrap() {
        if feature["type"] == "helix" {
            feature.as_object_mut().unwrap().remove("construction");
        }
    }
    let older = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    assert_eq!(feature_def(&older)["construction"], "freecad");
    let error = serde_json::from_value::<crate::features::FeatureDef<String>>(helix(
        json!({"growth": 1, "construction": "other"}),
    ))
    .unwrap_err();
    assert!(error.to_string().contains("unknown variant"), "{error}");
    // Defaults are left out of the definition; values are checked.
    b.doc.edit_feature(uid, &def(helix(json!({})))).unwrap();
    assert!(feature_def(&b.doc).get("flip").is_none());
    assert!(feature_def(&b.doc).get("growth").is_none());
    assert!(feature_def(&b.doc).get("construction").is_none());
    let message = add(&mut b.doc, helix(json!({"pitch": 0})))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("the helix's pitch must be greater than zero"),
        "{message}"
    );
}
