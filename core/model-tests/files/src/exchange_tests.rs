// SPDX-License-Identifier: MIT
//! Base features, import and export, with the mock kernel.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::exchange::*;
use mitcad_model::features::Operation;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::internal::exchange::sketch_drawing;
use mitcad_model::kernel::Kernel;
use mitcad_model::profile::SegmentGeometry;
use mitcad_model::test_support::{block, def, num};
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::topo::{EdgeName, FaceName, RoleKey};
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn body(name: Option<&str>, history: &str, faces: usize) -> ImportBody<MockShape> {
    ImportBody {
        name: name.map(str::to_owned),
        color: None,
        shape: MockShape::imported(history, faces),
    }
}

fn import_face(feature: FeatureUid, i: u32) -> FaceName {
    FaceName::new(feature, "import", Some(RoleKey::Index(i)))
}

fn histories(doc: &Document<MockKernel>) -> Vec<(String, String, String)> {
    doc.bodies()
        .into_iter()
        .map(|b| (b.uid.to_string(), b.name, b.shape.history.clone()))
        .collect()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|r| serde_json::from_str(&r).unwrap())
        .map_err(|e| e.to_string())
}

#[test]
fn base_features_make_new_bodies_with_faces_named_by_index() {
    let mut doc = Document::new(MockKernel::default());
    let imported = doc
        .add_base_feature(BaseInput {
            name: Some("Brackets".to_owned()),
            source: Some("brackets.f3d".to_owned()),
            ..BaseInput::new(vec![
                body(Some("Bracket"), "box", 6),
                body(None, "cylinder", 3),
                body(Some("Bracket"), "box", 6),
            ])
        })
        .unwrap();
    let f1 = FeatureUid(1);
    assert_eq!(imported.feature.uid, f1);
    assert_eq!(imported.feature.name, "Brackets");
    assert_eq!(
        imported.bodies,
        vec![
            ImportedBody {
                uid: BodyUid::new(f1, 0),
                name: "Bracket".to_owned(),
                kind: BodyKind::Solid
            },
            ImportedBody {
                uid: BodyUid::new(f1, 1),
                name: "Body1".to_owned(),
                kind: BodyKind::Solid
            },
            ImportedBody {
                uid: BodyUid::new(f1, 2),
                name: "Bracket (2)".to_owned(),
                kind: BodyKind::Solid
            },
        ]
    );
    assert_eq!(
        histories(&doc),
        vec![
            (
                "F1.b0".into(),
                "Bracket".into(),
                "import(F1:0..6,box)".into()
            ),
            (
                "F1.b1".into(),
                "Body1".into(),
                "import(F1:6..9,cylinder)".into()
            ),
            (
                "F1.b2".into(),
                "Bracket (2)".into(),
                "import(F1:9..15,box)".into()
            ),
        ]
    );
    // Faces are counted over the bodies; edges between them resolve.
    let second = doc.body_shape(BodyUid::new(f1, 1)).unwrap();
    assert_eq!(second.faces[0], import_face(f1, 6));
    let edge = EdgeName::new(import_face(f1, 6), import_face(f1, 7));
    assert_eq!(doc.kernel().count_edges(second, &edge), 1);
    assert_eq!(doc.undo_label(), Some("Add Brackets"));

    // A fillet on an imported edge, then undo and redo from the cache.
    let fillet = def(json!({"type": "fillet", "body": "F1.b1", "edges": [edge], "radius": 1}));
    doc.add_feature(&fillet, None).unwrap();
    assert_eq!(doc.status(FeatureUid(2)), Some(&FeatureStatus::Ok));
    doc.undo();
    doc.undo();
    assert!(doc.bodies().is_empty());
    doc.redo();
    assert_eq!(doc.stats().evaluated, vec![]);
    // The bodies adopted their shapes once (mitcad#244); no data was read.
    assert_eq!(doc.kernel().count("adopt"), 3);
    assert_eq!(doc.kernel().count("import"), 0);
}

#[test]
fn base_features_adopt_the_shapes_their_data_was_written_from() {
    // Added from shapes: each body adopts its shape instead of reading
    // the data it was written to back (mitcad#244), with the same faces
    // and names the data reads as.
    let mut doc = Document::new(MockKernel::default());
    let input = || {
        BaseInput::new(vec![
            body(Some("Bracket"), "box", 6),
            body(None, "cylinder", 3),
        ])
    };
    let f1 = doc.add_base_feature(input()).unwrap().feature.uid;
    assert_eq!(doc.kernel().count("adopt"), 2);
    assert_eq!(doc.kernel().count("import"), 0);
    let adopted = histories(&doc);
    assert_eq!(
        adopted,
        vec![
            (
                "F1.b0".into(),
                "Bracket".into(),
                "import(F1:0..6,box)".into()
            ),
            (
                "F1.b1".into(),
                "Body1".into(),
                "import(F1:6..9,cylinder)".into()
            ),
        ]
    );
    let shape = doc.body_shape(BodyUid::new(f1, 1)).unwrap();
    assert_eq!(shape.faces[0], import_face(f1, 6));

    // Without the shapes (a design read from its file) the data is read,
    // with the same result.
    let text = doc.to_json();
    let mut loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(loaded.kernel().count("adopt"), 0);
    assert_eq!(loaded.kernel().count("import"), 2);
    assert_eq!(histories(&loaded), adopted);

    // A kernel that cannot adopt reads the data.
    let kernel = MockKernel::default();
    kernel.fail.borrow_mut().insert("adopt");
    let mut failing = Document::new(kernel);
    failing.add_base_feature(input()).unwrap();
    assert_eq!(failing.kernel().count("import"), 2);
    assert_eq!(histories(&failing), adopted);
}

#[test]
fn base_features_round_trip_through_design_files() {
    let mut doc = Document::new(MockKernel::default());
    let mut part = body(Some("Part"), "box", 6);
    part.color = Some([0.8, 0.1, 0.25]);
    doc.add_base_feature(BaseInput {
        source: Some("part.step".to_owned()),
        ..BaseInput::new(vec![part, body(None, "mesh triangles", 1)])
    })
    .unwrap();
    let text = doc.to_json();
    let value: Value = serde_json::from_str(&text).unwrap();
    let feature = &value["features"][0];
    assert_eq!(feature["type"], "base");
    assert_eq!(feature["name"], "Base1");
    assert_eq!(feature["source"], "part.step");
    assert_eq!(feature["bodies"][0]["name"], "Part");
    assert_eq!(feature["bodies"][0]["color"], json!([0.8, 0.1, 0.25]));
    assert_eq!(feature["bodies"][0]["brep"]["compression"], "zlib");
    assert_eq!(feature["bodies"][0]["brep"]["size"], 5);
    assert_eq!(feature["bodies"][1]["mesh"], true);
    assert!(feature.get("operation").is_none());
    assert_eq!(
        value["bodies"],
        json!([{"uid": "F1.b0", "name": "Part"}, {"uid": "F1.b1", "name": "Body1"}])
    );

    let mut loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(histories(&loaded), histories(&doc));
    assert_eq!(loaded.to_json(), text);
    assert_eq!(
        loaded
            .kernel()
            .body_kind(loaded.body_shape(BodyUid::new(FeatureUid(1), 1)).unwrap()),
        Ok(BodyKind::Mesh)
    );

    // Corrupt data is reported with its place in the file.
    let broken = text.replacen("\"size\": 5", "\"size\": 6", 1);
    let error = Document::from_json(&broken, MockKernel::default())
        .err()
        .expect("rejected");
    let message = error.to_string();
    assert!(
        message.contains("features[0] (Base1)") && message.contains("B-rep data has 5 bytes"),
        "{message}"
    );
}

