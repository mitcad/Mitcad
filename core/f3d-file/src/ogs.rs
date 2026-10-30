// SPDX-License-Identifier: MIT
//! Display meshes of an `.f3d` document: the scene saved with it for
//! viewing (`OGS.BlobFolder/OGS/DefaultScene/{world, Fusion_mesh_000}`),
//! read far enough to give the triangle mesh of every body. The meshes are
//! the writer's own tessellation of its B-rep faces, so they are an independent
//! check of the bodies built from the ASM blobs (volume, area, bounding box).
//!
//! What is decoded (see `OGS_FORMAT.md` for the evidence):
//! - `world` is a serialized scene graph. Objects start with their type name
//!   (u32 character count + UTF-16LE). Every object of a body (its face
//!   records, its face list and the body record itself, which is written
//!   after its children) is preceded by the same 25 bytes:
//!   `u32 1, owner key (12 bytes), f64 number, u8 1`. The owner key ties the
//!   faces to their body; the number of a face is unique within its body.
//! - A `Face` record gives the face's place in a mesh buffer
//!   (`Fusion_mesh_NNN`): byte offset and four counts (position, normal and
//!   texture-coordinate floats, triangle indices), the edges bounding the
//!   face and the face's bounding box (6 x f64).
//! - The mesh buffer holds, per face, interleaved float32 vertices
//!   (position, normal, uv) followed by u32 triangle indices local to the
//!   face. Positions are in centimetres, in the coordinates of the body's
//!   component (the same as the ASM blobs). Faces of hidden bodies have no
//!   mesh.
//! - Some documents keep the face meshes in `stream_mesh_NNN` instead: the
//!   face record then carries an inline `OgsSimpleMesh` with a vertex
//!   format and the byte ranges of the face's vertices and u32 indices.
//!
//! The scene graph itself (groups, components, occurrence transforms) is
//! not decoded; the records are found by their type names and checked
//! against each other (face counts of the face lists, face bounding boxes,
//! buffer bounds). Problems are reported in [`DisplayScene::issues`];
//! nothing here panics on bad input.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use crate::container::{EntryKind, F3dFile};
use crate::zip::ZipError;

/// Millimetres per stored unit (the meshes are in centimetres).
pub const MM_PER_CM: f64 = 10.0;

/// One face of a display body.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayFace {
    /// The number written before the face record; unique within the body
    /// (an f64 holding an integer in every file seen).
    pub number: f64,
    /// Edge indices (into the body's edge list) bounding the face.
    pub edges: Vec<u32>,
    /// Bounding box stored with the face: min x, y, z, max x, y, z (cm).
    pub bbox_cm: [f64; 6],
    /// The face has a mesh (faces of hidden bodies have none).
    pub has_mesh: bool,
    /// The face's vertices in [`DisplayBody::positions_cm`].
    pub vertices: Range<usize>,
    /// The face's triangles in [`DisplayBody::triangles`].
    pub triangles: Range<usize>,
}

/// The display mesh of one body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayBody {
    /// Scene node id written with the body record (the object's address
    /// when the file was saved, e.g. `000002116E9F4EB8`), if found.
    pub node: Option<String>,
    /// Owner key shared by the body record and its faces (12 bytes, hex).
    pub key: String,
    /// Appearance (Protein asset id) of the body, if found.
    pub appearance: Option<String>,
    pub faces: Vec<DisplayFace>,
    /// Number of faces announced by the body's face lists, if found.
    pub declared_faces: Option<usize>,
    /// Vertex positions in centimetres, as stored. Faces are not welded:
    /// a vertex on an edge appears once per face.
    pub positions_cm: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// Vertices lying outside the bounding box stored with their face.
    pub off_bbox_vertices: usize,
}

/// Measures of a triangle mesh, in millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshStats {
    pub triangles: usize,
    /// Triangles with two corners at the same (welded) position.
    pub degenerate_triangles: usize,
    /// Signed volume by the divergence theorem (positive for a closed mesh
    /// with outward normals). For an open mesh the value depends on the
    /// origin; it is taken about the centre of the bounding box.
    pub volume_mm3: f64,
    pub area_mm2: f64,
    /// min x, y, z, max x, y, z; all zero for an empty mesh.
    pub bbox_mm: [f64; 6],
    /// Every edge (after welding coincident positions) is shared by
    /// exactly two triangles.
    pub closed: bool,
    /// Edges used by one triangle only.
    pub boundary_edges: usize,
    /// Edges used by more than two triangles.
    pub nonmanifold_edges: usize,
    /// Edges of two triangles that run the same way along the edge
    /// (inconsistent orientation).
    pub misoriented_edges: usize,
}

/// The display meshes of a document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayScene {
    /// Bodies in the order of their body records in `world`.
    pub bodies: Vec<DisplayBody>,
    /// Records that could not be read or did not agree with each other.
    pub issues: Vec<String>,
}

/// Error while locating the display scene in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OgsError {
    Zip(ZipError),
}

impl fmt::Display for OgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OgsError::Zip(e) => write!(f, "display scene: {e}"),
        }
    }
}

impl std::error::Error for OgsError {}

impl From<ZipError> for OgsError {
    fn from(e: ZipError) -> Self {
        OgsError::Zip(e)
    }
}

const SCENE_DIR: &str = "OGS.BlobFolder/OGS/DefaultScene/";
const MESH_PREFIX: &str = "Fusion_mesh_";
const STREAM_PREFIX: &str = "stream_mesh_";

/// Reads the display scene of a document (`.f3d`, or a document of an
/// `.f3z`). `Ok(None)` when the document has no scene (files saved before
/// the writer added one).
pub fn display_scene(doc: &F3dFile) -> Result<Option<DisplayScene>, OgsError> {
    // Scene folders by asset; the active asset first.
    let mut folders: Vec<String> = Vec::new();
    for e in doc.entries() {
        if let EntryKind::DisplayMesh { name } = &e.kind
            && name == "world"
            && let Some(dir) = e.name.strip_suffix("world")
            && dir.ends_with(SCENE_DIR)
        {
            folders.push(dir.to_string());
        }
    }
    folders.sort_by_key(|d| !d.starts_with("FusionAssetName[Active]/"));
    let Some(dir) = folders.first() else {
        return Ok(None);
    };
    let world = doc.read(&format!("{dir}world"))?;
    // Mesh buffers by number; a gap ends the list. The entries by name
    // (the first of a name, as `F3dFile::read` finds it), so a crafted
    // archive of many buffers is not searched once per buffer
    // (mitcad#934).
    let mut entries = HashMap::new();
    for e in doc.archive().entries() {
        entries.entry(e.name.as_str()).or_insert(e);
    }
    let numbered = |prefix: &str| -> Result<Vec<Vec<u8>>, OgsError> {
        let mut out = Vec::new();
        loop {
            let name = format!("{dir}{prefix}{:03}", out.len());
            let Some(&e) = entries.get(name.as_str()) else {
                return Ok(out);
            };
            out.push(doc.archive().read(e)?);
        }
    };
    let buffers = numbered(MESH_PREFIX)?;
    let streams = numbered(STREAM_PREFIX)?;
    Ok(Some(DisplayScene::parse_with_streams(
        &world, &buffers, &streams,
    )))
}

