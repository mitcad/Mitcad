// SPDX-License-Identifier: MIT
//! Sweeps, lofts, pipes, coils, ribs and webs (F3) in documents with the
//! mock kernel: paths, what reaches the kernel, checks, unsupported options,
//! unitless values and references.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

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
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
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

/// The preview's `sweep_path` of a new sweep (mitcad#795).
fn sweep_path(doc: &mut Document<MockKernel>, def: Value) -> Value {
    let command = json!({"cmd": "add_feature", "def": def});
    let report: Value = serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap();
    report["sweep_path"].clone()
}

#[test]
fn sweep_previews_tell_where_the_profile_lies_on_the_path() {
    // The profile (on YZ, round the origin) at the start of c1, which runs
    // from the origin along X; at its end when flipped.
    let (mut b, _) = with_path();
    let at = |closed: bool, place: &str| json!({"closed": closed, "profile_at": place});
    assert_eq!(
        sweep_path(&mut b.doc, sweep(&["c1"], json!({}))),
        at(false, "start")
    );
    assert_eq!(
        sweep_path(&mut b.doc, sweep(&["c1"], json!({"flip": true}))),
        at(false, "end")
    );
    // With a guide rail too (every sweep type of the panel).
    assert_eq!(
        sweep_path(
            &mut b.doc,
            sweep(
                &["c1"],
                json!({"guide_rail": {"sketch": "F3", "curves": ["c7"]}})
            )
        ),
        at(false, "start")
    );
    // A path through the profile's plane between its ends: inside, where
    // Distance 2 (the part before the profile) means something.
    let path = FeatureUid(3);
    let through = line(&mut b.doc, path, [-20.0, -10.0], [20.0, -10.0]);
    assert_eq!(
        sweep_path(
            &mut b.doc,
            sweep(
                &[through.as_str()],
                json!({"extent": {"type": "partial", "fraction": 1, "fraction2": 0}})
            )
        ),
        at(false, "inside")
    );
    // A closed path: the profile is inside it.
    let circle = command(
        &mut b.doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 50], "diameter": 20}),
    )["curves"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        sweep_path(&mut b.doc, sweep(&[circle.as_str()], json!({}))),
        at(true, "inside")
    );
    // Nothing was added.
    assert_eq!(b.doc.features().count(), 4);
}

#[test]
fn an_end_scale_and_a_second_rail_reach_the_kernel() {
    // mitcad#464: the end scale is a unitless slot; two rails both reach
    // the kernel; an end scale with a rail, a second rail without a first
    // and a scale out of range are refused.
    let (mut b, _) = with_path();
    let uid = add(
        &mut b.doc,
        sweep(&["c1"], json!({"end_scale": 0.5, "twist_angle": 0.25})),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0.25 taper 0 rail none end scale 0.5"
    );
    assert!(mitcad_model::parameters::slot_unit("end_scale").is_unitless());
    let answer: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert!(!answer["def"]["end_scale"].is_null(), "{answer}");
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c1"],
                json!({"guide_rail": {"sketch": "F3", "curves": ["c4"]},
                       "guide_rail2": {"sketch": "F3", "curves": ["c7"]},
                       "profile_scaling": "stretch"}),
            )),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail c4 Stretch rail2 c7"
    );
    for (extra, expected) in [
        (
            json!({"end_scale": 2, "guide_rail": {"sketch": "F3", "curves": ["c7"]}}),
            "an end scale is not taken with a guide rail",
        ),
        (
            json!({"guide_rail2": {"sketch": "F3", "curves": ["c7"]}}),
            "a second guide rail needs a first one",
        ),
        (json!({"end_scale": 0}), "between 0.01 and 100, got 0"),
        (
            json!({"end_scale": 2, "binormal": [0, 0, 1]}),
            "not taken with an end scale",
        ),
    ] {
        match b.doc.edit_feature(uid, &def(sweep(&["c1"], extra))) {
            Err(e) => assert!(e.to_string().contains(expected), "{e}"),
            Ok(_) => panic!("accepted; expected {expected}"),
        }
    }
}

