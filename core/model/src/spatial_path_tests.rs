// SPDX-License-Identifier: MIT
//! Pipe and Sweep along curves of 3D sketches (mitcad#1523), with the mock
//! kernel, whose pipe and sweep specs name the path's pieces in order: the
//! route of W1 on a turned support driving a Pipe and a Sweep (one
//! reversed), BC at 50 and the support turned further recomputing both
//! with the same ids and direction, saved and read again (W2, W8, W10); a
//! picked subchain of a branching sketch, a construction line picked
//! explicitly and left out of chain selection, joints made by a Coincident
//! relation, a closed loop and a closed spline as paths (mitcad#1550), and
//! the selections that fail (branching, disconnected, near but unjoined
//! ends, a crossing) or are refused (unknown ids, the planar form for a 3D
//! sketch), a sketch edit
//! that breaks a chain failing only the consumer, and the chains the
//! sketch reports (W9); a failed sketch failing the pipe with no last
//! geometry kept, and a failure of the pipe's own geometry leaving the
//! sketch editable; lines, a tangent arc and splines run either way, with
//! W11's edits; and a planar path rewritten into a 3D sketch's path with
//! its ids and direction (for mitcad#1544).

use serde_json::{Value, json};

use crate::features::path::{CurvePath, curve_ends, rewrite_spatial_paths, spatial_rewrite};
use crate::features::spatial_path::{Leg, SpatialPath, chains, pieces, route};
use crate::features::spatial_sketch::{SpatialAxis, SpatialSense, SpatialSource, to_component};
use crate::features::{FeatureDef, SketchPlane, ValueInput};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Curve3;
use crate::sketch::EntityKind;
use crate::sketch::edit::{PointInput, SketchEdit};
use crate::sketch::spatial_edit::{
    ArcPlacement, ArcRequest, SegmentMade, SegmentRequest, SpatialPointInput, SplineRequest,
};
use crate::sweeps::PathCurve;
use crate::test_support::{def, num};
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

/// The message of a failed feature.
fn failed(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("{uid} is {other:?}, not failed"),
    }
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

/// One segment as its own edit, with the parameters it made.
fn segment(
    doc: &mut Document<MockKernel>,
    sketch: FeatureUid,
    request: SegmentRequest,
) -> (SegmentMade, Vec<String>) {
    let (made, report) = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| e.add_segment(&request))
        .unwrap_or_else(|e| panic!("{e}"));
    (made, report.parameters)
}

/// A plain line as its own edit.
fn line(
    doc: &mut Document<MockKernel>,
    sketch: FeatureUid,
    start: SpatialPointInput,
    end: SpatialPointInput,
) -> SegmentMade {
    segment(doc, sketch, SegmentRequest::new(start, end)).0
}

fn names(curves: &[EntityUid]) -> Vec<String> {
    curves.iter().map(|c| c.curve_name()).collect()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None)
        .unwrap_or_else(|e| panic!("{e}"))
        .uid
}

fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    doc.add_feature(&def(value.clone()), None)
        .map(|_| panic!("{value} must be refused"))
        .unwrap_err()
        .to_string()
}

fn pipe(path: Value) -> Value {
    json!({"type": "pipe", "path": path, "size": 4, "operation": "new_body"})
}

/// A path along the curves of a 3D sketch.
fn spatial(sketch: FeatureUid, curves: &[EntityUid]) -> Value {
    json!({"spatial_sketch": sketch, "curves": names(curves)})
}

/// The same with chain selection.
fn chained(sketch: FeatureUid, first: EntityUid) -> Value {
    json!({"spatial_sketch": sketch, "curves": [first.curve_name()], "chain": true})
}

/// The pieces of the last pipe or sweep the mock kernel made for
/// `feature`, in order: `c3,c6,c9`.
fn path_of(doc: &Document<MockKernel>, feature: FeatureUid) -> String {
    let (pipe, sweep) = (format!("pipe {feature} "), format!("sweep {feature} "));
    let specs = doc.kernel().specs.borrow();
    let spec = specs
        .iter()
        .rev()
        .find(|s| s.starts_with(&pipe) || s.starts_with(&sweep))
        .unwrap_or_else(|| panic!("no spec of {feature}: {specs:?}"));
    let after = spec.split(" path ").nth(1).expect("a path");
    after.split(' ').next().expect("its pieces").to_owned()
}

