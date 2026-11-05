// SPDX-License-Identifier: MIT
//! Envelope (mitcad#1271): a new body, a light outer skin of bodies. The
//! source is every body of the feature's component or of a component an
//! occurrence places (a linked machine's too), with the bodies of the
//! occurrences placed in it, or bodies chosen there. Gaps, holes and
//! features smaller than the detail size are closed over; openings larger
//! than it (a tunnel where product flows through a machine, the space
//! under a table) stay open, and enclosed cavities fill.
//!
//! The bodies are tessellated by the kernel ([`Kernel::triangle_mesh`])
//! and wrapped on a grid of cells half the detail size in plain Rust
//! (`wrap.rs`); the wrapped surface's triangles merge into planar faces
//! (Convert Mesh's [`faceted_solid`]) and the kernel builds them as a
//! solid ([`Kernel::boundary_body`]), its faces named
//! `<feature>:import(<n>)`. The result is a faceted solid of few faces
//! whatever the source: its faces are parallel to the axes of the
//! feature's component, within one cell of the source's outside. It is
//! built again from the bodies on every recompute, so it follows their
//! edits; the bodies themselves are only read.

#[cfg(test)]
mod tests;
mod wrap;
#[cfg(test)]
mod wrap_tests;

use serde::{Deserialize, Serialize};

use super::joint::OccurrencePath;
use super::mapping::map_params;
use super::mesh_convert::{DEFAULT_ANGLE, faceted_solid};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::exchange::{MeshTolerance, TriangleMesh};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::{BoundingBox, Kernel};
use crate::parameters::ParamId;
use crate::transform::Transform;

pub use wrap::{MAX_CELLS, MAX_TRIANGLES, Wrapped, check_detail, wrap};

/// The detail size when the definition gives none: this share of the
/// source's diagonal, ...
pub const DEFAULT_DETAIL_SHARE: f64 = 0.01;

/// ... and at least this (mm).
pub const MIN_DEFAULT_DETAIL: f64 = 5.0;

/// Nesting of occurrences the source follows at most.
const MAX_DEPTH: usize = 64;

/// A new body wrapping the bodies of the feature's component, or of the
/// component at the end of `occurrence` (a path from the feature's
/// component), with the bodies of the occurrences placed in it; or only
/// `bodies` of that component. `detail` (mm): the size of the gaps and
/// holes closed over (default: 1 % of the source's diagonal, at least
/// 5 mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct EnvelopeDef<P = ParamId> {
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<P>,
}

impl<P> EnvelopeDef<P> {
    pub const TYPE: &'static str = "envelope";
    pub const BASE_NAME: &'static str = "Envelope";

    map_params! {
        EnvelopeDef {
            occurrence: clone,
            bodies: clone,
            detail: option,
        }
    }
}

/// The detail size of a source within `bounds` when none is given.
pub fn default_detail(bounds: &BoundingBox) -> f64 {
    let diagonal = mitcad_vec3::distance(bounds.max, bounds.min);
    (diagonal * DEFAULT_DETAIL_SHARE).max(MIN_DEFAULT_DETAIL)
}

/// How finely the source is tessellated for a detail size: within a
/// quarter of it (the wrap's cells are half of it).
pub fn mesh_tolerance(detail: f64) -> MeshTolerance {
    MeshTolerance {
        deviation: (detail / 4.0).max(0.001),
        angle: 0.5,
        face_triangles: 0,
        max_edge: 0.0,
    }
}

impl FeatureInfo for EnvelopeDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        for (i, body) in self.bodies.iter().enumerate() {
            if self.bodies[..i].contains(body) {
                return Err(format!("body {body} is listed more than once"));
            }
            ctx.body(*body)?;
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.detail
            .map_or(Ok(()), |detail| check_detail(value(detail)))
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

/// The source's shapes, each with the transform from its component's
/// coordinates into the feature's.
pub type Sources<S> = Vec<(S, Transform)>;

impl EnvelopeDef {
    /// The shapes the envelope wraps (see [`EnvelopeDef`]).
    fn sources<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Sources<K::Shape>, String> {
        let own = ctx.component();
        let (component, placement) = if self.occurrence.is_empty() {
            (own, Transform::IDENTITY)
        } else {
            let assembly = ctx.env.assembly;
            let at = crate::joints::path_component(assembly, own, &self.occurrence.0)?;
            let mut placement = Transform::IDENTITY;
            for uid in &self.occurrence.0 {
                if assembly
                    .occurrence(*uid)
                    .is_some_and(|o| o.pattern.is_some())
                    && !ctx.env.placements.contains_key(uid)
                {
                    return Err(format!(
                        "pattern occurrence {uid} is unavailable at this timeline position"
                    ));
                }
                placement = placement.after(&ctx.placement(*uid));
            }
            (at, placement)
        };
        let mut sources = Vec::new();
        if self.bodies.is_empty() {
            gather(ctx, component, placement, &mut sources, 0)?;
        } else {
            for body in &self.bodies {
                sources.push((ctx.body_in(component, *body)?, placement));
            }
        }
        Ok(sources)
    }
}

/// Every body of `component` and of the occurrences placed in it, at
/// this point of the timeline, moved by `placement`.
fn gather<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    component: ComponentUid,
    placement: Transform,
    sources: &mut Sources<K::Shape>,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "the occurrences are nested more than {MAX_DEPTH} deep"
        ));
    }
    for (_, shape) in ctx.bodies_in(component) {
        sources.push((shape, placement));
    }
    let assembly = ctx.env.assembly;
    let children: Vec<(OccurrenceUid, ComponentUid, bool)> = assembly
        .children(component)
        .map(|o| (o.uid, o.component, o.pattern.is_some()))
        .collect();
    for (uid, child, patterned) in children {
        // A pattern's occurrence placed only by a later feature is not
        // there yet.
        if patterned && !ctx.env.placements.contains_key(&uid) {
            continue;
        }
        let at = placement.after(&ctx.placement(uid));
        gather(ctx, child, at, sources, depth + 1)?;
    }
    Ok(())
}

