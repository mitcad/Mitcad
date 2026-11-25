// SPDX-License-Identifier: MIT
//! Faces coloured on their own in 3MF files (mitcad#2385), with the mock
//! kernel: a white body whose top faces have a gold appearance of their
//! own (lettering on a sign) is written with gold on those faces'
//! triangles, the same base material colour as a body that is gold as a
//! whole (circles on doors), and the 3MF file read back finds it there.

use std::collections::BTreeMap;

use mitcad_model::Document;
use mitcad_model::appearance::AppearanceChange;
use mitcad_model::exchange::{
    ExportFormat, ExportOptions, ExportPlacement, ImportBody, MeshTolerance, ReadOptions,
    Refinement,
};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::topo::{FaceName, RoleKey};

/// The colours as sRGB bytes.
const WHITE: [u8; 3] = [0xF6, 0xF6, 0xF3];
const GOLD: [u8; 3] = [0xE8, 0xAD, 0x23];

fn color(bytes: [u8; 3]) -> [f64; 3] {
    bytes.map(|c| f64::from(c) / 255.0)
}

fn bytes(color: [f64; 3]) -> [u8; 3] {
    color.map(|c| (c * 255.0).round() as u8)
}

fn import_face(feature: FeatureUid, i: u32) -> String {
    FaceName::new(feature, "import", Some(RoleKey::Index(i))).to_string()
}

/// A design of two bodies of six faces each: the lettering, white with
/// its faces 1 and 3 gold (the mock kernel meshes side 1, +Z, and side 3,
/// +Y, of its box on them: triangles 2, 3, 6 and 7), and the circles,
/// gold as a whole.
fn design() -> (Document<MockKernel>, BodyUid, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let body = |name: &str| ImportBody {
        name: Some(name.to_owned()),
        color: None,
        shape: MockShape::imported("block", 6),
        face_colors: Vec::new(),
    };
    doc.kernel().files.borrow_mut().insert(
        "/models/sign.step".to_owned(),
        vec![body("Lettering"), body("Circles")],
    );
    let imported = doc
        .import_file("/models/sign.step", &ReadOptions::default(), None)
        .unwrap();
    let f = imported.feature.uid;
    let (lettering, circles) = (BodyUid::new(f, 0), BodyUid::new(f, 1));
    let appearance = |doc: &mut Document<MockKernel>, name: &str, bytes: [u8; 3]| {
        doc.create_appearance(&AppearanceChange {
            name: Some(name.to_owned()),
            base_color: Some(color(bytes)),
            ..AppearanceChange::default()
        })
        .unwrap()
    };
    let white = appearance(&mut doc, "White", WHITE);
    let gold = appearance(&mut doc, "Gold", GOLD);
    doc.set_body_appearance(lettering, Some(white.as_str()))
        .unwrap();
    doc.set_body_appearance(circles, Some(gold.as_str()))
        .unwrap();
    doc.set_face_appearance(
        lettering,
        &[import_face(f, 1), import_face(f, 3)],
        Some(gold.as_str()),
    )
    .unwrap();
    (doc, lettering, circles)
}

/// Exports the design to a 3MF file and reads it back.
fn exported(
    doc: &Document<MockKernel>,
    name: &str,
    options: &ExportOptions,
    colors: &BTreeMap<BodyUid, [f64; 3]>,
) -> (mitcad_3mf::Model, String) {
    let dir = mitcad_model::testing::unique_temp_dir("3mf-face-colors");
    let path = dir.join(name).to_str().unwrap().to_owned();
    doc.export_bodies(&path, &[], options, colors, &ExportPlacement::default())
        .unwrap();
    let data = std::fs::read(&path).unwrap();
    let archive = mitcad_zip::Archive::new(data.clone()).unwrap();
    let xml = String::from_utf8(archive.read_by_name("3D/3dmodel.model").unwrap()).unwrap();
    (mitcad_3mf::read(&data).unwrap(), xml)
}

/// Each triangle's colour as bytes: its own, else its part's.
fn triangle_bytes(part: &mitcad_3mf::Part) -> Vec<Option<[u8; 3]>> {
    (0..part.mesh.triangles.len())
        .map(|t| {
            part.mesh
                .triangle_colors
                .get(t)
                .copied()
                .flatten()
                .or(part.color)
                .map(bytes)
        })
        .collect()
}

