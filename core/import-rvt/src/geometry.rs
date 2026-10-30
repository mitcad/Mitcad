// SPDX-License-Identifier: MIT
//! Elements as triangle meshes of the geometry the file keeps
//! (mitcad#333, [`mitcad_rvt::brep`]): an element's own bodies and the
//! types it places (a family instance's type at its transform, nested
//! types at theirs), in model millimetres.
//!
//! Each face is triangulated within its loops: a plane's (and a face of a
//! surface not read: its points from the neighbouring faces) in the
//! face's plane; a curved face's in its (u, v), each triangle halved until
//! its sides lie within [`CHORD`] of the surface. The triangles keep the
//! loops' direction; a body whose closed mesh encloses a negative volume
//! is turned over, as is a type placed by a mirroring transform.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use mitcad_ifc::geometry::triangulate;
use mitcad_ifc::math;
use mitcad_rvt::brep::{GRep, P3, Solid, Surface, Uv};
use mitcad_rvt::elements::{FOOT_MM, Model};
use mitcad_vec3::distance as dist;

use mitcad_import_ifc::brep::{mesh_closed, mesh_measures};

/// How far (feet) a curved face's triangles may lie from the surface.
pub const CHORD: f64 = 0.01;

/// How many times a curved face's triangle is halved at most.
const MAX_SPLITS: usize = 10;

/// Types placed in types are followed this deep.
const DEPTH: usize = 6;

/// Types placed per element at most, nested ones counted (mitcad#924):
/// each type follows its placed types six levels deep, so a crafted file
/// of a few types each placing the next a thousand times would place
/// 10^18. Real elements place one type (a family instance) or a few
/// hundred (a family's nested arrays); railings, curtain walls and
/// stairs place theirs as elements of their own.
const MAX_PLACED: usize = 100_000;

/// Triangles per element at most before no more types are placed: what
/// the placements copy (a type's mesh is triangulated once, then copied
/// per placement). Real elements' meshes stay below a million.
const MAX_TRIANGLES: usize = 10_000_000;

/// An element's mesh (mm).
#[derive(Debug, Clone, Default)]
pub struct ElementMesh {
    pub points: Vec<P3>,
    pub triangles: Vec<[u32; 3]>,
    /// Bodies and faces meshed, faces left out (and why, the first few).
    pub bodies: usize,
    pub faces: usize,
    pub faces_left_out: usize,
    pub notes: Vec<String>,
    /// Whether every edge joins two triangles, and the volume (mm³) then.
    pub closed: bool,
    pub volume: Option<f64>,
    /// Types placed (nested ones counted, [`MAX_PLACED`]), and whether
    /// the limits left placements out.
    pub placed: usize,
    pub placed_capped: bool,
}

impl ElementMesh {
    /// The box of its points (mm).
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = self.points.first()?;
        let (mut lo, mut hi) = (*first, *first);
        for p in &self.points {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        Some((lo, hi))
    }
}

type Trf = [f64; 12];

const IDENTITY: Trf = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn apply(t: &Trf, p: P3) -> P3 {
    std::array::from_fn(|r| t[3 * r] * p[0] + t[3 * r + 1] * p[1] + t[3 * r + 2] * p[2] + t[9 + r])
}

/// `outer` after `inner`.
fn compose(outer: &Trf, inner: &Trf) -> Trf {
    let mut m = [0.0; 12];
    for r in 0..3 {
        for c in 0..3 {
            m[3 * r + c] = (0..3).map(|k| outer[3 * r + k] * inner[3 * k + c]).sum();
        }
    }
    let o = apply(outer, [inner[9], inner[10], inner[11]]);
    m[9..].copy_from_slice(&o);
    m
}

fn determinant(t: &Trf) -> f64 {
    t[0] * (t[4] * t[8] - t[5] * t[7]) - t[1] * (t[3] * t[8] - t[5] * t[6])
        + t[2] * (t[3] * t[7] - t[4] * t[6])
}