/// A path's pieces as a Pipe takes them, from the sketch's definition and
/// its last output.
fn resolved(doc: &Document<MockKernel>, path: &SpatialPath) -> Result<Vec<PathCurve>, String> {
    let def = doc.spatial_sketch_def(path.sketch).expect("a 3D sketch");
    let output = doc.spatial_sketch(path.sketch).expect("evaluated");
    let legs = route(def, path, "3DSketch1", &|p| output.point(p))?;
    pieces(output, &legs, "3DSketch1")
}

fn ends(piece: &PathCurve) -> ([f64; 3], [f64; 3]) {
    curve_ends(&piece.curve).unwrap_or_else(|| panic!("{piece:?} has no ends"))
}

/// The stored path of a Pipe or a Sweep.
fn stored(doc: &Document<MockKernel>, uid: FeatureUid) -> CurvePath {
    match &doc.feature(uid).expect("a feature").def {
        FeatureDef::Pipe(pipe) => pipe.path.clone(),
        FeatureDef::Sweep(sweep) => sweep.path.clone(),
        other => panic!("{other:?}"),
    }
}

/// W1, W2, W8 and W10 with consumers: the route A-B-C-D on a construction
/// plane turned about X drives Pipe1 (chain selection from AB, A to D) and
/// Sweep1 (CD, BC and AB picked, reversed: D to A). BC at 50 and the plane
/// turned further recompute both with the same ids and direction, their
/// pieces the local route placed by the plane's frame once; saved and read
/// again, the paths are as they were written.
#[test]
fn a_pipe_and_a_sweep_follow_the_route_on_a_turned_support() {
    let mut doc = Document::new(MockKernel::default());
    let tilted = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "angle", "line": "x", "angle": 0.5,
                                       "plane": "xy"}})),
            None,
        )
        .unwrap();
    let sketch = doc
        .add_spatial_sketch(SketchPlane::Construction(tilted.uid))
        .unwrap()
        .uid;
    let (ab, mut params) = segment(
        &mut doc,
        sketch,
        SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            ..along(At([0.0; 3]), [37.0, 2.0, 1.0], X, 40.0)
        },
    );
    let (bc, more) = segment(
        &mut doc,
        sketch,
        along(Existing(ab.end), [42.0, 27.0, 0.0], Y, 30.0),
    );
    params.extend(more);
    let (cd, _) = segment(
        &mut doc,
        sketch,
        along(Existing(bc.end), [40.0, 33.0, 21.0], Z, 20.0),
    );
    let forward = names(&[ab.line, bc.line, cd.line]).join(",");
    let backward = names(&[cd.line, bc.line, ab.line]).join(",");

    let pipe1 = add(&mut doc, pipe(chained(sketch, ab.line)));
    assert_eq!(status(&doc, pipe1), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, pipe1), forward);
    // A square on YZ swept from D back to A.
    let profile = doc.add_sketch(SketchPlane::Yz).unwrap().uid;
    let region = doc
        .add_rectangle(profile, [-2.0, -2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let mut back_path = spatial(sketch, &[cd.line, bc.line, ab.line]);
    back_path["reversed"] = json!(true);
    let sweep1 = add(
        &mut doc,
        json!({"type": "sweep", "profiles": [{"sketch": profile, "region": region}],
               "path": back_path, "operation": "new_body"}),
    );
    assert_eq!(status(&doc, sweep1), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, sweep1), backward);

    // The pieces: the local route placed by the frame the sketch used.
    let check = |doc: &Document<MockKernel>, local: [[f64; 3]; 4]| {
        let frame = doc.spatial_sketch(sketch).unwrap().frame.unwrap();
        let place = |p: [f64; 3]| to_component(&frame, p);
        let along_route = SpatialPath {
            sketch,
            curves: vec![ab.line],
            reversed: false,
            chain: true,
        };
        let pieces = resolved(doc, &along_route).unwrap();
        assert_eq!(pieces.len(), 3);
        for (k, piece) in pieces.iter().enumerate() {
            let (s, e) = ends(piece);
            assert!(
                near(s, place(local[k])) && near(e, place(local[k + 1])),
                "{k}: {piece:?}"
            );
        }
        let back = SpatialPath {
            sketch,
            curves: vec![cd.line, bc.line, ab.line],
            reversed: true,
            chain: false,
        };
        let pieces = resolved(doc, &back).unwrap();
        assert!(near(ends(&pieces[0]).0, place(local[3])));
        assert!(near(ends(&pieces[2]).1, place(local[0])));
    };
    check(
        &doc,
        [
            [0.0, 0.0, 0.0],
            [40.0, 0.0, 0.0],
            [40.0, 30.0, 0.0],
            [40.0, 30.0, 20.0],
        ],
    );

    // W2: BC to 50 recomputes both consumers; same ids and direction.
    let made = (doc.kernel().count("pipe"), doc.kernel().count("sweep"));
    doc.set_parameter(&params[1], 50.0).unwrap();
    assert!(doc.kernel().count("pipe") > made.0 && doc.kernel().count("sweep") > made.1);
    assert_eq!(status(&doc, pipe1), FeatureStatus::Ok);
    assert_eq!(status(&doc, sweep1), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, pipe1), forward);
    assert_eq!(path_of(&doc, sweep1), backward);
    let moved = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 50.0, 0.0],
        [40.0, 50.0, 20.0],
    ];
    check(&doc, moved);

    // W10: the support turned further; the pieces follow it once.
    let before = doc.spatial_sketch(sketch).unwrap().frame.unwrap();
    doc.set_parameter(&tilted.parameters[0], 0.9).unwrap();
    let after = doc.spatial_sketch(sketch).unwrap().frame.unwrap();
    assert!(!near(before.y_axis, after.y_axis));
    assert_eq!(status(&doc, pipe1), FeatureStatus::Ok);
    check(&doc, moved);

    // The paths as written, also saved and read again.
    let expected = json!({"spatial_sketch": sketch, "curves": [ab.line.curve_name()],
                          "chain": true});
    assert_eq!(serde_json::to_value(stored(&doc, pipe1)).unwrap(), expected);
    let file = doc.to_json();
    let mut again = Document::from_json(&file, MockKernel::default()).unwrap();
    again.recompute();
    assert_eq!(stored(&again, pipe1), stored(&doc, pipe1));
    assert_eq!(stored(&again, sweep1), stored(&doc, sweep1));
    assert_eq!(status(&again, pipe1), FeatureStatus::Ok);
    assert_eq!(path_of(&again, pipe1), forward);
    assert_eq!(path_of(&again, sweep1), backward);
}

