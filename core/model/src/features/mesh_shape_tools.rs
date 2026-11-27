// SPDX-License-Identifier: MIT
//! Mesh edits that change a mesh's shape (mitcad#2515; the algorithms in
//! `crate::mesh_tools`): features that make new mesh bodies of mesh bodies
//! and leave them as they were, hidden in the same undo step as the other
//! mesh edits (`features/mesh_tools.rs`, `document/mesh_convert.rs`).
//!
//! - `mesh_smooth`: a mesh or a region of it smoothed by an amount and a
//!   number of passes, sharp edges kept, the volume kept;
//! - `mesh_combine`: mesh bodies joined, cut or intersected (the kernel's
//!   boolean of their faceted solids, the result's faces triangulated
//!   again), or merged into one body without a boolean;
//! - `mesh_offset`: a mesh offset by a distance (outward, inward, or both
//!   ways as a closed wall), or shelled into a closed wall of a thickness
//!   with openings;
//! - `mesh_align`: a mesh moved onto another body by picked point pairs
//!   and a best fit, the deviation left stated;
//! - `mesh_plane_cut`: a mesh cut by a plane into two mesh bodies, the cut
//!   capped or open.
//!
//! A mesh a tool cannot work on (open, crossing itself) is refused in
//! words, with the places that paint where (a box about each hole, points
//! where triangles cut through each other). The new bodies' one face each
//! is named `<feature>:import(<n>)`: 0, and for Plane Cut 0 above, 1 below.

#[cfg(test)]
#[path = "mesh_shape_tools_tests.rs"]
mod tests;

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::mesh_convert::{DEFAULT_ANGLE, faceted_solid};
use super::mesh_tools::{
    MeshLasso, check_angle, check_range, mesh_body, mesh_of, one_body, plural,
};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::exchange::{MeshTolerance, TriangleMesh};
use crate::ids::BodyUid;
use crate::kernel::{BooleanOp, Kernel};
use crate::mesh_tools::align::align;
use crate::mesh_tools::check::{Place, crossings, openings};
use crate::mesh_tools::offset::{Overreach, distance_range, offset, wall};
use crate::mesh_tools::plane_cut::plane_cut;
use crate::mesh_tools::region::{Brush, Lasso, Pick, picked};
use crate::mesh_tools::smooth::{MAX_PASSES, SmoothOptions, smooth};
use crate::mesh_tools::{joined, merged, volume};
use crate::notice::{FeatureError, NoticePlace};
use crate::parameters::ParamId;