#[test]
fn the_rail_point_rule_reaches_the_kernel() {
    // mitcad#469: `rail_point` "plane" reaches the kernel; a definition
    // without it (a design saved before the option) keeps the fraction
    // rule and is saved without it; the rule needs a guide rail.
    let (mut b, _) = with_path();
    let rail = json!({"guide_rail": {"sketch": "F3", "curves": ["c4"]}});
    let uid = add(&mut b.doc, sweep(&["c1"], rail.clone())).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail c4 Scale"
    );
    let saved = |b: &Block| -> Value {
        let answer: Value = serde_json::from_str(
            &b.doc
                .query(&json!({"query": "feature", "uid": uid}).to_string())
                .unwrap(),
        )
        .unwrap();
        answer["def"].clone()
    };
    assert!(saved(&b).get("rail_point").is_none(), "{}", saved(&b));
    let mut plane = rail.clone();
    plane["rail_point"] = json!("plane");
    b.doc
        .edit_feature(uid, &def(sweep(&["c1"], plane)))
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail c4 Scale rail point plane"
    );
    assert_eq!(saved(&b)["rail_point"], "plane");
    let mut fraction = rail;
    fraction["rail_point"] = json!("fraction");
    b.doc
        .edit_feature(uid, &def(sweep(&["c1"], fraction)))
        .unwrap();
    assert!(saved(&b).get("rail_point").is_none(), "{}", saved(&b));
    match b
        .doc
        .edit_feature(uid, &def(sweep(&["c1"], json!({"rail_point": "plane"}))))
    {
        Err(e) => assert!(
            e.to_string().contains("only taken with a guide rail"),
            "{e}"
        ),
        Ok(_) => panic!("a rail point without a rail was accepted"),
    }
}

#[test]
fn a_profile_sweeps_along_a_helix_path() {
    // (mitcad#385) A fixed helix of three turns about the x axis through
    // the profile's centre's circle: the kernel gets one helix.
    let (mut b, _) = with_path();
    let helix = |pitch: f64| {
        json!({"helix": {"center": [0, -20, 0], "axis": [1, 0, 0], "x_axis": [0, 1, 0],
            "radius": 20, "pitch": pitch, "start_angle": 0, "end_angle": 3.0 * std::f64::consts::TAU}})
    };
    let uid = add(&mut b.doc, sweep(&[], json!({"path": helix(15.0)}))).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path helix extents 1 1 Perpendicular twist 0 taper 0 rail none"
    );
    // A helix that does not rise is refused.
    let refused = b
        .doc
        .edit_feature(uid, &def(sweep(&[], json!({"path": helix(0.0)}))))
        .unwrap_err();
    assert!(refused.to_string().contains("pitch"), "{refused}");
}

