// SPDX-License-Identifier: MIT
//! Decimation (mitcad#1871): fewer triangles, to a count or within a
//! tolerance, keeping the mesh's outline and its sharp edges.
//!
//! Edges are collapsed one at a time, the cheapest first: a vertex moves
//! onto a neighbour (no new points are made, so every vertex left is one
//! of the mesh's), the two triangles on the edge go and the others at the
//! vertex follow it. The cost of moving a vertex is the sum of the squared
//! distances of its new place from the planes of the triangles it has
//! carried so far (the quadric error), so that a collapse within
//! `tolerance` leaves every merged triangle's plane within the tolerance of
//! the new surface.
//!
//! - Feature edges are edges of one triangle (the outline, the rims of
//!   holes), of more than two, and edges whose triangles meet at more than
//!   `sharp_angle`. A vertex on no feature edge moves freely; one on two
//!   moves only along them (the outline and a sharp edge stay where they
//!   were, with fewer points); a vertex on one or on three or more (a
//!   corner) stays.
//! - A collapse that would join two sheets (the two vertices share a
//!   neighbour that is not across the edge), turn a triangle over or leave
//!   one without area is not made.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use mitcad_vec3::{dot, norm};

use super::{Point, TriangleMesh, area_normal, corners, edge_key, edge_triangles, submesh};

/// What [`decimate`] does: it stops at whichever limit comes first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecimateOptions {
    /// Stop once the mesh has this many triangles or fewer.
    pub triangles: Option<usize>,
    /// Make no collapse whose error exceeds this (mm).
    pub tolerance: Option<f64>,
    /// Edges whose triangles meet at more than this are kept (radians).
    pub sharp_angle: f64,
}

/// What [`decimate`] made.
#[derive(Debug, Clone, PartialEq)]
pub struct Decimated {
    pub mesh: TriangleMesh,
    /// The largest error a collapse made (mm): how far the merged
    /// triangles' planes may lie from the new surface.
    pub error: f64,
}

type Quadric = [f64; 10];

fn plane_quadric(n: Point, p: Point) -> Quadric {
    let d = -dot(n, p);
    let [a, b, c] = n;
    [
        a * a,
        a * b,
        a * c,
        a * d,
        b * b,
        b * c,
        b * d,
        c * c,
        c * d,
        d * d,
    ]
}

fn quadric_error(q: &Quadric, p: Point) -> f64 {
    let [x, y, z] = p;
    let value = q[0] * x * x
        + 2.0 * q[1] * x * y
        + 2.0 * q[2] * x * z
        + 2.0 * q[3] * x
        + q[4] * y * y
        + 2.0 * q[5] * y * z
        + 2.0 * q[6] * y
        + q[7] * z * z
        + 2.0 * q[8] * z
        + q[9];
    value.max(0.0)
}

/// A candidate collapse of `from` onto `to`, cheapest first.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Candidate {
    cost: f64,
    from: u32,
    to: u32,
    stamp: u32,
}

impl Eq for Candidate {}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then(other.from.cmp(&self.from))
            .then(other.to.cmp(&self.to))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct State<'a> {
    points: &'a [Point],
    triangles: Vec<[u32; 3]>,
    alive: Vec<bool>,
    alive_count: usize,
    /// Each vertex's live triangles.
    around: Vec<Vec<usize>>,
    /// Each vertex's neighbours along feature edges.
    features: Vec<Vec<u32>>,
    quadrics: Vec<Quadric>,
    stamps: Vec<u32>,
    min_area2: f64,
}

impl State<'_> {
    fn neighbours(&self, v: u32) -> HashSet<u32> {
        self.around[v as usize]
            .iter()
            .flat_map(|&t| self.triangles[t])
            .filter(|&w| w != v)
            .collect()
    }

    fn movable(&self, from: u32, to: u32) -> bool {
        let f = &self.features[from as usize];
        match f.len() {
            0 => true,
            2 => f.contains(&to),
            _ => false,
        }
    }

    /// Whether collapsing `from` onto `to` keeps the mesh as it should be
    /// (see the module docs).
    fn valid(&self, from: u32, to: u32) -> bool {
        if from == to
            || self.around[from as usize].is_empty()
            || self.around[to as usize].is_empty()
        {
            return false;
        }
        if !self.movable(from, to) {
            return false;
        }
        let shared: Vec<usize> = self.around[from as usize]
            .iter()
            .copied()
            .filter(|&t| self.triangles[t].contains(&to))
            .collect();
        if shared.is_empty() {
            return false;
        }
        // The link condition: the common neighbours are the third corners
        // of the triangles on the edge.
        let across: HashSet<u32> = shared
            .iter()
            .flat_map(|&t| self.triangles[t])
            .filter(|&w| w != from && w != to)
            .collect();
        let common: HashSet<u32> = self
            .neighbours(from)
            .intersection(&self.neighbours(to))
            .copied()
            .collect();
        if common != across {
            return false;
        }
        // No triangle turned over or left without area.
        for &t in &self.around[from as usize] {
            let tri = self.triangles[t];
            if tri.contains(&to) {
                continue;
            }
            let before = area_normal(&tri.map(|v| self.points[v as usize]));
            let after =
                area_normal(&tri.map(|v| self.points[if v == from { to } else { v } as usize]));
            let (lb, la) = (norm(before), norm(after));
            if la * la <= self.min_area2 || dot(before, after) <= 0.2 * lb * la {
                return false;
            }
        }
        true
    }

    fn candidate(&self, from: u32, to: u32) -> Candidate {
        Candidate {
            cost: quadric_error(&self.quadrics[from as usize], self.points[to as usize]),
            from,
            to,
            stamp: self.stamps[from as usize],
        }
    }

    fn collapse(&mut self, from: u32, to: u32) {
        let at_from = std::mem::take(&mut self.around[from as usize]);
        for t in at_from {
            if self.triangles[t].contains(&to) {
                self.alive[t] = false;
                self.alive_count -= 1;
                for v in self.triangles[t] {
                    if v != from {
                        self.around[v as usize].retain(|&x| x != t);
                    }
                }
            } else {
                for v in &mut self.triangles[t] {
                    if *v == from {
                        *v = to;
                    }
                }
                self.around[to as usize].push(t);
            }
        }
        let q = self.quadrics[from as usize];
        for (a, b) in self.quadrics[to as usize].iter_mut().zip(q) {
            *a += b;
        }
        // Feature edges at `from` now end at `to`.
        let moved = std::mem::take(&mut self.features[from as usize]);
        for w in moved {
            let list = &mut self.features[w as usize];
            list.retain(|&x| x != from);
            if w != to && !list.contains(&to) {
                list.push(to);
                self.features[to as usize].push(w);
            }
        }
        self.features[to as usize].retain(|&x| x != from && x != to);
        self.features[to as usize].sort_unstable();
        self.features[to as usize].dedup();
        self.stamps[to as usize] += 1;
    }
}

