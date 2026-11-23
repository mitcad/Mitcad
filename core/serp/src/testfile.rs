// SPDX-License-Identifier: MIT
//! A writer of small `.serp` files for the tests: the reader's own, the
//! model's import tests (`core/model-tests/files`) and the import's test
//! file with B-reps that OCCT wrote (`mitcad-cli serp-test-file`). It
//! writes what the reader takes and the members a real file has beside
//! them; it is no general writer (MitCad does not write `.serp` files).

use serde_json::{Map, Value, json};

use crate::base64;

/// A scene to write.
#[derive(Debug, Clone)]
pub struct TestScene {
    /// The format version written (1 is bare JSON, 2 and later a zip).
    pub version: u64,
    /// The `format` member.
    pub format: String,
    /// The `units` member; left out when None.
    pub units: Option<String>,
    pub layers: Vec<TestLayer>,
    pub objects: Vec<TestObject>,
    pub blocks: Vec<TestBlock>,
    /// Members added to the scene as they are (`pointclouds`, `layouts`,
    /// `history_records`, ...).
    pub extra: Map<String, Value>,
    /// Binary members of the container by name (a point cloud's
    /// `blobs/<id>/xyz.f32`, mitcad#2240), stored as they are.
    pub members: Vec<(String, Vec<u8>)>,
}

impl Default for TestScene {
    fn default() -> Self {
        Self {
            version: 2,
            format: "serp".to_owned(),
            units: Some("mm".to_owned()),
            layers: vec![TestLayer::named("default", "Default", [0.85, 0.85, 0.85])],
            objects: Vec::new(),
            blocks: Vec::new(),
            extra: Map::new(),
            members: Vec::new(),
        }
    }
}

/// A point cloud as the writer keeps it (mitcad#2240): its entry for the
/// scene's `pointclouds` and its members, `blobs/<id>/xyz.f32` (the points
/// from `origin` as little-endian float32) and `blobs/<id>/rgb.u8`.
pub fn cloud(
    id: &str,
    name: &str,
    layer: &str,
    origin: Option<[f64; 3]>,
    points: &[[f32; 3]],
    colors: Option<&[[u8; 3]]>,
) -> (Value, Vec<(String, Vec<u8>)>) {
    let xyz = format!("blobs/{id}/xyz.f32");
    let mut blobs = json!({"xyz": xyz});
    let mut members = vec![(
        xyz,
        points
            .iter()
            .flatten()
            .flat_map(|c| c.to_le_bytes())
            .collect(),
    )];
    if let Some(colors) = colors {
        let rgb = format!("blobs/{id}/rgb.u8");
        blobs["rgb"] = json!(rgb);
        members.push((rgb, colors.iter().flatten().copied().collect()));
    }
    let mut entry = json!({
        "id": id, "name": name, "layer": layer, "visible": true,
        "count": points.len(), "blobs": blobs,
    });
    if let Some(origin) = origin {
        entry["origin"] = json!(origin);
    }
    (entry, members)
}

#[derive(Debug, Clone)]
pub struct TestLayer {
    pub id: String,
    pub name: String,
    pub color: [f64; 3],
    pub visible: bool,
    pub parent: Option<String>,
}

impl TestLayer {
    /// A shown layer at the top.
    pub fn named(id: &str, name: &str, color: [f64; 3]) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            color,
            visible: true,
            parent: None,
        }
    }
}

/// A mesh as the writer keeps it: float32 vertices, uint32 triangles and
/// an origin they are measured from.
#[derive(Debug, Clone, Default)]
pub struct TestMesh {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub origin: Option<[f64; 3]>,
}

