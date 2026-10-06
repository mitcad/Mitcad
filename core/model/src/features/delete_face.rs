// SPDX-License-Identifier: MIT
//! Delete face: removes faces of one body and heals it by extending the
//! neighbouring faces until they close the gap. It fails when they cannot
//! (two opposite sides of a box). The other faces keep their names.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::FaceName;

/// Has no values; `P` only matches the other feature types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteFaceDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> DeleteFaceDef<P> {
    pub const TYPE: &'static str = "delete_face";
    pub const BASE_NAME: &'static str = "DeleteFace";

    pub fn map_params<Q, E>(
        &self,
        _f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<DeleteFaceDef<Q>, E> {
        Ok(DeleteFaceDef {
            body: self.body,
            faces: self.faces.clone(),
            values: PhantomData,
        })
    }
}

impl FeatureInfo for DeleteFaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to delete")
    }
}

impl<K: Kernel> Evaluate<K> for DeleteFaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let shape = ctx.body(self.body)?;
        require_faces(ctx, self.body, &shape, &self.faces)?;
        let healed = ctx
            .kernel
            .delete_faces(ctx.uid, &shape, &self.faces)
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, healed)],
            ..FeatureOutput::default()
        })
    }
}
