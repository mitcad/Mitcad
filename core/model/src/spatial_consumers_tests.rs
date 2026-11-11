// SPDX-License-Identifier: MIT
//! Closed paths of 3D sketches and the other inputs that take a 3D
//! sketch's curves (mitcad#1550), with the mock kernel: a circle, a closed
//! fit-point spline and a closed chain of lines as Pipe paths, run either
//! way, the whole loop from one chained pick, a closed curve that is no
//! piece of a longer path, and the chains the sketch reports; a loft's
//! centre line and rails and a surface loft through a 3D sketch's curves;
//! a construction plane and point along a 3D sketch's path, an axis
//! through its line and a point at its point, a path pattern along it, and
//! an Intersection Curve of its curves; and the planar paths of all of
//! them, a closed one too, rewritten when their sketch becomes a 3D sketch.

use serde_json::{Value, json};

use crate::datum::Datum;
use crate::features::path::{CurvePath, is_closed};
use crate::features::spatial_path::{SpatialPath, chains, pieces, route};
use crate::features::spatial_sketch::{SpatialAxis, SpatialSense, SpatialSource};
use crate::features::{FeatureDef, PathRef, SketchPlane, ValueInput};
use crate::ids::{EntityUid, FeatureUid};
use crate::sketch::spatial_edit::{SegmentMade, SegmentRequest, SpatialPointInput, SplineRequest};
use crate::sweeps::PathCurve;
use crate::test_support::{def, num};
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus};

use SpatialPointInput::{At, Existing};

const TOL: f64 = 1e-6;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn failed(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("{uid} is {other:?}, not failed"),
    }
}

fn line(
    doc: &mut Document<MockKernel>,
    sketch: FeatureUid,
    start: SpatialPointInput,
    end: SpatialPointInput,
) -> SegmentMade {
    doc.edit_spatial_sketch(sketch, "Add Line", |e| {
        e.add_segment(&SegmentRequest::new(start, end))
    })
    .unwrap_or_else(|e| panic!("{e}"))
    .0
}

fn names(curves: &[EntityUid]) -> Vec<String> {
    curves.iter().map(|c| c.curve_name()).collect()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None)
        .unwrap_or_else(|e| panic!("{e}"))
        .uid
}

fn pipe(path: Value) -> Value {
    json!({"type": "pipe", "path": path, "size": 4, "operation": "new_body"})
}

fn spatial(sketch: FeatureUid, curves: &[EntityUid]) -> Value {
    json!({"spatial_sketch": sketch, "curves": names(curves)})
}