#[test]
fn base_features_join_and_cut_participant_bodies() {
    let mut b = block();
    let cut = BaseInput {
        operation: Operation::Cut,
        participants: vec![b.body],
        ..BaseInput::new(vec![body(Some("Tool"), "box", 6)])
    };
    let imported = b.doc.add_base_feature(cut).unwrap();
    let base = imported.feature.uid;
    assert!(imported.bodies.is_empty());
    assert_eq!(b.doc.status(base), Some(&FeatureStatus::Ok));
    assert_eq!(
        histories(&b.doc),
        vec![(
            "F2.b0".into(),
            "Body1".into(),
            "cut(prism(F2:0..20),import(F3:0..6,box))".into()
        )]
    );
    // The tool of a cut is not a body, so its name is not given to one.
    let saved: Value = serde_json::from_str(&b.doc.to_json()).unwrap();
    assert_eq!(saved["bodies"], json!([{"uid": "F2.b0", "name": "Body1"}]));

    // Two bodies join as one tool; a mesh cannot.
    let join = BaseInput {
        operation: Operation::Join,
        ..BaseInput::new(vec![body(None, "a", 4), body(None, "b", 4)])
    };
    b.doc.add_base_feature(join).unwrap();
    let history = &histories(&b.doc)[0].2;
    assert!(
        history.ends_with(",compound(import(F4:0..4,a),import(F4:4..8,b)))"),
        "{history}"
    );
    let mesh = BaseInput {
        operation: Operation::Join,
        ..BaseInput::new(vec![body(None, "mesh", 1)])
    };
    let error = b.doc.add_base_feature(mesh).unwrap_err().to_string();
    assert!(
        error.contains("mesh body, which can only be a new body"),
        "{error}"
    );

    // A cut that reaches nothing fails like an extrusion's.
    b.doc.kernel().touch.replace(Some(vec![false]));
    let missing = BaseInput {
        operation: Operation::Cut,
        ..BaseInput::new(vec![body(None, "far", 6)])
    };
    let uid = b.doc.add_base_feature(missing).unwrap().feature.uid;
    assert_eq!(
        b.doc.status(uid),
        Some(&FeatureStatus::failed(
            "the base feature does not cut into any participant body"
        ))
    );
}

#[test]
fn base_feature_definitions_are_checked() {
    let mut b = block();
    let brep = serde_json::to_value(mitcad_model::features::Brep::new(b"6 box".to_vec())).unwrap();
    for (definition, expected) in [
        (json!({"type": "base", "bodies": []}), "at least one body"),
        (
            json!({"type": "base", "bodies": [{"brep": brep, "color": [2, 0, 0]}]}),
            "between 0 and 1",
        ),
        (
            json!({"type": "base", "bodies": [{"brep": brep}], "participants": ["F2.b0"]}),
            "no participant bodies",
        ),
        (
            json!({"type": "base", "bodies": [{"brep": brep}], "operation": "join",
                   "participants": ["F1.b0"]}),
            "does not create bodies",
        ),
    ] {
        let error = b
            .doc
            .add_feature(&def(definition.clone()), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{definition}: {error}");
    }
    // The definition form works as a command too.
    let added = b
        .doc
        .add_feature(
            &def(json!({"type": "base", "bodies": [{"brep": brep}], "operation": "join"})),
            None,
        )
        .unwrap();
    assert_eq!(added.name, "Base1");
    assert!(histories(&b.doc)[0].2.contains("import(F3:0..6,box)"));
}

#[test]
fn import_file_command_makes_a_base_feature() {
    let mut doc = Document::new(MockKernel::default());
    let mut part = body(Some("Plate"), "plate", 6);
    part.color = Some([0.0, 0.5, 1.0]);
    doc.kernel().files.borrow_mut().insert(
        "/models/assembly.step".to_owned(),
        vec![part, body(Some("Plate"), "plate", 6)],
    );
    doc.kernel()
        .files
        .borrow_mut()
        .insert("scan.stl".to_owned(), vec![body(Some("scan"), "mesh", 1)]);
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "/models/assembly.step"}),
    )
    .unwrap();
    assert_eq!(
        result,
        json!({"uid": "F1", "name": "Base1", "bodies": [
            {"uid": "F1.b0", "name": "Plate", "kind": "solid"},
            {"uid": "F1.b1", "name": "Plate (2)", "kind": "solid"}],
            "recomputed": 1, "error": null})
    );
    assert_eq!(doc.undo_label(), Some("Import assembly.step"));
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "scan.stl", "name": "Scan", "unit_mm": 25.4}),
    )
    .unwrap();
    assert_eq!(result["bodies"][0]["kind"], "mesh");
    assert_eq!(result["name"], "Scan");
    let feature: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F1"}"#).unwrap()).unwrap();
    assert_eq!(feature["def"]["source"], "assembly.step");

    for (bad, expected) in [
        (
            json!({"cmd": "import_file", "path": "missing.step"}),
            "cannot read missing.step",
        ),
        (
            json!({"cmd": "import_file", "path": "scan.stl", "unit_mm": 0}),
            "unit must be greater than zero",
        ),
        (
            json!({"cmd": "import_file", "path": "scan.stl", "units": 1}),
            "unknown field `units`",
        ),
    ] {
        let error = command(&mut doc, bad.clone()).unwrap_err();
        assert!(error.contains(expected), "{bad}: {error}");
    }
}

#[test]
fn import_file_leaves_out_empty_bodies_with_a_warning() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().files.borrow_mut().insert(
        "support.step".to_owned(),
        vec![
            body(Some("Support"), "support", 6),
            body(Some("Screw"), "screw", 0),
            body(None, "nut", 0),
        ],
    );
    doc.kernel().files.borrow_mut().insert(
        "nothing.step".to_owned(),
        vec![body(Some("Screw"), "screw", 0)],
    );
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "support.step"}),
    )
    .unwrap();
    assert_eq!(
        result,
        json!({"uid": "F1", "name": "Base1", "bodies": [
            {"uid": "F1.b0", "name": "Support", "kind": "solid"}],
            "warnings": ["body 1 (Screw) is empty: left out", "body 2 is empty: left out"],
            // The warnings as notices too (mitcad#1040).
            "notices": [
                {"level": "warning", "message": "body 1 (Screw) is empty: left out"},
                {"level": "warning", "message": "body 2 is empty: left out"}],
            "recomputed": 1, "error": null})
    );
    assert_eq!(histories(&doc).len(), 1);
    assert_eq!(doc.undo_label(), Some("Import support.step"));

    let error = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "nothing.step"}),
    )
    .unwrap_err();
    assert!(
        error.contains("nothing.step has only empty bodies (1)"),
        "{error}"
    );
    assert_eq!(histories(&doc).len(), 1);

    // The shape API keeps refusing an empty body: its body options and an
    // importer's `replaces` count the shapes by index.
    let error = doc
        .add_base_feature(BaseInput::new(vec![
            body(Some("Support"), "support", 6),
            body(Some("Screw"), "screw", 0),
        ]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("body 1 is empty"), "{error}");
}

#[test]
fn export_writes_bodies_with_names_and_imported_colours() {
    let mut b = block();
    let mut part = body(Some("Part"), "part", 6);
    part.color = Some([1.0, 0.0, 0.0]);
    b.doc
        .add_base_feature(BaseInput::new(vec![part, body(Some("Scan"), "mesh", 1)]))
        .unwrap();
    // STL is the model's (mitcad#17), the other formats the kernel's.
    let dir = mitcad_model::testing::unique_temp_dir("export");
    let stl = dir.join("export.stl").to_str().unwrap().to_owned();
    let all = command(&mut b.doc, json!({"cmd": "export", "path": stl})).unwrap();
    assert_eq!(
        all,
        json!({"path": stl, "format": "stl", "bodies": ["F2.b0", "F3.b0", "F3.b1"],
               "recomputed": 0, "error": null})
    );
    assert_eq!(read_stl(&std::fs::read(&stl).unwrap()).len(), 36);
    mitcad_model::testing::remove_temp(&dir);
    let some = command(
        &mut b.doc,
        json!({"cmd": "export", "path": "part.STP", "bodies": ["Part", "F2.b0"],
               "schema": "ap242", "unit": "in"}),
    )
    .unwrap();
    assert_eq!(some["format"], "step");
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": "fine.obj", "bodies": ["F3.b1"],
               "refinement": {"deviation": 0.001, "angle": 0.1}}),
    )
    .unwrap();
    let written = b.doc.kernel().written.borrow().clone();
    assert_eq!(written.len(), 2);
    assert_eq!(
        written[0],
        (
            "part.STP".to_owned(),
            ExportFormat::Step,
            vec![
                ("Part".to_owned(), Some([1.0, 0.0, 0.0])),
                ("Body1".to_owned(), None)
            ]
        )
    );
    assert_eq!(written[1].2, vec![("Scan".to_owned(), None)]);

    for (bad, expected) in [
        (
            json!({"cmd": "export", "path": "a.step", "bodies": ["F3.b1"]}),
            "Scan is a mesh body",
        ),
        (
            json!({"cmd": "export", "path": "a.dwg"}),
            "unknown file type",
        ),
        (
            json!({"cmd": "export", "path": "a.step", "bodies": ["F9.b0"]}),
            "body F9.b0 does not exist",
        ),
        (
            json!({"cmd": "export", "path": "a.step", "bodies": ["Nope"]}),
            "named Nope",
        ),
        (
            json!({"cmd": "export", "path": "a.step", "bodies": ["F2.b0", "F2.b0"]}),
            "listed more than once",
        ),
        (
            json!({"cmd": "export", "path": "a.stl", "refinement": "coarse"}),
            "invalid command",
        ),
        (
            json!({"cmd": "export", "path": "a.stl", "refinement": {"deviation": 0, "angle": 1}}),
            "greater than zero",
        ),
    ] {
        let error = command(&mut b.doc, bad.clone()).unwrap_err();
        assert!(error.contains(expected), "{bad}: {error}");
    }
    assert_eq!(b.doc.kernel().written.borrow().len(), 2);
    assert_eq!(
        Refinement::Preset(RefinementPreset::High).tolerance(),
        MeshTolerance {
            deviation: 0.01,
            angle: 8f64.to_radians(),
            face_triangles: 50_000,
            max_edge: 0.0
        }
    );
    // Numbers are taken as they are: no limit unless one is given.
    let given: Refinement =
        serde_json::from_value(json!({"deviation": 0.05, "angle": 0.2})).unwrap();
    assert_eq!(given.tolerance().face_triangles, 0);
    let limited: Refinement =
        serde_json::from_value(json!({"deviation": 0.05, "angle": 0.2, "face_triangles": 1000}))
            .unwrap();
    assert_eq!(limited.tolerance().face_triangles, 1000);
}

