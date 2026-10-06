// SPDX-License-Identifier: MIT
//! Coil: a helix (or a flat spiral) with a circular, square or triangular
//! section, placed on a plane at a point in the plane's coordinates, with
//! the operations of an extrusion. The axis is the plane's normal; the
//! coil starts on the plane, on the side of the centre the plane's x axis
//! points to.

use serde::{Deserialize, Serialize};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::geom_ref::{GeomRef, Want};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::sweeps::{CoilPosition, CoilSection, CoilSpec};

/// How the helix is sized (the coil types).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Helix<P = ParamId> {
    RevolutionsAndHeight {
        revolutions: P,
        height: P,
    },
    RevolutionsAndPitch {
        revolutions: P,
        pitch: P,
    },
    HeightAndPitch {
        height: P,
        pitch: P,
    },
    /// A flat spiral growing `pitch` in radius per revolution.
    Spiral {
        revolutions: P,
        pitch: P,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoilDef<P = ParamId> {
    /// An origin plane, a construction plane or a planar face.
    pub plane: GeomRef,
    /// The centre, in the plane's coordinates.
    #[serde(default)]
    pub center: [f64; 2],
    /// The diameter the section lies on (see `section_position`).
    pub diameter: P,
    pub helix: Helix<P>,
    /// A cone's angle: positive widens the coil as it rises (not for a
    /// spiral).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub clockwise: bool,
    #[serde(default, skip_serializing_if = "is_circular")]
    pub section: CoilSection,
    #[serde(default, skip_serializing_if = "is_on_center")]
    pub section_position: CoilPosition,
    /// The diameter of the circle, or of the circle round the square or the
    /// triangle.
    pub section_size: P,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
}

fn is_circular(section: &CoilSection) -> bool {
    *section == CoilSection::Circular
}

fn is_on_center(position: &CoilPosition) -> bool {
    *position == CoilPosition::OnCenter
}

impl<P> CoilDef<P> {
    pub const TYPE: &'static str = "coil";
    pub const BASE_NAME: &'static str = "Coil";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<CoilDef<Q>, E> {
        let helix = match &self.helix {
            Helix::RevolutionsAndHeight {
                revolutions,
                height,
            } => Helix::RevolutionsAndHeight {
                revolutions: f("helix.revolutions", revolutions)?,
                height: f("helix.height", height)?,
            },
            Helix::RevolutionsAndPitch { revolutions, pitch } => Helix::RevolutionsAndPitch {
                revolutions: f("helix.revolutions", revolutions)?,
                pitch: f("helix.pitch", pitch)?,
            },
            Helix::HeightAndPitch { height, pitch } => Helix::HeightAndPitch {
                height: f("helix.height", height)?,
                pitch: f("helix.pitch", pitch)?,
            },
            Helix::Spiral { revolutions, pitch } => Helix::Spiral {
                revolutions: f("helix.revolutions", revolutions)?,
                pitch: f("helix.pitch", pitch)?,
            },
        };
        Ok(CoilDef {
            plane: self.plane.clone(),
            center: self.center,
            diameter: f("diameter", &self.diameter)?,
            helix,
            angle: self.angle.as_ref().map(|v| f("angle", v)).transpose()?,
            clockwise: self.clockwise,
            section: self.section,
            section_position: self.section_position,
            section_size: f("section_size", &self.section_size)?,
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

/// Revolutions, pitch and whether it is a spiral, from the helix's values.
fn turns(helix: &Helix, value: &dyn Fn(ParamId) -> f64) -> Result<(f64, f64, bool), String> {
    let positive = |id: ParamId, what: &str| {
        let v = value(id);
        if v > 0.0 && v.is_finite() {
            Ok(v)
        } else {
            Err(format!(
                "the coil's {what} must be greater than zero, got {v}"
            ))
        }
    };
    Ok(match helix {
        Helix::RevolutionsAndHeight {
            revolutions,
            height,
        } => {
            let n = positive(*revolutions, "revolutions")?;
            (n, positive(*height, "height")? / n, false)
        }
        Helix::RevolutionsAndPitch { revolutions, pitch } => (
            positive(*revolutions, "revolutions")?,
            positive(*pitch, "pitch")?,
            false,
        ),
        Helix::HeightAndPitch { height, pitch } => {
            let p = positive(*pitch, "pitch")?;
            (positive(*height, "height")? / p, p, false)
        }
        Helix::Spiral { revolutions, pitch } => (
            positive(*revolutions, "revolutions")?,
            positive(*pitch, "pitch")?,
            true,
        ),
    })
}

fn check_values(def: &CoilDef, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
    let (_, _, spiral) = turns(&def.helix, value)?;
    for (id, what) in [
        (def.diameter, "diameter"),
        (def.section_size, "section size"),
    ] {
        let v = value(id);
        if !(v > 0.0 && v.is_finite()) {
            return Err(format!(
                "the coil's {what} must be greater than zero, got {v}"
            ));
        }
    }
    if let Some(id) = def.angle {
        let angle = value(id);
        if spiral && angle != 0.0 {
            return Err("a spiral coil has no angle".to_owned());
        }
        if !(angle.is_finite() && angle.abs() < std::f64::consts::FRAC_PI_2) {
            return Err(format!(
                "the coil's angle must be less than 90 degrees either way, got {angle} rad"
            ));
        }
    }
    Ok(())
}

impl FeatureInfo for CoilDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.plane.add_to(&mut references);
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_participants(ctx, self.operation, &self.participants)?;
        self.plane.check(ctx, Want::Plane)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_values(self, value)
    }

    fn creates_bodies(&self) -> bool {
        true
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

impl<K: Kernel> Evaluate<K> for CoilDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let mut values = Vec::new();
        let _ = self.map_params(&mut |_, id: &ParamId| {
            values.push(*id);
            Ok::<_, ()>(())
        });
        let mut read = Vec::new();
        for id in values {
            read.push((id, ctx.param(id)?));
        }
        let value = |id: ParamId| {
            read.iter()
                .find(|(p, _)| *p == id)
                .map_or(f64::NAN, |(_, v)| *v)
        };
        check_values(self, &value)?;
        let (revolutions, pitch, spiral) = turns(&self.helix, &value)?;
        let plane = ctx.plane(&self.plane)?.frame();
        let frame = SketchFrame {
            origin: plane.point(self.center),
            ..plane
        };
        let spec = CoilSpec {
            feature: ctx.uid,
            frame,
            diameter: value(self.diameter),
            revolutions,
            pitch,
            angle: self.angle.map_or(0.0, value),
            spiral,
            clockwise: self.clockwise,
            section: self.section,
            position: self.section_position,
            size: value(self.section_size),
        };
        let tool = ctx.kernel.coil(&spec).map_err(|e| e.to_string())?;
        apply_operation(ctx, self.operation, participants, tool, "coil")
    }
}
