// SPDX-License-Identifier: MIT
//! Surface quality analyses (mitcad#2517), worked out of what the kernel
//! gives of the faces ([`crate::surface_quality::SurfaceSamples`] and
//! [`crate::surface_quality::IsoCurve`]): the analyses kept in the document
//! (`document/surface_analyses.rs`) and the `surface_analysis` query
//! (`mitcad_model_api`, `surface_quality.rs`) use it.
//!
//! - **Zebra** ([`zebra`]): stripes from a direction. A point is in stripe
//!   `k` where the angle between the face's normal and the direction is
//!   between `k` and `k + 1` times 180° / `stripes`, dark in the first
//!   `width` of each. The stripes are the lines of equal light from that
//!   direction (isophotes): they jump where faces meet at an angle (G0
//!   only), meet with a kink where faces are tangent (G1), and run on
//!   smoothly where faces are curvature continuous (G2). Each triangle is
//!   cut along the stripes' borders, so the stripes are sharp on a coarse
//!   mesh too.
//! - **Curvature Map** ([`CurvatureKind`], [`curvature_values`]): a value at
//!   each vertex from its principal curvatures, Gaussian (1/mm²), mean,
//!   largest or smallest (1/mm), signed as the kernel signs them: positive
//!   where the face bulges outwards.
//! - **Isocurve Analysis**: the kernel's iso curves, placed; the normal
//!   curvature along each as the curve's colour on request.
//! - **Environment Map**: the faces' mesh with its normals, placed; the
//!   view reflects an environment picture in it.

use serde::{Deserialize, Serialize};

use crate::datum::Vec3;
use crate::surface_quality::{IsoCurve, SurfaceSamples};
use crate::transform::Transform;

/// What a Curvature Map shows at a point of a face.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurvatureKind {
    /// The product of the principal curvatures (1/mm²): positive on a
    /// ball, zero on a plane or a cylinder, negative on a saddle.
    #[default]
    Gaussian,
    /// Their mean (1/mm).
    Mean,
    /// The larger of the two (1/mm).
    Maximum,
    /// The smaller of the two (1/mm).
    Minimum,
}

impl CurvatureKind {
    /// The value at a point of principal curvatures `[larger, smaller]`;
    /// None where they are not known (NaN).
    pub fn value(self, [k1, k2]: [f64; 2]) -> Option<f64> {
        if !(k1.is_finite() && k2.is_finite()) {
            return None;
        }
        let value = match self {
            Self::Gaussian => k1 * k2,
            Self::Mean => 0.5 * (k1 + k2),
            Self::Maximum => k1.max(k2),
            Self::Minimum => k1.min(k2),
        };
        // No -0 in the answers.
        Some(if value == 0.0 { 0.0 } else { value })
    }

    /// The unit of the values: `1/mm^2` for the Gaussian curvature, `1/mm`
    /// for the others.
    pub fn unit(self) -> &'static str {
        match self {
            Self::Gaussian => "1/mm^2",
            _ => "1/mm",
        }
    }

    /// The kind as the definitions write it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gaussian => "gaussian",
            Self::Mean => "mean",
            Self::Maximum => "maximum",
            Self::Minimum => "minimum",
        }
    }
}

/// The samples where a placement puts them: points moved, normals turned
/// (and kept unit), triangles turned round by a mirroring placement.
pub fn placed(samples: &SurfaceSamples, placement: &Transform) -> SurfaceSamples {
    let mirrored = placement.determinant() < 0.0;
    SurfaceSamples {
        vertices: samples
            .vertices
            .iter()
            .map(|v| placement.apply_point(*v))
            .collect(),
        normals: samples
            .normals
            .iter()
            .map(|n| mitcad_vec3::unit(placement.apply_vector(*n)).unwrap_or(*n))
            .collect(),
        curvatures: samples.curvatures.clone(),
        triangles: samples
            .triangles
            .iter()
            .map(|&[a, b, c]| if mirrored { [a, c, b] } else { [a, b, c] })
            .collect(),
        face_triangles: samples.face_triangles.clone(),
    }
}

