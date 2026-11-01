// SPDX-License-Identifier: MIT
//! The automatic orientation on the print bed (mitcad#1109): the planar
//! face a piece without a usable print bed face lies on, worked out on its
//! faces' triangles ([`crate::Kernel::face_meshes`]) as slicers orient a
//! part. Nothing here changes a body; the export (`bed.rs`) turns the piece
//! onto the face found.
//!
//! - **Candidates:** the planar faces whose plane supports the piece's
//!   convex hull: every vertex of every face lies on the plane or on the
//!   side the face's inward normal points to, so the piece lies on the
//!   plane without going below it. Coplanar faces are one candidate (the
//!   largest of them names it). A curved face is never one, so a body
//!   without planar faces (a sphere, a mesh body's triangles) has none.
//! - **Score** of each candidate, smaller is better, every term without a
//!   unit so that the size of the piece does not matter:
//!   - the overhang area after the turn: triangles that face down within
//!     45° of straight down ([`crate::print_analysis::overhangs`] at its
//!     default angle), the triangles on the bed left out, over the whole
//!     area, weighed [`OVERHANG_WEIGHT`]: what needs supports counts most;
//!   - the contact area with the bed (the triangles facing down on the
//!     plane) over the whole area, weighed [`CONTACT_WEIGHT`] and taken
//!     off: a large contact holds the piece on the bed;
//!   - the height after the turn over the box's diagonal, weighed
//!     [`HEIGHT_WEIGHT`]: lower prints faster and stands more firmly; it
//!     mostly decides between candidates alike otherwise.
//! - **Ties** (scores within [`TIE`]) go to the face whose outward normal
//!   is nearest to straight down in the design (−Z), so that a piece
//!   already lying well keeps its orientation, then to the face listed
//!   first.

use mitcad_vec3::{Vec3, add, cross, dot, norm, scale, sub, unit};

use crate::exchange::{FaceMeshes, TriangleMesh};
use crate::print_analysis::{self, Bed, DEFAULT_ANGLE};

/// The weight of the overhang area's share of the whole area: support
/// material and the marks it leaves are what an orientation avoids first.
pub const OVERHANG_WEIGHT: f64 = 1.0;
/// The weight of the contact area's share of the whole area (taken off the
/// score): half the overhang's, so that a face that holds the piece well is
/// still not bought with overhangs.
pub const CONTACT_WEIGHT: f64 = 0.5;
/// The weight of the height over the box's diagonal: small, a tie breaker
/// between faces that hold the piece alike and need no supports.
pub const HEIGHT_WEIGHT: f64 = 0.1;
/// Scores closer than this are alike (see the ties above).
pub const TIE: f64 = 1e-9;

/// The distance within which a point lies on a plane, mm, and the relative
/// part of the piece's size added to it (the vertices of placed and turned
/// meshes carry rounding errors).
const ON_PLANE: f64 = 1e-6;
const ON_PLANE_RELATIVE: f64 = 1e-9;
/// Unit normals this close (1 − cos) are the same direction.
const SAME_DIRECTION: f64 = 1e-9;

/// The face a piece lies on, and how it scored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BedOrientation {
    /// The index of the face in the face meshes' faces (`face_triangles`).
    pub face: usize,
    /// Its outward unit normal, which the turn takes straight down.
    pub normal: Vec3,
    /// The overhang area after the turn, mm².
    pub overhang: f64,
    /// The contact area with the bed, mm².
    pub contact: f64,
    /// The height after the turn, mm.
    pub height: f64,
    /// The score (smaller is better).
    pub score: f64,
}

