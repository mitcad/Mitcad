// SPDX-License-Identifier: MIT
//! Helix: profiles of a sketch turned about an axis while they move along
//! it (a screw motion: FreeCAD's additive and subtractive helices,
//! mitcad#4), into a tool body with the operations of an extrusion. Every
//! section of the tool in a plane through the axis is the profile turned
//! there; the coil's sections are fixed shapes at right angles to its
//! helix instead. A growth also moves the profiles out as they turn, by
//! MitCad's construction or FreeCAD's ([`HelixConstruction`], mitcad#83).

use serde::{Deserialize, Serialize};

use super::extrude::{
    Operation, ProfileRef, apply_operation, check_participants, participant_bodies,
    profile_references, profile_regions,
};
use super::geom_ref::{GeomRef, Want};
use super::path::SketchCurves;
use super::surface_sweeps;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    ToolUse, is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{Kernel, KernelError};
use crate::parameters::ParamId;
use crate::sweeps::HelixSpec;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelixDef<P = ParamId> {
    /// Regions of one sketch, a set (see `extrude::selected_regions`); none
    /// for a surface helix of curves only (mitcad#1777).
    #[serde(
        default,
        deserialize_with = "super::extrude::profile_set",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub profiles: Vec<ProfileRef>,
    /// A sketch line, an origin or construction axis, a straight edge or a
    /// fixed axis: the profiles move along its direction.
    pub axis: GeomRef,
    /// The rise per turn along the axis.
    pub pitch: P,
    /// The turns (need not be whole).
    pub revolutions: P,
    /// Turns left-handed about the direction of travel instead of
    /// right-handed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub left_handed: bool,
    /// Moves against the axis direction.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// How far the profiles move out from the axis per turn (mitcad#59),
    /// widening the helix (a cone's growth is the pitch times the tangent
    /// of its angle). Negative narrows it.
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub growth: Option<P>,
    /// How a growth is built (mitcad#83): MitCad's own construction unless
    /// FreeCAD's ([`HelixConstruction`]). A command without it means
    /// MitCad's; a stored growing helix always has it, except in files
    /// written before it existed, which mean FreeCAD's
    /// ([`HelixDef::settle_construction`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<HelixConstruction>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// How the helix starts and ends (mitcad#235): none for the pitch all
    /// the way (see [`HelixEnd`]).
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub start: Option<HelixEnd<P>>,
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub end: Option<HelixEnd<P>>,
    /// The seams of the helix's pieces shifted by this many turns (0, the
    /// default, for none; less than a third; mitcad#235): the same shape,
    /// built so that a boolean that fails or comes out wrong where those
    /// seams meet the seams of the bodies' faces about the axis works (the
    /// `.ipt` import's coils that give the file's state only so).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub piece_shift: f64,
    /// A surface helix (mitcad#1777, as a surface revolution): the
    /// profiles' walls without their caps and the curves' surfaces, new
    /// surface bodies.
    #[serde(default, skip_serializing_if = "is_false")]
    pub surface: bool,
    /// Sketch curves a surface helix moves by its screw motion beside or
    /// instead of the profiles, open chains or closed loops (a line gives a
    /// helicoid).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<SketchCurves>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// A flat end of a helix (mitcad#235), as a coil's: over `transition_angle` (an
/// angle) the pitch changes linearly between the helix's and zero at the
/// outer end, and over `flat_angle` (an angle) the profiles turn without rising,
/// beyond the transition. Both come in addition to the helix's turns (its
/// `revolutions` are those at the full pitch). With ends a growth follows
/// the rise: the profiles move out by the growth for every pitch they rise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelixEnd<P = ParamId> {
    pub transition_angle: P,
    pub flat_angle: P,
}

