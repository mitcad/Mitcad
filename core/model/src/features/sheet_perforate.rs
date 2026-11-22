// SPDX-License-Identifier: MIT
//! Perforate (mitcad#461, mitcad#2242): a field of round holes on a
//! staggered (60 degree) or square grid, kept clear of the material's edges,
//! its cut-outs and its bends. The holes are cut through the material as a
//! developed sheet cut, so the body, the Flat Pattern, the DXF and every
//! measure have them.

use super::extrude::ProfileRef;
use super::mapping::map_params;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sheet::PanelId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerforationPattern {
    /// Rows `pitch * sqrt(3) / 2` apart, every other row shifted by half a
    /// pitch: each hole `pitch` from its six neighbours.
    #[default]
    Staggered,
    /// Rows and columns `pitch` apart.
    Square,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerforationOutline {
    #[serde(deserialize_with = "super::extrude::profile_set")]
    pub profiles: Vec<ProfileRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetPerforateDef<P = ParamId> {
    pub body: BodyUid,
    pub diameter: P,
    /// The distance between neighbouring holes' centres.
    pub pitch: P,
    /// How far each hole keeps clear of the material's edges, its cut-outs
    /// and the bends' tangent lines (default: the diameter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<P>,
    #[serde(default, skip_serializing_if = "is_staggered")]
    pub pattern: PerforationPattern,
    /// The panel held while the sheet is developed: the grid lies in its
    /// chart's axes (default: the sheet's lowest panel, as the Flat
    /// Pattern's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stationary: Option<PanelId>,
    /// Profiles from one sketch on a flat face of this sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<PerforationOutline>,
}

fn is_staggered(p: &PerforationPattern) -> bool {
    *p == PerforationPattern::Staggered
}

impl<P> SheetPerforateDef<P> {
    pub const TYPE: &'static str = "sheet_perforate";
    pub const BASE_NAME: &'static str = "SheetPerforate";
    map_params! {
        SheetPerforateDef {
            body: copy,
            diameter: param,
            pitch: param,
            margin: option,
            pattern: copy,
            stationary: copy,
            outline: clone,
        }
    }
}

/// Refuses sizes that make no field: the diameter must be positive, the
/// pitch larger than it, the margin not negative.
pub(crate) fn check_sizes(diameter: f64, pitch: f64, margin: f64) -> Result<(), String> {
    if !(diameter.is_finite() && diameter > crate::sheet::TOLERANCE)
        || !(pitch.is_finite() && pitch > diameter)
        || !(margin.is_finite() && margin >= 0.0)
    {
        return Err(format!(
            "a perforation needs a positive diameter, a pitch larger than it and a margin not \
             below zero (diameter {diameter}, pitch {pitch}, margin {margin})"
        ));
    }
    Ok(())
}

impl FeatureInfo for SheetPerforateDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.features.extend(self.stationary.map(|s| s.feature));
        r.features.extend(self.profiles().iter().map(|p| p.sketch));
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        c.body(self.body)?;
        if let Some(outline) = &self.outline {
            let first = outline
                .profiles
                .first()
                .ok_or("select closed sketch profiles for the perforation outline")?;
            if outline.profiles.iter().any(|p| p.sketch != first.sketch) {
                return Err("the perforation outline's profiles must lie in one sketch".into());
            }
            c.profile_sketch(first.sketch)?;
        }
        for f in self.references().features {
            c.feature(f)?;
        }
        Ok(())
    }
    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let diameter = value(self.diameter);
        check_sizes(
            diameter,
            value(self.pitch),
            self.margin.map_or(diameter, value),
        )
    }
    fn creates_bodies(&self) -> bool {
        true
    }
    fn profiles(&self) -> &[ProfileRef] {
        self.outline
            .as_ref()
            .map_or(&[], |outline| outline.profiles.as_slice())
    }
}

impl<K: Kernel> Evaluate<K> for SheetPerforateDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::perforate(&d, ctx).map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
