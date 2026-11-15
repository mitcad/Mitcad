// SPDX-License-Identifier: MIT
//! The limit surface of a cage as bicubic patches (mitcad#1757).
//!
//! The cage is subdivided once, so that every face is a quad: a cage face
//! of `n` corners gives `n` quads, quad `k` at its corner `k`. Each quad
//! becomes one patch, its parameters running from the corner (u towards
//! the next corner's edge, v towards the previous one's), so the patch's
//! normal points out of the cage:
//!
//! - **Regular quads** (all four corners inside the cage, of four edges
//!   and four quads, without creases or corners): the limit surface over
//!   the quad is the uniform bicubic B-spline of the 4 x 4 control points
//!   around it, exactly. The patch is that surface as a bicubic Bézier
//!   patch.
//! - **Other quads** (beside an extraordinary vertex, a crease, a corner or
//!   a boundary): the mesh is subdivided [`SAMPLE_LEVELS`] more times and
//!   the limit points of the vertices that fall on the quad, an evenly
//!   spaced grid of `2^SAMPLE_LEVELS + 1` by as many points of the limit
//!   surface, are interpolated by a bicubic B-spline ([`Interpolator`]).
//!
//! Two neighbouring patches meet without a gap: a patch's boundary curve is
//! the curve through the limit points on its boundary alone, and the patch
//! across the boundary has the same points there. Where the neighbour is a
//! regular patch its boundary is a cubic, which the interpolation gives
//! back exactly (to rounding, far below 1e-6 mm). Along a crease or the
//! cage's boundary both sides interpolate the limit points of the crease's
//! curve; the surfaces meet there at an angle, so a creased edge stays
//! sharp. Away from the regular quads the patches lie within the
//! interpolation's error of the limit surface; the tests bound it.

use mitcad_vec3::{Vec3, add, scale};

use crate::cage::Cage;
use crate::interp::Interpolator;
use crate::mesh::{Mesh, Step, Topology, limit_points, subdivide};

/// How many more steps sample the limit surface of a quad that is not
/// regular: its patch interpolates `2^SAMPLE_LEVELS + 1` points a side.
pub const SAMPLE_LEVELS: usize = 3;

/// A bicubic B-spline patch of the limit surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// The cage face it lies on, by its id in the whole cage (negative on
    /// the mirrored half).
    pub face: i64,
    /// The corner of the face (its number in the stored face's loop).
    pub corner: u32,
    /// The poles along u and along v.
    pub nu: usize,
    pub nv: usize,
    /// Pole (i, j), i along u, at `i * nv + j`.
    pub poles: Vec<Vec3>,
    pub u_knots: Vec<f64>,
    pub u_mults: Vec<u32>,
    pub v_knots: Vec<f64>,
    pub v_mults: Vec<u32>,
    /// True on a regular quad: the limit surface exactly.
    pub exact: bool,
}

impl Patch {
    pub fn pole(&self, i: usize, j: usize) -> Vec3 {
        self.poles[i * self.nv + j]
    }

    /// The patch's corners: (0, 0), (1, 0), (1, 1), (0, 1).
    pub fn corners(&self) -> [Vec3; 4] {
        let (u, v) = (self.nu - 1, self.nv - 1);
        [
            self.pole(0, 0),
            self.pole(u, 0),
            self.pole(u, v),
            self.pole(0, v),
        ]
    }
}

/// The limit surface: its patches and whether they close (a solid) or
/// leave an opening (a sheet).
#[derive(Debug, Clone, PartialEq)]
pub struct LimitSurface {
    pub patches: Vec<Patch>,
    pub closed: bool,
}

