// SPDX-License-Identifier: MIT
//! Reader of glTF 2.0 files (mitcad#1826): the binary container (`.glb`,
//! self-contained) and the JSON form (`.gltf`) with its buffers and images
//! embedded as data URIs or as files beside it. What the import of mesh
//! bodies needs is read: the scene's node tree with each node's placement,
//! the meshes' triangles with the texture coordinates of their base
//! colour's image (mitcad#1862), the metallic-roughness materials and the
//! images of their base colour, metalness and roughness, normal and
//! occlusion maps. What it does not take
//! (animations, skins, morph targets, points and lines, compressed meshes,
//! extensions it does not know) is listed in [`Gltf::notes`] in words.
//!
//! The file is untrusted: every index, count, offset and stride is checked
//! against what it points into before it is used, and only files in the
//! file's own folder (or below it) are read for its resources.
//!
//! Lengths are glTF's metres and its axes are Y up; [`Y_UP_TO_Z_UP`] turns
//! them into MitCad's (Z up), and the import scales them to millimetres.

use std::path::{Component, Path};

use serde_json::{Value, json};

mod base64;
pub mod testfile;

#[cfg(test)]
mod tests;

/// A 4 x 4 matrix, row-major: `m[r][c]`, points as columns.
pub type Matrix = [[f64; 4]; 4];

pub const IDENTITY: Matrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// glTF's axes (Y up, +Z toward the viewer) as MitCad's (Z up, the viewer
/// of the front view on -Y): (x, y, z) becomes (x, -z, y).
pub const Y_UP_TO_Z_UP: Matrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, -1.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Millimetres per glTF unit (the metre).
pub const MM_PER_UNIT: f64 = 1000.0;

/// The most triangles a file may hold in all.
pub const MAX_TRIANGLES: usize = 50_000_000;

/// The most elements an accessor may have.
const MAX_COUNT: usize = 200_000_000;

/// The deepest node tree followed.
pub const MAX_DEPTH: usize = 64;

/// Extensions read, or whose absence changes nothing the import takes.
const KNOWN_EXTENSIONS: &[&str] = &[
    "KHR_materials_emissive_strength",
    "KHR_materials_ior",
    "KHR_materials_transmission",
    "KHR_mesh_quantization",
    // A material without lighting: its base colour is kept.
    "KHR_materials_unlit",
    // Applied to the base colour's texture coordinates (mitcad#1862).
    "KHR_texture_transform",
];

/// `a` then `b`: the map that applies `b` first.
pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

/// An sRGB value (as colour pickers show it) of a linear one, both 0 to 1.
pub fn srgb(linear: f64) -> f64 {
    let c = linear.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// A `KHR_texture_transform` as rows of a 2 x 3 matrix on (u, v, 1): the
/// texture coordinates scaled, turned by its rotation and moved by its
/// offset (glTF's translation x rotation x scale).
fn uv_transform(t: &Value) -> [[f64; 3]; 2] {
    let [ox, oy] = numbers(t, "offset", [0.0, 0.0]);
    let [sx, sy] = numbers(t, "scale", [1.0, 1.0]);
    let (sin, cos) = number(t, "rotation", 0.0).sin_cos();
    [[cos * sx, sin * sy, ox], [-sin * sx, cos * sy, oy]]
}

/// A file that cannot be read: what a person reads (`words`) and where it
/// is for agents and the log (`detail`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub words: String,
    pub detail: String,
}

impl Error {
    fn new(words: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            words: words.into(),
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.words)
    }
}

/// Something of the file the reader or the import does not take: in
/// words, once each, with its detail (indexes, names of members) for
/// agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub words: String,
    pub detail: String,
}

/// A node: its placement in its parent, its mesh and its children.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: Option<String>,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
    /// In the parent, glTF's units and axes.
    pub matrix: Matrix,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub name: Option<String>,
    /// The primitives of triangles that were read.
    pub primitives: Vec<Primitive>,
}

/// Triangles of one material.
#[derive(Debug, Clone, PartialEq)]
pub struct Primitive {
    /// In the mesh's coordinates, glTF's units and axes.
    pub positions: Vec<[f64; 3]>,
    /// Counter-clockwise seen from outside; degenerate ones left out.
    pub triangles: Vec<[u32; 3]>,
    pub material: Option<usize>,
    /// The texture coordinates of its material's base colour image, one
    /// per position (mitcad#1862): the set the image reads
    /// (`TEXCOORD_<n>`), moved by its `KHR_texture_transform`, in
    /// MitCad's convention (u to the image's right, v up it from its
    /// bottom; glTF's v runs down from its top, so v is 1 - v). None
    /// without a base colour image or where the file has no such set.
    pub uvs: Option<Vec<[f64; 2]>>,
}

