// SPDX-License-Identifier: MIT
//! Coordinate System (mitcad#1775): a frame of construction geometry with
//! its origin at a point and its axes along edges, axes or faces, named
//! `CoordinateSystem1`. Its datum is a plane with that frame (as a joint
//! origin's): x and y along the frame's X and Y, the normal along its Z,
//! so that sketches, joints and Align take it as a frame, and an export can
//! write the bodies in it (`coordinates: {"frame": ...}`, `exchange.rs`).
//!
//! - `origin`: a vertex, a construction point, the origin point, a circular
//!   edge's center (a straight edge's middle) or a sketch point;
//! - `z_axis`: an edge, an axis, a plane's or planar face's normal, or a
//!   cylinder's, cone's or torus's axis; the component's Z when left out;
//! - `x_axis`: the same, turned square to Z (projected onto the plane
//!   normal to it); the component's X so turned when left out (its Y when X
//!   is along Z);
//! - a straight edge points away from the origin: from its end nearer the
//!   origin to the other one, so that edges picked at a corner point into
//!   the body; another reference keeps its own direction (an edge's curve
//!   direction for a circle, a face's outward normal);
//! - `flip_z` and `flip_x` turn each axis round (Y follows, the frame stays
//!   right-handed); `offset` moves the origin along Z (mm) and `angle` turns
//!   X and Y about Z (radians), after the flips.
//!
//! The references are geometry of the feature's own component, in its
//! coordinates, as a joint origin's.

use serde::{Deserialize, Serialize};

use super::construction::{datum_kind, distinct, finite};
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::pattern::none;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::datum::{CurveGeometry, Datum, DatumKind, DatumPlane, SurfaceGeometry, Vec3};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinateSystemDef<P = ParamId> {
    /// The frame's origin: a point (see the module).
    pub origin: GeomRef,
    /// The direction of Z; the component's Z when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_axis: Option<GeomRef>,
    /// The direction of X, turned square to Z; the component's X when left
    /// out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_axis: Option<GeomRef>,
    /// Turns Z round.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_z: bool,
    /// Turns X round.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_x: bool,
    /// Along Z, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// About Z, radians.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
}

impl<P> CoordinateSystemDef<P> {
    pub const TYPE: &'static str = "coordinate_system";
    pub const BASE_NAME: &'static str = "CoordinateSystem";

    /// A frame at `origin` with the component's axes.
    pub fn at(origin: GeomRef) -> Self {
        Self {
            origin,
            z_axis: None,
            x_axis: None,
            flip_z: false,
            flip_x: false,
            offset: None,
            angle: None,
        }
    }

    map_params! {
        CoordinateSystemDef {
            origin: clone,
            z_axis: clone,
            x_axis: clone,
            flip_z: copy,
            flip_x: copy,
            offset: option,
            angle: option,
        }
    }

    fn axes(&self) -> impl Iterator<Item = &GeomRef> {
        self.z_axis.iter().chain(&self.x_axis)
    }
}

impl FeatureInfo for CoordinateSystemDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.origin.add_to(&mut references);
        for axis in self.axes() {
            axis.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.origin.check(ctx, Want::Point)?;
        for axis in self.axes() {
            check_direction(ctx, axis)?;
        }
        distinct(&self.axes().collect::<Vec<_>>())
    }
}

/// Checks that a reference can give an axis's direction: an axis or a line
/// (a straight or circular edge, a sketch line, a face of revolution), or a
/// plane (an origin or construction plane, a planar face, a fixed plane).
fn check_direction(ctx: &CheckContext<'_>, reference: &GeomRef) -> Result<(), String> {
    match reference {
        GeomRef::Origin(origin) if origin.kind() == DatumKind::Point => Err(format!(
            "{reference} is a point, which gives no direction for an axis"
        )),
        GeomRef::Origin(_) => Ok(()),
        GeomRef::Datum(uid) => {
            let entry = ctx.feature(*uid)?;
            match datum_kind(&entry.def) {
                Some(DatumKind::Plane | DatumKind::Axis) => Ok(()),
                Some(DatumKind::Point) => Err(format!(
                    "{} is a construction point, which gives no direction for an axis",
                    entry.name
                )),
                None => Err(format!(
                    "{} ({uid}) is not construction geometry",
                    entry.name
                )),
            }
        }
        // A face may be planar (its normal) or round about an axis; only
        // evaluation can tell.
        GeomRef::FixedPlane { .. } => reference.check(ctx, Want::Plane),
        _ => reference.check(ctx, Want::Axis),
    }
}