/// A refinement object with only `deviation` or only `angle` takes the
/// other from the Medium preset, and one with neither is refused by the
/// fields' names (mitcad#1732).
#[test]
fn a_refinement_object_takes_the_missing_tolerance_from_medium() {
    let medium = Refinement::Preset(RefinementPreset::Medium).tolerance();
    assert_eq!((medium.deviation, medium.angle), (0.03, 15f64.to_radians()));
    let tolerance = |value: serde_json::Value| {
        serde_json::from_value::<Refinement>(value).map(Refinement::tolerance)
    };
    assert_eq!(
        tolerance(json!({"deviation": 0.03})).unwrap(),
        MeshTolerance {
            deviation: 0.03,
            angle: medium.angle,
            face_triangles: 0,
            max_edge: 0.0
        }
    );
    assert_eq!(
        tolerance(json!({"deviation": 0.005, "max_edge": 2})).unwrap(),
        MeshTolerance {
            deviation: 0.005,
            angle: medium.angle,
            face_triangles: 0,
            max_edge: 2.0
        }
    );
    assert_eq!(
        tolerance(json!({"angle": 0.1})).unwrap(),
        MeshTolerance {
            deviation: medium.deviation,
            angle: 0.1,
            face_triangles: 0,
            max_edge: 0.0
        }
    );
    // Both given, and a preset, read as before.
    assert_eq!(
        tolerance(json!({"deviation": 0.05, "angle": 0.2})).unwrap(),
        MeshTolerance {
            deviation: 0.05,
            angle: 0.2,
            face_triangles: 0,
            max_edge: 0.0
        }
    );
    assert_eq!(tolerance(json!("medium")).unwrap(), medium);
    for (bad, expected) in [
        (json!({}), vec!["deviation", "angle"]),
        (json!({"max_edge": 2}), vec!["deviation", "angle"]),
        (
            json!({"tolerance": 0.03}),
            vec!["unknown field `tolerance`"],
        ),
        (json!("coarse"), vec!["coarse", "low", "medium", "high"]),
        (json!(0.03), vec!["a preset", "deviation", "angle"]),
    ] {
        let error = tolerance(bad.clone()).unwrap_err().to_string();
        for text in expected {
            assert!(error.contains(text), "{bad}: {error}");
        }
    }

    // Through the `export` command, which reaches the kernel with them.
    let mut b = block();
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": "only-deviation.obj", "refinement": {"deviation": 0.03}}),
    )
    .unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": "only-angle.obj", "refinement": {"angle": 0.1}}),
    )
    .unwrap();
    let written = b.doc.kernel().written.borrow().clone();
    assert_eq!(written.len(), 2);
    let error = command(
        &mut b.doc,
        json!({"cmd": "export", "path": "empty.obj", "refinement": {}}),
    )
    .unwrap_err();
    assert!(
        error.contains("deviation") && error.contains("angle"),
        "{error}"
    );
    assert_eq!(b.doc.kernel().written.borrow().len(), 2);
}

#[test]
fn sketches_export_to_dxf() {
    let mut b = block();
    b.doc
        .add_circle(b.sketch, [30.0, 20.0], &num(10.0))
        .unwrap();
    let dir = mitcad_model::testing::unique_temp_dir("f7");
    let path = dir.join("sketch.dxf").to_str().unwrap().to_owned();
    let result = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path}),
    )
    .unwrap();
    assert_eq!(result["entities"], 5);
    let drawing = mitcad_dxf::read_file(&path).unwrap();
    mitcad_model::testing::remove_temp(&dir);
    assert_eq!(drawing.units, mitcad_dxf::Units::Millimeters);
    let p = mitcad_dxf::Point2::new;
    assert_eq!(
        drawing.entities[0].geometry,
        mitcad_dxf::Geometry::Line {
            start: p(0.0, 0.0),
            end: p(60.0, 0.0)
        }
    );
    assert_eq!(
        drawing.entities[4].geometry,
        mitcad_dxf::Geometry::Circle {
            center: p(30.0, 20.0),
            radius: 5.0
        }
    );

    for (bad, expected) in [
        (
            json!({"cmd": "export_sketch", "sketch": "F2", "path": "a.dxf"}),
            "is not a sketch",
        ),
        (
            json!({"cmd": "export_sketch", "sketch": "F1", "path": "a.svg"}),
            ".dxf files",
        ),
        (
            json!({"cmd": "export_sketch", "sketch": "F1", "path": "a.dxf", "version": "r14"}),
            "unknown DXF version",
        ),
    ] {
        let error = command(&mut b.doc, bad.clone()).unwrap_err();
        assert!(error.contains(expected), "{bad}: {error}");
    }
    b.doc.set_marker(0).unwrap();
    let error = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "F1", "path": "a.dxf"}),
    )
    .unwrap_err();
    assert!(error.contains("has no result"), "{error}");
}

/// Construction geometry is left out of a sketch's DXF unless asked for
/// (mitcad#1445): a CAM program would cut it.
#[test]
fn sketch_dxf_leaves_construction_geometry_out_unless_asked_for() {
    use mitcad_dxf::{Geometry, Point2};

    let mut b = block();
    b.doc
        .add_circle(b.sketch, [30.0, 20.0], &num(10.0))
        .unwrap();
    // A horizontal construction line through the circle's centre, and a
    // center line beside the rectangle (not cut either).
    for line in [
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, 20], "end": [40, 20],
               "construction": true}),
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [70, 0], "end": [70, 40],
               "centerline": true}),
    ] {
        command(&mut b.doc, line).unwrap();
    }
    let dir = mitcad_model::testing::unique_temp_dir("sketch-dxf-construction");
    let path = dir.join("sketch.dxf").to_str().unwrap().to_owned();
    let p = Point2::new;

    // By default: the rectangle's four lines and the circle only.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path}),
    )
    .unwrap();
    assert_eq!(result["entities"], 5, "{result}");
    assert_eq!(result["construction"], false, "{result}");
    let drawing = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(drawing.entities.len(), 5);
    assert!(drawing.entities.iter().all(|e| e.layer == "0"));
    let lines = drawing
        .entities
        .iter()
        .filter(|e| matches!(e.geometry, Geometry::Line { .. }))
        .count();
    assert_eq!(lines, 4);
    let circle = Geometry::Circle {
        center: p(30.0, 20.0),
        radius: 5.0,
    };
    assert!(drawing.entities.iter().any(|e| e.geometry == circle));

    // With construction: the construction line and the center line too, on
    // the layer construction.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path,
               "construction": true}),
    )
    .unwrap();
    assert_eq!(result["entities"], 7, "{result}");
    assert_eq!(result["construction"], true, "{result}");
    let drawing = mitcad_dxf::read_file(&path).unwrap();
    mitcad_model::testing::remove_temp(&dir);
    assert_eq!(drawing.entities.len(), 7);
    let on_construction: Vec<&Geometry> = drawing
        .entities
        .iter()
        .filter(|e| e.layer == "construction")
        .map(|e| &e.geometry)
        .collect();
    assert_eq!(
        on_construction,
        [
            &Geometry::Line {
                start: p(20.0, 20.0),
                end: p(40.0, 20.0)
            },
            &Geometry::Line {
                start: p(70.0, 0.0),
                end: p(70.0, 40.0)
            },
        ]
    );
    assert_eq!(
        drawing.entities.iter().filter(|e| e.layer == "0").count(),
        5
    );
}

