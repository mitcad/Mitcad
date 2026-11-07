// SPDX-License-Identifier: MIT
//! The mesh containers of a design's mesh bodies (mitcad#1366):
//! `ParaMeshGeometry.BlobParts/ParaMeshGeometry.<uuid>.paramesh`, one
//! container per mesh body.
//!
//! Layout (from the published format description, see
//! `core/import/docs/f3d-undecoded.md`, *The published format
//! descriptions*; checked on the corpus's containers):
//!
//! ```text
//! 0x00  89 55 44 50 4D 45 53 48 0D 0A 1A 0A   magic
//! 0x0C  u32 2                                 container version
//! 0x10  32 zero bytes
//! 0x30  u64 registry byte count
//! 0x38  u32 1
//! 0x3C  registry (a protocol-buffer message)
//! then chunks to the end: u64 body bytes | u32 kind | body
//!       kind 3: a MessagePack map, stream name -> stream id
//!       kind 4: one stream, in ascending stream id order:
//!               u16 n | MessagePack descriptor (n bytes) | u32 bytes |
//!               2-byte encoding | data
//! ```
//!
//! An encoding's first byte is the LZMA properties byte (`5D`: lc 3, lp 0,
//! pb 2); its second byte `FE` stores the bytes as they are, else it is the
//! dictionary size's power of two (`14`: 1 MiB) and the data is a raw LZMA
//! stream ([`crate::lzma`]) of exactly the declared bytes.
//!
//! A descriptor is a map of `T` (0 bytes, 1 32-bit words, 3 `f32`), `D`
//! (components) and `U` (a direction as two octahedral `f32`). The
//! registry names the streams: field 21 the vertex positions (`f32` x, y, z
//! per vertex, in the container's coordinates, which the design's mesh
//! body record places, `mitcad_f3d_decode::mesh`), field 22 the triangle
//! corners and field 7 (its field 4) the feature edges. Corners and feature
//! edges are terminal-delta words: each word before the last is the
//! two's-complement difference to the next value, the last is the last
//! value itself. The feature edges pair their values `(low, high)`. Field
//! 8 holds text properties (field 1 the key, field 3 the text): one of
//! them is the GUID the design's record of the container repeats. Field 9
//! is the number of face groups, field 12 the container's own GUID. The
//! normals, texture coordinates, colours and face groups of the attribute
//! channels (fields 4 and 5) are not read: a mesh body takes its triangles
//! only.

use std::collections::HashMap;

use crate::lzma;

/// The first bytes of a mesh container.
pub const MAGIC: [u8; 12] = [
    0x89, b'U', b'D', b'P', b'M', b'E', b'S', b'H', 0x0d, 0x0a, 0x1a, 0x0a,
];

/// The folder of the mesh containers in a document.
pub const FOLDER: &str = "ParaMeshGeometry.BlobParts";

/// One mesh container, decoded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshContainer {
    /// The registry's text properties (key, text), in order.
    pub properties: Vec<(String, String)>,
    /// The container's own GUID (registry field 12).
    pub local_id: Option<String>,
    /// Vertex positions in the container's coordinates.
    pub vertices: Vec<[f32; 3]>,
    /// Triangles: vertex indices in the stored order.
    pub triangles: Vec<[u32; 3]>,
    /// The vertex pairs (low, high) the file classifies as feature edges.
    pub feature_edges: Vec<[u32; 2]>,
    /// The number of face groups (registry field 9).
    pub face_groups: u64,
}