/// The best face for `mesh` to lie on (the module's rule) among the faces
/// `planar` marks (one flag per face of `mesh.face_triangles`; a face
/// without a flag is no candidate), or None when no planar face supports
/// the hull. The triangles of every face count for the support, the
/// overhangs and the areas.
pub fn best_bed_face(mesh: &FaceMeshes, planar: &[bool]) -> Option<BedOrientation> {
    let triangles = TriangleMesh {
        vertices: mesh.vertices.clone(),
        triangles: mesh.triangles.clone(),
    };
    let corners =
        |t: usize| -> [Vec3; 3] { triangles.triangles[t].map(|i| triangles.vertices[i as usize]) };
    let area_normal = |t: usize| -> Vec3 {
        let p = corners(t);
        cross(sub(p[1], p[0]), sub(p[2], p[0]))
    };
    let total: f64 = (0..triangles.triangles.len())
        .map(|t| norm(area_normal(t)) / 2.0)
        .filter(|a| a.is_finite())
        .sum();
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for v in &triangles.vertices {
        min = std::array::from_fn(|i| min[i].min(v[i]));
        max = std::array::from_fn(|i| max[i].max(v[i]));
    }
    let diagonal = norm(sub(max, min));
    if total.is_nan() || total <= 0.0 || !diagonal.is_finite() || diagonal <= 0.0 {
        return None;
    }
    let tolerance = ON_PLANE + ON_PLANE_RELATIVE * diagonal;

    // Each planar face's plane: its outward normal (the sum of its
    // triangles' area normals) and its level along it.
    let mut first = 0usize;
    // (face, normal, level, area)
    let mut planes: Vec<(usize, Vec3, f64, f64)> = Vec::new();
    for (face, &count) in mesh.face_triangles.iter().enumerate() {
        let range = first..first + count as usize;
        first = range.end;
        if !planar.get(face).copied().unwrap_or(false) || range.is_empty() {
            continue;
        }
        if range.end > triangles.triangles.len() {
            break;
        }
        let summed = range
            .clone()
            .fold([0.0; 3], |sum, t| add(sum, area_normal(t)));
        let Some(normal) = unit(summed) else {
            continue;
        };
        let area = norm(summed) / 2.0;
        let level = dot(corners(range.start)[0], normal);
        // Flat indeed: every corner on the plane.
        let flat = range.clone().all(|t| {
            corners(t)
                .iter()
                .all(|p| (dot(*p, normal) - level).abs() <= tolerance)
        });
        if !flat {
            continue;
        }
        // The hull on the inner side: nothing beyond the plane.
        let supports = triangles
            .vertices
            .iter()
            .all(|v| dot(*v, normal) <= level + tolerance);
        if !supports {
            continue;
        }
        // Coplanar faces are one candidate, named by the largest.
        match planes.iter_mut().find(|(_, n, l, _)| {
            1.0 - dot(*n, normal) <= SAME_DIRECTION && (l - level).abs() <= tolerance
        }) {
            Some(same) => {
                if area > same.3 {
                    same.0 = face;
                    same.3 = area;
                }
            }
            None => planes.push((face, normal, level, area)),
        }
    }

    let mut best: Option<BedOrientation> = None;
    for &(face, normal, level, _) in &planes {
        let point = scale(normal, level);
        let found = print_analysis::overhangs(
            &triangles,
            normal,
            DEFAULT_ANGLE,
            Bed::Through(point),
            tolerance,
        );
        let contact: f64 = (0..triangles.triangles.len())
            .filter(|&t| {
                let n = area_normal(t);
                let double = norm(n);
                double > 0.0
                    && dot(n, normal) / double > 1.0 - 1e-6
                    && corners(t)
                        .iter()
                        .all(|p| (dot(*p, normal) - level).abs() <= tolerance)
            })
            .map(|t| norm(area_normal(t)) / 2.0)
            .sum();
        let lowest = triangles
            .vertices
            .iter()
            .map(|v| dot(*v, normal))
            .fold(f64::INFINITY, f64::min);
        let height = level - lowest;
        let score = OVERHANG_WEIGHT * found.overhang_area / total
            - CONTACT_WEIGHT * contact / total
            + HEIGHT_WEIGHT * height / diagonal;
        let candidate = BedOrientation {
            face,
            normal,
            overhang: found.overhang_area,
            contact,
            height,
            score,
        };
        let better = match &best {
            None => true,
            Some(b) if score < b.score - TIE => true,
            // Alike: the one facing down most in the design.
            Some(b) if (score - b.score).abs() <= TIE => -normal[2] > -b.normal[2] + SAME_DIRECTION,
            Some(_) => false,
        };
        if better {
            best = Some(candidate);
        }
    }
    best
}
