// SPDX-License-Identifier: MIT
//! Envelope's wrap (mitcad#1271): triangle meshes wrapped on a voxel grid,
//! without the geometry kernel.
//!
//! 1. The grid's cells are half the detail size; it reaches a few cells
//!    past the meshes on every side.
//! 2. Every cell a triangle passes through is marked: the triangles are
//!    sampled at most half a cell apart, and a sample on a cell boundary
//!    takes the cell on the side of the triangle and of the material, so
//!    that a face lying on a cell boundary marks the cells inside and a
//!    box on the grid wraps exactly.
//! 3. The marked cells are grown by one cell along every axis (a cube):
//!    gaps narrower than the detail size close, and so do holes in the
//!    surface that the sampling left.
//! 4. The cells reached from the grid's border without crossing a marked
//!    cell are outside; every other cell is solid, so enclosed cavities
//!    fill.
//! 5. The solid shrinks back by the cell it grew (the closing's second
//!    half): openings wider than the detail size come back as they were.
//! 6. The cells are made well-composed: wherever eight cells around a grid
//!    point hold solid cells (or empty ones) that touch only at an edge or
//!    a corner, empty cells there are filled until both sides are joined
//!    across faces, and cavities that makes are filled too. The surface is
//!    then a closed manifold.
//! 7. The surface is the cell faces between solid and empty cells. Each
//!    plane's faces of one side go in runs along a row; a run is two rows
//!    of points zipped into triangles, keeping only the points where the
//!    surface is not flat (where another run, row or face meets it), so
//!    flat stretches take few triangles and neighbours share every point:
//!    the mesh is closed, its triangles counter-clockwise seen from
//!    outside.
//!
//! The result is coarse but light: an outer skin within one cell of the
//! source, its faces parallel to the axes, which Convert Mesh's Faceted
//! solid ([`crate::features::mesh_convert::faceted_solid`]) merges into
//! a few planar faces.

use std::collections::HashMap;

use mitcad_vec3::{add, add_scaled, cross, norm, scale, sub};

use crate::exchange::TriangleMesh;

/// The most cells the grid may have; a detail size that needs more is
/// refused with the smallest one that fits.
pub const MAX_CELLS: usize = 1 << 24;

/// The most triangles the wrapped surface may have (what Faceted solid
/// takes).
pub const MAX_TRIANGLES: usize = crate::features::mesh_convert::MAX_TRIANGLES;

/// Cells grown and shrunk by the closing.
const RADIUS: usize = 1;

/// Empty cells beyond the meshes on every side: the closing's radius and
/// room for the repair of step 6.
const PAD: usize = RADIUS + 2;

/// How near a cell boundary (in cells) a sample counts as on it.
const SNAP: f64 = 1e-6;

/// The most rounds of repair and cavity filling (step 6) before the wrap
/// gives up.
const MAX_ROUNDS: usize = 64;

/// What [`wrap`] made.
#[derive(Debug, Clone, PartialEq)]
pub struct Wrapped {
    /// The closed surface, counter-clockwise seen from outside.
    pub mesh: TriangleMesh,
    /// The cells' size (mm): half the detail size.
    pub cell: f64,
    /// The grid's cells along x, y and z.
    pub cells: [usize; 3],
    /// The solid cells' volume (mm³): the volume the surface encloses.
    pub volume: f64,
}

/// The cells of the grid, x fastest.
struct Grid {
    n: [usize; 3],
    solid: Vec<bool>,
}

impl Grid {
    fn index(&self, p: [usize; 3]) -> usize {
        p[0] + self.n[0] * (p[1] + self.n[1] * p[2])
    }

    fn at(&self, p: [usize; 3]) -> bool {
        self.solid[self.index(p)]
    }

    fn stride(&self, axis: usize) -> usize {
        match axis {
            0 => 1,
            1 => self.n[0],
            _ => self.n[0] * self.n[1],
        }
    }

    fn coords(&self, index: usize) -> [usize; 3] {
        [
            index % self.n[0],
            (index / self.n[0]) % self.n[1],
            index / (self.n[0] * self.n[1]),
        ]
    }
}