fn chained(sketch: FeatureUid, first: EntityUid, reversed: bool) -> Value {
    json!({"spatial_sketch": sketch, "curves": [first.curve_name()], "chain": true,
           "reversed": reversed})
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

/// The pieces of the last pipe the mock kernel made for `feature`.
fn path_of(doc: &Document<MockKernel>, feature: FeatureUid) -> String {
    let spec = spec(doc, &format!("pipe {feature} "));
    let after = spec.split(" path ").nth(1).expect("a path");
    after.split(' ').next().expect("its pieces").to_owned()
}

fn resolved(doc: &Document<MockKernel>, path: &SpatialPath) -> Result<Vec<PathCurve>, String> {
    let def = doc.spatial_sketch_def(path.sketch).expect("a 3D sketch");
    let output = doc.spatial_sketch(path.sketch).expect("evaluated");
    let legs = route(def, path, "3DSketch1", &|p| output.point(p))?;
    pieces(output, &legs, "3DSketch1")
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

/// The route A (0, 0, 0) at the origin, B (40, 0, 0), C (40, 30, 0),
/// D (40, 30, 20): AB, BC and CD along X, Y and Z, 90 long; and AB's
/// length parameter.
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

/// W9 closed: a circle, a closed fit-point spline and a triangle of lines
/// are paths of a Pipe, either way; one chained pick takes the triangle
/// whole; a closed curve is no piece of a longer path; the chains list
/// each closed one with its path.
#[test]
fn closed_curves_and_loops_are_paths() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let (circle, _) = doc
        .edit_spatial_sketch(sketch, "Add Circle", |e| {
            e.add_circle(At([0.0, 0.0, 10.0]), 10.0, [0.0, 1.0, 1.0], None, false)
        })
        .unwrap();
    let (ring, _) = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            e.add_spline(&SplineRequest {
                closed: true,
                ..SplineRequest::fit(vec![
                    At([70.0, 0.0, 0.0]),
                    At([80.0, 0.0, 0.0]),
                    At([75.0, 5.0, 3.0]),
                ])
            })
        })
        .unwrap();
    let t1 = line(&mut doc, sketch, At([50.0, 0.0, 0.0]), At([60.0, 0.0, 0.0]));
    let t2 = line(&mut doc, sketch, Existing(t1.end), At([55.0, 5.0, 4.0]));
    let t3 = line(&mut doc, sketch, Existing(t2.end), Existing(t1.start));
    let tail = line(
        &mut doc,
        sketch,
        At([0.0, 50.0, 0.0]),
        At([10.0, 50.0, 0.0]),
    );

    // The circle and the closed spline, each a path by itself, either way.
    for curve in [circle, ring.spline] {
        let uid = add(&mut doc, pipe(spatial(sketch, &[curve])));
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
        assert_eq!(path_of(&doc, uid), curve.curve_name());
        let path = SpatialPath {
            sketch,
            curves: vec![curve],
            reversed: false,
            chain: false,
        };
        let ahead = resolved(&doc, &path).unwrap();
        assert!(is_closed(&ahead), "{ahead:?}");
        let back = resolved(
            &doc,
            &SpatialPath {
                reversed: true,
                ..path
            },
        )
        .unwrap();
        assert_eq!(back.len(), 1);
        assert_ne!(back[0].curve, ahead[0].curve);
        let turned = add(
            &mut doc,
            pipe(
                json!({"spatial_sketch": sketch, "curves": [curve.curve_name()],
                        "reversed": true}),
            ),
        );
        assert_eq!(status(&doc, turned), FeatureStatus::Ok);
    }

    // The triangle from one chained pick of T2: round from T2's start,
    // the other way when reversed.
    let loop_ahead = add(&mut doc, pipe(chained(sketch, t2.line, false)));
    assert_eq!(status(&doc, loop_ahead), FeatureStatus::Ok);
    assert_eq!(
        path_of(&doc, loop_ahead),
        names(&[t2.line, t3.line, t1.line]).join(",")
    );
    let loop_back = add(&mut doc, pipe(chained(sketch, t2.line, true)));
    assert_eq!(
        path_of(&doc, loop_back),
        names(&[t2.line, t1.line, t3.line]).join(",")
    );
    let pieces = resolved(
        &doc,
        &SpatialPath {
            sketch,
            curves: vec![t2.line],
            reversed: true,
            chain: true,
        },
    )
    .unwrap();
    assert!(is_closed(&pieces));
    let start = mitcad_model_base::curves::curve_ends(&pieces[0].curve)
        .unwrap()
        .0;
    assert!(near(start, [55.0, 5.0, 4.0]), "{start:?}");
    // Picked curve by curve, in any order: the same loop.
    let picked = add(
        &mut doc,
        pipe(spatial(sketch, &[t3.line, t1.line, t2.line])),
    );
    assert_eq!(
        path_of(&doc, picked),
        names(&[t3.line, t1.line, t2.line]).join(",")
    );

    // A closed curve with another: no path.
    let mixed = add(&mut doc, pipe(spatial(sketch, &[tail.line, circle])));
    assert!(
        failed(&doc, mixed).contains(&format!("{} is a closed curve", circle.curve_name())),
        "{}",
        failed(&doc, mixed)
    );

    // The chains: the circle, the closed spline and the triangle closed,
    // each with the path that takes it whole; the lone line open.
    let found = chains(doc.spatial_sketch_def(sketch).unwrap());
    assert_eq!(found.len(), 4, "{found:?}");
    for chain in &found {
        let path = chain.path(sketch).expect("no branches");
        assert_eq!(chain.closed, chain.legs[0].curve != tail.line, "{chain:?}");
        assert_eq!(path.curves.len(), chain.legs.len());
    }
    let triangle = found
        .iter()
        .find(|c| c.legs.len() == 3)
        .expect("the triangle");
    assert!(triangle.closed);
}

