// SPDX-License-Identifier: MIT
//! A closed mesh of a point cloud (mitcad#2516):
//!
//! 1. each point's normal estimated: the direction of least spread of its
//!    nearest points (their covariance's smallest eigenvector);
//! 2. the normals oriented alike: from the highest point, whose normal
//!    points up, along a minimum spanning tree of the neighbours, the most
//!    parallel neighbours first (a normal is turned over where it points
//!    against the one it is reached from);
//! 3. a signed distance sampled on a grid of cubes (the detail: cubes along
//!    the cloud's longest side): at each grid point, the distances from the
//!    tangent planes of the 8 nearest points (negative behind them,
//!    inside), each weighted by exp(-(d² - d₀²) / r²) for its distance d
//!    beyond the nearest one's d₀, r two of the points' usual spacings: on
//!    a face the plane itself; along a sharp edge rounded a little, where
//!    the nearest point alone would make ripples and bubbles;
//! 4. its zero level meshed by marching tetrahedra (each cube cut into six
//!    tetrahedra along its main diagonal, alike in every cube, so the
//!    triangles of neighbouring cubes share their edges): a closed,
//!    manifold mesh, its triangles facing outwards;
//! 5. shells that lie away from the points (more than a fifth of their
//!    corners farther than two cubes, or three times the points' usual
//!    spacing, from every point: a surface guessed where there are no
//!    points, such as round a flat scan) left out, and shells smaller than
//!    four cubes across: bubbles the field leaves where the nearest point
//!    and its tangent plane change, by up to half the points' spacing,
//!    along a sharp edge.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use mitcad_vec3::{cross, dot, norm, sub};

use super::kd::KdTree;
use super::matching::TrianglesTarget;
use super::{Vec3, centroid_and_scatter, symmetric_eigen, thinned};
use crate::exchange::TriangleMesh;

/// The fewest points a mesh is built of.
pub const MIN_POINTS: usize = 20;
/// The detail's range and default: cubes along the cloud's longest side.
pub const MIN_DETAIL: usize = 8;
pub const MAX_DETAIL: usize = 256;
pub const DEFAULT_DETAIL: usize = 64;
/// The neighbours a normal is estimated of, by default and at most.
pub const DEFAULT_NEIGHBOURS: usize = 12;
pub const MAX_NEIGHBOURS: usize = 100;
/// Grid points round the cloud's box, so that the surface closes inside it.
const PAD: usize = 3;
/// The nearest points whose tangent planes a grid point's value averages.
const FIELD_NEIGHBOURS: usize = 8;
/// A shell whose box's diagonal is shorter than this many cubes is a
/// bubble of the field, left out (step 5 of the module).
const SMALL_SHELL: f64 = 4.0;
/// The cloud's points the mesh's deviation is measured at, at most.
const DEVIATION_SAMPLES: usize = 20_000;

/// How a mesh is built.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReconstructOptions {
    pub detail: usize,
    pub neighbours: usize,
}

impl Default for ReconstructOptions {
    fn default() -> Self {
        Self {
            detail: DEFAULT_DETAIL,
            neighbours: DEFAULT_NEIGHBOURS,
        }
    }
}

/// The mesh built and what was found on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Reconstruction {
    pub mesh: TriangleMesh,
    /// The grid's cube size (mm).
    pub cell: f64,
    /// The closed shells kept, those left out away from the points, and
    /// the bubbles left out (shells under four cubes across).
    pub shells: usize,
    pub dropped_shells: usize,
    pub small_shells: usize,
    /// Every edge of the mesh is shared by exactly two triangles.
    pub closed: bool,
    /// The volume the mesh encloses (mm³).
    pub volume: f64,
    /// The cloud's points' distances from the mesh: mean and largest (mm),
    /// of at most 20 000 of them.
    pub mean_deviation: f64,
    pub max_deviation: f64,
}