/// A metallic-roughness material; colours linear, as glTF keeps them.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub name: Option<String>,
    /// Linear red, green, blue and alpha.
    pub base_color: [f64; 4],
    pub metallic: f64,
    pub roughness: f64,
    /// Linear, times its strength (`KHR_materials_emissive_strength`).
    pub emissive: [f64; 3],
    /// Alpha blends (`alphaMode` BLEND): see-through by its alpha.
    pub blend: bool,
    /// `KHR_materials_transmission`.
    pub transmission: f64,
    /// `KHR_materials_ior`.
    pub ior: Option<f64>,
    /// Images (indices into [`Gltf::images`]) of the base colour, the
    /// normal map and the ambient occlusion (its red channel).
    pub base_color_image: Option<usize>,
    pub normal_image: Option<usize>,
    pub occlusion_image: Option<usize>,
    /// The image of metalness (its blue channel) and roughness (its green
    /// channel), mitcad#1862.
    pub metallic_roughness_image: Option<usize>,
    /// The set of texture coordinates the base colour's image reads (its
    /// `texCoord`), and the `KHR_texture_transform` of the image as rows
    /// of a 2 x 3 matrix (u, v, 1), if it has one.
    pub tex_coord: usize,
    pub uv_transform: Option<[[f64; 3]; 2]>,
}

impl Default for Material {
    /// glTF's default material.
    fn default() -> Self {
        Self {
            name: None,
            base_color: [1.0; 4],
            metallic: 1.0,
            roughness: 1.0,
            emissive: [0.0; 3],
            blend: false,
            transmission: 0.0,
            ior: None,
            base_color_image: None,
            normal_image: None,
            occlusion_image: None,
            metallic_roughness_image: None,
            tex_coord: 0,
            uv_transform: None,
        }
    }
}

/// An image of a material: its bytes when they are a PNG or JPEG file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Image {
    pub name: Option<String>,
    /// A file name for it: the file it came from, its name, else
    /// `image<n>` with its type's extension.
    pub file_name: String,
    pub bytes: Option<Vec<u8>>,
    /// `png` or `jpeg`.
    pub format: Option<&'static str>,
}

/// A glTF file as read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gltf {
    /// The program that wrote it (`asset.generator`).
    pub generator: Option<String>,
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub images: Vec<Image>,
    /// The scene's root nodes.
    pub roots: Vec<usize>,
    pub notes: Vec<Note>,
}

/// Whether the bytes are a binary glTF file (`.glb`).
pub fn is_glb(bytes: &[u8]) -> bool {
    bytes.starts_with(b"glTF")
}

/// Reads a `.glb` or `.gltf` file; its buffers and images in other files
/// are read from its folder.
pub fn read_file(path: &Path) -> Result<Gltf, Error> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let bytes = std::fs::read(path).map_err(|e| {
        Error::new(
            format!("{name} cannot be read: {e}"),
            format!("{}: {e}", path.display()),
        )
    })?;
    read(&bytes, path.parent())
}

/// Reads a glTF file from its bytes; `folder` is where the files it refers
/// to are (None: such files are not read, a note says so).
pub fn read(bytes: &[u8], folder: Option<&Path>) -> Result<Gltf, Error> {
    let (json_bytes, bin) = if is_glb(bytes) {
        glb_chunks(bytes)?
    } else {
        let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        (text, None)
    };
    let json: Value = serde_json::from_slice(json_bytes).map_err(|e| {
        Error::new(
            "the file is not a glTF file: its JSON cannot be read",
            format!("JSON: {e}"),
        )
    })?;
    if !json.is_object() {
        return Err(Error::new(
            "the file is not a glTF file",
            "the JSON is not an object",
        ));
    }
    let version = json
        .get("asset")
        .and_then(|a| a.get("version"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !version.starts_with("2.") && version != "2" {
        return Err(Error::new(
            if version.is_empty() {
                "the file is not a glTF file: it gives no glTF version".to_owned()
            } else {
                format!("the file is glTF {version}: MitCad reads glTF 2.0")
            },
            format!("asset.version {version:?}"),
        ));
    }
    let mut reader = Reader {
        json: &json,
        buffers: Vec::new(),
        notes: Vec::new(),
    };
    reader.load_buffers(bin, folder);
    reader.read_all(folder)
}

const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The JSON chunk and the binary chunk of a `.glb` file.
fn glb_chunks(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), Error> {
    let damaged = |detail: String| Error::new("the glTF binary file is damaged", detail);
    let version = u32_at(bytes, 4).ok_or_else(|| damaged("the header ends early".into()))?;
    if version != 2 {
        return Err(Error::new(
            format!("the file is a glTF binary file of version {version}: MitCad reads version 2"),
            format!("glb version {version}"),
        ));
    }
    let length = u32_at(bytes, 8).ok_or_else(|| damaged("the header ends early".into()))? as usize;
    if !(12..=bytes.len()).contains(&length) {
        return Err(damaged(format!(
            "the header gives {length} bytes, the file has {}",
            bytes.len()
        )));
    }
    let data = &bytes[..length];
    let mut at = 12;
    let mut json = None;
    let mut bin = None;
    while at + 8 <= data.len() {
        let size = u32_at(data, at).unwrap_or(0) as usize;
        let kind = u32_at(data, at + 4).unwrap_or(0);
        let start = at + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| damaged(format!("the chunk at {at} runs past the end")))?;
        match kind {
            CHUNK_JSON if json.is_none() => json = Some(&data[start..end]),
            CHUNK_BIN if bin.is_none() && json.is_some() => bin = Some(&data[start..end]),
            _ => {}
        }
        at = end;
    }
    let json = json.ok_or_else(|| damaged("no JSON chunk".into()))?;
    Ok((json, bin))
}

fn array<'v>(v: &'v Value, key: &str) -> &'v [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn index(v: &Value, key: &str) -> Option<usize> {
    v.get(key)?.as_u64().and_then(|i| usize::try_from(i).ok())
}

