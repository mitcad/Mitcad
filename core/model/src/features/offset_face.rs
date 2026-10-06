// SPDX-License-Identifier: MIT
//! Offset face: moves faces of one body along their normals (which Press
//! Pull on a face records). A positive distance moves out of the material;
//! the neighbouring faces extend or shorten along their own surfaces. The
//! faces keep their names.

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::FaceName;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OffsetFaceDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    pub distance: P,
}

impl<P> OffsetFaceDef<P> {
    pub const TYPE: &'static str = "offset_face";
    pub const BASE_NAME: &'static str = "OffsetFace";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<OffsetFaceDef<Q>, E> {
        Ok(OffsetFaceDef {
            body: self.body,
            faces: self.faces.clone(),
            distance: f("distance", &self.distance)?,
        })
    }
}

impl FeatureInfo for OffsetFaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to offset")
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if value(self.distance) == 0.0 {
            Err("the offset distance must not be zero".to_owned())
        } else {
            Ok(())
        }
    }
}

impl<K: Kernel> Evaluate<K> for OffsetFaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let distance = ctx.param(self.distance)?;
        if distance == 0.0 || !distance.is_finite() {
            return Err(format!(
                "{} must not be zero",
                ctx.param_name(self.distance)
            ));
        }
        let shape = ctx.body(self.body)?;
        require_faces(ctx, self.body, &shape, &self.faces)?;
        let moved = ctx
            .kernel
            .offset_faces(ctx.uid, &shape, &self.faces, distance)
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, moved)],
            ..FeatureOutput::default()
        })
    }
}