/// Why no mesh was built.
#[derive(Debug, Clone, PartialEq)]
pub enum ReconstructError {
    Refused(String),
    Cancelled,
}

impl std::fmt::Display for ReconstructError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(why) => f.write_str(why),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

fn refused(why: impl Into<String>) -> ReconstructError {
    ReconstructError::Refused(why.into())
}

/// Why the settings are none, if they are not.
pub fn check_options(options: &ReconstructOptions) -> Result<(), String> {
    if !(MIN_DETAIL..=MAX_DETAIL).contains(&options.detail) {
        return Err(format!(
            "the detail must be from {MIN_DETAIL} to {MAX_DETAIL} cubes along the longest side, \
             got {}",
            options.detail
        ));
    }
    if !(3..=MAX_NEIGHBOURS).contains(&options.neighbours) {
        return Err(format!(
            "the normals' neighbors must be from 3 to {MAX_NEIGHBOURS}, got {}",
            options.neighbours
        ));
    }
    Ok(())
}

/// Each point's unit normal, oriented alike (steps 1 and 2 of the module).
pub fn oriented_normals(tree: &KdTree, neighbours: usize) -> Vec<Vec3> {
    let points = tree.points();
    let n = points.len();
    let mut near: Vec<Vec<u32>> = Vec::with_capacity(n);
    let mut normals: Vec<Vec3> = Vec::with_capacity(n);
    for (i, &p) in points.iter().enumerate() {
        let found = tree.nearest_k(p, neighbours + 1);
        let around: Vec<Vec3> = found.iter().map(|&(j, _)| points[j]).collect();
        let (_, scatter) = centroid_and_scatter(&around);
        let (_, vectors) = symmetric_eigen(scatter);
        normals.push([vectors[0][0], vectors[1][0], vectors[2][0]]);
        near.push(
            found
                .iter()
                .filter(|&&(j, _)| j != i)
                .map(|&(j, _)| j as u32)
                .collect(),
        );
    }
    // Both ways: a neighbour's neighbour list need not hold the point.
    let mut adjacency: Vec<Vec<u32>> = near.clone();
    for (i, list) in near.iter().enumerate() {
        for &j in list {
            adjacency[j as usize].push(i as u32);
        }
    }
    let mut seeds: Vec<usize> = (0..n).collect();
    seeds.sort_by(|&a, &b| points[b][2].total_cmp(&points[a][2]));
    let mut visited = vec![false; n];
    let mut heap: BinaryHeap<Edge> = BinaryHeap::new();
    for seed in seeds {
        if visited[seed] {
            continue;
        }
        // The highest point of a closed surface faces up.
        if normals[seed][2] < 0.0 {
            normals[seed] = normals[seed].map(|v| -v);
        }
        visited[seed] = true;
        push_edges(seed, &adjacency, &normals, &visited, &mut heap);
        while let Some(Edge { from, to, .. }) = heap.pop() {
            let (from, to) = (from as usize, to as usize);
            if visited[to] {
                continue;
            }
            if dot(normals[from], normals[to]) < 0.0 {
                normals[to] = normals[to].map(|v| -v);
            }
            visited[to] = true;
            push_edges(to, &adjacency, &normals, &visited, &mut heap);
        }
    }
    normals
}

/// An edge of the neighbours' graph by its weight, the smallest first.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Edge {
    weight: f64,
    from: u32,
    to: u32,
}

impl Eq for Edge {}

impl PartialOrd for Edge {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Edge {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed: the heap gives the most parallel normals first.
        other
            .weight
            .total_cmp(&self.weight)
            .then(other.to.cmp(&self.to))
            .then(other.from.cmp(&self.from))
    }
}

fn push_edges(
    from: usize,
    adjacency: &[Vec<u32>],
    normals: &[Vec3],
    visited: &[bool],
    heap: &mut BinaryHeap<Edge>,
) {
    for &to in &adjacency[from] {
        if !visited[to as usize] {
            heap.push(Edge {
                weight: 1.0 - dot(normals[from], normals[to as usize]).abs(),
                from: from as u32,
                to,
            });
        }
    }
}

