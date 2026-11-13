// SPDX-License-Identifier: MIT
//! Print Analysis (mitcad#620): where a body will print badly and what it
//! will take, worked out in Rust on the body's triangle mesh
//! ([`crate::Kernel::triangle_mesh`], the mesh the 3MF export gives a
//! slicer). Nothing here changes a body.
//!
//! - **Overhangs** ([`overhangs`]): each triangle's normal against the
//!   print direction. A triangle whose outward normal is within
//!   `90° − angle` of straight down is an overhang (`angle` is measured
//!   from the vertical, 45° by default), unless it lies on the bed. Curved
//!   faces are flagged where they are steep, not as a whole.
//! - **Thin walls** ([`thickness`]): from sample points on each triangle
//!   (its centroid, more on large triangles) a ray along the surface's
//!   inward normal (interpolated from the normals at the triangle's
//!   corners, [`corner_normals`]) to the first triangle of the same mesh
//!   where it leaves the material, through a bounding volume hierarchy of
//!   the triangles. On planar walls the value is exact (next to a tangent
//!   blend within the blend's facet angle); on curved ones it is within the
//!   mesh's deviation. A ray that leaves through a face meeting its start
//!   across a sharp edge within about one sample distance measures the
//!   edge, not a wall, and is left out (mitcad#689, [`Bvh::across_edge`]).
//!   What is shown thin is where the wall is thin, not the whole triangle:
//!   a triangle thin at only some of its samples (a long one at the edge of
//!   a thin region) is subdivided more finely and only its thin pieces are
//!   kept (mitcad#767, [`Bvh::wall`]).
//! - **Estimate** ([`estimate`]): shell, infill and filament from the
//!   body's volume and area.
//! - **Narrow gaps** (mitcad#1519, [`gap_candidates`], [`gap_kind`]):
//!   bodies printed in place closer than the gap a printer keeps open
//!   fuse. The pairs whose boxes lie within the gap are measured by the
//!   kernel's exact distance (`Kernel::measure_between`). Their contact
//!   itself is `interference`'s, but a pair that touches is not left out
//!   (mitcad#1672, [`face_gap`]): its faces are measured pair by pair
//!   (`Kernel::face_separations`), the faces that touch or cross are the
//!   contact, and the narrowest other pair whose faces face each other
//!   across air ([`faces_across_a_gap`]) is a narrow gap beside the
//!   contact (a peg in its hole beside a face both lie on).
//!
//! The functions take a [`TriangleMesh`] and the settings, so they are
//! tested without a kernel (`print_analysis_tests.rs`); the queries that
//! use them are in `model-api/src/print.rs`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;

use mitcad_vec3::{Vec3, add, cross, dot, norm, scale, sub, unit};
use serde_json::Value;

use crate::exchange::{MeshTolerance, TriangleMesh};
use crate::fingerprint::Fingerprint;
use crate::ids::BodyUid;
use crate::monitor::Cancelled;
use crate::transform::Transform;

/// Overhang angle from the vertical, degrees.
pub const DEFAULT_ANGLE: f64 = 45.0;
/// The thinnest wall a printer makes, mm (two line widths).
pub const DEFAULT_MINIMUM: f64 = 0.8;
/// Estimate settings, mm and fractions.
pub const DEFAULT_LINE_WIDTH: f64 = 0.4;
pub const DEFAULT_WALLS: u32 = 2;
pub const DEFAULT_LAYER_HEIGHT: f64 = 0.2;
pub const DEFAULT_TOP_BOTTOM_LAYERS: u32 = 4;
pub const DEFAULT_INFILL: f64 = 0.15;
pub const DEFAULT_FILAMENT_DIAMETER: f64 = 1.75;
/// The material of bodies that have none of their own.
pub const DEFAULT_MATERIAL: &str = "pla";

/// Rays shorter than this (mm) hit nothing: the triangle a ray starts on
/// and its coplanar neighbours.
const MIN_HIT: f64 = 1e-6;
/// A wall at the minimum is not thin: the baseline tolerance (mm).
const THIN_TOLERANCE: f64 = 1e-6;
/// A large triangle gets a sample about this often (mm) along its longest
/// edge, at most [`MAX_SPLITS`] per edge.
const SAMPLE_SPACING: f64 = 2.0;
const MAX_SPLITS: usize = 8;
/// A triangle that is thin only in part is subdivided this many times more
/// finely, and only its thin pieces are shown (mitcad#767): at most
/// (8 × 4)² = 1024 rays more for it. A multiple of 3 plus 1 keeps the
/// coarse samples among the fine ones (the centroid of coarse piece
/// `(i, j)` is that of fine piece `(4i + 1, 4j + 1)` pointing up, or
/// `(4i + 2, 4j + 2)` pointing down).
const REFINE: usize = 4;
/// Rays between two looks at the cancel request.
const CANCEL_EVERY: usize = 256;
/// Triangles in a leaf of the hierarchy.
const LEAF_SIZE: usize = 4;
/// Triangles whose normals are within 40° of each other belong to one
/// surface when the wall's ray direction is worked out: a curved surface's
/// facets differ by the mesh's angle (15° at the 3MF export's default), the
/// faces of a box or a 45° chamfer by more.
const SMOOTH_COS: f64 = 0.766_044_443_118_978; // cos 40°
/// A ray that leaves through a face meeting its start across an edge
/// within this distance (mm, about one sample distance) measures the edge,
/// not a wall (mitcad#689, [`Bvh::across_edge`]).
const EDGE_REACH: f64 = SAMPLE_SPACING;
/// How far from the exit the edge is looked for (mm): a wedge's far side
/// reaches the edge at its distance over the cosine of the wedge's angle.
const EDGE_SEARCH: f64 = 4.0 * EDGE_REACH;
/// Planes within about 1° of parallel never meet near a wall.
const EDGE_SINE: f64 = 0.017_452_406_437_283_51; // sin 1°
/// The most triangles looked at on either side of an edge.
const EDGE_TRIANGLES: usize = 4096;

