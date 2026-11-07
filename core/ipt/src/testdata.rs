// SPDX-License-Identifier: MIT
//! Small `.ipt` files made by Mitcad's own writers, for tests here, in the
//! bridge and in the command line and UI tests (no third-party files are
//! needed): a compound file with the segment database's streams, a B-rep
//! segment whose B-rep records hold ASM bodies written by the `.f3d`
//! reader's ASM writer (`mitcad_asm::testdata`), and the property sets the
//! import reads.

use crate::cfb::{Writer, guid_bytes};
use crate::props::{self, Value};
use crate::rse;
use crate::{
    BREP_RECORD_TYPE, MODEL_SETTINGS, PART_CLSID, PID_LENGTH_UNIT, PID_MATERIAL, PID_PART_NUMBER,
    PID_RELEASE, TRACKING_PROPERTIES,
};

/// Definitions segment records: (type, bytes).
pub type Records = Vec<([u8; 16], Vec<u8>)>;

/// What a test part holds.
pub struct TestPart<'a> {
    /// One B-rep record per ASM file.
    pub bodies: Vec<Vec<u8>>,
    pub part_number: &'a str,
    pub material: &'a str,
    /// The model settings' length unit code (11269: millimetres).
    pub length_unit: i64,
    /// The definitions segment's records (`testdesign`) and its major
    /// version; None for a segment of one record that is nothing.
    pub design: Option<(u8, Records)>,
}

/// The B-rep record of an ASM file: a 14-byte head, the file and an
/// 18-byte trailer, as in the files seen.
pub fn brep_record(asm: &[u8]) -> Vec<u8> {
    let mut r = vec![0, 0, 0, 0, 2, 0, 0x1D, 3, 0, 0, 0x2B, 0, 0, 0];
    r.extend_from_slice(asm);
    r.extend_from_slice(&[
        6, 0, 0, 0x80, 1, 0, 0, 0, 0, 0, 7, 0, 0, 0x80, 0xFF, 0xFF, 0xFF, 0xFF,
    ]);
    r
}

/// Writes the part file.
pub fn part_file(part: &TestPart) -> Vec<u8> {
    part_file_with_streams(part, &[])
}

/// Writes the part file with more streams (path, bytes).
pub fn part_file_with_streams(part: &TestPart, streams: &[(&str, Vec<u8>)]) -> Vec<u8> {
    // Another record type before the B-rep's, so that the type index
    // matters.
    let types = [[0x11; 16], BREP_RECORD_TYPE];
    let mut records = vec![(0u8, vec![1, 2, 3, 4])];
    records.extend(part.bodies.iter().map(|asm| (1u8, brep_record(asm))));
    records.push((0, vec![5; 10]));
    let (m, b) = rse::write_segment("PmBRepSegment", [0x42; 16], &types, &records);
    // The definitions, or a segment without B-rep records.
    let (m2, b2) = match &part.design {
        Some((major, records)) => {
            let mut types: Vec<[u8; 16]> = Vec::new();
            let records: Vec<(u8, Vec<u8>, Vec<u8>)> = records
                .iter()
                .map(|(t, bytes)| {
                    let k = types.iter().position(|x| x == t).unwrap_or_else(|| {
                        types.push(*t);
                        types.len() - 1
                    });
                    (k as u8, bytes.clone(), Vec::new())
                })
                .collect();
            rse::write_segment_with("PmDCSegment", [0x43; 16], *major, &types, &records)
        }
        None => rse::write_segment(
            "PmDCSegment",
            [0x43; 16],
            &[[0x12; 16]],
            &[(0, vec![0; 20])],
        ),
    };
    let tracking = props::write(
        guid_bytes(TRACKING_PROPERTIES).expect("format id"),
        &[
            (PID_PART_NUMBER, Value::Text(part.part_number.into())),
            (PID_MATERIAL, Value::Text(part.material.into())),
            (PID_RELEASE, Value::Text("Mitcad test writer".into())),
        ],
    );
    let settings = props::write(
        guid_bytes(MODEL_SETTINGS).expect("format id"),
        &[(PID_LENGTH_UNIT, Value::Int(part.length_unit))],
    );
    let mut w = Writer::new();
    w.clsid("", guid_bytes(PART_CLSID).expect("class id"))
        .stream("\u{5}TrackingProperties", &tracking)
        .stream("\u{5}ModelSettings", &settings)
        .stream("RSeStorage/RSeSegInfo", &[0; 8])
        .stream("RSeStorage/Mtestbrepsegment", &m)
        .stream("RSeStorage/Btestbrepsegment", &b)
        .stream("RSeStorage/Mtestdcsegment", &m2)
        .stream("RSeStorage/Btestdcsegment", &b2);
    for (path, bytes) in streams {
        w.stream(path, bytes);
    }
    w.finish()
}

