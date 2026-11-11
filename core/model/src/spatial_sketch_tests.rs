// SPDX-License-Identifier: MIT
//! 3D sketches in the model (mitcad#1521), with the mock kernel: the
//! reference route and its dimension edits (W1-W3), attachments to a
//! vertex that moves, is renamed or is lost, later and other components'
//! vertices refused (W4), refused edits leaving everything as it was (W5:
//! definition, parameters, ids and history across separate parts of the
//! sketch), Unfix versus Detach, a moved and turned support applied once
//! (W10), drags, and a closed loop in the sketch plane that bounds a region
//! an Extrude takes, unlike one out of the plane (W9, mitcad#1550); the bent route
//! of tangent arcs (W11), a fit-point spline with a tangent end (W17's
//! model part), invalid arcs and splines refused as whole edits, deleting a
//! fit-point spline, and each curve kind as one model curve under a turned
//! support.

use std::f64::consts::{FRAC_PI_2, TAU};

use serde_json::json;

use crate::datum::Datum;
use crate::features::spatial_sketch::{
    SpatialAxis, SpatialConstraintKind, SpatialDimensionKind, SpatialFailureKind, SpatialSense,
    SpatialSource, direction_to_component, to_component,
};
use crate::features::{SketchPlane, ValueInput};
use crate::ids::{ComponentUid, EntityUid, FeatureUid};
use crate::kernel::Curve3;
use crate::profile::SketchFrame;
use crate::sketch::spatial_edit::{
    ArcPlacement, ArcRequest, SegmentMade, SegmentRequest, SpatialPointInput, SplineRequest,
};
use crate::test_support::{block, def, side};
use crate::testing::MockKernel;
use crate::topo::{FaceName, RegionKey, VertexName};
use crate::transform::Transform;
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

/// Adds one segment as its own edit; returns what it made and the
/// parameters it created.
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

struct Route {
    points: [EntityUid; 4],
    lines: [EntityUid; 3],
    /// The lengths' parameters, AB, BC, CD.
    params: Vec<String>,
    /// The lengths' ids.
    lengths: Vec<crate::sketch::ConstraintUid>,
}

/// W1: A at the origin, then 40 along +X, 30 along +Y and 20 along +Z.
fn route(doc: &mut Document<MockKernel>, sketch: FeatureUid) -> Route {
    let (ab, mut params) = segment(
        doc,
        sketch,
        SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            ..along(At([0.0; 3]), [37.0, 2.0, 1.0], X, 40.0)
        },
    );
    let (bc, more) = segment(
        doc,
        sketch,
        along(Existing(ab.end), [42.0, 27.0, 0.0], Y, 30.0),
    );
    params.extend(more);
    let (cd, more) = segment(
        doc,
        sketch,
        along(Existing(bc.end), [40.0, 33.0, 21.0], Z, 20.0),
    );
    params.extend(more);
    assert_eq!(bc.start, ab.end, "a continued route shares its point");
    assert_eq!(cd.start, bc.end);
    Route {
        points: [ab.start, ab.end, bc.end, cd.end],
        lines: [ab.line, bc.line, cd.line],
        params,
        lengths: vec![ab.dimensions[0], bc.dimensions[0], cd.dimensions[0]],
    }
}

fn point(doc: &Document<MockKernel>, sketch: FeatureUid, id: EntityUid) -> [f64; 3] {
    doc.spatial_sketch(sketch)
        .and_then(|o| o.point(id))
        .unwrap_or_else(|| panic!("p{} is not solved", id.0))
}

/// W1-W3: the route solves fully constrained; BC at 50 moves C and D and
/// keeps every id; without BC's length C and D share one free motion.
#[test]
fn the_reference_route_and_its_dimension_edits() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let r = route(&mut doc, sketch);
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    let output = doc.spatial_sketch(sketch).unwrap();
    assert!(output.is_solved());
    let solved = output.status().unwrap();
    assert_eq!(solved.dof, 0);
    assert!(solved.fully_constrained);
    assert_eq!(solved.determined_points.len(), 4);
    let expected = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 30.0, 0.0],
        [40.0, 30.0, 20.0],
    ];
    for (p, at) in r.points.iter().zip(expected) {
        assert!(near(point(&doc, sketch, *p), at), "p{}", p.0);
    }
    // The stored positions are the solved ones (the next solve's start).
    let def = doc.spatial_sketch_def(sketch).unwrap().clone();
    assert!(near(def.position(r.points[3]).unwrap(), expected[3]));
    assert_eq!(def.points.len(), 4);
    assert_eq!(def.curves.len(), 3);

    // W2: BC to 50 through its parameter.
    doc.set_parameter(&r.params[1], 50.0).unwrap();
    assert!(near(point(&doc, sketch, r.points[2]), [40.0, 50.0, 0.0]));
    assert!(near(point(&doc, sketch, r.points[3]), [40.0, 50.0, 20.0]));
    let after = doc.spatial_sketch_def(sketch).unwrap();
    let ids = |d: &crate::features::SpatialSketchDef| -> Vec<EntityUid> {
        d.curves.iter().map(|c| c.id).collect()
    };
    assert_eq!(ids(after), ids(&def));
    // And back to 30 through an edit of the dimension: one undo step.
    doc.edit_spatial_sketch(sketch, "Edit Dimension", |e| {
        e.set_dimension_value(r.lengths[1], &ValueInput::Number(30.0))
    })
    .unwrap();
    assert!(near(point(&doc, sketch, r.points[3]), expected[3]));
    assert_eq!(doc.undo().as_deref(), Some("Edit Dimension in 3DSketch1"));
    assert!(near(point(&doc, sketch, r.points[3]), [40.0, 50.0, 20.0]));
    doc.redo();

    // W3: without BC's length, one freedom: C and D move together along Y.
    let ((), report) = doc
        .edit_spatial_sketch(sketch, "Delete Dimension", |e| {
            e.remove_relation(r.lengths[1])
        })
        .unwrap();
    assert_eq!(report.status.dof, 1);
    assert!(!report.status.fully_constrained);
    let c = point(&doc, sketch, r.points[2]);
    let d = point(&doc, sketch, r.points[3]);
    assert!((c[0] - 40.0).abs() <= TOL && c[2].abs() <= TOL, "{c:?}");
    assert!(near(d, [40.0, c[1], 20.0]), "{d:?}");
    // Its parameter went with it.
    assert!(doc.parameters().find(&r.params[1]).is_none());
}