#[test]
fn sketch_geometry_maps_to_dxf_entities_once() {
    use mitcad_dxf::{Geometry, Point2};
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    let curves = [
        SegmentGeometry::Line {
            start: [0.0, 0.0],
            end: [10.0, 0.0],
        },
        SegmentGeometry::Arc {
            center: [5.0, 0.0],
            radius: 5.0,
            start_angle: 0.0,
            end_angle: PI,
        },
        SegmentGeometry::EllipseArc {
            center: [5.0, 0.0],
            major_radius: 5.0,
            minor_radius: 8.0,
            rotation: 0.0,
            start_angle: PI,
            end_angle: TAU,
        },
        SegmentGeometry::BSpline {
            degree: 2,
            poles: vec![[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]],
            weights: vec![],
            knots: vec![0.0, 1.0],
            multiplicities: vec![3, 3],
            periodic: false,
        },
    ];
    let names = ["c1", "c2", "c3", "c4"];
    let layers = ["0", "0", "0", "construction"];
    let drawing = sketch_drawing((0..4).map(|i| (layers[i], names[i], &curves[i]))).unwrap();
    assert_eq!(drawing.entities.len(), 4);
    assert_eq!(drawing.entities[3].layer, "construction");
    assert!(matches!(drawing.entities[1].geometry, Geometry::Arc { .. }));
    // The longer second axis becomes the major axis.
    let Geometry::Ellipse {
        major_axis,
        ratio,
        start_param,
        end_param,
        ..
    } = drawing.entities[2].geometry
    else {
        panic!("an ellipse");
    };
    assert!((major_axis - Point2::new(0.0, 8.0)).length() < 1e-12);
    assert!((ratio - 5.0 / 8.0).abs() < 1e-12);
    assert!((start_param - FRAC_PI_2).abs() < 1e-12 && (end_param - 3.0 * FRAC_PI_2).abs() < 1e-12);
    let Geometry::Spline(spline) = &drawing.entities[3].geometry else {
        panic!("a spline");
    };
    assert_eq!(spline.knots, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);

    let periodic = SegmentGeometry::BSpline {
        degree: 3,
        poles: vec![[0.0, 0.0]; 4],
        weights: vec![],
        knots: vec![0.0, 1.0, 2.0, 3.0, 4.0],
        multiplicities: vec![1; 5],
        periodic: true,
    };
    let error = sketch_drawing([("0", "c5", &periodic)]).unwrap_err();
    assert!(error.contains("c5: periodic splines"), "{error}");
}

/// The volume of a mock body's mesh: its bounding box.
fn bounds_volume(doc: &Document<MockKernel>, body: BodyUid) -> f64 {
    let bounds = doc.body_shape(body).unwrap().bounds.unwrap();
    (0..3).map(|i| bounds.max[i] - bounds.min[i]).product()
}

#[test]
fn export_3mf_writes_one_object_with_a_part_per_body() {
    use mitcad_model::transform::Transform;
    let mut b = block();
    let mut part = body(Some("Part"), "part", 6);
    part.color = Some([1.0, 0.0, 0.0]);
    b.doc
        .add_base_feature(BaseInput::new(vec![
            part,
            body(Some("Scan"), "mesh", 1),
            body(Some("Surface"), "sheet", 4),
        ]))
        .unwrap();
    // Body1 in a component placed twice: at its place and 100 mm along x.
    b.doc.components_from_bodies(&[b.body]).unwrap();
    let first = b.doc.assembly().occurrences[0].uid;
    let copy = b
        .doc
        .copy_occurrence(first, Some(Transform::translation([100.0, 0.0, 0.0])))
        .unwrap();
    let dir = mitcad_model::testing::unique_temp_dir("3mf");
    let path = dir.join("design.3mf").to_str().unwrap().to_owned();

    // All bodies: the sheet is left out, the mesh body goes as it is.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "refinement": "high",
               "colors": {"Body1": [0.0, 0.5, 1.0], "Scan": [0.2, 0.2, 0.2]}}),
    )
    .unwrap();
    assert_eq!(result["format"], "3mf");
    assert_eq!(result["bodies"], json!(["F2.b0", "F3.b0", "F3.b1"]));
    assert_eq!(
        result["skipped"],
        json!([{"body": "F3.b2", "name": "Surface", "reason": "a sheet body"}])
    );
    assert_eq!(result["meshes"][0]["name"], "Body1");
    assert_eq!(result["meshes"][0]["triangles"], 12);
    let volume = bounds_volume(&b.doc, b.body);
    assert!((result["meshes"][0]["volume"].as_f64().unwrap() - volume).abs() < 1e-9);
    // The refinement reaches the kernel.
    assert_eq!(
        b.doc.kernel().mesh_tolerances.borrow()[0],
        Refinement::Preset(RefinementPreset::High).tolerance()
    );

    // The file read back: one object, its parts named after the bodies, in
    // their colours (given, else imported), placed by the occurrences.
    let model = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(model.name, "design");
    let names: Vec<_> = model.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Body1", "Part", "Scan"]);
    assert_eq!(model.parts[0].color, Some([0.0, 128.0 / 255.0, 1.0]));
    assert_eq!(model.parts[1].color, Some([1.0, 0.0, 0.0]));
    assert_eq!(model.parts[2].color, Some([0.2, 0.2, 0.2]));
    let shifts: Vec<_> = model.parts[0]
        .placements
        .iter()
        .map(|p| p.translation)
        .collect();
    assert_eq!(shifts, [[0.0; 3], [100.0, 0.0, 0.0]]);
    assert_eq!(model.parts[1].placements, [mitcad_3mf::Placement::IDENTITY]);
    for part in &model.parts {
        assert!((part.mesh.volume() - 1000.0).abs() < 1e-9 || part.name == "Body1");
    }
    assert!((model.parts[0].mesh.volume() - volume).abs() < 1e-6);

    // A hidden occurrence is not printed.
    b.doc.set_occurrence_visible(copy, false).unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "bodies": ["Body1"]}),
    )
    .unwrap();
    let model = mitcad_3mf::read(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(model.parts.len(), 1);
    assert_eq!(model.parts[0].placements.len(), 1);
    assert_eq!(model.parts[0].color, None);
    // By the format given, whatever the extension.
    let other = dir.join("design.bin").to_str().unwrap().to_owned();
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": other, "format": "3mf", "bodies": ["Part"]}),
    )
    .unwrap();
    assert_eq!(
        mitcad_3mf::read(&std::fs::read(&other).unwrap())
            .unwrap()
            .parts[0]
            .name,
        "Part"
    );
    mitcad_model::testing::remove_temp(&dir);

    for (bad, expected) in [
        (
            json!({"cmd": "export", "path": path, "bodies": ["Surface"]}),
            "Surface is a sheet body; 3MF takes solids and mesh bodies",
        ),
        (
            json!({"cmd": "export", "path": path, "colors": {"Part": [2, 0, 0]}}),
            "components from 0 to 1",
        ),
        (
            json!({"cmd": "export", "path": path, "colors": {"Nope": [1, 0, 0]}}),
            "named Nope",
        ),
        (
            json!({"cmd": "export", "path": dir.join("missing/a.3mf").to_str().unwrap(), "bodies": ["Part"]}),
            "cannot write",
        ),
    ] {
        let error = command(&mut b.doc, bad.clone()).unwrap_err();
        assert!(error.contains(expected), "{bad}: {error}");
    }
}

/// The corners of the box around points.
fn box_of(points: impl IntoIterator<Item = [f64; 3]>) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in points {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    (min, max)
}

fn assert_box_near(actual: ([f64; 3], [f64; 3]), expected: ([f64; 3], [f64; 3]), what: &str) {
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4);
    assert!(
        near(actual.0, expected.0) && near(actual.1, expected.1),
        "{what}: {actual:?}, expected {expected:?}"
    );
}

