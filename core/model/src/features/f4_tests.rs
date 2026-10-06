// SPDX-License-Identifier: MIT
//! Patterns, mirrors, combine, moves, alignments, scales and primitives in
//! documents, with the mock kernel.

use serde_json::{Value, json};

use crate::datum::SurfaceGeometry;
use crate::document_tests::{block, def, num};
use crate::features::SketchPlane;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::BoundingBox;
use crate::testing::{MockKernel, MockShape};
use crate::topo::{EdgeName, FaceName, RegionKey};
use crate::{Document, FeatureStatus};

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn shape(doc: &Document<MockKernel>, body: &str) -> MockShape {
    let uid: BodyUid = body.parse().unwrap();
    doc.body_shape(uid)
        .unwrap_or_else(|| panic!("no body {body}"))
        .clone()
}

fn bounds(doc: &Document<MockKernel>, body: &str) -> BoundingBox {
    shape(doc, body).bounds.expect("bounds")
}

fn assert_bounds(doc: &Document<MockKernel>, body: &str, min: [f64; 3], max: [f64; 3]) {
    let b = bounds(doc, body);
    let near = |a: [f64; 3], e: [f64; 3]| (0..3).all(|i| (a[i] - e[i]).abs() < 1e-9);
    assert!(
        near(b.min, min) && near(b.max, max),
        "{body}: {:?} .. {:?}, expected {min:?} .. {max:?}",
        b.min,
        b.max
    );
}

fn uids(doc: &Document<MockKernel>) -> Vec<String> {
    doc.bodies().iter().map(|b| b.uid.to_string()).collect()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn x_pattern(quantity: f64, distance: f64, extra: Value) -> Value {
    let mut value = json!({"type": "rectangular_pattern",
        "objects": {"type": "bodies", "bodies": ["F2.b0"]},
        "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": quantity,
                       "distance": distance},
        "distance_type": "spacing"});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

/// The block's region and a hole: Sketch2 (F3) with a 5 mm circle c1 at
/// (10, 10) cut through all by Extrude2 (F4).
fn hole(doc: &mut Document<MockKernel>) -> (FeatureUid, RegionKey) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [10.0, 10.0], &num(5.0))
        .unwrap()
        .region;
    let cut = add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "through_all"}, "operation": "cut"}),
    );
    (cut, region)
}

