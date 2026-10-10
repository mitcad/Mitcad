// SPDX-License-Identifier: MIT
//! Construction planes (also those of imported .f3d designs). Each
//! definition gives the plane and an in-plane frame, which sketches on the
//! plane use. Mitcad's rules (experiment K67 tests them on .f3d designs):
//!
//! - offset: the reference frame moved along its normal (a face's normal is
//!   outward); a negative distance goes the other way;
//! - angle: the plane through the line, the reference plane turned about
//!   the line's direction by the angle (right-handed); at 0 it is parallel
//!   to the reference plane, or as close to it as a plane through the line
//!   can be;
//! - tangent: the plane touching a cylinder or cone along the line where
//!   the reference plane's normal, turned about the face's axis by the
//!   angle, leaves the face; the normal points away from the axis;
//! - midplane: halfway between parallel planes with the first plane's
//!   normal and frame; between crossing planes the bisector through their
//!   intersection with the normal along n1 - n2 (between two faces of a
//!   solid, the plane through its inside);
//! - through two edges, an edge and a point, three points: the normal is
//!   d1 × d2 (d1 × (o2 - o1) for parallel lines), d × (p - o) and
//!   (p2 - p1) × (p3 - p1); the frame starts at the first line or point
//!   with x along it;
//! - tangent at a point, along a path, normal at a point: the outward face
//!   normal or the path's tangent, with the default x axis.

use serde::{Deserialize, Serialize};

use super::{GeomRef, PathRef, Want, distinct, finite};
use crate::datum::{
    ANGULAR, Datum, DatumAxis, DatumPlane, LINEAR, PathParameter, SurfaceGeometry, Vec3, add, norm,
    rotate, rotate_onto, scale, sub, unit,
};
use crate::features::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::{cross, dot};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructionPlaneDef<P = ParamId> {
    pub definition: PlaneDefinition<P>,
    /// Reverses the normal (y axis), e.g. to match an imported design whose
    /// stored plane faces the other way (K67).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
}

/// How the plane is defined; lengths in millimetres, angles in radians.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaneDefinition<P = ParamId> {
    /// Offset Plane: parallel to a plane or planar face.
    Offset { plane: GeomRef, distance: P },
    /// Plane at Angle: through a line, at an angle to a plane.
    Angle {
        line: GeomRef,
        angle: P,
        plane: GeomRef,
    },
    /// Tangent Plane: touching a cylindrical or conical face, at an angle
    /// about its axis from a reference plane.
    Tangent {
        face: GeomRef,
        angle: P,
        plane: GeomRef,
    },
    /// Midplane between two planes or planar faces.
    Midplane { plane1: GeomRef, plane2: GeomRef },
    /// Plane Through Two Edges: two lines in one plane.
    TwoEdges { line1: GeomRef, line2: GeomRef },
    /// Plane Through Three Points.
    ThreePoints {
        point1: GeomRef,
        point2: GeomRef,
        point3: GeomRef,
    },
    /// Through a line and a point off it.
    EdgeAndPoint { line: GeomRef, point: GeomRef },
    /// Tangent to a face at the face point nearest to a point.
    TangentAtPoint { face: GeomRef, point: GeomRef },
    /// Plane Along Path: normal to a path at a fraction of its length, or
    /// at a length (mm) from its start with `physical`.
    AlongPath {
        path: PathRef,
        distance: P,
        #[serde(default, skip_serializing_if = "is_false")]
        physical: bool,
    },
    /// Normal to a path at the path point nearest to a point.
    NormalAtPoint { path: PathRef, point: GeomRef },
    /// A fixed plane (the non-parametric `setByPlane` of .f3d designs, and the
    /// importer's fallback for definitions Mitcad lacks): its origin and
    /// frame. The x axis is normalized and y made perpendicular to it.
    Fixed {
        origin: Vec3,
        x_axis: Vec3,
        y_axis: Vec3,
    },
}

impl<P> ConstructionPlaneDef<P> {
    pub const TYPE: &'static str = "construction_plane";
    pub const BASE_NAME: &'static str = "Plane";