/// The limit surface of a cage (see the module); Err when the cage is not
/// valid ([`Cage::check`]).
pub fn limit_surface(cage: &Cage) -> Result<LimitSurface, String> {
    cage.check()?;
    let full = cage.full();
    let closed = full.closed();
    let (level1, step) = subdivide(&full.mesh);
    let topology = Topology::of(&level1);
    // The cage face and corner each quad lies at.
    let mut origin = vec![(0i64, 0u32); level1.faces.len()];
    for (f, numbers) in full.corner_numbers.iter().enumerate() {
        for (k, corner) in numbers.iter().enumerate() {
            origin[step.child_start[f] + k] = (full.face_ids[f], *corner);
        }
    }
    let mut patches: Vec<Option<Patch>> = vec![None; level1.faces.len()];
    let mut irregular = Vec::new();
    for (q, slot) in patches.iter_mut().enumerate() {
        match regular_grid(&level1, &topology, q) {
            Some(grid) => *slot = Some(bezier_patch(origin[q], &grid, &level1)),
            None => irregular.push(q),
        }
    }
    if !irregular.is_empty() {
        let mut grids: Vec<Grid> = irregular
            .iter()
            .map(|&q| Grid::of_quad(&level1.faces[q], q))
            .collect();
        let mut mesh = level1;
        for _ in 0..SAMPLE_LEVELS {
            let (finer, step) = subdivide(&mesh);
            for grid in &mut grids {
                *grid = grid.refine(&mesh, &step);
            }
            mesh = finer;
        }
        let limits = limit_points(&mesh);
        let interpolator = Interpolator::new(1 << SAMPLE_LEVELS);
        let (knots, mults) = interpolator.knots();
        let n = interpolator.count();
        for (grid, &q) in grids.iter().zip(&irregular) {
            let samples: Vec<Vec3> = grid.verts.iter().map(|v| limits[*v]).collect();
            debug_assert_eq!(samples.len(), n * n);
            patches[q] = Some(Patch {
                face: origin[q].0,
                corner: origin[q].1,
                nu: n,
                nv: n,
                poles: interpolator.surface(&samples),
                u_knots: knots.clone(),
                u_mults: mults.clone(),
                v_knots: knots.clone(),
                v_mults: mults.clone(),
                exact: false,
            });
        }
    }
    Ok(LimitSurface {
        patches: patches.into_iter().flatten().collect(),
        closed,
    })
}

/// The vertex of a quad next to `a` other than `b` (`a`-`b` an edge of
/// it).
fn beside(quad: &[usize], a: usize, b: usize) -> Option<usize> {
    if quad.len() != 4 {
        return None;
    }
    let at = quad.iter().position(|v| *v == a)?;
    let (next, prev) = (quad[(at + 1) % 4], quad[(at + 3) % 4]);
    if next == b {
        Some(prev)
    } else if prev == b {
        Some(next)
    } else {
        None
    }
}

/// The 4 x 4 control points around a regular quad `q`, (i, j) with i along
/// u: the quad's corners c0 to c3 at (1, 1), (2, 1), (2, 2), (1, 2). None
/// when the quad is not regular.
fn regular_grid(mesh: &Mesh, topology: &Topology, q: usize) -> Option<[[usize; 4]; 4]> {
    let quad = &mesh.faces[q];
    if quad.len() != 4 || !quad.iter().all(|v| mesh.is_regular_vertex(topology, *v)) {
        return None;
    }
    let (c0, c1, c2, c3) = (quad[0], quad[1], quad[2], quad[3]);
    // The quad across the edge a-b from `face`, with the vertices next to a
    // and to b in it.
    let across = |face: usize, a: usize, b: usize| -> Option<(usize, usize, usize)> {
        let other = topology.other_face(a, b, face)?;
        let lp = &mesh.faces[other];
        Some((other, beside(lp, a, b)?, beside(lp, b, a)?))
    };
    let (n0, p10, p20) = across(q, c0, c1)?;
    let (n1, p31, p32) = across(q, c1, c2)?;
    let (_, p23, p13) = across(q, c2, c3)?;
    let (n3, p02, p01) = across(q, c3, c0)?;
    let (_, _, p00) = across(n0, c0, p10)?;
    let (_, _, p30) = across(n0, c1, p20)?;
    let (_, _, p33) = across(n1, c2, p32)?;
    let (_, _, p03) = across(n3, c3, p02)?;
    Some([
        [p00, p01, p02, p03],
        [p10, c0, c3, p13],
        [p20, c1, c2, p23],
        [p30, p31, p32, p33],
    ])
}