impl TestMesh {
    /// A closed box of 12 triangles from `low` to `high`, facing out.
    pub fn cube(low: [f32; 3], high: [f32; 3]) -> Self {
        let corner = |i: usize| {
            [
                if i & 1 == 0 { low[0] } else { high[0] },
                if i & 2 == 0 { low[1] } else { high[1] },
                if i & 4 == 0 { low[2] } else { high[2] },
            ]
        };
        Self {
            vertices: (0..8).map(corner).collect(),
            triangles: vec![
                [0, 2, 1],
                [1, 2, 3],
                [4, 5, 6],
                [5, 7, 6],
                [0, 1, 4],
                [1, 5, 4],
                [2, 6, 3],
                [3, 6, 7],
                [0, 4, 2],
                [2, 4, 6],
                [1, 3, 5],
                [3, 7, 5],
            ],
            origin: None,
        }
    }

    fn vertex_bytes(&self) -> Vec<u8> {
        self.vertices
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }

    fn triangle_bytes(&self) -> Vec<u8> {
        self.triangles
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }

    /// The tagged bytes a block's mesh shape is kept as.
    pub fn tagged(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"SMSH");
        out.push(if self.origin.is_some() { 2 } else { 1 });
        out.extend_from_slice(&(self.vertices.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.triangles.len() as u32).to_le_bytes());
        if let Some(o) = self.origin {
            for v in o {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend(self.vertex_bytes());
        out.extend(self.triangle_bytes());
        out
    }

    fn json(&self) -> Value {
        let mut out = json!({
            "v": base64::encode(&self.vertex_bytes()),
            "t": base64::encode(&self.triangle_bytes()),
        });
        if let Some(o) = self.origin {
            out["o"] = json!(o);
        }
        out
    }
}

/// What an object holds: B-rep data (or any bytes, such as a tagged
/// text), or a mesh.
#[derive(Debug, Clone)]
pub enum TestShape {
    Bytes(Vec<u8>),
    Mesh(TestMesh),
}

#[derive(Debug, Clone)]
pub struct TestObject {
    pub name: Option<String>,
    pub layer: String,
    pub visible: bool,
    pub color: Option<[f64; 3]>,
    pub block: Option<String>,
    pub shape: TestShape,
}

impl TestObject {
    /// A shown object of its layer's colour.
    pub fn new(name: &str, layer: &str, shape: TestShape) -> Self {
        Self {
            name: Some(name.to_owned()),
            layer: layer.to_owned(),
            visible: true,
            color: None,
            block: None,
            shape,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestBlock {
    pub id: String,
    pub name: String,
    pub shapes: Vec<Vec<u8>>,
}

/// The scene as JSON.
pub fn scene_json(scene: &TestScene) -> Value {
    let layers: Vec<Value> = scene
        .layers
        .iter()
        .map(|l| {
            json!({
                "id": l.id, "name": l.name, "color": l.color, "visible": l.visible,
                "locked": false, "linetype": "Continuous", "parent": l.parent,
            })
        })
        .collect();
    let objects: Vec<Value> = scene
        .objects
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let (brep, mesh) = match &o.shape {
                TestShape::Bytes(data) => (json!(base64::encode(data)), Value::Null),
                TestShape::Mesh(m) => (Value::Null, m.json()),
            };
            json!({
                "id": format!("o{i}"), "name": o.name, "layer": o.layer, "visible": o.visible,
                "color": o.color, "material": null, "locked": false, "group": null,
                "block": o.block, "brep": brep, "mesh": mesh,
            })
        })
        .collect();
    let blocks: Map<String, Value> = scene
        .blocks
        .iter()
        .map(|b| {
            let shapes: Vec<String> = b.shapes.iter().map(|s| base64::encode(s)).collect();
            (b.id.clone(), json!({"name": b.name, "shapes": shapes}))
        })
        .collect();
    let mut doc = json!({
        "format": scene.format,
        "version": scene.version,
        "named_views": {},
        "block_defs": blocks,
        "layers": layers,
        "current_layer": scene.layers.first().map(|l| l.id.clone()),
        "objects": objects,
    });
    if let Some(units) = &scene.units {
        doc["units"] = json!(units);
    }
    for (key, value) in &scene.extra {
        doc[key] = value.clone();
    }
    doc
}

/// The file's bytes: bare JSON for version 1, else a zip of
/// `meta.json`, `document.json` (deflated) and a thumbnail.
pub fn write(scene: &TestScene) -> Vec<u8> {
    let doc = scene_json(scene).to_string();
    if scene.version <= 1 {
        return doc.into_bytes();
    }
    let meta = json!({
        "format": scene.format,
        "version": scene.version,
        "saved": "2026-01-01T00:00:00",
        "objects": scene.objects.len(),
        "layouts": 0,
    })
    .to_string();
    let mut writer = mitcad_zip::Writer::new();
    writer
        .add("meta.json", meta.as_bytes(), mitcad_zip::Method::Deflate)
        .expect("a small test archive");
    writer
        .add("document.json", doc.as_bytes(), mitcad_zip::Method::Deflate)
        .expect("a small test archive");
    writer
        .add(
            "thumbnail.png",
            b"\x89PNG\r\n\x1a\n",
            mitcad_zip::Method::Stored,
        )
        .expect("a small test archive");
    for (name, bytes) in &scene.members {
        writer
            .add(name, bytes, mitcad_zip::Method::Stored)
            .expect("a small test archive");
    }
    writer.finish().expect("a small test archive")
}

/// The scene of the import's tests around the given B-rep data (`box`,
/// a solid; `face`, a surface of one face; `part`, the shape of a block,
/// placed twice by `part_moved` and `part_turned`, its copies): on two
/// layers (Solids, red, with its child Bolts; Surfaces), the box in its
/// layer's colour, the face blue, a mesh cube on Surfaces, a hidden
/// object, a text, and a point cloud, a layout and a history record of the
/// file that the import leaves out. Lengths in centimetres.
pub fn sample(
    box_data: &[u8],
    face: &[u8],
    part: &[u8],
    part_moved: &[u8],
    part_turned: &[u8],
) -> TestScene {
    let solids = TestLayer::named("l1", "Solids", [0.8, 0.1, 0.1]);
    let mut bolts = TestLayer::named("l2", "Bolts", [0.2, 0.7, 0.2]);
    bolts.parent = Some("l1".to_owned());
    let surfaces = TestLayer::named("l3", "Surfaces", [0.5, 0.5, 0.5]);
    let mut blue = TestObject::new("Panel", "l3", TestShape::Bytes(face.to_vec()));
    blue.color = Some([0.1, 0.2, 0.9]);
    let mut hidden = TestObject::new("Spare", "l1", TestShape::Bytes(box_data.to_vec()));
    hidden.visible = false;
    let mut first = TestObject::new("Bolt 01", "l2", TestShape::Bytes(part_moved.to_vec()));
    first.block = Some("b1".to_owned());
    let mut second = TestObject::new("Bolt 02", "l2", TestShape::Bytes(part_turned.to_vec()));
    second.block = Some("b1".to_owned());
    let mut extra = Map::new();
    extra.insert(
        "pointclouds".to_owned(),
        json!([{"id": "c1", "name": "Scan", "layer": "l1", "count": 3, "blobs": {"xyz": "blobs/c1/xyz.f32"}}]),
    );
    extra.insert("layouts".to_owned(), json!([{"name": "Sheet 1"}]));
    extra.insert(
        "history_records".to_owned(),
        json!([{"command": "extrude", "inputs": ["o0"]}]),
    );
    TestScene {
        version: 6,
        units: Some("cm".to_owned()),
        layers: vec![
            TestLayer::named("default", "Default", [0.85, 0.85, 0.85]),
            solids,
            bolts,
            surfaces,
        ],
        objects: vec![
            TestObject::new("Block", "l1", TestShape::Bytes(box_data.to_vec())),
            blue,
            TestObject::new(
                "Scan mesh",
                "l3",
                TestShape::Mesh(TestMesh::cube([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])),
            ),
            hidden,
            TestObject::new("Note", "l3", TestShape::Bytes(b"STXT\x01note".to_vec())),
            first,
            second,
        ],
        blocks: vec![TestBlock {
            id: "b1".to_owned(),
            name: "Bolt".to_owned(),
            shapes: vec![part.to_vec()],
        }],
        extra,
        ..TestScene::default()
    }
}