/// A loft's centre line and rails along a 3D sketch's curves, typed as a
/// Pipe's path; a surface loft through curves of a 3D sketch.
#[test]
fn a_loft_takes_a_3d_sketch_as_center_line_rails_and_sections() {
    let mut doc = Document::new(MockKernel::default());
    let bottom = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let low = doc.add_circle(bottom, [0.0, 0.0], &num(20.0)).unwrap();
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 50}}),
    );
    let top = doc
        .add_sketch(SketchPlane::Construction(plane))
        .unwrap()
        .uid;
    let high = doc.add_circle(top, [0.0, 0.0], &num(10.0)).unwrap();
    let route = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let spine = line(&mut doc, route, At([0.0; 3]), At([5.0, 0.0, 25.0]));
    let bend = line(&mut doc, route, Existing(spine.end), At([0.0, 0.0, 50.0]));
    let rail = line(&mut doc, route, At([10.0, 0.0, 0.0]), At([5.0, 0.0, 50.0]));
    let sections = json!([
        {"type": "profile", "sketch": bottom, "region": low.region.to_string()},
        {"type": "profile", "sketch": top, "region": high.region.to_string()}]);

    let along = add(
        &mut doc,
        json!({"type": "loft", "sections": sections,
               "centerline": chained(route, spine.line, false), "operation": "new_body"}),
    );
    assert_eq!(status(&doc, along), FeatureStatus::Ok);
    let made = spec(&doc, &format!("loft {along} "));
    assert!(
        made.contains(&format!(
            "centerline {},{}",
            spine.line.curve_name(),
            bend.line.curve_name()
        )),
        "{made}"
    );
    let railed = add(
        &mut doc,
        json!({"type": "loft", "sections": sections,
               "rails": [spatial(route, &[rail.line])], "operation": "new_body"}),
    );
    assert_eq!(status(&doc, railed), FeatureStatus::Ok);
    let made = spec(&doc, &format!("loft {railed} "));
    assert!(
        made.contains(&format!("rails {}", rail.line.curve_name())),
        "{made}"
    );
    // The planar form of a 3D sketch's curves names the typed one.
    let error = doc
        .add_feature(
            &def(json!({"type": "loft", "sections": sections,
                        "rails": [{"sketch": route, "curves": [rail.line.curve_name()]}],
                        "operation": "new_body"})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("\"spatial_sketch\""), "{error}");

    // A surface loft through two of its curves.
    let sheet = add(
        &mut doc,
        json!({"type": "loft", "surface": true, "operation": "new_body", "sections": [
            {"type": "curves", "sketch": route, "curves": [spine.line.curve_name()],
             "chain": true},
            {"type": "curves", "sketch": route, "curves": [rail.line.curve_name()]}]}),
    );
    assert_eq!(status(&doc, sheet), FeatureStatus::Ok);
    let made = spec(&doc, &format!("loft_sheet({sheet}:"));
    assert!(made.contains(&bend.line.curve_name()), "{made}");

    // A surface loft of the profiles through a 3D sketch's rail, as the
    // Surface Loft panel makes it (mitcad#1550); a surface loft of curve
    // sections takes no rails.
    let surface = add(
        &mut doc,
        json!({"type": "loft", "surface": true, "operation": "new_body", "sections": sections,
               "rails": [spatial(route, &[rail.line])]}),
    );
    assert_eq!(status(&doc, surface), FeatureStatus::Ok);
    let made = spec(&doc, &format!("loft {surface} "));
    assert!(
        made.contains(&format!("surface rails {}", rail.line.curve_name())),
        "{made}"
    );
    let curves_railed = json!({"type": "loft", "surface": true, "operation": "new_body",
        "sections": [
            {"type": "curves", "sketch": route, "curves": [spine.line.curve_name()],
             "chain": true},
            {"type": "curves", "sketch": route, "curves": [rail.line.curve_name()]}],
        "rails": [spatial(route, &[rail.line])]});
    let error = doc
        .add_feature(&def(curves_railed), None)
        .unwrap_err()
        .to_string();
    let refused = "no end conditions, center line or rails";
    assert!(error.contains(refused), "{error}");
}