/// A mesh placed in a design: points in millimetres and triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlacedMesh {
    pub points: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl MeshContainer {
    /// Whether one of its text properties is `text` (case aside): the GUID
    /// the design's record of the container repeats.
    pub fn has_property_text(&self, text: &str) -> bool {
        self.properties
            .iter()
            .any(|(_, value)| value.eq_ignore_ascii_case(text))
    }

    /// The mesh in the design's millimetres: `transform` (row-major,
    /// container coordinates to centimetres; `None` the identity) moves
    /// the vertices. A mirroring transform (negative determinant) turns
    /// each triangle round, so that the triangles keep their side.
    pub fn placed(&self, transform: Option<&[[f64; 4]; 4]>) -> PlacedMesh {
        let points = self
            .vertices
            .iter()
            .map(|v| {
                let v = [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])];
                let cm = match transform {
                    Some(m) => [0, 1, 2]
                        .map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2] + m[r][3]),
                    None => v,
                };
                cm.map(|c| c * 10.0)
            })
            .collect();
        let mirrored = transform.is_some_and(|m| determinant(m) < 0.0);
        let triangles = self
            .triangles
            .iter()
            .map(|t| if mirrored { [t[0], t[2], t[1]] } else { *t })
            .collect();
        PlacedMesh { points, triangles }
    }
}

/// The determinant of a transform's linear part.
pub fn determinant(m: &[[f64; 4]; 4]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Decodes a mesh container.
pub fn parse(data: &[u8]) -> Result<MeshContainer, String> {
    if data.get(..MAGIC.len()) != Some(&MAGIC[..]) {
        return Err("not a mesh container (its first bytes differ)".to_owned());
    }
    let version = u32_at(data, 12).ok_or("the mesh container ends in its header")?;
    if version != 2 {
        return Err(format!("mesh container version {version} (2 is read)"));
    }
    let registry_bytes = u64_at(data, 0x30).ok_or("the mesh container ends in its header")?;
    let start = 0x3c;
    let end = usize::try_from(registry_bytes)
        .ok()
        .and_then(|n| n.checked_add(start))
        .filter(|&e| e <= data.len())
        .ok_or("the mesh container's registry runs past its end")?;
    let registry = Registry::parse(&data[start..end])?;
    // The chunks: the name table and the streams.
    let mut names: Option<Vec<(String, u64)>> = None;
    let mut streams: Vec<Stream> = Vec::new();
    let mut p = end;
    while p < data.len() {
        let size = u64_at(data, p)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("a mesh container chunk ends in its header")?;
        let kind = u32_at(data, p + 8).ok_or("a mesh container chunk ends in its header")?;
        let body = p
            .checked_add(12)
            .and_then(|b| Some((b, b.checked_add(size)?)))
            .and_then(|(b, e)| data.get(b..e))
            .ok_or("a mesh container chunk runs past the container's end")?;
        match kind {
            3 => names = Some(name_table(body)?),
            4 => streams.push(Stream::parse(body)?),
            other => return Err(format!("mesh container chunk of kind {other}")),
        }
        p += 12 + size;
    }
    // The streams follow the name table in ascending id order.
    let mut ids: Vec<(u64, String)> = names
        .ok_or("the mesh container has no name table")?
        .into_iter()
        .map(|(name, id)| (id, name))
        .collect();
    ids.sort();
    if ids.len() != streams.len() {
        return Err(format!(
            "the mesh container names {} streams and holds {}",
            ids.len(),
            streams.len()
        ));
    }
    let by_name: HashMap<String, Stream> =
        ids.into_iter().map(|(_, name)| name).zip(streams).collect();
    let stream = |name: &Option<String>, what: &str| named(&by_name, name.as_deref(), what);
    // The vertices.
    let positions = stream(&registry.positions, "vertex")?;
    if positions.kind != 3 || positions.components != Some(3) || positions.octahedral {
        return Err("the mesh container's vertex stream is not three f32 a vertex".to_owned());
    }
    let vertices: Vec<[f32; 3]> = positions
        .data
        .as_chunks::<12>()
        .0
        .iter()
        .map(|c| [0, 4, 8].map(|o| f32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]])))
        .collect();
    if vertices.iter().flatten().any(|c| !c.is_finite()) {
        return Err("the mesh container's vertices are not all finite".to_owned());
    }
    let count = vertices.len();
    let in_range = |v: u32| (v as usize) < count;
    // The triangles.
    let corners = stream(&registry.corners, "corner")?;
    if corners.kind != 1 {
        return Err("the mesh container's corner stream is not 32-bit words".to_owned());
    }
    let corners = terminal_deltas(&corners.data);
    if !corners.len().is_multiple_of(3) {
        return Err(format!(
            "the mesh container has {} triangle corners, not whole triangles",
            corners.len()
        ));
    }
    if !corners.iter().all(|&c| in_range(c)) {
        return Err(format!(
            "the mesh container's triangles name vertices past its {count}"
        ));
    }
    let triangles: Vec<[u32; 3]> = corners.as_chunks::<3>().0.to_vec();
    // The feature edges.
    let mut feature_edges = Vec::new();
    if registry.edges.is_some() {
        let edges = stream(&registry.edges, "feature edge")?;
        if edges.kind != 1 {
            return Err("the mesh container's feature edge stream is not 32-bit words".to_owned());
        }
        let ends = terminal_deltas(&edges.data);
        if !ends.len().is_multiple_of(2) {
            return Err("the mesh container's feature edges have an odd number of ends".to_owned());
        }
        for pair in ends.as_chunks::<2>().0 {
            if pair[0] >= pair[1] || !in_range(pair[1]) {
                return Err(format!(
                    "the mesh container's feature edge {} {} is not two of its vertices in order",
                    pair[0], pair[1]
                ));
            }
            feature_edges.push(*pair);
        }
    }
    Ok(MeshContainer {
        properties: registry.properties,
        local_id: registry.local_id,
        vertices,
        triangles,
        feature_edges,
        face_groups: registry.face_groups.unwrap_or(0),
    })
}

