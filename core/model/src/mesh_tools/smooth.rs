// SPDX-License-Identifier: MIT
//! Smoothing of a mesh or of a region of it (mitcad#2515), by Taubin's
//! two-step filter: each pass moves every vertex that may move towards the
//! middle of its neighbours by `amount` of the way, then back away from it
//! by a little more (`mu`, so that a smooth surface does not shrink), which
//! takes out the noise of a scan and keeps the shape.
//!
//! - A vertex on an edge whose triangles meet at more than the sharp angle
//!   moves only along that sharp edge (with its two neighbours on it), a
//!   corner of sharp edges (one, three or more of them) not at all, so
//!   that a box stays a box.
//! - A vertex on the rim of an open mesh, on an edge of more than two
//!   triangles or (with a region) on a triangle outside the region stays
//!   where it is.
//! - Volume kept: on a closed mesh the vertices free to move are then moved
//!   along their normals, all by one distance, until the mesh encloses the
//!   volume it had (to a billionth).

use mitcad_vec3::{add, dot, scale, sub};

use super::{Point, TriangleMesh, area_normal, corners, edge_triangles, unit_normal};

/// The share of the way `mu` overshoots `amount` by (Taubin's pass band).
const PASS_BAND: f64 = 0.1;

/// The most passes.
pub const MAX_PASSES: usize = 1000;

/// What [`smooth`] does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothOptions {
    /// How far each pass moves a vertex towards its neighbours' middle,
    /// more than 0 and at most 1.
    pub amount: f64,
    /// Passes, 1 to [`MAX_PASSES`].
    pub passes: usize,
    /// Edges whose triangles meet at more than this (radians) stay sharp;
    /// None keeps none.
    pub sharp_angle: Option<f64>,
    /// A closed mesh keeps the volume it encloses.
    pub keep_volume: bool,
}