/// An element's mesh, None without geometry of its own; the types and
/// elements in `skip` are left out where it places them (a railing's
/// balusters and rails, elements of their own).
pub fn element_mesh(
    model: &Model,
    id: i64,
    cache: &mut Cache,
    skip: &HashSet<i64>,
) -> Option<Result<ElementMesh, String>> {
    let g = model.geometry.get(&id)?;
    let mut out = ElementMesh::default();
    let mut seen = skip.clone();
    add_grep(model, g, &IDENTITY, 0, &mut seen, cache, &mut out);
    if out.triangles.is_empty() {
        let why = out
            .notes
            .first()
            .cloned()
            .unwrap_or_else(|| "its geometry has no faces".to_string());
        return Some(Err(why));
    }
    out.closed = mesh_closed(&out.points, &out.triangles);
    if out.closed {
        let (mut v, _) = mesh_measures(&out.points, &out.triangles);
        if v < 0.0 {
            for t in &mut out.triangles {
                t.swap(1, 2);
            }
            v = -v;
        }
        out.volume = Some(v);
    }
    Some(Ok(out))
}

/// A body's triangles (feet, its own coordinates), facing out (mitcad#345:
/// a family type's void geometry, cut from its host); an error when a face
/// is left out or the triangles do not close.
pub fn solid_mesh(s: &Solid) -> Result<(Vec<P3>, Vec<[u32; 3]>), String> {
    let g = GRep {
        solids: vec![s.clone()],
        ..GRep::default()
    };
    let tess = tessellate(&g);
    if tess.left_out > 0 {
        return Err(tess
            .notes
            .first()
            .cloned()
            .unwrap_or_else(|| "a face is left out".to_string()));
    }
    let (points, triangles) = tess.bodies.into_iter().next().ok_or("it has no faces")?;
    if !mesh_closed(&points, &triangles) {
        return Err("its faces do not close".into());
    }
    Ok((points, triangles))
}

/// A closed body of planes as its faces (feet, its own coordinates): each
/// face its outer loop and then its holes, facing out; None when a face
/// is not a plane (its triangles then, [`solid_mesh`]).
pub fn solid_faces(s: &Solid) -> Result<Option<Vec<Vec<Vec<P3>>>>, String> {
    if s.faces
        .iter()
        .any(|f| !matches!(f.surface, Surface::Plane { .. }))
    {
        return Ok(None);
    }
    // (Closed, every face read.)
    solid_mesh(s)?;
    let mut faces = Vec::new();
    for f in 0..s.faces.len() {
        if s.faces[f].loops.is_empty() {
            continue;
        }
        let loops: Vec<Vec<P3>> = (0..s.faces[f].loops.len())
            .map(|l| loop_points(s, f, l))
            .collect::<Result<_, _>>()?;
        faces.extend(nested(&loops));
    }
    let volume: f64 = faces
        .iter()
        .flatten()
        .map(|l| {
            (1..l.len().saturating_sub(1))
                .map(|k| math::dot(l[0], math::cross(l[k], l[k + 1])))
                .sum::<f64>()
        })
        .sum();
    if volume < 0.0 {
        for l in faces.iter_mut().flatten() {
            l.reverse();
        }
    }
    Ok(Some(faces))
}

/// A geometry's own bodies triangulated (feet, its own coordinates), each
/// body's triangles facing out where it is closed.
#[derive(Debug, Default)]
pub struct Tess {
    pub bodies: Vec<(Vec<P3>, Vec<[u32; 3]>)>,
    pub faces: usize,
    pub left_out: usize,
    pub notes: Vec<String>,
}

/// Triangulated geometries by their address in the model (a type placed
/// many times is triangulated once).
pub type Cache = HashMap<usize, Rc<Tess>>;