#[test]
fn rectangular_patterns_of_bodies_make_new_bodies() {
    let mut b = block();
    let added = b
        .doc
        .add_feature(&def(x_pattern(3.0, 100.0, json!({}))), None)
        .unwrap();
    assert_eq!(added.name, "RectangularPattern1");
    assert_eq!(added.parameters, vec!["d4", "d5"]);
    let pattern = added.uid;
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1"]);
    assert_bounds(&b.doc, "F3.b0", [100.0, 0.0, 0.0], [160.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [200.0, 0.0, 0.0], [260.0, 40.0, 20.0]);
    let names: Vec<String> = b.doc.bodies().iter().map(|b| b.name.clone()).collect();
    assert_eq!(names, ["Body1", "Body2", "Body3"]);
    // Copied faces carry the element and the original name.
    let end = FaceName::end(b.extrude, b.region.clone());
    let copy = shape(&b.doc, "F3.b1");
    assert!(
        copy.faces.contains(
            &"F3:inst2(F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}))"
                .parse()
                .unwrap()
        )
    );
    assert!(!copy.faces.contains(&end));

    // Extent: the distance spans all elements.
    let mut extent = x_pattern(3.0, 100.0, json!({"distance_type": "extent"}));
    extent["direction1"]["distance"] = json!("d5");
    extent["direction1"]["quantity"] = json!("d4");
    b.doc.edit_feature(pattern, &def(extent)).unwrap();
    assert_bounds(&b.doc, "F3.b0", [50.0, 0.0, 0.0], [110.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [100.0, 0.0, 0.0], [160.0, 40.0, 20.0]);

    // Symmetric: the quantity on each side, element 2 the first backwards.
    let mut symmetric = x_pattern(2.0, 100.0, json!({}));
    symmetric["direction1"]["symmetric"] = json!(true);
    b.doc.edit_feature(pattern, &def(symmetric)).unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1"]);
    assert_bounds(&b.doc, "F3.b1", [-100.0, 0.0, 0.0], [-40.0, 40.0, 20.0]);

    // A suppressed element leaves out its body; the others keep their ids.
    b.doc
        .edit_feature(
            pattern,
            &def(x_pattern(3.0, 100.0, json!({"suppressed_elements": [1]}))),
        )
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b1"]);
    assert_bounds(&b.doc, "F3.b1", [200.0, 0.0, 0.0], [260.0, 40.0, 20.0]);
    let error = b
        .doc
        .edit_feature(
            pattern,
            &def(x_pattern(3.0, 100.0, json!({"suppressed_elements": [0]}))),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("element 0 is the original"),
        "{error}"
    );
    let error = b
        .doc
        .edit_feature(pattern, &def(x_pattern(2.5, 100.0, json!({}))))
        .unwrap_err();
    assert!(error.to_string().contains("whole number"), "{error}");
    let error = b
        .doc
        .edit_feature(pattern, &def(x_pattern(3.0, 0.0, json!({}))))
        .unwrap_err();
    assert!(error.to_string().contains("must not be zero"), "{error}");
}

#[test]
fn two_directions_number_the_elements_direction_one_first() {
    let mut b = block();
    let mut value = x_pattern(3.0, 100.0, json!({"suppressed_elements": [4]}));
    value["direction2"] =
        json!({"axis": {"type": "origin", "axis": "y"}, "quantity": 2, "distance": 50});
    add(&mut b.doc, value);
    // Elements 1..5 are bodies b0..b4; element 4 is (1, 1).
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1", "F3.b2", "F3.b4"]);
    assert_bounds(&b.doc, "F3.b2", [0.0, 50.0, 0.0], [60.0, 90.0, 20.0]);
    assert_bounds(&b.doc, "F3.b4", [200.0, 50.0, 0.0], [260.0, 90.0, 20.0]);
}

#[test]
fn circular_patterns_turn_about_the_axis() {
    let mut b = block();
    let circular = |angle: f64, quantity: f64, symmetric: bool| {
        json!({"type": "circular_pattern",
               "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "axis": {"type": "origin", "axis": "z"}, "quantity": quantity, "angle": angle,
               "symmetric": symmetric})
    };
    let pattern = add(&mut b.doc, circular(std::f64::consts::TAU, 4.0, false));
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0", "F3.b1", "F3.b2"]);
    // A full turn spaces four elements by 90 degrees.
    assert_bounds(&b.doc, "F3.b0", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [-60.0, -40.0, 0.0], [0.0, 0.0, 20.0]);
    // A partial angle spans the elements: 90 / (4 - 1) = 30 degrees.
    b.doc
        .edit_feature(
            pattern,
            &def(circular(std::f64::consts::FRAC_PI_2, 4.0, false)),
        )
        .unwrap();
    assert_bounds(&b.doc, "F3.b2", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
    // Symmetric: the angle on each side; a negative angle turns back.
    b.doc
        .edit_feature(
            pattern,
            &def(circular(std::f64::consts::FRAC_PI_2, 2.0, true)),
        )
        .unwrap();
    assert_bounds(&b.doc, "F3.b1", [0.0, -60.0, 0.0], [40.0, 0.0, 20.0]);
    b.doc
        .edit_feature(
            pattern,
            &def(circular(-std::f64::consts::FRAC_PI_2, 2.0, false)),
        )
        .unwrap();
    assert_bounds(&b.doc, "F3.b0", [0.0, -60.0, 0.0], [40.0, 0.0, 20.0]);
}

#[test]
fn patterns_of_features_repeat_the_tool_and_follow_edits() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let cuts = b.doc.kernel().count("cut");
    let feature_pattern = |compute: &str, quantity: Value| {
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [cut]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": quantity,
                              "distance": 20},
               "distance_type": "spacing", "compute": compute})
    };
    let unions = b.doc.kernel().count("unite");
    let pattern = add(&mut b.doc, feature_pattern("adjust", json!(3)));
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    // Adjust rebuilds the tool at each copy and cuts with them together
    // (no body splits, so the bodies are those of one copy after another).
    assert_eq!(b.doc.kernel().count("cut"), cuts + 1);
    assert_eq!(b.doc.kernel().count("unite"), unions + 1);
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    let body = shape(&b.doc, "F2.b0");
    assert!(body.history.contains("inst2"), "{}", body.history);
    let wall: FaceName = "F5:inst2(F4:side(c1))".parse().unwrap();
    assert!(body.faces.contains(&wall));

    // A fillet on the rim of the third hole.
    let rim = EdgeName::new(wall.clone(), "F5:inst2(F4:end(r{c1}))".parse().unwrap());
    let fillet = add(
        &mut b.doc,
        json!({"type": "fillet", "body": "F2.b0", "edges": [rim], "radius": 1}),
    );
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);
    // More elements keep the names of the others (d5 is the quantity).
    b.doc.set_parameter("d5", 4.0).unwrap();
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);
    b.doc.set_parameter("d5", 2.0).unwrap();
    assert!(matches!(status(&b.doc, fillet), FeatureStatus::Failed(m) if m.contains("F5:inst2")));
    b.doc.undo();
    b.doc.undo();
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);

    // Changing the patterned feature (the hole's diameter d4) recomputes
    // the pattern; undo takes the cached results.
    b.doc.set_parameter("d4", 6.0).unwrap();
    assert!(b.doc.stats().evaluated.contains(&pattern));
    b.doc.undo();
    assert!(b.doc.stats().evaluated.is_empty());

    // Identical moves the stored tool to every element: one boolean.
    let (cuts, unions) = (b.doc.kernel().count("cut"), b.doc.kernel().count("unite"));
    b.doc
        .edit_feature(pattern, &def(feature_pattern("identical", json!("d5"))))
        .unwrap();
    assert_eq!(b.doc.kernel().count("cut"), cuts + 1);
    assert_eq!(b.doc.kernel().count("unite"), unions + 1);
    assert_eq!(status(&b.doc, fillet), FeatureStatus::Ok);

    // A suppressed patterned feature fails the pattern.
    b.doc.set_suppressed(cut, true).unwrap();
    assert!(
        matches!(status(&b.doc, pattern), FeatureStatus::Failed(m) if m.contains("suppressed"))
    );
}