impl DisplayScene {
    /// Parses the scene `world` with its mesh buffers (`Fusion_mesh_000`,
    /// `_001`, ... in order).
    pub fn parse(world: &[u8], buffers: &[Vec<u8>]) -> DisplayScene {
        DisplayScene::parse_with_streams(world, buffers, &[])
    }

    /// Parses the scene `world` with its mesh buffers (`Fusion_mesh_NNN`)
    /// and its stream buffers (`stream_mesh_NNN`), each in order.
    pub fn parse_with_streams(
        world: &[u8],
        buffers: &[Vec<u8>],
        streams: &[Vec<u8>],
    ) -> DisplayScene {
        let buffers = Buffers {
            packed: buffers,
            stream: streams,
        };
        let mut issues = Vec::new();

        // Face records.
        let formats = vertex_formats(world);
        let mut faces = Vec::new();
        let face_tag = type_tag("Face");
        for at in find_all(world, &face_tag) {
            let Some((key, number)) = member_prefix(world, at) else {
                continue;
            };
            match parse_face(world, at + face_tag.len(), &formats) {
                Ok(rec) => faces.push((at, key, number, rec)),
                Err(e) => issues.push(format!("face record at {at:#x}: {e}")),
            }
        }

        // Body records (written after the body's children).
        let mut tails: Vec<(usize, [u8; 12])> = Vec::new();
        for at in find_all(world, &type_tag("Body")) {
            if let Some((key, _)) = member_prefix(world, at) {
                tails.push((at, key));
            }
        }

        // Face lists: their face counts, per owner key.
        let mut declared: HashMap<[u8; 12], usize> = HashMap::new();
        let list_tag = type_tag("Faces");
        for at in find_all(world, &list_tag) {
            let Some((key, _)) = member_prefix(world, at) else {
                continue;
            };
            let e = at + list_tag.len();
            if let (Some(3), Some(0x10), Some(n)) = (
                read_u32(world, e + 29),
                read_u32(world, e + 41),
                read_u32(world, e + 45),
            ) {
                *declared.entry(key).or_insert(0) += n as usize;
            }
        }

        // Each face belongs to the first body record after it with the same
        // key (else the last one before it). Faces whose key has no body
        // record are kept in a body of their own.
        let mut bodies: Vec<DisplayBody> = tails
            .iter()
            .map(|&(at, key)| DisplayBody {
                node: node_id_before(world, at),
                key: hex(&key),
                ..DisplayBody::default()
            })
            .collect();
        let mut keys: Vec<[u8; 12]> = tails.iter().map(|&(_, k)| k).collect();
        let mut first_face: Vec<Option<usize>> = vec![None; bodies.len()];
        let mut orphans: HashMap<[u8; 12], usize> = HashMap::new();
        // The body records of each key, in stream order (`find_all` finds
        // them in increasing offsets): a face finds its owner by a binary
        // search, not by a scan of every record (mitcad#934).
        let mut by_key: HashMap<[u8; 12], Vec<usize>> = HashMap::new();
        for (i, &(_, k)) in tails.iter().enumerate() {
            by_key.entry(k).or_default().push(i);
        }
        for (at, key, number, rec) in faces {
            let owner = by_key
                .get(&key)
                .and_then(|list| {
                    let after = list.partition_point(|&i| tails[i].0 <= at);
                    list.get(after).or(list.last()).copied()
                })
                .or_else(|| orphans.get(&key).copied());
            let bi = owner.unwrap_or_else(|| {
                issues.push(format!("faces with key {} have no body record", hex(&key)));
                bodies.push(DisplayBody {
                    key: hex(&key),
                    ..DisplayBody::default()
                });
                keys.push(key);
                first_face.push(None);
                orphans.insert(key, bodies.len() - 1);
                bodies.len() - 1
            });
            first_face[bi] = Some(first_face[bi].map_or(at, |f| f.min(at)));
            if let Err(e) = add_face(&mut bodies[bi], number, &rec, buffers) {
                issues.push(format!("face record at {at:#x}: {e}"));
            }
        }

        // Face counts and appearances.
        for (bi, body) in bodies.iter_mut().enumerate() {
            body.declared_faces = declared.get(&keys[bi]).copied();
            if let Some(n) = body.declared_faces
                && n != body.faces.len()
            {
                issues.push(format!(
                    "body {bi}: face lists declare {n} faces, {} found",
                    body.faces.len()
                ));
            }
            if let Some(first) = first_face[bi] {
                // The last body record before the first face (the records
                // are in increasing offsets).
                let before = tails.partition_point(|&(t, _)| t < first);
                let start = before.checked_sub(1).map_or(0, |i| tails[i].0);
                body.appearance = appearance_between(world, start, first);
            }
        }

        DisplayScene { bodies, issues }
    }
}

impl DisplayBody {
    /// Faces that have no mesh.
    pub fn faces_without_mesh(&self) -> usize {
        self.faces.iter().filter(|f| !f.has_mesh).count()
    }

    /// Measures of the body's mesh in millimetres.
    pub fn stats(&self) -> MeshStats {
        mesh_stats(&self.positions_cm, &self.triangles)
    }
}

/// The location of a face's mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeshRef {
    /// Layout 7: interleaved vertices followed by the indices, in a
    /// `Fusion_mesh_NNN` buffer.
    Packed {
        buffer: u32,
        offset: u32,
        position_floats: u32,
        normal_floats: u32,
        uv_floats: u32,
        indices: u32,
    },
    /// Layout 0: vertices and indices described by an inline
    /// `OgsSimpleMesh`, in `stream_mesh_NNN` buffers.
    Stream {
        vertices: Span,
        format: VertexFormat,
        indices: Span,
    },
}

/// A byte range of a numbered buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    buffer: u32,
    offset: u32,
    bytes: u32,
}

/// A vertex layout of the stream buffers: floats per vertex and the float
/// offset of the position within a vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VertexFormat {
    stride: u32,
    position: u32,
}

/// A `Face` record after its type name.
#[derive(Clone, Debug, PartialEq)]
struct FaceRecord {
    mesh: Option<MeshRef>,
    edges: Vec<u32>,
    bbox: [f64; 6],
}

/// Flag byte of a face record: the face has a mesh.
const FACE_HAS_MESH: u8 = 0x08;
/// Vertex layout of faces in `Fusion_mesh_NNN`: interleaved position,
/// normal and uv floats, followed by triangle indices (four counts).
const FACE_LAYOUT: u32 = 7;
/// Vertex layout of faces in `stream_mesh_NNN`: an inline `OgsSimpleMesh`
/// object with a vertex format and vertex and index buffer ranges.
const FACE_LAYOUT_STREAM: u32 = 0;
/// Primitive type of an `OgsSimpleMesh` holding a triangle list.
const TRIANGLE_LIST: u32 = 4;

