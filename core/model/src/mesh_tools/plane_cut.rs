// SPDX-License-Identifier: MIT
//! Plane Cut (mitcad#2515): a mesh cut by a plane into the piece above it
//! (on the side its normal points to) and the piece below.
//!
//! Each vertex's distance from the plane decides its side; one within a
//! billionth of the mesh's size lies on the plane and belongs to both
//! pieces. A triangle across the plane is clipped into a part on either
//! side, the point where an edge crosses the plane shared by the triangles
//! of that edge, so that each piece is closed where the mesh was. A
//! triangle in the plane belongs to neither piece.
//!
//! Capped: the rims a piece has in the plane (its loops of open edges whose
//! corners all lie in the plane) are closed by triangles in the plane, the
//! loops nested as the section is (an outline with holes, islands in the
//! holes): each hole joined to its outline by a bridge, then ears clipped.
//! A rim that leaves the plane (an open mesh cut across its opening) is
//! left open and counted.

use std::collections::HashMap;

use mitcad_vec3::{add, cross, dot, scale, sub};

use super::{Point, TriangleMesh, edge_key, size, submesh};

/// The pieces of a mesh cut by a plane.
#[derive(Debug, Clone, PartialEq)]
pub struct CutPieces {
    /// On the side the plane's normal points to.
    pub above: TriangleMesh,
    pub below: TriangleMesh,
    /// Rims closed by a cap, both pieces together.
    pub capped: usize,
    /// Rims that leave the plane, left open, both pieces together.
    pub open: usize,
}

/// The mesh cut by the plane through `origin` with the normal `normal`
/// (see the module docs); `mesh` has its shared corners joined.
pub fn plane_cut(
    mesh: &TriangleMesh,
    origin: Point,
    normal: Point,
    cap: bool,
) -> Result<CutPieces, String> {
    let normal = mitcad_vec3::unit(normal).ok_or("the cutting plane has no normal")?;
    let eps = (1e-9 * size(&mesh.vertices)).max(1e-12);
    let side: Vec<f64> = mesh
        .vertices
        .iter()
        .map(|p| {
            let d = dot(sub(*p, origin), normal);
            if d.abs() <= eps { 0.0 } else { d }
        })
        .collect();
    let mut used = vec![false; mesh.vertices.len()];
    for tri in &mesh.triangles {
        for v in tri {
            used[*v as usize] = true;
        }
    }
    let above_any = side.iter().zip(&used).any(|(d, u)| *u && *d > 0.0);
    let below_any = side.iter().zip(&used).any(|(d, u)| *u && *d < 0.0);
    if !above_any || !below_any {
        return Err(format!(
            "the plane does not cut the mesh: the whole mesh lies {} it",
            if above_any { "above" } else { "below" }
        ));
    }
    let mut vertices = mesh.vertices.clone();
    let mut in_plane: Vec<bool> = side.iter().map(|d| *d == 0.0).collect();
    let mut crossing: HashMap<(u32, u32), u32> = HashMap::new();
    let mut above: Vec<[u32; 3]> = Vec::new();
    let mut below: Vec<[u32; 3]> = Vec::new();
    for tri in &mesh.triangles {
        let mut up: Vec<u32> = Vec::with_capacity(4);
        let mut down: Vec<u32> = Vec::with_capacity(4);
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            let (da, db) = (side[a as usize], side[b as usize]);
            if da >= 0.0 {
                up.push(a);
            }
            if da <= 0.0 {
                down.push(a);
            }
            if (da > 0.0 && db < 0.0) || (da < 0.0 && db > 0.0) {
                let x = *crossing.entry(edge_key(a, b)).or_insert_with(|| {
                    let (pa, pb) = (mesh.vertices[a as usize], mesh.vertices[b as usize]);
                    let t = da / (da - db);
                    vertices.push(add(pa, scale(sub(pb, pa), t)));
                    in_plane.push(true);
                    (vertices.len() - 1) as u32
                });
                up.push(x);
                down.push(x);
            }
        }
        if up.len() >= 3
            && up
                .iter()
                .any(|v| side.get(*v as usize).is_some_and(|d| *d > 0.0))
        {
            for i in 1..up.len() - 1 {
                above.push([up[0], up[i], up[i + 1]]);
            }
        }
        if down.len() >= 3
            && down
                .iter()
                .any(|v| side.get(*v as usize).is_some_and(|d| *d < 0.0))
        {
            for i in 1..down.len() - 1 {
                below.push([down[0], down[i], down[i + 1]]);
            }
        }
    }
    let mut capped = 0;
    let mut open = 0;
    if cap {
        let u = super::perpendicular(normal);
        let v = cross(normal, u);
        let flat = |p: Point| [dot(p, u), dot(p, v)];
        for piece in [&mut above, &mut below] {
            let mut rims: Vec<Vec<u32>> = Vec::new();
            for lp in super::repair::boundary_loops(piece) {
                let on = lp.iter().filter(|x| in_plane[**x as usize]).count();
                if on == lp.len() {
                    rims.push(lp);
                } else if on >= 2 {
                    open += 1;
                }
            }
            capped += rims.len();
            let points: HashMap<u32, [f64; 2]> = rims
                .iter()
                .flatten()
                .map(|x| (*x, flat(vertices[*x as usize])))
                .collect();
            piece.extend(cap_loops(&rims, &|x| points[&x]));
        }
    }
    let whole = TriangleMesh {
        vertices,
        triangles: Vec::new(),
    };
    let piece = |triangles: Vec<[u32; 3]>| {
        let mesh = TriangleMesh {
            vertices: whole.vertices.clone(),
            triangles,
        };
        submesh(&mesh, |_| true)
    };
    Ok(CutPieces {
        above: piece(above),
        below: piece(below),
        capped,
        open,
    })
}