/// The mesh bodies a mesh edit of this family makes its new bodies of:
/// what adding it hides.
pub fn edited_meshes<P>(def: &super::FeatureDef<P>) -> Vec<BodyUid> {
    use super::FeatureDef as F;
    match def {
        F::MeshSmooth(d) => vec![d.body],
        F::MeshCombine(d) => std::iter::once(d.target)
            .chain(d.tools.iter().copied())
            .collect(),
        F::MeshOffset(d) => vec![d.body],
        F::MeshAlign(d) => vec![d.body],
        F::MeshPlaneCut(d) => vec![d.body],
        _ => Vec::new(),
    }
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// A B-rep body's faces as triangles within 0.01 mm (Align's target), and
/// the faces of Combine's booleans (planes: their own triangles).
const FINE: MeshTolerance = MeshTolerance {
    deviation: 0.01,
    angle: 0.1,
    face_triangles: 0,
    max_edge: 0.0,
};

/// Rounded to a ten-thousandth, for the messages.
fn round(value: f64) -> f64 {
    (value * 1e4).round() / 1e4
}

fn notice_places(found: &[Place]) -> Vec<NoticePlace> {
    found
        .iter()
        .map(|place| match *place {
            Place::Point(at) => NoticePlace::point(at),
            Place::Box { min, max } => NoticePlace::Box { min, max },
        })
        .collect()
}

fn point_places(points: &[[f64; 3]]) -> Vec<NoticePlace> {
    points.iter().map(|p| NoticePlace::point(*p)).collect()
}

/// Refused in words, with the places that paint where.
fn refusal(message: String, places: Vec<NoticePlace>) -> String {
    FeatureError::new(message).with_places(places).text()
}

/// Refused unless the mesh is closed: the holes and the edges of more than
/// two triangles painted.
fn require_closed(mesh: &TriangleMesh, tool: &str, which: &str) -> Result<(), String> {
    let found = openings(mesh);
    if found.closed() {
        return Ok(());
    }
    Err(refusal(
        format!(
            "{tool} needs closed meshes, and {which} is open: {}. Repair Mesh fills holes and \
             makes a mesh manifold",
            found.describe()
        ),
        notice_places(&found.places),
    ))
}

/// Refused where the mesh crosses itself: the crossings painted.
fn require_uncrossed(mesh: &TriangleMesh, tool: &str, which: &str) -> Result<(), String> {
    let found = crossings(mesh);
    if found.pairs == 0 {
        return Ok(());
    }
    Err(refusal(
        format!(
            "{tool} needs meshes that do not cross themselves, and {which} does: {} of its \
             triangles cut through each other",
            plural(found.pairs, "pair", "pairs")
        ),
        point_places(&found.places),
    ))
}

// A region of a mesh: Smooth's region, Shell's openings.

/// Triangles of a mesh picked as Remove Region picks them (lassos, a
/// brush, numbers): Smooth's region, Shell's openings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshRegion<P = ParamId> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lassos: Vec<MeshLasso>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brush: Vec<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triangles: Vec<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub through: bool,
}

impl<P> MeshRegion<P> {
    map_params! {
        MeshRegion {
            lassos: clone,
            brush: clone,
            radius: option,
            triangles: clone,
            through: copy,
        }
    }
}

fn map_region<P, Q, E>(
    region: &Option<MeshRegion<P>>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<MeshRegion<Q>>, E> {
    region.as_ref().map(|r| r.map_params(f)).transpose()
}

impl MeshRegion {
    fn check(&self, what: &str) -> Result<(), String> {
        if self.lassos.is_empty() && self.brush.is_empty() && self.triangles.is_empty() {
            return Err(format!("{what} needs a lasso, a brush stroke or triangles"));
        }
        if !self.brush.is_empty() && self.radius.is_none() {
            return Err("a brush needs its radius".to_owned());
        }
        if let Some(lasso) = self.lassos.iter().find(|l| l.points.len() < 3) {
            return Err(format!(
                "a lasso needs at least 3 points, got {}",
                lasso.points.len()
            ));
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        match self.radius {
            Some(id) => check_range("the brush radius", value(id), 0.0, 1e6, " mm"),
            None => Ok(()),
        }
    }

    /// Whether each triangle of the mesh is picked.
    fn picked<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        mesh: &TriangleMesh,
    ) -> Result<Vec<bool>, String> {
        let radius = self.radius.map(|id| ctx.param(id)).transpose()?;
        if let Some(r) = radius {
            check_range("the brush radius", r, 0.0, 1e6, " mm")?;
        }
        let pick = Pick {
            lassos: self
                .lassos
                .iter()
                .map(|l| Lasso {
                    normal: l.normal,
                    points: l.points.clone(),
                })
                .collect(),
            brush: radius
                .filter(|_| !self.brush.is_empty())
                .map(|radius| Brush {
                    radius,
                    points: self.brush.clone(),
                }),
            triangles: self.triangles.clone(),
            through: self.through,
        };
        Ok(picked(mesh, &pick))
    }
}

// Smooth.

/// A mesh body smoothed: `passes` passes each moving a vertex `amount` (more
/// than 0, at most 1) of the way to its neighbours' middle and back a
/// little; edges sharper than `sharp_angle` (radians) kept when given; the
/// volume of a closed mesh kept unless `keep_volume` is false; only the
/// triangles of `region` when given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshSmoothDef<P = ParamId> {
    pub body: BodyUid,
    pub amount: P,
    pub passes: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sharp_angle: Option<P>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub keep_volume: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<MeshRegion<P>>,
}

