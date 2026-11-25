// SPDX-License-Identifier: MIT
//! A loft's Perpendicular to Face end condition (mitcad#2345) in documents
//! with the mock kernel: it reaches the kernel at profiles and faces, with
//! and without a face of its own; it fits no point; its face must be on
//! its body; a surface loft of curves takes end conditions now, through the
//! kernel's loft with the chains as sections (without conditions still its
//! loft_sheet), and still refuses rails; the face survives saving and
//! loading. A loose chain of a body's edges is a section of a surface loft
//! (mitcad#2377): its edges' curves in order, named after the edges, and a
//! Perpendicular end there leaves the face the edges all bound, or the one
//! it names; edges that bound no one face are refused in words. Free edges
//! of a surface body are such a section too (mitcad#2395), picked by their
//! opening, named after the face they bound, which a Perpendicular end
//! there leaves. The
//! geometry (the surface at right angles to the face within 1e-6 rad, also
//! between rails on a curved face) is checked in
//! `geometry.loft_perpendicular`.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::test_support::{block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, TopoName};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
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

/// The specs of the lofts the kernel was asked for.
fn loft_specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|spec| spec.starts_with("loft ") || spec.starts_with("loft_sheet("))
        .cloned()
        .collect()
}

fn last_loft(doc: &Document<MockKernel>) -> String {
    loft_specs(doc).last().cloned().unwrap_or_default()
}

fn with_fields(mut value: Value, extra: Value) -> Value {
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

/// The block (F1, F2) with a circle on a sketch of its own (F3) and one on
/// the YZ plane (F4).
struct Setup {
    doc: Document<MockKernel>,
    body: BodyUid,
    top: String,
    side: String,
}

fn setup() -> Setup {
    let b = block();
    let top = FaceName::end(b.extrude, b.region.clone()).to_string();
    let side = b
        .doc
        .body_shape(b.body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .find(|name| name.contains(":side("))
        .expect("a side face");
    let mut doc = b.doc;
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 0], "diameter": 10}),
    );
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F4", "center": [0, 5], "diameter": 10}),
    );
    Setup {
        doc,
        body: b.body,
        top,
        side,
    }
}

fn solid_loft(s: &Setup, extra: Value) -> Value {
    with_fields(
        json!({"type": "loft", "sections": [
            {"type": "face", "body": s.body, "face": s.top},
            {"type": "profile", "sketch": "F3", "region": "r{c1}"}],
            "operation": "new_body"}),
        extra,
    )
}

fn curve_loft(extra: Value) -> Value {
    with_fields(
        json!({"type": "loft", "sections": [
            {"type": "curves", "sketch": "F3", "curves": ["c1"]},
            {"type": "curves", "sketch": "F4", "curves": ["c1"]}],
            "surface": true, "operation": "new_body"}),
        extra,
    )
}

