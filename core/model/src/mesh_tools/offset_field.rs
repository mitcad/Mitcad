// SPDX-License-Identifier: MIT
//! The offset of a closed mesh built from its distance (mitcad#2564).
//!
//! Where the vertices [`super::offset`] moves turn triangles over (an
//! offset towards the inside of a sharp edge farther than the triangles
//! beside it are wide, a scan's fine triangles along every edge), the
//! offset of a closed mesh is built again as the surface where its signed
//! distance (negative inside; the sign by the angle-weighted normal at the
//! nearest point) is the distance: the parts that turned over are not part
//! of it, a sharp edge or corner towards which it moves stays sharp, one
//! away from which it moves becomes round, as an offset does.
//!
//! The surface is contoured on a grid of cubes (dual contouring): each
//! cube the surface passes through gets one vertex for each piece of the
//! surface in it (the pieces by the signs at its corners; a face whose
//! corners alternate keeps its inside corners apart, so that the result
//! is closed and manifold), placed where the planes through the surface's
//! crossings of the cube's edges, square to the distance's gradient there,
//! meet best (least squares; the directions in which they hardly differ
//! left out): on a plane, on the line of a sharp edge, at a sharp corner
//! exactly. Each grid edge the surface crosses gives a quad of the four
//! cubes round it. Only cubes near the surface are looked at: a box of
//! cubes whose middle lies farther from the surface than its corners are
//! from its middle holds none of it (the distance changes by at most the
//! move).
//!
//! A Shell's side with openings is the surface where the distance to the
//! surface kept (the openings taken out) is the thickness, inside (or
//! outside) the closed mesh: it ends on the openings, where its rim is
//! joined to the other side's rim by a strip across the wall.

use std::collections::{BTreeSet, HashMap};

use mitcad_vec3::{add, dot, norm, scale, sub};

use super::bvh::{Bvh, TriangleRegion, closest_on_triangle_at};
use super::{Point, TriangleMesh, corners, edge_key, edge_triangles, unit_normal};

/// The signed distance to a closed mesh facing out.
struct SignedDistance {
    tree: Bvh,
    triangles: Vec<[u32; 3]>,
    face_normals: Vec<Point>,
    edge_normals: HashMap<(u32, u32), Point>,
    vertex_normals: Vec<Point>,
}

impl SignedDistance {
    fn new(mesh: &TriangleMesh) -> Self {
        let face_normals: Vec<Point> = (0..mesh.triangles.len())
            .map(|t| unit_normal(&corners(mesh, t)))
            .collect();
        let mut edge_normals: HashMap<(u32, u32), Point> = HashMap::new();
        let mut vertex_normals = vec![[0.0; 3]; mesh.vertices.len()];
        for (t, tri) in mesh.triangles.iter().enumerate() {
            let n = face_normals[t];
            let p = corners(mesh, t);
            for k in 0..3 {
                let along = edge_normals
                    .entry(edge_key(tri[k], tri[(k + 1) % 3]))
                    .or_insert([0.0; 3]);
                *along = add(*along, n);
                let a = sub(p[(k + 1) % 3], p[k]);
                let b = sub(p[(k + 2) % 3], p[k]);
                let (la, lb) = (norm(a), norm(b));
                if la > 0.0 && lb > 0.0 {
                    let angle = (dot(a, b) / (la * lb)).clamp(-1.0, 1.0).acos();
                    let v = tri[k] as usize;
                    vertex_normals[v] = add(vertex_normals[v], scale(n, angle));
                }
            }
        }
        SignedDistance {
            tree: Bvh::new(mesh),
            triangles: mesh.triangles.clone(),
            face_normals,
            edge_normals,
            vertex_normals,
        }
    }