impl<K: Kernel> Evaluate<K> for CoordinateSystemDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let origin = ctx.point(&self.origin)?;
        let z = match &self.z_axis {
            Some(reference) => direction(ctx, reference, origin)?,
            None => [0.0, 0.0, 1.0],
        };
        let z = if self.flip_z { scale(z, -1.0) } else { z };
        let x = match &self.x_axis {
            Some(reference) => {
                let along = direction(ctx, reference, origin)?;
                unit(sub(along, scale(z, dot(along, z))))
                    .ok_or("the X axis is along the Z axis: pick an X axis across it")?
            }
            // The component's X turned square to Z (its Y when X is along
            // Z: the default axis of a plane).
            None => DatumPlane::from_normal(origin, z, Some([1.0, 0.0, 0.0])).x_axis,
        };
        let x = if self.flip_x { scale(x, -1.0) } else { x };
        let angle = match self.angle {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let offset = match self.offset {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let y = cross(z, x);
        let (sin, cos) = angle.sin_cos();
        let x_axis = add(scale(x, cos), scale(y, sin));
        let datum = Datum::Plane(DatumPlane {
            origin: add(origin, scale(z, offset)),
            x_axis,
            y_axis: cross(z, x_axis),
        });
        Ok(FeatureOutput {
            datum: Some(finite(datum)?),
            ..FeatureOutput::default()
        })
    }
}

/// The unit direction a reference gives an axis of the frame (see the
/// module): a straight edge away from `origin`, a plane's normal, an axis's
/// direction.
fn direction<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    reference: &GeomRef,
    origin: Vec3,
) -> Result<Vec3, String> {
    let along = match reference {
        GeomRef::Face { .. } => {
            match ctx.surface(reference)? {
                SurfaceGeometry::Plane { normal, .. } => normal,
                SurfaceGeometry::Cylinder { axis, .. }
                | SurfaceGeometry::Cone { axis, .. }
                | SurfaceGeometry::Torus { axis, .. } => axis,
                _ => {
                    return Err(reference.error(|what| {
                    format!("{what} is neither planar nor round about an axis: it gives no direction")
                }));
                }
            }
        }
        GeomRef::Edge { .. } => match ctx.curve(reference)? {
            // From the end nearer the origin to the other one.
            CurveGeometry::Line { start, end } => {
                if norm(sub(end, origin)) < norm(sub(start, origin)) {
                    sub(start, end)
                } else {
                    sub(end, start)
                }
            }
            CurveGeometry::Circle { .. } => ctx.axis(reference)?.direction,
            CurveGeometry::Other { .. } => {
                return Err(reference.error(|what| {
                    format!("{what} is neither straight nor circular: it gives no direction")
                }));
            }
        },
        GeomRef::Origin(datum) if datum.kind() == DatumKind::Plane => {
            ctx.plane(reference)?.normal()
        }
        GeomRef::FixedPlane { .. } => ctx.plane(reference)?.normal(),
        GeomRef::Datum(uid) => match ctx.datum(*uid)? {
            Datum::Plane(_) => ctx.plane(reference)?.normal(),
            _ => ctx.axis(reference)?.direction,
        },
        _ => ctx.axis(reference)?.direction,
    };
    unit(along).ok_or_else(|| reference.error(|what| format!("{what} gives no direction")))
}