/// An ASM file without bodies: the B-rep record of a part whose bodies
/// are meshes, or of an empty part.
pub fn empty_asm() -> Vec<u8> {
    let mut w = mitcad_asm::asm::writer::Writer::new(1, 0);
    w.record("asmheader")
        .ptr(-1)
        .int(-1)
        .str("231.6.3.65535")
        .end();
    w.finish()
}

/// A part whose body is a mesh feature's: an empty B-rep record, a mesh
/// feature in the definitions segment and a tetrahedron of 1 cm, 1.5 cm
/// and 2 cm legs at the origin in the graphics segment.
pub fn test_mesh_part() -> Vec<u8> {
    let types = [
        [0x21; 16],
        crate::mesh::VERTICES_TYPE,
        crate::mesh::TRIANGLES_TYPE,
    ];
    let (m, b) = rse::write_segment(
        "PmGraphicsSegment",
        [0x44; 16],
        &types,
        &[
            (0, vec![0; 12]),
            (
                1,
                crate::mesh::write_vertices(&[
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.5, 0.0],
                    [0.0, 0.0, 2.0],
                ]),
            ),
            (
                2,
                crate::mesh::write_triangles(&[[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]]),
            ),
        ],
    );
    part_file_with_streams(
        &TestPart {
            bodies: vec![empty_asm()],
            part_number: "MITCAD-TEST-MESH",
            material: "Generic",
            length_unit: 11269,
            design: Some((25, vec![(crate::mesh::MESH_FEATURE_TYPE, vec![0; 30])])),
        },
        &[
            ("RSeStorage/Mtestgraphicssegment", m),
            ("RSeStorage/Btestgraphicssegment", b),
        ],
    )
}

/// An empty part: a B-rep record without bodies and no features.
pub fn test_empty_part() -> Vec<u8> {
    part_file(&TestPart {
        bodies: vec![empty_asm()],
        part_number: "MITCAD-TEST-EMPTY",
        material: "Generic",
        length_unit: 11269,
        design: None,
    })
}

/// A part with two bodies (one B-rep record each): the 10 mm cube and the
/// cylinder of radius 10 mm and height 20 mm of `mitcad_asm::testdata`,
/// part number `MITCAD-TEST-1`, material `Aluminum`, inches.
pub fn test_part() -> Vec<u8> {
    part_file(&TestPart {
        bodies: vec![
            mitcad_asm::testdata::cube_blob(),
            mitcad_asm::testdata::cylinder_blob(),
        ],
        part_number: "MITCAD-TEST-1",
        material: "Aluminum",
        length_unit: 11272,
        design: None,
    })
}

