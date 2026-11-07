// SPDX-License-Identifier: MIT
//! A synthetic design with a mesh body (mitcad#1366), for the tests here
//! and in the import (no real design needed): the root component `Scan`,
//! a timeline of one mesh body item (`Mesh1`) whose mesh body is the
//! tetrahedron of `mitcad_f3d_file::paramesh::testdata`, a millimetre mesh
//! placed 2 cm along X ([`MESH_PLACEMENT`]); and a second container that
//! no record of the design names. The mesh body's presentation envelopes
//! name a physical material, steel ([`MATERIAL_ASSET`]), and the satin
//! metal appearance it brings ([`APPEARANCE_ASSET`]), both in the
//! document's asset package ([`asset_package`], mitcad#1376).

use crate::design::classes::{
    BLOB_HOLDER, COMPONENT, FEATURE_MANAGER, HEALTH, MESH_BODY, MESH_ENTRY, MESH_FEATURE,
    MESH_GUID, OCCURRENCE, TIMELINE,
};
use crate::design::writer::DesignWriter;
use crate::zip::{Method, Writer};
use mitcad_f3d_file::paramesh::testdata::{KEY, tetrahedron};

/// The root class of the design streams.
const ROOT: &str = "98542EB9-A4F2-4137-A808-DBB5B3CD6159";

/// The physical-material library of the mesh body's presentation.
const LIBRARY: &str = "C1EEA57C-3F56-45FC-B8CB-A9EC46A9994C";

