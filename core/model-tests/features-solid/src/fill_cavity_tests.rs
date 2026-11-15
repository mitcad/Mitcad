// SPDX-License-Identifier: MIT
//! Fill Cavity (mitcad#1818) in documents with the mock kernel: the box
//! below the cap that reaches the kernel, the pockets chosen with and
//! without a seed, the join that keeps the body, the failures in words with
//! the body left as it was, and a moved cap that recomputes the fill. The
//! geometry itself (a V slit's volume, the walls gone) is tested on OCCT:
//! `geometry.fill_cavity` and `cli.fill_cavity`.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::BoundingBox;
use mitcad_model::notice::{FeatureError, NoticePlace};
use mitcad_model::test_support::{Block, block, def, extrude, num, side};
use mitcad_model::testing::{MockCavity, MockKernel};
use mitcad_model::topo::FaceName;
use mitcad_model::{Document, FeatureStatus, ModelError};

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn failure(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureError {
    match status(doc, uid) {
        FeatureStatus::Failed(error) => error,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn cavity_specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with("cavity "))
        .cloned()
        .collect()
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn joins(doc: &Document<MockKernel>) -> usize {
    doc.kernel()
        .calls
        .borrow()
        .get("join")
        .copied()
        .unwrap_or(0)
}

/// A rectangular pocket cut 10 mm into the block (a sketch of its own with
/// c1..c4 at `corner`, cut by an extrude): its four walls.
fn pocket(b: &mut Block, corner: [f64; 2], size: [f64; 2]) -> Vec<FaceName> {
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, corner, &num(size[0]), &num(size[1]))
        .unwrap()
        .region;
    let cut = extrude(sketch, &[&region], 10.0, "cut", &[b.body]);
    let uid = b.doc.add_feature(&cut, None).unwrap().uid;
    assert_eq!(b.status(uid), FeatureStatus::Ok);
    (0..4).map(|i| side(uid, 1, i)).collect()
}

/// The pocket the kernel finds below the cap: its walls and its bounds
/// from 10 to 20 high.
fn cavity(walls: &[FaceName], corner: [f64; 2], size: [f64; 2], open: bool) -> MockCavity {
    MockCavity {
        walls: walls.to_vec(),
        bounds: BoundingBox {
            min: [corner[0], corner[1], 10.0],
            max: [corner[0] + size[0], corner[1] + size[1], 20.0],
        },
        open,
    }
}

/// The cap: the block's top plane, 20 high.
fn top_plane() -> Value {
    json!({"origin": [0, 0, 20], "normal": [0, 0, 1]})
}

#[test]
fn a_closed_pocket_is_filled_up_to_the_cap_and_joined_to_the_body() {
    let mut b = block();
    let walls = pocket(&mut b, [20.0, 15.0], [20.0, 10.0]);
    b.doc
        .kernel()
        .cavities
        .borrow_mut()
        .push(cavity(&walls, [20.0, 15.0], [20.0, 10.0], false));
    let before = joins(&b.doc);
    let fill = add(
        &mut b.doc,
        json!({"type": "fill_cavity", "body": b.body, "cap": top_plane()}),
    )
    .unwrap();
    assert_eq!(b.status(fill), FeatureStatus::Ok);
    assert_eq!(b.doc.feature(fill).unwrap().name, "FillCavity1");
    // One box: the block's extent in the cap's frame, 1 mm larger round
    // the sides and below, its top on the cap.
    assert_eq!(
        cavity_specs(&b.doc),
        [format!(
            "cavity {fill} top {fill}:fill at 0 0 20 normal 0 0 1 box -1 -1 -21 .. 61 41 0"
        )]
    );
    // The pocket (not the space round the block) joined to the body, which
    // keeps its id; the filled top is named after the feature.
    assert_eq!(joins(&b.doc), before + 1);
    let history = b.history(b.body);
    assert!(history.starts_with("join("), "{history}");
    assert!(history.contains(&format!("{fill}:fill)#0")), "{history}");
    assert!(!history.contains(":outside)"), "{history}");
    assert!(faces(&b.doc, b.body).contains(&format!("{fill}:fill")));
}

#[test]
fn a_cap_that_is_a_face_names_the_filled_top_after_it() {
    let mut b = block();
    let walls = pocket(&mut b, [20.0, 15.0], [20.0, 10.0]);
    b.doc
        .kernel()
        .cavities
        .borrow_mut()
        .push(cavity(&walls, [20.0, 15.0], [20.0, 10.0], false));
    // The block's top face, 20 high.
    let top = FaceName::end(b.extrude, b.region.clone());
    let fill = add(
        &mut b.doc,
        json!({"type": "fill_cavity", "body": b.body,
               "cap": {"body": b.body, "face": top}}),
    )
    .unwrap();
    assert_eq!(b.status(fill), FeatureStatus::Ok);
    let spec = cavity_specs(&b.doc).pop().unwrap();
    assert!(
        spec.starts_with(&format!("cavity {fill} top {fill}:fill({top}) at ")),
        "{spec}"
    );
    assert!(faces(&b.doc, b.body).contains(&format!("{fill}:fill({top})")));
}

#[test]
fn a_seed_fills_only_the_pocket_it_bounds() {
    let mut b = block();
    let a = pocket(&mut b, [5.0, 5.0], [10.0, 10.0]);
    let c = pocket(&mut b, [40.0, 20.0], [10.0, 10.0]);
    {
        let mut cavities = b.doc.kernel().cavities.borrow_mut();
        cavities.push(cavity(&a, [5.0, 5.0], [10.0, 10.0], false));
        cavities.push(cavity(&c, [40.0, 20.0], [10.0, 10.0], false));
    }
    // Without a seed both pockets the cap closes.
    let fill = add(
        &mut b.doc,
        json!({"type": "fill_cavity", "body": b.body, "cap": top_plane()}),
    )
    .unwrap();
    assert_eq!(b.status(fill), FeatureStatus::Ok);
    let history = b.history(b.body);
    assert!(history.contains(&format!("{fill}:fill)#0")), "{history}");
    assert!(history.contains(&format!("{fill}:fill)#1")), "{history}");
    // A wall of the second pocket as the seed: that pocket only.
    b.doc
        .edit_feature(
            fill,
            &def(
                json!({"type": "fill_cavity", "body": b.body, "cap": top_plane(),
                        "seed": c[1]}),
            ),
        )
        .unwrap();
    assert_eq!(b.status(fill), FeatureStatus::Ok);
    let history = b.history(b.body);
    assert!(history.contains(&format!("{fill}:fill)#1")), "{history}");
    assert!(!history.contains(&format!("{fill}:fill)#0")), "{history}");
}

#[test]
fn a_pocket_open_below_the_cap_fails_in_words_and_leaves_the_body() {
    let mut b = block();
    let walls = pocket(&mut b, [20.0, 15.0], [20.0, 10.0]);
    b.doc
        .kernel()
        .cavities
        .borrow_mut()
        .push(cavity(&walls, [20.0, 15.0], [20.0, 10.0], true));
    let cut = b.history(b.body);
    let fill = add(
        &mut b.doc,
        json!({"type": "fill_cavity", "body": b.body, "cap": top_plane(), "seed": walls[0]}),
    )
    .unwrap();
    let error = failure(&b.doc, fill);
    assert_eq!(
        error.message,
        "the cavity opens out of the body below the cap"
    );
    // The seed painted on hover with the opening; its name for agents only.
    assert!(
        error.places.contains(&NoticePlace::Face {
            body: Some(b.body.to_string()),
            name: walls[0].to_string(),
        }),
        "{:?}",
        error.places
    );
    assert!(
        error
            .places
            .iter()
            .any(|p| matches!(p, NoticePlace::Box { .. })),
        "{:?}",
        error.places
    );
    assert!(!error.message.contains(&walls[0].to_string()));
    assert!(error.agent_text().contains(&walls[0].to_string()));
    assert_eq!(b.history(b.body), cut);

    // Without a seed the open pocket is not a cavity: nothing to fill.
    b.doc
        .edit_feature(
            fill,
            &def(json!({"type": "fill_cavity", "body": b.body, "cap": top_plane()})),
        )
        .unwrap();
    assert_eq!(
        failure(&b.doc, fill).message,
        "found no closed cavity below the cap to fill"
    );
    assert_eq!(b.history(b.body), cut);

    // A cap below the whole body leaves nothing below it.
    b.doc
        .edit_feature(
            fill,
            &def(json!({"type": "fill_cavity", "body": b.body,
                        "cap": {"origin": [0, 0, -5], "normal": [0, 0, 1]}})),
        )
        .unwrap();
    assert_eq!(
        failure(&b.doc, fill).message,
        "the body lies wholly above the cap: there is nothing below it to fill"
    );
    assert_eq!(b.history(b.body), cut);
}

#[test]
fn moving_the_cap_recomputes_the_fill() {
    let mut b = block();
    let walls = pocket(&mut b, [20.0, 15.0], [20.0, 10.0]);
    b.doc
        .kernel()
        .cavities
        .borrow_mut()
        .push(cavity(&walls, [20.0, 15.0], [20.0, 10.0], false));
    let plane = add(
        &mut b.doc,
        json!({"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 20}}),
    )
    .unwrap();
    let fill = add(
        &mut b.doc,
        json!({"type": "fill_cavity", "body": b.body, "cap": plane}),
    )
    .unwrap();
    assert_eq!(b.status(fill), FeatureStatus::Ok);
    let first = cavity_specs(&b.doc).pop().unwrap();
    assert!(first.contains(" at 0 0 20 normal 0 0 1 "), "{first}");

    // The plane moved down to 15: the fill is computed again below it.
    b.doc
        .edit_feature(
            plane,
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": "xy", "distance": 15}})),
        )
        .unwrap();
    assert!(b.evaluated().contains(&fill));
    assert_eq!(b.status(fill), FeatureStatus::Ok);
    let moved = cavity_specs(&b.doc).pop().unwrap();
    assert!(moved.contains(" at 0 0 15 normal 0 0 1 "), "{moved}");
    assert!(moved.ends_with(" .. 61 41 0"), "{moved}");
    assert!(b.history(b.body).contains(&format!("{fill}:fill)#0")));

    // The offset moves the cap along its normal; flip fills above it.
    b.doc
        .edit_feature(
            fill,
            &def(json!({"type": "fill_cavity", "body": b.body, "cap": plane, "offset": 2})),
        )
        .unwrap();
    let offset = cavity_specs(&b.doc).pop().unwrap();
    assert!(offset.contains(" at 0 0 17 normal 0 0 1 "), "{offset}");
    b.doc
        .edit_feature(
            fill,
            &def(json!({"type": "fill_cavity", "body": b.body, "cap": plane, "flip": true})),
        )
        .unwrap();
    let flipped = cavity_specs(&b.doc).pop().unwrap();
    assert!(flipped.contains(" at 0 0 15 normal 0 0 -1 "), "{flipped}");
}

#[test]
fn the_cap_must_be_a_plane_and_the_seed_a_face_of_the_body() {
    let mut b = block();
    let error = add(
        &mut b.doc,
        json!({"type": "fill_cavity", "body": b.body, "cap": "z"}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("the cap"), "{error}");
    let error = add(
        &mut b.doc,
        json!({"type": "fill_cavity", "body": b.body, "cap": top_plane(), "seed": "F9:side"}),
    )
    .unwrap_err();
    assert!(!error.to_string().is_empty());
}