impl<P> MeshSmoothDef<P> {
    pub const TYPE: &'static str = "mesh_smooth";
    pub const BASE_NAME: &'static str = "Smooth";

    map_params! {
        MeshSmoothDef {
            body: copy,
            amount: param,
            passes: param,
            sharp_angle: option,
            keep_volume: copy,
            region: with(map_region),
        }
    }
}

fn check_amount(value: f64) -> Result<(), String> {
    check_range("the amount", value, 0.0, 1.0, "")
}

fn check_passes(value: f64) -> Result<(), String> {
    if value.is_finite() && value >= 1.0 && value <= MAX_PASSES as f64 && value.fract() == 0.0 {
        Ok(())
    } else {
        Err(format!(
            "the passes must be a whole number from 1 to {MAX_PASSES}, got {value}"
        ))
    }
}

impl FeatureInfo for MeshSmoothDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if let Some(region) = &self.region {
            region.check("a region")?;
        }
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_amount(value(self.amount))?;
        check_passes(value(self.passes))?;
        if let Some(id) = self.sharp_angle {
            check_angle("the sharp edge angle", value(id), 180.0)?;
        }
        match &self.region {
            Some(region) => region.check_values(value),
            None => Ok(()),
        }
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshSmoothDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let amount = ctx.param(self.amount)?;
        let passes = ctx.param(self.passes)?;
        check_amount(amount)?;
        check_passes(passes)?;
        let sharp_angle = self.sharp_angle.map(|id| ctx.param(id)).transpose()?;
        if let Some(angle) = sharp_angle {
            check_angle("the sharp edge angle", angle, 180.0)?;
        }
        let mesh = mesh_of(ctx, self.body, "Smooth")?;
        let region = match &self.region {
            Some(region) => Some(region.picked(ctx, &mesh)?),
            None => None,
        };
        let done = smooth(
            &mesh,
            region.as_deref(),
            &SmoothOptions {
                amount,
                passes: passes as usize,
                sharp_angle,
                keep_volume: self.keep_volume,
            },
        )?;
        let mut text = format!(
            "Smoothed {} in {}; moved up to {} mm",
            plural(done.free + done.along_edges, "vertex", "vertices"),
            plural(passes as usize, "pass", "passes"),
            round(done.moved)
        );
        if done.along_edges > 0 {
            text.push_str(&format!(
                "; {} along sharp edges",
                plural(done.along_edges, "vertex", "vertices")
            ));
        }
        match done.volume {
            Some((before, after)) if (after - before).abs() <= 1e-6 * before.abs().max(1e-9) => {
                text.push_str(&format!("; the volume kept at {} mm³", round(after)));
            }
            Some((before, after)) => text.push_str(&format!(
                "; the volume went from {} to {} mm³",
                round(before),
                round(after)
            )),
            None if self.keep_volume => {
                let found = openings(&mesh);
                ctx.warn_at(
                    format!(
                        "the mesh is open ({}), so it encloses no volume to keep; it was smoothed \
                         without keeping one",
                        found.describe()
                    ),
                    notice_places(&found.places),
                );
            }
            None => {}
        }
        ctx.inform(text);
        let shape = mesh_body(ctx, &done.mesh, 0)?;
        let uid = ctx.new_body();
        Ok(one_body(uid, shape))
    }
}

// Combine.

/// What `mesh_combine` does with its meshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshCombineOp {
    /// The volume of any of them.
    Join,
    /// The target's volume less the tools'.
    Cut,
    /// The volume all of them share.
    Intersect,
    /// The meshes put together into one body as they are, no boolean.
    Merge,
}