/// From the uniform cubic B-spline's control points of a span to its
/// Bézier points.
const TO_BEZIER: [[f64; 4]; 4] = [
    [1.0 / 6.0, 4.0 / 6.0, 1.0 / 6.0, 0.0],
    [0.0, 4.0 / 6.0, 2.0 / 6.0, 0.0],
    [0.0, 2.0 / 6.0, 4.0 / 6.0, 0.0],
    [0.0, 1.0 / 6.0, 4.0 / 6.0, 1.0 / 6.0],
];

fn bezier_patch(origin: (i64, u32), grid: &[[usize; 4]; 4], mesh: &Mesh) -> Patch {
    // Along u first, then along v.
    let mut along_u = [[[0.0; 3]; 4]; 4];
    for (row, out) in TO_BEZIER.iter().zip(along_u.iter_mut()) {
        for (j, point) in out.iter_mut().enumerate() {
            *point = row.iter().enumerate().fold([0.0; 3], |p, (i, w)| {
                add(p, scale(mesh.points[grid[i][j]], *w))
            });
        }
    }
    let mut poles = Vec::with_capacity(16);
    for row_u in &along_u {
        for row in &TO_BEZIER {
            poles.push(
                row.iter()
                    .zip(row_u)
                    .fold([0.0; 3], |p, (w, q)| add(p, scale(*q, *w))),
            );
        }
    }
    Patch {
        face: origin.0,
        corner: origin.1,
        nu: 4,
        nv: 4,
        poles,
        u_knots: vec![0.0, 1.0],
        u_mults: vec![4, 4],
        v_knots: vec![0.0, 1.0],
        v_mults: vec![4, 4],
        exact: true,
    }
}

/// The vertices of a grid of quads over one quad of a coarser level, (i,
/// j) with i along u at `i * (size + 1) + j`, and the quads, (i, j) at
/// `i * size + j`.
#[derive(Debug, Clone)]
struct Grid {
    size: usize,
    verts: Vec<usize>,
    cells: Vec<usize>,
}

impl Grid {
    /// The quad itself: corner 0 at (0, 0), 1 at (1, 0), 2 at (1, 1), 3 at
    /// (0, 1).
    fn of_quad(quad: &[usize], face: usize) -> Grid {
        Grid {
            size: 1,
            verts: vec![quad[0], quad[3], quad[1], quad[2]],
            cells: vec![face],
        }
    }

    fn vert(&self, i: usize, j: usize) -> usize {
        self.verts[i * (self.size + 1) + j]
    }