/// The grid of the signed distance.
struct Grid {
    low: Vec3,
    cell: f64,
    dims: [usize; 3],
    values: Vec<f32>,
}

impl Grid {
    fn index(&self, i: usize, j: usize, k: usize) -> usize {
        i + self.dims[0] * (j + self.dims[1] * k)
    }

    fn position(&self, g: usize) -> Vec3 {
        let i = g % self.dims[0];
        let j = (g / self.dims[0]) % self.dims[1];
        let k = g / (self.dims[0] * self.dims[1]);
        [
            self.low[0] + i as f64 * self.cell,
            self.low[1] + j as f64 * self.cell,
            self.low[2] + k as f64 * self.cell,
        ]
    }
}

/// The six tetrahedra of a cube, by its corners (corner `c` at x `c & 1`,
/// y `c >> 1 & 1`, z `c >> 2 & 1`): along the diagonal from corner 0 to 7,
/// one per order of the axes.
const TETRAHEDRA: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 1, 5, 7],
    [0, 2, 3, 7],
    [0, 2, 6, 7],
    [0, 4, 5, 7],
    [0, 4, 6, 7],
];

/// Builds a closed mesh of the points (see the module). `progress(step,
/// steps)` is asked between the grid's layers and stops it when it answers
/// false.
pub fn reconstruct(
    points: &[Vec3],
    options: &ReconstructOptions,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Reconstruction, ReconstructError> {
    check_options(options).map_err(refused)?;
    if points.len() < MIN_POINTS {
        return Err(refused(format!(
            "a mesh needs at least {MIN_POINTS} points, the cloud has {}",
            points.len()
        )));
    }
    if points.len() <= options.neighbours {
        return Err(refused(format!(
            "estimating the normals of {} neighbors needs more points than that, the cloud has {}",
            options.neighbours,
            points.len()
        )));
    }
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in points {
        for k in 0..3 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    let extent = (0..3).map(|k| high[k] - low[k]).fold(0.0, f64::max);
    if !(extent > 0.0 && extent.is_finite()) {
        return Err(refused("the points all lie at one place"));
    }
    let cell = extent / options.detail as f64;
    let tree = KdTree::new(points.to_vec());
    let normals = oriented_normals(&tree, options.neighbours);
    // The points' usual spacing: how far the field's weights reach, and
    // what lies away from the points (step 5).
    let gap = spacing(&tree);
    let reach2 = (2.0 * gap).max(1e-6 * extent).powi(2);
    let dims: [usize; 3] =
        std::array::from_fn(|k| ((high[k] - low[k]) / cell).ceil() as usize + 2 * PAD + 1);
    let origin: Vec3 = std::array::from_fn(|k| low[k] - PAD as f64 * cell);
    let mut grid = Grid {
        low: origin,
        cell,
        dims,
        values: vec![0.0; dims[0] * dims[1] * dims[2]],
    };
    let steps = dims[2] + 2;
    for k in 0..dims[2] {
        if !progress(k, steps) {
            return Err(ReconstructError::Cancelled);
        }
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                let g = grid.index(i, j, k);
                let x = grid.position(g);
                // The tangent planes' distances of the nearest points,
                // weighted by their distance beyond the nearest one's.
                let near = tree.nearest_k(x, FIELD_NEIGHBOURS);
                let value = match near.first() {
                    Some(&(_, nearest)) => {
                        let (mut sum, mut weights) = (0.0, 0.0);
                        for &(p, d) in &near {
                            let w = (-(d * d - nearest * nearest) / reach2).exp();
                            sum += w * dot(sub(x, points[p]), normals[p]);
                            weights += w;
                        }
                        sum / weights
                    }
                    None => cell,
                };
                grid.values[g] = value as f32;
            }
        }
    }
    let boundary = |i: usize, j: usize, k: usize| {
        i == 0 || j == 0 || k == 0 || i + 1 == dims[0] || j + 1 == dims[1] || k + 1 == dims[2]
    };
    // Far outside is outside: normals all turned inwards are turned back.
    let (mut outer, mut negative) = (0usize, 0usize);
    for k in 0..dims[2] {
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                if boundary(i, j, k) {
                    outer += 1;
                    if grid.values[grid.index(i, j, k)] < 0.0 {
                        negative += 1;
                    }
                }
            }
        }
    }
    if 2 * negative > outer {
        for v in &mut grid.values {
            *v = -*v;
        }
    }
    let tiny = (1e-9 * cell) as f32;
    for k in 0..dims[2] {
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                let g = grid.index(i, j, k);
                let v = &mut grid.values[g];
                if boundary(i, j, k) {
                    *v = v.max(cell as f32);
                } else if v.abs() < tiny {
                    // No grid point on the surface: just outside it.
                    *v = tiny;
                }
            }
        }
    }
    if !progress(dims[2], steps) {
        return Err(ReconstructError::Cancelled);
    }
    let (vertices, triangles) = march(&grid);
    if !progress(dims[2] + 1, steps) {
        return Err(ReconstructError::Cancelled);
    }
    // Away from the points: farther than two cubes, or three times the
    // points' usual spacing when they lie farther apart than that.
    let far = (2.0 * cell).max(3.0 * gap);
    let (mesh, shells, dropped, small) =
        keep_shells(vertices, &triangles, &tree, far, SMALL_SHELL * cell);
    if mesh.triangles.is_empty() {
        return Err(refused(if dropped > 0 {
            "the points enclose no volume at this detail (a flat scan, or one open on too many \
             sides): the surface would be guessed where there are no points"
                .to_owned()
        } else {
            "the points make no closed surface at this detail: try a finer detail".to_owned()
        }));
    }
    let mut mesh = mesh;
    let mut volume = signed_volume(&mesh);
    if volume < 0.0 {
        for t in &mut mesh.triangles {
            t.swap(1, 2);
        }
        volume = -volume;
    }
    let closed = is_closed(&mesh.triangles);
    let target = TrianglesTarget::new(mesh.vertices.clone(), mesh.triangles.clone());
    let sample = thinned(points.len(), DEVIATION_SAMPLES);
    let (mut sum, mut max) = (0.0, 0.0f64);
    for &i in &sample {
        if let Some((_, d2)) = target.closest(points[i]) {
            let d = d2.sqrt();
            sum += d;
            max = max.max(d);
        }
    }
    Ok(Reconstruction {
        mesh,
        cell,
        shells,
        dropped_shells: dropped,
        small_shells: small,
        closed,
        volume,
        mean_deviation: sum / sample.len().max(1) as f64,
        max_deviation: max,
    })
}