    pub fn new(definition: PlaneDefinition<P>) -> Self {
        Self {
            definition,
            flip: false,
        }
    }

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ConstructionPlaneDef<Q>, E> {
        use PlaneDefinition as D;
        let definition = match &self.definition {
            D::Offset { plane, distance } => D::Offset {
                plane: plane.clone(),
                distance: f("definition.distance", distance)?,
            },
            D::Angle { line, angle, plane } => D::Angle {
                line: line.clone(),
                angle: f("definition.angle", angle)?,
                plane: plane.clone(),
            },
            D::Tangent { face, angle, plane } => D::Tangent {
                face: face.clone(),
                angle: f("definition.angle", angle)?,
                plane: plane.clone(),
            },
            D::Midplane { plane1, plane2 } => D::Midplane {
                plane1: plane1.clone(),
                plane2: plane2.clone(),
            },
            D::TwoEdges { line1, line2 } => D::TwoEdges {
                line1: line1.clone(),
                line2: line2.clone(),
            },
            D::ThreePoints {
                point1,
                point2,
                point3,
            } => D::ThreePoints {
                point1: point1.clone(),
                point2: point2.clone(),
                point3: point3.clone(),
            },
            D::EdgeAndPoint { line, point } => D::EdgeAndPoint {
                line: line.clone(),
                point: point.clone(),
            },
            D::TangentAtPoint { face, point } => D::TangentAtPoint {
                face: face.clone(),
                point: point.clone(),
            },
            D::AlongPath {
                path,
                distance,
                physical,
            } => D::AlongPath {
                path: path.clone(),
                distance: f("definition.distance", distance)?,
                physical: *physical,
            },
            D::NormalAtPoint { path, point } => D::NormalAtPoint {
                path: path.clone(),
                point: point.clone(),
            },
            D::Fixed {
                origin,
                x_axis,
                y_axis,
            } => D::Fixed {
                origin: *origin,
                x_axis: *x_axis,
                y_axis: *y_axis,
            },
        };
        Ok(ConstructionPlaneDef {
            definition,
            flip: self.flip,
        })
    }

    /// The references with what each must be.
    fn inputs(&self) -> Vec<(&GeomRef, Want)> {
        use PlaneDefinition as D;
        match &self.definition {
            D::Offset { plane, .. } => vec![(plane, Want::Plane)],
            D::Angle { line, plane, .. } => vec![(line, Want::Line), (plane, Want::Plane)],
            D::Tangent { face, plane, .. } => vec![(face, Want::Face), (plane, Want::Plane)],
            D::Midplane { plane1, plane2 } => vec![(plane1, Want::Plane), (plane2, Want::Plane)],
            D::TwoEdges { line1, line2 } => vec![(line1, Want::Line), (line2, Want::Line)],
            D::ThreePoints {
                point1,
                point2,
                point3,
            } => vec![
                (point1, Want::Point),
                (point2, Want::Point),
                (point3, Want::Point),
            ],
            D::EdgeAndPoint { line, point } => vec![(line, Want::Line), (point, Want::Point)],
            D::TangentAtPoint { face, point } => vec![(face, Want::Face), (point, Want::Point)],
            D::AlongPath { .. } | D::Fixed { .. } => Vec::new(),
            D::NormalAtPoint { point, .. } => vec![(point, Want::Point)],
        }
    }

    fn path(&self) -> Option<&PathRef> {
        match &self.definition {
            PlaneDefinition::AlongPath { path, .. }
            | PlaneDefinition::NormalAtPoint { path, .. } => Some(path),
            _ => None,
        }
    }
}