/// W9 and the selections: in a sketch where three lines meet at B, a
/// construction line goes on from D, one line starts at a point of its own
/// where D is, one crosses BC, two lines are joined by a Coincident
/// relation, three close a triangle and a fit-point spline is closed.
#[test]
fn subchains_construction_and_broken_selections() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ab = line(&mut doc, sketch, At([0.0; 3]), At([10.0, 0.0, 0.0]));
    let bc = line(&mut doc, sketch, Existing(ab.end), At([10.0, 10.0, 0.0]));
    let cd = line(&mut doc, sketch, Existing(bc.end), At([10.0, 10.0, 10.0]));
    let be = line(&mut doc, sketch, Existing(ab.end), At([10.0, -10.0, 0.0]));
    let (df, _) = segment(
        &mut doc,
        sketch,
        SegmentRequest {
            construction: true,
            ..SegmentRequest::new(Existing(cd.end), At([20.0, 10.0, 10.0]))
        },
    );
    let near_d = line(
        &mut doc,
        sketch,
        At([10.0, 10.0, 10.0]),
        At([10.0, 20.0, 10.0]),
    );
    let across = line(&mut doc, sketch, At([0.0, 5.0, 0.0]), At([20.0, 5.0, 0.0]));
    let gh = line(&mut doc, sketch, At([30.0, 0.0, 0.0]), At([40.0, 0.0, 0.0]));
    let hi = line(
        &mut doc,
        sketch,
        At([40.0, 0.0, 0.0]),
        At([40.0, 10.0, 0.0]),
    );
    let (joint, _) = doc
        .edit_spatial_sketch(sketch, "Coincident", |e| e.coincident(gh.end, hi.start))
        .unwrap();
    let t1 = line(&mut doc, sketch, At([50.0, 0.0, 0.0]), At([60.0, 0.0, 0.0]));
    let t2 = line(&mut doc, sketch, Existing(t1.end), At([55.0, 5.0, 0.0]));
    line(&mut doc, sketch, Existing(t2.end), Existing(t1.start));
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

    // A subchain picked out of the branching sketch: A to D.
    let sub = add(
        &mut doc,
        pipe(spatial(sketch, &[bc.line, ab.line, cd.line])),
    );
    assert_eq!(status(&doc, sub), FeatureStatus::Ok);
    assert_eq!(
        path_of(&doc, sub),
        names(&[ab.line, bc.line, cd.line]).join(",")
    );
    // Chain selection from CD reaches B, where three lines meet; the
    // construction line from D is not taken.
    let branched = add(&mut doc, pipe(chained(sketch, cd.line)));
    let message = failed(&doc, branched);
    assert!(
        message.contains(&format!("branches at p{} (10.000000, 0.000000", ab.end.0)),
        "{message}"
    );
    assert!(message.contains(&be.line.curve_name()), "{message}");
    assert!(!message.contains(&df.line.curve_name()), "{message}");
    // The construction line picked explicitly is a piece.
    let explicit = add(&mut doc, pipe(spatial(sketch, &[cd.line, df.line])));
    assert_eq!(status(&doc, explicit), FeatureStatus::Ok);
    assert_eq!(
        path_of(&doc, explicit),
        names(&[cd.line, df.line]).join(",")
    );
    // A gap, ends near but not joined, and a crossing: no joints.
    for curves in [
        [ab.line, gh.line],
        [cd.line, near_d.line],
        [bc.line, across.line],
    ] {
        let uid = add(&mut doc, pipe(spatial(sketch, &curves)));
        let message = failed(&doc, uid);
        assert!(
            message.contains(&format!(
                "{} does not join the others",
                curves[1].curve_name()
            )),
            "{message}"
        );
    }
    let alone = add(&mut doc, pipe(chained(sketch, across.line)));
    assert_eq!(path_of(&doc, alone), across.line.curve_name());
    // Joined by the Coincident relation: picked, and chained from HI.
    let joined = add(&mut doc, pipe(spatial(sketch, &[gh.line, hi.line])));
    assert_eq!(status(&doc, joined), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, joined), names(&[gh.line, hi.line]).join(","));
    let from_hi = add(&mut doc, pipe(chained(sketch, hi.line)));
    assert_eq!(path_of(&doc, from_hi), names(&[gh.line, hi.line]).join(","));
    // A closed loop and a closed spline: closed paths (mitcad#1550).
    let looped = add(&mut doc, pipe(chained(sketch, t1.line)));
    assert_eq!(status(&doc, looped), FeatureStatus::Ok);
    let closed = add(&mut doc, pipe(spatial(sketch, &[ring.spline])));
    assert_eq!(status(&doc, closed), FeatureStatus::Ok);

    // Refused when added: ids it does not have, none, one twice, the
    // planar form, and a planar sketch in the 3D form.
    let error = refused(&mut doc, pipe(spatial(sketch, &[EntityUid(99)])));
    assert!(error.contains("has no curve c99"), "{error}");
    let error = refused(&mut doc, pipe(spatial(sketch, &[])));
    assert!(error.contains("no curves selected"), "{error}");
    let error = refused(&mut doc, pipe(spatial(sketch, &[ab.line, ab.line])));
    assert!(error.contains("listed more than once"), "{error}");
    let error = refused(
        &mut doc,
        pipe(json!({"sketch": sketch, "curves": [ab.line.curve_name()]})),
    );
    assert!(error.contains("\"spatial_sketch\""), "{error}");
    let planar = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let error = refused(&mut doc, pipe(spatial(planar, &[ab.line])));
    assert!(error.contains("is not a 3D sketch"), "{error}");
    let unknown = json!({"spatial_sketch": sketch, "curves": ["c3"], "curve": "c3"});
    assert!(serde_json::from_value::<CurvePath>(unknown).is_err());

    // The chains chain selection finds: the branching one, the lone and
    // the joined lines, the triangle and the closed spline; the
    // construction line is none.
    let found = chains(doc.spatial_sketch_def(sketch).unwrap());
    assert_eq!(found.len(), 6, "{found:?}");
    assert_eq!(found[0].branches, [ab.end]);
    assert!(found[0].path(sketch).is_none());
    let pair = found
        .iter()
        .find(|c| c.legs[0].curve == gh.line)
        .expect("the joined pair");
    assert_eq!(
        pair.legs,
        [
            Leg {
                curve: gh.line,
                forward: true
            },
            Leg {
                curve: hi.line,
                forward: true
            }
        ]
    );
    assert_eq!(
        pair.path(sketch),
        Some(SpatialPath {
            sketch,
            curves: vec![gh.line, hi.line],
            reversed: false,
            chain: false,
        })
    );
    assert!(found.iter().any(|c| c.closed && c.legs.len() == 3));
    assert!(
        found
            .iter()
            .any(|c| c.closed && c.legs.len() == 1 && c.legs[0].curve == ring.spline)
    );

    // An edit that breaks a chain is taken and fails the pipe that picks
    // both curves (chain selection now finds HI alone); one that deletes a
    // curve a pipe names is refused naming it.
    doc.edit_spatial_sketch(sketch, "Delete", |e| e.remove_relation(joint))
        .unwrap();
    assert!(failed(&doc, joined).contains("does not join the others"));
    assert_eq!(path_of(&doc, from_hi), hi.line.curve_name());
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    doc.undo();
    assert_eq!(status(&doc, joined), FeatureStatus::Ok);
    let error = doc
        .edit_spatial_sketch(sketch, "Delete", |e| e.delete_curve(gh.line))
        .unwrap_err();
    assert!(
        error.to_string().contains("would no longer work"),
        "{error}"
    );
}

