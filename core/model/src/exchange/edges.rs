// SPDX-License-Identifier: MIT
//! Meshes whose edges are at most a length (mitcad#734,
//! `MeshTolerance::max_edge`): the kernel meshes a body within its surface
//! deviation and normal angle, which leaves long thin triangles on planes
//! and extrusions; a mesh file with a longest edge (for programs that
//! simulate or deform the mesh) gets every longer edge split at its middle.
//!
//! Splitting keeps the mesh's shape: the new points lie on the triangles,
//! so the mesh is as far from the body as before and encloses the same
//! volume. An edge is split in both triangles that share it, so a closed
//! mesh stays closed, and the triangles keep their orientation. The
//! splitting goes round by round until no edge is longer: a triangle with
//! one long edge becomes two, with two three (its quadrilateral cut along
//! the shorter diagonal), with three four. No new edge is longer than the
//! longest edge of its triangle (a median is at most the longer of the
//! sides beside it), so the rounds end.

use std::collections::HashMap;

use super::TriangleMesh;

/// At most this many triangles a body's mesh gets by splitting: a longest
/// edge that would make more is refused before the round that would.
pub(super) const MOST_TRIANGLES: usize = 20_000_000;

/// Rounds at most (each halves the longest edges): far more than any
/// mesh within [`MOST_TRIANGLES`] needs.
const MOST_ROUNDS: usize = 64;

/// The split would make more than `most` triangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TooMany {
    pub most: usize,
}

fn distance_squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

/// The mesh with every edge longer than `max_edge` split at its middle,
/// round by round until none is longer; as it is with `max_edge` 0 (no
/// limit). More than `most` triangles is an error. The export splits with
/// [`split_long_edges_faced`].
#[cfg(test)]
fn split_long_edges(
    mesh: TriangleMesh,
    max_edge: f64,
    most: usize,
) -> Result<TriangleMesh, TooMany> {
    split_long_edges_faced(mesh, Vec::new(), max_edge, most).map(|(mesh, _)| mesh)
}

/// [`split_long_edges`] of a mesh with each triangle's face (`faces`, one
/// per triangle, or empty; mitcad#2385): the pieces of a triangle lie on
/// its face.
pub(super) fn split_long_edges_faced(
    mut mesh: TriangleMesh,
    mut faces: Vec<u32>,
    max_edge: f64,
    most: usize,
) -> Result<(TriangleMesh, Vec<u32>), TooMany> {
    if max_edge <= 0.0 {
        return Ok((mesh, faces));
    }
    let faced = !faces.is_empty();
    let limit = max_edge * max_edge;
    for _ in 0..MOST_ROUNDS {
        // Which edges of each triangle are long: edge i runs from corner i
        // to corner i + 1.
        let long: Vec<[bool; 3]> = mesh
            .triangles
            .iter()
            .map(|t| {
                std::array::from_fn(|i| {
                    let a = mesh.vertices[t[i] as usize];
                    let b = mesh.vertices[t[(i + 1) % 3] as usize];
                    distance_squared(a, b) > limit
                })
            })
            .collect();
        let pieces: usize = long
            .iter()
            .map(|edges| 1 + edges.iter().filter(|l| **l).count())
            .sum();
        if pieces == mesh.triangles.len() {
            return Ok((mesh, faces));
        }
        if pieces > most {
            return Err(TooMany { most });
        }
        // Each long edge's middle, once for both of its triangles.
        let mut middles = Middles {
            vertices: std::mem::take(&mut mesh.vertices),
            at: HashMap::new(),
        };
        let mut triangles = Vec::with_capacity(pieces);
        let mut pieces_faces = Vec::with_capacity(if faced { pieces } else { 0 });
        for (k, (t, edges)) in mesh.triangles.iter().zip(&long).enumerate() {
            let before = triangles.len();
            match edges.iter().filter(|l| **l).count() {
                0 => triangles.push(*t),
                1 => {
                    // p q the long edge, r the corner across it.
                    let i = edges.iter().position(|l| *l).unwrap_or(0);
                    let (p, q, r) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
                    let m = middles.of(p, q);
                    triangles.extend([[p, m, r], [m, q, r]]);
                }
                2 => {
                    // p q the short edge; q r and r p split at m and n.
                    let i = edges.iter().position(|l| !*l).unwrap_or(0);
                    let (p, q, r) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
                    let m = middles.of(q, r);
                    let n = middles.of(r, p);
                    triangles.push([m, r, n]);
                    // The quadrilateral p q m n along its shorter diagonal.
                    triangles.extend(quadrilateral(&middles.vertices, p, q, m, n));
                }
                _ => {
                    let [a, b, c] = *t;
                    let (ab, bc, ca) = (middles.of(a, b), middles.of(b, c), middles.of(c, a));
                    triangles.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
                }
            }
            if faced {
                let face = faces.get(k).copied().unwrap_or_default();
                pieces_faces.resize(pieces_faces.len() + triangles.len() - before, face);
            }
        }
        mesh.vertices = middles.vertices;
        mesh.triangles = triangles;
        if faced {
            faces = pieces_faces;
        }
    }
    Ok((mesh, faces))
}