#[test]
fn a_sweep_with_a_fixed_binormal() {
    // (mitcad#397) The binormal reaches the kernel as a unit vector and
    // round-trips through the definition.
    let (mut b, _) = with_path();
    let uid = add(
        &mut b.doc,
        sweep(&["c1", "c4", "c7"], json!({"binormal": [0, 0, 2]})),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1,c4,c7 extents 1 1 Perpendicular twist 0 taper 0 rail none binormal 0 0 1"
    );
    let answer: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(answer["def"]["binormal"], json!([0.0, 0.0, 2.0]));
    // Not with the parallel orientation, a guide rail, a twist or a taper,
    // and not of no length.
    for (extra, expected) in [
        (
            json!({"binormal": [0, 0, 1], "orientation": "parallel"}),
            "perpendicular orientation",
        ),
        (
            json!({"binormal": [0, 0, 1], "guide_rail": {"sketch": "F3", "curves": ["c7"]}}),
            "guide rail",
        ),
        (
            json!({"binormal": [0, 0, 1], "twist_angle": 0.5}),
            "twist or a taper",
        ),
        (json!({"binormal": [0, 0, 0]}), "of some length"),
    ] {
        match b.doc.edit_feature(uid, &def(sweep(&["c1"], extra))) {
            Err(e) => assert!(e.to_string().contains(expected), "{e}"),
            Ok(_) => panic!("accepted; expected {expected}"),
        }
    }
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
    let edge = |i: usize| mitcad_model::test_support::top_edge(extrude, 1, &region, i).to_string();
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

/// Guide surfaces (mitcad#157): faces of a body reach the kernel's
/// `surface_sweep`; twist and taper do not, a rail and the parallel
/// orientation are refused.
#[test]
fn sweeps_along_guide_surfaces() {
    let (mut b, _) = with_path();
    let block = b.doc.body_shape(BodyUid::new(b.extrude, 0)).unwrap();
    let top = block.faces[1].to_string();
    let side = block.faces[2].to_string();
    let uid = add(
        &mut b.doc,
        sweep(
            &["c1"],
            json!({"guide_surfaces": [{"body": "F2.b0", "face": top},
                                      {"body": "F2.b0", "face": side}],
                   "twist_angle": 1.0}),
        ),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!(
            "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail none surfaces {top},{side}"
        )
    );
    // The sweep refers to the faces' body and its creator.
    let feature = b.doc.feature(uid).unwrap();
    assert!(feature.def.references().features.contains(&b.extrude));
    match &feature.def {
        mitcad_model::FeatureDef::Sweep(s) => {
            assert_eq!(s.guide_surfaces[1].face.to_string(), side)
        }
        other => panic!("not a sweep: {other:?}"),
    }

    for (extra, expected) in [
        (
            json!({"guide_surfaces": [{"body": "F2.b0", "face": top}],
                   "guide_rail": {"sketch": "F3", "curves": ["c7"]}}),
            "a guide rail or guide surfaces, not both",
        ),
        (
            json!({"guide_surfaces": [{"body": "F2.b0", "face": top}],
                   "orientation": "parallel"}),
            "perpendicular orientation",
        ),
        (
            json!({"guide_surfaces": [{"body": "F2.b0", "face": top},
                                      {"body": "F2.b0", "face": top}]}),
            "selected twice",
        ),
        (
            json!({"guide_surfaces": [{"body": "F9.b0", "face": top}]}),
            "guide surface",
        ),
    ] {
        match add(&mut b.doc, sweep(&["c1"], extra)) {
            Err(e) => assert!(e.to_string().contains(expected), "{e}"),
            Ok(uid) => panic!("{uid} was added; expected {expected}"),
        }
    }

    // A face the body does not have fails the feature.
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c1"],
                json!({"guide_surfaces": [{"body": "F2.b0", "face": "F2:side(c99)"}]}),
            )),
        )
        .unwrap();
    assert!(
        error(&b.doc, uid).contains("F2:side(c99)"),
        "{}",
        error(&b.doc, uid)
    );
}