/// Checks a detail size: finite and more than zero.
pub fn check_detail(detail: f64) -> Result<(), String> {
    if detail.is_finite() && detail > 0.0 {
        Ok(())
    } else {
        Err(format!(
            "the detail size must be more than 0 mm, got {detail}"
        ))
    }
}

/// The meshes wrapped with gaps and openings smaller than `detail` (mm)
/// closed (see the module docs).
pub fn wrap(meshes: &[TriangleMesh], detail: f64) -> Result<Wrapped, String> {
    check_detail(detail)?;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut triangles = 0usize;
    for mesh in meshes {
        for tri in &mesh.triangles {
            for &v in tri {
                let p = mesh
                    .vertices
                    .get(v as usize)
                    .ok_or("a triangle refers to a vertex its mesh does not have")?;
                if p.iter().any(|c| !c.is_finite()) {
                    return Err("a vertex of the source is not a finite point".to_owned());
                }
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
        }
        triangles += mesh.triangles.len();
    }
    if triangles == 0 {
        return Err("the bodies have no triangles to wrap".to_owned());
    }
    let cell = detail / 2.0;
    let spans: [f64; 3] = std::array::from_fn(|i| ((max[i] - min[i]) / cell).floor());
    let wanted: f64 = spans.iter().map(|s| s + (2 * PAD + 1) as f64).product();
    if !wanted.is_finite() || wanted > MAX_CELLS as f64 {
        // The cells grow with the cube of 1 / detail.
        let at_least = detail * (wanted / MAX_CELLS as f64).cbrt() * 1.05;
        let at_least = if at_least < 10.0 {
            (at_least * 10.0).ceil() / 10.0
        } else {
            at_least.ceil()
        };
        return Err(format!(
            "a detail size of {detail} mm needs a grid of {} × {} × {} cells, more than {MAX_CELLS}; \
             use a detail size of at least {at_least} mm",
            spans[0] + (2 * PAD + 1) as f64,
            spans[1] + (2 * PAD + 1) as f64,
            spans[2] + (2 * PAD + 1) as f64
        ));
    }
    let n: [usize; 3] = std::array::from_fn(|i| spans[i] as usize + 2 * PAD + 1);
    let origin: [f64; 3] = std::array::from_fn(|i| min[i] - PAD as f64 * cell);
    let mut grid = Grid {
        n,
        solid: vec![false; n[0] * n[1] * n[2]],
    };

    // 2. Mark.
    for mesh in meshes {
        for tri in &mesh.triangles {
            let corners = tri.map(|v| mesh.vertices[v as usize]);
            mark_triangle(&mut grid, origin, cell, corners);
        }
    }

    // 3.-5. Close, filling what is enclosed.
    morph(&mut grid, RADIUS, true);
    fill_enclosed(&mut grid);
    morph(&mut grid, RADIUS, false);

    // 6. Well-composed, without cavities.
    let table = Table::new();
    let mut rounds = 0;
    loop {
        let repaired = repair(&mut grid, &table);
        let filled = fill_enclosed(&mut grid);
        if repaired == 0 && filled == 0 {
            break;
        }
        rounds += 1;
        if rounds > MAX_ROUNDS {
            return Err("the wrapped cells do not settle into a closed surface".to_owned());
        }
    }
    let count = grid.solid.iter().filter(|s| **s).count();
    if count == 0 {
        return Err("nothing is left of the bodies on the grid".to_owned());
    }

    // 7. The surface.
    let mesh = surface(&grid, origin, cell);
    if mesh.triangles.len() > MAX_TRIANGLES {
        return Err(format!(
            "the envelope at a detail size of {detail} mm has {} triangles, more than \
             {MAX_TRIANGLES}; use a larger detail size",
            mesh.triangles.len()
        ));
    }
    Ok(Wrapped {
        mesh,
        cell,
        cells: n,
        volume: count as f64 * cell * cell * cell,
    })
}

/// The cell of a point of a triangle, within the grid. A coordinate on a
/// cell boundary (within [`SNAP`] of a cell) takes the cell on the side
/// `toward` points to along that axis: into the triangle and into the
/// material, so that a face on a cell boundary marks the cells inside it
/// and its edges do not mark the cells beyond the next face.
fn cell_of(grid: &Grid, origin: [f64; 3], cell: f64, p: [f64; 3], toward: [f64; 3]) -> [usize; 3] {
    std::array::from_fn(|i| {
        let g = (p[i] - origin[i]) / cell;
        let nearest = g.round();
        let k = if (g - nearest).abs() <= SNAP {
            if toward[i] < 0.0 {
                nearest - 1.0
            } else {
                nearest
            }
        } else {
            g.floor()
        };
        if k <= 0.0 {
            0
        } else {
            (k as usize).min(grid.n[i] - 1)
        }
    })
}

/// Marks the cells a triangle passes through (step 2): samples at most
/// half a cell apart from the corner between its longest and shortest
/// edges along those two edges, and along the third.
fn mark_triangle(grid: &mut Grid, origin: [f64; 3], cell: f64, corners: [[f64; 3]; 3]) {
    let normal = cross(sub(corners[1], corners[0]), sub(corners[2], corners[0]));
    let area = norm(normal);
    // Into the material: against the normal.
    let inward = if area > 0.0 {
        scale(normal, -1.0 / area)
    } else {
        [0.0; 3]
    };
    let centroid = scale(add(add(corners[0], corners[1]), corners[2]), 1.0 / 3.0);
    let mut mark = |p: [f64; 3]| {
        // Into the triangle (towards its centroid) and into the material.
        let to_centre = sub(centroid, p);
        let length = norm(to_centre);
        let toward = if length > 0.0 {
            add(scale(to_centre, 1.0 / length), inward)
        } else {
            inward
        };
        let at = cell_of(grid, origin, cell, p, toward);
        let index = grid.index(at);
        grid.solid[index] = true;
    };
    // Edge k runs from corner k to corner k + 1.
    let lengths: [f64; 3] = std::array::from_fn(|k| norm(sub(corners[(k + 1) % 3], corners[k])));
    let longest = (0..3)
        .max_by(|a, b| lengths[*a].total_cmp(&lengths[*b]))
        .unwrap_or(0);
    let shortest = (0..3)
        .filter(|k| *k != longest)
        .min_by(|a, b| lengths[*a].total_cmp(&lengths[*b]))
        .unwrap_or((longest + 1) % 3);
    // The corner both edges share, and the far ends of the two edges.
    let shared = if (longest + 1) % 3 == shortest {
        shortest
    } else {
        longest
    };
    let a = corners[shared];
    let b = corners[(shared + 1) % 3];
    let c = corners[(shared + 2) % 3];
    let step = 0.5 * cell;
    let count = |length: f64| (length / step).ceil().max(1.0) as usize;
    let along_b = sub(b, a);
    let along_c = sub(c, a);
    let nb = count(norm(along_b));
    let nc = count(norm(along_c));
    for i in 0..=nb {
        let s = i as f64 / nb as f64;
        // The samples of this column end on the far edge (s + t <= 1).
        let last = ((1.0 - s) * nc as f64).floor() as usize;
        let start = add_scaled(a, along_b, s);
        for j in 0..=last {
            mark(add_scaled(start, along_c, j as f64 / nc as f64));
        }
    }
    let along = sub(c, b);
    let nf = count(norm(along));
    for i in 0..=nf {
        mark(add_scaled(b, along, i as f64 / nf as f64));
    }
}

/// Grows (`grow`) or shrinks the solid cells by `radius` cells along each
/// axis in turn: a cube. Shrinking treats the cells beyond the grid as
/// empty.
fn morph(grid: &mut Grid, radius: usize, grow: bool) {
    for axis in 0..3 {
        let source = grid.solid.clone();
        let stride = grid.stride(axis);
        let n = grid.n[axis];
        for (index, out) in grid.solid.iter_mut().enumerate() {
            let k = (index / stride) % n;
            let base = index - k * stride;
            let lo = k.saturating_sub(radius);
            let hi = (k + radius).min(n - 1);
            let mut window = (lo..=hi).map(|j| source[base + j * stride]);
            *out = if grow {
                window.any(|s| s)
            } else {
                k >= radius && k + radius < n && window.all(|s| s)
            };
        }
    }
}

/// The empty cells reached from the grid's border across faces of empty
/// cells.
fn outside(grid: &Grid) -> Vec<bool> {
    let mut out = vec![false; grid.solid.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (index, cell) in out.iter_mut().enumerate() {
        let p = grid.coords(index);
        let border = (0..3).any(|i| p[i] == 0 || p[i] + 1 == grid.n[i]);
        if border && !grid.solid[index] {
            *cell = true;
            stack.push(index);
        }
    }
    while let Some(index) = stack.pop() {
        let p = grid.coords(index);
        for (axis, &at) in p.iter().enumerate() {
            let stride = grid.stride(axis);
            let mut visit = |next: usize| {
                if !grid.solid[next] && !out[next] {
                    out[next] = true;
                    stack.push(next);
                }
            };
            if at > 0 {
                visit(index - stride);
            }
            if at + 1 < grid.n[axis] {
                visit(index + stride);
            }
        }
    }
    out
}

/// Makes every empty cell not reached from outside solid; returns how
/// many it filled.
fn fill_enclosed(grid: &mut Grid) -> usize {
    let out = outside(grid);
    let mut filled = 0;
    for (solid, out) in grid.solid.iter_mut().zip(out) {
        if !*solid && !out {
            *solid = true;
            filled += 1;
        }
    }
    filled
}

/// What the eight cells around a grid point need: their configurations
/// (bit `dx + 2 dy + 4 dz` set for a solid cell) whose solid or empty
/// cells are not joined across faces, and for each the empty cell to fill
/// first (the one with the most solid neighbours among the eight, the
/// lowest bit of a tie).
struct Table {
    bad: [bool; 256],
    fill: [u8; 256],
}

impl Table {
    fn new() -> Self {
        let joined = |mask: u8| -> bool {
            if mask == 0 {
                return true;
            }
            let first = mask.trailing_zeros() as u8;
            let mut seen = 1u8 << first;
            let mut stack = vec![first];
            while let Some(i) = stack.pop() {
                for flip in [1u8, 2, 4] {
                    let j = i ^ flip;
                    if mask & (1 << j) != 0 && seen & (1 << j) == 0 {
                        seen |= 1 << j;
                        stack.push(j);
                    }
                }
            }
            seen == mask
        };
        let mut bad = [false; 256];
        let mut fill = [0u8; 256];
        for config in 0..256usize {
            let solid = config as u8;
            let empty = !solid;
            bad[config] = solid != 0 && empty != 0 && !(joined(solid) && joined(empty));
            let mut best: Option<(u32, u8)> = None;
            for i in 0..8u8 {
                if solid & (1 << i) != 0 {
                    continue;
                }
                let score = [1u8, 2, 4]
                    .iter()
                    .filter(|flip| solid & (1 << (i ^ **flip)) != 0)
                    .count() as u32;
                if best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, i));
                }
            }
            fill[config] = best.map_or(0, |(_, i)| i);
        }
        Self { bad, fill }
    }
}