#[test]
fn perpendicular_reaches_the_kernel_with_and_without_a_face() {
    let mut s = setup();
    // At a face section without a face of its own: the section's face.
    let condition = json!({"start_condition": {"type": "perpendicular", "weight": 1.5}});
    let uid = s
        .doc
        .add_feature(&def(solid_loft(&s, condition)), None)
        .unwrap()
        .uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    let spec = last_loft(&s.doc);
    assert!(
        spec.ends_with(" start Perpendicular angle 0 weight 1.5"),
        "{spec}"
    );
    // At a profile, at the end, with a face of the body named.
    let condition = json!({"end_condition": {"type": "perpendicular", "weight": 1,
        "face": {"body": s.body, "face": s.side}}});
    let uid = s
        .doc
        .add_feature(&def(solid_loft(&s, condition)), None)
        .unwrap()
        .uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    let spec = last_loft(&s.doc);
    assert!(
        spec.ends_with(&format!(
            " end Perpendicular angle 0 weight 1 face {}",
            s.side
        )),
        "{spec}"
    );
    // The definition keeps the face.
    let answer: Value = serde_json::from_str(
        &s.doc
            .query(&json!({"query": "feature", "uid": uid.to_string()}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        answer["def"]["end_condition"]["face"]["face"], s.side,
        "{answer}"
    );
}

#[test]
fn perpendicular_is_refused_in_words_where_it_does_not_fit() {
    let mut s = setup();
    // Not at a point.
    let pointed = json!({"type": "loft", "sections": [
        {"type": "profile", "sketch": "F3", "region": "r{c1}"},
        {"type": "point", "point": "origin"}],
        "end_condition": {"type": "perpendicular", "weight": 1},
        "operation": "new_body"});
    let refused = s.doc.add_feature(&def(pointed), None).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("the end condition perpendicular does not fit a point section"),
        "{refused}"
    );
    // A body that is not there.
    let missing = solid_loft(
        &s,
        json!({"start_condition": {"type": "perpendicular", "weight": 1,
            "face": {"body": "F9.b0", "face": s.side}}}),
    );
    let refused = s.doc.add_feature(&def(missing), None).unwrap_err();
    assert!(refused.to_string().contains("F9"), "{refused}");
    // A face the body does not have fails the feature, naming the face.
    let wrong = solid_loft(
        &s,
        json!({"start_condition": {"type": "perpendicular", "weight": 1,
            "face": {"body": s.body, "face": "F2:side(c99)"}}}),
    );
    let uid = s.doc.add_feature(&def(wrong), None).unwrap().uid;
    assert!(
        error(&s.doc, uid).contains("F2:side(c99)"),
        "{}",
        error(&s.doc, uid)
    );
    // A weight above 10.
    let heavy = solid_loft(
        &s,
        json!({"start_condition": {"type": "perpendicular", "weight": 11}}),
    );
    let uid = s.doc.add_feature(&def(heavy), None).unwrap().uid;
    assert!(
        error(&s.doc, uid).contains("the start condition's weight must be at most 10, got 11"),
        "{}",
        error(&s.doc, uid)
    );
    // A ruled loft has none.
    let ruled = solid_loft(
        &s,
        json!({"ruled": true, "start_condition": {"type": "perpendicular", "weight": 1}}),
    );
    let refused = s.doc.add_feature(&def(ruled), None).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("a ruled loft has no end conditions or rails"),
        "{refused}"
    );
}

#[test]
fn a_surface_loft_of_curves_takes_end_conditions_through_the_kernels_loft() {
    let mut s = setup();
    // Without conditions: the kernel's loft_sheet, as before.
    let uid = s
        .doc
        .add_feature(&def(curve_loft(json!({}))), None)
        .unwrap()
        .uid;
    assert_eq!(status(&s.doc, uid), FeatureStatus::Ok);
    assert!(
        last_loft(&s.doc).starts_with("loft_sheet("),
        "{}",
        last_loft(&s.doc)
    );
    // Perpendicular at both ends: the kernel's loft of chains, each with its
    // sketch's plane, a surface.
    let both = json!({"start_condition": {"type": "perpendicular", "weight": 1},
        "end_condition": {"type": "perpendicular", "weight": 2}});
    let uid = s.doc.add_feature(&def(curve_loft(both)), None).unwrap().uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    let spec = last_loft(&s.doc);
    assert!(spec.starts_with("loft F"), "{spec}");
    assert!(spec.contains("sections chain c1 normal "), "{spec}");
    assert!(spec.contains(" surface"), "{spec}");
    assert!(
        spec.contains(" start Perpendicular angle 0 weight 1 end Perpendicular angle 0 weight 2"),
        "{spec}"
    );
    // Its faces are the sheet's, as loft_sheet names them.
    let shape = s.doc.body_shape(BodyUid::new(uid, 0)).unwrap();
    let faces: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert_eq!(faces, [format!("{uid}:side(c1)")]);
    // A direction at a planar sketch's curves too.
    let direction =
        json!({"start_condition": {"type": "direction", "angle": "10 deg", "weight": 1}});
    let uid = s
        .doc
        .add_feature(&def(curve_loft(direction)), None)
        .unwrap()
        .uid;
    assert_eq!(status(&s.doc, uid), FeatureStatus::Ok);
    assert!(
        last_loft(&s.doc).contains(" start Direction"),
        "{}",
        last_loft(&s.doc)
    );
    // Tangent and point conditions fit no curves; rails stay refused.
    for (extra, expected) in [
        (
            json!({"start_condition": {"type": "tangent", "weight": 1}}),
            "the start condition tangent does not fit a curves section",
        ),
        (
            json!({"rails": [{"sketch": "F3", "curves": ["c1"]}]}),
            "a loft of curve sections has no center line or rails",
        ),
    ] {
        let refused = s
            .doc
            .add_feature(&def(curve_loft(extra)), None)
            .unwrap_err();
        assert!(refused.to_string().contains(expected), "{refused}");
    }
}

