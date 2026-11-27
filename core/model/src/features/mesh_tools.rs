// SPDX-License-Identifier: MIT
//! Mesh edits for reverse engineering (mitcad#1871; the algorithms in
//! `crate::mesh_tools`): features that make new bodies of a mesh body and
//! leave it as it was, as Convert Mesh's Faceted solid does. Adding one
//! hides the mesh body in the same undo step (`document/mesh_convert.rs`).
//!
//! - `mesh_decimate`: fewer triangles, to a count or within a tolerance,
//!   keeping the outline and the sharp edges; a new mesh body.
//! - `mesh_repair`: welded, made manifold, oriented, holes filled up to a
//!   size; a new mesh body, and what was fixed said in words.
//! - `mesh_remove`: triangles picked by lassos, a brush or their numbers
//!   removed, the hole left open or filled; a new mesh body.
//! - `mesh_segment`: the mesh split into regions by curvature (planes,
//!   cylinders, free-form): one new mesh body per region, largest first.
//! - `mesh_fit_surface`: a plane, cylinder, cone, sphere or B-spline
//!   surface fitted to the mesh (a region, typically a segment's body), as
//!   a new surface body built by [`Kernel::boundary_body`].
//!
//! The new mesh bodies' one face each is named `<feature>:import(<n>)` (the
//! region's number for a segmentation), the surface body's faces likewise.

#[cfg(test)]
#[path = "mesh_tools_tests.rs"]
mod tests;

use serde::{Deserialize, Serialize};

use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::exchange::{MeshTolerance, TriangleMesh};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::mesh_tools::decimate::{DecimateOptions, decimate};
use crate::mesh_tools::fit::{SurfaceKind, boundary, fit_auto, fit_surface};
use crate::mesh_tools::region::{Brush, Lasso, Pick, remove};
use crate::mesh_tools::repair::{RepairOptions, repair};
use crate::mesh_tools::segment::{RegionKind, segment};
use crate::mesh_tools::submesh;
use crate::parameters::ParamId;

/// The mesh bodies a mesh edit makes its new bodies of, which adding it
/// hides (`document/mesh_convert.rs`); none for other features.
pub fn edited_meshes<P>(def: &super::FeatureDef<P>) -> Vec<BodyUid> {
    use super::FeatureDef as F;
    match def {
        F::MeshDecimate(d) => vec![d.body],
        F::MeshRepair(d) => vec![d.body],
        F::MeshRemove(d) => vec![d.body],
        F::MeshSegment(d) => vec![d.body],
        F::MeshFitSurface(d) => vec![d.body],
        // Smooth, Combine, Offset and Shell, Align, Plane Cut (mitcad#2515).
        other => super::mesh_shape_tools::edited_meshes(other),
    }
}

/// A mesh body's triangles as they are stored.
pub(super) const AS_STORED: MeshTolerance = MeshTolerance {
    deviation: 0.01,
    angle: 0.1,
    face_triangles: 0,
    max_edge: 0.0,
};

/// The mesh body's triangles as stored; refused, in words, for a body of
/// another kind.
pub(super) fn stored_mesh_of<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    tool: &str,
) -> Result<TriangleMesh, String> {
    let shape = ctx.body(body)?;
    if !ctx.is_mesh(&shape) {
        let from = ctx.feature_name(body.feature);
        return Err(crate::notice::said(
            format!(
                "{tool} works on mesh bodies (triangles from an STL, OBJ or 3MF file); the body \
                 picked is not one"
            ),
            format!("body {body} (from {from}) is not a mesh body; {tool} works on mesh bodies"),
        ));
    }
    let mesh = ctx
        .kernel
        .triangle_mesh(&shape, &AS_STORED)
        .map_err(|e| format!("the mesh's triangles: {e}"))?;
    if mesh.triangles.is_empty() {
        return Err("the mesh body has no triangles".to_owned());
    }
    Ok(mesh)
}

/// The mesh body's triangles with their shared corners joined
/// ([`crate::mesh_tools::joined`]).
pub(super) fn mesh_of<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    tool: &str,
) -> Result<TriangleMesh, String> {
    let mesh = crate::mesh_tools::joined(&stored_mesh_of(ctx, body, tool)?);
    if mesh.triangles.is_empty() {
        return Err("the mesh body has no triangles with area".to_owned());
    }
    Ok(mesh)
}

/// A new mesh body of the triangles, its face `<feature>:import(<face>)`.
pub(super) fn mesh_body<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    mesh: &TriangleMesh,
    face: u32,
) -> Result<K::Shape, String> {
    let made = ctx
        .kernel
        .triangle_body(&mesh.vertices, &mesh.triangles)
        .map_err(|e| format!("the new mesh body: {e}"))?;
    let (adopted, _) = ctx
        .kernel
        .adopt_shape(ctx.uid, &made, face)
        .map_err(|e| format!("the new mesh body: {e}"))?;
    Ok(adopted)
}

