// SPDX-License-Identifier: MIT
//! Reader of `.serp` files (mitcad#1870), the native files of an
//! open-source NURBS modeller built on OCCT, versions 1 to
//! [`NEWEST_VERSION`]:
//!
//! - Version 1 is a bare JSON scene; versions 2 and later a zip holding
//!   it as `document.json` beside `meta.json`, a thumbnail, and from
//!   version 3 binary members (`blobs/`, `images/`) of point clouds and
//!   pictures. The point clouds' points are read ([`Cloud`]: the scene's
//!   `pointclouds` entries, their `blobs/<id>/xyz.f32` little-endian
//!   float32 points from an optional float64 `origin`, `rgb.u8` colours;
//!   mitcad#2240); the pictures are not taken.
//! - The scene: its `units`, its `layers` (an id, a name, a colour,
//!   whether it is shown, its parent layer), its `objects` (a name, a
//!   layer, whether it is shown, a colour, the block it places, and its
//!   shape) and its `block_defs` (a name and the block's shapes).
//! - A shape is base64: the B-rep as OCCT's binary format writes it
//!   ([`Shape::Brep`], read by the kernel), a mesh (`mesh` of an object:
//!   base64 float32 vertices `v`, uint32 triangles `t`, an optional origin
//!   `o`; a block's mesh shape as tagged bytes), or the tagged bytes of a
//!   text, a hatch, a dimension, a picture or a point cloud
//!   ([`Shape::Other`]), which the import leaves out.
//!
//! A version newer than the reader knows is refused in words, never read
//! half. The file is untrusted: every count and index is checked before it
//! is used; an object whose data cannot be read is [`Shape::Unreadable`]
//! and the rest of the file is read. Errors and notes have `words` for a
//! person and a `detail` for agents and the log.
//!
//! Written from the format as its writer saves it; no other reader's code.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

mod base64;
pub mod testfile;

#[cfg(test)]
mod tests;

/// The newest format version this reader knows.
pub const NEWEST_VERSION: u64 = 9;

/// The most bytes the scene's JSON may take once unpacked.
const MAX_SCENE: u64 = 2 << 30;

/// The most triangles one mesh may have.
pub const MAX_TRIANGLES: usize = 50_000_000;

/// What separates the names of a layer's path (`Parts::Bolts`).
pub const PATH_SEPARATOR: &str = "::";

/// A file that cannot be read, or something of it that is not read: what
/// a person reads (`words`) and where it is for agents and the log
/// (`detail`).
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

/// The scene's length unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Units {
    /// The unit's symbol as MitCad writes it (`mm`, `cm`, `m`, `in`, `ft`).
    pub symbol: &'static str,
    /// Millimetres per unit.
    pub mm: f64,
    /// The file gives no unit, or one not known: millimeters assumed.
    pub assumed: bool,
}

/// A layer of the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub id: String,
    pub name: String,
    /// sRGB, 0 to 1.
    pub color: Option<[f64; 3]>,
    pub visible: bool,
    /// Its parent layer (an index into [`Serp::layers`]).
    pub parent: Option<usize>,
}

/// A mesh in the scene's units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    /// Indices into `vertices`, each checked.
    pub triangles: Vec<[u32; 3]>,
}

/// Kinds of the scene's content that the import does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Other {
    Text,
    Hatch,
    Dimension,
    Picture,
    PointCloud,
}

/// The shape of an object or of a block.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// OCCT's binary B-rep data (or its text form), for the kernel.
    Brep(Vec<u8>),
    Mesh(Mesh),
    Other(Other),
    /// Data that cannot be read: why, for agents and the log.
    Unreadable(String),
}

/// An object of the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    /// Its name; None when it has none.
    pub name: Option<String>,
    /// Its layer (an index into [`Serp::layers`]).
    pub layer: Option<usize>,
    pub visible: bool,
    /// Its own colour (sRGB, 0 to 1); its layer's when None.
    pub color: Option<[f64; 3]>,
    /// The block it places (an index into [`Serp::blocks`]): its shape is
    /// the block's shapes placed (the file keeps no placement).
    pub block: Option<usize>,
    pub shape: Shape,
}

/// A block definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: String,
    pub name: String,
    pub shapes: Vec<Shape>,
}

