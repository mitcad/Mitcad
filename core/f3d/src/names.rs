// SPDX-License-Identifier: MIT
//! Names of B-rep entities in the ASM blobs, and the entities that feature
//! inputs name.
//!
//! Faces and bodies carry `ATTRIB_CUSTOM-attrib` records named
//! `generic_tag_attrib_def` *(verified on the corpus and the reference
//! models)*:
//!
//! ```text
//! "generic_tag_attrib_def" 3 3 -1 "generic_tag_attrib_def " n
//!   n × name: int type | str tag | int kind | int c | c × int op | int 0
//! ```
//!
//! `type` is 1 for faces and 3 for bodies (other values occur, meaning
//! open); `tag` is chosen by the operation that made the entity (a face of
//! an extrusion: `"1"` its start, `"2"` its end, `"3"`... its sides; a
//! body: the number of the operation that made it); `ops` are the ASM
//! state numbers of the operations that made and changed it. A face split
//! or merged by later operations carries several names.
//!
//! The design streams name a feature's inputs with the same names
//! ([`crate::design::recipe`]): an edge by the two faces along it and the
//! faces at its ends. [`NamedState`] finds them among the faces of a history
//! state, and [`NamedState::edge`] gives the edge's geometry.

use std::collections::{HashMap, HashSet};

use crate::asm::file::AsmFile;
use crate::asm::geom::Cursor;
use crate::asm::token::Token;
use crate::brep::{self, Curve, P3, Surface};
use crate::convert::{self, Options};
use crate::design::ir::EntityName;

mod topology;

/// A name on an entity of a blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsmName {
    /// 1 for faces, 3 for bodies.
    pub kind_type: i64,
    pub tag: String,
    pub kind: i64,
    pub ops: Vec<i64>,
}

impl AsmName {
    /// The same name as a recipe's: the tag and the operations.
    pub fn is(&self, name: &EntityName) -> bool {
        self.tag == name.tag && self.ops == name.ops
    }
}

/// The names in a `generic_tag_attrib_def` record's fields, if it is one.
pub fn attribute_names(toks: &[Token]) -> Option<Vec<AsmName>> {
    let start = toks
        .iter()
        .position(|t| matches!(t, Token::Str(s) if s == "generic_tag_attrib_def "))?;
    let mut c = Cursor::new(toks, start + 1);
    let n = c.int().ok()?;
    if !(0..=100_000).contains(&n) {
        return None;
    }
    let mut out = Vec::new();
    for _ in 0..n {
        let kind_type = c.int().ok()?;
        let tag = c.string().ok()?.to_owned();
        let kind = c.int().ok()?;
        let count = c.int().ok()?;
        if !(0..=100_000).contains(&count) {
            return None;
        }
        let mut ops = Vec::with_capacity(count as usize);
        for _ in 0..count {
            ops.push(c.int().ok()?);
        }
        c.int().ok()?;
        out.push(AsmName {
            kind_type,
            tag,
            kind,
            ops,
        });
    }
    Some(out)
}

/// Reads the topology of a blob at a history state.
struct Walker<'a> {
    file: &'a AsmFile,
    view: Option<&'a HashMap<usize, Option<usize>>>,
}

impl<'a> Walker<'a> {
    /// The record holding an entity's data at the state (`None`: the entity
    /// does not exist then).
    fn data(&self, entity: usize) -> Option<usize> {
        match self.view.and_then(|v| v.get(&entity)) {
            None => Some(entity),
            Some(r) => *r,
        }
    }