impl<P> HelixEnd<P> {
    // Written by hand: the slots take the end (`start.flat_angle`).
    fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
        side: &str,
    ) -> Result<HelixEnd<Q>, E> {
        Ok(HelixEnd {
            transition_angle: f(&format!("{side}.transition_angle"), &self.transition_angle)?,
            flat_angle: f(&format!("{side}.flat_angle"), &self.flat_angle)?,
        })
    }
}

fn end_angle(value: f64, what: &str) -> Result<f64, String> {
    if value >= 0.0 && value.is_finite() {
        Ok(value)
    } else {
        Err(format!(
            "the helix's {what} must be an angle of zero or more, got {value}"
        ))
    }
}

/// How a helix with a growth moves its profiles (mitcad#83).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelixConstruction {
    /// MitCad's: the screw motion with the profiles moved out along the
    /// direction from the axis towards them, in proportion to the turn.
    Mitcad,
    /// FreeCAD's construction of its conical and growing helices, for the
    /// FreeCAD import (the profiles follow the Frenet frame of a far
    /// spiral; a narrowing helix whose profiles sit at the axis' origin
    /// runs the other way).
    Freecad,
}

impl<P> HelixDef<P> {
    pub const TYPE: &'static str = "helix";
    pub const BASE_NAME: &'static str = "Helix";

    // Written by hand: the start and the end map with their prefixes.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<HelixDef<Q>, E> {
        Ok(HelixDef {
            profiles: self.profiles.clone(),
            axis: self.axis.clone(),
            pitch: f("pitch", &self.pitch)?,
            revolutions: f("revolutions", &self.revolutions)?,
            left_handed: self.left_handed,
            flip: self.flip,
            growth: self.growth.as_ref().map(|g| f("growth", g)).transpose()?,
            construction: self.construction,
            operation: self.operation,
            participants: self.participants.clone(),
            start: self
                .start
                .as_ref()
                .map(|e| e.map_params(f, "start"))
                .transpose()?,
            end: self
                .end
                .as_ref()
                .map(|e| e.map_params(f, "end"))
                .transpose()?,
            piece_shift: self.piece_shift,
            surface: self.surface,
            curves: self.curves.clone(),
        })
    }

    /// Gives a growing helix without a construction `construction`: MitCad's
    /// for a command, FreeCAD's for a file written before mitcad#83 (its
    /// growing helices came from the FreeCAD import, built that way).
    pub(crate) fn settle_construction(&mut self, construction: HelixConstruction) {
        if self.growth.is_some() && self.construction.is_none() {
            self.construction = Some(construction);
        }
    }
}

fn positive(value: f64, what: &str) -> Result<f64, String> {
    if value > 0.0 && value.is_finite() {
        Ok(value)
    } else {
        Err(format!(
            "the helix's {what} must be greater than zero, got {value}"
        ))
    }
}

fn finite_growth(value: f64) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!(
            "the helix's growth must be a finite length, got {value}"
        ))
    }
}

impl FeatureInfo for HelixDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        if let Some(curves) = &self.curves {
            curves.add_references(&mut references);
        }
        self.axis.add_to(&mut references);
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        // Profiles, or with `surface` also curves (mitcad#1777).
        surface_sweeps::check_inputs(
            ctx,
            self.surface,
            self.operation,
            &self.profiles,
            self.curves.as_ref(),
            "helix",
        )?;
        if self.surface {
            if self.growth.is_some() || self.start.is_some() || self.end.is_some() {
                return Err(
                    "a surface helix keeps its radius and pitch: it has no growth and no flat \
                     ends"
                        .to_owned(),
                );
            }
            if self.construction == Some(HelixConstruction::Freecad) || self.piece_shift != 0.0 {
                return Err("a surface helix is built by MitCad's construction".to_owned());
            }
        }
        check_participants(ctx, self.operation, &self.participants)?;
        self.axis.check(ctx, Want::Line)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        positive(value(self.pitch), "pitch")?;
        positive(value(self.revolutions), "revolutions")?;
        if let Some(g) = self.growth {
            finite_growth(value(g))?;
        }
        for (side, end) in [("start", &self.start), ("end", &self.end)] {
            if let Some(e) = end {
                end_angle(value(e.transition_angle), &format!("{side} transition"))?;
                end_angle(value(e.flat_angle), &format!("{side} flat"))?;
            }
        }
        if self.construction == Some(HelixConstruction::Freecad)
            && (self.start.is_some() || self.end.is_some())
        {
            return Err("a helix with flat ends is built by MitCad's construction".to_owned());
        }
        if !(0.0..1.0 / 3.0).contains(&self.piece_shift) {
            return Err(format!(
                "the helix's piece shift must be at least 0 and less than a third of a turn, \
                 got {}",
                self.piece_shift
            ));
        }
        Ok(())
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

