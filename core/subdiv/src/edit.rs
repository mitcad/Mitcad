// SPDX-License-Identifier: MIT
//! Edits of a cage (mitcad#1757): moves, rotations and scalings of
//! vertices, edges and faces, positions set directly (a drag), extrusion of
//! faces, insertion and removal of edge loops, removal of faces, openings
//! filled and bridged (mitcad#1941), creases, corners and the symmetry
//! plane.
//!
//! Elements are named by their ids in the whole cage ([`Cage::full`]): a
//! negative id is the mirror image of the stored element with that id, an
//! edge is its two vertices. On a symmetric cage every edit keeps the
//! halves equal: a move of a mirrored vertex moves the stored one by the
//! mirror image of the motion, and a vertex on the symmetry plane stays on
//! it (it moves only along the plane). The topological edits (extrusion,
//! edge loops) are made on the whole cage, mirror image and all, and the
//! stored half is cut from the result again; a face that would cross the
//! plane refuses the edit.
//!
//! An edit that is refused may have changed the cage part way: edit a copy
//! and keep it only when the edit succeeds (the document does). Each
//! successful edit leaves a cage that [`Cage::check`] accepts.

use std::collections::{BTreeMap, BTreeSet};

use mitcad_vec3::{Vec3, add, cross, dot, scale, sub, unit};

use crate::cage::{Cage, CageFace, CageVertex, Crease, ON_PLANE, SymmetryPlane, edge_key};

/// Vertices, edges and faces by their ids in the whole cage.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selection {
    pub vertices: Vec<i64>,
    pub edges: Vec<[i64; 2]>,
    pub faces: Vec<i64>,
}

/// A motion of selected vertices, in the coordinates of the whole cage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    Translate(Vec3),
    /// About the axis through `origin` (none: the selection's centre)
    /// along `axis`, by `angle` radians, right-handed.
    Rotate {
        origin: Option<Vec3>,
        axis: Vec3,
        angle: f64,
    },
    /// By a factor along each axis about `center` (none: the selection's
    /// centre).
    Scale {
        center: Option<Vec3>,
        factors: Vec3,
    },
}

/// What an edit made and took out, by stored ids.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Made {
    pub vertices: Vec<u32>,
    pub faces: Vec<u32>,
    pub edges: Vec<[u32; 2]>,
    pub removed_vertices: Vec<u32>,
    pub removed_faces: Vec<u32>,
}

impl Made {
    fn extend(&mut self, other: Made) {
        self.vertices.extend(other.vertices);
        self.faces.extend(other.faces);
        self.edges.extend(other.edges);
        self.removed_vertices.extend(other.removed_vertices);
        self.removed_faces.extend(other.removed_faces);
    }

    /// Only what the (stored half of the) cage has; removed elements only
    /// when the cage had them before (`before`).
    fn kept_in(mut self, cage: &Cage, before: &Cage) -> Made {
        let edges = cage.edges();
        self.vertices.retain(|v| cage.vertex(*v).is_some());
        self.faces.retain(|f| cage.face(*f).is_some());
        self.edges
            .retain(|e| edges.contains_key(&edge_key(e[0], e[1])));
        self.removed_vertices
            .retain(|v| before.vertex(*v).is_some() && cage.vertex(*v).is_none());
        self.removed_faces
            .retain(|f| before.face(*f).is_some() && cage.face(*f).is_none());
        self
    }
}

/// A stored id and whether the reference names its mirror image.
fn stored(id: i64, symmetric: bool, what: &str) -> Result<(u32, bool), String> {
    let mirrored = id < 0;
    if mirrored && !symmetric {
        return Err("the cage has no mirrored half: it has no symmetry plane".to_owned());
    }
    match u32::try_from(id.unsigned_abs()) {
        Ok(stored) if stored > 0 => Ok((stored, mirrored)),
        _ => Err(format!("a selected {what} is not in the cage")),
    }
}

fn vertex_ref(cage: &Cage, id: i64) -> Result<(u32, bool), String> {
    let (v, mirrored) = stored(id, cage.symmetry.is_some(), "vertex")?;
    let at = cage
        .position(v)
        .ok_or_else(|| "a selected vertex is not in the cage".to_owned())?;
    // The mirror image of a vertex on the plane is the vertex itself.
    let on_plane = cage.symmetry.is_some_and(|plane| plane.holds(at));
    Ok((v, mirrored && !on_plane))
}

fn face_ref(cage: &Cage, id: i64) -> Result<(u32, bool), String> {
    let (f, mirrored) = stored(id, cage.symmetry.is_some(), "face")?;
    if cage.face(f).is_none() {
        return Err("a selected face is not in the cage".to_owned());
    }
    Ok((f, mirrored))
}

/// An edge's stored ends and whether it is the mirror image (an edge with
/// a mirrored end, or two ends on the plane, which is its own).
fn edge_ref(cage: &Cage, edge: [i64; 2]) -> Result<([u32; 2], bool), String> {
    let (a, ma) = vertex_ref(cage, edge[0])?;
    let (b, mb) = vertex_ref(cage, edge[1])?;
    if !cage.edges().contains_key(&edge_key(a, b)) {
        return Err("a selected edge is not an edge of the cage".to_owned());
    }
    Ok(([a, b], ma || mb))
}

/// The stored vertices a selection moves, each with whether it moves as
/// the mirror image (a vertex picked both ways moves as itself).
fn selected_vertices(cage: &Cage, selection: &Selection) -> Result<BTreeMap<u32, bool>, String> {
    let mut picked: BTreeMap<u32, bool> = BTreeMap::new();
    let mut pick = |v: u32, mirrored: bool| {
        let entry = picked.entry(v).or_insert(mirrored);
        *entry = *entry && mirrored;
    };
    for id in &selection.vertices {
        let (v, mirrored) = vertex_ref(cage, *id)?;
        pick(v, mirrored);
    }
    for edge in &selection.edges {
        let ([a, b], mirrored) = edge_ref(cage, *edge)?;
        for v in [a, b] {
            let on_plane = cage
                .symmetry
                .is_some_and(|plane| cage.position(v).is_some_and(|p| plane.holds(p)));
            pick(v, mirrored && !on_plane);
        }
    }
    for id in &selection.faces {
        let (f, mirrored) = face_ref(cage, *id)?;
        let face = cage.face(f).expect("a face of the cage").clone();
        for v in face.vertices {
            let on_plane = cage
                .symmetry
                .is_some_and(|plane| cage.position(v).is_some_and(|p| plane.holds(p)));
            pick(v, mirrored && !on_plane);
        }
    }
    Ok(picked)
}

fn finite3(v: Vec3) -> bool {
    v.iter().all(|c| c.is_finite())
}

/// Checks a motion's values.
fn check_motion(motion: &Motion) -> Result<(), String> {
    match motion {
        Motion::Translate(by) if !finite3(*by) => Err("the move must be finite".to_owned()),
        Motion::Rotate {
            origin,
            axis,
            angle,
        } => {
            if unit(*axis).is_none() {
                return Err("the rotation's axis has no direction".to_owned());
            }
            if !angle.is_finite() || origin.is_some_and(|o| !finite3(o)) {
                return Err("the rotation must be finite".to_owned());
            }
            Ok(())
        }
        Motion::Scale { center, factors } => {
            if !finite3(*factors) || factors.contains(&0.0) || center.is_some_and(|c| !finite3(c)) {
                return Err("a scale factor must be finite and not zero".to_owned());
            }
            Ok(())
        }
        Motion::Translate(_) => Ok(()),
    }
}

/// The point moved by the motion; `middle` is the selection's centre.
fn apply_motion(motion: &Motion, middle: Vec3, p: Vec3) -> Vec3 {
    match *motion {
        Motion::Translate(by) => add(p, by),
        Motion::Rotate {
            origin,
            axis,
            angle,
        } => {
            let o = origin.unwrap_or(middle);
            let k = unit(axis).unwrap_or([0.0, 0.0, 1.0]);
            let v = sub(p, o);
            let (sin, cos) = angle.sin_cos();
            let turned = add(
                add(scale(v, cos), scale(cross(k, v), sin)),
                scale(k, dot(k, v) * (1.0 - cos)),
            );
            add(o, turned)
        }
        Motion::Scale { center, factors } => {
            let c = center.unwrap_or(middle);
            std::array::from_fn(|i| c[i] + (p[i] - c[i]) * factors[i])
        }
    }
}

fn mean(points: &[Vec3]) -> Vec3 {
    let sum = points.iter().fold([0.0; 3], |s, p| add(s, *p));
    scale(sum, 1.0 / points.len().max(1) as f64)
}