/// The mesh placed in the design: its vertices moved by `transform`, and a
/// mirroring placement's triangles turned so that they stay
/// counter-clockwise seen from outside.
pub fn placed(mesh: &TriangleMesh, transform: &Transform) -> TriangleMesh {
    let mirrored = transform.determinant() < 0.0;
    TriangleMesh {
        vertices: mesh
            .vertices
            .iter()
            .map(|v| transform.apply_point(*v))
            .collect(),
        triangles: mesh
            .triangles
            .iter()
            .map(|&[a, b, c]| if mirrored { [a, c, b] } else { [a, b, c] })
            .collect(),
    }
}

fn corners(mesh: &TriangleMesh, triangle: usize) -> [Vec3; 3] {
    mesh.triangles[triangle].map(|i| mesh.vertices[i as usize])
}

/// Twice the triangle's area times its unit normal (outward when the
/// triangle is counter-clockwise seen from outside).
fn area_normal(p: &[Vec3; 3]) -> Vec3 {
    cross(sub(p[1], p[0]), sub(p[2], p[0]))
}

/// A triangle's corners as the 9 numbers the queries answer with.
pub fn flat(mesh: &TriangleMesh, triangle: usize) -> [f64; 9] {
    flat_corners(&corners(mesh, triangle))
}

/// Three corners (a piece of a triangle, [`Thickness::pieces`]) as the 9
/// numbers the queries answer with.
pub fn flat_corners([a, b, c]: &[Vec3; 3]) -> [f64; 9] {
    [a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]
}

/// Where the bed is, along the print direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bed {
    /// At the body's lowest point (no bed face).
    Lowest,
    /// The plane through this point normal to the print direction (the bed
    /// face's plane).
    Through(Vec3),
}

/// A body's overhangs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overhangs {
    /// Of all triangles, mm².
    pub area: f64,
    /// Of the overhanging triangles, mm².
    pub overhang_area: f64,
    /// The overhanging triangles, indices into the mesh's.
    pub triangles: Vec<usize>,
}

impl Overhangs {
    /// The overhanging part of the area, 0 to 1.
    pub fn share(&self) -> f64 {
        if self.area > 0.0 {
            self.overhang_area / self.area
        } else {
            0.0
        }
    }
}

/// The overhangs of `mesh` printed along `down` (a unit vector, the
/// direction towards the bed) at `angle` degrees from the vertical. The
/// triangles that face down with every corner within `tolerance` of the
/// bed lie on it and are no overhangs.
pub fn overhangs(
    mesh: &TriangleMesh,
    down: Vec3,
    angle: f64,
    bed: Bed,
    tolerance: f64,
) -> Overhangs {
    let mut result = Overhangs::default();
    let bed_level = match bed {
        Bed::Through(point) => dot(point, down),
        Bed::Lowest => mesh
            .vertices
            .iter()
            .map(|v| dot(*v, down))
            .fold(f64::NEG_INFINITY, f64::max),
    };
    // Within 90° − angle of straight down: cos(90° − angle) = sin(angle).
    // A face exactly at the angle prints.
    let limit = angle.to_radians().sin() + 1e-9;
    for t in 0..mesh.triangles.len() {
        let p = corners(mesh, t);
        let n = area_normal(&p);
        let double = norm(n);
        if double.is_nan() || double <= 0.0 {
            continue;
        }
        let area = double / 2.0;
        result.area += area;
        let facing = dot(n, down) / double;
        if facing <= limit {
            continue;
        }
        let on_bed = p
            .iter()
            .all(|v| (dot(*v, down) - bed_level).abs() <= tolerance);
        if on_bed {
            continue;
        }
        result.overhang_area += area;
        result.triangles.push(t);
    }
    result
}

/// A body's walls.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Thickness {
    /// The thinnest wall found, mm; None when no ray met the other side
    /// (an open mesh, or no triangles).
    pub thinnest: Option<f64>,
    /// Where it is: the sample point on the surface.
    pub at: Option<Vec3>,
    /// The point on the other side of that wall.
    pub to: Option<Vec3>,
    /// The triangles with a wall thinner than the minimum somewhere: one of
    /// their samples is.
    pub thin: Vec<usize>,
    /// Where the walls are thinner than the minimum, each as its three
    /// corners: a thin triangle whole, or, where only some of its samples
    /// are thin (a large triangle at the edge of a thin region), the pieces
    /// of a finer subdivision of it that are (mitcad#767), so that a thin
    /// rim at the edge of a large planar face is not shown as slivers
    /// across the face.
    pub pieces: Vec<[Vec3; 3]>,
    /// The thin pieces in connected groups: those that touch (at a corner
    /// or along an edge), and the two sides of one wall.
    pub regions: usize,
}

/// A ray's first exit from the material.
#[derive(Debug, Clone, Copy)]
struct Hit {
    distance: f64,
    from: Vec3,
    to: Vec3,
    triangle: usize,
}

/// A piece of a triangle: in the subdivision of the triangle into `k` × `k`
/// similar triangles, the one at grid row `i`, column `j`, pointing up
/// (grid corners `(i, j)`, `(i + 1, j)`, `(i, j + 1)`) or down (`(i + 1,
/// j)`, `(i + 1, j + 1)`, `(i, j + 1)`). Grid point `(a, b)` is at
/// `p0 + (p1 − p0) a / k + (p2 − p0) b / k`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Cell {
    k: usize,
    i: usize,
    j: usize,
    up: bool,
}

impl Cell {
    /// The whole triangle.
    const WHOLE: Cell = Cell {
        k: 1,
        i: 0,
        j: 0,
        up: true,
    };

    /// The pieces of the `k` × `k` subdivision, row by row.
    fn all(k: usize) -> Vec<Cell> {
        let mut out = Vec::with_capacity(k * k);
        for i in 0..k {
            for j in 0..k - i {
                out.push(Cell { k, i, j, up: true });
                if i + j + 1 < k {
                    out.push(Cell { k, i, j, up: false });
                }
            }
        }
        out
    }

    /// The grid points of its corners.
    fn grid(self) -> [(usize, usize); 3] {
        let (i, j) = (self.i, self.j);
        if self.up {
            [(i, j), (i + 1, j), (i, j + 1)]
        } else {
            [(i + 1, j), (i + 1, j + 1), (i, j + 1)]
        }
    }

