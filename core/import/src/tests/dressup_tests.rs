// SPDX-License-Identifier: MIT
//! Fillets, chamfers, shells, replaced and moved faces, and embossing.

use super::*;
use mitcad_model_api::DocumentApi;

#[test]
fn edges_the_decoder_found_by_their_names_come_first() {
    let block_items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(block_items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (a, b) = shapes(&reference)[0].edges[0].clone();
    let p = edge_point(&a, &b);
    let names = json!({"recipe": "edge", "entities": [[{"tag": "3", "kind": 0, "ops": [301]}],
                       [{"tag": "4", "kind": 0, "ops": [301]}]]});
    let replay = |edge: Value| {
        let mut items = block_items.clone();
        let set = json!({"radius": param("d5", "2 mm", 0.2, "mm"), "edges": [edge]});
        let fillet = json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
                            "detail": {"edgeSets": [set]}});
        items.push(fillet);
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        )
    };
    // Found in the history: the decoder wrote its geometry (cm).
    let report = replay(json!({"kind": "edge", "length": 0.1,
        "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0],
        "_f3d": {"found": "BREP.x.smbh before state 4", "recipe": "edge", "entities": names["entities"]}}));
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(
        item.note.as_deref(),
        Some("edges found by their names in the file"),
        "{}",
        report.text()
    );
    // Not found: the edges are left to the history (here there is none).
    let report = replay(json!({"kind": "edge",
        "_f3d": {"found": "a face of the edge is not in the history state", "recipe": "edge",
                 "entities": names["entities"]}}));
    let item = &report.items[2];
    assert_ne!(item.outcome, Outcome::Parametric);
    assert!(
        report.text().contains("no history to find them"),
        "{}",
        report.text()
    );
    assert!(
        report
            .text()
            .contains("edge set 0, input 0: decoder_unresolved")
    );
    assert!(
        report
            .text()
            .contains("a face of the edge is not in the history state")
    );
    // The decoder succeeded, but the replay does not have that edge:
    // keep this separate from a recipe the decoder could not resolve.
    let report = replay(json!({"kind": "edge", "length": 0.1,
        "mid_point": [1000.0, 1000.0, 1000.0],
        "_f3d": {"found": "BREP.x.smbh before state 4", "recipe": "edge", "entities": names["entities"]}}));
    assert!(
        report
            .text()
            .contains("edge set 0, input 0: missing_in_replay")
    );
    assert!(report.text().contains("BREP.x.smbh before state 4"));
}

/// An edge the file names whose faces its history no longer joins (the
/// decoder: "no edge between the faces named") is no input of the file's
/// operation: the others come first, noted as left out and not read; an
/// edge the decoder did not resolve for another reason is not left out.
#[test]
fn edges_the_file_no_longer_has_are_left_out() {
    let block_items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(block_items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (a, b) = shapes(&reference)[0].edges[0].clone();
    let p = edge_point(&a, &b);
    let names = json!([[{"tag": "3", "kind": 0, "ops": [301]}],
                       [{"tag": "4", "kind": 0, "ops": [301]}]]);
    let found = json!({"kind": "edge", "length": 0.1,
        "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0],
        "_f3d": {"object_id": 7, "found": "BREP.x.smbh before state 4", "recipe": "edge",
                 "entities": names}});
    let replay = |why: &str| {
        let lost = json!({"kind": "edge",
            "_f3d": {"object_id": 8, "found": why, "recipe": "edge", "entities": names}});
        let mut items = block_items.clone();
        let set = json!({"radius": param("d5", "2 mm", 0.2, "mm"), "edges": [found, lost]});
        items.push(
            json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
                          "detail": {"edgeSets": [set]}}),
        );
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        )
    };
    for why in [
        "no edge between the faces named",
        "a face of the edge is not in the history state",
    ] {
        let report = replay(why);
        let item = &report.items[2];
        assert_eq!(
            item.outcome,
            Outcome::Parametric,
            "{why}: {}",
            report.text()
        );
        assert_eq!(
            item.note.as_deref(),
            Some("edges found by their names in the file; 1 edge the file no longer has left out"),
            "{why}"
        );
        assert_eq!(item.input_provenance, Some(InputProvenance::Inferred));
    }
    // Another reason (the names fit several edges): the set is not
    // reduced.
    let report = replay("2 edges fit the names");
    assert_ne!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
}