#[test]
fn patterns_of_new_body_features_make_bodies() {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [100.0, 0.0], &num(10.0))
        .unwrap()
        .region;
    let peg = add(
        &mut b.doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    for compute in ["adjust", "identical"] {
        let pattern = add(
            &mut b.doc,
            json!({"type": "circular_pattern", "objects": {"type": "features", "features": [peg]},
                   "axis": {"type": "origin", "axis": "z"}, "quantity": 3,
                   "angle": std::f64::consts::TAU, "compute": compute}),
        );
        assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
        let names: Vec<String> = b.doc.bodies().iter().map(|b| b.name.clone()).collect();
        assert_eq!(names, ["Body1", "Body2", "Body3", "Body4"], "{compute}");
        assert!(
            shape(&b.doc, &format!("{pattern}.b0")).faces.contains(
                &format!("{pattern}:inst1({peg}:end({region}))")
                    .parse()
                    .unwrap()
            )
        );
        b.doc.undo();
    }
}

#[test]
fn patterns_of_patterns_repeat_the_originals_at_every_product() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let row = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [cut]},
               "direction1": {"axis": "x", "quantity": 3, "distance": 20},
               "distance_type": "spacing", "compute": "identical", "suppressed_elements": [1]}),
    );
    let faces = |doc: &Document<MockKernel>| -> Vec<String> {
        shape(doc, "F2.b0")
            .faces
            .iter()
            .map(ToString::to_string)
            .filter(|f| f.contains("side(c1)"))
            .collect()
    };
    for compute in ["identical", "adjust"] {
        let turn = add(
            &mut b.doc,
            json!({"type": "circular_pattern", "objects": {"type": "features", "features": [row]},
                   "axis": "z", "quantity": 2, "angle": std::f64::consts::TAU,
                   "compute": compute}),
        );
        assert_eq!(status(&b.doc, turn), FeatureStatus::Ok, "{compute}");
        // The hole, the row's element 2 (1 is suppressed), and both turned:
        // the inner instance named inside the outer one.
        let mut names = faces(&b.doc);
        names.sort();
        assert_eq!(
            names,
            [
                "F4:side(c1)",
                "F5:inst2(F4:side(c1))",
                "F6:inst1(F4:side(c1))",
                "F6:inst1(F5:inst2(F4:side(c1)))",
            ],
            "{compute}"
        );
        b.doc.undo();
    }
    // A mirror of the row, and a pattern of that mirror: reflections are
    // moved as finished tools.
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [row]},
               "plane": "yz"}),
    );
    let shifted = add(
        &mut b.doc,
        json!({"type": "rectangular_pattern", "objects": {"type": "features", "features": [mirror]},
               "direction1": {"axis": "y", "quantity": 2, "distance": 5},
               "distance_type": "spacing"}),
    );
    assert_eq!(status(&b.doc, shifted), FeatureStatus::Ok);
    let names = faces(&b.doc);
    for name in [
        "F7:inst1(F6:inst1(F5:inst2(F4:side(c1))))",
        "F7:inst1(F6:inst1(F4:side(c1)))",
        "F7:inst1(F5:inst2(F4:side(c1)))",
        "F7:inst1(F4:side(c1))",
    ] {
        assert!(names.iter().any(|n| n == name), "{name}: {names:?}");
    }
    // The inner pattern's change recomputes the outer one.
    b.doc.set_parameter("d6", 30.0).unwrap();
    assert!(b.doc.stats().evaluated.contains(&shifted));
    b.doc.undo();
    // Patterns of bodies cannot be patterned.
    let bodies = add(&mut b.doc, x_pattern(2.0, 100.0, json!({})));
    let error = b
        .doc
        .add_feature(
            &def(
                json!({"type": "mirror", "objects": {"type": "features", "features": [bodies]},
                        "plane": "yz"}),
            ),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("repeats bodies"), "{error}");
}

