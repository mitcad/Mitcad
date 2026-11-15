// SPDX-License-Identifier: MIT
//! Catmull-Clark subdivision of a polygon mesh with creases, corners and
//! boundaries, and the limit positions of its vertices.
//!
//! One step ([`subdivide`]) puts a face point at the centre of each face,
//! an edge point on each edge and moves each vertex; each face of `n`
//! corners becomes `n` quads, quad `i` at corner `i`: (the corner's vertex
//! point, the edge point of the edge to the next corner, the face point,
//! the edge point of the edge from the corner before). The new vertices are
//! numbered vertex points first (vertex `v` keeps its number), then edge
//! points, then face points ([`Step`]).
//!
//! The rules (Catmull and Clark 1978; creases after DeRose, Kass and Truong
//! 1998):
//!
//! - an edge point is the mean of the edge's ends and its two faces' points
//!   on a smooth edge, the edge's midpoint on a creased edge of sharpness 1
//!   or more and on a boundary edge (an edge of one face, sharp for good),
//!   the two blended by the sharpness below 1;
//! - a vertex with fewer than two creased edges moves by the smooth rule
//!   `(Q + 2R + (n - 3) P) / n` (`Q` the mean of its faces' points, `R` of
//!   its edges' midpoints, `n` its edges); one with two by the crease rule
//!   `(A + 6 P + B) / 8` (`A`, `B` the creased edges' other ends); one with
//!   more, a corner and a boundary vertex of one face stay where they are;
//!   the vertex's sharpness (the mean of its creased edges') blends the
//!   smooth and the sharp rule below 1;
//! - the halves of a creased edge are one less sharp; a sharp crease stays
//!   sharp.
//!
//! The limit positions ([`limit_points`]) are those of a mesh of quads (one
//! step or more): `(n² P + 4 Σ edge neighbours + Σ diagonal neighbours) /
//! (n (n + 5))` for a smooth vertex, `(A + 4 P + B) / 6` on a crease, the
//! vertex itself at a corner.

use std::collections::HashMap;

use mitcad_vec3::{Vec3, add, scale, sub};

/// A polygon mesh: the points, the faces as loops of point indices, the
/// creased edges (by [`key`]) with their sharpness (infinite: sharp for
/// good) and the corner vertices.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub points: Vec<Vec3>,
    pub faces: Vec<Vec<usize>>,
    pub creases: HashMap<(usize, usize), f64>,
    pub corners: Vec<bool>,
}

