// SPDX-License-Identifier: MIT
//! Triangles picked in the view and removed (mitcad#1871).
//!
//! A pick is any of:
//! - a lasso: a closed polygon drawn on a plane square to the view, its
//!   `normal` towards the viewer: the triangles whose centre, seen along
//!   the normal, lies inside it, of those facing the viewer (their normals
//!   within 85 degrees of it), or all of them (`through`);
//! - a brush: spheres of one radius about the points the brush touched:
//!   the triangles whose centre lies in one;
//! - triangles by their numbers in the mesh.
//!
//! The picked triangles are removed and the vertices no triangle uses any
//! more with them; the holes they leave stay open, or are filled
//! ([`super::repair::fill_loop`]): only holes that run along a removed
//! triangle are filled, a hole the mesh had before stays as it was.

use std::collections::HashSet;

use mitcad_vec3::{cross, dot, norm, sub};

use super::{Point, TriangleMesh, centroid, corners, edge_key, submesh_map, unit_normal};

/// A triangle faces the viewer when its normal lies within 85 degrees of
/// the direction to the viewer (its cosine above this): a wall seen edge
/// on, a little noisy, is not taken.
pub const FACING: f64 = 0.087_155_742_747_658_17;

/// A lasso: a polygon on a plane whose normal points to the viewer.
#[derive(Debug, Clone, PartialEq)]
pub struct Lasso {
    pub normal: Point,
    pub points: Vec<Point>,
}

/// A brush's spheres.
#[derive(Debug, Clone, PartialEq)]
pub struct Brush {
    pub radius: f64,
    pub points: Vec<Point>,
}

/// The triangles picked: by lassos, a brush and numbers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pick {
    pub lassos: Vec<Lasso>,
    pub brush: Option<Brush>,
    pub triangles: Vec<u32>,
    /// A lasso takes the triangles facing away from the viewer too.
    pub through: bool,
}

/// Whether each triangle is picked.
pub fn picked(mesh: &TriangleMesh, pick: &Pick) -> Vec<bool> {
    let n = mesh.triangles.len();
    let mut out = vec![false; n];
    for &t in &pick.triangles {
        if let Some(slot) = out.get_mut(t as usize) {
            *slot = true;
        }
    }
    let flat: Vec<(Point, Point, Point, Vec<[f64; 2]>)> = pick
        .lassos
        .iter()
        .filter_map(|lasso| {
            let normal = mitcad_vec3::unit(lasso.normal)?;
            let u = super::perpendicular(normal);
            let v = cross(normal, u);
            let polygon: Vec<[f64; 2]> = lasso
                .points
                .iter()
                .map(|p| [dot(*p, u), dot(*p, v)])
                .collect();
            (polygon.len() >= 3).then_some((normal, u, v, polygon))
        })
        .collect();
    for (t, slot) in out.iter_mut().enumerate() {
        if *slot {
            continue;
        }
        let p = corners(mesh, t);
        let c = centroid(&p);
        if let Some(brush) = &pick.brush
            && brush
                .points
                .iter()
                .any(|b| norm(sub(c, *b)) <= brush.radius)
        {
            *slot = true;
            continue;
        }
        if flat.is_empty() {
            continue;
        }
        let facing = unit_normal(&p);
        *slot = flat.iter().any(|(normal, u, v, polygon)| {
            (pick.through || dot(facing, *normal) > FACING)
                && inside(polygon, [dot(c, *u), dot(c, *v)])
        });
    }
    out
}

/// Whether `p` lies inside the polygon (even-odd rule).
fn inside(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = polygon.len();
    let mut odd = false;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                odd = !odd;
            }
        }
    }
    odd
}

/// What [`remove`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Removed {
    pub triangles: usize,
    pub holes_filled: usize,
}

/// The mesh without the picked triangles, the holes they leave filled when
/// `fill` (see the module docs).
pub fn remove(
    mesh: &TriangleMesh,
    pick: &Pick,
    fill: bool,
) -> Result<(TriangleMesh, Removed), String> {
    let chosen = picked(mesh, pick);
    let count = chosen.iter().filter(|c| **c).count();
    if count == 0 {
        return Err("the region picks no triangle of the mesh".to_owned());
    }
    if count == mesh.triangles.len() {
        return Err(
            "the region picks every triangle of the mesh; nothing would be left".to_owned(),
        );
    }
    // The edges between a removed and a kept triangle: the new holes run
    // along them.
    let mut rim: HashSet<(u32, u32)> = HashSet::new();
    let edges = super::edge_triangles(&mesh.triangles);
    for (key, on) in &edges {
        if on.iter().any(|&t| chosen[t]) && on.iter().any(|&t| !chosen[t]) {
            rim.insert(*key);
        }
    }
    let (mut out, index) = submesh_map(mesh, |t| !chosen[t]);
    let mut removed = Removed {
        triangles: count,
        holes_filled: 0,
    };
    if fill {
        let kept_rim: HashSet<(u32, u32)> = rim
            .iter()
            .filter_map(|(a, b)| Some(edge_key(index[*a as usize]?, index[*b as usize]?)))
            .collect();
        let along_rim = |lp: &[u32]| {
            (0..lp.len()).any(|i| kept_rim.contains(&edge_key(lp[i], lp[(i + 1) % lp.len()])))
        };
        let (filled, _) = super::repair::fill_holes(&mut out, f64::INFINITY, Some(&along_rim));
        removed.holes_filled = filled;
    }
    Ok((out, removed))
}