/// The points' usual spacing: the median distance from a point to its
/// nearest other point, of at most 2000 of them.
fn spacing(tree: &KdTree) -> f64 {
    let mut gaps: Vec<f64> = thinned(tree.len(), 2000)
        .into_iter()
        .filter_map(|i| tree.nearest_k(tree.point(i), 2).get(1).map(|&(_, d)| d))
        .collect();
    if gaps.is_empty() {
        return 0.0;
    }
    let mid = gaps.len() / 2;
    *gaps.select_nth_unstable_by(mid, f64::total_cmp).1
}

/// The zero level of the grid's values as triangles facing the positive
/// side (step 4 of the module): vertices on the grid's edges, shared.
fn march(grid: &Grid) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut vertices: Vec<Vec3> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut on_edge: HashMap<(u32, u32), u32> = HashMap::new();
    let [nx, ny, nz] = grid.dims;
    let mut vertex = |a: usize, b: usize, vertices: &mut Vec<Vec3>| -> u32 {
        let key = if a < b {
            (a as u32, b as u32)
        } else {
            (b as u32, a as u32)
        };
        *on_edge.entry(key).or_insert_with(|| {
            let (fa, fb) = (f64::from(grid.values[a]), f64::from(grid.values[b]));
            let t = fa / (fa - fb);
            let (pa, pb) = (grid.position(a), grid.position(b));
            vertices.push(std::array::from_fn(|k| pa[k] + t * (pb[k] - pa[k])));
            (vertices.len() - 1) as u32
        })
    };
    for k in 0..nz - 1 {
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let corners: [usize; 8] = std::array::from_fn(|c| {
                    grid.index(i + (c & 1), j + ((c >> 1) & 1), k + ((c >> 2) & 1))
                });
                let inside = corners.iter().filter(|&&g| grid.values[g] < 0.0).count();
                if inside == 0 || inside == 8 {
                    continue;
                }
                for tet in TETRAHEDRA {
                    let g: [usize; 4] = tet.map(|c| corners[c]);
                    let (ins, outs): (Vec<usize>, Vec<usize>) =
                        g.iter().copied().partition(|&v| grid.values[v] < 0.0);
                    if ins.is_empty() || outs.is_empty() {
                        continue;
                    }
                    // The triangles by their crossings, each an edge from an
                    // inside corner to an outside one.
                    let faces: Vec<[(usize, usize); 3]> = match (ins.len(), outs.len()) {
                        (1, 3) => vec![[(ins[0], outs[0]), (ins[0], outs[1]), (ins[0], outs[2])]],
                        (3, 1) => vec![[(ins[0], outs[0]), (ins[1], outs[0]), (ins[2], outs[0])]],
                        _ => {
                            // Two inside (a, b), two outside (c, d): the
                            // quad ac, ad, bd, bc.
                            let (a, b, c, d) = (ins[0], ins[1], outs[0], outs[1]);
                            vec![[(a, c), (a, d), (b, d)], [(a, c), (b, d), (b, c)]]
                        }
                    };
                    // From the inside corners to the outside ones.
                    let middle = |vs: &[usize]| -> Vec3 {
                        let mut m = [0.0; 3];
                        for &v in vs {
                            let p = grid.position(v);
                            for c in 0..3 {
                                m[c] += p[c] / vs.len() as f64;
                            }
                        }
                        m
                    };
                    let outward = sub(middle(&outs), middle(&ins));
                    for face in faces {
                        // Turned by the triangle of the edges' midpoints:
                        // exact grid places, never without area, turned as
                        // the triangle of the crossings is (the crossings
                        // lie inside the same edges).
                        let mid = face.map(|(a, b)| {
                            let (pa, pb) = (grid.position(a), grid.position(b));
                            std::array::from_fn::<f64, 3, _>(|k| 0.5 * (pa[k] + pb[k]))
                        });
                        let n = cross(sub(mid[1], mid[0]), sub(mid[2], mid[0]));
                        let [a, b, c] = face.map(|(i, o)| vertex(i, o, &mut vertices));
                        triangles.push(if dot(n, outward) < 0.0 {
                            [a, c, b]
                        } else {
                            [a, b, c]
                        });
                    }
                }
            }
        }
    }
    (vertices, triangles)
}

