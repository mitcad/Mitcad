// SPDX-License-Identifier: MIT
//! Sections of a mesh body into sketches (mitcad#1871; commands.md,
//! "Mesh tools for reverse engineering"): where a sketch's plane cuts the
//! mesh's triangles, fitted to lines, arcs and circles within a tolerance
//! (`crate::mesh_tools::section`), put into the sketch as its own curves
//! (not linked to the mesh: they stay when the mesh changes, as an
//! inserted drawing's do). A series adds sketches on planes parallel to
//! the first, `spacing` apart along its normal, each with its section; one
//! undo step for all.
//!
//! The first sketch is one given, or one made on a plane (an origin or
//! construction plane, a planar face). The further sketches lie on the
//! XY plane of the first's component, in a frame of their own moved from
//! the first's along its normal (fixed: they do not follow its plane).

use super::{Document, ModelError, invalid};
use crate::exchange::MeshTolerance;
use crate::features::{FeatureDef, FrameDef, SketchDef, SketchPlane, ValueInput};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::mesh_tools::section::{SectionCurve, SectionCurves, fit_curves, plane_section};
use crate::sketch::edit::SketchEdit;
use crate::sketch::{Entity, EntityKind};

/// The most sketches a series makes.
pub const MAX_SECTIONS: usize = 200;

/// Where the first section goes.
#[derive(Debug, Clone)]
pub enum SectionTarget {
    /// Into this sketch.
    Sketch(FeatureUid),
    /// Into a new sketch on this plane, in the active component.
    Plane(SketchPlane),
}

/// What [`Document::mesh_sections`] does.
#[derive(Debug, Clone)]
pub struct MeshSectionRequest {
    pub target: SectionTarget,
    pub body: BodyUid,
    /// How far a fitted curve may lie from the section's points (mm).
    pub tolerance: f64,
    /// Sections in all, the first included.
    pub count: usize,
    /// Between neighbouring sections, along the first sketch's normal (mm).
    pub spacing: f64,
}

/// What [`Document::mesh_sections`] made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshSections {
    /// The sketches the sections went into, the first first.
    pub sketches: Vec<FeatureUid>,
    pub lines: usize,
    pub arcs: usize,
    pub circles: usize,
    /// Planes of the series that do not cut the mesh (no sketch made).
    pub missed: usize,
}

const AS_STORED: MeshTolerance = MeshTolerance {
    deviation: 0.01,
    angle: 0.1,
    face_triangles: 0,
    max_edge: 0.0,
};

impl<K: Kernel> Document<K> {
    /// Sections of a mesh body into sketches (see the module docs).
    pub fn mesh_sections(
        &mut self,
        request: &MeshSectionRequest,
    ) -> Result<MeshSections, ModelError> {
        let MeshSectionRequest {
            target,
            body,
            tolerance,
            count,
            spacing,
        } = request.clone();
        if !(tolerance.is_finite() && tolerance > 0.0 && tolerance <= 10.0) {
            return Err(invalid(format!(
                "the tolerance must be more than 0 and at most 10 mm, got {tolerance} mm"
            )));
        }
        if !(1..=MAX_SECTIONS).contains(&count) {
            return Err(invalid(format!(
                "the number of sections must be from 1 to {MAX_SECTIONS}, got {count}"
            )));
        }
        if count > 1 && !(spacing.is_finite() && spacing != 0.0) {
            return Err(invalid(
                "sections in a series need a spacing other than 0".to_owned(),
            ));
        }
        let depth = self.undo_depth();
        let result = self.sections_into(target, body, tolerance, count, spacing);
        match result {
            Ok(made) => {
                let label = if made.sketches.len() > 1 {
                    "Mesh Sections"
                } else {
                    "Mesh Section"
                };
                self.merge_undo(depth, label);
                Ok(made)
            }
            Err(e) => {
                while self.undo_depth() > depth {
                    if self.undo().is_none() {
                        break;
                    }
                }
                Err(e)
            }
        }
    }

