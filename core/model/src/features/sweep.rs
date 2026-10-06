// SPDX-License-Identifier: MIT
//! Sweep: profiles of a sketch swept along a path into a tool body that
//! becomes new bodies or joins, cuts or intersects bodies: perpendicular
//! or parallel orientation, twist and taper, a guide rail that turns and
//! scales the profile, and partial extents.

use serde::{Deserialize, Serialize};

use super::extrude::{
    Operation, ProfileRef, apply_operation, check_participants, check_profiles, participant_bodies,
    profile_references, profile_regions,
};
use super::path::{self, CurvePath};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sweeps::{PathCurve, ProfileScaling, SweepGuide, SweepOrientation, SweepSpec};

/// How much of a path a sweep or a pipe covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathExtent<P = ParamId> {
    /// All of it, both ways from the profile (once round a closed path).
    Full,
    /// `fraction` of the path's length from the profile towards its end
    /// and `fraction2` of the length from the profile towards its start
    /// (`distanceOne` and `distanceTwo` in .f3d designs). Left out, `fraction2` is the
    /// whole part before the profile, or none on a closed path.
    Partial {
        fraction: P,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        fraction2: Option<P>,
    },
}

impl<P> PathExtent<P> {
    pub fn full() -> Self {
        Self::Full
    }

    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }

    pub(crate) fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PathExtent<Q>, E> {
        Ok(match self {
            Self::Full => PathExtent::Full,
            Self::Partial {
                fraction,
                fraction2,
            } => PathExtent::Partial {
                fraction: f("extent.fraction", fraction)?,
                fraction2: fraction2
                    .as_ref()
                    .map(|v| f("extent.fraction2", v))
                    .transpose()?,
            },
        })
    }
}

impl PathExtent {
    /// The fractions of the path on each side of the profile.
    pub(crate) fn fractions<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        closed: bool,
    ) -> Result<(f64, f64), String> {
        let (one, two) = match self {
            Self::Full => (1.0, if closed { 0.0 } else { 1.0 }),
            Self::Partial {
                fraction,
                fraction2,
            } => (
                ctx.param(*fraction)?,
                match fraction2 {
                    Some(id) => ctx.param(*id)?,
                    None if closed => 0.0,
                    None => 1.0,
                },
            ),
        };
        check_fractions(one, two, closed)?;
        Ok((one, two))
    }

    pub(crate) fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Self::Partial {
            fraction,
            fraction2,
        } = self
        {
            for id in std::iter::once(fraction).chain(fraction2) {
                check_fraction(value(*id))?;
            }
        }
        Ok(())
    }
}

fn check_fraction(value: f64) -> Result<(), String> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "a fraction of the path must be between 0 and 1, got {value}"
        ))
    }
}

fn check_fractions(one: f64, two: f64, closed: bool) -> Result<(), String> {
    check_fraction(one)?;
    check_fraction(two)?;
    if one + two <= 0.0 {
        return Err("the sweep covers none of the path".to_owned());
    }
    if closed && one + two > 1.0 + 1e-9 {
        return Err(format!(
            "on a closed path the fractions add up to at most 1, got {}",
            one + two
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepDef<P = ParamId> {
    /// Regions of one sketch.
    pub profiles: Vec<ProfileRef>,
    pub path: CurvePath,
    #[serde(default, skip_serializing_if = "is_perpendicular")]
    pub orientation: SweepOrientation,
    /// The profile's turn about the path over the swept length.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub twist_angle: Option<P>,
    /// Widens (positive) or narrows the profile along the path.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub taper_angle: Option<P>,
    /// A curve that turns the profile towards it and sizes it; twist and
    /// taper are then ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_rail: Option<CurvePath>,
    #[serde(default, skip_serializing_if = "is_scale")]
    pub profile_scaling: ProfileScaling,
    #[serde(
        default = "PathExtent::full",
        skip_serializing_if = "PathExtent::is_full"
    )]
    pub extent: PathExtent<P>,
    /// Runs the path the other way (`isDirectionFlipped` in .f3d designs).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
}

fn is_perpendicular(orientation: &SweepOrientation) -> bool {
    *orientation == SweepOrientation::Perpendicular
}

fn is_scale(scaling: &ProfileScaling) -> bool {
    *scaling == ProfileScaling::Scale
}

impl<P> SweepDef<P> {
    pub const TYPE: &'static str = "sweep";
    pub const BASE_NAME: &'static str = "Sweep";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SweepDef<Q>, E> {
        Ok(SweepDef {
            profiles: self.profiles.clone(),
            path: self.path.clone(),
            orientation: self.orientation,
            twist_angle: self
                .twist_angle
                .as_ref()
                .map(|v| f("twist_angle", v))
                .transpose()?,
            taper_angle: self
                .taper_angle
                .as_ref()
                .map(|v| f("taper_angle", v))
                .transpose()?,
            guide_rail: self.guide_rail.clone(),
            profile_scaling: self.profile_scaling,
            extent: self.extent.map_params(f)?,
            flip: self.flip,
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

impl FeatureInfo for SweepDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        self.path.add_references(&mut references);
        if let Some(rail) = &self.guide_rail {
            rail.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_profiles(ctx, &self.profiles)?;
        check_participants(ctx, self.operation, &self.participants)?;
        self.path.check(ctx).map_err(|e| format!("path: {e}"))?;
        if let Some(rail) = &self.guide_rail {
            rail.check(ctx).map_err(|e| format!("guide rail: {e}"))?;
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.extent.check_values(value)?;
        if let Some(taper) = self.taper_angle {
            check_taper(value(taper))?;
        }
        if let Some(twist) = self.twist_angle
            && !value(twist).is_finite()
        {
            return Err("the twist angle must be a number".to_owned());
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

fn check_taper(taper: f64) -> Result<(), String> {
    if taper.is_finite() && taper.abs() < std::f64::consts::FRAC_PI_2 {
        Ok(())
    } else {
        Err(format!(
            "the taper angle must be less than 90 degrees either way, got {taper} rad"
        ))
    }
}

impl<K: Kernel> Evaluate<K> for SweepDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let mut path = self.path.resolve(ctx).map_err(|e| format!("path: {e}"))?;
        if self.flip {
            path = path::reversed_path(path);
        }
        let (extent1, extent2) = self.extent.fractions(ctx, path::is_closed(&path))?;
        let rail: Option<Vec<PathCurve>> = match &self.guide_rail {
            Some(rail) => Some(rail.resolve(ctx).map_err(|e| format!("guide rail: {e}"))?),
            None => None,
        };
        // A guide rail overrides twist and taper.
        let (mut twist, mut taper) = (0.0, 0.0);
        if rail.is_none() {
            if let Some(id) = self.twist_angle {
                twist = ctx.param(id)?;
            }
            if let Some(id) = self.taper_angle {
                taper = ctx.param(id)?;
                check_taper(taper)?;
            }
        }
        let spec = SweepSpec {
            feature: ctx.uid,
            frame: sketch.frame,
            regions: &regions,
            path: &path,
            extent1,
            extent2,
            orientation: self.orientation,
            twist,
            taper,
            guide: rail.as_deref().map(|rail| SweepGuide {
                rail,
                scaling: self.profile_scaling,
            }),
        };
        let tool = ctx.kernel.sweep(&spec).map_err(|e| e.to_string())?;
        apply_operation(ctx, self.operation, participants, tool, "sweep")
    }
}