/// What [`smooth`] made.
#[derive(Debug, Clone, PartialEq)]
pub struct Smoothed {
    pub mesh: TriangleMesh,
    /// Vertices free to move, along a sharp edge, kept where they are.
    pub free: usize,
    pub along_edges: usize,
    pub kept: usize,
    /// The farthest a vertex moved (mm).
    pub moved: f64,
    /// The volume before and after, for a closed mesh (mm³).
    pub volume: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Freedom {
    Free,
    Along(u32, u32),
    Kept,
}

/// The mesh smoothed (see the module docs); `region` (one flag per
/// triangle) limits it to the vertices of the triangles picked. `mesh` has
/// its shared corners joined.
pub fn smooth(
    mesh: &TriangleMesh,
    region: Option<&[bool]>,
    options: &SmoothOptions,
) -> Result<Smoothed, String> {
    if !(options.amount > 0.0 && options.amount <= 1.0) {
        return Err(format!(
            "the amount must be more than 0 and at most 1, got {}",
            options.amount
        ));
    }
    if !(1..=MAX_PASSES).contains(&options.passes) {
        return Err(format!(
            "the passes must be 1 to {MAX_PASSES}, got {}",
            options.passes
        ));
    }
    let n = mesh.vertices.len();
    let edges = edge_triangles(&mesh.triangles);
    let normals: Vec<Point> = (0..mesh.triangles.len())
        .map(|t| unit_normal(&corners(mesh, t)))
        .collect();
    let cos_sharp = options.sharp_angle.map(f64::cos);
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut sharp: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut kept = vec![false; n];
    let mut used = vec![false; n];
    for tri in &mesh.triangles {
        for v in tri {
            used[*v as usize] = true;
        }
    }
    let mut closed = true;
    for (&(a, b), on) in &edges {
        neighbours[a as usize].push(b);
        neighbours[b as usize].push(a);
        if on.len() != 2 {
            closed = false;
            kept[a as usize] = true;
            kept[b as usize] = true;
            continue;
        }
        if let Some(limit) = cos_sharp
            && dot(normals[on[0]], normals[on[1]]) < limit
        {
            sharp[a as usize].push(b);
            sharp[b as usize].push(a);
        }
    }
    if let Some(picked) = region {
        if !picked.iter().any(|p| *p) {
            return Err("the region picks no triangle of the mesh".to_owned());
        }
        for (t, tri) in mesh.triangles.iter().enumerate() {
            if !picked.get(t).copied().unwrap_or(false) {
                for v in tri {
                    kept[*v as usize] = true;
                }
            }
        }
    }
    let freedom: Vec<Freedom> = (0..n)
        .map(|v| {
            if kept[v] || !used[v] {
                return Freedom::Kept;
            }
            match sharp[v].as_slice() {
                [] => Freedom::Free,
                [a, b] => Freedom::Along(*a, *b),
                _ => Freedom::Kept,
            }
        })
        .collect();
    let free = freedom.iter().filter(|f| **f == Freedom::Free).count();
    let along_edges = freedom
        .iter()
        .filter(|f| matches!(f, Freedom::Along(..)))
        .count();
    if free + along_edges == 0 {
        return Err(
            "no vertex of the mesh is free to move: each lies on a rim, a sharp corner or \
             outside the region"
                .to_owned(),
        );
    }
    let lambda = options.amount;
    let mu = 1.0 / (PASS_BAND - 1.0 / lambda);
    let mut points = mesh.vertices.clone();
    let mut next = points.clone();
    for _ in 0..options.passes {
        for factor in [lambda, mu] {
            for v in 0..n {
                let middle = match freedom[v] {
                    Freedom::Kept => continue,
                    Freedom::Free => {
                        let around = &neighbours[v];
                        let mut sum = [0.0; 3];
                        for &u in around {
                            sum = add(sum, points[u as usize]);
                        }
                        scale(sum, 1.0 / around.len().max(1) as f64)
                    }
                    Freedom::Along(a, b) => scale(add(points[a as usize], points[b as usize]), 0.5),
                };
                next[v] = add(points[v], scale(sub(middle, points[v]), factor));
            }
            std::mem::swap(&mut points, &mut next);
            // The vertices that stay are the same in both.
            next.clone_from(&points);
        }
    }
    let mut out = TriangleMesh {
        vertices: points,
        triangles: mesh.triangles.clone(),
    };
    let mut volume = None;
    if closed {
        let before = super::volume(mesh);
        if options.keep_volume && free > 0 {
            for _ in 0..8 {
                let now = super::volume(&out);
                let gap = before - now;
                if gap.abs() <= 1e-9 * before.abs().max(1e-12) {
                    break;
                }
                let (normals, areas) = vertex_normals(&out);
                let movable: f64 = (0..n)
                    .filter(|v| freedom[*v] == Freedom::Free)
                    .map(|v| areas[v])
                    .sum();
                if movable <= 0.0 {
                    break;
                }
                let step = gap / movable;
                for v in 0..n {
                    if freedom[v] == Freedom::Free {
                        out.vertices[v] = add(out.vertices[v], scale(normals[v], step));
                    }
                }
            }
        }
        volume = Some((before, super::volume(&out)));
    }
    let moved = mesh
        .vertices
        .iter()
        .zip(&out.vertices)
        .map(|(a, b)| mitcad_vec3::norm(sub(*b, *a)))
        .fold(0.0, f64::max);
    Ok(Smoothed {
        mesh: out,
        free,
        along_edges,
        kept: n - free - along_edges,
        moved,
        volume,
    })
}

/// Each vertex's unit normal (its triangles' normals weighted by their
/// areas) and a third of its triangles' area.
pub(crate) fn vertex_normals(mesh: &TriangleMesh) -> (Vec<Point>, Vec<f64>) {
    let mut sums = vec![[0.0; 3]; mesh.vertices.len()];
    let mut areas = vec![0.0; mesh.vertices.len()];
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let normal = area_normal(&corners(mesh, t));
        let third = mitcad_vec3::norm(normal) / 6.0;
        for v in tri {
            sums[*v as usize] = add(sums[*v as usize], normal);
            areas[*v as usize] += third;
        }
    }
    (
        sums.into_iter()
            .map(|s| mitcad_vec3::unit(s).unwrap_or([0.0; 3]))
            .collect(),
        areas,
    )
}
