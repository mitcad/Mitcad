// SPDX-License-Identifier: MIT
//! Whether a mesh is one a tool can work on (mitcad#2515): Combine's
//! booleans, a capped Plane Cut and a volume kept by Smooth need a closed
//! mesh, and Combine and the results of Offset and Shell one that does
//! not cross itself. What is wrong is counted, with the places it is at
//! (a box about each hole, a point at each edge of more than two triangles
//! and where triangles cross), for the refusal to paint them in the view.

use mitcad_vec3::{add, cross, dot, norm, scale, sub};

use super::bvh::Bvh;
use super::{Point, TriangleMesh, area_normal, edge_triangles, repair, size};

/// The most places a refusal gives.
pub const MAX_PLACES: usize = 24;

/// A place a refusal paints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    Point(Point),
    Box { min: Point, max: Point },
}

/// Where a mesh is open: its holes (loops of edges of one triangle), its
/// open edges in all, its edges of more than two triangles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Openings {
    pub holes: usize,
    pub open_edges: usize,
    pub non_manifold_edges: usize,
    /// A box about each hole, a point at each edge of more than two
    /// triangles (at most [`MAX_PLACES`]).
    pub places: Vec<Place>,
}

impl Openings {
    /// Whether the mesh is closed: every edge on two triangles.
    pub fn closed(&self) -> bool {
        self.open_edges == 0 && self.non_manifold_edges == 0
    }

    /// What is open, in words: "2 holes", "an edge of more than two
    /// triangles", both.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.open_edges > 0 {
            parts.push(match self.holes {
                0 => "open edges that form no loop".to_owned(),
                1 => "a hole".to_owned(),
                n => format!("{n} holes"),
            });
        }
        if self.non_manifold_edges > 0 {
            parts.push(match self.non_manifold_edges {
                1 => "an edge of more than two triangles".to_owned(),
                n => format!("{n} edges of more than two triangles"),
            });
        }
        parts.join(" and ")
    }
}

/// Where the mesh is open (see [`Openings`]); `mesh` with its shared
/// corners joined ([`super::joined`]).
pub fn openings(mesh: &TriangleMesh) -> Openings {
    let edges = edge_triangles(&mesh.triangles);
    let mut out = Openings::default();
    let mut non_manifold: Vec<(u32, u32)> = Vec::new();
    for (key, on) in &edges {
        match on.len() {
            1 => out.open_edges += 1,
            2 => {}
            _ => non_manifold.push(*key),
        }
    }
    out.non_manifold_edges = non_manifold.len();
    if out.open_edges > 0 {
        let loops = repair::boundary_loops(&mesh.triangles);
        out.holes = loops.len();
        for lp in loops.iter().take(MAX_PLACES) {
            let mut min = [f64::INFINITY; 3];
            let mut max = [f64::NEG_INFINITY; 3];
            for &v in lp {
                let p = mesh.vertices[v as usize];
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
            out.places.push(Place::Box { min, max });
        }
    }
    non_manifold.sort_unstable();
    for (a, b) in non_manifold {
        if out.places.len() >= MAX_PLACES {
            break;
        }
        let middle = scale(
            add(mesh.vertices[a as usize], mesh.vertices[b as usize]),
            0.5,
        );
        out.places.push(Place::Point(middle));
    }
    out
}

/// Where a mesh crosses itself: pairs of triangles that cut through each
/// other (touching, lying on each other or sharing an edge is no
/// crossing), with a point of each crossing (at most [`MAX_PLACES`], at
/// least a tenth of the mesh's size apart).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Crossings {
    pub pairs: usize,
    pub places: Vec<Point>,
}

/// Where the mesh crosses itself (see [`Crossings`]); `mesh` with its
/// shared corners joined.
pub fn crossings(mesh: &TriangleMesh) -> Crossings {
    let tree = Bvh::new(mesh);
    let extent = size(&mesh.vertices);
    let eps = (1e-9 * extent).max(1e-12);
    let spacing = 0.1 * extent;
    let mut out = Crossings::default();
    tree.overlapping_pairs(eps, |a, b| {
        let ta = mesh.triangles[a];
        let tb = mesh.triangles[b];
        let shared: Vec<u32> = ta.iter().copied().filter(|v| tb.contains(v)).collect();
        if shared.len() >= 2 {
            return;
        }
        let pa = tree.triangle(a);
        let pb = tree.triangle(b);
        let hit = if let Some(&s) = shared.first() {
            // Neighbours at a corner: an edge away from that corner through
            // the inside of the other triangle (strictly: around a corner
            // the triangles' edges meet).
            let opposite = |tri: &[u32; 3], p: &[Point; 3]| {
                let k = tri.iter().position(|v| *v == s).unwrap_or(0);
                (p[(k + 1) % 3], p[(k + 2) % 3])
            };
            let (a0, a1) = opposite(&ta, pa);
            let (b0, b1) = opposite(&tb, pb);
            segment_through(a0, a1, pb, eps, eps).or_else(|| segment_through(b0, b1, pa, eps, eps))
        } else {
            triangles_cross(pa, pb, eps)
        };
        if let Some(at) = hit {
            out.pairs += 1;
            if out.places.len() < MAX_PLACES
                && out.places.iter().all(|p| norm(sub(*p, at)) >= spacing)
            {
                out.places.push(at);
            }
        }
    });
    out
}

/// A point where the triangles cut through each other: an edge of one
/// through the other (two triangles that are not in one plane and cross
/// meet so). None when they do not, or only touch.
pub(crate) fn triangles_cross(a: &[Point; 3], b: &[Point; 3], eps: f64) -> Option<Point> {
    for k in 0..3 {
        if let Some(at) = segment_crosses(a[k], a[(k + 1) % 3], b, eps) {
            return Some(at);
        }
        if let Some(at) = segment_crosses(b[k], b[(k + 1) % 3], a, eps) {
            return Some(at);
        }
    }
    None
}

/// Where the segment from `p` to `q` passes through the triangle: its ends
/// on either side of its plane by more than `eps`, the point inside the
/// triangle or on its edges (within `eps`). Triangles that share no corner
/// and meet so cross: where two meshes cross exactly along the edges of
/// their triangles (boxes on one grid), each crossing point lies on an edge.
pub(crate) fn segment_crosses(p: Point, q: Point, tri: &[Point; 3], eps: f64) -> Option<Point> {
    segment_through(p, q, tri, eps, -eps)
}

/// [`segment_crosses`], the point more than `inside` (positive: strictly
/// inside; negative: also on the edges) inside each of the triangle's
/// edges.
fn segment_through(p: Point, q: Point, tri: &[Point; 3], eps: f64, inside: f64) -> Option<Point> {
    let n = mitcad_vec3::unit(area_normal(tri))?;
    let dp = dot(sub(p, tri[0]), n);
    let dq = dot(sub(q, tri[0]), n);
    if !((dp > eps && dq < -eps) || (dp < -eps && dq > eps)) {
        return None;
    }
    let t = dp / (dp - dq);
    let x = add(p, scale(sub(q, p), t));
    for k in 0..3 {
        let a = tri[k];
        let b = tri[(k + 1) % 3];
        let edge = sub(b, a);
        if dot(cross(edge, sub(x, a)), n) <= inside * norm(edge) {
            return None;
        }
    }
    Some(x)
}
