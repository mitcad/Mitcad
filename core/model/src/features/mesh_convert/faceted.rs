// SPDX-License-Identifier: MIT
//! Convert Mesh's Faceted solid (mitcad#1082): a mesh body's triangles made
//! a solid of planar faces, for any closed mesh (a printed part's STL file),
//! not only a straight-extruded profile.
//!
//! 1. Vertices within the tolerance of each other are welded into one (the
//!    first of them stays where it is).
//! 2. Triangles that welding made degenerate (two corners the same) are
//!    left out; every other edge must be on exactly two triangles: edges of
//!    one triangle (open) and of more (non-manifold) are counted and refused.
//! 3. The triangles are turned to agree with their neighbours across every
//!    edge, each connected shell outwards (a positive volume); a mesh that
//!    cannot be oriented, a shell that encloses nothing and a shell inside
//!    another (a hollow part) are refused.
//! 4. Neighbouring triangles merge into one planar face while their normals
//!    lie within the angle of the first one's (the largest triangle first)
//!    and their corners within the tolerance of its plane.
//! 5. Each face's boundary loops follow its triangles' edges (the outer
//!    loop first, counter-clockwise seen from outside); a run of edges
//!    between the same two faces whose inner corners lie within the
//!    tolerance of a straight line between its ends is one edge.
//!
//! The result is a neutral boundary ([`BoundaryBody`]) of planes and lines
//! that the kernel builds as a solid of each shell.

use std::collections::{HashMap, HashSet};

use crate::boundary::{BoundaryBody, BoundaryCurve, BoundaryEdge, BoundaryFace, BoundarySurface};
use crate::exchange::TriangleMesh;

/// The most triangles Faceted solid takes: a printed part's mesh of a few
/// hundred thousand triangles converts, a larger one is refused with its
/// count.
pub const MAX_TRIANGLES: usize = 500_000;

/// The angle within which neighbouring triangles merge must be more than
/// zero and at most this (radians, 30 degrees).
pub const MAX_ANGLE: f64 = std::f64::consts::PI / 6.0;

/// The longest run of mesh edges one straight edge takes.
const MAX_RUN: usize = 256;

/// A mesh made a boundary of planar faces.
#[derive(Debug, Clone, PartialEq)]
pub struct Faceted {
    pub boundary: BoundaryBody,
    /// The enclosed volume of the welded, oriented triangles (mm³).
    pub volume: f64,
    /// Their area (mm²).
    pub area: f64,
    /// Connected closed shells: each becomes a solid.
    pub shells: usize,
    /// Vertices welded away (the mesh's vertices less those left).
    pub welded: usize,
    /// Triangles left out as degenerate after welding.
    pub degenerate: usize,
}

type Point = [f64; 3];

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: Point) -> f64 {
    dot(a, a).sqrt()
}

