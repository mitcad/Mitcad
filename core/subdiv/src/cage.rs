// SPDX-License-Identifier: MIT
//! The control cage a Form stores (mitcad#1757).
//!
//! Vertices and faces have ids of their own (from 1), which stay with them
//! through edits: a face's id names the faces its limit surface makes, so
//! that later features find them again. A face is the loop of its
//! vertices, counter-clockwise seen from outside. Edges are not stored:
//! two vertices that follow each other in a face's loop are an edge, named
//! by the two vertex ids. A crease makes an edge sharp: fully (no
//! sharpness) or for as many subdivision steps as its sharpness says, then
//! smooth (a semi-sharp crease, a rounded edge). A corner keeps a vertex
//! where it is.
//!
//! With a symmetry plane the cage stores the half on the side the plane's
//! normal points to, with the vertices of the cut on the plane (within
//! [`ON_PLANE`]); the other half is its mirror image. In the whole cage
//! ([`Cage::full`]) the mirrored vertices and faces have the negative ids
//! of the ones they mirror; a vertex on the plane is its own mirror image
//! and has only its own id.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mitcad_vec3::{Vec3, add, cross, dot, scale, sub, unit};
use serde::{Deserialize, Serialize};

use crate::mesh::{Mesh, Topology, key};

/// How far a vertex of a symmetric cage may lie from the symmetry plane
/// and still be on it, mm.
pub const ON_PLANE: f64 = 1e-6;

/// A control cage (see the module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cage {
    pub vertices: Vec<CageVertex>,
    pub faces: Vec<CageFace>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creases: Vec<Crease>,
    /// Vertices kept where they are (sharp corners).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corners: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symmetry: Option<SymmetryPlane>,
    /// The id the next new vertex gets at least, and the next new face
    /// (0: one more than the largest there is). Kept with the cage, so that
    /// an id an edit took out is never given again: a later feature's
    /// reference to a face that is gone must not find a new one.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_vertex: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_face: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// A control vertex, mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CageVertex {
    pub id: u32,
    pub at: Vec3,
}

/// A face: its vertices counter-clockwise seen from outside.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CageFace {
    pub id: u32,
    pub vertices: Vec<u32>,
}

/// A creased edge, by its two vertices.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crease {
    pub edge: [u32; 2],
    /// How many subdivision steps the edge stays sharp (a fraction blends
    /// the last one); none: sharp for good.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sharpness: Option<f64>,
}

impl Crease {
    /// The sharpness, infinite for a sharp crease.
    pub fn value(&self) -> f64 {
        self.sharpness.unwrap_or(f64::INFINITY)
    }
}

/// A plane of symmetry: through `origin`, the stored half on the side
/// `normal` points to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymmetryPlane {
    pub origin: Vec3,
    pub normal: Vec3,
}

impl SymmetryPlane {
    /// The plane with its normal made a unit vector; Err for a normal
    /// without a direction or a point that is not finite.
    pub fn new(origin: Vec3, normal: Vec3) -> Result<Self, String> {
        let normal = unit(normal).ok_or("the symmetry plane's normal has no direction")?;
        if !origin.iter().all(|c| c.is_finite()) {
            return Err("the symmetry plane's point is not finite".to_owned());
        }
        Ok(Self { origin, normal })
    }

    fn unit_normal(&self) -> Vec3 {
        unit(self.normal).unwrap_or([0.0, 0.0, 1.0])
    }

    /// The signed distance of a point from the plane, positive on the
    /// stored side.
    pub fn distance(&self, p: Vec3) -> f64 {
        dot(sub(p, self.origin), self.unit_normal())
    }

    /// Whether a point lies on the plane (within [`ON_PLANE`]).
    pub fn holds(&self, p: Vec3) -> bool {
        self.distance(p).abs() <= ON_PLANE
    }

    /// The point's mirror image.
    pub fn mirror(&self, p: Vec3) -> Vec3 {
        sub(p, scale(self.unit_normal(), 2.0 * self.distance(p)))
    }