#[test]
fn stl_export_places_bodies_where_the_design_shows_them() {
    use std::f64::consts::FRAC_PI_2;
    let mut b = block();
    // Body1 (60 x 40 x 20) in a component placed 100 mm along x and turned
    // a quarter about z, and a copy of it 50 mm up, not turned.
    b.doc.components_from_bodies(&[b.body]).unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "O1", "transform":
               {"translation": [100, 0, 0], "rotation": {"axis": [0, 0, 1], "angle": FRAC_PI_2}}}),
    )
    .unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1", "transform": {"translation": [0, 0, 50]}}),
    )
    .unwrap();
    let dir = mitcad_model::testing::unique_temp_dir("stl");
    let file = |name: &str| dir.join(name).to_str().unwrap().to_owned();
    let triangles = |path: &str| read_stl(&std::fs::read(path).unwrap());
    let own = {
        let bounds = b.doc.body_shape(b.body).unwrap().bounds.unwrap();
        (bounds.min, bounds.max)
    };

    // The view's places: the body's box moved by each instance's transform,
    // as the view draws it.
    let instances: Value =
        serde_json::from_str(&b.doc.query(r#"{"query": "instances"}"#).unwrap()).unwrap();
    let view: Vec<_> = instances
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["body"] == "F2.b0")
        .map(|i| {
            let m = |r: usize, c: usize| i["transform"][r][c].as_f64().unwrap();
            box_of((0..8).map(|n| {
                let corner: [f64; 3] = std::array::from_fn(|k| {
                    if (n >> k) & 1 == 1 {
                        own.1[k]
                    } else {
                        own.0[k]
                    }
                });
                std::array::from_fn(|r| (0..3).map(|c| m(r, c) * corner[c]).sum::<f64>() + m(r, 3))
            }))
        })
        .collect();
    assert_eq!(view.len(), 2);
    assert_box_near(
        view[0],
        ([60.0, 0.0, 0.0], [100.0, 60.0, 20.0]),
        "the turned occurrence",
    );
    assert_box_near(view[1], ([0.0, 0.0, 50.0], [60.0, 40.0, 70.0]), "the copy");

    // Part in the root (the mock measures no imported body).
    b.doc
        .add_base_feature(BaseInput::new(vec![body(Some("Part"), "part", 6)]))
        .unwrap();

    // STL (the design's coordinates by default): the body twice, where the
    // view shows it and the 3MF part's placements put its mesh.
    let placed = file("placed.stl");
    let result = command(
        &mut b.doc,
        json!({"cmd": "export", "path": placed, "bodies": ["Body1"]}),
    )
    .unwrap();
    assert_eq!(
        result,
        json!({"path": placed, "format": "stl", "bodies": ["F2.b0"], "recomputed": 0, "error": null})
    );
    let written = triangles(&placed);
    assert_eq!(written.len(), 24);
    for (copy, expected) in written.chunks(12).zip(&view) {
        assert_box_near(
            box_of(copy.iter().flatten().copied()),
            *expected,
            "an STL copy",
        );
    }
    let three = file("placed.3mf");
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": three, "bodies": ["Body1"]}),
    )
    .unwrap();
    let model = mitcad_3mf::read(&std::fs::read(&three).unwrap()).unwrap();
    let part = &model.parts[0];
    assert_eq!(part.placements.len(), 2);
    for (placement, expected) in part.placements.iter().zip(&view) {
        let moved = part.mesh.vertices.iter().map(|v| {
            std::array::from_fn(|r| {
                (0..3).map(|c| placement.linear[r][c] * v[c]).sum::<f64>()
                    + placement.translation[r]
            })
        });
        assert_box_near(box_of(moved), *expected, "a 3MF placement");
    }
    // Facing outwards: the turned copy's triangles enclose its volume.
    let six: f64 = written[..12]
        .iter()
        .map(|[a, b, c]| {
            a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum();
    assert!((six / 6.0 - 48000.0).abs() < 1e-2, "{}", six / 6.0);

    // One occurrence: the copy alone. Text STL: the same triangles.
    let copy = file("copy.stl");
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": copy, "bodies": ["Body1"], "occurrence": "O2", "ascii": true}),
    )
    .unwrap();
    assert!(std::fs::read(&copy).unwrap().starts_with(b"solid copy\n"));
    let text = triangles(&copy);
    assert_eq!(text.len(), 12);
    assert_box_near(
        box_of(text.iter().flatten().copied()),
        view[1],
        "the copy alone",
    );
    // All bodies of an occurrence: those it places (not Part).
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": copy, "occurrence": "Body1:2"}),
    )
    .unwrap();
    assert_eq!(triangles(&copy).len(), 12);

    // A hidden occurrence is left out, unless asked for or none is shown.
    command(
        &mut b.doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "O2", "visible": false}),
    )
    .unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": placed, "bodies": ["Body1"]}),
    )
    .unwrap();
    let shown = triangles(&placed);
    assert_eq!(shown.len(), 12);
    assert_box_near(
        box_of(shown.iter().flatten().copied()),
        view[0],
        "the shown one",
    );
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": copy, "bodies": ["Body1"], "occurrence": "O2"}),
    )
    .unwrap();
    assert_box_near(
        box_of(triangles(&copy).iter().flatten().copied()),
        view[1],
        "a hidden one asked for",
    );

    // Component coordinates: the body once, as its component has it.
    let local = file("local.stl");
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": local, "bodies": ["Body1"], "coordinates": "component"}),
    )
    .unwrap();
    let once = triangles(&local);
    assert_eq!(once.len(), 12);
    assert_box_near(
        box_of(once.iter().flatten().copied()),
        own,
        "component coordinates",
    );
    mitcad_model::testing::remove_temp(&dir);

    for (bad, expected) in [
        (
            json!({"cmd": "export", "path": "a.stl", "coordinates": "component", "occurrence": "O1"}),
            "the placements of an occurrence are in the design's coordinates",
        ),
        (
            json!({"cmd": "export", "path": "a.obj", "coordinates": "component", "occurrence": "O1"}),
            "the placements of an occurrence are in the design's coordinates",
        ),
        (
            json!({"cmd": "export", "path": "a.stl", "occurrence": "O7"}),
            "there is no occurrence O7",
        ),
        (
            json!({"cmd": "export", "path": "a.stl", "bodies": ["Part"], "occurrence": "O1"}),
            "Part is not placed in Body1:1",
        ),
        (
            json!({"cmd": "export", "path": "a.stl", "coordinates": "world"}),
            "unknown variant `world`",
        ),
    ] {
        let error = command(&mut b.doc, bad.clone()).unwrap_err();
        assert!(error.contains(expected), "{bad}: {error}");
    }
}

