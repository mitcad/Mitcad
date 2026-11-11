// SPDX-License-Identifier: MIT
//! What grows from curves in their sketch's plane takes a 3D sketch's
//! curves in its sketch plane (mitcad#1550), with the mock kernel: on the
//! route A-B-C-D, AB and BC lie in the sketch plane and CD leaves it. A rib
//! and a web from AB and BC, a chained pick of AB that takes BC and not CD;
//! thin walls along AB and BC and the curves of a surface extrusion,
//! revolution and sweep (along CD), following AB's length; CD refused at
//! the evaluation naming the sketch; a planar sketch whose curves a rib, a
//! surface extrusion and a surface sweep use turned into a 3D sketch and
//! back with them.

use serde_json::{Value, json};

use crate::features::spatial_sketch::{SpatialAxis, SpatialSense, SpatialSource};
use crate::features::{SketchPlane, ValueInput};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::sketch::edit::PointInput;
use crate::sketch::spatial_edit::{SegmentMade, SegmentRequest, SpatialPointInput};
use crate::test_support::def;
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus};

use SpatialPointInput::{At, Existing};

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn ok(doc: &Document<MockKernel>, uid: FeatureUid) {
    assert_eq!(status(doc, uid), FeatureStatus::Ok, "{uid}");
}

fn failed(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("{uid} is {other:?}, not failed"),
    }
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None)
        .unwrap_or_else(|e| panic!("{e}"))
        .uid
}

fn names(curves: &[EntityUid]) -> Vec<String> {
    curves.iter().map(|c| c.curve_name()).collect()
}

/// The planar form of curves of `sketch`.
fn curves(sketch: FeatureUid, list: &[EntityUid]) -> Value {
    json!({"sketch": sketch, "curves": names(list)})
}

/// The last spec the mock kernel recorded that starts with `start`.
fn spec(doc: &Document<MockKernel>, start: &str) -> String {
    let specs = doc.kernel().specs.borrow();
    specs
        .iter()
        .rev()
        .find(|s| s.starts_with(start))
        .unwrap_or_else(|| panic!("no spec {start}: {specs:?}"))
        .clone()
}

/// The largest x of the first body of `feature`.
fn max_x(doc: &Document<MockKernel>, feature: FeatureUid) -> f64 {
    doc.body_shape(BodyUid::new(feature, 0))
        .and_then(|shape| shape.bounds)
        .unwrap_or_else(|| panic!("no bounds of {feature}.b0"))
        .max[0]
}

/// A dimensioned segment along a frame axis as its own edit, with the
/// parameters it made.
fn along(
    doc: &mut Document<MockKernel>,
    sketch: FeatureUid,
    request: SegmentRequest,
    axis: SpatialAxis,
    length: f64,
) -> (SegmentMade, Vec<String>) {
    let request = SegmentRequest {
        axis: Some((axis, SpatialSense::Positive)),
        length: Some(ValueInput::Number(length)),
        ..request
    };
    let (made, report) = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| e.add_segment(&request))
        .unwrap_or_else(|e| panic!("{e}"));
    (made, report.parameters)
}

/// 3DSketch1 on XY: the route A (0, 0, 0) at the origin, B (40, 0, 0),
/// C (40, 30, 0), D (40, 30, 20); AB and BC in the sketch plane, CD along
/// Z out of it; and AB's length parameter.
fn route_abcd(doc: &mut Document<MockKernel>) -> (FeatureUid, [SegmentMade; 3], String) {
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let start = SegmentRequest {
        start_source: Some(SpatialSource::Origin),
        ..SegmentRequest::new(At([0.0; 3]), At([38.0, 1.0, 0.0]))
    };
    let (ab, params) = along(doc, sketch, start, SpatialAxis::X, 40.0);
    let next = SegmentRequest::new(Existing(ab.end), At([41.0, 28.0, 0.0]));
    let (bc, _) = along(doc, sketch, next, SpatialAxis::Y, 30.0);
    let last = SegmentRequest::new(Existing(bc.end), At([40.0, 31.0, 19.0]));
    let (cd, _) = along(doc, sketch, last, SpatialAxis::Z, 20.0);
    (sketch, [ab, bc, cd], params[0].clone())
}

/// A block for the ribs and webs to join (a join needs a body).
fn block(doc: &mut Document<MockKernel>) -> FeatureUid {
    add(
        doc,
        json!({"type": "box", "plane": "xy", "corner": [0, 0], "length": 40,
               "width": 30, "height": 10, "operation": "new_body"}),
    )
}