    /// A direction's mirror image.
    pub fn mirror_vector(&self, v: Vec3) -> Vec3 {
        let n = self.unit_normal();
        sub(v, scale(n, 2.0 * dot(v, n)))
    }

    /// The point moved onto the plane along its normal.
    pub fn project(&self, p: Vec3) -> Vec3 {
        sub(p, scale(self.unit_normal(), self.distance(p)))
    }
}

/// An edge's two vertex ids, the smaller first.
pub fn edge_key(a: u32, b: u32) -> [u32; 2] {
    if a <= b { [a, b] } else { [b, a] }
}

/// The unit normal of a polygon by Newell's method (zero when it has
/// none) and its centre (the mean of its points).
pub fn polygon_normal(points: &[Vec3]) -> (Vec3, Vec3) {
    let n = points.len();
    let mut normal = [0.0; 3];
    let mut center = [0.0; 3];
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % n];
        normal = add(normal, cross(*p, q));
        center = add(center, *p);
    }
    let center = if n == 0 {
        center
    } else {
        scale(center, 1.0 / n as f64)
    };
    (unit(normal).unwrap_or([0.0; 3]), center)
}

impl Cage {
    pub fn vertex(&self, id: u32) -> Option<&CageVertex> {
        self.vertices.iter().find(|v| v.id == id)
    }

    pub fn vertex_mut(&mut self, id: u32) -> Option<&mut CageVertex> {
        self.vertices.iter_mut().find(|v| v.id == id)
    }

    pub fn position(&self, id: u32) -> Option<Vec3> {
        self.vertex(id).map(|v| v.at)
    }

    pub fn face(&self, id: u32) -> Option<&CageFace> {
        self.faces.iter().find(|f| f.id == id)
    }

    /// The id a new vertex gets: past every vertex there is and every one
    /// there was ([`Cage::next_vertex`]).
    pub fn next_vertex_id(&self) -> u32 {
        let past = self.vertices.iter().map(|v| v.id).max().unwrap_or(0) + 1;
        past.max(self.next_vertex)
    }

    /// The id a new face gets: past every face there is and every one there
    /// was ([`Cage::next_face`]).
    pub fn next_face_id(&self) -> u32 {
        let past = self.faces.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        past.max(self.next_face)
    }

    /// Records the ids given so far ([`Cage::next_vertex`],
    /// [`Cage::next_face`]), so that none is given again after its element
    /// goes.
    pub fn note_ids(&mut self) {
        self.next_vertex = self.next_vertex_id();
        self.next_face = self.next_face_id();
    }

    /// Takes the counters of `earlier` too, where they are further on: a
    /// cage that replaces it gives no id it gave.
    pub fn keep_ids_of(&mut self, earlier: &Cage) {
        self.next_vertex = self.next_vertex_id().max(earlier.next_vertex_id());
        self.next_face = self.next_face_id().max(earlier.next_face_id());
    }

    /// Every edge with the ids of its faces.
    pub fn edges(&self) -> BTreeMap<[u32; 2], Vec<u32>> {
        let mut edges: BTreeMap<[u32; 2], Vec<u32>> = BTreeMap::new();
        for face in &self.faces {
            let n = face.vertices.len();
            for k in 0..n {
                let a = face.vertices[k];
                let b = face.vertices[(k + 1) % n];
                edges.entry(edge_key(a, b)).or_default().push(face.id);
            }
        }
        edges
    }

    /// The edge's sharpness when it is creased (infinite for a sharp
    /// crease).
    pub fn crease(&self, a: u32, b: u32) -> Option<f64> {
        let edge = edge_key(a, b);
        self.creases
            .iter()
            .find(|c| edge_key(c.edge[0], c.edge[1]) == edge)
            .map(Crease::value)
    }