    fn sections_into(
        &mut self,
        target: SectionTarget,
        body: BodyUid,
        tolerance: f64,
        count: usize,
        spacing: f64,
    ) -> Result<MeshSections, ModelError> {
        let sketch = match target {
            SectionTarget::Sketch(sketch) => sketch,
            SectionTarget::Plane(plane) => {
                let def = FeatureDef::Sketch(SketchDef::<ValueInput>::new(plane));
                self.add_feature(&def, None)?.uid
            }
        };
        let fail = |e: String| invalid(format!("mesh section: {e}"));
        let found = self.projection_target(sketch, None, None, &fail)?;
        let component = self.state.features[found.position].component;
        let frame = found.frame;
        let shape = self
            .bodies_before(found.position)
            .remove(&body)
            .ok_or_else(|| {
                invalid(crate::notice::said(
                    "the mesh body is not there before the sketch",
                    format!("mesh section: body {body} is not before sketch {sketch}"),
                ))
            })?;
        if self.body_component(body).is_some_and(|c| c != component) {
            return Err(invalid(crate::notice::said(
                "the mesh body is in another component than the sketch",
                format!("mesh section: body {body} is not in the sketch's component"),
            )));
        }
        if !crate::features::surface_bodies::is_mesh(&self.kernel, &shape) {
            return Err(invalid(crate::notice::said(
                "Mesh Section cuts mesh bodies (triangles from an STL, OBJ or 3MF file); the body \
                 picked is not one (a sketch's Intersect cuts solids and surfaces)",
                format!("mesh section: body {body} is not a mesh body"),
            )));
        }
        let mesh = self
            .kernel
            .triangle_mesh(&shape, &AS_STORED)
            .map_err(|e| invalid(format!("mesh section: the mesh's triangles: {e}")))?;
        let mesh = crate::mesh_tools::joined(&mesh);
        let mut made = MeshSections::default();
        let normal = frame.normal();
        for k in 0..count {
            let offset = spacing * k as f64;
            let curves = fit_curves(&plane_section(&mesh, &frame, offset), tolerance);
            if curves.count() == 0 {
                if k == 0 {
                    return Err(invalid(
                        "the sketch's plane does not cut the mesh".to_owned(),
                    ));
                }
                made.missed += 1;
                continue;
            }
            let into = if k == 0 {
                sketch
            } else {
                let mut def = SketchDef::<ValueInput>::new(SketchPlane::Xy);
                def.frame = Some(Box::new(FrameDef {
                    origin: std::array::from_fn(|i| frame.origin[i] + offset * normal[i]),
                    x_axis: frame.x_axis,
                    y_axis: frame.y_axis,
                }));
                self.add_feature_to(&FeatureDef::Sketch(def), None, Some(component))?
                    .uid
            };
            let (counted, _) = self.edit_sketch(into, "Mesh Section into {sketch}", |edit| {
                Ok(insert_curves(edit, &curves))
            })?;
            made.lines += counted[0];
            made.arcs += counted[1];
            made.circles += counted[2];
            made.sketches.push(into);
        }
        Ok(made)
    }
}

/// The section's curves put into the sketch, neighbouring curves sharing
/// their end points; the lines, arcs and circles added.
fn insert_curves(edit: &mut SketchEdit<'_>, curves: &SectionCurves) -> [usize; 3] {
    let mut counted = [0; 3];
    for chain in &curves.chains {
        // The chain's points by their place (ends are copies of the same
        // section point, so they are equal).
        let mut points: Vec<([f64; 2], EntityUid)> = Vec::new();
        let mut point = |edit: &mut SketchEdit<'_>, p: [f64; 2]| -> EntityUid {
            if let Some((_, id)) = points
                .iter()
                .find(|(q, _)| (q[0] - p[0]).hypot(q[1] - p[1]) <= 1e-9)
            {
                return *id;
            }
            let id = edit.add_point(p);
            points.push((p, id));
            id
        };
        for curve in chain {
            let kind = match *curve {
                SectionCurve::Line { start, end } => {
                    counted[0] += 1;
                    EntityKind::Line {
                        start: point(edit, start),
                        end: point(edit, end),
                        centerline: false,
                    }
                }
                SectionCurve::Arc { center, start, end } => {
                    counted[1] += 1;
                    let center = edit.add_point(center);
                    EntityKind::Arc {
                        center,
                        start: point(edit, start),
                        end: point(edit, end),
                    }
                }
                SectionCurve::Circle { center, radius } => {
                    counted[2] += 1;
                    EntityKind::Circle {
                        center: edit.add_point(center),
                        radius,
                        start: 0.0,
                    }
                }
            };
            let id = edit.id();
            edit.push(Entity::new(id, kind));
        }
    }
    counted
}