    /// The entity's fields after the entity header (attribute, history id,
    /// one more pointer).
    fn cursor(&self, entity: usize) -> Option<Cursor<'a>> {
        let r = self.data(entity)?;
        Some(Cursor::new(
            &self.file.tokens,
            self.file.records[r].fields.start + 3,
        ))
    }

    fn base_type(&self, entity: usize) -> Option<&'a str> {
        Some(self.file.records[self.data(entity)?].base_type())
    }

    fn ptr(&self, c: &mut Cursor) -> Option<Option<usize>> {
        let p = c.ptr().ok()?;
        Some(if p < 0 { None } else { self.file.record_of(p) })
    }

    /// The names of an entity: its `generic_tag_attrib_def` attributes.
    fn names(&self, entity: usize) -> Vec<AsmName> {
        let mut out = Vec::new();
        let Some(r) = self.data(entity) else {
            return out;
        };
        let mut c = Cursor::new(&self.file.tokens, self.file.records[r].fields.start);
        let mut at = self.ptr(&mut c).flatten();
        let mut seen = HashSet::new();
        while let Some(a) = at {
            if !seen.insert(a) {
                break;
            }
            let Some(ar) = self.data(a) else { break };
            let rec = &self.file.records[ar];
            if rec.base_type() != "attrib" {
                break;
            }
            let fields = self.file.fields(ar);
            if let Some(names) = attribute_names(fields) {
                out.extend(names);
            }
            // attribute, history id, next, previous, owner.
            let mut c = Cursor::new(&self.file.tokens, rec.fields.start + 2);
            at = self.ptr(&mut c).flatten();
        }
        out
    }

    /// A linked list from `first` along each entity's `next` (its first
    /// field after the header).
    fn list(&self, first: Option<usize>, kind: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut at = first;
        let mut seen = HashSet::new();
        while let Some(e) = at {
            if !seen.insert(e) || self.base_type(e) != Some(kind) {
                break;
            }
            out.push(e);
            at = self.cursor(e).and_then(|mut c| self.ptr(&mut c)).flatten();
        }
        out
    }
}

/// A face of a history state.
#[derive(Clone, Debug)]
pub struct NamedFace {
    pub entity: usize,
    /// Index into [`NamedState::bodies`].
    pub body: usize,
    pub names: Vec<AsmName>,
}

/// An edge of a history state.
#[derive(Clone, Debug)]
pub struct NamedEdge {
    pub entity: usize,
    pub body: usize,
    /// Indices into [`NamedState::faces`], once each.
    pub faces: Vec<usize>,
    /// For each of `faces`: its coedge runs against the edge.
    pub reversed: Vec<bool>,
    /// The vertex entities at its start and end.
    pub vertices: [Option<usize>; 2],
    /// Its own names, where the blob gives it some (mitcad#96).
    pub names: Vec<AsmName>,
}

/// A body of a history state.
#[derive(Clone, Debug)]
pub struct NamedBody {
    pub record: usize,
    pub names: Vec<AsmName>,
}

/// The bodies, faces and edges of a blob at a history state, with names.
pub struct NamedState<'a> {
    file: &'a AsmFile,
    view: Option<&'a HashMap<usize, Option<usize>>>,
    pub bodies: Vec<NamedBody>,
    pub faces: Vec<NamedFace>,
    pub edges: Vec<NamedEdge>,
}

/// The geometry of an edge in millimetres, in its component's coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeGeometry {
    /// At the middle of its parameter range.
    pub mid: P3,
    pub length: f64,
    pub start: P3,
    pub end: P3,
    /// The unit tangent at `mid` along the edge's own direction (from its
    /// start vertex to its end vertex: the face whose coedge is not
    /// reversed lies on its left, seen against the face's normal).
    pub direction: P3,
}

/// A cylindrical face's geometry in millimetres, in its component's
/// coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct CylinderGeometry {
    /// A point on the axis.
    pub origin: P3,
    /// The axis' unit direction, as the file's surface has it.
    pub axis: P3,
    pub radius: f64,
    /// The face's normal points to the axis (a hole's wall).
    pub internal: bool,
    /// A point inside the face, and the face's normal there.
    pub point: P3,
    pub normal: P3,
}

/// A planar face's plane (mm, in its component's coordinates): a point
/// in it (the middle of its boundary, projected onto the plane) and its
/// normal with the face's orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneGeometry {
    pub point: P3,
    pub normal: P3,
}