/// The mesh body's placement: container coordinates (mm) to cm, 2 cm
/// along X.
pub const MESH_PLACEMENT: [[f64; 4]; 4] = [
    [0.1, 0.0, 0.0, 2.0],
    [0.0, 0.1, 0.0, 0.0],
    [0.0, 0.0, 0.1, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// The container the design's record names.
pub const MESH_FILE: &str = "ParaMeshGeometry.6a1f0c2e-0000-4000-8000-000000000001.paramesh";

/// A container no record of the design names.
pub const STRAY_FILE: &str = "ParaMeshGeometry.6a1f0c2e-0000-4000-8000-000000000002.paramesh";

/// The object ids of the root component and of the mesh body item.
pub const ROOT_COMPONENT: u64 = 2;
pub const MESH_ITEM: u64 = 10;

/// The mesh body's name.
pub const MESH_NAME: &str = "Scanned part";

/// The mesh body record's object id (`_f3d.mesh_bodies[].id`).
pub const MESH_BODY_RECORD: u64 = 20;

/// The physical material the mesh body's first envelope names (its token
/// and asset) and its preset.
pub const MATERIAL_ASSET: &str = "E7566FA6-0000-468D-BFDC-383370014633";
pub const MATERIAL_PRESET: &str = "PrismMaterial-018";

/// The material's structural aspect (its density, 7850 kg/m³).
pub const ASPECT_ASSET: &str = "F3981BFD-0000-497A-A042-21FC315C9862";

/// The appearance the second envelope names: its token, the asset (the
/// token's newer form) and its preset.
pub const APPEARANCE_TOKEN: &str = "D5EA7CB4-0000-49E5-A099-8AD50EB050AB";
pub const APPEARANCE_ASSET: &str = "D5EA7CB4-0000-49E5-A099-8AD50EB050AB_Post2015";
pub const APPEARANCE_PRESET: &str = "PrismMaterial-018_physmat_aspects:Prism-256";

/// The appearance's description (the name it comes in by) and its
/// stored colour and roughness.
pub const APPEARANCE_NAME: &str = "Steel - satin";
pub const APPEARANCE_COLOR: [f64; 4] = [0.6262, 0.6262, 0.6262, 1.0];
pub const APPEARANCE_ROUGHNESS: f64 = 0.2646;

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

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// A placement as the mesh body record keeps it: the flag 0 and the
/// matrix, row-major.
fn placement(m: &[[f64; 4]; 4]) -> Vec<u8> {
    let mut v = vec![0];
    v.extend(m.iter().flatten().flat_map(|x| x.to_le_bytes()));
    v
}

/// A timeline item tail: `i32 result_no | str16 base | u32 index | u32 0 |
/// str16 custom | u32 0 | flags | ref health`.
fn tail(result_no: i32, base: &str, index: u32, health: u64) -> Vec<u8> {
    [
        &result_no.to_le_bytes()[..],
        &s16(base),
        &u32b(index),
        &u32b(0),
        &s16(""),
        &u32b(0),
        &[0, 0, 0],
        &r(health),
        &[0],
    ]
    .concat()
}

/// The design streams (MetaStream, BulkStream).
pub fn mesh_design_streams() -> (Vec<u8>, Vec<u8>) {
    let mut d = DesignWriter::default();
    for (g, v) in [
        (ROOT, 0),
        (COMPONENT, 1),
        (OCCURRENCE, 1),
        (FEATURE_MANAGER, 1),
        (TIMELINE, 3),
        (HEALTH, 1),
        (BLOB_HOLDER, 4),
        (MESH_FEATURE, 0),
        (MESH_BODY, 7),
        (MESH_GUID, 1),
        (MESH_ENTRY, 0),
    ] {
        d.class(g, if g == ROOT { "" } else { ROOT }, v);
    }
    d.obj(
        1,
        FEATURE_MANAGER,
        [&[0u8, 0][..], &r(ROOT_COMPONENT)].concat(),
    );
    d.obj(
        ROOT_COMPONENT,
        COMPONENT,
        [&[0u8, 0][..], &s16("0_2"), &s16("Scan")].concat(),
    );
    d.obj(
        3,
        TIMELINE,
        [&[0u8, 0][..], &r(1), &u32b(1), &r(MESH_ITEM)].concat(),
    );
    // The root occurrence (no container) names the root component.
    d.obj(
        4,
        OCCURRENCE,
        [&[0u8, 0][..], &r(ROOT_COMPONENT), &[1]].concat(),
    );
    // The item: its mesh bodies, then its tail.
    d.obj(
        MESH_ITEM,
        MESH_FEATURE,
        [
            &[0u8, 0][..],
            &u32b(1),
            &r(MESH_BODY_RECORD),
            &tail(1, "Mesh", 1, 11),
        ]
        .concat(),
    );
    d.obj(11, HEALTH, vec![0, 0]);
    // The mesh body: its prologue to offset 41 (the header is 19 bytes),
    // both placements, the presentation envelopes with its name between
    // them, then its item, body holder and GUID record. An envelope is a
    // name as a body record's: the token, the library, an empty display
    // name (`u32 0`) and the preset (mitcad#1376).
    let envelope =
        |guid: &str, preset: &str| [s16(guid), s16(LIBRARY), u32b(0), s16(preset)].concat();
    d.obj(
        MESH_BODY_RECORD,
        MESH_BODY,
        [
            &[0u8; 22][..],
            &placement(&MESH_PLACEMENT),
            &placement(&MESH_PLACEMENT),
            &envelope(MATERIAL_ASSET, MATERIAL_PRESET),
            &s16(MESH_NAME),
            &envelope(APPEARANCE_TOKEN, APPEARANCE_PRESET),
            &r(MESH_ITEM),
            &r(21),
            &r(30),
        ]
        .concat(),
    );
    d.obj(
        21,
        BLOB_HOLDER,
        [&[0u8, 0][..], &r(ROOT_COMPONENT)].concat(),
    );
    // The GUID record: the key at offset 32, then the entry-name record.
    d.obj(
        30,
        MESH_GUID,
        [&[0u8; 13][..], &u32b(36), KEY.as_bytes(), &r(31)].concat(),
    );
    d.obj(
        31,
        MESH_ENTRY,
        [&[0u8, 0][..], &r(30), &s16(MESH_FILE)].concat(),
    );
    d.streams()
}

/// The `.f3d` file: its manifest, design streams and the two containers.
pub fn mesh_file() -> Vec<u8> {
    let (meta, bulk) = mesh_design_streams();
    let mut w = Writer::new();
    let folder = "FusionAssetName[Active]";
    for (name, data) in [
        ("Manifest.dat".to_owned(), manifest()),
        (
            format!("{folder}/FusionDesignSegmentType1/MetaStream.dat"),
            meta,
        ),
        (
            format!("{folder}/FusionDesignSegmentType1/BulkStream.dat"),
            bulk,
        ),
        (
            format!("{folder}/ParaMeshGeometry.BlobParts/{MESH_FILE}"),
            tetrahedron(),
        ),
        (
            format!("{folder}/ParaMeshGeometry.BlobParts/{STRAY_FILE}"),
            tetrahedron(),
        ),
        (
            format!(
                "{folder}/ProteinAssets.BlobParts/ProteinAsset.6a1f0c2e-0000-4000-8000-0000000000a1.protein"
            ),
            asset_package(),
        ),
    ] {
        w.add(&name, &data, Method::Stored)
            .expect("a small entry is written");
    }
    w.finish().expect("the archive is written")
}

/// The document's asset package (`.protein`, mitcad#1376): the physical
/// material (steel by its classification, 7850 kg/m³ by its structural
/// aspect, which it is connected to) and the metal appearance it brings,
/// described as [`APPEARANCE_NAME`].
pub fn asset_package() -> Vec<u8> {
    let str32 = |s: &str| {
        let mut v = u32b(s.len() as u32);
        v.extend_from_slice(s.as_bytes());
        v
    };
    let header = |schema: &str, guid: &str, name: &str| {
        [str32(schema), str32(guid), str32(name), str32(LIBRARY)].concat()
    };
    let f64s =
        |values: &[f64]| -> Vec<u8> { values.iter().flat_map(|x| x.to_le_bytes()).collect() };
    // Its properties by id: the aspects (a reference, its connections:
    // `u8 1 | u8 1 | u32 n | n × str`), the classification.
    let material = [
        header("PhysMatSchema", MATERIAL_ASSET, MATERIAL_PRESET),
        vec![1, 1],
        u32b(1),
        str32(ASPECT_ASSET),
        str32("Metal.Ferrous.Steel"),
    ]
    .concat();
    // The density: its unit tag (kg/m³) and value.
    let aspect = [
        header("StructuralMetalSchema", ASPECT_ASSET, "PhysicalMetal_124"),
        u32b(0x11001),
        f64s(&[7850.0]),
    ]
    .concat();
    let appearance = [
        header("PrismMetalSchema", APPEARANCE_ASSET, APPEARANCE_PRESET),
        f64s(&APPEARANCE_COLOR),
        f64s(&[APPEARANCE_ROUGHNESS]),
    ]
    .concat();
    // The appearance's description (version 0: the description alone).
    let description = [
        str32("PrismMetalSchema"),
        vec![0],
        str32("Prism-256"),
        str32(APPEARANCE_PRESET),
        u32b(0),
        str32(APPEARANCE_NAME),
    ]
    .concat();
    let schemas = [
        (
            "PhysMatSchema",
            r#"<AssetSchema><UID val="PhysMatSchema"/><String id="AssetLibID" val=""/><Reference id="physmat_aspects" allowconnectedassets="multiple" allowmultiplevalues="true" val=""/><String id="physmat_classification" val=""/></AssetSchema>"#,
        ),
        (
            "StructuralMetalSchema",
            r#"<AssetSchema><UID val="StructuralMetalSchema"/><String id="AssetLibID" val=""/><Float id="structural_Density" unit="KilogramsPerCubicMeter" val="0"/></AssetSchema>"#,
        ),
        (
            "PrismMetalSchema",
            r#"<AssetSchema><UID val="PrismMetalSchema"/><String id="AssetLibID" val=""/><Color id="metal_f0" valR="1" valG="1" valB="1" valA="1"/><Float id="surface_roughness" val="0"/></AssetSchema>"#,
        ),
    ];
    let mut w = Writer::new();
    for (name, xml) in schemas {
        w.add(
            &format!("F/Schemas/{name}.xml"),
            xml.as_bytes(),
            Method::Stored,
        )
        .expect("a small entry is written");
    }
    for (name, records) in [
        ("InstanceProperties.bin", vec![material, aspect, appearance]),
        ("DefinitionIteratorProperties.bin", vec![description]),
    ] {
        w.add(
            &format!("F/AssetData/{name}"),
            &mitcad_f3d_file::protein::paged_stream(&records),
            Method::Stored,
        )
        .expect("a small entry is written");
    }
    w.finish().expect("the package is written")
}

/// A `Manifest.dat` that lists the document's GUID.
fn manifest() -> Vec<u8> {
    [
        &3u32.to_le_bytes()[..],
        b"f3d",
        &s16("A Mitcad test document"),
        &s16("6a1f0c2e-0000-4000-8000-0000000000d0"),
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::F3dFile;
    use mitcad_f3d_file::paramesh;

    #[test]
    fn the_design_joins_its_mesh_body_to_its_container() {
        let file = F3dFile::from_bytes(mesh_file()).unwrap();
        let entries = file.mesh_entries();
        assert_eq!(entries.len(), 2);
        let (meta, bulk) = mesh_design_streams();
        let seg = crate::design::stream::Segment::parse(&meta, bulk).unwrap();
        let bodies = crate::design::mesh::mesh_bodies(&seg);
        assert_eq!(bodies.len(), 1);
        let body = &bodies[0];
        assert_eq!(body.error, None);
        assert_eq!(body.file.as_deref(), Some(MESH_FILE));
        assert_eq!(body.key.as_deref(), Some(KEY));
        assert_eq!(body.feature, Some(MESH_ITEM));
        assert_eq!(body.component, Some(ROOT_COMPONENT));
        assert_eq!(body.name.as_deref(), Some(MESH_NAME));
        assert_eq!(body.transform, Some(MESH_PLACEMENT));
        // The container the record names, placed: the tetrahedron's corner
        // at 1 mm along X of the container is 21 mm along X.
        let entry = entries.iter().find(|e| e.ends_with(MESH_FILE)).unwrap();
        let mesh = paramesh::parse(&file.read(entry).unwrap()).unwrap();
        assert!(mesh.has_property_text(body.key.as_deref().unwrap()));
        let placed = mesh.placed(body.transform.as_ref());
        assert!((placed.points[0][0] - 20.0).abs() < 1e-9);
        assert!((placed.points[1][0] - 21.0).abs() < 1e-9);
        assert_eq!(placed.triangles.len(), 4);
    }

    #[test]
    fn the_dump_lists_the_mesh_bodies() {
        let file = F3dFile::from_bytes(mesh_file()).unwrap();
        let design = crate::design::decode_streams(&file, "test").unwrap();
        let dump = design.dump.unwrap();
        let meshes = dump.f3d.unwrap().mesh_bodies.unwrap();
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].feature, Some(MESH_ITEM));
        assert_eq!(meshes[0].component, Some(ROOT_COMPONENT));
        assert_eq!(meshes[0].transform, Some(MESH_PLACEMENT));
        let items = dump.timeline.unwrap().items.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].object_type(), Some("MeshFeature"));
        assert_eq!(items[0].f3d.as_ref().unwrap().object_id, Some(MESH_ITEM));
    }

    /// The mesh body's presentation envelopes name its physical material
    /// and the appearance it brings (mitcad#1376), as a body record names
    /// a body's: the decoder gives them in `_f3d.appearance.meshes` with
    /// the assets they name and the material's aspect.
    #[test]
    fn the_dump_names_the_mesh_body_s_material_and_appearance() {
        let file = F3dFile::from_bytes(mesh_file()).unwrap();
        let design = crate::design::decode_streams(&file, "test").unwrap();
        let data = design.dump.unwrap().f3d.unwrap().appearance.unwrap();
        assert_eq!(data.warnings, None);
        let meshes = data.meshes.as_deref().unwrap();
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].entity, Some(MESH_BODY_RECORD));
        let material = meshes[0].material.as_ref().unwrap();
        assert_eq!(material.token.as_deref(), Some(MATERIAL_ASSET));
        assert_eq!(material.asset.as_deref(), Some(MATERIAL_ASSET));
        assert_eq!(material.preset.as_deref(), Some(MATERIAL_PRESET));
        let appearance = meshes[0].appearance.as_ref().unwrap();
        assert_eq!(appearance.token.as_deref(), Some(APPEARANCE_TOKEN));
        assert_eq!(appearance.asset.as_deref(), Some(APPEARANCE_ASSET));
        assert_eq!(appearance.preset.as_deref(), Some(APPEARANCE_PRESET));
        let assets: Vec<&str> = data
            .assets
            .iter()
            .flatten()
            .filter_map(|a| a.id.as_deref())
            .collect();
        for asset in [MATERIAL_ASSET, ASPECT_ASSET, APPEARANCE_ASSET] {
            assert!(assets.contains(&asset), "{assets:?}");
        }
        // No B-rep body records.
        assert_eq!(data.bodies.as_deref().map(<[_]>::len), Some(0));
    }

    #[test]
    fn a_record_without_placements_is_the_identity() {
        // The flags `1 1` in place of the two matrices (256 bytes shorter).
        let mut d = DesignWriter::default();
        for (g, v) in [(ROOT, 0), (MESH_BODY, 7)] {
            d.class(g, if g == ROOT { "" } else { ROOT }, v);
        }
        d.obj(20, MESH_BODY, [&[0u8; 22][..], &[1, 1]].concat());
        let seg = d.segment();
        let bodies = crate::design::mesh::mesh_bodies(&seg);
        assert_eq!(
            (bodies[0].transform, bodies[0].error.as_deref()),
            (None, None)
        );
        // An unknown flag, and two placements that differ, are not read.
        let mut d = DesignWriter::default();
        for (g, v) in [(ROOT, 0), (MESH_BODY, 7)] {
            d.class(g, if g == ROOT { "" } else { ROOT }, v);
        }
        d.obj(20, MESH_BODY, [&[0u8; 22][..], &[2, 1]].concat());
        let mut moved = MESH_PLACEMENT;
        moved[0][3] = 3.0;
        d.obj(
            21,
            MESH_BODY,
            [
                &[0u8; 22][..],
                &placement(&MESH_PLACEMENT),
                &placement(&moved),
            ]
            .concat(),
        );
        let bodies = crate::design::mesh::mesh_bodies(&d.segment());
        assert!(
            bodies
                .iter()
                .all(|b| b.error.is_some() && b.transform.is_none())
        );
        // Another class version is not read either.
        let mut d = DesignWriter::default();
        for (g, v) in [(ROOT, 0), (MESH_BODY, 6)] {
            d.class(g, if g == ROOT { "" } else { ROOT }, v);
        }
        d.obj(20, MESH_BODY, [&[0u8; 22][..], &[1, 1]].concat());
        assert!(
            crate::design::mesh::mesh_bodies(&d.segment())[0]
                .error
                .is_some()
        );
    }
}
