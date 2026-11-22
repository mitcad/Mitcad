// SPDX-License-Identifier: MIT
//! Cone (mitcad#2239): a cone or a frustum placed as a cylinder, its faces,
//! its parameters and its refusals, with the mock kernel. The volumes are
//! OCCT's: `cli.primitive_cone` (`tools/cli/tests/primitive_cone.json`).

use serde_json::{Value, json};

use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{block, def};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::{Document, FeatureStatus};

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn shape(doc: &Document<MockKernel>, body: &str) -> MockShape {
    let uid: BodyUid = body.parse().unwrap();
    doc.body_shape(uid)
        .unwrap_or_else(|| panic!("no body {body}"))
        .clone()
}

fn faces(doc: &Document<MockKernel>, body: &str) -> Vec<String> {
    shape(doc, body)
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn cone(extra: Value) -> Value {
    let mut value = json!({"type": "cone", "plane": {"type": "origin", "plane": "xy"},
        "radius": 10, "height": 30, "operation": "new_body"});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

/// A cone without a top radius ends in an apex: its base and its side, no
/// top; its values are parameters (radius, height). Edited into a frustum
/// it gains the top face, and the names of the others stay.
#[test]
fn a_cone_and_a_frustum_name_their_faces() {
    let mut doc = Document::new(MockKernel::default());
    let added = doc.add_feature(&def(cone(json!({}))), None).unwrap();
    assert_eq!(added.name, "Cone1");
    assert_eq!(added.parameters, vec!["d1", "d2"]);
    let uid = added.uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(faces(&doc, "F1.b0"), ["F1:bottom", "F1:side0"]);
    assert!(
        shape(&doc, "F1.b0")
            .history
            .contains("cone(F1:10x0x30@0,0,0)"),
        "{}",
        shape(&doc, "F1.b0").history
    );
    let FeatureDef::Cone(stored) = &doc.feature(uid).unwrap().def else {
        panic!("not a cone")
    };
    assert_eq!(stored.top_radius, None);

    // A frustum: the top face too.
    doc.edit_feature(
        uid,
        &def(cone(
            json!({"radius": "d1", "height": "d2", "top_radius": 4}),
        )),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(faces(&doc, "F1.b0"), ["F1:bottom", "F1:top", "F1:side0"]);
    // An apex at the base: no bottom face.
    doc.edit_feature(
        uid,
        &def(cone(json!({"radius": 0, "height": "d2", "top_radius": 4}))),
    )
    .unwrap();
    assert_eq!(faces(&doc, "F1.b0"), ["F1:top", "F1:side0"]);
}

/// Placed as a cylinder: at its centre on the plane; a negative height
/// keeps the base on the plane (the frame turned over), a symmetric one
/// puts the base half the height below.
#[test]
fn a_cone_stands_on_its_plane() {
    let mut doc = Document::new(MockKernel::default());
    add(
        &mut doc,
        cone(json!({"center": [5, 6], "top_radius": 2, "height": -30})),
    );
    let history = shape(&doc, "F1.b0").history;
    assert!(history.contains("cone(F1:10x2x30@5,6,0)"), "{history}");
    add(
        &mut doc,
        cone(json!({"center": [50, 0], "height": 30, "symmetric": true})),
    );
    let history = shape(&doc, "F2.b0").history;
    assert!(history.contains("cone(F2:10x0x30@50,0,-15)"), "{history}");
}

/// A fillet on the base edge is kept when the radius changes: the edge's
/// name is that of the base and the side, whatever their sizes.
#[test]
fn a_fillet_on_the_base_edge_follows_the_radius() {
    let mut doc = Document::new(MockKernel::default());
    add(&mut doc, cone(json!({"top_radius": 4})));
    let fillet = add(
        &mut doc,
        json!({"type": "fillet", "body": "F1.b0", "edges": ["E{F1:bottom|F1:side0}"], "radius": 2}),
    );
    assert_eq!(status(&doc, fillet), FeatureStatus::Ok);
    doc.set_parameter("d1", 12.0).unwrap();
    assert_eq!(status(&doc, fillet), FeatureStatus::Ok);
    let history = shape(&doc, "F1.b0").history;
    assert!(history.contains("cone(F1:12x4x30@0,0,0)"), "{history}");
}

/// A cone cut from a block, apex down from its top: the block stays one
/// body, the cone its tool.
#[test]
fn a_cone_cuts_a_box() {
    let mut b = block();
    let cut = add(
        &mut b.doc,
        cone(
            json!({"plane": {"type": "fixed", "origin": [30, 20, 20], "normal": [0, 0, 1],
                              "x_axis": [1, 0, 0]},
                    "radius": 5, "height": -10, "operation": "cut", "participants": ["F2.b0"]}),
        ),
    );
    assert_eq!(status(&b.doc, cut), FeatureStatus::Ok);
    assert_eq!(b.doc.bodies().len(), 1);
    let history = shape(&b.doc, "F2.b0").history;
    assert!(history.contains("cone(F3:5x0x10@30,20,20)"), "{history}");
}

/// Refused in words: both radii zero, a negative radius, a zero height,
/// equal radii (a cylinder); a value changed into one of them fails the
/// feature with the same words.
#[test]
fn a_cone_refuses_what_is_no_cone() {
    let mut doc = Document::new(MockKernel::default());
    let refused = |doc: &mut Document<MockKernel>, extra: Value| {
        doc.add_feature(&def(cone(extra)), None)
            .unwrap_err()
            .to_string()
    };
    let error = refused(&mut doc, json!({"radius": 0, "top_radius": 0}));
    assert!(
        error.contains("a cone needs a radius greater than zero at its base or its top"),
        "{error}"
    );
    let error = refused(&mut doc, json!({"radius": -1}));
    assert!(
        error.contains("the cone radius must not be negative"),
        "{error}"
    );
    let error = refused(&mut doc, json!({"top_radius": -1}));
    assert!(
        error.contains("the cone top radius must not be negative"),
        "{error}"
    );
    let error = refused(&mut doc, json!({"height": 0}));
    assert!(error.contains("cone height must not be zero"), "{error}");
    let error = refused(&mut doc, json!({"top_radius": 10}));
    assert!(
        error.contains("that is a cylinder, use Cylinder"),
        "{error}"
    );
    assert!(doc.features().next().is_none());

    // A parameter that makes the radii equal fails the feature.
    let mut doc = Document::new(MockKernel::default());
    let uid = add(&mut doc, cone(json!({"top_radius": 4})));
    doc.set_parameter("d2", 10.0).unwrap();
    let FeatureStatus::Failed(error) = status(&doc, uid) else {
        panic!("the cone did not fail")
    };
    assert!(
        format!("{error:?}").contains("that is a cylinder, use Cylinder"),
        "{error:?}"
    );
}