    /// Its centroid on the triangle `p` and the barycentric weights of the
    /// triangle's corners there.
    fn sample(self, p: &[Vec3; 3]) -> (Vec3, [f64; 3]) {
        let shift = if self.up { 1.0 / 3.0 } else { 2.0 / 3.0 };
        let k = self.k as f64;
        let (u, v) = ((self.i as f64 + shift) / k, (self.j as f64 + shift) / k);
        let (e1, e2) = (sub(p[1], p[0]), sub(p[2], p[0]));
        (
            add(p[0], add(scale(e1, u), scale(e2, v))),
            [1.0 - u - v, u, v],
        )
    }

    /// Its corners on the triangle `p`: the triangle's own at the grid's
    /// corners, so that a whole triangle is exactly itself.
    fn corners(self, p: &[Vec3; 3]) -> [Vec3; 3] {
        let k = self.k;
        self.grid().map(|(a, b)| match grid_corner(k, a, b) {
            Some(c) => p[c],
            None => {
                let (u, v) = (a as f64 / k as f64, b as f64 / k as f64);
                add(
                    p[0],
                    add(scale(sub(p[1], p[0]), u), scale(sub(p[2], p[0]), v)),
                )
            }
        })
    }

    /// The piece of the `k` × `k` subdivision that holds the point of
    /// barycentric weights `u`, `v` (of the second and third corners).
    fn holding(k: usize, u: f64, v: f64) -> Cell {
        let (a, b) = (u.max(0.0) * k as f64, v.max(0.0) * k as f64);
        let i = (a.floor() as usize).min(k - 1);
        let j = (b.floor() as usize).min(k - 1 - i);
        let up = i + j + 1 >= k || (a - i as f64) + (b - j as f64) <= 1.0;
        Cell { k, i, j, up }
    }
}

/// Which corner of the triangle the grid point `(a, b)` of a `k` × `k`
/// subdivision is, if it is one.
fn grid_corner(k: usize, a: usize, b: usize) -> Option<usize> {
    match (a, b) {
        (0, 0) => Some(0),
        _ if a == k && b == 0 => Some(1),
        _ if a == 0 && b == k => Some(2),
        _ => None,
    }
}

/// A thin piece of a triangle and where the ray of its sample left the
/// material (the triangle and the point): the wall's other side.
#[derive(Debug, Clone, Copy)]
struct Piece {
    cell: Cell,
    exit: (usize, Vec3),
}

/// What the samples of one triangle found: the thinnest wall under it and
/// its thin pieces (none, the whole triangle, or those of a finer
/// subdivision, [`Bvh::wall`]).
#[derive(Debug, Clone, Default)]
struct Wall {
    best: Option<Hit>,
    pieces: Vec<Piece>,
}

/// The walls of `mesh` (closed, counter-clockwise seen from outside), and
/// those thinner than `minimum` (mm). Stops with [`Cancelled`] once
/// `cancelled` says so; the samples run on all cores.
pub fn thickness(
    mesh: &TriangleMesh,
    minimum: f64,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Thickness, Cancelled> {
    let count = mesh.triangles.len();
    let bvh = Bvh::new(mesh);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(count / 512 + 1)
        .max(1);
    let chunk = count.div_ceil(threads).max(1);
    let limit = minimum - THIN_TOLERANCE;
    let mut walls: Vec<Wall> = vec![Wall::default(); count];
    let stopped = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        for (part, slots) in walls.chunks_mut(chunk).enumerate() {
            let (bvh, stopped) = (&bvh, &stopped);
            scope.spawn(move || {
                let first = part * chunk;
                let stop = || {
                    if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                        return true;
                    }
                    let now = cancelled();
                    if now {
                        stopped.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    now
                };
                for (i, slot) in slots.iter_mut().enumerate() {
                    if (i.is_multiple_of(CANCEL_EVERY) && stop())
                        || stopped.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        return;
                    }
                    *slot = bvh.wall(first + i, limit, &stop);
                }
            });
        }
    });
    if stopped.into_inner() {
        return Err(Cancelled);
    }
    let mut result = Thickness::default();
    let mut thinnest: Option<Hit> = None;
    for (t, wall) in walls.iter().enumerate() {
        let Some(hit) = wall.best else { continue };
        if thinnest.is_none_or(|best| hit.distance < best.distance) {
            thinnest = Some(hit);
        }
        if !wall.pieces.is_empty() {
            result.thin.push(t);
            let p = corners(mesh, t);
            result
                .pieces
                .extend(wall.pieces.iter().map(|piece| piece.cell.corners(&p)));
        }
    }
    if let Some(best) = thinnest {
        result.thinnest = Some(best.distance);
        result.at = Some(best.from);
        result.to = Some(best.to);
    }
    result.regions = regions(mesh, &walls);
    Ok(result)
}