#[test]
fn features_without_a_tool_cannot_be_patterned() {
    let mut b = block();
    let edge = crate::document_tests::corner(b.extrude, 1, 0);
    let fillet = add(
        &mut b.doc,
        json!({"type": "fillet", "body": "F2.b0", "edges": [edge], "radius": 1}),
    );
    let error = b
        .doc
        .add_feature(
            &def(
                json!({"type": "mirror", "objects": {"type": "features", "features": [fillet]},
                        "plane": {"type": "origin", "plane": "yz"}}),
            ),
            None,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Fillet1 (F3) cannot be patterned"),
        "{error}"
    );
}

#[test]
fn mirrors_copy_bodies_and_features() {
    let mut b = block();
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "plane": {"type": "origin", "plane": "yz"}}),
    );
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0"]);
    assert_bounds(&b.doc, "F3.b0", [-60.0, 0.0, 0.0], [0.0, 40.0, 20.0]);
    assert!(shape(&b.doc, "F3.b0").history.contains("inst1"));
    // Combined: the copy touches the original and joins it.
    b.doc
        .edit_feature(
            mirror,
            &def(
                json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "plane": {"type": "origin", "plane": "yz"}, "combine": true}),
            ),
        )
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("join("));
    // A copy that does not touch stays a body.
    *b.doc.kernel().touch.borrow_mut() = Some(vec![false]);
    b.doc
        .edit_feature(
            mirror,
            &def(
                json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
                        "plane": {"type": "origin", "plane": "yz"}, "combine": true,
                        "compute": "identical"}),
            ),
        )
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F3.b0"]);
    *b.doc.kernel().touch.borrow_mut() = None;

    // A feature mirrored in a planar face of the block (x = 60).
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    let face = "F2:side(c2[c1,c3])";
    b.doc.kernel().surfaces.borrow_mut().insert(
        face.to_owned(),
        SurfaceGeometry::Plane {
            origin: [60.0, 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
        },
    );
    let mirror = add(
        &mut b.doc,
        json!({"type": "mirror", "objects": {"type": "features", "features": [cut]},
               "plane": {"type": "face", "body": "F2.b0", "face": face}}),
    );
    assert_eq!(status(&b.doc, mirror), FeatureStatus::Ok);
    let body = shape(&b.doc, "F2.b0");
    assert!(
        body.faces
            .contains(&"F5:inst1(F4:side(c1))".parse().unwrap())
    );
    assert!(body.history.contains("moved("), "{}", body.history);
    // The mirror depends on the face's feature.
    let error = b.doc.delete_feature(b.extrude, false).unwrap_err();
    assert!(error.to_string().contains("Mirror1"), "{error}");
    let error = b
        .doc
        .add_feature(
            &def(
                json!({"type": "mirror", "objects": {"type": "features", "features": [cut]},
                        "plane": {"type": "origin", "plane": "yz"}, "combine": true}),
            ),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("bodies only"), "{error}");
}