impl FeatureInfo for ConstructionPlaneDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for (reference, _) in self.inputs() {
            reference.add_to(&mut references);
        }
        if let Some(path) = self.path() {
            path.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let inputs = self.inputs();
        for (reference, want) in &inputs {
            reference.check(ctx, *want)?;
        }
        distinct(&inputs.iter().map(|(r, _)| *r).collect::<Vec<_>>())?;
        if let Some(path) = self.path() {
            path.check(ctx)?;
        }
        if let PlaneDefinition::Fixed {
            origin,
            x_axis,
            y_axis,
        } = &self.definition
        {
            fixed(*origin, *x_axis, *y_axis)?;
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ConstructionPlaneDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        use PlaneDefinition as D;
        let plane = match &self.definition {
            D::Offset { plane, distance } => {
                let base = ctx.plane(plane)?;
                let distance = ctx.param(*distance)?;
                base.translated(scale(base.normal(), distance))
            }
            D::Angle { line, angle, plane } => {
                let line = ctx.axis(line)?;
                let base = ctx.plane(plane)?;
                at_angle(&line, &base, ctx.param(*angle)?)?
            }
            D::Tangent { face, angle, plane } => {
                let surface = ctx.surface(face)?;
                let base = ctx.plane(plane)?;
                tangent(&surface, &base, ctx.param(*angle)?)?
            }
            D::Midplane { plane1, plane2 } => midplane(&ctx.plane(plane1)?, &ctx.plane(plane2)?),
            D::TwoEdges { line1, line2 } => through_lines(&ctx.axis(line1)?, &ctx.axis(line2)?)?,
            D::ThreePoints {
                point1,
                point2,
                point3,
            } => through_points(ctx.point(point1)?, ctx.point(point2)?, ctx.point(point3)?)?,
            D::EdgeAndPoint { line, point } => {
                through_line_and_point(&ctx.axis(line)?, ctx.point(point)?)?
            }
            D::TangentAtPoint { face, point } => {
                let point = ctx.point(point)?;
                let touch = ctx.face_point(face, point)?;
                DatumPlane::from_normal(touch.point, touch.normal, None)
            }
            D::AlongPath {
                path,
                distance,
                physical,
            } => {
                let value = ctx.param(*distance)?;
                let at = if *physical {
                    PathParameter::Length(value)
                } else {
                    PathParameter::Fraction(value)
                };
                let on = ctx.path_point(path, at)?;
                DatumPlane::from_normal(on.point, on.tangent, None)
            }
            D::NormalAtPoint { path, point } => {
                let point = ctx.point(point)?;
                let on = ctx.path_point(path, PathParameter::Near(point))?;
                DatumPlane::from_normal(on.point, on.tangent, None)
            }
            D::Fixed {
                origin,
                x_axis,
                y_axis,
            } => fixed(*origin, *x_axis, *y_axis)?,
        };
        let plane = if self.flip {
            DatumPlane {
                y_axis: scale(plane.y_axis, -1.0),
                ..plane
            }
        } else {
            plane
        };
        Ok(FeatureOutput {
            datum: Some(finite(Datum::Plane(plane))?),
            ..FeatureOutput::default()
        })
    }
}

/// A fixed plane from its origin and (nearly) perpendicular axes.
pub(crate) fn fixed(origin: Vec3, x_axis: Vec3, y_axis: Vec3) -> Result<DatumPlane, String> {
    if !origin
        .iter()
        .chain(&x_axis)
        .chain(&y_axis)
        .all(|v| v.is_finite())
    {
        return Err("the plane's numbers must be finite".to_owned());
    }
    let x_axis = unit(x_axis).ok_or("the plane's x axis is zero")?;
    let y_axis = unit(sub(y_axis, scale(x_axis, dot(y_axis, x_axis))))
        .filter(|_| norm(cross(x_axis, y_axis)) > ANGULAR * norm(y_axis))
        .ok_or("the plane's y axis is zero or along its x axis")?;
    Ok(DatumPlane {
        origin,
        x_axis,
        y_axis,
    })
}

/// The plane through `line` turned by `angle` about it from `base`.
pub(crate) fn at_angle(
    line: &DatumAxis,
    base: &DatumPlane,
    angle: f64,
) -> Result<DatumPlane, String> {
    let d = line.direction;
    let n0 = base.normal();
    // The plane through the line closest to parallel with the base.
    let n1 = unit(sub(n0, scale(d, dot(n0, d))))
        .ok_or("the line is at right angles to the reference plane")?;
    let turn = |v: Vec3| rotate(rotate_onto(v, n0, n1), d, angle);
    Ok(DatumPlane {
        origin: line.project(base.origin),
        x_axis: turn(base.x_axis),
        y_axis: turn(base.y_axis),
    })
}

