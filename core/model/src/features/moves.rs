// SPDX-License-Identifier: MIT
//! Move/Copy of bodies: a rigid motion given as a matrix (free move),
//! distances along the model axes or an axis, a rotation about an axis, or
//! from a point to a point or a position. A moved body keeps its id and its
//! faces their names. With `copy` the bodies stay and moved copies become
//! new bodies, `<move>.b<j>` with faces `<move>:inst1(<name>)`; a copy that
//! does not move is a copy and paste of bodies.

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Want};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::{Instance, Transform, scaled, sub};

/// The motion of a move. Distances in millimetres, angles in radians.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MoveSpec<P = ParamId> {
    /// Rows of a rotation and a translation, `[[r00, r01, r02, tx], …]`
    /// (the `defineAsFreeMove` matrix of .f3d designs, in millimetres).
    Free { matrix: [[f64; 4]; 3] },
    /// Along the model axes.
    TranslateXyz { x: P, y: P, z: P },
    /// Along an axis (an edge's own direction).
    TranslateAlong { axis: GeomRef, distance: P },
    /// About an axis, right-handed.
    Rotate { axis: GeomRef, angle: P },
    /// Takes `from` onto `to`.
    PointToPoint { from: GeomRef, to: GeomRef },
    /// Takes `point` to the position.
    PointToPosition { point: GeomRef, x: P, y: P, z: P },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveDef<P = ParamId> {
    pub bodies: Vec<BodyUid>,
    pub transform: MoveSpec<P>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub copy: bool,
}

impl<P> MoveDef<P> {
    pub const TYPE: &'static str = "move";
    pub const BASE_NAME: &'static str = "Move";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<MoveDef<Q>, E> {
        Ok(MoveDef {
            bodies: self.bodies.clone(),
            transform: self.transform.map_params(f)?,
            copy: self.copy,
        })
    }
}

impl<P> MoveSpec<P> {
    /// Converts the value fields (slots `transform.x`, ...).
    pub(crate) fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<MoveSpec<Q>, E> {
        Ok(match self {
            MoveSpec::Free { matrix } => MoveSpec::Free { matrix: *matrix },
            MoveSpec::TranslateXyz { x, y, z } => MoveSpec::TranslateXyz {
                x: f("transform.x", x)?,
                y: f("transform.y", y)?,
                z: f("transform.z", z)?,
            },
            MoveSpec::TranslateAlong { axis, distance } => MoveSpec::TranslateAlong {
                axis: axis.clone(),
                distance: f("transform.distance", distance)?,
            },
            MoveSpec::Rotate { axis, angle } => MoveSpec::Rotate {
                axis: axis.clone(),
                angle: f("transform.angle", angle)?,
            },
            MoveSpec::PointToPoint { from, to } => MoveSpec::PointToPoint {
                from: from.clone(),
                to: to.clone(),
            },
            MoveSpec::PointToPosition { point, x, y, z } => MoveSpec::PointToPosition {
                point: point.clone(),
                x: f("transform.x", x)?,
                y: f("transform.y", y)?,
                z: f("transform.z", z)?,
            },
        })
    }
}

impl MoveSpec {
    pub(crate) fn add_references(&self, references: &mut References) {
        match self {
            Self::TranslateAlong { axis, .. } | Self::Rotate { axis, .. } => {
                axis.add_to(references);
            }
            Self::PointToPoint { from, to } => {
                from.add_to(references);
                to.add_to(references);
            }
            Self::PointToPosition { point, .. } => point.add_to(references),
            Self::Free { .. } | Self::TranslateXyz { .. } => {}
        }
    }

    pub(crate) fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match self {
            Self::Free { matrix } => free_transform(matrix).map(|_| ()),
            Self::TranslateXyz { .. } => Ok(()),
            Self::TranslateAlong { axis, .. } | Self::Rotate { axis, .. } => {
                axis.check(ctx, Want::Axis)
            }
            Self::PointToPoint { from, to } => {
                from.check(ctx, Want::Point)?;
                to.check(ctx, Want::Point)
            }
            Self::PointToPosition { point, .. } => point.check(ctx, Want::Point),
        }
    }

    pub(crate) fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Transform, String> {
        let transform = match self {
            Self::Free { matrix } => free_transform(matrix)?,
            Self::TranslateXyz { x, y, z } => {
                Transform::translation([ctx.param(*x)?, ctx.param(*y)?, ctx.param(*z)?])
            }
            Self::TranslateAlong { axis, distance } => {
                let axis = ctx.axis(axis)?;
                Transform::translation(scaled(axis.direction, ctx.param(*distance)?))
            }
            Self::Rotate { axis, angle } => {
                let axis = ctx.axis(axis)?;
                Transform::rotation(axis.origin, axis.direction, ctx.param(*angle)?)
                    .expect("a unit axis direction")
            }
            Self::PointToPoint { from, to } => {
                let (from, to) = (ctx.point(from)?, ctx.point(to)?);
                Transform::translation(sub(to, from))
            }
            Self::PointToPosition { point, x, y, z } => {
                let point = ctx.point(point)?;
                let position = [ctx.param(*x)?, ctx.param(*y)?, ctx.param(*z)?];
                Transform::translation(sub(position, point))
            }
        };
        if transform.is_finite() {
            Ok(transform)
        } else {
            Err("the move is not finite".to_owned())
        }
    }
}

/// A free move's matrix as a transform; it must be a rotation and a
/// translation.
fn free_transform(matrix: &[[f64; 4]; 3]) -> Result<Transform, String> {
    let transform = Transform {
        linear: std::array::from_fn(|r| std::array::from_fn(|c| matrix[r][c])),
        translation: std::array::from_fn(|r| matrix[r][3]),
    };
    if transform.is_finite() && transform.is_rigid() {
        Ok(transform)
    } else {
        Err("a free move must be a rotation and a translation".to_owned())
    }
}

/// Checks a list of bodies to move, scale or align.
pub(crate) fn check_bodies(ctx: &CheckContext<'_>, bodies: &[BodyUid]) -> Result<(), String> {
    if bodies.is_empty() {
        return Err("no bodies selected".to_owned());
    }
    for (i, body) in bodies.iter().enumerate() {
        if bodies[..i].contains(body) {
            return Err(format!("body {body} is listed more than once"));
        }
        ctx.body(*body)?;
    }
    Ok(())
}

/// Moves the bodies, or copies them with `copy`: the output shared by
/// moves, alignments and scales.
pub(crate) fn place_bodies<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    bodies: &[BodyUid],
    transform: &Transform,
    copy: bool,
) -> Result<FeatureOutput<K::Shape>, String> {
    let mut changes = Vec::new();
    for (j, body) in bodies.iter().enumerate() {
        let shape = ctx.body(*body)?;
        let instance = copy.then_some(Instance {
            feature: ctx.uid,
            index: 1,
        });
        let moved = ctx
            .kernel
            .transform_shape(&shape, transform, instance)
            .map_err(|e| e.to_string())?;
        let uid = if copy {
            BodyUid::new(ctx.uid, j as u32)
        } else {
            *body
        };
        changes.push(BodyChange::Set(uid, moved));
    }
    Ok(FeatureOutput {
        changes,
        ..FeatureOutput::default()
    })
}

impl FeatureInfo for MoveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        self.transform.add_references(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_bodies(ctx, &self.bodies)?;
        self.transform.check(ctx)
    }

    fn creates_bodies(&self) -> bool {
        self.copy
    }
}

impl<K: Kernel> Evaluate<K> for MoveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let transform = self.transform.resolve(ctx)?;
        place_bodies(ctx, &self.bodies, &transform, self.copy)
    }
}
