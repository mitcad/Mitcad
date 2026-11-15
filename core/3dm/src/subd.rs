// SPDX-License-Identifier: MIT
//! SubD objects (mitcad#1757): their control net as the file keeps it, and
//! the net as the control cage of a Form ([`mitcad_subdiv::Cage`]), which
//! the import makes an editable Form feature of.
//!
//! A SubD object's data is a byte that says whether it has a net (0: an
//! empty SubD), then a chunk with a version of two ints (major 1) holding
//! the count of its levels, the largest vertex, edge and face ids, a
//! bounding box (always empty), and a chunk per level (major 1, minor 1 or
//! later). Level 0 is the control net; the later levels are the net
//! subdivided and are not read. After the levels come texture settings,
//! the symmetry and the face packing, which are not read either (the chunk
//! is left with them).
//!
//! A level holds its index (u16), three bytes, a bounding box, then four
//! u32 archive ids that part its components: the vertices from the first
//! to the second, the edges to the third, the faces to the fourth. Each
//! component in turn has the next archive id (the reader checks that they
//! follow on), its own id, its level (u16) and then, from archive version
//! 70, additions: a size byte and that many bytes each, 0 for one not
//! there, 254 for a chunk, 255 the end (before version 70 two optional
//! points behind a flag byte each). A list of components (a vertex's
//! edges and faces, an edge's vertices and faces, a face's edges) is a u16
//! count and per item its archive id (u32) and a flag byte, whose lowest
//! bit says that the item is taken against its own direction.
//!
//! - A vertex: its tag (smooth 1, crease 2, corner 3, dart 4), its point,
//!   the counts of its edges and faces (u16 each), a byte 0 (or a list of
//!   saved limit points), its edge list and its face list, then its
//!   additions (version 70 and later) or a byte 0.
//! - An edge: its tag (smooth 1, crease 2, smooth with sharp ends 4), the
//!   count of its faces (u16), two sector coefficients and the sharpness at
//!   its start (f64s), its vertex list (two), its face list; from archive
//!   version 80 its first addition (8 bytes) is the sharpness at its end. A
//!   smooth edge with a sharpness is a semi-sharp crease.
//! - A face: the id of the level-0 face it comes from, an obsolete int, the
//!   count of its edges (u16) and its edge list (each edge reversed where
//!   the face runs along it from its second vertex to its first), then its
//!   additions (texture domain, material, colour, packing, texture points).
//!
//! Every count is checked against the bytes left in its chunk before it is
//! used, and every archive id against its part of the level, so a damaged
//! file is refused object by object.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mitcad_subdiv::orient::orient;
use mitcad_subdiv::{Cage, CageFace, CageVertex, Crease};

use crate::archive::{Reader, Result, err, err_at};
use crate::geom::P3;

/// A vertex tag: sharp corner.
pub const VERTEX_CORNER: u8 = 3;
/// An edge tag: crease.
pub const EDGE_CREASE: u8 = 2;

/// A SubD's control net (level 0) as the file keeps it; the components
/// refer to each other by their index here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubD {
    pub vertices: Vec<SubDVertex>,
    pub edges: Vec<SubDEdge>,
    pub faces: Vec<SubDFace>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubDVertex {
    /// The SubD's own id of the vertex.
    pub id: u32,
    pub tag: u8,
    pub point: P3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubDEdge {
    pub id: u32,
    pub tag: u8,
    /// Indices into the vertices.
    pub vertices: [usize; 2],
    /// The sharpness at its start and end (archive version 80 and later),
    /// for a smooth edge.
    pub sharpness: Option<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubDFace {
    pub id: u32,
    /// Its edges in order round it: an index into the edges and whether
    /// the face runs along the edge from its second vertex to its first.
    pub edges: Vec<(usize, bool)>,
}

/// The least bytes a vertex, an edge and a face take in a level, for
/// checking their counts before reading them.
const VERTEX_BYTES: usize = 40;
const EDGE_BYTES: usize = 40;
const FACE_BYTES: usize = 20;

/// The additions of a component (archive version 70 and later): the data
/// of each, in order, until the end byte (255); a byte 0 is a slot not
/// there and 254 a chunk, both left out.
fn additions<'a>(r: &mut Reader<'a>) -> Result<Vec<&'a [u8]>> {
    let mut found = Vec::new();
    loop {
        match r.u8()? {
            255 => return Ok(found),
            0 => {}
            254 => {
                r.chunk()?;
            }
            n => found.push(r.bytes(usize::from(n))?),
        }
    }
}

