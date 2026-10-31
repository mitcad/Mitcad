// SPDX-License-Identifier: MIT
//! Scale Pattern (mitcad#539): copies of bodies or features scaled where
//! they are, growing (or shrinking) element by element: element e of q is
//! scaled by 1 + (scale − 1) · e / (q − 1) about the centre of mass of the
//! first object (a feature's tool, the first body), or about
//! `scale_center` (mitcad#542). Element 0 is the original. It is a pattern
//! whose elements do not move ([`super::pattern`]: copies of bodies are new
//! bodies `<pattern>.b<k>` unless `combine` joins them to their original,
//! copies of features are combined with the bodies again with the
//! features' operations, and patterns of it place its scaled copies).
//! FreeCAD's Scaled transformation alone or first in a MultiTransform is
//! one. `scales` and `skipped_copies` as a pattern's (mitcad#542).

use serde::{Deserialize, Serialize};

use super::geom_ref::GeomRef;
use super::mapping::map_params;
use super::pattern::{
    Combine, ComputeOption, PatternObjects, ScaleRule, ScaleSpec, active, add_scale_references,
    check_combine_any, check_options, check_scale_values, check_suppressed, is_adjust, map_scales,
    none, quantity, repeat, scale_elements,
};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalePatternDef<P = ParamId> {
    pub objects: PatternObjects,
    /// Elements, the original included.
    pub quantity: P,
    /// The last element's factor.
    pub scale: P,
    #[serde(default, skip_serializing_if = "is_adjust")]
    pub compute: ComputeOption,
    /// As a rectangular pattern's `original_bodies`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub original_bodies: bool,
    /// Joins each copy of a body to its original when they touch; bodies
    /// only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine: bool,
    /// As a pattern's `combine_any` (mitcad#1006).
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine_any: bool,
    /// Elements left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed_elements: Vec<u32>,
    /// More scales of the copies, as a pattern's (mitcad#542).
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub scales: Vec<ScaleRule<P>>,
    /// The point the copies are scaled about instead of the first object's
    /// centre of mass (mitcad#542).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_center: Option<GeomRef>,
    /// As a pattern's `skipped_copies` (mitcad#542).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_copies: Vec<u32>,
}

impl<P> ScalePatternDef<P> {
    pub const TYPE: &'static str = "scale_pattern";
    pub const BASE_NAME: &'static str = "ScalePattern";

    map_params! {
        ScalePatternDef {
            objects: clone,
            quantity: param,
            scale: param,
            compute: copy,
            original_bodies: copy,
            combine: copy,
            combine_any: copy,
            suppressed_elements: clone,
            scales: with(map_scales),
            scale_center: clone,
            skipped_copies: clone,
        }
    }
}

impl ScalePatternDef {
    /// Its scale options together: its own scale, one run per element.
    pub(super) fn scale_spec(&self) -> ScaleSpec<'_> {
        ScaleSpec {
            scale: Some(self.scale),
            groups: None,
            copies: None,
            more: &self.scales,
            center: self.scale_center.as_ref(),
        }
    }
}

impl FeatureInfo for ScalePatternDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.objects.add_references(&mut references);
        add_scale_references(&self.scale_spec(), &mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        check_combine_any(self.combine, self.combine_any)?;
        check_options(
            ctx,
            &self.objects,
            self.combine,
            &self.scale_spec(),
            &self.skipped_copies,
        )?;
        check_suppressed(&self.suppressed_elements)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        quantity("pattern quantity", value(self.quantity))?;
        check_scale_values(value, &self.scale_spec())
    }

    fn creates_bodies(&self) -> bool {
        self.objects.creates_bodies()
    }
}

impl<K: Kernel> Evaluate<K> for ScalePatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let q = quantity(&ctx.param_name(self.quantity), ctx.param(self.quantity)?)?;
        let all = vec![Transform::IDENTITY; q as usize];
        let (all, scaling) = scale_elements(ctx, &self.objects, &self.scale_spec(), all)?;
        ctx.elements.clone_from(&all);
        let elements = active(all, &self.suppressed_elements)?;
        repeat(
            ctx,
            &self.objects,
            self.compute,
            self.original_bodies,
            &elements,
            Combine::of(self.combine, self.combine_any),
            scaling.as_ref(),
            &self.skipped_copies,
        )
    }
}
