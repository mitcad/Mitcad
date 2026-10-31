// SPDX-License-Identifier: MIT
//! Mirror: bodies, features or faces reflected in a plane. It
//! is a pattern of one copy, element 1 (faces `<mirror>:inst1(<name>)`). A
//! mirrored body is a new body unless `combine` joins it to its original
//! when they touch; a mirrored feature's tool is reflected as a finished
//! body (a reflected sketch frame would be left-handed) and combined with
//! the bodies again with the feature's operation.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::pattern::{
    Combine, ComputeOption, Element, PatternObjects, ScaleRule, ScaleSpec, add_scale_references,
    check_combine_any, check_options, check_scale_values, map_scale, map_scale_copies,
    map_scale_groups, map_scales, none, repeat, scale_elements,
};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorDef<P = ParamId> {
    pub objects: PatternObjects,
    pub plane: GeomRef,
    /// Joins each mirrored body to its original when they touch
    /// (`isCombine` in .f3d designs); bodies only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine: bool,
    /// With `combine`: each image joins every body of the mirror it
    /// touches (the mirrored bodies and the images before it), not only
    /// its original, as a pattern's `combine_any` (mitcad#1006).
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine_any: bool,
    #[serde(default, skip_serializing_if = "is_adjust")]
    pub compute: ComputeOption,
    /// Copies of features act only on the bodies each feature changed
    /// where the feature has no participants of its own (the `.f3d`
    /// import's mirrors, mitcad#74); by default on the feature's
    /// participants, every body without them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub original_bodies: bool,
    /// Scales the image by this factor about its own centre, as a
    /// pattern's `scale` with two elements (FreeCAD's Scaled
    /// transformation after a mirror, mitcad#539).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale: Option<P>,
    /// As a pattern's `scale_groups` (2 at most: the original's and the
    /// image's run, or one run over the copies of patterns among the
    /// features).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_groups: Option<P>,
    /// As a pattern's `scale_copies` (mitcad#542).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_copies: Option<P>,
    /// As a pattern's `scales` (mitcad#542).
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub scales: Vec<ScaleRule<P>>,
    /// As a pattern's `scale_center` (mitcad#542).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_center: Option<GeomRef>,
    /// As a pattern's `skipped_copies` (mitcad#542).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_copies: Vec<u32>,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

fn is_adjust(option: &ComputeOption) -> bool {
    *option == ComputeOption::Adjust
}

impl<P> MirrorDef<P> {
    pub const TYPE: &'static str = "mirror";
    pub const BASE_NAME: &'static str = "Mirror";

    map_params! {
        MirrorDef {
            objects: clone,
            plane: clone,
            combine: copy,
            combine_any: copy,
            compute: copy,
            original_bodies: copy,
            scale: with(map_scale),
            scale_groups: with(map_scale_groups),
            scale_copies: with(map_scale_copies),
            scales: with(map_scales),
            scale_center: clone,
            skipped_copies: clone,
            marker: phantom,
        }
    }
}

impl MirrorDef {
    /// Its scale options together.
    pub(super) fn scale_spec(&self) -> ScaleSpec<'_> {
        ScaleSpec {
            scale: self.scale,
            groups: self.scale_groups,
            copies: self.scale_copies,
            more: &self.scales,
            center: self.scale_center.as_ref(),
        }
    }
}

impl FeatureInfo for MirrorDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.objects.add_references(&mut references);
        self.plane.add_to(&mut references);
        add_scale_references(&self.scale_spec(), &mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.plane.check(ctx, Want::Plane)?;
        check_combine_any(self.combine, self.combine_any)?;
        check_options(
            ctx,
            &self.objects,
            self.combine,
            &self.scale_spec(),
            &self.skipped_copies,
        )
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_scale_values(value, &self.scale_spec())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MirrorDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let frame = ctx.plane(&self.plane)?;
        let reflection = Transform::mirror(frame.origin, frame.normal())
            .ok_or("the mirror plane has no normal")?;
        let (all, scaling) = scale_elements(
            ctx,
            &self.objects,
            &self.scale_spec(),
            vec![Transform::IDENTITY, reflection],
        )?;
        let element = Element {
            index: 1,
            transform: all[1],
        };
        // Its elements, for patterns of it (patterns of patterns).
        ctx.elements = all;
        repeat(
            ctx,
            &self.objects,
            self.compute,
            self.original_bodies,
            &[element],
            Combine::of(self.combine, self.combine_any),
            scaling.as_ref(),
            &self.skipped_copies,
        )
    }
}
