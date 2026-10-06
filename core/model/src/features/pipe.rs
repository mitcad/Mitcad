// SPDX-License-Identifier: MIT
//! Pipe: a circular, square or triangular section swept along a path,
//! solid or hollow, with the operations of an extrusion.

use serde::{Deserialize, Serialize};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::path::{self, CurvePath};
use super::sweep::PathExtent;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sweeps::{PipeSection, PipeSpec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipeDef<P = ParamId> {
    pub path: CurvePath,
    #[serde(default, skip_serializing_if = "is_circular")]
    pub section: PipeSection,
    /// The diameter of the circle, or of the circle round the square or
    /// the triangle (`sectionSize` in .f3d designs).
    pub size: P,
    /// Makes the pipe hollow with this wall inside the section (`isHollow`
    /// and `sectionThickness` in .f3d designs).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thickness: Option<P>,
    #[serde(
        default = "PathExtent::full",
        skip_serializing_if = "PathExtent::is_full"
    )]
    pub extent: PathExtent<P>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
}

fn is_circular(section: &PipeSection) -> bool {
    *section == PipeSection::Circular
}

impl<P> PipeDef<P> {
    pub const TYPE: &'static str = "pipe";
    pub const BASE_NAME: &'static str = "Pipe";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PipeDef<Q>, E> {
        Ok(PipeDef {
            path: self.path.clone(),
            section: self.section,
            size: f("size", &self.size)?,
            thickness: self
                .thickness
                .as_ref()
                .map(|v| f("thickness", v))
                .transpose()?,
            extent: self.extent.map_params(f)?,
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

/// The largest wall a section takes: the radius of the circle inside it.
fn inner_radius(section: PipeSection, size: f64) -> f64 {
    let radius = size / 2.0;
    match section {
        PipeSection::Circular => radius,
        PipeSection::Square => radius * std::f64::consts::FRAC_1_SQRT_2,
        PipeSection::Triangular => radius / 2.0,
    }
}

fn check_sizes(section: PipeSection, size: f64, thickness: Option<f64>) -> Result<(), String> {
    if !(size > 0.0 && size.is_finite()) {
        return Err(format!(
            "the section size must be greater than zero, got {size}"
        ));
    }
    if let Some(t) = thickness {
        if !(t > 0.0 && t.is_finite()) {
            return Err(format!(
                "the wall thickness must be greater than zero, got {t}"
            ));
        }
        if t >= inner_radius(section, size) {
            return Err(format!(
                "the wall thickness {t} fills the section; it must be less than {}",
                inner_radius(section, size)
            ));
        }
    }
    Ok(())
}

impl FeatureInfo for PipeDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.path.add_references(&mut references);
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_participants(ctx, self.operation, &self.participants)?;
        self.path.check(ctx).map_err(|e| format!("path: {e}"))
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.extent.check_values(value)?;
        check_sizes(self.section, value(self.size), self.thickness.map(value))
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

impl<K: Kernel> Evaluate<K> for PipeDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let size = ctx.param(self.size)?;
        let thickness = self.thickness.map(|id| ctx.param(id)).transpose()?;
        check_sizes(self.section, size, thickness)?;
        let path = self.path.resolve(ctx).map_err(|e| format!("path: {e}"))?;
        let closed = path::is_closed(&path);
        // The section starts the path, so only a closed path has a part
        // before it.
        let (extent1, extent2) = match &self.extent {
            PathExtent::Partial {
                fraction2: Some(_), ..
            } if !closed => {
                return Err("only a pipe along a closed path has a second fraction".to_owned());
            }
            extent => extent.fractions(ctx, true)?,
        };
        let spec = PipeSpec {
            feature: ctx.uid,
            path: &path,
            extent1,
            extent2,
            section: self.section,
            size,
            thickness,
        };
        let tool = ctx.kernel.pipe(&spec).map_err(|e| e.to_string())?;
        apply_operation(ctx, self.operation, participants, tool, "pipe")
    }
}