/// How many of each kind the scene has that the import does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Skipped {
    pub point_clouds: usize,
    pub pictures: usize,
    pub texts: usize,
    pub hatches: usize,
    pub dimensions: usize,
    pub layouts: usize,
    pub history_records: usize,
    pub scan_paths: usize,
}

impl Skipped {
    fn count(&mut self, other: Other) {
        match other {
            Other::Text => self.texts += 1,
            Other::Hatch => self.hatches += 1,
            Other::Dimension => self.dimensions += 1,
            Other::Picture => self.pictures += 1,
            Other::PointCloud => self.point_clouds += 1,
        }
    }

    /// Each kind with its count in words (`2 point clouds`), in a fixed
    /// order; empty when nothing is left out.
    pub fn parts(&self) -> Vec<String> {
        let kinds = [
            (self.point_clouds, "point cloud", "point clouds"),
            (self.scan_paths, "scan path", "scan paths"),
            (self.pictures, "picture", "pictures"),
            (self.hatches, "hatch", "hatches"),
            (self.texts, "text", "texts"),
            (self.dimensions, "dimension", "dimensions"),
            (self.layouts, "layout", "layouts"),
            (self.history_records, "history record", "history records"),
        ];
        kinds
            .iter()
            .filter(|(n, _, _)| *n > 0)
            .map(|(n, one, many)| format!("{n} {}", if *n == 1 { one } else { many }))
            .collect()
    }
}

/// A `.serp` file as read.
#[derive(Debug, Clone, PartialEq)]
pub struct Serp {
    /// The format version (1 for a bare JSON scene without one).
    pub version: u64,
    /// A zip (version 2 and later) rather than bare JSON.
    pub container: bool,
    pub units: Units,
    pub layers: Vec<Layer>,
    pub objects: Vec<Object>,
    pub blocks: Vec<Block>,
    pub skipped: Skipped,
    /// What was not read as it should be, in words with a detail.
    pub notes: Vec<Error>,
    /// The point clouds whose points the container holds (mitcad#2240).
    pub clouds: Vec<Cloud>,
}

/// The most points one point cloud may have.
pub const MAX_CLOUD_POINTS: usize = 20_000_000;

/// A point cloud of the scene: its points from the container (versions 3
/// and later), in the file's units.
#[derive(Debug, Clone, PartialEq)]
pub struct Cloud {
    pub name: Option<String>,
    /// Its layer (an index into [`Serp::layers`]).
    pub layer: Option<usize>,
    pub visible: bool,
    /// What the points are measured from (a far cloud's), else zero.
    pub origin: [f64; 3],
    /// x, y, z per point from the origin.
    pub points: Vec<[f32; 3]>,
    /// The colour of each point, when the file has them.
    pub colors: Option<Vec<[u8; 3]>>,
}

impl Serp {
    /// The layer's path of names from the top (`Parts::Bolts`).
    pub fn layer_path(&self, layer: usize) -> String {
        let mut names = Vec::new();
        let mut at = Some(layer);
        // A file's parents could loop: each layer is taken at most once.
        while let Some(i) = at {
            if names.len() > self.layers.len() {
                break;
            }
            let Some(l) = self.layers.get(i) else { break };
            names.push(l.name.as_str());
            at = l.parent;
        }
        names.reverse();
        names.join(PATH_SEPARATOR)
    }

    /// Whether the layer and every layer above it are shown.
    pub fn layer_shown(&self, layer: usize) -> bool {
        let mut at = Some(layer);
        let mut steps = 0;
        while let Some(i) = at {
            if steps > self.layers.len() {
                break;
            }
            steps += 1;
            let Some(l) = self.layers.get(i) else { break };
            if !l.visible {
                return false;
            }
            at = l.parent;
        }
        true
    }

    /// Whether the object is shown: itself and its layers.
    pub fn shown(&self, object: &Object) -> bool {
        object.visible && object.layer.is_none_or(|l| self.layer_shown(l))
    }

    /// Whether a point cloud is shown: itself and its layers.
    pub fn cloud_shown(&self, cloud: &Cloud) -> bool {
        cloud.visible && cloud.layer.is_none_or(|l| self.layer_shown(l))
    }

