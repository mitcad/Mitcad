// SPDX-License-Identifier: MIT
//! Patch (mitcad#456): new surface bodies that fill regions of a sketch or
//! a closed loop of a body's edges. Each region becomes a body of one
//! planar face, `<feature>:patch(<region>)`, facing along the sketch's
//! normal; the loop becomes a body of one face, `<feature>:patch`: a plane
//! when the loop lies in one and the patch only passes through its edges,
//! else a surface fitted to the edges that meets the face beside each edge
//! with the continuity asked for (G0 through the edge, G1 tangent, G2 of
//! the same curvature) and passes through the points given. The loop is
//! named edges of the body, or one of its openings: a loop of free edges
//! (an open boundary of a surface body, whose edges have no names), found
//! by the faces it runs along; or named edges and single free edges
//! (`openings`, mitcad#1955) in one loop, which can close part of an
//! opening; sketch curves (`sketch_curves`, mitcad#1972) may join them in
//! the loop, so that a line across an opening closes it with free edges.
//! The body itself is not changed, unless the
//! loop has `stitch` (mitcad#1927): then the patch is sewn onto it, as
//! Stitch sews, and the body becomes a solid where the patch closes it. The
//! new bodies are the regions' in their order, then the loop's (none when
//! it is stitched).

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::extrude::{
    ProfileRef, check_profiles, profile_references, profile_regions, profile_set,
};
use super::face_refs::{add_faces, check_faces, kernel_error};
use super::mapping::map_params;
use super::path::SketchCurves;
use super::sheet_edges::{self, OpeningEdgesRef};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::surfaces::{PatchBoundary, PatchContinuity, PatchLoopSpec};
use crate::topo::{EdgeName, FaceName};

/// Has no values (its points are fixed); `P` only matches the other
/// feature types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchDef<P = ParamId> {
    /// Regions of one sketch: a planar surface body each.
    #[serde(
        default,
        deserialize_with = "profile_set",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub profiles: Vec<ProfileRef>,
    /// A closed loop of a body's edges: one surface body filling it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary: Option<PatchLoop>,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

/// The loop a patch fills and how the patch meets its faces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchLoop {
    pub body: BodyUid,
    /// Named edges of the loop: every edge each name resolves to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeName>,
    /// Free edges of the loop beside the named ones, as the surface
    /// features pick them (mitcad#1955, `sheet_edges`: an opening, and the
    /// edge nearest each point of `at`): with `edges`, one loop that can
    /// close part of an opening.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<OpeningEdgesRef>,
    /// Sketch curves of the loop beside its edges (mitcad#1972): curves
    /// of planar or 3D sketches, projected ones too, whose ends meet the
    /// edges' ends; the patch passes through them. With single free edges
    /// they close part of an opening across it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sketch_curves: Vec<SketchCurves>,
    /// Or a whole opening of the body (its free edges).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opening: Option<Opening>,
    #[serde(default, skip_serializing_if = "PatchContinuity::is_g0")]
    pub continuity: PatchContinuity,
    /// Named edges of the loop that meet their face otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_continuity: Vec<EdgeContinuity>,
    /// Points the patch passes through (mm, in the component's coordinates).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f64; 3]>,
    /// Sews the patch onto the body (mitcad#1927): the body becomes the
    /// sewn one, a solid where the patch closes it, and no new body is made.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stitch: bool,
}

/// A loop of a body's free edges: the `index`-th, in the kernel's
/// geometric order, of those whose edges bound exactly the named `faces`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub faces: Vec<FaceName>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub index: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// The continuity of one edge of the loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeContinuity {
    pub edge: EdgeName,
    pub continuity: PatchContinuity,
}

impl<P> PatchDef<P> {
    pub const TYPE: &'static str = "patch";
    pub const BASE_NAME: &'static str = "Patch";

    map_params! {
        PatchDef {
            profiles: clone,
            boundary: clone,
            values: phantom,
        }
    }
}

impl PatchLoop {
    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        let picked = !self.edges.is_empty() || !self.openings.is_empty();
        match (&self.opening, picked) {
            (Some(_), true) => {
                return Err("a patch fills its edges or an opening, not both".to_owned());
            }
            (None, false) => return Err("no edges or opening to fill selected".to_owned()),
            (None, true) => {
                if !self.edges.is_empty() {
                    ctx.edges(&self.edges)?;
                }
                sheet_edges::check(ctx, &self.openings)?;
            }
            (Some(opening), false) => check_faces(ctx, &opening.faces, "faces of the opening")?,
        }
        // Sketch curves close a loop with the body's edges (mitcad#1972).
        if !self.sketch_curves.is_empty() {
            if self.opening.is_some() {
                return Err(
                    "a whole opening is a loop by itself; sketch curves close a loop with single edges"
                        .to_owned(),
                );
            }
            for curves in &self.sketch_curves {
                if curves.reversed {
                    return Err("the sketch curves of a patch have no direction".to_owned());
                }
                curves.check_any(ctx)?;
            }
        }
        for (i, set) in self.edge_continuity.iter().enumerate() {
            if !self.edges.contains(&set.edge) {
                return Err(format!(
                    "edge {} has a continuity of its own but is not an edge of the loop",
                    set.edge
                ));
            }
            if self.edge_continuity[..i].iter().any(|s| s.edge == set.edge) {
                return Err(format!(
                    "edge {} has more than one continuity of its own",
                    set.edge
                ));
            }
        }
        if self.points.iter().flatten().any(|v| !v.is_finite()) {
            return Err("a point of the patch is not finite".to_owned());
        }
        Ok(())
    }
}

