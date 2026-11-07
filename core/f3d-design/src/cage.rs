// SPDX-License-Identifier: MIT
//! The control cages of Form bodies (mitcad#1365): the document's
//! `TSplines.BlobParts/*.tsm` entries read into `_f3d.cages`
//! ([`mitcad_dump::cage`]).
//!
//! A Form body's faces are in the B-rep blob like any other body's (the
//! import brings them in as the file's bodies); the cage they come from is
//! a UTF-8, line-oriented `TS0200` program, one record per line, a keyword
//! and its operands:
//!
//! - declarations: `degree 3`, `cap-type`, `star-smoothness`, `units`,
//!   `end-conditions`, `star-knot-rule`, and the versions `ver`,
//!   `behavior-version`, `compat-version`; the tolerances `tol`,
//!   `geom-tol` (cm);
//! - topology, every index zero-based in its keyword's record order:
//!   `f root flags` a face and a half-edge of its boundary; `e root knot` an
//!   edge, one of its half-edges and its knot interval; `v root direction`
//!   a control vertex, the half-edge its fan starts at and the direction
//!   word of its grip layout; `l next previous mate vertex face edge
//!   flags` a half-edge (`vertex` its terminal vertex, `face` -1 on the
//!   boundary); `ec edge flags` a creased edge. A keyword with no operands
//!   is a deleted slot: it takes an index and defines nothing;
//! - grips: `0g x y z weight`, a control point (cm, the coordinates the
//!   program gives in spite of its `units 1 meters`). Without a grip map
//!   grip `i` is vertex `i`'s point. A map (`0m odd-grip-map`, then one
//!   `0m gvp v` or `0m gv v` per grip, before the first `0g`) gives each
//!   grip its vertex: `gvp` the vertex's own point, `gv` a secondary grip
//!   (`gv -1` a deleted grip). `0m cg …` gives a vertex's secondary grips
//!   their layout;
//! - editor state that does not change the cage: symmetry blocks (`105sym`,
//!   `105plane`, `105a`, `105r`), selections (`100edges`, `100verts`,
//!   `50000grip`) and the edge-knot mirror `106ek`.
//!
//! The cage keeps the populated elements renumbered in order: each face
//! its ring of vertices (its root half-edge's `next` ring, each half-edge's
//! terminal vertex), each edge its two vertices (from its root's mate's
//! vertex to its root's), knot, crease and boundary, each vertex its own
//! grip and its tag (smooth, dart, crease or corner by its creased edges),
//! and the secondary grips; the editor records and the grip layouts as
//! written (`native`), and each element's native slot, so a writer can give
//! the program back.
//!
//! The design joins a cage to its records by the entry's name: an object
//! of the design (the cage's serializer) holds `TSpline.<uuid>.tsm` as a
//! `str16`, followed by a reference to the cage's surface record; the
//! record that owns the surface refers to the Form item ([`join`]).
//!
//! Layouts after the cadmpeg project's format description
//! (`docs/formats/f3d.md`, *Form control cages*, CC BY 4.0,
//! <https://github.com/cadmpeg/cadmpeg>), written in our own words;
//! no text or code was taken over (`core/import/docs/f3d-undecoded.md`,
//! *The published format descriptions*).

use std::collections::{BTreeSet, HashMap};

use mitcad_dump::Dump;
use mitcad_dump::cage::{CageEdge, CageGrip, CageVertex, FormCage, VertexTag};
use mitcad_f3d_file::container::F3dFile;

use crate::DESIGN_SEGMENTS;
use crate::stream::{Segment, str16_at};

/// The folder of the document's cage entries.
pub const CAGE_FOLDER: &str = "TSplines.BlobParts";

