// SPDX-License-Identifier: MIT
//! Repair of a mesh (mitcad#1871), in this order:
//!
//! 1. Vertices within the tolerance of a vertex kept before them are that
//!    vertex (the nearest; the first stays where it is).
//! 2. Triangles that welding left with two corners the same are taken out
//!    (degenerate), and of triangles on the same three vertices all but the
//!    first (duplicates).
//! 3. Manifold: at an edge of more than two triangles the two largest stay
//!    and the others are taken out; a vertex where separate fans of
//!    triangles touch (two cones tip to tip) becomes one vertex per fan.
//! 4. Orientation: the triangles are turned to agree with their neighbours
//!    across every edge, a closed shell outwards (a positive volume), an
//!    open surface the way most of its area already faced. A shell that
//!    cannot be oriented (a one-sided surface) is left as it was.
//! 5. Holes (loops of edges of one triangle) up to `max_hole` across (the
//!    diagonal of the loop's box) are filled with triangles: ears clipped
//!    in the loop's plane, or a fan to its centre where no ear is free.
//!
//! What it did is counted in a [`RepairReport`], which says it in words.

use std::collections::{HashMap, HashSet};

use mitcad_vec3::{add, cross, dot, scale, sub};

use super::{Point, TriangleMesh, area, corners, edge_key, edge_triangles, size, submesh};

/// The longest loop a hole fill takes (edges).
pub const MAX_HOLE_EDGES: usize = 4096;

/// What [`repair`] does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepairOptions {
    /// Vertices closer than this are welded (mm).
    pub tolerance: f64,
    /// Holes up to this across are filled (mm); None fills none.
    pub max_hole: Option<f64>,
}

/// What [`repair`] did, counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub welded: usize,
    pub degenerate: usize,
    pub duplicate: usize,
    /// Triangles taken out at edges of more than two triangles.
    pub non_manifold: usize,
    /// Vertices made one per fan where separate fans touched.
    pub split: usize,
    /// Triangles turned over.
    pub flipped: usize,
    /// Shells that could not be oriented (one-sided surfaces).
    pub unorientable: usize,
    pub holes_filled: usize,
    /// Holes larger than the size, or too long a loop, left open.
    pub holes_left: usize,
}

impl RepairReport {
    /// What was repaired, in words for the person who asked.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let count = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        if self.welded > 0 {
            parts.push(format!(
                "welded {}",
                count(self.welded, "vertex", "vertices")
            ));
        }
        if self.degenerate > 0 {
            parts.push(format!(
                "removed {} without area",
                count(self.degenerate, "triangle", "triangles")
            ));
        }
        if self.duplicate > 0 {
            parts.push(format!(
                "removed {} lying on another",
                count(self.duplicate, "triangle", "triangles")
            ));
        }
        if self.non_manifold > 0 {
            parts.push(format!(
                "removed {} at edges shared by more than two",
                count(self.non_manifold, "triangle", "triangles")
            ));
        }
        if self.split > 0 {
            parts.push(format!(
                "separated {} where surfaces touched",
                count(self.split, "vertex", "vertices")
            ));
        }
        if self.flipped > 0 {
            parts.push(format!(
                "turned {} to face the same way as their neighbors",
                count(self.flipped, "triangle", "triangles")
            ));
        }
        if self.holes_filled > 0 {
            parts.push(format!(
                "filled {}",
                count(self.holes_filled, "hole", "holes")
            ));
        }
        let mut text = if parts.is_empty() {
            "the mesh needed no repair".to_owned()
        } else {
            let mut text = parts.join(", ");
            if let Some(first) = text.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            text
        };
        if self.holes_left > 0 {
            text.push_str(&format!(
                "; {} larger than the size to fill left open",
                count(self.holes_left, "hole", "holes")
            ));
        }
        if self.unorientable > 0 {
            text.push_str(&format!(
                "; {} could not be oriented (a surface with one side only)",
                count(self.unorientable, "piece", "pieces")
            ));
        }
        text
    }
}