/// W5 and the whole-edit rollback: an edit that adds a separate segment
/// with its own length (another part of the sketch, a new parameter, new
/// ids) and a length that contradicts AB is refused as a whole; a zero
/// length segment and a non-finite point are refused before any solve; a
/// duplicate length is kept as redundant.
#[test]
fn a_refused_edit_leaves_everything_as_it_was() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let r = route(&mut doc, sketch);
    let before = doc.spatial_sketch_def(sketch).unwrap().clone();
    let params = doc.parameters().clone();
    let error = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| {
            e.add_segment(&along(At([100.0, 0.0, 0.0]), [115.0, 0.0, 0.0], X, 15.0))?;
            e.add_dimension(
                SpatialDimensionKind::Length { line: r.lines[0] },
                Some(&ValueInput::Number(41.0)),
                false,
            )
        })
        .unwrap_err();
    let failure = error.failure.expect("the solve refused it");
    assert_eq!(failure.kind, SpatialFailureKind::Conflict, "{failure}");
    assert!(
        failure
            .constraints
            .iter()
            .any(|k| *k == r.lengths[0] || k.0 > r.lengths[2].0),
        "{failure:?}"
    );
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &before);
    assert_eq!(doc.parameters(), &params);
    assert_eq!(
        doc.spatial_sketch_def(sketch).unwrap().next_entity(),
        before.next_entity()
    );
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    // The last step in the history is still the route's last segment.
    assert_eq!(doc.undo().as_deref(), Some("Add Line in 3DSketch1"));
    doc.redo();

    // A segment without length and a point that is not finite.
    for request in [
        SegmentRequest::new(At([5.0, 5.0, 5.0]), At([5.0, 5.0, 5.0])),
        along(Existing(r.points[3]), [40.0, 30.0, 30.0], Z, 0.0),
    ] {
        let error = doc
            .edit_spatial_sketch(sketch, "Add Line", |e| e.add_segment(&request))
            .unwrap_err();
        assert!(error.to_string().contains("no length"), "{error}");
    }
    assert!(
        doc.edit_spatial_sketch(sketch, "Add Point", |e| e.add_point([
            0.0,
            f64::INFINITY,
            0.0
        ]))
        .is_err()
    );
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &before);

    // The same length again is consistent: kept, and said.
    let (twin, report) = doc
        .edit_spatial_sketch(sketch, "Add Dimension", |e| {
            e.add_dimension(
                SpatialDimensionKind::Length { line: r.lines[0] },
                Some(&ValueInput::Number(40.0)),
                false,
            )
        })
        .unwrap();
    assert_eq!(report.status.redundant, [twin]);
    assert_eq!(report.status.dof, 0);
    assert!(
        doc.warnings(sketch).iter().any(|w| w.contains("redundant")),
        "{:?}",
        doc.warnings(sketch)
    );
}

/// The block's corner at (0, 0, 20): the faces of c1 and c4 and the top.
fn corner_vertex(sides: [FaceName; 2], top: FaceName) -> String {
    let [a, b] = sides;
    VertexName::new([a, b, top]).unwrap().to_string()
}

fn top(region: &RegionKey) -> FaceName {
    FaceName::end(FeatureUid(2), region.clone())
}

/// The face of the body whose name starts with `prefix`.
fn face(doc: &Document<MockKernel>, prefix: &str) -> FaceName {
    let body = crate::ids::BodyUid::new(FeatureUid(2), 0);
    let found: Vec<FaceName> = doc
        .body_shape(body)
        .expect("the block")
        .faces
        .iter()
        .filter(|f| f.to_string().starts_with(prefix))
        .cloned()
        .collect();
    assert_eq!(found.len(), 1, "{prefix}: {found:?}");
    found[0].clone()
}

/// Rounds the block's corner of c1 and c2 in its sketch (F1).
fn round_corner(doc: &mut Document<MockKernel>) {
    doc.edit_sketch(FeatureUid(1), "Fillet", |e| {
        e.fillet(EntityUid(1), EntityUid(2), &ValueInput::Number(3.0))
    })
    .unwrap();
}

