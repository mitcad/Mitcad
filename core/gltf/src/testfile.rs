// SPDX-License-Identifier: MIT
//! A writer of small glTF files for the tests: the reader's own and the
//! import's (`core/model-tests/files`). Not a general writer: MitCad's
//! files are written by the application (`app/web/WebGlb.cpp`).

use serde_json::{Map, Value, json};

/// A scene to write: nodes, meshes, materials and PNG or JPEG images.
#[derive(Debug, Clone, Default)]
pub struct TestScene {
    pub nodes: Vec<TestNode>,
    pub roots: Vec<usize>,
    pub meshes: Vec<TestMesh>,
    pub materials: Vec<TestMaterial>,
    pub images: Vec<Vec<u8>>,
    /// Members added to the top level as they are (`animations`,
    /// `extensionsUsed`, ...).
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct TestNode {
    pub name: Option<String>,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
    /// Column-major, as the file keeps it.
    pub matrix: Option<[f64; 16]>,
    pub translation: Option<[f64; 3]>,
    /// A quaternion: x, y, z, w.
    pub rotation: Option<[f64; 4]>,
    pub scale: Option<[f64; 3]>,
}

impl TestNode {
    /// A node of a name, its mesh and children.
    pub fn named(name: &str, mesh: Option<usize>, children: &[usize]) -> Self {
        Self {
            name: Some(name.to_owned()),
            mesh,
            children: children.to_vec(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TestMesh {
    pub name: Option<String>,
    pub primitives: Vec<TestPrimitive>,
}

#[derive(Debug, Clone, Default)]
pub struct TestPrimitive {
    pub positions: Vec<[f32; 3]>,
    /// Without them the positions are the corners in order.
    pub indices: Option<Vec<u32>>,
    /// The indices as 16-bit numbers instead of 32-bit.
    pub short_indices: bool,
    /// 4 triangles (the default), 5 a strip, 6 a fan, 0 to 3 points and
    /// lines.
    pub mode: Option<u32>,
    pub material: Option<usize>,
    /// Interleaved with this many bytes between positions (a stride).
    pub padding: usize,
    /// A morph target (the same positions), which the reader notes.
    pub target: bool,
    /// Texture coordinates (`TEXCOORD_0`, glTF's: v down the image), one
    /// per position.
    pub uvs: Option<Vec<[f32; 2]>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestMaterial {
    pub name: Option<String>,
    /// Linear red, green, blue, alpha.
    pub base_color: [f64; 4],
    pub metallic: f64,
    pub roughness: f64,
    /// An image of [`TestScene::images`] as the base colour.
    pub image: Option<usize>,
    /// An `alphaMode` of BLEND.
    pub blend: bool,
    /// An image of [`TestScene::images`] as the metalness (blue) and
    /// roughness (green), and as the occlusion (red).
    pub metallic_roughness: Option<usize>,
    pub occlusion: Option<usize>,
    /// The base colour image's `KHR_texture_transform` as it is written.
    pub texture_transform: Option<Value>,
}

impl Default for TestMaterial {
    fn default() -> Self {
        Self {
            name: None,
            base_color: [1.0; 4],
            metallic: 0.0,
            roughness: 0.5,
            image: None,
            blend: false,
            metallic_roughness: None,
            occlusion: None,
            texture_transform: None,
        }
    }
}

/// Where the buffer and the images go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    /// A `.glb` file: one binary chunk.
    Glb,
    /// A `.gltf` file with the buffer and images as data URIs.
    Embedded,
    /// A `.gltf` file with the buffer in a file of this name and the
    /// images as `image<n>.png` beside it.
    External(String),
}

/// A box of `size` (its corner at the origin) as 8 corners and 12
/// triangles, counter-clockwise seen from outside.
pub fn box_primitive(size: [f32; 3], material: Option<usize>) -> TestPrimitive {
    let [x, y, z] = size;
    let positions = vec![
        [0.0, 0.0, 0.0],
        [x, 0.0, 0.0],
        [x, y, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, z],
        [x, 0.0, z],
        [x, y, z],
        [0.0, y, z],
    ];
    let indices = vec![
        0, 2, 1, 0, 3, 2, // bottom
        4, 5, 6, 4, 6, 7, // top
        0, 1, 5, 0, 5, 4, // front
        1, 2, 6, 1, 6, 5, // right
        2, 3, 7, 2, 7, 6, // back
        3, 0, 4, 3, 4, 7, // left
    ];
    TestPrimitive {
        positions,
        indices: Some(indices),
        material,
        ..TestPrimitive::default()
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A 2 x 2 RGB PNG image (a checker of two colours), its pixels in a
/// stored deflate block.
pub fn png() -> Vec<u8> {
    let chunk = |out: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend(&body);
        out.extend(crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend(2u32.to_be_bytes());
    header.extend(2u32.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    let (a, b) = ([200u8, 60, 40], [240u8, 230, 200]);
    let raw: Vec<u8> = [[0].as_slice(), &a, &b, &[0], &b, &a].concat();
    let adler = {
        let (mut s1, mut s2) = (1u32, 0u32);
        for &byte in &raw {
            s1 = (s1 + u32::from(byte)) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        (s2 << 16) | s1
    };
    let length = raw.len() as u16;
    let mut zlib = vec![0x78, 0x01, 1];
    zlib.extend(length.to_le_bytes());
    zlib.extend((!length).to_le_bytes());
    zlib.extend(&raw);
    zlib.extend(adler.to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The buffer as it is built: bytes, views and accessors.
#[derive(Default)]
struct Built {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Built {
    fn view(&mut self, bytes: &[u8], stride: Option<usize>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut view =
            json!({"buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len()});
        if let Some(stride) = stride {
            view["byteStride"] = json!(stride);
        }
        self.bin.extend_from_slice(bytes);
        self.views.push(view);
        self.views.len() - 1
    }

    fn positions(&mut self, p: &TestPrimitive) -> usize {
        let element = 12 + p.padding;
        let mut bytes = Vec::with_capacity(p.positions.len() * element);
        let mut low = [f32::MAX; 3];
        let mut high = [f32::MIN; 3];
        for point in &p.positions {
            for (k, v) in point.iter().enumerate() {
                bytes.extend(v.to_le_bytes());
                low[k] = low[k].min(*v);
                high[k] = high[k].max(*v);
            }
            bytes.extend(std::iter::repeat_n(0u8, p.padding));
        }
        let view = self.view(&bytes, (p.padding > 0).then_some(element));
        self.accessors.push(json!({
            "bufferView": view, "componentType": 5126, "count": p.positions.len(),
            "type": "VEC3", "min": low, "max": high,
        }));
        self.accessors.len() - 1
    }

    fn uvs(&mut self, uvs: &[[f32; 2]]) -> usize {
        let mut bytes = Vec::with_capacity(uvs.len() * 8);
        for uv in uvs {
            for v in uv {
                bytes.extend(v.to_le_bytes());
            }
        }
        let view = self.view(&bytes, None);
        self.accessors.push(json!({
            "bufferView": view, "componentType": 5126, "count": uvs.len(), "type": "VEC2",
        }));
        self.accessors.len() - 1
    }

    fn indices(&mut self, indices: &[u32], short: bool) -> usize {
        let mut bytes = Vec::new();
        for i in indices {
            if short {
                bytes.extend((*i as u16).to_le_bytes());
            } else {
                bytes.extend(i.to_le_bytes());
            }
        }
        let view = self.view(&bytes, None);
        self.accessors.push(json!({
            "bufferView": view, "componentType": if short { 5123 } else { 5125 },
            "count": indices.len(), "type": "SCALAR",
        }));
        self.accessors.len() - 1
    }
}

fn encode(bytes: &[u8]) -> String {
    crate::base64::encode(bytes)
}

/// The scene as a file of `form`: its bytes, and the other files it refers
/// to (name, bytes) for [`Form::External`].
pub fn write(scene: &TestScene, form: &Form) -> (Vec<u8>, Vec<(String, Vec<u8>)>) {
    let mut built = Built::default();
    let mut meshes = Vec::new();
    for m in &scene.meshes {
        let mut primitives = Vec::new();
        for p in &m.primitives {
            let position = built.positions(p);
            let mut primitive = json!({"attributes": {"POSITION": position}});
            if let Some(uvs) = &p.uvs {
                primitive["attributes"]["TEXCOORD_0"] = json!(built.uvs(uvs));
            }
            if let Some(indices) = &p.indices {
                primitive["indices"] = json!(built.indices(indices, p.short_indices));
            }
            if let Some(mode) = p.mode {
                primitive["mode"] = json!(mode);
            }
            if let Some(material) = p.material {
                primitive["material"] = json!(material);
            }
            if p.target {
                primitive["targets"] = json!([{"POSITION": position}]);
            }
            primitives.push(primitive);
        }
        let mut mesh = json!({"primitives": primitives});
        if let Some(name) = &m.name {
            mesh["name"] = json!(name);
        }
        meshes.push(mesh);
    }
    let mut files = Vec::new();
    let mut images = Vec::new();
    for (i, bytes) in scene.images.iter().enumerate() {
        let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else {
            "image/png"
        };
        images.push(match form {
            Form::Glb => json!({"bufferView": built.view(bytes, None), "mimeType": mime}),
            Form::Embedded => json!({"uri": format!("data:{mime};base64,{}", encode(bytes))}),
            Form::External(_) => {
                let name = format!("image{}.png", i + 1);
                files.push((name.clone(), bytes.clone()));
                json!({"uri": name})
            }
        });
    }
    let materials: Vec<Value> = scene
        .materials
        .iter()
        .map(|m| {
            let mut pbr = json!({
                "baseColorFactor": m.base_color,
                "metallicFactor": m.metallic,
                "roughnessFactor": m.roughness,
            });
            if let Some(image) = m.image {
                pbr["baseColorTexture"] = json!({"index": image});
                if let Some(transform) = &m.texture_transform {
                    pbr["baseColorTexture"]["extensions"] =
                        json!({"KHR_texture_transform": transform});
                }
            }
            if let Some(image) = m.metallic_roughness {
                pbr["metallicRoughnessTexture"] = json!({"index": image});
            }
            let mut out = json!({"pbrMetallicRoughness": pbr});
            if let Some(image) = m.occlusion {
                out["occlusionTexture"] = json!({"index": image});
            }
            if let Some(name) = &m.name {
                out["name"] = json!(name);
            }
            if m.blend {
                out["alphaMode"] = json!("BLEND");
            }
            out
        })
        .collect();
    let nodes: Vec<Value> = scene
        .nodes
        .iter()
        .map(|n| {
            let mut out = json!({});
            if let Some(name) = &n.name {
                out["name"] = json!(name);
            }
            if let Some(mesh) = n.mesh {
                out["mesh"] = json!(mesh);
            }
            if !n.children.is_empty() {
                out["children"] = json!(n.children);
            }
            if let Some(m) = n.matrix {
                out["matrix"] = json!(m);
            }
            if let Some(t) = n.translation {
                out["translation"] = json!(t);
            }
            if let Some(r) = n.rotation {
                out["rotation"] = json!(r);
            }
            if let Some(s) = n.scale {
                out["scale"] = json!(s);
            }
            out
        })
        .collect();
    let mut root = json!({
        "asset": {"version": "2.0", "generator": "mitcad-gltf test writer"},
        "scene": 0,
        "scenes": [{"nodes": scene.roots}],
        "nodes": nodes,
    });
    let mut put = |key: &str, list: Vec<Value>| {
        if !list.is_empty() {
            root[key] = Value::Array(list);
        }
    };
    put("meshes", meshes);
    put("materials", materials);
    put(
        "textures",
        (0..scene.images.len())
            .map(|i| json!({"source": i}))
            .collect(),
    );
    put("images", images);
    put("accessors", built.accessors);
    put("bufferViews", built.views);
    if !built.bin.is_empty() {
        let mut buffer = json!({"byteLength": built.bin.len()});
        match form {
            Form::Glb => {}
            Form::Embedded => {
                buffer["uri"] = json!(format!(
                    "data:application/octet-stream;base64,{}",
                    encode(&built.bin)
                ));
            }
            Form::External(name) => {
                buffer["uri"] = json!(name);
                files.push((name.clone(), built.bin.clone()));
            }
        }
        root["buffers"] = json!([buffer]);
    }
    for (key, value) in &scene.extra {
        root[key] = value.clone();
    }
    let text = serde_json::to_string(&root).expect("the JSON serializes");
    if *form != Form::Glb {
        return (text.into_bytes(), files);
    }
    let mut json_chunk = text.into_bytes();
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(b' ');
    }
    let mut bin = built.bin;
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + json_chunk.len() + if bin.is_empty() { 0 } else { 8 + bin.len() };
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json_chunk.len() as u32).to_le_bytes());
    out.extend(super::CHUNK_JSON.to_le_bytes());
    out.extend(json_chunk);
    if !bin.is_empty() {
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(super::CHUNK_BIN.to_le_bytes());
        out.extend(bin);
    }
    (out, files)
}

/// A textured cube 1 m a side, `Crate` (mitcad#1862): each of its six
/// sides over the whole image (`TEXCOORD_0`), its own four corners, so
/// that corners at the same point differ in their texture coordinates;
/// its material `Wood` with image 0 as its base colour and image 1 as its
/// metalness and roughness and its occlusion.
pub fn textured_cube() -> TestScene {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    // Each side's four corners, counter-clockwise seen from outside.
    let sides: [[[f32; 3]; 4]; 6] = [
        [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
        [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
        [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
        [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
        [[1., 1., 0.], [0., 1., 0.], [0., 1., 1.], [1., 1., 1.]],
        [[0., 1., 0.], [0., 0., 0.], [0., 0., 1.], [0., 1., 1.]],
    ];
    for side in sides {
        let first = positions.len() as u32;
        positions.extend(side);
        uvs.extend([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    TestScene {
        nodes: vec![TestNode::named("Crate", Some(0), &[])],
        roots: vec![0],
        meshes: vec![TestMesh {
            name: Some("crate".into()),
            primitives: vec![TestPrimitive {
                positions,
                indices: Some(indices),
                material: Some(0),
                uvs: Some(uvs),
                ..TestPrimitive::default()
            }],
        }],
        materials: vec![TestMaterial {
            name: Some("Wood".into()),
            image: Some(0),
            metallic_roughness: Some(1),
            occlusion: Some(1),
            ..TestMaterial::default()
        }],
        images: vec![png(), png()],
        ..TestScene::default()
    }
}

/// The import's test scene: a root node holding two copies of one
/// component (a red box), a textured box of its own, and a box scaled
/// twice its size (which no occurrence can place).
///
/// - node 0 `Assembly`: children 1, 2, 4, 5 (no mesh);
/// - nodes 1 and 2 `Bracket`: translations x 0.1 m and 0.3 m, each holding
///   a copy of node 3 `Plate` (mesh 0, the red box 0.02 x 0.01 x 0.005 m)
///   — node 2's child is node 6, a second node of the same plate;
/// - node 4 `Label`: mesh 1, the textured box, at z 0.05 m;
/// - node 5 `Scaled`: scale 2, holding node 7 `Pin` (mesh 0).
pub fn assembly() -> TestScene {
    let plate = TestMesh {
        name: Some("plate".into()),
        primitives: vec![box_primitive([0.02, 0.01, 0.005], Some(0))],
    };
    let label = TestMesh {
        name: Some("label".into()),
        primitives: vec![box_primitive([0.01, 0.01, 0.001], Some(1))],
    };
    let at = |name: &str, children: &[usize], t: [f64; 3]| TestNode {
        translation: Some(t),
        ..TestNode::named(name, None, children)
    };
    TestScene {
        nodes: vec![
            TestNode::named("Assembly", None, &[1, 2, 4, 5]),
            at("Bracket", &[3], [0.1, 0.0, 0.0]),
            at("Bracket", &[6], [0.3, 0.0, 0.0]),
            TestNode::named("Plate", Some(0), &[]),
            TestNode {
                translation: Some([0.0, 0.0, 0.05]),
                ..TestNode::named("Label", Some(1), &[])
            },
            TestNode {
                scale: Some([2.0, 2.0, 2.0]),
                ..TestNode::named("Scaled", None, &[7])
            },
            TestNode::named("Plate", Some(0), &[]),
            TestNode::named("Pin", Some(0), &[]),
        ],
        roots: vec![0],
        meshes: vec![plate, label],
        materials: vec![
            TestMaterial {
                name: Some("Red".into()),
                base_color: [0.8, 0.05, 0.05, 1.0],
                metallic: 0.0,
                roughness: 0.4,
                ..TestMaterial::default()
            },
            TestMaterial {
                name: Some("Label".into()),
                image: Some(0),
                ..TestMaterial::default()
            },
        ],
        images: vec![png()],
        extra: Map::new(),
    }
}
