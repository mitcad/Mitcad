// SPDX-License-Identifier: MIT
//! Analysis results the kernel computes (physical properties, measurement,
//! interference, sections, comparison with a STEP file) and the materials
//! that give bodies their density.
//!
//! Units of the properties dialog, with Mitcad's millimetres:
//! lengths mm, areas mm², volumes mm³, densities g/cm³, masses kg, moments
//! of inertia kg mm², angles radians.

use crate::datum::Vec3;
use crate::topo::TopoName;

/// Exact symmetric Hausdorff distance from nearest-face distances at all
/// vertices of two bounded planar polygons (packed xyz, in boundary order).
/// The kernel first checks planarity, straight edges and a single closed
/// wire. Distance to a convex set is convex, so its maximum on a polygon
/// occurs at a vertex. Reject non-convex faces instead of sampling a maximum.
pub fn maximum_polygon_face_gap(a: &[f64], b: &[f64], distances: &[f64]) -> Option<f64> {
    if !convex_polygon(a) || !convex_polygon(b) || distances.len() != (a.len() + b.len()) / 3 {
        return None;
    }
    if distances.iter().any(|d| !d.is_finite() || *d < 0.0) {
        return None;
    }
    distances.iter().copied().reduce(f64::max)
}

fn convex_polygon(coordinates: &[f64]) -> bool {
    use mitcad_vec3::{cross, dot, sub, unit_above};
    let (vertices, remainder) = coordinates.as_chunks::<3>();
    if !remainder.is_empty() || vertices.len() < 3 || coordinates.iter().any(|p| !p.is_finite()) {
        return false;
    }
    let origin = vertices[0];
    let Some(normal) = (1..vertices.len() - 1).find_map(|i| {
        unit_above(
            cross(sub(vertices[i], origin), sub(vertices[i + 1], origin)),
            1e-12,
        )
    }) else {
        return false;
    };
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let Some(direction) = unit_above(sub(b, a), 1e-12) else {
            return false;
        };
        if vertices
            .iter()
            .any(|p| dot(cross(direction, sub(*p, a)), normal) < -1e-6)
        {
            return false;
        }
    }
    true
}

/// Physical properties of a body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalProperties {
    /// Of the closed solids.
    pub volume: f64,
    pub area: f64,
    pub mass: f64,
    /// Of the volume; of the surface for a body without volume.
    pub center_of_mass: Vec3,
    /// The inertia tensor about the centre of mass in model-parallel axes:
    /// Ixx = ∫(y² + z²) dm on the diagonal, Ixy = -∫xy dm off it.
    pub inertia: [[f64; 3]; 3],
    /// Ascending, with their unit axes.
    pub principal_moments: [f64; 3],
    pub principal_axes: [Vec3; 3],
}

/// What to measure: a body, or a face, edge or vertex of it by name (all
/// pieces of a name without `#k`).
#[derive(Debug, Clone, Copy)]
pub struct Selection<'a, S> {
    pub shape: &'a S,
    pub name: Option<&'a TopoName>,
}

/// A circular edge or arc, or a cylindrical or spherical face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircleMeasure {
    pub center: Vec3,
    /// The circle's normal or the cylinder's axis; zero for a sphere.
    pub axis: Vec3,
    pub radius: f64,
    /// The arc's angle, 2π for a full circle, 0 for faces.
    pub sweep: f64,
}

/// Measurements of one selection; the fields that apply are set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Measurement {
    /// `body`, `face`, `edge` or `vertex`.
    pub kind: String,
    pub volume: Option<f64>,
    pub area: Option<f64>,
    pub length: Option<f64>,
    /// The coordinates of a vertex.
    pub point: Option<Vec3>,
    pub circle: Option<CircleMeasure>,
}

/// Between two selections.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Separation {
    /// The minimum distance, 0 when they touch or one is inside the other.
    pub distance: f64,
    pub on_a: Vec3,
    pub on_b: Vec3,
    /// One lies (partly) inside a solid of the other.
    pub inside: bool,
    /// Between two planar faces (their normals, 0 to π), two straight edges
    /// (0 to π/2, or 0 to π from a shared vertex) or a straight edge and a
    /// planar face (0 to π/2); None for other selections.
    pub angle: Option<f64>,
}

/// Two bodies that overlap, by their indices in the list asked about.
#[derive(Debug, Clone)]
pub struct Interference<S> {
    pub first: usize,
    pub second: usize,
    pub volume: f64,
    /// The overlapping solid.
    pub shape: S,
}