/// How many connected groups the thin pieces make (mitcad#767): pieces
/// that touch, in one triangle at a grid point and across triangles at a
/// vertex of the mesh or along an edge of it (the parts of the edge they
/// reach overlap), and a piece with the thin piece its ray left the
/// material through (the wall's other side). Whole thin triangles are
/// grouped as before the pieces: by shared corners and the walls' sides.
fn regions(mesh: &TriangleMesh, walls: &[Wall]) -> usize {
    // The pieces in one list: their triangle and cell.
    let pieces: Vec<(usize, Piece)> = walls
        .iter()
        .enumerate()
        .flat_map(|(t, wall)| wall.pieces.iter().map(move |p| (t, *p)))
        .collect();
    if pieces.is_empty() {
        return 0;
    }
    let mut groups = UnionFind::new(pieces.len());
    let mut at_vertex: Vec<Option<usize>> = vec![None; mesh.vertices.len()];
    let mut at_grid: HashMap<(usize, usize, usize), usize> = HashMap::new();
    let mut by_cell: HashMap<(usize, Cell), usize> = HashMap::new();
    // Along each edge of the mesh (its vertices, the lower first): the
    // part of it a piece reaches, from the lower vertex (0) to the other.
    let mut along: Vec<((u32, u32), f64, f64, usize)> = Vec::new();
    for (id, &(t, Piece { cell, .. })) in pieces.iter().enumerate() {
        by_cell.insert((t, cell), id);
        let k = cell.k;
        let vertices = mesh.triangles[t];
        // The triangle's edges: from corner to corner, where a grid point
        // `(a, b)` lies on it and how far along it is (of k).
        let edges: [(usize, usize); 3] = [(0, 1), (1, 2), (0, 2)];
        let mut reach = [(usize::MAX, 0usize); 3];
        for (a, b) in cell.grid() {
            match grid_corner(k, a, b) {
                Some(c) => {
                    let v = vertices[c] as usize;
                    match at_vertex[v] {
                        Some(other) => groups.join(id, other),
                        None => at_vertex[v] = Some(id),
                    }
                }
                None => match at_grid.get(&(t, a, b)) {
                    Some(&other) => groups.join(id, other),
                    None => {
                        at_grid.insert((t, a, b), id);
                    }
                },
            }
            let on = [
                (b == 0).then_some(a),
                (a + b == k).then_some(b),
                (a == 0).then_some(b),
            ];
            for (e, step) in on.into_iter().enumerate() {
                if let Some(step) = step {
                    reach[e] = (reach[e].0.min(step), reach[e].1.max(step));
                }
            }
        }
        for (e, &(c0, c1)) in edges.iter().enumerate() {
            let (low, high) = reach[e];
            if low == usize::MAX {
                continue;
            }
            let (s0, s1) = (low as f64 / k as f64, high as f64 / k as f64);
            let (v0, v1) = (vertices[c0], vertices[c1]);
            if v0 < v1 {
                along.push(((v0, v1), s0, s1, id));
            } else {
                along.push(((v1, v0), 1.0 - s1, 1.0 - s0, id));
            }
        }
    }
    along.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)));
    for run in along.chunk_by(|x, y| x.0 == y.0) {
        let (_, _, mut far, mut last) = run[0];
        for &(_, s0, s1, id) in &run[1..] {
            if s0 <= far + 1e-9 {
                groups.join(id, last);
            }
            far = far.max(s1);
            last = id;
        }
    }
    // The wall's other side: the thin piece of the exit's triangle that
    // holds the exit point.
    let cell_size: Vec<usize> = walls
        .iter()
        .map(|w| w.pieces.first().map_or(0, |p| p.cell.k))
        .collect();
    for (id, (_, piece)) in pieces.iter().enumerate() {
        let (exit, to) = piece.exit;
        let k = cell_size[exit];
        if k == 0 {
            continue;
        }
        let (u, v) = barycentric(&corners(mesh, exit), to);
        if let Some(&other) = by_cell.get(&(exit, Cell::holding(k, u, v))) {
            groups.join(id, other);
        }
    }
    let mut roots: Vec<usize> = (0..pieces.len()).map(|id| groups.find(id)).collect();
    roots.sort_unstable();
    roots.dedup();
    roots.len()
}

/// The barycentric weights of the second and third corners of `p` at the
/// point `q` (projected onto the triangle's plane).
fn barycentric(p: &[Vec3; 3], q: Vec3) -> (f64, f64) {
    let (e1, e2, d) = (sub(p[1], p[0]), sub(p[2], p[0]), sub(q, p[0]));
    let (d00, d01, d11) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
    let (d20, d21) = (dot(d, e1), dot(d, e2));
    let den = d00 * d11 - d01 * d01;
    if den.abs() < 1e-300 || den.is_nan() {
        return (0.0, 0.0);
    }
    ((d11 * d20 - d01 * d21) / den, (d00 * d21 - d01 * d20) / den)
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// How finely a triangle is sampled: its samples are the centroids of the
/// pieces of a k × k subdivision ([`Cell::all`]), k growing with the
/// longest edge (the centroid alone on small ones).
fn subdivision(p: &[Vec3; 3]) -> usize {
    let longest = [
        norm(sub(p[1], p[0])),
        norm(sub(p[2], p[1])),
        norm(sub(p[0], p[2])),
    ]
    .into_iter()
    .fold(0.0, f64::max);
    ((longest / SAMPLE_SPACING).ceil() as usize).clamp(1, MAX_SPLITS)
}

/// The surface's unit normal at each corner of each triangle, from the
/// triangles that meet at the corner's vertex: their area normals added up,
/// of those facing within [`SMOOTH_COS`] of the triangle (so the normal of
/// a curved surface, the triangle's own on a planar face that meets others
/// at an edge). A ray along a triangle's own normal misses the surface's
/// normal on a curved face by the triangle's tilt, which a skewed triangle
/// of a fine mesh makes large (a sphere's wall then measures a chord
/// beside its centre, 1.3 % short at a 0.01 mm deviation); the interpolated
/// corner normals follow the surface. The triangle's own normal where the
/// mesh shares no vertices.
fn corner_normals(
    mesh: &TriangleMesh,
    triangles: &[([Vec3; 3], Vec3)],
    incident: &[Vec<usize>],
) -> Vec<[Vec3; 3]> {
    let units: Vec<Option<Vec3>> = triangles.iter().map(|(_, n)| unit(*n)).collect();
    mesh.triangles
        .iter()
        .enumerate()
        .map(|(t, corners)| {
            let Some(own) = units[t] else {
                return [[0.0; 3]; 3];
            };
            (*corners).map(|v| {
                let mut sum = [0.0; 3];
                for &s in &incident[v as usize] {
                    if units[s].is_some_and(|n| dot(n, own) >= SMOOTH_COS) {
                        sum = add(sum, triangles[s].1);
                    }
                }
                unit(sum).unwrap_or(own)
            })
        })
        .collect()
}

/// The triangles at each vertex of `mesh`.
fn incident_triangles(mesh: &TriangleMesh) -> Vec<Vec<usize>> {
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); mesh.vertices.len()];
    for (t, corners) in mesh.triangles.iter().enumerate() {
        for v in corners {
            incident[*v as usize].push(t);
        }
    }
    incident
}

/// A bounding volume hierarchy of a mesh's triangles: nodes in depth-first
/// order, a leaf holding a run of `order`.
struct Bvh {
    /// Each triangle's corners and its area normal.
    triangles: Vec<([Vec3; 3], Vec3)>,
    /// The surface's unit normal at each triangle's corners
    /// ([`corner_normals`]).
    normals: Vec<[Vec3; 3]>,
    nodes: Vec<Node>,
    order: Vec<usize>,
    /// Each triangle's vertex indices and the triangles at each vertex: the
    /// surfaces around an edge ([`Bvh::across_edge`], mitcad#689).
    vertices: Vec<[u32; 3]>,
    incident: Vec<Vec<usize>>,
}