    /// Creases the edge (`None`: sharp for good) or, with a sharpness of
    /// zero or less, makes it smooth.
    pub fn set_crease(&mut self, a: u32, b: u32, sharpness: Option<f64>) {
        let edge = edge_key(a, b);
        self.creases
            .retain(|c| edge_key(c.edge[0], c.edge[1]) != edge);
        if sharpness.is_none_or(|s| s > 0.0) {
            self.creases.push(Crease { edge, sharpness });
            self.creases.sort_by_key(|c| edge_key(c.edge[0], c.edge[1]));
        }
    }

    pub fn is_corner(&self, id: u32) -> bool {
        self.corners.contains(&id)
    }

    /// A face's points (the vertices it names that the cage has).
    pub fn face_points(&self, face: &CageFace) -> Vec<Vec3> {
        face.vertices
            .iter()
            .filter_map(|v| self.position(*v))
            .collect()
    }

    /// A face's unit normal (zero when it has none) and centre.
    pub fn face_normal(&self, face: &CageFace) -> (Vec3, Vec3) {
        polygon_normal(&self.face_points(face))
    }

    /// Takes out the vertices no face uses, the creases of edges that are
    /// gone and the corners of vertices that are gone.
    pub fn remove_orphans(&mut self) {
        let used: BTreeSet<u32> = self
            .faces
            .iter()
            .flat_map(|f| f.vertices.iter().copied())
            .collect();
        self.vertices.retain(|v| used.contains(&v.id));
        let edges = self.edges();
        self.creases
            .retain(|c| edges.contains_key(&edge_key(c.edge[0], c.edge[1])));
        self.corners.retain(|v| used.contains(v));
        self.corners.sort_unstable();
        self.corners.dedup();
    }

    /// Checks the cage: ids from 1 and each once, finite positions, faces
    /// of three vertices or more that the cage has, each edge in at most
    /// two faces that run along it opposite ways, every vertex in a face,
    /// creases on edges and corners on vertices of the cage; with a
    /// symmetry plane, the vertices on its stored side, no face in the
    /// plane and every edge in the plane in one face (the cut). The
    /// messages say what is wrong in words.
    pub fn check(&self) -> Result<(), String> {
        if self.faces.is_empty() {
            return Err("the cage has no faces".to_owned());
        }
        let mut ids = BTreeSet::new();
        for v in &self.vertices {
            if v.id == 0 {
                return Err("a vertex of the cage has the id 0; ids start at 1".to_owned());
            }
            if !ids.insert(v.id) {
                return Err("two vertices of the cage have the same id".to_owned());
            }
            if !v.at.iter().all(|c| c.is_finite()) {
                return Err("a vertex of the cage is not at a finite position".to_owned());
            }
        }
        let mut face_ids = BTreeSet::new();
        let mut used = BTreeSet::new();
        let mut directed = BTreeSet::new();
        let mut undirected: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        for face in &self.faces {
            if face.id == 0 {
                return Err("a face of the cage has the id 0; ids start at 1".to_owned());
            }
            if !face_ids.insert(face.id) {
                return Err("two faces of the cage have the same id".to_owned());
            }
            let n = face.vertices.len();
            if n < 3 {
                return Err("a face of the cage has fewer than three corners".to_owned());
            }
            let distinct: BTreeSet<u32> = face.vertices.iter().copied().collect();
            if distinct.len() != n {
                return Err("a face of the cage passes through one vertex twice".to_owned());
            }
            for (k, &a) in face.vertices.iter().enumerate() {
                if !ids.contains(&a) {
                    return Err(
                        "a face of the cage uses a vertex the cage does not have".to_owned()
                    );
                }
                used.insert(a);
                let b = face.vertices[(k + 1) % n];
                if !directed.insert((a, b)) {
                    return Err("two faces of the cage run the same way along an edge: \
                                their corners must go round the same way, \
                                counter-clockwise seen from outside"
                        .to_owned());
                }
                let count = undirected.entry(edge_key(a, b)).or_insert(0);
                *count += 1;
                if *count > 2 {
                    return Err("an edge of the cage has more than two faces".to_owned());
                }
            }
        }
        if used.len() != ids.len() {
            return Err("a vertex of the cage belongs to no face".to_owned());
        }
        let mut creased = BTreeSet::new();
        for crease in &self.creases {
            let edge = edge_key(crease.edge[0], crease.edge[1]);
            if !undirected.contains_key(&edge) {
                return Err("a crease is on an edge the cage does not have".to_owned());
            }
            if !creased.insert(edge) {
                return Err("an edge of the cage is creased twice".to_owned());
            }
            if crease
                .sharpness
                .is_some_and(|s| !(s.is_finite() && s > 0.0))
            {
                return Err("a crease's sharpness must be greater than zero".to_owned());
            }
        }
        if self.corners.iter().any(|c| !ids.contains(c)) {
            return Err("a corner is a vertex the cage does not have".to_owned());
        }
        if let Some(plane) = &self.symmetry {
            SymmetryPlane::new(plane.origin, plane.normal)?;
            if self
                .vertices
                .iter()
                .any(|v| plane.distance(v.at) < -ON_PLANE)
            {
                return Err(
                    "a vertex of the cage lies on the mirrored side of the symmetry plane"
                        .to_owned(),
                );
            }
            let on_plane = |id: &u32| self.position(*id).is_some_and(|p| plane.holds(p));
            if self.faces.iter().any(|f| f.vertices.iter().all(on_plane)) {
                return Err("a face of the cage lies in the symmetry plane".to_owned());
            }
            for (edge, count) in &undirected {
                if *count != 1 && on_plane(&edge[0]) && on_plane(&edge[1]) {
                    return Err("an edge in the symmetry plane must be on the cut of the \
                                stored half, with a face on one side only"
                        .to_owned());
                }
            }
        }
        Ok(())
    }

