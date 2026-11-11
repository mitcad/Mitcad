// SPDX-License-Identifier: MIT
//! Linked reference geometry of 3D sketches (mitcad#1543), with the mock
//! kernel (whose edges are the curves a test gives them): an edge's
//! middle, its start and a point on it follow a resized body, records are
//! reused, replaced (undo and redo), detached (a record without users goes)
//! and saved and read again, and a lost source is a reference failure
//! (W12); a turned support includes and projects an edge; another
//! sketch's line end and a point on its circle follow its dimension edits,
//! and a later planar sketch projects a 3D spline and follows it (W13; a
//! circle stands in for the arc); another component's vertex and edge
//! follow its occurrence and its feature, and a reference that would make
//! a link cycle is refused (W14).

use serde_json::json;

use crate::features::sketch::sketch_entity_curve;
use crate::features::spatial_sketch::{
    SpatialAxis, SpatialCurvePoint, SpatialDimensionKind, SpatialFailureKind, SpatialSource,
};
use crate::features::{SketchPlane, ValueInput};
use crate::ids::{BodyUid, ComponentUid, EntityUid, FeatureUid};
use crate::kernel::Curve3;
use crate::sketch::spatial_edit::{SpatialPointInput, SplineRequest};
use crate::sketch::{Ref, SketchEntityRef};
use crate::test_support::{block, corner, def, num, top_edge};
use crate::testing::MockKernel;
use crate::topo::{EdgeName, TopoName};
use crate::transform::Transform;
use crate::{Document, FeatureStatus};

use SpatialPointInput::{At, Existing};

const TOL: f64 = 1e-6;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

/// A solved point in the component's coordinates.
fn point(doc: &Document<MockKernel>, sketch: FeatureUid, id: EntityUid) -> [f64; 3] {
    doc.spatial_sketch(sketch)
        .and_then(|o| o.point(id))
        .unwrap_or_else(|| panic!("p{} is not solved", id.0))
}

/// The line the mock kernel gives for an edge.
fn set_edge(doc: &Document<MockKernel>, edge: &EdgeName, start: [f64; 3], end: [f64; 3]) {
    doc.kernel().named_curves.borrow_mut().insert(
        TopoName::Edge(edge.clone()).to_string(),
        Curve3::Line { start, end },
    );
}

fn edge(body: BodyUid, edge: &EdgeName, at: Option<SpatialCurvePoint>) -> SpatialSource {
    SpatialSource::Edge {
        body,
        edge: Box::new(edge.clone()),
        at,
        link: None,
    }
}

