// SPDX-License-Identifier: MIT
//! Combine: a target body joined with, cut by or intersected
//! with tool bodies. The target keeps its id and the faces of all bodies
//! keep their names through the boolean. Join gives one body (several
//! solids if the bodies do not touch); Cut may split the target, the first
//! piece in geometric order keeping its id and the others becoming bodies
//! of the combine; Intersect keeps the common volume as one body (the tools
//! count as their union, experiment K48). The tools are consumed unless
//! kept. With `new_component` the result (the target) goes into a new
//! component (F6).
//!
//! Tools may be bodies of another component (mitcad#104): `tool_links`
//! names, per such tool, where it and the combine's component are seen
//! ([`OccurrenceLink`], `links.rs`). The tool is read in its component and
//! moved into the combine's coordinates by the placements at the combine's
//! point of the timeline; a consumed tool leaves its own component.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{BooleanOp, Kernel};
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;

/// The combine operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombineOperation {
    Join,
    Cut,
    Intersect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombineDef<P = ParamId> {
    pub target: BodyUid,
    pub tools: Vec<BodyUid>,
    pub operation: CombineOperation,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keep_tools: bool,
    /// Puts the result into a new component (`isNewComponent` in .f3d imports).
    #[serde(default, skip_serializing_if = "is_false")]
    pub new_component: bool,
    /// Tools of other components, by tool (mitcad#104).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_links: BTreeMap<BodyUid, OccurrenceLink>,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

impl<P> CombineDef<P> {
    pub const TYPE: &'static str = "combine";
    pub const BASE_NAME: &'static str = "Combine";

    pub fn map_params<Q, E>(
        &self,
        _f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<CombineDef<Q>, E> {
        Ok(CombineDef {
            target: self.target,
            tools: self.tools.clone(),
            operation: self.operation,
            keep_tools: self.keep_tools,
            new_component: self.new_component,
            tool_links: self.tool_links.clone(),
            marker: PhantomData,
        })
    }
}

impl FeatureInfo for CombineDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.target);
        for tool in &self.tools {
            references.body(*tool);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.target)?;
        if self.tools.is_empty() {
            return Err("no tool bodies selected".to_owned());
        }
        for (i, tool) in self.tools.iter().enumerate() {
            if *tool == self.target {
                return Err(format!("body {tool} is both the target and a tool"));
            }
            if self.tools[..i].contains(tool) {
                return Err(format!("body {tool} is listed more than once"));
            }
            ctx.body(*tool)?;
        }
        if let Some(other) = self.tool_links.keys().find(|b| !self.tools.contains(b)) {
            return Err(format!("body {other} is linked but not a tool"));
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        // The pieces of a target that a cut splits.
        true
    }

    fn new_component(&self) -> bool {
        self.new_component
    }
}

impl<K: Kernel> Evaluate<K> for CombineDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let target = ctx.body(self.target)?;
        let tools = self
            .tools
            .iter()
            .map(|uid| match self.tool_links.get(uid) {
                Some(link) => linked_tool(ctx, *uid, link),
                None => ctx.body(*uid),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let kernel = ctx.kernel;
        let error = |e: crate::kernel::KernelError| e.to_string();
        let mut changes = Vec::new();
        match self.operation {
            CombineOperation::Join => {
                let mut all = vec![&target];
                all.extend(tools.iter());
                let joined = kernel.unite(&all).map_err(error)?;
                changes.push(BodyChange::Set(self.target, joined));
            }
            CombineOperation::Cut | CombineOperation::Intersect => {
                let refs: Vec<&K::Shape> = tools.iter().collect();
                let tool = if tools.len() == 1 {
                    tools[0].clone()
                } else {
                    kernel.unite(&refs).map_err(error)?
                };
                let op = if self.operation == CombineOperation::Cut {
                    BooleanOp::Cut
                } else {
                    BooleanOp::Intersect
                };
                let result = kernel.boolean(op, &[&target], &tool).map_err(error)?;
                let pieces: Vec<K::Shape> = result.pieces.into_iter().map(|p| p.shape).collect();
                if op == BooleanOp::Intersect {
                    if !result.touched[0] || pieces.is_empty() {
                        return Err("the target and the tool bodies do not intersect".to_owned());
                    }
                    let common = if pieces.len() == 1 {
                        pieces.into_iter().next().expect("one piece")
                    } else {
                        let refs: Vec<&K::Shape> = pieces.iter().collect();
                        kernel.unite(&refs).map_err(error)?
                    };
                    changes.push(BodyChange::Set(self.target, common));
                } else if result.touched[0] {
                    let mut pieces = pieces.into_iter();
                    match pieces.next() {
                        // The tools cover the whole target.
                        None => changes.push(BodyChange::Remove(self.target)),
                        Some(first) => {
                            changes.push(BodyChange::Set(self.target, first));
                            for piece in pieces {
                                changes.push(BodyChange::Set(ctx.new_body(), piece));
                            }
                        }
                    }
                }
            }
        }
        if !self.keep_tools {
            changes.extend(self.tools.iter().map(|uid| BodyChange::Remove(*uid)));
        }
        Ok(FeatureOutput {
            changes,
            ..FeatureOutput::default()
        })
    }
}

/// A tool of another component, moved into the combine's coordinates.
fn linked_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: BodyUid,
    link: &OccurrenceLink,
) -> Result<K::Shape, String> {
    let (component, transform) = ctx.linked(link).map_err(|e| format!("tool {uid}: {e}"))?;
    let shape = ctx.body_in(component, uid)?;
    ctx.kernel
        .transform_shape(&shape, &transform, None)
        .map_err(|e| format!("tool {uid}: {e}"))
}