/// The mesh decimated (see the module docs).
pub fn decimate(mesh: &TriangleMesh, options: &DecimateOptions) -> Result<Decimated, String> {
    if options.triangles.is_none() && options.tolerance.is_none() {
        return Err("decimation needs a triangle count or a tolerance".to_owned());
    }
    let n = mesh.triangles.len();
    let target = options.triangles.unwrap_or(0);
    let limit = options.tolerance.map_or(f64::INFINITY, |t| t * t);
    let scale = super::size(&mesh.vertices).max(1e-9);
    let mut state = State {
        points: &mesh.vertices,
        triangles: mesh.triangles.clone(),
        alive: vec![true; n],
        alive_count: n,
        around: vec![Vec::new(); mesh.vertices.len()],
        features: vec![Vec::new(); mesh.vertices.len()],
        quadrics: vec![[0.0; 10]; mesh.vertices.len()],
        stamps: vec![0; mesh.vertices.len()],
        min_area2: (1e-12 * scale * scale).powi(2),
    };
    let normals: Vec<Point> = (0..n)
        .map(|t| mitcad_vec3::unit(area_normal(&corners(mesh, t))).unwrap_or([0.0; 3]))
        .collect();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let q = plane_quadric(normals[t], mesh.vertices[tri[0] as usize]);
        for &v in tri {
            state.around[v as usize].push(t);
            for (a, b) in state.quadrics[v as usize].iter_mut().zip(q) {
                *a += b;
            }
        }
    }
    let cos_sharp = options.sharp_angle.cos();
    let mut feature_edges: Vec<(u32, u32)> = edge_triangles(&mesh.triangles)
        .into_iter()
        .filter(|(_, on)| on.len() != 2 || dot(normals[on[0]], normals[on[1]]) < cos_sharp)
        .map(|(key, _)| key)
        .collect();
    feature_edges.sort_unstable();
    for (a, b) in feature_edges {
        state.features[a as usize].push(b);
        state.features[b as usize].push(a);
    }
    let mut heap = BinaryHeap::new();
    let mut edges: Vec<(u32, u32)> = mesh
        .triangles
        .iter()
        .flat_map(|tri| (0..3).map(move |k| edge_key(tri[k], tri[(k + 1) % 3])))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    for (a, b) in edges {
        for (from, to) in [(a, b), (b, a)] {
            if state.movable(from, to) {
                heap.push(state.candidate(from, to));
            }
        }
    }
    let mut error: f64 = 0.0;
    while state.alive_count > target.max(1) {
        let Some(c) = heap.pop() else {
            break;
        };
        if c.cost > limit {
            break;
        }
        if c.stamp != state.stamps[c.from as usize] || !state.valid(c.from, c.to) {
            continue;
        }
        error = error.max(c.cost.sqrt());
        state.collapse(c.from, c.to);
        // Fresh candidates about the vertex kept and its neighbours.
        let near: Vec<u32> = state.neighbours(c.to).into_iter().collect();
        for &w in &near {
            for (from, to) in [(c.to, w), (w, c.to)] {
                if state.movable(from, to) {
                    heap.push(state.candidate(from, to));
                }
            }
        }
        for &w in &near {
            for x in state.neighbours(w) {
                if x != c.to && state.movable(w, x) {
                    heap.push(state.candidate(w, x));
                }
            }
        }
    }
    let out = TriangleMesh {
        vertices: mesh.vertices.clone(),
        triangles: state.triangles,
    };
    let alive = state.alive;
    Ok(Decimated {
        mesh: submesh(&out, |t| alive[t]),
        error,
    })
}
