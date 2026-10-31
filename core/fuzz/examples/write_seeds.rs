// SPDX-License-Identifier: MIT
//! Writes the seed corpora of the fuzz targets (mitcad#939): small
//! synthetic inputs made by the readers' own writers and the crates' test
//! builders (`testfile`, `testdata`, `test_support`), never model files.
//! Each target's seeds go to `<folder>/<target>/seed-<name>`; the folder
//! is `corpus/` beside this crate's `Cargo.toml` (the corpus cargo-fuzz
//! reads and adds to, git-ignored) unless given:
//!
//!   cargo run --manifest-path fuzz/Cargo.toml --example write_seeds [-- <folder>]

use std::path::{Path, PathBuf};

use mitcad_asm::testdata as asm_data;
use mitcad_model::Document;
use mitcad_model::test_support::{block, corner};
use mitcad_model::testing::MockKernel;
use mitcad_rvt::RvtFile;
use mitcad_zip::{Method, Writer, stored_zip};

type Seeds = Vec<(String, Vec<u8>)>;

fn seed(name: &str, bytes: impl Into<Vec<u8>>) -> (String, Vec<u8>) {
    (name.to_owned(), bytes.into())
}

/// A zip of deflated entries.
fn deflated_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = Writer::new();
    for (name, data) in files {
        writer
            .add(name, data, Method::Deflate)
            .expect("a small archive");
    }
    writer.finish().expect("a small archive")
}

fn zip_seeds() -> Seeds {
    let text = b"<Document><Object name=\"Box\"/></Document>".repeat(8);
    vec![
        seed(
            "stored",
            stored_zip(&[
                ("a.txt", b"hello"),
                ("dir/", b""),
                ("dir/b.bin", &[0, 1, 2, 3]),
            ]),
        ),
        seed(
            "deflated",
            deflated_zip(&[("Document.xml", &text[..]), ("empty", b"")]),
        ),
        seed("empty", stored_zip(&[])),
    ]
}

fn cfb_seeds() -> Seeds {
    let big: Vec<u8> = (0..6000u32).map(|i| (i * 7 % 251) as u8).collect();
    let mut seeds = Vec::new();
    for version in [3u16, 4] {
        let mut writer = mitcad_cfb::Writer::new();
        writer
            .version(version)
            .stream("Top", b"small stream")
            .stream("Store/Inner/Big", &big)
            .stream("Store/Tiny", b"x")
            .stream("Store/Empty", b"");
        seeds.push(seed(&format!("version{version}"), writer.finish()));
    }
    seeds.push(seed("ipt-part", mitcad_ipt::testdata::test_part()));
    seeds.push(seed("rvt-family", mitcad_rvt::testfile::family()));
    seeds
}

/// DXF text from group code and value pairs.
fn dxf(pairs: &[(i32, &str)]) -> String {
    let mut out = String::new();
    for (code, value) in pairs {
        out.push_str(&format!("{code}\n{value}\n"));
    }
    out
}