/// The mesh's points with the middles of the edges split so far.
struct Middles {
    vertices: Vec<[f64; 3]>,
    /// The middle of an edge by its ends, the lower index first.
    at: HashMap<(u32, u32), u32>,
}

impl Middles {
    /// The index of the middle of the edge a b, added when it is new.
    fn of(&mut self, a: u32, b: u32) -> u32 {
        let vertices = &mut self.vertices;
        *self.at.entry((a.min(b), a.max(b))).or_insert_with(|| {
            let (p, q) = (vertices[a as usize], vertices[b as usize]);
            vertices.push(std::array::from_fn(|i| (p[i] + q[i]) / 2.0));
            (vertices.len() - 1) as u32
        })
    }
}

/// The quadrilateral p q m n (counter-clockwise) as two triangles, cut
/// along the shorter of its diagonals p m and q n.
fn quadrilateral(vertices: &[[f64; 3]], p: u32, q: u32, m: u32, n: u32) -> [[u32; 3]; 2] {
    let at = |i: u32| vertices[i as usize];
    if distance_squared(at(p), at(m)) <= distance_squared(at(q), at(n)) {
        [[p, q, m], [p, m, n]]
    } else {
        [[p, q, n], [q, m, n]]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// A box of 12 triangles facing outwards, as the mock kernel meshes
    /// one.
    fn cuboid(size: [f64; 3]) -> TriangleMesh {
        let corner = |i: usize| {
            std::array::from_fn(|axis| {
                if (i >> axis) & 1 == 1 {
                    size[axis]
                } else {
                    0.0
                }
            })
        };
        let quads: [[u32; 4]; 6] = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        TriangleMesh {
            vertices: (0..8).map(corner).collect(),
            triangles: quads
                .iter()
                .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
                .collect(),
        }
    }

    fn volume(mesh: &TriangleMesh) -> f64 {
        mesh.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
                let cross = [
                    b[1] * c[2] - b[2] * c[1],
                    b[2] * c[0] - b[0] * c[2],
                    b[0] * c[1] - b[1] * c[0],
                ];
                (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0
            })
            .sum()
    }

    fn longest(mesh: &TriangleMesh) -> f64 {
        mesh.triangles
            .iter()
            .flat_map(|t| (0..3).map(move |i| (t[i], t[(i + 1) % 3])))
            .map(|(a, b)| {
                distance_squared(mesh.vertices[a as usize], mesh.vertices[b as usize]).sqrt()
            })
            .fold(0.0, f64::max)
    }

    /// Every edge is met once each way: the mesh is closed and its
    /// triangles agree in orientation.
    fn closed(mesh: &TriangleMesh) -> bool {
        let mut directed: BTreeMap<(u32, u32), i32> = BTreeMap::new();
        for t in &mesh.triangles {
            for i in 0..3 {
                *directed.entry((t[i], t[(i + 1) % 3])).or_default() += 1;
            }
        }
        directed
            .iter()
            .all(|((a, b), n)| *n == 1 && directed.get(&(*b, *a)) == Some(&1))
    }

    /// The pieces of a split triangle keep its face (mitcad#2385): each
    /// side of the box its own face, every piece within its side's plane.
    #[test]
    fn split_triangles_keep_their_faces() {
        let size = [10.0, 20.0, 30.0];
        let box_mesh = cuboid(size);
        let faces: Vec<u32> = (0..12).map(|t| t / 2).collect();
        let (unsplit, same) =
            split_long_edges_faced(box_mesh.clone(), faces.clone(), 0.0, MOST_TRIANGLES).unwrap();
        assert_eq!((unsplit, same), (box_mesh.clone(), faces.clone()));
        let (split, split_faces) =
            split_long_edges_faced(box_mesh.clone(), faces, 3.0, MOST_TRIANGLES).unwrap();
        assert_eq!(split.triangles.len(), split_faces.len());
        assert!(split.triangles.len() > box_mesh.triangles.len());
        // The sides in turn: -Z, +Z, -Y, +Y, -X, +X.
        for (t, face) in split.triangles.iter().zip(&split_faces) {
            let axis = [2, 2, 1, 1, 0, 0][*face as usize];
            let at = if face % 2 == 0 { 0.0 } else { size[axis] };
            for v in t {
                let p = split.vertices[*v as usize];
                assert!((p[axis] - at).abs() <= 1e-9, "{p:?} is off face {face}");
            }
        }
        // Without faces, none.
        let (_, none) = split_long_edges_faced(box_mesh, Vec::new(), 3.0, MOST_TRIANGLES).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn no_limit_keeps_the_mesh() {
        let mesh = cuboid([10.0, 20.0, 30.0]);
        assert_eq!(
            split_long_edges(mesh.clone(), 0.0, MOST_TRIANGLES).unwrap(),
            mesh
        );
        // Edges already short enough stay too.
        assert_eq!(
            split_long_edges(mesh.clone(), 100.0, MOST_TRIANGLES).unwrap(),
            mesh
        );
    }

    #[test]
    fn long_edges_are_split_keeping_the_shape() {
        let box_mesh = cuboid([10.0, 20.0, 30.0]);
        let split = split_long_edges(box_mesh.clone(), 3.0, MOST_TRIANGLES).unwrap();
        assert!(
            longest(&split) <= 3.0 + 1e-9,
            "longest edge {}",
            longest(&split)
        );
        assert!(split.triangles.len() > box_mesh.triangles.len());
        assert!(
            (volume(&split) - 6000.0).abs() <= 1e-6,
            "volume {}",
            volume(&split)
        );
        assert!(
            closed(&split),
            "the split mesh has gaps or flipped triangles"
        );
        // Every new point lies on the box's surface.
        for v in &split.vertices {
            let on_side =
                (0..3).any(|i| v[i].abs() <= 1e-9 || (v[i] - [10.0, 20.0, 30.0][i]).abs() <= 1e-9);
            assert!(on_side, "{v:?} is off the box");
        }
        // A thin sliver too: one long edge and two short ones.
        let sliver = TriangleMesh {
            vertices: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [5.0, 0.5, 0.0]],
            triangles: vec![[0, 1, 2]],
        };
        let split = split_long_edges(sliver, 1.0, MOST_TRIANGLES).unwrap();
        assert!(
            longest(&split) <= 1.0 + 1e-9,
            "longest edge {}",
            longest(&split)
        );
        let areas: Vec<f64> = split
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| split.vertices[i as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2.0
            })
            .collect();
        // Every triangle counter-clockwise as before, the same area.
        assert!(areas.iter().all(|a| *a > 0.0), "{areas:?}");
        let area: f64 = areas.iter().sum();
        assert!((area - 2.5).abs() <= 1e-9, "area {area}");
    }

    #[test]
    fn too_many_triangles_are_refused() {
        let error = split_long_edges(cuboid([10.0, 10.0, 10.0]), 0.01, 10_000).unwrap_err();
        assert_eq!(error, TooMany { most: 10_000 });
    }
}