/// Fills empty cells until every eight cells around a grid point are
/// well-composed (step 6); returns how many it filled.
fn repair(grid: &mut Grid, table: &Table) -> usize {
    let n = grid.n;
    let mut filled = 0;
    loop {
        let mut pass = 0;
        for z in 0..n[2] - 1 {
            for y in 0..n[1] - 1 {
                for x in 0..n[0] - 1 {
                    loop {
                        let mut config = 0usize;
                        for bit in 0..8 {
                            let p = [x + (bit & 1), y + ((bit >> 1) & 1), z + ((bit >> 2) & 1)];
                            if grid.at(p) {
                                config |= 1 << bit;
                            }
                        }
                        if !table.bad[config] {
                            break;
                        }
                        let bit = table.fill[config] as usize;
                        let p = [x + (bit & 1), y + ((bit >> 1) & 1), z + ((bit >> 2) & 1)];
                        let index = grid.index(p);
                        grid.solid[index] = true;
                        pass += 1;
                    }
                }
            }
        }
        if pass == 0 {
            return filled;
        }
        filled += pass;
    }
}

/// The vertex of a grid point, added once.
fn vertex(
    points: &mut HashMap<[u32; 3], u32>,
    vertices: &mut Vec<[f64; 3]>,
    key: [u32; 3],
    origin: [f64; 3],
    cell: f64,
) -> u32 {
    *points.entry(key).or_insert_with(|| {
        vertices.push(std::array::from_fn(|i| origin[i] + key[i] as f64 * cell));
        (vertices.len() - 1) as u32
    })
}