    /// The object's colour: its own, else its layer's.
    pub fn color(&self, object: &Object) -> Option<[f64; 3]> {
        object
            .color
            .or_else(|| object.layer.and_then(|l| self.layers.get(l)?.color))
    }
}

/// Reads a `.serp` file.
pub fn read_file(path: &Path) -> Result<Serp, Error> {
    let bytes =
        std::fs::read(path).map_err(|e| Error::new("the file cannot be read", e.to_string()))?;
    read(&bytes)
}

/// The scene's JSON of a container: `document.json`, and the version of
/// `meta.json` where it has one.
fn unpack(bytes: &[u8]) -> Result<(Vec<u8>, Option<u64>, mitcad_zip::Archive), Error> {
    let archive = mitcad_zip::Archive::new(bytes.to_vec()).map_err(|e| {
        Error::new(
            "the file is damaged: its container cannot be read",
            format!("zip: {e}"),
        )
    })?;
    let scene = archive
        .read_by_name_with_limit("document.json", MAX_SCENE)
        .map_err(|e| match e {
            mitcad_zip::ZipError::NotFound(_) => Error::new(
                "the file holds no scene: it is not a .serp file",
                "no document.json member",
            ),
            other => Error::new(
                "the file is damaged: its scene cannot be unpacked",
                format!("document.json: {other}"),
            ),
        })?;
    let meta = archive
        .read_by_name_with_limit("meta.json", 1 << 20)
        .ok()
        .and_then(|m| serde_json::from_slice::<Value>(&m).ok())
        .and_then(|m| m.get("version").and_then(version_of));
    Ok((scene, meta, archive))
}

fn version_of(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Reads a `.serp` file's bytes.
pub fn read(bytes: &[u8]) -> Result<Serp, Error> {
    let container = bytes.starts_with(b"PK");
    let (scene, meta_version, archive) = if container {
        let (scene, meta, archive) = unpack(bytes)?;
        (scene, meta, Some(archive))
    } else {
        (bytes.to_vec(), None, None)
    };
    let text = scene.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&scene);
    let doc: Value = serde_json::from_slice(text).map_err(|e| {
        Error::new(
            "the file is not a .serp file or its scene is damaged",
            format!("the scene's JSON: {e}"),
        )
    })?;
    drop(scene);
    if !doc.is_object() {
        return Err(Error::new(
            "the file is not a .serp file",
            "the scene is not a JSON object",
        ));
    }
    // The writer names its format after itself; every name it has used
    // starts as the files' extension does.
    let format = doc.get("format").and_then(Value::as_str).unwrap_or("");
    if !format.to_ascii_lowercase().starts_with("serp") {
        return Err(Error::new(
            "the file is not a .serp file: its scene does not say it is one",
            format!("format {format:?}"),
        ));
    }
    let version = doc.get("version").and_then(version_of).unwrap_or(1);
    let newest = version.max(meta_version.unwrap_or(0));
    if newest > NEWEST_VERSION {
        return Err(Error::new(
            format!(
                "the file was made by a newer version of its program (format version {newest}; MitCad reads versions 1 to {NEWEST_VERSION})"
            ),
            format!("version {version}, meta.json version {meta_version:?}"),
        ));
    }

    let mut notes = Vec::new();
    let units = units_of(doc.get("units"), &mut notes);
    let layers = layers_of(doc.get("layers"));
    let layer_index: HashMap<&str, usize> = layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.id.as_str(), i))
        .collect();
    let mut skipped = Skipped::default();

    let mut blocks = Vec::new();
    if let Some(defs) = doc.get("block_defs").and_then(Value::as_object) {
        for (id, def) in defs {
            let name = def
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map_or_else(|| format!("Block {}", blocks.len() + 1), str::to_owned);
            let shapes = def
                .get("shapes")
                .and_then(Value::as_array)
                .map(|shapes| {
                    shapes
                        .iter()
                        .map(|s| match s.as_str() {
                            Some(text) => shape_of_base64(text),
                            None => Shape::Unreadable("a block's shape is not text".to_owned()),
                        })
                        .collect()
                })
                .unwrap_or_default();
            blocks.push(Block {
                id: id.clone(),
                name,
                shapes,
            });
        }
    }
    let block_index: HashMap<String, usize> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.clone(), i))
        .collect();

    let mut objects = Vec::new();
    for od in doc
        .get("objects")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let shape = if let Some(mesh) = od.get("mesh").filter(|m| m.is_object()) {
            match mesh_from_json(mesh) {
                Ok(mesh) => Shape::Mesh(mesh),
                Err(why) => Shape::Unreadable(why),
            }
        } else if let Some(text) = od.get("brep").and_then(Value::as_str) {
            shape_of_base64(text)
        } else {
            Shape::Unreadable("the object has neither a B-rep nor a mesh".to_owned())
        };
        if let Shape::Other(other) = shape {
            skipped.count(other);
        }
        objects.push(Object {
            name: od
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned),
            layer: od
                .get("layer")
                .and_then(id_of)
                .and_then(|id| layer_index.get(id.as_str()).copied()),
            visible: od.get("visible").and_then(Value::as_bool).unwrap_or(true),
            color: od.get("color").and_then(color_of),
            block: od
                .get("block")
                .and_then(id_of)
                .and_then(|id| block_index.get(&id).copied()),
            shape,
        });
    }
    let count = |key: &str| doc.get(key).and_then(Value::as_array).map_or(0, Vec::len);
    // Point clouds (mitcad#2240): their points from the container's blobs;
    // one whose points cannot be read (a bare scene keeps none) is counted
    // as left out.
    let mut clouds = Vec::new();
    for entry in doc
        .get("pointclouds")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        match archive.as_ref().map(|a| cloud_of(a, entry, &layer_index)) {
            Some(Ok(cloud)) => clouds.push(cloud),
            // Counted in words with the rest that is left out.
            Some(Err(_)) | None => skipped.point_clouds += 1,
        }
    }
    skipped.pictures += count("image_planes");
    skipped.layouts += count("layouts");
    skipped.history_records += count("history_records");
    skipped.scan_paths += count("trajectories");

    Ok(Serp {
        version,
        container,
        units,
        layers,
        objects,
        blocks,
        skipped,
        notes,
        clouds,
    })
}