struct Node {
    min: Vec3,
    max: Vec3,
    /// A leaf: the first of its triangles in `order`; else the index of the
    /// second child (the first follows the node).
    index: usize,
    /// A leaf's number of triangles, 0 for inner nodes.
    count: usize,
}

impl Bvh {
    fn new(mesh: &TriangleMesh) -> Self {
        let triangles: Vec<([Vec3; 3], Vec3)> = (0..mesh.triangles.len())
            .map(|t| {
                let p = corners(mesh, t);
                (p, area_normal(&p))
            })
            .collect();
        let incident = incident_triangles(mesh);
        let normals = corner_normals(mesh, &triangles, &incident);
        let mut bvh = Self {
            triangles,
            normals,
            nodes: Vec::new(),
            order: (0..mesh.triangles.len()).collect(),
            vertices: mesh.triangles.clone(),
            incident,
        };
        if !bvh.order.is_empty() {
            let count = bvh.order.len();
            bvh.build(0, count);
        }
        bvh
    }

    fn centroid(&self, t: usize) -> Vec3 {
        let p = &self.triangles[t].0;
        scale(add(add(p[0], p[1]), p[2]), 1.0 / 3.0)
    }

    /// Builds the node of `order[start..end]`; returns its index.
    fn build(&mut self, start: usize, end: usize) -> usize {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        let mut cmin = [f64::INFINITY; 3];
        let mut cmax = [f64::NEG_INFINITY; 3];
        for &t in &self.order[start..end] {
            for p in &self.triangles[t].0 {
                min = std::array::from_fn(|a| min[a].min(p[a]));
                max = std::array::from_fn(|a| max[a].max(p[a]));
            }
            let c = self.centroid(t);
            cmin = std::array::from_fn(|a| cmin[a].min(c[a]));
            cmax = std::array::from_fn(|a| cmax[a].max(c[a]));
        }
        let node = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            index: start,
            count: end - start,
        });
        if end - start <= LEAF_SIZE {
            return node;
        }
        let axis = (0..3)
            .max_by(|&a, &b| (cmax[a] - cmin[a]).total_cmp(&(cmax[b] - cmin[b])))
            .unwrap_or(0);
        let middle = start + (end - start) / 2;
        let triangles = &self.triangles;
        let key = |t: &usize| {
            let p = &triangles[*t].0;
            p[0][axis] + p[1][axis] + p[2][axis]
        };
        self.order[start..end]
            .select_nth_unstable_by(middle - start, |a, b| key(a).total_cmp(&key(b)));
        self.nodes[node].count = 0;
        self.build(start, middle);
        let second = self.build(middle, end);
        self.nodes[node].index = second;
        node
    }

    /// The walls under triangle `t`: of its samples, the shortest ray along
    /// the surface's inward normal there (the corners' normals
    /// interpolated) to where it leaves the material, and the pieces where
    /// the wall is under `limit` (mm). When every sample is thin, the whole
    /// triangle; when only some are, the thin pieces of a subdivision
    /// [`REFINE`] times finer (mitcad#767). `stop` is asked every
    /// [`CANCEL_EVERY`] rays of the finer subdivision; once it says so, the
    /// triangle's pieces are left incomplete (the caller drops them all).
    fn wall(&self, t: usize, limit: f64, stop: &dyn Fn() -> bool) -> Wall {
        let p = &self.triangles[t].0;
        let mut wall = Wall::default();
        let cells = Cell::all(subdivision(p));
        let mut thin = Vec::new();
        for &cell in &cells {
            let below = wall.best.map_or(f64::INFINITY, |b| b.distance).max(limit);
            let Some(hit) = self.sample(t, cell, below) else {
                continue;
            };
            if wall.best.is_none_or(|b| hit.distance < b.distance) {
                wall.best = Some(hit);
            }
            if hit.distance < limit {
                thin.push(Piece {
                    cell,
                    exit: (hit.triangle, hit.to),
                });
            }
        }
        if thin.is_empty() {
            return wall;
        }
        if thin.len() == cells.len() {
            // Thin all over: the whole triangle, its wall's other side where
            // it is thinnest.
            if let Some(best) = wall.best {
                wall.pieces.push(Piece {
                    cell: Cell::WHOLE,
                    exit: (best.triangle, best.to),
                });
            }
            return wall;
        }
        // Thin in part: the thin pieces of the finer subdivision. Its
        // samples include the coarse ones ([`REFINE`]), so some are thin;
        // should none be (a ray rounded the other way), the coarse pieces.
        let fine = Cell::all(cells[0].k * REFINE);
        let mut pieces = Vec::new();
        for (n, &cell) in fine.iter().enumerate() {
            if n % CANCEL_EVERY == CANCEL_EVERY - 1 && stop() {
                return wall;
            }
            if let Some(hit) = self.sample(t, cell, limit)
                && hit.distance < limit
            {
                pieces.push(Piece {
                    cell,
                    exit: (hit.triangle, hit.to),
                });
            }
        }
        wall.pieces = if pieces.is_empty() { thin } else { pieces };
        wall
    }

    /// The wall at the sample of `cell` on triangle `t`: the ray along the
    /// surface's inward normal there to where it leaves the material. Only
    /// a ray shorter than `below` is checked for crossing an edge rather
    /// than a wall (mitcad#689, [`Bvh::across_edge`]; such a ray gives
    /// None); a longer one is answered unchecked, as the callers want
    /// nothing of it.
    fn sample(&self, t: usize, cell: Cell, below: f64) -> Option<Hit> {
        let (p, n) = &self.triangles[t];
        let own = unit(*n)?;
        let corners = &self.normals[t];
        let (from, w) = cell.sample(p);
        let blended = add(
            add(scale(corners[0], w[0]), scale(corners[1], w[1])),
            scale(corners[2], w[2]),
        );
        let outward = unit(blended)
            .filter(|s| dot(*s, own) >= SMOOTH_COS)
            .unwrap_or(own);
        let inward = scale(outward, -1.0);
        let (distance, triangle) = self.first_exit(from, inward, t)?;
        let to = add(from, scale(inward, distance));
        // Across the edge next to it, the ray measures the edge, not a wall
        // (mitcad#689).
        if distance < below && self.across_edge(t, from, triangle, to) {
            return None;
        }
        Some(Hit {
            distance,
            from,
            to,
            triangle,
        })
    }

    /// Whether a ray from `from` on triangle `start` that leaves the
    /// material at `to` on triangle `exit` only crossed the wedge at an edge
    /// (mitcad#689): the two surfaces meet across a sharp edge within
    /// [`EDGE_REACH`] of the start. Next to an edge whose faces meet at an
    /// acute angle (a split plane through a fillet, the rim of a hole through
    /// a curved wall) the ray along the normal leaves through the
    /// neighbouring face at once; that is the edge's shape, not a wall. The
    /// two sides of a real thin wall (a fin, a lip) are parallel or do not
    /// meet near the start, so their walls are still found.
    fn across_edge(&self, start: usize, from: Vec3, exit: usize, to: Vec3) -> bool {
        let (Some(a), Some(b)) = (unit(self.triangles[start].1), unit(self.triangles[exit].1))
        else {
            return false;
        };
        // Where the two triangles' planes meet: parallel sides never do.
        let sine = norm(cross(a, b));
        if sine.is_nan() || sine <= EDGE_SINE {
            return false;
        }
        // In each plane, the distance to that line.
        let from_line = dot(sub(from, self.triangles[exit].0[0]), b).abs() / sine;
        let to_line = dot(sub(to, self.triangles[start].0[0]), a).abs() / sine;
        let near = from_line <= EDGE_REACH && to_line <= EDGE_SEARCH;
        if !near {
            return false;
        }
        // The surfaces around the start and the exit (as far as the line,
        // each across its smooth neighbours) meet at a vertex: an edge of
        // the body, not the far side of a wall.
        let margin = EDGE_REACH / 4.0;
        let near_start = self.surface_near(start, from, from_line + margin);
        let near_exit = self.surface_near(exit, to, to_line + margin);
        let mut ids: Vec<u32> = near_start.iter().flat_map(|&t| self.vertices[t]).collect();
        ids.sort_unstable();
        near_exit
            .iter()
            .flat_map(|&t| self.vertices[t])
            .any(|v| ids.binary_search(&v).is_ok())
    }

    /// The triangles of the surface `seed` is on within `radius` of
    /// `around`: neighbours across shared vertices whose normals are within
    /// [`SMOOTH_COS`] of each other, at most [`EDGE_TRIANGLES`] of them.
    fn surface_near(&self, seed: usize, around: Vec3, radius: f64) -> Vec<usize> {
        let mut found = vec![seed];
        let mut seen = HashSet::from([seed]);
        let mut queue = VecDeque::from([seed]);
        while let Some(t) = queue.pop_front() {
            let Some(own) = unit(self.triangles[t].1) else {
                continue;
            };
            for v in self.vertices[t] {
                for &s in &self.incident[v as usize] {
                    if found.len() >= EDGE_TRIANGLES {
                        return found;
                    }
                    if seen.contains(&s)
                        || !unit(self.triangles[s].1).is_some_and(|n| dot(n, own) >= SMOOTH_COS)
                        || box_distance(&self.triangles[s].0, around) > radius
                    {
                        continue;
                    }
                    seen.insert(s);
                    found.push(s);
                    queue.push_back(s);
                }
            }
        }
        found
    }

    /// The nearest triangle but `skip` that the ray from `origin` along
    /// `direction` (unit) meets from inside (its outward normal along the
    /// ray): the distance and the triangle.
    fn first_exit(&self, origin: Vec3, direction: Vec3, skip: usize) -> Option<(f64, usize)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inverse = direction.map(|d| 1.0 / d);
        let mut nearest = f64::INFINITY;
        let mut found = None;
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            if !box_hit(node, origin, inverse, nearest) {
                continue;
            }
            if node.count == 0 {
                stack.push(node.index);
                stack.push(index + 1);
                continue;
            }
            for &t in &self.order[node.index..node.index + node.count] {
                if t == skip {
                    continue;
                }
                let (p, n) = &self.triangles[t];
                if dot(*n, direction) <= 0.0 {
                    continue;
                }
                if let Some(d) = ray_triangle(origin, direction, p)
                    && d > MIN_HIT
                    && d < nearest
                {
                    nearest = d;
                    found = Some(t);
                }
            }
        }
        found.map(|t| (nearest, t))
    }
}