/// Vertex formats by id: where the format is first written (its definition)
/// and the format, or why it cannot be used.
type VertexFormats = HashMap<String, (usize, Result<VertexFormat, String>)>;

/// The bytes that start a `VertexFormat` object: flag 1 and its type name.
fn vertex_format_tag() -> Vec<u8> {
    let mut tag = vec![1];
    tag.extend(type_tag("VertexFormat"));
    tag
}

/// Finds the vertex formats of `world`. A `VertexFormat` object is written
/// as flag 1, its type name and its id string; the first object with an id
/// defines it (u32 attribute count, then per attribute 6 x u32: type,
/// semantic, float offset within the vertex, three zeros), later ones only
/// repeat the id. Types: 2 three floats, 1 two floats; semantics: 0
/// position, 1 normal, 2 texture coordinates, 3 and 4 further vectors.
fn vertex_formats(d: &[u8]) -> VertexFormats {
    let tag = vertex_format_tag();
    let mut out = VertexFormats::new();
    for at in find_all(d, &tag) {
        let Some(id) = read_utf16_string(d, at + tag.len()) else {
            continue;
        };
        if out.contains_key(&id) {
            continue;
        }
        let attrs_at = at + tag.len() + 4 + 2 * id.len();
        out.insert(id, (at, read_vertex_format(d, attrs_at).map(|(f, _)| f)));
    }
    out
}

/// Reads a vertex format definition (attribute count and attributes) at
/// `at`; returns the format and the end of the definition.
fn read_vertex_format(d: &[u8], at: usize) -> Result<(VertexFormat, usize), String> {
    let n = read_u32(d, at).ok_or("truncated")?;
    if n == 0 || n > 16 {
        return Err(format!("vertex format with {n} attributes"));
    }
    let mut stride = 0;
    let mut position = None;
    for i in 0..n as usize {
        let field = |k: usize| read_u32(d, at + 4 + 24 * i + 4 * k).ok_or("truncated");
        let (kind, semantic, offset) = (field(0)?, field(1)?, field(2)?);
        let floats = match kind {
            1 => 2,
            2 => 3,
            other => return Err(format!("unknown vertex attribute type {other}")),
        };
        if offset > 64 {
            return Err(format!("vertex attribute at float {offset}"));
        }
        if semantic == 0 && kind == 2 {
            position = Some(offset);
        }
        stride = stride.max(offset + floats);
    }
    let position = position.ok_or("vertex format without positions")?;
    Ok((VertexFormat { stride, position }, at + 4 + 24 * n as usize))
}

/// Reads `(buffer, offset, bytes)` at `at`.
fn read_span(d: &[u8], at: usize) -> Option<Span> {
    Some(Span {
        buffer: read_u32(d, at)?,
        offset: read_u32(d, at + 4)?,
        bytes: read_u32(d, at + 8)?,
    })
}

/// Expects the object header `flag 1, type name` at `at`; returns its end.
fn expect_object(d: &[u8], at: usize, name: &str) -> Result<usize, String> {
    let tag = type_tag(name);
    if read_u8(d, at) != Some(1) || d.get(at + 1..at + 1 + tag.len()) != Some(&tag[..]) {
        return Err(format!("no {name} at {at:#x}"));
    }
    Ok(at + 1 + tag.len())
}

/// Parses the inline mesh of a layout-0 face, starting at `at`: an
/// `OgsSimpleMesh` (6 x f32 box, u32 primitive type 4, 2 bytes 1 0), its
/// `VertexFormat` (id, the definition the first time, u32 stream count 1,
/// then buffer, byte offset and byte count of the vertices) and its
/// `AIndexBuffer` (id, u8 0, buffer, byte offset and byte count of the u32
/// indices), then 5 zero bytes. Returns the mesh and the end.
fn parse_stream_mesh(
    d: &[u8],
    at: usize,
    formats: &VertexFormats,
) -> Result<(MeshRef, usize), String> {
    let mut o = expect_object(d, at, "OgsSimpleMesh")? + 24;
    let primitive = read_u32(d, o).ok_or("truncated")?;
    if primitive != TRIANGLE_LIST {
        return Err(format!("unknown primitive type {primitive}"));
    }
    o += 6;
    let format_at = o;
    o = expect_object(d, o, "VertexFormat")?;
    let id = read_utf16_string(d, o).ok_or("no vertex format id")?;
    o += 4 + 2 * id.len();
    let (first, format) = formats
        .get(&id)
        .ok_or_else(|| format!("unknown vertex format {id}"))?;
    let format = format
        .clone()
        .map_err(|e| format!("vertex format {id}: {e}"))?;
    if *first == format_at {
        o = read_vertex_format(d, o)?.1;
    }
    let streams = read_u32(d, o).ok_or("truncated")?;
    if streams != 1 {
        return Err(format!("{streams} vertex streams"));
    }
    let vertices = read_span(d, o + 4).ok_or("truncated")?;
    o = expect_object(d, o + 16, "AIndexBuffer")?;
    let id = read_utf16_string(d, o).ok_or("no index buffer id")?;
    o += 4 + 2 * id.len();
    let kind = read_u8(d, o).ok_or("truncated")?;
    if kind != 0 {
        return Err(format!("unknown index buffer type {kind}"));
    }
    let indices = read_span(d, o + 1).ok_or("truncated")?;
    o += 13;
    if d.get(o..o + 5) != Some(&[0u8; 5][..]) {
        return Err(format!("unknown data after the index buffer at {o:#x}"));
    }
    let mesh = MeshRef::Stream {
        vertices,
        format,
        indices,
    };
    Ok((mesh, o + 5))
}

