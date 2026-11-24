// SPDX-License-Identifier: MIT
//! Where a sketch's plane cuts the visible bodies (mitcad#2324): the curves
//! of the section, each with the face it lies on, in sketch coordinates, as
//! `sketch.project` with `section` makes them of that face. What the
//! drawing tools snap to besides the body edges in the plane: a point on a
//! section curve, its ends and the centre of a circle or arc, projected
//! (the face's section, linked) only when a point is placed there. The
//! bodies are those of the `plane_geometry` query (the visible ones before
//! the sketch, other components' where the design policy lets the sketch
//! reference them); a body whose box the plane does not cross is not cut.

use mitcad_vec3::cross;

use super::{Document, ModelError};
use super::{Inputs, PlaneItem, PlaneShape, box_reach, off_plane, unit};
use crate::datum::DatumPlane;
use crate::ids::FeatureUid;
use crate::kernel::{BoundingBox, Kernel};
use crate::sketch::project::{Projected, project, section_plane, transform_curve};
use crate::topo::TopoName;

impl<K: Kernel> Document<K> {
    /// The curves where the plane of `sketch` cuts the visible bodies before
    /// it (the `plane_geometry` query with `section`), in sketch
    /// coordinates, each as a [`PlaneItem`] whose `source` is the face it
    /// lies on (the `source` of `sketch.project` with `section`): a line,
    /// circle, arc, ellipse or B-spline per piece, as
    /// [`Kernel::section_curves`] gives the body's section. Only bodies
    /// whose bounding box the plane crosses are cut. Pieces on faces without
    /// names, and B-splines joined across faces (no single face's section
    /// gives them), are left out: nothing could follow them. A body the
    /// kernel cannot cut gives nothing. A job that reads it in the
    /// background stops at a cancel between bodies.
    pub fn plane_section(&self, sketch: FeatureUid) -> Result<Vec<PlaneItem>, ModelError> {
        let Inputs { frame, seen } = self.plane_inputs(sketch)?;
        let normal = unit(cross(frame.x_axis, frame.y_axis));
        let plane = DatumPlane {
            origin: frame.origin,
            x_axis: frame.x_axis,
            y_axis: frame.y_axis,
        };
        let kernel = &self.kernel;
        let mut items = Vec::new();
        for body in &seen {
            if self.monitor().is_some_and(|m| m.is_cancelled()) {
                return Err(ModelError::Cancelled);
            }
            let to_sketch = body.to_sketch.as_ref();
            let reaches = |b: &BoundingBox| {
                let (low, high, diagonal) = box_reach(b, to_sketch, frame.origin, normal);
                !off_plane(low, high, diagonal.max(1.0))
            };
            // The cheap enclosing box first, as for the edges in the plane;
            // the tight one only for a body that can reach the plane.
            let cut = match kernel.enclosing_box(body.shape) {
                Ok(Some(b)) if !reaches(&b) => false,
                _ => match kernel.bounding_box(body.shape) {
                    Ok(Some(b)) => reaches(&b),
                    _ => true,
                },
            };
            if !cut {
                continue;
            }
            // Cut where the body is: the sketch's plane moved back by the
            // link, the curves moved into the sketch's coordinates.
            let local = match to_sketch {
                Some(t) => section_plane(&crate::assembly::inverse(t), &plane),
                None => plane,
            };
            let Ok(pieces) = kernel.section_curves(body.shape, None, &local) else {
                continue;
            };
            for piece in pieces {
                let Some(face) = piece.faces.first() else {
                    continue;
                };
                if piece.faces.iter().any(|f| f != face) {
                    continue;
                }
                let curve = match to_sketch {
                    Some(t) => transform_curve(t, &piece.curve),
                    None => piece.curve,
                };
                let Some(Projected::Curve(c)) = project(std::slice::from_ref(&curve), &frame)
                    .into_iter()
                    .next()
                else {
                    continue;
                };
                items.push(PlaneItem {
                    source: TopoName::Face(face.clone()),
                    body: body.body,
                    occurrence: body.occurrence.clone(),
                    shape: PlaneShape::Curve(c),
                    out_of_plane: None,
                });
            }
        }
        Ok(items)
    }
}