#[test]
fn the_face_survives_saving_and_loading() {
    let mut s = setup();
    let condition = json!({"start_condition": {"type": "perpendicular", "weight": 1,
        "face": {"body": s.body, "face": s.top}}});
    let uid = s
        .doc
        .add_feature(&def(solid_loft(&s, condition)), None)
        .unwrap()
        .uid;
    let saved = s.doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let feature = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == uid.to_string())
        .unwrap();
    assert_eq!(feature["start_condition"]["type"], "perpendicular");
    assert_eq!(feature["start_condition"]["face"]["face"], s.top);
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), saved);
    command(&mut opened, json!({"cmd": "recompute"}));
    assert_eq!(opened.status(uid), Some(&FeatureStatus::Ok));
    assert!(
        last_loft(&opened).ends_with(&format!(" face {}", s.top)),
        "{}",
        last_loft(&opened)
    );
}

/// Three of the block's top edges, each between the top and a side face,
/// with their curves in the mock kernel (a U round the top, joined end to
/// end), in order along the U.
fn top_edges(s: &Setup) -> Vec<String> {
    let shape = s.doc.body_shape(s.body).unwrap();
    let top: FaceName = s.top.parse().unwrap();
    let sides: Vec<FaceName> = shape
        .faces
        .iter()
        .filter(|face| face.to_string().contains(":side("))
        .take(3)
        .cloned()
        .collect();
    assert_eq!(sides.len(), 3, "{:?}", shape.faces);
    let corners = [
        [0.0, 0.0, 20.0],
        [60.0, 0.0, 20.0],
        [60.0, 40.0, 20.0],
        [0.0, 40.0, 20.0],
    ];
    let mut names = Vec::new();
    let mut named = s.doc.kernel().named_curves.borrow_mut();
    for (i, side) in sides.into_iter().enumerate() {
        let edge = EdgeName::new(top.clone(), side);
        named.insert(
            TopoName::Edge(edge.clone()).to_string(),
            Curve3::Line {
                start: corners[i],
                end: corners[i + 1],
            },
        );
        names.push(edge.to_string());
    }
    names
}

/// A surface loft from a chain of the block's edges to the circle of F3.
fn edge_loft(s: &Setup, edges: &[String], extra: Value) -> Value {
    with_fields(
        json!({"type": "loft", "sections": [
            {"type": "edges", "body": s.body, "edges": edges},
            {"type": "curves", "sketch": "F3", "curves": ["c1"]}],
            "surface": true, "operation": "new_body"}),
        extra,
    )
}