/// W12: the block (60 x 40 x 20, Body1 F2.b0): a point at the middle of
/// its top front edge, one at the start of its upright edge and one on
/// that edge 15 above it follow the block to 30 high; a second point on
/// the edge reuses its record; Replace Reference moves the middle to the
/// back edge's (one undo step); Detach removes the Point on Curve
/// relations and the record with its last user; a suppressed block is a
/// reference failure with the definition kept; saved and read again, the
/// records are as they were.
#[test]
fn edge_points_follow_their_body_and_records_are_edited() {
    let mut b = block();
    let body = b.body;
    let front = top_edge(b.extrude, 1, &b.region, 0);
    let back = top_edge(b.extrude, 1, &b.region, 2);
    let upright = corner(b.extrude, 1, 1);
    let shape = |doc: &Document<MockKernel>, h: f64| {
        set_edge(doc, &front, [0.0, 0.0, h], [60.0, 0.0, h]);
        set_edge(doc, &back, [60.0, 40.0, h], [0.0, 40.0, h]);
        set_edge(doc, &upright, [60.0, 0.0, 0.0], [60.0, 0.0, h]);
    };
    shape(&b.doc, 20.0);
    let height = b.doc.feature(b.extrude).unwrap().def.params()[0];
    let height = b.doc.parameters().name(height);
    let sketch = b.doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((middle, bottom, on, reference), report) = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let middle = e.add_point([29.0, 1.0, 19.0])?;
            e.attach(middle, edge(body, &front, Some(SpatialCurvePoint::Middle)))?;
            let bottom = e.add_point([60.0, 0.0, 1.0])?;
            e.attach(bottom, edge(body, &upright, Some(SpatialCurvePoint::Start)))?;
            let on = e.add_point([60.0, 0.0, 14.0])?;
            let k = e.attach(on, edge(body, &upright, None))?;
            let reference = e
                .def
                .constraint(k)
                .and_then(|c| c.kind.curves().first().copied())
                .ok_or("a Point on Curve relation")?;
            e.add_dimension(
                SpatialDimensionKind::Offset {
                    a: bottom,
                    b: on,
                    axis: SpatialAxis::Z,
                },
                Some(&ValueInput::Number(15.0)),
                false,
            )?;
            Ok((middle, bottom, on, reference))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.status.dof, 0, "{:?}", report.status);
    assert_eq!(report.curves, [reference]);
    assert!(near(point(&b.doc, sketch, middle), [30.0, 0.0, 20.0]));
    assert!(near(point(&b.doc, sketch, bottom), [60.0, 0.0, 0.0]));
    assert!(near(point(&b.doc, sketch, on), [60.0, 0.0, 15.0]));

    // The block 30 high: the middle stays the middle, the point on the
    // edge keeps its 15 above the edge's start.
    shape(&b.doc, 30.0);
    b.doc.set_parameter(&height, 30.0).unwrap();
    assert_eq!(status(&b.doc, sketch), FeatureStatus::Ok);
    assert!(near(point(&b.doc, sketch, middle), [30.0, 0.0, 30.0]));
    assert!(near(point(&b.doc, sketch, bottom), [60.0, 0.0, 0.0]));
    assert!(near(point(&b.doc, sketch, on), [60.0, 0.0, 15.0]));

    // A second point on the edge reuses its record.
    let (second, _) = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let q = e.add_point([60.0, 0.0, 25.0])?;
            e.attach(q, edge(body, &upright, None))?;
            Ok(q)
        })
        .unwrap();
    let def = b.doc.spatial_sketch_def(sketch).unwrap();
    assert_eq!(def.reference_curves().count(), 1);
    assert_eq!(def.reference_users(reference).len(), 2);
    let attachment = def
        .attachments()
        .find(|(_, p, _)| *p == middle)
        .map(|(k, _, _)| k)
        .unwrap();

    // Replace Reference: of the same kind only, then the back edge's
    // middle; one undo step.
    let error = b
        .doc
        .edit_spatial_sketch(sketch, "Replace Reference", |e| {
            e.replace_attachment(attachment, edge(body, &back, None))
        })
        .unwrap_err();
    assert!(error.to_string().contains("is a curve"), "{error}");
    b.doc
        .edit_spatial_sketch(sketch, "Replace Reference", |e| {
            e.replace_attachment(
                attachment,
                edge(body, &back, Some(SpatialCurvePoint::Middle)),
            )
        })
        .unwrap();
    assert!(near(point(&b.doc, sketch, middle), [30.0, 40.0, 30.0]));
    assert_eq!(
        b.doc.undo().as_deref(),
        Some("Replace Reference in 3DSketch1")
    );
    assert!(near(point(&b.doc, sketch, middle), [30.0, 0.0, 30.0]));
    b.doc.redo();
    assert!(near(point(&b.doc, sketch, middle), [30.0, 40.0, 30.0]));

    // Saved and read again: the records, their sources and the geometry.
    let file = b.doc.to_json();
    let mut back_doc = Document::from_json(&file, MockKernel::default()).unwrap();
    shape(&back_doc, 30.0);
    back_doc.recompute();
    assert_eq!(
        back_doc.spatial_sketch_def(sketch),
        b.doc.spatial_sketch_def(sketch)
    );
    for p in [middle, bottom, on, second] {
        assert!(near(point(&back_doc, sketch, p), point(&b.doc, sketch, p)));
    }

    // Detach: the Point on Curve relation goes, the record stays while
    // another point uses it and goes with its last user.
    let (removed, report) = b
        .doc
        .edit_spatial_sketch(sketch, "Detach", |e| e.detach(on))
        .unwrap();
    assert_eq!(removed.len(), 1);
    assert!(report.removed.iter().all(|r| !r.starts_with('c')));
    let (_, report) = b
        .doc
        .edit_spatial_sketch(sketch, "Detach", |e| e.detach(second))
        .unwrap();
    assert!(
        report.removed.contains(&format!("c{}", reference.0)),
        "{:?}",
        report.removed
    );
    assert_eq!(
        b.doc
            .spatial_sketch_def(sketch)
            .unwrap()
            .reference_curves()
            .count(),
        0
    );
    // The point stays where it was, free along the edge's former line.
    assert!(near(point(&b.doc, sketch, second), [60.0, 0.0, 25.0]));
    assert_eq!(b.doc.undo().as_deref(), Some("Detach in 3DSketch1"));
    assert_eq!(
        b.doc
            .spatial_sketch_def(sketch)
            .unwrap()
            .reference_curves()
            .count(),
        1
    );

    // The block suppressed: a reference failure, the definition kept and
    // no geometry shown.
    let kept = b.doc.spatial_sketch_def(sketch).unwrap().clone();
    b.doc.set_suppressed(b.extrude, true).unwrap();
    assert!(matches!(status(&b.doc, sketch), FeatureStatus::Failed(_)));
    let output = b.doc.spatial_sketch(sketch).unwrap();
    let failure = output.failure.as_ref().expect("failed");
    assert_eq!(failure.kind, SpatialFailureKind::Reference, "{failure}");
    assert!(output.solved.is_none() && output.point(middle).is_none());
    assert_eq!(b.doc.spatial_sketch_def(sketch).unwrap(), &kept);
    b.doc.set_suppressed(b.extrude, false).unwrap();
    assert_eq!(status(&b.doc, sketch), FeatureStatus::Ok);
}