impl<'a> NamedState<'a> {
    /// The state of a blob: `view` from [`crate::asm::history::History::view`]
    /// (`None`: the stored state).
    pub fn new(file: &'a AsmFile, view: Option<&'a HashMap<usize, Option<usize>>>) -> Self {
        let w = Walker { file, view };
        let mut state = NamedState {
            file,
            view,
            bodies: Vec::new(),
            faces: Vec::new(),
            edges: Vec::new(),
        };
        let mut edge_index: HashMap<usize, usize> = HashMap::new();
        for (record, _) in convert::body_records(file) {
            if w.data(record).is_none() {
                continue;
            }
            let body = state.bodies.len();
            state.bodies.push(NamedBody {
                record,
                names: w.names(record),
            });
            let Some(mut c) = w.cursor(record) else {
                continue;
            };
            let first_lump = w.ptr(&mut c).flatten();
            for lump in w.list(first_lump, "lump") {
                let Some(mut c) = w.cursor(lump) else {
                    continue;
                };
                let _next = w.ptr(&mut c);
                let first_shell = w.ptr(&mut c).flatten();
                for shell in w.list(first_shell, "shell") {
                    let Some(mut c) = w.cursor(shell) else {
                        continue;
                    };
                    let _next = w.ptr(&mut c);
                    let _subshell = w.ptr(&mut c);
                    let first_face = w.ptr(&mut c).flatten();
                    for face in w.list(first_face, "face") {
                        let fi = state.faces.len();
                        state.faces.push(NamedFace {
                            entity: face,
                            body,
                            names: w.names(face),
                        });
                        state.face_edges(&w, face, fi, body, &mut edge_index);
                    }
                }
            }
        }
        state
    }

    fn face_edges(
        &mut self,
        w: &Walker,
        face: usize,
        fi: usize,
        body: usize,
        edge_index: &mut HashMap<usize, usize>,
    ) {
        let Some(mut c) = w.cursor(face) else {
            return;
        };
        let _next = w.ptr(&mut c);
        let first_loop = w.ptr(&mut c).flatten();
        for lp in w.list(first_loop, "loop") {
            let Some(mut c) = w.cursor(lp) else {
                continue;
            };
            let _next = w.ptr(&mut c);
            let first = w.ptr(&mut c).flatten();
            let mut at = first;
            let mut seen = HashSet::new();
            while let Some(ce) = at {
                if !seen.insert(ce) || w.base_type(ce) != Some("coedge") {
                    break;
                }
                let Some(mut c) = w.cursor(ce) else { break };
                let next = w.ptr(&mut c).flatten();
                let _prev = w.ptr(&mut c);
                let _partner = w.ptr(&mut c);
                if let Some(Some(edge)) = w.ptr(&mut c) {
                    let reversed = c.boolean().unwrap_or(false);
                    let ei = *edge_index.entry(edge).or_insert_with(|| {
                        let vertices = w
                            .cursor(edge)
                            .and_then(|mut c| {
                                let a = w.ptr(&mut c)?;
                                c.double().ok()?;
                                let b = w.ptr(&mut c)?;
                                Some([a, b])
                            })
                            .unwrap_or([None, None]);
                        self.edges.push(NamedEdge {
                            entity: edge,
                            body,
                            faces: Vec::new(),
                            reversed: Vec::new(),
                            vertices,
                            names: w.names(edge),
                        });
                        self.edges.len() - 1
                    });
                    let e = &mut self.edges[ei];
                    if !e.faces.contains(&fi) {
                        e.faces.push(fi);
                        e.reversed.push(reversed);
                    }
                }
                at = next;
                if at == first {
                    break;
                }
            }
        }
    }

    /// The faces one entity of a recipe names: those with the most of its
    /// names (at least one).
    pub fn faces_named(&self, names: &[EntityName]) -> Vec<usize> {
        let score = |f: &NamedFace| {
            names
                .iter()
                .filter(|n| f.names.iter().any(|m| m.is(n)))
                .count()
        };
        let best = self.faces.iter().map(score).max().unwrap_or(0);
        if best == 0 {
            return Vec::new();
        }
        (0..self.faces.len())
            .filter(|&i| score(&self.faces[i]) == best)
            .collect()
    }