#[test]
fn kernel_formats_place_bodies_where_the_design_shows_them() {
    use std::f64::consts::FRAC_PI_2;
    let mut b = block();
    // Body1 in a component placed 100 mm along x and turned a quarter
    // about z, and a copy of it 50 mm up; Part in the root.
    b.doc.components_from_bodies(&[b.body]).unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": "O1", "transform":
               {"translation": [100, 0, 0], "rotation": {"axis": [0, 0, 1], "angle": FRAC_PI_2}}}),
    )
    .unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1", "transform": {"translation": [0, 0, 50]}}),
    )
    .unwrap();
    b.doc
        .add_base_feature(BaseInput::new(vec![body(Some("Part"), "part", 6)]))
        .unwrap();

    // The view's places of Body1: its instances' transforms.
    let instances: Value =
        serde_json::from_str(&b.doc.query(r#"{"query": "instances"}"#).unwrap()).unwrap();
    let view: Vec<Transform> = instances
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["body"] == "F2.b0")
        .map(|i| {
            let m = |r: usize, c: usize| i["transform"][r][c].as_f64().unwrap();
            Transform {
                linear: std::array::from_fn(|r| std::array::from_fn(|c| m(r, c))),
                translation: std::array::from_fn(|r| m(r, 3)),
            }
        })
        .collect();
    assert_eq!(view.len(), 2);
    let corner = view[0].apply_point([60.0, 40.0, 20.0]);
    assert!(
        (0..3).all(|k| (corner[k] - [60.0, 60.0, 20.0][k]).abs() < 1e-9),
        "{corner:?}"
    );
    let near = |a: &Transform, b: &Transform| {
        (0..3).all(|r| {
            (a.translation[r] - b.translation[r]).abs() < 1e-9
                && (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() < 1e-12)
        })
    };
    let last = |doc: &Document<MockKernel>| -> Vec<Vec<BodyPlacement>> {
        doc.kernel()
            .written_placements
            .borrow()
            .last()
            .cloned()
            .unwrap()
    };
    let named = |placements: &[BodyPlacement]| -> Vec<String> {
        placements.iter().map(|p| p.name.clone()).collect()
    };

    // STEP, IGES, OBJ and BRep by default (mitcad#19): Body1 at each
    // occurrence where the view shows it, Part once as it is.
    for path in ["placed.step", "placed.igs", "placed.obj", "placed.brep"] {
        let result = command(&mut b.doc, json!({"cmd": "export", "path": path})).unwrap();
        assert_eq!(result["bodies"], json!(["F2.b0", "F4.b0"]), "{path}");
        let placements = last(&b.doc);
        assert_eq!(placements.len(), 2, "{path}");
        assert_eq!(named(&placements[0]), ["Body1:1", "Body1:2"], "{path}");
        for (placement, expected) in placements[0].iter().zip(&view) {
            assert!(
                near(&placement.transform, expected),
                "{path}: {placement:?}"
            );
        }
        assert_eq!(
            placements[1],
            vec![BodyPlacement {
                transform: Transform::IDENTITY,
                name: String::new()
            }],
            "{path}"
        );
    }
    let written = b.doc.kernel().written.borrow().clone();
    assert_eq!(written[0].1, ExportFormat::Step);
    assert_eq!(
        written[0].2,
        vec![("Body1".to_owned(), None), ("Part".to_owned(), None)]
    );

    // One occurrence: its placement alone, and only the bodies it places.
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": "copy.step", "occurrence": "Body1:2"}),
    )
    .unwrap();
    let placements = last(&b.doc);
    assert_eq!(placements.len(), 1);
    assert_eq!(named(&placements[0]), ["Body1:2"]);
    assert!(near(&placements[0][0].transform, &view[1]));

    // A hidden occurrence is left out.
    command(
        &mut b.doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "O2", "visible": false}),
    )
    .unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": "shown.obj", "bodies": ["Body1"]}),
    )
    .unwrap();
    assert_eq!(named(&last(&b.doc)[0]), ["Body1:1"]);

    // Component coordinates: each body once as its component has it (the
    // export before mitcad#19).
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": "local.step", "coordinates": "component"}),
    )
    .unwrap();
    assert_eq!(last(&b.doc), vec![Vec::new(), Vec::new()]);
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": "local.brep", "bodies": ["Body1"], "coordinates": "component"}),
    )
    .unwrap();
    assert_eq!(last(&b.doc), vec![Vec::new()]);
}

/// A field's value in a USDZ file's layer (mitcad#123).
fn usd_field(layer: &mitcad_usdz::Layer, path: &str, field: &str) -> mitcad_usdz::sdf::Value {
    layer
        .field(path, field)
        .unwrap_or_else(|| panic!("{path} has no {field}"))
        .clone()
}

fn usd_target(layer: &mitcad_usdz::Layer, path: &str) -> String {
    match usd_field(layer, path, "targetPaths") {
        mitcad_usdz::sdf::Value::PathListOp(op) => op.explicit_items[0].to_string(),
        other => panic!("{path}: {other:?}"),
    }
}

/// The prims (not properties) of a layer, as paths.
fn usd_prims(layer: &mitcad_usdz::Layer) -> Vec<String> {
    layer
        .specs
        .iter()
        .filter(|s| s.spec_type == mitcad_usdz::sdf::SpecType::Prim)
        .map(|s| s.path.to_string())
        .collect()
}