fn dxf_seeds() -> Seeds {
    let header = [
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$INSUNITS"),
        (70, "4"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LAYER"),
        (0, "LAYER"),
        (2, "Holes"),
        (62, "1"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
    ];
    let blocks = [
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (8, "0"),
        (2, "BOLT"),
        (10, "0"),
        (20, "0"),
        (0, "CIRCLE"),
        (8, "Holes"),
        (10, "0"),
        (20, "0"),
        (40, "2"),
        (0, "ENDBLK"),
        (0, "ENDSEC"),
    ];
    let entities = [
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (11, "10"),
        (21, "5"),
        (0, "ARC"),
        (8, "0"),
        (10, "5"),
        (20, "5"),
        (40, "3"),
        (50, "0"),
        (51, "90"),
        (0, "LWPOLYLINE"),
        (8, "0"),
        (90, "3"),
        (70, "1"),
        (10, "0"),
        (20, "0"),
        (42, "0.5"),
        (10, "10"),
        (20, "0"),
        (10, "10"),
        (20, "10"),
        (0, "ELLIPSE"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (11, "4"),
        (21, "0"),
        (40, "0.5"),
        (41, "0"),
        (42, "6.283185307179586"),
        (0, "SPLINE"),
        (8, "0"),
        (70, "8"),
        (71, "3"),
        (72, "0"),
        (73, "0"),
        (74, "4"),
        (11, "0"),
        (21, "0"),
        (11, "1"),
        (21, "2"),
        (11, "3"),
        (21, "1"),
        (11, "4"),
        (21, "3"),
        (0, "TEXT"),
        (8, "0"),
        (10, "1"),
        (20, "1"),
        (40, "2.5"),
        (1, "Label"),
        (0, "INSERT"),
        (8, "0"),
        (2, "BOLT"),
        (10, "20"),
        (20, "20"),
        (50, "30"),
        (70, "3"),
        (71, "2"),
        (44, "10"),
        (45, "10"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ];
    let full: Vec<(i32, &str)> = header
        .iter()
        .chain(&blocks)
        .chain(&entities)
        .copied()
        .collect();
    let text = dxf(&full);
    let mut seeds = vec![seed("entities", dxf(&entities)), seed("full", text.clone())];
    if let Ok(drawing) = mitcad_dxf::read_str(&text) {
        for version in [mitcad_dxf::DxfVersion::R12, mitcad_dxf::DxfVersion::R2000] {
            let options = mitcad_dxf::WriteOptions {
                version,
                ..mitcad_dxf::WriteOptions::default()
            };
            if let Ok(written) = mitcad_dxf::write_string(&drawing, &options) {
                seeds.push(seed(&format!("written-{version:?}"), written));
            }
        }
    }
    seeds
}

/// A cube of side `size`, its triangles facing outwards.
fn cube(size: f64) -> mitcad_3mf::Mesh {
    let vertices = (0..8)
        .map(|i| {
            [
                size * f64::from(i & 1),
                size * f64::from((i >> 1) & 1),
                size * f64::from((i >> 2) & 1),
            ]
        })
        .collect();
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let triangles = quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect();
    mitcad_3mf::Mesh {
        vertices,
        triangles,
    }
}

fn seeds_3mf() -> Seeds {
    let mut moved = mitcad_3mf::Placement::IDENTITY;
    moved.translation = [20.0, 0.0, 0.0];
    let part = |name: &str, color, placements| mitcad_3mf::Part {
        name: name.to_owned(),
        color,
        mesh: cube(10.0),
        placements,
        metadata: Vec::new(),
    };
    let one = mitcad_3mf::Model {
        name: "Seed".to_owned(),
        parts: vec![
            part(
                "Block",
                Some([0.8, 0.2, 0.2]),
                vec![mitcad_3mf::Placement::IDENTITY],
            ),
            part("Pin", None, vec![mitcad_3mf::Placement::IDENTITY, moved]),
        ],
        application: "MitCad".to_owned(),
        files: vec![(
            "Metadata/project_settings.config".to_owned(),
            b"{\"layer_height\": \"0.2\"}".to_vec(),
        )],
        ..mitcad_3mf::Model::default()
    };
    let mut several = one.clone();
    several.objects = vec![
        mitcad_3mf::BuildObject {
            name: "First".to_owned(),
            components: vec![(0, 0)],
        },
        mitcad_3mf::BuildObject {
            name: "Second".to_owned(),
            components: vec![(1, 0), (1, 1)],
        },
    ];
    let mut inches = one.clone();
    inches.unit = mitcad_3mf::Unit::Inch;
    let mut seeds = Vec::new();
    for (name, model) in [("one", one), ("several", several), ("inches", inches)] {
        if let Ok(bytes) = mitcad_3mf::write(&model) {
            seeds.push(seed(name, bytes));
        }
    }
    seeds
}

/// The version 1 example of the design file's documentation.
const DESIGN_V1: &str = r#"{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0, "comment": "Sketch1 width" },
    { "name": "d2", "value": 40.0, "comment": "Sketch1 height" },
    { "name": "d3", "value": 20.0, "comment": "Extrude1 distance" },
    { "name": "d4", "value": 20.0, "comment": "Sketch2 width" },
    { "name": "d5", "value": 10.0, "comment": "Sketch2 height" },
    { "name": "d6", "value": 10.0, "comment": "Sketch2 diameter" },
    { "name": "d7", "value": 30.0, "comment": "Extrude2 distance" },
    { "name": "d8", "value": 30.0, "comment": "Extrude3 distance" },
    { "name": "d9", "value": 2.0, "comment": "Fillet1 radius" }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" }
    ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" },
    { "type": "sketch", "name": "Sketch2", "shapes": [
      { "type": "rectangle", "corner": [10.0, 10.0], "width": "d4", "height": "d5" },
      { "type": "circle", "center": [45.0, 20.0], "diameter": "d6" }
    ] },
    { "type": "extrude", "name": "Extrude2", "sketch": "Sketch2", "profile": 0,
      "distance": "d7", "operation": "join", "body": "Extrude1" },
    { "type": "extrude", "name": "Extrude3", "sketch": "Sketch2", "profile": 1,
      "distance": "d8", "operation": "cut", "body": "Extrude1" },
    { "type": "fillet", "name": "Fillet1", "body": "Extrude1", "edges": [
      { "extrude": "Extrude1", "role": "side", "index": 1 },
      { "extrude": "Extrude2", "role": "side", "index": 1 }
    ], "radius": "d9" }
  ]
}"#;

fn design_seeds() -> Seeds {
    let empty = Document::new(MockKernel::default()).to_json();
    let plain = block();
    let plain_json = plain.doc.to_json();
    let mut boss = block();
    boss.join_boss();
    let edge = corner(boss.extrude, 1, 0);
    boss.fillet(&[&edge], 2.0);
    let mut cut = block();
    let body = cut.body;
    cut.circle_extrude("cut", &[body]);
    vec![
        seed("empty", empty),
        seed("block", plain_json),
        seed("boss-and-fillet", boss.doc.to_json()),
        seed("hole", cut.doc.to_json()),
        seed("version1", DESIGN_V1),
    ]
}

fn settings_seeds() -> Seeds {
    vec![
        seed(
            "shared",
            r#"{"format": "mitcad-project", "version": 1,
  "edit_locks": {"enabled": true, "idle_minutes": 15, "poll_seconds": 30},
  "live_updates": {"broker": "mqtts://broker.example.com:8883", "prefix": "team/mitcad"},
  "web_viewer": {"enabled": false}}"#,
        ),
        seed(
            "local",
            r#"{"live": {"mode": "broker", "broker": "mqtt://[::1]:1883", "prefix": "a"},
  "sync": {"send_at_once": false, "check_minutes": 30}, "name": "Someone"}"#,
        ),
    ]
}