/// Moves, turns or scales the selected vertices, edges and faces (see
/// [`Motion`]); returns the stored vertices that moved.
pub fn transform(
    cage: &mut Cage,
    selection: &Selection,
    motion: Motion,
) -> Result<Vec<u32>, String> {
    cage.check()?;
    check_motion(&motion)?;
    let picked = selected_vertices(cage, selection)?;
    if picked.is_empty() {
        return Err("nothing is selected".to_owned());
    }
    let plane = cage.symmetry;
    let shown = |p: Vec3, mirrored: bool| match (&plane, mirrored) {
        (Some(plane), true) => plane.mirror(p),
        _ => p,
    };
    let at: Vec<Vec3> = picked
        .iter()
        .map(|(v, mirrored)| shown(cage.position(*v).expect("a vertex of the cage"), *mirrored))
        .collect();
    let middle = mean(&at);
    for ((v, mirrored), p) in picked.iter().zip(&at) {
        let before = cage.position(*v).expect("a vertex of the cage");
        let mut moved = shown(apply_motion(&motion, middle, *p), *mirrored);
        if let Some(plane) = &plane
            && plane.holds(before)
        {
            moved = plane.project(moved);
        }
        cage.vertex_mut(*v).expect("a vertex of the cage").at = moved;
    }
    cage.check()?;
    Ok(picked.keys().copied().collect())
}

/// Puts vertices where they are dragged (positions in the whole cage); a
/// vertex on the symmetry plane stays on it.
pub fn set_positions(cage: &mut Cage, positions: &[(i64, Vec3)]) -> Result<(), String> {
    cage.check()?;
    if positions.is_empty() {
        return Err("no vertices are given".to_owned());
    }
    let plane = cage.symmetry;
    for (id, at) in positions {
        if !finite3(*at) {
            return Err("a vertex's position must be finite".to_owned());
        }
        let (v, mirrored) = vertex_ref(cage, *id)?;
        let before = cage.position(v).expect("a vertex of the cage");
        let mut moved = match (&plane, mirrored) {
            (Some(plane), true) => plane.mirror(*at),
            _ => *at,
        };
        if let Some(plane) = &plane
            && plane.holds(before)
        {
            moved = plane.project(moved);
        }
        cage.vertex_mut(v).expect("a vertex of the cage").at = moved;
    }
    cage.check()
}

/// A symmetric cage made whole: the mirrored half added with ids of its
/// own, and the maps from a stored element to its mirror image (a vertex on
/// the plane to itself) and back.
struct Expanded {
    cage: Cage,
    plane: SymmetryPlane,
    mirror_vertex: BTreeMap<u32, u32>,
    mirror_face: BTreeMap<u32, u32>,
}

impl Expanded {
    fn of(cage: &Cage) -> Option<Expanded> {
        let plane = cage.symmetry?;
        let mut whole = cage.clone();
        whole.symmetry = None;
        let mut next_vertex = cage.next_vertex_id();
        let mut mirror_vertex = BTreeMap::new();
        for v in &cage.vertices {
            if plane.holds(v.at) {
                mirror_vertex.insert(v.id, v.id);
                if let Some(on) = whole.vertex_mut(v.id) {
                    on.at = plane.project(v.at);
                }
            } else {
                whole.vertices.push(CageVertex {
                    id: next_vertex,
                    at: plane.mirror(v.at),
                });
                mirror_vertex.insert(v.id, next_vertex);
                mirror_vertex.insert(next_vertex, v.id);
                next_vertex += 1;
            }
        }
        let mut mirror_face = BTreeMap::new();
        for (id, face) in (cage.next_face_id()..).zip(&cage.faces) {
            let n = face.vertices.len();
            whole.faces.push(CageFace {
                id,
                vertices: (0..n)
                    .map(|p| mirror_vertex[&face.vertices[(n - p) % n]])
                    .collect(),
            });
            mirror_face.insert(face.id, id);
            mirror_face.insert(id, face.id);
        }
        for crease in &cage.creases {
            let [a, b] = crease.edge;
            let (ma, mb) = (mirror_vertex[&a], mirror_vertex[&b]);
            if edge_key(ma, mb) != edge_key(a, b) {
                whole.creases.push(Crease {
                    edge: [ma, mb],
                    sharpness: crease.sharpness,
                });
            }
        }
        for corner in &cage.corners {
            let m = mirror_vertex[corner];
            if m != *corner {
                whole.corners.push(m);
            }
        }
        Some(Expanded {
            cage: whole,
            plane,
            mirror_vertex,
            mirror_face,
        })
    }

    /// A vertex of the whole cage by its id in the whole cage.
    fn vertex(&self, stored_cage: &Cage, id: i64) -> Result<u32, String> {
        let (v, mirrored) = vertex_ref(stored_cage, id)?;
        Ok(if mirrored { self.mirror_vertex[&v] } else { v })
    }

    /// The half cut from the whole cage again.
    fn fold(&self) -> Result<Cage, String> {
        cut(&self.cage, self.plane)
    }
}

/// The half of a cage on the side of the plane its normal points to, with
/// the plane as its symmetry plane: the faces there are kept, the others
/// left out; vertices on the plane are put on it exactly. Err for a face
/// across the plane.
fn cut(whole: &Cage, plane: SymmetryPlane) -> Result<Cage, String> {
    let mut half = whole.clone();
    half.symmetry = Some(plane);
    let mut kept = Vec::new();
    for face in &whole.faces {
        let d: Vec<f64> = face
            .vertices
            .iter()
            .map(|v| whole.position(*v).map_or(0.0, |p| plane.distance(p)))
            .collect();
        let above = d.iter().any(|x| *x > ON_PLANE);
        let below = d.iter().any(|x| *x < -ON_PLANE);
        if above && below {
            return Err("a face of the cage crosses the symmetry plane: \
                        insert an edge loop on the plane first"
                .to_owned());
        }
        if above {
            kept.push(face.clone());
        }
    }
    if kept.is_empty() {
        return Err(
            "no face of the cage lies on the side of the symmetry plane its normal points to"
                .to_owned(),
        );
    }
    half.faces = kept;
    for v in &mut half.vertices {
        if plane.holds(v.at) {
            v.at = plane.project(v.at);
        }
    }
    half.remove_orphans();
    Ok(half)
}

/// Extrudes faces by `distance` along their normals (each vertex along the
/// mean of its extruded faces' normals): the faces move, keeping their ids,
/// and a new side face stands on each edge round the selection. Returns
/// the new vertices and side faces.
pub fn extrude(cage: &mut Cage, faces: &[i64], distance: f64) -> Result<Made, String> {
    cage.check()?;
    cage.note_ids();
    if faces.is_empty() {
        return Err("no faces are selected".to_owned());
    }
    let before = cage.clone();
    let made = match Expanded::of(cage) {
        None => {
            let region = faces
                .iter()
                .map(|f| face_ref(cage, *f).map(|(f, _)| f))
                .collect::<Result<BTreeSet<u32>, String>>()?;
            extrude_plain(cage, &region, distance)?
        }
        Some(mut whole) => {
            let mut region = BTreeSet::new();
            for f in faces {
                let (f, _) = face_ref(cage, *f)?;
                region.insert(f);
                region.insert(whole.mirror_face[&f]);
            }
            let made = extrude_plain(&mut whole.cage, &region, distance)?;
            *cage = whole.fold()?;
            made
        }
    };
    cage.note_ids();
    cage.check()?;
    Ok(made.kept_in(cage, &before))
}