/// The intersection of a body with a plane.
#[derive(Debug, Clone)]
pub struct Section<S> {
    /// The section curves (edges).
    pub curves: S,
    /// The planar faces where the plane cuts the solids.
    pub faces: S,
    pub edge_count: usize,
    pub face_count: usize,
    pub length: f64,
    pub area: f64,
}

/// Options of [`crate::Kernel::compare_step`].
#[derive(Debug, Clone, PartialEq)]
pub struct CompareOptions {
    /// Points sampled on the faces of each side.
    pub samples: usize,
    /// Fuzzy tolerance of the boolean differences, mm.
    pub fuzzy: f64,
    /// The STEP body to compare with, by name or 0-based index; all bodies
    /// of the file when None.
    pub step_body: Option<String>,
    /// How long the comparison may take, s: the boolean differences are
    /// missing when they do not finish by then, and fewer points are
    /// sampled. None: no limit.
    pub seconds: Option<f64>,
    /// Leave out the boolean differences when every point was sampled and
    /// none lies farther than this from the other side, mm (0: never):
    /// of nearly coincident bodies they are slow and say little more
    /// (mitcad#69).
    pub booleans_above: f64,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            samples: 2000,
            fuzzy: 1e-4,
            step_body: None,
            seconds: None,
            booleans_above: 0.0,
        }
    }
}

/// Distances of sample points of one side from the other's surface.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Deviation {
    pub max: f64,
    pub rms: f64,
    /// The sample with the largest distance.
    pub at: Vec3,
    pub samples: usize,
}

/// Mitcad's bodies (A) against a STEP file (B).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comparison {
    /// The names of the STEP bodies compared.
    pub step_bodies: Vec<String>,
    pub volume_a: f64,
    pub volume_b: f64,
    /// Volumes of A - B and B - A; None when the boolean failed.
    pub a_minus_b: Option<f64>,
    pub b_minus_a: Option<f64>,
    /// (|A - B| + |B - A|) / max(|A|, |B|).
    pub relative_difference: Option<f64>,
    pub a_to_b: Deviation,
    pub b_to_a: Deviation,
    pub max_deviation: f64,
    /// The largest difference of the bounding box corners, mm.
    pub bounds_difference: f64,
}

/// A physical material: what a body's density comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    /// The id in files and commands, e.g. `aluminum`.
    pub id: &'static str,
    pub name: &'static str,
    /// g/cm³.
    pub density: f64,
}

/// Bodies without a material are steel, the default material.
pub const DEFAULT_MATERIAL: &str = "steel";

/// Built-in materials with typical densities. A body may name any material
/// id (a library may come later); only these give a density.
pub const MATERIALS: &[Material] = &[
    Material {
        id: "steel",
        name: "Steel",
        density: 7.85,
    },
    Material {
        id: "stainless_steel",
        name: "Stainless Steel",
        density: 8.0,
    },
    Material {
        id: "cast_iron",
        name: "Cast Iron",
        density: 7.2,
    },
    Material {
        id: "aluminum",
        name: "Aluminum",
        density: 2.70,
    },
    Material {
        id: "copper",
        name: "Copper",
        density: 8.96,
    },
    Material {
        id: "brass",
        name: "Brass",
        density: 8.5,
    },
    Material {
        id: "titanium",
        name: "Titanium",
        density: 4.51,
    },
    Material {
        id: "abs",
        name: "ABS Plastic",
        density: 1.06,
    },
    Material {
        id: "pla",
        name: "PLA Plastic",
        density: 1.24,
    },
    Material {
        id: "nylon",
        name: "Nylon",
        density: 1.14,
    },
    Material {
        id: "polycarbonate",
        name: "Polycarbonate",
        density: 1.2,
    },
    Material {
        id: "glass",
        name: "Glass",
        density: 2.5,
    },
    Material {
        id: "water",
        name: "Water",
        density: 1.0,
    },
    // A standard photopolymer resin of resin printers, cured (mitcad#1102):
    // the middle of the 1.1-1.2 g/cm³ standard resins' data sheets give.
    Material {
        id: "resin_standard",
        name: "Resin - Standard",
        density: 1.15,
    },
];

pub fn material(id: &str) -> Option<&'static Material> {
    MATERIALS.iter().find(|m| m.id == id)
}