/// Mesh bodies combined into a new mesh body: `tools` joined to, cut from
/// or intersected with `target` (the kernel's boolean of their faceted
/// solids, closed meshes that do not cross themselves), or all merged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshCombineDef<P = ParamId> {
    pub target: BodyUid,
    pub tools: Vec<BodyUid>,
    pub operation: MeshCombineOp,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> MeshCombineDef<P> {
    pub const TYPE: &'static str = "mesh_combine";
    pub const BASE_NAME: &'static str = "MeshCombine";

    map_params! {
        MeshCombineDef {
            target: copy,
            tools: clone,
            operation: copy,
            values: phantom,
        }
    }
}

/// How finely Combine welds a mesh into a faceted solid (mm).
const FACETED_TOLERANCE: f64 = 1e-5;

impl FeatureInfo for MeshCombineDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.target);
        for tool in &self.tools {
            references.body(*tool);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.tools.is_empty() {
            return Err("Combine needs a tool mesh besides the target".to_owned());
        }
        if self.tools.contains(&self.target) {
            return Err("the target mesh is one of the tools too".to_owned());
        }
        let mut seen = std::collections::HashSet::new();
        if !self.tools.iter().all(|t| seen.insert(*t)) {
            return Err("a tool mesh is picked more than once".to_owned());
        }
        ctx.body(self.target)?;
        for tool in &self.tools {
            ctx.body(*tool)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshCombineDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut meshes = vec![mesh_of(ctx, self.target, "Combine")?];
        for tool in &self.tools {
            meshes.push(mesh_of(ctx, *tool, "Combine")?);
        }
        let which = |k: usize| {
            if k == 0 {
                "the target mesh".to_owned()
            } else if meshes.len() == 2 {
                "the tool mesh".to_owned()
            } else {
                format!("tool mesh {k}")
            }
        };
        let (op, verb) = match self.operation {
            MeshCombineOp::Merge => {
                let all = merged(&meshes);
                ctx.inform(format!(
                    "Merged {} into one body of {}",
                    plural(meshes.len(), "mesh", "meshes"),
                    plural(all.triangles.len(), "triangle", "triangles")
                ));
                let shape = mesh_body(ctx, &all, 0)?;
                let uid = ctx.new_body();
                return Ok(one_body(uid, shape));
            }
            MeshCombineOp::Join => (BooleanOp::Join, "Joined"),
            MeshCombineOp::Cut => (BooleanOp::Cut, "Cut"),
            MeshCombineOp::Intersect => (BooleanOp::Intersect, "Intersected"),
        };
        for (k, mesh) in meshes.iter().enumerate() {
            require_closed(mesh, "Combine", &which(k))?;
            require_uncrossed(mesh, "Combine", &which(k))?;
        }
        let mut solids = Vec::with_capacity(meshes.len());
        for (k, mesh) in meshes.iter().enumerate() {
            let faceted = faceted_solid(mesh, FACETED_TOLERANCE, DEFAULT_ANGLE)
                .map_err(|e| format!("{} as a solid: {e}", which(k)))?;
            let (built, report) = ctx
                .kernel
                .boundary_body(&faceted.boundary)
                .map_err(|e| format!("{} as a solid: {e}", which(k)))?;
            if report.left_out > 0 || !report.solid {
                return Err(format!(
                    "{}'s faces do not build a closed solid for the boolean ({} of {} left out)",
                    which(k),
                    report.left_out,
                    faceted.boundary.faces.len()
                ));
            }
            solids.push(built);
        }
        let mut solids = solids.into_iter();
        let mut current = vec![solids.next().ok_or("Combine has no target mesh")?];
        for (k, tool) in solids.enumerate() {
            let targets: Vec<&K::Shape> = current.iter().collect();
            let out = ctx
                .kernel
                .boolean(op, &targets, &tool)
                .map_err(|e| format!("the boolean with {}: {e}", which(k + 1)))?;
            current = out.pieces.into_iter().map(|piece| piece.shape).collect();
            if current.is_empty() {
                return Err(match op {
                    BooleanOp::Intersect => format!(
                        "nothing is left: the target mesh and {} share no volume",
                        which(k + 1)
                    ),
                    _ => format!(
                        "nothing is left: {} takes the whole target away",
                        which(k + 1)
                    ),
                });
            }
        }
        let mut pieces = Vec::with_capacity(current.len());
        for shape in &current {
            pieces.push(
                ctx.kernel
                    .triangle_mesh(shape, &FINE)
                    .map_err(|e| format!("the combined body's triangles: {e}"))?,
            );
        }
        let result = joined(&merged(&pieces));
        if result.triangles.is_empty() {
            return Err("the combined body has no triangles".to_owned());
        }
        let found = openings(&result);
        if found.closed() {
            ctx.inform(format!(
                "{verb} {}: a closed mesh of {} enclosing {} mm³",
                plural(meshes.len(), "mesh", "meshes"),
                plural(result.triangles.len(), "triangle", "triangles"),
                round(volume(&result))
            ));
        } else {
            ctx.warn_at(
                format!(
                    "the combined mesh is open: {}; Repair Mesh can close it",
                    found.describe()
                ),
                notice_places(&found.places),
            );
        }
        let shape = mesh_body(ctx, &result, 0)?;
        let uid = ctx.new_body();
        Ok(one_body(uid, shape))
    }
}