/// The end of a vertex, an edge or a face: its additions (archive version
/// 70 and later) or a byte 0.
fn component_end<'a>(r: &mut Reader<'a>) -> Result<Vec<&'a [u8]>> {
    if r.version >= 70 {
        return additions(r);
    }
    match r.u8()? {
        0 => Ok(Vec::new()),
        b => err_at(
            "a SubD component of a form MitCad does not read",
            format!("the end byte {b} where 0 was expected"),
        ),
    }
}

/// A component's base: its archive id (which must be `expected`), its own
/// id and level, and what follows them. Returns its own id.
fn base(r: &mut Reader, expected: u32) -> Result<u32> {
    let archive_id = r.u32()?;
    if archive_id != expected {
        return err_at(
            "the components of a SubD are not in order",
            format!("archive id {archive_id} where {expected} was expected"),
        );
    }
    let id = r.u32()?;
    r.u16()?; // its level
    if r.version >= 70 {
        additions(r)?;
    } else {
        for _ in 0..2 {
            if r.u8()? != 0 {
                r.skip(24)?;
            }
        }
    }
    Ok(id)
}

/// A list of components: its count and each item's archive id and flags.
fn list(r: &mut Reader) -> Result<Vec<(u32, u8)>> {
    let n = usize::from(r.u16()?);
    if n.saturating_mul(5) > r.remaining() {
        return err_at(
            "a count larger than its data",
            format!(
                "a list of {n} SubD components, {} bytes left",
                r.remaining()
            ),
        );
    }
    (0..n).map(|_| Ok((r.u32()?, r.u8()?))).collect()
}

/// The index of a component by its archive id, in its part `from..to` of
/// the level.
fn index(archive_id: u32, from: u32, to: u32, what: &str) -> Result<usize> {
    if !(from..to).contains(&archive_id) {
        return err_at(
            format!("a SubD refers to {what} it does not have"),
            format!("archive id {archive_id} outside {from}..{to}"),
        );
    }
    Ok((archive_id - from) as usize)
}