fn number(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .unwrap_or(default)
}

fn numbers<const N: usize>(v: &Value, key: &str, default: [f64; N]) -> [f64; N] {
    let list = array(v, key);
    if list.len() != N {
        return default;
    }
    let mut out = default;
    for (o, n) in out.iter_mut().zip(list) {
        match n.as_f64().filter(|n| n.is_finite()) {
            Some(n) => *o = n,
            None => return default,
        }
    }
    out
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

fn hex(b: u8) -> Option<u8> {
    char::from(b)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

/// Percent-decoding of a URI's path (`%20` and the like).
fn percent_decoded(uri: &str) -> Vec<u8> {
    let b = uri.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(high), Some(low)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(high * 16 + low);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// What a URI gives: a data URI's bytes, or a file's beside the glTF
/// file (in its folder or below; never elsewhere).
fn resource(uri: &str, folder: Option<&Path>) -> Result<Vec<u8>, String> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (header, data) = rest
            .split_once(',')
            .ok_or_else(|| "a data URI without data".to_owned())?;
        return if header.ends_with(";base64") {
            base64::decode(data)
        } else {
            Ok(percent_decoded(data))
        };
    }
    if uri.contains("://") || uri.starts_with('/') || uri.starts_with('\\') {
        return Err(format!(
            "{uri} is not a file beside the glTF file: such files are not read"
        ));
    }
    let decoded = String::from_utf8_lossy(&percent_decoded(uri)).into_owned();
    let relative = Path::new(&decoded);
    let within = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !within {
        return Err(format!(
            "{decoded} is not in the glTF file's folder: such files are not read"
        ));
    }
    let folder = folder.ok_or_else(|| format!("{decoded} beside the file is not read"))?;
    std::fs::read(folder.join(relative)).map_err(|e| format!("{decoded} cannot be read: {e}"))
}

/// An accessor's elements as numbers.
struct Accessor {
    count: usize,
    values: Vec<f64>,
}

/// Why a primitive is left out: in words, and the detail.
struct Skip {
    words: String,
    detail: String,
}

impl Skip {
    fn new(words: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            words: words.into(),
            detail: detail.into(),
        }
    }
}

const SPARSE: &str = "sparse accessor";

struct Reader<'a> {
    json: &'a Value,
    buffers: Vec<Result<Vec<u8>, String>>,
    notes: Vec<Note>,
}