    /// The whole cage, the mirrored half added (see [`Full`]). On a cage
    /// that [`Cage::check`] accepts; faces that name a vertex the cage does
    /// not have are left out.
    pub fn full(&self) -> Full {
        let plane = self.symmetry;
        let mut points = Vec::new();
        let mut vertex_ids = Vec::new();
        let mut index: HashMap<u32, usize> = HashMap::new();
        for v in &self.vertices {
            let at = match &plane {
                Some(plane) if plane.holds(v.at) => plane.project(v.at),
                _ => v.at,
            };
            index.insert(v.id, points.len());
            points.push(at);
            vertex_ids.push(i64::from(v.id));
        }
        let mut mirror: HashMap<u32, usize> = HashMap::new();
        if let Some(plane) = &plane {
            for v in &self.vertices {
                if plane.holds(v.at) {
                    mirror.insert(v.id, index[&v.id]);
                } else {
                    mirror.insert(v.id, points.len());
                    points.push(plane.mirror(v.at));
                    vertex_ids.push(-i64::from(v.id));
                }
            }
        }
        let mut faces = Vec::new();
        let mut face_ids = Vec::new();
        let mut corner_numbers = Vec::new();
        for face in &self.faces {
            let Some(lp) = face
                .vertices
                .iter()
                .map(|v| index.get(v).copied())
                .collect::<Option<Vec<usize>>>()
            else {
                continue;
            };
            let n = lp.len();
            faces.push(lp);
            face_ids.push(i64::from(face.id));
            corner_numbers.push((0..n as u32).collect());
        }
        if plane.is_some() {
            for face in &self.faces {
                let n = face.vertices.len();
                let Some(lp) = (0..n)
                    .map(|p| mirror.get(&face.vertices[(n - p) % n]).copied())
                    .collect::<Option<Vec<usize>>>()
                else {
                    continue;
                };
                faces.push(lp);
                face_ids.push(-i64::from(face.id));
                corner_numbers.push((0..n).map(|p| ((n - p) % n) as u32).collect());
            }
        }
        let mut creases = HashMap::new();
        for crease in &self.creases {
            let [a, b] = crease.edge;
            if let (Some(&ia), Some(&ib)) = (index.get(&a), index.get(&b)) {
                creases.insert(key(ia, ib), crease.value());
            }
            if let (Some(&ma), Some(&mb)) = (mirror.get(&a), mirror.get(&b)) {
                creases.insert(key(ma, mb), crease.value());
            }
        }
        let mut corners = vec![false; points.len()];
        for c in &self.corners {
            if let Some(&i) = index.get(c) {
                corners[i] = true;
            }
            if let Some(&m) = mirror.get(c) {
                corners[m] = true;
            }
        }
        Full {
            mesh: Mesh {
                points,
                faces,
                creases,
                corners,
            },
            vertex_ids,
            face_ids,
            corner_numbers,
        }
    }
}