/// Parses a `Face` record starting `e` bytes into `d` (after the type name).
///
/// Layout (offsets from `e`): 46 bytes common to all faces (not decoded),
/// u8 flags, u16 0; with a mesh, u32 layout and either (layout 7) u32
/// buffer number, u32 byte offset, u32 x 4 counts, u32 n, n x u32 edge
/// indices, or (layout 0) an inline mesh object, see [`parse_stream_mesh`];
/// then 6 x f64 box.
fn parse_face(d: &[u8], e: usize, formats: &VertexFormats) -> Result<FaceRecord, String> {
    let flags = read_u8(d, e + 46).ok_or("truncated")?;
    let (mesh, edges, bbox_at) = match flags {
        0 => (None, Vec::new(), e + 49),
        FACE_HAS_MESH => match read_u32(d, e + 49).ok_or("truncated")? {
            FACE_LAYOUT => {
                let field = |i: usize| read_u32(d, e + 53 + 4 * i).ok_or("truncated");
                let mesh = MeshRef::Packed {
                    buffer: field(0)?,
                    offset: field(1)?,
                    position_floats: field(2)?,
                    normal_floats: field(3)?,
                    uv_floats: field(4)?,
                    indices: field(5)?,
                };
                let n = field(6)? as usize;
                let list = e + 81;
                if n > d.len().saturating_sub(list) / 4 {
                    return Err(format!("edge count {n} beyond the data"));
                }
                let edges = (0..n)
                    .map(|i| read_u32(d, list + 4 * i).ok_or("truncated"))
                    .collect::<Result<Vec<_>, _>>()?;
                (Some(mesh), edges, list + 4 * n)
            }
            FACE_LAYOUT_STREAM => {
                let (mesh, end) = parse_stream_mesh(d, e + 53, formats)?;
                (Some(mesh), Vec::new(), end)
            }
            layout => return Err(format!("unknown vertex layout {layout}")),
        },
        other => return Err(format!("unknown face flags {other:#x}")),
    };
    let mut bbox = [0.0; 6];
    for (i, v) in bbox.iter_mut().enumerate() {
        *v = read_f64(d, bbox_at + 8 * i).ok_or("truncated")?;
    }
    if !bbox.iter().all(|v| v.is_finite()) || (0..3).any(|i| bbox[i] > bbox[i + 3]) {
        return Err("invalid bounding box".into());
    }
    Ok(FaceRecord { mesh, edges, bbox })
}

/// Vertex positions and triangles (indices into the positions).
type Mesh = (Vec<[f32; 3]>, Vec<[u32; 3]>);

/// The mesh buffers of a scene: `Fusion_mesh_NNN` and `stream_mesh_NNN`.
#[derive(Clone, Copy)]
struct Buffers<'a> {
    packed: &'a [Vec<u8>],
    stream: &'a [Vec<u8>],
}

/// The bytes `start..start + len` of a buffer, if they lie in it.
fn buffer_bytes(buf: &[u8], start: u64, len: u64) -> Result<&[u8], String> {
    let end = start + len;
    if end > buf.len() as u64 {
        return Err(format!(
            "bytes {start}..{end} beyond the buffer ({})",
            buf.len()
        ));
    }
    // Within the buffer, hence in usize.
    Ok(&buf[start as usize..end as usize])
}

/// Reads one face's vertices and triangles from its buffers.
fn read_face_mesh(buffers: Buffers<'_>, m: &MeshRef) -> Result<Mesh, String> {
    match *m {
        MeshRef::Packed {
            buffer,
            offset,
            position_floats,
            normal_floats,
            uv_floats,
            indices,
        } => {
            let buf = buffers
                .packed
                .get(buffer as usize)
                .ok_or_else(|| format!("no mesh buffer {buffer}"))?;
            // Sizes in u64 first: the counts are untrusted.
            let np = u64::from(position_floats);
            let ni = u64::from(indices);
            let all_floats = np + u64::from(normal_floats) + u64::from(uv_floats);
            if !np.is_multiple_of(3) {
                return Err(format!("{np} position floats"));
            }
            if np == 0 {
                return if ni == 0 && all_floats == 0 {
                    Ok((Vec::new(), Vec::new()))
                } else {
                    Err("indices without vertices".into())
                };
            }
            if !all_floats.is_multiple_of(np / 3) {
                return Err(format!("{all_floats} floats for {} vertices", np / 3));
            }
            let data = buffer_bytes(buf, u64::from(offset), 4 * (all_floats + ni))?;
            let (vertices, indices) = data.split_at(4 * all_floats as usize);
            let format = VertexFormat {
                stride: (all_floats / (np / 3)) as u32,
                position: 0,
            };
            read_triangles(vertices, format, indices)
        }
        MeshRef::Stream {
            vertices,
            format,
            indices,
        } => {
            let span = |s: Span| {
                let buf = buffers
                    .stream
                    .get(s.buffer as usize)
                    .ok_or_else(|| format!("no stream buffer {}", s.buffer))?;
                buffer_bytes(buf, u64::from(s.offset), u64::from(s.bytes))
            };
            let (v, i) = (span(vertices)?, span(indices)?);
            if !v.len().is_multiple_of(4 * format.stride as usize) {
                return Err(format!(
                    "{} vertex bytes for {} floats per vertex",
                    v.len(),
                    format.stride
                ));
            }
            if !i.len().is_multiple_of(4) {
                return Err(format!("{} index bytes", i.len()));
            }
            read_triangles(v, format, i)
        }
    }
}

/// Reads whole vertices of `format` from `vertices` and the u32 triangle
/// list from `indices` (both little-endian, already bounds-checked).
fn read_triangles(vertices: &[u8], format: VertexFormat, indices: &[u8]) -> Result<Mesh, String> {
    let stride = format.stride as usize;
    let nv = vertices.len() / (4 * stride);
    let ni = indices.len() / 4;
    if !ni.is_multiple_of(3) {
        return Err(format!("{ni} indices"));
    }
    if nv == 0 && ni > 0 {
        return Err("indices without vertices".into());
    }
    let word = |d: &[u8], i: usize| [d[4 * i], d[4 * i + 1], d[4 * i + 2], d[4 * i + 3]];
    let float = |i: usize| f32::from_le_bytes(word(vertices, i));
    let p = format.position as usize;
    let positions: Vec<[f32; 3]> = (0..nv)
        .map(|v| {
            [
                float(v * stride + p),
                float(v * stride + p + 1),
                float(v * stride + p + 2),
            ]
        })
        .collect();
    if positions.iter().flatten().any(|c| !c.is_finite()) {
        return Err("non-finite position".into());
    }
    let index = |i: usize| u32::from_le_bytes(word(indices, i));
    let triangles: Vec<[u32; 3]> = (0..ni / 3)
        .map(|t| [index(3 * t), index(3 * t + 1), index(3 * t + 2)])
        .collect();
    if let Some(bad) = triangles.iter().flatten().find(|&&i| i as usize >= nv) {
        return Err(format!("index {bad} beyond {nv} vertices"));
    }
    Ok((positions, triangles))
}

