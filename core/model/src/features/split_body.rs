// SPDX-License-Identifier: MIT
//! Split body: cuts bodies into pieces with a tool.
//! The tool is a plane (moved by `offset`), a face (of any body, extended
//! along its surface with `extend`, the default), a body or sketch curves
//! swept along `direction` (the sketch normal by default). Every body must
//! be divided.
//!
//! The pieces come in order along a planar tool's normal (else in geometric
//! order): the first keeps the body (its id and name), the others become new
//! bodies of the split (`<split>.b0`, ...), named `Body<n>` like new bodies;
//! later evaluations keep the pieces' ids (`pieces.rs`, mitcad#408).
//! The cut faces are `<split>:split`, after a face tool
//! `<split>:split(<tool face>)`; split faces of the body get `#k`.
//!
//! A tool body, or the body of a tool face, may belong to another component
//! (`tool_links`, as a combine's, mitcad#104): it is read in its component
//! and moved into the split's coordinates by the placements at the split's
//! point of the timeline.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::face_refs::{
    Resolved, check_tool, extended_tools, kernel_error, map_offset, resolve_tool,
};
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::pattern::none;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::datum::Vec3;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitBodyDef<P = ParamId> {
    pub bodies: Vec<BodyUid>,
    pub tool: GeomRef,
    /// Further tools the bodies are divided by as well, one after another
    /// (the pieces a tool does not reach stay as they are): the cells are
    /// the same as with all tools at once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more_tools: Vec<GeomRef>,
    /// Moves a plane tool along its normal.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// The direction sketch curves are swept along.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Vec3>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub extend: bool,
    /// Tool bodies (and bodies of tool faces) of other components, by body
    /// (mitcad#104): where each and the split's component are seen.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_links: BTreeMap<BodyUid, OccurrenceLink>,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// The body a tool is, or belongs to.
fn tool_body(tool: &GeomRef) -> Option<BodyUid> {
    match tool {
        GeomRef::Body(body) | GeomRef::Face { body, .. } => Some(*body),
        _ => None,
    }
}

impl<P> SplitBodyDef<P> {
    pub const TYPE: &'static str = "split_body";
    pub const BASE_NAME: &'static str = "SplitBody";

    /// The tools, the first one then the further ones.
    fn tools(&self) -> impl Iterator<Item = &GeomRef> {
        std::iter::once(&self.tool).chain(&self.more_tools)
    }

    map_params! {
        SplitBodyDef {
            bodies: clone,
            tool: clone,
            more_tools: clone,
            offset: with(map_offset),
            direction: copy,
            extend: copy,
            tool_links: clone,
        }
    }
}

impl FeatureInfo for SplitBodyDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        self.tool.add_to(&mut references);
        for tool in &self.more_tools {
            tool.add_to(&mut references);
        }
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
        for tool in &self.more_tools {
            check_tool(ctx, tool, Want::Tool, false, None, "a splitting tool")?;
        }
        if let Some(other) = self
            .tool_links
            .keys()
            .find(|b| !self.tools().any(|t| tool_body(t) == Some(**b)))
        {
            return Err(format!("body {other} is linked but not a tool"));
        }
        match std::iter::once(&self.tool)
            .chain(&self.more_tools)
            .find(|tool| matches!(tool, GeomRef::Body(body) if self.bodies.contains(body)))
        {
            Some(GeomRef::Body(body)) => Err(format!("body {body} cannot split itself")),
            _ => Ok(()),
        }
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SplitBodyDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let tool = split_tool(
            ctx,
            &self.tool_links,
            &self.tool,
            self.offset,
            self.direction,
        )?;
        let mut more = Vec::with_capacity(self.more_tools.len());
        for t in &self.more_tools {
            more.push(split_tool(ctx, &self.tool_links, t, None, None)?);
        }
        let mut output = FeatureOutput::default();
        for body in &self.bodies {
            let shape = ctx.surface_body(*body)?;
            let label = |message: String| {
                if self.bodies.len() > 1 {
                    format!("body {body}: {message}")
                } else {
                    message
                }
            };
            let pieces = ctx
                .kernel
                .split_body(ctx.uid, &shape, &tool.input(), self.extend)
                .map_err(|e| label(kernel_error(e)))?;
            // Each further tool divides the pieces it reaches; one that
            // reaches none divides nothing.
            let mut pieces = pieces;
            for (k, t) in more.iter().enumerate() {
                let mut next = Vec::with_capacity(pieces.len());
                let mut divided = false;
                let mut last_error = None;
                for piece in pieces {
                    match ctx
                        .kernel
                        .split_body(ctx.uid, &piece, &t.input(), self.extend)
                    {
                        Ok(parts) => {
                            divided = true;
                            next.extend(parts);
                        }
                        Err(e) => {
                            last_error = Some(kernel_error(e));
                            next.push(piece);
                        }
                    }
                }
                if !divided {
                    let why = last_error.unwrap_or_else(|| "it divides nothing".to_owned());
                    return Err(label(format!("splitting tool {}: {why}", k + 2)));
                }
                pieces = next;
            }
            if pieces.is_empty() {
                return Err(format!("body {body} has no pieces"));
            }
            // The pieces' ids (mitcad#408); none left for the body removes
            // it.
            let named = ctx.piece_ids(*body, pieces);
            if !named.iter().any(|(id, _)| id == body) {
                output.changes.push(BodyChange::Remove(*body));
            }
            for (id, piece) in named {
                output.changes.push(BodyChange::Set(id, piece));
            }
        }
        Ok(output)
    }

    /// The tools as the split extends them, for the open command's
    /// preview only (mitcad#712): against the bodies as the split finds
    /// them.
    fn preview_tool(&self, ctx: &mut EvalContext<'_, K>) -> Option<K::Shape> {
        if !self.extend {
            return None;
        }
        let mut tools = vec![
            split_tool(
                ctx,
                &self.tool_links,
                &self.tool,
                self.offset,
                self.direction,
            )
            .ok()?,
        ];
        for t in &self.more_tools {
            tools.push(split_tool(ctx, &self.tool_links, t, None, None).ok()?);
        }
        let mut targets = Vec::with_capacity(self.bodies.len());
        for body in &self.bodies {
            targets.push(ctx.surface_body(*body).ok()?);
        }
        let targets: Vec<&K::Shape> = targets.iter().collect();
        let tools: Vec<&Resolved<K::Shape>> = tools.iter().collect();
        extended_tools(ctx.kernel, &targets, &tools, self.extend)
    }
}

