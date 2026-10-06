// SPDX-License-Identifier: MIT
//! Scale of bodies: uniform about a point, or non-uniform along the model
//! axes (experiment K53 asks which axes .f3d designs use in a moved
//! component). Bodies keep their ids and faces their names; a
//! non-uniform scale turns curved faces into B-splines.

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Want};
use super::moves::{check_bodies, place_bodies};
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScaleSpec<P = ParamId> {
    Uniform { factor: P },
    NonUniform { x: P, y: P, z: P },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaleDef<P = ParamId> {
    pub bodies: Vec<BodyUid>,
    pub point: GeomRef,
    pub scale: ScaleSpec<P>,
}

impl<P> ScaleDef<P> {
    pub const TYPE: &'static str = "scale";
    pub const BASE_NAME: &'static str = "Scale";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ScaleDef<Q>, E> {
        let scale = match &self.scale {
            ScaleSpec::Uniform { factor } => ScaleSpec::Uniform {
                factor: f("scale.factor", factor)?,
            },
            ScaleSpec::NonUniform { x, y, z } => ScaleSpec::NonUniform {
                x: f("scale.x", x)?,
                y: f("scale.y", y)?,
                z: f("scale.z", z)?,
            },
        };
        Ok(ScaleDef {
            bodies: self.bodies.clone(),
            point: self.point.clone(),
            scale,
        })
    }
}

impl ScaleSpec {
    fn params(&self) -> Vec<ParamId> {
        match self {
            Self::Uniform { factor } => vec![*factor],
            Self::NonUniform { x, y, z } => vec![*x, *y, *z],
        }
    }
}

fn check_factor(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!(
            "a scale factor must be greater than zero, got {value}"
        ))
    }
}

impl FeatureInfo for ScaleDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        self.point.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_bodies(ctx, &self.bodies)?;
        self.point.check(ctx, Want::Point)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.scale
            .params()
            .into_iter()
            .try_for_each(|id| check_factor(value(id)))
    }
}

impl<K: Kernel> Evaluate<K> for ScaleDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut factors = [0.0; 3];
        match &self.scale {
            ScaleSpec::Uniform { factor } => factors = [ctx.positive(*factor)?; 3],
            ScaleSpec::NonUniform { x, y, z } => {
                for (factor, id) in factors.iter_mut().zip([x, y, z]) {
                    *factor = ctx.positive(*id)?;
                }
            }
        }
        let center = ctx.point(&self.point)?;
        place_bodies(ctx, &self.bodies, &Transform::scale(center, factors), false)
    }
}