/// A sketch that fails (its support suppressed) fails the pipe along it,
/// which keeps no last geometry; a failure of the pipe's own geometry
/// leaves the sketch solved and editable.
#[test]
fn a_failed_sketch_or_pipe_fails_the_consumer_only() {
    let mut doc = Document::new(MockKernel::default());
    let plane = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": "xy", "distance": 5}})),
            None,
        )
        .unwrap();
    let sketch = doc
        .add_spatial_sketch(SketchPlane::Construction(plane.uid))
        .unwrap()
        .uid;
    let ab = line(&mut doc, sketch, At([0.0; 3]), At([30.0, 0.0, 0.0]));
    let bc = line(&mut doc, sketch, Existing(ab.end), At([30.0, 20.0, 10.0]));
    let tube = add(&mut doc, pipe(chained(sketch, ab.line)));
    assert_eq!(status(&doc, tube), FeatureStatus::Ok);

    doc.set_suppressed(plane.uid, true).unwrap();
    assert!(matches!(status(&doc, sketch), FeatureStatus::Failed(_)));
    assert!(matches!(status(&doc, tube), FeatureStatus::Failed(_)));
    doc.set_suppressed(plane.uid, false).unwrap();
    assert_eq!(status(&doc, tube), FeatureStatus::Ok);

    doc.kernel().fail.borrow_mut().insert("pipe");
    doc.edit_spatial_sketch(sketch, "Add Line", |e| {
        e.add_segment(&SegmentRequest::new(
            Existing(bc.end),
            At([30.0, 40.0, 10.0]),
        ))
    })
    .unwrap();
    assert!(failed(&doc, tube).contains("pipe failed"));
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    doc.kernel().fail.borrow_mut().clear();
    doc.edit_spatial_sketch(sketch, "Add Line", |e| {
        e.add_segment(&SegmentRequest::new(
            At([90.0, 0.0, 0.0]),
            At([95.0, 0.0, 0.0]),
        ))
    })
    .unwrap();
    assert_eq!(status(&doc, tube), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, tube).split(',').count(), 3);
}