/// The whole cage as a mesh: the stored vertices and faces, then (with a
/// symmetry plane) the mirrored ones, with the ids they have in the whole
/// cage (negative for mirror images).
#[derive(Debug, Clone)]
pub struct Full {
    pub mesh: Mesh,
    /// Each mesh vertex's id.
    pub vertex_ids: Vec<i64>,
    /// Each mesh face's id.
    pub face_ids: Vec<i64>,
    /// For each mesh face, the number each of its corners has in the stored
    /// face's loop (a mirrored face runs the other way round).
    pub corner_numbers: Vec<Vec<u32>>,
}

impl Full {
    /// Whether the cage is closed: every edge between two faces.
    pub fn closed(&self) -> bool {
        Topology::of(&self.mesh).is_closed()
    }

    /// The mesh index of a vertex by its id in the whole cage.
    pub fn vertex_index(&self, id: i64) -> Option<usize> {
        self.vertex_ids.iter().position(|v| *v == id)
    }

    /// The mesh index of a face by its id in the whole cage.
    pub fn face_index(&self, id: i64) -> Option<usize> {
        self.face_ids.iter().position(|f| *f == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::box_cage;

    #[test]
    fn a_box_cage_is_valid_and_closed() {
        let cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        cage.check().unwrap();
        assert_eq!(cage.vertices.len(), 8);
        assert_eq!(cage.faces.len(), 6);
        let full = cage.full();
        assert!(full.closed());
        // Every face's normal points away from the centre.
        for face in &cage.faces {
            let (normal, center) = cage.face_normal(face);
            assert!(dot(normal, center) > 0.0, "{face:?}");
        }
    }

    #[test]
    fn check_refuses_broken_cages() {
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        cage.faces[0].vertices.reverse();
        let error = cage.check().unwrap_err();
        assert!(error.contains("same way"), "{error}");
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        cage.faces[0].vertices.truncate(2);
        assert!(cage.check().unwrap_err().contains("three corners"));
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        cage.creases.push(Crease {
            edge: [1, 7],
            sharpness: None,
        });
        assert!(cage.check().is_err());
        let mut cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [1, 1, 1]).unwrap();
        cage.vertices.push(CageVertex {
            id: 99,
            at: [0.0; 3],
        });
        assert!(cage.check().unwrap_err().contains("no face"));
    }

    #[test]
    fn the_full_cage_mirrors_the_stored_half() {
        let cage = box_cage([0.0; 3], [20.0, 20.0, 20.0], [2, 1, 1]).unwrap();
        let mut symmetric = cage.clone();
        crate::edit::set_symmetry(
            &mut symmetric,
            Some(SymmetryPlane::new([0.0; 3], [1.0, 0.0, 0.0]).unwrap()),
        )
        .unwrap();
        symmetric.check().unwrap();
        assert_eq!(symmetric.faces.len(), 5);
        let full = symmetric.full();
        assert!(full.closed());
        assert_eq!(full.mesh.faces.len(), 10);
        assert_eq!(full.mesh.points.len(), cage.vertices.len());
        // Every mirrored vertex is the mirror image of its stored one.
        for (i, id) in full.vertex_ids.iter().enumerate() {
            if *id < 0 {
                let stored = full.vertex_index(-id).unwrap();
                let [x, y, z] = full.mesh.points[stored];
                assert_eq!(full.mesh.points[i], [-x, y, z]);
            }
        }
    }
}