/// The stream the registry names for `what`.
fn named<'a>(
    by_name: &'a HashMap<String, Stream>,
    name: Option<&str>,
    what: &str,
) -> Result<&'a Stream, String> {
    let name =
        name.ok_or_else(|| format!("the mesh container's registry names no {what} stream"))?;
    by_name
        .get(name)
        .ok_or_else(|| format!("the mesh container has no {what} stream {name:?}"))
}

/// Terminal-delta words: each before the last the difference to the next
/// value, the last the last value.
pub fn terminal_deltas(data: &[u8]) -> Vec<u32> {
    let words: Vec<u32> = data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect();
    let Some((&last, steps)) = words.split_last() else {
        return Vec::new();
    };
    let total = steps.iter().fold(0u32, |s, d| s.wrapping_add(*d));
    let mut value = last.wrapping_sub(total);
    let mut out = Vec::with_capacity(words.len());
    out.push(value);
    for d in steps {
        value = value.wrapping_add(*d);
        out.push(value);
    }
    out
}

/// What the registry says of the streams.
#[derive(Debug, Default)]
struct Registry {
    positions: Option<String>,
    corners: Option<String>,
    edges: Option<String>,
    properties: Vec<(String, String)>,
    face_groups: Option<u64>,
    local_id: Option<String>,
}

impl Registry {
    fn parse(data: &[u8]) -> Result<Self, String> {
        let mut out = Registry::default();
        for (field, value) in fields(data)? {
            match (field, value) {
                (7, Field::Bytes(edges)) => {
                    for (inner, value) in fields(edges)? {
                        if let (4, Field::Bytes(name)) = (inner, value) {
                            out.edges = Some(text(name)?);
                        }
                    }
                }
                (8, Field::Bytes(property)) => {
                    let (mut key, mut value) = (None, None);
                    for (inner, v) in fields(property)? {
                        match (inner, v) {
                            (1, Field::Bytes(k)) => key = Some(text(k)?),
                            (3, Field::Bytes(t)) => value = Some(text(t)?),
                            _ => {}
                        }
                    }
                    if let (Some(key), Some(value)) = (key, value) {
                        out.properties.push((key, value));
                    }
                }
                (9, Field::Varint(n)) => out.face_groups = Some(n),
                (12, Field::Bytes(id)) => out.local_id = Some(text(id)?),
                (21, Field::Bytes(name)) => out.positions = Some(text(name)?),
                (22, Field::Bytes(name)) => out.corners = Some(text(name)?),
                _ => {}
            }
        }
        Ok(out)
    }
}