fn scaled(a: Point, s: f64) -> Point {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn plus(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// An undirected edge's key (the smaller vertex first).
fn edge_key(a: u32, b: u32) -> u64 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    (u64::from(lo) << 32) | u64::from(hi)
}

/// A directed edge's key.
fn half_key(a: u32, b: u32) -> u64 {
    (u64::from(a) << 32) | u64::from(b)
}

/// Each vertex's welded vertex and the welded vertices' points: a vertex
/// within `tolerance` of one kept before it (the nearest) is that one.
fn weld(points: &[Point], tolerance: f64) -> (Vec<Point>, Vec<u32>) {
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

/// The triangles on each undirected edge (one, two, or more).
fn edge_uses(triangles: &[[u32; 3]]) -> HashMap<u64, Vec<u32>> {
    let mut uses: HashMap<u64, Vec<u32>> = HashMap::with_capacity(triangles.len() * 3 / 2);
    for (t, tri) in triangles.iter().enumerate() {
        for k in 0..3 {
            uses.entry(edge_key(tri[k], tri[(k + 1) % 3]))
                .or_default()
                .push(t as u32);
        }
    }
    uses
}

/// Whether triangle `tri` runs along `a` -> `b`.
fn runs_along(tri: &[u32; 3], a: u32, b: u32) -> bool {
    (0..3).any(|k| tri[k] == a && tri[(k + 1) % 3] == b)
}

/// The signed volume the triangles enclose.
fn signed_volume(points: &[Point], triangles: &[[u32; 3]], of: &[u32]) -> f64 {
    of.iter()
        .map(|&t| {
            let [a, b, c] = triangles[t as usize].map(|v| points[v as usize]);
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
}

/// Whether the ray from `origin` along `dir` crosses the triangle.
fn ray_crosses(origin: Point, dir: Point, [a, b, c]: [Point; 3]) -> bool {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-300 {
        return false;
    }
    let s = sub(origin, a);
    let u = dot(s, p) / det;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(s, e1);
    let v = dot(dir, q) / det;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    dot(e2, q) / det > 0.0
}

/// The box of a shell's vertices.
fn shell_box(points: &[Point], triangles: &[[u32; 3]], of: &[u32]) -> [Point; 2] {
    let mut b = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for &t in of {
        for v in triangles[t as usize] {
            for (i, c) in points[v as usize].iter().enumerate() {
                b[0][i] = b[0][i].min(*c);
                b[1][i] = b[1][i].max(*c);
            }
        }
    }
    b
}

/// The mesh as a boundary of planar faces (see the module docs), welded
/// within `tolerance` (mm) and merged within `angle` (radians).
pub fn faceted_solid(mesh: &TriangleMesh, tolerance: f64, angle: f64) -> Result<Faceted, String> {
    let count = mesh.triangles.len();
    if count > MAX_TRIANGLES {
        return Err(format!(
            "the mesh has {count} triangles; Faceted solid takes at most {MAX_TRIANGLES}"
        ));
    }
    if count < 4 {
        return Err(format!(
            "the mesh has {count} triangles; a closed mesh has at least 4"
        ));
    }
    if mesh
        .triangles
        .iter()
        .flatten()
        .any(|&v| v as usize >= mesh.vertices.len())
    {
        return Err("a triangle refers to a vertex the mesh does not have".to_owned());
    }
    if mesh.vertices.iter().flatten().any(|c| !c.is_finite()) {
        return Err("a vertex of the mesh is not a finite point".to_owned());
    }

    // 1. Weld.
    let (points, map) = weld(&mesh.vertices, tolerance);
    let welded = mesh.vertices.len() - points.len();

    // 2. Degenerate triangles out; every edge on two triangles.
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(count);
    for tri in &mesh.triangles {
        let t = tri.map(|v| map[v as usize]);
        if t[0] != t[1] && t[1] != t[2] && t[2] != t[0] {
            triangles.push(t);
        }
    }
    let degenerate = count - triangles.len();
    let uses = edge_uses(&triangles);
    let open = uses.values().filter(|u| u.len() == 1).count();
    let non_manifold = uses.values().filter(|u| u.len() > 2).count();
    if open > 0 || non_manifold > 0 {
        let mut what = Vec::new();
        if open > 0 {
            what.push(format!("{open} open edges (edges of one triangle only)"));
        }
        if non_manifold > 0 {
            what.push(format!(
                "{non_manifold} non-manifold edges (edges of more than two triangles)"
            ));
        }
        return Err(format!(
            "the mesh has {} after welding within {tolerance} mm; Faceted solid needs a closed \
             mesh in which every edge is on two triangles",
            what.join(" and ")
        ));
    }
    let n = triangles.len();

    // 3. Orientation: each triangle's neighbour across each edge.
    let neighbour = |t: usize, k: usize| -> usize {
        let tri = triangles[t];
        let on = &uses[&edge_key(tri[k], tri[(k + 1) % 3])];
        if on[0] as usize == t {
            on[1] as usize
        } else {
            on[0] as usize
        }
    };
    let mut flipped = vec![false; n];
    let mut shell_of = vec![u32::MAX; n];
    let mut shells: Vec<Vec<u32>> = Vec::new();
    for seed in 0..n {
        if shell_of[seed] != u32::MAX {
            continue;
        }
        let id = shells.len() as u32;
        let mut members = vec![seed as u32];
        shell_of[seed] = id;
        let mut stack = vec![seed];
        while let Some(t) = stack.pop() {
            let tri = triangles[t];
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let u = neighbour(t, k);
                // The neighbour must run along b -> a once both are turned.
                let needs = runs_along(&triangles[u], a, b) != flipped[t];
                if shell_of[u] == u32::MAX {
                    flipped[u] = needs;
                    shell_of[u] = id;
                    members.push(u as u32);
                    stack.push(u);
                } else if flipped[u] != needs {
                    return Err(
                        "the mesh cannot be oriented: its triangles do not agree on a side \
                         (a one-sided surface)"
                            .to_owned(),
                    );
                }
            }
        }
        shells.push(members);
    }
    for (t, tri) in triangles.iter_mut().enumerate() {
        if flipped[t] {
            tri.swap(1, 2);
        }
    }
    // Each shell outwards.
    let diagonal = {
        let b = shell_box(&points, &triangles, &(0..n as u32).collect::<Vec<_>>());
        length(sub(b[1], b[0])).max(f64::MIN_POSITIVE)
    };
    let mut volume = 0.0;
    for members in &shells {
        let v = signed_volume(&points, &triangles, members);
        if v.abs() <= 1e-12 * diagonal.powi(3) {
            return Err("a shell of the mesh encloses no volume".to_owned());
        }
        if v < 0.0 {
            for &t in members {
                triangles[t as usize].swap(1, 2);
            }
        }
        volume += v.abs();
    }
    // Shells inside others (hollow parts) are refused.
    if shells.len() > 1 {
        let boxes: Vec<[Point; 2]> = shells
            .iter()
            .map(|members| shell_box(&points, &triangles, members))
            .collect();
        // A direction no mesh is likely to be aligned with.
        let dir = [0.573_462_7, 0.577_140_3, 0.581_588_9];
        let mut inside = 0;
        for (s, members) in shells.iter().enumerate() {
            let p = points[triangles[members[0] as usize][0] as usize];
            let within = |o: usize| {
                let b = boxes[o];
                (0..3).all(|i| b[0][i] <= p[i] && p[i] <= b[1][i])
            };
            let nested = (0..shells.len()).filter(|&o| o != s && within(o)).any(|o| {
                let crossings = shells[o]
                    .iter()
                    .filter(|&&t| {
                        ray_crosses(p, dir, triangles[t as usize].map(|v| points[v as usize]))
                    })
                    .count();
                crossings % 2 == 1
            });
            if nested {
                inside += 1;
            }
        }
        if inside > 0 {
            return Err(format!(
                "{inside} of the mesh's {} shells lie inside other shells (a hollow part); \
                 Faceted solid makes a solid of each shell that lies apart",
                shells.len()
            ));
        }
    }

    // The oriented half-edges: (triangle, edge) by their vertices.
    let mut half: HashMap<u64, u32> = HashMap::with_capacity(n * 3);
    for (t, tri) in triangles.iter().enumerate() {
        for k in 0..3 {
            half.insert(half_key(tri[k], tri[(k + 1) % 3]), (t * 3 + k) as u32);
        }
    }
    let across = |h: usize| -> usize {
        let tri = triangles[h / 3];
        let k = h % 3;
        half[&half_key(tri[(k + 1) % 3], tri[k])] as usize
    };

    // 4. Planar faces.
    let normals: Vec<Point> = triangles
        .iter()
        .map(|tri| {
            let [a, b, c] = tri.map(|v| points[v as usize]);
            cross(sub(b, a), sub(c, a))
        })
        .collect();
    let area: f64 = normals.iter().map(|m| length(*m) / 2.0).sum();
    let flat = 1e-12 * diagonal * diagonal;
    let cos_angle = angle.cos();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| length(normals[b]).total_cmp(&length(normals[a])));
    let mut face_of = vec![u32::MAX; n];
    let mut faces: Vec<Vec<u32>> = Vec::new();
    for &seed in &order {
        if face_of[seed] != u32::MAX {
            continue;
        }
        let id = faces.len() as u32;
        let m = length(normals[seed]);
        let unit = if m > flat {
            scaled(normals[seed], 1.0 / m)
        } else {
            [0.0; 3]
        };
        let origin = points[triangles[seed][0] as usize];
        let fits = |u: usize| {
            if m <= flat {
                return false; // a degenerate seed stays alone
            }
            let mu = length(normals[u]);
            (mu <= flat || dot(normals[u], unit) >= cos_angle * mu)
                && triangles[u]
                    .iter()
                    .all(|&v| dot(sub(points[v as usize], origin), unit).abs() <= tolerance)
        };
        face_of[seed] = id;
        let mut members = vec![seed as u32];
        let mut stack = vec![seed];
        while let Some(t) = stack.pop() {
            for k in 0..3 {
                let u = across(t * 3 + k) / 3;
                if face_of[u] == u32::MAX && fits(u) {
                    face_of[u] = id;
                    members.push(u as u32);
                    stack.push(u);
                }
            }
        }
        faces.push(members);
    }
    let boundary_he = |h: usize| face_of[h / 3] != face_of[across(h) / 3];

    // 5. Each face's loops of half-edges.
    let start_of = |h: usize| triangles[h / 3][h % 3];
    let end_of = |h: usize| triangles[h / 3][(h % 3 + 1) % 3];
    // The boundary half-edge after `h` in its face's loop: around h's end,
    // across the face's own edges until one is on the boundary.
    let next_boundary = |h: usize| -> Result<usize, String> {
        let mut c = (h / 3) * 3 + (h % 3 + 1) % 3;
        for _ in 0..n.max(3) {
            if boundary_he(c) {
                return Ok(c);
            }
            let twin = across(c);
            c = (twin / 3) * 3 + (twin % 3 + 1) % 3;
        }
        Err("a face's boundary does not close".to_owned())
    };
    let mut visited = vec![false; n * 3];
    let mut face_loops: Vec<Vec<Vec<usize>>> = vec![Vec::new(); faces.len()];
    for (f, members) in faces.iter().enumerate() {
        for &t in members {
            for k in 0..3 {
                let h = t as usize * 3 + k;
                if visited[h] || !boundary_he(h) {
                    continue;
                }
                let mut ring = Vec::new();
                let mut c = h;
                loop {
                    if visited[c] {
                        return Err("a face's boundary runs into another loop".to_owned());
                    }
                    visited[c] = true;
                    ring.push(c);
                    c = next_boundary(c)?;
                    if c == h {
                        break;
                    }
                }
                face_loops[f].push(ring);
            }
        }
    }

    // Runs of edges between corners: a vertex on other than two boundary
    // edges is a corner; so is the first vertex of a closed run.
    let mut links: HashMap<u32, Vec<u32>> = HashMap::new();
    for h in 0..n * 3 {
        let (a, b) = (start_of(h), end_of(h));
        if a < b && boundary_he(h) {
            links.entry(a).or_default().push(b);
            links.entry(b).or_default().push(a);
        }
    }
    let corner = |v: u32| links.get(&v).is_none_or(|l| l.len() != 2);
    let mut used: HashSet<u64> = HashSet::new();
    let mut runs: Vec<(Vec<u32>, bool)> = Vec::new();
    let walk = |start: u32, first: u32, used: &mut HashSet<u64>| -> Vec<u32> {
        let mut run = vec![start, first];
        used.insert(edge_key(start, first));
        let (mut prev, mut cur) = (start, first);
        while !corner(cur) && cur != start {
            let l = &links[&cur];
            let next = if l[0] == prev { l[1] } else { l[0] };
            if !used.insert(edge_key(cur, next)) {
                break;
            }
            run.push(next);
            prev = cur;
            cur = next;
        }
        run
    };
    let mut starts: Vec<u32> = links.keys().copied().collect();
    starts.sort_unstable();
    for &v in &starts {
        if !corner(v) {
            continue;
        }
        let mut ends = links[&v].clone();
        ends.sort_unstable();
        for w in ends {
            if !used.contains(&edge_key(v, w)) {
                runs.push((walk(v, w, &mut used), false));
            }
        }
    }
    for &v in &starts {
        let w = links[&v][0];
        if !corner(v) && !used.contains(&edge_key(v, w)) {
            runs.push((walk(v, w, &mut used), true));
        }
    }

    // Straight edges along the runs, and each half-edge's edge.
    let mut vertex_index: HashMap<u32, usize> = HashMap::new();
    let mut boundary = BoundaryBody::default();
    let mut edge_of: HashMap<u64, (usize, bool)> = HashMap::new();
    let mut vertex = |v: u32, body: &mut BoundaryBody| -> usize {
        *vertex_index.entry(v).or_insert_with(|| {
            body.vertices.push(points[v as usize]);
            body.vertices.len() - 1
        })
    };
    let straight = |run: &[u32], i: usize, e: usize| -> bool {
        let a = points[run[i] as usize];
        let b = points[run[e] as usize];
        let d = sub(b, a);
        let l2 = dot(d, d);
        if l2 <= 0.0 {
            return false;
        }
        run[i + 1..e].iter().all(|&v| {
            let p = sub(points[v as usize], a);
            let s = dot(p, d) / l2;
            s > 0.0 && s < 1.0 && length(sub(p, scaled(d, s))) <= tolerance
        })
    };
    for (run, closed) in &runs {
        let last = run.len() - 1;
        let mut pieces = Vec::new();
        let mut i = 0;
        while i < last {
            let mut j = i + 1;
            while j < last && j - i < MAX_RUN && straight(run, i, j + 1) {
                j += 1;
            }
            pieces.push((i, j));
            i = j;
        }
        // A closed run keeps at least three edges.
        if *closed && pieces.len() < 3 {
            pieces = (0..last).map(|k| (k, k + 1)).collect();
        }
        for (i, j) in pieces {
            let (a, b) = (run[i], run[j]);
            let start = vertex(a, &mut boundary);
            let end = vertex(b, &mut boundary);
            let d = sub(points[b as usize], points[a as usize]);
            let l = length(d);
            let index = boundary.edges.len();
            boundary.curves.push(BoundaryCurve::Line {
                origin: points[a as usize],
                dir: scaled(d, 1.0 / l),
            });
            boundary.edges.push(BoundaryEdge {
                curve: index,
                range: [0.0, l],
                vertices: [start, end],
            });
            for k in i..j {
                edge_of.insert(half_key(run[k], run[k + 1]), (index, true));
                edge_of.insert(half_key(run[k + 1], run[k]), (index, false));
            }
        }
    }

    // The faces: planes through their first triangle's corner with their
    // triangles' mean normal, loops of edges, the outer one first.
    let mut face_shell = Vec::with_capacity(faces.len());
    for (f, members) in faces.iter().enumerate() {
        let sum = members
            .iter()
            .fold([0.0; 3], |s, &t| plus(s, normals[t as usize]));
        let m = length(sum);
        if m <= flat {
            return Err(format!(
                "face {f} of the mesh has no area (its triangles are degenerate)"
            ));
        }
        let normal = scaled(sum, 1.0 / m);
        let origin = points[triangles[members[0] as usize][0] as usize];
        let axis = if normal[0].abs() <= normal[1].abs() && normal[0].abs() <= normal[2].abs() {
            [1.0, 0.0, 0.0]
        } else if normal[1].abs() <= normal[2].abs() {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let u = cross(axis, normal);
        let u_dir = scaled(u, 1.0 / length(u));
        boundary.surfaces.push(BoundarySurface::Plane {
            origin,
            normal,
            u_dir,
        });
        // The loops by their area seen from outside: the largest outer.
        let mut loops: Vec<(f64, Vec<(usize, bool)>)> = Vec::new();
        for ring in &face_loops[f] {
            let p0 = points[start_of(ring[0]) as usize];
            let mut twice = 0.0;
            for &h in ring {
                let a = sub(points[start_of(h) as usize], p0);
                let b = sub(points[end_of(h) as usize], p0);
                twice += dot(cross(a, b), normal);
            }
            let mut coedges: Vec<(usize, bool)> = Vec::new();
            for &h in ring {
                let Some(&e) = edge_of.get(&half_key(start_of(h), end_of(h))) else {
                    return Err("an edge of a face's boundary has no curve".to_owned());
                };
                if coedges.last() != Some(&e) {
                    coedges.push(e);
                }
            }
            if coedges.len() > 1 && coedges.first() == coedges.last() {
                coedges.pop();
            }
            loops.push((twice, coedges));
        }
        loops.sort_by(|a, b| b.0.total_cmp(&a.0));
        boundary.faces.push(BoundaryFace {
            surface: f,
            reversed: false,
            loops: loops.into_iter().map(|(_, l)| l).collect(),
        });
        face_shell.push(shell_of[members[0] as usize] as usize);
    }
    boundary.shells = vec![Vec::new(); shells.len()];
    for (f, s) in face_shell.into_iter().enumerate() {
        boundary.shells[s].push(f);
    }
    Ok(Faceted {
        boundary,
        volume,
        area,
        shells: shells.len(),
        welded,
        degenerate,
    })
}