impl Reader<'_> {
    fn note(&mut self, words: impl Into<String>, detail: impl Into<String>) {
        let words = words.into();
        let detail = detail.into();
        if let Some(known) = self.notes.iter_mut().find(|n| n.words == words) {
            // One note per kind; the details of the first few.
            if known.detail.len() < 400 && !known.detail.contains(&detail) {
                known.detail.push_str("; ");
                known.detail.push_str(&detail);
            }
            return;
        }
        self.notes.push(Note { words, detail });
    }

    fn load_buffers(&mut self, bin: Option<&[u8]>, folder: Option<&Path>) {
        let json = self.json;
        for (i, b) in array(json, "buffers").iter().enumerate() {
            let length = index(b, "byteLength").unwrap_or(0);
            let data = match b.get("uri").and_then(Value::as_str) {
                None if i == 0 => bin
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| "the file has no binary chunk".to_owned()),
                None => Err("a buffer without data".to_owned()),
                Some(uri) => resource(uri, folder),
            };
            let data = data.and_then(|d| {
                if d.len() < length {
                    Err(format!("a buffer holds {} of its {length} bytes", d.len()))
                } else {
                    Ok(d)
                }
            });
            self.buffers.push(data);
        }
    }

    /// A buffer view's bytes and its stride.
    fn view(&self, i: usize) -> Result<(&[u8], Option<usize>), String> {
        let v = array(self.json, "bufferViews")
            .get(i)
            .ok_or_else(|| format!("buffer view {i} is not in the file"))?;
        let buffer = index(v, "buffer").ok_or_else(|| format!("buffer view {i} has no buffer"))?;
        let data = match self.buffers.get(buffer) {
            Some(Ok(data)) => data,
            Some(Err(why)) => return Err(why.clone()),
            None => return Err(format!("buffer {buffer} is not in the file")),
        };
        let offset = index(v, "byteOffset").unwrap_or(0);
        let length =
            index(v, "byteLength").ok_or_else(|| format!("buffer view {i} has no length"))?;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| format!("buffer view {i} runs past the end of buffer {buffer}"))?;
        Ok((&data[offset..end], index(v, "byteStride")))
    }

    /// Accessor `i`, of one of the `types` (`SCALAR`, `VEC3`, ...).
    fn accessor(&self, i: usize, types: &[&str]) -> Result<Accessor, String> {
        let a = array(self.json, "accessors")
            .get(i)
            .ok_or_else(|| format!("accessor {i} is not in the file"))?;
        let kind = a.get("type").and_then(Value::as_str).unwrap_or_default();
        if !types.contains(&kind) {
            return Err(format!(
                "accessor {i} is {kind:?}, not {}",
                types.join(" or ")
            ));
        }
        if a.get("sparse").is_some() {
            return Err(SPARSE.to_owned());
        }
        let components = match kind {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return Err(format!("accessor {i} is {kind:?}")),
        };
        let component_type = index(a, "componentType").unwrap_or(0);
        let size = match component_type {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            other => return Err(format!("accessor {i} has component type {other}")),
        };
        let normalized = a.get("normalized").and_then(Value::as_bool) == Some(true);
        let count = index(a, "count").ok_or_else(|| format!("accessor {i} has no count"))?;
        if count > MAX_COUNT {
            return Err(format!("accessor {i} has {count} elements"));
        }
        // Without a buffer view an accessor is zeros (or sparse values on
        // zeros), which no mesh is made of.
        let view = index(a, "bufferView").ok_or_else(|| format!("accessor {i} has no data"))?;
        let (data, stride) = self.view(view)?;
        let element = size * components;
        let stride = stride.unwrap_or(element);
        if stride < element {
            return Err(format!(
                "accessor {i}'s stride {stride} is less than its element's {element} bytes"
            ));
        }
        let offset = index(a, "byteOffset").unwrap_or(0);
        if count > 0 {
            let last = (count - 1)
                .checked_mul(stride)
                .and_then(|s| s.checked_add(offset))
                .and_then(|s| s.checked_add(element));
            if last.is_none_or(|end| end > data.len()) {
                return Err(format!(
                    "accessor {i} runs past the end of buffer view {view}"
                ));
            }
        }
        let read = |at: usize| -> f64 {
            let b = &data[at..at + size];
            match component_type {
                5120 => {
                    let v = f64::from(b[0] as i8);
                    if normalized { (v / 127.0).max(-1.0) } else { v }
                }
                5121 => {
                    let v = f64::from(b[0]);
                    if normalized { v / 255.0 } else { v }
                }
                5122 => {
                    let v = f64::from(i16::from_le_bytes([b[0], b[1]]));
                    if normalized {
                        (v / 32767.0).max(-1.0)
                    } else {
                        v
                    }
                }
                5123 => {
                    let v = f64::from(u16::from_le_bytes([b[0], b[1]]));
                    if normalized { v / 65535.0 } else { v }
                }
                5125 => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                _ => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            }
        };
        let mut values = Vec::with_capacity(count * components);
        for k in 0..count {
            let base = offset + k * stride;
            for c in 0..components {
                values.push(read(base + c * size));
            }
        }
        Ok(Accessor { count, values })
    }

    /// A primitive's texture coordinates for its material's base colour
    /// image (see [`Primitive::uvs`]), or why there are none.
    fn uvs(
        &self,
        attributes: &Value,
        material: &Material,
        count: usize,
    ) -> Result<Vec<[f64; 2]>, Skip> {
        let set = format!("TEXCOORD_{}", material.tex_coord);
        let Some(accessor) = index(attributes, &set) else {
            return Err(Skip::new(
                "a textured mesh has no texture coordinates: its texture is projected as MitCad \
                 projects textures",
                format!("no {set}"),
            ));
        };
        let unreadable = |why: String| {
            Skip::new(
                "texture coordinates that cannot be read are left out: their texture is projected \
                 as MitCad projects textures",
                format!("{set}: {why}"),
            )
        };
        let read = self.accessor(accessor, &["VEC2"]).map_err(unreadable)?;
        if read.count != count {
            return Err(unreadable(format!(
                "{} texture coordinates for {count} positions",
                read.count
            )));
        }
        let mut out = Vec::with_capacity(count);
        for &[u, v] in read.values.as_chunks::<2>().0 {
            let [u, v] = match &material.uv_transform {
                Some(m) => [
                    m[0][0] * u + m[0][1] * v + m[0][2],
                    m[1][0] * u + m[1][1] * v + m[1][2],
                ],
                None => [u, v],
            };
            if !u.is_finite() || !v.is_finite() {
                return Err(unreadable(
                    "a texture coordinate is not a number".to_owned(),
                ));
            }
            // glTF's v runs down the image from its top; MitCad's up it
            // from its bottom.
            out.push([u, 1.0 - v]);
        }
        Ok(out)
    }

    /// A primitive's triangles (with a note on its texture coordinates
    /// where they are not read), or why it is left out.
    fn primitive(
        &self,
        p: &Value,
        materials: &[Material],
    ) -> Result<(Primitive, Option<Skip>), Skip> {
        let mode = index(p, "mode").unwrap_or(4);
        if mode <= 3 {
            return Err(Skip::new(
                "points and lines are left out: MitCad imports triangles",
                format!("mode {mode}"),
            ));
        }
        if mode > 6 {
            return Err(Skip::new(
                "a mesh of an unknown kind is left out",
                format!("mode {mode}"),
            ));
        }
        if let Some(extensions) = p.get("extensions").and_then(Value::as_object) {
            for compressed in ["KHR_draco_mesh_compression", "EXT_meshopt_compression"] {
                if extensions.contains_key(compressed) {
                    return Err(Skip::new(
                        format!("compressed meshes ({compressed}) are not read"),
                        compressed,
                    ));
                }
            }
        }
        let attributes = p.get("attributes").cloned().unwrap_or(Value::Null);
        let Some(position) = index(&attributes, "POSITION") else {
            return Err(Skip::new(
                "a mesh without positions is left out",
                "no POSITION",
            ));
        };
        let skip = |why: String| {
            if why == SPARSE {
                Skip::new("meshes in sparse accessors are not read", why)
            } else {
                Skip::new("a mesh whose data cannot be read is left out", why)
            }
        };
        let positions = self.accessor(position, &["VEC3"]).map_err(skip)?;
        let points: Vec<[f64; 3]> = positions.values.as_chunks::<3>().0.to_vec();
        if points.iter().flatten().any(|v| !v.is_finite()) {
            return Err(Skip::new(
                "a mesh whose data cannot be read is left out",
                "a position is not a number",
            ));
        }
        let corners: Vec<u32> = match index(p, "indices") {
            Some(a) => {
                let indices = self.accessor(a, &["SCALAR"]).map_err(skip)?;
                let mut out = Vec::with_capacity(indices.count);
                for v in indices.values {
                    if !(0.0..points.len() as f64).contains(&v) || v.fract() != 0.0 {
                        return Err(Skip::new(
                            "a mesh whose data cannot be read is left out",
                            format!("index {v} of {} positions", points.len()),
                        ));
                    }
                    out.push(v as u32);
                }
                out
            }
            None => (0..u32::try_from(points.len()).unwrap_or(u32::MAX)).collect(),
        };
        let n = corners.len();
        let mut triangles = Vec::with_capacity(n / 3);
        match mode {
            4 => triangles.extend_from_slice(corners.as_chunks::<3>().0),
            5 => {
                for i in 0..n.saturating_sub(2) {
                    let (a, b, c) = (corners[i], corners[i + 1], corners[i + 2]);
                    triangles.push(if i % 2 == 0 { [a, b, c] } else { [b, a, c] });
                }
            }
            _ => {
                for i in 1..n.saturating_sub(1) {
                    triangles.push([corners[0], corners[i], corners[i + 1]]);
                }
            }
        }
        triangles.retain(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
        if triangles.is_empty() {
            return Err(Skip::new(
                "a mesh without triangles is left out",
                "no triangles",
            ));
        }
        let material = index(p, "material").filter(|m| *m < materials.len());
        let textured = material
            .map(|m| &materials[m])
            .filter(|m| m.base_color_image.is_some());
        let (uvs, note) = match textured {
            Some(m) => match self.uvs(&attributes, m, points.len()) {
                Ok(uvs) => (Some(uvs), None),
                Err(skip) => (None, Some(skip)),
            },
            None => (None, None),
        };
        Ok((
            Primitive {
                positions: points,
                triangles,
                material,
                uvs,
            },
            note,
        ))
    }

    /// The image a texture shows.
    fn texture_image(&self, info: Option<&Value>) -> Option<usize> {
        let texture = index(info?, "index")?;
        let t = array(self.json, "textures").get(texture)?;
        index(t, "source").or_else(|| {
            let ext = t.get("extensions")?.as_object()?;
            ext.values().find_map(|e| index(e, "source"))
        })
    }

    fn material(&self, m: &Value) -> Material {
        let pbr = m
            .get("pbrMetallicRoughness")
            .cloned()
            .unwrap_or(Value::Null);
        let ext = |name: &str| m.get("extensions").and_then(|e| e.get(name));
        let strength = ext("KHR_materials_emissive_strength")
            .map_or(1.0, |e| number(e, "emissiveStrength", 1.0).max(0.0));
        let base = numbers(&pbr, "baseColorFactor", [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
        let base_texture = pbr.get("baseColorTexture");
        let transform = base_texture
            .and_then(|t| t.get("extensions"))
            .and_then(|e| e.get("KHR_texture_transform"));
        let tex_coord = transform
            .and_then(|t| index(t, "texCoord"))
            .or_else(|| base_texture.and_then(|t| index(t, "texCoord")))
            .unwrap_or(0);
        Material {
            name: text(m, "name"),
            base_color: base,
            metallic: number(&pbr, "metallicFactor", 1.0).clamp(0.0, 1.0),
            roughness: number(&pbr, "roughnessFactor", 1.0).clamp(0.0, 1.0),
            emissive: numbers(m, "emissiveFactor", [0.0; 3]).map(|v| v.clamp(0.0, 1.0) * strength),
            blend: m.get("alphaMode").and_then(Value::as_str) == Some("BLEND"),
            transmission: ext("KHR_materials_transmission").map_or(0.0, |e| {
                number(e, "transmissionFactor", 0.0).clamp(0.0, 1.0)
            }),
            ior: ext("KHR_materials_ior").map(|e| number(e, "ior", 1.5)),
            base_color_image: self.texture_image(base_texture),
            normal_image: self.texture_image(m.get("normalTexture")),
            occlusion_image: self.texture_image(m.get("occlusionTexture")),
            metallic_roughness_image: self.texture_image(pbr.get("metallicRoughnessTexture")),
            tex_coord,
            uv_transform: transform.map(uv_transform),
        }
    }

    fn image(&self, i: usize, folder: Option<&Path>) -> (Image, Option<String>) {
        let Some(v) = array(self.json, "images").get(i) else {
            return (
                Image::default(),
                Some(format!("image {i} is not in the file")),
            );
        };
        let uri = v.get("uri").and_then(Value::as_str);
        let bytes = match (index(v, "bufferView"), uri) {
            (Some(view), _) => self.view(view).map(|(data, _)| data.to_vec()),
            (None, Some(uri)) => resource(uri, folder),
            (None, None) => Err(format!("image {i} has no data")),
        };
        let name = text(v, "name");
        let mut image = Image {
            name: name.clone(),
            ..Image::default()
        };
        let why = match bytes {
            Ok(bytes) => {
                image.format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                    Some("png")
                } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                    Some("jpeg")
                } else {
                    None
                };
                if image.format.is_some() {
                    image.bytes = Some(bytes);
                    None
                } else {
                    Some(format!(
                        "image {i} ({}) is not a PNG or JPEG file",
                        text(v, "mimeType").unwrap_or_else(|| "no type".to_owned())
                    ))
                }
            }
            Err(why) => Some(why),
        };
        let extension = image
            .format
            .map_or("png", |f| if f == "jpeg" { "jpg" } else { "png" });
        let from_uri = uri
            .filter(|u| !u.starts_with("data:"))
            .map(|u| String::from_utf8_lossy(&percent_decoded(u)).into_owned())
            .and_then(|u| {
                Path::new(&u)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
            });
        image.file_name = from_uri.unwrap_or_else(|| {
            let stem = name.unwrap_or_else(|| format!("image{}", i + 1));
            if Path::new(&stem).extension().is_some() {
                stem
            } else {
                format!("{stem}.{extension}")
            }
        });
        (image, why)
    }

    fn node(&mut self, i: usize, v: &Value, nodes: usize, meshes: usize) -> Node {
        let matrix = if let Some(m) = v.get("matrix").and_then(Value::as_array) {
            let values: Vec<f64> = m.iter().filter_map(Value::as_f64).collect();
            if values.len() == 16 && values.iter().all(|x| x.is_finite()) {
                // Column-major in the file.
                std::array::from_fn(|r| std::array::from_fn(|c| values[c * 4 + r]))
            } else {
                self.note(
                    "a node whose placement cannot be read is placed where its parent is",
                    format!("node {i}: matrix"),
                );
                IDENTITY
            }
        } else {
            let t = numbers(v, "translation", [0.0; 3]);
            let [mut x, mut y, mut z, mut w] = numbers(v, "rotation", [0.0, 0.0, 0.0, 1.0]);
            let length = (x * x + y * y + z * z + w * w).sqrt();
            if length > 0.0 {
                (x, y, z, w) = (x / length, y / length, z / length, w / length);
            } else {
                (x, y, z, w) = (0.0, 0.0, 0.0, 1.0);
            }
            let s = numbers(v, "scale", [1.0; 3]);
            let r = [
                [
                    1.0 - 2.0 * (y * y + z * z),
                    2.0 * (x * y - z * w),
                    2.0 * (x * z + y * w),
                ],
                [
                    2.0 * (x * y + z * w),
                    1.0 - 2.0 * (x * x + z * z),
                    2.0 * (y * z - x * w),
                ],
                [
                    2.0 * (x * z - y * w),
                    2.0 * (y * z + x * w),
                    1.0 - 2.0 * (x * x + y * y),
                ],
            ];
            [
                [r[0][0] * s[0], r[0][1] * s[1], r[0][2] * s[2], t[0]],
                [r[1][0] * s[0], r[1][1] * s[1], r[1][2] * s[2], t[1]],
                [r[2][0] * s[0], r[2][1] * s[1], r[2][2] * s[2], t[2]],
                [0.0, 0.0, 0.0, 1.0],
            ]
        };
        if v.get("skin").is_some() {
            self.note(
                "skins are not read: skinned meshes come in as their nodes place them",
                format!("node {i}"),
            );
        }
        let mut children = Vec::new();
        for c in array(v, "children") {
            match c.as_u64().and_then(|c| usize::try_from(c).ok()) {
                Some(c) if c < nodes && c != i => children.push(c),
                _ => self.note(
                    "a node that is not in the file is left out",
                    format!("node {i}: child {c}"),
                ),
            }
        }
        let mesh = index(v, "mesh");
        if mesh.is_some_and(|m| m >= meshes) {
            self.note(
                "a mesh that is not in the file is left out",
                format!("node {i}: mesh {}", mesh.unwrap_or_default()),
            );
        }
        Node {
            name: text(v, "name"),
            mesh: mesh.filter(|m| *m < meshes),
            children,
            matrix,
        }
    }

    fn read_all(mut self, folder: Option<&Path>) -> Result<Gltf, Error> {
        let json = self.json;
        // The materials, and the images they show.
        let materials: Vec<Material> = array(json, "materials")
            .iter()
            .map(|m| self.material(m))
            .collect();
        let image_count = array(json, "images").len();
        let mut images: Vec<Image> = vec![Image::default(); image_count];
        let mut used = vec![false; image_count];
        for (k, m) in materials.iter().enumerate() {
            for i in [
                m.base_color_image,
                m.normal_image,
                m.occlusion_image,
                m.metallic_roughness_image,
            ] {
                let Some(i) = i else { continue };
                if i >= image_count {
                    self.note(
                        "a texture image that is not in the file is left out",
                        format!("material {k}: image {i}"),
                    );
                    continue;
                }
                if !used[i] {
                    used[i] = true;
                    let (image, why) = self.image(i, folder);
                    if let Some(why) = why {
                        self.note(
                            "a texture image that is not a PNG or JPEG file it can read is left out \
                             (its material keeps its colour)",
                            why,
                        );
                    }
                    images[i] = image;
                }
            }
        }

        // The meshes' triangles.
        let mut meshes = Vec::new();
        let mut total = 0usize;
        for (k, m) in array(json, "meshes").iter().enumerate() {
            let mut primitives = Vec::new();
            for (j, p) in array(m, "primitives").iter().enumerate() {
                if !array(p, "targets").is_empty() {
                    self.note(
                        "morph targets are not read: the meshes come in in their base shape",
                        format!("mesh {k}, primitive {j}"),
                    );
                }
                match self.primitive(p, &materials) {
                    Ok((primitive, uv_note)) => {
                        if let Some(skip) = uv_note {
                            self.note(
                                skip.words,
                                format!("mesh {k}, primitive {j}: {}", skip.detail),
                            );
                        }
                        total += primitive.triangles.len();
                        if total > MAX_TRIANGLES {
                            return Err(Error::new(
                                format!(
                                    "the file has more than {} million triangles: more than MitCad imports",
                                    MAX_TRIANGLES / 1_000_000
                                ),
                                format!("mesh {k}"),
                            ));
                        }
                        primitives.push(primitive);
                    }
                    Err(skip) => {
                        self.note(
                            skip.words,
                            format!("mesh {k}, primitive {j}: {}", skip.detail),
                        );
                    }
                }
            }
            meshes.push(Mesh {
                name: text(m, "name"),
                primitives,
            });
        }

        // The nodes and the scene.
        let node_values = array(json, "nodes");
        let count = node_values.len();
        let mut nodes = Vec::with_capacity(count);
        for (i, v) in node_values.iter().enumerate() {
            nodes.push(self.node(i, v, count, meshes.len()));
        }
        let scenes = array(json, "scenes");
        let roots: Vec<usize> = if scenes.is_empty() {
            let mut child = vec![false; count];
            for n in &nodes {
                for c in &n.children {
                    child[*c] = true;
                }
            }
            (0..count).filter(|i| !child[*i]).collect()
        } else {
            let scene = index(json, "scene")
                .filter(|s| *s < scenes.len())
                .unwrap_or(0);
            if scenes.len() > 1 {
                self.note(
                    "only one of the file's scenes is imported",
                    format!("scene {scene} of {}", scenes.len()),
                );
            }
            array(&scenes[scene], "nodes")
                .iter()
                .filter_map(|n| n.as_u64().and_then(|n| usize::try_from(n).ok()))
                .filter(|n| *n < count)
                .collect()
        };

        // The rest of the file that is not read.
        let animations = array(json, "animations").len();
        if animations > 0 {
            self.note(
                "animations are not read: the bodies come in as the file places them",
                format!("{animations} animations"),
            );
        }
        let required: Vec<&str> = array(json, "extensionsRequired")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for ext in array(json, "extensionsUsed")
            .iter()
            .filter_map(Value::as_str)
        {
            if KNOWN_EXTENSIONS.contains(&ext)
                || ext == "KHR_draco_mesh_compression"
                || ext == "EXT_meshopt_compression"
            {
                continue;
            }
            let words = if required.contains(&ext) {
                format!("the file's extension {ext} is not read, although the file needs it")
            } else {
                format!("the file's extension {ext} is not read")
            };
            self.note(words, ext);
        }

        Ok(Gltf {
            generator: json.get("asset").and_then(|a| text(a, "generator")),
            nodes,
            meshes,
            materials,
            images,
            roots,
            notes: self.notes,
        })
    }
}