/// Combined properties of bodies: masses add, the centre of mass is the
/// mass-weighted mean, and inertia tensors move to it by the parallel axis
/// theorem. Principal moments and axes are not combined (zero).
pub fn combine(parts: &[PhysicalProperties]) -> PhysicalProperties {
    let mut total = PhysicalProperties {
        volume: 0.0,
        area: 0.0,
        mass: 0.0,
        center_of_mass: [0.0; 3],
        inertia: [[0.0; 3]; 3],
        principal_moments: [0.0; 3],
        principal_axes: [[0.0; 3]; 3],
    };
    let weight = |p: &PhysicalProperties| if p.mass > 0.0 { p.mass } else { p.volume };
    let weights: f64 = parts.iter().map(weight).sum();
    for part in parts {
        total.volume += part.volume;
        total.area += part.area;
        total.mass += part.mass;
        if weights > 0.0 {
            for i in 0..3 {
                total.center_of_mass[i] += part.center_of_mass[i] * weight(part) / weights;
            }
        }
    }
    // kg mm²: I about c = I about the part's centre + m (|r|² E - r rᵀ).
    for part in parts {
        let r: Vec3 = std::array::from_fn(|i| part.center_of_mass[i] - total.center_of_mass[i]);
        let r2 = r.iter().map(|v| v * v).sum::<f64>();
        for i in 0..3 {
            for j in 0..3 {
                let shift = if i == j {
                    r2 - r[i] * r[j]
                } else {
                    -r[i] * r[j]
                };
                total.inertia[i][j] += part.inertia[i][j] + part.mass * shift;
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    #[test]
    fn maximum_face_gap_requires_two_convex_polygons_and_all_vertex_distances() {
        let square = [0., 0., 0., 20., 0., 0., 20., 20., 0., 0., 20., 0.];
        let reversed = [0., 20., 0., 20., 20., 0., 20., 0., 0., 0., 0., 0.];
        assert_eq!(
            super::maximum_polygon_face_gap(&square, &reversed, &[5.; 8]),
            Some(5.)
        );
        let distances = [5., 17., 17., 5., 5., 16., 16., 5.];
        assert_eq!(
            super::maximum_polygon_face_gap(&square, &square, &distances),
            Some(17.)
        );
        let concave = [
            0., 0., 0., 20., 0., 0., 10., 10., 0., 20., 20., 0., 0., 20., 0.,
        ];
        assert_eq!(
            super::maximum_polygon_face_gap(&square, &concave, &[5.; 9]),
            None
        );
        assert_eq!(
            super::maximum_polygon_face_gap(&concave, &square, &[5.; 9]),
            None
        );
        assert_eq!(
            super::maximum_polygon_face_gap(&square, &square, &[5.; 7]),
            None
        );
        let mut invalid = [5.; 8];
        invalid[3] = f64::NAN;
        assert_eq!(
            super::maximum_polygon_face_gap(&square, &square, &invalid),
            None
        );
    }

    use super::*;

    fn cube(center: Vec3, mass: f64, side: f64) -> PhysicalProperties {
        let moment = mass * side * side / 6.0;
        PhysicalProperties {
            volume: side.powi(3),
            area: 6.0 * side * side,
            mass,
            center_of_mass: center,
            inertia: [[moment, 0.0, 0.0], [0.0, moment, 0.0], [0.0, 0.0, moment]],
            principal_moments: [moment; 3],
            principal_axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    #[test]
    fn combined_properties_use_the_parallel_axis_theorem() {
        let total = combine(&[
            cube([-10.0, 0.0, 0.0], 1.0, 2.0),
            cube([10.0, 0.0, 0.0], 1.0, 2.0),
        ]);
        assert_eq!(total.mass, 2.0);
        assert_eq!(total.volume, 16.0);
        assert_eq!(total.center_of_mass, [0.0; 3]);
        let own = 4.0 / 6.0;
        assert!((total.inertia[0][0] - 2.0 * own).abs() < 1e-12);
        assert!((total.inertia[1][1] - (2.0 * own + 2.0 * 100.0)).abs() < 1e-12);
        assert!((total.inertia[2][2] - (2.0 * own + 2.0 * 100.0)).abs() < 1e-12);
        assert_eq!(total.inertia[0][1], 0.0);
    }

    #[test]
    fn steel_is_the_default_material() {
        assert_eq!(material(DEFAULT_MATERIAL).unwrap().density, 7.85);
        assert!(material("unobtainium").is_none());
        let mut ids: Vec<_> = MATERIALS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), MATERIALS.len());
    }
}