    /// The faces at an edge's two ends, in the direction its coedge on
    /// `face` (an index into [`Self::faces`]) runs (the edge's own
    /// direction when `face` is not one of its faces).
    pub fn ends(&self, edge: usize, face: Option<usize>) -> [HashSet<usize>; 2] {
        let e = &self.edges[edge];
        let flip = face
            .and_then(|f| e.faces.iter().position(|&x| x == f))
            .is_some_and(|k| e.reversed[k]);
        let (a, b) = (self.faces_at(e.vertices[0]), self.faces_at(e.vertices[1]));
        if flip { [b, a] } else { [a, b] }
    }

    /// Ordered faces around the edge's endpoints, for diagnostics (mitcad#106).
    ///
    /// `edge` and `face` index [`Self::edges`] and [`Self::faces`]. Endpoints
    /// follow the coedge on `face`; each cycle starts with `face`, then the
    /// other face along the edge, then the remaining incident faces. Order
    /// follows partner and loop links, without a geometric angle sort.
    /// Boundaries, seams, nonmanifold or incomplete vertex fans fail.
    ///
    /// This proves a topological order only. Its relationship to opaque
    /// recipe tails is unverified and must not decide an edge match.
    pub fn edge_endpoint_face_cycles(
        &self,
        edge: usize,
        face: usize,
    ) -> Result<[Vec<usize>; 2], String> {
        topology::endpoint_face_cycles(self, edge, face)
    }

    /// The faces at a vertex: those of the edges that end at it.
    fn faces_at(&self, vertex: Option<usize>) -> HashSet<usize> {
        let Some(v) = vertex else {
            return HashSet::new();
        };
        self.edges
            .iter()
            .filter(|e| e.vertices.contains(&Some(v)))
            .flat_map(|e| e.faces.iter().copied())
            .collect()
    }

    /// The edges an edge recipe names (indices into [`Self::edges`], in
    /// their order; one, unless the names fit several equally): along
    /// faces named by its first two entities; when several are, those whose
    /// ends touch faces named by the others most. `Err` says why there is
    /// none.
    ///
    /// A first entity named with a negative tag is the edge itself, and the
    /// faces follow it (mitcad#96: `[edge, face, face, end faces...]`):
    /// the edge with that name when the blob names its edges, else by the
    /// faces after it.
    pub fn edges_fitting(&self, entities: &[Vec<EntityName>]) -> Result<Vec<usize>, String> {
        if let Some((own, faces)) = entities.split_first()
            && own.iter().any(|n| n.tag.starts_with('-'))
        {
            let named: HashSet<usize> = (0..self.edges.len())
                .filter(|&i| {
                    let e = &self.edges[i];
                    own.iter().any(|n| e.names.iter().any(|m| m.is(n)))
                })
                .collect();
            return self
                .edges_between(faces, (!named.is_empty()).then_some(&named))
                .or_else(|e| self.edges_between(faces, None).map_err(|_| e));
        }
        self.edges_between(entities, None)
    }

    /// The edge an edge recipe names, as [`Self::edges_fitting`] when one
    /// fits.
    pub fn find_edge(&self, entities: &[Vec<EntityName>]) -> Result<usize, String> {
        match self.edges_fitting(entities)?[..] {
            [i] => Ok(i),
            ref top => Err(format!("{} edges fit the names", top.len())),
        }
    }

