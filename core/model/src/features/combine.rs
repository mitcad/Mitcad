// SPDX-License-Identifier: MIT
//! Combine: a target body joined with, cut by or intersected
//! with tool bodies. The target keeps its id and the faces of all bodies
//! keep their names through the boolean. Join gives one body (several
//! solids if the bodies do not touch); Cut may split the target, the first
//! piece in geometric order keeping its id and the others becoming bodies
//! of the combine (ids kept after upstream edits, `pieces.rs`); Intersect keeps the common volume as one body (the tools
//! count as their union, experiment K48). The tools are consumed unless
//! kept. With `new_component` the result (the target) goes into a new
//! component (F6).
//!
//! Tools may be bodies of another component (mitcad#104), each read where
//! an occurrence shows it ([`OccurrenceLink`], `links.rs`): a tool entry
//! `{"body": "F2.b0", "occurrence": "O1"}` (mitcad#544; the same body once
//! per occurrence, two screws of one part as two tools), or, as files of
//! earlier versions have them, a plain id with its link in `tool_links`.
//! The tool is read in its component and moved into the combine's
//! coordinates by the placements at the combine's point of the timeline;
//! a consumed tool leaves its own component.

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use super::joint::OccurrencePath;
use super::mapping::map_params;
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

/// A tool body: written as its id (`"F4.b0"`, a body of the combine's own
/// component, or one `tool_links` links), or as a body where an occurrence
/// shows it (`{"body": "F2.b0", "occurrence": "O1", "context": "O2"}`,
/// mitcad#544): `occurrence` the path to the tool's component (an
/// [`OccurrenceLink`]'s `source`), `context` the one to the combine's (its
/// `target`; default the first path).
#[derive(Debug, Clone, PartialEq)]
pub struct CombineTool {
    pub body: BodyUid,
    /// Where the tool is seen; none for a plain id.
    pub link: Option<OccurrenceLink>,
}

impl CombineTool {
    /// A body of the combine's own component (or one `tool_links` links).
    pub fn own(body: BodyUid) -> Self {
        Self { body, link: None }
    }

    /// A body where an occurrence shows it.
    pub fn linked(body: BodyUid, link: OccurrenceLink) -> Self {
        Self {
            body,
            link: Some(link),
        }
    }
}

impl fmt::Display for CombineTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.link {
            Some(link) if !link.source.is_empty() => write!(f, "{} in {}", self.body, link.source),
            Some(_) => write!(f, "{} in the root", self.body),
            None => write!(f, "{}", self.body),
        }
    }
}

/// A tool entry as written in files and commands.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OccurrenceTool {
    body: BodyUid,
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    occurrence: OccurrencePath,
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    context: OccurrencePath,
}