/// The mesh of the shells (sets of triangles joined by their corners) that
/// lie along the points, its vertices renumbered; the shells kept and those
/// left out (step 5 of the module).
fn keep_shells(
    vertices: Vec<Vec3>,
    triangles: &[[u32; 3]],
    tree: &KdTree,
    far: f64,
    small: f64,
) -> (TriangleMesh, usize, usize, usize) {
    let n = vertices.len();
    let mut parent: Vec<u32> = (0..n).map(|i| i as u32).collect();
    fn root(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            let up = parent[parent[i as usize] as usize];
            parent[i as usize] = up;
            i = up;
        }
        i
    }
    for t in triangles {
        let a = root(&mut parent, t[0]);
        for &v in &t[1..] {
            let b = root(&mut parent, v);
            if a != b {
                parent[b as usize] = a;
            }
        }
    }
    // Each shell's corners, how many lie far from the points, and its box.
    struct Shell {
        corners: usize,
        away: usize,
        low: Vec3,
        high: Vec3,
    }
    let mut shells: HashMap<u32, Shell> = HashMap::new();
    for (v, &corner) in vertices.iter().enumerate() {
        let r = root(&mut parent, v as u32);
        let away = tree
            .nearest(corner)
            .is_none_or(|(_, distance)| distance > far);
        let shell = shells.entry(r).or_insert(Shell {
            corners: 0,
            away: 0,
            low: corner,
            high: corner,
        });
        shell.corners += 1;
        shell.away += usize::from(away);
        for (k, &c) in corner.iter().enumerate() {
            shell.low[k] = shell.low[k].min(c);
            shell.high[k] = shell.high[k].max(c);
        }
    }
    let used: std::collections::HashSet<u32> =
        triangles.iter().map(|t| root(&mut parent, t[0])).collect();
    let (mut kept_shells, mut dropped, mut bubbles) = (0, 0, 0);
    let mut keep: HashMap<u32, bool> = HashMap::new();
    for r in used {
        let Some(shell) = shells.get(&r) else {
            continue;
        };
        let along = 5 * shell.away <= shell.corners;
        let size = norm(sub(shell.high, shell.low));
        let kept = along && size >= small;
        keep.insert(r, kept);
        if kept {
            kept_shells += 1;
        } else if along {
            bubbles += 1;
        } else {
            dropped += 1;
        }
    }
    let mut renumber: Vec<u32> = vec![u32::MAX; n];
    let mut mesh = TriangleMesh::default();
    for t in triangles {
        let r = root(&mut parent, t[0]);
        if !keep.get(&r).copied().unwrap_or(false) {
            continue;
        }
        let mut out = [0u32; 3];
        for (slot, &v) in out.iter_mut().zip(t) {
            if renumber[v as usize] == u32::MAX {
                renumber[v as usize] = mesh.vertices.len() as u32;
                mesh.vertices.push(vertices[v as usize]);
            }
            *slot = renumber[v as usize];
        }
        mesh.triangles.push(out);
    }
    (mesh, kept_shells, dropped, bubbles)
}