impl Gltf {
    /// A node's name for a person: its own, its mesh's, else `Node <n>`.
    pub fn node_name(&self, i: usize) -> String {
        let node = &self.nodes[i];
        node.name
            .clone()
            .or_else(|| node.mesh.and_then(|m| self.meshes[m].name.clone()))
            .unwrap_or_else(|| format!("Node {}", i + 1))
    }

    /// The nodes the scene places, each once, in the order of a walk from
    /// the roots: (node, parent, its placement in MitCad's millimetres and
    /// axes in the parent, and in the design). A root's placement includes
    /// [`Y_UP_TO_Z_UP`].
    pub fn placed_nodes(&self) -> Vec<PlacedNode> {
        let mut seen = vec![false; self.nodes.len()];
        let mut out = Vec::new();
        let mut stack: Vec<(usize, Option<usize>, Matrix, usize)> = self
            .roots
            .iter()
            .rev()
            .map(|r| (*r, None, IDENTITY, 0))
            .collect();
        while let Some((i, parent, above, depth)) = stack.pop() {
            if i >= self.nodes.len() || seen[i] || depth > MAX_DEPTH {
                continue;
            }
            seen[i] = true;
            let mut local = self.nodes[i].matrix;
            if parent.is_none() {
                local = multiply(&Y_UP_TO_Z_UP, &local);
            }
            for row in local.iter_mut().take(3) {
                row[3] *= MM_PER_UNIT;
            }
            let world = multiply(&above, &local);
            for c in self.nodes[i].children.iter().rev() {
                stack.push((*c, Some(i), world, depth + 1));
            }
            out.push(PlacedNode {
                node: i,
                parent,
                local,
                world,
            });
        }
        out
    }