/// The bodies of [`test_part`] with a result segment (`result`) that lists
/// the cube as shown and the cylinder as hidden surfaces (a surface a
/// feature consumed), millimetres.
pub fn test_part_with_hidden_body() -> Vec<u8> {
    use crate::result::{RESULT_BODIES, RESULT_SEGMENT, ResultBody, write};
    let entry = |node, visible, kind, range| ResultBody {
        node,
        features: vec![node + 1],
        visible,
        solid: kind == 2,
        kind,
        members: Vec::new(),
        range,
    };
    let bodies = [
        entry(2, true, 2, [[0.0; 3], [1.0; 3]]),
        entry(5, false, 1, [[-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]]),
    ];
    let (m, b) = rse::write_segment(
        RESULT_SEGMENT,
        [0x45; 16],
        &[[0x31; 16], RESULT_BODIES],
        &[(1, write(&bodies)), (0, vec![0; 8])],
    );
    part_file_with_streams(
        &TestPart {
            bodies: vec![
                mitcad_asm::testdata::cube_blob(),
                mitcad_asm::testdata::cylinder_blob(),
            ],
            part_number: "MITCAD-TEST-HIDDEN",
            material: "Generic",
            length_unit: 11269,
            design: None,
        },
        &[
            ("RSeStorage/Mtestresultsegment", m),
            ("RSeStorage/Btestresultsegment", b),
        ],
    )
}

/// A part with the 10 mm cube as its body and the design that makes it
/// (`testdesign::cube_design`): a parameter, a sketch and an extrusion;
/// millimetres. The B-rep record has no ASM history.
pub fn test_part_with_design() -> Vec<u8> {
    part_file(&TestPart {
        bodies: vec![mitcad_asm::testdata::cube_blob()],
        part_number: "MITCAD-TEST-2",
        material: "Steel",
        length_unit: 11269,
        design: Some((25, crate::testdesign::cube_design(25))),
    })
}

/// The designed cube with a result-list entry that keeps it hidden. Its
/// box also matches a cube that already exists in the receiving document,
/// so import tests can check that hiding is confined to imported bodies.
pub fn test_part_with_hidden_design_body() -> Vec<u8> {
    use crate::result::{RESULT_BODIES, RESULT_SEGMENT, ResultBody, write};
    let bodies = [ResultBody {
        node: 2,
        features: vec![3],
        visible: false,
        solid: true,
        kind: 2,
        members: Vec::new(),
        range: [[0.0; 3], [1.0; 3]],
    }];
    let (m, b) = rse::write_segment(
        RESULT_SEGMENT,
        [0x45; 16],
        &[[0x31; 16], RESULT_BODIES],
        &[(1, write(&bodies)), (0, vec![0; 8])],
    );
    part_file_with_streams(
        &TestPart {
            bodies: vec![mitcad_asm::testdata::cube_blob()],
            part_number: "MITCAD-TEST-HIDDEN-DESIGN",
            material: "Steel",
            length_unit: 11269,
            design: Some((25, crate::testdesign::cube_design(25))),
        },
        &[
            ("RSeStorage/Mtestresultsegment", m),
            ("RSeStorage/Btestresultsegment", b),
        ],
    )
}

/// The asset GUID of the document's appearance of
/// [`test_part_with_appearances`].
pub const BLUE_ASSET: &str = "0B1E0000-0000-4000-8000-000000000001";