#[test]
fn a_chain_of_a_bodys_edges_is_a_section_of_a_surface_loft() {
    let mut s = setup();
    let edges = top_edges(&s);
    // Without conditions: the kernel's loft_sheet through the edges' curves
    // in order, named after the edges.
    let uid = s
        .doc
        .add_feature(&def(edge_loft(&s, &edges, json!({}))), None)
        .unwrap()
        .uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    let spec = last_loft(&s.doc);
    assert!(spec.starts_with("loft_sheet("), "{spec}");
    assert!(spec.contains(&edges.join(">")), "{spec}");
    // Perpendicular at the chain without a face: the face all its edges
    // bound, the block's top; the chain has no plane of its own.
    let condition = json!({"start_condition": {"type": "perpendicular", "weight": 1}});
    let uid = s
        .doc
        .add_feature(&def(edge_loft(&s, &edges, condition)), None)
        .unwrap()
        .uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    let spec = last_loft(&s.doc);
    assert!(
        spec.contains(&format!(
            "sections chain {}; chain c1 normal ",
            edges.join(">")
        )),
        "{spec}"
    );
    assert!(
        spec.contains(&format!(
            " start Perpendicular angle 0 weight 1 face {}",
            s.top
        )),
        "{spec}"
    );
    // A face named: that one.
    let named = json!({"start_condition": {"type": "perpendicular", "weight": 1,
        "face": {"body": s.body, "face": s.side}}});
    let uid = s
        .doc
        .add_feature(&def(edge_loft(&s, &edges, named)), None)
        .unwrap()
        .uid;
    assert_eq!(status(&s.doc, uid), FeatureStatus::Ok);
    assert!(
        last_loft(&s.doc).contains(&format!(" face {}", s.side)),
        "{}",
        last_loft(&s.doc)
    );
    // The definition keeps the section as it was given, through saving.
    let saved = s.doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let feature = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == uid.to_string())
        .unwrap();
    assert_eq!(feature["sections"][0]["type"], "edges");
    assert_eq!(feature["sections"][0]["edges"], json!(edges));
    let opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), saved);
}

#[test]
fn a_chain_of_edges_is_refused_in_words_where_it_does_not_fit() {
    let mut s = setup();
    let edges = top_edges(&s);
    // One edge lies between two faces: which one to leave is not known.
    let condition = json!({"start_condition": {"type": "perpendicular", "weight": 1}});
    let uid = s
        .doc
        .add_feature(&def(edge_loft(&s, &edges[..1], condition)), None)
        .unwrap()
        .uid;
    assert!(
        error(&s.doc, uid).contains(
            "the start section's edges lie between the same two faces: give the one to leave \
             at right angles as the start condition's face"
        ),
        "{}",
        error(&s.doc, uid)
    );
    // A direction needs a plane, which a chain of edges is not given.
    let direction =
        json!({"start_condition": {"type": "direction", "angle": "10 deg", "weight": 1}});
    let refused = s
        .doc
        .add_feature(&def(edge_loft(&s, &edges, direction)), None)
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("the start condition direction does not fit a chain of a body's edges"),
        "{refused}"
    );
    // A solid loft has no sections of curves or edges.
    let solid = json!({"type": "loft", "sections": [
        {"type": "edges", "body": s.body, "edges": edges},
        {"type": "profile", "sketch": "F3", "region": "r{c1}"}],
        "operation": "new_body"});
    let refused = s.doc.add_feature(&def(solid), None).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("only a surface loft has sections of curves"),
        "{refused}"
    );
    // No edges.
    let empty = edge_loft(&s, &[], json!({}));
    let refused = s.doc.add_feature(&def(empty), None).unwrap_err();
    assert!(
        refused.to_string().contains("no edges selected"),
        "{refused}"
    );
}