impl FeatureInfo for PatchDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        if let Some(boundary) = &self.boundary {
            references.body(boundary.body);
            references.edges(&boundary.edges);
            sheet_edges::add_references(&mut references, &boundary.openings);
            for curves in &boundary.sketch_curves {
                references.features.insert(curves.sketch);
            }
            if let Some(opening) = &boundary.opening {
                add_faces(&mut references, &opening.faces);
            }
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.profiles.is_empty() && self.boundary.is_none() {
            return Err("a patch needs sketch regions or a loop of edges".to_owned());
        }
        if !self.profiles.is_empty() {
            check_profiles(ctx, &self.profiles)?;
        }
        if let Some(boundary) = &self.boundary {
            boundary.check(ctx)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        !self.profiles.is_empty() || self.boundary.as_ref().is_none_or(|b| !b.stitch)
    }

    fn profiles(&self) -> &[ProfileRef] {
        &self.profiles
    }
}

impl<K: Kernel> Evaluate<K> for PatchDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut output = FeatureOutput::default();
        if !self.profiles.is_empty() {
            let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
            let patches = ctx
                .kernel
                .patch_regions(ctx.uid, &sketch.frame, &regions)
                .map_err(kernel_error)?;
            for patch in patches {
                output.changes.push(BodyChange::Set(ctx.new_body(), patch));
            }
        }
        if let Some(boundary) = &self.boundary {
            let patch = fill_loop(ctx, boundary)?;
            if boundary.stitch {
                // Sewn onto the body it fills, as Stitch sews: a solid where
                // the patch closes the body's last opening.
                let shape = ctx.surface_body(boundary.body)?;
                let sewn = ctx
                    .kernel
                    .stitch(&[&shape, &patch], STITCH_TOLERANCE)
                    .map_err(kernel_error)?;
                output.tool = Some(patch);
                output.changes.push(BodyChange::Set(boundary.body, sewn));
            } else {
                output.changes.push(BodyChange::Set(ctx.new_body(), patch));
            }
        }
        Ok(output)
    }
}

/// The patch's edges are the loop's own, fitted where the patch is not
/// planar: they are sewn onto the body within this (mm), as a blend's.
const STITCH_TOLERANCE: f64 = 0.01;

/// The surface body that fills the loop.
fn fill_loop<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    boundary: &PatchLoop,
) -> Result<K::Shape, String> {
    let shape = ctx.surface_body(boundary.body)?;
    // The names the kernel gets: each edge's own, or that of the edge
    // between the faces a sketch change renamed (mitcad#272).
    let edges = if boundary.edges.is_empty() {
        Vec::new()
    } else {
        ctx.require_edges(boundary.body, &shape, &boundary.edges)?
    };
    let mut edge_continuity = Vec::new();
    for set in &boundary.edge_continuity {
        let found = ctx.require_edges(boundary.body, &shape, std::slice::from_ref(&set.edge))?;
        edge_continuity.extend(found.into_iter().map(|e| (e, set.continuity)));
    }
    let opening = match &boundary.opening {
        Some(opening) => Some(find_opening(ctx, boundary.body, &shape, opening)?),
        None => None,
    };
    // Free edges beside the named ones (mitcad#1955).
    let resolved = sheet_edges::resolve(ctx, boundary.body, &shape, &boundary.openings)?;
    let openings = sheet_edges::kernel_openings(&resolved);
    // Sketch curves beside them (mitcad#1972), in model space.
    let mut curves = Vec::new();
    for set in &boundary.sketch_curves {
        let (_, pieces) = set.curves(ctx)?;
        curves.extend(pieces);
    }
    let spec = PatchLoopSpec {
        feature: ctx.uid,
        body: &shape,
        boundary: match opening {
            Some(index) => PatchBoundary::Opening(index),
            None => PatchBoundary::Edges(&edges),
        },
        openings: &openings,
        curves: &curves,
        continuity: boundary.continuity,
        edge_continuity: &edge_continuity,
        points: &boundary.points,
    };
    ctx.kernel.patch_loop(&spec).map_err(kernel_error)
}

/// The index, among the body's open boundaries, of the opening.
pub(crate) fn find_opening<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
    opening: &Opening,
) -> Result<usize, String> {
    let boundaries = ctx.kernel.open_boundaries(shape).map_err(kernel_error)?;
    let mut wanted: Vec<String> = opening.faces.iter().map(ToString::to_string).collect();
    wanted.sort();
    wanted.dedup();
    let along: Vec<usize> = boundaries
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            let mut faces: Vec<String> = b.faces.iter().map(ToString::to_string).collect();
            faces.sort();
            faces.dedup();
            faces == wanted
        })
        .map(|(i, _)| i)
        .collect();
    let index = opening.index as usize;
    match along.get(index) {
        Some(found) => Ok(*found),
        None if along.is_empty() => Err(format!(
            "body {body} (from {}) has no opening along the faces {}",
            ctx.feature_name(body.feature),
            wanted.join(", ")
        )),
        None => Err(format!(
            "body {body} (from {}) has {} openings along the faces {}, not {}",
            ctx.feature_name(body.feature),
            along.len(),
            wanted.join(", "),
            index + 1
        )),
    }
}