/// One new body.
pub(super) fn one_body<S>(ctx_uid: BodyUid, shape: S) -> FeatureOutput<S> {
    FeatureOutput {
        changes: vec![BodyChange::Set(ctx_uid, shape)],
        ..FeatureOutput::default()
    }
}

pub(super) fn check_range(
    what: &str,
    value: f64,
    min: f64,
    max: f64,
    unit: &str,
) -> Result<(), String> {
    if value.is_finite() && value > min && value <= max {
        Ok(())
    } else {
        Err(format!(
            "{what} must be more than {min}{unit} and at most {max}{unit}, got {value}{unit}"
        ))
    }
}

pub(super) fn check_angle(what: &str, value: f64, max_degrees: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 && value <= max_degrees.to_radians() + 1e-12 {
        Ok(())
    } else {
        Err(format!(
            "{what} must be more than 0 and at most {max_degrees} degrees, got {}°",
            value.to_degrees()
        ))
    }
}

pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

// Decimate.

/// The angle above which Decimate keeps an edge as sharp when the
/// definition gives none (30 degrees, in radians).
pub const DEFAULT_SHARP_ANGLE: f64 = std::f64::consts::PI / 6.0;

/// A mesh body decimated to `triangles` or within `tolerance` (mm),
/// whichever comes first; edges sharper than `sharp_angle` (radians, 30
/// degrees when left out) and the outline kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshDecimateDef<P = ParamId> {
    pub body: BodyUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triangles: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sharp_angle: Option<P>,
}

impl<P> MeshDecimateDef<P> {
    pub const TYPE: &'static str = "mesh_decimate";
    pub const BASE_NAME: &'static str = "Decimate";

    map_params! {
        MeshDecimateDef {
            body: copy,
            triangles: option,
            tolerance: option,
            sharp_angle: option,
        }
    }
}

fn check_triangles(value: f64) -> Result<(), String> {
    if value.is_finite() && value >= 4.0 && value.fract() == 0.0 && value <= 1e9 {
        Ok(())
    } else {
        Err(format!(
            "the triangle count must be a whole number of at least 4, got {value}"
        ))
    }
}

impl FeatureInfo for MeshDecimateDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.triangles.is_none() && self.tolerance.is_none() {
            return Err("Decimate needs a triangle count, a tolerance or both".to_owned());
        }
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(id) = self.triangles {
            check_triangles(value(id))?;
        }
        if let Some(id) = self.tolerance {
            check_range("the tolerance", value(id), 0.0, 100.0, " mm")?;
        }
        if let Some(id) = self.sharp_angle {
            check_angle("the sharp edge angle", value(id), 180.0)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshDecimateDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let triangles = self.triangles.map(|id| ctx.param(id)).transpose()?;
        let tolerance = self.tolerance.map(|id| ctx.param(id)).transpose()?;
        let sharp_angle = match self.sharp_angle {
            Some(id) => ctx.param(id)?,
            None => DEFAULT_SHARP_ANGLE,
        };
        if let Some(t) = triangles {
            check_triangles(t)?;
        }
        if let Some(t) = tolerance {
            check_range("the tolerance", t, 0.0, 100.0, " mm")?;
        }
        check_angle("the sharp edge angle", sharp_angle, 180.0)?;
        let mesh = mesh_of(ctx, self.body, "Decimate")?;
        let out = decimate(
            &mesh,
            &DecimateOptions {
                triangles: triangles.map(|t| t as usize),
                tolerance,
                sharp_angle,
            },
        )?;
        ctx.inform(format!(
            "Decimated from {} to {}; the surface moved up to {} mm",
            plural(mesh.triangles.len(), "triangle", "triangles"),
            plural(out.mesh.triangles.len(), "triangle", "triangles"),
            (out.error * 1e4).round() / 1e4
        ));
        let shape = mesh_body(ctx, &out.mesh, 0)?;
        let uid = ctx.new_body();
        Ok(one_body(uid, shape))
    }
}

// Repair.

/// A mesh body repaired: vertices within `tolerance` (mm) welded,
/// degenerate, duplicate and non-manifold triangles taken out, oriented,
/// holes up to `max_hole` across (mm) filled (none when left out).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshRepairDef<P = ParamId> {
    pub body: BodyUid,
    pub tolerance: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hole: Option<P>,
}

impl<P> MeshRepairDef<P> {
    pub const TYPE: &'static str = "mesh_repair";
    pub const BASE_NAME: &'static str = "Repair";