fn extrude_plain(cage: &mut Cage, region: &BTreeSet<u32>, distance: f64) -> Result<Made, String> {
    if !distance.is_finite() || distance == 0.0 {
        return Err("the extrusion's distance must not be zero".to_owned());
    }
    let mut count: BTreeMap<[u32; 2], usize> = BTreeMap::new();
    let mut directed: Vec<(u32, u32)> = Vec::new();
    let mut normals: BTreeMap<u32, Vec3> = BTreeMap::new();
    for face in cage.faces.iter().filter(|f| region.contains(&f.id)) {
        let (normal, _) = cage.face_normal(face);
        let n = face.vertices.len();
        for (k, &x) in face.vertices.iter().enumerate() {
            let y = face.vertices[(k + 1) % n];
            *count.entry(edge_key(x, y)).or_insert(0) += 1;
            directed.push((x, y));
            let sum = normals.entry(x).or_insert([0.0; 3]);
            *sum = add(*sum, normal);
        }
    }
    if normals.is_empty() {
        return Err("no faces are selected".to_owned());
    }
    // The edges round the selection, as the selected faces run along them.
    let boundary: Vec<(u32, u32)> = directed
        .into_iter()
        .filter(|(x, y)| count[&edge_key(*x, *y)] == 1)
        .collect();
    // Vertices that other faces keep get a copy; the others just move.
    let mut copied: BTreeSet<u32> = boundary.iter().flat_map(|(x, y)| [*x, *y]).collect();
    for face in cage.faces.iter().filter(|f| !region.contains(&f.id)) {
        copied.extend(face.vertices.iter().filter(|v| normals.contains_key(v)));
    }
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    let mut made = Made::default();
    let mut next_vertex = cage.next_vertex_id();
    for (v, sum) in &normals {
        let direction = unit(*sum).ok_or("the faces to extrude face opposite ways at a vertex")?;
        let at = add(
            cage.position(*v).expect("a vertex of the faces"),
            scale(direction, distance),
        );
        if copied.contains(v) {
            cage.vertices.push(CageVertex {
                id: next_vertex,
                at,
            });
            if cage.is_corner(*v) {
                cage.corners.push(next_vertex);
            }
            map.insert(*v, next_vertex);
            made.vertices.push(next_vertex);
            next_vertex += 1;
        } else {
            cage.vertex_mut(*v).expect("a vertex of the faces").at = at;
            map.insert(*v, *v);
        }
    }
    // Creases of the faces' edges go with them; those round the selection
    // stay where they were too.
    let mut creases = Vec::new();
    for crease in &cage.creases {
        let [a, b] = crease.edge;
        match count.get(&edge_key(a, b)) {
            Some(n) => {
                if *n == 1 {
                    creases.push(*crease);
                }
                creases.push(Crease {
                    edge: [map[&a], map[&b]],
                    sharpness: crease.sharpness,
                });
            }
            None => creases.push(*crease),
        }
    }
    cage.creases = creases;
    for face in cage.faces.iter_mut().filter(|f| region.contains(&f.id)) {
        for v in &mut face.vertices {
            *v = map[&*v];
        }
    }
    for (id, (x, y)) in (cage.next_face_id()..).zip(boundary) {
        cage.faces.push(CageFace {
            id,
            vertices: vec![x, y, map[&y], map[&x]],
        });
        made.faces.push(id);
    }
    cage.corners.sort_unstable();
    cage.corners.dedup();
    Ok(made)
}

/// Inserts an edge loop through the edge `edge` at `at` along it (0 at its
/// first vertex, 1 at its second): across the quads on either side, from
/// each to the one beyond the opposite edge, until the loop closes, meets
/// the cage's boundary or a face that is not a quad (which takes the new
/// vertex on its edge). Each quad crossed is split in two: one half keeps
/// its id, the other gets a new one. Returns the new vertices, the new
/// faces and the edges across the quads.
pub fn insert_loop(cage: &mut Cage, edge: [i64; 2], at: f64) -> Result<Made, String> {
    cage.check()?;
    cage.note_ids();
    if !(at > 0.0 && at < 1.0) {
        return Err(
            "an edge loop goes between the ends of the edge: at must be between 0 and 1".to_owned(),
        );
    }
    let before = cage.clone();
    let made = match Expanded::of(cage) {
        None => {
            let ([a, b], _) = edge_ref(cage, edge)?;
            insert_loop_plain(cage, a, b, at)?
        }
        Some(mut whole) => {
            edge_ref(cage, edge)?;
            let a = whole.vertex(cage, edge[0])?;
            let b = whole.vertex(cage, edge[1])?;
            let mut made = insert_loop_plain(&mut whole.cage, a, b, at)?;
            // The mirror image, unless the loop went through it already.
            let (ma, mb) = (whole.mirror_vertex[&a], whole.mirror_vertex[&b]);
            if whole.cage.edges().contains_key(&edge_key(ma, mb)) {
                made.extend(insert_loop_plain(&mut whole.cage, ma, mb, at)?);
            }
            *cage = whole.fold()?;
            made
        }
    };
    cage.note_ids();
    cage.check()?;
    Ok(made.kept_in(cage, &before))
}

fn insert_loop_plain(cage: &mut Cage, a: u32, b: u32, t: f64) -> Result<Made, String> {
    let edges = cage.edges();
    let start = edge_key(a, b);
    let first = edges
        .get(&start)
        .cloned()
        .ok_or("the selected edge is not an edge of the cage")?;
    let index: BTreeMap<u32, usize> = cage
        .faces
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id, i))
        .collect();
    // The edges the loop crosses, each as the walk meets it (its end on the
    // side of the first edge's first vertex first).
    let mut split: BTreeMap<[u32; 2], (u32, u32)> = BTreeMap::new();
    split.insert(start, (a, b));
    let mut quads: Vec<usize> = Vec::new();
    let mut closed = false;
    for &first_face in &first {
        if closed {
            break;
        }
        let (mut u, mut v) = (a, b);
        let mut current = first_face;
        loop {
            let fi = index[&current];
            let lp = &cage.faces[fi].vertices;
            if lp.len() != 4 || quads.contains(&fi) {
                break;
            }
            let Some(p) = (0..4).find(|&p| edge_key(lp[p], lp[(p + 1) % 4]) == edge_key(u, v))
            else {
                break;
            };
            quads.push(fi);
            let (p0, p2, p3) = (lp[p], lp[(p + 2) % 4], lp[(p + 3) % 4]);
            // The opposite edge, its end across from u first.
            let (eu, ev) = if p0 == u { (p3, p2) } else { (p2, p3) };
            let exit = edge_key(eu, ev);
            if exit == start {
                closed = true;
                break;
            }
            if split.contains_key(&exit) {
                break;
            }
            split.insert(exit, (eu, ev));
            match edges[&exit].iter().find(|g| **g != current) {
                Some(&g) => {
                    current = g;
                    u = eu;
                    v = ev;
                }
                None => break,
            }
        }
    }
    let mut made = Made::default();
    let mut new_of: BTreeMap<[u32; 2], u32> = BTreeMap::new();
    for (id, (edge, (u, v))) in (cage.next_vertex_id()..).zip(&split) {
        let (pu, pv) = (
            cage.position(*u).expect("a vertex of the cage"),
            cage.position(*v).expect("a vertex of the cage"),
        );
        cage.vertices.push(CageVertex {
            id,
            at: add(pu, scale(sub(pv, pu), t)),
        });
        new_of.insert(*edge, id);
        made.vertices.push(id);
        if let Some(s) = cage.crease(*u, *v) {
            let sharpness = if s.is_infinite() { None } else { Some(s) };
            cage.set_crease(*u, *v, Some(0.0));
            cage.set_crease(*u, id, sharpness);
            cage.set_crease(id, *v, sharpness);
        }
    }
    // Each quad crossed, turned so that the loop enters it between its
    // corners 0 and 1 and leaves between 3 and 2.
    let crossed: Vec<(usize, Vec<u32>)> = quads
        .iter()
        .filter_map(|&fi| {
            let lp = &cage.faces[fi].vertices;
            let p = (0..2).find(|&p| {
                new_of.contains_key(&edge_key(lp[p], lp[p + 1]))
                    && new_of.contains_key(&edge_key(lp[p + 2], lp[(p + 3) % 4]))
            })?;
            Some((fi, (0..4).map(|k| lp[(p + k) % 4]).collect()))
        })
        .collect();
    let mut added = Vec::new();
    for (id, (fi, lp)) in (cage.next_face_id()..).zip(crossed) {
        let (p0, p1, p2, p3) = (lp[0], lp[1], lp[2], lp[3]);
        let na = new_of[&edge_key(p0, p1)];
        let nb = new_of[&edge_key(p3, p2)];
        cage.faces[fi].vertices = vec![p0, na, nb, p3];
        added.push(CageFace {
            id,
            vertices: vec![na, p1, p2, nb],
        });
        made.faces.push(id);
        made.edges.push(edge_key(na, nb));
    }
    cage.faces.extend(added);
    // Other faces on a split edge (a face that is not a quad where the
    // loop ends) take the new vertex.
    for face in &mut cage.faces {
        let mut k = 0;
        while k < face.vertices.len() {
            let n = face.vertices.len();
            let (x, y) = (face.vertices[k], face.vertices[(k + 1) % n]);
            match new_of.get(&edge_key(x, y)) {
                Some(&new) => {
                    face.vertices.insert(k + 1, new);
                    k += 2;
                }
                None => k += 1,
            }
        }
    }
    Ok(made)
}

/// The loop of a face starting at `start`.
fn rotated(lp: &[u32], start: u32) -> Vec<u32> {
    let at = lp.iter().position(|v| *v == start).unwrap_or(0);
    lp[at..].iter().chain(&lp[..at]).copied().collect()
}

/// Whether the loop runs from `x` to `y`.
fn runs(lp: &[u32], x: u32, y: u32) -> bool {
    let n = lp.len();
    (0..n).any(|k| lp[k] == x && lp[(k + 1) % n] == y)
}