fn tessellate(g: &GRep) -> Tess {
    let mut out = Tess::default();
    for s in &g.solids {
        // A single plane bounds no volume: a sheet (a wall's layer
        // boundaries up to its unconnected height, a symbol's face), not a
        // body a 3D view shows.
        if s.faces.len() == 1 && matches!(s.faces[0].surface, Surface::Plane { .. }) {
            continue;
        }
        let mut points: Vec<P3> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        for f in 0..s.faces.len() {
            // A plane without loops bounds nothing: a face left over in the
            // body, its edges all on other faces (seen in doors' and
            // windows' void geometry).
            if s.faces[f].loops.is_empty() && matches!(s.faces[f].surface, Surface::Plane { .. }) {
                continue;
            }
            match face_mesh(s, f, &mut points, &mut triangles) {
                Ok(()) => out.faces += 1,
                Err(why) => {
                    out.left_out += 1;
                    if out.notes.len() < 5 {
                        out.notes.push(format!(
                            "a face ({}) is left out: {why}",
                            s.faces[f].surface.class()
                        ));
                    }
                }
            }
        }
        if triangles.is_empty() {
            continue;
        }
        // A closed body turned inside out: turned over.
        if mesh_closed(&points, &triangles) && mesh_measures(&points, &triangles).0 < 0.0 {
            for t in &mut triangles {
                t.swap(1, 2);
            }
        }
        out.bodies.push((points, triangles));
    }
    out
}

fn add_grep(
    model: &Model,
    g: &GRep,
    t: &Trf,
    depth: usize,
    seen: &mut HashSet<i64>,
    cache: &mut Cache,
    out: &mut ElementMesh,
) {
    let mirrored = determinant(t) < 0.0;
    let tess = cache
        .entry(g as *const GRep as usize)
        .or_insert_with(|| Rc::new(tessellate(g)))
        .clone();
    out.faces += tess.faces;
    out.faces_left_out += tess.left_out;
    for n in &tess.notes {
        if out.notes.len() < 5 {
            out.notes.push(n.clone());
        }
    }
    for (points, triangles) in &tess.bodies {
        let first = out.points.len() as u32;
        out.points
            .extend(points.iter().map(|p| apply(t, *p).map(|v| v * FOOT_MM)));
        out.triangles.extend(triangles.iter().map(|tr| {
            let mut tr = tr.map(|i| i + first);
            if mirrored {
                tr.swap(1, 2);
            }
            tr
        }));
        out.bodies += 1;
    }
    if depth >= DEPTH {
        const DEEP: &str = "types placed deeper than six levels are left out";
        // (Once: a crafted file can reach this level many times.)
        if !g.instances.is_empty() && !out.notes.iter().any(|n| n == DEEP) {
            out.notes.push(DEEP.into());
        }
        return;
    }
    for p in &g.instances {
        if out.placed >= MAX_PLACED || out.triangles.len() >= MAX_TRIANGLES {
            if !out.placed_capped {
                out.placed_capped = true;
                out.notes.push(format!(
                    "more than {MAX_PLACED} placed types or {MAX_TRIANGLES} triangles: the rest are left out"
                ));
            }
            return;
        }
        out.placed += 1;
        let placed = compose(t, &p.transform);
        if let Some(e) = &p.embedded {
            add_grep(model, e, &placed, depth + 1, seen, cache, out);
        }
        if let Some(sym) = p.symbol {
            match model.geometry.get(&sym) {
                // (A type placing itself would not end.)
                Some(sg) if seen.insert(sym) => {
                    add_grep(model, sg, &placed, depth + 1, seen, cache, out);
                    seen.remove(&sym);
                }
                Some(_) => {}
                None if p.embedded.is_none() && out.notes.len() < 5 => {
                    out.notes
                        .push(format!("the type {sym} it places keeps no geometry"));
                }
                None => {}
            }
        }
    }
}