/// The edges of a face a chamfer's set selects (the decoder turned the face
/// into its edges: a `bounded_face` recipe found "an edge of a face") are
/// named by the file as the edges of an edge recipe are: the chamfer's
/// inputs are read; an edge of that recipe the decoder did not take from a
/// face is not.
#[test]
fn a_chamfer_of_a_faces_edges_is_read() {
    let block_items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(block_items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (a, b) = shapes(&reference)[0].edges[0].clone();
    let p = edge_point(&a, &b);
    let entities = json!([[{"tag": "3", "kind": 0, "ops": [301]}],
                          [{"tag": "4", "kind": 0, "ops": [301]}]]);
    let provenance = |recipe: &str, found: &str| {
        let edge = json!({"kind": "edge", "length": 0.1,
            "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0],
            "_f3d": {"object_id": 7, "found": found, "recipe": recipe,
                     "entities": entities}});
        let mut items = block_items.clone();
        items.push(
            json!({"index": 2, "name": "Chamfer1", "objectType": "ChamferFeature",
            "detail": {"chamferType": "EqualDistanceChamferType",
                       "cornerType": "ChamferCornerType",
                       "edgeSets": [{"edges": [edge], "isTangentChain": true,
                                     "distance": param("d5", "2 mm", 0.2, "mm")}]}}),
        );
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        assert_eq!(
            report.items[2].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        report.items[2].input_provenance
    };
    let state = "BREP.x.smbh before state 4";
    assert_eq!(provenance("edge", state), Some(InputProvenance::Read));
    assert_eq!(
        provenance("bounded_face", &format!("{state} (an edge of a face)")),
        Some(InputProvenance::Read)
    );
    assert_ne!(
        provenance("bounded_face", state),
        Some(InputProvenance::Read)
    );
}

#[test]
fn fillet_and_chamfer_options_come_in_as_their_definitions() {
    // The block's first edge, fingerprinted as external dumps do it (cm).
    let block_items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(block_items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (a, b) = shapes(&reference)[0].edges[0].clone();
    let p = edge_point(&a, &b);
    let edge = json!({"kind": "edge", "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0],
                      "length": 0.1});
    let fillet = |set: Value| {
        let mut set = set;
        set["edges"] = json!([edge]);
        json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
               "detail": {"edgeSets": [set]}})
    };
    let radius = param("d5", "2 mm", 0.2, "mm");
    let three = param("d6", "3 mm", 0.3, "mm");
    for item in [
        fillet(
            json!({"radius": radius, "continuity": "CurvatureSurfaceContinuityType",
                      "tangencyWeight": param("d7", "1.5", 1.5, "")}),
        ),
        fillet(
            json!({"_type": "VariableRadiusFilletEdgeSet", "startRadius": radius,
                      "endRadius": radius, "midRadii": [three], "midPositions": [0.25]}),
        ),
        fillet(
            json!({"_type": "AsymmetricFilletEdgeSet", "offsetOne": radius,
                      "offsetTwo": three, "isFlipped": true}),
        ),
        json!({"index": 2, "name": "Chamfer1", "objectType": "ChamferFeature",
               "detail": {"edgeSets": [{"edges": [edge], "distance": radius}],
                          "cornerType": "MiterCornerType"}}),
    ] {
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        let mut items = block_items.clone();
        items.push(item);
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        let outcome = &report.items[2];
        assert_eq!(outcome.outcome, Outcome::Parametric, "{}", report.text());
        let def: Value = serde_json::from_str::<Value>(
            &doc.query(&format!(
                r#"{{"query": "feature", "uid": "{}"}}"#,
                outcome.features[0]
            ))
            .unwrap(),
        )
        .unwrap()["def"]
            .clone();
        let value = |slot: &Value| {
            let parameters = doc.parameters();
            match slot.as_str() {
                Some(name) => parameters
                    .get(parameters.find(name).unwrap())
                    .map(|p| p.value()),
                None => slot.as_f64(),
            }
        };
        let set = &def["sets"][0];
        match def["type"].as_str().unwrap() {
            "chamfer" => assert_eq!(def["corner"], "miter"),
            _ if set["continuity"] == "curvature" => {
                assert_eq!(value(&set["tangency_weight"]), Some(1.5));
            }
            _ if set["size"]["type"] == "variable" => {
                assert_eq!(set["size"]["mid"][0]["position"], 0.25);
                assert_eq!(value(&set["size"]["mid"][0]["radius"]), Some(3.0));
            }
            _ => {
                assert_eq!(set["size"]["type"], "asymmetric");
                assert_eq!(value(&set["size"]["distance1"]), Some(2.0));
                assert_eq!(value(&set["size"]["distance2"]), Some(3.0));
                assert_eq!(set["size"]["flip"], true);
            }
        }
        // Mitcad's own shapes: matched against the history more loosely.
        assert_eq!(
            Candidate::new(def.clone()).tolerance(),
            history::CONVENTIONS
        );
    }
    let plain = json!({"type": "fillet", "sets": [{"size": {"type": "constant", "radius": 2}}]});
    assert_eq!(Candidate::new(plain).tolerance(), history::RELATIVE);
}

/// A distance and angle chamfer whose face the file does not say (no
/// `isFlipped`, as the `.ipt` import reads it): the distance may lie on
/// either face of the edge, and the history state chooses (mitcad#235).
#[test]
fn a_distance_and_angle_chamfers_face_comes_from_the_history() {
    let block_items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut block = Document::new(TestKernel::default());
    import_design(
        &mut block,
        &timeline_dump(block_items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (a, b) = shapes(&block)[0].edges[0].clone();
    let p = edge_point(&a, &b);
    let edge = json!({"kind": "edge", "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0],
                      "length": 0.1});
    let mut items = block_items.clone();
    items.push(
        json!({"index": 2, "name": "Chamfer1", "objectType": "ChamferFeature",
        "detail": {"edgeSets": [{"_type": "DistanceAndAngleChamferEdgeSet", "edges": [edge],
                                 "distance": param("d5", "2 mm", 0.2, "mm"),
                                 "angle": param("d6", "15 deg", 0.2617993877991494, "rad")}]}}),
    );
    let full = timeline_dump(items);
    let import = |geometry: &mut dyn StoredGeometry<MockShape>| {
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        let report = import_design(&mut doc, &full, geometry, &Options::default());
        let uid = report.items[2].features[0].clone();
        let def: Value = serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone();
        (doc, report, def)
    };
    // Without the history: the distance on the edge's first face.
    let (mut reference, report, def) = import(&mut NoGeometry);
    assert_eq!(report.items[2].outcome, Outcome::Parametric);
    assert_ne!(def["flip"], true, "{def}");
    let before = shapes(&reference);
    // The file's state: the distance on the other face.
    let mut flipped = def.clone();
    flipped["flip"] = json!(true);
    let uid: mitcad_model::FeatureUid = report.items[2].features[0].parse().unwrap();
    reference
        .edit_feature(uid, &serde_json::from_value(flipped).unwrap())
        .unwrap();
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&block), shapes(&reference)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    assert_ne!(before, shapes(&reference));
    let (_, report, def) = import(&mut geometry);
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true), "{}", report.text());
    assert_eq!(def["flip"], true, "{def}");
}