/// Removes the edge loop through the edge `edge`: from each end on along
/// the edge across from it at a vertex of four quads, until the loop closes
/// or meets another vertex. The faces either side of each of its edges
/// become one, and its vertices left between two edges go. Returns the
/// faces and vertices taken out.
pub fn delete_loop(cage: &mut Cage, edge: [i64; 2]) -> Result<Made, String> {
    cage.check()?;
    cage.note_ids();
    let before = cage.clone();
    let made = match Expanded::of(cage) {
        None => {
            let ([a, b], _) = edge_ref(cage, edge)?;
            delete_loop_plain(cage, a, b)?
        }
        Some(mut whole) => {
            edge_ref(cage, edge)?;
            let a = whole.vertex(cage, edge[0])?;
            let b = whole.vertex(cage, edge[1])?;
            let mut made = delete_loop_plain(&mut whole.cage, a, b)?;
            let (ma, mb) = (whole.mirror_vertex[&a], whole.mirror_vertex[&b]);
            if whole.cage.edges().contains_key(&edge_key(ma, mb)) {
                made.extend(delete_loop_plain(&mut whole.cage, ma, mb)?);
            }
            *cage = whole.fold()?;
            made
        }
    };
    cage.note_ids();
    cage.check()?;
    Ok(made.kept_in(cage, &before))
}

fn delete_loop_plain(cage: &mut Cage, a: u32, b: u32) -> Result<Made, String> {
    let refused = || "the edge loop cannot be removed here".to_owned();
    let edges = cage.edges();
    if !edges.contains_key(&edge_key(a, b)) {
        return Err("the selected edge is not an edge of the cage".to_owned());
    }
    let mut around: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for [x, y] in edges.keys() {
        around.entry(*x).or_default().insert(*y);
        around.entry(*y).or_default().insert(*x);
    }
    let faces_of =
        |x: u32, y: u32| -> Vec<u32> { edges.get(&edge_key(x, y)).cloned().unwrap_or_default() };
    let quad: BTreeMap<u32, bool> = cage
        .faces
        .iter()
        .map(|f| (f.id, f.vertices.len() == 4))
        .collect();
    // Where the loop goes on at `v`, having come from `prev`: the edge that
    // shares no face with the one it came along, at a vertex of four edges
    // between quads.
    let onward = |prev: u32, v: u32| -> Option<u32> {
        let neighbours = around.get(&v)?;
        let came = faces_of(prev, v);
        if neighbours.len() != 4 || came.len() != 2 {
            return None;
        }
        let mut candidates = neighbours.iter().filter(|w| {
            let faces = faces_of(v, **w);
            **w != prev
                && faces.len() == 2
                && faces.iter().all(|f| quad[f])
                && !faces.iter().any(|f| came.contains(f))
        });
        let w = *candidates.next()?;
        candidates.next().is_none().then_some(w)
    };
    let mut chain = vec![a, b];
    let mut closed = false;
    loop {
        let (prev, v) = (chain[chain.len() - 2], chain[chain.len() - 1]);
        match onward(prev, v) {
            Some(w) if w == chain[0] => {
                closed = true;
                break;
            }
            Some(w) if !chain.contains(&w) => chain.push(w),
            _ => break,
        }
    }
    if !closed {
        let mut back = vec![b, a];
        loop {
            let (prev, v) = (back[back.len() - 2], back[back.len() - 1]);
            match onward(prev, v) {
                Some(w) if !back.contains(&w) && !chain.contains(&w) => back.push(w),
                _ => break,
            }
        }
        let mut whole: Vec<u32> = back[2..].iter().rev().copied().collect();
        whole.extend(&chain);
        chain = whole;
    }
    let mut loop_edges: Vec<(u32, u32)> = chain.windows(2).map(|w| (w[0], w[1])).collect();
    if closed {
        loop_edges.push((chain[chain.len() - 1], chain[0]));
    }
    if loop_edges.iter().any(|(x, y)| faces_of(*x, *y).len() != 2) {
        return Err("an edge loop along the cage's boundary cannot be removed".to_owned());
    }
    let mut made = Made::default();
    // The faces either side of each edge become one, which keeps the
    // smaller id.
    for (x, y) in &loop_edges {
        let holders: Vec<usize> = cage
            .faces
            .iter()
            .enumerate()
            .filter(|(_, f)| runs(&f.vertices, *x, *y) || runs(&f.vertices, *y, *x))
            .map(|(i, _)| i)
            .collect();
        let &[i, j] = holders.as_slice() else {
            return Err(refused());
        };
        let (along, against) = if runs(&cage.faces[i].vertices, *x, *y) {
            (i, j)
        } else {
            (j, i)
        };
        let f = rotated(&cage.faces[along].vertices, *y);
        let g = rotated(&cage.faces[against].vertices, *x);
        let mut merged = f;
        merged.extend(&g[1..g.len() - 1]);
        let distinct: BTreeSet<u32> = merged.iter().copied().collect();
        if distinct.len() != merged.len() {
            return Err(refused());
        }
        let (keep, drop) = if cage.faces[i].id < cage.faces[j].id {
            (i, j)
        } else {
            (j, i)
        };
        cage.faces[keep].vertices = merged;
        made.removed_faces.push(cage.faces[drop].id);
        cage.faces.remove(drop);
    }
    // The loop's vertices left between two edges go, their two edges
    // becoming one (creased when either was).
    for v in &chain {
        let edges = cage.edges();
        let neighbours: Vec<u32> = edges
            .keys()
            .filter(|[x, y]| x == v || y == v)
            .map(|[x, y]| if x == v { *y } else { *x })
            .collect();
        if neighbours.len() != 2 {
            continue;
        }
        let sharpness = [
            cage.crease(neighbours[0], *v),
            cage.crease(*v, neighbours[1]),
        ]
        .into_iter()
        .flatten()
        .fold(None, |s: Option<f64>, x| Some(s.map_or(x, |s| s.max(x))));
        for face in &mut cage.faces {
            face.vertices.retain(|x| x != v);
            if face.vertices.len() < 3 {
                return Err(
                    "removing the edge loop would leave a face with fewer than three corners"
                        .to_owned(),
                );
            }
        }
        if let Some(s) = sharpness {
            cage.set_crease(
                neighbours[0],
                neighbours[1],
                if s.is_infinite() { None } else { Some(s) },
            );
        }
        made.removed_vertices.push(*v);
    }
    cage.remove_orphans();
    Ok(made)
}

/// Takes faces out of the cage (it opens there), with the vertices no
/// other face uses.
pub fn delete_faces(cage: &mut Cage, faces: &[i64]) -> Result<Made, String> {
    cage.check()?;
    cage.note_ids();
    if faces.is_empty() {
        return Err("no faces are selected".to_owned());
    }
    let before = cage.clone();
    let gone = faces
        .iter()
        .map(|f| face_ref(cage, *f).map(|(f, _)| f))
        .collect::<Result<BTreeSet<u32>, String>>()?;
    cage.faces.retain(|f| !gone.contains(&f.id));
    if cage.faces.is_empty() {
        return Err("the cage would have no faces left".to_owned());
    }
    cage.remove_orphans();
    cage.check()?;
    Ok(Made {
        removed_vertices: before
            .vertices
            .iter()
            .filter(|v| cage.vertex(v.id).is_none())
            .map(|v| v.id)
            .collect(),
        removed_faces: gone.into_iter().collect(),
        ..Made::default()
    })
}

/// A run along the cage's boundary: its vertices in the order the faces
/// beside it run along it, closed when it goes round an opening.
#[derive(Debug, Clone, PartialEq)]
struct Run {
    vertices: Vec<u32>,
    closed: bool,
}

impl Run {
    /// Its number of edges.
    fn edges(&self) -> usize {
        if self.closed {
            self.vertices.len()
        } else {
            self.vertices.len() - 1
        }
    }
}

/// The cage's boundary edges by their keys, each as its one face runs
/// along it (`x -> y`).
fn boundary_edges(cage: &Cage) -> BTreeMap<[u32; 2], (u32, u32)> {
    let edges = cage.edges();
    let mut boundary = BTreeMap::new();
    for face in &cage.faces {
        let n = face.vertices.len();
        for k in 0..n {
            let (x, y) = (face.vertices[k], face.vertices[(k + 1) % n]);
            let edge = edge_key(x, y);
            if edges.get(&edge).is_some_and(|faces| faces.len() == 1) {
                boundary.insert(edge, (x, y));
            }
        }
    }
    boundary
}

/// Checks that `a`, `b` is an edge of the cage on its boundary.
fn on_boundary(cage: &Cage, a: u32, b: u32) -> Result<(), String> {
    match cage.edges().get(&edge_key(a, b)) {
        None => Err("a picked edge is not an edge of the cage".to_owned()),
        Some(faces) if faces.len() != 1 => Err("a picked edge is not on an opening of the \
                                                cage: it has a face on both sides"
            .to_owned()),
        Some(_) => Ok(()),
    }
}