    /// The signed distance at `p` and its gradient (unit, pointing out);
    /// within `near` of the mesh the gradient is the mesh's normal there.
    fn at(&self, p: Point, near: f64) -> (f64, Point) {
        let Some(closest) = self.tree.closest(p) else {
            return (f64::INFINITY, [0.0, 0.0, 1.0]);
        };
        let t = closest.triangle;
        let tri = self.triangles[t];
        let (_, region) = closest_on_triangle_at(p, self.tree.triangle(t));
        let normal = match region {
            TriangleRegion::Inside => self.face_normals[t],
            TriangleRegion::Edge(k) => self
                .edge_normals
                .get(&edge_key(tri[k], tri[(k + 1) % 3]))
                .copied()
                .unwrap_or(self.face_normals[t]),
            TriangleRegion::Corner(k) => self.vertex_normals[tri[k] as usize],
        };
        let d = sub(p, closest.point);
        let sign = if dot(d, normal) >= 0.0 { 1.0 } else { -1.0 };
        let gradient = if closest.distance > near {
            scale(d, sign / closest.distance)
        } else {
            mitcad_vec3::unit(normal).unwrap_or(self.face_normals[t])
        };
        (sign * closest.distance, gradient)
    }
}

/// A part of the field whose largest value is the field's.
#[derive(Debug, Clone, Copy)]
enum Term {
    /// `sign` times the signed distance to the closed mesh, less `level`.
    Signed { sign: f64, level: f64 },
    /// `radius` less the distance to the surface kept (a Shell's openings
    /// taken out).
    Kept { radius: f64 },
}

/// The field whose negative side is the region bounded by the surface
/// sought; it changes by at most the length of a move.
struct Field<'a> {
    full: &'a SignedDistance,
    kept: Option<&'a Bvh>,
    terms: Vec<Term>,
    near: f64,
}

impl Field<'_> {
    /// The value at `p`, its gradient (unit, out of the region) and the
    /// term that gives it.
    fn at(&self, p: Point) -> (f64, Point, usize) {
        let mut best = (f64::NEG_INFINITY, [0.0, 0.0, 1.0], 0);
        let mut signed: Option<(f64, Point)> = None;
        for (k, term) in self.terms.iter().enumerate() {
            let (value, gradient) = match *term {
                Term::Signed { sign, level } => {
                    let (s, g) = *signed.get_or_insert_with(|| self.full.at(p, self.near));
                    (sign * s - level, scale(g, sign))
                }
                Term::Kept { radius } => {
                    let Some(closest) = self.kept.and_then(|tree| tree.closest(p)) else {
                        continue;
                    };
                    let d = sub(p, closest.point);
                    let gradient = if closest.distance > self.near {
                        scale(d, -1.0 / closest.distance)
                    } else {
                        [0.0, 0.0, 1.0]
                    };
                    (radius - closest.distance, gradient)
                }
            };
            if value > best.0 {
                best = (value, gradient, k);
            }
        }
        best
    }
}

/// Where the surface crosses a grid edge.
#[derive(Debug, Clone, Copy)]
struct Crossing {
    point: Point,
    normal: Point,
    term: usize,
    /// The edge's first node inside the region.
    inside_first: bool,
}

/// A cube's pieces of the surface: the piece each of its twelve edges
/// belongs to (`NONE` where the surface does not cross it) and each
/// piece's vertex.
#[derive(Debug, Clone)]
struct CubePieces {
    piece_of_edge: [u8; 12],
    vertices: Vec<Point>,
}

const NONE: u8 = u8::MAX;

/// Each face of a cube by its corners in order round it (corner `c` at
/// `(c & 1, (c >> 1) & 1, (c >> 2) & 1)`).
const FACES: [[usize; 4]; 6] = [
    [0, 2, 6, 4],
    [1, 3, 7, 5],
    [0, 1, 5, 4],
    [2, 3, 7, 6],
    [0, 1, 3, 2],
    [4, 5, 7, 6],
];

/// The cube's edge between two corners that differ along one axis: four
/// edges along each axis, numbered by the other two axes' bits.
fn cube_edge(a: usize, b: usize) -> usize {
    let axis = (a ^ b).trailing_zeros() as usize;
    let base = a.min(b);
    let index = match axis {
        0 => (base >> 1) & 3,
        1 => (base & 1) | (((base >> 2) & 1) << 1),
        _ => base & 3,
    };
    axis * 4 + index
}

/// The corner an edge starts at (its lower end) and its axis.
fn edge_start(edge: usize) -> (usize, usize) {
    let (axis, index) = (edge / 4, edge % 4);
    let start = match axis {
        0 => ((index & 1) << 1) | ((index >> 1) << 2),
        1 => (index & 1) | ((index >> 1) << 2),
        _ => index,
    };
    (start, axis)
}