/// The faces between solid and empty cells as triangles (step 7).
fn surface(grid: &Grid, origin: [f64; 3], cell: f64) -> TriangleMesh {
    let mut points: HashMap<[u32; 3], u32> = HashMap::new();
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for a in 0..3 {
        // The plane's own axes: (b, c, a) is right-handed, so a run
        // counter-clockwise in (b, c) faces +a.
        let b = (a + 1) % 3;
        let c = (a + 2) % 3;
        let (nb, nc) = (grid.n[b], grid.n[c]);
        let mut faces = vec![0i8; nb * nc];
        for k in 1..grid.n[a] {
            // The faces on the plane between cells k - 1 and k: +1 where
            // the solid is below (the face looks along +a), -1 above.
            let mut any = false;
            for w in 0..nc {
                for u in 0..nb {
                    let mut below = [0usize; 3];
                    below[a] = k - 1;
                    below[b] = u;
                    below[c] = w;
                    let mut above = below;
                    above[a] = k;
                    let side = match (grid.at(below), grid.at(above)) {
                        (true, false) => 1,
                        (false, true) => -1,
                        _ => 0,
                    };
                    faces[u + nb * w] = side;
                    any |= side != 0;
                }
            }
            if !any {
                continue;
            }
            let face = |u: usize, w: usize| -> i8 {
                if u < nb && w < nc {
                    faces[u + nb * w]
                } else {
                    0
                }
            };
            // A point inside a flat stretch of one side: the four faces
            // around it are all there (u, w > 0 here).
            let flat = |u: usize, w: usize, side: i8| -> bool {
                w > 0
                    && face(u - 1, w - 1) == side
                    && face(u, w - 1) == side
                    && face(u - 1, w) == side
                    && face(u, w) == side
            };
            for w in 0..nc {
                let mut u = 0;
                while u < nb {
                    let side = faces[u + nb * w];
                    if side == 0 {
                        u += 1;
                        continue;
                    }
                    let u0 = u;
                    while u < nb && faces[u + nb * w] == side {
                        u += 1;
                    }
                    let u1 = u;
                    let mut row = |line: usize| -> Vec<(usize, u32)> {
                        let mut at = vec![u0];
                        at.extend((u0 + 1..u1).filter(|p| !flat(*p, line, side)));
                        at.push(u1);
                        at.into_iter()
                            .map(|p| {
                                let mut key = [0u32; 3];
                                key[a] = k as u32;
                                key[b] = p as u32;
                                key[c] = line as u32;
                                (p, vertex(&mut points, &mut vertices, key, origin, cell))
                            })
                            .collect()
                    };
                    let bottom = row(w);
                    let top = row(w + 1);
                    zip(&bottom, &top, side > 0, &mut triangles);
                }
            }
        }
    }
    TriangleMesh {
        vertices,
        triangles,
    }
}

/// Triangles between two rows of points on parallel lines, each point its
/// place along the run and its vertex, both rows from the run's start to
/// its end (`bottom` below `top` in the plane's own axes):
/// counter-clockwise there when `forward`, else turned. Any order of
/// taking the rows' points makes triangles that cover the run once; the
/// point less far along is taken next (the bottom's at a tie), which
/// keeps them from growing long and thin.
fn zip(
    bottom: &[(usize, u32)],
    top: &[(usize, u32)],
    forward: bool,
    triangles: &mut Vec<[u32; 3]>,
) {
    let (mut i, mut j) = (0, 0);
    let (p, q) = (bottom.len() - 1, top.len() - 1);
    while i < p || j < q {
        let take_bottom = j == q || (i < p && bottom[i + 1].0 <= top[j + 1].0);
        let tri = if take_bottom {
            i += 1;
            [bottom[i - 1].1, bottom[i].1, top[j].1]
        } else {
            j += 1;
            [bottom[i].1, top[j].1, top[j - 1].1]
        };
        triangles.push(if forward {
            tri
        } else {
            [tri[0], tri[2], tri[1]]
        });
    }
}