/// The synthetic `.rvt`/`.rfa` files, in the order `rvt_walk` numbers
/// their schemas.
fn rvt_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("project", mitcad_rvt::testfile::project()),
        ("family", mitcad_rvt::testfile::family()),
        ("house", mitcad_rvt::testfile::building::house()),
    ]
}

fn rvt_file_seeds() -> Seeds {
    let mut seeds: Seeds = rvt_files()
        .into_iter()
        .map(|(name, bytes)| seed(name, bytes))
        .collect();
    seeds.push(seed("building", mitcad_rvt::testfile::building::project()));
    seeds
}

fn rvt_schema_seeds() -> Seeds {
    rvt_files()
        .into_iter()
        .filter_map(|(name, bytes)| {
            let file = RvtFile::parse(bytes).ok()?;
            Some(seed(name, file.inflated("/Formats/Latest").ok()?))
        })
        .collect()
}

/// The element records of the synthetic files' partitions, each after
/// the byte that picks its file's schema in `rvt_walk`.
fn rvt_walk_seeds() -> Seeds {
    let mut seeds = Vec::new();
    for (index, (name, bytes)) in rvt_files().into_iter().enumerate() {
        let Ok(file) = RvtFile::parse(bytes) else {
            continue;
        };
        let Ok(schema) = file.schema() else {
            continue;
        };
        let Ok(partitions) = file.partitions(&schema) else {
            continue;
        };
        for partition in &partitions {
            for (i, slot) in partition
                .headers
                .iter()
                .chain(&partition.objects)
                .enumerate()
            {
                let mut record = vec![index as u8];
                record.extend_from_slice(partition.bytes(slot));
                seeds.push(seed(&format!("{name}-{}-{i}", partition.number), record));
            }
        }
    }
    seeds
}