/// The opening the boundary edge `a`, `b` lies on, as a closed run.
fn opening_of(cage: &Cage, a: u32, b: u32) -> Result<Run, String> {
    on_boundary(cage, a, b)?;
    let boundary = boundary_edges(cage);
    let (x, y) = boundary[&edge_key(a, b)];
    let mut from: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (p, q) in boundary.values() {
        from.entry(*p).or_default().push(*q);
    }
    let mut vertices = vec![x];
    let mut at = y;
    while at != x {
        let next = from.get(&at).map(Vec::as_slice).unwrap_or_default();
        if next.len() != 1 || vertices.len() > boundary.len() {
            return Err("two openings of the cage meet at a vertex: \
                        the opening is not one loop of edges"
                .to_owned());
        }
        vertices.push(at);
        at = next[0];
    }
    Ok(Run {
        vertices,
        closed: true,
    })
}

/// How sharply a closed loop turns at each of its points: 0 straight on,
/// 2 straight back.
fn turning(points: &[Vec3]) -> Vec<f64> {
    let n = points.len();
    (0..n)
        .map(|k| {
            let before = unit(sub(points[k], points[(k + n - 1) % n]));
            let after = unit(sub(points[(k + 1) % n], points[k]));
            match (before, after) {
                (Some(u), Some(v)) => 1.0 - dot(u, v),
                _ => 0.0,
            }
        })
        .collect()
}

/// Fills a loop (its vertices as the new faces run round it) with a grid
/// of quads: the loop of `n` edges taken as four sides of `a`, `b`, `a`
/// and `b` edges (`a + b = n / 2`, its corners where the loop turns most),
/// the inner vertices on the Coons patch of the sides.
fn grid_fill(cage: &mut Cage, lp: &[u32], made: &mut Made) {
    let n = lp.len();
    let a = n / 4;
    let b = n / 2 - a;
    let points: Vec<Vec3> = lp
        .iter()
        .map(|v| cage.position(*v).expect("a vertex of the cage"))
        .collect();
    let turn = turning(&points);
    let sharpness = |s: usize| -> f64 {
        [s, s + a, s + a + b, s + 2 * a + b]
            .iter()
            .map(|k| turn[k % n])
            .sum()
    };
    // The first of the sharpest.
    let mut start = 0;
    for s in 1..n {
        if sharpness(s) > sharpness(start) + 1e-12 {
            start = s;
        }
    }
    let on = |k: usize| (start + k) % n;
    // The grid's rim on the loop: j = 0 along the first side, i = a along
    // the second, j = b back along the third, i = 0 back along the fourth.
    let rim = |i: usize, j: usize| -> Option<usize> {
        if j == 0 {
            Some(on(i))
        } else if i == a {
            Some(on(a + j))
        } else if j == b {
            Some(on(a + b + (a - i)))
        } else if i == 0 {
            Some(on(2 * a + b + (b - j)))
        } else {
            None
        }
    };
    let point = |i: usize, j: usize| points[rim(i, j).expect("a point of the rim")];
    let mut grid = vec![vec![0_u32; b + 1]; a + 1];
    let mut next_vertex = cage.next_vertex_id();
    for (i, column) in grid.iter_mut().enumerate() {
        for (j, cell) in column.iter_mut().enumerate() {
            if let Some(k) = rim(i, j) {
                *cell = lp[k];
                continue;
            }
            let (u, w) = (i as f64 / a as f64, j as f64 / b as f64);
            let (bottom, top) = (point(i, 0), point(i, b));
            let (left, right) = (point(0, j), point(a, j));
            let (p00, p10, p01, p11) = (point(0, 0), point(a, 0), point(0, b), point(a, b));
            let at = std::array::from_fn(|c| {
                (1.0 - w) * bottom[c] + w * top[c] + (1.0 - u) * left[c] + u * right[c]
                    - ((1.0 - u) * (1.0 - w) * p00[c]
                        + u * (1.0 - w) * p10[c]
                        + (1.0 - u) * w * p01[c]
                        + u * w * p11[c])
            });
            cage.vertices.push(CageVertex {
                id: next_vertex,
                at,
            });
            made.vertices.push(next_vertex);
            *cell = next_vertex;
            next_vertex += 1;
        }
    }
    let mut next_face = cage.next_face_id();
    for i in 0..a {
        for j in 0..b {
            cage.faces.push(CageFace {
                id: next_face,
                vertices: vec![
                    grid[i][j],
                    grid[i + 1][j],
                    grid[i + 1][j + 1],
                    grid[i][j + 1],
                ],
            });
            made.faces.push(next_face);
            next_face += 1;
        }
    }
}

/// Fills the opening `run` (as the faces beside it run along it): one
/// face, or with `quads` and an even number of edges a grid of quads.
fn fill_plain(cage: &mut Cage, run: &Run, quads: bool) -> Made {
    // The new faces run round the opening the other way.
    let lp: Vec<u32> = run.vertices.iter().rev().copied().collect();
    let mut made = Made::default();
    if quads && lp.len().is_multiple_of(2) {
        grid_fill(cage, &lp, &mut made);
    } else {
        let id = cage.next_face_id();
        cage.faces.push(CageFace { id, vertices: lp });
        made.faces.push(id);
    }
    made
}

/// Fills the opening the boundary edge `edge` lies on, so that the cage
/// closes there: with one face, or with `quads` and an even number of
/// edges round the opening with a grid of quads, as edge loops across one
/// face would leave. On a symmetric cage the mirrored opening is filled
/// too; an opening across the symmetry plane is refused. Returns the new
/// faces and vertices.
pub fn fill_hole(cage: &mut Cage, edge: [i64; 2], quads: bool) -> Result<Made, String> {
    cage.check()?;
    cage.note_ids();
    let before = cage.clone();
    let made = match Expanded::of(cage) {
        None => {
            let (a, _) = vertex_ref(cage, edge[0])?;
            let (b, _) = vertex_ref(cage, edge[1])?;
            let run = opening_of(cage, a, b)?;
            fill_plain(cage, &run, quads)
        }
        Some(mut whole) => {
            let a = whole.vertex(cage, edge[0])?;
            let b = whole.vertex(cage, edge[1])?;
            let run = opening_of(&whole.cage, a, b)?;
            let plane = whole.plane;
            let side =
                |v: &u32| plane.distance(whole.cage.position(*v).expect("a vertex of the cage"));
            if run.vertices.iter().any(|v| side(v) > ON_PLANE)
                && run.vertices.iter().any(|v| side(v) < -ON_PLANE)
            {
                return Err("the opening crosses the symmetry plane: \
                            take the symmetry away to fill it"
                    .to_owned());
            }
            let mut made = fill_plain(&mut whole.cage, &run, quads);
            // The mirror image, unless the fill closed it already.
            let (ma, mb) = (whole.mirror_vertex[&a], whole.mirror_vertex[&b]);
            if boundary_edges(&whole.cage).contains_key(&edge_key(ma, mb)) {
                let mirrored = opening_of(&whole.cage, ma, mb)?;
                made.extend(fill_plain(&mut whole.cage, &mirrored, quads));
            }
            *cage = whole.fold()?;
            made
        }
    };
    cage.note_ids();
    cage.check()?;
    Ok(made.kept_in(cage, &before))
}

/// The picked boundary edges as runs along the boundary (the edges that
/// follow one another), with `loops` each whole opening a picked edge lies
/// on.
fn boundary_runs(cage: &Cage, edges: &[[u32; 2]], loops: bool) -> Result<Vec<Run>, String> {
    let boundary = boundary_edges(cage);
    let mut picked: BTreeSet<[u32; 2]> = BTreeSet::new();
    for &[a, b] in edges {
        if loops {
            let run = opening_of(cage, a, b)?;
            let n = run.vertices.len();
            for k in 0..n {
                picked.insert(edge_key(run.vertices[k], run.vertices[(k + 1) % n]));
            }
        } else {
            on_boundary(cage, a, b)?;
            picked.insert(edge_key(a, b));
        }
    }
    // Each vertex's way on and back along the picked edges, as the faces
    // run along them.
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    let mut previous: BTreeMap<u32, u32> = BTreeMap::new();
    for edge in &picked {
        let (x, y) = boundary[edge];
        if next.insert(x, y).is_some() || previous.insert(y, x).is_some() {
            return Err("two openings of the cage meet at a vertex of the picked edges".to_owned());
        }
    }
    let mut left: BTreeSet<u32> = next.keys().copied().collect();
    let mut runs = Vec::new();
    while let Some(&first) = left.iter().next() {
        // Back to where the run starts; round to `first` on a closed one.
        let mut start = first;
        let mut closed = false;
        while let Some(&p) = previous.get(&start) {
            if p == first {
                closed = true;
                break;
            }
            start = p;
        }
        if closed {
            start = first;
        }
        let mut vertices = vec![start];
        let mut at = start;
        while let Some(&y) = next.get(&at) {
            left.remove(&at);
            if y == start {
                break;
            }
            vertices.push(y);
            at = y;
        }
        runs.push(Run { vertices, closed });
    }
    Ok(runs)
}