/// W11 and W17 as paths: a line, a tangent arc, a control-point spline and
/// an open fit-point spline joined end to end, picked by chain selection
/// from the line (A to the end), and from the fit-point spline reversed
/// (its end to A): every piece turned round, the arc on its own circle.
/// AB to 60 and the radius to 12 keep the ids, the order and the
/// direction.
#[test]
fn lines_arcs_and_splines_run_either_way() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let (ab, ab_params) = segment(
        &mut doc,
        sketch,
        SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            ..along(At([0.0; 3]), [38.0, 1.0, 0.0], X, 40.0)
        },
    );
    let (bc, report) = doc
        .edit_spatial_sketch(sketch, "Add Arc", |e| {
            e.add_arc(&ArcRequest {
                radius: Some(ValueInput::Number(10.0)),
                ..ArcRequest::new(
                    Existing(ab.end),
                    At([50.0, 10.0, 0.0]),
                    ArcPlacement::Tangent { curve: ab.line },
                )
            })
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let radius = report.parameters[0].clone();
    let (control, _) = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            e.add_spline(&SplineRequest::control(
                vec![
                    Existing(bc.end),
                    At([50.0, 30.0, 0.0]),
                    At([50.0, 30.0, 20.0]),
                ],
                2,
            ))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let last = control.points[2];
    let (fitted, _) = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            e.add_spline(&SplineRequest::fit(vec![
                Existing(last),
                At([60.0, 40.0, 25.0]),
                At([70.0, 40.0, 30.0]),
            ]))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let curves = [ab.line, bc.arc, control.spline, fitted.spline];
    let forward = names(&curves).join(",");
    let mut turned = curves;
    turned.reverse();
    let backward = names(&turned).join(",");

    let ahead = add(&mut doc, pipe(chained(sketch, ab.line)));
    assert_eq!(status(&doc, ahead), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, ahead), forward);
    let back_path = SpatialPath {
        sketch,
        curves: vec![fitted.spline],
        reversed: true,
        chain: true,
    };
    let back = add(
        &mut doc,
        pipe(serde_json::to_value(CurvePath::Spatial(back_path.clone())).unwrap()),
    );
    assert_eq!(status(&doc, back), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, back), backward);
    let def = doc.spatial_sketch_def(sketch).unwrap();
    let output = doc.spatial_sketch(sketch).unwrap();
    let legs = route(def, &back_path, "3DSketch1", &|p| output.point(p)).unwrap();
    assert!(legs.iter().all(|leg| !leg.forward), "{legs:?}");

    // B is held; the arc's end C and its centre are where the solve put
    // them (nothing holds the arc's sweep or the turn of its normal about
    // AB, so C slides on its circle when AB and the radius change).
    let check = |doc: &Document<MockKernel>, b: [f64; 3], r: f64| {
        let output = doc.spatial_sketch(sketch).unwrap();
        let end = output.point(fitted.points[2]).unwrap();
        let c = output.point(bc.end).unwrap();
        let centre = output.point(bc.center).unwrap();
        assert!((mitcad_vec3::distance(c, centre) - r).abs() <= TOL);
        let pieces = resolved(doc, &back_path).unwrap();
        assert_eq!(pieces.len(), 4);
        assert!(near(ends(&pieces[0]).0, end), "{:?}", pieces[0]);
        assert!(near(ends(&pieces[1]).1, c), "{:?}", pieces[1]);
        assert!(near(ends(&pieces[2]).0, c) && near(ends(&pieces[2]).1, b));
        assert!(near(ends(&pieces[3]).0, b) && near(ends(&pieces[3]).1, [0.0; 3]));
        for pair in pieces.windows(2) {
            assert!(near(ends(&pair[0]).1, ends(&pair[1]).0), "{pair:?}");
        }
        // The arc turned round is still its circle.
        match &pieces[2].curve {
            Curve3::Conic {
                center,
                major,
                minor,
                closed: false,
                ..
            } => {
                assert!(near(*center, centre), "{center:?}");
                assert!((major - r).abs() <= TOL && (minor - r).abs() <= TOL);
            }
            other => panic!("{other:?}"),
        }
        // The control-point spline turned round: its poles the other way.
        match &pieces[1].curve {
            Curve3::BSpline { poles, .. } => {
                assert!(near(poles[0], output.point(last).unwrap()));
                assert!(near(poles[2], c));
            }
            other => panic!("{other:?}"),
        }
    };
    check(&doc, [40.0, 0.0, 0.0], 10.0);

    // W11's edits: AB to 60, the radius to 12.
    doc.set_parameter(&ab_params[0], 60.0).unwrap();
    doc.set_parameter(&radius, 12.0).unwrap();
    for uid in [sketch, ahead, back] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    }
    assert_eq!(path_of(&doc, ahead), forward);
    assert_eq!(path_of(&doc, back), backward);
    check(&doc, [60.0, 0.0, 0.0], 12.0);
}