/// W4: a point attached to an earlier vertex of the same component
/// follows it when it moves and when a sketch change renames it; an
/// incompatible length and a lost source fail the sketch, keeping its
/// definition and showing no geometry; Detach frees the point where it is.
#[test]
fn a_vertex_attachment_follows_its_source_and_fails_without_it() {
    let mut b = block();
    let vertex: VertexName = corner_vertex(
        [side(b.extrude, 1, 0), side(b.extrude, 1, 3)],
        top(&b.region),
    )
    .parse()
    .unwrap();
    b.doc
        .kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.to_string(), [0.0, 0.0, 20.0]);
    let doc = &mut b.doc;
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let source = SpatialSource::Vertex {
        body: b.body,
        vertex: vertex.clone(),
        link: None,
    };
    let (ab, _) = segment(
        doc,
        sketch,
        SegmentRequest {
            start_source: Some(source.clone()),
            ..along(At([1.0, 1.0, 19.0]), [11.0, 1.0, 19.0], X, 10.0)
        },
    );
    assert_eq!(status(doc, sketch), FeatureStatus::Ok);
    assert!(near(point(doc, sketch, ab.start), [0.0, 0.0, 20.0]));
    assert!(near(point(doc, sketch, ab.end), [10.0, 0.0, 20.0]));

    // The block 30 high: the vertex moves up, and the route with it.
    b.doc
        .kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.to_string(), [0.0, 0.0, 30.0]);
    let doc = &mut b.doc;
    let height = doc.feature(b.extrude).unwrap().def.params()[0];
    let height = doc.parameters().name(height);
    doc.set_parameter(&height, 30.0).unwrap();
    assert_eq!(status(doc, sketch), FeatureStatus::Ok);
    assert!(near(point(doc, sketch, ab.end), [10.0, 0.0, 30.0]));

    // A point at the origin 30 away along a line of length 30 agrees ...
    let (far, _) = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| {
            let origin = e.add_point([0.0, 0.0, 0.0])?;
            e.attach(origin, SpatialSource::Origin)?;
            let line = e.add_line(Existing(ab.start), Existing(origin), false)?;
            e.add_dimension(
                SpatialDimensionKind::Length { line },
                Some(&ValueInput::Number(30.0)),
                false,
            )
        })
        .unwrap();
    // ... until the source moves to 40: the sketch fails, keeps its
    // definition, and its output has the conflict and no geometry.
    b.doc
        .kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.to_string(), [0.0, 0.0, 40.0]);
    let doc = &mut b.doc;
    let kept = doc.spatial_sketch_def(sketch).unwrap().clone();
    doc.set_parameter(&height, 40.0).unwrap();
    assert!(matches!(status(doc, sketch), FeatureStatus::Failed(_)));
    let output = doc.spatial_sketch(sketch).unwrap();
    let failure = output.failure.as_ref().expect("failed");
    assert_eq!(failure.kind, SpatialFailureKind::Conflict, "{failure}");
    assert!(failure.constraints.contains(&far), "{failure:?}");
    assert!(output.solved.is_none() && output.point(ab.end).is_none());
    assert!(output.frame.is_some());
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &kept);
    // Without that length it solves again.
    doc.edit_spatial_sketch(sketch, "Delete Dimension", |e| e.remove_relation(far))
        .unwrap();
    assert_eq!(status(doc, sketch), FeatureStatus::Ok);
    assert!(near(point(doc, sketch, ab.end), [10.0, 0.0, 40.0]));

    // The source lost (its feature suppressed): a reference failure.
    doc.set_suppressed(b.extrude, true).unwrap();
    match status(doc, sketch) {
        FeatureStatus::Failed(message) => assert!(message.contains("attached to"), "{message}"),
        other => panic!("{other:?}"),
    }
    let output = doc.spatial_sketch(sketch).unwrap();
    let failure = output.failure.as_ref().unwrap();
    assert_eq!(failure.kind, SpatialFailureKind::Reference);
    assert_eq!(failure.entities, [ab.start]);
    assert!(output.solved.is_none());
    // Detach while it is lost: the point stays where it was, free.
    let (detached, _) = doc
        .edit_spatial_sketch(sketch, "Detach", |e| e.detach(ab.start))
        .unwrap();
    assert_eq!(detached.len(), 1);
    assert_eq!(status(doc, sketch), FeatureStatus::Ok);
    assert!(near(point(doc, sketch, ab.start), [0.0, 0.0, 40.0]));
    doc.set_suppressed(b.extrude, false).unwrap();
    b.doc
        .kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.to_string(), [0.0, 0.0, 20.0]);
    b.doc.set_parameter(&height, 20.0).unwrap();
    assert!(near(point(&b.doc, sketch, ab.start), [0.0, 0.0, 40.0]));
}