/// Each point's welded point and the welded points: a point within
/// `tolerance` of one kept before it (the nearest) is that one.
pub(crate) fn weld(points: &[Point], tolerance: f64) -> (Vec<Point>, Vec<u32>) {
    if tolerance <= 0.0 {
        return (points.to_vec(), (0..points.len() as u32).collect());
    }
    let cell =
        |p: Point| -> [i64; 3] { std::array::from_fn(|i| (p[i] / tolerance).floor() as i64) };
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut kept: Vec<Point> = Vec::new();
    let mut map = Vec::with_capacity(points.len());
    let limit = tolerance * tolerance;
    for p in points {
        let c = cell(*p);
        let mut best: Option<(f64, u32)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(near) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else {
                        continue;
                    };
                    for &k in near {
                        let d = sub(*p, kept[k as usize]);
                        let d2 = dot(d, d);
                        if d2 <= limit && best.is_none_or(|(b, _)| d2 < b) {
                            best = Some((d2, k));
                        }
                    }
                }
            }
        }
        let index = match best {
            Some((_, k)) => k,
            None => {
                let k = kept.len() as u32;
                kept.push(*p);
                grid.entry(c).or_default().push(k);
                k
            }
        };
        map.push(index);
    }
    (kept, map)
}

/// The loops of open edges (edges of one triangle), each in the direction
/// a fill runs: against the triangle's edge, so that triangles made in the
/// loop's order face as their neighbours do. Sorted by their first vertex.
pub(crate) fn boundary_loops(triangles: &[[u32; 3]]) -> Vec<Vec<u32>> {
    let edges = edge_triangles(triangles);
    // Fill edges b -> a of each open edge a -> b of its triangle.
    let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut open: Vec<(u32, u32)> = Vec::new();
    for tri in triangles {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            if edges.get(&edge_key(a, b)).is_some_and(|t| t.len() == 1) {
                next.entry(b).or_default().push(a);
                open.push((b, a));
            }
        }
    }
    open.sort_unstable();
    for targets in next.values_mut() {
        targets.sort_unstable();
    }
    let mut used: HashSet<(u32, u32)> = HashSet::new();
    let mut loops = Vec::new();
    for &(start, second) in &open {
        if used.contains(&(start, second)) {
            continue;
        }
        let mut lp = vec![start];
        used.insert((start, second));
        let mut at = second;
        let mut closed = false;
        for _ in 0..open.len() {
            if at == start {
                closed = true;
                break;
            }
            lp.push(at);
            let Some(to) = next
                .get(&at)
                .and_then(|t| t.iter().copied().find(|&to| !used.contains(&(at, to))))
            else {
                break;
            };
            used.insert((at, to));
            at = to;
        }
        if closed && lp.len() >= 3 {
            loops.push(lp);
        }
    }
    loops
}

