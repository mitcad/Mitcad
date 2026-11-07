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
//! (`mitcad_f3d::design::recipe`): an edge by the two faces along it and the
//! faces at its ends. [`NamedState`] finds them among the faces of a history
//! state, and [`NamedState::edge`] gives the edge's geometry.

use mitcad_asm::ids::{IdMap, IdSet, View};
use std::collections::{HashMap, HashSet};

use mitcad_asm::asm::file::AsmFile;
use mitcad_asm::asm::geom::Cursor;
use mitcad_asm::asm::token::Token;
use mitcad_asm::brep::{self, Curve, P3, Surface};
use mitcad_asm::convert::{self, Options};
use mitcad_dump::EntityName;

// The tail of an edge recipe: the faces round the edge's ends (mitcad#106).
mod recipe_tail;
mod topology;
// Points inside faces of any surface type (mitcad#161).
pub mod surface;
// Sampled helices on the cubic through their samples (mitcad#161).
mod smooth;

pub use recipe_tail::{EdgeTail, edge_tail};

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
    view: Option<&'a View>,
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
        let mut seen = IdSet::default();
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
        let mut seen = IdSet::default();
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
///
/// It owns its view of the state, so that one naming of a state can be
/// kept and shared by every lookup in it (mitcad#246).
pub struct NamedState<'a> {
    file: &'a AsmFile,
    view: Option<View>,
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
    /// The unit direction of the surface's parameter origin (where the
    /// cylinder's seam lies, round its axis).
    pub ref_dir: P3,
    /// The face goes all the way round its axis (not a part of a cylinder).
    pub full: bool,
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
    /// The state of a blob: `view` from [`mitcad_asm::asm::history::History::view`]
    /// (`None`: the stored state).
    pub fn new(file: &'a AsmFile, view: Option<View>) -> Self {
        let mut state = NamedState {
            file,
            view: None,
            bodies: Vec::new(),
            faces: Vec::new(),
            edges: Vec::new(),
        };
        let w = Walker {
            file,
            view: view.as_ref(),
        };
        let mut edge_index: IdMap<usize> = IdMap::default();
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
        state.view = view;
        // (A state may be kept and shared: without the lists' spare room.)
        state.bodies.shrink_to_fit();
        state.faces.shrink_to_fit();
        state.edges.shrink_to_fit();
        state
    }

    /// How large the state is in memory, roughly: its faces, edges and the
    /// entities its view changes (each about the same size).
    pub fn size(&self) -> usize {
        self.faces.len() + self.edges.len() + self.view.as_ref().map_or(0, |v| v.len()) + 1
    }

    fn face_edges(
        &mut self,
        w: &Walker,
        face: usize,
        fi: usize,
        body: usize,
        edge_index: &mut IdMap<usize>,
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
            let mut seen = IdSet::default();
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
    /// The recipe's tail lists the faces round an edge's ends in this order
    /// (EDGE_ORDER.md): the start's cycle reversed, the end's as it is.
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
        self.edges_fitting_ordered(entities, None)
    }

    /// [`Self::edges_fitting`], and where edges still fit equally, the
    /// order in which the recipe's tail lists the faces around the edge's
    /// ends tells them apart (mitcad#106: mirror-image edges between the
    /// same two faces have the same faces at their ends, in opposite
    /// cyclic order). The tail decides only among edges that fit equally
    /// otherwise, and only where its order can be compared with the
    /// topology ([`Self::order_agrees`]).
    pub fn edges_fitting_ordered(
        &self,
        entities: &[Vec<EntityName>],
        tail: Option<&EdgeTail>,
    ) -> Result<Vec<usize>, String> {
        if let Some((own, faces)) = entities.split_first()
            && own.iter().any(|n| n.tag.starts_with('-'))
        {
            let named: HashSet<usize> = (0..self.edges.len())
                .filter(|&i| {
                    let e = &self.edges[i];
                    own.iter().any(|n| e.names.iter().any(|m| m.is(n)))
                })
                .collect();
            // The tail's indices count the edge's own entity too.
            let tail = tail.map(|t| (t, 1));
            return self
                .edges_between(faces, (!named.is_empty()).then_some(&named), tail)
                .or_else(|e| self.edges_between(faces, None, tail).map_err(|_| e));
        }
        self.edges_between(entities, None, tail.map(|t| (t, 0)))
    }

    /// Whether the faces around an edge's ends, in the order its recipe's
    /// tail lists them, are those of the topology: `Some(true)` when both
    /// ends' faces, in cyclic order, are the entities' (`base` entities
    /// precede `entities`; an entity no face of the state has stands for
    /// any), `Some(false)` when they are not, `None` when the topology or
    /// the tail cannot say (a closed edge, a vertex fan that does not
    /// close, an entity the tail names that the list lacks).
    ///
    /// The tail's first fan runs the way against the walk
    /// [`Self::edge_endpoint_face_cycles`] makes at the edge's start, the
    /// second the way with it at the end *(the walks turn opposite ways at
    /// the two ends; 12961 of the 13600 edges of the corpora with names
    /// that all resolve agree, the rest name faces as an earlier state had
    /// them)*.
    fn order_agrees(
        &self,
        edge: usize,
        entities: &[Vec<EntityName>],
        tail: &EdgeTail,
        base: usize,
    ) -> Option<bool> {
        let e = &self.edges[edge];
        // The coedge that runs along the edge: its end vertex is the end.
        let anchor = e.faces[e.reversed.iter().position(|&r| !r)?];
        let cycles = self.edge_endpoint_face_cycles(edge, anchor).ok()?;
        if tail.fans.len() != 2 {
            return None;
        }
        let mut all = true;
        for (v, fan) in tail.fans.iter().enumerate() {
            let mut fan: Vec<&Vec<EntityName>> = fan
                .iter()
                .map(|&i| i.checked_sub(base).and_then(|i| entities.get(i)))
                .collect::<Option<_>>()?;
            if v == 0 {
                fan.reverse();
            }
            let cycle = &cycles[v];
            if fan.len() != cycle.len() {
                return Some(false);
            }
            let k = cycle.len();
            let wild: Vec<bool> = fan.iter().map(|e| self.faces_named(e).is_empty()).collect();
            let fits = |face: usize, entity: &[EntityName]| {
                self.faces[face]
                    .names
                    .iter()
                    .any(|m| entity.iter().any(|n| m.is(n)))
            };
            let turned =
                (0..k).any(|r| (0..k).all(|j| wild[j] || fits(cycle[(j + r) % k], fan[j])));
            all &= turned;
        }
        Some(all)
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
        tail: Option<(&EdgeTail, usize)>,
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
        let top: Vec<usize> = along.into_iter().filter(|&i| score(i) == best).collect();
        let Some((tail, base)) = tail.filter(|_| top.len() > 1) else {
            return Ok(top);
        };
        // Equally good: those whose faces around the ends are in the
        // tail's order, else those it cannot be compared for.
        let order: Vec<Option<bool>> = top
            .iter()
            .map(|&i| self.order_agrees(i, entities, tail, base))
            .collect();
        let keep = |want: Option<bool>| -> Vec<usize> {
            top.iter()
                .zip(&order)
                .filter(|(_, o)| **o == want)
                .map(|(&i, _)| i)
                .collect()
        };
        Ok([keep(Some(true)), keep(None)]
            .into_iter()
            .find(|kept| !kept.is_empty())
            .unwrap_or(top))
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

    /// How many of the faces a face recipe lists after its first (those
    /// around the face, `entities[1..]`) are next to face `f`.
    pub fn faces_around(&self, f: usize, entities: &[Vec<EntityName>]) -> usize {
        let next: HashSet<usize> = self
            .edges
            .iter()
            .filter(|e| e.faces.contains(&f))
            .flat_map(|e| e.faces.iter().copied())
            .filter(|&g| g != f)
            .collect();
        entities
            .iter()
            .skip(1)
            .filter(|e| self.faces_named(e).into_iter().any(|g| next.contains(&g)))
            .count()
    }

    /// [`Self::find_face`], and where several faces fit the names, the
    /// first of them when all lie on one surface (the same cylinder or the
    /// same plane: a face the history split in pieces that keep its
    /// names), for what needs the surface alone.
    pub fn find_face_surface(&self, entities: &[Vec<EntityName>]) -> Result<usize, String> {
        let error = match self.find_face(entities) {
            Ok(i) => return Ok(i),
            Err(e) => e,
        };
        let named = self.faces_named(entities.first().ok_or("the recipe names no face")?);
        let Some((&first, rest)) = named.split_first() else {
            return Err(error);
        };
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * (1.0 + a.abs().max(b.abs()));
        let parallel = |a: P3, b: P3| brep::norm(brep::cross(a, b)) <= 1e-9;
        let on_line = |p: P3, o: P3, d: P3| brep::norm(brep::cross(brep::sub(p, o), d)) <= 1e-6;
        let same = if let Ok(a) = self.cylinder(first) {
            rest.iter().all(|&i| {
                self.cylinder(i).is_ok_and(|b| {
                    close(a.radius, b.radius)
                        && parallel(a.axis, b.axis)
                        && on_line(b.origin, a.origin, a.axis)
                })
            })
        } else if let Ok(a) = self.plane(first) {
            rest.iter().all(|&i| {
                self.plane(i).is_ok_and(|b| {
                    parallel(a.normal, b.normal)
                        && brep::dot(brep::sub(b.point, a.point), a.normal).abs() <= 1e-6
                })
            })
        } else {
            false
        };
        if same { Ok(first) } else { Err(error) }
    }

    /// The vertex a vertex recipe names (a vertex entity of the history
    /// state, with the index of the body it lies on): the one at which
    /// faces meet that the recipe's entities name, one face of each
    /// entity *(verified on the reference models: a corner of a block
    /// names its three faces, the vertex of a cylinder's top edge its two)*.
    /// With several equally good, the one with the fewest other faces;
    /// `Err` says why there is none.
    pub fn find_vertex(&self, entities: &[Vec<EntityName>]) -> Result<(usize, usize), String> {
        let named: Vec<HashSet<usize>> = entities
            .iter()
            .map(|e| self.faces_named(e).into_iter().collect::<HashSet<_>>())
            .filter(|s| !s.is_empty())
            .collect();
        if named.is_empty() {
            return Err("the faces at the vertex are not in the history state".to_owned());
        }
        let mut at: HashMap<usize, (usize, HashSet<usize>)> = HashMap::new();
        for e in &self.edges {
            for v in e.vertices.iter().flatten() {
                let entry = at.entry(*v).or_insert_with(|| (e.body, HashSet::new()));
                entry.1.extend(e.faces.iter().copied());
            }
        }
        let score = |faces: &HashSet<usize>| {
            (
                named.iter().filter(|s| !s.is_disjoint(faces)).count(),
                std::cmp::Reverse(faces.len()),
            )
        };
        let best = at.values().map(|(_, f)| score(f)).max();
        let Some(best) = best.filter(|b| b.0 == named.len()) else {
            return Err("no vertex lies at all the faces named".to_owned());
        };
        let mut top: Vec<(usize, usize)> = at
            .iter()
            .filter(|(_, (_, f))| score(f) == best)
            .map(|(v, (b, _))| (*v, *b))
            .collect();
        top.sort_unstable();
        match top[..] {
            [v] => Ok(v),
            _ => Err(format!("{} vertices fit the names", top.len())),
        }
    }

    /// A vertex's position (mm, in its component's coordinates), as found
    /// by [`Self::find_vertex`].
    pub fn vertex(&self, vertex: usize, body: usize) -> Result<P3, String> {
        convert::vertex_position(
            self.file,
            self.bodies[body].record,
            vertex,
            &Options::default(),
            self.view.as_ref(),
        )
        .map_err(|e| e.0)
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
            self.view.as_ref(),
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
            ref_dir: x,
            full: gap <= std::f64::consts::FRAC_PI_6,
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
            out.ref_dir = brep::normalize(rotate(x));
            out.normal = brep::normalize(rotate(normal));
            out.radius = radius * brep::norm(tr.m[0]);
        }
        Ok(out)
    }

    /// A planar face's plane. `Err` for other faces. The point is the mean
    /// of the boundary's points projected onto the plane where that lies in
    /// the face (a convex face: mirror planes and split tools, mitcad#67),
    /// else a point inside the face ([`point_inside`]: a face with a hole
    /// round its middle, mitcad#158).
    pub fn plane(&self, index: usize) -> Result<PlaneGeometry, String> {
        let f = &self.faces[index];
        let body = self.bodies[f.body].record;
        let face = convert::face_surface(
            self.file,
            body,
            f.entity,
            &Options::default(),
            self.view.as_ref(),
            PLANE_SAMPLES,
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
        let edges: Vec<&[P3]> = face.boundary.chunks(PLANE_SAMPLES).collect();
        let point = point_inside(normal, &edges, point);
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

    /// A face of any other surface type than planes and cylinders
    /// (mitcad#161): its surface's type and analytic data and a point
    /// inside it (mm, in its component's coordinates; [`surface`]).
    pub fn surface_point(&self, index: usize) -> Result<SurfacePoint, String> {
        let f = &self.faces[index];
        let body = self.bodies[f.body].record;
        let face = convert::face_surface(
            self.file,
            body,
            f.entity,
            &Options::default(),
            self.view.as_ref(),
            SURFACE_SAMPLES,
        )
        .map_err(|e| e.0)?;
        let edges: Vec<&[P3]> = face.boundary.chunks(SURFACE_SAMPLES).collect();
        let inside = surface::point_inside_face(&face.surface, face.reversed, &edges)?;
        let mut out = SurfacePoint {
            kind: match &face.surface {
                Surface::Plane { .. } => "Plane",
                Surface::Cone { half_angle, .. } if *half_angle == 0.0 => "Cylinder",
                Surface::Cone { .. } => "Cone",
                Surface::Sphere { .. } => "Sphere",
                Surface::Torus { .. } => "Torus",
                _ => "NurbsSurface",
            },
            point: inside.point,
            normal: inside.normal,
            origin: None,
            axis: None,
            radius: None,
            half_angle: None,
            minor: None,
            spread: inside.spread,
        };
        match face.surface {
            Surface::Cone {
                origin,
                axis,
                radius,
                half_angle,
                ..
            } => {
                (out.origin, out.axis, out.radius) = (Some(origin), Some(axis), Some(radius));
                out.half_angle = Some(half_angle);
            }
            Surface::Sphere { center, radius, .. } => {
                (out.origin, out.radius) = (Some(center), Some(radius));
            }
            Surface::Torus {
                center,
                axis,
                major,
                minor,
                ..
            } => {
                (out.origin, out.axis) = (Some(center), Some(axis));
                (out.radius, out.minor) = (Some(major), Some(minor));
            }
            _ => {}
        }
        if let Some(tr) = &face.transform {
            let rotate = |p: P3| {
                [
                    brep::dot(tr.m[0], p),
                    brep::dot(tr.m[1], p),
                    brep::dot(tr.m[2], p),
                ]
            };
            let scale = brep::norm(tr.m[0]);
            out.point = brep::add(rotate(out.point), tr.t);
            out.normal = brep::normalize(rotate(out.normal));
            for p in &mut out.spread {
                *p = brep::add(rotate(*p), tr.t);
            }
            out.origin = out.origin.map(|o| brep::add(rotate(o), tr.t));
            out.axis = out.axis.map(|a| brep::normalize(rotate(a)));
            out.radius = out.radius.map(|r| r * scale);
            out.minor = out.minor.map(|r| r * scale);
        }
        Ok(out)
    }

    /// The kind of a face's surface ([`Surface::kind`]), or why it is not
    /// known.
    pub fn surface_kind(&self, index: usize) -> String {
        let f = &self.faces[index];
        let body = self.bodies[f.body].record;
        match convert::face_surface(
            self.file,
            body,
            f.entity,
            &Options::default(),
            self.view.as_ref(),
            2,
        ) {
            Ok(face) => face.surface.kind().to_owned(),
            Err(e) => format!("({})", e.0),
        }
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

    // Appearances (mitcad#228).

    /// The key a body's record carries in its header's second field (the
    /// design's body map names a body by it); None for `-1`.
    pub fn body_key(&self, body: usize) -> Option<i64> {
        let w = Walker {
            file: self.file,
            view: self.view.as_ref(),
        };
        let r = w.data(self.bodies[body].record)?;
        match self
            .file
            .tokens
            .get(self.file.records[r].fields.start + 1)?
        {
            Token::Int(k) | Token::Ptr(k) if *k >= 0 => Some(*k),
            _ => None,
        }
    }

    /// The key a face's record carries in its header's second field (its
    /// history id; the `.ipt` graphics faces name faces by it, mitcad#1367);
    /// None for `-1`.
    pub fn face_key(&self, face: usize) -> Option<i64> {
        let w = Walker {
            file: self.file,
            view: self.view.as_ref(),
        };
        let r = w.data(self.faces[face].entity)?;
        match self
            .file
            .tokens
            .get(self.file.records[r].fields.start + 1)?
        {
            Token::Int(k) | Token::Ptr(k) if *k >= 0 => Some(*k),
            _ => None,
        }
    }

    /// The GUID of a face's appearance assignment: the string after the
    /// name `NEUTRON_Material_attrib_def` in one of its `ATTRIB_CUSTOM`
    /// attributes (lower case, 36 characters).
    pub fn face_appearance_guid(&self, face: usize) -> Option<String> {
        let w = Walker {
            file: self.file,
            view: self.view.as_ref(),
        };
        let r = w.data(self.faces[face].entity)?;
        let mut c = Cursor::new(&self.file.tokens, self.file.records[r].fields.start);
        let mut at = w.ptr(&mut c).flatten();
        let mut seen = HashSet::new();
        while let Some(a) = at {
            if !seen.insert(a) {
                break;
            }
            let ar = w.data(a)?;
            let rec = &self.file.records[ar];
            if rec.base_type() != "attrib" {
                break;
            }
            let fields = self.file.fields(ar);
            if let Some(k) = fields
                .iter()
                .position(|t| matches!(t, Token::Str(s) if s == "NEUTRON_Material_attrib_def"))
                && let Some(Token::Str(g)) =
                    fields[k + 1..].iter().find(|t| matches!(t, Token::Str(_)))
                && g.len() == 36
            {
                return Some(g.to_ascii_lowercase());
            }
            let mut c = Cursor::new(&self.file.tokens, rec.fields.start + 2);
            at = w.ptr(&mut c).flatten();
        }
        None
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
        let (curve, t, transform, along) = convert::edge_curve_along(
            self.file,
            body,
            e.entity,
            &Options::default(),
            self.view.as_ref(),
        )
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
        let rotate = |v: P3| match &transform {
            Some(tr) => [
                brep::dot(tr.m[0], v),
                brep::dot(tr.m[1], v),
                brep::dot(tr.m[2], v),
            ],
            None => v,
        };
        // A helix, exactly (mitcad_private#198): its length integrated, its
        // direction its derivative.
        if let Curve::Helix(h) = &curve {
            let m = 0.5 * (t[0] + t[1]);
            let ahead = rotate(h.derivative(m));
            return Ok(EdgeGeometry {
                mid: apply(h.eval(m)),
                length: scale * h.length(t[0], t[1]),
                start: apply(h.eval(t[0])),
                end: apply(h.eval(t[1])),
                direction: brep::normalize(if along {
                    ahead
                } else {
                    brep::scale(ahead, -1.0)
                }),
            });
        }
        // A helix known by its samples (mitcad#161): on the cubic through
        // them, as the builder interpolates it.
        if let Curve::Interpolated { params, points } = &curve
            && params.len() >= 3
        {
            let m = 0.5 * (t[0] + t[1]);
            let (_, ahead) = smooth::point(params, points, m);
            let ahead = rotate(ahead);
            return Ok(EdgeGeometry {
                mid: apply(smooth::point(params, points, m).0),
                length: scale * smooth::length(params, points, t[0], t[1]),
                start: apply(smooth::point(params, points, t[0]).0),
                end: apply(smooth::point(params, points, t[1]).0),
                direction: brep::normalize(if along {
                    ahead
                } else {
                    brep::scale(ahead, -1.0)
                }),
            });
        }
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

/// Points along each edge of a planar face for [`NamedState::plane`].
const PLANE_SAMPLES: usize = 16;

/// Points along each edge of a face for [`NamedState::surface_point`]
/// (a thread flank's helical edges run round several turns).
const SURFACE_SAMPLES: usize = 64;

/// A face's surface and a point inside it ([`NamedState::surface_point`]),
/// mm, in its component's coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfacePoint {
    /// The surface's type as dumps name it (`Cone`, `Sphere`, `Torus`,
    /// `NurbsSurface` for every free-form surface).
    pub kind: &'static str,
    pub point: P3,
    /// The face's normal at `point`, with the face's orientation.
    pub normal: P3,
    /// A cone's or torus' point on the axis, a sphere's centre.
    pub origin: Option<P3>,
    pub axis: Option<P3>,
    /// A cone's radius at `origin`, a sphere's radius, a torus' major
    /// radius.
    pub radius: Option<f64>,
    pub half_angle: Option<f64>,
    /// A torus' minor radius.
    pub minor: Option<f64>,
    /// More points inside a free-form face, spread over it
    /// ([`surface::Inside::spread`]).
    pub spread: Vec<P3>,
}

/// A point inside a planar face whose edges run along the polylines
/// `edges` (each edge's points in order, the loops in any order and
/// direction), in the plane through `guess` with the normal `normal`:
/// `guess` where it lies in the face, else, of the middles of the face's
/// stretches along lines across it, the one farthest from the edges (the
/// face's points by the even-odd rule over all its loops, so a hole's loop
/// takes its inside out of the face). `guess` when the edges enclose
/// nothing.
pub(crate) fn point_inside(normal: P3, edges: &[&[P3]], guess: P3) -> P3 {
    // The plane's own axes, from the guess.
    let any = if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = brep::normalize(brep::cross(normal, any));
    let v = brep::cross(normal, u);
    let flat = |p: P3| {
        let d = brep::sub(p, guess);
        (brep::dot(d, u), brep::dot(d, v))
    };
    let mut segments: Vec<((f64, f64), (f64, f64))> = edges
        .iter()
        .flat_map(|e| e.windows(2).map(|w| (flat(w[0]), flat(w[1]))))
        .filter(|(a, b)| a != b)
        .collect();
    // An edge's end meets the next edge's start (or its own, a closed
    // edge) only to rounding: close the gap, else a line through it misses
    // the loop (mitcad#228: a hole's centre on the line through a circle's
    // seam was taken for a point inside the face).
    let extent = edges
        .iter()
        .flat_map(|e| e.iter())
        .map(|p| brep::norm(brep::sub(*p, guess)))
        .fold(0.0, f64::max);
    for e in edges {
        let Some(&end) = e.last() else { continue };
        let nearest = edges
            .iter()
            .filter_map(|s| s.first())
            .min_by(|a, b| brep::dist(**a, end).total_cmp(&brep::dist(**b, end)));
        if let Some(&start) = nearest {
            let d = brep::dist(start, end);
            if d > 0.0 && d <= 1e-9 * (1.0 + extent) {
                segments.push((flat(end), flat(start)));
            }
        }
    }
    // Where a line at height y crosses the edges, in order along it (each
    // segment counts its lower end and not its upper one).
    let crossings = |y: f64| {
        let mut xs: Vec<f64> = segments
            .iter()
            .filter(|((_, ya), (_, yb))| (*ya <= y) != (*yb <= y))
            .map(|((xa, ya), (xb, yb))| xa + (y - ya) / (yb - ya) * (xb - xa))
            .collect();
        xs.sort_by(f64::total_cmp);
        xs
    };
    // The guess is inside where an odd number of crossings lie past it.
    let through = crossings(0.0);
    if through.iter().filter(|&&x| x > 0.0).count() % 2 == 1 {
        return guess;
    }
    let (low, high) = segments
        .iter()
        .flat_map(|(a, b)| [a.1, b.1])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), y| {
            (l.min(y), h.max(y))
        });
    if high <= low || !(high - low).is_finite() {
        return guess;
    }
    // How far a point is from the nearest edge.
    let clearance = |x: f64, y: f64| {
        segments
            .iter()
            .map(|((xa, ya), (xb, yb))| {
                let (dx, dy) = (xb - xa, yb - ya);
                let t = (((x - xa) * dx + (y - ya) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                (x - xa - t * dx).hypot(y - ya - t * dy)
            })
            .fold(f64::INFINITY, f64::min)
    };
    // The middles of the stretches inside on lines across the face
    // (heights between the sampled points' to stay off vertices); the one
    // farthest from the edges.
    const LINES: usize = 32;
    let mut best: Option<(f64, f64, f64)> = None;
    for k in 0..LINES {
        let y = low + (high - low) * (k as f64 + 0.5) / LINES as f64;
        let xs = crossings(y);
        if xs.len() % 2 != 0 {
            continue;
        }
        for pair in xs.chunks(2) {
            let x = 0.5 * (pair[0] + pair[1]);
            let c = clearance(x, y);
            if best.is_none_or(|(b, _, _)| c > b) {
                best = Some((c, x, y));
            }
        }
    }
    match best {
        Some((_, x, y)) => brep::add(guess, brep::add(brep::scale(u, x), brep::scale(v, y))),
        None => guess,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_asm::testdata;

    /// A disc with a square hole round its centre, the circle one closed
    /// edge whose ends meet only to rounding where the line through the
    /// mean of the points crosses it: the point lies between the hole and
    /// the circle, not in the hole (mitcad#228).
    #[test]
    fn a_closed_edges_seam_does_not_open_the_loop() {
        let circle: Vec<P3> = (0..16)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 15.0;
                [0.8 * a.sin(), 0.0, 3.075 - 0.8 * a.cos()]
            })
            .map(|p| [if p[0].abs() < 1e-12 { 5e-16 } else { p[0] }, p[1], p[2]])
            .enumerate()
            .map(|(i, p)| if i == 15 { [-3.5e-16, p[1], p[2]] } else { p })
            .collect();
        let line = |a: P3, b: P3| -> Vec<P3> {
            (0..16)
                .map(|i| brep::add(a, brep::scale(brep::sub(b, a), i as f64 / 15.0)))
                .collect()
        };
        let (l, r, b, t) = (-0.4, 0.4, 2.675, 3.475);
        let square = [
            line([l, 0.0, t], [r, 0.0, t]),
            line([r, 0.0, t], [r, 0.0, b]),
            line([r, 0.0, b], [l, 0.0, b]),
            line([l, 0.0, b], [l, 0.0, t]),
        ];
        let mut edges: Vec<&[P3]> = vec![&circle];
        edges.extend(square.iter().map(Vec::as_slice));
        let p = point_inside([0.0, 1.0, 0.0], &edges, [0.0, 0.0, 3.065]);
        let in_hole = p[0] > l && p[0] < r && p[2] > b && p[2] < t;
        let in_disc = p[0].hypot(p[2] - 3.075) < 0.8;
        assert!(in_disc && !in_hole, "{p:?}");
    }

    /// A face with a hole round the mean of its edges' points (a ring):
    /// the point lies between the hole and the outside, and a convex face
    /// keeps the mean (mitcad#158).
    #[test]
    fn a_planar_face_point_lies_inside_the_face() {
        let circle = |r: f64| -> Vec<P3> {
            (0..=64)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / 64.0;
                    [r * a.cos(), r * a.sin(), 15.0]
                })
                .collect()
        };
        let (outer, hole) = (circle(13.0), circle(5.0));
        let edges: Vec<&[P3]> = vec![&outer, &hole];
        let p = point_inside([0.0, 0.0, 1.0], &edges, [0.0, 0.0, 15.0]);
        let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
        assert!((r - 9.0).abs() < 0.5 && (p[2] - 15.0).abs() < 1e-9, "{p:?}");
        // A square keeps its middle; a guess outside an L comes inside it.
        let square: Vec<P3> = vec![
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let p = point_inside([0.0, 0.0, 1.0], &[&square], [2.0, 2.0, 0.0]);
        assert_eq!(p, [2.0, 2.0, 0.0]);
        let ell: Vec<P3> = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let p = point_inside([0.0, 0.0, 1.0], &[&ell], [3.0, 3.0, 0.0]);
        let inside = (p[1] > 0.0 && p[1] < 1.0 && p[0] > 0.0 && p[0] < 10.0)
            || (p[0] > 0.0 && p[0] < 1.0 && p[1] > 0.0 && p[1] < 10.0);
        assert!(inside, "{p:?}");
    }

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

    /// The cube's edge along x at the bottom front (faces "1" and "3"),
    /// the edge along x at the top front (faces "2" and "3"), and the
    /// names the recipe's entities use: bottom or top (`0`), front (`1`),
    /// left (`2`, at the start) and right (`3`, at the end).
    fn cube_pair(state: &NamedState) -> (usize, usize, Vec<Vec<EntityName>>) {
        let face = |tag: &str| {
            state
                .faces
                .iter()
                .position(|f| f.names.iter().any(|m| m.is(&n(tag, &[301]))))
                .unwrap()
        };
        let (bottom, top, front) = (face("1"), face("2"), face("3"));
        let along = |a: usize, b: usize| {
            state
                .edges
                .iter()
                .position(|e| e.faces.contains(&a) && e.faces.contains(&b))
                .unwrap()
        };
        let entities = vec![
            vec![n("1", &[301]), n("2", &[301])],
            vec![n("3", &[301])],
            vec![n("6", &[301])],
            vec![n("4", &[301])],
        ];
        (along(bottom, front), along(top, front), entities)
    }

    /// Both edges along x at the front fit the names of the two faces
    /// (one named for either the bottom or the top) and of the ends:
    /// the faces round the ends in the recipe's order tell them apart
    /// (mitcad#106). Seen from outside the body they run counter-
    /// clockwise: at the bottom front edge's start the bottom, the front
    /// and the left face, at its end the front, the bottom and the right
    /// face; at the top front edge's start the front, the top and the
    /// left face, at its end the top, the front and the right face.
    #[test]
    fn the_order_of_the_faces_round_the_ends_tells_equal_edges_apart() {
        let data = testdata::named_cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        let (bottom, top, entities) = cube_pair(&state);
        assert_ne!(bottom, top);
        let tail = |fans: [[usize; 3]; 2]| EdgeTail {
            faces: [0, 1],
            fans: fans.iter().map(|f| f.to_vec()).collect(),
        };
        let bottom_front = tail([[0, 1, 2], [1, 0, 3]]);
        let top_front = tail([[1, 0, 2], [0, 1, 3]]);
        // Without the tail both fit, with the names of the ends as well.
        let both = state.edges_fitting(&entities).unwrap();
        assert_eq!(both.len(), 2);
        assert!(both.contains(&bottom) && both.contains(&top));
        // Each tail picks its edge.
        assert_eq!(
            state.edges_fitting_ordered(&entities, Some(&bottom_front)),
            Ok(vec![bottom])
        );
        assert_eq!(
            state.edges_fitting_ordered(&entities, Some(&top_front)),
            Ok(vec![top])
        );
        // Rotating a fan keeps the cyclic order; reversing both the other
        // edge's; ends in the wrong places fit neither (all stay).
        assert_eq!(
            state.edges_fitting_ordered(&entities, Some(&tail([[2, 0, 1], [3, 1, 0]]))),
            Ok(vec![bottom])
        );
        assert_eq!(
            state.edges_fitting_ordered(&entities, Some(&tail([[2, 1, 0], [0, 1, 3]]))),
            Ok(vec![top])
        );
        let wrong = tail([[0, 1, 3], [1, 0, 2]]);
        assert_eq!(
            state
                .edges_fitting_ordered(&entities, Some(&wrong))
                .map(|v| v.len()),
            Ok(2)
        );
        // A tail that is not two fans, or names an entity the list does
        // not have, says nothing.
        let one = EdgeTail {
            faces: [0, 1],
            fans: vec![vec![0, 1, 2]],
        };
        assert_eq!(
            state
                .edges_fitting_ordered(&entities, Some(&one))
                .map(|v| v.len()),
            Ok(2)
        );
        assert_eq!(
            state
                .edges_fitting_ordered(&entities, Some(&tail([[0, 1, 9], [1, 0, 3]])))
                .map(|v| v.len()),
            Ok(2)
        );
    }

    /// An entity that no face of the state has (a face named as an earlier
    /// state had it) stands for any face; one that fits another face does
    /// not.
    #[test]
    fn an_entity_no_face_has_stands_for_any_face() {
        let data = testdata::named_cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        let (bottom, top, mut entities) = cube_pair(&state);
        let tail = EdgeTail {
            faces: [0, 1],
            fans: vec![vec![0, 1, 2], vec![1, 0, 3]],
        };
        assert_eq!(state.order_agrees(bottom, &entities, &tail, 0), Some(true));
        assert_eq!(state.order_agrees(top, &entities, &tail, 0), Some(false));
        entities[2] = vec![n("7", &[999])];
        assert_eq!(state.order_agrees(bottom, &entities, &tail, 0), Some(true));
        // The left face's place taken by another face: the order fails.
        entities[2] = vec![n("5", &[301])];
        assert_eq!(state.order_agrees(bottom, &entities, &tail, 0), Some(false));
        // The same tail for the entities after an edge's own: indices
        // count from the entity before the faces.
        let shifted = EdgeTail {
            faces: [1, 2],
            fans: vec![vec![1, 2, 3], vec![2, 1, 4]],
        };
        entities[2] = vec![n("6", &[301])];
        assert_eq!(
            state.order_agrees(bottom, &entities, &shifted, 1),
            Some(true)
        );
        assert_eq!(state.order_agrees(bottom, &entities, &shifted, 5), None);
    }

    /// A whole cylinder's seam direction is its surface's parameter origin.
    /// is; a face that is a part of a cylinder (the cylinder cut by a
    /// plane along its axis is not in the test data: the cube's planes
    /// are not cylinders) has none to read.
    #[test]
    fn a_whole_cylinder_has_its_seam_direction() {
        let data = testdata::cylinder_blob();
        let file = AsmFile::parse(&data).unwrap();
        let state = NamedState::new(&file, None);
        let i = (0..state.faces.len())
            .find(|&i| state.cylinder(i).is_ok())
            .expect("a cylindrical face");
        let g = state.cylinder(i).unwrap();
        assert!(g.full);
        let dot = brep::dot(g.ref_dir, g.axis);
        assert!(dot.abs() < 1e-9, "{dot}");
        assert!((brep::norm(g.ref_dir) - 1.0).abs() < 1e-9);
        assert!((g.ref_dir[0] - 1.0).abs() < 1e-9, "{:?}", g.ref_dir);
    }
}