impl<K: Kernel> Evaluate<K> for HelixDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        if self.surface {
            return self.surface_output(ctx);
        }
        let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let line = ctx.axis(&self.axis)?;
        let length = line.direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length.is_nan() || length <= 1e-12 {
            return Err("the helix's axis has no direction".to_owned());
        }
        let sign = if self.flip { -1.0 } else { 1.0 };
        let direction = line.direction.map(|v| sign * v / length);
        let pitch = positive(ctx.param(self.pitch)?, "pitch")?;
        let revolutions = positive(ctx.param(self.revolutions)?, "revolutions")?;
        let growth = match self.growth {
            Some(g) => finite_growth(ctx.param(g)?)?,
            None => 0.0,
        };
        let bodies: Vec<K::Shape> = participants.iter().map(|(_, s)| s.clone()).collect();
        let mut ends = [0.0; 4];
        if let Some(e) = &self.start {
            ends[0] = end_angle(ctx.param(e.flat_angle)?, "start flat")?;
            ends[1] = end_angle(ctx.param(e.transition_angle)?, "start transition")?;
        }
        if let Some(e) = &self.end {
            ends[2] = end_angle(ctx.param(e.transition_angle)?, "end transition")?;
            ends[3] = end_angle(ctx.param(e.flat_angle)?, "end flat")?;
        }
        let mut spec = HelixSpec {
            feature: ctx.uid,
            frame: sketch.frame,
            regions: &regions,
            origin: line.origin,
            direction,
            pitch,
            revolutions,
            left_handed: self.left_handed,
            growth,
            flip: self.flip,
            freecad: self.construction == Some(HelixConstruction::Freecad),
            bodies: &bodies,
            ends,
            piece_shift: self.piece_shift,
        };
        let tool = ctx.kernel.helix(&spec).map_err(|e| e.to_string())?;
        // (FreeCAD's widening helix is one piece: no seams to shift.)
        if matches!(self.operation, Operation::NewBody | Operation::NewComponent)
            || (spec.freecad && growth != 0.0)
            || self.piece_shift != 0.0
        {
            return apply_operation(ctx, self.operation, participants, tool, "helix");
        }
        // The boolean with the bodies; once more with the pieces' seams
        // shifted when it fails or its result is not valid (mitcad#235, see
        // `PIECE_SHIFT`). A helix whose boolean works is built only once.
        let next_body = ctx.next_body;
        let first = apply_operation(
            ctx,
            self.operation,
            participants.clone(),
            tool.clone(),
            "helix",
        );
        let failure = match &first {
            Err(error) => error.clone(),
            Ok(output) if invalid_change(ctx, output) => "its result is not valid".to_owned(),
            Ok(_) => return first,
        };
        if !worth_retrying(ctx, &failure, &participants, &tool) {
            return first;
        }
        spec.piece_shift = PIECE_SHIFT;
        let Ok(shifted) = ctx.kernel.helix(&spec) else {
            return first;
        };
        let after_first = ctx.next_body;
        ctx.next_body = next_body;
        match apply_operation(ctx, self.operation, participants, shifted, "helix") {
            Ok(output) if !invalid_change(ctx, &output) => {
                ctx.warn(format!(
                    "built with its pieces' seams shifted by a sixth of a turn, \
                     as the first build failed: {failure}"
                ));
                Ok(output)
            }
            _ => {
                ctx.next_body = after_first;
                first
            }
        }
    }
}