    map_params! {
        MeshRepairDef {
            body: copy,
            tolerance: param,
            max_hole: option,
        }
    }
}

fn check_weld(value: f64) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "the weld tolerance must be between 0 and 1 mm, got {value} mm"
        ))
    }
}

impl FeatureInfo for MeshRepairDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_weld(value(self.tolerance))?;
        if let Some(id) = self.max_hole {
            check_range("the largest hole to fill", value(id), 0.0, 1e6, " mm")?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshRepairDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let tolerance = ctx.param(self.tolerance)?;
        check_weld(tolerance)?;
        let max_hole = self.max_hole.map(|id| ctx.param(id)).transpose()?;
        if let Some(h) = max_hole {
            check_range("the largest hole to fill", h, 0.0, 1e6, " mm")?;
        }
        let mesh = stored_mesh_of(ctx, self.body, "Repair")?;
        let (fixed, report) = repair(
            &mesh,
            &RepairOptions {
                tolerance,
                max_hole,
            },
        )?;
        if report.unorientable > 0 {
            ctx.warn(format!(
                "{} of the mesh could not be oriented (a surface with one side only); left as \
                 it was",
                plural(report.unorientable, "piece", "pieces")
            ));
        }
        ctx.inform(report.describe());
        let shape = mesh_body(ctx, &fixed, 0)?;
        let uid = ctx.new_body();
        Ok(one_body(uid, shape))
    }
}

// Remove Region.

/// A lasso of `mesh_remove`: a polygon drawn on a plane square to the view
/// (`points`, in the component's coordinates), `normal` towards the viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshLasso {
    pub normal: [f64; 3],
    pub points: Vec<[f64; 3]>,
}

/// Triangles of a mesh body removed: those whose centres lie inside a
/// lasso (seen along its normal, facing the viewer unless `through`),
/// within `radius` (mm) of a brush point, or numbered in `triangles`; the
/// holes left open, or filled when `fill`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshRemoveDef<P = ParamId> {
    pub body: BodyUid,
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub fill: bool,
}

impl<P> MeshRemoveDef<P> {
    pub const TYPE: &'static str = "mesh_remove";
    pub const BASE_NAME: &'static str = "RemoveRegion";

    map_params! {
        MeshRemoveDef {
            body: copy,
            lassos: clone,
            brush: clone,
            radius: option,
            triangles: clone,
            through: copy,
            fill: copy,
        }
    }
}

impl FeatureInfo for MeshRemoveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.lassos.is_empty() && self.brush.is_empty() && self.triangles.is_empty() {
            return Err(
                "Remove Region needs a lasso, a brush stroke or triangles to remove".to_owned(),
            );
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
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(id) = self.radius {
            check_range("the brush radius", value(id), 0.0, 1e6, " mm")?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshRemoveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let radius = self.radius.map(|id| ctx.param(id)).transpose()?;
        if let Some(r) = radius {
            check_range("the brush radius", r, 0.0, 1e6, " mm")?;
        }
        let mesh = mesh_of(ctx, self.body, "Remove Region")?;
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
        let (out, removed) = remove(&mesh, &pick, self.fill)?;
        let mut text = format!(
            "Removed {}",
            plural(removed.triangles, "triangle", "triangles")
        );
        if self.fill {
            text.push_str(&format!(
                "; filled {}",
                plural(removed.holes_filled, "hole", "holes")
            ));
        }
        ctx.inform(text);
        let shape = mesh_body(ctx, &out, 0)?;
        let uid = ctx.new_body();
        Ok(one_body(uid, shape))
    }
}

// Segment.

/// A mesh body split into regions by curvature: neighbouring triangles
/// whose normals differ by more than `angle` (radians) part regions; a
/// region within `tolerance` (mm) of a plane or a cylinder is one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshSegmentDef<P = ParamId> {
    pub body: BodyUid,
    pub angle: P,
    pub tolerance: P,
}

impl<P> MeshSegmentDef<P> {
    pub const TYPE: &'static str = "mesh_segment";
    pub const BASE_NAME: &'static str = "Segment";

    map_params! {
        MeshSegmentDef {
            body: copy,
            angle: param,
            tolerance: param,
        }
    }
}

/// The most regions a segmentation makes bodies of.
pub const MAX_REGIONS: usize = 1000;