/// The cages of a document: every `.tsm` entry of the design's asset
/// folder (of any folder when the document has no design segment), read
/// and joined to the design's records. `items` gives each timeline item's
/// timeline index by its object id.
pub fn decode(seg: &Segment, doc: &F3dFile, items: &HashMap<u64, i64>) -> Vec<FormCage> {
    let folder = design_folder(doc);
    let mut cages = Vec::new();
    for e in doc.archive().entries() {
        let parts: Vec<&str> = e.name.split('/').collect();
        let [asset, CAGE_FOLDER, name] = parts[..] else {
            continue;
        };
        if !name.ends_with(".tsm") || folder.as_deref().is_some_and(|f| f != asset) {
            continue;
        }
        let mut cage = match doc.archive().read(e) {
            Ok(bytes) => parse(&String::from_utf8_lossy(&bytes)),
            Err(err) => FormCage {
                warnings: vec![format!("the entry does not read: {err}")],
                ..FormCage::default()
            },
        };
        cage.entry = e.name.clone();
        cage.name = name.to_owned();
        cages.push(cage);
    }
    if !cages.is_empty() {
        join(seg, items, &mut cages);
    }
    cages
}

/// [`decode`] into a dump's `_f3d.cages`, the dump's timeline items giving
/// the items' indices; a document without cages leaves the key out.
pub fn decode_into(dump: &mut Dump, seg: &Segment, doc: &F3dFile) {
    let items: HashMap<u64, i64> = dump
        .timeline_items()
        .iter()
        .filter_map(|item| Some((item.f3d.as_ref()?.object_id?, item.index?)))
        .collect();
    let cages = decode(seg, doc, &items);
    if !cages.is_empty() {
        dump.f3d.get_or_insert_with(Default::default).cages = Some(cages);
    }
}

/// The asset folder of the design segment (as `find_design_streams`
/// finds it).
fn design_folder(doc: &F3dFile) -> Option<String> {
    doc.archive().entries().iter().find_map(|e| {
        let parts: Vec<&str> = e.name.split('/').collect();
        match parts[..] {
            [asset, segment, "MetaStream.dat"]
                if asset.starts_with("FusionAssetName") && DESIGN_SEGMENTS.contains(&segment) =>
            {
                Some(asset.to_owned())
            }
            _ => None,
        }
    })
}

/// A populated half-edge record (`l`).
#[derive(Clone, Copy, Debug)]
struct Half {
    next: i64,
    previous: i64,
    mate: i64,
    vertex: i64,
    face: i64,
    edge: i64,
}

/// What a grip map says of one grip.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GripMark {
    /// The vertex's own point (`gvp`).
    Own(i64),
    /// A secondary grip of the vertex (`gv`; -1 a deleted grip).
    Secondary(i64),
}

/// The records of a program by keyword, in slot order (`None` a deleted or
/// unreadable slot).
#[derive(Default)]
struct Program {
    faces: Vec<Option<i64>>,
    edges: Vec<Option<(i64, f64)>>,
    vertices: Vec<Option<(i64, String)>>,
    halves: Vec<Option<Half>>,
    grips: Vec<Option<([f64; 3], f64)>>,
    grip_map: Option<Vec<GripMark>>,
    creases: BTreeSet<i64>,
}

fn int(s: &str) -> Option<i64> {
    s.parse().ok()
}