/// A point cloud entry of the scene with its points read from the
/// container (`blobs.xyz`, little-endian float32 x, y, z per point;
/// `blobs.rgb`, a byte per channel, dropped when its count differs), or
/// why it cannot be read.
fn cloud_of(
    archive: &mitcad_zip::Archive,
    entry: &Value,
    layer_index: &HashMap<&str, usize>,
) -> Result<Cloud, String> {
    let blobs = entry.get("blobs");
    let member = |key: &str| blobs.and_then(|b| b.get(key)).and_then(Value::as_str);
    let xyz = member("xyz").ok_or_else(|| "it names no xyz member".to_owned())?;
    let limit = (MAX_CLOUD_POINTS * 12) as u64;
    let bytes = archive
        .read_by_name_with_limit(xyz, limit)
        .map_err(|e| format!("{xyz}: {e}"))?;
    if bytes.is_empty() || !bytes.len().is_multiple_of(12) {
        return Err(format!("{xyz}: not whole x, y, z triples"));
    }
    let points: Vec<[f32; 3]> = bytes
        .as_chunks::<12>()
        .0
        .iter()
        .map(|p| {
            std::array::from_fn(|k| {
                f32::from_le_bytes([p[4 * k], p[4 * k + 1], p[4 * k + 2], p[4 * k + 3]])
            })
        })
        .collect();
    if !points.iter().flatten().all(|c| c.is_finite()) {
        return Err(format!("{xyz}: a coordinate is not finite"));
    }
    let colors = member("rgb")
        .and_then(|rgb| archive.read_by_name_with_limit(rgb, limit / 4).ok())
        .filter(|c| c.len() == points.len() * 3)
        .map(|c| c.as_chunks::<3>().0.to_vec());
    let origin = match entry.get("origin").and_then(Value::as_array) {
        Some(o) if o.len() == 3 => {
            let o: Vec<f64> = o.iter().filter_map(Value::as_f64).collect();
            if o.len() != 3 || !o.iter().all(|c| c.is_finite()) {
                return Err("its origin is not three numbers".to_owned());
            }
            [o[0], o[1], o[2]]
        }
        _ => [0.0; 3],
    };
    Ok(Cloud {
        name: entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_owned),
        layer: entry
            .get("layer")
            .and_then(id_of)
            .and_then(|id| layer_index.get(id.as_str()).copied()),
        visible: entry
            .get("visible")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        origin,
        points,
        colors,
    })
}