impl Serialize for CombineTool {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match &self.link {
            None => self.body.serialize(s),
            Some(link) => OccurrenceTool {
                body: self.body,
                occurrence: link.source.clone(),
                context: link.target.clone(),
            }
            .serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for CombineTool {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'v> de::Visitor<'v> for Visitor {
            type Value = CombineTool;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a body id like \"F3.b0\" or a body in an occurrence like \
                     {\"body\": \"F3.b0\", \"occurrence\": \"O1\"}",
                )
            }
            fn visit_str<E: de::Error>(self, text: &str) -> Result<CombineTool, E> {
                text.parse().map(CombineTool::own).map_err(E::custom)
            }
            fn visit_map<A: de::MapAccess<'v>>(self, map: A) -> Result<CombineTool, A::Error> {
                let tool = OccurrenceTool::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(CombineTool::linked(
                    tool.body,
                    OccurrenceLink {
                        source: tool.occurrence,
                        target: tool.context,
                    },
                ))
            }
        }
        d.deserialize_any(Visitor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombineDef<P = ParamId> {
    pub target: BodyUid,
    pub tools: Vec<CombineTool>,
    pub operation: CombineOperation,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keep_tools: bool,
    /// Puts the result into a new component (`isNewComponent` in .f3d imports).
    #[serde(default, skip_serializing_if = "is_false")]
    pub new_component: bool,
    /// Links of tools written as plain ids, by tool (mitcad#104; files of
    /// earlier versions and imports). A tool in an occurrence carries its
    /// own link instead.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_links: BTreeMap<BodyUid, OccurrenceLink>,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

impl<P> CombineDef<P> {
    pub const TYPE: &'static str = "combine";
    pub const BASE_NAME: &'static str = "Combine";

    map_params! {
        CombineDef {
            target: copy,
            tools: clone,
            operation: copy,
            keep_tools: copy,
            new_component: copy,
            tool_links: clone,
            marker: phantom,
        }
    }

    /// The tool bodies, a body in two occurrences twice.
    pub fn tool_bodies(&self) -> impl Iterator<Item = BodyUid> {
        self.tools.iter().map(|t| t.body)
    }

    /// The link a tool is read through: its own, else its `tool_links`
    /// entry; none for a body of the combine's own component.
    pub fn link_of<'a>(&'a self, tool: &'a CombineTool) -> Option<&'a OccurrenceLink> {
        tool.link
            .as_ref()
            .or_else(|| self.tool_links.get(&tool.body))
    }

    /// Every link of the tools, with its body: `tool_links`' and the tools
    /// in occurrences'.
    pub fn links(&self) -> impl Iterator<Item = (BodyUid, &OccurrenceLink)> {
        self.tool_links.iter().map(|(b, l)| (*b, l)).chain(
            self.tools
                .iter()
                .filter_map(|t| t.link.as_ref().map(|l| (t.body, l))),
        )
    }

    /// The same, to change (occurrences moved, copied or reparented).
    pub fn links_mut(&mut self) -> impl Iterator<Item = (BodyUid, &mut OccurrenceLink)> {
        self.tool_links
            .iter_mut()
            .map(|(b, l)| (*b, l))
            .chain(self.tools.iter_mut().filter_map(|t| {
                let body = t.body;
                t.link.as_mut().map(|l| (body, l))
            }))
    }

    /// Whether a tool is read in another component.
    pub fn has_links(&self) -> bool {
        self.links().next().is_some()
    }
}

impl FeatureInfo for CombineDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.target);
        for tool in self.tool_bodies() {
            references.body(tool);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.target)?;
        if self.tools.is_empty() {
            return Err("no tool bodies selected".to_owned());
        }
        for (i, tool) in self.tools.iter().enumerate() {
            if tool.body == self.target {
                return Err(format!("body {} is both the target and a tool", tool.body));
            }
            // A body is a tool once where it is read: once per occurrence
            // (mitcad#544).
            let link = self.link_of(tool);
            if self.tools[..i]
                .iter()
                .any(|t| t.body == tool.body && self.link_of(t) == link)
            {
                return Err(format!("body {tool} is listed more than once"));
            }
            ctx.body(tool.body)?;
        }
        if let Some(other) = self
            .tool_links
            .keys()
            .find(|b| !self.tools.iter().any(|t| t.link.is_none() && t.body == **b))
        {
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
        // Mesh bodies are refused (mitcad#399).
        let target = ctx.surface_body(self.target)?;
        let tools = self
            .tools
            .iter()
            .map(|tool| match self.link_of(tool) {
                Some(link) => linked_tool(ctx, tool, link),
                None => ctx.surface_body(tool.body),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let kernel = ctx.kernel;
        let error = |e: crate::kernel::KernelError| e.to_string();
        let mut changes = Vec::new();
        // Whether the bodies hold material: an empty result of bodies that
        // do is broken, and storing it would replace the target with
        // nothing and remove the tools (mitcad#1273). A cut's or an
        // intersection's pieces are of the target (a sheet's are sheets).
        let target_material = has_solids(kernel, std::iter::once(&target));
        let material = target_material || has_solids(kernel, tools.iter());
        let count = tools.len() + 1;
        match self.operation {
            CombineOperation::Join => {
                let mut all = vec![&target];
                all.extend(tools.iter());
                // (Within the document's join tolerance, mitcad#397.)
                let joined = ctx.unite(&all).map_err(error)?;
                if material {
                    require_solid(kernel, &joined, || {
                        format!("the join of {count} bodies gave an empty result")
                    })?;
                }
                changes.push(BodyChange::Set(self.target, joined));
            }
            CombineOperation::Cut | CombineOperation::Intersect => {
                let refs: Vec<&K::Shape> = tools.iter().collect();
                let tool = if tools.len() == 1 {
                    tools[0].clone()
                } else {
                    let united = kernel.unite(&refs).map_err(error)?;
                    if has_solids(kernel, tools.iter()) {
                        require_solid(kernel, &united, || {
                            format!(
                                "the union of the {} tool bodies gave an empty result",
                                tools.len()
                            )
                        })?;
                    }
                    united
                };
                let verb = if self.operation == CombineOperation::Cut {
                    "cut"
                } else {
                    "intersection"
                };
                let op = if self.operation == CombineOperation::Cut {
                    BooleanOp::Cut
                } else {
                    BooleanOp::Intersect
                };
                let result = kernel.boolean(op, &[&target], &tool).map_err(error)?;
                let pieces: Vec<K::Shape> = result.pieces.into_iter().map(|p| p.shape).collect();
                // A piece without material is broken; no piece at all is a
                // target the tools cover (a cut) or miss (an intersection).
                if target_material {
                    for piece in &pieces {
                        require_solid(kernel, piece, || {
                            format!("the {verb} of {count} bodies gave an empty result")
                        })?;
                    }
                }
                if op == BooleanOp::Intersect {
                    // A target lying wholly inside the tools is touched,
                    // its one piece the target with its names (mitcad#1534);
                    // one the tools only touch or miss leaves no piece.
                    if !result.touched[0] || pieces.is_empty() {
                        return Err("the target and the tool bodies do not intersect".to_owned());
                    }
                    let common = if pieces.len() == 1 {
                        pieces.into_iter().next().expect("one piece")
                    } else {
                        let refs: Vec<&K::Shape> = pieces.iter().collect();
                        kernel.unite(&refs).map_err(error)?
                    };
                    if target_material {
                        require_solid(kernel, &common, || {
                            format!("the {verb} of {count} bodies gave an empty result")
                        })?;
                    }
                    changes.push(BodyChange::Set(self.target, common));
                } else if result.touched[0] {
                    // Needles a cut leaves are lumps of the first piece.
                    let pieces = super::base::lump_needles(kernel, pieces);
                    // The pieces' ids (mitcad#408); none left for the target
                    // (the tools cover it all) removes it.
                    let named = ctx.piece_ids(self.target, pieces);
                    if !named.iter().any(|(id, _)| *id == self.target) {
                        changes.push(BodyChange::Remove(self.target));
                    }
                    for (id, piece) in named {
                        changes.push(BodyChange::Set(id, piece));
                    }
                }
            }
        }
        if !self.keep_tools {
            // A body that is a tool in two occurrences goes once.
            let mut removed: Vec<BodyUid> = Vec::new();
            for body in self.tool_bodies() {
                if !removed.contains(&body) {
                    removed.push(body);
                    changes.push(BodyChange::Remove(body));
                }
            }
        }
        Ok(FeatureOutput {
            changes,
            ..FeatureOutput::default()
        })
    }
}

/// Whether any of the shapes holds a solid, or may (a kernel that cannot
/// count solids).
fn has_solids<'a, K: Kernel>(kernel: &K, shapes: impl IntoIterator<Item = &'a K::Shape>) -> bool
where
    K::Shape: 'a,
{
    shapes
        .into_iter()
        .any(|shape| !matches!(kernel.solid_count(shape), Ok(0)))
}

/// Fails a result of bodies with material that holds no solid (mitcad#1273:
/// a union that came out empty was stored as the target's shape, and the
/// tools it consumed were removed, so the model vanished without an error).
/// A kernel that cannot count solids lets the result pass.
fn require_solid<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    message: impl FnOnce() -> String,
) -> Result<(), String> {
    match kernel.solid_count(shape) {
        Ok(0) => Err(message()),
        _ => Ok(()),
    }
}

/// A tool of another component, moved into the combine's coordinates.
fn linked_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    tool: &CombineTool,
    link: &OccurrenceLink,
) -> Result<K::Shape, String> {
    let uid = tool.body;
    let (component, transform) = ctx.linked(link).map_err(|e| format!("tool {tool}: {e}"))?;
    let shape = ctx.body_in(component, uid)?;
    ctx.require_surfaces(uid, &shape)?;
    ctx.kernel
        .transform_shape(&shape, &transform, None)
        .map_err(|e| format!("tool {tool}: {e}"))
}
