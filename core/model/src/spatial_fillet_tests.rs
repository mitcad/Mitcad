// SPDX-License-Identifier: MIT
//! A 3D sketch's Fillet (mitcad#1548), with the mock kernel: the route A-B-C-D
//! of W1 rounded at B trims AB and BC back to the tangent points with their
//! ids, alignments and lengths (moved to construction lines to the corner,
//! so they keep measuring to it), puts a tangent arc of the radius in their
//! plane with two tangencies and a radius dimension, and keeps B where the
//! lines' extensions meet, fully constrained with nothing redundant; the
//! radius edited through its dimension; both corners in one edit, the
//! second arc Equal to the first; the refusals (collinear lines, lines that
//! share no end, a radius that does not fit, a point that is no corner)
//! leaving everything as it was; undo restoring the corner; and a Pipe along
//! the route, its explicit curves taking the arcs and a chain selection
//! finding them, through the fillet.

use serde_json::json;

use crate::features::path::CurvePath;
use crate::features::spatial_sketch::{
    SpatialAxis, SpatialConstraintKind, SpatialCurveKind, SpatialDimensionKind, SpatialSense,
    SpatialSource,
};
use crate::features::{FeatureDef, SketchPlane, ValueInput};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Curve3;
use crate::sketch::spatial_edit::{
    FilletCorner, FilletMade, SegmentMade, SegmentRequest, SpatialEditReport, SpatialPointInput,
};
use crate::test_support::def;
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus};

use SpatialAxis::{X, Y, Z};
use SpatialPointInput::{At, Existing};

const TOL: f64 = 1e-6;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn along(
    start: SpatialPointInput,
    toward: [f64; 3],
    axis: SpatialAxis,
    length: f64,
) -> SegmentRequest {
    SegmentRequest {
        axis: Some((axis, SpatialSense::Positive)),
        length: Some(ValueInput::Number(length)),
        ..SegmentRequest::new(start, At(toward))
    }
}

fn segment(
    doc: &mut Document<MockKernel>,
    sketch: FeatureUid,
    request: SegmentRequest,
) -> SegmentMade {
    doc.edit_spatial_sketch(sketch, "Add Line", |e| e.add_segment(&request))
        .unwrap_or_else(|e| panic!("{e}"))
        .0
}

/// W1: A at the origin, then 40 along +X, 30 along +Y and 20 along +Z;
/// fully constrained. The segments AB, BC and CD.
fn route(doc: &mut Document<MockKernel>, sketch: FeatureUid) -> [SegmentMade; 3] {
    let ab = segment(
        doc,
        sketch,
        SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            ..along(At([0.0; 3]), [37.0, 2.0, 1.0], X, 40.0)
        },
    );
    let bc = segment(
        doc,
        sketch,
        along(Existing(ab.end), [42.0, 27.0, 0.0], Y, 30.0),
    );
    let cd = segment(
        doc,
        sketch,
        along(Existing(bc.end), [40.0, 33.0, 21.0], Z, 20.0),
    );
    [ab, bc, cd]
}

fn fillet(
    doc: &mut Document<MockKernel>,
    sketch: FeatureUid,
    corners: &[FilletCorner],
    radius: f64,
) -> Result<(Vec<FilletMade>, SpatialEditReport), String> {
    doc.edit_spatial_sketch(sketch, "Fillet", |e| {
        e.fillets(corners, &ValueInput::Number(radius))
    })
    .map_err(|e| e.to_string())
}

fn point(doc: &Document<MockKernel>, sketch: FeatureUid, id: EntityUid) -> [f64; 3] {
    doc.spatial_sketch(sketch)
        .and_then(|o| o.point(id))
        .unwrap_or_else(|| panic!("p{} is not solved", id.0))
}

