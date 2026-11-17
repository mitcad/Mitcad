// SPDX-License-Identifier: MIT
//! Contour Flange (mitcad#461): a sheet bent along an open chain of sketch
//! lines. The chain is the sheet's surface on the rules' reference side;
//! every corner of it is the virtual sharp of a bend of the rules' radius,
//! and the sheet is `width` wide along the sketch's normal (against it with
//! `reverse`).

use super::mapping::map_params;
use super::sheet::SheetRulesDef;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use serde::{Deserialize, Serialize};

/// Which side of the chain the material lies on, travelling along it and
/// looking against the sketch's normal (the sketch seen from its front).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContourSide {
    #[default]
    Right,
    Left,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetContourDef<P = ParamId> {
    pub rules: FeatureUid,
    pub sketch: FeatureUid,
    /// Sketch lines joined end to end, in the order the chain runs: the
    /// first line runs away from the second (a single line runs from its
    /// start).
    #[serde(with = "mitcad_sketch::internal::curves_serde")]
    pub curves: Vec<EntityUid>,
    /// The sheet's extent along the sketch's normal.
    pub width: P,
    #[serde(default, skip_serializing_if = "is_right")]
    pub side: ContourSide,
    /// The width runs against the sketch's normal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
    /// The bends' inside radius in place of the rules'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
}

fn is_right(side: &ContourSide) -> bool {
    *side == ContourSide::Right
}

impl<P> SheetContourDef<P> {
    pub const TYPE: &'static str = "sheet_contour";
    pub const BASE_NAME: &'static str = "SheetContour";
    map_params! {
        SheetContourDef {
            rules: copy,
            sketch: copy,
            curves: clone,
            width: param,
            side: copy,
            reverse: copy,
            radius: option,
        }
    }
}

impl FeatureInfo for SheetContourDef {
    fn references(&self) -> References {
        References {
            features: [self.rules, self.sketch].into_iter().collect(),
            ..References::default()
        }
    }
    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.sketch(self.sketch)?;
        if !matches!(ctx.feature(self.rules)?.def, FeatureDef::SheetRules(_)) {
            return Err(format!("{} is not a sheet rules feature", self.rules));
        }
        if self.curves.is_empty() {
            return Err("a contour flange needs at least one sketch line".into());
        }
        for (i, c) in self.curves.iter().enumerate() {
            if self.curves[..i].contains(c) {
                return Err(format!("curve {} is listed more than once", c.curve_name()));
            }
        }
        Ok(())
    }
    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let width = value(self.width);
        if !(width.is_finite() && width > crate::sheet::TOLERANCE) {
            return Err(format!(
                "the contour flange width must be positive, got {width}"
            ));
        }
        if let Some(r) = self.radius.map(value)
            && !(r.is_finite() && r > crate::sheet::TOLERANCE)
        {
            return Err(format!(
                "the contour flange radius must be positive, got {r}"
            ));
        }
        Ok(())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SheetContourDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::contour(&d, ctx).map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}

/// The rules a contour or a conversion is evaluated with.
pub(crate) fn rules_of<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    rules: FeatureUid,
) -> Result<(SheetRulesDef<f64>, String), String> {
    let r = ctx.sheet_rules(rules)?;
    Ok((r.value, format!("{:032x}", r.version)))
}