/// Joins two runs of the boundary of as many edges, both closed or both
/// open, with a strip of quads `segments` rows across.
fn bridge_plain(cage: &mut Cage, from: &Run, to: &Run, segments: u32) -> Result<Made, String> {
    if from.closed != to.closed {
        return Err(
            "a bridge joins two openings or two runs of open edges, not one of each".to_owned(),
        );
    }
    let k = from.edges();
    if k != to.edges() {
        return Err("the two sides of a bridge must have as many edges each".to_owned());
    }
    let at = |run: &Run| -> Vec<Vec3> {
        run.vertices
            .iter()
            .map(|v| cage.position(*v).expect("a vertex of the cage"))
            .collect()
    };
    let (pa, pb) = (at(from), at(to));
    // The vertex of `to` each vertex of `from` is joined to: `to` runs the
    // other way along the strip, so that the strip's faces run as the faces
    // beside both sides do; round two openings, turned to where the joins
    // are shortest.
    let count = from.vertices.len();
    let partner: Vec<usize> = if from.closed {
        let length = |s: usize| -> f64 {
            (0..count)
                .map(|i| mitcad_vec3::distance(pa[i], pb[(s + count - i) % count]))
                .sum()
        };
        let mut shift = 0;
        for s in 1..count {
            if length(s) < length(shift) - 1e-9 {
                shift = s;
            }
        }
        (0..count).map(|i| (shift + count - i) % count).collect()
    } else {
        (0..count).map(|i| count - 1 - i).collect()
    };
    let rows = segments as usize;
    let mut made = Made::default();
    let mut next_vertex = cage.next_vertex_id();
    // The rows of vertices from `from` (0) across to `to` (rows).
    let mut grid: Vec<Vec<u32>> = vec![from.vertices.clone()];
    for r in 1..rows {
        let t = r as f64 / rows as f64;
        let mut row = Vec::with_capacity(count);
        for (p, &j) in pa.iter().zip(&partner) {
            cage.vertices.push(CageVertex {
                id: next_vertex,
                at: add(*p, scale(sub(pb[j], *p), t)),
            });
            made.vertices.push(next_vertex);
            row.push(next_vertex);
            next_vertex += 1;
        }
        grid.push(row);
    }
    grid.push(partner.iter().map(|j| to.vertices[*j]).collect());
    let mut next_face = cage.next_face_id();
    for r in 0..rows {
        for i in 0..k {
            let j = (i + 1) % count;
            cage.faces.push(CageFace {
                id: next_face,
                vertices: vec![grid[r][j], grid[r][i], grid[r + 1][i], grid[r + 1][j]],
            });
            made.faces.push(next_face);
            next_face += 1;
        }
    }
    Ok(made)
}

/// Splits runs into the two sides of a bridge.
fn two_sides(runs: Vec<Run>) -> Result<(Run, Run), String> {
    match <[Run; 2]>::try_from(runs) {
        Ok([a, b]) => Ok((a, b)),
        Err(runs) => Err(format!(
            "a bridge joins two openings or two runs of open edges: \
             the picked edges make {} apart",
            runs.len()
        )),
    }
}

/// Joins two openings, or two runs of open edges of as many edges each,
/// with a strip of quads `segments` rows across. `edges` are the picked
/// boundary edges of both sides; with `loops` each stands for the whole
/// opening it lies on (one edge a side is enough). On a symmetric cage the
/// mirror image is bridged too, unless the bridge is its own mirror image.
/// Returns the new faces and vertices.
pub fn bridge(
    cage: &mut Cage,
    edges: &[[i64; 2]],
    loops: bool,
    segments: u32,
) -> Result<Made, String> {
    cage.check()?;
    cage.note_ids();
    if edges.is_empty() {
        return Err("no edges are picked: pick edges of the two openings to join".to_owned());
    }
    if !(1..=100).contains(&segments) {
        return Err("a bridge has 1 to 100 segments across".to_owned());
    }
    let before = cage.clone();
    let made = match Expanded::of(cage) {
        None => {
            let mut picked = Vec::with_capacity(edges.len());
            for e in edges {
                picked.push([vertex_ref(cage, e[0])?.0, vertex_ref(cage, e[1])?.0]);
            }
            let (a, b) = two_sides(boundary_runs(cage, &picked, loops)?)?;
            bridge_plain(cage, &a, &b, segments)?
        }
        Some(mut whole) => {
            let mut picked = Vec::with_capacity(edges.len());
            for e in edges {
                picked.push([whole.vertex(cage, e[0])?, whole.vertex(cage, e[1])?]);
            }
            let (a, b) = two_sides(boundary_runs(&whole.cage, &picked, loops)?)?;
            let mut made = bridge_plain(&mut whole.cage, &a, &b, segments)?;
            // The mirror image, unless the bridge joined it already.
            let mirrored: Vec<[u32; 2]> = picked
                .iter()
                .map(|[x, y]| [whole.mirror_vertex[x], whole.mirror_vertex[y]])
                .collect();
            let boundary = boundary_edges(&whole.cage);
            if mirrored
                .iter()
                .all(|[x, y]| boundary.contains_key(&edge_key(*x, *y)))
            {
                let (ma, mb) = two_sides(boundary_runs(&whole.cage, &mirrored, loops)?)?;
                made.extend(bridge_plain(&mut whole.cage, &ma, &mb, segments)?);
            }
            *cage = whole.fold()?;
            made
        }
    };
    cage.note_ids();
    cage.check()?;
    Ok(made.kept_in(cage, &before))
}

/// Creases edges (a sharpness, none for sharp for good, zero for smooth
/// again) and makes vertices corners (a sharpness of zero: no longer
/// corners). A mirrored edge or vertex is its stored one's mirror image:
/// they are creased together.
pub fn crease(
    cage: &mut Cage,
    edges: &[[i64; 2]],
    vertices: &[i64],
    sharpness: Option<f64>,
) -> Result<(), String> {
    cage.check()?;
    if edges.is_empty() && vertices.is_empty() {
        return Err("nothing is selected".to_owned());
    }
    if sharpness.is_some_and(|s| !(s.is_finite() && s >= 0.0)) {
        return Err("a crease's sharpness must not be less than zero".to_owned());
    }
    for edge in edges {
        let ([a, b], _) = edge_ref(cage, *edge)?;
        cage.set_crease(a, b, sharpness);
    }
    for id in vertices {
        let (v, _) = vertex_ref(cage, *id)?;
        cage.corners.retain(|c| *c != v);
        if sharpness != Some(0.0) {
            cage.corners.push(v);
        }
    }
    cage.corners.sort_unstable();
    cage.corners.dedup();
    cage.check()
}

