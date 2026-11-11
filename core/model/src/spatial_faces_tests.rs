// SPDX-License-Identifier: MIT
//! Points of 3D sketches on faces of bodies (mitcad#1549), with the mock
//! kernel (whose faces have the surfaces a test gives them): a line end on
//! the block's top face is determined by two offsets and follows the
//! block's height; a free point on it keeps two degrees of freedom; Detach
//! frees it and Undo brings it back; saved and read again; the block
//! suppressed is a reference failure with the definition kept. Points on a
//! cylinder and a sphere are solved in closed form, one on a free-form face
//! through the kernel's projection. A Fix on the face makes the relation
//! redundant, one off it a conflict. A converted sketch on a face puts a
//! corner on that face, which holds it once it is released from the plane.

use crate::datum::SurfaceGeometry;
use crate::features::spatial_sketch::{
    SpatialAxis, SpatialConstraintKind, SpatialDimensionKind, SpatialFailureKind, SpatialSource,
};
use crate::features::{SketchPlane, ValueInput};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::sketch::spatial_edit::SpatialPointInput::Existing;
use crate::test_support::{block, num, side};
use crate::testing::MockKernel;
use crate::topo::FaceName;
use crate::{Document, FeatureStatus};

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

fn face(body: BodyUid, face: &FaceName) -> SpatialSource {
    SpatialSource::Face {
        body,
        face: Box::new(face.clone()),
        link: None,
    }
}

fn offset(a: EntityUid, b: EntityUid, axis: SpatialAxis) -> SpatialDimensionKind {
    SpatialDimensionKind::Offset { a, b, axis }
}

/// A line from a fixed point to the block's top face (z = 20), its end 20
/// and 10 along X and Y from the start: the end is determined (DOF 0) and
/// follows the block to 30 high; a free point on the face has two degrees
/// of freedom; Detach, Undo, a saved and read design; the block suppressed
/// is a reference failure, nothing detached.
#[test]
fn a_line_end_on_a_planar_face_follows_the_block() {
    let mut b = block();
    let body = b.body;
    let top = FaceName::end(b.extrude, b.region.clone());
    let height = b.doc.feature(b.extrude).unwrap().def.params()[0];
    let height = b.doc.parameters().name(height);
    let sketch = b.doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((start, end, on), report) = b
        .doc
        .edit_spatial_sketch(sketch, "Add Line", |e| {
            let start = e.add_point([10.0, 10.0, 0.0])?;
            e.fix(start)?;
            let end = e.add_point([28.0, 21.0, 26.0])?;
            e.add_line(Existing(start), Existing(end), false)?;
            let on = e.attach(end, face(body, &top))?;
            for (axis, value) in [(SpatialAxis::X, 20.0), (SpatialAxis::Y, 10.0)] {
                e.add_dimension(
                    offset(start, end, axis),
                    Some(&ValueInput::Number(value)),
                    false,
                )?;
            }
            Ok((start, end, on))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.status.dof, 0, "{:?}", report.status);
    assert!(report.status.fully_constrained);
    assert!(near(point(&b.doc, sketch, end), [30.0, 20.0, 20.0]));
    let def = b.doc.spatial_sketch_def(sketch).unwrap().clone();
    assert!(matches!(
        &def.constraint(on).unwrap().kind,
        SpatialConstraintKind::OnFace { point, source } if *point == end && source.is_face()
    ));
    assert_eq!(def.face_records().count(), 1);

    // The block 30 high: the end stays on the top face.
    b.doc.set_parameter(&height, 30.0).unwrap();
    assert_eq!(status(&b.doc, sketch), FeatureStatus::Ok);
    assert!(near(point(&b.doc, sketch, end), [30.0, 20.0, 30.0]));
    assert!(near(point(&b.doc, sketch, start), [10.0, 10.0, 0.0]));

    // A free point on the face: one relation, two degrees of freedom left.
    let (free, report) = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let p = e.add_point([5.0, 5.0, 12.0])?;
            e.attach(p, face(body, &top))?;
            Ok(p)
        })
        .unwrap();
    assert_eq!(report.status.dof, 2, "{:?}", report.status);
    assert!(near(point(&b.doc, sketch, free), [5.0, 5.0, 30.0]));
    // The same face again is refused; as a projection too.
    let error = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| e.attach(free, face(body, &top)))
        .unwrap_err();
    assert!(error.to_string().contains("already"), "{error}");
    let error = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            e.attach_as(free, face(body, &top), true)
        })
        .unwrap_err();
    assert!(error.to_string().contains("not projected"), "{error}");

    // Detach takes the relation away and nothing else; Undo brings it back.
    let (removed, report) = b
        .doc
        .edit_spatial_sketch(sketch, "Detach", |e| e.detach(free))
        .unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(report.status.dof, 3);
    assert!(near(point(&b.doc, sketch, free), [5.0, 5.0, 30.0]));
    assert_eq!(b.doc.undo().as_deref(), Some("Detach in 3DSketch1"));
    assert_eq!(
        b.doc
            .spatial_sketch_def(sketch)
            .unwrap()
            .face_records()
            .count(),
        2
    );

    // Saved and read again: the records and the geometry.
    let file = b.doc.to_json();
    let mut again = Document::from_json(&file, MockKernel::default()).unwrap();
    again.recompute();
    assert_eq!(
        again.spatial_sketch_def(sketch),
        b.doc.spatial_sketch_def(sketch)
    );
    assert!(near(point(&again, sketch, end), [30.0, 20.0, 30.0]));

    // The block suppressed: the face is lost, a reference failure naming
    // the relation; the definition is kept and nothing is detached.
    let kept = b.doc.spatial_sketch_def(sketch).unwrap().clone();
    b.doc.set_suppressed(b.extrude, true).unwrap();
    assert!(matches!(status(&b.doc, sketch), FeatureStatus::Failed(_)));
    let output = b.doc.spatial_sketch(sketch).unwrap();
    let failure = output.failure.as_ref().expect("failed");
    assert_eq!(failure.kind, SpatialFailureKind::Reference, "{failure}");
    assert_eq!(failure.constraints, [on]);
    assert!(output.solved.is_none());
    assert_eq!(b.doc.spatial_sketch_def(sketch).unwrap(), &kept);
    b.doc.set_suppressed(b.extrude, false).unwrap();
    assert_eq!(status(&b.doc, sketch), FeatureStatus::Ok);
}