fn real(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// An `l` record's operands: `next previous mate vertex face edge flags`.
fn half_record(ops: &[&str]) -> Option<Half> {
    let [next, previous, mate, vertex, face, edge, ..] = ops else {
        return None;
    };
    Some(Half {
        next: int(next)?,
        previous: int(previous)?,
        mate: int(mate)?,
        vertex: int(vertex)?,
        face: int(face)?,
        edge: int(edge)?,
    })
}

/// A `0g` record's operands: `x y z weight` (a weight of 1 when left out).
fn grip_record(ops: &[&str]) -> Option<([f64; 3], f64)> {
    let [x, y, z, rest @ ..] = ops else {
        return None;
    };
    let weight = match rest.first() {
        Some(w) => real(w)?,
        None => 1.0,
    };
    Some(([real(x)?, real(y)?, real(z)?], weight))
}

/// Reads a `TS0200` program into a cage (without its entry and joins).
/// It never fails: what does not read or hold together is a warning, and
/// the cage keeps the rest.
pub fn parse(text: &str) -> FormCage {
    let mut cage = FormCage::default();
    let mut p = Program::default();
    let mut bad = 0usize;
    for (line_no, line) in (0u32..).zip(text.lines()) {
        let line = line.trim();
        let mut words = line.split_whitespace();
        let Some(key) = words.next() else {
            continue;
        };
        let ops: Vec<&str> = words.collect();
        if line_no == 0 && key == "#TS0200" {
            continue;
        }
        if line_no == 0 {
            cage.warnings.push(format!(
                "the program does not start with #TS0200 but with {key:?}"
            ));
        }
        // A keyword of a slotted kind with no operands: a deleted slot.
        let read = match (key, &ops[..]) {
            ("f", []) => {
                p.faces.push(None);
                true
            }
            ("f", [root, _flags, ..]) => {
                let root = int(root);
                p.faces.push(root);
                root.is_some()
            }
            ("e", []) => {
                p.edges.push(None);
                true
            }
            ("e", [root, knot, ..]) => {
                let edge = int(root).zip(real(knot));
                p.edges.push(edge);
                edge.is_some()
            }
            ("v", []) => {
                p.vertices.push(None);
                true
            }
            ("v", [root, rest @ ..]) => {
                let root = int(root);
                let direction = rest.first().map_or(String::new(), |d| (*d).to_owned());
                p.vertices.push(root.map(|r| (r, direction)));
                root.is_some()
            }
            ("l", []) => {
                p.halves.push(None);
                true
            }
            ("l", _) => {
                let half = half_record(&ops);
                p.halves.push(half);
                half.is_some()
            }
            ("0g", []) => {
                p.grips.push(None);
                true
            }
            ("0g", _) => {
                let grip = grip_record(&ops);
                p.grips.push(grip);
                grip.is_some()
            }
            // A slotted record that does not read still takes its slot.
            ("f", _) => {
                p.faces.push(None);
                false
            }
            ("e", _) => {
                p.edges.push(None);
                false
            }
            ("ec", [edge, ..]) => match int(edge) {
                Some(e) => {
                    p.creases.insert(e);
                    true
                }
                None => false,
            },
            ("0m", ["odd-grip-map"]) => {
                p.grip_map.get_or_insert_with(Vec::new);
                true
            }
            ("0m", [kind @ ("gvp" | "gv"), vertex]) => match (int(vertex), &mut p.grip_map) {
                (Some(v), Some(map)) => {
                    map.push(if *kind == "gvp" {
                        GripMark::Own(v)
                    } else {
                        GripMark::Secondary(v)
                    });
                    true
                }
                _ => false,
            },
            ("degree", [n]) => match n.parse() {
                Ok(n) => {
                    cage.degree = n;
                    true
                }
                Err(_) => false,
            },
            ("tol", [t]) => {
                cage.tolerance = real(t);
                cage.tolerance.is_some()
            }
            ("geom-tol", [t]) => {
                cage.geometric_tolerance = real(t);
                cage.geometric_tolerance.is_some()
            }
            (
                "cap-type" | "star-smoothness" | "units" | "end-conditions" | "star-knot-rule"
                | "ver" | "behavior-version" | "compat-version",
                _,
            ) => {
                cage.settings.push((key.to_owned(), ops.join(" ")));
                true
            }
            // Editor state, grip layouts, the knot mirror and anything
            // unknown: kept as written.
            _ => {
                cage.native.push((line_no, line.to_owned()));
                true
            }
        };
        if !read {
            bad += 1;
            if bad <= 10 {
                cage.warnings
                    .push(format!("record {line_no} does not read: {line:?}"));
            }
            if !matches!(key, "f" | "e" | "v" | "l" | "0g") {
                cage.native.push((line_no, line.to_owned()));
            }
        }
    }
    if bad > 10 {
        cage.warnings
            .push(format!("{bad} records in all do not read"));
    }
    build(&p, &mut cage);
    cage
}

/// The cage's neutral elements from the program's records.
fn build(p: &Program, cage: &mut FormCage) {
    cage.half_edges = p.halves.len() as u64;
    let half = |h: i64| -> Option<Half> {
        usize::try_from(h)
            .ok()
            .and_then(|h| p.halves.get(h)?.as_ref().copied())
    };
    // Vertices: the populated `v` slots in order.
    let mut vertex_of: HashMap<i64, u32> = HashMap::new();
    for (slot, v) in (0i64..).zip(&p.vertices) {
        if let Some((_, direction)) = v {
            vertex_of.insert(slot, cage.vertices.len() as u32);
            cage.vertex_slots.push(slot as u32);
            cage.vertices.push(CageVertex {
                direction: direction.clone(),
                ..CageVertex::default()
            });
        }
    }
    // Each vertex's own grip and the secondary grips.
    let mut placed = vec![false; cage.vertices.len()];
    let mut unplaced_grips = 0usize;
    for (slot, grip) in (0i64..).zip(&p.grips) {
        let mark = match &p.grip_map {
            Some(map) => map.get(slot as usize).copied(),
            None => Some(GripMark::Own(slot)),
        };
        let Some((position, weight)) = grip else {
            continue;
        };
        match mark {
            Some(GripMark::Own(v)) => match vertex_of.get(&v) {
                Some(&i) if !placed[i as usize] => {
                    placed[i as usize] = true;
                    let vertex = &mut cage.vertices[i as usize];
                    vertex.position = *position;
                    vertex.weight = *weight;
                }
                _ => unplaced_grips += 1,
            },
            Some(GripMark::Secondary(v)) => match vertex_of.get(&v) {
                Some(&i) => cage.grips.push(CageGrip {
                    vertex: i,
                    position: *position,
                    weight: *weight,
                }),
                None => unplaced_grips += 1,
            },
            None => unplaced_grips += 1,
        }
    }
    if unplaced_grips > 0 {
        cage.warnings.push(format!(
            "{unplaced_grips} grips name no vertex or a vertex that has its point already"
        ));
    }
    let without = placed.iter().filter(|done| !**done).count();
    if without > 0 {
        cage.warnings.push(format!(
            "{without} vertices have no grip (placed at the origin)"
        ));
    }
    // Faces: the ring of each root half-edge.
    let mut broken_faces = 0usize;
    for (slot, root) in (0u32..).zip(&p.faces) {
        let Some(root) = *root else { continue };
        match ring(root, p.halves.len(), half, &vertex_of) {
            Some(vertices) => {
                cage.face_slots.push(slot);
                cage.faces.push(vertices);
            }
            None => broken_faces += 1,
        }
    }
    if broken_faces > 0 {
        cage.warnings.push(format!(
            "{broken_faces} faces have no closed ring of half-edges (left out)"
        ));
    }
    // Edges: from the root's mate's vertex to the root's.
    let mut broken_edges = 0usize;
    let mut creases_at = vec![0usize; cage.vertices.len()];
    for (slot, edge) in (0i64..).zip(&p.edges) {
        let Some((root, knot)) = *edge else { continue };
        let ends = half(root).and_then(|h| {
            let mate = half(h.mate)?;
            let a = *vertex_of.get(&mate.vertex)?;
            let b = *vertex_of.get(&h.vertex)?;
            Some(([a, b], h.face < 0 || mate.face < 0))
        });
        let Some((vertices, boundary)) = ends else {
            broken_edges += 1;
            continue;
        };
        let crease = p.creases.contains(&slot);
        if crease {
            for v in vertices {
                creases_at[v as usize] += 1;
            }
        }
        cage.edge_slots.push(slot as u32);
        cage.edges.push(CageEdge {
            vertices,
            knot,
            crease,
            boundary,
        });
    }
    if broken_edges > 0 {
        cage.warnings.push(format!(
            "{broken_edges} edges have no root half-edge with a mate (left out)"
        ));
    }
    let stray = p
        .creases
        .iter()
        .filter(|e| {
            usize::try_from(**e)
                .ok()
                .and_then(|e| p.edges.get(e)?.as_ref())
                .is_none()
        })
        .count();
    if stray > 0 {
        cage.warnings.push(format!("{stray} creases name no edge"));
    }
    for (vertex, creases) in cage.vertices.iter_mut().zip(creases_at) {
        vertex.tag = VertexTag::of(creases);
    }
    // The half-edges hold together: next and previous, mates, edges.
    let mut loose = 0usize;
    for (slot, h) in (0i64..).zip(&p.halves) {
        let Some(h) = h else { continue };
        let fits = half(h.next).is_some_and(|n| n.previous == slot)
            && half(h.mate).is_some_and(|m| m.mate == slot && m.edge == h.edge)
            && vertex_of.contains_key(&h.vertex)
            && usize::try_from(h.edge)
                .ok()
                .and_then(|e| p.edges.get(e)?.as_ref())
                .is_some();
        if !fits {
            loose += 1;
        }
    }
    if loose > 0 {
        cage.warnings.push(format!(
            "{loose} half-edges do not agree with their next, previous or mate, or name no vertex or edge"
        ));
    }
}

/// A face's vertices: the terminal vertex of each half-edge of the ring
/// that starts at `root` (at most `limit` steps); None when the ring does
/// not close or names a vertex that is not there.
fn ring(
    root: i64,
    limit: usize,
    half: impl Fn(i64) -> Option<Half>,
    vertex_of: &HashMap<i64, u32>,
) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    let mut h = root;
    loop {
        let this = half(h)?;
        out.push(*vertex_of.get(&this.vertex)?);
        h = this.next;
        if h == root {
            return (out.len() >= 3).then_some(out);
        }
        if out.len() > limit {
            return None;
        }
    }
}