// Offset and Shell.

/// Whether `mesh_offset` moves the surface or makes a wall of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshOffsetMode {
    /// The surface moved by the distance (both ways: the closed wall
    /// between the two offsets).
    #[default]
    Offset,
    /// A closed wall of the distance's thickness, with openings.
    Shell,
}

/// Which way `mesh_offset` goes from the surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshOffsetSide {
    /// Along the triangles' normals (out of a closed mesh).
    #[default]
    Outward,
    Inward,
    /// Both ways: an offset by the distance each way, a shell half the
    /// thickness each way.
    Both,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// A mesh body offset or shelled: with `mode` `offset` the surface moved
/// by `distance` (mm) to `side` (both: the closed wall between the moves
/// either way); with `shell` a closed wall `distance` thick on `side` of
/// the surface (both: half each way), the triangles of `openings` taken
/// out and the rims they leave joined across the wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshOffsetDef<P = ParamId> {
    pub body: BodyUid,
    #[serde(default, skip_serializing_if = "is_default")]
    pub mode: MeshOffsetMode,
    #[serde(default, skip_serializing_if = "is_default")]
    pub side: MeshOffsetSide,
    pub distance: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openings: Option<MeshRegion<P>>,
}

impl<P> MeshOffsetDef<P> {
    pub const TYPE: &'static str = "mesh_offset";
    pub const BASE_NAME: &'static str = "MeshOffset";

    map_params! {
        MeshOffsetDef {
            body: copy,
            mode: copy,
            side: copy,
            distance: param,
            openings: with(map_region),
        }
    }
}

impl FeatureInfo for MeshOffsetDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if let Some(openings) = &self.openings {
            if self.mode != MeshOffsetMode::Shell {
                return Err("openings are for a shell; an offset has none".to_owned());
            }
            openings.check("the openings")?;
        }
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_range(self.distance_name(), value(self.distance), 0.0, 1e5, " mm")?;
        match &self.openings {
            Some(openings) => openings.check_values(value),
            None => Ok(()),
        }
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl MeshOffsetDef {
    fn distance_name(&self) -> &'static str {
        match self.mode {
            MeshOffsetMode::Offset => "the distance",
            MeshOffsetMode::Shell => "the thickness",
        }
    }
}