/// A SubD object's data (the class chunk's).
pub fn read_subd(r: &mut Reader) -> Result<SubD> {
    if r.u8()? == 0 {
        return Ok(SubD::default());
    }
    let (major, _, mut d) = r.versioned_chunk("a SubD")?;
    if major != 1 {
        return err_at(
            "a SubD of a version MitCad does not read",
            format!("version {major}"),
        );
    }
    let levels = d.i32()?;
    d.skip(12)?; // the largest vertex, edge and face ids
    d.skip(48)?; // a bounding box, always empty
    if levels < 1 {
        return Ok(SubD::default());
    }
    let (major, minor, mut l) = d.versioned_chunk("a SubD's control net")?;
    if major != 1 || minor < 1 {
        return err_at(
            "a SubD of a version MitCad does not read",
            format!("level version {major}.{minor}"),
        );
    }
    if l.u16()? != 0 {
        return err("a SubD's control net is missing");
    }
    l.skip(3)?;
    l.skip(48)?; // the net's bounding box
    let parts = [l.u32()?, l.u32()?, l.u32()?, l.u32()?];
    if parts.windows(2).any(|w| w[0] > w[1]) {
        return err_at(
            "a SubD whose parts are out of order",
            format!("archive ids {parts:?}"),
        );
    }
    let counts = [
        (parts[1] - parts[0]) as usize,
        (parts[2] - parts[1]) as usize,
        (parts[3] - parts[2]) as usize,
    ];
    let least = counts[0]
        .saturating_mul(VERTEX_BYTES)
        .saturating_add(counts[1].saturating_mul(EDGE_BYTES))
        .saturating_add(counts[2].saturating_mul(FACE_BYTES));
    if least > l.remaining() {
        return err_at(
            "a count larger than its data",
            format!(
                "{} vertices, {} edges and {} faces, {} bytes left",
                counts[0],
                counts[1],
                counts[2],
                l.remaining()
            ),
        );
    }
    let mut subd = SubD::default();
    let mut next = parts[0];
    for _ in 0..counts[0] {
        let id = base(&mut l, next)?;
        next += 1;
        let tag = l.u8()?;
        let point = l.point()?;
        l.u16()?; // the count of its edges
        let face_count = l.u16()?;
        if l.u8()? != 0 {
            // Saved limit points: a count, then per point four points and
            // a face.
            let n = l.count(101)?;
            if n == 0 || n > usize::from(face_count) {
                return err_at(
                    "a SubD vertex of a form MitCad does not read",
                    format!("{n} saved limit points for {face_count} faces"),
                );
            }
            l.skip(n * 101)?;
        }
        list(&mut l)?; // its edges
        list(&mut l)?; // its faces
        component_end(&mut l)?;
        subd.vertices.push(SubDVertex { id, tag, point });
    }
    for _ in 0..counts[1] {
        let id = base(&mut l, next)?;
        next += 1;
        let tag = l.u8()?;
        l.u16()?; // the count of its faces
        l.skip(16)?; // sector coefficients
        let start = l.f64()?;
        let ends = list(&mut l)?;
        if ends.len() != 2 {
            return err_at(
                "a SubD edge without two vertices",
                format!("{} vertices", ends.len()),
            );
        }
        let vertices = [
            index(ends[0].0, parts[0], parts[1], "a vertex")?,
            index(ends[1].0, parts[0], parts[1], "a vertex")?,
        ];
        list(&mut l)?;
        let added = component_end(&mut l)?;
        let sharpness = match added.first() {
            Some(end) if l.version >= 80 && end.len() == 8 => {
                let end = f64::from_le_bytes(end[..8].try_into().expect("eight bytes"));
                Some([start, end])
            }
            _ => None,
        };
        subd.edges.push(SubDEdge {
            id,
            tag,
            vertices,
            sharpness,
        });
    }
    for _ in 0..counts[2] {
        let id = base(&mut l, next)?;
        next += 1;
        l.i32()?; // the level-0 face it comes from
        l.i32()?; // obsolete
        let edge_count = l.u16()?;
        let edges = list(&mut l)?;
        if edges.len() != usize::from(edge_count) {
            return err_at(
                "a SubD face whose edges do not add up",
                format!("{} edges listed of {edge_count}", edges.len()),
            );
        }
        let edges = edges
            .into_iter()
            .map(|(a, flags)| Ok((index(a, parts[1], parts[2], "an edge")?, flags & 1 != 0)))
            .collect::<Result<Vec<_>>>()?;
        component_end(&mut l)?;
        subd.faces.push(SubDFace { id, edges });
    }
    Ok(subd)
}

/// A SubD's control net as a Form's cage, with what the import says of it
/// in words (what was not carried as it is).
#[derive(Clone, Debug, PartialEq)]
pub struct SubdCage {
    pub cage: Cage,
    /// What a person reads: in words, without ids or points.
    pub notes: Vec<String>,
}

/// Ids for the items: their own where every one is above 0 and none is
/// repeated, else 1, 2, … in order.
fn ids(own: &[u32]) -> Vec<u32> {
    let distinct: BTreeSet<u32> = own.iter().copied().collect();
    if distinct.len() == own.len() && !distinct.contains(&0) {
        own.to_vec()
    } else {
        (1..=own.len() as u32).collect()
    }
}