/// Appends a face (and its mesh) to a body.
fn add_face(
    body: &mut DisplayBody,
    number: f64,
    rec: &FaceRecord,
    buffers: Buffers<'_>,
) -> Result<(), String> {
    let v0 = body.positions_cm.len();
    let t0 = body.triangles.len();
    let mut result = Ok(());
    if let Some(m) = &rec.mesh {
        match read_face_mesh(buffers, m) {
            Ok((positions, _)) if v0 + positions.len() > u32::MAX as usize => {
                result = Err("more than 2^32 vertices in a body".into());
            }
            Ok((positions, triangles)) => {
                let tol = |v: f64| 1e-4 + 1e-6 * v.abs();
                body.off_bbox_vertices += positions
                    .iter()
                    .filter(|p| {
                        (0..3).any(|i| {
                            let c = f64::from(p[i]);
                            c < rec.bbox[i] - tol(rec.bbox[i])
                                || c > rec.bbox[i + 3] + tol(rec.bbox[i + 3])
                        })
                    })
                    .count();
                let base = v0 as u32;
                body.positions_cm.extend(positions);
                body.triangles
                    .extend(triangles.into_iter().map(|t| t.map(|i| i + base)));
            }
            Err(e) => result = Err(e),
        }
    }
    body.faces.push(DisplayFace {
        number,
        edges: rec.edges.clone(),
        bbox_cm: rec.bbox,
        has_mesh: rec.mesh.is_some() && result.is_ok(),
        vertices: v0..body.positions_cm.len(),
        triangles: t0..body.triangles.len(),
    });
    result
}