#[test]
fn lofts_check_sections_conditions_and_rails() {
    let (mut b, _) = with_path();
    let top = mitcad_model::topo::FaceName::end(b.extrude, b.region.clone()).to_string();
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
            "a center line or rails, not both",
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
        (
            loft(json!([profile, face]), json!({"closed": true})),
            "a smooth closed loft needs three or more sections",
        ),
        (
            loft(
                json!([profile, face]),
                json!({"surface": true, "operation": "join"}),
            ),
            "a surface loft is a new body or a new component",
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
    // A surface loft reaches the kernel as one.
    let uid = add(
        &mut b.doc,
        loft(json!([profile, face]), json!({"surface": true})),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(last_spec(&b.doc).ends_with(" surface"));
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
    let error = serde_json::from_value::<mitcad_model::features::FeatureDef<String>>(json!({
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
    // MitCad's construction, which a command without one means and the
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
    let error = serde_json::from_value::<mitcad_model::features::FeatureDef<String>>(helix(
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
    // Flat ends (mitcad#235): angles, passed in the order start flat, start
    // transition, end transition, end flat; kept by saving and opening.
    b.doc
        .edit_feature(
            uid,
            &def(helix(
                json!({"end": {"transition_angle": "90 deg", "flat_angle": "270 deg"}}),
            )),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&b.doc).contains("left false ends 0 0 1.57")
            && last_spec(&b.doc).contains(" 4.71"),
        "{}",
        last_spec(&b.doc)
    );
    let reopened = Document::from_json(&b.doc.to_json(), MockKernel::default()).unwrap();
    assert!(feature_def(&reopened).get("end").is_some());
    assert!(feature_def(&reopened).get("start").is_none());
    let message = add(
        &mut b.doc,
        helix(json!({"start": {"transition_angle": "-10 deg", "flat_angle": 0}})),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("start transition"), "{message}");
    let message = add(
        &mut b.doc,
        helix(json!({"growth": 1, "construction": "freecad",
                     "end": {"transition_angle": 1, "flat_angle": 1}})),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("MitCad's construction"), "{message}");
}

#[test]
fn a_helix_whose_boolean_fails_is_retried_with_shifted_seams() {
    // mitcad#235: a helix's cut that fails is retried once with the seams of
    // the helix's pieces shifted by a sixth of a turn; one that works is
    // built once, as before.
    let (mut b, _) = with_path();
    let cut = |revolutions: f64| {
        json!({"type": "helix", "profiles": [{"sketch": "F4", "region": "r{c1}"}],
               "axis": "y", "pitch": 5, "revolutions": revolutions, "operation": "cut"})
    };
    let shift = format!(" shift {}", 1.0 / 6.0);
    // Fails with the first layout of pieces, works with shifted seams.
    b.doc
        .kernel()
        .boolean_fail_tools
        .borrow_mut()
        .insert("helix(F5)".to_owned());
    let uid = add(&mut b.doc, cut(3.0)).unwrap();
    assert_eq!(uid.to_string(), "F5");
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(b.doc.kernel().count("helix"), 2);
    assert_eq!(b.doc.kernel().count("cut"), 2);
    assert!(last_spec(&b.doc).ends_with(&shift), "{}", last_spec(&b.doc));
    let warnings = b.doc.warnings(uid).join("; ");
    assert!(
        warnings.contains("shifted by a sixth of a turn")
            && warnings.contains("tool Boolean failed"),
        "{warnings}"
    );
    let histories: Vec<String> = b
        .doc
        .bodies()
        .iter()
        .map(|v| v.shape.history.clone())
        .collect();
    assert!(
        histories
            .iter()
            .any(|h| h.contains(&format!("helix(F5{shift})"))),
        "{histories:?}"
    );
    // One whose boolean works is built once, without a shift or a warning.
    b.doc.kernel().boolean_fail_tools.borrow_mut().clear();
    b.doc.edit_feature(uid, &def(cut(3.5))).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(b.doc.kernel().count("helix"), 3);
    assert!(!last_spec(&b.doc).contains("shift"));
    assert!(b.doc.warnings(uid).is_empty());
    // When the shifted one fails too, the first failure stands.
    b.doc.kernel().fail.borrow_mut().insert("cut");
    b.doc.edit_feature(uid, &def(cut(4.0))).unwrap();
    assert!(
        error(&b.doc, uid).contains("cut failed"),
        "{}",
        error(&b.doc, uid)
    );
    assert_eq!(b.doc.kernel().count("helix"), 5);
    b.doc.kernel().fail.borrow_mut().clear();
    // A boolean that ran out of memory is not retried.
    b.doc.kernel().out_of_memory.borrow_mut().insert("cut");
    b.doc.edit_feature(uid, &def(cut(4.5))).unwrap();
    assert!(error(&b.doc, uid).contains("out of memory"));
    assert_eq!(b.doc.kernel().count("helix"), 6);
    b.doc.kernel().out_of_memory.borrow_mut().clear();
    // A definition with the shift (the `.ipt` import's retried coils) is
    // built with it, keeps it when saved and is not shifted again.
    let mut shifted = cut(5.0);
    shifted["piece_shift"] = json!(1.0 / 6.0);
    b.doc.edit_feature(uid, &def(shifted.clone())).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(last_spec(&b.doc).ends_with(&shift), "{}", last_spec(&b.doc));
    let file: Value = serde_json::from_str(&b.doc.to_json()).unwrap();
    let stored = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "helix")
        .unwrap()
        .clone();
    assert_eq!(stored["piece_shift"], json!(1.0 / 6.0));
    b.doc.kernel().fail.borrow_mut().insert("cut");
    shifted["revolutions"] = json!(5.5);
    b.doc.edit_feature(uid, &def(shifted.clone())).unwrap();
    assert_eq!(b.doc.kernel().count("helix"), 8);
    b.doc.kernel().fail.borrow_mut().clear();
    // Without one the field is left out; a third of a turn is too much.
    b.doc.edit_feature(uid, &def(cut(3.0))).unwrap();
    let file: Value = serde_json::from_str(&b.doc.to_json()).unwrap();
    assert!(
        file["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f.get("piece_shift").is_none())
    );
    shifted["piece_shift"] = json!(1.0 / 3.0);
    let message = b
        .doc
        .edit_feature(uid, &def(shifted))
        .map(|_| error(&b.doc, uid))
        .unwrap_or_else(|e| e.to_string());
    assert!(message.contains("piece shift"), "{message}");
}

#[test]
fn a_sweep_follows_a_frenet_or_auxiliary_frame() {
    // (mitcad#539) The frame reaches the kernel (an auxiliary curve as its
    // curves) and round-trips through the definition; it is refused with
    // the parallel orientation, a fixed binormal, a guide rail, a twist, a
    // taper or an end scale.
    let (mut b, _) = with_path();
    let uid = add(
        &mut b.doc,
        sweep(&["c1", "c4", "c7"], json!({"frame": {"type": "frenet"}})),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1,c4,c7 extents 1 1 Perpendicular twist 0 taper 0 rail none frame frenet"
    );
    let auxiliary = json!({"frame": {"type": "auxiliary",
        "curve": {"sketch": "F3", "curves": ["c7"]}, "curvilinear": true}});
    b.doc
        .edit_feature(uid, &def(sweep(&["c1"], auxiliary.clone())))
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail none frame auxiliary c7 curvilinear"
    );
    let answer: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(answer["def"]["frame"]["type"], "auxiliary");
    assert_eq!(answer["def"]["frame"]["curvilinear"], true);
    assert_eq!(answer["def"]["frame"]["curve"]["curves"], json!(["c7"]));
    // The plane rule by default: `curvilinear` left out.
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c1"],
                json!({"frame": {"type": "auxiliary", "curve": {"sketch": "F3", "curves": ["c7"]}}}),
            )),
        )
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail none frame auxiliary c7"
    );
    // A constant binormal reaches the kernel as a unit vector.
    b.doc
        .edit_feature(
            uid,
            &def(sweep(
                &["c1"],
                json!({"frame": {"type": "constant_binormal", "binormal": [0, 0, 2]}}),
            )),
        )
        .unwrap();
    assert_eq!(
        last_spec(&b.doc),
        "sweep F5 path c1 extents 1 1 Perpendicular twist 0 taper 0 rail none frame constant binormal 0 0 1"
    );
    match b.doc.edit_feature(
        uid,
        &def(sweep(
            &["c1"],
            json!({"frame": {"type": "constant_binormal", "binormal": [0, 0, 0]}}),
        )),
    ) {
        Err(e) => assert!(e.to_string().contains("of some length"), "{e}"),
        Ok(_) => panic!("a constant binormal of no length was accepted"),
    }
    let frenet = json!({"type": "frenet"});
    for (extra, expected) in [
        (
            json!({"frame": frenet, "orientation": "parallel"}),
            "perpendicular orientation",
        ),
        (
            json!({"frame": frenet, "binormal": [0, 0, 1]}),
            "a fixed binormal",
        ),
        (
            json!({"frame": frenet, "guide_rail": {"sketch": "F3", "curves": ["c7"]}}),
            "a guide rail",
        ),
        (
            json!({"frame": frenet, "twist_angle": 0.5}),
            "a twist, a taper or an end scale",
        ),
        (
            json!({"frame": frenet, "end_scale": 2}),
            "a twist, a taper or an end scale",
        ),
        (
            json!({"frame": {"type": "auxiliary", "curve": {"sketch": "F3", "curves": ["c99"]}}}),
            "auxiliary curve",
        ),
    ] {
        match b.doc.edit_feature(uid, &def(sweep(&["c1"], extra))) {
            Err(e) => assert!(e.to_string().contains(expected), "{e}"),
            Ok(_) => panic!("accepted; expected {expected}"),
        }
    }
}

/// The block's top face (Extrude1's end cap).
const BLOCK_TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

#[test]
fn a_body_face_sweeps_along_a_path() {
    // (mitcad#543) The face reaches the kernel as found and as stored, the
    // sweep's caps keyed by it; the definition keeps it; sketch profiles,
    // rails and twists are refused beside it.
    let (mut b, _) = with_path();
    let face_sweep = |extra: Value| {
        let mut value = json!({"type": "sweep",
            "profile_face": {"body": "F2.b0", "face": BLOCK_TOP},
            "path": {"sketch": "F3", "curves": ["c1"]}, "operation": "new_body"});
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    };
    let uid = add(&mut b.doc, face_sweep(json!({}))).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&b.doc),
        format!("face_sweep F5 {BLOCK_TOP} key {BLOCK_TOP} path c1 Perpendicular")
    );
    let shape = b.doc.body_shape(BodyUid::new(uid, 0)).unwrap();
    let faces: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert!(
        faces.contains(&format!("F5:start({BLOCK_TOP})")),
        "{faces:?}"
    );
    let answer: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(answer["def"]["profile_face"]["face"], BLOCK_TOP);
    assert!(answer["def"].get("profiles").is_none());
    // A frame goes along.
    b.doc
        .edit_feature(uid, &def(face_sweep(json!({"frame": {"type": "frenet"}}))))
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    for (extra, expected) in [
        (
            json!({"profiles": [{"sketch": "F4", "region": "r{c1}"}]}),
            "takes no sketch profiles",
        ),
        (
            json!({"guide_rail": {"sketch": "F3", "curves": ["c7"]}}),
            "guide rails or guide surfaces",
        ),
        (
            json!({"twist_angle": 0.5}),
            "a twist, a taper or an end scale",
        ),
    ] {
        match b.doc.edit_feature(uid, &def(face_sweep(extra))) {
            Err(e) => assert!(e.to_string().contains(expected), "{e}"),
            Ok(_) => panic!("accepted; expected {expected}"),
        }
    }
}