fn corner_offset(corner: usize) -> [usize; 3] {
    [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1]
}

fn node_key(i: [usize; 3]) -> u64 {
    i[0] as u64 | ((i[1] as u64) << 21) | ((i[2] as u64) << 42)
}

struct Contour<'a> {
    field: &'a Field<'a>,
    origin: Point,
    h: f64,
    values: HashMap<u64, f64>,
    /// Cubes with corners on both sides.
    mixed: Vec<[usize; 3]>,
    crossings: HashMap<(u64, usize), Crossing>,
    pieces: HashMap<u64, CubePieces>,
}

impl<'a> Contour<'a> {
    fn point(&self, i: [usize; 3]) -> Point {
        std::array::from_fn(|a| self.origin[a] + self.h * i[a] as f64)
    }

    fn value(&mut self, i: [usize; 3]) -> f64 {
        let key = node_key(i);
        if let Some(v) = self.values.get(&key) {
            return *v;
        }
        let v = self.field.at(self.point(i)).0;
        self.values.insert(key, v);
        v
    }

    /// Finds the cubes of the box from `low`, `size` cubes on a side,
    /// with corners on both sides.
    fn sweep(&mut self, low: [usize; 3], size: usize) {
        if size == 1 {
            let inside: Vec<bool> = (0..8)
                .map(|c| {
                    let o = corner_offset(c);
                    self.value(std::array::from_fn(|a| low[a] + o[a])) < 0.0
                })
                .collect();
            if inside.iter().any(|x| *x) && inside.iter().any(|x| !*x) {
                self.mixed.push(low);
            }
            return;
        }
        let half = size / 2;
        let middle: [usize; 3] = std::array::from_fn(|a| low[a] + half);
        let v = self.value(middle);
        let reach = size as f64 * self.h * 0.5 * 3f64.sqrt();
        if v.abs() > reach * (1.0 + 1e-9) {
            return;
        }
        for octant in 0..8 {
            let o = corner_offset(octant);
            self.sweep(std::array::from_fn(|a| low[a] + o[a] * half), half);
        }
    }

    /// Where the surface crosses the grid edge from `node` along `axis`
    /// (its ends on both sides): by false position on the field.
    fn crossing(&mut self, node: [usize; 3], axis: usize) -> Crossing {
        let key = (node_key(node), axis);
        if let Some(c) = self.crossings.get(&key) {
            return *c;
        }
        let mut end = node;
        end[axis] += 1;
        let (fa, fb) = (self.value(node), self.value(end));
        let (pa, pb) = (self.point(node), self.point(end));
        let at = |t: f64| add(pa, scale(sub(pb, pa), t));
        let (mut t0, mut f0, mut t1, mut f1) = (0.0, fa, 1.0, fb);
        let mut last = 0i8;
        let mut t = 0.5;
        for _ in 0..80 {
            t = (t0 * f1 - t1 * f0) / (f1 - f0);
            if !(t > t0 && t < t1) {
                t = 0.5 * (t0 + t1);
            }
            let f = self.field.at(at(t)).0;
            if f.abs() <= 1e-10 * self.h || (t1 - t0) <= 1e-12 {
                break;
            }
            if (f < 0.0) == (f0 < 0.0) {
                t0 = t;
                f0 = f;
                if last == -1 {
                    f1 *= 0.5;
                }
                last = -1;
            } else {
                t1 = t;
                f1 = f;
                if last == 1 {
                    f0 *= 0.5;
                }
                last = 1;
            }
        }
        let point = at(t);
        let (_, normal, term) = self.field.at(point);
        let c = Crossing {
            point,
            normal,
            term,
            inside_first: fa < 0.0,
        };
        self.crossings.insert(key, c);
        c
    }