/// An id the file keeps as text or as a number.
fn id_of(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn units_of(value: Option<&Value>, notes: &mut Vec<Error>) -> Units {
    let known = |symbol: &str| -> Option<(&'static str, f64)> {
        Some(match symbol {
            "mm" => ("mm", 1.0),
            "cm" => ("cm", 10.0),
            "m" => ("m", 1000.0),
            "in" => ("in", 25.4),
            "ft" => ("ft", 304.8),
            _ => return None,
        })
    };
    let name = value.and_then(Value::as_str).map(str::trim);
    match name.map(|n| (n, known(&n.to_ascii_lowercase()))) {
        Some((_, Some((symbol, mm)))) => Units {
            symbol,
            mm,
            assumed: false,
        },
        Some((other, None)) => {
            notes.push(Error::new(
                "the file's length unit is not one MitCad knows: millimeters assumed",
                format!("units {other:?}"),
            ));
            Units {
                symbol: "mm",
                mm: 1.0,
                assumed: true,
            }
        }
        None => Units {
            symbol: "mm",
            mm: 1.0,
            assumed: true,
        },
    }
}

/// A colour of 3 (or more) numbers from 0 to 1, or from 0 to 255.
fn color_of(value: &Value) -> Option<[f64; 3]> {
    let list = value.as_array()?;
    if list.len() < 3 {
        return None;
    }
    let mut c = [0.0; 3];
    for (k, v) in c.iter_mut().enumerate() {
        *v = list[k].as_f64().filter(|v| v.is_finite())?;
    }
    let scale = if c.iter().any(|v| *v > 1.0) {
        255.0
    } else {
        1.0
    };
    Some(c.map(|v| (v / scale).clamp(0.0, 1.0)))
}

fn layers_of(value: Option<&Value>) -> Vec<Layer> {
    let list = value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut layers: Vec<Layer> = list
        .iter()
        .enumerate()
        .map(|(i, l)| Layer {
            id: l
                .get("id")
                .and_then(id_of)
                .unwrap_or_else(|| format!("#{i}")),
            name: l
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map_or_else(|| format!("Layer {}", i + 1), str::to_owned),
            color: l.get("color").and_then(color_of),
            visible: l.get("visible").and_then(Value::as_bool).unwrap_or(true),
            parent: None,
        })
        .collect();
    // Parents in a second pass: a layer may come before its parent.
    let index: HashMap<String, usize> = layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.id.clone(), i))
        .collect();
    for (i, l) in list.iter().enumerate() {
        layers[i].parent = l
            .get("parent")
            .and_then(id_of)
            .and_then(|p| index.get(&p).copied())
            .filter(|p| *p != i);
    }
    layers
}

/// The tags of the shapes the writer keeps in its own byte forms.
const MESH_TAG: &[u8] = b"SMSH";
const OTHER_TAGS: [(&[u8], Other); 5] = [
    (b"STXT", Other::Text),
    (b"SHCH", Other::Hatch),
    (b"SDIM", Other::Dimension),
    (b"SERPPICTURE", Other::Picture),
    (b"SPCL", Other::PointCloud),
];

fn shape_of_base64(text: &str) -> Shape {
    match base64::decode(text) {
        Ok(data) => shape_of(data),
        Err(why) => Shape::Unreadable(why),
    }
}