/// Joins the cages to the design's records: the object that names each
/// cage's entry (its serializer), the object the serializer refers to right
/// after the name (the cage's surface), and the timeline item that the
/// objects referring to either refer to (the Form item that owns them).
pub fn join(seg: &Segment, items: &HashMap<u64, i64>, cages: &mut [FormCage]) {
    let needle: Vec<u8> = "TSpline."
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let by_name: HashMap<String, usize> = cages
        .iter()
        .enumerate()
        .map(|(i, c)| (c.name.clone(), i))
        .collect();
    let mut found: Vec<Vec<(u64, Option<u64>)>> = vec![Vec::new(); cages.len()];
    for o in &seg.objects {
        let d = seg.data(o);
        let mut at = 0;
        while let Some(k) = find(&d[at.min(d.len())..], &needle) {
            let p = at + k;
            at = p + 1;
            let Some((name, end)) = p.checked_sub(4).and_then(|q| str16_at(d, q)) else {
                continue;
            };
            if let Some(&i) = by_name.get(&name) {
                found[i].push((o.id, seg.ref_at(d, end).map(|r| r.id)));
            }
        }
    }
    // The objects that refer to a serializer or a surface, and what those
    // refer to (read only when some cage has a serializer).
    if found.iter().all(Vec::is_empty) {
        for cage in cages.iter_mut() {
            cage.warnings
                .push("no record of the design names the entry".to_owned());
        }
        return;
    }
    let refs: HashMap<u64, Vec<u64>> = seg
        .objects
        .iter()
        .map(|o| (o.id, seg.ref_ids(seg.data(o))))
        .collect();
    for (cage, found) in cages.iter_mut().zip(found) {
        let Some(&(serializer, surface)) = found.first() else {
            cage.warnings
                .push("no record of the design names the entry".to_owned());
            continue;
        };
        if found.len() > 1 {
            cage.warnings.push(format!(
                "{} records name the entry; the first is taken",
                found.len()
            ));
        }
        cage.serializer = Some(serializer);
        cage.surface = surface;
        let ends: Vec<u64> = std::iter::once(serializer).chain(surface).collect();
        let mut near: BTreeSet<u64> = ends.iter().copied().collect();
        near.extend(refs.get(&serializer).into_iter().flatten().copied());
        for (id, to) in &refs {
            if to.iter().any(|t| ends.contains(t)) {
                near.insert(*id);
                near.extend(to.iter().copied());
            }
        }
        let owners: BTreeSet<(i64, u64)> = near
            .iter()
            .filter_map(|id| items.get(id).map(|index| (*index, *id)))
            .collect();
        match owners.len() {
            1 => {
                let (index, id) = *owners.first().expect("one owner");
                cage.item = Some(id);
                cage.item_index = Some(index);
            }
            0 => cage
                .warnings
                .push("no timeline item refers to the cage's records".to_owned()),
            n => cage.warnings.push(format!(
                "{n} timeline items refer to the cage's records; none is taken"
            )),
        }
    }
}