impl HelixDef {
    /// A surface helix's bodies (mitcad#1777, as a surface revolution's):
    /// the profiles' walls (the solid helix without its caps), then the
    /// curves' surfaces moved by the screw motion.
    fn surface_output<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let (sketch, regions) = match (&self.curves, self.profiles.is_empty()) {
            (Some(curves), true) => (ctx.sketch(curves.sketch)?, Vec::new()),
            _ => profile_regions(ctx, &self.profiles)?,
        };
        let frame = sketch.frame;
        let chains = match &self.curves {
            Some(curves) => Some(surface_sweeps::chains(ctx, curves, &frame)?),
            None => None,
        };
        let line = ctx.axis(&self.axis)?;
        let length = line.direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length.is_nan() || length <= 1e-12 {
            return Err("the helix's axis has no direction".to_owned());
        }
        let sign = if self.flip { -1.0 } else { 1.0 };
        let mut spec = HelixSpec {
            feature: ctx.uid,
            frame,
            regions: &regions,
            origin: line.origin,
            direction: line.direction.map(|v| sign * v / length),
            pitch: positive(ctx.param(self.pitch)?, "pitch")?,
            revolutions: positive(ctx.param(self.revolutions)?, "revolutions")?,
            left_handed: self.left_handed,
            growth: 0.0,
            flip: self.flip,
            freecad: false,
            bodies: &[],
            ends: [0.0; 4],
            piece_shift: 0.0,
        };
        let mut sheets = Vec::new();
        if !regions.is_empty() {
            let tool = ctx.kernel.helix(&spec).map_err(|e| e.to_string())?;
            sheets.push(surface_sweeps::walls(ctx, &tool)?);
        }
        if let Some(chains) = &chains {
            spec.regions = &[];
            sheets.push(
                ctx.kernel
                    .helix_sheet(&spec, chains)
                    .map_err(|e| e.to_string())?,
            );
        }
        surface_sweeps::output(ctx, sheets)
    }
}

/// How far the helix's pieces' seams are shifted (turns) when its boolean
/// failed (mitcad#235): a boolean with bodies that have faces about the
/// axis can fail where the seams of the helix's pieces (a third of a turn
/// apart) meet those faces' seams, and work with the seams moved (on a
/// coil's cut, seams shifted by 1/12 to 1/4 of a turn worked).
pub(crate) const PIECE_SHIFT: f64 = 1.0 / 6.0;

/// Whether a body the operation set is not valid by the geometry kernel's
/// checker (a kernel that cannot check finds none).
fn invalid_change<K: Kernel>(ctx: &EvalContext<'_, K>, output: &FeatureOutput<K::Shape>) -> bool {
    output.changes.iter().any(|change| match change {
        BodyChange::Set(_, shape) => matches!(ctx.kernel.is_valid(shape), Ok(false)),
        BodyChange::Remove(_) => false,
    })
}

/// Whether a failed boolean is retried: not when the process ran low on
/// memory or the evaluation was cancelled, and not when the tool's box is
/// clear of every participant's (it cannot reach them however it is built).
fn worth_retrying<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    failure: &str,
    participants: &[(BodyUid, K::Shape)],
    tool: &K::Shape,
) -> bool {
    if KernelError::is_out_of_memory(failure)
        || failure.contains("cancelled")
        || failure.contains("canceled")
    {
        return false;
    }
    let Ok(Some(tool)) = ctx.kernel.bounding_box(tool) else {
        return true;
    };
    participants.iter().any(|(_, body)| {
        !matches!(ctx.kernel.bounding_box(body), Ok(Some(b)) if !b.overlaps(&tool, 1e-7))
    })
}