/// Triangles that close the loop (vertex indices into `points`, in the
/// loop's fill direction); `points` gains a centre vertex when no ear is
/// free. None for a loop too long to fill.
pub(crate) fn fill_loop(points: &mut Vec<Point>, lp: &[u32]) -> Option<Vec<[u32; 3]>> {
    let n = lp.len();
    if !(3..=MAX_HOLE_EDGES).contains(&n) {
        return None;
    }
    if n == 3 {
        return Some(vec![[lp[0], lp[1], lp[2]]]);
    }
    // The loop's plane by Newell's method; its normal follows the loop.
    let mut normal = [0.0; 3];
    for i in 0..n {
        let a = points[lp[i] as usize];
        let b = points[lp[(i + 1) % n] as usize];
        normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
        normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
        normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let normal = mitcad_vec3::unit(normal).unwrap_or([0.0, 0.0, 1.0]);
    let u = super::perpendicular(normal);
    let v = cross(normal, u);
    let flat: Vec<[f64; 2]> = lp
        .iter()
        .map(|&k| {
            let p = points[k as usize];
            [dot(p, u), dot(p, v)]
        })
        .collect();
    let mut remaining: Vec<usize> = (0..n).collect();
    let mut triangles = Vec::with_capacity(n - 2);
    let cross2 = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    while remaining.len() > 3 {
        let m = remaining.len();
        let mut best: Option<(f64, usize)> = None;
        for i in 0..m {
            let (a, b, c) = (
                remaining[(i + m - 1) % m],
                remaining[i],
                remaining[(i + 1) % m],
            );
            let (pa, pb, pc) = (flat[a], flat[b], flat[c]);
            let turn = cross2(pa, pb, pc);
            if turn <= 1e-15 {
                continue;
            }
            let inside = remaining.iter().any(|&k| {
                k != a
                    && k != b
                    && k != c
                    && cross2(pa, pb, flat[k]) >= 0.0
                    && cross2(pb, pc, flat[k]) >= 0.0
                    && cross2(pc, pa, flat[k]) >= 0.0
            });
            if inside {
                continue;
            }
            // The sharpest ear first: its angle's cosine, largest.
            let e1 = [pa[0] - pb[0], pa[1] - pb[1]];
            let e2 = [pc[0] - pb[0], pc[1] - pb[1]];
            let lengths = (e1[0].hypot(e1[1]) * e2[0].hypot(e2[1])).max(1e-300);
            let cosine = (e1[0] * e2[0] + e1[1] * e2[1]) / lengths;
            if best.is_none_or(|(c, _)| cosine > c) {
                best = Some((cosine, i));
            }
        }
        let Some((_, i)) = best else {
            // No free ear (the loop folds in its plane): a fan to its centre.
            let rest: Vec<u32> = remaining.iter().map(|&k| lp[k]).collect();
            triangles.extend(fan(points, &rest));
            return Some(triangles);
        };
        let (a, b, c) = (
            remaining[(i + m - 1) % m],
            remaining[i],
            remaining[(i + 1) % m],
        );
        triangles.push([lp[a], lp[b], lp[c]]);
        remaining.remove(i);
    }
    triangles.push([lp[remaining[0]], lp[remaining[1]], lp[remaining[2]]]);
    Some(triangles)
}

/// A fan of triangles from a new vertex at the loop's centre.
fn fan(points: &mut Vec<Point>, lp: &[u32]) -> Vec<[u32; 3]> {
    let mut centre = [0.0; 3];
    for &k in lp {
        centre = add(centre, points[k as usize]);
    }
    let centre = scale(centre, 1.0 / lp.len() as f64);
    let c = points.len() as u32;
    points.push(centre);
    (0..lp.len())
        .map(|i| [lp[i], lp[(i + 1) % lp.len()], c])
        .collect()
}

/// The diagonal of the box of the loop's points: how large a hole is.
pub(crate) fn loop_size(points: &[Point], lp: &[u32]) -> f64 {
    let pts: Vec<Point> = lp.iter().map(|&k| points[k as usize]).collect();
    size(&pts)
}

/// Which holes a fill takes besides their size (a loop's vertices).
pub(crate) type HoleFilter<'a> = &'a dyn Fn(&[u32]) -> bool;

/// Fills the holes of the mesh up to `max_hole` across (those `only` takes,
/// all when None); the number filled and left.
pub(crate) fn fill_holes(
    mesh: &mut TriangleMesh,
    max_hole: f64,
    only: Option<HoleFilter<'_>>,
) -> (usize, usize) {
    let loops = boundary_loops(&mesh.triangles);
    let (mut filled, mut left) = (0, 0);
    for lp in loops {
        if only.is_some_and(|only| !only(&lp)) {
            continue;
        }
        if loop_size(&mesh.vertices, &lp) > max_hole {
            left += 1;
            continue;
        }
        match fill_loop(&mut mesh.vertices, &lp) {
            Some(mut triangles) => {
                mesh.triangles.append(&mut triangles);
                filled += 1;
            }
            None => left += 1,
        }
    }
    (filled, left)
}

/// The mesh repaired (see the module docs), with what was done.
pub fn repair(
    mesh: &TriangleMesh,
    options: &RepairOptions,
) -> Result<(TriangleMesh, RepairReport), String> {
    if mesh.triangles.is_empty() {
        return Err("the mesh has no triangles".to_owned());
    }
    let mut report = RepairReport::default();
    // 1. Weld.
    let (points, map) = weld(&mesh.vertices, options.tolerance);
    let used: HashSet<u32> = mesh.triangles.iter().flatten().copied().collect();
    let welded_used: HashSet<u32> = used.iter().map(|&v| map[v as usize]).collect();
    report.welded = used.len() - welded_used.len();
    // 2. Degenerate and duplicate triangles; `source` keeps each kept
    // triangle's input number for the orientation count.
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(mesh.triangles.len());
    let mut source: Vec<usize> = Vec::with_capacity(mesh.triangles.len());
    let mut seen: HashSet<[u32; 3]> = HashSet::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let w = tri.map(|v| map[v as usize]);
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            report.degenerate += 1;
            continue;
        }
        let mut key = w;
        key.sort_unstable();
        if !seen.insert(key) {
            report.duplicate += 1;
            continue;
        }
        triangles.push(w);
        source.push(t);
    }
    let mut out = TriangleMesh {
        vertices: points,
        triangles,
    };
    // 3. Edges of more than two triangles: the two largest stay.
    let areas: Vec<f64> = (0..out.triangles.len())
        .map(|t| area(&corners(&out, t)))
        .collect();
    let mut removed = vec![false; out.triangles.len()];
    let mut crowded: Vec<Vec<usize>> = edge_triangles(&out.triangles)
        .into_values()
        .filter(|t| t.len() > 2)
        .collect();
    crowded.sort_unstable();
    for mut on in crowded {
        on.retain(|&t| !removed[t]);
        on.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]).then(a.cmp(&b)));
        for &t in on.iter().skip(2) {
            removed[t] = true;
            report.non_manifold += 1;
        }
    }
    if report.non_manifold > 0 {
        let keep: Vec<usize> = (0..out.triangles.len()).filter(|&t| !removed[t]).collect();
        out.triangles = keep.iter().map(|&t| out.triangles[t]).collect();
        source = keep.iter().map(|&t| source[t]).collect();
    }
    report.split = split_fans(&mut out);
    // 4. Orientation.
    let (flips, unorientable) = orient(&out);
    report.unorientable = unorientable;
    for (t, flip) in flips.iter().enumerate() {
        if *flip {
            out.triangles[t].swap(1, 2);
        }
    }
    // Turned from the input: the welded input triangle runs the other way.
    report.flipped = source
        .iter()
        .zip(&out.triangles)
        .filter(|(s, tri)| {
            let input = mesh.triangles[**s].map(|v| map[v as usize]);
            !same_turn(&input, tri)
        })
        .count();
    // 5. Holes.
    if let Some(max_hole) = options.max_hole {
        let (filled, left) = fill_holes(&mut out, max_hole, None);
        report.holes_filled = filled;
        report.holes_left = left;
    }
    let out = submesh(&out, |_| true);
    if out.triangles.is_empty() {
        return Err("no triangles are left after the repair".to_owned());
    }
    Ok((out, report))
}