/// A line's start and end ids.
fn ends(doc: &Document<MockKernel>, sketch: FeatureUid, line: EntityUid) -> [EntityUid; 2] {
    doc.spatial_sketch_def(sketch)
        .and_then(|d| d.line(line))
        .unwrap_or_else(|| panic!("c{} is no line", line.0))
}

/// The tangencies of the sketch, in its order: the curves, the point and
/// the sense the solve stored.
fn tangencies(
    doc: &Document<MockKernel>,
    sketch: FeatureUid,
) -> Vec<(EntityUid, EntityUid, EntityUid, Option<SpatialSense>)> {
    doc.spatial_sketch_def(sketch)
        .unwrap()
        .constraints
        .iter()
        .filter_map(|c| match c.kind {
            SpatialConstraintKind::Tangent { a, b, at, sense } => Some((a, b, at, sense)),
            _ => None,
        })
        .collect()
}

/// The fillet at B: AB and BC end at the tangent points with their ids,
/// the arc of radius 10 is tangent to both in the XY plane, B stays at the
/// corner and the lengths still measure AB and BC to it.
#[test]
fn a_fillet_trims_both_lines_and_keeps_their_ids() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let [ab, bc, cd] = route(&mut doc, sketch);
    let b = ab.end;
    let (made, report) =
        fillet(&mut doc, sketch, &[FilletCorner::Point(b)], 10.0).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(doc.undo_label(), Some("Fillet in 3DSketch1"));
    let f = &made[0];
    assert_eq!(f.corner, b);
    assert_eq!(f.lines, [ab.line, bc.line]);
    let [ta, tb] = f.tangent_points;
    // The lines keep their ids and directions, from A to the first tangent
    // point and from the second to C.
    assert_eq!(ends(&doc, sketch, ab.line), [ab.start, ta]);
    assert_eq!(ends(&doc, sketch, bc.line), [tb, bc.end]);
    assert_eq!(ends(&doc, sketch, cd.line), [bc.end, cd.end]);
    for (p, at) in [
        (ab.start, [0.0, 0.0, 0.0]),
        (b, [40.0, 0.0, 0.0]),
        (bc.end, [40.0, 30.0, 0.0]),
        (cd.end, [40.0, 30.0, 20.0]),
        (ta, [30.0, 0.0, 0.0]),
        (tb, [40.0, 10.0, 0.0]),
        (f.center, [30.0, 10.0, 0.0]),
    ] {
        let solved = point(&doc, sketch, p);
        assert!(near(solved, at), "p{}: {solved:?}, not {at:?}", p.0);
    }
    // The corner's construction lines from the tangent points to B; the
    // lengths of AB and BC moved to construction lines A-B and B-C.
    assert_eq!(ends(&doc, sketch, f.corner_lines[0]), [ta, b]);
    assert_eq!(ends(&doc, sketch, f.corner_lines[1]), [b, tb]);
    assert_eq!(f.full_lines.len(), 2);
    assert_eq!(ends(&doc, sketch, f.full_lines[0]), [ab.start, b]);
    assert_eq!(ends(&doc, sketch, f.full_lines[1]), [b, bc.end]);
    let def = doc.spatial_sketch_def(sketch).unwrap();
    for line in f.corner_lines.iter().chain(&f.full_lines) {
        assert!(def.curve(*line).unwrap().construction, "c{}", line.0);
    }
    assert_eq!(
        def.dimension(ab.dimensions[0]).unwrap().kind,
        SpatialDimensionKind::Length {
            line: f.full_lines[0]
        }
    );
    // AB keeps its alignment along +X.
    assert!(def.constraints.iter().any(|c| c.kind
        == SpatialConstraintKind::Align {
            line: ab.line,
            axis: X,
            sense: SpatialSense::Positive,
        }));
    // Two tangencies of the arc and the corner line in line with AB, all
    // pointing the same way.
    let tangent = tangencies(&doc, sketch);
    assert_eq!(
        tangent,
        [
            (ab.line, f.corner_lines[0], ta, Some(SpatialSense::Positive)),
            (ab.line, f.arc, ta, Some(SpatialSense::Positive)),
            (f.arc, bc.line, tb, Some(SpatialSense::Positive)),
        ]
    );
    let radius = f.dimension.expect("a radius dimension");
    assert_eq!(
        def.dimension(radius).unwrap().kind,
        SpatialDimensionKind::Radius { curve: f.arc }
    );
    let output = doc.spatial_sketch(sketch).unwrap();
    match output.curve(f.arc) {
        Some(Curve3::Conic {
            normal,
            major,
            minor,
            start,
            end,
            closed: false,
            ..
        }) => {
            assert!(near(normal, [0.0, 0.0, 1.0]), "{normal:?}");
            assert!((major - 10.0).abs() <= TOL && (minor - 10.0).abs() <= TOL);
            assert!((end - start - std::f64::consts::FRAC_PI_2).abs() <= TOL);
        }
        other => panic!("{other:?}"),
    }
    // Nothing free, nothing redundant; the lengths still measure to B.
    let status = &report.status;
    assert_eq!(status.dof, 0, "{status:?}");
    assert!(status.fully_constrained);
    assert!(status.redundant.is_empty(), "{:?}", status.redundant);
    assert!((status.measured[&ab.dimensions[0]] - 40.0).abs() <= TOL);
    assert!((status.measured[&bc.dimensions[0]] - 30.0).abs() <= TOL);
    assert!((status.measured[&radius] - 10.0).abs() <= TOL);
    assert_eq!(report.rounded, [([ab.line, bc.line], f.arc)]);
}