#[test]
fn faces_pattern_as_bosses_or_pockets() {
    let mut b = block();
    let (cut, region) = hole(&mut b.doc);
    b.doc.kernel().voids.set(true);
    let wall = format!("{cut}:side(c1)");
    let pattern = add(
        &mut b.doc,
        json!({"type": "circular_pattern",
               "objects": {"type": "faces", "body": "F2.b0", "faces": [wall]},
               "axis": {"type": "fixed", "origin": [30, 20, 0], "direction": [0, 0, 1]},
               "quantity": 2, "angle": std::f64::consts::TAU}),
    );
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    assert_eq!(b.doc.kernel().count("face_tool"), 1);
    let body = shape(&b.doc, "F2.b0");
    assert!(body.history.starts_with("cut("), "{}", body.history);
    assert!(
        body.faces
            .contains(&format!("F5:inst1({wall})").parse().unwrap())
    );
    let _ = region;
}

fn second_block(doc: &mut Document<MockKernel>) -> FeatureUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [20.0, 0.0], &num(40.0), &num(40.0))
        .unwrap()
        .region;
    add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}),
    )
}

#[test]
fn combine_joins_cuts_and_intersects_bodies() {
    let combine = |operation: &str, keep: bool| {
        json!({"type": "combine", "target": "F2.b0", "tools": ["F4.b0"],
               "operation": operation, "keep_tools": keep})
    };
    let mut b = block();
    second_block(&mut b.doc);
    let uid = add(&mut b.doc, combine("join", false));
    assert_eq!(b.doc.feature(uid).unwrap().name, "Combine1");
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("unite("));
    b.doc
        .edit_feature(uid, &def(combine("join", true)))
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F4.b0"]);

    // A cut that splits the target makes bodies of the combine.
    b.doc.kernel().split.set(Some(2));
    b.doc
        .edit_feature(uid, &def(combine("cut", false)))
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0", "F5.b0"]);
    // An intersection keeps its pieces as one body.
    b.doc
        .edit_feature(uid, &def(combine("intersect", false)))
        .unwrap();
    assert_eq!(uids(&b.doc), ["F2.b0"]);
    assert!(shape(&b.doc, "F2.b0").history.starts_with("unite(common("));
    b.doc.kernel().split.set(None);
    *b.doc.kernel().touch.borrow_mut() = Some(vec![false]);
    b.doc
        .edit_feature(uid, &def(combine("intersect", true)))
        .unwrap();
    assert!(
        matches!(status(&b.doc, uid), FeatureStatus::Failed(m) if m.contains("do not intersect"))
    );
    *b.doc.kernel().touch.borrow_mut() = None;

    for (value, message) in [
        (
            json!({"type": "combine", "target": "F2.b0", "tools": ["F2.b0"], "operation": "join"}),
            "both the target and a tool",
        ),
        (
            json!({"type": "combine", "target": "F2.b0", "tools": [], "operation": "join"}),
            "no tool bodies",
        ),
        (
            json!({"type": "combine", "target": "F2.b0", "tools": ["F1.b0"], "operation": "cut"}),
            "does not create bodies",
        ),
    ] {
        let error = b.doc.add_feature(&def(value), None).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn moves_and_copies_place_bodies() {
    let mut b = block();
    let moved = add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"],
               "transform": {"type": "translate_xyz", "x": 10, "y": 5, "z": 0}}),
    );
    assert_eq!(b.doc.feature(moved).unwrap().name, "Move1");
    assert_bounds(&b.doc, "F2.b0", [10.0, 5.0, 0.0], [70.0, 45.0, 20.0]);
    // A move keeps the face names.
    assert!(
        shape(&b.doc, "F2.b0")
            .faces
            .contains(&FaceName::end(b.extrude, b.region.clone()))
    );

    let copied = add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"], "copy": true,
               "transform": {"type": "rotate", "axis": {"type": "origin", "axis": "z"},
                             "angle": std::f64::consts::FRAC_PI_2}}),
    );
    assert_eq!(uids(&b.doc), ["F2.b0", "F4.b0"]);
    assert_bounds(&b.doc, "F4.b0", [-45.0, 10.0, 0.0], [-5.0, 70.0, 20.0]);
    assert!(
        shape(&b.doc, "F4.b0")
            .faces
            .iter()
            .all(|f| f.to_string().starts_with("F4:inst1(F2:"))
    );
    let _ = copied;

    add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"],
               "transform": {"type": "point_to_point", "from": {"type": "fixed", "point": [10, 5, 0]},
                             "to": {"type": "origin"}}}),
    );
    assert_bounds(&b.doc, "F2.b0", [0.0, 0.0, 0.0], [60.0, 40.0, 20.0]);
    add(
        &mut b.doc,
        json!({"type": "move", "bodies": ["F2.b0"],
               "transform": {"type": "free", "matrix": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 0]]}}),
    );
    assert_bounds(&b.doc, "F2.b0", [60.0, 0.0, 0.0], [100.0, 60.0, 20.0]);

    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "move", "bodies": ["F2.b0"],
                        "transform": {"type": "free", "matrix": [[2, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]]}})),
            None,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("rotation and a translation"),
        "{error}"
    );
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "move", "bodies": [],
                        "transform": {"type": "translate_xyz", "x": 1, "y": 0, "z": 0}})),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("no bodies"), "{error}");
}