/// Sets the symmetry plane (the half on the side its normal points to is
/// kept and the other becomes its mirror image) or, with none, makes the
/// mirrored half real geometry of the cage. A face across the plane refuses
/// it: an edge loop on the plane must come first.
pub fn set_symmetry(cage: &mut Cage, plane: Option<SymmetryPlane>) -> Result<(), String> {
    cage.check()?;
    cage.note_ids();
    let whole = match Expanded::of(cage) {
        Some(whole) => whole.cage,
        None => cage.clone(),
    };
    *cage = match plane {
        None => whole,
        Some(plane) => cut(&whole, SymmetryPlane::new(plane.origin, plane.normal)?)?,
    };
    cage.note_ids();
    cage.check()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::box_cage;

    fn x_plane() -> SymmetryPlane {
        SymmetryPlane::new([0.0; 3], [1.0, 0.0, 0.0]).unwrap()
    }

    /// Every vertex of the whole cage has its mirror image in it.
    fn assert_symmetric(cage: &Cage) {
        let full = cage.full();
        assert!(full.closed());
        for p in &full.mesh.points {
            let image = [-p[0], p[1], p[2]];
            assert!(
                full.mesh
                    .points
                    .iter()
                    .any(|q| mitcad_vec3::distance(*q, image) < 1e-9),
                "{p:?}"
            );
        }
    }

    fn top_face(cage: &Cage) -> u32 {
        cage.faces
            .iter()
            .find(|f| cage.face_normal(f).0[2] > 0.99)
            .map(|f| f.id)
            .unwrap()
    }

    #[test]
    fn faces_edges_and_vertices_move_turn_and_scale() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        let top = top_face(&cage);
        let moved = transform(
            &mut cage,
            &Selection {
                faces: vec![i64::from(top)],
                ..Selection::default()
            },
            Motion::Translate([0.0, 0.0, 5.0]),
        )
        .unwrap();
        assert_eq!(moved.len(), 4);
        assert_eq!(cage.vertices.iter().filter(|v| v.at[2] == 15.0).count(), 4);
        assert_eq!(cage.vertices.iter().filter(|v| v.at[2] == -10.0).count(), 4);
        // The top scaled to half about its centre.
        transform(
            &mut cage,
            &Selection {
                faces: vec![i64::from(top)],
                ..Selection::default()
            },
            Motion::Scale {
                center: None,
                factors: [0.5, 0.5, 1.0],
            },
        )
        .unwrap();
        assert!(
            cage.vertices
                .iter()
                .filter(|v| v.at[2] == 15.0)
                .all(|v| v.at[0].abs() == 5.0 && v.at[1].abs() == 5.0)
        );
        // A quarter turn of one edge about z through the origin.
        let face = cage.face(top).unwrap().vertices.clone();
        let edge = [i64::from(face[0]), i64::from(face[1])];
        let before = cage.position(face[0]).unwrap();
        transform(
            &mut cage,
            &Selection {
                edges: vec![edge],
                ..Selection::default()
            },
            Motion::Rotate {
                origin: Some([0.0; 3]),
                axis: [0.0, 0.0, 1.0],
                angle: std::f64::consts::FRAC_PI_2,
            },
        )
        .unwrap();
        let after = cage.position(face[0]).unwrap();
        assert!((after[0] + before[1]).abs() < 1e-12 && (after[1] - before[0]).abs() < 1e-12);
        // Nothing selected, a mirrored id without a plane, a zero scale.
        assert!(
            transform(
                &mut cage,
                &Selection::default(),
                Motion::Translate([1.0; 3])
            )
            .is_err()
        );
        assert!(
            transform(
                &mut cage,
                &Selection {
                    vertices: vec![-1],
                    ..Selection::default()
                },
                Motion::Translate([1.0; 3])
            )
            .unwrap_err()
            .contains("no symmetry plane")
        );
    }

    #[test]
    fn symmetry_keeps_the_halves_equal() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 1, 1]).unwrap();
        // A box of one segment across the plane cannot be cut.
        let mut single = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        assert!(
            set_symmetry(&mut single, Some(x_plane()))
                .unwrap_err()
                .contains("crosses the symmetry plane")
        );
        set_symmetry(&mut cage, Some(x_plane())).unwrap();
        assert_eq!(cage.faces.len(), 5);
        assert_symmetric(&cage);
        // A mirrored corner moved: the stored one moves by the mirror image.
        let corner = cage
            .vertices
            .iter()
            .find(|v| v.at == [10.0, 10.0, 10.0])
            .unwrap()
            .id;
        transform(
            &mut cage,
            &Selection {
                vertices: vec![-i64::from(corner)],
                ..Selection::default()
            },
            Motion::Translate([-3.0, 1.0, 2.0]),
        )
        .unwrap();
        assert_eq!(cage.position(corner).unwrap(), [13.0, 11.0, 12.0]);
        assert_symmetric(&cage);
        // A vertex on the plane stays on it.
        let seam = cage.vertices.iter().find(|v| v.at[0] == 0.0).unwrap().id;
        transform(
            &mut cage,
            &Selection {
                vertices: vec![i64::from(seam)],
                ..Selection::default()
            },
            Motion::Translate([4.0, 0.0, 1.0]),
        )
        .unwrap();
        assert_eq!(cage.position(seam).unwrap()[0], 0.0);
        assert_symmetric(&cage);
        // The plane taken away: the mirrored half becomes the cage's own.
        set_symmetry(&mut cage, None).unwrap();
        assert_eq!(cage.faces.len(), 10);
        assert!(cage.symmetry.is_none());
        assert_symmetric(&cage);
    }

    #[test]
    fn faces_extrude_with_walls_round_them() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        let top = top_face(&cage);
        let made = extrude(&mut cage, &[i64::from(top)], 5.0).unwrap();
        assert_eq!(made.vertices.len(), 4);
        assert_eq!(made.faces.len(), 4);
        assert_eq!(cage.faces.len(), 10);
        assert!(cage.full().closed());
        // The top keeps its id, 5 higher.
        let face = cage.face(top).unwrap();
        assert!(
            face.vertices
                .iter()
                .all(|v| cage.position(*v).unwrap()[2] == 15.0)
        );
        assert!(extrude(&mut cage, &[i64::from(top)], 0.0).is_err());
    }

    #[test]
    fn a_symmetric_extrusion_takes_its_mirror_image() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 1, 1]).unwrap();
        set_symmetry(&mut cage, Some(x_plane())).unwrap();
        let top = top_face(&cage);
        let made = extrude(&mut cage, &[i64::from(top)], 5.0).unwrap();
        // Walls on the front, the back and the end; none on the plane.
        assert_eq!(made.faces.len(), 3);
        assert_eq!(cage.faces.len(), 8);
        assert_symmetric(&cage);
        // The mirrored face picked does the same.
        let mut other = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 1, 1]).unwrap();
        set_symmetry(&mut other, Some(x_plane())).unwrap();
        let top = top_face(&other);
        extrude(&mut other, &[-i64::from(top)], 5.0).unwrap();
        assert_eq!(other, cage);
    }

    #[test]
    fn edge_loops_go_in_and_come_out() {
        let original = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        let mut cage = original.clone();
        // A vertical edge: the loop runs round the four sides.
        let edge = cage
            .edges()
            .keys()
            .copied()
            .find(|[a, b]| {
                let (p, q) = (cage.position(*a).unwrap(), cage.position(*b).unwrap());
                p[0] == q[0] && p[1] == q[1]
            })
            .unwrap();
        let made = insert_loop(&mut cage, [i64::from(edge[0]), i64::from(edge[1])], 0.25).unwrap();
        assert_eq!(made.vertices.len(), 4);
        assert_eq!(made.faces.len(), 4);
        assert_eq!(cage.faces.len(), 10);
        assert!(cage.full().closed());
        // A quarter up from the edge's first vertex.
        let low = cage.position(edge[0]).unwrap()[2];
        let high = cage.position(edge[1]).unwrap()[2];
        let level = low + 0.25 * (high - low);
        assert!(
            made.vertices
                .iter()
                .all(|v| (cage.position(*v).unwrap()[2] - level).abs() < 1e-12)
        );
        // Taken out again: the box as it was, but for the faces' ids.
        let across = made.edges[0];
        let removed = delete_loop(&mut cage, [i64::from(across[0]), i64::from(across[1])]).unwrap();
        assert_eq!(removed.removed_vertices.len(), 4);
        assert_eq!(removed.removed_faces.len(), 4);
        assert_eq!(cage.faces.len(), 6);
        assert_eq!(cage.vertices, original.vertices);
    }

    #[test]
    fn a_symmetric_edge_loop_goes_round_both_halves() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 1, 1]).unwrap();
        set_symmetry(&mut cage, Some(x_plane())).unwrap();
        // The vertical edge at the end of the stored half.
        let edge = cage
            .edges()
            .keys()
            .copied()
            .find(|[a, b]| {
                let (p, q) = (cage.position(*a).unwrap(), cage.position(*b).unwrap());
                p[0] == 10.0 && q[0] == 10.0 && p[1] == q[1]
            })
            .unwrap();
        let made = insert_loop(&mut cage, [i64::from(edge[0]), i64::from(edge[1])], 0.5).unwrap();
        // Two vertices at the end, two on the plane.
        assert_eq!(made.vertices.len(), 4);
        assert_eq!(cage.faces.len(), 8);
        assert_symmetric(&cage);
    }

    #[test]
    fn creases_and_corners_are_set_and_cleared() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 1, 1]).unwrap();
        set_symmetry(&mut cage, Some(x_plane())).unwrap();
        let [a, b] = *cage.edges().keys().next().unwrap();
        crease(&mut cage, &[[i64::from(a), i64::from(b)]], &[], None).unwrap();
        assert_eq!(cage.crease(a, b), Some(f64::INFINITY));
        crease(&mut cage, &[[i64::from(a), i64::from(b)]], &[], Some(2.5)).unwrap();
        assert_eq!(cage.crease(a, b), Some(2.5));
        crease(&mut cage, &[[i64::from(a), i64::from(b)]], &[], Some(0.0)).unwrap();
        assert_eq!(cage.crease(a, b), None);
        // Picked on the mirrored half, the stored vertex becomes a corner.
        let off_plane = cage.vertices.iter().find(|v| v.at[0] > 0.0).unwrap().id;
        crease(&mut cage, &[], &[-i64::from(off_plane)], None).unwrap();
        assert!(cage.is_corner(off_plane));
        assert!(cage.full().mesh.corners.iter().filter(|c| **c).count() == 2);
        assert!(crease(&mut cage, &[], &[], None).is_err());
    }

    /// Ids that went are not given again: the face with the largest id
    /// removed, an extrusion's walls get new ids past it, and the counters
    /// are saved with the cage.
    #[test]
    fn ids_that_went_are_not_given_again() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        let last = cage.faces.iter().map(|f| f.id).max().unwrap();
        assert_eq!(last, 6);
        delete_faces(&mut cage, &[i64::from(last)]).unwrap();
        assert_eq!(cage.next_face_id(), 7);
        let text = serde_json::to_string(&cage).unwrap();
        let read: Cage = serde_json::from_str(&text).unwrap();
        assert_eq!(read, cage);
        assert_eq!(read.next_face_id(), 7);
        let top = top_face(&cage);
        let made = extrude(&mut cage, &[i64::from(top)], 5.0).unwrap();
        assert!(made.faces.iter().all(|f| *f > last), "{made:?}");
        // A cage that replaces it keeps the counters.
        let mut fresh = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        fresh.keep_ids_of(&cage);
        assert!(fresh.next_face_id() > 6 + 4);
    }

    #[test]
    fn faces_are_removed() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        let top = top_face(&cage);
        let made = delete_faces(&mut cage, &[i64::from(top)]).unwrap();
        assert_eq!(made.removed_faces, [top]);
        assert!(made.removed_vertices.is_empty());
        assert!(!cage.full().closed());
    }

    /// The faces of the cage that face up with their centre at `z`.
    fn faces_up_at(cage: &Cage, z: f64) -> Vec<u32> {
        cage.faces
            .iter()
            .filter(|f| {
                let (normal, center) = cage.face_normal(f);
                normal[2] > 0.99 && (center[2] - z).abs() < 1e-9
            })
            .map(|f| f.id)
            .collect()
    }

    #[test]
    fn an_opening_is_filled_with_one_face_or_with_quads() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        let top = top_face(&cage);
        let rim = cage.face(top).unwrap().vertices.clone();
        delete_faces(&mut cage, &[i64::from(top)]).unwrap();
        // An edge with a face on both sides is not on an opening.
        let bottom = cage
            .faces
            .iter()
            .find(|f| cage.face_normal(f).0[2] < -0.99)
            .unwrap()
            .vertices
            .clone();
        assert!(
            fill_hole(
                &mut cage.clone(),
                [i64::from(bottom[0]), i64::from(bottom[1])],
                false
            )
            .unwrap_err()
            .contains("not on an opening")
        );
        let edge = [i64::from(rim[0]), i64::from(rim[1])];
        let mut one = cage.clone();
        let made = fill_hole(&mut one, edge, false).unwrap();
        assert_eq!(made.faces.len(), 1);
        assert!(made.vertices.is_empty());
        assert!(one.full().closed());
        // The new face faces out (up), as the one taken out did.
        let face = one.face(made.faces[0]).unwrap();
        assert!(one.face_normal(face).0[2] > 0.99);
        // Four edges round it: one quad, as with one face.
        let mut quads = cage.clone();
        assert_eq!(fill_hole(&mut quads, edge, true).unwrap().faces.len(), 1);

        // A top of 2 x 2 faces taken out: eight edges round the opening,
        // filled with a grid of 2 x 2 quads round a new vertex in the middle.
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 2, 2]).unwrap();
        let tops = faces_up_at(&cage, 10.0);
        assert_eq!(tops.len(), 4);
        let ids: Vec<i64> = tops.iter().map(|f| i64::from(*f)).collect();
        delete_faces(&mut cage, &ids).unwrap();
        let edge = *boundary_edges(&cage).keys().next().unwrap();
        let edge = [i64::from(edge[0]), i64::from(edge[1])];
        let mut one = cage.clone();
        let made = fill_hole(&mut one, edge, false).unwrap();
        assert_eq!(made.faces.len(), 1);
        assert_eq!(one.face(made.faces[0]).unwrap().vertices.len(), 8);
        assert!(one.full().closed());
        let made = fill_hole(&mut cage, edge, true).unwrap();
        assert_eq!(made.faces.len(), 4);
        assert_eq!(made.vertices.len(), 1);
        let middle = cage.position(made.vertices[0]).unwrap();
        assert!(
            mitcad_vec3::distance(middle, [0.0, 0.0, 10.0]) < 1e-9,
            "{middle:?}"
        );
        assert!(cage.full().closed());
        assert_eq!(faces_up_at(&cage, 10.0).len(), 4);
    }

    #[test]
    fn a_symmetric_cage_fills_both_openings_and_refuses_one_across_the_plane() {
        let mut cage = box_cage([0.0; 3], [40.0, 20.0, 20.0], [4, 1, 1]).unwrap();
        set_symmetry(&mut cage, Some(x_plane())).unwrap();
        assert_eq!(cage.faces.len(), 9);
        // The top face away from the plane.
        let outer = cage
            .faces
            .iter()
            .find(|f| {
                let (normal, center) = cage.face_normal(f);
                normal[2] > 0.99 && center[0] > 10.0
            })
            .unwrap()
            .clone();
        delete_faces(&mut cage, &[i64::from(outer.id)]).unwrap();
        assert!(!cage.full().closed());
        // Picked on the mirrored half: both are filled.
        let mut filled = cage.clone();
        let made = fill_hole(
            &mut filled,
            [-i64::from(outer.vertices[0]), -i64::from(outer.vertices[1])],
            false,
        )
        .unwrap();
        assert_eq!(made.faces.len(), 1);
        assert_eq!(filled.faces.len(), 9);
        assert_symmetric(&filled);

        // The top face beside the plane: the opening runs across it.
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 1, 1]).unwrap();
        set_symmetry(&mut cage, Some(x_plane())).unwrap();
        let top = top_face(&cage);
        let rim = cage.face(top).unwrap().vertices.clone();
        delete_faces(&mut cage, &[i64::from(top)]).unwrap();
        let off_plane: Vec<u32> = rim
            .iter()
            .copied()
            .filter(|v| cage.position(*v).unwrap()[0] > 1.0)
            .collect();
        let error = fill_hole(
            &mut cage,
            [i64::from(off_plane[0]), i64::from(off_plane[1])],
            false,
        )
        .unwrap_err();
        assert!(error.contains("crosses the symmetry plane"), "{error}");
    }

    /// Two tubes (cylinders without one cap each) in one cage, their open
    /// ends facing each other.
    fn two_tubes() -> Cage {
        let mut cage = crate::primitive::cylinder_cage([0.0; 3], 20.0, 10.0, 8, 1).unwrap();
        let upper = crate::primitive::cylinder_cage([0.0, 0.0, 30.0], 20.0, 10.0, 8, 1).unwrap();
        // The lower tube's top cap and the upper one's bottom cap go.
        cage.faces.retain(|f| f.id != 2);
        cage.vertices
            .extend(upper.vertices.iter().map(|v| CageVertex {
                id: v.id + 100,
                at: v.at,
            }));
        cage.faces
            .extend(upper.faces.iter().filter(|f| f.id != 1).map(|f| CageFace {
                id: f.id + 100,
                vertices: f.vertices.iter().map(|v| v + 100).collect(),
            }));
        cage.check().unwrap();
        cage
    }

    #[test]
    fn two_openings_are_bridged() {
        let mut cage = two_tubes();
        // The lower tube's top ring is 9 to 16, the upper one's bottom ring
        // 101 to 108: one edge of each with `loops`, two rows across.
        let made = bridge(&mut cage, &[[9, 10], [101, 102]], true, 2).unwrap();
        assert_eq!(made.faces.len(), 16);
        assert_eq!(made.vertices.len(), 8);
        assert!(cage.full().closed());
        // The middle row half way up, on the tubes' radius.
        for v in &made.vertices {
            let p = cage.position(*v).unwrap();
            assert!((p[2] - 15.0).abs() < 1e-9, "{p:?}");
            assert!(
                ((p[0] * p[0] + p[1] * p[1]).sqrt() - 10.0).abs() < 1e-9,
                "{p:?}"
            );
        }
        // The rings picked edge by edge: the same.
        let mut picked = two_tubes();
        let ring = |first: i64| (0..8).map(move |k| [first + k, first + (k + 1) % 8]);
        let edges: Vec<[i64; 2]> = ring(9).chain(ring(101)).collect();
        let made = bridge(&mut picked, &edges, false, 1).unwrap();
        assert_eq!(made.faces.len(), 8);
        assert!(picked.full().closed());

        // Runs of open edges: two edges of each ring, one row across.
        let mut runs = two_tubes();
        let made = bridge(
            &mut runs,
            &[[9, 10], [10, 11], [101, 102], [102, 103]],
            false,
            1,
        )
        .unwrap();
        assert_eq!(made.faces.len(), 2);
        assert!(!runs.full().closed());
        // Sides of different lengths, one side only, an edge inside.
        let mut cage = two_tubes();
        assert!(
            bridge(&mut cage, &[[9, 10], [10, 11], [101, 102]], false, 1)
                .unwrap_err()
                .contains("as many edges")
        );
        assert!(
            bridge(&mut cage, &[[9, 10]], false, 1)
                .unwrap_err()
                .contains("make 1 apart")
        );
        assert!(
            bridge(&mut cage, &[[1, 9], [101, 102]], false, 1)
                .unwrap_err()
                .contains("not on an opening")
        );
    }
}