/// A replace face of the block's top onto the XY plane, as external dumps give
/// it (no source faces), and the history with the file's result.
fn replace_face_history() -> (Vec<Value>, TestGeometry, String) {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "ReplaceFace1", "objectType": "ReplaceFaceFeature",
               "detail": {"targetFaces": [{"kind": "construction_plane", "name": "XY", "origin": "XY"}],
                          "isTangentChain": true}}),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let top = block[0]
        .faces
        .iter()
        .find(|f| f.to_string().starts_with("F2:end("))
        .expect("the block's top")
        .to_string();
    add(
        &mut reference,
        json!({"type": "replace_face", "body": "F2.b0", "faces": [top], "target": "xy",
               "tangent_chain": false}),
    );
    let geometry = TestGeometry {
        states: vec![Vec::new(), block, shapes(&reference)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    (items, geometry, top)
}

#[test]
fn replace_faces_come_in_with_the_faces_the_history_lost() {
    let (items, mut geometry, top) = replace_face_history();
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items.clone()),
        &mut geometry,
        &Options::default(),
    );
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(item.verified, Some(true));
    let def: Value = serde_json::from_str::<Value>(
        &doc.query(&format!(
            r#"{{"query": "feature", "uid": "{}"}}"#,
            item.features[0]
        ))
        .unwrap(),
    )
    .unwrap()["def"]
        .clone();
    assert_eq!(def["type"], "replace_face");
    assert_eq!(def["faces"], json!([top]));
    assert_eq!(def["target"], "xy");
    assert_eq!(def["tangent_chain"], false);

    // Without a history the faces are not known: the file's bodies instead.
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
    let note = report.items[2].note.clone().unwrap_or_default();
    assert!(
        note.contains("source faces were not decoded"),
        "{}",
        report.text()
    );
    // Replace faces follow Mitcad's conventions: matched more loosely.
    assert_eq!(
        Candidate::new(json!({"type": "replace_face"})).tolerance(),
        history::CONVENTIONS
    );
}