#[test]
fn alignments_and_scales() {
    let mut b = block();
    // The block's top face (the mock's middle (30, 20, 20)) against a plane
    // at z = 100 facing up: turned over and its middle onto the plane's
    // origin.
    let align = add(
        &mut b.doc,
        json!({"type": "align", "bodies": ["F2.b0"],
               "from": {"type": "plane", "plane": {"type": "face", "body": "F2.b0",
                        "face": "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}},
               "to": {"type": "plane", "plane": {"type": "fixed", "origin": [0, 0, 100],
                      "normal": [0, 0, 1]}}}),
    );
    assert_eq!(status(&b.doc, align), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F2.b0", [-30.0, -20.0, 100.0], [30.0, 20.0, 120.0]);
    let error = b
        .doc
        .add_feature(
            &def(json!({"type": "align", "bodies": ["F2.b0"],
                        "from": {"type": "axis", "axis": {"type": "origin", "axis": "x"}},
                        "to": {"type": "point", "point": {"type": "origin"}}})),
            None,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot align an axis to a point"),
        "{error}"
    );

    let mut b = block();
    let scale = add(
        &mut b.doc,
        json!({"type": "scale", "bodies": ["F2.b0"], "point": {"type": "origin"},
               "scale": {"type": "non_uniform", "x": 2, "y": 1, "z": 0.5}}),
    );
    assert_eq!(b.doc.feature(scale).unwrap().name, "Scale1");
    assert_bounds(&b.doc, "F2.b0", [0.0, 0.0, 0.0], [120.0, 40.0, 10.0]);
    let error = b
        .doc
        .add_feature(
            &def(
                json!({"type": "scale", "bodies": ["F2.b0"], "point": {"type": "origin"},
                        "scale": {"type": "uniform", "factor": 0}}),
            ),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("greater than zero"), "{error}");
}

#[test]
fn primitives_are_tools_that_patterns_rebuild() {
    let mut doc = Document::new(MockKernel::default());
    let added = doc
        .add_feature(
            &def(
                json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
                        "corner": [5, 5], "length": 30, "width": 20, "height": 10,
                        "operation": "new_body"}),
            ),
            None,
        )
        .unwrap();
    assert_eq!(added.name, "Box1");
    assert_eq!(added.parameters, vec!["d1", "d2", "d3"]);
    assert_eq!(uids(&doc), ["F1.b0"]);
    assert!(
        shape(&doc, "F1.b0")
            .faces
            .contains(&"F1:side3".parse().unwrap())
    );
    assert_bounds(&doc, "F1.b0", [5.0, 5.0, 0.0], [35.0, 25.0, 10.0]);
    // A symmetric height centres the box on the plane.
    doc.edit_feature(
        added.uid,
        &def(
            json!({"type": "box", "plane": {"type": "origin", "plane": "xy"},
                    "corner": [5, 5], "length": "d1", "width": "d2", "height": "d3",
                    "symmetric": true, "operation": "new_body"}),
        ),
    )
    .unwrap();
    assert_bounds(&doc, "F1.b0", [5.0, 5.0, -5.0], [35.0, 25.0, 5.0]);

    // A cylinder joined to the box, patterned with Adjust: each copy is
    // rebuilt on the moved plane.
    let cylinder = add(
        &mut doc,
        json!({"type": "cylinder", "plane": {"type": "origin", "plane": "xy"}, "center": [10, 10],
               "diameter": 4, "height": 20, "operation": "join"}),
    );
    let primitives = doc.kernel().count("primitive");
    add(
        &mut doc,
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [cylinder]},
               "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 3,
                              "distance": 5},
               "distance_type": "spacing"}),
    );
    assert_eq!(doc.kernel().count("primitive"), primitives + 2);
    let body = shape(&doc, "F1.b0");
    assert!(
        body.history.contains("cylinder(F2:2x2x20@20,10,0)"),
        "{}",
        body.history
    );
    assert!(body.faces.contains(&"F3:inst2(F2:top)".parse().unwrap()));

    let error = doc
        .add_feature(
            &def(
                json!({"type": "torus", "plane": {"type": "origin", "plane": "xy"},
                        "diameter": 10, "section_diameter": 10, "position": "inside",
                        "operation": "new_body"}),
            ),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("too large"), "{error}");
    let error = doc
        .add_feature(
            &def(
                json!({"type": "sphere", "plane": {"type": "origin", "plane": "xy"},
                        "diameter": 10, "operation": "new_body", "participants": ["F1.b0"]}),
            ),
            None,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("no participant bodies"),
        "{error}"
    );
    // A cut sphere that misses every body fails.
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    let sphere = add(
        &mut doc,
        json!({"type": "sphere", "plane": {"type": "origin", "plane": "xz"},
               "center": [100, 100], "diameter": 10, "operation": "cut"}),
    );
    assert!(
        matches!(status(&doc, sphere), FeatureStatus::Failed(m) if m.contains("does not cut into"))
    );
}