/// The box around the sources as placed.
fn placed_bounds<K: Kernel>(
    kernel: &K,
    sources: &Sources<K::Shape>,
) -> Result<Option<BoundingBox>, String> {
    let mut bounds: Option<BoundingBox> = None;
    for (shape, placement) in sources {
        let Some(b) = kernel.bounding_box(shape).map_err(|e| e.to_string())? else {
            continue;
        };
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for corner in b.corners() {
            let p = placement.apply_point(corner);
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let b = BoundingBox { min, max };
        bounds = Some(match bounds {
            Some(a) => a.union(&b),
            None => b,
        });
    }
    Ok(bounds)
}

/// An envelope the kernel built ([`build`]), not yet adopted by a feature.
pub struct Built<S> {
    pub shape: S,
    /// The detail size used (the default when none was given).
    pub detail: f64,
    /// Its planar faces.
    pub faces: usize,
    /// The grid it was wrapped on: cells along x, y and z, and their size.
    pub cells: [usize; 3],
    pub cell: f64,
}

impl<S> Built<S> {
    /// What the feature's details tell of it.
    pub fn info(&self) -> String {
        format!(
            "detail size {} mm: {} faces wrapped on {} × {} × {} cells of {} mm",
            round(self.detail),
            self.faces,
            self.cells[0],
            self.cells[1],
            self.cells[2],
            round(self.cell)
        )
    }
}

/// The envelope of shapes, each moved by its transform, at `detail` (mm;
/// None: [`default_detail`] of their box): tessellated by the kernel,
/// wrapped ([`wrap`]), merged into planar faces ([`faceted_solid`]) and
/// built as a solid ([`Kernel::boundary_body`]). The shapes are only
/// read. Shared by the feature and by linked components shown by their
/// envelopes (`document/envelope.rs`).
pub fn build<K: Kernel>(
    kernel: &K,
    sources: &Sources<K::Shape>,
    detail: Option<f64>,
) -> Result<Built<K::Shape>, String> {
    if let Some(detail) = detail {
        check_detail(detail)?;
    }
    let bounds =
        placed_bounds(kernel, sources)?.ok_or_else(|| "there are no bodies to wrap".to_owned())?;
    let detail = detail.unwrap_or_else(|| default_detail(&bounds));
    let tolerance = mesh_tolerance(detail);
    let mut meshes: Vec<TriangleMesh> = Vec::with_capacity(sources.len());
    for (shape, placement) in sources {
        let mut mesh = kernel
            .triangle_mesh(shape, &tolerance)
            .map_err(|e| format!("the bodies' triangles: {e}"))?;
        if !placement.is_identity() {
            for v in &mut mesh.vertices {
                *v = placement.apply_point(*v);
            }
        }
        meshes.push(mesh);
    }
    let wrapped = wrap(&meshes, detail)?;
    // The meshes are no longer needed: the source's triangles go before
    // the solid is built.
    drop(meshes);
    let faceted = faceted_solid(&wrapped.mesh, wrapped.cell * 1e-3, DEFAULT_ANGLE)
        .map_err(|e| format!("the envelope's faces: {e}"))?;
    let faces = faceted.boundary.faces.len();
    let (shape, report) = kernel
        .boundary_body(&faceted.boundary)
        .map_err(|e| format!("the envelope's solid: {e}"))?;
    if report.left_out > 0 || !report.solid {
        let mut message = format!(
            "the envelope's {faces} faces do not build a closed solid ({} left out)",
            report.left_out
        );
        if let Some(first) = report.messages.first() {
            message.push_str(&format!(": {first}"));
        }
        return Err(message);
    }
    if matches!(kernel.is_valid(&shape), Ok(false)) {
        return Err("the envelope's solid is not valid".to_owned());
    }
    Ok(Built {
        shape,
        detail,
        faces,
        cells: wrapped.cells,
        cell: wrapped.cell,
    })
}

impl<K: Kernel> Evaluate<K> for EnvelopeDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let detail = match self.detail {
            Some(id) => Some(ctx.param(id)?),
            None => None,
        };
        let sources = self.sources(ctx)?;
        let built = build(ctx.kernel, &sources, detail)?;
        let (solid, _) = ctx
            .kernel
            .adopt_shape(ctx.uid, &built.shape, 0)
            .map_err(|e| format!("the envelope's faces: {e}"))?;
        ctx.inform(built.info());
        let body = ctx.new_body();
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(body, solid)],
            ..FeatureOutput::default()
        })
    }
}

/// A size in a message: to a thousandth of a millimetre.
fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