impl<K: Kernel> Evaluate<K> for MeshOffsetDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let distance = ctx.param(self.distance)?;
        check_range(self.distance_name(), distance, 0.0, 1e5, " mm")?;
        let tool = match self.mode {
            MeshOffsetMode::Offset => "Offset",
            MeshOffsetMode::Shell => "Shell",
        };
        let mesh = mesh_of(ctx, self.body, tool)?;
        let side = match self.side {
            MeshOffsetSide::Outward => "outward",
            MeshOffsetSide::Inward => "inward",
            MeshOffsetSide::Both => "both ways",
        };
        // Where the moved triangles turned over and a closed mesh's offset
        // was built from its distance instead (mitcad#2564).
        const TRIMMED: &str = ", trimmed where the moved surface turned over";
        let (made, folded, folded_places, overreach, text) = match (self.mode, self.side) {
            (MeshOffsetMode::Offset, MeshOffsetSide::Outward | MeshOffsetSide::Inward) => {
                let signed = if self.side == MeshOffsetSide::Inward {
                    -distance
                } else {
                    distance
                };
                let moved = offset(&mesh, signed);
                let mut text = format!("Offset {} mm {side}", round(distance));
                if moved.trimmed {
                    text.push_str(TRIMMED);
                }
                if moved.folded == 0
                    && let Some((least, most)) = distance_range(&mesh, &moved.mesh)
                {
                    text.push_str(&format!(
                        "; the surface lies {} to {} mm from the mesh",
                        round(least),
                        round(most)
                    ));
                }
                (
                    moved.mesh,
                    moved.folded,
                    moved.folded_places,
                    moved.overreach,
                    text,
                )
            }
            (MeshOffsetMode::Offset, MeshOffsetSide::Both) => {
                let made = wall(&mesh, -distance, distance, None)?;
                let mut text = format!(
                    "Offset {} mm both ways: a closed wall {} mm thick",
                    round(distance),
                    round(2.0 * distance)
                );
                if made.trimmed {
                    text.push_str(TRIMMED);
                }
                (
                    made.mesh,
                    made.folded,
                    made.folded_places,
                    made.overreach,
                    text,
                )
            }
            (MeshOffsetMode::Shell, _) => {
                let (inner, outer) = match self.side {
                    MeshOffsetSide::Outward => (0.0, distance),
                    MeshOffsetSide::Inward => (-distance, 0.0),
                    MeshOffsetSide::Both => (-distance / 2.0, distance / 2.0),
                };
                let openings = match &self.openings {
                    Some(region) => Some(region.picked(ctx, &mesh)?),
                    None => None,
                };
                let made = wall(&mesh, inner, outer, openings.as_deref())?;
                let mut text = format!(
                    "Shelled {} mm {side}: a closed wall of {}",
                    round(distance),
                    plural(made.mesh.triangles.len(), "triangle", "triangles")
                );
                if made.rims > 0 {
                    text.push_str(&format!(", open at {}", plural(made.rims, "rim", "rims")));
                }
                if made.trimmed {
                    text.push_str(TRIMMED);
                }
                (
                    made.mesh,
                    made.folded,
                    made.folded_places,
                    made.overreach,
                    text,
                )
            }
        };
        let smaller = match self.mode {
            MeshOffsetMode::Offset => "distance",
            MeshOffsetMode::Shell => "thickness",
        };
        if folded > 0 {
            let message = match overreach {
                Overreach::TurnedOver => format!(
                    "{tool} by {} mm turns {} over where the mesh curves tighter than that; a \
                     smaller {smaller} works",
                    round(distance),
                    plural(folded, "triangle", "triangles"),
                ),
                Overreach::TooThin => match self.mode {
                    MeshOffsetMode::Offset => format!(
                        "Offset by {} mm is more than the mesh can take: where its sides come \
                         closer than that allows ({}), the offset surface would split apart, \
                         close a hole or vanish; a smaller distance works",
                        round(distance),
                        plural(folded, "place", "places"),
                    ),
                    MeshOffsetMode::Shell => format!(
                        "Shell by {} mm is more than the mesh can take: where its sides come \
                         closer than that allows ({}), the shell would split apart, close a hole \
                         or vanish; a smaller thickness works",
                        round(distance),
                        plural(folded, "place", "places"),
                    ),
                },
            };
            return Err(refusal(message, point_places(&folded_places)));
        }
        let crossed = crossings(&made);
        if crossed.pairs > 0 {
            return Err(refusal(
                format!(
                    "the {} would cross itself: {} of its triangles cut through each other (where \
                     the mesh's sides come closer than the {}); a smaller one works",
                    match self.mode {
                        MeshOffsetMode::Offset => "offset surface",
                        MeshOffsetMode::Shell => "shell",
                    },
                    plural(crossed.pairs, "pair", "pairs"),
                    match self.mode {
                        MeshOffsetMode::Offset => "distance",
                        MeshOffsetMode::Shell => "thickness",
                    }
                ),
                point_places(&crossed.places),
            ));
        }
        ctx.inform(text);
        let shape = mesh_body(ctx, &made, 0)?;
        let uid = ctx.new_body();
        Ok(one_body(uid, shape))
    }
}