#[test]
fn a_fillet_with_edge_references_the_file_lost_says_so() {
    // The file names an edge that is not in the model where the fillet was
    // made (no edge lies between the faces it names): the application that
    // wrote the file keeps the geometry the item had before. The history's
    // state is not what any definition of the item's inputs makes (the
    // state's radius is another), so it falls back, and its note says why.
    use mitcad_model::FilletSize;
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let kernel = TestKernel {
        edge_points: true,
        ..TestKernel::default()
    };
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let data = kernel.brep_data(&block[0]).unwrap();
    let (stored, _) = kernel.import_brep(FeatureUid(9), &data, 0).unwrap();
    let edge = EdgeName::new(stored.faces[0].clone(), stored.faces[1].clone());
    let set = FilletSet {
        edges: std::slice::from_ref(&edge),
        faces: &[],
        size: FilletSize::Constant { radius: 3.0 },
        tangent_chain: true,
        curvature: false,
        weight: 0.0,
    };
    let filleted = kernel
        .fillet(FeatureUid(10), &stored, &[set], true)
        .unwrap();
    let lost = |found: &str| {
        json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
        "detail": {"edgeSets": [{"radius": param("d5", "2 mm", 0.2, "mm"), "edges": [{
            "kind": "edge",
            "_f3d": {"found": found, "recipe": "edge",
                     "entities": [[{"tag": "3", "kind": 0, "ops": [301]}],
                                  [{"tag": "4", "kind": 0, "ops": [301]}]]}
        }]}]}})
    };
    let import = |found: &str| {
        let mut with = items.clone();
        with.push(lost(found));
        let mut geometry = TestGeometry {
            states: vec![Vec::new(), block.clone(), vec![filleted.clone()]],
            items: HashMap::from([(1, 1), (2, 2)]),
            ..TestGeometry::default()
        };
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        import_design(
            &mut doc,
            &timeline_dump(with),
            &mut geometry,
            &Options::default(),
        )
    };
    for found in [
        "no edge between the faces named",
        "a face of the edge is not in the history state",
    ] {
        let report = import(found);
        let item = &report.items[2];
        assert_eq!(item.outcome, Outcome::Fallback, "{}", report.text());
        let note = item.note.clone().unwrap_or_default();
        assert!(
            note.starts_with("1 of its 1 edge references are lost in the file"),
            "{note}"
        );
    }
    // Other reasons the decoder found no edge are not the file's.
    let report = import("2 edges fit the names");
    let note = report.items[2].note.clone().unwrap_or_default();
    assert!(!note.contains("are lost in the file"), "{note}");
}