/// W4: a vertex a sketch change renamed is found again when that is
/// unambiguous, with a warning; the definition keeps the stored name.
#[test]
fn a_renamed_vertex_is_found_again() {
    // The names after the change, from a twin of the design.
    let mut twin = block();
    round_corner(&mut twin.doc);
    let renamed = corner_vertex(
        [face(&twin.doc, "F2:side(c1["), side(twin.extrude, 1, 3)],
        face(&twin.doc, "F2:end("),
    );

    let mut b = block();
    let stored = corner_vertex(
        [side(b.extrude, 1, 0), side(b.extrude, 1, 3)],
        top(&b.region),
    );
    assert_ne!(stored, renamed);
    for name in [&stored, &renamed] {
        b.doc
            .kernel()
            .vertex_points
            .borrow_mut()
            .insert(name.clone(), [0.0, 0.0, 20.0]);
    }
    let doc = &mut b.doc;
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let p = doc
        .edit_spatial_sketch(sketch, "Add Point", |e| {
            let p = e.add_point([1.0, 2.0, 3.0])?;
            e.attach(
                p,
                SpatialSource::Vertex {
                    body: b.body,
                    vertex: stored.parse().unwrap(),
                    link: None,
                },
            )?;
            Ok(p)
        })
        .unwrap()
        .0;
    assert!(near(point(doc, sketch, p), [0.0, 0.0, 20.0]));
    assert!(doc.warnings(sketch).is_empty());
    round_corner(doc);
    assert_eq!(status(doc, sketch), FeatureStatus::Ok);
    let warnings = doc.warnings(sketch);
    assert!(
        warnings.iter().any(|w| w.contains("found as")),
        "{warnings:?}"
    );
    assert!(near(point(doc, sketch, p), [0.0, 0.0, 20.0]));
    let def = doc.spatial_sketch_def(sketch).unwrap();
    let names: Vec<String> = def
        .attachments()
        .map(|(_, _, s)| match s {
            SpatialSource::Vertex { vertex, .. } => vertex.to_string(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(names, [stored]);
}

/// W4: vertices of later features and of other components are refused,
/// and the sketch stays as it was.
#[test]
fn later_and_other_components_vertices_are_refused() {
    let mut b = block();
    let vertex = corner_vertex(
        [side(b.extrude, 1, 0), side(b.extrude, 1, 3)],
        top(&b.region),
    );
    b.doc
        .kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.clone(), [0.0, 0.0, 20.0]);
    let doc = &mut b.doc;
    // A sketch before the block.
    doc.set_marker(0).unwrap();
    let early = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    doc.set_marker(3).unwrap();
    let source = SpatialSource::Vertex {
        body: b.body,
        vertex: vertex.parse().unwrap(),
        link: None,
    };
    let error = doc
        .edit_spatial_sketch(early, "Attach", |e| {
            let p = e.add_point([0.0; 3])?;
            e.attach(p, source.clone())
        })
        .unwrap_err();
    assert!(
        error.to_string().contains("does not come before"),
        "{error}"
    );
    assert!(doc.spatial_sketch_def(early).unwrap().points.is_empty());

    // A box in component A, a 3D sketch in the root.
    let (a, _) = doc
        .create_component(Some("A"), Transform::IDENTITY, true)
        .unwrap();
    let boxed = doc
        .add_feature(
            &def(
                json!({"type": "box", "plane": "xy", "corner": [0, 0], "length": 10,
                        "width": 10, "height": 10, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let theirs = format!("V{{{boxed}:side0|{boxed}:side1|{boxed}:top}}");
    doc.kernel().vertex_points.borrow_mut().insert(
        theirs.parse::<VertexName>().unwrap().to_string(),
        [10.0, 0.0, 10.0],
    );
    doc.activate_component(ComponentUid::ROOT).unwrap();
    assert_ne!(a, ComponentUid::ROOT);
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let error = doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let p = e.add_point([0.0; 3])?;
            e.attach(
                p,
                SpatialSource::Vertex {
                    body: crate::ids::BodyUid::new(boxed, 0),
                    vertex: theirs.parse().unwrap(),
                    link: None,
                },
            )
        })
        .unwrap_err();
    assert_eq!(
        error.failure.as_ref().map(|f| f.kind),
        Some(SpatialFailureKind::Reference),
        "{error}"
    );
    assert!(doc.spatial_sketch_def(sketch).unwrap().points.is_empty());
}

/// Unfix removes Fix and nothing else; Detach removes the attachment and
/// nothing else.
#[test]
fn unfix_and_detach_each_remove_their_own_relation() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let (p, q) = doc
        .edit_spatial_sketch(sketch, "Add Points", |e| {
            let p = e.add_point([0.0; 3])?;
            let q = e.add_point([3.0, 4.0, 5.0])?;
            e.attach(p, SpatialSource::Origin)?;
            e.fix(q)?;
            Ok((p, q))
        })
        .unwrap()
        .0;
    assert_eq!(doc.spatial_sketch(sketch).unwrap().status().unwrap().dof, 0);
    let error = doc
        .edit_spatial_sketch(sketch, "Unfix", |e| e.unfix(p))
        .unwrap_err();
    assert!(error.to_string().contains("not fixed"), "{error}");
    let error = doc
        .edit_spatial_sketch(sketch, "Detach", |e| e.detach(q))
        .unwrap_err();
    assert!(error.to_string().contains("not attached"), "{error}");
    doc.edit_spatial_sketch(sketch, "Unfix", |e| e.unfix(q))
        .unwrap();
    let def = doc.spatial_sketch_def(sketch).unwrap();
    assert_eq!(def.attachments().count(), 1, "the attachment stays");
    assert_eq!(doc.spatial_sketch(sketch).unwrap().status().unwrap().dof, 3);
    assert!(near(point(&doc, sketch, q), [3.0, 4.0, 5.0]));
}

/// W10: a 3D sketch on a construction plane turned about X and moved
/// along its normal: local dimensions keep their meaning, and its points
/// in the component are the local ones placed by the plane's frame once,
/// also after the plane turns and moves.
#[test]
fn a_moved_and_turned_support_is_applied_once() {
    let mut doc = Document::new(MockKernel::default());
    let tilted = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "angle", "line": "x", "angle": 0.5,
                                       "plane": "xy"}})),
            None,
        )
        .unwrap();
    let raised = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": tilted.uid.to_string(),
                                       "distance": 12}})),
            None,
        )
        .unwrap();
    let sketch = doc
        .add_spatial_sketch(SketchPlane::Construction(raised.uid))
        .unwrap()
        .uid;
    let r = route(&mut doc, sketch);
    let frame_of = |doc: &Document<MockKernel>| -> SketchFrame {
        match doc.datum(raised.uid) {
            Some(Datum::Plane(plane)) => plane.frame(),
            other => panic!("{other:?}"),
        }
    };
    let local = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 30.0, 0.0],
        [40.0, 30.0, 20.0],
    ];
    let check = |doc: &Document<MockKernel>| {
        let frame = frame_of(doc);
        let output = doc.spatial_sketch(sketch).unwrap();
        let used = output.frame.unwrap();
        assert!(near(used.origin, frame.origin) && near(used.x_axis, frame.x_axis));
        for (p, at) in r.points.iter().zip(local) {
            assert!(near(output.local_point(*p).unwrap(), at), "p{}", p.0);
            assert!(near(output.point(*p).unwrap(), to_component(&frame, at)));
        }
        assert!(output.status().unwrap().fully_constrained);
    };
    check(&doc);
    let before = frame_of(&doc);
    // The plane turned further and moved: the sketch follows once.
    doc.set_parameter(&tilted.parameters[0], 0.9).unwrap();
    doc.set_parameter(&raised.parameters[0], 25.0).unwrap();
    let after = frame_of(&doc);
    assert!(!near(before.origin, after.origin) || !near(before.y_axis, after.y_axis));
    check(&doc);
    // The route's lengths are local: still 40 from A to B.
    let output = doc.spatial_sketch(sketch).unwrap();
    let [a, b] = [
        output.point(r.points[0]).unwrap(),
        output.point(r.points[1]).unwrap(),
    ];
    assert!((mitcad_vec3::distance(a, b) - 40.0).abs() <= TOL);
    assert_eq!(r.params.len(), 3);
}

/// A drag pulls points with soft goals: its preview changes nothing, and
/// taking it is one undo step that adds no relation.
#[test]
fn a_drag_is_previewed_without_trace_and_taken_as_one_step() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let (ab, _) = segment(
        &mut doc,
        sketch,
        SegmentRequest {
            start_source: Some(SpatialSource::Origin),
            axis: Some((Z, SpatialSense::Positive)),
            ..SegmentRequest::new(At([0.0; 3]), At([0.0, 0.0, 10.0]))
        },
    );
    let before = doc.spatial_sketch_def(sketch).unwrap().clone();
    let preview = doc
        .preview_spatial_drag(sketch, &[(ab.end, [2.0, 2.0, 25.0])])
        .unwrap();
    assert!(near(preview.points[&ab.end], [0.0, 0.0, 25.0]));
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &before);
    assert!(near(point(&doc, sketch, ab.end), [0.0, 0.0, 10.0]));

    let report = doc
        .drag_spatial_sketch(sketch, &[(ab.end, [2.0, 2.0, 25.0])])
        .unwrap();
    assert_eq!(report.status.dof, 1);
    assert!(near(point(&doc, sketch, ab.end), [0.0, 0.0, 25.0]));
    let after = doc.spatial_sketch_def(sketch).unwrap();
    assert_eq!(after.constraints, before.constraints);
    assert!(after.dimensions.is_empty());
    assert_eq!(doc.undo().as_deref(), Some("Drag in 3DSketch1"));
    assert!(near(point(&doc, sketch, ab.end), [0.0, 0.0, 10.0]));
}