/// A construction plane and a point along a 3D sketch's route, an axis
/// through one of its lines and a point at one of its points; a path
/// pattern along the route; all follow an edit of the sketch.
#[test]
fn construction_and_patterns_go_along_a_3d_sketch() {
    let mut doc = Document::new(MockKernel::default());
    let (sketch, [ab, bc, cd], ab_length) = route_abcd(&mut doc);
    let path = chained(sketch, ab.line, false);
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "along_path", "path": path, "distance": 0.25}}),
    );
    let point = add(
        &mut doc,
        json!({"type": "construction_point",
               "definition": {"type": "along_path", "path": path, "distance": 0.5}}),
    );
    let normal = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "normal_at_point", "path": path,
                              "point": {"point": [45, 30, 10]}}}),
    );
    let axis = add(
        &mut doc,
        json!({"type": "construction_axis",
               "definition": {"type": "edge",
                              "edge": {"sketch": sketch, "curve": cd.line.curve_name()}}}),
    );
    let corner = add(
        &mut doc,
        json!({"type": "construction_point",
               "definition": {"type": "point",
                              "point": {"sketch": sketch, "point": format!("p{}", bc.end.0)}}}),
    );
    let check = |doc: &Document<MockKernel>, ab_length: f64| {
        let total = ab_length + 30.0 + 20.0;
        let Some(Datum::Plane(p)) = doc.datum(plane) else {
            panic!("{:?}", doc.datum(plane))
        };
        assert!(near(p.origin, [0.25 * total, 0.0, 0.0]), "{p:?}");
        assert!(near(p.normal(), [1.0, 0.0, 0.0]), "{p:?}");
        let Some(Datum::Point(q)) = doc.datum(point) else {
            panic!("{:?}", doc.datum(point))
        };
        let along_bc = 0.5 * total - ab_length;
        assert!(near(q.point, [ab_length, along_bc, 0.0]), "{q:?}");
        let Some(Datum::Plane(n)) = doc.datum(normal) else {
            panic!("{:?}", doc.datum(normal))
        };
        assert!(near(n.origin, [ab_length, 30.0, 10.0]), "{n:?}");
        assert!(near(n.normal(), [0.0, 0.0, 1.0]), "{n:?}");
        let Some(Datum::Axis(a)) = doc.datum(axis) else {
            panic!("{:?}", doc.datum(axis))
        };
        assert!(near(a.origin, [ab_length, 30.0, 0.0]), "{a:?}");
        assert!(near(a.direction, [0.0, 0.0, 1.0]), "{a:?}");
        let Some(Datum::Point(c)) = doc.datum(corner) else {
            panic!("{:?}", doc.datum(corner))
        };
        assert!(near(c.point, [ab_length, 30.0, 0.0]), "{c:?}");
    };
    check(&doc, 40.0);

    // A path pattern of a body along the route.
    let block = add(
        &mut doc,
        json!({"type": "box", "plane": "xy", "corner": [-2, -2], "length": 4, "width": 4,
               "height": 4, "operation": "new_body"}),
    );
    let pattern = add(
        &mut doc,
        json!({"type": "path_pattern",
               "objects": {"type": "bodies", "bodies": [format!("{block}.b0")]},
               "path": path, "quantity": 3, "distance": 30, "distance_type": "spacing",
               "along_path": true}),
    );
    assert_eq!(status(&doc, pattern), FeatureStatus::Ok);

    // AB to 50: everything follows.
    doc.set_parameter(&ab_length, 50.0).unwrap();
    for uid in [sketch, plane, point, normal, axis, corner, pattern] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    }
    check(&doc, 50.0);

    // An axis is one line, of a 3D sketch too.
    let error = doc
        .add_feature(
            &def(json!({"type": "construction_axis",
                        "definition": {"type": "edge",
                                       "edge": {"sketch": sketch,
                                                "curves": names(&[ab.line, bc.line])}}})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("an axis is one line"), "{error}");
}

