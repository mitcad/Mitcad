// SPDX-License-Identifier: MIT
//! Split body: cuts bodies into pieces with a tool.
//! The tool is a plane (moved by `offset`), a face (of any body, extended
//! along its surface with `extend`, the default), a body or sketch curves
//! swept along `direction` (the sketch normal by default). Every body must
//! be divided.
//!
//! The pieces come in order along a planar tool's normal (else in geometric
//! order): the first keeps the body (its id and name), the others become new
//! bodies of the split (`<split>.b0`, ...), named `Body<n>` like new bodies.
//! The cut faces are `<split>:split`, after a face tool
//! `<split>:split(<tool face>)`; split faces of the body get `#k`.

use serde::{Deserialize, Serialize};

use super::face_refs::{check_tool, kernel_error, map_offset, resolve_tool};
use super::geom_ref::{GeomRef, Want};
use super::pattern::none;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::datum::Vec3;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitBodyDef<P = ParamId> {
    pub bodies: Vec<BodyUid>,
    pub tool: GeomRef,
    /// Moves a plane tool along its normal.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// The direction sketch curves are swept along.
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

impl<P> SplitBodyDef<P> {
    pub const TYPE: &'static str = "split_body";
    pub const BASE_NAME: &'static str = "SplitBody";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SplitBodyDef<Q>, E> {
        Ok(SplitBodyDef {
            bodies: self.bodies.clone(),
            tool: self.tool.clone(),
            offset: map_offset(&self.offset, f)?,
            direction: self.direction,
            extend: self.extend,
        })
    }
}

impl FeatureInfo for SplitBodyDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        self.tool.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.bodies.is_empty() {
            return Err("no bodies to split".to_owned());
        }
        for (i, body) in self.bodies.iter().enumerate() {
            if self.bodies[..i].contains(body) {
                return Err(format!("body {body} is listed more than once"));
            }
            ctx.body(*body)?;
        }
        check_tool(
            ctx,
            &self.tool,
            Want::Tool,
            self.offset.is_some(),
            self.direction,
            "the splitting tool",
        )?;
        match &self.tool {
            GeomRef::Body(body) if self.bodies.contains(body) => {
                Err(format!("body {body} cannot split itself"))
            }
            _ => Ok(()),
        }
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SplitBodyDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let tool = resolve_tool(ctx, &self.tool, self.offset, self.direction)?;
        let mut output = FeatureOutput::default();
        for body in &self.bodies {
            let shape = ctx.body(*body)?;
            let pieces = ctx
                .kernel
                .split_body(ctx.uid, &shape, &tool.input(), self.extend)
                .map_err(|e| {
                    let message = kernel_error(e);
                    if self.bodies.len() > 1 {
                        format!("body {body}: {message}")
                    } else {
                        message
                    }
                })?;
            let mut pieces = pieces.into_iter();
            let first = pieces
                .next()
                .ok_or_else(|| format!("body {body} has no pieces"))?;
            output.changes.push(BodyChange::Set(*body, first));
            for piece in pieces {
                output.changes.push(BodyChange::Set(ctx.new_body(), piece));
            }
        }
        Ok(output)
    }
}