/// Points on a cylinder (the block's front side given one, radius 50
/// about Z) and on a sphere (its right side, radius 40 about the origin),
/// each at two offsets from the origin: determined in closed form. A point
/// on a free-form face (its back side, a sphere of radius 30 about
/// (0, 0, 100) that only the kernel's projection knows) reaches it from
/// the planes touching it.
#[test]
fn points_on_a_cylinder_a_sphere_and_a_free_form_face() {
    let mut b = block();
    let body = b.body;
    let front = side(b.extrude, 1, 0);
    let right = side(b.extrude, 1, 1);
    let back = side(b.extrude, 1, 2);
    {
        let kernel = b.doc.kernel();
        let mut surfaces = kernel.surfaces.borrow_mut();
        surfaces.insert(
            front.to_string(),
            SurfaceGeometry::Cylinder {
                origin: [0.0; 3],
                axis: [0.0, 0.0, 1.0],
                radius: 50.0,
            },
        );
        surfaces.insert(
            right.to_string(),
            SurfaceGeometry::Sphere {
                center: [0.0; 3],
                radius: 40.0,
            },
        );
        surfaces.insert(
            back.to_string(),
            SurfaceGeometry::Other {
                kind: "bspline".to_owned(),
            },
        );
        kernel
            .projected_faces
            .borrow_mut()
            .insert(back.to_string(), ([0.0, 0.0, 100.0], 30.0));
    }
    let sketch = b.doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((c, s, f), report) = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let o = e.add_point([0.0; 3])?;
            e.attach(o, SpatialSource::Origin)?;
            let c = e.add_point([38.0, 30.0, 5.0])?;
            e.attach(c, face(body, &front))?;
            let s = e.add_point([24.0, 0.0, 30.0])?;
            e.attach(s, face(body, &right))?;
            let f = e.add_point([0.0, 10.0, 75.0])?;
            e.attach(f, face(body, &back))?;
            for (p, axis, value) in [
                (c, SpatialAxis::Y, 30.0),
                (c, SpatialAxis::Z, 5.0),
                (s, SpatialAxis::X, 24.0),
                (s, SpatialAxis::Y, 0.0),
                (f, SpatialAxis::X, 0.0),
                (f, SpatialAxis::Y, 18.0),
            ] {
                e.add_dimension(offset(o, p, axis), Some(&ValueInput::Number(value)), false)?;
            }
            Ok((c, s, f))
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.status.dof, 0, "{:?}", report.status);
    assert!(near(point(&b.doc, sketch, c), [40.0, 30.0, 5.0]));
    assert!(near(point(&b.doc, sketch, s), [24.0, 0.0, 32.0]));
    // On the sphere about (0, 0, 100) of radius 30, 18 off its axis, on
    // the side the point started: z = 100 - 24.
    assert!(near(point(&b.doc, sketch, f), [0.0, 18.0, 76.0]));
    let output = b.doc.spatial_sketch(sketch).unwrap();
    assert_eq!(output.surfaces.len(), 3);
}