#[test]
fn export_usdz_keeps_components_and_appearances() {
    use mitcad_model::transform::Transform;
    use mitcad_usdz::sdf::Value as Usd;
    let mut b = block();
    b.doc
        .add_base_feature(BaseInput::new(vec![
            body(Some("Part"), "part", 6),
            body(Some("Surface"), "sheet", 4),
            body(Some("Hidden"), "part", 6),
        ]))
        .unwrap();
    let hidden = b
        .doc
        .bodies()
        .iter()
        .find(|v| v.name == "Hidden")
        .unwrap()
        .uid;
    b.doc.set_body_visible(hidden, false).unwrap();
    // Body1 in a component placed twice: at its place and 100 mm along x;
    // red paint, one face labelled with an embedded image.
    b.doc.components_from_bodies(&[b.body]).unwrap();
    let first = b.doc.assembly().occurrences[0].uid;
    let copy = b
        .doc
        .copy_occurrence(first, Some(Transform::translation([100.0, 0.0, 0.0])))
        .unwrap();
    b.doc
        .set_body_appearance(b.body, Some("paint_red"))
        .unwrap();
    let png = b"\x89PNG\r\n\x1a\nlabel".to_vec();
    command(
        &mut b.doc,
        json!({"cmd": "create_appearance", "id": "label", "name": "Label",
               "texture": {"path": "label.png", "size": [20, 10], "projection": "planar",
                           "data": mitcad_model::internal::base64::encode(&png)}}),
    )
    .unwrap();
    let faces = b
        .doc
        .kernel()
        .faces(b.doc.body_shape(b.body).unwrap())
        .unwrap();
    let top = faces[1].names[0].clone();
    b.doc
        .set_face_appearance(b.body, std::slice::from_ref(&top), Some("label"))
        .unwrap();
    let dir = mitcad_model::testing::unique_temp_dir("usdz");
    let path = dir.join("design.usdz").to_str().unwrap().to_owned();
    let read = |path: &str| mitcad_usdz::read_package(&std::fs::read(path).unwrap()).unwrap();

    // All bodies: the hidden one is left out, the sheet is two-sided.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "refinement": "high", "colors": {"Part": [0, 0.5, 1]}}),
    )
    .unwrap();
    assert_eq!(result["format"], "usdz");
    assert_eq!(result["bodies"], json!(["F2.b0", "F3.b0", "F3.b1"]));
    assert_eq!(
        result["skipped"],
        json!([{"body": "F3.b2", "name": "Hidden", "reason": "hidden"}])
    );
    assert_eq!(
        result["materials"],
        json!(["paint_red", "label", "color 0080ff", "default look"])
    );
    assert_eq!(result["warnings"], json!([]));
    assert_eq!(
        b.doc.kernel().mesh_tolerances.borrow()[0],
        Refinement::Preset(RefinementPreset::High).tolerance()
    );
    let package = read(&path);
    let names: Vec<&str> = package.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["design.usdc", "textures/label.png"]);
    assert_eq!(package.files[1].data, png);
    let layer = &package.layer;
    let prims = usd_prims(layer);
    for expected in [
        "/design",
        "/design/Materials/paint_red",
        "/design/Materials/label/Texture",
        "/design/Materials/color_0080ff",
        "/design/Materials/default_look",
        "/design/Part",
        "/design/Surface",
        "/design/Body1_1/Body1",
        "/design/Body1_1/Body1/Faces_label",
        "/design/Body1_2/Body1",
    ] {
        assert!(
            prims.iter().any(|p| p == expected),
            "{expected} in {prims:?}"
        );
    }
    assert!(!prims.iter().any(|p| p.contains("Hidden")));
    // The copy's placement; the root turned Z up to Y up.
    match usd_field(layer, "/design/Body1_2.xformOp:transform", "default") {
        Usd::Matrix4d(m) => assert_eq!(m[3], [100.0, 0.0, 0.0, 1.0]),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        usd_field(layer, "/design.xformOp:rotateX", "default"),
        Usd::Float(-90.0)
    );
    // The body's box: 24 points, its normals, 12 triangles.
    match usd_field(layer, "/design/Body1_1/Body1.points", "default") {
        Usd::Vec3fArray(points) => {
            assert_eq!(points.len(), 24);
            let bounds = b.doc.body_shape(b.body).unwrap().bounds.unwrap();
            assert!(points.iter().all(|p| {
                (0..3).all(|k| p[k] as f64 >= bounds.min[k] && p[k] as f64 <= bounds.max[k])
            }));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        usd_field(layer, "/design/Body1_1/Body1.faceVertexCounts", "default"),
        Usd::IntArray(vec![3; 12])
    );
    // Materials: the body's paint, the label's two triangles (the second
    // face's), the colour given, the default look.
    assert_eq!(
        usd_target(layer, "/design/Body1_1/Body1.material:binding"),
        "/design/Materials/paint_red"
    );
    assert_eq!(
        usd_target(layer, "/design/Body1_1/Body1/Faces_label.material:binding"),
        "/design/Materials/label"
    );
    assert_eq!(
        usd_field(
            layer,
            "/design/Body1_1/Body1/Faces_label.indices",
            "default"
        ),
        Usd::IntArray(vec![2, 3])
    );
    assert_eq!(
        usd_target(layer, "/design/Part.material:binding"),
        "/design/Materials/color_0080ff"
    );
    assert_eq!(
        usd_target(layer, "/design/Surface.material:binding"),
        "/design/Materials/default_look"
    );
    assert_eq!(
        usd_field(layer, "/design/Surface.doubleSided", "default"),
        Usd::Bool(true)
    );
    assert!(layer.field("/design/Part.doubleSided", "default").is_none());
    // Red paint in linear light; texture coordinates by the label's planar
    // projection on its triangles, none on the others.
    let red = mitcad_model::appearance::library_appearance("paint_red").unwrap();
    match usd_field(
        layer,
        "/design/Materials/paint_red/Surface.inputs:diffuseColor",
        "default",
    ) {
        Usd::Vec3f(c) => {
            assert!(
                (f64::from(c[0]) - mitcad_model::internal::exchange::linear(red.base_color[0]))
                    .abs()
                    < 1e-6
            );
            assert!(c[0] < red.base_color[0] as f32 || red.base_color[0] >= 1.0);
        }
        other => panic!("{other:?}"),
    }
    match usd_field(layer, "/design/Body1_1/Body1.primvars:st", "default") {
        Usd::Vec2fArray(st) => {
            assert_eq!(st.len(), 36);
            let label = &st[6..12];
            let points = match usd_field(layer, "/design/Body1_1/Body1.points", "default") {
                Usd::Vec3fArray(p) => p,
                other => panic!("{other:?}"),
            };
            let corners =
                match usd_field(layer, "/design/Body1_1/Body1.faceVertexIndices", "default") {
                    Usd::IntArray(i) => i,
                    other => panic!("{other:?}"),
                };
            for (k, uv) in label.iter().enumerate() {
                let p = points[corners[6 + k] as usize];
                assert!((uv[0] - p[0] / 20.0).abs() < 1e-5 && (uv[1] - p[1] / 10.0).abs() < 1e-5);
            }
            assert!(st[..6].iter().all(|uv| *uv == [0.0, 0.0]));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        usd_field(
            layer,
            "/design/Materials/label/Texture.inputs:file",
            "default"
        ),
        Usd::Asset("textures/label.png".to_owned())
    );

    // A hidden occurrence is left out; listed bodies are written where
    // they are shown.
    b.doc.set_occurrence_visible(copy, false).unwrap();
    command(&mut b.doc, json!({"cmd": "export", "path": path})).unwrap();
    let prims = usd_prims(&read(&path).layer);
    assert!(prims.iter().any(|p| p == "/design/Body1_1/Body1"));
    assert!(!prims.iter().any(|p| p.starts_with("/design/Body1_2")));
    // The hidden occurrence asked for is written, alone.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "occurrence": "Body1:2"}),
    )
    .unwrap();
    assert_eq!(result["bodies"], json!(["F2.b0"]));
    let prims = usd_prims(&read(&path).layer);
    assert!(prims.iter().any(|p| p == "/design/Body1_2/Body1"));
    assert!(
        !prims
            .iter()
            .any(|p| p.starts_with("/design/Body1_1") || p == "/design/Part")
    );
    // A hidden body listed is written.
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "bodies": ["Hidden"]}),
    )
    .unwrap();
    assert!(
        usd_prims(&read(&path).layer)
            .iter()
            .any(|p| p == "/design/Hidden")
    );
    // The components' coordinates: each body once, in the root.
    command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "bodies": ["Body1"], "coordinates": "component"}),
    )
    .unwrap();
    let prims = usd_prims(&read(&path).layer);
    assert!(prims.iter().any(|p| p == "/design/Body1"));
    assert!(!prims.iter().any(|p| p.starts_with("/design/Body1_1")));

    // A texture file relative to the design's folder: found with the
    // folder, else the base colour is written with a warning.
    std::fs::write(dir.join("wood.png"), &png).unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "create_appearance", "id": "wood", "texture": {"path": "wood.png"}}),
    )
    .unwrap();
    b.doc.set_body_appearance(b.body, Some("wood")).unwrap();
    let result = command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "bodies": ["Body1"]}),
    )
    .unwrap();
    assert!(
        result["warnings"][0]
            .as_str()
            .unwrap()
            .contains("not known"),
        "{result}"
    );
    let result = command(
        &mut b.doc,
        json!({"cmd": "export", "path": path, "bodies": ["Body1"],
               "design_folder": dir.to_str().unwrap()}),
    )
    .unwrap();
    assert_eq!(result["warnings"], json!([]));
    let names: Vec<String> = read(&path).files.iter().map(|f| f.name.clone()).collect();
    assert!(names.contains(&"textures/wood.png".to_owned()), "{names:?}");
    mitcad_model::testing::remove_temp(&dir);

    for (bad, expected) in [
        (
            json!({"cmd": "export", "path": "a.usdz", "bodies": ["Nope"]}),
            "named Nope",
        ),
        (
            json!({"cmd": "export", "path": dir.join("missing/a.usdz").to_str().unwrap(), "bodies": ["Part"]}),
            "cannot write",
        ),
    ] {
        let error = command(&mut b.doc, bad.clone()).unwrap_err();
        assert!(error.contains(expected), "{bad}: {error}");
    }
}

#[test]
fn import_file_keeps_a_step_assembly_as_components() {
    let mut doc = Document::new(MockKernel::default());
    let instance = |part: usize, x: f64| ImportInstance {
        part,
        name: None,
        transform: Transform::translation([x, 0.0, 0.0]),
    };
    let mut plate = body(Some("Plate"), "plate", 6);
    plate.color = Some([0.0, 0.5, 1.0]);
    let assembly = ImportAssembly {
        parts: vec![
            ImportPart {
                name: Some("Plate".to_owned()),
                bodies: vec![plate],
                instances: Vec::new(),
            },
            // A part with an empty body beside its solid.
            ImportPart {
                name: Some("Pin".to_owned()),
                bodies: vec![body(Some("Pin"), "pin", 3), body(None, "nothing", 0)],
                instances: Vec::new(),
            },
            // A sub-assembly placing the pin twice.
            ImportPart {
                name: Some("Arm".to_owned()),
                bodies: Vec::new(),
                instances: vec![instance(1, 0.0), instance(1, 5.0)],
            },
            // A part without geometry.
            ImportPart {
                name: Some("Label".to_owned()),
                bodies: Vec::new(),
                instances: Vec::new(),
            },
        ],
        roots: vec![
            instance(0, 0.0),
            instance(0, 100.0),
            instance(0, 200.0),
            instance(0, 300.0),
            instance(2, 50.0),
            instance(2, 150.0),
            instance(3, 0.0),
        ],
    };
    let path = "/models/machine.step";
    doc.kernel()
        .assemblies
        .borrow_mut()
        .insert(path.to_owned(), assembly);
    // The same file flattened, as the kernel reads it for one base feature.
    doc.kernel().files.borrow_mut().insert(
        path.to_owned(),
        (0..8).map(|_| body(Some("Plate"), "plate", 6)).collect(),
    );
    let result = command(&mut doc, json!({"cmd": "import_file", "path": path})).unwrap();
    assert_eq!(result["structure"], "components");
    let components = result["components"].as_array().unwrap();
    let summary: Vec<(String, usize, usize)> = components
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_owned(),
                c["occurrences"].as_array().unwrap().len(),
                c["bodies"].as_u64().unwrap() as usize,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Plate".to_owned(), 4, 1),
            ("Arm".to_owned(), 2, 0),
            ("Pin".to_owned(), 2, 1)
        ]
    );
    assert_eq!(result["occurrences"], 8);
    assert_eq!(result["bodies"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["warnings"],
        json!(["Pin: body 1 is empty: left out", "Label is empty: left out"])
    );
    assert_eq!(doc.undo_label(), Some("Import machine.step"));
    // Each part once: two body definitions, eight placed instances (four
    // plates and the two pins of each of the two arms).
    assert_eq!(doc.bodies().len(), 2);
    let instances = doc.instances();
    assert_eq!(instances.len(), 8);
    let plate = doc
        .bodies()
        .into_iter()
        .find(|b| b.name == "Plate")
        .unwrap();
    assert_eq!(instances.iter().filter(|i| i.body == plate.uid).count(), 4);
    let mut xs: Vec<f64> = instances
        .iter()
        .filter(|i| i.body != plate.uid)
        .map(|i| i.transform.translation[0])
        .collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs, [50.0, 55.0, 150.0, 155.0]);
    // The design file holds each part's B-rep once.
    let file = doc.to_json();
    assert_eq!(file.matches("\"brep\"").count(), 2, "{file}");
    // One undo step takes it all back.
    doc.undo();
    assert!(doc.bodies().is_empty());
    assert!(doc.assembly().is_empty());

    // One base feature: the file as the kernel flattens it.
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": path, "structure": "base"}),
    )
    .unwrap();
    assert!(result.get("structure").is_none(), "{result}");
    assert_eq!(result["bodies"].as_array().unwrap().len(), 8);
    assert_eq!(doc.bodies().len(), 8);
    let error = command(
        &mut doc,
        json!({"cmd": "import_file", "path": path, "structure": "parts"}),
    )
    .unwrap_err();
    assert!(error.contains("unknown variant"), "{error}");
}