// Align.

/// A point picked on the mesh (`from`) and where it belongs on the target
/// (`to`), in the component's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshPointPair {
    pub from: [f64; 3],
    pub to: [f64; 3],
}

/// A mesh body moved onto `target` (a mesh body, or a solid or surface
/// body by its faces' triangles) as a new mesh body: by the point pairs
/// (three or more), then by best fit unless `best_fit` is false.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshAlignDef<P = ParamId> {
    pub body: BodyUid,
    pub target: BodyUid,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pairs: Vec<MeshPointPair>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub best_fit: bool,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> MeshAlignDef<P> {
    pub const TYPE: &'static str = "mesh_align";
    pub const BASE_NAME: &'static str = "MeshAlign";

    map_params! {
        MeshAlignDef {
            body: copy,
            target: copy,
            pairs: clone,
            best_fit: copy,
            values: phantom,
        }
    }
}

impl FeatureInfo for MeshAlignDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references.body(self.target);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.body == self.target {
            return Err(
                "Align moves a mesh onto another body; the target is the mesh itself".to_owned(),
            );
        }
        if !self.pairs.is_empty() && self.pairs.len() < 3 {
            return Err(format!(
                "aligning by points needs three point pairs or more, got {}",
                self.pairs.len()
            ));
        }
        if self.pairs.is_empty() && !self.best_fit {
            return Err("Align needs point pairs, best fit or both".to_owned());
        }
        if self
            .pairs
            .iter()
            .any(|p| p.from.iter().chain(&p.to).any(|x| !x.is_finite()))
        {
            return Err("a point of a pair is not three finite numbers".to_owned());
        }
        ctx.body(self.body)?;
        ctx.body(self.target)
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshAlignDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let moving = mesh_of(ctx, self.body, "Align")?;
        let target_shape = ctx.body(self.target)?;
        let target = if ctx.is_mesh(&target_shape) {
            mesh_of(ctx, self.target, "Align")?
        } else {
            joined(
                &ctx.kernel
                    .triangle_mesh(&target_shape, &FINE)
                    .map_err(|e| format!("the target's faces as triangles: {e}"))?,
            )
        };
        if target.triangles.is_empty() {
            return Err("the target has no faces to align to".to_owned());
        }
        let pairs: Vec<([f64; 3], [f64; 3])> = self.pairs.iter().map(|p| (p.from, p.to)).collect();
        let aligned = align(&moving.vertices, &target, &pairs, self.best_fit)?;
        let moved = TriangleMesh {
            vertices: moving
                .vertices
                .iter()
                .map(|p| aligned.transform.apply(*p))
                .collect(),
            triangles: moving.triangles.clone(),
        };
        if self.best_fit && !aligned.converged {
            ctx.warn(format!(
                "the best fit stopped after {} before it settled; the mesh may lie a little off",
                plural(aligned.iterations, "iteration", "iterations")
            ));
        }
        let middle = |mesh: &TriangleMesh| {
            let n = mesh.vertices.len().max(1) as f64;
            mesh.vertices.iter().fold([0.0; 3], |sum, p| {
                [sum[0] + p[0] / n, sum[1] + p[1] / n, sum[2] + p[2] / n]
            })
        };
        let shift = mitcad_vec3::distance(middle(&moving), middle(&moved));
        ctx.inform(format!(
            "Moved {} mm and turned {}°; the mesh lies within {} mm of the target, {} mm on \
             average (root mean square)",
            round(shift),
            round(aligned.transform.angle().to_degrees()),
            round(aligned.max),
            round(aligned.rms)
        ));
        let shape = mesh_body(ctx, &moved, 0)?;
        let uid = ctx.new_body();
        Ok(one_body(uid, shape))
    }
}