/// A surface body, F5.b0: F3's circle extruded 5 mm as a surface, one
/// face; the mock kernel's one opening along it (index 0) has the two
/// lines of an L as its free edges, given out of order. The body and its
/// face.
fn open_sheet(s: &mut Setup) -> (BodyUid, String) {
    let uid = s
        .doc
        .add_feature(
            &def(
                json!({"type": "extrude", "surface": true, "operation": "new_body",
                "profiles": [{"sketch": "F3", "region": "r{c1}"}],
                "extent": {"type": "distance", "distance": 5}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    let body: BodyUid = format!("{uid}.b0").parse().unwrap();
    let faces = s.doc.body_shape(body).unwrap().faces.clone();
    assert_eq!(faces.len(), 1, "{faces:?}");
    s.doc.kernel().free_edge_curves.borrow_mut().insert(
        0,
        vec![
            Curve3::Line {
                start: [10.0, 0.0, 5.0],
                end: [10.0, 10.0, 5.0],
            },
            Curve3::Line {
                start: [0.0, 0.0, 5.0],
                end: [10.0, 0.0, 5.0],
            },
        ],
    );
    (body, faces[0].to_string())
}

/// A surface loft from free edges of F5.b0 (the section's `openings`) to
/// the circle of F4.
fn free_edge_loft(body: BodyUid, openings: Value, extra: Value) -> Value {
    with_fields(
        json!({"type": "loft", "sections": [
            {"type": "edges", "body": body, "openings": openings},
            {"type": "curves", "sketch": "F4", "curves": ["c1"]}],
            "surface": true, "operation": "new_body"}),
        extra,
    )
}

#[test]
fn a_chain_of_a_surface_bodys_free_edges_is_a_section() {
    // mitcad#2395: free edges have no names; the section picks them by
    // their opening, as the surface features do, and its pieces are named
    // after the face they bound.
    let mut s = setup();
    let (body, face) = open_sheet(&mut s);
    let whole = json!([{"opening": {"faces": [face]}}]);
    let uid = s
        .doc
        .add_feature(&def(free_edge_loft(body, whole.clone(), json!({}))), None)
        .unwrap()
        .uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    // Both free edges, joined end to end into one chain whatever order the
    // kernel gave them in, each named after the face it bounds.
    let spec = last_loft(&s.doc);
    assert!(spec.starts_with("loft_sheet("), "{spec}");
    assert!(spec.contains(&format!(": {face}>{face} | c1 ")), "{spec}");
    assert!(s.doc.kernel().count("opening_curves") > 0);

    // Perpendicular without a face: the face the free edges bound.
    let condition = json!({"start_condition": {"type": "perpendicular", "weight": 1}});
    let uid = s
        .doc
        .add_feature(&def(free_edge_loft(body, whole.clone(), condition)), None)
        .unwrap()
        .uid;
    assert_eq!(
        status(&s.doc, uid),
        FeatureStatus::Ok,
        "{:?}",
        status(&s.doc, uid)
    );
    let spec = last_loft(&s.doc);
    assert!(
        spec.contains(&format!("sections chain {face}>{face}; chain c1 normal ")),
        "{spec}"
    );
    assert!(
        spec.contains(&format!(
            " start Perpendicular angle 0 weight 1 face {face}"
        )),
        "{spec}"
    );

    // A point picks the free edge nearest it: one piece.
    let single = json!([{"opening": {"faces": [face]}, "at": [[10, 5, 5]]}]);
    let uid = s
        .doc
        .add_feature(&def(free_edge_loft(body, single, json!({}))), None)
        .unwrap()
        .uid;
    assert_eq!(status(&s.doc, uid), FeatureStatus::Ok);
    let spec = last_loft(&s.doc);
    assert!(spec.contains(&format!(": {face} | c1 ")), "{spec}");

    // The definition keeps the free edges as they were given, without
    // named edges, through saving.
    let saved = s.doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let feature = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == uid.to_string())
        .unwrap();
    assert_eq!(feature["sections"][0]["type"], "edges");
    assert!(feature["sections"][0].get("edges").is_none(), "{feature}");
    assert_eq!(
        feature["sections"][0]["openings"][0]["at"][0],
        json!([10.0, 5.0, 5.0])
    );
    let opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), saved);
}

#[test]
fn free_edges_that_are_not_there_are_refused_in_words() {
    let mut s = setup();
    let (body, face) = open_sheet(&mut s);
    // Only the free edges of another face: none.
    let other = json!([{"opening": {"faces": [face]}, "faces": [s.top]}]);
    let uid = s
        .doc
        .add_feature(&def(free_edge_loft(body, other, json!({}))), None)
        .unwrap()
        .uid;
    assert!(
        error(&s.doc, uid).contains(&format!(
            "the section's openings pick no free edges of body {body}"
        )),
        "{}",
        error(&s.doc, uid)
    );
    // A solid has no openings.
    let solid = json!([{"opening": {"faces": [s.top]}}]);
    let uid = s
        .doc
        .add_feature(&def(free_edge_loft(s.body, solid, json!({}))), None)
        .unwrap()
        .uid;
    assert!(
        error(&s.doc, uid).contains("has no opening along the faces"),
        "{}",
        error(&s.doc, uid)
    );
}
