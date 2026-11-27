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
//!
//! `keep` (mitcad#2504) keeps only some pieces and removes the others in
//! the same feature; without it every piece stays. Each entry picks pieces
//! again at every evaluation: `{"point": [x, y, z]}` the piece that holds
//! the point (inside it, not on its faces), `{"side": "front"}` or
//! `{"side": "back"}` the pieces on that side of a single plane tool as the
//! plane is now (front along its normal), so that moving the plane keeps
//! the same side. The kept pieces get their ids as above among themselves:
//! the first keeps the body. An entry that picks no piece warns that the
//! piece is gone; when no piece is left at all the feature fails. Every
//! evaluation lists the pieces before keeping ([`PieceChoice`]: a point
//! inside each, what would keep it, whether it is kept) for the panel's
//! dots, also when it kept none.

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
    /// The pieces to keep (mitcad#2504); none keeps every piece.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep: Vec<KeepPiece>,
}

/// What keeps a piece of a split (mitcad#2504), found again at every
/// evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepPiece {
    /// The piece that holds the point, inside it (a point on its faces
    /// holds nothing).
    Point(Vec3),
    /// The pieces on one side of the plane tool, as the plane is now (a
    /// single plane tool only).
    Side(PlaneSide),
}

/// A side of a plane: `front` is where its normal points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaneSide {
    Front,
    Back,
}