/// An iso curve where a placement puts it.
pub fn placed_curve(curve: &IsoCurve, placement: &Transform) -> IsoCurve {
    IsoCurve {
        points: curve
            .points
            .iter()
            .map(|p| placement.apply_point(*p))
            .collect(),
        normals: curve
            .normals
            .iter()
            .map(|n| mitcad_vec3::unit(placement.apply_vector(*n)).unwrap_or(*n))
            .collect(),
        ..curve.clone()
    }
}

/// A Zebra's stripes over the faces: the pieces of the triangles in the
/// dark stripes and in the light ones (9 numbers each, three corners), and
/// the area of each.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZebraStripes {
    pub dark: Vec<[f64; 9]>,
    pub light: Vec<[f64; 9]>,
    pub dark_area: f64,
    pub light_area: f64,
}

/// The stripe coordinate of a normal: the angle between it and `direction`
/// (both unit) in stripes, 0 along the direction to `stripes` against it.
/// Within 1e-9 of a stripe's start it is that start, so that a plane
/// facing the direction, against it or (with an even number of stripes)
/// square to it is in one stripe whatever its normal's last bits.
pub fn stripe_coordinate(normal: Vec3, direction: Vec3, stripes: u32) -> f64 {
    let cosine = mitcad_vec3::dot(normal, direction).clamp(-1.0, 1.0);
    let coordinate = cosine.acos() / std::f64::consts::PI * f64::from(stripes);
    let start = coordinate.round();
    if (coordinate - start).abs() < 1e-9 {
        start
    } else {
        coordinate
    }
}

/// Whether a stripe coordinate lies in a dark stripe: the first `width` of
/// each stripe.
pub fn is_dark(coordinate: f64, width: f64) -> bool {
    coordinate - coordinate.floor() < width
}

/// The Zebra stripes of the samples (see the module): `direction` a unit
/// vector, `stripes` how many over the half turn from it to its reverse,
/// `width` the dark share of each (0 to 1).
pub fn zebra(samples: &SurfaceSamples, direction: Vec3, stripes: u32, width: f64) -> ZebraStripes {
    let direction = mitcad_vec3::unit(direction).unwrap_or([0.0, 0.0, 1.0]);
    let coordinate: Vec<f64> = samples
        .normals
        .iter()
        .map(|n| stripe_coordinate(*n, direction, stripes))
        .collect();
    let mut out = ZebraStripes::default();
    let mut keep = |triangle: [Vec3; 3], dark: bool| {
        let area = triangle_area(&triangle);
        if area <= 0.0 {
            return;
        }
        if dark {
            out.dark.push(flat(&triangle));
            out.dark_area += area;
        } else {
            out.light.push(flat(&triangle));
            out.light_area += area;
        }
    };
    for &[a, b, c] in &samples.triangles {
        let corners = [a, b, c].map(|i| i as usize);
        if corners.iter().any(|&i| {
            i >= samples.vertices.len() || i >= coordinate.len() || !coordinate[i].is_finite()
        }) {
            continue;
        }
        let polygon: Vec<(Vec3, f64)> = corners
            .iter()
            .map(|&i| (samples.vertices[i], coordinate[i]))
            .collect();
        let low = polygon.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let high = polygon
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        if low == high {
            // One coordinate over the whole triangle (a plane): whole in
            // its stripe.
            keep(corners.map(|i| samples.vertices[i]), is_dark(low, width));
            continue;
        }
        // Each stripe the triangle reaches, its dark band then its light
        // one.
        let mut k = low.floor();
        while k < high {
            for (from, to, dark) in [(k, k + width, true), (k + width, k + 1.0, false)] {
                if to <= low || from >= high {
                    continue;
                }
                let above = clip(&polygon, |s| s - from);
                let piece = clip(&above, |s| to - s);
                for triangle in fan(&piece) {
                    keep(triangle, dark);
                }
            }
            k += 1.0;
        }
    }
    out
}