#[test]
fn path_patterns_follow_sketch_curves() {
    let mut b = block();
    // Line c1 of the block's rectangle runs from (0, 0) to (60, 0).
    let path = |path: Value, along: bool| {
        json!({"type": "path_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "path": path, "quantity": 3, "distance": 60, "distance_type": "extent",
               "start": 0.5, "along_path": along})
    };
    let pattern = add(
        &mut b.doc,
        path(
            json!({"type": "sketch_curve", "sketch": "F1", "curve": "c1"}),
            false,
        ),
    );
    assert_bounds(&b.doc, "F3.b0", [30.0, 0.0, 0.0], [90.0, 40.0, 20.0]);
    assert_bounds(&b.doc, "F3.b1", [60.0, 0.0, 0.0], [120.0, 40.0, 20.0]);

    // Along a circle of radius 10 about the origin, a quarter turn apart.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let curve = b
        .doc
        .add_circle(sketch, [0.0, 0.0], &num(20.0))
        .unwrap()
        .curves[0];
    let mut circle = path(
        json!({"type": "sketch_curve", "sketch": sketch, "curve": curve.curve_name()}),
        true,
    );
    circle["start"] = json!(0);
    circle["distance"] = json!(std::f64::consts::PI * 5.0);
    circle["distance_type"] = json!("spacing");
    let around = add(&mut b.doc, circle);
    assert_eq!(status(&b.doc, around), FeatureStatus::Ok);
    assert_bounds(&b.doc, "F5.b0", [-40.0, 0.0, 0.0], [0.0, 60.0, 20.0]);
    assert_bounds(&b.doc, "F5.b1", [-60.0, -40.0, 0.0], [0.0, 0.0, 20.0]);
    assert_eq!(status(&b.doc, pattern), FeatureStatus::Ok);
    let error = b
        .doc
        .add_feature(
            &def(path(
                json!({"type": "sketch_curve", "sketch": "F1", "curve": "c9"}),
                false,
            )),
            None,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("Sketch1 has no curve c9"),
        "{error}"
    );
}