/// A u32-counted UTF-16 text.
fn text16(s: &str) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// A style's colours as the records keep them (`appearance::Colors`):
/// ambient and diffuse this colour, specular white, emissive black and a
/// shininess of 50.
pub fn style_colors(diffuse: [f32; 4]) -> [u8; 68] {
    let mut out = Vec::new();
    let white = [1.0f32; 4];
    let black = [0.0, 0.0, 0.0, 1.0f32];
    for c in [diffuse, diffuse, white, black] {
        for x in c {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    out.extend_from_slice(&50.0f32.to_le_bytes());
    let mut c = [0u8; 68];
    c.copy_from_slice(&out);
    c
}

/// A rendering or material style record of the application segment
/// (`appearance`): its prefix (27 bytes from major 23 on, else 25), name,
/// long name, label and, from major 17 on, the asset's GUID, the preset
/// and the library, then its colours.
pub fn rendering_style(
    name: &str,
    guid: &str,
    preset: &str,
    library: &str,
    diffuse: [f32; 4],
    major: u8,
) -> Vec<u8> {
    let mut out = vec![0u8; if major > 22 { 27 } else { 25 }];
    out.extend(text16(name));
    out.extend(text16(""));
    out.extend_from_slice(&[0, 0]);
    out.extend(text16(&format!("1:{name}\n")));
    if major > 16 {
        for s in [guid, preset, library] {
            out.extend(text16(s));
        }
        out.extend_from_slice(&[0, 0, 2, 0]);
        out.extend_from_slice(&[0x5A; 16]);
    }
    out.extend_from_slice(&style_colors(diffuse));
    out.extend_from_slice(&[0; 8]);
    out
}

/// The record of the document's default styles: the material style's and
/// the rendering style's records (indices).
fn default_styles(material: u32, rendering: u32) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0, 1, 0];
    for r in [material, rendering] {
        out.extend_from_slice(&((r + 1) | 0x8000_0000).to_le_bytes());
    }
    out.extend_from_slice(&[0; 28 + 1 + 4 + 8]);
    out
}