/// Sutherland-Hodgman: the part of a convex polygon where `side` of the
/// coordinate is not negative, the crossings interpolated linearly.
fn clip(polygon: &[(Vec3, f64)], side: impl Fn(f64) -> f64) -> Vec<(Vec3, f64)> {
    let mut out = Vec::with_capacity(polygon.len() + 2);
    for i in 0..polygon.len() {
        let current = polygon[i];
        let next = polygon[(i + 1) % polygon.len()];
        let (a, b) = (side(current.1), side(next.1));
        if a >= 0.0 {
            out.push(current);
        }
        if (a >= 0.0) != (b >= 0.0) {
            let t = a / (a - b);
            let point = std::array::from_fn(|k| current.0[k] + t * (next.0[k] - current.0[k]));
            out.push((point, current.1 + t * (next.1 - current.1)));
        }
    }
    out
}

/// A convex polygon as a fan of triangles from its first corner.
fn fan(polygon: &[(Vec3, f64)]) -> Vec<[Vec3; 3]> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    (1..polygon.len() - 1)
        .map(|i| [polygon[0].0, polygon[i].0, polygon[i + 1].0])
        .collect()
}

fn triangle_area([a, b, c]: &[Vec3; 3]) -> f64 {
    0.5 * mitcad_vec3::norm(mitcad_vec3::cross(
        mitcad_vec3::sub(*b, *a),
        mitcad_vec3::sub(*c, *a),
    ))
}

fn flat([a, b, c]: &[Vec3; 3]) -> [f64; 9] {
    [a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]
}

/// A Curvature Map's values: one per vertex (None where the surface has
/// no curvature and no neighbour of the vertex has one), and the smallest
/// and the largest with where they are.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CurvatureValues {
    pub values: Vec<Option<f64>>,
    pub min: Option<(f64, Vec3)>,
    pub max: Option<(f64, Vec3)>,
}

/// The values of `kind` at the samples' vertices. A vertex without
/// curvature (a cone's apex) takes the mean of the vertices it shares a
/// triangle with that have one.
pub fn curvature_values(samples: &SurfaceSamples, kind: CurvatureKind) -> CurvatureValues {
    let own: Vec<Option<f64>> = samples.curvatures.iter().map(|k| kind.value(*k)).collect();
    let mut sums = vec![(0.0, 0usize); own.len()];
    for triangle in &samples.triangles {
        for &i in triangle {
            let i = i as usize;
            if i >= own.len() || own[i].is_some() {
                continue;
            }
            for &j in triangle {
                if let Some(value) = own.get(j as usize).copied().flatten() {
                    sums[i].0 += value;
                    sums[i].1 += 1;
                }
            }
        }
    }
    let values: Vec<Option<f64>> = own
        .iter()
        .zip(&sums)
        .map(|(value, &(sum, count))| value.or((count > 0).then(|| sum / count as f64)))
        .collect();
    let mut out = CurvatureValues {
        values,
        min: None,
        max: None,
    };
    for (i, value) in out.values.iter().enumerate() {
        let (Some(value), Some(point)) = (*value, samples.vertices.get(i)) else {
            continue;
        };
        if out.min.is_none_or(|(m, _)| value < m) {
            out.min = Some((value, *point));
        }
        if out.max.is_none_or(|(m, _)| value > m) {
            out.max = Some((value, *point));
        }
    }
    out
}

/// The colour scale's reach for values from `min` to `max` without a limit
/// given: the larger magnitude, so that zero stays in the middle; a tiny
/// positive number when every value is zero (a plane).
pub fn automatic_limit(min: Option<f64>, max: Option<f64>) -> f64 {
    let reach = min.unwrap_or(0.0).abs().max(max.unwrap_or(0.0).abs());
    if reach > 0.0 && reach.is_finite() {
        reach
    } else {
        1e-9
    }
}