/// W9 (mitcad#1550): a closed loop in the sketch plane and one out of it
/// both solve; the one in the plane bounds a profile region, which an
/// Extrude takes, the other none.
#[test]
fn a_closed_loop_in_the_sketch_plane_bounds_a_region() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    doc.edit_spatial_sketch(sketch, "Add Lines", |e| {
        let square = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
        ];
        let bent = [
            [20.0, 0.0, 0.0],
            [30.0, 0.0, 0.0],
            [30.0, 10.0, 5.0],
            [20.0, 10.0, 0.0],
        ];
        for corners in [square, bent] {
            let ids: Vec<EntityUid> = corners
                .iter()
                .map(|at| e.add_point(*at))
                .collect::<Result<_, _>>()?;
            for (i, id) in ids.iter().enumerate() {
                e.add_line(Existing(*id), Existing(ids[(i + 1) % ids.len()]), false)?;
            }
            e.fix(ids[0])?;
        }
        Ok(())
    })
    .unwrap();
    let output = doc.spatial_sketch(sketch).unwrap();
    assert!(output.is_solved());
    assert_eq!(output.model_curves().len(), 8);
    let planar = doc
        .sketch_output(sketch)
        .expect("its sketch plane's output");
    assert_eq!(planar.regions.len(), 1);
    let key = planar.regions[0].key.to_string();
    assert_eq!(key, "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}");
    let extrude = doc
        .add_feature(
            &def(json!({"type": "extrude",
                        "profiles": [{"sketch": sketch, "region": key}],
                        "extent": {"type": "distance", "distance": 5},
                        "operation": "new_body"})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&doc, extrude), FeatureStatus::Ok);
    // The loop out of the plane bounds none.
    let error = doc
        .add_feature(
            &def(json!({"type": "extrude",
                        "profiles": [{"sketch": sketch,
                                      "region": "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}"}],
                        "extent": {"type": "distance", "distance": 5},
                        "operation": "new_body"})),
            None,
        )
        .map(|added| status(&doc, added.uid));
    assert!(
        !matches!(error, Ok(FeatureStatus::Ok)),
        "the loop out of the plane bounds no region: {error:?}"
    );
}

// Arcs and splines (the owner's scope decision of 8.11.2026 on mitcad#1501).

fn tangents(doc: &Document<MockKernel>, sketch: FeatureUid) -> Vec<Option<SpatialSense>> {
    doc.spatial_sketch_def(sketch)
        .unwrap()
        .constraints
        .iter()
        .filter_map(|c| match c.kind {
            SpatialConstraintKind::Tangent { sense, .. } => Some(sense),
            _ => None,
        })
        .collect()
}