/// The distance from `point` to the box around the triangle `p`: at most
/// the distance to the triangle.
fn box_distance(p: &[Vec3; 3], point: Vec3) -> f64 {
    let gap: Vec3 = std::array::from_fn(|a| {
        let low = p[0][a].min(p[1][a]).min(p[2][a]);
        let high = p[0][a].max(p[1][a]).max(p[2][a]);
        (low - point[a]).max(point[a] - high).max(0.0)
    });
    norm(gap)
}

/// Whether the ray enters the node's box nearer than `limit`.
fn box_hit(node: &Node, origin: Vec3, inverse: Vec3, limit: f64) -> bool {
    let mut near = 0.0_f64;
    let mut far = limit;
    let slabs = node.min.iter().zip(&node.max).zip(origin).zip(inverse);
    for (((low, high), origin), inverse) in slabs {
        // A ray parallel to the slab: inside it or not at all.
        if !inverse.is_finite() {
            if origin < low - 1e-9 || origin > high + 1e-9 {
                return false;
            }
            continue;
        }
        let t1 = (low - origin) * inverse;
        let t2 = (high - origin) * inverse;
        let (lo, hi) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        near = near.max(lo);
        far = far.min(hi);
        if near > far + 1e-9 {
            return false;
        }
    }
    true
}

/// The distance along `direction` to the triangle, edges included (the
/// classic ray and triangle test by barycentric coordinates).
fn ray_triangle(origin: Vec3, direction: Vec3, p: &[Vec3; 3]) -> Option<f64> {
    const EPS: f64 = 1e-12;
    const EDGE: f64 = 1e-9;
    let e1 = sub(p[1], p[0]);
    let e2 = sub(p[2], p[0]);
    let h = cross(direction, e2);
    let det = dot(e1, h);
    if det.abs() < EPS {
        return None;
    }
    let f = 1.0 / det;
    let s = sub(origin, p[0]);
    let u = f * dot(s, h);
    if !(-EDGE..=1.0 + EDGE).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = f * dot(direction, q);
    if v < -EDGE || u + v > 1.0 + EDGE {
        return None;
    }
    Some(f * dot(e2, q))
}