/// An Intersection Curve of a 3D sketch's curves, swept along its support
/// plane's normal, with a planar sketch's line on XZ.
#[test]
fn an_intersection_curve_takes_a_3d_sketch() {
    let mut doc = Document::new(MockKernel::default());
    let (sketch, [ab, bc, _], _) = route_abcd(&mut doc);
    let side = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let (across, _) = doc
        .edit_sketch(side, "Add Line", |e| {
            e.add_line(
                crate::sketch::edit::PointInput::At([0.0, 0.0]),
                crate::sketch::edit::PointInput::At([60.0, 0.0]),
                false,
                false,
            )
        })
        .unwrap();
    let crossing = add(
        &mut doc,
        json!({"type": "intersection_curve",
               "first": {"sketch": sketch, "curves": names(&[ab.line, bc.line])},
               "second": {"sketch": side, "curves": [across.curve_name()]}}),
    );
    assert_eq!(status(&doc, crossing), FeatureStatus::Ok);
    let made = spec(&doc, "intersection_curves ");
    assert!(
        made.starts_with(&format!(
            "intersection_curves {},{} along 0 0 1",
            ab.line.curve_name(),
            bc.line.curve_name()
        )) || made.starts_with(&format!(
            "intersection_curves {},{} along 0 0 1",
            bc.line.curve_name(),
            ab.line.curve_name()
        )),
        "{made}"
    );
}

/// The planar paths of a Pipe (round a circle), a loft's centre line, a
/// path pattern and a construction plane along a sketch are rewritten
/// into its typed paths when it becomes a 3D sketch, and keep their
/// geometry.
#[test]
fn every_planar_path_is_rewritten_by_a_conversion() {
    let mut doc = Document::new(MockKernel::default());
    let planar = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let (first, _) = doc
        .edit_sketch(planar, "Add Line", |e| {
            e.add_line(
                crate::sketch::edit::PointInput::At([0.0, 0.0]),
                crate::sketch::edit::PointInput::At([0.0, 40.0]),
                false,
                false,
            )
        })
        .unwrap();
    let round = doc.add_circle(planar, [80.0, 0.0], &num(20.0)).unwrap();
    let bottom = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let low = doc.add_circle(bottom, [0.0, 0.0], &num(20.0)).unwrap();
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 40}}),
    );
    let top = doc
        .add_sketch(SketchPlane::Construction(plane))
        .unwrap()
        .uid;
    let high = doc.add_circle(top, [0.0, 0.0], &num(10.0)).unwrap();

    let ring = add(
        &mut doc,
        pipe(json!({"sketch": planar, "curves": names(&round.curves)})),
    );
    let loft = add(
        &mut doc,
        json!({"type": "loft", "operation": "new_body",
               "sections": [
                   {"type": "profile", "sketch": bottom, "region": low.region.to_string()},
                   {"type": "profile", "sketch": top, "region": high.region.to_string()}],
               "centerline": {"sketch": planar, "curves": [first.curve_name()]}}),
    );
    let along = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "along_path",
                              "path": {"sketch": planar, "curve": first.curve_name()},
                              "distance": 0.25}}),
    );
    let block = add(
        &mut doc,
        json!({"type": "box", "plane": "xy", "corner": [-2, -2], "length": 4, "width": 4,
               "height": 4, "operation": "new_body"}),
    );
    let pattern = add(
        &mut doc,
        json!({"type": "path_pattern",
               "objects": {"type": "bodies", "bodies": [format!("{block}.b0")]},
               "path": {"sketch": planar, "curve": first.curve_name()},
               "quantity": 2, "distance": 10, "distance_type": "spacing"}),
    );
    for uid in [ring, loft, along, pattern] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    }
    let before = doc.datum(along);

    doc.convert_sketch_to_spatial(planar)
        .unwrap_or_else(|e| panic!("{e}"));
    for uid in [ring, loft, along, pattern] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok, "{uid}");
    }
    let typed = |path: SpatialPath| {
        assert_eq!(path.sketch, planar);
        assert!(!path.reversed);
        path.curves
    };
    match &doc.feature(ring).unwrap().def {
        FeatureDef::Pipe(p) => match &p.path {
            CurvePath::Spatial(path) => assert_eq!(typed(path.clone()), round.curves),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    match &doc.feature(loft).unwrap().def {
        FeatureDef::Loft(l) => match &l.centerline {
            Some(CurvePath::Spatial(path)) => assert_eq!(typed(path.clone()), [first]),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    match &doc.feature(pattern).unwrap().def {
        FeatureDef::PathPattern(p) => match &p.path {
            PathRef::Spatial(path) => assert_eq!(typed(path.clone()), [first]),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    let after = doc.datum(along);
    match (before, after) {
        (Some(Datum::Plane(a)), Some(Datum::Plane(b))) => {
            assert!(
                near(a.origin, b.origin) && near(a.normal(), b.normal()),
                "{a:?} {b:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}