/// W11 at the model level: the bent route of lines and tangent arcs
/// solves with nothing free; AB at 60 and the radius at 12 move it with
/// its tangencies kept in their senses, and its arcs are conics with the
/// normals the tangencies force.
#[test]
fn the_bent_route_follows_its_length_and_radius() {
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
    let (cd, _) = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| {
            let cd = e.add_segment(&along(Existing(bc.end), [50.0, 38.0, 0.0], Y, 30.0))?;
            e.tangent(bc.arc, cd.line, bc.end)?;
            Ok(cd)
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let (de, _) = doc
        .edit_spatial_sketch(sketch, "Add Arc", |e| {
            let de = e.add_arc(&ArcRequest::new(
                Existing(cd.end),
                At([50.0, 50.0, 10.0]),
                ArcPlacement::Tangent { curve: cd.line },
            ))?;
            e.equal(bc.arc, de.arc)?;
            Ok(de)
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let (ef, report) = doc
        .edit_spatial_sketch(sketch, "Add Line", |e| {
            let ef = e.add_segment(&along(Existing(de.end), [50.0, 50.0, 28.0], Z, 20.0))?;
            e.tangent(de.arc, ef.line, de.end)?;
            Ok(ef)
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.status.dof, 0, "{:?}", report.status);
    assert!(report.status.fully_constrained);
    assert!(report.status.determined_curves.contains(&de.arc));
    let points = [
        ab.start, ab.end, bc.center, bc.end, cd.end, de.center, de.end, ef.end,
    ];
    let check = |doc: &Document<MockKernel>, expected: [[f64; 3]; 8]| {
        for (p, at) in points.iter().zip(expected) {
            let solved = point(doc, sketch, *p);
            assert!(near(solved, at), "p{}: {solved:?}, not {at:?}", p.0);
        }
    };
    check(
        &doc,
        [
            [0.0, 0.0, 0.0],
            [40.0, 0.0, 0.0],
            [40.0, 10.0, 0.0],
            [50.0, 10.0, 0.0],
            [50.0, 40.0, 0.0],
            [50.0, 40.0, 10.0],
            [50.0, 50.0, 10.0],
            [50.0, 50.0, 30.0],
        ],
    );
    assert_eq!(tangents(&doc, sketch), [Some(SpatialSense::Positive); 4]);
    let normals = |doc: &Document<MockKernel>, r: f64| {
        let output = doc.spatial_sketch(sketch).unwrap();
        for (arc, normal) in [(bc.arc, [0.0, 0.0, 1.0]), (de.arc, [1.0, 0.0, 0.0])] {
            match output.curve(arc) {
                Some(Curve3::Conic {
                    normal: n,
                    major,
                    minor,
                    start,
                    end,
                    closed: false,
                    ..
                }) => {
                    assert!(near(n, normal), "c{}: {n:?}", arc.0);
                    assert!((major - r).abs() <= TOL && (minor - r).abs() <= TOL);
                    // A quarter turn each.
                    assert!(start.abs() <= TOL && (end - FRAC_PI_2).abs() <= TOL);
                }
                other => panic!("{other:?}"),
            }
        }
    };
    normals(&doc, 10.0);
    let ids = doc.spatial_sketch_def(sketch).unwrap().clone();

    // AB to 60 and the radius to 12: everything follows.
    doc.set_parameter(&ab_params[0], 60.0).unwrap();
    doc.set_parameter(&radius, 12.0).unwrap();
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    check(
        &doc,
        [
            [0.0, 0.0, 0.0],
            [60.0, 0.0, 0.0],
            [60.0, 12.0, 0.0],
            [72.0, 12.0, 0.0],
            [72.0, 42.0, 0.0],
            [72.0, 42.0, 12.0],
            [72.0, 54.0, 12.0],
            [72.0, 54.0, 32.0],
        ],
    );
    normals(&doc, 12.0);
    let after = doc.spatial_sketch_def(sketch).unwrap();
    assert_eq!(after.curves.len(), ids.curves.len());
    assert_eq!(after.constraints, ids.constraints);
}

/// W17's model part: a fit-point spline from a fixed line's end, tangent
/// to it there; its first control leg follows the line, also after a
/// fit point is dragged (one undo step).
#[test]
fn a_fit_point_spline_keeps_its_tangent_end() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((_, spline), _) = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            let line = e.add_line(At([0.0; 3]), At([10.0, 0.0, 0.0]), false)?;
            let [a, b] = e.def.line(line).ok_or("no line")?;
            e.fix(a)?;
            e.fix(b)?;
            let spline = e.add_spline(&SplineRequest::fit(vec![
                Existing(b),
                At([20.0, 5.0, 0.0]),
                At([30.0, 0.0, 5.0]),
                At([40.0, 5.0, 5.0]),
            ]))?;
            e.tangent(line, spline.spline, b)?;
            Ok((line, spline))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    // Three free fit points, four hidden control points; the free end's
    // curvature and the interpolation rows, the tangency: one more for its
    // magnitude.
    assert_eq!(
        doc.spatial_sketch(sketch).unwrap().status().unwrap().dof,
        10
    );
    let leg =
        |doc: &Document<MockKernel>| match doc.spatial_sketch(sketch).unwrap().curve(spline.spline)
        {
            Some(Curve3::BSpline { degree, poles, .. }) => {
                assert_eq!((degree, poles.len()), (3, 6));
                assert!(near(poles[0], [10.0, 0.0, 0.0]));
                let leg = mitcad_vec3::sub(poles[1], poles[0]);
                assert!(
                    leg[0] > 0.0 && leg[1].abs() <= TOL && leg[2].abs() <= TOL,
                    "{leg:?}"
                );
            }
            other => panic!("{other:?}"),
        };
    leg(&doc);
    assert_eq!(tangents(&doc, sketch), [Some(SpatialSense::Positive)]);
    doc.drag_spatial_sketch(sketch, &[(spline.points[1], [20.0, 8.0, 2.0])])
        .unwrap();
    assert!(near(
        point(&doc, sketch, spline.points[1]),
        [20.0, 8.0, 2.0]
    ));
    leg(&doc);
    assert_eq!(doc.undo().as_deref(), Some("Drag in 3DSketch1"));
    assert!(near(
        point(&doc, sketch, spline.points[1]),
        [20.0, 5.0, 0.0]
    ));
}

/// Invalid arcs and splines are refused as whole edits, before the solve
/// (collinear Three-Point input, a zero radius, coincident fit points) or
/// by it (a radius its fixed points contradict): the definition, the
/// parameters and the history stay as they were.
#[test]
fn invalid_arcs_and_splines_are_refused_as_whole_edits() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    route(&mut doc, sketch);
    let before = doc.spatial_sketch_def(sketch).unwrap().clone();
    let params = doc.parameters().clone();
    let error = doc
        .edit_spatial_sketch(sketch, "Add Arc", |e| {
            e.add_segment(&along(At([100.0, 0.0, 0.0]), [115.0, 0.0, 0.0], X, 15.0))?;
            e.add_arc(&ArcRequest::new(
                At([0.0, 50.0, 0.0]),
                At([20.0, 50.0, 0.0]),
                ArcPlacement::ThreePoint {
                    through: [10.0, 50.0, 0.0],
                },
            ))
        })
        .unwrap_err();
    assert!(error.to_string().contains("one line"), "{error}");
    let error = doc
        .edit_spatial_sketch(sketch, "Add Arc", |e| {
            e.add_arc(&ArcRequest {
                radius: Some(ValueInput::Number(0.0)),
                ..ArcRequest::new(
                    At([0.0, 50.0, 0.0]),
                    At([-10.0, 60.0, 0.0]),
                    ArcPlacement::Centre {
                        center: At([0.0, 60.0, 0.0]),
                        normal: [0.0, 0.0, 1.0],
                    },
                )
            })
        })
        .unwrap_err();
    assert!(error.to_string().contains("greater than"), "{error}");
    let error = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            e.add_spline(&SplineRequest::fit(vec![
                At([0.0, 70.0, 0.0]),
                At([0.0, 70.0, 0.0]),
                At([10.0, 70.0, 0.0]),
            ]))
        })
        .unwrap_err();
    assert!(error.to_string().contains("apart"), "{error}");
    let error = doc
        .edit_spatial_sketch(sketch, "Add Arc", |e| {
            let arc = e.add_arc(&ArcRequest::new(
                At([10.0, 80.0, 0.0]),
                At([-10.0, 80.0, 0.0]),
                ArcPlacement::ThreePoint {
                    through: [0.0, 90.0, 0.0],
                },
            ))?;
            for p in [arc.center, arc.start, arc.end] {
                e.fix(p)?;
            }
            e.add_dimension(
                SpatialDimensionKind::Radius { curve: arc.arc },
                Some(&ValueInput::Number(12.0)),
                false,
            )
        })
        .unwrap_err();
    assert!(error.failure.is_some(), "{error}");
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &before);
    assert_eq!(doc.parameters(), &params);
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    assert_eq!(doc.undo().as_deref(), Some("Add Line in 3DSketch1"));
}