    /// The edges between the faces the first two entities name (of
    /// `among`, when given), as [`Self::edges_fitting`].
    fn edges_between(
        &self,
        entities: &[Vec<EntityName>],
        among: Option<&HashSet<usize>>,
    ) -> Result<Vec<usize>, String> {
        if entities.len() < 2 {
            return Err("the recipe names fewer than two faces".to_owned());
        }
        let a: HashSet<usize> = self.faces_named(&entities[0]).into_iter().collect();
        let b: HashSet<usize> = self.faces_named(&entities[1]).into_iter().collect();
        if a.is_empty() || b.is_empty() {
            return Err("a face of the edge is not in the history state".to_owned());
        }
        let along: Vec<usize> = (0..self.edges.len())
            .filter(|&i| {
                let f = &self.edges[i].faces;
                f.len() == 2
                    && ((a.contains(&f[0]) && b.contains(&f[1]))
                        || (b.contains(&f[0]) && a.contains(&f[1])))
                    && among.is_none_or(|s| s.contains(&i))
            })
            .collect();
        match along.len() {
            0 => return Err("no edge between the faces named".to_owned()),
            1 => return Ok(along),
            _ => {}
        }
        // Several: the faces at the ends, the third entity's at the edge's
        // start and the fourth's at its end *(so on the corpus: 1870 of the
        // 2049 edges found otherwise, 8 the other way round)*, then any
        // others at either end.
        let ends: Vec<HashSet<usize>> = entities[2..]
            .iter()
            .map(|e| self.faces_named(e).into_iter().collect())
            .collect();
        let score = |i: usize| {
            let [start, end] = self.ends(i, None);
            let at: HashSet<usize> = start.union(&end).copied().collect();
            let oriented = [&start, &end]
                .iter()
                .zip(&ends)
                .filter(|(at, e)| !e.is_disjoint(at))
                .count();
            (
                oriented,
                ends.iter().filter(|s| !s.is_disjoint(&at)).count(),
            )
        };
        let best = along.iter().map(|&i| score(i)).max().unwrap_or((0, 0));
        Ok(along.into_iter().filter(|&i| score(i) == best).collect())
    }

    /// The edges of a face (indices into [`Self::edges`]) between it and
    /// another face: those a fillet of the face rounds (mitcad#96), no
    /// seams.
    pub fn edges_of_face(&self, face: usize) -> Vec<usize> {
        (0..self.edges.len())
            .filter(|&i| {
                let f = &self.edges[i].faces;
                f.len() == 2 && f.contains(&face)
            })
            .collect()
    }

    /// The face a face recipe names (index into [`Self::faces`]): one named
    /// by its first entity; when several are, the one next to the most
    /// faces the others name (a `bounded_face` recipe lists the faces
    /// around it). `Err` says why there is none.
    pub fn find_face(&self, entities: &[Vec<EntityName>]) -> Result<usize, String> {
        let names = entities.first().ok_or("the recipe names no face")?;
        let named = self.faces_named(names);
        match named.len() {
            0 => return Err("the face is not in the history state".to_owned()),
            1 => return Ok(named[0]),
            _ => {}
        }
        let around: Vec<HashSet<usize>> = entities[1..]
            .iter()
            .map(|e| self.faces_named(e).into_iter().collect())
            .collect();
        let score = |f: usize| {
            let next: HashSet<usize> = self
                .edges
                .iter()
                .filter(|e| e.faces.contains(&f))
                .flat_map(|e| e.faces.iter().copied())
                .filter(|&g| g != f)
                .collect();
            around.iter().filter(|s| !s.is_disjoint(&next)).count()
        };
        let best = named.iter().map(|&f| score(f)).max().unwrap_or(0);
        let top: Vec<usize> = named.into_iter().filter(|&f| score(f) == best).collect();
        match top[..] {
            [f] => Ok(f),
            _ => Err(format!("{} faces fit the names", top.len())),
        }
    }

