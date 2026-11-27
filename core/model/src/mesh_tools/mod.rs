// SPDX-License-Identifier: MIT
//! Mesh tools for reverse engineering (mitcad#1871): what turns a scanned
//! or exported mesh (an STL, OBJ or 3MF file's triangles) into geometry to
//! build on. Each works on a [`TriangleMesh`] and leaves its input as it is:
//!
//! - [`section`]: a plane's section of a mesh as polylines in the plane,
//!   fitted to lines, arcs and circles within a tolerance (sketch curves);
//! - [`decimate`]: fewer triangles, to a count or within a tolerance,
//!   keeping the outline and the sharp edges;
//! - [`repair`]: vertices welded, degenerate, duplicate and non-manifold
//!   triangles taken out, the triangles oriented, holes filled up to a size;
//! - [`region`]: triangles picked by a lasso, a brush or their numbers,
//!   removed, the hole left open or filled;
//! - [`segment`]: the mesh split into regions by its curvature: planes,
//!   cylinders and free-form regions;
//! - [`fit`]: a plane, a cylinder, a cone, a sphere or a B-spline surface
//!   fitted to a region, bounded as a face of a surface body;
//! - [`smooth`] (mitcad#2515): the noise of a scan taken out, a sharp edge
//!   kept sharp, the volume kept;
//! - [`offset`] (mitcad#2515): the surface moved by a distance, or made a
//!   closed wall of a thickness with openings; where the moved triangles
//!   turn over, a closed mesh's offset built from its distance
//!   (`offset_field`, mitcad#2564);
//! - [`align`] (mitcad#2515): points moved onto a mesh by picked pairs and
//!   a best fit, the deviation left measured;
//! - [`plane_cut`] (mitcad#2515): a mesh cut by a plane into two pieces,
//!   the cut capped or open;
//! - [`check`] (mitcad#2515): where a mesh is open or crosses itself, for
//!   the tools that need a closed mesh to refuse it with the places;
//! - [`merged`] (mitcad#2515): meshes put together into one; Combine's
//!   booleans themselves are the kernel's, on faceted solids.
//!
//! The features (`features/mesh_tools.rs`) make new mesh and surface bodies
//! of their results; the mesh they start from stays as it was.

pub mod decimate;
pub mod fit;
pub mod region;
pub mod repair;
pub mod section;
pub mod segment;
// Smooth, Offset and Shell, Align, Plane Cut and the checks Combine needs
// (mitcad#2515).
pub mod align;
pub(crate) mod bvh;
pub mod check;
pub mod offset;
// An offset that turns over, built from the distance (mitcad#2564).
pub(crate) mod offset_field;
pub mod plane_cut;
pub mod smooth;

#[cfg(test)]
mod shape_tests;
#[cfg(test)]
pub(crate) mod synthetic;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use mitcad_vec3::{cross, dot, norm, sub};

pub use crate::exchange::TriangleMesh;

pub(crate) type Point = [f64; 3];