impl FeatureInfo for MeshSegmentDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_angle("the angle", value(self.angle), 90.0)?;
        check_range("the tolerance", value(self.tolerance), 0.0, 100.0, " mm")
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshSegmentDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let angle = ctx.param(self.angle)?;
        let tolerance = ctx.param(self.tolerance)?;
        check_angle("the angle", angle, 90.0)?;
        check_range("the tolerance", tolerance, 0.0, 100.0, " mm")?;
        let mesh = mesh_of(ctx, self.body, "Segment")?;
        let regions = segment(&mesh, angle, tolerance)?;
        if regions.len() > MAX_REGIONS {
            return Err(format!(
                "the mesh splits into {} regions; Segment makes at most {MAX_REGIONS} (a larger \
                 angle or tolerance makes fewer)",
                regions.len()
            ));
        }
        let count = |kind: RegionKind| regions.iter().filter(|r| r.kind == kind).count();
        ctx.inform(format!(
            "Found {}: {}, {} and {}",
            plural(regions.len(), "region", "regions"),
            plural(count(RegionKind::Plane), "plane", "planes"),
            plural(count(RegionKind::Cylinder), "cylinder", "cylinders"),
            plural(
                count(RegionKind::FreeForm),
                "free-form region",
                "free-form regions"
            ),
        ));
        let mut output = FeatureOutput::default();
        for (k, region) in regions.iter().enumerate() {
            let mut keep = vec![false; mesh.triangles.len()];
            for &t in &region.triangles {
                keep[t] = true;
            }
            let part = submesh(&mesh, |t| keep[t]);
            let shape = mesh_body(ctx, &part, k as u32)?;
            let uid = ctx.new_body();
            output.changes.push(BodyChange::Set(uid, shape));
        }
        Ok(output)
    }
}

// Fit Surface.

/// The surface `mesh_fit_surface` fits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitKind {
    /// The simplest that lies within the tolerance: a plane, a cylinder, a
    /// sphere, a cone, then a B-spline surface.
    #[default]
    Auto,
    Plane,
    Cylinder,
    Cone,
    Sphere,
    #[serde(rename = "bspline")]
    BSpline,
}

fn is_auto(kind: &FitKind) -> bool {
    *kind == FitKind::Auto
}

/// A surface fitted to a mesh body (a region of a mesh, as Segment makes
/// them) as a new surface body; `tolerance` (mm) is how far Auto lets it
/// lie from the triangles' corners, and above which a chosen kind warns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshFitSurfaceDef<P = ParamId> {
    pub body: BodyUid,
    #[serde(default, skip_serializing_if = "is_auto")]
    pub surface: FitKind,
    pub tolerance: P,
}

impl<P> MeshFitSurfaceDef<P> {
    pub const TYPE: &'static str = "mesh_fit_surface";
    pub const BASE_NAME: &'static str = "FitSurface";

    map_params! {
        MeshFitSurfaceDef {
            body: copy,
            surface: copy,
            tolerance: param,
        }
    }
}

impl FeatureInfo for MeshFitSurfaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_range("the tolerance", value(self.tolerance), 0.0, 100.0, " mm")
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for MeshFitSurfaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let tolerance = ctx.param(self.tolerance)?;
        check_range("the tolerance", tolerance, 0.0, 100.0, " mm")?;
        let mesh = mesh_of(ctx, self.body, "Fit Surface")?;
        let kind = match self.surface {
            FitKind::Auto => None,
            FitKind::Plane => Some(SurfaceKind::Plane),
            FitKind::Cylinder => Some(SurfaceKind::Cylinder),
            FitKind::Cone => Some(SurfaceKind::Cone),
            FitKind::Sphere => Some(SurfaceKind::Sphere),
            FitKind::BSpline => Some(SurfaceKind::BSpline),
        };
        let fitted = match kind {
            None => fit_auto(&mesh, tolerance)?,
            Some(kind) => fit_surface(&mesh, kind)?,
        };
        let deviation = (fitted.deviation * 1e4).round() / 1e4;
        if fitted.deviation > tolerance {
            ctx.warn(format!(
                "the fitted {} lies up to {deviation} mm from the mesh, more than the tolerance \
                 {tolerance} mm",
                fitted.kind.name()
            ));
        }
        ctx.inform(format!(
            "Fitted {}; the mesh lies within {deviation} mm of it",
            fitted.describe()
        ));
        let body = boundary(&fitted)?;
        let (built, report) = ctx
            .kernel
            .boundary_body(&body)
            .map_err(|e| format!("the fitted surface: {e}"))?;
        if report.left_out > 0 {
            let mut message = format!(
                "the fitted {}'s face could not be built; no surface was made",
                fitted.kind.name()
            );
            if let Some(first) = report.messages.first() {
                message.push_str(&format!(": {first}"));
            }
            return Err(message);
        }
        let (surface, _) = ctx
            .kernel
            .adopt_shape(ctx.uid, &built, 0)
            .map_err(|e| format!("the fitted surface: {e}"))?;
        let uid = ctx.new_body();
        Ok(one_body(uid, surface))
    }
}
