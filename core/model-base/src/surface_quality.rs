// SPDX-License-Identifier: MIT
//! Surface quality analyses (mitcad#2517): what the kernel gives of a
//! body's faces for Zebra, Curvature Map, Isocurve Analysis and Environment
//! Map ([`crate::kernel::Kernel::surface_samples`] and
//! [`crate::kernel::Kernel::face_isocurves`]); the model works the analyses
//! out of them (`mitcad_model::surface_quality`).
//!
//! Curvatures are in 1/mm, signed by the body's outside: positive where a
//! face bulges outwards (a ball's outside, a shaft), negative where it is
//! hollow (a bore), zero on a plane. NaN where the surface has none (a
//! cone's apex, the faces of a mesh body).

use crate::datum::Vec3;

/// Faces triangulated, each vertex with the surface's outward normal and
/// its principal curvatures.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SurfaceSamples {
    pub vertices: Vec<Vec3>,
    /// A unit normal per vertex, out of the body.
    pub normals: Vec<Vec3>,
    /// The principal curvatures at each vertex (1/mm), the larger first.
    pub curvatures: Vec<[f64; 2]>,
    /// Counter-clockwise seen from outside, face by face.
    pub triangles: Vec<[u32; 3]>,
    /// Each face's number of triangles, in the order the faces were asked
    /// (the shape's face order when none was named).
    pub face_triangles: Vec<u32>,
}

/// One iso curve of a face, trimmed to the face: a polyline.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IsoCurve {
    /// The face's index in the order the faces were asked.
    pub face: u32,
    /// Along the surface's u parameter (v held); else along v.
    pub along_u: bool,
    /// The held parameter as a fraction of the face's range, 0 to 1.
    pub at: f64,
    pub points: Vec<Vec3>,
    /// The outward unit normal at each point (zero where there is none).
    pub normals: Vec<Vec3>,
    /// The surface's normal curvature along the curve at each point (1/mm).
    pub curvatures: Vec<f64>,
}