/// A support turned to the YZ plane: an included edge middle is where the
/// edge is; a projected one and a point on a projected edge lie on the
/// sketch's plane (x = 0); an edge seen end on projects to a point and is
/// refused as a reference curve.
#[test]
fn a_turned_support_includes_and_projects_edges() {
    let mut b = block();
    let body = b.body;
    let front = top_edge(b.extrude, 1, &b.region, 0);
    let upright = corner(b.extrude, 1, 1);
    set_edge(&b.doc, &front, [0.0, 0.0, 20.0], [60.0, 0.0, 20.0]);
    set_edge(&b.doc, &upright, [60.0, 0.0, 0.0], [60.0, 0.0, 20.0]);
    let sketch = b.doc.add_spatial_sketch(SketchPlane::Yz).unwrap().uid;
    let ((included, projected, on), _) = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let included = e.add_point([1.0, 1.0, 29.0])?;
            e.attach(
                included,
                edge(body, &front, Some(SpatialCurvePoint::Middle)),
            )?;
            let projected = e.add_point([1.0, 1.0, 1.0])?;
            e.attach_as(
                projected,
                edge(body, &front, Some(SpatialCurvePoint::Middle)),
                true,
            )?;
            let on = e.add_point([1.0, 9.0, 1.0])?;
            e.attach_as(on, edge(body, &upright, None), true)?;
            Ok((included, projected, on))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(near(point(&b.doc, sketch, included), [30.0, 0.0, 20.0]));
    assert!(near(point(&b.doc, sketch, projected), [0.0, 0.0, 20.0]));
    let output = b.doc.spatial_sketch(sketch).unwrap();
    assert!(output.local_point(projected).unwrap()[2].abs() <= TOL);
    let at = point(&b.doc, sketch, on);
    assert!(at[0].abs() <= TOL && at[1].abs() <= TOL, "{at:?}");
    let error = b
        .doc
        .edit_spatial_sketch(sketch, "Project", |e| {
            e.include(edge(body, &front, None), true)
        })
        .unwrap_err();
    assert!(error.to_string().contains("is a point"), "{error}");
}