    /// A cylindrical face's geometry (mm, in its component's coordinates):
    /// its axis, radius, side and a point inside it. `Err` for other faces.
    pub fn cylinder(&self, index: usize) -> Result<CylinderGeometry, String> {
        let f = &self.faces[index];
        let body = self.bodies[f.body].record;
        let face = convert::face_surface(
            self.file,
            body,
            f.entity,
            &Options::default(),
            self.view,
            64,
        )
        .map_err(|e| e.0)?;
        let Surface::Cone {
            origin,
            axis,
            ref_dir,
            radius,
            half_angle,
        } = face.surface
        else {
            return Err(format!(
                "the face is a {}, not a cylinder",
                face.surface.kind()
            ));
        };
        if half_angle != 0.0 {
            return Err("the face is a cone, not a cylinder".to_owned());
        }
        if face.boundary.is_empty() {
            return Err("the face has no edges".to_owned());
        }
        // Where the edges run along the axis and round it: the point is
        // half way along, on the far side of the widest gap between the
        // edges' angles (a part of a cylinder), else in the widest gap.
        let x = ref_dir;
        let y = brep::cross(axis, x);
        let mut heights = Vec::new();
        let mut angles = Vec::new();
        for &p in &face.boundary {
            let d = brep::sub(p, origin);
            heights.push(brep::dot(d, axis));
            angles.push(brep::dot(d, y).atan2(brep::dot(d, x)));
        }
        let low = heights.iter().copied().fold(f64::INFINITY, f64::min);
        let high = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        angles.sort_by(f64::total_cmp);
        let tau = std::f64::consts::TAU;
        let last = angles[angles.len() - 1];
        let mut gap = angles[0] + tau - last;
        let mut at = last + gap / 2.0;
        for w in angles.windows(2) {
            if w[1] - w[0] > gap {
                gap = w[1] - w[0];
                at = (w[0] + w[1]) / 2.0;
            }
        }
        if gap > std::f64::consts::FRAC_PI_6 {
            at += std::f64::consts::PI;
        }
        let radial = brep::add(brep::scale(x, at.cos()), brep::scale(y, at.sin()));
        let local = brep::add(
            brep::add(origin, brep::scale(axis, (low + high) / 2.0)),
            brep::scale(radial, radius),
        );
        let normal = if face.reversed {
            brep::scale(radial, -1.0)
        } else {
            radial
        };
        let mut out = CylinderGeometry {
            origin,
            axis,
            radius,
            internal: face.reversed,
            point: local,
            normal,
        };
        if let Some(tr) = &face.transform {
            let rotate = |p: P3| {
                [
                    brep::dot(tr.m[0], p),
                    brep::dot(tr.m[1], p),
                    brep::dot(tr.m[2], p),
                ]
            };
            out.origin = brep::add(rotate(origin), tr.t);
            out.point = brep::add(rotate(local), tr.t);
            out.axis = brep::normalize(rotate(axis));
            out.normal = brep::normalize(rotate(normal));
            out.radius = radius * brep::norm(tr.m[0]);
        }
        Ok(out)
    }

    /// A planar face's plane. `Err` for other faces. The point is the mean
    /// of the boundary's points projected onto the plane, which lies in a
    /// convex face (mirror planes and split tools, mitcad#67).
    pub fn plane(&self, index: usize) -> Result<PlaneGeometry, String> {
        let f = &self.faces[index];
        let body = self.bodies[f.body].record;
        let face = convert::face_surface(
            self.file,
            body,
            f.entity,
            &Options::default(),
            self.view,
            16,
        )
        .map_err(|e| e.0)?;
        let Surface::Plane { origin, normal, .. } = face.surface else {
            return Err(format!(
                "the face is a {}, not a plane",
                face.surface.kind()
            ));
        };
        if face.boundary.is_empty() {
            return Err("the face has no edges".to_owned());
        }
        let n = face.boundary.len() as f64;
        let mean = face
            .boundary
            .iter()
            .fold([0.0; 3], |a, p| brep::add(a, brep::scale(*p, 1.0 / n)));
        let point = brep::sub(
            mean,
            brep::scale(normal, brep::dot(brep::sub(mean, origin), normal)),
        );
        let normal = if face.reversed {
            brep::scale(normal, -1.0)
        } else {
            normal
        };
        let mut out = PlaneGeometry { point, normal };
        if let Some(tr) = &face.transform {
            let rotate = |p: P3| {
                [
                    brep::dot(tr.m[0], p),
                    brep::dot(tr.m[1], p),
                    brep::dot(tr.m[2], p),
                ]
            };
            out.point = brep::add(rotate(point), tr.t);
            out.normal = brep::normalize(rotate(normal));
        }
        Ok(out)
    }