#[test]
fn the_lettering_s_gold_faces_are_gold_on_its_triangles_as_the_circles() {
    let (doc, lettering, circles) = design();
    // The colours the application gives the bodies (their display colours).
    let colors = BTreeMap::from([(lettering, color(WHITE)), (circles, color(GOLD))]);
    let (model, xml) = exported(
        &doc,
        "sign.3mf",
        &ExportOptions::new(ExportFormat::ThreeMf),
        &colors,
    );
    let part = |name: &str| {
        model
            .parts
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no part {name} in {xml}"))
    };
    let (letters, circles) = (part("Lettering"), part("Circles"));
    // The circles gold as a whole, the lettering white with gold on the
    // triangles of its gold faces.
    assert_eq!(circles.color.map(bytes), Some(GOLD));
    assert!(circles.mesh.triangle_colors.is_empty());
    assert_eq!(letters.color.map(bytes), Some(WHITE));
    let gold: Vec<usize> = triangle_bytes(letters)
        .iter()
        .enumerate()
        .filter(|(_, c)| **c == Some(GOLD))
        .map(|(t, _)| t)
        .collect();
    assert_eq!(gold, [2, 3, 6, 7], "{xml}");
    assert!(
        triangle_bytes(letters)
            .iter()
            .all(|c| *c == Some(GOLD) || *c == Some(WHITE))
    );
    // In the file: a base material of the gold colour that the triangles
    // name, in the group of the parts' colours.
    assert!(
        xml.contains("<base name=\"Color #E8AD23\" displaycolor=\"#E8AD23FF\"/>"),
        "{xml}"
    );
    assert_eq!(xml.matches(" pid=\"1\" p1=\"").count(), 4, "{xml}");

    // Without the application's colours (the command line), the lettering
    // takes its appearance's colour as its own all the same.
    let (model, xml) = exported(
        &doc,
        "plain.3mf",
        &ExportOptions::new(ExportFormat::ThreeMf),
        &BTreeMap::new(),
    );
    let letters = model.parts.iter().find(|p| p.name == "Lettering").unwrap();
    assert_eq!(letters.color.map(bytes), Some(WHITE), "{xml}");
    let gold = triangle_bytes(letters)
        .iter()
        .filter(|c| **c == Some(GOLD))
        .count();
    assert_eq!(gold, 4);
}

#[test]
fn split_long_edges_keep_their_faces_colours() {
    let (doc, lettering, circles) = design();
    let colors = BTreeMap::from([(lettering, color(WHITE)), (circles, color(GOLD))]);
    let mut options = ExportOptions::new(ExportFormat::ThreeMf);
    options.refinement = Refinement::Custom(MeshTolerance {
        max_edge: 3.0,
        ..Refinement::default().tolerance()
    });
    let (model, xml) = exported(&doc, "split.3mf", &options, &colors);
    let letters = model.parts.iter().find(|p| p.name == "Lettering").unwrap();
    assert!(letters.mesh.triangles.len() > 12);
    assert_eq!(
        letters.mesh.triangle_colors.len(),
        letters.mesh.triangles.len()
    );
    // The gold triangles are those on the box's top (z 10) and its +Y side
    // (y 10), and all of those.
    for (t, c) in letters.mesh.triangles.iter().zip(triangle_bytes(letters)) {
        let on = |axis: usize| {
            t.iter()
                .all(|v| (letters.mesh.vertices[*v as usize][axis] - 10.0).abs() < 1e-9)
        };
        let expected = if on(2) || on(1) { GOLD } else { WHITE };
        assert_eq!(c, Some(expected), "{t:?} in {xml}");
    }
}

#[test]
fn faces_of_the_body_s_own_colour_add_nothing() {
    let (mut doc, lettering, _) = design();
    // The lettering's faces in its own white: no triangle colours.
    let white = doc
        .custom_appearances()
        .iter()
        .find(|a| a.name == "White")
        .unwrap()
        .id
        .clone();
    let f = lettering.feature;
    doc.set_face_appearance(
        lettering,
        &[import_face(f, 1), import_face(f, 3)],
        Some(white.as_str()),
    )
    .unwrap();
    let (model, xml) = exported(
        &doc,
        "white.3mf",
        &ExportOptions::new(ExportFormat::ThreeMf),
        &BTreeMap::new(),
    );
    let letters = model.parts.iter().find(|p| p.name == "Lettering").unwrap();
    assert!(letters.mesh.triangle_colors.is_empty(), "{xml}");
    assert!(!xml.contains(" p1=\""), "{xml}");
}
