// SPDX-License-Identifier: MIT
//! Segmentation of a mesh by its curvature (mitcad#1871): regions that are
//! planes, cylinders or free-form surfaces.
//!
//! 1. Smooth patches: triangles joined across edges where their normals
//!    differ by at most `angle`; edges of one triangle or of more than two
//!    end a patch.
//! 2. Each patch is fitted ([`super::fit`]): a plane when every vertex lies
//!    within `tolerance` of the plane fitted to it, else a cylinder when
//!    every vertex lies within `tolerance` of the cylinder fitted to it.
//! 3. A patch that is neither is searched for planes inside it: regions
//!    grown from its largest triangles while each new triangle's corners lie
//!    within `tolerance` of the region's plane, kept when they hold at least
//!    a twentieth of the patch's area; what is left of the patch, piece by
//!    connected piece, is fitted again as in 2, and free-form where nothing
//!    fits.
//!
//! The regions come largest first; their triangles are the mesh's numbers.

use std::collections::VecDeque;

use mitcad_vec3::{dot, sub};

use super::fit::{SurfaceKind, fit_surface};
use super::{Point, TriangleMesh, area, corners, edge_key, edge_triangles, submesh, unit_normal};

/// What a region is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Plane,
    Cylinder,
    FreeForm,
}

impl RegionKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Cylinder => "cylinder",
            Self::FreeForm => "free-form",
        }
    }
}

/// A region of the mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub kind: RegionKind,
    pub triangles: Vec<usize>,
    pub area: f64,
}

/// The most triangles segmentation takes.
pub const MAX_TRIANGLES: usize = 500_000;

/// The mesh's regions (see the module docs).
pub fn segment(mesh: &TriangleMesh, angle: f64, tolerance: f64) -> Result<Vec<Region>, String> {
    let n = mesh.triangles.len();
    if n == 0 {
        return Err("the mesh has no triangles".to_owned());
    }
    if n > MAX_TRIANGLES {
        return Err(format!(
            "the mesh has {n} triangles; segmentation takes at most {MAX_TRIANGLES} (decimate it first)"
        ));
    }
    let normals: Vec<Point> = (0..n).map(|t| unit_normal(&corners(mesh, t))).collect();
    let areas: Vec<f64> = (0..n).map(|t| area(&corners(mesh, t))).collect();
    let edges = edge_triangles(&mesh.triangles);
    let neighbours = |t: usize| -> Vec<usize> {
        let tri = mesh.triangles[t];
        (0..3)
            .filter_map(|k| {
                let on = &edges[&edge_key(tri[k], tri[(k + 1) % 3])];
                (on.len() == 2).then(|| if on[0] == t { on[1] } else { on[0] })
            })
            .collect()
    };
    let cos_angle = angle.cos();
    // 1. Smooth patches.
    let patches = grow(
        n,
        &areas,
        |t, other| dot(normals[t], normals[other]) >= cos_angle,
        &neighbours,
    );
    let mut regions = Vec::new();
    for patch in patches {
        if let Some(kind) = classify(mesh, &patch, tolerance) {
            regions.push(region(kind, patch, &areas));
            continue;
        }
        // 3. Planes inside a free-form patch.
        let total: f64 = patch.iter().map(|&t| areas[t]).sum();
        let mut in_patch = vec![false; n];
        for &t in &patch {
            in_patch[t] = true;
        }
        let mut order = patch.clone();
        order.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]).then(a.cmp(&b)));
        let mut taken = vec![false; n];
        for seed in order {
            if taken[seed] {
                continue;
            }
            let plane = planar_region(
                mesh,
                seed,
                &in_patch,
                &taken,
                &neighbours,
                &normals,
                tolerance,
            );
            let plane_area: f64 = plane.iter().map(|&t| areas[t]).sum();
            if plane.len() >= 2 && plane_area >= total / 20.0 {
                for &t in &plane {
                    taken[t] = true;
                }
                regions.push(region(RegionKind::Plane, plane, &areas));
            }
        }
        // What is left, piece by piece.
        let rest: Vec<usize> = patch.iter().copied().filter(|&t| !taken[t]).collect();
        let mut left = vec![false; n];
        for &t in &rest {
            left[t] = true;
        }
        let pieces = grow_within(&rest, &left, &neighbours);
        for piece in pieces {
            let kind = classify(mesh, &piece, tolerance).unwrap_or(RegionKind::FreeForm);
            regions.push(region(kind, piece, &areas));
        }
    }
    regions.sort_by(|a, b| {
        b.area
            .total_cmp(&a.area)
            .then(a.triangles.first().cmp(&b.triangles.first()))
    });
    Ok(regions)
}