// Plane Cut.

/// Which pieces `mesh_plane_cut` keeps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshCutKeep {
    #[default]
    Both,
    /// The piece on the side the plane's normal points to.
    Above,
    Below,
}

/// A mesh body cut by `plane` (an origin plane, a construction plane, a
/// planar face) into the piece above it and the piece below, as new mesh
/// bodies (`keep` either or both: above first); the cut closed by a cap
/// unless `cap` is false.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshPlaneCutDef<P = ParamId> {
    pub body: BodyUid,
    pub plane: GeomRef,
    #[serde(default, skip_serializing_if = "is_default")]
    pub keep: MeshCutKeep,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub cap: bool,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> MeshPlaneCutDef<P> {
    pub const TYPE: &'static str = "mesh_plane_cut";
    pub const BASE_NAME: &'static str = "PlaneCut";

    map_params! {
        MeshPlaneCutDef {
            body: copy,
            plane: clone,
            keep: copy,
            cap: copy,
            values: phantom,
        }
    }
}

impl FeatureInfo for MeshPlaneCutDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        self.plane.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.plane.check(ctx, Want::Plane)?;
        ctx.body(self.body)
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshPlaneCutDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let frame = ctx.plane(&self.plane)?;
        let mesh = mesh_of(ctx, self.body, "Plane Cut")?;
        let cut = plane_cut(&mesh, frame.origin, frame.normal(), self.cap)?;
        if self.cap && cut.open > 0 {
            let found = openings(&mesh);
            return Err(refusal(
                format!(
                    "the cut runs across an opening of the mesh ({}), so it cannot be capped; \
                     Repair Mesh fills holes, or cut without a cap",
                    found.describe()
                ),
                notice_places(&found.places),
            ));
        }
        let mut output = FeatureOutput::default();
        let mut kept = Vec::new();
        for (k, (piece, wanted, name)) in [
            (&cut.above, self.keep != MeshCutKeep::Below, "above"),
            (&cut.below, self.keep != MeshCutKeep::Above, "below"),
        ]
        .into_iter()
        .enumerate()
        {
            if !wanted {
                continue;
            }
            if piece.triangles.is_empty() {
                return Err(format!(
                    "no triangle of the mesh lies {name} the plane; it only touches the plane there"
                ));
            }
            let shape = mesh_body(ctx, piece, k as u32)?;
            let uid = ctx.new_body();
            output.changes.push(BodyChange::Set(uid, shape));
            kept.push(if self.cap {
                format!("{} mm³ {name}", round(volume(piece)))
            } else {
                name.to_owned()
            });
        }
        let pieces = match kept.as_slice() {
            [one] => format!("the piece {one}"),
            [above, below] => format!("the pieces {above} and {below}"),
            _ => String::new(),
        };
        ctx.inform(if self.cap {
            format!("Cut the mesh, the cut capped: kept {pieces}")
        } else {
            format!("Cut the mesh, the cut left open: kept {pieces}")
        });
        Ok(output)
    }
}
