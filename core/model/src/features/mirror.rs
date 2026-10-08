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
use super::pattern::{ComputeOption, Element, PatternObjects, repeat};
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
    #[serde(default, skip_serializing_if = "is_adjust")]
    pub compute: ComputeOption,
    /// Copies of features act only on the bodies each feature changed
    /// where the feature has no participants of its own (the `.f3d`
    /// import's mirrors, mitcad#74); by default on the feature's
    /// participants, every body without them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub original_bodies: bool,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

fn is_adjust(option: &ComputeOption) -> bool {
    *option == ComputeOption::Adjust
}

impl<P> MirrorDef<P> {
    pub const TYPE: &'static str = "mirror";
    pub const BASE_NAME: &'static str = "Mirror";

    pub fn map_params<Q, E>(
        &self,
        _f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<MirrorDef<Q>, E> {
        Ok(MirrorDef {
            objects: self.objects.clone(),
            plane: self.plane.clone(),
            combine: self.combine,
            compute: self.compute,
            original_bodies: self.original_bodies,
            marker: PhantomData,
        })
    }
}

impl FeatureInfo for MirrorDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.objects.add_references(&mut references);
        self.plane.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.plane.check(ctx, Want::Plane)?;
        if self.combine && !matches!(self.objects, PatternObjects::Bodies { .. }) {
            return Err("combine applies to mirrored bodies only".to_owned());
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MirrorDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let frame = ctx.plane(&self.plane)?;
        let transform = Transform::mirror(frame.origin, frame.normal())
            .ok_or("the mirror plane has no normal")?;
        let element = Element {
            index: 1,
            transform,
        };
        // Its elements, for patterns of it (patterns of patterns).
        ctx.elements = vec![Transform::IDENTITY, transform];
        repeat(
            ctx,
            &self.objects,
            self.compute,
            self.original_bodies,
            &[element],
            self.combine,
        )
    }
}
