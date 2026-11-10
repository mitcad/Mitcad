// SPDX-License-Identifier: MIT
//! Patch, Stitch and Thicken on the mock kernel (mitcad#456): the bodies
//! they make, change and leave, the names of their faces, what the kernel
//! is asked for and what they refuse. The geometry is checked with OCCT by
//! `geometry/tests/test_surfaces.cpp` (`geometry.surfaces`) and the
//! `.solidcraft` test design `patch-stitch-thicken` (`cli.import_solidcraft`).

use serde_json::{Value, json};

use crate::exchange::BodyKind;
use crate::features::SketchPlane;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::test_support::{Block, block, def, num, top_edge};
use crate::testing::MockKernel;
use crate::topo::RegionKey;
use crate::{Document, FeatureStatus};

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    match doc.add_feature(&def(value.clone()), None) {
        Ok(added) => panic!("{value} was added as {}", added.uid),
        Err(e) => e.to_string(),
    }
}

fn failure(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match doc.status(uid).cloned().expect("recomputed") {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("{uid} did not fail: {other:?}"),
    }
}

fn ok(doc: &Document<MockKernel>, uid: FeatureUid) {
    let status = doc.status(uid).cloned().expect("recomputed");
    assert!(status.is_ok(), "{uid}: {status:?}");
}

fn history(doc: &Document<MockKernel>, body: BodyUid) -> String {
    doc.body_shape(body).expect("the body").history.clone()
}

fn kind(doc: &Document<MockKernel>, body: BodyUid) -> BodyKind {
    let shape = doc.body_shape(body).expect("the body");
    doc.kernel().body_kind(shape).unwrap()
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .expect("the body")
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn body(text: &str) -> BodyUid {
    text.parse().unwrap()
}

fn specs(doc: &Document<MockKernel>, prefix: &str) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(prefix))
        .cloned()
        .collect()
}