/// A SubD's control net as a Form's cage in millimetres (`mm` per unit of
/// the file), its faces counter-clockwise seen from outside. Its creased
/// edges come as creases, sharp for good; a smooth edge with a sharpness
/// as a crease of that sharpness (the mean of its ends' where they differ);
/// its corner vertices as corners. Vertices on no face are left out. The
/// cage keeps the SubD's own ids of its vertices and faces where they are
/// all usable. Err in words when the net makes no cage.
pub fn subd_cage(subd: &SubD, mm: f64) -> std::result::Result<SubdCage, String> {
    if subd.faces.is_empty() {
        return Err("the SubD has no faces".to_owned());
    }
    let mut notes = Vec::new();
    // Each face's loop of vertices, from its edges in order.
    let mut faces = Vec::with_capacity(subd.faces.len());
    for face in &subd.faces {
        let mut lp = Vec::with_capacity(face.edges.len());
        for (k, &(e, reversed)) in face.edges.iter().enumerate() {
            let [a, b] = subd.edges[e].vertices;
            let (from, to) = if reversed { (b, a) } else { (a, b) };
            let (next, next_reversed) = face.edges[(k + 1) % face.edges.len()];
            let [c, d] = subd.edges[next].vertices;
            let next_from = if next_reversed { d } else { c };
            if to != next_from {
                return Err("a face of the SubD does not go round its edges in order".to_owned());
            }
            lp.push(from);
        }
        if lp.len() < 3 {
            return Err("a face of the SubD has fewer than three corners".to_owned());
        }
        if lp.iter().collect::<BTreeSet<_>>().len() != lp.len() {
            return Err("a face of the SubD passes through one vertex twice".to_owned());
        }
        faces.push(lp);
    }
    let mut on_edge: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for face in &faces {
        for k in 0..face.len() {
            let (a, b) = (face[k], face[(k + 1) % face.len()]);
            *on_edge.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    if on_edge.values().any(|&n| n > 2) {
        return Err(
            "an edge of the SubD has more than two faces, which a Form's cage \
                    cannot have"
                .to_owned(),
        );
    }
    let points: Vec<P3> = subd
        .vertices
        .iter()
        .map(|v| v.point.map(|c| c * mm))
        .collect();
    if points.iter().flatten().any(|c| !c.is_finite()) {
        return Err("a vertex of the SubD is not at a finite position".to_owned());
    }
    orient(&points, &mut faces).map_err(|e| format!("the SubD's control net: {e}"))?;

    let used: BTreeSet<usize> = faces.iter().flatten().copied().collect();
    let wire = subd.vertices.len() - used.len();
    if wire == 1 {
        notes.push("a vertex of the SubD on no face was left out".to_owned());
    } else if wire > 1 {
        notes.push(format!(
            "{wire} vertices of the SubD on no face were left out"
        ));
    }
    let used: Vec<usize> = used.into_iter().collect();
    let vertex_ids = ids(&used
        .iter()
        .map(|&v| subd.vertices[v].id)
        .collect::<Vec<_>>());
    let id_of: HashMap<usize, u32> = used.iter().copied().zip(vertex_ids).collect();
    let face_ids = ids(&subd.faces.iter().map(|f| f.id).collect::<Vec<_>>());

    let mut creases = Vec::new();
    let mut varying = 0;
    for edge in &subd.edges {
        let [a, b] = edge.vertices;
        // A boundary is sharp already; an edge on no face is not in the cage.
        if on_edge.get(&(a.min(b), a.max(b))) != Some(&2) {
            continue;
        }
        let sharpness = if edge.tag == EDGE_CREASE {
            None
        } else {
            match edge.sharpness {
                Some([s, t]) if s.is_finite() && t.is_finite() && (s > 0.0 || t > 0.0) => {
                    if s != t {
                        varying += 1;
                    }
                    Some((s.max(0.0) + t.max(0.0)) / 2.0)
                }
                _ => continue,
            }
        };
        let edge = mitcad_subdiv::cage::edge_key(id_of[&a], id_of[&b]);
        if !creases.iter().any(|c: &Crease| c.edge == edge) {
            creases.push(Crease { edge, sharpness });
        }
    }
    creases.sort_by_key(|c| c.edge);
    if varying == 1 {
        notes.push(
            "an edge of the SubD whose sharpness changes along it has the mean of its \
             ends' sharpness"
                .to_owned(),
        );
    } else if varying > 1 {
        notes.push(format!(
            "{varying} edges of the SubD whose sharpness changes along them have the mean \
             of their ends' sharpness"
        ));
    }
    let mut corners: Vec<u32> = used
        .iter()
        .filter(|&&v| subd.vertices[v].tag == VERTEX_CORNER)
        .map(|v| id_of[v])
        .collect();
    corners.sort_unstable();

    let cage = Cage {
        vertices: used
            .iter()
            .map(|&v| CageVertex {
                id: id_of[&v],
                at: points[v],
            })
            .collect(),
        faces: faces
            .iter()
            .zip(face_ids)
            .map(|(lp, id)| CageFace {
                id,
                vertices: lp.iter().map(|v| id_of[v]).collect(),
            })
            .collect(),
        creases,
        corners,
        symmetry: None,
        next_vertex: 0,
        next_face: 0,
    };
    cage.check()
        .map_err(|e| format!("the SubD's control net makes no Form cage: {e}"))?;
    Ok(SubdCage { cage, notes })
}
