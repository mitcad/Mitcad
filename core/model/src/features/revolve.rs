// SPDX-License-Identifier: MIT
//! Revolve: profiles of a sketch turned about an axis into a tool body,
//! with the operations of an extrusion.

use serde::{Deserialize, Serialize};

use super::extrude::{
    Operation, ProfileRef, apply_operation, check_participants, check_profiles, participant_bodies,
    profile_references, profile_regions,
};
use super::geom_ref::{GeomRef, Want};
use super::reference::{ExtentObject, project_axis};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Axis, Kernel, RevolveSpec};
use crate::parameters::ParamId;

const FULL_TURN: f64 = std::f64::consts::TAU;

/// How far the revolution turns; angles in radians, positive by the right
/// hand rule about the axis direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RevolveExtent<P = ParamId> {
    /// One side; a negative angle turns the other way, 2 pi is a full turn.
    Angle { angle: P },
    /// A full turn.
    Full,
    /// `angle` to each side (a symmetric revolve measures per side).
    Symmetric { angle: P },
    /// `angle1` the positive way, `angle2` the other.
    TwoSides { angle1: P, angle2: P },
    /// The positive way up to an object (first face reached).
    ToObject { object: Box<ExtentObject> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevolveDef<P = ParamId> {
    /// Regions of one sketch.
    pub profiles: Vec<ProfileRef>,
    /// A sketch line, an origin or construction axis, a straight edge or a
    /// fixed axis.
    pub axis: GeomRef,
    /// Projects the axis into the sketch plane (`isProjectAxis` in .f3d designs).
    #[serde(default, skip_serializing_if = "is_false")]
    pub project_axis: bool,
    pub extent: RevolveExtent<P>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
}

impl<P> RevolveDef<P> {
    pub const TYPE: &'static str = "revolve";
    pub const BASE_NAME: &'static str = "Revolve";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<RevolveDef<Q>, E> {
        let extent = match &self.extent {
            RevolveExtent::Angle { angle } => RevolveExtent::Angle {
                angle: f("extent.angle", angle)?,
            },
            RevolveExtent::Full => RevolveExtent::Full,
            RevolveExtent::Symmetric { angle } => RevolveExtent::Symmetric {
                angle: f("extent.angle", angle)?,
            },
            RevolveExtent::TwoSides { angle1, angle2 } => RevolveExtent::TwoSides {
                angle1: f("extent.angle1", angle1)?,
                angle2: f("extent.angle2", angle2)?,
            },
            RevolveExtent::ToObject { object } => RevolveExtent::ToObject {
                object: object.clone(),
            },
        };
        Ok(RevolveDef {
            profiles: self.profiles.clone(),
            axis: self.axis.clone(),
            project_axis: self.project_axis,
            extent,
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }

    /// The sketch of the profiles.
    pub fn sketch(&self) -> Option<FeatureUid> {
        self.profiles.first().map(|p| p.sketch)
    }
}

impl FeatureInfo for RevolveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        self.axis.add_to(&mut references);
        if let RevolveExtent::ToObject { object } = &self.extent {
            object.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_profiles(ctx, &self.profiles)?;
        check_participants(ctx, self.operation, &self.participants)?;
        self.axis.check(ctx, Want::Line)?;
        if let RevolveExtent::ToObject { object } = &self.extent {
            object.check(ctx)?;
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_angles(&self.extent, value)
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn profiles(&self) -> &[ProfileRef] {
        &self.profiles
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

fn check_angles(extent: &RevolveExtent, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
    let total = match extent {
        RevolveExtent::Angle { angle } => {
            let a = value(*angle);
            if a == 0.0 || !a.is_finite() {
                return Err(format!("the revolve angle must not be zero, got {a}"));
            }
            a.abs()
        }
        RevolveExtent::Symmetric { angle } => {
            let a = value(*angle);
            if a.is_nan() || a <= 0.0 {
                return Err(format!(
                    "the revolve angle must be greater than zero, got {a}"
                ));
            }
            2.0 * a
        }
        RevolveExtent::TwoSides { angle1, angle2 } => {
            let (a, b) = (value(*angle1), value(*angle2));
            if !(a >= 0.0 && b >= 0.0 && a + b > 0.0) {
                return Err(format!(
                    "the revolve angles must not be negative and add up to more than zero, got {a} and {b}"
                ));
            }
            a + b
        }
        RevolveExtent::Full | RevolveExtent::ToObject { .. } => 0.0,
    };
    if total > FULL_TURN + 1e-9 {
        return Err(format!(
            "the revolve angles add up to more than a full turn ({total} rad)"
        ));
    }
    Ok(())
}

impl<K: Kernel> Evaluate<K> for RevolveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let line = ctx.axis(&self.axis)?;
        let mut axis = Axis {
            origin: line.origin,
            direction: line.direction,
        };
        if self.project_axis {
            axis = project_axis(axis, &sketch.frame)?;
        }
        let mut values = Vec::new();
        for id in self.params_of_extent() {
            values.push((id, ctx.param(id)?));
        }
        let value = |id: ParamId| {
            values
                .iter()
                .find(|(p, _)| *p == id)
                .map_or(f64::NAN, |(_, v)| *v)
        };
        check_angles(&self.extent, &value)?;
        let (mut angle1, mut angle2, mut target) = (0.0, None, None);
        match &self.extent {
            RevolveExtent::Angle { angle } => {
                let a = value(*angle);
                if a < 0.0 {
                    axis.direction = axis.direction.map(|v| -v);
                }
                angle1 = a.abs().min(FULL_TURN);
            }
            RevolveExtent::Full => angle1 = FULL_TURN,
            RevolveExtent::Symmetric { angle } => {
                angle1 = value(*angle);
                angle2 = Some(angle1);
            }
            RevolveExtent::TwoSides {
                angle1: one,
                angle2: two,
            } => {
                angle1 = value(*one);
                angle2 = Some(value(*two));
                // A side of nothing leaves one side.
                if angle1 == 0.0 {
                    axis.direction = axis.direction.map(|v| -v);
                    angle1 = value(*two);
                    angle2 = None;
                } else if value(*two) == 0.0 {
                    angle2 = None;
                }
            }
            RevolveExtent::ToObject { object } => target = Some(object.resolve(ctx)?),
        }
        let spec = RevolveSpec {
            feature: ctx.uid,
            frame: sketch.frame,
            regions: &regions,
            axis,
            angle1,
            angle2,
            target,
        };
        let tool = ctx.kernel.revolve(&spec).map_err(|e| e.to_string())?;
        apply_operation(ctx, self.operation, participants, tool, "revolution")
    }
}

impl RevolveDef {
    fn params_of_extent(&self) -> Vec<ParamId> {
        match &self.extent {
            RevolveExtent::Angle { angle } | RevolveExtent::Symmetric { angle } => vec![*angle],
            RevolveExtent::TwoSides { angle1, angle2 } => vec![*angle1, *angle2],
            RevolveExtent::Full | RevolveExtent::ToObject { .. } => Vec::new(),
        }
    }
}