/// A loop's model points (feet): each edge's points on this face's
/// surface, or on the other face's where this one's is not read, chained
/// in the loop's direction.
fn loop_points(s: &Solid, face: usize, l: usize) -> Result<Vec<P3>, String> {
    let surface = &s.faces[face].surface;
    let mut runs: Vec<Vec<P3>> = Vec::new();
    for (e, side) in &s.faces[face].loops[l] {
        let edge = &s.edges[*e];
        let own: Option<Vec<P3>> = edge.points[*side]
            .iter()
            .map(|uv| surface.at(*uv))
            .collect();
        let run = match own {
            Some(r) => r,
            None => {
                let other = 1 - side;
                let f = edge.faces[other].ok_or("an open edge on a surface not read")?;
                edge.points[other]
                    .iter()
                    .map(|uv| s.faces[f].surface.at(*uv))
                    .collect::<Option<Vec<P3>>>()
                    .ok_or("an edge between two surfaces not read")?
            }
        };
        runs.push(run);
    }
    let mut out: Vec<P3> = Vec::new();
    let n = runs.len();
    for k in 0..n {
        let run = &runs[k];
        let (Some(first), Some(last)) = (run.first().copied(), run.last().copied()) else {
            continue;
        };
        let forward = match out.last() {
            Some(end) => dist(first, *end) <= dist(last, *end),
            None if n > 1 => {
                let next = &runs[(k + 1) % n];
                let near = |p: P3| {
                    let a = next.first().map_or(f64::INFINITY, |q| dist(p, *q));
                    let b = next.last().map_or(f64::INFINITY, |q| dist(p, *q));
                    a.min(b)
                };
                near(last) <= near(first)
            }
            None => true,
        };
        let mut pts = run.clone();
        if !forward {
            pts.reverse();
        }
        for p in pts {
            if out.last().is_none_or(|q| dist(*q, p) > 1e-9) {
                out.push(p);
            }
        }
    }
    if out.len() > 1 && dist(out[0], out[out.len() - 1]) <= 1e-9 {
        out.pop();
    }
    Ok(out)
}