/// An edge's key: its two vertices, the smaller first.
pub fn key(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

fn lerp(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    add(a, scale(sub(b, a), t))
}

fn mean(points: impl Iterator<Item = Vec3>) -> Vec3 {
    let mut sum = [0.0; 3];
    let mut count = 0usize;
    for p in points {
        sum = add(sum, p);
        count += 1;
    }
    if count == 0 {
        sum
    } else {
        scale(sum, 1.0 / count as f64)
    }
}

/// The edges of a mesh and what meets at each vertex.
#[derive(Debug, Clone, Default)]
pub struct Topology {
    /// Each edge's ends, the smaller first, in the order the faces meet
    /// them.
    pub edges: Vec<(usize, usize)>,
    pub edge_index: HashMap<(usize, usize), usize>,
    /// The faces at each edge.
    pub edge_faces: Vec<Vec<usize>>,
    pub vertex_edges: Vec<Vec<usize>>,
    pub vertex_faces: Vec<Vec<usize>>,
}

impl Topology {
    pub fn of(mesh: &Mesh) -> Self {
        let mut topology = Topology {
            vertex_edges: vec![Vec::new(); mesh.points.len()],
            vertex_faces: vec![Vec::new(); mesh.points.len()],
            ..Topology::default()
        };
        for (f, face) in mesh.faces.iter().enumerate() {
            let n = face.len();
            for (i, &a) in face.iter().enumerate() {
                topology.vertex_faces[a].push(f);
                let b = face[(i + 1) % n];
                let k = key(a, b);
                let e = match topology.edge_index.get(&k) {
                    Some(&e) => e,
                    None => {
                        let e = topology.edges.len();
                        topology.edges.push(k);
                        topology.edge_faces.push(Vec::new());
                        topology.edge_index.insert(k, e);
                        topology.vertex_edges[k.0].push(e);
                        topology.vertex_edges[k.1].push(e);
                        e
                    }
                };
                topology.edge_faces[e].push(f);
            }
        }
        topology
    }

    pub fn edge(&self, a: usize, b: usize) -> Option<usize> {
        self.edge_index.get(&key(a, b)).copied()
    }

    /// The face on the edge `a`-`b` other than `face`.
    pub fn other_face(&self, a: usize, b: usize, face: usize) -> Option<usize> {
        let e = self.edge(a, b)?;
        self.edge_faces[e].iter().copied().find(|f| *f != face)
    }

    /// Whether every edge has two faces.
    pub fn is_closed(&self) -> bool {
        self.edge_faces.iter().all(|f| f.len() == 2)
    }

    /// The edge's other end.
    pub fn other_end(&self, e: usize, v: usize) -> usize {
        let (a, b) = self.edges[e];
        if a == v { b } else { a }
    }
}

impl Mesh {
    /// The edge's sharpness: infinite on a boundary (or an edge of more
    /// than two faces), its crease's, else zero.
    pub fn edge_sharpness(&self, topology: &Topology, e: usize) -> f64 {
        if topology.edge_faces[e].len() != 2 {
            return f64::INFINITY;
        }
        self.creases.get(&topology.edges[e]).copied().unwrap_or(0.0)
    }

    /// The creased edges at a vertex (the other end and the sharpness) and
    /// whether it is a corner (marked, or a boundary vertex of one face).
    fn sharp_edges(&self, topology: &Topology, v: usize) -> (Vec<(usize, f64)>, bool) {
        let sharp: Vec<(usize, f64)> = topology.vertex_edges[v]
            .iter()
            .map(|&e| (topology.other_end(e, v), self.edge_sharpness(topology, e)))
            .filter(|(_, s)| *s > 0.0)
            .collect();
        let corner = self.corners.get(v).copied().unwrap_or(false)
            || (topology.vertex_faces[v].len() == 1 && sharp.len() >= 2);
        (sharp, corner)
    }

    /// Whether a vertex is regular: inside, four edges and four quads, no
    /// crease at it and no corner.
    pub fn is_regular_vertex(&self, topology: &Topology, v: usize) -> bool {
        !self.corners.get(v).copied().unwrap_or(false)
            && topology.vertex_edges[v].len() == 4
            && topology.vertex_faces[v].len() == 4
            && topology.vertex_faces[v]
                .iter()
                .all(|f| self.faces[*f].len() == 4)
            && topology.vertex_edges[v]
                .iter()
                .all(|e| self.edge_sharpness(topology, *e) == 0.0)
    }
}

/// How a step numbered what it made (see the module).
#[derive(Debug, Clone, Default)]
pub struct Step {
    /// The coarse mesh's vertex count: the first edge point's number.
    pub vertices: usize,
    /// The coarse mesh's edge count.
    pub edges: usize,
    /// The coarse mesh's edges.
    pub edge_index: HashMap<(usize, usize), usize>,
    /// The first child of each coarse face.
    pub child_start: Vec<usize>,
}

impl Step {
    /// The edge point of the coarse edge `a`-`b`.
    pub fn edge_point(&self, a: usize, b: usize) -> usize {
        self.vertices + self.edge_index[&key(a, b)]
    }

    /// The face point of the coarse face `f`.
    pub fn face_point(&self, f: usize) -> usize {
        self.vertices + self.edges + f
    }

    /// The child of the coarse face `f` at its corner `corner` (a vertex).
    pub fn child(&self, coarse: &Mesh, f: usize, corner: usize) -> usize {
        let at = coarse.faces[f]
            .iter()
            .position(|v| *v == corner)
            .expect("a corner of the face");
        self.child_start[f] + at
    }
}

/// One Catmull-Clark step (see the module).
pub fn subdivide(mesh: &Mesh) -> (Mesh, Step) {
    let topology = Topology::of(mesh);
    let nv = mesh.points.len();
    let ne = topology.edges.len();
    let face_points: Vec<Vec3> = mesh
        .faces
        .iter()
        .map(|face| mean(face.iter().map(|v| mesh.points[*v])))
        .collect();
    let sharpness: Vec<f64> = (0..ne).map(|e| mesh.edge_sharpness(&topology, e)).collect();
    let mut points = Vec::with_capacity(nv + ne + face_points.len());
    for v in 0..nv {
        points.push(vertex_point(mesh, &topology, &face_points, v));
    }
    for (e, &(a, b)) in topology.edges.iter().enumerate() {
        let (pa, pb) = (mesh.points[a], mesh.points[b]);
        let mid = scale(add(pa, pb), 0.5);
        let s = sharpness[e];
        points.push(if s >= 1.0 {
            mid
        } else {
            let faces = &topology.edge_faces[e];
            let smooth = scale(
                add(
                    add(pa, pb),
                    add(face_points[faces[0]], face_points[faces[1]]),
                ),
                0.25,
            );
            if s > 0.0 {
                lerp(smooth, mid, s)
            } else {
                smooth
            }
        });
    }
    points.extend(face_points.iter().copied());
    let mut faces = Vec::new();
    let mut child_start = Vec::with_capacity(mesh.faces.len());
    for (f, face) in mesh.faces.iter().enumerate() {
        child_start.push(faces.len());
        let n = face.len();
        for (i, &v) in face.iter().enumerate() {
            let next = face[(i + 1) % n];
            let prev = face[(i + n - 1) % n];
            faces.push(vec![
                v,
                nv + topology.edge_index[&key(v, next)],
                nv + ne + f,
                nv + topology.edge_index[&key(prev, v)],
            ]);
        }
    }
    let mut creases = HashMap::new();
    for (&(a, b), &s) in &mesh.creases {
        let Some(&e) = topology.edge_index.get(&(a, b)) else {
            continue;
        };
        let child = if s.is_infinite() { s } else { s - 1.0 };
        if child > 0.0 {
            creases.insert(key(a, nv + e), child);
            creases.insert(key(nv + e, b), child);
        }
    }
    let mut corners = mesh.corners.clone();
    corners.resize(points.len(), false);
    let step = Step {
        vertices: nv,
        edges: ne,
        edge_index: topology.edge_index,
        child_start,
    };
    (
        Mesh {
            points,
            faces,
            creases,
            corners,
        },
        step,
    )
}

/// A vertex's sharpness from its creased edges: their mean, infinite when
/// one is sharp for good or the vertex is a corner.
fn vertex_sharpness(sharp: &[(usize, f64)], corner: bool) -> f64 {
    if corner || sharp.iter().any(|(_, s)| s.is_infinite()) {
        return f64::INFINITY;
    }
    sharp.iter().map(|(_, s)| s).sum::<f64>() / sharp.len().max(1) as f64
}

fn vertex_point(mesh: &Mesh, topology: &Topology, face_points: &[Vec3], v: usize) -> Vec3 {
    let p = mesh.points[v];
    if topology.vertex_faces[v].is_empty() {
        return p;
    }
    let (sharp, corner) = mesh.sharp_edges(topology, v);
    let smooth = || {
        let n = topology.vertex_edges[v].len() as f64;
        let q = mean(topology.vertex_faces[v].iter().map(|f| face_points[*f]));
        let r = mean(topology.vertex_edges[v].iter().map(|e| {
            let (a, b) = topology.edges[*e];
            scale(add(mesh.points[a], mesh.points[b]), 0.5)
        }));
        scale(add(add(q, scale(r, 2.0)), scale(p, n - 3.0)), 1.0 / n)
    };
    if !corner && sharp.len() < 2 {
        return smooth();
    }
    let sharp_rule = if corner || sharp.len() > 2 {
        p
    } else {
        let (a, b) = (mesh.points[sharp[0].0], mesh.points[sharp[1].0]);
        scale(add(add(a, b), scale(p, 6.0)), 1.0 / 8.0)
    };
    let s = vertex_sharpness(&sharp, corner);
    if s >= 1.0 {
        sharp_rule
    } else {
        lerp(smooth(), sharp_rule, s)
    }
}

/// The limit position of every vertex of a mesh of quads (see the
/// module); a face of another count counts its centre as the diagonal
/// neighbour.
pub fn limit_points(mesh: &Mesh) -> Vec<Vec3> {
    let topology = Topology::of(mesh);
    (0..mesh.points.len())
        .map(|v| limit_point(mesh, &topology, v))
        .collect()
}

fn limit_point(mesh: &Mesh, topology: &Topology, v: usize) -> Vec3 {
    let p = mesh.points[v];
    if topology.vertex_faces[v].is_empty() {
        return p;
    }
    let (sharp, corner) = mesh.sharp_edges(topology, v);
    let smooth = || {
        let n = topology.vertex_edges[v].len() as f64;
        let mut edge_sum = [0.0; 3];
        for &e in &topology.vertex_edges[v] {
            edge_sum = add(edge_sum, mesh.points[topology.other_end(e, v)]);
        }
        let mut diagonal_sum = [0.0; 3];
        for &f in &topology.vertex_faces[v] {
            let face = &mesh.faces[f];
            let diagonal = if face.len() == 4 {
                let at = face.iter().position(|x| *x == v).unwrap_or(0);
                mesh.points[face[(at + 2) % 4]]
            } else {
                mean(face.iter().map(|x| mesh.points[*x]))
            };
            diagonal_sum = add(diagonal_sum, diagonal);
        }
        scale(
            add(add(scale(p, n * n), scale(edge_sum, 4.0)), diagonal_sum),
            1.0 / (n * (n + 5.0)),
        )
    };
    if !corner && sharp.len() < 2 {
        return smooth();
    }
    let sharp_limit = if corner || sharp.len() > 2 {
        p
    } else {
        let (a, b) = (mesh.points[sharp[0].0], mesh.points[sharp[1].0]);
        scale(add(add(a, b), scale(p, 4.0)), 1.0 / 6.0)
    };
    let s = vertex_sharpness(&sharp, corner);
    if s >= 1.0 {
        sharp_limit
    } else {
        lerp(smooth(), sharp_limit, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of `n` x `n` quads on z = 0, unit spacing, `(i, j)` at index
    /// `i (n + 1) + j`.
    fn grid(n: usize) -> Mesh {
        let mut points = Vec::new();
        for i in 0..=n {
            for j in 0..=n {
                points.push([i as f64, j as f64, 0.0]);
            }
        }
        let at = |i: usize, j: usize| i * (n + 1) + j;
        let mut faces = Vec::new();
        for i in 0..n {
            for j in 0..n {
                faces.push(vec![at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        let corners = vec![false; points.len()];
        Mesh {
            points,
            faces,
            creases: HashMap::new(),
            corners,
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() < 1e-12)
    }

    #[test]
    fn a_step_numbers_vertex_edge_and_face_points() {
        let mesh = grid(1);
        let (fine, step) = subdivide(&mesh);
        assert_eq!(fine.points.len(), 4 + 4 + 1);
        assert_eq!(fine.faces.len(), 4);
        assert_eq!(step.face_point(0), 8);
        // Child 0 is at corner 0: the corner, the edge to corner 1, the
        // face point, the edge from corner 3.
        assert_eq!(fine.faces[0][0], 0);
        assert_eq!(fine.faces[0][1], step.edge_point(0, 2));
        assert_eq!(fine.faces[0][2], 8);
        assert_eq!(fine.faces[0][3], step.edge_point(1, 0));
        assert_eq!(step.child(&mesh, 0, 3), 2);
    }

    /// Inside a regular grid the surface is the uniform bicubic B-spline:
    /// a flat grid stays flat and a vertex's limit is its B-spline point.
    #[test]
    fn limit_points_on_a_regular_grid_are_the_b_spline_points() {
        let mut mesh = grid(6);
        // Lift the middle vertex: its limit is 16/36 of the lift, its edge
        // neighbours' 4/36, its diagonal neighbours' 1/36.
        let middle = 3 * 7 + 3;
        mesh.points[middle][2] = 36.0;
        let limits = limit_points(&mesh);
        assert!(near(limits[middle], [3.0, 3.0, 16.0]));
        assert!(near(limits[middle + 1], [3.0, 4.0, 4.0]));
        assert!(near(limits[middle + 7 + 1], [4.0, 4.0, 1.0]));
        // A step changes no limit point of an old vertex.
        let (fine, _) = subdivide(&mesh);
        let fine_limits = limit_points(&fine);
        for v in [middle, middle + 1, middle + 8, 2 * 7 + 2] {
            assert!(near(fine_limits[v], limits[v]), "{v}");
        }
    }

    /// A sharp crease along a line of the grid: its vertices follow the
    /// cubic B-spline of the crease (1, 4, 1) / 6 and its edge points are
    /// the midpoints.
    #[test]
    fn a_crease_keeps_its_curve() {
        let mut mesh = grid(4);
        let at = |i: usize, j: usize| i * 5 + j;
        for i in 0..4 {
            mesh.creases
                .insert(key(at(i, 2), at(i + 1, 2)), f64::INFINITY);
        }
        for i in 0..=4 {
            mesh.points[at(i, 2)][2] = 6.0;
        }
        // Raise one crease vertex further.
        mesh.points[at(2, 2)][2] = 12.0;
        let limits = limit_points(&mesh);
        assert!(near(limits[at(2, 2)], [2.0, 2.0, (6.0 + 48.0 + 6.0) / 6.0]));
        let (fine, step) = subdivide(&mesh);
        let edge = step.edge_point(at(1, 2), at(2, 2));
        assert!(near(fine.points[edge], [1.5, 2.0, 9.0]));
        // The crease stays sharp on the halves.
        assert_eq!(
            fine.creases.get(&key(at(1, 2), edge)).copied(),
            Some(f64::INFINITY)
        );
        // A semi-sharp crease of 1 is gone after a step.
        let mut soft = grid(4);
        soft.creases.insert(key(at(1, 2), at(2, 2)), 1.0);
        let (fine, _) = subdivide(&soft);
        assert!(fine.creases.is_empty());
    }

    /// The boundary of an open grid follows the cubic B-spline of its
    /// boundary vertices; a boundary vertex of one face stays put.
    #[test]
    fn boundaries_follow_their_curves_and_corners_stay() {
        let mut mesh = grid(4);
        let at = |i: usize, j: usize| i * 5 + j;
        mesh.points[at(2, 0)][2] = 6.0;
        let limits = limit_points(&mesh);
        assert!(near(limits[at(2, 0)], [2.0, 0.0, 4.0]));
        assert!(near(limits[at(1, 0)], [1.0, 0.0, 1.0]));
        assert!(near(limits[at(0, 0)], [0.0, 0.0, 0.0]));
        let (fine, _) = subdivide(&mesh);
        assert!(near(fine.points[at(0, 0)], [0.0, 0.0, 0.0]));
        assert!(near(
            fine.points[at(2, 0)],
            [2.0, 0.0, (1.0 * 0.0 + 36.0 + 0.0) / 8.0]
        ));
    }
}