/// The rewrite a conversion (mitcad#1544) makes of a planar path: Pipe1
/// runs from the free end of the planar sketch's second line (listed
/// first, chained) to the start of its first; the rewritten path names the
/// same curves, chained, reversed. Along a 3D sketch with the same ids and
/// points it runs the same way through the same places. A path round a
/// circle is rewritten too, closed (mitcad#1550).
#[test]
fn a_planar_path_is_rewritten_with_its_ids_and_direction() {
    fn ends_of(e: &SketchEdit<'_>, id: EntityUid) -> Result<(EntityUid, EntityUid), String> {
        match e.def.entity(id).map(|x| &x.kind) {
            Some(EntityKind::Line { start, end, .. }) => Ok((*start, *end)),
            _ => Err("a line".to_owned()),
        }
    }
    let mut doc = Document::new(MockKernel::default());
    let planar = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let ((first, second, points), _) = doc
        .edit_sketch(planar, "Add Line", |e| {
            let first = e.add_line(
                PointInput::At([0.0, 0.0]),
                PointInput::At([40.0, 0.0]),
                false,
                false,
            )?;
            let (a, b) = ends_of(e, first)?;
            let second = e.add_line(
                PointInput::Existing(b),
                PointInput::At([40.0, 30.0]),
                false,
                false,
            )?;
            let (_, c) = ends_of(e, second)?;
            Ok((first, second, [a, b, c]))
        })
        .unwrap();
    let planar_path = json!({"sketch": planar, "curves": [second.curve_name()], "chain": true});
    let tube = add(&mut doc, pipe(planar_path));
    assert_eq!(status(&doc, tube), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, tube), names(&[second, first]).join(","));

    let sketch_def = match &doc.feature(planar).unwrap().def {
        FeatureDef::Sketch(def) => def.clone(),
        other => panic!("{other:?}"),
    };
    let output = doc.sketch_output(planar).unwrap().clone();
    let rewritten = spatial_rewrite(&stored(&doc, tube), planar, &sketch_def, &output)
        .expect("along the sketch")
        .unwrap();
    assert_eq!(
        rewritten,
        SpatialPath {
            sketch: planar,
            curves: vec![second],
            reversed: true,
            chain: true,
        }
    );
    let mut tube_def = doc.feature(tube).unwrap().def.clone();
    assert_eq!(
        rewrite_spatial_paths(&mut tube_def, planar, &sketch_def, &output),
        Ok(true)
    );
    let FeatureDef::Pipe(rewritten_pipe) = &tube_def else {
        panic!("{tube_def:?}")
    };
    assert_eq!(rewritten_pipe.path, CurvePath::Spatial(rewritten.clone()));

    // A 3D sketch with the planar sketch's ids and points.
    let p = |id: EntityUid| format!("p{}", id.0);
    let spatial_sketch = add(
        &mut doc,
        json!({"type": "spatial_sketch", "support": "xy",
               "points": [{"id": p(points[0]), "at": [0, 0, 0]},
                          {"id": p(points[1]), "at": [40, 0, 0]},
                          {"id": p(points[2]), "at": [40, 30, 0]}],
               "curves": [{"id": first.curve_name(), "type": "line",
                           "start": p(points[0]), "end": p(points[1])},
                          {"id": second.curve_name(), "type": "line",
                           "start": p(points[1]), "end": p(points[2])}]}),
    );
    let along_3d = SpatialPath {
        sketch: spatial_sketch,
        ..rewritten
    };
    let converted = add(
        &mut doc,
        pipe(serde_json::to_value(CurvePath::Spatial(along_3d.clone())).unwrap()),
    );
    assert_eq!(status(&doc, converted), FeatureStatus::Ok);
    assert_eq!(path_of(&doc, converted), path_of(&doc, tube));
    let pieces = resolved(&doc, &along_3d).unwrap();
    assert!(near(ends(&pieces[0]).0, [40.0, 30.0, 0.0]));
    assert!(near(ends(&pieces[0]).1, [40.0, 0.0, 0.0]));
    assert!(near(ends(&pieces[1]).1, [0.0, 0.0, 0.0]));

    // Round a circle: a closed path of the 3D sketch, along the circle's
    // own direction (mitcad#1550).
    let round = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let circle = doc.add_circle(round, [0.0, 0.0], &num(20.0)).unwrap();
    let ring = add(
        &mut doc,
        pipe(json!({"sketch": round, "curves": names(&circle.curves)})),
    );
    let round_def = match &doc.feature(round).unwrap().def {
        FeatureDef::Sketch(def) => def.clone(),
        other => panic!("{other:?}"),
    };
    let round_output = doc.sketch_output(round).unwrap().clone();
    let around = spatial_rewrite(&stored(&doc, ring), round, &round_def, &round_output)
        .expect("along the sketch")
        .unwrap();
    assert_eq!(around.curves, circle.curves);
    assert!(!around.reversed);
    // A path along another sketch is not rewritten.
    assert!(spatial_rewrite(&stored(&doc, ring), planar, &sketch_def, &output).is_none());
}