    /// The file as the `gltf_file` query answers it: each placed node
    /// with its name, path of names, placements (MitCad's millimetres and
    /// axes, rows of 4), triangles, whether its mesh has texture
    /// coordinates for its images (`texture_coordinates`) and their box
    /// (`uv_bounds`), its mesh's materials (sRGB), and the notes in words.
    pub fn summary(&self) -> Value {
        let placed = self.placed_nodes();
        let mut paths: Vec<String> = vec![String::new(); self.nodes.len()];
        let rows = |m: &Matrix| -> Value { json!([m[0], m[1], m[2]]) };
        let material = |k: Option<usize>| -> Value {
            let m = k
                .and_then(|k| self.materials.get(k))
                .cloned()
                .unwrap_or_default();
            json!({
                "name": m.name,
                "base_color": [srgb(m.base_color[0]), srgb(m.base_color[1]), srgb(m.base_color[2])],
                "alpha": m.base_color[3],
                "metallic": m.metallic,
                "roughness": m.roughness,
                "texture": m.base_color_image.and_then(|i| self.images.get(i)).is_some_and(|i| i.bytes.is_some()),
                "metallic_roughness_texture": m.metallic_roughness_image.and_then(|i| self.images.get(i)).is_some_and(|i| i.bytes.is_some()),
            })
        };
        let mut list = Vec::new();
        for p in &placed {
            let name = self.node_name(p.node);
            paths[p.node] = match p.parent {
                Some(parent) => format!("{}/{name}", paths[parent]),
                None => name.clone(),
            };
            let mesh = self.nodes[p.node].mesh.map(|m| &self.meshes[m]);
            let triangles: usize =
                mesh.map_or(0, |m| m.primitives.iter().map(|p| p.triangles.len()).sum());
            let materials: Vec<Value> = mesh
                .map(|m| m.primitives.iter().map(|p| material(p.material)).collect())
                .unwrap_or_default();
            let uvs = mesh.is_some_and(|m| m.primitives.iter().any(|p| p.uvs.is_some()));
            // The box of its texture coordinates (MitCad's), [[u, v] low,
            // [u, v] high].
            let uv_bounds = mesh
                .into_iter()
                .flat_map(|m| &m.primitives)
                .filter_map(|p| p.uvs.as_ref())
                .flatten()
                .fold(None, |bounds: Option<[[f64; 2]; 2]>, uv| {
                    Some(match bounds {
                        None => [*uv, *uv],
                        Some([low, high]) => [
                            [low[0].min(uv[0]), low[1].min(uv[1])],
                            [high[0].max(uv[0]), high[1].max(uv[1])],
                        ],
                    })
                });
            list.push(json!({
                "name": name,
                "path": paths[p.node],
                "mesh": self.nodes[p.node].mesh,
                "children": self.nodes[p.node].children.len(),
                "triangles": triangles,
                "texture_coordinates": uvs,
                "uv_bounds": uv_bounds,
                "materials": materials,
                "transform": rows(&p.local),
                "world": rows(&p.world),
            }));
        }
        json!({
            "generator": self.generator,
            "nodes": list,
            "meshes": self.meshes.len(),
            "triangles": self.meshes.iter().flat_map(|m| &m.primitives).map(|p| p.triangles.len()).sum::<usize>(),
            "materials": self.materials.len(),
            "images": self.images.iter().filter(|i| i.bytes.is_some()).count(),
            "notes": self.notes.iter().map(|n| n.words.clone()).collect::<Vec<_>>(),
            "note_details": self.notes.iter().map(|n| json!({"note": n.words, "detail": n.detail})).collect::<Vec<_>>(),
        })
    }
}

/// A node as the scene places it ([`Gltf::placed_nodes`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedNode {
    pub node: usize,
    pub parent: Option<usize>,
    /// In its parent, MitCad's millimetres and axes.
    pub local: Matrix,
    /// In the design.
    pub world: Matrix,
}