/// W13: a 3D line ends at the end of a planar sketch's line and a fit-point
/// spline passes through a point on its circle; the planar sketch's width
/// and diameter edited, both follow. A later planar sketch projects the
/// spline and follows its changes; the 3D sketch referring to that later
/// sketch would make a link cycle and is refused.
#[test]
fn another_sketchs_points_and_curves_are_followed() {
    let mut doc = Document::new(MockKernel::default());
    let planar = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let rectangle = doc
        .add_rectangle(planar, [0.0, 0.0], &num(30.0), &num(20.0))
        .unwrap();
    let circle = doc.add_circle(planar, [60.0, 10.0], &num(20.0)).unwrap();
    let (line, round) = (rectangle.curves[0], circle.curves[0]);
    let (width, diameter) = (
        rectangle.parameters[0].clone(),
        circle.parameters[0].clone(),
    );
    let of = |entity: Ref, at: Option<SpatialCurvePoint>| SpatialSource::Sketch {
        entity: SketchEntityRef {
            sketch: planar,
            entity,
        },
        at,
        link: None,
    };
    let planar_curve = |doc: &Document<MockKernel>, id: EntityUid| {
        sketch_entity_curve(doc.sketch_output(planar).unwrap(), Ref::Curve(id)).unwrap()
    };
    let line_end = |doc: &Document<MockKernel>| match planar_curve(doc, line) {
        Curve3::Line { end, .. } => end,
        other => panic!("{other:?}"),
    };
    // Where the circle is: its centre, normal and radius.
    let ring = |doc: &Document<MockKernel>| match planar_curve(doc, round) {
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            ..
        } => (center, normal, x_axis, major),
        other => panic!("{other:?}"),
    };
    let on_ring = |doc: &Document<MockKernel>, p: [f64; 3]| {
        let (center, normal, _, radius) = ring(doc);
        let d = mitcad_vec3::sub(p, center);
        (mitcad_vec3::norm(d) - radius).abs() <= TOL && mitcad_vec3::dot(d, normal).abs() <= TOL
    };
    let (center, _, x_axis, radius) = ring(&doc);
    let start: [f64; 3] = std::array::from_fn(|k| center[k] + radius * x_axis[k]);
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((end, through, spline), _) = doc
        .edit_spatial_sketch(sketch, "Add", |e| {
            let end = e.add_point([29.0, 1.0, 1.0])?;
            e.attach(end, of(Ref::Curve(line), Some(SpatialCurvePoint::End)))?;
            e.add_line(Existing(end), At([10.0, 10.0, 10.0]), false)?;
            let through = e.add_point(start)?;
            e.attach(through, of(Ref::Curve(round), None))?;
            let made = e.add_spline(&SplineRequest::fit(vec![
                Existing(through),
                At([80.0, 20.0, 5.0]),
                At([90.0, 0.0, 15.0]),
            ]))?;
            e.fix(made.points[1])?;
            e.fix(made.points[2])?;
            Ok((end, through, made.spline))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(near(point(&doc, sketch, end), line_end(&doc)));
    assert!(on_ring(&doc, point(&doc, sketch, through)));

    // The planar sketch's width and diameter edited: both follow.
    doc.set_parameter(&width, 40.0).unwrap();
    doc.set_parameter(&diameter, 30.0).unwrap();
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    assert!(near(point(&doc, sketch, end), line_end(&doc)));
    assert!((ring(&doc).3 - 15.0).abs() <= TOL);
    assert!(on_ring(&doc, point(&doc, sketch, through)));

    // A later planar sketch projects the spline and follows it.
    let later = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    doc.project_sketch_geometry(
        later,
        sketch,
        Some(&[Ref::Curve(spline)]),
        true,
        None,
        None,
        false,
    )
    .unwrap();
    let first_poles = |doc: &Document<MockKernel>| {
        let output = doc.sketch_output(later).unwrap();
        let id = *output.solved.curves.keys().next().expect("the projection");
        let projected = match sketch_entity_curve(output, Ref::Curve(id)) {
            Some(Curve3::BSpline { poles, .. }) => poles[0],
            other => panic!("{other:?}"),
        };
        let own = match doc.spatial_sketch(sketch).unwrap().curve(spline) {
            Some(Curve3::BSpline { poles, .. }) => poles[0],
            other => panic!("{other:?}"),
        };
        (projected, own, id)
    };
    let (projected, own, _) = first_poles(&doc);
    assert!(
        near(projected, [own[0], own[1], 0.0]),
        "{projected:?} {own:?}"
    );
    doc.set_parameter(&diameter, 40.0).unwrap();
    assert_eq!(status(&doc, later), FeatureStatus::Ok);
    let (projected, own, id) = first_poles(&doc);
    assert!(
        near(projected, [own[0], own[1], 0.0]),
        "{projected:?} {own:?}"
    );
    assert!(on_ring(&doc, point(&doc, sketch, through)));

    // The 3D sketch referring to the later sketch: a link cycle.
    let error = doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let p = e.add_point([0.0; 3])?;
            e.attach(
                p,
                SpatialSource::Sketch {
                    entity: SketchEntityRef {
                        sketch: later,
                        entity: Ref::Curve(id),
                    },
                    at: Some(SpatialCurvePoint::Start),
                    link: None,
                },
            )
        })
        .unwrap_err();
    assert!(error.to_string().contains("link cycle"), "{error}");
}