    /// The cube's pieces of the surface and their vertices.
    fn cube(&mut self, low: [usize; 3]) -> CubePieces {
        let key = node_key(low);
        if let Some(p) = self.pieces.get(&key) {
            return p.clone();
        }
        let inside: [bool; 8] = std::array::from_fn(|c| {
            let o = corner_offset(c);
            self.value(std::array::from_fn(|a| low[a] + o[a])) < 0.0
        });
        let mut parent: [usize; 12] = std::array::from_fn(|e| e);
        fn root(parent: &mut [usize; 12], mut e: usize) -> usize {
            while parent[e] != e {
                parent[e] = parent[parent[e]];
                e = parent[e];
            }
            e
        }
        let mut join = |a: usize, b: usize| {
            let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        };
        let crossed = |e: usize| {
            let (start, axis) = edge_start(e);
            inside[start] != inside[start | (1 << axis)]
        };
        for face in FACES {
            let edges: [usize; 4] = std::array::from_fn(|k| cube_edge(face[k], face[(k + 1) % 4]));
            let cut: Vec<usize> = edges.iter().copied().filter(|e| crossed(*e)).collect();
            match cut.len() {
                2 => join(cut[0], cut[1]),
                4 => {
                    // Corners alternating: each inside corner apart, its
                    // two edges one piece.
                    for k in 0..4 {
                        if inside[face[k]] {
                            join(edges[(k + 3) % 4], edges[k]);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut piece_of_edge = [NONE; 12];
        let mut roots: Vec<usize> = Vec::new();
        for (e, slot) in piece_of_edge.iter_mut().enumerate() {
            if !crossed(e) {
                continue;
            }
            let r = root(&mut parent, e);
            let piece = match roots.iter().position(|x| *x == r) {
                Some(p) => p,
                None => {
                    roots.push(r);
                    roots.len() - 1
                }
            };
            *slot = piece as u8;
        }
        let low_point = self.point(low);
        let high_point = add(low_point, [self.h; 3]);
        let mut vertices = Vec::with_capacity(roots.len());
        for piece in 0..roots.len() {
            let mut points = Vec::new();
            let mut normals = Vec::new();
            for (e, of) in piece_of_edge.iter().enumerate() {
                if *of as usize != piece {
                    continue;
                }
                let (start, axis) = edge_start(e);
                let o = corner_offset(start);
                let c = self.crossing(std::array::from_fn(|a| low[a] + o[a]), axis);
                points.push(c.point);
                normals.push(c.normal);
            }
            vertices.push(best_point(&points, &normals, low_point, high_point, self.h));
        }
        let made = CubePieces {
            piece_of_edge,
            vertices,
        };
        self.pieces.insert(key, made.clone());
        made
    }
}

/// The point nearest, in least squares, to the planes through `points`
/// square to `normals`: the directions in which the normals hardly differ
/// left out (about their mean point there), and more of them while the
/// point lies more than a cube outside the cube from `low` to `high`.
fn best_point(points: &[Point], normals: &[Point], low: Point, high: Point, h: f64) -> Point {
    let count = points.len().max(1) as f64;
    let mut mean = [0.0; 3];
    for p in points {
        mean = add(mean, scale(*p, 1.0 / count));
    }
    let mut a = [[0.0f64; 3]; 3];
    let mut b = [0.0f64; 3];
    for (p, n) in points.iter().zip(normals) {
        let along = dot(*n, sub(*p, mean));
        for i in 0..3 {
            for j in 0..3 {
                a[i][j] += n[i] * n[j];
            }
            b[i] += n[i] * along;
        }
    }
    let (values, vectors) = super::symmetric_eigen(a);
    let largest = values[2];
    if largest <= 0.0 {
        return mean;
    }
    for share in [0.02, 0.1, 0.5] {
        let mut x = mean;
        for k in 0..3 {
            if values[k] > share * largest {
                x = add(x, scale(vectors[k], dot(vectors[k], b) / values[k]));
            }
        }
        if (0..3).all(|i| x[i] >= low[i] - h && x[i] <= high[i] + h) {
            return x;
        }
    }
    mean
}

/// The surface where `field` is 0 on a grid of cubes `h` on a side over
/// the box from `low` to `high` (which holds it), facing out of the
/// field's negative side; only the quads of crossings by term `keep` when
/// given.
fn contour(field: &Field, low: Point, high: Point, h: f64, keep: Option<usize>) -> TriangleMesh {
    // The grid a little past the box, its nodes off round numbers (a
    // node on a face of the mesh would sit exactly on the surface).
    let pad = 3.237_067_977;
    let origin: Point = std::array::from_fn(|i| low[i] - pad * h);
    let extent = (0..3).map(|i| high[i] - low[i]).fold(0.0f64, f64::max);
    let needed = (extent / h).ceil() as usize + 7;
    let cubes = needed.next_power_of_two();
    let mut c = Contour {
        field,
        origin,
        h,
        values: HashMap::new(),
        mixed: Vec::new(),
        crossings: HashMap::new(),
        pieces: HashMap::new(),
    };
    c.sweep([0; 3], cubes);
    // Each grid edge with ends on both sides, once.
    let mut edges: BTreeSet<([usize; 3], usize)> = BTreeSet::new();
    let mixed = std::mem::take(&mut c.mixed);
    for low in &mixed {
        for e in 0..12 {
            let (start, axis) = edge_start(e);
            let o = corner_offset(start);
            let node: [usize; 3] = std::array::from_fn(|a| low[a] + o[a]);
            let mut end = node;
            end[axis] += 1;
            if (c.value(node) < 0.0) != (c.value(end) < 0.0) {
                edges.insert((node, axis));
            }
        }
    }
    let mut out = TriangleMesh::default();
    let mut index: HashMap<(u64, u8), u32> = HashMap::new();
    'edges: for (node, axis) in edges {
        let crossing = c.crossing(node, axis);
        if keep.is_some_and(|k| k != crossing.term) {
            continue;
        }
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut quad = [0u32; 4];
        // The four cubes round the edge, counter-clockwise seen from
        // along the axis.
        for (slot, (du, dv)) in [(1, 1), (0, 1), (0, 0), (1, 0)].into_iter().enumerate() {
            let mut low = node;
            let (Some(lu), Some(lv)) = (node[u].checked_sub(du), node[v].checked_sub(dv)) else {
                continue 'edges;
            };
            low[u] = lu;
            low[v] = lv;
            let start = (du << u) | (dv << v);
            let edge = cube_edge(start, start | (1 << axis));
            let pieces = c.cube(low);
            let piece = pieces.piece_of_edge[edge];
            if piece == NONE {
                continue 'edges;
            }
            let at = *index.entry((node_key(low), piece)).or_insert_with(|| {
                out.vertices.push(pieces.vertices[piece as usize]);
                (out.vertices.len() - 1) as u32
            });
            quad[slot] = at;
        }
        if !crossing.inside_first {
            quad.reverse();
        }
        // The diagonal whose two triangles face most along the surface's
        // normal there.
        let facing = |tris: [[u32; 3]; 2]| {
            tris.iter()
                .map(|t| {
                    let p = t.map(|v| out.vertices[v as usize]);
                    dot(unit_normal(&p), crossing.normal)
                })
                .fold(f64::INFINITY, f64::min)
        };
        let first = [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]];
        let second = [[quad[0], quad[1], quad[3]], [quad[1], quad[2], quad[3]]];
        let tris = if facing(second) > facing(first) {
            second
        } else {
            first
        };
        out.triangles.extend(tris);
    }
    out
}

/// The grid's spacing for an offset by `distance` of a mesh of size
/// `extent` (its box's diagonal): a quarter of the distance, at most 400
/// cubes across.
fn spacing(distance: f64, extent: f64) -> f64 {
    (distance.abs() / 4.0).max(extent / 400.0).max(1e-9)
}

fn bounds(mesh: &TriangleMesh) -> (Point, Point) {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for tri in &mesh.triangles {
        for v in tri {
            let p = mesh.vertices[*v as usize];
            for i in 0..3 {
                low[i] = low[i].min(p[i]);
                high[i] = high[i].max(p[i]);
            }
        }
    }
    (low, high)
}

/// Whether the mesh is closed and facing one way: each edge on two
/// triangles that run along it in opposite directions, the volume
/// positive (facing out).
pub(crate) fn closed_solid(mesh: &TriangleMesh) -> bool {
    if mesh.triangles.is_empty() {
        return false;
    }
    let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
    for tri in &mesh.triangles {
        for k in 0..3 {
            *directed.entry((tri[k], tri[(k + 1) % 3])).or_default() += 1;
        }
    }
    directed
        .iter()
        .all(|(&(a, b), &n)| n == 1 && directed.get(&(b, a)) == Some(&1))
        && super::volume(mesh) > 0.0
}

/// The surface `distance` (outward positive) from the closed `mesh`
/// ([`closed_solid`]): where its signed distance is `distance`, facing
/// out.
pub(crate) fn offset_surface(mesh: &TriangleMesh, distance: f64) -> TriangleMesh {
    let full = SignedDistance::new(mesh);
    let (low, high) = bounds(mesh);
    let h = spacing(distance, norm(sub(high, low)));
    let grow = distance.max(0.0);
    let field = Field {
        full: &full,
        kept: None,
        terms: vec![Term::Signed {
            sign: 1.0,
            level: distance,
        }],
        near: 1e-6 * h,
    };
    contour(
        &field,
        low.map(|x| x - grow),
        high.map(|x| x + grow),
        h,
        None,
    )
}

/// A side of a Shell's wall, `distance` from `kept` (the closed `mesh`
/// less its openings; None: no openings): inside the mesh where the
/// distance is negative, outside where positive; facing away from the
/// wall's side of it.
pub(crate) fn wall_side(
    mesh: &TriangleMesh,
    kept: Option<&TriangleMesh>,
    distance: f64,
) -> TriangleMesh {
    let full = SignedDistance::new(mesh);
    let (low, high) = bounds(mesh);
    let h = spacing(distance, norm(sub(high, low)));
    let grow = distance.max(0.0);
    let sign = if distance < 0.0 { 1.0 } else { -1.0 };
    let kept_tree = kept.map(Bvh::new);
    let (terms, keep) = match &kept_tree {
        // Inside: s < distance; outside: s > distance.
        None => (
            vec![Term::Signed {
                sign,
                level: sign * distance,
            }],
            None,
        ),
        // On the mesh's side (sign s < 0) and farther than |distance|
        // from the surface kept; only that part (term 1) is the side.
        Some(_) => (
            vec![
                Term::Signed { sign, level: 0.0 },
                Term::Kept {
                    radius: distance.abs(),
                },
            ],
            Some(1),
        ),
    };
    let field = Field {
        full: &full,
        kept: kept_tree.as_ref(),
        terms,
        near: 1e-6 * h,
    };
    contour(
        &field,
        low.map(|x| x - grow),
        high.map(|x| x + grow),
        h,
        keep,
    )
}

/// A mesh's shape as far as an offset must keep it: its pieces, its Euler
/// characteristic (one less for each hole through it), its rims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape {
    pub pieces: usize,
    pub euler: i64,
    pub rims: usize,
}

pub(crate) fn shape(mesh: &TriangleMesh) -> Shape {
    let n = mesh.vertices.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut v: usize) -> usize {
        while parent[v] != v {
            parent[v] = parent[parent[v]];
            v = parent[v];
        }
        v
    }
    let mut used = vec![false; n];
    for tri in &mesh.triangles {
        for k in 0..3 {
            used[tri[k] as usize] = true;
            let (a, b) = (
                root(&mut parent, tri[k] as usize),
                root(&mut parent, tri[(k + 1) % 3] as usize),
            );
            if a != b {
                parent[a.max(b)] = a.min(b);
            }
        }
    }
    let vertices = used.iter().filter(|u| **u).count();
    let pieces = (0..n)
        .filter(|&v| used[v] && root(&mut parent, v) == v)
        .count();
    let edges = edge_triangles(&mesh.triangles).len();
    Shape {
        pieces,
        euler: vertices as i64 - edges as i64 + mesh.triangles.len() as i64,
        rims: super::repair::boundary_loops(&mesh.triangles).len(),
    }
}

/// The places where the surface's sides come closer than an offset by
/// `distance` (outward positive) allows: from a triangle's middle, along
/// its normal the way the offset goes, a part of the surface facing back
/// lies nearer than twice the distance (a wall thinner than that, a hole
/// narrower). Their count and the first places, apart from each other.
pub(crate) fn thin_places(surface: &TriangleMesh, distance: f64) -> (usize, Vec<Point>) {
    let tree = Bvh::new(surface);
    let normals: Vec<Point> = (0..surface.triangles.len())
        .map(|t| unit_normal(&corners(surface, t)))
        .collect();
    let spacing = 0.05 * super::size(&surface.vertices);
    let mut count = 0;
    let mut places: Vec<Point> = Vec::new();
    for t in 0..surface.triangles.len() {
        let n = normals[t];
        if n == [0.0; 3] {
            continue;
        }
        let middle = super::centroid(&corners(surface, t));
        let way = if distance < 0.0 { scale(n, -1.0) } else { n };
        let across = 2.0 * distance.abs() * (1.0 - 1e-6);
        let hit = tree.first_hit(middle, way, across, |other| {
            other != t && dot(normals[other], n) < -0.5
        });
        if hit.is_some() {
            count += 1;
            if places.len() < super::check::MAX_PLACES
                && places.iter().all(|q| norm(sub(*q, middle)) >= spacing)
            {
                places.push(middle);
            }
        }
    }
    (count, places)
}

/// The loops of the mesh's open edges, each in the direction its
/// triangle runs along it; None where a vertex has two open edges going
/// out.
fn directed_loops(triangles: &[[u32; 3]]) -> Option<Vec<Vec<u32>>> {
    let edges = edge_triangles(triangles);
    let mut next: HashMap<u32, u32> = HashMap::new();
    for tri in triangles {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            if edges.get(&edge_key(a, b)).is_some_and(|on| on.len() == 1)
                && next.insert(a, b).is_some()
            {
                return None;
            }
        }
    }
    let mut starts: Vec<u32> = next.keys().copied().collect();
    starts.sort_unstable();
    let mut done: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut loops = Vec::new();
    for start in starts {
        if done.contains(&start) {
            continue;
        }
        let mut lp = Vec::new();
        let mut at = start;
        loop {
            if !done.insert(at) {
                return None;
            }
            lp.push(at);
            at = *next.get(&at)?;
            if at == start {
                break;
            }
        }
        loops.push(lp);
    }
    Some(loops)
}

