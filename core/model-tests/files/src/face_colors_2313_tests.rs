// SPDX-License-Identifier: MIT
//! Faces coloured on their own in STEP and IGES files (mitcad#2313), with
//! the mock kernel: the import gives them face appearances in the
//! colours, in the import's one undo step, and the export writes face
//! appearances as the faces' colours, so that a design round-trips.

use std::collections::BTreeMap;

use mitcad_model::Document;
use mitcad_model::exchange::{
    ExportFormat, ExportOptions, ExportPlacement, FaceColor, ImportBody, ReadOptions,
};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::topo::{FaceName, RoleKey};

const GREY: [f64; 3] = [0.8, 0.8, 0.8];
const GREEN: [f64; 3] = [0.0, 1.0, 0.0];
const RED: [f64; 3] = [1.0, 0.0, 0.0];

fn import_face(feature: FeatureUid, i: u32) -> String {
    FaceName::new(feature, "import", Some(RoleKey::Index(i))).to_string()
}

/// A grey block of six faces whose faces 1 and 3 are green and face 5 red,
/// and whose face 4 carries the body's own grey (no appearance of its own).
fn colored_block() -> ImportBody<MockShape> {
    ImportBody {
        name: Some("Block".to_owned()),
        color: Some(GREY),
        shape: MockShape::imported("block", 6),
        face_colors: vec![
            FaceColor {
                face: 1,
                color: GREEN,
            },
            FaceColor {
                face: 3,
                color: GREEN,
            },
            FaceColor {
                face: 4,
                color: GREY,
            },
            FaceColor {
                face: 5,
                color: RED,
            },
        ],
    }
}

/// The body's faces with appearances of their own: face name -> the
/// appearance's name and colour.
fn painted(doc: &Document<MockKernel>, body: BodyUid) -> BTreeMap<String, (String, [f64; 3])> {
    doc.face_appearances(body)
        .into_iter()
        .map(|f| {
            let appearance = doc
                .appearance(&f.appearance)
                .expect("the face's appearance");
            (
                f.face,
                (appearance.name.clone(), appearance.display_color()),
            )
        })
        .collect()
}

fn read(doc: &Document<MockKernel>, path: &str, bodies: Vec<ImportBody<MockShape>>) {
    doc.kernel()
        .files
        .borrow_mut()
        .insert(path.to_owned(), bodies);
}

#[test]
fn faces_of_their_own_color_get_appearances_in_the_import_step() {
    let mut doc = Document::new(MockKernel::default());
    read(&doc, "/models/colored.step", vec![colored_block()]);
    let imported = doc
        .import_file("/models/colored.step", &ReadOptions::default(), None)
        .unwrap();
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    let f1 = imported.feature.uid;
    let body = BodyUid::new(f1, 0);
    let green = ("Imported color 0 255 0".to_owned(), GREEN);
    let red = ("Imported color 255 0 0".to_owned(), RED);
    assert_eq!(
        painted(&doc, body),
        BTreeMap::from([
            (import_face(f1, 1), green.clone()),
            (import_face(f1, 3), green),
            (import_face(f1, 5), red),
        ])
    );
    // One appearance per colour; the body keeps its colour.
    assert_eq!(doc.custom_appearances().len(), 2);
    // The import is one undo step, appearances and all.
    assert_eq!(doc.undo_depth(), 1);
    assert_eq!(doc.undo_label(), Some("Import colored.step"));
    assert!(doc.undo().is_some());
    assert!(doc.bodies().is_empty());
    assert!(doc.custom_appearances().is_empty());
}

#[test]
fn a_file_without_face_colors_imports_as_before() {
    let mut doc = Document::new(MockKernel::default());
    let plain = ImportBody {
        face_colors: Vec::new(),
        ..colored_block()
    };
    read(&doc, "/models/plain.step", vec![plain]);
    let imported = doc
        .import_file("/models/plain.step", &ReadOptions::default(), None)
        .unwrap();
    assert!(imported.warnings.is_empty());
    assert!(
        doc.face_appearances(BodyUid::new(imported.feature.uid, 0))
            .is_empty()
    );
    assert!(doc.custom_appearances().is_empty());
    assert_eq!(doc.undo_depth(), 1);
}

#[test]
fn faces_the_body_does_not_have_are_counted_in_a_warning() {
    let mut doc = Document::new(MockKernel::default());
    let mut block = colored_block();
    block.face_colors.push(FaceColor {
        face: 9,
        color: GREEN,
    });
    read(&doc, "/models/short.step", vec![block]);
    let imported = doc
        .import_file("/models/short.step", &ReadOptions::default(), None)
        .unwrap();
    assert_eq!(imported.warnings.len(), 1, "{:?}", imported.warnings);
    assert!(imported.warnings[0].starts_with("Block: 1 faces"));
    assert_eq!(
        painted(&doc, BodyUid::new(imported.feature.uid, 0)).len(),
        3
    );
}

#[test]
fn face_appearances_are_written_as_face_colors_and_read_back() {
    let mut doc = Document::new(MockKernel::default());
    read(&doc, "/models/colored.step", vec![colored_block()]);
    let imported = doc
        .import_file("/models/colored.step", &ReadOptions::default(), None)
        .unwrap();
    let body = BodyUid::new(imported.feature.uid, 0);
    let written = |doc: &Document<MockKernel>| {
        doc.kernel()
            .written_face_colors
            .borrow()
            .last()
            .cloned()
            .expect("a file written")
    };
    let expected = vec![
        FaceColor {
            face: 1,
            color: GREEN,
        },
        FaceColor {
            face: 3,
            color: GREEN,
        },
        FaceColor {
            face: 5,
            color: RED,
        },
    ];
    for (format, path) in [
        (ExportFormat::Step, "/out/colored.step"),
        (ExportFormat::Iges, "/out/colored.igs"),
    ] {
        doc.export_bodies(
            path,
            &[],
            &ExportOptions::new(format),
            &BTreeMap::new(),
            &ExportPlacement::default(),
        )
        .unwrap();
        assert_eq!(written(&doc), vec![expected.clone()], "{format:?}");
    }
    // A STEP file of the component tree writes them too.
    doc.kernel().step_trees.set(true);
    doc.export_bodies(
        "/out/colored.step",
        &[],
        &ExportOptions::new(ExportFormat::Step),
        &BTreeMap::new(),
        &ExportPlacement::default(),
    )
    .unwrap();
    assert_eq!(written(&doc), vec![expected.clone()]);
    // The mesh formats write the body's colour only.
    doc.export_bodies(
        "/out/colored.obj",
        &[],
        &ExportOptions::new(ExportFormat::Obj),
        &BTreeMap::new(),
        &ExportPlacement::default(),
    )
    .unwrap();
    assert_eq!(written(&doc), vec![Vec::new()]);

    // Read back as the kernel reads the written file: the same faces in
    // the same colours, with the appearances the design already has.
    read(
        &doc,
        "/models/again.step",
        vec![ImportBody {
            face_colors: expected,
            ..colored_block()
        }],
    );
    let again = doc
        .import_file("/models/again.step", &ReadOptions::default(), None)
        .unwrap();
    let again_body = BodyUid::new(again.feature.uid, 0);
    let names = |doc: &Document<MockKernel>, body: BodyUid| -> Vec<(String, [f64; 3])> {
        painted(doc, body).into_values().collect()
    };
    assert_eq!(names(&doc, again_body), names(&doc, body));
    assert_eq!(doc.custom_appearances().len(), 2);
}