/// Sketch1 (F1) with two rectangles, 20 x 10 at the origin and 10 x 10 at
/// (40, 0).
fn two_rectangles() -> (Document<MockKernel>, FeatureUid, RegionKey, RegionKey) {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let a = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let b = doc
        .add_rectangle(sketch, [40.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    (doc, sketch, a, b)
}

fn patch_of(sketch: FeatureUid, regions: &[&RegionKey]) -> Value {
    let profiles: Vec<Value> = regions
        .iter()
        .map(|r| json!({"sketch": sketch, "region": r}))
        .collect();
    json!({"type": "patch", "profiles": profiles})
}

#[test]
fn a_patch_of_regions_makes_a_surface_body_of_each() {
    let (mut doc, sketch, a, b) = two_rectangles();
    let patch = add(&mut doc, patch_of(sketch, &[&a, &b]));
    ok(&doc, patch);
    assert_eq!(patch.to_string(), "F2");
    for (k, region) in [&a, &b].into_iter().enumerate() {
        let uid = body(&format!("F2.b{k}"));
        assert_eq!(kind(&doc, uid), BodyKind::Sheet);
        assert_eq!(faces(&doc, uid), [format!("F2:patch({region})")]);
    }
    assert_eq!(doc.bodies().len(), 2);
    assert_eq!(
        specs(&doc, "patch_regions"),
        [
            format!("patch_regions(F2:{a})"),
            format!("patch_regions(F2:{b})")
        ]
    );
    // The patch lies in the sketch's plane.
    let bounds = doc.body_shape(body("F2.b0")).unwrap().bounds.unwrap();
    assert_eq!((bounds.min, bounds.max), ([0.0; 3], [20.0, 10.0, 0.0]));
}

#[test]
fn a_patch_of_a_loop_of_named_edges_leaves_the_body_as_it_is() {
    let Block {
        mut doc,
        extrude,
        region,
        body: block_body,
        ..
    } = block();
    let before = history(&doc, block_body);
    let edges: Vec<String> = (0..4)
        .map(|i| top_edge(extrude, 1, &region, i).to_string())
        .collect();
    let patch = add(
        &mut doc,
        json!({"type": "patch", "boundary": {"body": block_body, "edges": edges,
               "continuity": "g1", "edge_continuity": [{"edge": edges[2], "continuity": "g0"}],
               "points": [[30, 20, 25]]}}),
    );
    ok(&doc, patch);
    let lid = body("F3.b0");
    assert_eq!(kind(&doc, lid), BodyKind::Sheet);
    assert_eq!(faces(&doc, lid), ["F3:patch"]);
    assert_eq!(history(&doc, block_body), before);
    assert_eq!(
        specs(&doc, "patch_loop"),
        [format!(
            "patch_loop(F3: edges {}, g1 {}=g0, 1 points)",
            edges.join(" "),
            edges[2]
        )]
    );
    // The definition is written back as it was given (values by their
    // parameters' names, as design files hold them).
    let entry = doc.features().find(|f| f.uid == patch).unwrap();
    let named = entry
        .def
        .map_params(&mut |_, id: &crate::ParamId| Ok::<_, ()>(doc.parameters().name(*id)))
        .unwrap();
    let written = serde_json::to_value(&named).unwrap();
    assert_eq!(written["boundary"]["continuity"], "g1");
    assert_eq!(written["boundary"]["edges"].as_array().unwrap().len(), 4);
    assert!(written.get("profiles").is_none(), "{written}");
}

#[test]
fn a_patch_fills_an_opening_found_by_its_faces() {
    let (mut doc, sketch, a, _) = two_rectangles();
    add(&mut doc, patch_of(sketch, &[&a]));
    let face = format!("F2:patch({a})");
    let patch = add(
        &mut doc,
        json!({"type": "patch", "boundary": {"body": "F2.b0", "opening": {"faces": [face]}}}),
    );
    ok(&doc, patch);
    assert_eq!(
        specs(&doc, "patch_loop"),
        ["patch_loop(F3: opening 0, g0, 0 points)"]
    );
    // A second opening along the same faces does not exist.
    let second = add(
        &mut doc,
        json!({"type": "patch", "boundary": {"body": "F2.b0",
               "opening": {"faces": [face], "index": 1}}}),
    );
    let message = failure(&doc, second);
    assert!(
        message.contains("has 1 openings along the faces") && message.ends_with("not 2"),
        "{message}"
    );
    // Faces no opening runs along.
    let other = add(
        &mut doc,
        json!({"type": "patch", "boundary": {"body": "F2.b0",
               "opening": {"faces": ["F2:patch(r{c9})"]}}}),
    );
    assert!(
        failure(&doc, other).contains("has no opening along the faces F2:patch(r{c9})"),
        "{}",
        failure(&doc, other)
    );
}

#[test]
fn a_patch_refuses_what_it_cannot_fill() {
    let Block {
        mut doc,
        extrude,
        region,
        body: block_body,
        ..
    } = block();
    let edge = top_edge(extrude, 1, &region, 0).to_string();
    for (value, expected) in [
        (
            json!({"type": "patch"}),
            "a patch needs sketch regions or a loop of edges",
        ),
        (
            json!({"type": "patch", "boundary": {"body": block_body}}),
            "no edges or opening to fill selected",
        ),
        (
            json!({"type": "patch", "boundary": {"body": block_body, "edges": [edge],
                   "opening": {"faces": ["F2:end(r{c1})"]}}}),
            "a patch fills its edges or an opening, not both",
        ),
        (
            json!({"type": "patch", "boundary": {"body": block_body, "edges": [edge],
                   "edge_continuity": [{"edge": top_edge(extrude, 1, &region, 1).to_string(),
                                        "continuity": "g2"}]}}),
            "has a continuity of its own but is not an edge of the loop",
        ),
    ] {
        let message = refused(&mut doc, value.clone());
        assert!(message.contains(expected), "{value}: {message}");
    }
    // A solid's edge that is not there fails in the kernel.
    let missing = add(
        &mut doc,
        json!({"type": "patch", "boundary": {"body": block_body, "edges": ["E{F2:end(r{c1})|F2:start(r{c1})}"]}}),
    );
    assert!(doc.status(missing).is_some_and(|s| !s.is_ok()));
}

#[test]
fn stitch_sews_bodies_into_the_first() {
    let (mut doc, sketch, a, b) = two_rectangles();
    add(&mut doc, patch_of(sketch, &[&a, &b]));
    let stitch = add(
        &mut doc,
        json!({"type": "stitch", "bodies": ["F2.b0", "F2.b1"], "tolerance": 0.01}),
    );
    ok(&doc, stitch);
    let uids: Vec<BodyUid> = doc.bodies().iter().map(|b| b.uid).collect();
    assert_eq!(uids, [body("F2.b0")]);
    assert_eq!(kind(&doc, body("F2.b0")), BodyKind::Sheet);
    assert_eq!(
        faces(&doc, body("F2.b0")),
        [format!("F2:patch({a})"), format!("F2:patch({b})")]
    );
    assert_eq!(specs(&doc, "stitch"), ["stitch(2 bodies, 0.01)"]);
    // Faces that close up make a solid.
    doc.kernel().stitch_closes.set(true);
    let again = add(
        &mut doc,
        json!({"type": "stitch", "bodies": ["F2.b0"], "tolerance": 0}),
    );
    ok(&doc, again);
    assert_eq!(kind(&doc, body("F2.b0")), BodyKind::Solid);
    for (value, expected) in [
        (
            json!({"type": "stitch", "bodies": [], "tolerance": 0}),
            "no bodies to stitch selected",
        ),
        (
            json!({"type": "stitch", "bodies": ["F2.b0", "F2.b0"], "tolerance": 0}),
            "listed more than once",
        ),
        (
            json!({"type": "stitch", "bodies": ["F2.b0"], "tolerance": -1}),
            "must not be negative",
        ),
    ] {
        let message = refused(&mut doc, value.clone());
        assert!(message.contains(expected), "{value}: {message}");
    }
}

#[test]
fn thicken_makes_new_solids_and_leaves_the_surface() {
    let (mut doc, sketch, a, _) = two_rectangles();
    add(&mut doc, patch_of(sketch, &[&a]));
    let sheet = body("F2.b0");
    let before = history(&doc, sheet);
    let thicken = add(
        &mut doc,
        json!({"type": "thicken", "bodies": ["F2.b0"], "thickness": 2, "operation": "new_body"}),
    );
    ok(&doc, thicken);
    let solid = body("F3.b0");
    assert_eq!(kind(&doc, solid), BodyKind::Solid);
    let face = format!("F2:patch({a})");
    assert_eq!(
        faces(&doc, solid),
        [
            format!("F3:front({face})"),
            format!("F3:back({face})"),
            format!("F3:wall({face})")
        ]
    );
    assert_eq!(history(&doc, sheet), before);
    let symmetric = add(
        &mut doc,
        json!({"type": "thicken", "bodies": ["F2.b0"], "thickness": -3, "symmetric": true,
               "operation": "new_body"}),
    );
    ok(&doc, symmetric);
    assert_eq!(
        specs(&doc, "thicken"),
        [
            "thicken(F3: 1 faces, 2, one side)",
            "thicken(F4: 1 faces, -3, symmetric)"
        ]
    );
    assert_eq!(doc.bodies().len(), 3);
    for (value, expected) in [
        (
            json!({"type": "thicken", "bodies": ["F2.b0"], "thickness": 0, "operation": "new_body"}),
            "the thickness must not be zero",
        ),
        (
            json!({"type": "thicken", "bodies": [], "thickness": 1, "operation": "new_body"}),
            "no surface bodies to thicken selected",
        ),
        (
            json!({"type": "thicken", "bodies": ["F2.b0"], "thickness": 1, "operation": "join",
                   "participants": ["F2.b0"]}),
            "cannot also be a participant",
        ),
    ] {
        let message = refused(&mut doc, value.clone());
        assert!(message.contains(expected), "{value}: {message}");
    }
}

#[test]
fn thicken_joins_other_bodies_and_refuses_solids() {
    let Block {
        mut doc,
        sketch,
        region,
        body: block_body,
        ..
    } = block();
    // The block's sketch region again, as a surface body (F3.b0).
    add(&mut doc, patch_of(sketch, &[&region]));
    let sheet = body("F3.b0");
    let before = history(&doc, sheet);
    // Without participants the join works on the block, not on the sheet.
    let join = add(
        &mut doc,
        json!({"type": "thicken", "bodies": ["F3.b0"], "thickness": 2, "operation": "join"}),
    );
    ok(&doc, join);
    assert!(
        history(&doc, block_body).starts_with("join("),
        "{}",
        history(&doc, block_body)
    );
    assert_eq!(history(&doc, sheet), before);
    let solid = add(
        &mut doc,
        json!({"type": "thicken", "bodies": [block_body], "thickness": 2, "operation": "new_body"}),
    );
    assert!(
        failure(&doc, solid).contains("is a solid; Thicken makes surface bodies solid"),
        "{}",
        failure(&doc, solid)
    );
}

#[test]
fn surface_features_refuse_mesh_bodies() {
    let (mut doc, sketch, a, _) = two_rectangles();
    add(&mut doc, patch_of(sketch, &[&a]));
    let imported = doc
        .add_base_feature(crate::exchange::BaseInput {
            name: Some("scan".to_owned()),
            ..crate::exchange::BaseInput::new(vec![crate::exchange::ImportBody {
                name: Some("Scan".to_owned()),
                color: None,
                shape: crate::testing::MockShape::imported("mesh", 1),
            }])
        })
        .unwrap();
    let mesh = imported.bodies[0].uid.to_string();
    for (value, operation) in [
        (
            json!({"type": "stitch", "bodies": ["F2.b0", mesh], "tolerance": 0}),
            "Stitch",
        ),
        (
            json!({"type": "thicken", "bodies": [mesh], "thickness": 1, "operation": "new_body"}),
            "Thicken",
        ),
    ] {
        let uid = add(&mut doc, value.clone());
        let message = failure(&doc, uid);
        assert!(
            message.contains("is a mesh body") && message.contains(operation),
            "{value}: {message}"
        );
        doc.delete_feature(uid, false).unwrap();
    }
    assert_eq!(doc.kernel().count("stitch"), 0);
    assert_eq!(doc.kernel().count("thicken"), 0);
}