/// The settings of an estimate (mm, fractions, a price per kilogram).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EstimateSettings {
    pub line_width: f64,
    pub walls: u32,
    pub layer_height: f64,
    pub top_bottom_layers: u32,
    pub infill: f64,
    pub filament_diameter: f64,
    pub price_per_kg: Option<f64>,
}

impl Default for EstimateSettings {
    fn default() -> Self {
        Self {
            line_width: DEFAULT_LINE_WIDTH,
            walls: DEFAULT_WALLS,
            layer_height: DEFAULT_LAYER_HEIGHT,
            top_bottom_layers: DEFAULT_TOP_BOTTOM_LAYERS,
            infill: DEFAULT_INFILL,
            filament_diameter: DEFAULT_FILAMENT_DIAMETER,
            price_per_kg: None,
        }
    }
}

impl EstimateSettings {
    /// Why the settings make no estimate, if they do not.
    pub fn check(&self) -> Result<(), String> {
        let positive = |value: f64, what: &str| {
            if value > 0.0 && value.is_finite() {
                Ok(())
            } else {
                Err(format!("the {what} must be greater than zero, got {value}"))
            }
        };
        positive(self.line_width, "line width")?;
        positive(self.layer_height, "layer height")?;
        positive(self.filament_diameter, "filament diameter")?;
        if !(0.0..=1.0).contains(&self.infill) {
            return Err(format!(
                "the infill must be a fraction from 0 to 1, got {}",
                self.infill
            ));
        }
        if let Some(price) = self.price_per_kg
            && !(price >= 0.0 && price.is_finite())
        {
            return Err(format!("the price must not be negative, got {price}"));
        }
        Ok(())
    }
}

/// What printing a body takes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Estimate {
    /// The body's, mm³ and mm².
    pub volume: f64,
    pub area: f64,
    /// The walls: area × walls × line width (the top and bottom layers
    /// counted in it), at most the volume, mm³.
    pub shell_volume: f64,
    /// (volume − shell) × infill, mm³.
    pub infill_volume: f64,
    /// shell + infill, mm³.
    pub filament_volume: f64,
    /// Of the filament, g.
    pub mass: f64,
    /// Of the filament, m.
    pub length: f64,
    /// mass × price per kilogram, when a price is given.
    pub cost: Option<f64>,
}

/// The estimate of a body of `volume` (mm³) and `area` (mm²) printed in a
/// material of `density` (g/cm³).
pub fn estimate(volume: f64, area: f64, density: f64, settings: &EstimateSettings) -> Estimate {
    let volume = volume.max(0.0);
    let area = area.max(0.0);
    let shell_volume = (area * f64::from(settings.walls) * settings.line_width).min(volume);
    let infill_volume = (volume - shell_volume) * settings.infill;
    let filament_volume = shell_volume + infill_volume;
    // mm³ → cm³.
    let mass = filament_volume / 1000.0 * density;
    let radius = settings.filament_diameter / 2.0;
    // mm → m.
    let length = filament_volume / (std::f64::consts::PI * radius * radius) / 1000.0;
    Estimate {
        volume,
        area,
        shell_volume,
        infill_volume,
        filament_volume,
        mass,
        length,
        cost: settings.price_per_kg.map(|price| price * mass / 1000.0),
    }
}

/// The gap between bodies printed in place under which they fuse, mm
/// (mitcad#1519): about 0.3 mm on a common FDM printer. A design's user
/// parameter `print_in_place_gap` (a printed fit parameter) replaces it.
pub const DEFAULT_MIN_GAP: f64 = 0.3;
/// Bodies closer than this (mm) touch: the baseline tolerance. A gap at
/// the threshold is not narrow either.
pub const GAP_TOLERANCE: f64 = 1e-6;

/// What the smallest distance of two bodies makes of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// They touch, overlap or one lies inside the other: `interference`'s.
    Touching,
    /// Apart, but closer than the gap: they may fuse when printed.
    Narrow,
    /// At least the gap apart.
    Clear,
}

/// The kind of a pair `distance` mm apart (`inside`: one lies, partly,
/// inside the other's material) for the gap `gap` (mm).
pub fn gap_kind(distance: f64, inside: bool, gap: f64) -> GapKind {
    if inside || distance <= GAP_TOLERANCE {
        GapKind::Touching
    } else if distance < gap - GAP_TOLERANCE {
        GapKind::Narrow
    } else {
        GapKind::Clear
    }
}