fn asm_seeds() -> Seeds {
    vec![
        seed("cube", asm_data::cube_blob()),
        seed("named-cube", asm_data::named_cube_blob()),
        seed("cube-history", asm_data::cube_with_history_blob()),
        seed("cube-without-points", asm_data::cube_without_points_blob()),
        seed("cylinder", asm_data::cylinder_blob()),
        seed("cone", asm_data::cone_blob()),
        seed("bevelled-cube", asm_data::bevelled_cube_history_blob(1.0)),
    ]
}

fn ipt_seeds() -> Seeds {
    use mitcad_ipt::{testassembly, testdata};
    vec![
        seed("part", testdata::test_part()),
        seed("empty-part", testdata::test_empty_part()),
        seed("mesh-part", testdata::test_mesh_part()),
        seed("hidden-body", testdata::test_part_with_hidden_body()),
        seed("design", testdata::test_part_with_design()),
        seed(
            "assembly",
            testassembly::assembly_file(&testassembly::top_assembly()),
        ),
        seed(
            "sub-assembly",
            testassembly::assembly_file(&testassembly::sub_assembly()),
        ),
    ]
}

fn dwg_seeds() -> Seeds {
    use mitcad_dwg::{Version, testfile};
    let mut seeds = Vec::new();
    let drawing = testfile::test_drawing();
    for version in Version::ALL {
        seeds.push(seed(
            &format!("drawing-{version:?}"),
            testfile::write(&drawing, version),
        ));
    }
    if let Some(solid) = testfile::test_case("empty-solid") {
        seeds.push(seed("empty-solid", testfile::write(&solid, Version::R2018)));
    }
    // A 2D drawing written as MitCad exports sketches.
    let text = dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (11, "10"),
        (21, "5"),
        (0, "CIRCLE"),
        (8, "0"),
        (10, "5"),
        (20, "5"),
        (40, "2"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    if let Ok(flat) = mitcad_dxf::read_str(&text) {
        let options = mitcad_dwg::write::WriteOptions {
            time: Some(0.0),
            paper: None,
        };
        if let Ok(bytes) = mitcad_dwg::write::drawing(&flat, &options) {
            seeds.push(seed("flat", bytes));
        }
    }
    seeds
}

fn ifc_seeds() -> Seeds {
    vec![
        seed("building", mitcad_ifc::testdata::BUILDING),
        seed(
            "minimal",
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
             FILE_NAME('a.ifc','',(''),(''),'','','');\nFILE_SCHEMA(('IFC2X3'));\nENDSEC;\n\
             DATA;\n#1=IFCCARTESIANPOINT((0.,0.,0.));\n#2=IFCDIRECTION((0.,0.,1.));\n\
             #3=IFCAXIS2PLACEMENT3D(#1,#2,$);\nENDSEC;\nEND-ISO-10303-21;\n",
        ),
    ]
}

const FREECAD_0_21: &str = include_str!("../../freecad/tests/data/document_0_21.xml");
const FREECAD_1_0: &str = include_str!("../../freecad/tests/data/document_1_0.xml");
const FREECAD_GUI: &str = include_str!("../../freecad/tests/data/gui_document.xml");

fn freecad_document_seeds() -> Seeds {
    vec![
        seed("document-0-21", FREECAD_0_21),
        seed("document-1-0", FREECAD_1_0),
        seed("gui-document", FREECAD_GUI),
    ]
}

fn freecad_file_seeds() -> Seeds {
    vec![
        seed(
            "file-0-21",
            deflated_zip(&[
                ("Document.xml", FREECAD_0_21.as_bytes()),
                ("GuiDocument.xml", FREECAD_GUI.as_bytes()),
            ]),
        ),
        seed(
            "file-1-0",
            stored_zip(&[("Document.xml", FREECAD_1_0.as_bytes())]),
        ),
    ]
}

/// UTF-16 text as the design streams hold names.
fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
}