/// Deleting a fit-point spline takes its own fit points and its tangency
/// (its hidden control points are the solver's); the point it shares
/// with a line stays.
#[test]
fn deleting_a_fit_point_spline_keeps_its_shared_point() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((b, spline), _) = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            let line = e.add_line(At([0.0; 3]), At([10.0, 0.0, 0.0]), false)?;
            let [a, b] = e.def.line(line).ok_or("no line")?;
            e.fix(a)?;
            e.fix(b)?;
            let spline = e.add_spline(&SplineRequest::fit(vec![
                Existing(b),
                At([20.0, 5.0, 0.0]),
                At([30.0, 0.0, 5.0]),
            ]))?;
            e.tangent(line, spline.spline, b)?;
            Ok((b, spline))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let ((), report) = doc
        .edit_spatial_sketch(sketch, "Delete", |e| e.delete_curve(spline.spline))
        .unwrap_or_else(|e| panic!("{e}"));
    let def = doc.spatial_sketch_def(sketch).unwrap();
    assert!(def.curve(spline.spline).is_none());
    assert!(def.point(b).is_some(), "the line's end stays");
    for p in &spline.points[1..] {
        assert!(def.point(*p).is_none(), "p{}", p.0);
    }
    assert!(tangents(&doc, sketch).is_empty());
    assert!(report.removed.contains(&format!("c{}", spline.spline.0)));
    assert_eq!(doc.spatial_sketch(sketch).unwrap().model_curves().len(), 1);
    assert_eq!(doc.undo().as_deref(), Some("Delete in 3DSketch1"));
    assert!(
        doc.spatial_sketch_def(sketch)
            .unwrap()
            .curve(spline.spline)
            .is_some()
    );
}

