// SPDX-License-Identifier: MIT
//! Align: bodies moved rigidly so that a point, axis or plane of theirs
//! lands on another. An .f3d design may record it as a move (experiment
//! K52); Mitcad keeps the geometry so that the alignment follows edits.
//!
//! - point to point, axis or plane: the point moves onto the target
//!   point, onto the nearest point of the target line, or straight onto the
//!   target plane;
//! - axis to axis: the direction turns onto the target direction and the
//!   axis's point (a circle's centre) onto the target line (onto the target
//!   centre for two circles);
//! - plane to plane: the faces end up against each other, normals opposite,
//!   and the plane's origin onto the target origin. A face's origin here
//!   is its middle (the frame's axes are the face's usual ones).
//!
//! `flip` turns the moved geometry over (directions and normals the same
//! way), and `angle` turns the bodies about the target axis or normal.

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Want};
use super::moves::{check_bodies, place_bodies};
use super::pattern::none;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::datum::DatumAxis as Axis;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::transform::{Transform, add, dot, rotation_between, scaled, sub};

/// The geometry an alignment moves from or to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AlignRef {
    Point { point: GeomRef },
    Axis { axis: GeomRef },
    Plane { plane: GeomRef },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlignDef<P = ParamId> {
    pub bodies: Vec<BodyUid>,
    pub from: AlignRef,
    pub to: AlignRef,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// Radians about the target axis or normal.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
}

impl<P> AlignDef<P> {
    pub const TYPE: &'static str = "align";
    pub const BASE_NAME: &'static str = "Align";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<AlignDef<Q>, E> {
        Ok(AlignDef {
            bodies: self.bodies.clone(),
            from: self.from.clone(),
            to: self.to.clone(),
            flip: self.flip,
            angle: match &self.angle {
                Some(angle) => Some(f("angle", angle)?),
                None => None,
            },
        })
    }
}

/// Resolved alignment geometry.
enum Resolved {
    Point([f64; 3]),
    Axis(Axis),
    Plane(SketchFrame),
}

impl AlignRef {
    fn reference(&self) -> (&GeomRef, Want) {
        match self {
            Self::Point { point } => (point, Want::Point),
            Self::Axis { axis } => (axis, Want::Axis),
            Self::Plane { plane } => (plane, Want::Plane),
        }
    }

    fn add_references(&self, references: &mut References) {
        self.reference().0.add_to(references);
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let (reference, want) = self.reference();
        reference.check(ctx, want)
    }

    fn resolve<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Resolved, String> {
        Ok(match self {
            Self::Point { point } => Resolved::Point(ctx.point(point)?),
            Self::Axis { axis } => Resolved::Axis(ctx.axis(axis)?),
            Self::Plane { plane } => {
                let mut frame = ctx.plane(plane)?.frame();
                if let GeomRef::Face { body, face } = plane {
                    let shape = ctx.body(*body)?;
                    frame.origin = ctx
                        .kernel
                        .face_plane(&shape, face)
                        .map_err(|e| format!("face {face}: {e}"))?
                        .origin;
                }
                Resolved::Plane(frame)
            }
        })
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Point { .. } => "a point",
            Self::Axis { .. } => "an axis",
            Self::Plane { .. } => "a plane",
        }
    }
}

/// The point of a line nearest to `p`.
fn foot(axis: &Axis, p: [f64; 3]) -> [f64; 3] {
    add(
        axis.origin,
        scaled(axis.direction, dot(sub(p, axis.origin), axis.direction)),
    )
}

/// The alignment as a transform, and the target axis for `angle`.
fn alignment(
    from: &Resolved,
    to: &Resolved,
    flip: bool,
) -> Result<(Transform, Option<Axis>), String> {
    let sign = if flip { -1.0 } else { 1.0 };
    Ok(match (from, to) {
        (Resolved::Point(p), Resolved::Point(q)) => (Transform::translation(sub(*q, *p)), None),
        (Resolved::Point(p), Resolved::Axis(axis)) => {
            (Transform::translation(sub(foot(axis, *p), *p)), Some(*axis))
        }
        (Resolved::Point(p), Resolved::Plane(frame)) => {
            let n = frame.normal();
            let offset = dot(sub(frame.origin, *p), n);
            (
                Transform::translation(scaled(n, offset)),
                Some(Axis {
                    origin: frame.origin,
                    direction: n,
                }),
            )
        }
        (Resolved::Axis(a), Resolved::Axis(b)) => {
            let fallback = crate::transform::perpendicular(a.direction);
            let turn = rotation_between(a.origin, a.direction, scaled(b.direction, sign), fallback);
            // The axis's own point lands on the target line; centres of
            // circles meet.
            let moved = turn.apply_point(a.origin);
            let target = foot(b, moved);
            (
                Transform::translation(sub(target, moved)).after(&turn),
                Some(*b),
            )
        }
        (Resolved::Plane(a), Resolved::Plane(b)) => {
            // Faces against each other: normals opposite unless flipped.
            let turn = rotation_between(a.origin, a.normal(), scaled(b.normal(), -sign), a.x_axis);
            let moved = turn.apply_point(a.origin);
            (
                Transform::translation(sub(b.origin, moved)).after(&turn),
                Some(Axis {
                    origin: b.origin,
                    direction: b.normal(),
                }),
            )
        }
        _ => return Err("only a point can be aligned to geometry of another kind".to_owned()),
    })
}

impl FeatureInfo for AlignDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        self.from.add_references(&mut references);
        self.to.add_references(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_bodies(ctx, &self.bodies)?;
        self.from.check(ctx)?;
        self.to.check(ctx)?;
        let compatible = matches!(
            (&self.from, &self.to),
            (AlignRef::Point { .. }, _)
                | (AlignRef::Axis { .. }, AlignRef::Axis { .. })
                | (AlignRef::Plane { .. }, AlignRef::Plane { .. })
        );
        if compatible {
            Ok(())
        } else {
            Err(format!(
                "cannot align {} to {}",
                self.from.kind(),
                self.to.kind()
            ))
        }
    }
}

impl<K: Kernel> Evaluate<K> for AlignDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let from = self.from.resolve(ctx)?;
        let to = self.to.resolve(ctx)?;
        let (mut transform, axis) = alignment(&from, &to, self.flip)?;
        if let Some(angle) = self.angle {
            let angle = ctx.param(angle)?;
            if angle != 0.0 {
                let axis = axis.ok_or("an alignment to a point has no axis to turn about")?;
                let turn = Transform::rotation(axis.origin, axis.direction, angle)
                    .expect("a unit axis direction");
                transform = turn.after(&transform);
            }
        }
        place_bodies(ctx, &self.bodies, &transform, false)
    }
}