/// The arc is tangent to both lines whatever the corner's angle: an obtuse
/// corner of fixed points in a plane slanted to the sketch's, filleted by
/// naming its lines.
#[test]
fn the_arc_is_tangent_to_both_lines() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let (made, _) = doc
        .edit_spatial_sketch(sketch, "Add Lines", |e| {
            let a = e.add_point([0.0, 0.0, 0.0])?;
            e.fix(a)?;
            let corner = e.add_point([30.0, 0.0, 10.0])?;
            e.fix(corner)?;
            let c = e.add_point([30.0 + 15.0, 25.980762113533157, 10.0])?;
            e.fix(c)?;
            let first = e.add_line(Existing(a), Existing(corner), false)?;
            let second = e.add_line(Existing(corner), Existing(c), false)?;
            Ok([first, second])
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let [first, second] = made;
    let (fillets, report) = doc
        .edit_spatial_sketch(sketch, "Fillet", |e| {
            e.fillet(first, second, &ValueInput::Number(4.0))
                .map(|f| vec![f])
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let f = &fillets[0];
    assert_eq!(report.status.dof, 0, "{:?}", report.status);
    assert!(report.status.redundant.is_empty());
    let [a, corner] = [
        point(&doc, sketch, ends(&doc, sketch, first)[0]),
        point(&doc, sketch, f.corner),
    ];
    let c = point(&doc, sketch, ends(&doc, sketch, second)[1]);
    let center = point(&doc, sketch, f.center);
    let [ta, tb] = f.tangent_points.map(|p| point(&doc, sketch, p));
    let sub = mitcad_vec3::sub;
    let dot = mitcad_vec3::dot;
    // Radius 4 to both tangent points, each radius square to its line, the
    // tangent points on the lines, the centre in their plane.
    assert!((mitcad_vec3::distance(center, ta) - 4.0).abs() <= TOL);
    assert!((mitcad_vec3::distance(center, tb) - 4.0).abs() <= TOL);
    let u1 = mitcad_vec3::unit(sub(corner, a)).unwrap();
    let u2 = mitcad_vec3::unit(sub(c, corner)).unwrap();
    assert!(dot(sub(ta, center), u1).abs() <= TOL);
    assert!(dot(sub(tb, center), u2).abs() <= TOL);
    let n = mitcad_vec3::unit(mitcad_vec3::cross(u1, u2)).unwrap();
    for p in [ta, tb, center] {
        assert!(dot(sub(p, corner), n).abs() <= TOL);
    }
    assert!(mitcad_vec3::norm(mitcad_vec3::cross(sub(ta, a), u1)) <= TOL);
    assert!(mitcad_vec3::norm(mitcad_vec3::cross(sub(c, tb), u2)) <= TOL);
    // Each tangent point 4 / tan(half the angle between the lines) from
    // the corner.
    let half = 0.5 * dot(u1.map(|v| -v), u2).acos();
    let setback = 4.0 / half.tan();
    assert!((mitcad_vec3::distance(corner, ta) - setback).abs() <= TOL);
    assert!((mitcad_vec3::distance(corner, tb) - setback).abs() <= TOL);
}

/// The radius edited through its dimension moves the tangent points and
/// the centre; the corner and the lines' ends stay.
#[test]
fn the_radius_is_edited_through_its_dimension() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let [ab, _, _] = route(&mut doc, sketch);
    let (made, report) = fillet(&mut doc, sketch, &[FilletCorner::Point(ab.end)], 10.0)
        .unwrap_or_else(|e| panic!("{e}"));
    let f = &made[0];
    assert_eq!(report.parameters.len(), 1);
    doc.set_parameter(&report.parameters[0], 5.0).unwrap();
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    for (p, at) in [
        (ab.end, [40.0, 0.0, 0.0]),
        (f.tangent_points[0], [35.0, 0.0, 0.0]),
        (f.tangent_points[1], [40.0, 5.0, 0.0]),
        (f.center, [35.0, 5.0, 0.0]),
    ] {
        let solved = point(&doc, sketch, p);
        assert!(near(solved, at), "p{}: {solved:?}, not {at:?}", p.0);
    }
    // Through the edit command's own step as well.
    let k = f.dimension.unwrap();
    doc.edit_spatial_sketch(sketch, "Edit Dimension", |e| {
        e.set_dimension_value(k, &ValueInput::Number(8.0))
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(near(
        point(&doc, sketch, f.tangent_points[0]),
        [32.0, 0.0, 0.0]
    ));
}

/// Both corners of the route in one edit: the second arc is Equal to the
/// first, BC is trimmed at both ends, and one undo takes both back.
#[test]
fn several_corners_in_one_edit() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let [ab, bc, cd] = route(&mut doc, sketch);
    let before = doc.spatial_sketch_def(sketch).unwrap().clone();
    let (made, report) = fillet(
        &mut doc,
        sketch,
        &[FilletCorner::Point(ab.end), FilletCorner::Point(bc.end)],
        10.0,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 2);
    assert!(made[0].dimension.is_some() && made[1].dimension.is_none());
    let def = doc.spatial_sketch_def(sketch).unwrap();
    assert!(def.constraints.iter().any(|c| c.kind
        == SpatialConstraintKind::Equal {
            a: made[1].arc,
            b: made[0].arc,
        }));
    assert_eq!(made[1].lines, [bc.line, cd.line]);
    assert_eq!(
        ends(&doc, sketch, bc.line),
        [made[0].tangent_points[1], made[1].tangent_points[0]]
    );
    // BC's length moved once, with the first fillet; CD's with the second.
    assert_eq!(made[0].full_lines.len(), 2);
    assert_eq!(made[1].full_lines.len(), 1);
    for (p, at) in [
        (made[1].tangent_points[0], [40.0, 20.0, 0.0]),
        (made[1].tangent_points[1], [40.0, 30.0, 10.0]),
        (made[1].center, [40.0, 20.0, 10.0]),
        (bc.end, [40.0, 30.0, 0.0]),
    ] {
        let solved = point(&doc, sketch, p);
        assert!(near(solved, at), "p{}: {solved:?}, not {at:?}", p.0);
    }
    assert_eq!(report.status.dof, 0, "{:?}", report.status);
    assert!(report.status.redundant.is_empty());
    assert_eq!(doc.undo().as_deref(), Some("Fillet in 3DSketch1"));
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &before);
}

/// The refusals: lines collinear at their corner (straight on and folded
/// back), lines that share no end, a radius that does not fit either line,
/// a point that is no corner of two lines; each leaves the definition, the
/// parameters and the history as they were.
#[test]
fn refused_fillets_change_nothing() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let [ab, bc, cd] = route(&mut doc, sketch);
    // A line straight on from D, and one folded back onto CD.
    let straight = segment(
        &mut doc,
        sketch,
        along(Existing(cd.end), [40.0, 30.0, 35.0], Z, 10.0),
    );
    let lone = segment(
        &mut doc,
        sketch,
        SegmentRequest::new(At([0.0, -20.0, 0.0]), At([10.0, -20.0, 0.0])),
    );
    let folded = segment(
        &mut doc,
        sketch,
        SegmentRequest::new(Existing(lone.end), At([2.0, -20.0, 0.0])),
    );
    let def = doc.spatial_sketch_def(sketch).unwrap().clone();
    let params = doc.parameters().len();
    let undo = doc.undo_label().map(str::to_owned);
    let refused = |doc: &mut Document<MockKernel>, corners: &[FilletCorner], radius: f64| {
        let error = fillet(doc, sketch, corners, radius)
            .map(|_| ())
            .unwrap_err();
        assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &def, "{error}");
        assert_eq!(doc.parameters().len(), params);
        assert_eq!(doc.undo_label().map(str::to_owned), undo);
        error
    };
    let name = |c: EntityUid| c.curve_name();
    let error = refused(
        &mut doc,
        &[FilletCorner::Lines(cd.line, straight.line)],
        2.0,
    );
    assert!(error.contains("are collinear"), "{error}");
    assert!(error.contains(&name(cd.line)), "{error}");
    let error = refused(
        &mut doc,
        &[FilletCorner::Lines(lone.line, folded.line)],
        1.0,
    );
    assert!(error.contains("are collinear"), "{error}");
    let error = refused(&mut doc, &[FilletCorner::Lines(ab.line, cd.line)], 2.0);
    assert!(error.contains("do not share an end"), "{error}");
    // 90 degrees: a radius of 35 takes 35 mm of BC, which is 30 long.
    let error = refused(&mut doc, &[FilletCorner::Lines(ab.line, bc.line)], 35.0);
    assert!(error.contains("does not fit"), "{error}");
    assert!(error.contains(&name(bc.line)), "{error}");
    let error = refused(&mut doc, &[FilletCorner::Point(ab.start)], 2.0);
    assert!(error.contains("is no corner of two lines"), "{error}");
    // A refused second corner takes the first back with it.
    let error = refused(
        &mut doc,
        &[
            FilletCorner::Point(ab.end),
            FilletCorner::Lines(cd.line, straight.line),
        ],
        2.0,
    );
    assert!(error.contains("are collinear"), "{error}");
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
}