fn text(b: &[u8]) -> Result<String, String> {
    String::from_utf8(b.to_vec()).map_err(|_| "a mesh container name is not UTF-8".to_owned())
}

/// A protocol-buffer field's value.
enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

/// The fields of a protocol-buffer message, in order.
fn fields(data: &[u8]) -> Result<Vec<(u64, Field<'_>)>, String> {
    let bad = || "the mesh container's registry is not a valid message".to_owned();
    let mut out = Vec::new();
    let mut p = 0;
    while p < data.len() {
        let key = varint(data, &mut p).ok_or_else(bad)?;
        let value = match key & 7 {
            0 => Field::Varint(varint(data, &mut p).ok_or_else(bad)?),
            1 => {
                p = p
                    .checked_add(8)
                    .filter(|&e| e <= data.len())
                    .ok_or_else(bad)?;
                Field::Fixed
            }
            2 => {
                let n = varint(data, &mut p)
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(bad)?;
                let end = p
                    .checked_add(n)
                    .filter(|&e| e <= data.len())
                    .ok_or_else(bad)?;
                let bytes = &data[p..end];
                p = end;
                Field::Bytes(bytes)
            }
            5 => {
                p = p
                    .checked_add(4)
                    .filter(|&e| e <= data.len())
                    .ok_or_else(bad)?;
                Field::Fixed
            }
            _ => return Err(bad()),
        };
        out.push((key >> 3, value));
    }
    Ok(out)
}