#[test]
fn a_shell_reports_when_its_body_is_inferred_from_the_only_solid() {
    let flags = json!({"isTangentChain": true, "shellType": "RoundedOffsetShellType"});
    for (body, with_flags, provenance) in [
        (
            json!({"kind": "body", "objectType": "BRepBody",
                   "_f3d": {"producer": 1, "body_index": 0}}),
            false,
            InputProvenance::Unknown,
        ),
        // The body, thickness, type and tangent-chain flag all decoded.
        (
            json!({"kind": "body", "objectType": "BRepBody",
                   "_f3d": {"producer": 1, "body_index": 0}}),
            true,
            InputProvenance::Read,
        ),
        (
            json!({"kind": "body", "objectType": "BRepBody"}),
            false,
            InputProvenance::Inferred,
        ),
    ] {
        let mut detail = json!({
            "inputEntities": [body],
            "insideThickness": {"kind": "parameter", "value": 0.1}
        });
        if with_flags {
            for (key, value) in flags.as_object().unwrap() {
                detail[key] = value.clone();
            }
        }
        let items = vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
            json!({"index": 2, "objectType": "ShellFeature", "detail": detail}),
        ];
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &timeline_dump(items));
        let shell = &report.items[2];
        assert_eq!(shell.outcome, Outcome::Parametric, "{}", report.text());
        assert_eq!(shell.input_provenance, Some(provenance));
        let feature: Value = serde_json::from_str(
            &doc.query(&json!({"query": "feature", "uid": shell.features[0]}).to_string())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(feature["def"]["body"], "F2.b0");
    }
}

/// A shell of the block whose removed face the dump does not name, only
/// how many it removes (the `.ipt` import's): the face the next history
/// state lost.
#[test]
fn a_shell_removes_the_faces_the_history_lost() {
    let shell = json!({"index": 2, "name": "Shell1", "objectType": "ShellFeature",
        "detail": {"inputEntities": [{"kind": "body", "objectType": "BRepBody",
                                      "_f3d": {"producer": 1, "body_index": 0}}],
                   "insideThickness": {"kind": "parameter", "value": 0.1},
                   "isTangentChain": false, "shellType": "SharpOffsetShellType",
                   "_ipt_removed_faces": 1}});
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        shell,
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let top = block[0]
        .faces
        .iter()
        .find(|f| f.to_string().starts_with("F2:end("))
        .expect("the block's top")
        .to_string();
    add(
        &mut reference,
        json!({"type": "shell", "body": "F2.b0", "faces": [top], "inside": 1,
               "tangent_chain": false}),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), block, shapes(&reference)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items.clone()),
        &mut geometry,
        &Options::default(),
    );
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    let def: Value = serde_json::from_str::<Value>(
        &doc.query(&format!(
            r#"{{"query": "feature", "uid": "{}"}}"#,
            item.features[0]
        ))
        .unwrap(),
    )
    .unwrap()["def"]
        .clone();
    assert_eq!(def["faces"], json!([top]));

    // Without a history the faces are not known.
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
}