    /// The body a body recipe names (index into [`Self::bodies`]): the one
    /// with the most of its tags among its body names (type 3).
    pub fn find_body(&self, entities: &[Vec<EntityName>]) -> Result<usize, String> {
        let names = entities.first().ok_or("the recipe names no body")?;
        let score = |b: &NamedBody| {
            names
                .iter()
                .filter(|n| b.names.iter().any(|m| m.kind_type == 3 && m.tag == n.tag))
                .count()
        };
        let best = self.bodies.iter().map(score).max().unwrap_or(0);
        if best == 0 {
            return Err("the body is not in the history state".to_owned());
        }
        let top: Vec<usize> = (0..self.bodies.len())
            .filter(|&i| score(&self.bodies[i]) == best)
            .collect();
        match top[..] {
            [b] => Ok(b),
            _ => Err(format!("{} bodies fit the names", top.len())),
        }
    }

    /// The middle points of up to `n` edges of a body (mm), spread over
    /// its edges.
    pub fn body_points(&self, body: usize, n: usize) -> Vec<P3> {
        let edges: Vec<usize> = (0..self.edges.len())
            .filter(|&i| self.edges[i].body == body)
            .collect();
        let step = edges.len().div_ceil(n.max(1)).max(1);
        edges
            .iter()
            .step_by(step)
            .filter_map(|&i| self.edge(i).ok())
            .map(|g| g.mid)
            .collect()
    }