/// The pairs `(i, j)`, `i < j`, of bodies whose boxes lie within `gap` of
/// each other: only they can be closer than the gap, so only they are
/// measured. A body without a box (an empty one) pairs with none.
pub fn gap_candidates(
    boxes: &[Option<crate::kernel::BoundingBox>],
    gap: f64,
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (i, a) in boxes.iter().enumerate() {
        let Some(a) = a else { continue };
        for (j, b) in boxes.iter().enumerate().skip(i + 1) {
            if let Some(b) = b
                && a.overlaps(b, gap)
            {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// Why a gap threshold (mm) is not one.
pub fn check_min_gap(gap: f64) -> Result<(), String> {
    if gap > 0.0 && gap.is_finite() {
        Ok(())
    } else {
        Err(format!("the gap must be greater than zero, got {gap}"))
    }
}

/// The [`PrintCache`] key of the distance of two placed bodies: each
/// body, its shape's version and its placement, in the order asked.
pub fn gap_key(a: (BodyUid, u128, &Transform), b: (BodyUid, u128, &Transform)) -> u128 {
    let mut fp = Fingerprint::new("print-gap-v1");
    for (body, version, placement) in [a, b] {
        fp.str(&body.to_string()).u128(version);
        for row in placement.linear {
            for value in row {
                fp.f64(value);
            }
        }
        for value in placement.translation {
            fp.f64(value);
        }
    }
    fp.finish()
}

/// Faces face each other across a gap when each one's outward normal at
/// its closest point lies within 30° of the line to the other face
/// (mitcad#1672): the cosine.
pub const FACING_COS: f64 = 0.866;

/// The narrowest gap between two bodies that touch (mitcad#1672), from
/// their faces' separations ([`crate::kernel::Kernel::face_separations`]):
/// a pair of faces that touch or cross is a contact and left out; of the
/// others, a pair narrow by [`gap_kind`] whose faces face each other
/// across air ([`faces_across_a_gap`]). None: no narrow gap beside the
/// contact.
pub fn face_gap(
    separations: &[crate::kernel::FaceSeparation],
    gap: f64,
) -> Option<&crate::kernel::FaceSeparation> {
    separations
        .iter()
        .filter(|s| gap_kind(s.distance, false, gap) == GapKind::Narrow && faces_across_a_gap(s))
        .min_by(|x, y| x.distance.total_cmp(&y.distance))
}

/// Whether the two faces face each other across air at their closest
/// points: the first's outward normal points to the second, the second's
/// back to the first, each within the angle of [`FACING_COS`]. Faces that
/// meet the contact at its edge only (a side face above a face the body
/// stands on) do not, and faces of bodies that overlap face away from
/// each other, so neither is a gap. A face without a normal there faces
/// nothing.
pub fn faces_across_a_gap(separation: &crate::kernel::FaceSeparation) -> bool {
    let Some(across) = unit(sub(separation.on_b, separation.on_a)) else {
        return false;
    };
    dot(separation.normal_a, across) >= FACING_COS
        && dot(separation.normal_b, across) <= -FACING_COS
}

/// The [`PrintCache`] key of the narrowest gap between the faces of two
/// bodies that touch, by the pair's [`gap_key`] and the gap asked for.
pub fn face_gap_key(pair: u128, gap: f64) -> u128 {
    let mut fp = Fingerprint::new("print-face-gap-v1");
    fp.u128(pair).f64(gap);
    fp.finish()
}

/// The mesh tolerance's part of a cache key.
fn tolerance_key(fp: &mut Fingerprint, tolerance: &MeshTolerance) {
    fp.f64(tolerance.deviation)
        .f64(tolerance.angle)
        .u64(tolerance.face_triangles);
}

/// The [`PrintCache`] key of a body's mesh: the body, its shape's version
/// and the mesh tolerance. The Print Analysis queries and `model_render`'s
/// overlay (mitcad#1488) share the meshes by it.
pub fn mesh_key(body: BodyUid, version: u128, tolerance: &MeshTolerance) -> u128 {
    let mut fp = Fingerprint::new("print-mesh-v1");
    fp.str(&body.to_string()).u128(version);
    tolerance_key(&mut fp, tolerance);
    fp.finish()
}

/// The [`PrintCache`] key of a body's result: what it is of (`tag`), the
/// body, its shape's version and placement, the mesh tolerance and the
/// settings.
pub fn result_key(
    tag: &str,
    body: BodyUid,
    version: u128,
    placement: &Transform,
    tolerance: &MeshTolerance,
    settings: &Value,
) -> u128 {
    let mut fp = Fingerprint::new(tag);
    fp.str(&body.to_string()).u128(version);
    for row in placement.linear {
        for value in row {
            fp.f64(value);
        }
    }
    for value in placement.translation {
        fp.f64(value);
    }
    tolerance_key(&mut fp, tolerance);
    fp.str(&settings.to_string());
    fp.finish()
}

/// What a body's walls are kept as ([`result_key`] with this tag and the
/// settings `{"minimum": …}`), by `print_thickness` and `model_render`'s
/// overlay alike (mitcad#1488).
pub const THICKNESS_TAG: &str = "print-thickness-v3";

/// A body's walls as they are kept: the thinnest, where it is and where
/// the other side is, the regions, and the thin pieces as 9 numbers each
/// in design coordinates (`triangles`).
pub fn thickness_value(found: &Thickness) -> Value {
    let triangles: Vec<[f64; 9]> = found.pieces.iter().map(flat_corners).collect();
    serde_json::json!({
        "thinnest": found.thinnest,
        "at": found.at,
        "to": found.to,
        "regions": found.regions,
        "triangles": triangles,
    })
}

/// Bounds of what the print cache keeps: meshes are large.
const CACHED_MESHES: usize = 8;
const CACHED_RESULTS: usize = 32;

/// The meshes and results of print analyses (mitcad#620), by the
/// fingerprint of what they depend on (the body's shape version, its
/// placement, the mesh tolerance and the settings), so that showing an
/// analysis again computes nothing. Separate from modelling results:
/// analysing changes no state. The oldest go first.
#[derive(Default)]
#[doc(hidden)]
pub struct PrintCache {
    inner: Mutex<CacheEntries>,
}

#[derive(Default)]
struct CacheEntries {
    meshes: VecDeque<(u128, Arc<TriangleMesh>)>,
    results: VecDeque<(u128, Arc<Value>)>,
}

fn find<T>(entries: &VecDeque<(u128, Arc<T>)>, key: u128) -> Option<Arc<T>> {
    entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| Arc::clone(v))
}

fn keep<T>(entries: &mut VecDeque<(u128, Arc<T>)>, key: u128, value: Arc<T>, bound: usize) {
    entries.retain(|(k, _)| *k != key);
    entries.push_back((key, value));
    while entries.len() > bound {
        entries.pop_front();
    }
}

impl PrintCache {
    pub fn mesh(&self, key: u128) -> Option<Arc<TriangleMesh>> {
        self.inner.lock().ok().and_then(|c| find(&c.meshes, key))
    }

    pub fn keep_mesh(&self, key: u128, mesh: Arc<TriangleMesh>) {
        if let Ok(mut c) = self.inner.lock() {
            keep(&mut c.meshes, key, mesh, CACHED_MESHES);
        }
    }

    pub fn result(&self, key: u128) -> Option<Arc<Value>> {
        self.inner.lock().ok().and_then(|c| find(&c.results, key))
    }

    pub fn keep_result(&self, key: u128, result: Arc<Value>) {
        if let Ok(mut c) = self.inner.lock() {
            keep(&mut c.results, key, result, CACHED_RESULTS);
        }
    }
}