/// A point fixed on the top face and put on it: the relation is redundant
/// (kept, said); fixed off it, the relation is refused as a conflict with
/// the Fix, nothing changed.
#[test]
fn a_fixed_point_on_a_face_is_redundant_and_off_it_a_conflict() {
    let mut b = block();
    let body = b.body;
    let top = FaceName::end(b.extrude, b.region.clone());
    let sketch = b.doc.add_spatial_sketch(SketchPlane::Xy).unwrap().uid;
    let ((on, held), report) = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let p = e.add_point([10.0, 10.0, 20.0])?;
            let held = e.fix(p)?;
            let on = e.attach(p, face(body, &top))?;
            Ok((on, held))
        })
        .unwrap();
    assert_eq!(report.status.dof, 0);
    assert_eq!(report.status.redundant, [on], "{:?}", report.status);
    assert!(!report.status.redundant.contains(&held));

    let before = b.doc.spatial_sketch_def(sketch).unwrap().clone();
    let error = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| {
            let q = e.add_point([20.0, 10.0, 10.0])?;
            e.fix(q)?;
            e.attach(q, face(body, &top))
        })
        .unwrap_err();
    let failure = error.failure.expect("refused by the solve");
    assert_eq!(failure.kind, SpatialFailureKind::Conflict, "{failure}");
    assert_eq!(failure.constraints.len(), 2, "{failure}");
    assert_eq!(b.doc.spatial_sketch_def(sketch).unwrap(), &before);
}

/// A planar rectangle on the block's top face converted into a 3D sketch:
/// a corner put on that face is redundant with the sketch plane, and holds
/// the corner on the face once it is released from the plane (no freedom
/// comes back); it follows the block to 30 high.
#[test]
fn a_converted_sketch_on_a_face_puts_a_corner_on_it() {
    let mut b = block();
    let body = b.body;
    let top = FaceName::end(b.extrude, b.region.clone());
    let height = b.doc.feature(b.extrude).unwrap().def.params()[0];
    let height = b.doc.parameters().name(height);
    let sketch = b
        .doc
        .add_sketch(SketchPlane::Face {
            face: Box::new(top.clone()),
            body: Some(body),
        })
        .unwrap()
        .uid;
    b.doc
        .add_rectangle(sketch, [0.0, 0.0], &num(20.0), &num(10.0))
        .unwrap();
    let converted = b.doc.convert_sketch_to_spatial(sketch).unwrap();
    assert_eq!(converted.status.dof, 0, "{:?}", converted.status);
    let corner = b.doc.spatial_sketch_def(sketch).unwrap().points[2].id;
    let (on, report) = b
        .doc
        .edit_spatial_sketch(sketch, "Attach", |e| e.attach(corner, face(body, &top)))
        .unwrap();
    assert_eq!(report.status.redundant, [on], "{:?}", report.status);
    let (_, report) = b
        .doc
        .edit_spatial_sketch(sketch, "Release from Plane", |e| {
            e.release_from_plane(&[corner])
        })
        .unwrap();
    assert_eq!(report.status.dof, 0, "{:?}", report.status);
    assert!(report.status.redundant.is_empty(), "{:?}", report.status);
    assert!((point(&b.doc, sketch, corner)[2] - 20.0).abs() <= TOL);
    b.doc.set_parameter(&height, 30.0).unwrap();
    assert_eq!(status(&b.doc, sketch), FeatureStatus::Ok);
    assert!((point(&b.doc, sketch, corner)[2] - 30.0).abs() <= TOL);
}