/// A face of the graphics segment: its style list (a record index, or none)
/// and its key.
fn graphics_face(styles: Option<u32>, key: u32) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0, 0, 0];
    out.extend_from_slice(&0x4540_0000u32.to_le_bytes());
    out.extend_from_slice(&styles.map_or(0, |s| (s + 1) | 0x8000_0000).to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    // No edges, the visibility, the box, the key, two u32.
    out.extend_from_slice(&[2, 0, 0, 0x30, 0, 0, 0, 0]);
    out.push(0);
    for x in [0.0f64, 0.0, 0.0, 1.0, 1.0, 1.0] {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.extend_from_slice(&key.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out
}

/// A list of the graphics segment of these records (indices).
fn graphics_list(items: &[u32]) -> Vec<u8> {
    let mut out = vec![2, 0, 0, 0x30];
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    if !items.is_empty() {
        out.extend_from_slice(&(items.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x1000_0000u32.to_le_bytes());
    }
    for i in items {
        out.extend_from_slice(&((i + 1) | 0x8000_0000).to_le_bytes());
    }
    out
}

/// A primary colour style of the graphics segment (94 bytes) with a
/// style's colours.
fn primary_color(colors: [u8; 68]) -> Vec<u8> {
    let mut out = vec![2, 0, 0, 0];
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&colors);
    out.extend_from_slice(&[0, 0, 0, 0, 0, 1]);
    out
}

/// A `Protein` stream of one appearance of the document: a generic one of
/// the colour (0.41, 0.49, 0.54) with the GUID [`BLUE_ASSET`].
fn protein_stream() -> Vec<u8> {
    let schema = r#"<?xml version="1.0"?><AssetSchema><UID val="GenericSchema"/><String id="AssetLibID" val=""/><Color id="generic_diffuse" valR="1" valG="1" valB="1" valA="1"/></AssetSchema>"#;
    let str32 = |s: &str| {
        let mut out = (s.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(s.as_bytes());
        out
    };
    let mut rec = Vec::new();
    for s in ["GenericSchema", BLUE_ASSET, "Generic", "LIB"] {
        rec.extend(str32(s));
    }
    for c in [0.41f64, 0.49, 0.54, 1.0] {
        rec.extend_from_slice(&c.to_le_bytes());
    }
    let zip = mitcad_f3d_file::zip::stored_zip(&[
        ("F/Schemas/GenericSchema.xml", schema.as_bytes()),
        (
            "F/AssetData/InstanceProperties.bin",
            &mitcad_f3d_file::protein::paged_stream(&[rec]),
        ),
    ]);
    let mut out = (zip.len() as u32).to_le_bytes().to_vec();
    out.extend(zip);
    out
}

/// A part of the keyed cube (`mitcad_asm::testdata::keyed_cube_blob`, the
/// faces' keys 11 bottom, 12 top, 14 y = 0, 13 x = 1, 15 y = 1, 16 x = 0)
/// with the appearances (mitcad#1367): a `Protein` package of one asset,
/// the material style `Steel` and the rendering styles `Semi-Polished`
/// (the default, a library preset not in the package), `Blue Grey` (the
/// package's asset) and `Red`; the faces of the keys in `colored` have the
/// colours of the style `colored_style` (1 Blue Grey, 2 Red); with the
/// definitions `design`, when given.
fn test_part_with_face_colors(
    colored: &[u32],
    colored_style: usize,
    design: Option<(u8, Records)>,
) -> Vec<u8> {
    use crate::appearance::{
        DEFAULT_STYLES, GRAPHICS_FACE, MATERIAL_STYLE, PRIMARY_COLOR, RENDERING_STYLE, STYLE_LIST,
    };
    let styles = [
        ("Semi-Polished", "", "Metal-045", [0.75f32, 0.75, 0.75, 1.0]),
        ("Blue Grey", BLUE_ASSET, "", [0.25, 0.5, 0.75, 1.0]),
        ("Red", "", "Paint-001", [0.8, 0.1, 0.1, 1.0]),
    ];
    let mut app = vec![
        (0u8, default_styles(1, 2)),
        (
            1,
            rendering_style("Steel", "", "MaterialInv_029", "LIB", [0.5; 4], 25),
        ),
    ];
    for (name, guid, preset, c) in styles {
        app.push((2, rendering_style(name, guid, preset, "LIB", c, 25)));
    }
    let (am, ab) = rse::write_segment(
        crate::appearance::APP_SEGMENT,
        [0x46; 16],
        &[DEFAULT_STYLES, MATERIAL_STYLE, RENDERING_STYLE],
        &app,
    );
    // Faces 0..6, the style list 6 and the primary colour 7.
    let mut graphics: Vec<(u8, Vec<u8>)> = (11..17u32)
        .map(|key| {
            let list = colored.contains(&key).then_some(6);
            (0u8, graphics_face(list, key))
        })
        .collect();
    graphics.push((1, graphics_list(&[7])));
    graphics.push((2, primary_color(style_colors(styles[colored_style].3))));
    let (gm, gb) = rse::write_segment(
        crate::appearance::GRAPHICS_SEGMENT,
        [0x47; 16],
        &[GRAPHICS_FACE, STYLE_LIST, PRIMARY_COLOR],
        &graphics,
    );
    part_file_with_streams(
        &TestPart {
            bodies: vec![mitcad_asm::testdata::keyed_cube_blob([
                11, 12, 14, 13, 15, 16,
            ])],
            part_number: "MITCAD-TEST-APPEARANCE",
            material: "Steel",
            length_unit: 11269,
            design,
        },
        &[
            ("RSeStorage/Mtestappsegment", am),
            ("RSeStorage/Btestappsegment", ab),
            ("RSeStorage/Mtestgraphicssegment", gm),
            ("RSeStorage/Btestgraphicssegment", gb),
            ("Protein", protein_stream()),
        ],
    )
}

/// The appearance part ([`test_part_with_face_colors`]) whose top and face
/// x = 1 are blue grey.
pub fn test_part_with_appearances() -> Vec<u8> {
    test_part_with_face_colors(&[12, 13], 1, None)
}

/// The appearance part whose faces are all red.
pub fn test_part_with_body_appearance() -> Vec<u8> {
    test_part_with_face_colors(&[11, 12, 13, 14, 15, 16], 2, None)
}

/// The appearance part of [`test_part_with_appearances`] (its top and face
/// x = 1 blue grey) with the design that makes its cube
/// (`testdesign::cube_design`, as [`test_part_with_design`]'s): the import
/// with the history and the import of the bodies only give it the same
/// appearances (mitcad#1375).
pub fn test_part_with_designed_appearances() -> Vec<u8> {
    test_part_with_face_colors(&[12, 13], 1, Some((25, crate::testdesign::cube_design(25))))
}