/// A tool evaluated: one on a body of another component is read there and
/// moved into the split's coordinates, the rest as [`resolve_tool`].
fn split_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    links: &BTreeMap<BodyUid, OccurrenceLink>,
    tool: &GeomRef,
    offset: Option<ParamId>,
    direction: Option<Vec3>,
) -> Result<Resolved<K::Shape>, String> {
    let Some(body) = tool_body(tool) else {
        return resolve_tool(ctx, tool, offset, direction);
    };
    let Some(link) = links.get(&body) else {
        // A tool of another component without its link (mitcad#2299): said
        // in words, not as a body id and an activation that cannot help
        // (the split is in the component of the bodies it splits).
        return resolve_tool(ctx, tool, offset, direction)
            .map_err(|e| unlinked_tool(ctx, body).unwrap_or(e));
    };
    let (component, transform) = ctx.linked(link).map_err(|e| format!("tool {body}: {e}"))?;
    let shape = ctx.body_in(component, body)?;
    ctx.require_surfaces(body, &shape)?;
    let shape = ctx
        .kernel
        .transform_shape(&shape, &transform, None)
        .map_err(|e| format!("tool {body}: {e}"))?;
    Ok(match tool {
        GeomRef::Face { face, .. } => {
            let face = ctx.face_on(body, &shape, face)?;
            Resolved::Face { shape, face }
        }
        _ => Resolved::Body { shape },
    })
}

/// Why a tool on `body` cannot be read when `body` is a body of another
/// component than the split's and no `tool_links` entry says where that
/// component is seen; `None` for a body of the split's own component.
fn unlinked_tool<K: Kernel>(ctx: &EvalContext<'_, K>, body: BodyUid) -> Option<String> {
    if ctx.env.bodies.contains_key(&body) {
        return None;
    }
    let owner = *ctx.env.owners.get(&body)?;
    (owner != ctx.component()).then(|| {
        format!(
            "the splitting tool is in {}, another component, and the split does not say where \
             that component is placed: pick the tool again in the view, where that component \
             is seen",
            ctx.env.assembly.name(owner)
        )
    })
}