#[test]
fn files_keep_the_definitions() {
    let mut b = block();
    let (cut, _) = hole(&mut b.doc);
    second_block(&mut b.doc);
    for value in [
        x_pattern(
            3.0,
            100.0,
            json!({"compute": "identical", "suppressed_elements": [2]}),
        ),
        json!({"type": "circular_pattern", "objects": {"type": "features", "features": [cut]},
               "axis": {"type": "edge", "body": "F2.b0",
                        "edge": "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"},
               "quantity": 4, "angle": std::f64::consts::TAU}),
        json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
               "plane": {"type": "fixed", "origin": [30, 0, 0], "normal": [1, 0, 0]},
               "combine": true}),
        json!({"type": "combine", "target": "F2.b0", "tools": ["F6.b0"], "operation": "cut",
               "keep_tools": true, "new_component": true}),
        json!({"type": "move", "bodies": ["F2.b0"], "copy": true,
               "transform": {"type": "translate_along",
                             "axis": {"type": "fixed", "origin": [0, 0, 0], "direction": [0, 1, 0]},
                             "distance": 50}}),
        json!({"type": "scale", "bodies": ["F2.b0"], "point": {"type": "fixed", "point": [1, 2, 3]},
               "scale": {"type": "uniform", "factor": 2}}),
        json!({"type": "torus", "plane": {"type": "origin", "plane": "yz"}, "diameter": 40,
               "section_diameter": 10, "operation": "new_body"}),
    ] {
        add(&mut b.doc, value);
    }
    let json = b.doc.to_json();
    let loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    let features: Vec<_> = loaded.features().map(|f| f.def.clone()).collect();
    let original: Vec<_> = b.doc.features().map(|f| f.def.clone()).collect();
    assert_eq!(features, original);
    assert!(json.contains("rectangular_pattern"), "{json}");
}