/// Each curve is one model curve in the component, under a turned and
/// moved support: a line, an arc as a conic from its start through its
/// sweep, a circle as a closed conic, a control-point spline by its
/// points and a fit-point spline by its solved control net.
#[test]
fn each_curve_is_one_model_curve_under_a_turned_support() {
    let mut doc = Document::new(MockKernel::default());
    let tilted = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "angle", "line": "x", "angle": 0.5,
                                       "plane": "xy"}})),
            None,
        )
        .unwrap();
    let raised = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": tilted.uid.to_string(),
                                       "distance": 12}})),
            None,
        )
        .unwrap();
    let sketch = doc
        .add_spatial_sketch(SketchPlane::Construction(raised.uid))
        .unwrap()
        .uid;
    let ((line, arc, circle, control, fitted), _) = doc
        .edit_spatial_sketch(sketch, "Add Curves", |e| {
            let line = e.add_line(At([0.0; 3]), At([10.0, 0.0, 0.0]), false)?;
            let arc = e.add_arc(&ArcRequest::new(
                At([30.0, 0.0, 0.0]),
                At([20.0, 10.0, 0.0]),
                ArcPlacement::Centre {
                    center: At([20.0, 0.0, 0.0]),
                    normal: [0.0, 0.0, 1.0],
                },
            ))?;
            let circle = e.add_circle(At([40.0, 0.0, 0.0]), 5.0, [0.0, 1.0, 0.0], None, false)?;
            let control = e.add_spline(&SplineRequest::control(
                vec![
                    At([0.0, 20.0, 0.0]),
                    At([5.0, 25.0, 3.0]),
                    At([10.0, 20.0, 6.0]),
                ],
                2,
            ))?;
            let fitted = e.add_spline(&SplineRequest::fit(vec![
                At([0.0, 40.0, 0.0]),
                At([5.0, 45.0, 2.0]),
                At([10.0, 40.0, 4.0]),
            ]))?;
            Ok((line, arc, circle, control, fitted))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let output = doc.spatial_sketch(sketch).unwrap();
    let frame = output.frame.unwrap();
    assert!(
        !near(frame.y_axis, [0.0, 1.0, 0.0]),
        "the support is turned"
    );
    let place = |p: [f64; 3]| to_component(&frame, p);
    let turn = |v: [f64; 3]| direction_to_component(&frame, v);
    match output.curve(line) {
        Some(Curve3::Line { start, end }) => {
            assert!(near(start, place([0.0; 3])) && near(end, place([10.0, 0.0, 0.0])));
        }
        other => panic!("{other:?}"),
    }
    match output.curve(arc.arc) {
        Some(Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        }) => {
            assert!(near(center, place([20.0, 0.0, 0.0])));
            assert!(near(normal, turn([0.0, 0.0, 1.0])));
            assert!(near(x_axis, turn([1.0, 0.0, 0.0])));
            assert!((major - 10.0).abs() <= TOL && (minor - 10.0).abs() <= TOL);
            assert!(start.abs() <= TOL && (end - FRAC_PI_2).abs() <= TOL && !closed);
        }
        other => panic!("{other:?}"),
    }
    match output.curve(circle) {
        Some(Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            end,
            closed,
            ..
        }) => {
            assert!(near(center, place([40.0, 0.0, 0.0])));
            assert!(near(normal, turn([0.0, 1.0, 0.0])));
            assert!(mitcad_vec3::dot(x_axis, normal).abs() <= TOL);
            assert!((mitcad_vec3::norm(x_axis) - 1.0).abs() <= TOL);
            assert!((major - 5.0).abs() <= TOL && (end - TAU).abs() <= TOL && closed);
        }
        other => panic!("{other:?}"),
    }
    match output.curve(control.spline) {
        Some(Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        }) => {
            assert_eq!(degree, 2);
            assert!(weights.is_empty());
            assert_eq!(knots, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
            let local = [[0.0, 20.0, 0.0], [5.0, 25.0, 3.0], [10.0, 20.0, 6.0]];
            assert_eq!(poles.len(), 3);
            for (pole, at) in poles.iter().zip(local) {
                assert!(near(*pole, place(at)));
            }
        }
        other => panic!("{other:?}"),
    }
    match output.curve(fitted.spline) {
        Some(Curve3::BSpline { degree, poles, .. }) => {
            assert_eq!((degree, poles.len()), (3, 5), "the fit points' control net");
            assert!(near(poles[0], place([0.0, 40.0, 0.0])));
            assert!(near(poles[4], place([10.0, 40.0, 4.0])));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(output.model_curves().len(), 5);
    // No closed loop in the sketch plane: no profile regions.
    assert!(
        doc.sketch_output(sketch)
            .is_some_and(|o| o.regions.is_empty())
    );
}

/// The end tangent of a fit-point spline's `end` where the model solved
/// it, from the solve's control net.
fn end_tangent(
    doc: &Document<MockKernel>,
    sketch: FeatureUid,
    spline: EntityUid,
    end: usize,
) -> [f64; 3] {
    use crate::features::spatial_sketch::{SolvedCurve, spline_end_tangent};
    let solved = doc
        .spatial_sketch(sketch)
        .and_then(|o| o.solved.as_ref())
        .expect("solved");
    match solved.curves.get(&spline) {
        Some(SolvedCurve::Spline(g)) => spline_end_tangent(g, end).expect("a tangent"),
        other => panic!("{other:?}"),
    }
}

fn stored_tangents(
    doc: &Document<MockKernel>,
    sketch: FeatureUid,
    spline: EntityUid,
) -> [Option<[f64; 3]>; 2] {
    use crate::features::spatial_sketch::SpatialCurveKind;
    match &doc
        .spatial_sketch_def(sketch)
        .and_then(|d| d.curve(spline))
        .expect("the spline")
        .kind
    {
        SpatialCurveKind::FittedSpline { tangents, .. } => *tangents,
        other => panic!("{other:?}"),
    }
}

/// A tangent handle's drag (mitcad#1526, W17): an open fit-point spline's
/// free start tangent pulled to a new direction and magnitude, its fit
/// points fixed, as one undo step. The definition keeps the tangent, so a
/// later edit's solve, the design file and redo keep it too; the natural
/// end stores none and is refused, as is a control-point spline, with
/// nothing changed.
#[test]
fn a_dragged_tangent_handle_stays() {
    use crate::features::spatial_sketch::{SpatialCurveKind, SpatialSplineEnd, SpatialTangentPull};
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let (spline, _) = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            let made = e.add_spline(&SplineRequest {
                ends: [SpatialSplineEnd::Free, SpatialSplineEnd::Natural],
                ..SplineRequest::fit(vec![
                    At([0.0; 3]),
                    At([20.0, 10.0, 0.0]),
                    At([40.0, 0.0, 10.0]),
                ])
            })?;
            for &p in &made.points {
                e.fix(p)?;
            }
            Ok(made)
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let before = end_tangent(&doc, sketch, spline.spline, 0);
    let stored = stored_tangents(&doc, sketch, spline.spline);
    assert!(near(stored[0].expect("the free start's"), before));
    assert!(stored[1].is_none(), "a natural end stores none");

    // Pulled up along Z, longer than the chord.
    let wanted = [10.0, 0.0, 90.0];
    let pull = SpatialTangentPull {
        curve: spline.spline,
        end: 0,
        tangent: wanted,
    };
    let preview = doc
        .preview_spatial_drag_with(sketch, &[], &[pull])
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(near(preview.points[&spline.points[1]], [20.0, 10.0, 0.0]));
    assert!(
        near(end_tangent(&doc, sketch, spline.spline, 0), before),
        "a preview changes nothing"
    );
    doc.drag_spatial_sketch_with(sketch, &[], &[pull])
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(near(end_tangent(&doc, sketch, spline.spline, 0), wanted));
    assert!(near(
        stored_tangents(&doc, sketch, spline.spline)[0].unwrap(),
        wanted
    ));
    for (p, at) in spline
        .points
        .iter()
        .zip([[0.0; 3], [20.0, 10.0, 0.0], [40.0, 0.0, 10.0]])
    {
        assert!(near(point(&doc, sketch, *p), at), "the fit points stay");
    }

    // A later edit solves the sketch again from its definition.
    doc.edit_spatial_sketch(sketch, "Add Point", |e| e.add_point([0.0, 50.0, 0.0]))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(near(end_tangent(&doc, sketch, spline.spline, 0), wanted));

    // The design file keeps it (the curve's written form).
    let kind = doc
        .spatial_sketch_def(sketch)
        .and_then(|d| d.curve(spline.spline))
        .unwrap()
        .kind
        .clone();
    let written = serde_json::to_value(&kind).unwrap();
    assert_eq!(written["type"], "fitted_spline", "{written}");
    assert_eq!(written["tangents"][1], json!(null));
    assert_eq!(written["tangents"][0].as_array().map(Vec::len), Some(3));
    let read: SpatialCurveKind = serde_json::from_value(written).unwrap();
    assert_eq!(read, kind);

    // Undo takes the point and then the drag back; redo puts the drag back.
    assert_eq!(doc.undo().as_deref(), Some("Add Point in 3DSketch1"));
    assert_eq!(doc.undo().as_deref(), Some("Drag in 3DSketch1"));
    assert!(near(end_tangent(&doc, sketch, spline.spline, 0), before));
    assert_eq!(doc.redo().as_deref(), Some("Drag in 3DSketch1"));
    assert!(near(end_tangent(&doc, sketch, spline.spline, 0), wanted));

    // The natural end and a control-point spline have no handle.
    let after = doc.spatial_sketch_def(sketch).unwrap().clone();
    let natural = SpatialTangentPull { end: 1, ..pull };
    let error = doc
        .drag_spatial_sketch_with(sketch, &[], &[natural])
        .unwrap_err();
    assert!(error.to_string().contains("natural"), "{error}");
    let error = doc
        .preview_spatial_drag_with(sketch, &[], &[natural])
        .unwrap_err();
    assert!(error.to_string().contains("natural"), "{error}");
    let (control, _) = doc
        .edit_spatial_sketch(sketch, "Add Spline", |e| {
            e.add_spline(&SplineRequest::control(
                vec![
                    At([0.0, -20.0, 0.0]),
                    At([10.0, -25.0, 0.0]),
                    At([20.0, -20.0, 0.0]),
                ],
                2,
            ))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let error = doc
        .drag_spatial_sketch_with(
            sketch,
            &[],
            &[SpatialTangentPull {
                curve: control.spline,
                ..pull
            }],
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("no open fit-point spline"),
        "{error}"
    );
    assert_eq!(doc.undo().as_deref(), Some("Add Spline in 3DSketch1"));
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &after);
}