/// A piece a split made before it kept some (mitcad#2504), for the panel:
/// the body it is of, a point inside it (where the panel draws its dot),
/// what keeps it and whether the split keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PieceChoice {
    pub body: BodyUid,
    pub point: Vec3,
    pub keep: KeepPiece,
    pub kept: bool,
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
            keep: clone,
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
        if self
            .keep
            .iter()
            .any(|k| matches!(k, KeepPiece::Point(p) if p.iter().any(|v| !v.is_finite())))
        {
            return Err("a point of a piece to keep is not finite".to_owned());
        }
        if !self.more_tools.is_empty() && self.keep.iter().any(|k| matches!(k, KeepPiece::Side(_)))
        {
            return Err(
                "a side of the plane keeps pieces only of a single plane tool: keep pieces by \
                 their points with further tools"
                    .to_owned(),
            );
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
        // A single plane tool's plane, for the sides of `keep` (mitcad#2504).
        let plane = match &tool {
            Resolved::Plane { origin, normal } if more.is_empty() => Some((*origin, *normal)),
            _ => None,
        };
        if plane.is_none() && self.keep.iter().any(|k| matches!(k, KeepPiece::Side(_))) {
            return Err(
                "a side of the plane keeps pieces only when the splitting tool is a plane: keep \
                 pieces by their points"
                    .to_owned(),
            );
        }
        let mut split = Vec::with_capacity(self.bodies.len());
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
            split.push((*body, pieces));
        }
        // The pieces to keep (mitcad#2504), listed for the panel first:
        // also a split that keeps none shows them.
        let kept = keep_pieces(ctx, &self.keep, plane, self.bodies.len() == 1, &split)?;
        let mut output = FeatureOutput::default();
        for ((body, pieces), kept) in split.into_iter().zip(kept) {
            let pieces: Vec<K::Shape> = pieces
                .into_iter()
                .zip(kept)
                .filter_map(|(piece, kept)| kept.then_some(piece))
                .collect();
            if pieces.is_empty() {
                // None of its pieces is kept: the body goes.
                output.changes.push(BodyChange::Remove(body));
                continue;
            }
            // The pieces' ids (mitcad#408); none left for the body removes
            // it.
            let named = ctx.piece_ids(body, pieces);
            if !named.iter().any(|(id, _)| *id == body) {
                output.changes.push(BodyChange::Remove(body));
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

/// Which pieces of each body the split keeps (mitcad#2504), in the order of
/// `split`: every piece when `keep` is empty. First lists the pieces for
/// the panel (`ctx.piece_choices`), so that a split that keeps none still
/// shows them: a piece is kept by its side when it is the only piece on
/// that side of a single plane tool and only one body is split, else by a
/// point inside it; one the kernel finds no point inside is not listed. An
/// entry that picks no piece warns; none kept at all fails.
fn keep_pieces<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    keep: &[KeepPiece],
    plane: Option<([f64; 3], [f64; 3])>,
    one_body: bool,
    split: &[(BodyUid, Vec<K::Shape>)],
) -> Result<Vec<Vec<bool>>, String> {
    let kernel = ctx.kernel;
    let points: Vec<Vec3> = keep
        .iter()
        .filter_map(|k| match k {
            KeepPiece::Point(p) => Some(*p),
            KeepPiece::Side(_) => None,
        })
        .collect();
    let mut picked = vec![false; keep.len()];
    let mut kept = Vec::with_capacity(split.len());
    let mut choices = Vec::new();
    for (body, pieces) in split {
        let found: Vec<Found> = pieces.iter().map(|p| found(kernel, p, plane)).collect();
        let mut body_kept = Vec::with_capacity(pieces.len());
        for (piece, f) in pieces.iter().zip(&found) {
            let holds = if points.is_empty() {
                Vec::new()
            } else {
                kernel.points_inside(piece, &points).map_err(kernel_error)?
            };
            let mut point = 0;
            let mut is_kept = keep.is_empty();
            for (i, entry) in keep.iter().enumerate() {
                let picks = match entry {
                    KeepPiece::Point(_) => {
                        point += 1;
                        holds.get(point - 1).copied().unwrap_or(false)
                    }
                    KeepPiece::Side(side) => f.side == Some(*side),
                };
                if picks {
                    picked[i] = true;
                    is_kept = true;
                }
            }
            body_kept.push(is_kept);
        }
        let alone = |side: PlaneSide| found.iter().filter(|g| g.side == Some(side)).count() == 1;
        for (f, is_kept) in found.iter().zip(&body_kept) {
            let entry = match (f.side, f.inside) {
                (Some(side), _) if one_body && alone(side) => KeepPiece::Side(side),
                (_, Some(p)) => KeepPiece::Point(p),
                _ => continue,
            };
            let Some(point) = f.inside.or(f.centre) else {
                continue;
            };
            choices.push(PieceChoice {
                body: *body,
                point,
                keep: entry,
                kept: *is_kept,
            });
        }
        kept.push(body_kept);
    }
    ctx.piece_choices = choices;
    let mut point = 0;
    let mut lost = Vec::new();
    for (entry, picked) in keep.iter().zip(&picked) {
        if let KeepPiece::Point(_) = entry {
            point += 1;
        }
        if *picked {
            continue;
        }
        lost.push(match entry {
            KeepPiece::Point(p) => format!(
                "no piece holds point {point} ({}, {}, {})",
                mm(p[0]),
                mm(p[1]),
                mm(p[2])
            ),
            // One whole message per side (docs/translations.md).
            KeepPiece::Side(PlaneSide::Front) => "no piece lies in front of the plane".to_owned(),
            KeepPiece::Side(PlaneSide::Back) => "no piece lies behind the plane".to_owned(),
        });
    }
    if !keep.is_empty() && !kept.iter().flatten().any(|k| *k) {
        return Err(format!(
            "none of the pieces to keep is left ({}): pick the pieces to keep again",
            lost.join("; ")
        ));
    }
    for reason in lost {
        ctx.warn(format!(
            "a piece to keep is gone ({reason}): the others are kept"
        ));
    }
    Ok(kept)
}

/// A length in mm for a message, to the micrometre.
fn mm(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0 + 0.0
}

/// What a piece's choice is made of: its centre of mass, a point inside it
/// and the side of a single plane tool it lies on (front along the normal).
struct Found {
    centre: Option<Vec3>,
    inside: Option<Vec3>,
    side: Option<PlaneSide>,
}

fn found<K: Kernel>(kernel: &K, piece: &K::Shape, plane: Option<([f64; 3], [f64; 3])>) -> Found {
    let centre = kernel.mass_properties(piece).ok().map(|m| m.center);
    let inside = inside_point(kernel, piece, centre);
    // A piece lies wholly on one side of the plane, and so does its centre.
    let side = plane.and_then(|(origin, normal)| {
        let at = centre.or(inside)?;
        let along: f64 = (0..3).map(|i| (at[i] - origin[i]) * normal[i]).sum();
        Some(if along >= 0.0 {
            PlaneSide::Front
        } else {
            PlaneSide::Back
        })
    });
    Found {
        centre,
        inside,
        side,
    }
}

/// A point inside the piece: its centre of mass when that lies inside it,
/// else, of the inside points of a grid over its box (a coarse grid, then
/// a finer one), the one farthest from its faces among the few nearest
/// the centre (so that only a few distances are measured); None when the
/// kernel finds none or cannot tell inside from outside.
fn inside_point<K: Kernel>(kernel: &K, piece: &K::Shape, centre: Option<Vec3>) -> Option<Vec3> {
    // The inside points whose distance from the faces is measured.
    const MEASURED: usize = 16;
    if let Some(c) = centre
        && kernel.points_inside(piece, &[c]).ok()?.first() == Some(&true)
    {
        return Some(c);
    }
    let b = kernel.bounding_box(piece).ok().flatten()?;
    let middle: Vec3 =
        centre.unwrap_or_else(|| std::array::from_fn(|i| (b.min[i] + b.max[i]) / 2.0));
    for cells in [4_usize, 8] {
        let at = |axis: usize, n: usize| {
            b.min[axis] + (b.max[axis] - b.min[axis]) * (n as f64 + 0.5) / cells as f64
        };
        let mut points = Vec::with_capacity(cells * cells * cells);
        for i in 0..cells {
            for j in 0..cells {
                for k in 0..cells {
                    points.push([at(0, i), at(1, j), at(2, k)]);
                }
            }
        }
        let inside = kernel.points_inside(piece, &points).ok()?;
        let mut candidates: Vec<Vec3> = points
            .into_iter()
            .zip(inside)
            .filter_map(|(p, i)| i.then_some(p))
            .collect();
        let off = |p: &Vec3| (0..3).map(|i| (p[i] - middle[i]).powi(2)).sum::<f64>();
        candidates.sort_by(|a, b| off(a).total_cmp(&off(b)));
        candidates.truncate(MEASURED);
        let Some(first) = candidates.first().copied() else {
            continue;
        };
        let deepest = kernel
            .boundary_distances(piece, &candidates)
            .ok()
            .and_then(|d| {
                d.iter()
                    .enumerate()
                    .filter(|(_, d)| d.is_finite())
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(i, _)| candidates[i])
            });
        return Some(deepest.unwrap_or(first));
    }
    None
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