/// The volume the triangles enclose, positive when they face outwards.
pub fn signed_volume(mesh: &TriangleMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
}

/// Whether every edge is shared by exactly two triangles, once each way.
pub fn is_closed(triangles: &[[u32; 3]]) -> bool {
    let mut edges: HashMap<(u32, u32), i32> = HashMap::with_capacity(triangles.len() * 3);
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (key, way) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            // 1001 one way, -999 the other: once each way sums to 2.
            *edges.entry(key).or_insert(0) += way * 1000 + 1;
        }
    }
    !triangles.is_empty() && edges.values().all(|&v| v == 2)
}

/// The largest distance of the mesh's vertices from a shape's surface,
/// for checks: `distance(p)` is a vertex's distance from it.
pub fn largest_vertex_distance(mesh: &TriangleMesh, distance: impl Fn(Vec3) -> f64) -> f64 {
    mesh.vertices
        .iter()
        .map(|&p| distance(p))
        .fold(0.0, f64::max)
}

/// The length of the longest edge of the mesh.
pub fn longest_edge(mesh: &TriangleMesh) -> f64 {
    mesh.triangles
        .iter()
        .flat_map(|t| (0..3).map(move |k| (t[k], t[(k + 1) % 3])))
        .map(|(a, b)| norm(sub(mesh.vertices[a as usize], mesh.vertices[b as usize])))
        .fold(0.0, f64::max)
}