/// An emboss (mitcad#458) comes in as Mitcad's: the decoded profiles of
/// its sketch on the face the file names, sunk where the file's depth is
/// negative; profiles moved across the face are not translated.
#[test]
fn an_emboss_comes_in_on_its_face() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let base = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(base.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let top = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "end")
        .cloned()
        .expect("a top face");
    let point = face_point(&top).map(|x| x / 10.0);
    // Sketch3: the rectangle again, without its dimension.
    let mut sketch = rectangle_sketch(2);
    sketch["detail"]
        .as_object_mut()
        .unwrap()
        .remove("dimensions");
    let emboss = |across: f64| {
        json!({"index": 3, "name": "Emboss1", "objectType": "EmbossFeature", "detail": {
            "profile": [{"kind": "profile", "sketch": "Sketch3", "sketch_timeline_index": 2}],
            "inputFaces": [{"kind": "face", "objectType": "BRepFace",
                "geometry": {"type": "Plane"}, "point_on_face": point,
                "_f3d": {"recipe": "bounded_face"}}],
            "depth": param("d6", "-1 mm", -0.1, "mm"),
            "horizontalDistance": param("d7", "0 mm", across, "mm"),
            "verticalDistance": param("d8", "0 mm", 0.0, "mm"),
            "rotationAngle": param("d9", "0.0 deg", 0.0, "deg")}})
    };
    let run = |across: f64| {
        let mut items = base.clone();
        items.extend([sketch.clone(), emboss(across)]);
        let mut doc = Document::new(kernel());
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        let query = json!({"query": "feature", "uid": "F4"}).to_string();
        let def = doc
            .query(&query)
            .ok()
            .and_then(|a| serde_json::from_str::<Value>(&a).ok())
            .map(|a| a["def"].clone())
            .filter(|d| d["type"] == "emboss");
        (report, def)
    };
    let (report, def) = run(0.0);
    let item = report.items.iter().find(|i| i.name == "Emboss1").unwrap();
    assert!(
        matches!(item.outcome, Outcome::Parametric | Outcome::Partial),
        "{}",
        report.text()
    );
    let def = def.expect("an emboss");
    assert_eq!(def["mode"], "sink", "{def}");
    assert_eq!(
        def["face"],
        json!({"body": "F2.b0", "face": top.to_string()})
    );
    assert_eq!(def["profiles"][0]["sketch"], "F3", "{def}");

    // Moved across the face: no counterpart.
    let (report, def) = run(0.5);
    let item = report.items.iter().find(|i| i.name == "Emboss1").unwrap();
    assert!(
        item.note
            .as_deref()
            .is_some_and(|n| n.contains("moved across the face")),
        "{}",
        report.text()
    );
    assert!(def.is_none());
}

#[test]
fn moved_faces_come_in_as_a_move_face() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let base = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(base.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let top = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "end")
        .cloned()
        .expect("a top face");
    let point = face_point(&top).map(|x| x / 10.0);
    // The top moved up 0.5 cm, as the decoder gives it (the matrix in cm).
    let moved = |transform: Value| {
        let mut detail = json!({"faces": [{"kind": "face", "objectType": "BRepFace",
            "geometry": {"type": "Plane"}, "point_on_face": point,
            "_f3d": {"recipe": "bounded_face"}}]});
        if !transform.is_null() {
            detail["transform"] = transform;
        }
        json!({"index": 2, "name": "MoveFace1", "objectType": "MoveFaceFeature",
               "detail": detail})
    };
    let run = |transform: Value| {
        let mut items = base.clone();
        items.push(moved(transform));
        let mut doc = Document::new(kernel());
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        let query = json!({"query": "feature", "uid": "F3"}).to_string();
        let def = doc
            .query(&query)
            .ok()
            .and_then(|a| serde_json::from_str::<Value>(&a).ok())
            .map(|a| a["def"].clone())
            .filter(|d| d["type"] == "move_face");
        (report, def)
    };
    let (report, def) = run(json!([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.5],
        [0.0, 0.0, 0.0, 1.0]
    ]));
    let item = report.items.iter().find(|i| i.name == "MoveFace1").unwrap();
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(item.input_provenance, Some(InputProvenance::Read));
    let def = def.expect("a move face");
    assert_eq!(def["body"], "F2.b0", "{def}");
    assert_eq!(def["faces"], json!([top.to_string()]));
    assert_eq!(def["transform"]["type"], "free");
    assert_eq!(def["transform"]["matrix"][2][3], json!(5.0));

    // Its motion not decoded: not translated.
    let (report, def) = run(Value::Null);
    let item = report.items.iter().find(|i| i.name == "MoveFace1").unwrap();
    assert_ne!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert!(
        item.note
            .as_deref()
            .is_some_and(|n| n.contains("motion was not decoded")),
        "{}",
        report.text()
    );
    assert!(def.is_none());
}