/// Undo of a fillet puts the corner back: the lines end at it again, the
/// lengths are theirs, the new points, curves, relations and parameters go;
/// redo rounds it again.
#[test]
fn undo_restores_the_corner() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let [ab, bc, _] = route(&mut doc, sketch);
    let before = doc.spatial_sketch_def(sketch).unwrap().clone();
    let params = doc.parameters().len();
    let (made, _) = fillet(&mut doc, sketch, &[FilletCorner::Point(ab.end)], 10.0)
        .unwrap_or_else(|e| panic!("{e}"));
    let after = doc.spatial_sketch_def(sketch).unwrap().clone();
    assert_eq!(doc.undo().as_deref(), Some("Fillet in 3DSketch1"));
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &before);
    assert_eq!(doc.parameters().len(), params);
    assert_eq!(ends(&doc, sketch, ab.line), [ab.start, ab.end]);
    assert_eq!(ends(&doc, sketch, bc.line), [ab.end, bc.end]);
    assert!(near(point(&doc, sketch, ab.end), [40.0, 0.0, 0.0]));
    assert!(
        doc.spatial_sketch_def(sketch)
            .unwrap()
            .curve(made[0].arc)
            .is_none()
    );
    assert!(doc.redo().is_some());
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &after);
}

fn names(curves: &[EntityUid]) -> Vec<String> {
    curves.iter().map(|c| c.curve_name()).collect()
}