/// Whether two triangles on the same vertices run the same way round.
fn same_turn(a: &[u32; 3], b: &[u32; 3]) -> bool {
    (0..3).any(|k| b[k] == a[0] && b[(k + 1) % 3] == a[1] && b[(k + 2) % 3] == a[2])
}

/// A vertex where separate fans of triangles touch becomes one vertex per
/// fan (the first fan keeps it); the number of vertices added.
fn split_fans(mesh: &mut TriangleMesh) -> usize {
    let edges = edge_triangles(&mesh.triangles);
    let mut around: Vec<Vec<usize>> = vec![Vec::new(); mesh.vertices.len()];
    for (t, tri) in mesh.triangles.iter().enumerate() {
        for &v in tri {
            around[v as usize].push(t);
        }
    }
    // The fans are found on the triangles as they are, then renamed.
    let mut renames: Vec<(usize, u32, u32)> = Vec::new();
    let mut points = mesh.vertices.clone();
    for (v, fan) in around.iter().enumerate() {
        if fan.len() < 2 {
            continue;
        }
        // Union of the triangles at v across the edges at v they share.
        let mut parent: Vec<usize> = (0..fan.len()).collect();
        for (i, &t) in fan.iter().enumerate() {
            for &w in &mesh.triangles[t] {
                if w as usize == v {
                    continue;
                }
                let Some(on) = edges.get(&edge_key(v as u32, w)) else {
                    continue;
                };
                if on.len() != 2 {
                    continue;
                }
                for &other in on {
                    if let Some(j) = fan.iter().position(|&f| f == other) {
                        let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                        if a != b {
                            parent[a.max(b)] = a.min(b);
                        }
                    }
                }
            }
        }
        let groups: Vec<usize> = (0..fan.len()).map(|i| root(&mut parent, i)).collect();
        let first = groups[0];
        let mut copies: HashMap<usize, u32> = HashMap::new();
        for (i, &t) in fan.iter().enumerate() {
            let g = groups[i];
            if g == first {
                continue;
            }
            let copy = *copies.entry(g).or_insert_with(|| {
                points.push(points[v]);
                (points.len() - 1) as u32
            });
            renames.push((t, v as u32, copy));
        }
    }
    let added = points.len() - mesh.vertices.len();
    mesh.vertices = points;
    for (t, v, copy) in renames {
        for corner in &mut mesh.triangles[t] {
            if *corner == v {
                *corner = copy;
            }
        }
    }
    added
}