/// Joins each rim of a wall's outer side (vertices below `outer_count`)
/// to the rim of its inner side nearest to it by a strip of triangles
/// across the wall; false where the rims do not pair up.
pub(crate) fn join_rims(wall: &mut TriangleMesh, outer_count: u32) -> bool {
    let Some(loops) = directed_loops(&wall.triangles) else {
        return false;
    };
    let (outer, inner): (Vec<Vec<u32>>, Vec<Vec<u32>>) =
        loops.into_iter().partition(|lp| lp[0] < outer_count);
    if outer.len() != inner.len() || outer.iter().chain(&inner).any(|lp| lp.len() < 3) {
        return false;
    }
    let at = |v: u32| wall.vertices[v as usize];
    let mut used = vec![false; inner.len()];
    let mut strips = Vec::new();
    for p in &outer {
        // The inner rim nearest, by the mean distance from this rim's
        // vertices to its nearest vertex.
        let gap = |q: &Vec<u32>| {
            p.iter()
                .map(|a| {
                    q.iter()
                        .map(|b| norm(sub(at(*a), at(*b))))
                        .fold(f64::INFINITY, f64::min)
                })
                .sum::<f64>()
                / p.len() as f64
        };
        let Some(j) = (0..inner.len())
            .filter(|j| !used[*j])
            .min_by(|&x, &y| gap(&inner[x]).total_cmp(&gap(&inner[y])))
        else {
            return false;
        };
        used[j] = true;
        // The inner rim runs the other way round: reversed, both run
        // alike, its start the vertex nearest the outer rim's start.
        let mut q: Vec<u32> = inner[j].iter().rev().copied().collect();
        let first = (0..q.len())
            .min_by(|&x, &y| {
                norm(sub(at(q[x]), at(p[0]))).total_cmp(&norm(sub(at(q[y]), at(p[0]))))
            })
            .unwrap_or(0);
        q.rotate_left(first);
        let (n, m) = (p.len(), q.len());
        let (mut i, mut k) = (0usize, 0usize);
        while i < n || k < m {
            let along_outer = if i == n {
                false
            } else if k == m {
                true
            } else {
                norm(sub(at(p[(i + 1) % n]), at(q[k % m])))
                    <= norm(sub(at(p[i % n]), at(q[(k + 1) % m])))
            };
            if along_outer {
                strips.push([p[(i + 1) % n], p[i % n], q[k % m]]);
                i += 1;
            } else {
                strips.push([p[i % n], q[k % m], q[(k + 1) % m]]);
                k += 1;
            }
        }
    }
    wall.triangles.extend(strips);
    true
}