/// The occurrences that place a component, as (parent, transform).
fn placements(
    doc: &Document<MockKernel>,
    component: mitcad_model::ids::ComponentUid,
) -> Vec<(mitcad_model::ids::ComponentUid, Transform)> {
    doc.assembly()
        .occurrences
        .iter()
        .filter(|o| o.component == component)
        .map(|o| (o.parent, o.transform))
        .collect()
}

fn component_named(
    doc: &Document<MockKernel>,
    name: &str,
) -> Option<mitcad_model::ids::ComponentUid> {
    doc.assembly()
        .components
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.uid)
}

#[test]
fn import_file_wraps_a_file_in_a_component_named_after_it() {
    use mitcad_model::ids::ComponentUid;
    let mut doc = Document::new(MockKernel::default());
    let single = "/models/bracket.step";
    doc.kernel()
        .files
        .borrow_mut()
        .insert(single.to_owned(), vec![body(Some("Bracket"), "bracket", 6)]);
    // An assembly placing one part twice.
    let instance = |x: f64| ImportInstance {
        part: 0,
        name: None,
        transform: Transform::translation([x, 0.0, 0.0]),
    };
    let assembly = "/models/pins.step";
    doc.kernel().assemblies.borrow_mut().insert(
        assembly.to_owned(),
        ImportAssembly {
            parts: vec![ImportPart {
                name: Some("Pin".to_owned()),
                bodies: vec![body(Some("Pin"), "pin", 3)],
                instances: Vec::new(),
            }],
            roots: vec![instance(0.0), instance(50.0)],
        },
    );

    // An empty design takes the file as it is: no component around it.
    let result = command(&mut doc, json!({"cmd": "import_file", "path": single})).unwrap();
    assert!(result.get("component").is_none(), "{result}");
    assert!(doc.assembly().is_empty());
    let base = doc.feature(FeatureUid(1)).unwrap();
    assert_eq!(base.component, ComponentUid::ROOT);
    doc.undo();
    assert!(!doc.has_content());

    // A design with a body.
    doc.add_base_feature(BaseInput::new(vec![body(Some("Block"), "box", 6)]))
        .unwrap();
    assert!(doc.import_wraps_by_default(single));
    assert!(doc.import_wraps_by_default("/models/bracket.IGS"));
    assert!(doc.import_wraps_by_default("/models/bracket.brp"));
    assert!(!doc.import_wraps_by_default("/models/scan.stl"));

    // A single-part file: its base feature in a new component named after
    // the file, placed once in the root at the identity.
    let result = command(&mut doc, json!({"cmd": "import_file", "path": single})).unwrap();
    assert_eq!(result["component"]["name"], "bracket", "{result}");
    let bracket = component_named(&doc, "bracket").expect("the component bracket");
    assert_eq!(result["component"]["uid"], bracket.to_string());
    assert_eq!(
        placements(&doc, bracket),
        [(ComponentUid::ROOT, Transform::IDENTITY)]
    );
    let feature: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    assert_eq!(doc.feature(feature).unwrap().component, bracket);
    assert_eq!(doc.component_bodies(bracket).len(), 1);
    assert_eq!(doc.undo_label(), Some("Import bracket.step"));
    // One undo step takes the component with it.
    doc.undo();
    assert!(doc.assembly().is_empty());
    assert_eq!(doc.bodies().len(), 1);

    // An assembly: its top instances are placed in the new component.
    let result = command(&mut doc, json!({"cmd": "import_file", "path": assembly})).unwrap();
    assert_eq!(result["structure"], "components");
    assert_eq!(result["component"]["name"], "pins", "{result}");
    let pins = component_named(&doc, "pins").expect("the component pins");
    assert_eq!(
        placements(&doc, pins),
        [(ComponentUid::ROOT, Transform::IDENTITY)]
    );
    let pin = component_named(&doc, "Pin").expect("the part's component");
    let mut xs: Vec<f64> = placements(&doc, pin)
        .into_iter()
        .map(|(parent, transform)| {
            assert_eq!(parent, pins);
            transform.translation[0]
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs, [0.0, 50.0]);
    assert_eq!(doc.undo_label(), Some("Import pins.step"));
    // The pins where the file places them, in the design.
    assert_eq!(
        doc.instances()
            .iter()
            .filter(|i| i.component == pin)
            .count(),
        2
    );
    // The same file again: the name gets a suffix.
    let result = command(&mut doc, json!({"cmd": "import_file", "path": assembly})).unwrap();
    assert_eq!(result["component"]["name"], "pins (1)", "{result}");
    doc.undo();
    doc.undo();
    assert!(doc.assembly().is_empty());

    // wrap false: into the active component, as before.
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": single, "wrap": false}),
    )
    .unwrap();
    assert!(result.get("component").is_none(), "{result}");
    assert!(doc.assembly().is_empty());
    let feature: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    assert_eq!(doc.feature(feature).unwrap().component, ComponentUid::ROOT);
    doc.undo();
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": assembly, "wrap": false}),
    )
    .unwrap();
    assert!(result.get("component").is_none(), "{result}");
    assert!(component_named(&doc, "pins").is_none());
    let pin = component_named(&doc, "Pin").expect("the part's component");
    assert!(
        placements(&doc, pin)
            .iter()
            .all(|(parent, _)| *parent == ComponentUid::ROOT)
    );
    doc.undo();

    // wrap true into an empty design, and for a mesh file.
    doc.undo();
    assert!(!doc.has_content());
    doc.kernel()
        .files
        .borrow_mut()
        .insert("scan.stl".to_owned(), vec![body(Some("scan"), "mesh", 1)]);
    let result = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "scan.stl", "wrap": true}),
    )
    .unwrap();
    assert_eq!(result["component"]["name"], "scan", "{result}");
    // A failed read makes no component.
    let error = command(
        &mut doc,
        json!({"cmd": "import_file", "path": "missing.step", "wrap": true}),
    )
    .unwrap_err();
    assert!(error.contains("cannot read missing.step"), "{error}");
    assert_eq!(doc.assembly().components.len(), 1);
}

#[test]
fn file_structure_query_finds_step_assemblies() {
    let dir = mitcad_model::testing::unique_temp_dir("file-structure");
    let write = |name: &str, text: &str| {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path.to_str().unwrap().to_owned()
    };
    let usage = "#7=NEXT_ASSEMBLY_USAGE_OCCURRENCE('1','','',#1,#2,$);\n";
    let assembly = write(
        "machine.step",
        &format!("ISO-10303-21;\n{usage}{usage}END-ISO-10303-21;\n"),
    );
    let once = write(
        "one.stp",
        &format!("ISO-10303-21;\n{usage}END-ISO-10303-21;\n"),
    );
    let other = write("machine.igs", usage);
    let doc = Document::new(MockKernel::default());
    for (path, expected) in [(&assembly, true), (&once, false), (&other, false)] {
        let result: Value = serde_json::from_str(
            &doc.query(&json!({"query": "file_structure", "path": path}).to_string())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["assembly"], expected, "{path}");
    }
    // A name that spans two blocks of the scan is found too.
    let long = format!(
        "ISO-10303-21;\n{}{usage}{usage}",
        " ".repeat((1 << 20) - 20)
    );
    let spanning = write("long.step", &long);
    assert!(file_has_assembly(&spanning).unwrap());
    assert!(file_has_assembly(dir.join("missing.step").to_str().unwrap()).is_err());
    mitcad_model::testing::remove_temp(&dir);
}