#[test]
fn a_loft_along_its_centre_line_follows_a_frame() {
    // (mitcad#543) A loft's frame reaches the kernel; without a centre line
    // it is refused.
    let (mut b, _) = with_path();
    let loft = |extra: Value| {
        let mut value = json!({"type": "loft", "sections": [
                {"type": "profile", "sketch": "F4", "region": "r{c1}"},
                {"type": "face", "body": "F2.b0", "face": BLOCK_TOP}],
            "centerline": {"sketch": "F3", "curves": ["c1"]}, "operation": "new_body"});
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    };
    let uid = add(
        &mut b.doc,
        loft(json!({"frame": {"type": "constant_binormal", "binormal": [0, 0, 3]}})),
    )
    .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&b.doc).ends_with("frame constant binormal 0 0 1"),
        "{}",
        last_spec(&b.doc)
    );
    let mut plain = loft(json!({"frame": {"type": "frenet"}}));
    plain.as_object_mut().unwrap().remove("centerline");
    match b.doc.edit_feature(uid, &def(plain)) {
        Err(e) => assert!(e.to_string().contains("needs one"), "{e}"),
        Ok(_) => panic!("a frame without a center line was accepted"),
    }
}

#[test]
fn a_parallel_loft_moves_its_sections_along_the_centre_line() {
    // (mitcad#547) The parallel orientation reaches the kernel; without a
    // centre line, or with a frame, it is refused.
    let (mut b, _) = with_path();
    let loft = |extra: Value| {
        let mut value = json!({"type": "loft", "sections": [
                {"type": "profile", "sketch": "F4", "region": "r{c1}"},
                {"type": "face", "body": "F2.b0", "face": BLOCK_TOP}],
            "centerline": {"sketch": "F3", "curves": ["c1"]}, "operation": "new_body",
            "orientation": "parallel"});
        for (key, item) in extra.as_object().unwrap() {
            value[key] = item.clone();
        }
        value
    };
    let uid = add(&mut b.doc, loft(json!({}))).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&b.doc).ends_with(" parallel"),
        "{}",
        last_spec(&b.doc)
    );
    let mut plain = loft(json!({}));
    plain.as_object_mut().unwrap().remove("centerline");
    match b.doc.edit_feature(uid, &def(plain)) {
        Err(e) => assert!(e.to_string().contains("needs one"), "{e}"),
        Ok(_) => panic!("a parallel loft without a center line was accepted"),
    }
    match b
        .doc
        .edit_feature(uid, &def(loft(json!({"frame": {"type": "frenet"}}))))
    {
        Err(e) => assert!(e.to_string().contains("takes no frame"), "{e}"),
        Ok(_) => panic!("a parallel loft with a frame was accepted"),
    }
}