/// The shape of a block's or an object's bytes: a mesh, another kind by
/// its tag, else B-rep data.
pub fn shape_of(data: Vec<u8>) -> Shape {
    if data.is_empty() {
        return Shape::Unreadable("the shape's data is empty".to_owned());
    }
    if data.starts_with(MESH_TAG) {
        return match mesh_from_tagged(&data) {
            Ok(mesh) => Shape::Mesh(mesh),
            Err(why) => Shape::Unreadable(why),
        };
    }
    for (tag, other) in OTHER_TAGS {
        if data.starts_with(tag) {
            return Shape::Other(other);
        }
    }
    Shape::Brep(data)
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn f64_at(data: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

/// A mesh of the writer's tagged form: `SMSH`, its form (1, or 2 with a
/// float64 origin), the vertex and triangle counts (uint32), then the
/// float32 vertices and uint32 triangles, little-endian.
fn mesh_from_tagged(data: &[u8]) -> Result<Mesh, String> {
    let form = data.get(4).copied().unwrap_or(0);
    let (nv, nt) = match (u32_at(data, 5), u32_at(data, 9)) {
        (Some(nv), Some(nt)) => (nv as usize, nt as usize),
        _ => return Err("the mesh's data ends before its counts".to_owned()),
    };
    let (origin, head) = match form {
        1 => ([0.0; 3], 13),
        2 => {
            let o: Option<Vec<f64>> = (0..3).map(|k| f64_at(data, 13 + 8 * k)).collect();
            let o = o.ok_or("the mesh's data ends before its origin")?;
            ([o[0], o[1], o[2]], 13 + 24)
        }
        other => {
            return Err(format!(
                "a mesh of form {other}, which this reader does not know"
            ));
        }
    };
    let vertex_bytes = nv.checked_mul(12).ok_or("too many vertices")?;
    let triangle_bytes = nt.checked_mul(12).ok_or("too many triangles")?;
    let cut = head + vertex_bytes;
    let end = cut
        .checked_add(triangle_bytes)
        .ok_or("too many triangles")?;
    if data.len() < end {
        return Err(format!(
            "the mesh's data ends early ({end} bytes needed, {} there)",
            data.len()
        ));
    }
    mesh(&data[head..cut], &data[cut..end], origin)
}

/// An object's mesh: base64 float32 vertices `v`, uint32 triangles `t`
/// and an optional origin `o` they are measured from.
fn mesh_from_json(value: &Value) -> Result<Mesh, String> {
    let vertices = value
        .get("v")
        .and_then(Value::as_str)
        .ok_or("the mesh has no vertices")?;
    let triangles = value
        .get("t")
        .and_then(Value::as_str)
        .ok_or("the mesh has no triangles")?;
    let origin = match value.get("o") {
        None | Some(Value::Null) => [0.0; 3],
        Some(o) => {
            let list = o.as_array().ok_or("the mesh's origin is not a point")?;
            let p: Option<Vec<f64>> = list.iter().take(3).map(Value::as_f64).collect();
            match p.as_deref() {
                Some([x, y, z]) if [x, y, z].iter().all(|v| v.is_finite()) => [*x, *y, *z],
                _ => return Err("the mesh's origin is not a point".to_owned()),
            }
        }
    };
    mesh(
        &base64::decode(vertices)?,
        &base64::decode(triangles)?,
        origin,
    )
}

fn mesh(vertices: &[u8], triangles: &[u8], origin: [f64; 3]) -> Result<Mesh, String> {
    if !vertices.len().is_multiple_of(12) || !triangles.len().is_multiple_of(12) {
        return Err("the mesh's data does not hold whole points and triangles".to_owned());
    }
    if triangles.len() / 12 > MAX_TRIANGLES {
        return Err(format!(
            "the mesh has {} triangles, more than MitCad reads ({MAX_TRIANGLES})",
            triangles.len() / 12
        ));
    }
    let f32_at = |b: &[u8], k: usize| f32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]);
    let mut points = Vec::with_capacity(vertices.len() / 12);
    for p in vertices.as_chunks::<12>().0 {
        let point: [f64; 3] = std::array::from_fn(|k| f64::from(f32_at(p, 4 * k)) + origin[k]);
        if !point.iter().all(|v| v.is_finite()) {
            return Err(format!(
                "point {} of the mesh is not a number",
                points.len()
            ));
        }
        points.push(point);
    }
    let count = points.len();
    let mut list = Vec::with_capacity(triangles.len() / 12);
    for t in triangles.as_chunks::<12>().0 {
        let triangle: [u32; 3] = std::array::from_fn(|k| {
            u32::from_le_bytes([t[4 * k], t[4 * k + 1], t[4 * k + 2], t[4 * k + 3]])
        });
        if triangle.iter().any(|i| *i as usize >= count) {
            return Err(format!(
                "triangle {} of the mesh names a point it does not have",
                list.len()
            ));
        }
        list.push(triangle);
    }
    Ok(Mesh {
        vertices: points,
        triangles: list,
    })
}