fn pipe_path(doc: &Document<MockKernel>, uid: FeatureUid) -> Vec<String> {
    match &doc.feature(uid).expect("a feature").def {
        FeatureDef::Pipe(pipe) => match &pipe.path {
            CurvePath::Spatial(path) => path.curves.iter().map(|c| c.curve_name()).collect(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

/// The pieces of the last pipe the mock kernel made for `feature`, in
/// order: `c3,c11,c5`.
fn pieces_of(doc: &Document<MockKernel>, feature: FeatureUid) -> String {
    let start = format!("pipe {feature} ");
    let specs = doc.kernel().specs.borrow();
    let spec = specs
        .iter()
        .rev()
        .find(|s| s.starts_with(&start))
        .unwrap_or_else(|| panic!("no spec of {feature}: {specs:?}"));
    let after = spec.split(" path ").nth(1).expect("a path");
    after.split(' ').next().expect("its pieces").to_owned()
}

/// A route through fillets is a Pipe's path: a pipe that lists the lines
/// takes the arcs in the fillet's step, one that chains from AB finds them;
/// both run A, AB, the arc at B, BC, the arc at C, CD.
#[test]
fn a_pipe_runs_through_the_fillets() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let [ab, bc, cd] = route(&mut doc, sketch);
    let listed = doc
        .add_feature(
            &def(json!({"type": "pipe", "size": 4, "operation": "new_body",
                        "path": {"spatial_sketch": sketch, "curves": names(&[ab.line, bc.line, cd.line])}})),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .uid;
    let (made, _) = fillet(
        &mut doc,
        sketch,
        &[FilletCorner::Point(ab.end), FilletCorner::Point(bc.end)],
        10.0,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let (b_arc, c_arc) = (made[0].arc, made[1].arc);
    assert_eq!(
        pipe_path(&doc, listed),
        names(&[ab.line, bc.line, cd.line, b_arc, c_arc])
    );
    let chained = doc
        .add_feature(
            &def(json!({"type": "pipe", "size": 4, "operation": "new_body",
                        "path": {"spatial_sketch": sketch, "curves": [ab.line.curve_name()],
                                 "chain": true}})),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .uid;
    let order = names(&[ab.line, b_arc, bc.line, c_arc, cd.line]).join(",");
    for pipe in [listed, chained] {
        assert_eq!(status(&doc, pipe), FeatureStatus::Ok);
        assert_eq!(pieces_of(&doc, pipe), order);
    }
    // A chain selection stays as it was picked: it finds the arcs itself,
    // and the corners' construction lines are in no chain.
    assert_eq!(pipe_path(&doc, chained), names(&[ab.line]));
    // Undone, the listed pipe lists the lines alone again.
    assert!(doc.undo().is_some()); // the chained pipe
    assert_eq!(doc.undo().as_deref(), Some("Fillet in 3DSketch1"));
    assert_eq!(pipe_path(&doc, listed), names(&[ab.line, bc.line, cd.line]));
    assert_eq!(status(&doc, listed), FeatureStatus::Ok);
    let def = doc.spatial_sketch_def(sketch).unwrap();
    assert!(
        def.curves
            .iter()
            .all(|c| !matches!(c.kind, SpatialCurveKind::Arc { .. }))
    );
}