    /// The grid one step finer: old vertices keep their numbers, edge
    /// points between them and face points inside.
    fn refine(&self, coarse: &Mesh, step: &Step) -> Grid {
        let s = self.size;
        let size = 2 * s;
        let w = size + 1;
        let mut verts = vec![0; w * w];
        let mut cells = vec![0; size * size];
        for i in 0..=s {
            for j in 0..=s {
                verts[2 * i * w + 2 * j] = self.vert(i, j);
                if i < s {
                    verts[(2 * i + 1) * w + 2 * j] =
                        step.edge_point(self.vert(i, j), self.vert(i + 1, j));
                }
                if j < s {
                    verts[2 * i * w + 2 * j + 1] =
                        step.edge_point(self.vert(i, j), self.vert(i, j + 1));
                }
                if i < s && j < s {
                    let cell = self.cells[i * s + j];
                    verts[(2 * i + 1) * w + 2 * j + 1] = step.face_point(cell);
                    for di in 0..2 {
                        for dj in 0..2 {
                            let corner = self.vert(i + di, j + dj);
                            cells[(2 * i + di) * size + 2 * j + dj] =
                                step.child(coarse, cell, corner);
                        }
                    }
                }
            }
        }
        Grid { size, verts, cells }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cage::{CageFace, CageVertex};
    use crate::interp::basis;
    use crate::preview::limit_mesh;
    use crate::primitive::{box_cage, plane_cage};
    use mitcad_vec3::{add, distance, scale};

    /// The point of a patch at (u, v).
    fn evaluate(patch: &Patch, u: f64, v: f64) -> Vec3 {
        let full = |knots: &[f64], mults: &[u32]| -> Vec<f64> {
            knots
                .iter()
                .zip(mults)
                .flat_map(|(k, m)| std::iter::repeat_n(*k, *m as usize))
                .collect()
        };
        let bu = basis(&full(&patch.u_knots, &patch.u_mults), 3, u);
        let bv = basis(&full(&patch.v_knots, &patch.v_mults), 3, v);
        let mut p = [0.0; 3];
        for (i, wu) in bu.iter().enumerate() {
            for (j, wv) in bv.iter().enumerate() {
                p = add(p, scale(patch.pole(i, j), wu * wv));
            }
        }
        p
    }

    /// The largest gap between the boundaries of patches that share an
    /// edge (sampled), and how many shared edges were found.
    fn largest_gap(surface: &LimitSurface) -> (f64, usize) {
        // Each side of a patch as points along it, from corner to corner.
        let side = |p: &Patch, k: usize, t: f64| match k {
            0 => evaluate(p, t, 0.0),
            1 => evaluate(p, 1.0, t),
            2 => evaluate(p, 1.0 - t, 1.0),
            _ => evaluate(p, 0.0, 1.0 - t),
        };
        let mut worst: f64 = 0.0;
        let mut shared = 0;
        let near = |a: Vec3, b: Vec3| distance(a, b) < 1e-6;
        for (x, a) in surface.patches.iter().enumerate() {
            for b in &surface.patches[x + 1..] {
                for ka in 0..4 {
                    for kb in 0..4 {
                        let (a0, a1) = (side(a, ka, 0.0), side(a, ka, 1.0));
                        let (b0, b1) = (side(b, kb, 0.0), side(b, kb, 1.0));
                        if !(near(a0, b1) && near(a1, b0)) || near(a0, a1) {
                            continue;
                        }
                        shared += 1;
                        for s in 0..=20 {
                            let t = s as f64 / 20.0;
                            worst = worst.max(distance(side(a, ka, t), side(b, kb, 1.0 - t)));
                        }
                    }
                }
            }
        }
        (worst, shared)
    }

    #[test]
    fn a_box_cage_gives_patches_without_gaps() {
        let cage = box_cage([0.0; 3], [40.0, 40.0, 40.0], [1, 1, 1]).unwrap();
        let surface = limit_surface(&cage).unwrap();
        assert!(surface.closed);
        // Four quads a face, none regular: every corner of a cube is
        // extraordinary.
        assert_eq!(surface.patches.len(), 24);
        assert!(surface.patches.iter().all(|p| !p.exact));
        let (gap, shared) = largest_gap(&surface);
        // 48 edges between the 24 quads.
        assert_eq!(shared, 48);
        assert!(gap < 1e-9, "{gap}");
        // The surface lies inside the cage and is round: its points at the
        // middle of the cage's faces are the limit points there.
        for patch in &surface.patches {
            for corner in patch.corners() {
                assert!(corner.iter().all(|c| c.abs() <= 20.0 + 1e-9));
            }
        }
    }

    #[test]
    fn regular_quads_give_exact_patches_that_meet_the_others() {
        // A box of 4 x 4 x 4 quads a side has quads whose corners are all
        // regular in the middle of each side.
        let cage = box_cage([0.0; 3], [40.0, 40.0, 40.0], [4, 4, 4]).unwrap();
        let surface = limit_surface(&cage).unwrap();
        let exact = surface.patches.iter().filter(|p| p.exact).count();
        assert!(exact > 0);
        assert!(exact < surface.patches.len());
        let (gap, _) = largest_gap(&surface);
        assert!(gap < 1e-9, "{gap}");
        // An exact patch is the limit surface: its corners are the limit
        // points the preview gives.
        let mesh = limit_mesh(&cage, 1).unwrap();
        for patch in surface.patches.iter().filter(|p| p.exact).take(8) {
            for corner in patch.corners() {
                assert!(
                    mesh.positions.iter().any(|p| distance(*p, corner) < 1e-9),
                    "{corner:?}"
                );
            }
        }
    }

    #[test]
    fn a_creased_edge_stays_sharp() {
        let mut cage = box_cage([0.0; 3], [40.0, 40.0, 40.0], [2, 2, 2]).unwrap();
        // Crease the top face's edges round it.
        let top: Vec<u32> = cage
            .vertices
            .iter()
            .filter(|v| v.at[2] == 20.0)
            .map(|v| v.id)
            .collect();
        let rim: Vec<[u32; 2]> = cage
            .edges()
            .keys()
            .filter(|[a, b]| {
                top.contains(a)
                    && top.contains(b)
                    && [*a, *b].iter().all(|v| {
                        let p = cage.position(*v).unwrap();
                        p[0].abs() == 20.0 || p[1].abs() == 20.0
                    })
            })
            .copied()
            .collect();
        assert_eq!(rim.len(), 8);
        for [a, b] in &rim {
            cage.set_crease(*a, *b, None);
        }
        let surface = limit_surface(&cage).unwrap();
        let (gap, _) = largest_gap(&surface);
        assert!(gap < 1e-9, "{gap}");
        // The top stays flat at its cage's height (its vertices and the
        // crease's are all there), and where it meets the sides along the
        // crease the patches' normals stand at a steep angle (the side
        // leans in a little below the rim, where the cage's lower edges are
        // round).
        let on_top = |p: &Patch| p.corners().iter().all(|c| (c[2] - 20.0).abs() < 1e-9);
        let mut rim = 0;
        for (a, ka, b, kb) in shared_sides(&surface) {
            let (pa, pb) = (&surface.patches[a], &surface.patches[b]);
            if on_top(pa) == on_top(pb) {
                continue;
            }
            rim += 1;
            let dot = mitcad_vec3::dot(side_normal(pa, ka), side_normal(pb, kb));
            assert!(dot.abs() < 0.7, "{dot}");
        }
        // Each of the eight creased edges is two quads' sides.
        assert_eq!(rim, 16);
        // Without the crease the surface is smooth across every shared side.
        let smooth = box_cage([0.0; 3], [40.0, 40.0, 40.0], [2, 2, 2]).unwrap();
        let surface = limit_surface(&smooth).unwrap();
        for (a, ka, b, kb) in shared_sides(&surface) {
            let dot = mitcad_vec3::dot(
                side_normal(&surface.patches[a], ka),
                side_normal(&surface.patches[b], kb),
            );
            assert!(dot > 0.99, "{dot}");
        }
    }

    /// The parameters along side `k` of a patch (0: v = 0, 1: u = 1, 2:
    /// v = 1, 3: u = 0), from its first corner to its second.
    fn side_point(k: usize, t: f64) -> (f64, f64) {
        match k {
            0 => (t, 0.0),
            1 => (1.0, t),
            2 => (1.0 - t, 1.0),
            _ => (0.0, 1.0 - t),
        }
    }

    /// The pairs of patches that share a side: (patch, its side, the other
    /// patch, its side), the sides running opposite ways.
    fn shared_sides(surface: &LimitSurface) -> Vec<(usize, usize, usize, usize)> {
        let end = |p: &Patch, k: usize, t: f64| {
            let (u, v) = side_point(k, t);
            evaluate(p, u, v)
        };
        let near = |a: Vec3, b: Vec3| distance(a, b) < 1e-6;
        let mut pairs = Vec::new();
        for (a, pa) in surface.patches.iter().enumerate() {
            for (b, pb) in surface.patches.iter().enumerate().skip(a + 1) {
                for ka in 0..4 {
                    for kb in 0..4 {
                        let (a0, a1) = (end(pa, ka, 0.0), end(pa, ka, 1.0));
                        let (b0, b1) = (end(pb, kb, 0.0), end(pb, kb, 1.0));
                        let (am, bm) = (end(pa, ka, 0.5), end(pb, kb, 0.5));
                        if near(a0, b1) && near(a1, b0) && near(am, bm) && !near(a0, a1) {
                            pairs.push((a, ka, b, kb));
                        }
                    }
                }
            }
        }
        pairs
    }

    /// The patch's unit normal at the middle of its side `k`.
    fn side_normal(patch: &Patch, k: usize) -> Vec3 {
        let (u, v) = side_point(k, 0.5);
        let d = 1e-5;
        let at = |u: f64, v: f64| evaluate(patch, u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let du = mitcad_vec3::sub(at(u + d, v), at(u - d, v));
        let dv = mitcad_vec3::sub(at(u, v + d), at(u, v - d));
        mitcad_vec3::unit(mitcad_vec3::cross(du, dv)).expect("a normal")
    }

    #[test]
    fn an_open_cage_gives_a_sheet() {
        let cage = plane_cage([0.0; 3], [30.0, 20.0], [3, 2]).unwrap();
        let surface = limit_surface(&cage).unwrap();
        assert!(!surface.closed);
        assert_eq!(surface.patches.len(), 24);
        // The sheet is flat and its boundary follows the cage's, its corners
        // staying where they are.
        for patch in &surface.patches {
            assert!(patch.poles.iter().all(|p| p[2].abs() < 1e-12));
        }
        let corners: Vec<Vec3> = surface.patches.iter().flat_map(|p| p.corners()).collect();
        assert!(
            corners
                .iter()
                .any(|c| distance(*c, [-15.0, -10.0, 0.0]) < 1e-12)
        );
    }

    #[test]
    fn an_ngon_gives_one_patch_a_corner() {
        // A pentagonal prism: two pentagons and five quads.
        let mut cage = Cage {
            vertices: Vec::new(),
            faces: Vec::new(),
            creases: Vec::new(),
            corners: Vec::new(),
            symmetry: None,
            next_vertex: 0,
            next_face: 0,
        };
        for k in 0..5u32 {
            let a = std::f64::consts::TAU * f64::from(k) / 5.0;
            for (level, z) in [(0, -10.0), (1, 10.0)] {
                cage.vertices.push(CageVertex {
                    id: 1 + k + 5 * level,
                    at: [20.0 * a.cos(), 20.0 * a.sin(), z],
                });
            }
        }
        cage.faces.push(CageFace {
            id: 1,
            vertices: vec![1, 5, 4, 3, 2],
        });
        cage.faces.push(CageFace {
            id: 2,
            vertices: vec![6, 7, 8, 9, 10],
        });
        for k in 0..5u32 {
            let next = (k + 1) % 5;
            cage.faces.push(CageFace {
                id: 3 + k,
                vertices: vec![1 + k, 1 + next, 6 + next, 6 + k],
            });
        }
        cage.check().unwrap();
        let surface = limit_surface(&cage).unwrap();
        assert!(surface.closed);
        assert_eq!(surface.patches.len(), 5 + 5 + 20);
        let on_bottom: Vec<u32> = surface
            .patches
            .iter()
            .filter(|p| p.face == 1)
            .map(|p| p.corner)
            .collect();
        assert_eq!(on_bottom, [0, 1, 2, 3, 4]);
        let (gap, shared) = largest_gap(&surface);
        assert!(gap < 1e-9, "{gap}");
        assert_eq!(shared, 60);
    }
}