fn signed_area(lp: &[u32], at: &dyn Fn(u32) -> [f64; 2]) -> f64 {
    let n = lp.len();
    (0..n)
        .map(|i| {
            let a = at(lp[i]);
            let b = at(lp[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn inside(lp: &[u32], at: &dyn Fn(u32) -> [f64; 2], p: [f64; 2]) -> bool {
    let n = lp.len();
    let mut odd = false;
    for i in 0..n {
        let (a, b) = (at(lp[i]), at(lp[(i + 1) % n]));
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                odd = !odd;
            }
        }
    }
    odd
}

/// Triangles that close the loops (vertex numbers, in the loops' order, as
/// [`super::repair::boundary_loops`] gives them), the loops lying in one
/// plane, `at` their points in it: the outlines and their holes, nested.
pub(crate) fn cap_loops(loops: &[Vec<u32>], at: &dyn Fn(u32) -> [f64; 2]) -> Vec<[u32; 3]> {
    if loops.is_empty() {
        return Vec::new();
    }
    let areas: Vec<f64> = loops.iter().map(|lp| signed_area(lp, at)).collect();
    // The largest loop is an outline: the outlines turn its way.
    let largest = (0..loops.len())
        .max_by(|&a, &b| areas[a].abs().total_cmp(&areas[b].abs()))
        .unwrap_or(0);
    let flip = areas[largest] < 0.0;
    let at_turned = |x: u32| {
        let p = at(x);
        if flip { [p[0], -p[1]] } else { p }
    };
    let areas: Vec<f64> = areas.iter().map(|a| if flip { -*a } else { *a }).collect();
    let outlines: Vec<usize> = (0..loops.len()).filter(|&i| areas[i] > 0.0).collect();
    let mut holes_of: Vec<Vec<usize>> = vec![Vec::new(); loops.len()];
    for hole in (0..loops.len()).filter(|&i| areas[i] < 0.0) {
        let probe = at_turned(loops[hole][0]);
        let owner = outlines
            .iter()
            .copied()
            .filter(|&o| inside(&loops[o], &at_turned, probe))
            .min_by(|&a, &b| areas[a].total_cmp(&areas[b]));
        if let Some(o) = owner {
            holes_of[o].push(hole);
        }
    }
    let mut out = Vec::new();
    for &o in &outlines {
        let mut polygon = loops[o].clone();
        let mut holes: Vec<&Vec<u32>> = holes_of[o].iter().map(|&h| &loops[h]).collect();
        // The hole reaching farthest along x first.
        let reach = |lp: &Vec<u32>| {
            lp.iter()
                .map(|x| at_turned(*x)[0])
                .fold(f64::NEG_INFINITY, f64::max)
        };
        holes.sort_by(|a, b| reach(b).total_cmp(&reach(a)));
        for hole in holes {
            polygon = bridge(&polygon, hole, &at_turned);
        }
        out.extend(clip_ears(&polygon, &at_turned));
    }
    out
}

/// The outline with the hole joined in by a bridge from the hole's
/// rightmost corner to a corner of the outline it sees (Eberly's way).
fn bridge(polygon: &[u32], hole: &[u32], at: &dyn Fn(u32) -> [f64; 2]) -> Vec<u32> {
    let m_index = (0..hole.len())
        .max_by(|&a, &b| at(hole[a])[0].total_cmp(&at(hole[b])[0]))
        .unwrap_or(0);
    let m = at(hole[m_index]);
    let n = polygon.len();
    // The nearest crossing of the ray from m along +x with an edge.
    let mut best: Option<(f64, usize)> = None;
    for i in 0..n {
        let a = at(polygon[i]);
        let b = at(polygon[(i + 1) % n]);
        if (a[1] > m[1]) == (b[1] > m[1]) {
            continue;
        }
        let x = a[0] + (m[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
        if x >= m[0] && best.is_none_or(|(bx, _)| x < bx) {
            best = Some((x, i));
        }
    }
    let p_index = match best {
        None => (0..n)
            .min_by(|&a, &b| {
                let da = sub2(at(polygon[a]), m);
                let db = sub2(at(polygon[b]), m);
                (da[0] * da[0] + da[1] * da[1]).total_cmp(&(db[0] * db[0] + db[1] * db[1]))
            })
            .unwrap_or(0),
        Some((x, i)) => {
            let j = (i + 1) % n;
            let candidate = if at(polygon[i])[0] > at(polygon[j])[0] {
                i
            } else {
                j
            };
            let p = at(polygon[candidate]);
            let hit = [x, m[1]];
            // A corner of the outline inside the triangle (m, hit, p)
            // blocks the view: the one at the smallest angle from the ray.
            let mut chosen = candidate;
            let mut chosen_key = (f64::INFINITY, f64::INFINITY);
            for k in 0..n {
                if polygon[k] == polygon[candidate] {
                    continue;
                }
                let q = at(polygon[k]);
                if q[0] < m[0] || !in_triangle(q, m, hit, p) {
                    continue;
                }
                let d = sub2(q, m);
                let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
                if length <= 0.0 {
                    continue;
                }
                let key = ((d[1] / length).abs(), length);
                if key < chosen_key {
                    chosen_key = key;
                    chosen = k;
                }
            }
            chosen
        }
    };
    let mut out = Vec::with_capacity(n + hole.len() + 2);
    out.extend_from_slice(&polygon[..=p_index]);
    for k in 0..=hole.len() {
        out.push(hole[(m_index + k) % hole.len()]);
    }
    out.push(polygon[p_index]);
    out.extend_from_slice(&polygon[p_index + 1..]);
    out
}

fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn turn(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Whether `p` lies in the triangle or on its sides, either way round.
fn in_triangle(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let d1 = turn(a, b, p);
    let d2 = turn(b, c, p);
    let d3 = turn(c, a, p);
    let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(negative && positive)
}

/// Triangles of a polygon turning counter-clockwise (in `at`), corners
/// possibly repeated where bridges join holes in: ears clipped, each with
/// no other corner in it or on its sides. Where no ear is free (a polygon
/// that crosses itself) the most convex corner goes.
fn clip_ears(polygon: &[u32], at: &dyn Fn(u32) -> [f64; 2]) -> Vec<[u32; 3]> {
    let mut ring: Vec<u32> = polygon.to_vec();
    let mut out = Vec::with_capacity(ring.len().saturating_sub(2));
    let scale_area = {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for x in &ring {
            let p = at(*x);
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let d = [max[0] - min[0], max[1] - min[1]];
        (d[0] * d[0] + d[1] * d[1]).max(1e-300)
    };
    let eps = 1e-14 * scale_area;
    let mut i = 0;
    let mut misses = 0;
    while ring.len() > 3 {
        let n = ring.len();
        let (ip, inx) = ((i + n - 1) % n, (i + 1) % n);
        let (a, b, c) = (ring[ip], ring[i], ring[inx]);
        let (pa, pb, pc) = (at(a), at(b), at(c));
        let convex = turn(pa, pb, pc) > eps;
        let ear = convex
            && ring.iter().all(|&x| {
                x == a || x == b || x == c || {
                    let q = at(x);
                    !in_triangle(q, pa, pb, pc)
                }
            });
        if ear {
            out.push([a, b, c]);
            ring.remove(i);
            if i >= ring.len() {
                i = 0;
            }
            misses = 0;
            continue;
        }
        misses += 1;
        i = (i + 1) % n;
        if misses > n {
            // No free ear: the most convex corner goes.
            let k = (0..n)
                .max_by(|&x, &y| {
                    let tx = turn(
                        at(ring[(x + n - 1) % n]),
                        at(ring[x]),
                        at(ring[(x + 1) % n]),
                    );
                    let ty = turn(
                        at(ring[(y + n - 1) % n]),
                        at(ring[y]),
                        at(ring[(y + 1) % n]),
                    );
                    tx.total_cmp(&ty)
                })
                .unwrap_or(0);
            out.push([ring[(k + n - 1) % n], ring[k], ring[(k + 1) % n]]);
            ring.remove(k);
            i = 0;
            misses = 0;
        }
    }
    if ring.len() == 3 {
        out.push([ring[0], ring[1], ring[2]]);
    }
    out
}
