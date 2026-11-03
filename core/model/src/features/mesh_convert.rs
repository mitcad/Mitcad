// SPDX-License-Identifier: MIT
//! Convert Mesh (mitcad#462): a mesh body of one profile extruded straight
//! (a constant-section part from an STL or OBJ file) rebuilt as a solid of
//! planar and cylindrical faces. The section is found within a tolerance by
//! the conversion adapted from SolidCraft
//! (`third_party/solidcraft/mesh_convert.rs`): the extrusion direction among
//! the triangles' normals, the two caps' boundary loops (which must be the
//! same), every side vertex on them; a loop of corners on one circle with
//! its chords within the tolerance is that circle, other loops polygons. A
//! mesh of any other shape is refused and nothing is converted.
//!
//! The solid is the kernel's extrusion of the section, so its faces are
//! named as an extrusion's: `<feature>:side(<segment>)`, `start(<region>)`
//! at the low cap and `end(<region>)` at the high one, the section's edges
//! numbered as curves `c1`, `c2`, ... loop by loop (an outer loop
//! counter-clockwise from its lowest corner, a hole clockwise, a circle one
//! curve). The body keeps its id, name and appearance; the mesh it was
//! stays as the features before this one left it.
//!
//! The Faceted solid mode (mitcad#1082, `faceted.rs`) takes any closed
//! mesh instead: its vertices welded within the tolerance, its triangles
//! oriented and merged into planar faces within an angle, built by
//! [`Kernel::boundary_body`] as a new body (`<feature>.b0`, faces named
//! `<feature>:import(<n>)`); the mesh body stays, and adding the feature
//! hides it (`document/mesh_convert.rs`).

mod faceted;
#[cfg(test)]
mod faceted_tests;
#[path = "../../../../third_party/solidcraft/mesh_convert.rs"]
mod prism;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::exchange::MeshTolerance;
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{ExtrudeSpec, Kernel};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry};
use crate::topo::{RegionKey, SegmentKey};

pub use faceted::{Faceted, MAX_ANGLE, MAX_TRIANGLES, faceted_solid};
pub use prism::{
    MAX_TOLERANCE, MIN_TOLERANCE, Prism, SectionLoop, SectionRegion, convert_prismatic_mesh,
};

/// How a mesh body is converted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvertMode {
    /// One profile extruded straight, rebuilt in place as planes and
    /// cylinders (mitcad#462); the default of a definition without `mode`.
    #[default]
    Analytic,
    /// Any closed mesh as a new solid of planar faces (mitcad#1082).
    Faceted,
}

fn is_analytic(mode: &ConvertMode) -> bool {
    *mode == ConvertMode::Analytic
}

/// The angle within which Faceted solid merges neighbouring triangles when
/// the definition gives none (0.5 degrees, in radians).
pub const DEFAULT_ANGLE: f64 = 0.5 * std::f64::consts::PI / 180.0;

/// A mesh body (`body`) rebuilt as planes and cylinders within `tolerance`
/// (mm, 0.000001 to 1), or with `mode` `faceted` made a new solid of planar
/// faces: vertices welded within `tolerance`, neighbouring triangles merged
/// within `angle` (radians; 0.5 degrees when left out).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MeshConvertDef<P = ParamId> {
    pub body: BodyUid,
    pub tolerance: P,
    #[serde(default, skip_serializing_if = "is_analytic")]
    pub mode: ConvertMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
}

impl<P> MeshConvertDef<P> {
    pub const TYPE: &'static str = "mesh_convert";
    pub const BASE_NAME: &'static str = "ConvertMesh";

    map_params! {
        MeshConvertDef {
            body: copy,
            tolerance: param,
            mode: copy,
            angle: option,
        }
    }

    /// The mesh body a new feature hides (`document/mesh_convert.rs`):
    /// Faceted solid's, which stays beside the solid made of it.
    pub fn replaced_mesh(&self) -> Option<BodyUid> {
        (self.mode == ConvertMode::Faceted).then_some(self.body)
    }
}

fn check_tolerance(value: f64) -> Result<(), String> {
    if value.is_finite() && (MIN_TOLERANCE..=MAX_TOLERANCE).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "the tolerance must be between 0.000001 and 1 mm, got {value}"
        ))
    }
}

fn check_angle(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 && value <= MAX_ANGLE {
        Ok(())
    } else {
        Err(format!(
            "the angle must be more than 0 and at most 30 degrees, got {}°",
            value.to_degrees()
        ))
    }
}

impl FeatureInfo for MeshConvertDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.angle.is_some() && self.mode == ConvertMode::Analytic {
            return Err(
                "an angle is Faceted solid's (mode faceted); the analytic conversion \
                        has none"
                    .to_owned(),
            );
        }
        ctx.body(self.body)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_tolerance(value(self.tolerance))?;
        self.angle.map_or(Ok(()), |angle| check_angle(value(angle)))
    }

    fn creates_bodies(&self) -> bool {
        self.mode == ConvertMode::Faceted
    }
}

/// A mesh body's triangles come as they are; the meshing tolerance is not
/// used for them.
const AS_STORED: MeshTolerance = MeshTolerance {
    deviation: 0.01,
    angle: 0.1,
    face_triangles: 0,
    max_edge: 0.0,
};