/// The message of a curve of 3DSketch1 out of its sketch plane.
fn off_plane(curve: EntityUid) -> String {
    format!(
        "3DSketch1's {} does not lie in its sketch plane",
        curve.curve_name()
    )
}

/// A rib from AB and BC, chained in the plane; a chained pick of AB takes
/// BC and stops where CD leaves the plane; a web from both, one wall each;
/// CD refused at the evaluation, alone or with AB, naming the sketch.
#[test]
fn ribs_and_webs_take_a_3d_sketchs_curves_in_its_plane() {
    let mut doc = Document::new(MockKernel::default());
    let (sketch, [ab, bc, cd], _) = route_abcd(&mut doc);
    let (ab_c, bc_c) = (ab.line.curve_name(), bc.line.curve_name());
    block(&mut doc);

    let rib = add(
        &mut doc,
        json!({"type": "rib", "curves": curves(sketch, &[ab.line, bc.line]),
               "thickness": 4, "extent": {"type": "depth", "depth": 10}}),
    );
    ok(&doc, rib);
    let made = spec(&doc, &format!("rib {rib} "));
    assert!(made.contains(&format!("chains {ab_c},{bc_c} ")), "{made}");

    let chained = add(
        &mut doc,
        json!({"type": "rib",
               "curves": {"sketch": sketch, "curves": [ab_c], "chain": true},
               "thickness": 4, "extent": {"type": "depth", "depth": 10}}),
    );
    ok(&doc, chained);
    let made = spec(&doc, &format!("rib {chained} "));
    assert!(made.contains(&format!("chains {ab_c},{bc_c} ")), "{made}");

    let web = add(
        &mut doc,
        json!({"type": "web", "curves": curves(sketch, &[ab.line, bc.line]),
               "thickness": 2, "extent": {"type": "depth", "depth": 10}}),
    );
    ok(&doc, web);
    let made = spec(&doc, &format!("web {web} "));
    assert!(made.contains(&format!("chains {ab_c}; {bc_c} ")), "{made}");

    for list in [vec![cd.line], vec![ab.line, cd.line]] {
        let uid = add(
            &mut doc,
            json!({"type": "rib", "curves": curves(sketch, &list), "thickness": 4,
                   "extent": {"type": "depth", "depth": 10}}),
        );
        let message = failed(&doc, uid);
        assert!(message.contains(&off_plane(cd.line)), "{message}");
    }

    // An id the sketch does not have and one listed twice: refused when
    // added, as a path's selection is.
    for list in [vec![EntityUid(99)], vec![ab.line, ab.line]] {
        let error = doc
            .add_feature(
                &def(json!({"type": "rib", "curves": curves(sketch, &list),
                            "thickness": 4, "extent": {"type": "to_next"}})),
                None,
            )
            .map(|_| panic!("{list:?} must be refused"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("has no curve c99") || error.contains("listed more than once"),
            "{error}"
        );
    }
}

/// Thin walls along AB and BC, and the curves a surface extrusion,
/// revolution and sweep (along CD, the route's own way out of the plane)
/// sweep: all follow AB's length; CD among the curves is refused naming
/// the sketch.
#[test]
fn thin_walls_and_surfaces_take_a_3d_sketchs_curves_in_its_plane() {
    let mut doc = Document::new(MockKernel::default());
    let (sketch, [ab, bc, cd], length) = route_abcd(&mut doc);
    let (ab_c, bc_c) = (ab.line.curve_name(), bc.line.curve_name());
    let in_plane = curves(sketch, &[ab.line, bc.line]);

    let thin = add(
        &mut doc,
        json!({"type": "extrude", "extent": {"type": "distance", "distance": 10},
               "thin": {"location": "side1", "thickness": 2, "curves": in_plane},
               "operation": "new_body"}),
    );
    ok(&doc, thin);
    let made = spec(&doc, &format!("extrude {thin} "));
    assert!(
        made.contains(&format!("chains {ab_c}>{bc_c} open")),
        "{made}"
    );
    assert!((max_x(&doc, thin) - 40.0).abs() < 1e-9);

    let extruded = add(
        &mut doc,
        json!({"type": "extrude", "curves": in_plane,
               "extent": {"type": "distance", "distance": 10},
               "operation": "new_body", "surface": true}),
    );
    ok(&doc, extruded);
    let made = spec(&doc, &format!("extrude_sheet({extruded}: "));
    assert!(
        made.contains(&format!("{ab_c}>{bc_c} along [0, 0, 1]")),
        "{made}"
    );

    let revolved = add(
        &mut doc,
        json!({"type": "revolve", "curves": in_plane, "axis": "y",
               "extent": {"type": "angle", "angle": "90 deg"},
               "operation": "new_body", "surface": true}),
    );
    ok(&doc, revolved);
    let made = spec(&doc, &format!("revolve_sheet({revolved}: "));
    assert!(made.contains(&format!("{ab_c}>{bc_c}")), "{made}");

    let swept = add(
        &mut doc,
        json!({"type": "sweep", "curves": in_plane,
               "path": {"spatial_sketch": sketch, "curves": [cd.line.curve_name()]},
               "operation": "new_body", "surface": true}),
    );
    ok(&doc, swept);
    let made = spec(&doc, &format!("sweep_sheet({swept}: "));
    assert!(
        made.contains(&format!("path {}", cd.line.curve_name())),
        "{made}"
    );

    // AB to 50: the walls follow.
    doc.set_parameter(&length, 50.0).unwrap();
    for uid in [thin, extruded, revolved, swept] {
        ok(&doc, uid);
    }
    assert!((max_x(&doc, thin) - 50.0).abs() < 1e-9);

    // CD among the curves: each fails naming the sketch.
    let with_cd = curves(sketch, &[bc.line, cd.line]);
    let walls = add(
        &mut doc,
        json!({"type": "extrude", "extent": {"type": "distance", "distance": 10},
               "thin": {"location": "side1", "thickness": 2, "curves": with_cd},
               "operation": "new_body"}),
    );
    let sheet = add(
        &mut doc,
        json!({"type": "extrude", "curves": with_cd,
               "extent": {"type": "distance", "distance": 10},
               "operation": "new_body", "surface": true}),
    );
    for uid in [walls, sheet] {
        let message = failed(&doc, uid);
        assert!(message.contains(&off_plane(cd.line)), "{message}");
    }
}

/// A planar sketch whose line a rib, a surface extrusion and a surface
/// sweep use turns into a 3D sketch with them, its line in its sketch
/// plane, and back into 2D; each keeps its curves and evaluates.
#[test]
fn a_sketch_with_ribs_and_surface_curves_converts_both_ways() {
    let mut doc = Document::new(MockKernel::default());
    let planar = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let (line, _) = doc
        .edit_sketch(planar, "Add Line", |e| {
            e.add_line(
                PointInput::At([0.0, 0.0]),
                PointInput::At([40.0, 0.0]),
                false,
                false,
            )
        })
        .unwrap();
    let rail_sketch = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let (rail, _) = doc
        .edit_sketch(rail_sketch, "Add Line", |e| {
            e.add_line(
                PointInput::At([0.0, 0.0]),
                PointInput::At([0.0, 30.0]),
                false,
                false,
            )
        })
        .unwrap();
    block(&mut doc);
    let rib = add(
        &mut doc,
        json!({"type": "rib", "curves": curves(planar, &[line]), "thickness": 4,
               "extent": {"type": "depth", "depth": 10}}),
    );
    let sheet = add(
        &mut doc,
        json!({"type": "extrude", "curves": curves(planar, &[line]),
               "extent": {"type": "distance", "distance": 10},
               "operation": "new_body", "surface": true}),
    );
    let swept = add(
        &mut doc,
        json!({"type": "sweep", "curves": curves(planar, &[line]),
               "path": curves(rail_sketch, &[rail]),
               "operation": "new_body", "surface": true}),
    );
    let users = [rib, sheet, swept];
    for uid in users {
        ok(&doc, uid);
    }
    let stored = |doc: &Document<MockKernel>| -> Vec<String> {
        users
            .iter()
            .map(|uid| format!("{:?}", doc.feature(*uid).unwrap().def))
            .collect()
    };
    let before = stored(&doc);

    doc.convert_sketch_to_spatial(planar)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(doc.spatial_sketch_def(planar).is_some());
    for uid in users {
        ok(&doc, uid);
    }
    assert_eq!(stored(&doc), before, "the curves keep their form");
    let made = spec(&doc, &format!("rib {rib} "));
    assert!(
        made.contains(&format!("chains {} ", line.curve_name())),
        "{made}"
    );

    doc.convert_sketch_to_planar(planar)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(doc.spatial_sketch_def(planar).is_none());
    for uid in users {
        ok(&doc, uid);
    }
    assert_eq!(stored(&doc), before);
}