fn f3d_seeds() -> Seeds {
    let mut properties = 16u32.to_le_bytes().to_vec();
    properties.extend_from_slice(b"{\"name\":\"Seed\"}\n");
    let mut stream = b"stream ".to_vec();
    stream.extend(utf16("Body1"));
    stream.extend(utf16("BREP.1.smb"));
    stream.push(0);
    let cube = asm_data::cube_blob();
    let history = asm_data::cube_with_history_blob();
    let document = deflated_zip(&[
        ("Manifest.dat", b"manifest"),
        ("Properties.dat", &properties[..]),
        ("Asset/Design1/BulkStream.dat", &stream[..]),
        ("Asset/Design1/MetaStream.dat", b"meta"),
        ("Asset/Breps.BlobParts/BREP.1.smb", &cube[..]),
        ("Asset/Breps.BlobParts/BREP.2.smbh", &history[..]),
    ]);
    let package = stored_zip(&[
        ("Manifest.json", b"{\"root\": \"inner.f3d\"}"),
        ("inner.f3d", &document[..]),
    ]);
    vec![seed("document", document), seed("package", package)]
}

/// The designs written for the `.solidcraft` reader's tests
/// (`solidcraft/tests/designs`), and a container with a STEP and a mesh
/// blob.
fn solidcraft_seeds() -> Seeds {
    let designs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../solidcraft/tests/designs");
    let mut seeds: Seeds = std::fs::read_dir(designs)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".solidcraft.json")?.to_owned();
            Some(seed(&stem, std::fs::read(entry.path()).ok()?))
        })
        .collect();
    seeds.sort();
    let mut mesh = b"SCMESH01".to_vec();
    mesh.extend(9u64.to_le_bytes());
    mesh.extend(3u64.to_le_bytes());
    for x in [0.0f64, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        mesh.extend(x.to_le_bytes());
    }
    for i in [0u32, 1, 2] {
        mesh.extend(i.to_le_bytes());
    }
    let hash = "a".repeat(64);
    let document = format!(
        r#"{{"format": "solidcraft/27", "name": "A", "params": [], "features": [
  {{"id": 1, "name": "Import1", "type": "import", "file": "a.step", "step": "", "step_blob": "blobs/{hash}.step"}},
  {{"id": 2, "name": "Mesh1", "type": "mesh_import", "file": "a.stl",
   "meshes": [{{"name": "M", "positions": [], "triangles": [], "mesh_blob": "blobs/{hash}.mesh"}}]}}]}}"#
    );
    let step = format!("blobs/{hash}.step");
    let mesh_name = format!("blobs/{hash}.mesh");
    seeds.push(seed(
        "container",
        stored_zip(&[
            ("container.json", br#"{"format":"solidcraft-container/1"}"#),
            ("document.json", document.as_bytes()),
            (&step, b"ISO-10303-21;"),
            (&mesh_name, &mesh[..]),
        ]),
    ));
    seeds
}

fn all() -> Vec<(&'static str, Seeds)> {
    let mut zip = zip_seeds();
    zip.extend(freecad_file_seeds());
    zip.extend(seeds_3mf());
    vec![
        ("zip_archive", zip),
        ("cfb_compound", cfb_seeds()),
        ("dxf_read", dxf_seeds()),
        ("package_3mf", seeds_3mf()),
        ("design_file", design_seeds()),
        ("project_settings", settings_seeds()),
        ("rvt_schema", rvt_schema_seeds()),
        ("rvt_walk", rvt_walk_seeds()),
        ("rvt_file", rvt_file_seeds()),
        ("asm_blob", asm_seeds()),
        ("ipt_file", ipt_seeds()),
        ("dwg_file", dwg_seeds()),
        ("ifc_text", ifc_seeds()),
        ("freecad_document", freecad_document_seeds()),
        ("freecad_file", freecad_file_seeds()),
        ("f3d_file", f3d_seeds()),
        ("solidcraft_file", solidcraft_seeds()),
    ]
}

fn main() -> std::io::Result<()> {
    let folder = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus"));
    for (target, seeds) in all() {
        let dir = folder.join(target);
        std::fs::create_dir_all(&dir)?;
        for (name, bytes) in &seeds {
            std::fs::write(dir.join(format!("seed-{name}")), bytes)?;
        }
        println!("{target}: {} seeds in {}", seeds.len(), dir.display());
    }
    Ok(())
}