#[test]
fn a_sweep_follows_a_path_of_fixed_curves() {
    // (mitcad#548) A line along x, then a quadratic spline rising out of
    // the plane: the kernel gets both, named by their places; a spline
    // short of knots is refused.
    let (mut b, _) = with_path();
    let spline = |knots: Value| {
        json!({"fixed_curves": [
            {"type": "line", "start": [0, 0, 0], "end": [20, 0, 0]},
            {"type": "spline", "degree": 2,
             "poles": [[20, 0, 0], [30, 0, 0], [30, 10, 5]], "knots": knots}]})
    };
    let mut value = sweep(&["c1"], json!({}));
    value["path"] = spline(json!([0, 0, 0, 1, 1, 1]));
    let uid = add(&mut b.doc, value.clone()).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&b.doc).starts_with("sweep F5 path fixed1,fixed2 "),
        "{}",
        last_spec(&b.doc)
    );
    value["path"] = spline(json!([0, 0, 1, 1]));
    match b.doc.edit_feature(uid, &def(value)) {
        Err(e) => assert!(e.to_string().contains("knots"), "{e}"),
        Ok(_) => panic!("a spline short of knots was accepted"),
    }
}

#[test]
fn a_surface_sweep_takes_fixed_curves_in_no_plane() {
    // (mitcad#548) Three lines that lie in no plane, at the start of the
    // path: one chain of a surface sweep; not with sketch profiles, nor
    // in a solid sweep.
    let (mut b, _) = with_path();
    let mut value = json!({"type": "sweep", "surface": true,
        "fixed_curves": [
            {"type": "line", "start": [0, 0, 0], "end": [0, 10, 0]},
            {"type": "line", "start": [0, 10, 0], "end": [0, 10, 10]},
            {"type": "line", "start": [0, 10, 10], "end": [5, 10, 10]}],
        "path": {"sketch": "F3", "curves": ["c1"]}, "operation": "new_body"});
    let uid = add(&mut b.doc, value.clone()).unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(spec.starts_with("sweep_sheet(F5: "), "{spec}");
    assert!(
        spec.contains("fixed1") && spec.contains("fixed2") && spec.contains("fixed3"),
        "{spec}"
    );
    assert!(spec.contains(" path c1 "), "{spec}");
    value["profiles"] = json!([{"sketch": "F4", "region": "r{c1}"}]);
    match b.doc.edit_feature(uid, &def(value.clone())) {
        Err(e) => assert!(e.to_string().contains("alone"), "{e}"),
        Ok(_) => panic!("fixed curves with profiles were accepted"),
    }
    value.as_object_mut().unwrap().remove("profiles");
    value["surface"] = json!(false);
    assert!(b.doc.edit_feature(uid, &def(value)).is_err());
}

#[test]
fn a_path_with_a_curve_twice_takes_it_once() {
    // The bend a second time over itself (as an edge projected again,
    // mitcad#741): chained, it was a branch at both its ends.
    let (mut b, _) = with_path();
    let path = FeatureUid(3);
    let twice = line(&mut b.doc, path, [40.0, 0.0], [40.0, 30.0]);
    assert_ne!(twice, "c4");
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
}
