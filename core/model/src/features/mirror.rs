// SPDX-License-Identifier: MIT
//! Mirror: bodies, features or faces reflected in a plane. It
//! is a pattern of one copy, element 1 (faces `<mirror>:inst1(<name>)`). A
//! mirrored body is a new body unless `combine` joins it to its original
//! when they touch. A mirrored feature's tool is combined with the bodies
//! again with the feature's operation: with `compute` `adjust` (the default
//! of a mirror made or edited since mitcad#2430) an extrusion whose end
//! depends on bodies (to the next face, up to an object, through all) is
//! rebuilt at the mirrored place, its profile and direction reflected and
//! its end found again there, as a pattern's copies are, and so is a rib or
//! a web To Next or with Extend Ends (mitcad#2431); any other tool,
//! and every tool with `identical` or `optimized`, is reflected as a
//! finished body. A mirror saved without `compute` (a design written
//! before mitcad#2430) reflects every finished tool, as it did, until it is
//! edited ([`MirrorDef::settle_compute`]). A pattern with Adjust whose
//! features include a mirror that rebuilds builds the mirrored tool again
//! at each of its copies too (mitcad#2432).

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
    /// How the copies of features are computed, as a pattern's `compute`.
    /// A version flag besides (mitcad#2430): a mirror saved without it (a
    /// design written before) works as `adjust` but reflects every
    /// finished tool, as mirrors did, until it is edited; a mirror of
    /// features added or edited without it gets `adjust`
    /// ([`MirrorDef::settle_compute`], `Document::resolve`), which rebuilds
    /// an extrusion up to an object or through all at the mirrored place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compute: Option<ComputeOption>,
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

impl<P> MirrorDef<P> {
    pub const TYPE: &'static str = "mirror";
    pub const BASE_NAME: &'static str = "Mirror";

    /// The compute option it works with: `adjust` when none is stored.
    pub fn compute_option(&self) -> ComputeOption {
        self.compute.unwrap_or_default()
    }

    /// Whether its copies of features are rebuilt where they are reflected
    /// (mitcad#2430): `adjust` stored, as a mirror made or edited since
    /// has it. A mirror saved without `compute` reflects the finished
    /// tools, as mirrors did before.
    pub fn rebuilds_reflected_tools(&self) -> bool {
        self.compute == Some(ComputeOption::Adjust)
    }

    /// Gives a mirror of features made or edited by a command without
    /// `compute` the default, `adjust` (mitcad#2430). Design files are read
    /// without it, so that their mirrors keep the results they had.
    pub(crate) fn settle_compute(&mut self) {
        if matches!(self.objects, PatternObjects::Features { .. }) {
            self.compute.get_or_insert(ComputeOption::Adjust);
        }
    }

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
            self.compute_option(),
            self.original_bodies,
            &[element],
            Combine::of(self.combine, self.combine_any),
            scaling.as_ref(),
            &self.skipped_copies,
        )
    }
}