fn region(kind: RegionKind, mut triangles: Vec<usize>, areas: &[f64]) -> Region {
    triangles.sort_unstable();
    Region {
        kind,
        area: triangles.iter().map(|&t| areas[t]).sum(),
        triangles,
    }
}

/// Patches of triangles joined where `joins(t, neighbour)` holds, grown
/// from the largest triangles first.
fn grow(
    n: usize,
    areas: &[f64],
    joins: impl Fn(usize, usize) -> bool,
    neighbours: &dyn Fn(usize) -> Vec<usize>,
) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]).then(a.cmp(&b)));
    let mut seen = vec![false; n];
    let mut patches = Vec::new();
    for seed in order {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        let mut patch = vec![seed];
        let mut queue = VecDeque::from([seed]);
        while let Some(t) = queue.pop_front() {
            for other in neighbours(t) {
                if !seen[other] && joins(t, other) {
                    seen[other] = true;
                    patch.push(other);
                    queue.push_back(other);
                }
            }
        }
        patches.push(patch);
    }
    patches
}

/// The connected pieces of `triangles` (those marked in `within`).
fn grow_within(
    triangles: &[usize],
    within: &[bool],
    neighbours: &dyn Fn(usize) -> Vec<usize>,
) -> Vec<Vec<usize>> {
    let mut seen = vec![false; within.len()];
    let mut pieces = Vec::new();
    for &seed in triangles {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        let mut piece = vec![seed];
        let mut queue = VecDeque::from([seed]);
        while let Some(t) = queue.pop_front() {
            for other in neighbours(t) {
                if within[other] && !seen[other] {
                    seen[other] = true;
                    piece.push(other);
                    queue.push_back(other);
                }
            }
        }
        pieces.push(piece);
    }
    pieces
}

/// A plane grown from `seed` inside the patch: triangles whose corners lie
/// within `tolerance` of the plane of the seed (refitted as it grows) and
/// whose normals agree with it within 10 degrees.
fn planar_region(
    mesh: &TriangleMesh,
    seed: usize,
    in_patch: &[bool],
    taken: &[bool],
    neighbours: &dyn Fn(usize) -> Vec<usize>,
    normals: &[Point],
    tolerance: f64,
) -> Vec<usize> {
    let mut normal = normals[seed];
    let mut origin = corners(mesh, seed)[0];
    let mut region = vec![seed];
    let mut inside = vec![false; in_patch.len()];
    inside[seed] = true;
    let mut queue = VecDeque::from([seed]);
    let cos_limit = 10f64.to_radians().cos();
    while let Some(t) = queue.pop_front() {
        for other in neighbours(t) {
            if !in_patch[other] || taken[other] || inside[other] {
                continue;
            }
            if dot(normals[other], normal) < cos_limit {
                continue;
            }
            let fits = corners(mesh, other)
                .iter()
                .all(|p| dot(sub(*p, origin), normal).abs() <= tolerance);
            if !fits {
                continue;
            }
            inside[other] = true;
            region.push(other);
            queue.push_back(other);
            // Refit now and then, as the region grows.
            if region.len().is_power_of_two() {
                let points: Vec<Point> = region.iter().flat_map(|&t| corners(mesh, t)).collect();
                let (mean, scatter) = super::scatter(&points);
                let (_, vectors) = super::symmetric_eigen(scatter);
                let mut fitted = vectors[0];
                if dot(fitted, normal) < 0.0 {
                    fitted = fitted.map(|c| -c);
                }
                normal = fitted;
                origin = mean;
            }
        }
    }
    region
}

/// A plane or a cylinder within `tolerance` of every vertex of the
/// triangles; None when neither fits.
fn classify(mesh: &TriangleMesh, triangles: &[usize], tolerance: f64) -> Option<RegionKind> {
    let part = {
        let mut keep = vec![false; mesh.triangles.len()];
        for &t in triangles {
            keep[t] = true;
        }
        submesh(mesh, |t| keep[t])
    };
    if fit_surface(&part, SurfaceKind::Plane).is_ok_and(|f| f.deviation <= tolerance) {
        return Some(RegionKind::Plane);
    }
    if triangles.len() >= 4
        && fit_surface(&part, SurfaceKind::Cylinder).is_ok_and(|f| f.deviation <= tolerance)
    {
        return Some(RegionKind::Cylinder);
    }
    None
}