/// A draft (mitcad#1363) comes in as MitCad's `draft`: about its neutral
/// face pulled out of the material (`flip`, as the file's face drafts
/// pull), about an origin plane along its normal; the other way is a
/// guess after it, and a zero angle is not translated.
#[test]
fn drafts_come_in_about_their_neutral_plane() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let base = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(base.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let faces = reference.bodies()[0].shape.faces.clone();
    let side = faces.iter().find(|f| f.role == "side").cloned().unwrap();
    let bottom = faces.iter().find(|f| f.role == "start").cloned().unwrap();
    let face = |f: &mitcad_model::FaceName| {
        json!({"kind": "face", "objectType": "BRepFace", "geometry": {"type": "Plane"},
               "point_on_face": face_point(f).map(|x| x / 10.0),
               "_f3d": {"recipe": "bounded_face"}})
    };
    let drafted = |plane: Value, value: f64| {
        json!({"index": 2, "name": "Draft1", "objectType": "DraftFeature", "detail": {
            "inputFaces": [face(&side)], "plane": plane, "isTangentChain": true,
            "draftDefinition": {"_type": "AngleExtentDefinition",
                                "angle": param("d7", "5 deg", value, "deg")},
            "_f3d_angle_two": param("d8", "0.0 deg", 0.0, "deg")}})
    };
    let run = |item: Value| {
        let value = item["detail"]["draftDefinition"]["angle"]["value"].clone();
        let mut items = base.clone();
        items.push(item);
        let dump = dump(json!({
            "schema": "mitcad-f3d-dump", "schema_version": 2,
            "source": {"mode": "f3d_stream", "file": "test.f3d"},
            "parameters": {"model": [
                {"name": "d2", "expression": "10 mm", "value": 1.0, "unit": "mm"},
                {"name": "d3", "expression": "0.0 deg", "value": 0.0, "unit": "deg"},
                {"name": "d7", "expression": "5 deg", "value": value, "unit": "deg"},
                {"name": "d8", "expression": "0.0 deg", "value": 0.0, "unit": "deg"}
            ]},
            "timeline": {"items": items}
        }));
        let mut doc = Document::new(kernel());
        let report = import_design(&mut doc, &dump, &mut NoGeometry, &Options::default());
        let query = json!({"query": "feature", "uid": "F3"}).to_string();
        let def = doc
            .query(&query)
            .ok()
            .and_then(|a| serde_json::from_str::<Value>(&a).ok())
            .map(|a| a["def"].clone())
            .filter(|d| d["type"] == "draft");
        (report, def)
    };
    let angle = 5f64.to_radians();

    // About the bottom face: pulled out of the material.
    let (report, def) = run(drafted(face(&bottom), angle));
    let item = report.items.iter().find(|i| i.name == "Draft1").unwrap();
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(item.input_provenance, Some(InputProvenance::Read));
    let def = def.expect("a draft");
    assert_eq!(def["body"], "F2.b0", "{def}");
    assert_eq!(def["faces"], json!([side.to_string()]));
    assert_eq!(
        def["plane"],
        json!({"body": "F2.b0", "face": bottom.to_string()})
    );
    assert_eq!(def["flip"], true, "{def}");
    // (`tangent_chain` true is the default, left out.)
    assert_ne!(def["tangent_chain"], false, "{def}");
    assert_eq!(def["angle"], "d7", "{def}");
    assert!(def.get("angle2").is_none_or(Value::is_null), "{def}");

    // About the XY plane: along its normal.
    let xy = json!({"kind": "construction_plane", "name": "XY", "origin": "XY"});
    let (report, def) = run(drafted(xy, angle));
    let item = report.items.iter().find(|i| i.name == "Draft1").unwrap();
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    let def = def.expect("a draft");
    assert_eq!(def["plane"], "xy", "{def}");
    assert_ne!(def["flip"], true, "{def}");

    // A zero angle: not translated.
    let (report, def) = run(drafted(face(&bottom), 0.0));
    let item = report.items.iter().find(|i| i.name == "Draft1").unwrap();
    assert_ne!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert!(
        item.note
            .as_deref()
            .is_some_and(|n| n.contains("angle zero")),
        "{}",
        report.text()
    );
    assert!(def.is_none());
}