    /// The geometry of an edge.
    pub fn edge(&self, index: usize) -> Result<EdgeGeometry, String> {
        let e = &self.edges[index];
        let body = self.bodies[e.body].record;
        let (curve, t, transform, along) =
            convert::edge_curve_along(self.file, body, e.entity, &Options::default(), self.view)
                .map_err(|err| err.0)?;
        let apply = |p: P3| match &transform {
            Some(tr) => brep::add(
                [
                    brep::dot(tr.m[0], p),
                    brep::dot(tr.m[1], p),
                    brep::dot(tr.m[2], p),
                ],
                tr.t,
            ),
            None => p,
        };
        let scale = transform.map_or(1.0, |tr| brep::norm(tr.m[0]));
        let length = scale
            * match &curve {
                Curve::Line { .. } => t[1] - t[0],
                Curve::Ellipse { major, minor, .. } if (major - minor).abs() <= 1e-12 * major => {
                    major * (t[1] - t[0])
                }
                _ => {
                    const N: usize = 512;
                    let mut sum = 0.0;
                    let mut last = curve.eval(t[0]);
                    for i in 1..=N {
                        let p = curve.eval(t[0] + (t[1] - t[0]) * i as f64 / N as f64);
                        sum += brep::dist(p, last);
                        last = p;
                    }
                    sum
                }
            };
        let m = 0.5 * (t[0] + t[1]);
        let h = 1e-4 * (t[1] - t[0]);
        let ahead = brep::sub(apply(curve.eval(m + h)), apply(curve.eval(m - h)));
        let direction = brep::normalize(if along {
            ahead
        } else {
            brep::scale(ahead, -1.0)
        });
        Ok(EdgeGeometry {
            mid: apply(curve.eval(m)),
            length,
            start: apply(curve.eval(t[0])),
            end: apply(curve.eval(t[1])),
            direction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    fn n(tag: &str, ops: &[i64]) -> EntityName {
        EntityName {
            tag: tag.to_owned(),
            kind: 0,
            ops: ops.to_vec(),
        }
    }

    #[test]
    fn reads_names_and_finds_edges() {
        let data = testdata::named_cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        assert_eq!(state.bodies.len(), 1);
        assert_eq!(state.bodies[0].names[0].tag, "301");
        assert_eq!(state.bodies[0].names[0].kind_type, 3);
        assert_eq!(state.faces.len(), 6);
        assert_eq!(state.edges.len(), 12);
        assert!(state.edges.iter().all(|e| e.faces.len() == 2));
        // Faces: 1 bottom, 2 top, 3 front (y = 0), 4 right (x = 1).
        let e = state
            .find_edge(&[
                vec![n("3", &[301])],
                vec![n("4", &[301])],
                vec![n("1", &[301])],
                vec![n("2", &[301])],
            ])
            .unwrap();
        let g = state.edge(e).unwrap();
        // The vertical edge at x = 1, y = 0 (cm -> mm).
        assert!(brep::dist(g.mid, [10.0, 0.0, 5.0]) < 1e-9, "{g:?}");
        assert!((g.length - 10.0).abs() < 1e-9);
        // It runs from z = 0 up: the bottom face at its start, the top at
        // its end.
        let bottom = state.faces_named(&[n("1", &[301])]);
        let top = state.faces_named(&[n("2", &[301])]);
        let [start, end] = state.ends(e, None);
        assert!(start.contains(&bottom[0]) && !start.contains(&top[0]));
        assert!(end.contains(&top[0]) && !end.contains(&bottom[0]));
        assert!(brep::dist(g.direction, [0.0, 0.0, 1.0]) < 1e-9, "{g:?}");
        // An edge named first by its own (negative) tag: the faces follow
        // (the cube's edges carry no names of their own).
        assert_eq!(
            state.find_edge(&[
                vec![n("-1029", &[416])],
                vec![n("3", &[301])],
                vec![n("4", &[301])],
                vec![n("1", &[301])],
            ]),
            Ok(e)
        );
        // A face's edges to other faces.
        assert_eq!(state.edges_of_face(bottom[0]).len(), 4);
        assert!(
            state
                .edges_of_face(bottom[0])
                .iter()
                .all(|&i| state.edges[i].faces.len() == 2)
        );
        // The body by its tag, and points on it.
        let b = state.find_body(&[vec![n("301", &[])]]).unwrap();
        assert_eq!(b, 0);
        assert!(state.find_body(&[vec![n("302", &[])]]).is_err());
        let points = state.body_points(b, 4);
        assert_eq!(points.len(), 4);
        assert!(
            points
                .iter()
                .all(|p| p.iter().all(|x| (0.0..=10.0).contains(x)))
        );
        // Order of the faces does not matter; unknown names fail.
        assert_eq!(
            state.find_edge(&[vec![n("4", &[301])], vec![n("3", &[301])]]),
            Ok(e)
        );
        assert!(
            state
                .find_edge(&[vec![n("4", &[302])], vec![n("3", &[301])]])
                .is_err()
        );
        // Faces that do not meet.
        assert!(
            state
                .find_edge(&[vec![n("1", &[301])], vec![n("2", &[301])]])
                .is_err()
        );
    }

    #[test]
    fn parses_attribute_fields() {
        let toks = vec![
            Token::Ptr(-1),
            Token::Int(-1),
            Token::Str("generic_tag_attrib_def".into()),
            Token::Int(3),
            Token::Int(3),
            Token::Int(-1),
            Token::Str("generic_tag_attrib_def ".into()),
            Token::Int(2),
            Token::Int(1),
            Token::Str("5".into()),
            Token::Int(0),
            Token::Int(2),
            Token::Int(304),
            Token::Int(-317),
            Token::Int(0),
            Token::Int(3),
            Token::Str("301".into()),
            Token::Int(7),
            Token::Int(0),
            Token::Int(0),
        ];
        let names = attribute_names(&toks).unwrap();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0].ops, vec![304, -317]);
        assert!(names[0].is(&n("5", &[304, -317])));
        assert!(!names[0].is(&n("5", &[304])));
        assert_eq!(names[1].kind, 7);
        assert_eq!(attribute_names(&toks[..10]), None);
    }
}