/// Measures a triangle mesh given in centimetres; results in millimetres.
/// Coincident positions are welded with a tolerance of 1e-6 of the
/// bounding-box diagonal before the edges are counted. Triangles with an
/// index out of range are ignored.
pub fn mesh_stats(positions_cm: &[[f32; 3]], triangles: &[[u32; 3]]) -> MeshStats {
    let p: Vec<[f64; 3]> = positions_cm
        .iter()
        .map(|q| q.map(|c| f64::from(c) * MM_PER_CM))
        .collect();
    let tris: Vec<[usize; 3]> = triangles
        .iter()
        .map(|t| t.map(|i| i as usize))
        .filter(|t| t.iter().all(|&i| i < p.len()))
        .collect();
    let mut s = MeshStats {
        triangles: tris.len(),
        ..MeshStats::default()
    };
    if tris.is_empty() {
        return s;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for t in &tris {
        for &i in t {
            for k in 0..3 {
                lo[k] = lo[k].min(p[i][k]);
                hi[k] = hi[k].max(p[i][k]);
            }
        }
    }
    s.bbox_mm = [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]];
    let centre = [0, 1, 2].map(|k| 0.5 * (lo[k] + hi[k]));
    use mitcad_vec3::{cross, dot, sub};
    for t in &tris {
        let [a, b, c] = t.map(|i| sub(p[i], centre));
        s.volume_mm3 += dot(a, cross(b, c)) / 6.0;
        let n = cross(sub(b, a), sub(c, a));
        s.area_mm2 += 0.5 * dot(n, n).sqrt();
    }

    let diag = dot(sub(hi, lo), sub(hi, lo)).sqrt();
    let welded = weld(&p, (1e-6 * diag).max(1e-9));
    // Per undirected edge: uses in the direction low -> high and back.
    let mut edges: HashMap<(usize, usize), (u32, u32)> = HashMap::new();
    for t in &tris {
        let w = t.map(|i| welded[i]);
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            s.degenerate_triangles += 1;
            continue;
        }
        for (a, b) in [(w[0], w[1]), (w[1], w[2]), (w[2], w[0])] {
            let e = edges.entry((a.min(b), a.max(b))).or_insert((0, 0));
            if a < b {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
    }
    for &(f, b) in edges.values() {
        match f + b {
            1 => s.boundary_edges += 1,
            2 if f != 1 => s.misoriented_edges += 1,
            2 => {}
            _ => s.nonmanifold_edges += 1,
        }
    }
    s.closed = s.boundary_edges == 0 && s.nonmanifold_edges == 0;
    s
}

/// Maps each point to the first earlier point within `tol` (max norm).
fn weld(p: &[[f64; 3]], tol: f64) -> Vec<usize> {
    // Cell coordinates clamped well inside i64 (mitcad#934): a far-away
    // point (1e30 cm at the smallest tolerance) saturated the cast to
    // i64::MAX and the neighbour search below overflowed adding 1. Points
    // beyond the clamp share border cells; the search still compares the
    // real coordinates, so the welding stays the same.
    const LIMIT: f64 = (1u64 << 60) as f64;
    let cell = |q: &[f64; 3]| q.map(|c| (c / tol).floor().clamp(-LIMIT, LIMIT) as i64);
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    let mut out = Vec::with_capacity(p.len());
    for (i, q) in p.iter().enumerate() {
        let c = cell(q);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &j in list {
                            if (0..3).all(|k| (p[j][k] - q[k]).abs() <= tol) {
                                found = Some(j);
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        match found {
            Some(j) => out.push(j),
            None => {
                grid.entry(c).or_default().push(i);
                out.push(i);
            }
        }
    }
    out
}

/// The bytes that start an object of type `name`: u32 character count and
/// the name in UTF-16LE.
fn type_tag(name: &str) -> Vec<u8> {
    let mut out = (name.encode_utf16().count() as u32).to_le_bytes().to_vec();
    out.extend(name.encode_utf16().flat_map(|c| c.to_le_bytes()));
    out
}

/// The 25 bytes before a member object of a body: `u32 1`, owner key
/// (12 bytes), f64 number, `u8 1`. Returns the key and the number.
fn member_prefix(d: &[u8], at: usize) -> Option<([u8; 12], f64)> {
    let start = at.checked_sub(25)?;
    if d.get(at - 1) != Some(&1) || read_u32(d, start) != Some(1) {
        return None;
    }
    let key: [u8; 12] = d.get(start + 4..start + 16)?.try_into().ok()?;
    let number = read_f64(d, start + 16)?;
    Some((key, number))
}

/// The nearest scene node id (u32 16 + 16 upper-case hex digits in
/// UTF-16LE) ending within 1 KiB before `at`.
fn node_id_before(d: &[u8], at: usize) -> Option<String> {
    // A string starting at p ends at p + 36 <= at.
    let last = at.checked_sub(36)?;
    (at.saturating_sub(1024)..=last).rev().find_map(|p| {
        read_utf16_string(d, p).filter(|s| {
            s.len() == 16
                && s.chars()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase() && c.is_ascii_hexdigit())
        })
    })
}

/// The value of the last `FPrioritizedProteinAttribute` in `d[start..end]`:
/// five bytes after the name, a UTF-16 string.
fn appearance_between(d: &[u8], start: usize, end: usize) -> Option<String> {
    let tag = type_tag("FPrioritizedProteinAttribute");
    let hay = d.get(start..end)?;
    let at = start + find_all(hay, &tag).into_iter().last()?;
    read_utf16_string(d, at + tag.len() + 5)
}

/// A u32 character count followed by that many printable ASCII characters
/// in UTF-16LE (at most 256).
fn read_utf16_string(d: &[u8], at: usize) -> Option<String> {
    let n = read_u32(d, at)? as usize;
    if n == 0 || n > 256 {
        return None;
    }
    let bytes = d.get(at + 4..at + 4 + 2 * n)?;
    (0..n)
        .map(|i| {
            let (lo, hi) = (bytes[2 * i], bytes[2 * i + 1]);
            (hi == 0 && (0x20..0x7f).contains(&lo)).then_some(lo as char)
        })
        .collect()
}

fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || hay.len() < needle.len() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i] == needle[0] && &hay[i..i + needle.len()] == needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn read_u8(d: &[u8], at: usize) -> Option<u8> {
    d.get(at).copied()
}

fn read_u32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        d.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_f64(d: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(
        d.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes scene records the way the `world` stream lays them out.
    struct World {
        d: Vec<u8>,
    }

    const KEY: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x04, 0x94];

    impl World {
        fn new() -> World {
            // Some unrelated leading content.
            let mut d = type_tag("ARenderList");
            d.extend([5, 0, 0, 0, 1, 0, 0, 0]);
            World { d }
        }

        fn prefix(&mut self, key: [u8; 12], number: f64) {
            self.d.extend([2, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0]);
            self.d.extend(1u32.to_le_bytes());
            self.d.extend(key);
            self.d.extend(number.to_le_bytes());
            self.d.push(1);
        }

        fn string(&mut self, s: &str) {
            self.d.extend(type_tag(s));
        }

        fn faces_list(&mut self, key: [u8; 12], count: u32) {
            self.prefix(key, 0.0);
            self.string("Faces");
            self.d.extend([0u8; 25]);
            self.d.extend(1.0f32.to_le_bytes());
            self.d.extend(3u32.to_le_bytes());
            self.d.extend([0, 0, 0, 0, 0, 0, 0x80, 0]);
            self.d.extend(0x10u32.to_le_bytes());
            self.d.extend(count.to_le_bytes());
        }

        fn face(
            &mut self,
            key: [u8; 12],
            number: f64,
            mesh: Option<(u32, [u32; 4])>,
            bbox: [f64; 6],
        ) {
            self.prefix(key, number);
            self.string("Face");
            self.d.extend([0u8; 25]);
            self.d.extend(1.0f32.to_le_bytes());
            self.d.extend([0u8; 17]);
            match mesh {
                Some((offset, counts)) => {
                    self.d.extend([FACE_HAS_MESH, 0, 0]);
                    self.d.extend(FACE_LAYOUT.to_le_bytes());
                    self.d.extend(0u32.to_le_bytes());
                    self.d.extend(offset.to_le_bytes());
                    for c in counts {
                        self.d.extend(c.to_le_bytes());
                    }
                    self.d.extend(2u32.to_le_bytes());
                    self.d.extend(0u32.to_le_bytes());
                    self.d.extend(1u32.to_le_bytes());
                }
                None => self.d.extend([0, 0, 0]),
            }
            for v in bbox {
                self.d.extend(v.to_le_bytes());
            }
            self.d.push(0);
        }

        /// A layout-0 face: its mesh is an inline `OgsSimpleMesh` whose
        /// data lie in `stream_mesh_000`. The first face with a vertex
        /// format id writes the format's definition.
        fn stream_face(
            &mut self,
            key: [u8; 12],
            number: f64,
            define_format: bool,
            (vertices, indices): (Span, Span),
            bbox: [f64; 6],
        ) {
            self.prefix(key, number);
            self.string("Face");
            self.d.extend([0u8; 25]);
            self.d.extend(1.0f32.to_le_bytes());
            self.d.extend([0u8; 17]);
            self.d.extend([FACE_HAS_MESH, 0, 0]);
            self.d.extend(FACE_LAYOUT_STREAM.to_le_bytes());
            self.d.push(1);
            self.string("OgsSimpleMesh");
            for v in [-1.0f32, -1.0, -1.0, 2.0, 2.0, 2.0] {
                self.d.extend(v.to_le_bytes());
            }
            self.d.extend(TRIANGLE_LIST.to_le_bytes());
            self.d.extend([1, 0]);
            self.d.push(1);
            self.string("VertexFormat");
            self.string("00000367CCB9E090");
            if define_format {
                // Position, normal, two further vectors and uv: 14 floats.
                let attrs = [
                    (2u32, 0u32, 0u32),
                    (2, 1, 3),
                    (2, 3, 8),
                    (2, 4, 11),
                    (1, 2, 6),
                ];
                self.d.extend((attrs.len() as u32).to_le_bytes());
                for (kind, semantic, offset) in attrs {
                    for v in [kind, semantic, offset, 0, 0, 0] {
                        self.d.extend(v.to_le_bytes());
                    }
                }
            }
            self.d.extend(1u32.to_le_bytes());
            self.span(vertices);
            self.d.push(1);
            self.string("AIndexBuffer");
            self.string(&format!("{:016X}", 0x367_0000_0000u64 + number as u64));
            self.d.push(0);
            self.span(indices);
            self.d.extend([0u8; 5]);
            for v in bbox {
                self.d.extend(v.to_le_bytes());
            }
            self.d.push(0);
        }

        fn span(&mut self, s: Span) {
            for v in [s.buffer, s.offset, s.bytes] {
                self.d.extend(v.to_le_bytes());
            }
        }

        fn body(&mut self, key: [u8; 12], node: &str) {
            self.string("TextureMappingAttribute");
            self.d.extend([0, 0, 0, 0, 0, 1]);
            self.string(node);
            self.d.extend([0, 0, 0, 0]);
            self.d.extend(1u32.to_le_bytes());
            self.d.extend(key);
            self.d.extend(0.0f64.to_le_bytes());
            self.d.push(1);
            self.string("Body");
            self.d.extend([0u8; 45]);
        }

        fn appearance(&mut self, id: &str) {
            self.string("FPrioritizedProteinAttribute");
            self.d.extend([0u8; 5]);
            self.string(id);
        }
    }

    /// Appends one face mesh (8 floats per vertex, u32 indices) to `buf`;
    /// returns its offset and counts.
    fn mesh(buf: &mut Vec<u8>, points: &[[f32; 3]], tris: &[[u32; 3]]) -> (u32, [u32; 4]) {
        let offset = buf.len() as u32;
        for p in points {
            for c in p.iter().chain(&[0.0, 0.0, 1.0, 0.5, 0.5]) {
                buf.extend(c.to_le_bytes());
            }
        }
        for t in tris {
            for i in t {
                buf.extend(i.to_le_bytes());
            }
        }
        let n = points.len() as u32;
        (offset, [3 * n, 3 * n, 2 * n, 3 * tris.len() as u32])
    }

    /// Appends one face mesh to a stream buffer (14 floats per vertex, then
    /// the u32 indices); returns the vertex and index ranges.
    fn stream_mesh(buf: &mut Vec<u8>, points: &[[f32; 3]], tris: &[[u32; 3]]) -> (Span, Span) {
        let start = buf.len() as u32;
        for p in points {
            let rest = [0.0, 0.0, 1.0, 0.5, 0.5, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
            for c in p.iter().chain(&rest) {
                buf.extend(c.to_le_bytes());
            }
        }
        let mid = buf.len() as u32;
        for t in tris {
            for i in t {
                buf.extend(i.to_le_bytes());
            }
        }
        let span = |offset: u32, end: u32| Span {
            buffer: 0,
            offset,
            bytes: end - offset,
        };
        (span(start, mid), span(mid, buf.len() as u32))
    }

    fn bbox_of(points: &[[f32; 3]]) -> [f64; 6] {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in points {
            for k in 0..3 {
                b[k] = b[k].min(f64::from(p[k]));
                b[k + 3] = b[k + 3].max(f64::from(p[k]));
            }
        }
        b
    }

    /// A 1 cm cube as six faces of two triangles each, outward normals.
    fn cube_faces() -> Vec<Mesh> {
        let quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| {
            (vec![a, b, c, d], vec![[0, 1, 2], [0, 2, 3]])
        };
        vec![
            quad([0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]),
            quad([0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]),
            quad([0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]),
            quad([0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]),
            quad([0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]),
            quad([1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]),
        ]
    }

    fn cube_scene() -> (Vec<u8>, Vec<u8>) {
        let mut w = World::new();
        let mut buf = Vec::new();
        w.appearance("ACAC3EC6-00A8-4312-9E37-D49AADEF7012");
        w.faces_list(KEY, 6);
        for (i, (pts, tris)) in cube_faces().iter().enumerate() {
            let m = mesh(&mut buf, pts, tris);
            w.face(KEY, i as f64 + 1.0, Some(m), bbox_of(pts));
        }
        w.body(KEY, "000002116E9F4EB8");
        (w.d, buf)
    }

    #[test]
    fn reads_a_cube_body() {
        let (world, buf) = cube_scene();
        let scene = DisplayScene::parse(&world, &[buf]);
        assert_eq!(scene.issues, Vec::<String>::new());
        assert_eq!(scene.bodies.len(), 1);
        let b = &scene.bodies[0];
        assert_eq!(b.node.as_deref(), Some("000002116E9F4EB8"));
        assert_eq!(b.key, "000000000000000000000494");
        assert_eq!(
            b.appearance.as_deref(),
            Some("ACAC3EC6-00A8-4312-9E37-D49AADEF7012")
        );
        assert_eq!(b.faces.len(), 6);
        assert_eq!(b.declared_faces, Some(6));
        assert_eq!(b.faces[2].number, 3.0);
        assert_eq!(b.faces[2].edges, vec![0, 1]);
        assert_eq!(b.faces[2].triangles, 4..6);
        assert_eq!(b.positions_cm.len(), 24);
        assert_eq!(b.off_bbox_vertices, 0);
        let s = b.stats();
        assert_eq!(s.triangles, 12);
        assert!(s.closed);
        assert_eq!(
            (s.boundary_edges, s.nonmanifold_edges, s.misoriented_edges),
            (0, 0, 0)
        );
        assert!((s.volume_mm3 - 1000.0).abs() < 1e-9, "{}", s.volume_mm3);
        assert!((s.area_mm2 - 600.0).abs() < 1e-9);
        assert_eq!(s.bbox_mm, [0.0, 0.0, 0.0, 10.0, 10.0, 10.0]);
    }

    /// The cube with layout-0 faces (meshes in `stream_mesh_000`).
    fn stream_cube_scene() -> (Vec<u8>, Vec<u8>) {
        let mut w = World::new();
        let mut buf = Vec::new();
        // Data of something else before the faces.
        buf.extend([0u8; 40]);
        w.faces_list(KEY, 6);
        for (i, (pts, tris)) in cube_faces().iter().enumerate() {
            let m = stream_mesh(&mut buf, pts, tris);
            w.stream_face(KEY, i as f64 + 1.0, i == 0, m, bbox_of(pts));
        }
        w.body(KEY, "000002116E9F4EB8");
        (w.d, buf)
    }

    #[test]
    fn reads_layout_0_faces_from_the_stream_buffer() {
        let (world, buf) = stream_cube_scene();
        let scene = DisplayScene::parse_with_streams(&world, &[], std::slice::from_ref(&buf));
        assert_eq!(scene.issues, Vec::<String>::new());
        assert_eq!(scene.bodies.len(), 1);
        let b = &scene.bodies[0];
        assert_eq!(b.faces.len(), 6);
        assert_eq!(b.declared_faces, Some(6));
        assert_eq!(b.faces_without_mesh(), 0);
        assert_eq!(b.faces[2].number, 3.0);
        assert!(b.faces[2].edges.is_empty());
        assert_eq!(b.faces[2].vertices, 8..12);
        assert_eq!(b.faces[2].triangles, 4..6);
        assert_eq!(b.positions_cm[8], [0.0, 0.0, 0.0]);
        assert_eq!(b.positions_cm[10], [1.0, 0.0, 1.0]);
        assert_eq!(b.off_bbox_vertices, 0);
        let s = b.stats();
        assert_eq!(s.triangles, 12);
        assert!(s.closed);
        assert!((s.volume_mm3 - 1000.0).abs() < 1e-9, "{}", s.volume_mm3);
        assert_eq!(s.bbox_mm, [0.0, 0.0, 0.0, 10.0, 10.0, 10.0]);

        // Without the stream buffer, or with a format reference whose
        // definition is missing, the faces are issues.
        let scene = DisplayScene::parse(&world, &[]);
        assert_eq!(scene.bodies[0].faces_without_mesh(), 6);
        assert!(
            scene
                .issues
                .iter()
                .all(|i| i.contains("no stream buffer 0"))
        );
        let mut w = World::new();
        let (pts, tris) = &cube_faces()[0];
        let m = stream_mesh(&mut Vec::new(), pts, tris);
        w.stream_face(KEY, 1.0, false, m, bbox_of(pts));
        w.body(KEY, "0000000000000001");
        let scene = DisplayScene::parse_with_streams(&w.d, &[], std::slice::from_ref(&buf));
        assert_eq!(scene.bodies[0].faces.len(), 0);
        assert!(
            scene.issues[0].contains("vertex format"),
            "{:?}",
            scene.issues
        );

        // Truncated or corrupted input never panics.
        for n in (0..world.len()).step_by(7) {
            let _ = DisplayScene::parse_with_streams(&world[..n], &[], std::slice::from_ref(&buf));
        }
        for n in (0..buf.len()).step_by(5) {
            let s = DisplayScene::parse_with_streams(&world, &[], &[buf[..n].to_vec()]);
            assert_eq!(s.bodies.len(), 1);
        }
        for i in 0..world.len() {
            let mut w = world.clone();
            w[i] ^= 0xff;
            let _ = DisplayScene::parse_with_streams(&w, &[], std::slice::from_ref(&buf));
        }

        // Through the document entries.
        let dir = "FusionAssetName[Active]/OGS.BlobFolder/OGS/DefaultScene/";
        let zip = crate::zip::stored_zip(&[
            (&format!("{dir}world"), &world),
            (&format!("{dir}stream_mesh_000"), &buf),
        ]);
        let doc = F3dFile::from_bytes(zip).unwrap();
        let scene = display_scene(&doc).unwrap().unwrap();
        assert_eq!(scene.issues, Vec::<String>::new());
        assert!(scene.bodies[0].stats().closed);
    }

    #[test]
    fn open_and_flipped_meshes() {
        let faces = cube_faces();
        let mut p = Vec::new();
        let mut t = Vec::new();
        for (pts, tris) in &faces[..5] {
            let base = p.len() as u32;
            p.extend(pts);
            t.extend(tris.iter().map(|x| x.map(|i| i + base)));
        }
        let s = mesh_stats(&p, &t);
        assert!(!s.closed);
        assert_eq!(s.boundary_edges, 4);
        // Flip one face: closed but inconsistently oriented.
        let (pts, tris) = &faces[5];
        let base = p.len() as u32;
        p.extend(pts);
        t.extend(tris.iter().map(|x| [x[0] + base, x[2] + base, x[1] + base]));
        let s = mesh_stats(&p, &t);
        assert!(s.closed);
        assert_eq!(s.misoriented_edges, 4);
        // Out-of-range indices are ignored.
        t.push([0, 1, 9999]);
        assert_eq!(mesh_stats(&p, &t).triangles, 12);
        assert_eq!(mesh_stats(&[], &[]), MeshStats::default());
    }

    /// Far-away positions at the smallest weld tolerance (a zero bounding
    /// box): the cell coordinates are clamped, so the neighbour search does
    /// not overflow in a debug build (mitcad#934).
    #[test]
    fn far_away_positions_do_not_overflow_the_weld_grid() {
        let s = mesh_stats(&[[1e30, 0.0, 0.0]; 3], &[[0, 1, 2]]);
        assert_eq!((s.triangles, s.degenerate_triangles), (1, 1));
        let s = mesh_stats(&[[-1e30, 1e30, -1e30]; 3], &[[0, 1, 2]]);
        assert_eq!(s.degenerate_triangles, 1);
        // Distinct far-away points stay apart: the search still compares
        // the coordinates themselves.
        let welded = weld(
            &[[1e31, 0.0, 0.0], [2e31, 0.0, 0.0], [1e31, 0.0, 0.0]],
            1e-9,
        );
        assert_eq!(welded, vec![0, 1, 0]);
    }

    #[test]
    fn faces_without_mesh_and_orphans() {
        let mut w = World::new();
        let other = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0x0a];
        w.faces_list(KEY, 3);
        w.face(KEY, 1.0, None, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        w.face(KEY, 2.0, None, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        w.body(KEY, "0000000000000001");
        w.face(other, 1.0, None, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        // "Face" as text elsewhere (no member prefix) is not a face.
        w.d.extend([0, 0, 0]);
        w.string("Face");
        let scene = DisplayScene::parse(&w.d, &[]);
        assert_eq!(scene.bodies.len(), 2);
        assert_eq!(scene.bodies[0].faces.len(), 2);
        assert_eq!(scene.bodies[0].faces_without_mesh(), 2);
        assert!(scene.bodies[0].triangles.is_empty());
        assert_eq!(scene.bodies[0].declared_faces, Some(3));
        assert_eq!(scene.bodies[1].node, None);
        // The face without a body record and the declared count.
        assert_eq!(scene.issues.len(), 2, "{:?}", scene.issues);
    }

    #[test]
    fn bad_references_are_issues() {
        let mut w = World::new();
        let pts = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mut buf = Vec::new();
        let (off, counts) = mesh(&mut buf, &pts, &[[0, 1, 2]]);
        let bb = bbox_of(&pts);
        w.faces_list(KEY, 5);
        w.face(KEY, 1.0, Some((off, counts)), bb);
        w.face(KEY, 2.0, Some((off + 4, counts)), bb); // runs past the end
        w.face(
            KEY,
            3.0,
            Some((off, [counts[0], counts[1], counts[2], 4])),
            bb,
        );
        w.face(
            KEY,
            4.0,
            Some((off, counts)),
            [5.0, 5.0, 5.0, 6.0, 6.0, 6.0],
        );
        w.face(KEY, 5.0, Some((u32::MAX, [u32::MAX; 4])), bb);
        w.body(KEY, "0000000000000001");
        // An index beyond the vertices.
        let mut buf2 = buf.clone();
        let last = buf2.len() - 4;
        buf2[last..].copy_from_slice(&7u32.to_le_bytes());
        let scene = DisplayScene::parse(&w.d, &[buf.clone()]);
        let b = &scene.bodies[0];
        assert_eq!(b.faces.len(), 5);
        assert_eq!(b.faces_without_mesh(), 3);
        assert_eq!(b.triangles.len(), 2);
        assert_eq!(b.off_bbox_vertices, 3);
        // Three bad faces; the declared count is right.
        assert_eq!(scene.issues.len(), 3, "{:?}", scene.issues);
        let scene = DisplayScene::parse(&w.d, &[buf2]);
        assert!(
            scene.issues.iter().any(|i| i.contains("index 7")),
            "{:?}",
            scene.issues
        );
        let scene = DisplayScene::parse(&w.d, &[]);
        assert!(scene.issues.iter().any(|i| i.contains("no mesh buffer")));
    }

    #[test]
    fn truncated_input_never_panics() {
        let (world, buf) = cube_scene();
        for n in 0..world.len() {
            let s = DisplayScene::parse(&world[..n], std::slice::from_ref(&buf));
            assert!(s.bodies.len() <= 1);
        }
        for n in 0..buf.len() {
            let s = DisplayScene::parse(&world, &[buf[..n].to_vec()]);
            assert_eq!(s.bodies.len(), 1);
        }
        // Corrupt every byte in turn.
        for i in 0..world.len() {
            let mut w = world.clone();
            w[i] ^= 0xff;
            let _ = DisplayScene::parse(&w, std::slice::from_ref(&buf));
        }
    }

    #[test]
    fn reads_the_scene_of_a_document() {
        let (world, buf) = cube_scene();
        let dir = "FusionAssetName[Active]/OGS.BlobFolder/OGS/DefaultScene/";
        let zip = crate::zip::stored_zip(&[
            (&format!("{dir}world"), &world),
            (&format!("{dir}Fusion_mesh_000"), &buf),
            (&format!("{dir}stream_mesh_000"), b"xx"),
        ]);
        let doc = F3dFile::from_bytes(zip).unwrap();
        let scene = display_scene(&doc).unwrap().unwrap();
        assert_eq!(scene.bodies.len(), 1);
        assert!(scene.bodies[0].stats().closed);
        let empty = crate::zip::stored_zip(&[("Properties.dat", b"\0\0\0\0")]);
        assert_eq!(
            display_scene(&F3dFile::from_bytes(empty).unwrap()).unwrap(),
            None
        );
    }
}