/// The loop with the largest area first (the outer one), by Newell's
/// normal's length.
fn outer_first<T: Clone>(loops: &[Vec<T>], to3: impl Fn(&T) -> P3) -> Vec<Vec<T>> {
    let area = |l: &Vec<T>| {
        let mut n = [0.0; 3];
        for k in 0..l.len() {
            let (a, b) = (to3(&l[k]), to3(&l[(k + 1) % l.len()]));
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        math::length(n)
    };
    let mut v: Vec<Vec<T>> = loops.to_vec();
    v.sort_by(|a, b| area(b).total_cmp(&area(a)));
    v
}

/// A plane's loops (model points) grouped as areas, each outer loop first
/// and then the holes in it: a loop within an odd number of others is a
/// hole of the smallest that holds it (a face may have several outer
/// loops, *seen* on in-place masses); loops of no area are left out.
fn nested(loops: &[Vec<P3>]) -> Vec<Vec<Vec<P3>>> {
    let newell = |l: &Vec<P3>| {
        let mut n = [0.0; 3];
        for k in 0..l.len() {
            let (a, b) = (l[k], l[(k + 1) % l.len()]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        n
    };
    let loops: Vec<&Vec<P3>> = loops
        .iter()
        .filter(|l| l.len() >= 3 && math::length(newell(l)) > 1e-9)
        .collect();
    let Some(largest) = loops
        .iter()
        .max_by(|a, b| math::length(newell(a)).total_cmp(&math::length(newell(b))))
    else {
        return Vec::new();
    };
    // In the plane's coordinates: the two axes off its normal's largest.
    let n = newell(largest);
    let k = (0..3)
        .max_by(|a, b| n[*a].abs().total_cmp(&n[*b].abs()))
        .unwrap_or(2);
    let (u, v) = ((k + 1) % 3, (k + 2) % 3);
    let inside = |p: P3, l: &Vec<P3>| {
        let mut c = false;
        for i in 0..l.len() {
            let (a, b) = (l[i], l[(i + 1) % l.len()]);
            if (a[v] > p[v]) != (b[v] > p[v]) {
                let x = a[u] + (p[v] - a[v]) / (b[v] - a[v]) * (b[u] - a[u]);
                if p[u] < x {
                    c = !c;
                }
            }
        }
        c
    };
    let area = |l: &Vec<P3>| math::length(newell(l));
    // Each loop's holders (by a point of it not on the other's boundary:
    // its first).
    let holders: Vec<Vec<usize>> = (0..loops.len())
        .map(|i| {
            (0..loops.len())
                .filter(|j| {
                    *j != i && area(loops[*j]) > area(loops[i]) && inside(loops[i][0], loops[*j])
                })
                .collect()
        })
        .collect();
    let mut out: Vec<(usize, Vec<Vec<P3>>)> = Vec::new();
    for i in 0..loops.len() {
        if holders[i].len().is_multiple_of(2) {
            out.push((i, vec![loops[i].clone()]));
        }
    }
    for i in 0..loops.len() {
        if !holders[i].len().is_multiple_of(2) {
            // The smallest outer loop holding it.
            let owner = holders[i]
                .iter()
                .filter(|j| holders[**j].len().is_multiple_of(2))
                .min_by(|a, b| area(loops[**a]).total_cmp(&area(loops[**b])));
            if let Some(o) = owner
                && let Some(g) = out.iter_mut().find(|g| g.0 == *o)
            {
                g.1.push(loops[i].clone());
            }
        }
    }
    out.into_iter().map(|(_, g)| g).collect()
}

/// Triangles of a polygon with holes (the first loop the outer one),
/// appended with their points.
fn polygon(loops: &[Vec<P3>], points: &mut Vec<P3>, out: &mut Vec<[u32; 3]>) -> usize {
    let mut local: Vec<P3> = Vec::new();
    let mut rings: Vec<Vec<u32>> = Vec::new();
    for l in loops {
        let first = local.len() as u32;
        local.extend_from_slice(l);
        rings.push((first..first + l.len() as u32).collect());
    }
    let tris = triangulate(&local, &rings[0], &rings[1..]);
    let base = points.len() as u32;
    points.extend_from_slice(&local);
    let n = tris.len();
    out.extend(tris.into_iter().map(|t| t.map(|i| i + base)));
    n
}

fn face_mesh(
    s: &Solid,
    face: usize,
    points: &mut Vec<P3>,
    triangles: &mut Vec<[u32; 3]>,
) -> Result<(), String> {
    let f = &s.faces[face];
    if f.loops.is_empty() {
        return Err("it has no loops".into());
    }
    match &f.surface {
        Surface::Plane { .. } | Surface::Other(_) => {
            let loops: Vec<Vec<P3>> = (0..f.loops.len())
                .map(|l| loop_points(s, face, l))
                .collect::<Result<_, _>>()?;
            let groups = nested(&loops);
            if groups.is_empty() {
                return Err("its loops have no area".into());
            }
            for group in groups {
                if polygon(&group, points, triangles) == 0 {
                    return Err("its loops did not triangulate".into());
                }
            }
            Ok(())
        }
        surface => {
            let loops: Vec<Vec<Uv>> = (0..f.loops.len()).map(|l| s.loop_uv(face, l)).collect();
            let loops: Vec<Vec<Uv>> = outer_first(&loops, |p| [p[0], p[1], 0.0])
                .into_iter()
                .filter(|l| l.len() >= 3)
                .collect();
            if loops.is_empty() {
                return band(s, face, surface, points, triangles);
            }
            let flat: Vec<Vec<P3>> = loops
                .iter()
                .map(|l| l.iter().map(|p| [p[0], p[1], 0.0]).collect())
                .collect();
            let mut uv_points = Vec::new();
            let mut uv_tris = Vec::new();
            if polygon(&flat, &mut uv_points, &mut uv_tris) == 0 {
                return band(s, face, surface, points, triangles);
            }
            for t in uv_tris {
                let tri = t.map(|i| {
                    let p = uv_points[i as usize];
                    [p[0], p[1]]
                });
                curved_triangle(surface, tri, 0, points, triangles)?;
            }
            Ok(())
        }
    }
}

/// A curved face's triangle in (u, v): on the surface, halved across the
/// side whose middle lies furthest off its chord while that is more than
/// [`CHORD`].
fn curved_triangle(
    surface: &Surface,
    t: [Uv; 3],
    depth: usize,
    points: &mut Vec<P3>,
    triangles: &mut Vec<[u32; 3]>,
) -> Result<(), String> {
    let at = |uv: Uv| surface.at(uv).ok_or("a surface not read");
    let p = [at(t[0])?, at(t[1])?, at(t[2])?];
    let mid = |a: Uv, b: Uv| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let mut worst = (0.0, 0);
    for k in 0..3 {
        let q = at(mid(t[k], t[(k + 1) % 3]))?;
        let c = math::scale(math::add(p[k], p[(k + 1) % 3]), 0.5);
        let d = dist(q, c);
        if d > worst.0 {
            worst = (d, k);
        }
    }
    if worst.0 > CHORD && depth < MAX_SPLITS {
        let k = worst.1;
        let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
        let m = mid(a, b);
        curved_triangle(surface, [a, m, c], depth + 1, points, triangles)?;
        curved_triangle(surface, [m, b, c], depth + 1, points, triangles)?;
        return Ok(());
    }
    let base = points.len() as u32;
    points.extend_from_slice(&p);
    triangles.push([base, base + 1, base + 2]);
    Ok(())
}

/// A band round a surface of revolution between two loops that each go
/// once round (in (u, v) each a line across the period): the strip
/// between them, in the loops' directions.
fn band(
    s: &Solid,
    face: usize,
    surface: &Surface,
    points: &mut Vec<P3>,
    triangles: &mut Vec<[u32; 3]>,
) -> Result<(), String> {
    let f = &s.faces[face];
    if f.loops.len() != 2 {
        return Err(format!(
            "its {} loops have no area in its parameters",
            f.loops.len()
        ));
    }
    let a = s.loop_uv(face, 0);
    let b = s.loop_uv(face, 1);
    if a.len() < 2 || b.len() < 2 {
        return Err("its loops have too few points".into());
    }
    // Both runs along increasing u: a as its loop runs, b against it.
    let rising = |l: &Vec<Uv>| l[l.len() - 1][0] >= l[0][0];
    let (mut a, mut b) = (a, b);
    let a_forward = rising(&a);
    if !a_forward {
        a.reverse();
    }
    let b_reversed = rising(&b);
    if b_reversed {
        b.reverse();
    }
    // (b now rising too: in its loop's direction it falls.)
    b.reverse();
    let at = |uv: Uv| surface.at(uv).ok_or("a surface not read".to_string());
    let base = points.len() as u32;
    for p in a.iter().chain(&b) {
        points.push(at(*p)?);
    }
    let (n, m) = (a.len(), b.len());
    let (mut i, mut j) = (0, 0);
    let mut tris = Vec::new();
    while i + 1 < n || j + 1 < m {
        let take_a = j + 1 >= m || (i + 1 < n && a[i + 1][0] <= b[j + 1][0]);
        if take_a {
            tris.push([base + i as u32, base + i as u32 + 1, base + (n + j) as u32]);
            i += 1;
        } else {
            tris.push([
                base + (n + j) as u32 + 1,
                base + (n + j) as u32,
                base + i as u32,
            ]);
            j += 1;
        }
    }
    // The loops' own directions: a's as built when it rose.
    let keep = a_forward;
    triangles.extend(tris.into_iter().map(|mut t| {
        if !keep {
            t.swap(1, 2);
        }
        t
    }));
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use mitcad_rvt::brep::{Edge, Face};

    /// A unit cube as the file keeps one: six planes, twelve edges.
    pub(crate) fn cube() -> Solid {
        // Faces: 0 bottom (z=0), 1 top (z=1), 2 y=0, 3 x=1, 4 y=1, 5 x=0;
        // each plane's (u, v) its model x and y, x and y, x and z, y and
        // z, x and z, y and z.
        let plane = |o: P3, x: P3, y: P3| Surface::Plane { origin: o, x, y };
        let faces = vec![
            plane([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            plane([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            plane([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            plane([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            plane([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            plane([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ];
        // The model point on a face's (u, v).
        let uv = |f: usize, p: P3| -> Uv {
            match f {
                0 | 1 => [p[0], p[1]],
                2 | 4 => [p[0], p[2]],
                _ => [p[1], p[2]],
            }
        };
        // Each face's loop as corners, counter-clockwise seen from
        // outside.
        let corners: [[P3; 4]; 6] = [
            [
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
            ],
            [
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            [
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
                [1.0, 0.0, 1.0],
            ],
            [
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
            ],
            [
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
        ];
        let mut edges: Vec<(P3, P3, [Option<usize>; 2])> = Vec::new();
        let mut loops: Vec<Vec<(usize, usize)>> = vec![Vec::new(); 6];
        for (f, c) in corners.iter().enumerate() {
            for k in 0..4 {
                let (a, b) = (c[k], c[(k + 1) % 4]);
                if let Some(e) = edges.iter().position(|(p, q, _)| *p == b && *q == a) {
                    edges[e].2[1] = Some(f);
                    loops[f].push((e, 1));
                } else {
                    edges.push((a, b, [Some(f), None]));
                    loops[f].push((edges.len() - 1, 0));
                }
            }
        }
        Solid {
            faces: faces
                .into_iter()
                .zip(loops)
                .map(|(surface, l)| Face {
                    surface,
                    loops: vec![l],
                    material: None,
                })
                .collect(),
            edges: edges
                .into_iter()
                .map(|(a, b, faces)| Edge {
                    faces,
                    points: [
                        vec![uv(faces[0].unwrap(), a), uv(faces[0].unwrap(), b)],
                        vec![uv(faces[1].unwrap(), a), uv(faces[1].unwrap(), b)],
                    ],
                })
                .collect(),
        }
    }

    #[test]
    fn a_cube_placed_by_a_type() {
        let mut model = Model::default();
        model.geometry.insert(
            7,
            GRep {
                solids: vec![cube()],
                ..GRep::default()
            },
        );
        // The instance places the type moved by (10, 0, 0) and mirrored in
        // x.
        model.geometry.insert(
            8,
            GRep {
                instances: vec![mitcad_rvt::brep::Placed {
                    symbol: Some(7),
                    transform: [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 10.0, 0.0, 0.0],
                    embedded: None,
                }],
                ..GRep::default()
            },
        );
        let m = element_mesh(&model, 8, &mut Cache::new(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!((m.bodies, m.faces, m.faces_left_out), (1, 6, 0));
        assert_eq!(m.triangles.len(), 12);
        assert!(m.closed);
        let v = m.volume.unwrap();
        assert!((v - FOOT_MM.powi(3)).abs() < 1e-6 * v, "{v}");
        let (lo, hi) = m.bounds().unwrap();
        assert!((lo[0] - 9.0 * FOOT_MM).abs() < 1e-9 && (hi[0] - 10.0 * FOOT_MM).abs() < 1e-9);
        // Its triangles face out: the volume they enclose is positive.
        assert!(mesh_measures(&m.points, &m.triangles).0 > 0.0);
        // A type without geometry.
        model.geometry.get_mut(&8).unwrap().instances[0].symbol = Some(99);
        assert!(
            element_mesh(&model, 8, &mut Cache::new(), &HashSet::new())
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn placed_types_that_fan_out_stop_at_the_limit() {
        // mitcad#924: an element placing type 11 a hundred times, types 11
        // to 15 each placing the next a hundred times, type 16 empty: 10^12
        // placements without the limit.
        let fan = |symbol: i64| GRep {
            instances: (0..100)
                .map(|k| mitcad_rvt::brep::Placed {
                    symbol: Some(symbol),
                    transform: [
                        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, k as f64, 0.0, 0.0,
                    ],
                    embedded: None,
                })
                .collect(),
            ..GRep::default()
        };
        let mut model = Model::default();
        model.geometry.insert(1, fan(11));
        for s in 11..16 {
            model.geometry.insert(s, fan(s + 1));
        }
        model.geometry.insert(16, GRep::default());
        let started = std::time::Instant::now();
        let why = element_mesh(&model, 1, &mut Cache::new(), &HashSet::new())
            .unwrap()
            .unwrap_err();
        assert!(
            started.elapsed().as_secs_f64() < 1.0,
            "{:?}",
            started.elapsed()
        );
        assert!(why.contains("placed types"), "{why}");
        let mut out = ElementMesh::default();
        let mut seen = HashSet::new();
        add_grep(
            &model,
            &model.geometry[&1],
            &IDENTITY,
            0,
            &mut seen,
            &mut Cache::new(),
            &mut out,
        );
        assert_eq!(out.placed, MAX_PLACED);
        assert!(out.placed_capped);
        assert_eq!(out.notes.len(), 1, "{:?}", out.notes);
    }

    #[test]
    fn a_body_s_closed_mesh() {
        // mitcad#345: a void's body as a cut tool: closed, facing out.
        let (points, triangles) = solid_mesh(&cube()).unwrap();
        assert_eq!(triangles.len(), 12);
        assert!(mesh_measures(&points, &triangles).0 > 0.0);
        let mut open = cube();
        open.faces.pop();
        assert!(solid_mesh(&open).is_err());
    }

    #[test]
    fn a_cylinder_is_split_until_it_fits() {
        // A quarter cylinder face of radius 1, height 1.
        let surface = Surface::Cylinder {
            center: [0.0; 3],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            z: [0.0, 0.0, 1.0],
            radius: 1.0,
        };
        let mut points = Vec::new();
        let mut triangles = Vec::new();
        let q = std::f64::consts::FRAC_PI_2;
        curved_triangle(
            &surface,
            [[0.0, 0.0], [q, 0.0], [q, 1.0]],
            0,
            &mut points,
            &mut triangles,
        )
        .unwrap();
        assert!(triangles.len() > 4);
        for p in &points {
            assert!((p[0].hypot(p[1]) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_plane_s_separate_outer_loops_and_its_holes() {
        let square = |x: f64, y: f64, s: f64| -> Vec<P3> {
            vec![
                [x, y, 0.0],
                [x + s, y, 0.0],
                [x + s, y + s, 0.0],
                [x, y + s, 0.0],
            ]
        };
        let mut hole = square(1.0, 1.0, 1.0);
        hole.reverse();
        // A sliver of no area, as the file can keep one.
        let sliver = vec![[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [10.0, 0.0, 0.0]];
        let groups = nested(&[
            square(0.0, 0.0, 4.0),
            square(10.0, 0.0, 2.0),
            hole.clone(),
            sliver,
        ]);
        assert_eq!(groups.len(), 2);
        let big = groups.iter().find(|g| g[0][0] == [0.0, 0.0, 0.0]).unwrap();
        assert_eq!(big.len(), 2);
        assert_eq!(big[1], hole);
        let small = groups.iter().find(|g| g[0][0] == [10.0, 0.0, 0.0]).unwrap();
        assert_eq!(small.len(), 1);
        let (mut points, mut triangles) = (Vec::new(), Vec::new());
        for g in &groups {
            assert!(polygon(g, &mut points, &mut triangles) > 0);
        }
        // 16 - 1 + 4 square feet.
        let area: f64 = triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| points[i as usize]);
                math::length(math::cross(math::sub(b, a), math::sub(c, a))) / 2.0
            })
            .sum();
        assert!((area - 19.0).abs() < 1e-9, "{area}");
    }

    #[test]
    fn faces_that_bound_nothing_are_left_out() {
        // A face left over without loops, and a sheet of one plane (a
        // wall's layer boundary): the body still meshes closed.
        let mut body = cube();
        body.faces.push(Face {
            surface: Surface::Plane {
                origin: [0.0; 3],
                x: [1.0, 0.0, 0.0],
                y: [0.0, 0.0, -1.0],
            },
            loops: Vec::new(),
            material: None,
        });
        let (points, triangles) = solid_mesh(&body).unwrap();
        assert!(mesh_measures(&points, &triangles).0 > 0.0);
        let mut sheet = cube();
        sheet.faces.truncate(1);
        let mut model = Model::default();
        model.geometry.insert(
            3,
            GRep {
                solids: vec![body, sheet],
                ..GRep::default()
            },
        );
        let m = element_mesh(&model, 3, &mut Cache::new(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!((m.bodies, m.faces, m.faces_left_out), (1, 6, 0));
        assert!(m.closed);
    }

    #[test]
    fn a_body_of_planes_as_its_faces_facing_out() {
        let faces = solid_faces(&cube()).unwrap().unwrap();
        assert_eq!(faces.len(), 6);
        assert!(faces.iter().all(|f| f.len() == 1 && f[0].len() == 4));
        let volume: f64 = faces
            .iter()
            .flatten()
            .map(|l| {
                (1..l.len() - 1)
                    .map(|k| math::dot(l[0], math::cross(l[k], l[k + 1])))
                    .sum::<f64>()
                    / 6.0
            })
            .sum();
        assert!((volume - 1.0).abs() < 1e-9, "{volume}");
    }
}