impl<K: Kernel> Evaluate<K> for MeshConvertDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let tolerance = ctx.param(self.tolerance)?;
        check_tolerance(tolerance)?;
        let shape = ctx.body(self.body)?;
        if !ctx.is_mesh(&shape) {
            return Err(format!(
                "body {} (from {}) is not a mesh body; Convert Mesh rebuilds the triangles of a \
                 mesh body (an STL or OBJ file's) as planes and cylinders",
                self.body,
                ctx.feature_name(self.body.feature)
            ));
        }
        let mesh = ctx
            .kernel
            .triangle_mesh(&shape, &AS_STORED)
            .map_err(|e| format!("the mesh's triangles: {e}"))?;
        if self.mode == ConvertMode::Faceted {
            let angle = match self.angle {
                Some(id) => ctx.param(id)?,
                None => DEFAULT_ANGLE,
            };
            check_angle(angle)?;
            return faceted_output(ctx, &mesh, tolerance, angle);
        }
        let prism = convert_prismatic_mesh(&mesh, tolerance)?;
        let regions = profile_regions(&prism);
        let solid = ctx
            .kernel
            .extrude(&ExtrudeSpec {
                feature: ctx.uid,
                frame: prism.frame,
                regions: &regions,
                direction: prism.frame.normal(),
                start: 0.0,
                end: prism.height,
            })
            .map_err(|e| e.to_string())?;
        if matches!(ctx.kernel.is_valid(&solid), Ok(false)) {
            return Err("the rebuilt solid is not valid; no geometry was converted".to_owned());
        }
        // The kernel's solid holds the section's volume (a kernel that does
        // not measure is trusted).
        if let Ok(properties) = ctx.kernel.mass_properties(&solid) {
            let expected = prism.volume();
            if (properties.volume - expected).abs() > 1e-5 * expected.abs() + 1e-9 {
                return Err(format!(
                    "the rebuilt solid's volume {} mm³ is not its section's {expected} mm³; no \
                     geometry was converted",
                    properties.volume
                ));
            }
        }
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, solid)],
            ..FeatureOutput::default()
        })
    }
}

/// Faceted solid: the mesh's planar faces built by the kernel as a new
/// body, its faces named `<feature>:import(<n>)`; refused unless every
/// shell became a solid of every face with the mesh's volume (within the
/// tolerance times the area).
fn faceted_output<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    mesh: &crate::exchange::TriangleMesh,
    tolerance: f64,
    angle: f64,
) -> Result<FeatureOutput<K::Shape>, String> {
    let faceted = faceted_solid(mesh, tolerance, angle)?;
    let faces = faceted.boundary.faces.len();
    let (built, report) = ctx
        .kernel
        .boundary_body(&faceted.boundary)
        .map_err(|e| format!("the faceted solid: {e}"))?;
    if report.left_out > 0 || !report.solid {
        let mut message = format!(
            "the {faces} planar faces of the mesh do not build a closed solid ({} left out); no \
             geometry was converted",
            report.left_out
        );
        if let Some(first) = report.messages.first() {
            message.push_str(&format!(": {first}"));
        }
        return Err(message);
    }
    let (solid, _) = ctx
        .kernel
        .adopt_shape(ctx.uid, &built, 0)
        .map_err(|e| format!("the faceted solid's faces: {e}"))?;
    if matches!(ctx.kernel.is_valid(&solid), Ok(false)) {
        return Err("the faceted solid is not valid; no geometry was converted".to_owned());
    }
    // The kernel's solid holds the mesh's volume (a kernel that does not
    // measure is trusted).
    if let Ok(properties) = ctx.kernel.mass_properties(&solid) {
        let expected = faceted.volume;
        if (properties.volume - expected).abs() > tolerance * faceted.area + 1e-6 * expected {
            return Err(format!(
                "the faceted solid's volume {} mm³ is not the mesh's {expected} mm³; no geometry \
                 was converted",
                properties.volume
            ));
        }
    }
    Ok(FeatureOutput {
        changes: vec![BodyChange::Set(BodyUid::new(ctx.uid, 0), solid)],
        ..FeatureOutput::default()
    })
}

/// The section as the kernel's profile regions, its edges numbered as
/// curves `c1`, `c2`, ... loop by loop.
pub(crate) fn profile_regions(prism: &Prism) -> Vec<ProfileRegion> {
    let mut next = 1u32;
    let mut regions = Vec::with_capacity(prism.regions.len());
    for region in &prism.regions {
        let outer = loop_segments(&region.outer, &mut next);
        let key = RegionKey::new(outer.iter().map(|s| s.key.clone())).expect("a loop has segments");
        let mut loops = vec![ProfileLoop { segments: outer }];
        for hole in &region.holes {
            loops.push(ProfileLoop {
                segments: loop_segments(hole, &mut next),
            });
        }
        regions.push(ProfileRegion { key, loops });
    }
    regions
}

/// A loop's segments: a polygon's edges between their neighbours
/// (`c2[c1,c3]`), a circle one closed curve (`c5`).
fn loop_segments(section: &SectionLoop, next: &mut u32) -> Vec<ProfileSegment> {
    match section {
        SectionLoop::Circle { center, radius } => {
            let curve = EntityUid(*next);
            *next += 1;
            vec![ProfileSegment {
                key: SegmentKey::closed(curve),
                geometry: SegmentGeometry::Circle {
                    center: *center,
                    radius: *radius,
                    start: 0.0,
                },
            }]
        }
        SectionLoop::Polygon(points) => {
            let n = points.len();
            let first = *next;
            *next += n as u32;
            let curve = |i: usize| EntityUid(first + (i % n) as u32);
            (0..n)
                .map(|i| ProfileSegment {
                    key: SegmentKey::between(curve(i), curve(i + n - 1), curve(i + 1)),
                    geometry: SegmentGeometry::Line {
                        start: points[i],
                        end: points[(i + 1) % n],
                    },
                })
                .collect()
        }
    }
}
