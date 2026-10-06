// SPDX-License-Identifier: MIT
//! Replace face: replaces faces of one body with a target; the neighbouring
//! faces extend or shorten along their own surfaces to the target. The
//! faces and the target may be of any surface type. The target is a plane
//! reference (moved by `offset`), a face of a body or a body (its faces:
//! the surface bodies and construction planes of .f3d designs); a face of
//! the body itself works as a surface too, the body itself does not.
//!
//! The new face is `<feature>:replace(<face>)`, after the first replaced
//! face (pieces `#k`); the other faces keep their names.

use serde::{Deserialize, Serialize};

use super::face_refs::{
    add_faces, check_faces, check_tool, kernel_error, map_offset, require_faces, resolve_tool,
};
use super::geom_ref::{GeomRef, Want};
use super::pattern::none;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::FaceName;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceFaceDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    /// A plane, a face or another body.
    pub target: GeomRef,
    /// Moves a plane target (not a face) along its normal.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub tangent_chain: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl<P> ReplaceFaceDef<P> {
    pub const TYPE: &'static str = "replace_face";
    pub const BASE_NAME: &'static str = "ReplaceFace";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ReplaceFaceDef<Q>, E> {
        Ok(ReplaceFaceDef {
            body: self.body,
            faces: self.faces.clone(),
            target: self.target.clone(),
            offset: map_offset(&self.offset, f)?,
            tangent_chain: self.tangent_chain,
        })
    }
}

impl FeatureInfo for ReplaceFaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        self.target.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to replace")?;
        check_tool(
            ctx,
            &self.target,
            Want::Surface,
            self.offset.is_some(),
            None,
            "the target",
        )?;
        match &self.target {
            GeomRef::Body(body) if *body == self.body => Err(format!(
                "the target: body {body} cannot replace its own faces"
            )),
            _ => Ok(()),
        }
    }
}

impl<K: Kernel> Evaluate<K> for ReplaceFaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let shape = ctx.body(self.body)?;
        require_faces(ctx, self.body, &shape, &self.faces)?;
        let target = resolve_tool(ctx, &self.target, self.offset, None)?;
        let replaced = ctx
            .kernel
            .replace_faces(
                ctx.uid,
                &shape,
                &self.faces,
                &target.input(),
                self.tangent_chain,
            )
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, replaced)],
            ..FeatureOutput::default()
        })
    }
}