/// A protocol-buffer varint at `*p`, moving `*p` past it.
fn varint(data: &[u8], p: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *data.get(*p)?;
        *p += 1;
        value |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// One stream of a container, inflated.
#[derive(Debug)]
struct Stream {
    /// `T`: 0 bytes, 1 32-bit words, 3 `f32`.
    kind: u64,
    /// `D`: components per element.
    components: Option<u64>,
    /// `U`: directions as two octahedral `f32`.
    octahedral: bool,
    data: Vec<u8>,
}

impl Stream {
    fn parse(body: &[u8]) -> Result<Self, String> {
        let short = || "a mesh container stream ends in its header".to_owned();
        let n = usize::from(u16::from_le_bytes([
            *body.first().ok_or_else(short)?,
            *body.get(1).ok_or_else(short)?,
        ]));
        let descriptor = body.get(2..2 + n).ok_or_else(short)?;
        let size = u32_at(body, 2 + n).ok_or_else(short)? as usize;
        let encoding = body.get(6 + n..8 + n).ok_or_else(short)?;
        let payload = &body[8 + n..];
        let mut kind = None;
        let mut components = None;
        let mut octahedral = false;
        let mut p = 0;
        let Msg::Map(entries) = msgpack(descriptor, &mut p, 0)? else {
            return Err("a mesh container stream's descriptor is not a map".to_owned());
        };
        for (key, value) in entries {
            match (key, value) {
                (Msg::Str(k), Msg::Int(v)) if k == "T" => kind = Some(v),
                (Msg::Str(k), Msg::Int(v)) if k == "D" => components = Some(v),
                (Msg::Str(k), Msg::Bool(v)) if k == "U" => octahedral = v,
                (Msg::Str(k), Msg::Int(1)) if k == "d" => {}
                _ => return Err("a mesh container stream's descriptor is not known".to_owned()),
            }
        }
        let kind = kind.ok_or("a mesh container stream's descriptor has no type")?;
        let width: u64 = match (kind, components, octahedral) {
            (0, None, false) => 1,
            (1, None, false) => 4,
            (3, Some(d @ 1..=4), false) => 4 * d,
            (3, Some(3), true) => 8,
            _ => {
                return Err(format!(
                    "a mesh container stream of type {kind} is not known"
                ));
            }
        };
        if !(size as u64).is_multiple_of(width) {
            return Err(format!(
                "a mesh container stream of {size} bytes is not whole elements of {width}"
            ));
        }
        let data = match encoding {
            [_, 0xfe] => payload
                .get(..size)
                .ok_or("a mesh container stream is shorter than it says")?
                .to_vec(),
            [props, power] if *props < 225 && *power < 32 => lzma::decompress(
                payload,
                u32::from(props % 9),
                u32::from(props / 9 % 5),
                u32::from(props / 45),
                1u32 << power,
                size,
            )?,
            _ => {
                return Err(format!(
                    "a mesh container stream of encoding {:02x}{:02x}",
                    encoding[0], encoding[1]
                ));
            }
        };
        Ok(Self {
            kind,
            components,
            octahedral,
            data,
        })
    }
}

/// The name table: a MessagePack map of stream names to ids.
fn name_table(body: &[u8]) -> Result<Vec<(String, u64)>, String> {
    let mut p = 0;
    let Msg::Map(entries) = msgpack(body, &mut p, 0)? else {
        return Err("the mesh container's name table is not a map".to_owned());
    };
    entries
        .into_iter()
        .map(|entry| match entry {
            (Msg::Str(name), Msg::Int(id)) => Ok((name, id)),
            _ => Err("the mesh container's name table is not names and ids".to_owned()),
        })
        .collect()
}

/// The MessagePack values the containers use.
#[derive(Debug, PartialEq)]
enum Msg {
    Nil,
    Bool(bool),
    Int(u64),
    Str(String),
    Map(Vec<(Msg, Msg)>),
}

/// A MessagePack value at `*p`, moving `*p` past it.
fn msgpack(d: &[u8], p: &mut usize, depth: usize) -> Result<Msg, String> {
    let bad = || "a mesh container map is not valid MessagePack".to_owned();
    if depth > 8 {
        return Err(bad());
    }
    let b = *d.get(*p).ok_or_else(bad)?;
    *p += 1;
    Ok(match b {
        0x00..=0x7f => Msg::Int(u64::from(b)),
        0xc0 => Msg::Nil,
        0xc2 => Msg::Bool(false),
        0xc3 => Msg::Bool(true),
        0xcc..=0xcf => {
            let n = 1usize << (b - 0xcc);
            Msg::Int(be_uint(d, p, n).ok_or_else(bad)?)
        }
        0xa0..=0xbf | 0xd9 | 0xda => {
            let n = match b {
                0xd9 => be_uint(d, p, 1).ok_or_else(bad)? as usize,
                0xda => be_uint(d, p, 2).ok_or_else(bad)? as usize,
                _ => usize::from(b & 0x1f),
            };
            let s = take(d, p, n).ok_or_else(bad)?;
            Msg::Str(String::from_utf8(s.to_vec()).map_err(|_| bad())?)
        }
        0x80..=0x8f | 0xde => {
            let n = if b == 0xde {
                be_uint(d, p, 2).ok_or_else(bad)? as usize
            } else {
                usize::from(b & 0x0f)
            };
            let mut entries = Vec::new();
            for _ in 0..n {
                let key = msgpack(d, p, depth + 1)?;
                let value = msgpack(d, p, depth + 1)?;
                entries.push((key, value));
            }
            Msg::Map(entries)
        }
        _ => return Err(bad()),
    })
}

/// The `n` bytes at `*p` as a big-endian number, moving `*p` past them.
fn be_uint(d: &[u8], p: &mut usize, n: usize) -> Option<u64> {
    Some(
        take(d, p, n)?
            .iter()
            .fold(0u64, |v, &x| (v << 8) | u64::from(x)),
    )
}

/// The `n` bytes at `*p`, moving `*p` past them.
fn take<'a>(d: &'a [u8], p: &mut usize, n: usize) -> Option<&'a [u8]> {
    let end = p.checked_add(n).filter(|&e| e <= d.len())?;
    let s = &d[*p..end];
    *p = end;
    Some(s)
}

fn u32_at(d: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        d.get(p..p.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn u64_at(d: &[u8], p: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        d.get(p..p.checked_add(8)?)?.try_into().ok()?,
    ))
}

/// Synthetic mesh containers for tests, here and in the crates above
/// (`mitcad_f3d::testmeshes`): no real mesh needed.
#[doc(hidden)]
pub mod testdata {
    use super::MAGIC;