/// The root of `i` in a union-find forest, its path halved.
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Which triangles to turn over so that each connected piece agrees with
/// itself across its edges of two triangles, a closed piece outwards and
/// an open one the way most of its area faces; and how many pieces could
/// not be made to agree (they are left as they were).
pub(crate) fn orient(mesh: &TriangleMesh) -> (Vec<bool>, usize) {
    let n = mesh.triangles.len();
    let edges = edge_triangles(&mesh.triangles);
    let areas: Vec<f64> = (0..n).map(|t| area(&corners(mesh, t))).collect();
    let mut flip = vec![false; n];
    let mut seen = vec![false; n];
    let mut unorientable = 0;
    // Seeds: the largest triangles first.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]).then(a.cmp(&b)));
    for seed in order {
        if seen[seed] {
            continue;
        }
        let mut piece = vec![seed];
        let mut queue = std::collections::VecDeque::from([seed]);
        seen[seed] = true;
        let mut consistent = true;
        let mut closed = true;
        while let Some(t) = queue.pop_front() {
            let tri = oriented(&mesh.triangles[t], flip[t]);
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let on = &edges[&edge_key(a, b)];
                if on.len() != 2 {
                    closed = false;
                    continue;
                }
                let other = if on[0] == t { on[1] } else { on[0] };
                // Agreeing neighbours run the edge b -> a.
                let runs_back = |f: bool| {
                    let o = oriented(&mesh.triangles[other], f);
                    (0..3).any(|j| o[j] == b && o[(j + 1) % 3] == a)
                };
                if seen[other] {
                    if !runs_back(flip[other]) {
                        consistent = false;
                    }
                } else {
                    flip[other] = !runs_back(false);
                    seen[other] = true;
                    piece.push(other);
                    queue.push_back(other);
                }
            }
        }
        if !consistent {
            unorientable += 1;
            for &t in &piece {
                flip[t] = false;
            }
            continue;
        }
        let turn_all = if closed {
            let volume: f64 = piece
                .iter()
                .map(|&t| {
                    let tri = oriented(&mesh.triangles[t], flip[t]);
                    let p = tri.map(|v| mesh.vertices[v as usize]);
                    dot(p[0], cross(p[1], p[2]))
                })
                .sum();
            volume < 0.0
        } else {
            let turned: f64 = piece.iter().filter(|&&t| flip[t]).map(|&t| areas[t]).sum();
            let total: f64 = piece.iter().map(|&t| areas[t]).sum();
            turned > 0.5 * total
        };
        if turn_all {
            for &t in &piece {
                flip[t] = !flip[t];
            }
        }
    }
    (flip, unorientable)
}

fn oriented(tri: &[u32; 3], flip: bool) -> [u32; 3] {
    if flip { [tri[0], tri[2], tri[1]] } else { *tri }
}