/// An edge by its two vertices, the smaller first.
pub(crate) fn edge_key(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

/// The triangles on each edge, in the triangles' order.
pub(crate) fn edge_triangles(triangles: &[[u32; 3]]) -> HashMap<(u32, u32), Vec<usize>> {
    let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::with_capacity(triangles.len() * 2);
    for (t, tri) in triangles.iter().enumerate() {
        for k in 0..3 {
            edges
                .entry(edge_key(tri[k], tri[(k + 1) % 3]))
                .or_default()
                .push(t);
        }
    }
    edges
}

/// A triangle's corners.
pub(crate) fn corners(mesh: &TriangleMesh, t: usize) -> [Point; 3] {
    let tri = mesh.triangles[t];
    [
        mesh.vertices[tri[0] as usize],
        mesh.vertices[tri[1] as usize],
        mesh.vertices[tri[2] as usize],
    ]
}

/// Twice the triangle's area along its normal (not unit).
pub(crate) fn area_normal(p: &[Point; 3]) -> Point {
    cross(sub(p[1], p[0]), sub(p[2], p[0]))
}

/// The triangle's area.
pub(crate) fn area(p: &[Point; 3]) -> f64 {
    0.5 * norm(area_normal(p))
}

/// The triangle's unit normal, zero for one without area.
pub(crate) fn unit_normal(p: &[Point; 3]) -> Point {
    mitcad_vec3::unit(area_normal(p)).unwrap_or([0.0; 3])
}

pub(crate) fn centroid(p: &[Point; 3]) -> Point {
    std::array::from_fn(|i| (p[0][i] + p[1][i] + p[2][i]) / 3.0)
}

/// The mesh's size: the diagonal of its vertices' box (0 for none).
pub(crate) fn size(points: &[Point]) -> f64 {
    let Some(first) = points.first() else {
        return 0.0;
    };
    let mut min = *first;
    let mut max = *first;
    for p in points {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    norm(sub(max, min))
}

/// The triangles kept as a mesh of their own: the vertices they use, in
/// the order they come first.
pub(crate) fn submesh(mesh: &TriangleMesh, keep: impl Fn(usize) -> bool) -> TriangleMesh {
    submesh_map(mesh, keep).0
}

/// [`submesh`] with each old vertex's new number (None where unused).
pub(crate) fn submesh_map(
    mesh: &TriangleMesh,
    keep: impl Fn(usize) -> bool,
) -> (TriangleMesh, Vec<Option<u32>>) {
    let mut index: Vec<Option<u32>> = vec![None; mesh.vertices.len()];
    let mut out = TriangleMesh::default();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        if !keep(t) {
            continue;
        }
        let mapped = tri.map(|v| {
            *index[v as usize].get_or_insert_with(|| {
                out.vertices.push(mesh.vertices[v as usize]);
                (out.vertices.len() - 1) as u32
            })
        });
        out.triangles.push(mapped);
    }
    (out, index)
}

/// The enclosed volume of the triangles (positive when they face out).
#[cfg(test)]
pub(crate) fn signed_volume(mesh: &TriangleMesh, triangles: &[usize]) -> f64 {
    triangles
        .iter()
        .map(|&t| {
            let p = corners(mesh, t);
            dot(p[0], cross(p[1], p[2])) / 6.0
        })
        .sum()
}

/// The volume the mesh encloses (mm³; positive when its triangles face
/// out), summed about the middle of its box to keep the sum's error small.
pub fn volume(mesh: &TriangleMesh) -> f64 {
    let Some(first) = mesh.vertices.first() else {
        return 0.0;
    };
    let mut min = *first;
    let mut max = *first;
    for p in &mesh.vertices {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let middle: Point = std::array::from_fn(|i| 0.5 * (min[i] + max[i]));
    (0..mesh.triangles.len())
        .map(|t| {
            let p = corners(mesh, t).map(|q| sub(q, middle));
            dot(p[0], cross(p[1], p[2])) / 6.0
        })
        .sum()
}

/// The meshes put together into one, each as it is (no boolean: where they
/// overlap, they still do).
pub fn merged(meshes: &[TriangleMesh]) -> TriangleMesh {
    let mut out = TriangleMesh::default();
    for mesh in meshes {
        let base = out.vertices.len() as u32;
        out.vertices.extend_from_slice(&mesh.vertices);
        out.triangles
            .extend(mesh.triangles.iter().map(|t| t.map(|v| v + base)));
    }
    out
}

/// The mesh with the corners its triangles share joined: vertices within a
/// billionth of the mesh's size (at least 1e-9 mm) of each other are one,
/// and triangles left with two corners the same are left out. A mesh as an
/// STL file stores it has each triangle's own three corners; the tools need
/// them shared to see which triangles are neighbours. The other triangles
/// keep their order.
pub fn joined(mesh: &TriangleMesh) -> TriangleMesh {
    let tolerance = (1e-9 * size(&mesh.vertices)).max(1e-9);
    let (points, map) = repair::weld(&mesh.vertices, tolerance);
    let welded = TriangleMesh {
        vertices: points,
        triangles: mesh
            .triangles
            .iter()
            .map(|tri| tri.map(|v| map[v as usize]))
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect(),
    };
    submesh(&welded, |_| true)
}

/// What [`mesh_stats`] counts of a mesh.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MeshStats {
    pub vertices: usize,
    pub triangles: usize,
    /// Edges of one triangle only (the rims of holes and of open surfaces).
    pub open_edges: usize,
    /// Edges of more than two triangles.
    pub non_manifold_edges: usize,
    /// Loops of open edges.
    pub holes: usize,
    pub area: f64,
}

/// Counts of the mesh's vertices, triangles, open and non-manifold edges
/// and holes, and its area.
pub fn mesh_stats(mesh: &TriangleMesh) -> MeshStats {
    let edges = edge_triangles(&mesh.triangles);
    let open_edges = edges.values().filter(|t| t.len() == 1).count();
    let non_manifold_edges = edges.values().filter(|t| t.len() > 2).count();
    MeshStats {
        vertices: mesh.vertices.len(),
        triangles: mesh.triangles.len(),
        open_edges,
        non_manifold_edges,
        holes: repair::boundary_loops(&mesh.triangles).len(),
        area: (0..mesh.triangles.len())
            .map(|t| area(&corners(mesh, t)))
            .sum(),
    }
}

// Small dense linear algebra for the fits.

/// Solves `a x = b` (n × n, row-major) by Gaussian elimination with
/// partial pivoting; None when it is singular.
pub(crate) fn solve(mut a: Vec<f64>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    debug_assert_eq!(a.len(), n * n);
    for col in 0..n {
        let pivot =
            (col..n).max_by(|&i, &j| a[i * n + col].abs().total_cmp(&a[j * n + col].abs()))?;
        let scale = a[pivot * n + col];
        if !scale.is_finite() || scale.abs() < 1e-300 {
            return None;
        }
        if pivot != col {
            for k in 0..n {
                a.swap(col * n + k, pivot * n + k);
            }
            b.swap(col, pivot);
        }
        for row in col + 1..n {
            let factor = a[row * n + col] / a[col * n + col];
            if factor == 0.0 {
                continue;
            }
            for k in col..n {
                a[row * n + k] -= factor * a[col * n + k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut sum = b[row];
        for k in row + 1..n {
            sum -= a[row * n + k] * x[k];
        }
        x[row] = sum / a[row * n + row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// The eigenvalues (ascending) and unit eigenvectors of a symmetric 3 × 3
/// matrix, by Jacobi rotations.
pub(crate) fn symmetric_eigen(m: [[f64; 3]; 3]) -> ([f64; 3], [Point; 3]) {
    let mut a = m;
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..64 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off < 1e-300 {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for row in &mut a {
                let akp = row[p];
                let akq = row[q];
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
            }
            let (row_p, row_q) = (a[p], a[q]);
            for k in 0..3 {
                a[p][k] = c * row_p[k] - s * row_q[k];
                a[q][k] = s * row_p[k] + c * row_q[k];
            }
            for row in &mut v {
                let vp = row[p];
                let vq = row[q];
                row[p] = c * vp - s * vq;
                row[q] = s * vp + c * vq;
            }
        }
    }
    let mut order = [0usize, 1, 2];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let values = order.map(|i| a[i][i]);
    let vectors = order.map(|i| {
        let column = [v[0][i], v[1][i], v[2][i]];
        mitcad_vec3::unit(column).unwrap_or(column)
    });
    (values, vectors)
}

/// The points' mean and their scatter matrix about it.
pub(crate) fn scatter(points: &[Point]) -> (Point, [[f64; 3]; 3]) {
    let n = points.len().max(1) as f64;
    let mut mean = [0.0; 3];
    for p in points {
        for i in 0..3 {
            mean[i] += p[i] / n;
        }
    }
    let mut m = [[0.0; 3]; 3];
    for p in points {
        let d = sub(*p, mean);
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += d[i] * d[j];
            }
        }
    }
    (mean, m)
}

/// A unit vector at right angles to `n` (unit), along the model axis
/// least along `n`.
pub(crate) fn perpendicular(n: Point) -> Point {
    let axis = if n[0].abs() <= n[1].abs() && n[0].abs() <= n[2].abs() {
        [1.0, 0.0, 0.0]
    } else if n[1].abs() <= n[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let d = dot(axis, n);
    mitcad_vec3::unit(std::array::from_fn(|i| axis[i] - d * n[i])).unwrap_or([1.0, 0.0, 0.0])
}