    /// A protocol-buffer length-delimited field.
    fn pb_bytes(field: u64, bytes: &[u8]) -> Vec<u8> {
        let mut out = pb_varint((field << 3) | 2);
        out.extend(pb_varint(bytes.len() as u64));
        out.extend_from_slice(bytes);
        out
    }

    fn pb_varint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    /// A MessagePack string of at most 31 bytes.
    pub fn fixstr(s: &str) -> Vec<u8> {
        let mut out = vec![0xa0 | s.len() as u8];
        out.extend_from_slice(s.as_bytes());
        out
    }

    /// Terminal-delta words of `values`.
    pub fn deltas(values: &[u32]) -> Vec<u8> {
        let mut words: Vec<u32> = values.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect();
        if let Some(&last) = values.last() {
            words.push(last);
        }
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// One stream chunk: its descriptor and data, raw (`5D FE`) or the
    /// given LZMA stream of `data`.
    pub fn stream_chunk(descriptor: &[u8], data: &[u8], lzma: Option<&[u8]>) -> Vec<u8> {
        let mut body = (descriptor.len() as u16).to_le_bytes().to_vec();
        body.extend_from_slice(descriptor);
        body.extend((data.len() as u32).to_le_bytes());
        match lzma {
            Some(packed) => {
                body.extend([0x5d, 0x14]);
                body.extend_from_slice(packed);
            }
            None => {
                body.extend([0x5d, 0xfe]);
                body.extend_from_slice(data);
            }
        }
        chunk(4, &body)
    }

    fn chunk(kind: u32, body: &[u8]) -> Vec<u8> {
        let mut out = (body.len() as u64).to_le_bytes().to_vec();
        out.extend(kind.to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    /// The GUID the synthetic container's design record repeats.
    pub const KEY: &str = "e5115d46-0000-4561-a97d-21ce643f8551";

    /// The tetrahedron's triangle corners.
    pub const CORNERS: [u32; 12] = [0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3];

    /// A synthetic container of a tetrahedron (edge 1, in the container's
    /// coordinates), its triangles counter-clockwise from outside, two
    /// feature edges, the stream names other than `v` and `t` (the
    /// registry assigns the roles), the name table not in id order.
    pub fn tetrahedron() -> Vec<u8> {
        container(&CORNERS, &[0, 1, 2, 3])
    }

    /// The tetrahedron's container with these corners and feature edge
    /// ends.
    pub fn container(corners: &[u32], edges: &[u32]) -> Vec<u8> {
        let vertices: [[f32; 3]; 4] = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let mut registry = Vec::new();
        registry.extend(pb_bytes(7, &pb_bytes(4, b"fe")));
        let mut property = pb_bytes(1, b"guid_key");
        property.extend(pb_bytes(3, KEY.as_bytes()));
        registry.extend(pb_bytes(8, &property));
        registry.extend(pb_varint(9 << 3));
        registry.extend(pb_varint(1));
        registry.extend(pb_bytes(12, b"37320454-67d1-4eb0-9d12-eec2fa411b88"));
        registry.extend(pb_bytes(21, b"pos"));
        registry.extend(pb_bytes(22, b"tri"));
        let mut out = MAGIC.to_vec();
        out.extend(2u32.to_le_bytes());
        out.extend([0u8; 32]);
        out.extend((registry.len() as u64).to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(registry);
        let mut names = vec![0x83];
        for (name, id) in [("tri", 3u8), ("fe", 4), ("pos", 2)] {
            names.extend(fixstr(name));
            names.push(id);
        }
        out.extend(chunk(3, &names));
        // Ids 2 (positions), 3 (corners), 4 (feature edges).
        let floats: Vec<u8> = vertices
            .iter()
            .flatten()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let mut d3 = vec![0x82];
        d3.extend(fixstr("D"));
        d3.push(3);
        d3.extend(fixstr("T"));
        d3.push(3);
        out.extend(stream_chunk(&d3, &floats, None));
        let mut words = vec![0x82];
        words.extend(fixstr("T"));
        words.push(1);
        words.extend(fixstr("d"));
        words.push(1);
        out.extend(stream_chunk(&words, &deltas(corners), None));
        out.extend(stream_chunk(&words, &deltas(edges), None));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testdata::*;
    use super::*;

    #[test]
    fn decodes_a_synthetic_container() {
        let mesh = parse(&tetrahedron()).unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.vertices[3], [0.0, 0.0, 1.0]);
        assert_eq!(mesh.triangles, [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]);
        assert_eq!(mesh.feature_edges, [[0, 1], [2, 3]]);
        assert_eq!(mesh.face_groups, 1);
        assert!(mesh.has_property_text(&KEY.to_uppercase()));
        assert_eq!(
            mesh.local_id.as_deref(),
            Some("37320454-67d1-4eb0-9d12-eec2fa411b88")
        );
    }

    #[test]
    fn places_a_mesh_in_millimetres() {
        let mesh = parse(&tetrahedron()).unwrap();
        // Without a transform the container's coordinates are centimetres.
        let placed = mesh.placed(None);
        assert_eq!(placed.points[1], [10.0, 0.0, 0.0]);
        assert_eq!(placed.triangles, mesh.triangles);
        // A millimetre mesh moved 2 cm along X.
        let moved = [
            [0.1, 0.0, 0.0, 2.0],
            [0.0, 0.1, 0.0, 0.0],
            [0.0, 0.0, 0.1, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let placed = mesh.placed(Some(&moved));
        let p = placed.points[1];
        assert!((p[0] - 21.0).abs() < 1e-12 && p[1] == 0.0 && p[2] == 0.0);
        assert_eq!(placed.triangles, mesh.triangles);
        // Mirrored in X: each triangle turns round.
        let mirrored = [
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let placed = mesh.placed(Some(&mirrored));
        assert_eq!(placed.points[1], [-10.0, 0.0, 0.0]);
        assert_eq!(placed.triangles[0], [0, 1, 2]);
    }

    #[test]
    fn reads_a_compressed_stream() {
        // The LZMA sample of `lzma.rs` as a byte stream of a container.
        let sample = crate::lzma::tests::sample();
        let packed = crate::lzma::tests::sample_packed();
        let mut bytes = vec![0x81];
        bytes.extend(fixstr("T"));
        bytes.push(0);
        let chunk = stream_chunk(&bytes, &sample, Some(&packed));
        let stream = Stream::parse(&chunk[12..]).unwrap();
        assert_eq!(stream.kind, 0);
        assert_eq!(stream.data, sample);
    }

    #[test]
    fn terminal_deltas_end_at_their_last_value() {
        assert_eq!(terminal_deltas(&deltas(&[5, 3, 9, 0])), [5, 3, 9, 0]);
        assert_eq!(terminal_deltas(&[]), Vec::<u32>::new());
        assert_eq!(terminal_deltas(&7u32.to_le_bytes()), [7]);
    }

    #[test]
    fn refuses_broken_containers() {
        let good = tetrahedron();
        assert!(parse(&good[..good.len() - 3]).is_err());
        let mut other = good.clone();
        other[12] = 3;
        assert!(parse(&other).unwrap_err().contains("version 3"));
        assert!(parse(b"not a mesh").is_err());
        // A corner past the vertices, a corner short of a whole triangle,
        // feature edges out of order or past the vertices.
        let tri = [0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3];
        let mut far = tri;
        far[11] = 4;
        assert!(
            parse(&container(&far, &[0, 1]))
                .unwrap_err()
                .contains("past")
        );
        assert!(parse(&container(&tri[..11], &[0, 1])).is_err());
        assert!(parse(&container(&tri, &[1, 0])).is_err());
        assert!(parse(&container(&tri, &[0, 4])).is_err());
        assert!(parse(&container(&tri, &[0, 1, 2])).is_err());
        // No feature edges at all.
        assert!(
            parse(&container(&tri, &[]))
                .unwrap()
                .feature_edges
                .is_empty()
        );
    }
}
