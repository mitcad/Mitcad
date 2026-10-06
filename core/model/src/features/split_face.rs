// SPDX-License-Identifier: MIT
//! Split face: splits faces of one body along a tool (surface
//! intersection, or along a vector for sketch curves). The tool is a plane
//! (moved by `offset`), a face (of any body, extended along its surface
//! with `extend`, the default), a body or sketch curves swept along
//! `direction` (the sketch normal by default). The body's shape does not
//! change; the pieces of each face keep its name with `#k`. Every face must
//! be split. A closest point split is not supported.

use serde::{Deserialize, Serialize};

use super::face_refs::{
    add_faces, check_faces, check_tool, kernel_error, map_offset, require_faces, resolve_tool,
};
use super::geom_ref::{GeomRef, Want};
use super::pattern::none;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::datum::Vec3;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::FaceName;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitFaceDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    pub tool: GeomRef,
    /// Moves a plane tool along its normal.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// The direction sketch curves are swept along (the along vector).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Vec3>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub extend: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl<P> SplitFaceDef<P> {
    pub const TYPE: &'static str = "split_face";
    pub const BASE_NAME: &'static str = "SplitFace";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SplitFaceDef<Q>, E> {
        Ok(SplitFaceDef {
            body: self.body,
            faces: self.faces.clone(),
            tool: self.tool.clone(),
            offset: map_offset(&self.offset, f)?,
            direction: self.direction,
            extend: self.extend,
        })
    }
}

impl FeatureInfo for SplitFaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        self.tool.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to split")?;
        check_tool(
            ctx,
            &self.tool,
            Want::Tool,
            self.offset.is_some(),
            self.direction,
            "the splitting tool",
        )?;
        match &self.tool {
            GeomRef::Body(body) if *body == self.body => {
                Err("a body cannot split its own faces".to_owned())
            }
            _ => Ok(()),
        }
    }
}

impl<K: Kernel> Evaluate<K> for SplitFaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let shape = ctx.body(self.body)?;
        require_faces(ctx, self.body, &shape, &self.faces)?;
        let tool = resolve_tool(ctx, &self.tool, self.offset, self.direction)?;
        let split = ctx
            .kernel
            .split_faces(ctx.uid, &shape, &self.faces, &tool.input(), self.extend)
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, split)],
            ..FeatureOutput::default()
        })
    }
}