/// The plane touching a cylinder or cone, at `angle` about its axis from
/// the direction of the reference plane's normal.
pub(crate) fn tangent(
    surface: &SurfaceGeometry,
    base: &DatumPlane,
    angle: f64,
) -> Result<DatumPlane, String> {
    let (origin, axis, radius, half_angle) = match *surface {
        SurfaceGeometry::Cylinder {
            origin,
            axis,
            radius,
        } => (origin, axis, radius, 0.0),
        SurfaceGeometry::Cone {
            origin,
            axis,
            radius,
            half_angle,
        } => (origin, axis, radius, half_angle),
        ref other => {
            return Err(format!(
                "a tangent plane needs a cylindrical or conical face, not a {} face",
                other.kind()
            ));
        }
    };
    let n0 = base.normal();
    let u0 = unit(sub(n0, scale(axis, dot(n0, axis))))
        .ok_or("the reference plane is at right angles to the face's axis")?;
    let u = rotate(u0, axis, angle);
    let (sin, cos) = half_angle.sin_cos();
    let normal = sub(scale(u, cos), scale(axis, sin));
    let line = DatumAxis {
        origin: add(origin, scale(u, radius)),
        direction: add(scale(axis, cos), scale(u, sin)),
    };
    let turn = |v: Vec3| rotate_onto(rotate(rotate_onto(v, n0, u0), axis, angle), u, normal);
    Ok(DatumPlane {
        origin: line.project(base.origin),
        x_axis: turn(base.x_axis),
        y_axis: turn(base.y_axis),
    })
}

pub(crate) fn midplane(first: &DatumPlane, second: &DatumPlane) -> DatumPlane {
    let (n1, n2) = (first.normal(), second.normal());
    match first.intersection(second) {
        None => {
            let gap = dot(sub(second.origin, first.origin), n1);
            first.translated(scale(n1, gap / 2.0))
        }
        Some(line) => {
            let normal = unit(sub(n1, n2)).expect("crossing planes have different normals");
            DatumPlane::from_normal(line.origin, normal, Some(first.x_axis))
        }
    }
}

pub(crate) fn through_lines(first: &DatumAxis, second: &DatumAxis) -> Result<DatumPlane, String> {
    let d1 = first.direction;
    let gap = sub(second.origin, first.origin);
    let crossing = cross(d1, second.direction);
    let normal = if norm(crossing) < ANGULAR {
        unit(cross(d1, gap))
            .filter(|_| first.distance_to(second.origin) > LINEAR)
            .ok_or("the two lines are the same line")?
    } else {
        let normal = scale(crossing, 1.0 / norm(crossing));
        let apart = dot(gap, normal).abs();
        if apart > LINEAR {
            return Err(format!(
                "the two lines do not lie in one plane (they pass {apart:.6} mm apart)"
            ));
        }
        normal
    };
    Ok(DatumPlane::from_normal(first.origin, normal, Some(d1)))
}

pub(crate) fn through_points(p1: Vec3, p2: Vec3, p3: Vec3) -> Result<DatumPlane, String> {
    let (a, b) = (sub(p2, p1), sub(p3, p1));
    let normal = cross(a, b);
    let size = norm(a).max(norm(b));
    // Points that are not finite are refused here too.
    if !normal.iter().all(|v| v.is_finite()) || norm(normal) <= LINEAR * size.max(1.0) {
        return Err("the three points lie on one line".to_owned());
    }
    let normal = unit(normal).ok_or("the three points lie on one line")?;
    Ok(DatumPlane::from_normal(p1, normal, Some(a)))
}

pub(crate) fn through_line_and_point(line: &DatumAxis, point: Vec3) -> Result<DatumPlane, String> {
    let off = line.distance_to(point);
    if off.is_nan() || off <= LINEAR {
        return Err("the point lies on the line".to_owned());
    }
    let normal =
        unit(cross(line.direction, sub(point, line.origin))).ok_or("the point lies on the line")?;
    Ok(DatumPlane::from_normal(
        line.origin,
        normal,
        Some(line.direction),
    ))
}