/// The first position of `needle` in `hay`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cube of six quads (eight vertices, twelve edges, 24 half-edges),
    /// with a deleted slot of each kind, two creased edges at one vertex, a
    /// grip map with one secondary grip and the editor records.
    fn cube() -> String {
        // Vertices 0..7: bottom 0 1 2 3, top 4 5 6 7 (x, y, z in {0, 1}).
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        // Faces as vertex rings, outward.
        let faces: [[usize; 4]; 6] = [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ];
        // Half-edge h = 4 * face + k runs from ring[k] to ring[k + 1]: its
        // terminal vertex is ring[k + 1].
        let mut edges: Vec<(usize, usize)> = Vec::new();
        let mut edge_of = [0usize; 24];
        let mut root_of_edge: Vec<usize> = Vec::new();
        for (f, ring) in faces.iter().enumerate() {
            for k in 0..4 {
                let (a, b) = (ring[k], ring[(k + 1) % 4]);
                let key = (a.min(b), a.max(b));
                let e = match edges.iter().position(|x| *x == key) {
                    Some(e) => e,
                    None => {
                        edges.push(key);
                        root_of_edge.push(4 * f + k);
                        edges.len() - 1
                    }
                };
                edge_of[4 * f + k] = e;
            }
        }
        let mate = |h: usize| -> usize {
            let (f, k) = (h / 4, h % 4);
            let (a, b) = (faces[f][k], faces[f][(k + 1) % 4]);
            (0..24)
                .find(|&g| {
                    let (fg, kg) = (g / 4, g % 4);
                    faces[fg][kg] == b && faces[fg][(kg + 1) % 4] == a
                })
                .unwrap()
        };
        let mut out = String::from("#TS0200\ndegree 3\ncap-type 1\nunits 1 meters\n");
        out += "end-conditions SUBD_CREASES\n";
        // Faces (slot 0 deleted: face f is slot f + 1).
        out += "f\n";
        for f in 0..6 {
            out += &format!("f {} 0\n", 4 * f + 1);
        }
        // Edges (slot 0 deleted); the half-edges name edge slot e + 1.
        out += "e\n";
        for (e, _) in edges.iter().enumerate() {
            out += &format!("e {} 1\n", root_of_edge[e] + 1);
        }
        // Vertices (slot 0 deleted): vertex v is slot v + 1; each fan
        // starts at a half-edge out of it (one that ends at its successor).
        out += "v\n";
        for v in 0..8 {
            let h = (0..24).find(|&h| faces[h / 4][h % 4] == v).unwrap();
            out += &format!("v {} NORTH\n", h + 1);
        }
        // Half-edges (slot 0 deleted): half-edge h is slot h + 1.
        out += "l\n";
        for (h, edge) in edge_of.iter().enumerate() {
            let (f, k) = (h / 4, h % 4);
            let next = 4 * f + (k + 1) % 4;
            let previous = 4 * f + (k + 3) % 4;
            out += &format!(
                "l {} {} {} {} {} {} 0\n",
                next + 1,
                previous + 1,
                mate(h) + 1,
                faces[f][(k + 1) % 4] + 1,
                f + 1,
                edge + 1
            );
        }
        // Creases on the two bottom edges at vertex 0: 0-1 and 0-3.
        for key in [(0, 1), (0, 3)] {
            let e = edges.iter().position(|x| *x == key).unwrap();
            out += &format!("ec {} 0\n", e + 1);
        }
        // The grip map: the deleted grip, the eight vertices' own points
        // and a secondary grip of vertex 6 (slot 7).
        out += "0m odd-grip-map\n0m gv -1\n";
        for v in 0..8 {
            out += &format!("0m gvp {}\n", v + 1);
        }
        out += "0m gv 7\n0m cg 7 4 1 0 0 0 0\n";
        out += "0g\n";
        for p in points {
            out += &format!("0g {} {} {} 1\n", p[0], p[1], p[2]);
        }
        out += "0g 1.25 1.25 1.25 0.5\n";
        out += "105sym 0\n100verts 3\ntol 0.001\ngeom-tol 0.0001\nver 2\n";
        out
    }

    #[test]
    fn a_synthetic_cage_reads() {
        let cage = parse(&cube());
        assert!(cage.warnings.is_empty(), "{:?}", cage.warnings);
        assert_eq!(cage.degree, 3);
        assert_eq!(cage.vertices.len(), 8);
        assert_eq!(cage.faces.len(), 6);
        assert_eq!(cage.edges.len(), 12);
        assert_eq!(cage.half_edges, 25);
        // Deleted slots take their indices.
        assert_eq!(cage.vertex_slots, (1..=8).collect::<Vec<u32>>());
        assert_eq!(cage.face_slots[0], 1);
        // The grips: the vertices' points after the deleted grip.
        assert_eq!(cage.vertices[6].position, [1.0, 1.0, 1.0]);
        assert_eq!(cage.vertices[6].direction, "NORTH");
        assert_eq!(
            cage.grips,
            [CageGrip {
                vertex: 6,
                position: [1.25; 3],
                weight: 0.5
            }]
        );
        // The bottom face's ring: the terminal vertices from its root
        // half-edge (0 → 3 → 2 → 1 → 0).
        assert_eq!(cage.faces[0], [3, 2, 1, 0]);
        // Edge 0 is the bottom face's first: 0 → 3, from its root's mate's
        // vertex to its root's; creased, inside the closed cage.
        assert_eq!(cage.edges[0].vertices, [0, 3]);
        assert!(cage.edges[0].crease && !cage.edges[0].boundary);
        assert_eq!(cage.edges.iter().filter(|e| e.crease).count(), 2);
        // Vertex tags by their creased edges: two at vertex 0, one at 1
        // and 3.
        assert_eq!(cage.vertices[0].tag, VertexTag::Crease);
        assert_eq!(cage.vertices[1].tag, VertexTag::Dart);
        assert_eq!(cage.vertices[3].tag, VertexTag::Dart);
        assert_eq!(cage.vertices[6].tag, VertexTag::Smooth);
        assert_eq!(cage.tolerance, Some(0.001));
        assert_eq!(cage.geometric_tolerance, Some(0.0001));
        assert!(
            cage.settings
                .contains(&("end-conditions".into(), "SUBD_CREASES".into()))
        );
        // Editor records and grip layouts as written.
        let native: Vec<&str> = cage.native.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(native, ["0m cg 7 4 1 0 0 0 0", "105sym 0", "100verts 3"]);
        assert_eq!(cage.grip_box(), Some(([0.0; 3], [1.25; 3])));
    }

    #[test]
    fn grips_follow_the_vertices_without_a_map() {
        // One triangle: three vertices, three edges, six half-edges.
        let text = "#TS0200\ndegree 3\n\
            f 0 0\n\
            e 0 1\ne 1 2\ne 2 1\n\
            v 2 NORTH\nv 0 EAST\nv 1 SOUTH\n\
            l 1 2 3 1 0 0 0\nl 2 0 4 2 0 1 0\nl 0 1 5 0 0 2 0\n\
            l 5 4 0 0 -1 0 0\nl 3 5 1 1 -1 1 0\nl 4 3 2 2 -1 2 0\n\
            0g 0 0 0 1\n0g 2 0 0 1\n0g 0 3 0 1\n";
        let cage = parse(text);
        assert!(cage.warnings.is_empty(), "{:?}", cage.warnings);
        assert_eq!(cage.faces, [vec![1, 2, 0]]);
        assert_eq!(cage.vertices[2].position, [0.0, 3.0, 0.0]);
        assert_eq!(cage.edges[1].knot, 2.0);
        assert!(cage.edges.iter().all(|e| e.boundary && !e.crease));
        assert_eq!(cage.edges[0].vertices, [0, 1]);
    }

    #[test]
    fn a_broken_program_keeps_what_reads() {
        let cage = parse("#TS0200\nv 0 NORTH\nv x NORTH\nf 7 0\n0g 1 2 3 1\n");
        // Two vertex slots, one readable; its grip is slot 0's.
        assert_eq!(cage.vertices.len(), 1);
        assert_eq!(cage.vertices[0].position, [1.0, 2.0, 3.0]);
        assert!(cage.faces.is_empty());
        assert!(cage.warnings.iter().any(|w| w.contains("record 2")));
        assert!(cage.warnings.iter().any(|w| w.contains("faces")));
        assert!(!parse("TS0100\n").warnings.is_empty());
    }

    #[test]
    fn a_cage_joins_its_serializer_and_item() {
        use crate::writer::DesignWriter;
        fn s16(s: &str) -> Vec<u8> {
            let units: Vec<u16> = s.encode_utf16().collect();
            let mut v = (units.len() as u32).to_le_bytes().to_vec();
            v.extend(units.iter().flat_map(|u| u.to_le_bytes()));
            v
        }
        fn r(id: u64) -> Vec<u8> {
            let mut v = vec![1];
            v.extend(id.to_le_bytes());
            v.extend([0, 0]);
            v
        }
        const ANY: &str = "0000AAAA-0000-0000-0000-0000000000C1";
        let mut d = DesignWriter::default();
        d.class(ANY, "", 1);
        let name = "TSpline.0f0e-1.tsm";
        // The Form item (10), its surface (11), the serializer (12) naming
        // the entry and the surface, the record owning the surface (13)
        // that refers to the item, and an unrelated item (14).
        d.obj(10, ANY, vec![0, 0]);
        d.obj(11, ANY, vec![0, 0]);
        d.obj(
            12,
            ANY,
            [&[0u8; 10][..], &s16(name), &r(11), &[0, 0]].concat(),
        );
        d.obj(13, ANY, [&[0u8; 4][..], &r(10), &r(11)].concat());
        d.obj(14, ANY, vec![0, 0]);
        let seg = d.segment();
        let items: HashMap<u64, i64> = [(10, 3), (14, 4)].into_iter().collect();
        let mut cages = vec![
            FormCage {
                name: name.into(),
                ..FormCage::default()
            },
            FormCage {
                name: "TSpline.other.tsm".into(),
                ..FormCage::default()
            },
        ];
        join(&seg, &items, &mut cages);
        assert_eq!(cages[0].serializer, Some(12));
        assert_eq!(cages[0].surface, Some(11));
        assert_eq!((cages[0].item, cages[0].item_index), (Some(10), Some(3)));
        assert!(cages[0].warnings.is_empty(), "{:?}", cages[0].warnings);
        // A stale entry no record names joins nothing.
        assert_eq!(cages[1].serializer, None);
        assert!(!cages[1].warnings.is_empty());
    }

    #[test]
    fn a_document_gives_its_cages() {
        let zip = mitcad_zip::stored_zip(&[(
            "FusionAssetName[Active]/TSplines.BlobParts/TSpline.a.tsm",
            cube().as_bytes(),
        )]);
        let doc = F3dFile::from_bytes(zip).unwrap();
        let mut d = crate::writer::DesignWriter::default();
        d.class("0000AAAA-0000-0000-0000-0000000000C2", "", 1);
        d.obj(1, "0000AAAA-0000-0000-0000-0000000000C2", vec![0, 0]);
        let seg = d.segment();
        let cages = decode(&seg, &doc, &HashMap::new());
        assert_eq!(cages.len(), 1);
        assert_eq!(cages[0].name, "TSpline.a.tsm");
        assert_eq!(
            cages[0].entry,
            "FusionAssetName[Active]/TSplines.BlobParts/TSpline.a.tsm"
        );
        assert_eq!(cages[0].faces.len(), 6);
    }
}