/// W14: component A's box seen through its occurrence: a line's end
/// attached to its vertex and a spline's point on its edge follow when
/// the occurrence moves and when the box is edited; the records keep their
/// links. A planar sketch of A that projects the 3D sketch's line comes
/// after it: the 3D sketch referring to that sketch's curve through A's
/// occurrence would make a link cycle and is refused.
#[test]
fn another_components_vertex_and_edge_follow_its_occurrence() {
    let mut doc = Document::new(MockKernel::default());
    let (a, occurrence) = doc
        .create_component(Some("A"), Transform::translation([100.0, 0.0, 0.0]), true)
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
    let body = BodyUid::new(boxed, 0);
    let vertex = format!("V{{{boxed}:side0|{boxed}:side1|{boxed}:top}}");
    let upright: EdgeName = format!("E{{{boxed}:side0|{boxed}:side1}}").parse().unwrap();
    let shape = |doc: &Document<MockKernel>, h: f64| {
        doc.kernel().vertex_points.borrow_mut().insert(
            vertex
                .parse::<crate::topo::VertexName>()
                .unwrap()
                .to_string(),
            [10.0, 0.0, h],
        );
        set_edge(doc, &upright, [10.0, 0.0, 0.0], [10.0, 0.0, h]);
    };
    shape(&doc, 10.0);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let link = crate::links::link_for(doc.assembly(), ComponentUid::ROOT, &[occurrence], None)
        .unwrap()
        .expect("another component");
    let sketch = doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let corner_source = SpatialSource::Vertex {
        body,
        vertex: vertex.parse().unwrap(),
        link: Some(link.clone()),
    };
    let edge_source = SpatialSource::Edge {
        body,
        edge: Box::new(upright.clone()),
        at: None,
        link: Some(link.clone()),
    };
    // Without the link, the body is another component's.
    let error = doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let p = e.add_point([110.0, 0.0, 10.0])?;
            e.attach(p, edge(body, &upright, Some(SpatialCurvePoint::End)))
        })
        .unwrap_err();
    assert_eq!(
        error.failure.as_ref().map(|f| f.kind),
        Some(SpatialFailureKind::Reference),
        "{error}"
    );
    let ((tip, on, line), _) = doc
        .edit_spatial_sketch(sketch, "Add", |e| {
            let tip = e.add_point([109.0, 1.0, 9.0])?;
            e.attach(tip, corner_source.clone())?;
            let line = e.add_line(Existing(tip), At([90.0, 0.0, 10.0]), false)?;
            let on = e.add_point([110.0, 0.0, 5.0])?;
            e.attach(on, edge_source.clone())?;
            let made = e.add_spline(&SplineRequest::fit(vec![
                Existing(on),
                At([120.0, 10.0, 0.0]),
                At([130.0, 0.0, 5.0]),
            ]))?;
            e.fix(made.points[1])?;
            e.fix(made.points[2])?;
            Ok((tip, on, line))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(near(point(&doc, sketch, tip), [110.0, 0.0, 10.0]));
    let on_upright = |doc: &Document<MockKernel>, y: f64, h: f64| {
        let p = point(doc, sketch, on);
        (p[0] - 110.0).abs() <= TOL && (p[1] - y).abs() <= TOL && p[2] >= -TOL && p[2] <= h + TOL
    };
    assert!(on_upright(&doc, 0.0, 10.0));

    // The occurrence moved: the sketch follows it once.
    doc.set_occurrence_transform(
        occurrence,
        Transform::translation([100.0, 20.0, 0.0]),
        false,
    )
    .unwrap();
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    assert!(near(point(&doc, sketch, tip), [110.0, 20.0, 10.0]));
    assert!(on_upright(&doc, 20.0, 10.0));

    // The box edited to 15 high: its vertex goes up, and the line's end.
    shape(&doc, 15.0);
    doc.edit_feature(
        boxed,
        &def(
            json!({"type": "box", "plane": "xy", "corner": [0, 0], "length": 10,
                    "width": 10, "height": 15, "operation": "new_body"}),
        ),
    )
    .unwrap();
    assert_eq!(status(&doc, sketch), FeatureStatus::Ok);
    assert!(near(point(&doc, sketch, tip), [110.0, 20.0, 15.0]));
    assert!(on_upright(&doc, 20.0, 15.0));
    let def = doc.spatial_sketch_def(sketch).unwrap().clone();
    assert!(
        def.attachments().all(|(_, _, s)| s.link() == Some(&link)),
        "{def:?}"
    );

    // A planar sketch of A, after the 3D sketch, projects its line.
    doc.activate_component(a).unwrap();
    let planar = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    doc.project_sketch_geometry(
        planar,
        sketch,
        Some(&[Ref::Curve(line)]),
        true,
        Some(&[]),
        None,
        false,
    )
    .unwrap();
    let projected = *doc
        .sketch_output(planar)
        .unwrap()
        .solved
        .curves
        .keys()
        .next()
        .expect("the projection");
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let error = doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let p = e.add_point([0.0; 3])?;
            e.attach(
                p,
                SpatialSource::Sketch {
                    entity: SketchEntityRef {
                        sketch: planar,
                        entity: Ref::Curve(projected),
                    },
                    at: Some(SpatialCurvePoint::End),
                    link: Some(link.clone()),
                },
            )
        })
        .unwrap_err();
    assert!(error.to_string().contains("link cycle"), "{error}");
    assert_eq!(doc.spatial_sketch_def(sketch).unwrap(), &def);
}
