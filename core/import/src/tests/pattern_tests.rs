// SPDX-License-Identifier: MIT
//! Rectangular, circular and path patterns of features and bodies.

use super::*;
use mitcad_model_api::DocumentApi;

#[test]
fn pattern_provenance_marks_only_omitted_dressup_inputs() {
    wall_clock();
    let mut doc = Document::new(MockKernel::default());
    let dump = timeline_dump(Vec::new());
    let options = Options::default();
    let mut geometry = NoGeometry;
    let mut oracle = Oracle::new(&mut geometry);
    oracle.enabled = false;
    // Candidate translation needs only the imported feature identities;
    // no geometry is evaluated or selected in this test.
    let mut importer = Importer {
        body_scope: body_scope::Scope::capture(&doc).enter(&doc),
        families: &crate::ReplayFamilies,
        oracle,
        doc: &mut doc,
        dump: &dump,
        options: &options,
        params: ParamMap::default(),
        sketches: HashMap::new(),
        features: HashMap::from([
            (1, "F2".parse().unwrap()),
            (2, "F3".parse().unwrap()),
            (3, "F4".parse().unwrap()),
            (5, "F6".parse().unwrap()),
        ]),
        report: DesignReport::default(),
        pending: Vec::new(),
        defined: Default::default(),
        at_final: false,
        started: std::time::Instant::now(),
        components: components::Components::default(),
        component: ComponentUid::ROOT,
        sketch_copies: HashMap::new(),
        stopped_at: Cell::new(None),
        trying: false,
        alternatives: HashMap::new(),
        guesses: None,
        defer_guesses: false,
        own_state: None,
        untranslated: None,
        kernel_failed: false,
        start_bases: HashMap::new(),
        deadline: None,
        captures: captures::Captures::default(),
        memory_released: 0,
        frame_entities: Default::default(),
        workers: Default::default(),
        learn: None,
        dressups: HashMap::from([
            (2, vec!["F3".parse().unwrap()]),
            (3, vec!["F4".parse().unwrap()]),
            (5, vec!["F7".parse().unwrap()]),
        ]),
        notes: Default::default(),
        suppressed: Default::default(),
        made_ahead: HashMap::new(),
        exact_pairs: None,
        waited: Default::default(),
        deviations: Default::default(),
        deviated: Default::default(),
        deviated_log: Default::default(),
        deviation_points: Default::default(),
        sources: None,
        linker: None,
        focus_paths: HashMap::new(),
        link_paths: Default::default(),
    };
    let feature = |index: i64, kind: &str| json!({"kind": "feature", "objectType": kind, "timeline_index": index});
    let plain = feature(1, "ExtrudeFeature");
    let fillet = feature(2, "FilletFeature");
    let chamfer = feature(3, "ChamferFeature");
    for kind in [
        "CircularPatternFeature",
        "RectangularPatternFeature",
        "MirrorFeature",
    ] {
        for (inputs, expected) in [
            (
                vec![plain.clone()],
                vec![(vec!["F2"], InputProvenance::Unknown)],
            ),
            (
                vec![plain.clone(), fillet.clone(), chamfer.clone()],
                vec![
                    (vec!["F2", "F3", "F4"], InputProvenance::Unknown),
                    (vec!["F2"], InputProvenance::Inferred),
                ],
            ),
            (
                vec![fillet.clone(), chamfer.clone()],
                vec![(vec!["F3", "F4"], InputProvenance::Unknown)],
            ),
        ] {
            let item = serde_json::from_value(json!({"index": 4, "objectType": kind,
            "detail": {
                "inputEntities": inputs,
                "axis": {"kind": "construction_axis", "origin": "Z"},
                "quantity": {"kind": "parameter", "value": 2.0, "unit": ""},
                "totalAngle": {"kind": "parameter", "value": 180.0, "unit": "deg"},
                "directionOne": [1.0, 0.0, 0.0],
                "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
                "distanceOne": {"kind": "parameter", "value": 1.0, "unit": "cm"},
                "mirrorPlane": {"kind": "construction_plane", "origin": "YZ"},
                "isCombine": false
            }}))
            .unwrap();
            let candidates = importer.translate_op(kind, &item).unwrap();
            let multiplier = if kind == "MirrorFeature" { 2 } else { 1 };
            assert_eq!(candidates.len(), expected.len() * multiplier, "{kind}");
            // Without history, the complete selection remains first. The
            // mirror's opposite-combine candidates keep their prior flags.
            for (candidate, (features, provenance)) in candidates.iter().zip(&expected) {
                assert_eq!(candidate.input_provenance, *provenance, "{kind}");
                assert_eq!(
                    candidate.defs[0]["objects"],
                    json!({"type": "features", "features": features}),
                    "{kind}"
                );
                assert!(!candidate.guess, "{kind}");
            }
            for candidate in &candidates[expected.len()..] {
                assert!(candidate.guess);
                assert_eq!(candidate.input_provenance, InputProvenance::Inferred);
            }
        }
    }
    // The settings the file stores (mitcad#186) make the complete selection
    // read, and tried before the history's suggestions; without them (or
    // with an omitted dressup) it stays unknown.
    for (kind, settings, key, expected) in [
        (
            "CircularPatternFeature",
            json!({"isSymmetric": true, "patternComputeOption": "OptimizedPatternCompute",
                   "suppressedElementsIds": [2]}),
            "compute",
            "optimized",
        ),
        (
            "RectangularPatternFeature",
            json!({"isSymmetricInDirectionOne": true,
                   "patternDistanceType": "ExtentPatternDistanceType",
                   "patternComputeOption": "IdenticalPatternCompute",
                   "suppressedElementsIds": []}),
            "distance_type",
            "extent",
        ),
        (
            "MirrorFeature",
            json!({"patternComputeOption": "IdenticalPatternCompute"}),
            "compute",
            "identical",
        ),
    ] {
        for with in [true, false] {
            let mut detail = json!({
                "inputEntities": [plain.clone()],
                "axis": {"kind": "construction_axis", "origin": "Z"},
                "quantity": {"kind": "parameter", "value": 2.0, "unit": ""},
                "totalAngle": {"kind": "parameter", "value": 180.0, "unit": "deg"},
                "directionOne": [1.0, 0.0, 0.0],
                "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
                "distanceOne": {"kind": "parameter", "value": 1.0, "unit": "cm"},
                "mirrorPlane": {"kind": "construction_plane", "origin": "YZ"},
                "isCombine": false
            });
            if with {
                for (k, v) in settings.as_object().unwrap() {
                    detail[k] = v.clone();
                }
            }
            let item =
                serde_json::from_value(json!({"index": 4, "objectType": kind, "detail": detail}))
                    .unwrap();
            let candidates = importer.translate_op(kind, &item).unwrap();
            let c = &candidates[0];
            assert_eq!(c.first, with, "{kind} {with}");
            assert_eq!(
                c.input_provenance,
                if with {
                    InputProvenance::Read
                } else {
                    InputProvenance::Unknown
                },
                "{kind} {with}"
            );
            if with {
                assert_eq!(c.defs[0][key], expected, "{kind}");
            }
        }
    }
    // A direction named by an axis that is not an origin axis is tried by
    // its stored vector as well, after the axis (the replay's axis need not
    // point where the file's does); an origin axis points where its vector
    // does, so only one definition is made.
    for (axis, count) in [
        (json!({"kind": "construction_axis", "timeline_index": 2}), 2),
        (json!({"kind": "construction_axis", "origin": "X"}), 1),
    ] {
        let item = serde_json::from_value(
            json!({"index": 4, "objectType": "RectangularPatternFeature",
            "detail": {"inputEntities": [plain.clone()],
                       "directionOneEntity": axis, "directionOne": [0.0, -1.0, 0.0],
                       "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
                       "distanceOne": {"kind": "parameter", "value": -1.0, "unit": "cm"}}}),
        )
        .unwrap();
        let candidates = importer
            .translate_op("RectangularPatternFeature", &item)
            .unwrap();
        assert_eq!(candidates.len(), count, "{axis}");
        if count == 2 {
            assert_eq!(candidates[0].defs[0]["direction1"]["axis"], "F3");
            assert_eq!(
                candidates[1].defs[0]["direction1"]["axis"],
                json!({"origin": [0.0, 0.0, 0.0], "direction": [0.0, -1.0, 0.0]})
            );
            assert_eq!(
                candidates[1].defs[0]["direction1"]["distance"],
                candidates[0].defs[0]["direction1"]["distance"]
            );
        }
    }
    // An origin axis whose stored vector points the other way (mitcad#641):
    // the copies go against the axis, the distance negated.
    for (vector, distance) in [([1.0, 0.0, 0.0], -10.0), ([-1.0, 0.0, 0.0], 10.0)] {
        let item = serde_json::from_value(
            json!({"index": 4, "objectType": "RectangularPatternFeature",
            "detail": {"inputEntities": [plain.clone()],
                       "directionOneEntity": {"kind": "construction_axis", "origin": "X"},
                       "directionOne": vector,
                       "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
                       "distanceOne": {"kind": "parameter", "value": -1.0, "unit": "cm"}}}),
        )
        .unwrap();
        let candidates = importer
            .translate_op("RectangularPatternFeature", &item)
            .unwrap();
        let direction = &candidates[0].defs[0]["direction1"];
        assert_eq!(direction["axis"], "x");
        assert_eq!(direction["distance"].as_f64(), Some(distance), "{vector:?}");
    }
    // So does an axis of its own that runs the other way (mitcad#719).
    for (vector, distance) in [([1.0, 0.0, 0.0], -10.0), ([-1.0, 0.0, 0.0], 10.0)] {
        let item = serde_json::from_value(
            json!({"index": 4, "objectType": "RectangularPatternFeature",
            "detail": {"inputEntities": [plain.clone()],
                       "directionOneEntity": {"kind": "construction_axis",
                           "geometry": {"type": "InfiniteLine3D",
                                        "origin": [0.0, 0.0, 0.0], "direction": [1.0, 0.0, 0.0]}},
                       "directionOne": vector,
                       "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
                       "distanceOne": {"kind": "parameter", "value": -1.0, "unit": "cm"}}}),
        )
        .unwrap();
        let candidates = importer
            .translate_op("RectangularPatternFeature", &item)
            .unwrap();
        let direction = &candidates[0].defs[0]["direction1"];
        assert_eq!(
            direction["axis"],
            json!({"origin": [0.0, 0.0, 0.0], "direction": [1.0, 0.0, 0.0]})
        );
        assert_eq!(direction["distance"].as_f64(), Some(distance), "{vector:?}");
    }
    // A tapped hole's pattern copies its thread with it (mitcad#719).
    let item = serde_json::from_value(json!({"index": 6, "objectType": "CircularPatternFeature",
    "detail": {
        "inputEntities": [feature(5, "HoleFeature")],
        "axis": {"kind": "construction_axis", "origin": "Z"},
        "quantity": {"kind": "parameter", "value": 2.0, "unit": ""},
        "totalAngle": {"kind": "parameter", "value": 180.0, "unit": "deg"}
    }}))
    .unwrap();
    let candidates = importer
        .translate_op("CircularPatternFeature", &item)
        .unwrap();
    assert_eq!(
        candidates[0].defs[0]["objects"],
        json!({"type": "features", "features": ["F6", "F7"]})
    );
    // A hole whose guesses are asked for in parts (mitcad#719) has none
    // after its first part where nothing is guessed: without the history,
    // and with the history but no decoded position (its definitions are
    // the history's alone).
    let hole = |position: bool| -> mitcad_dump::TimelineItem {
        let mut detail = json!({
            "holeType": "SimpleHoleType",
            "holeDiameter": {"kind": "parameter", "value": 0.5, "unit": "mm"},
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": {"kind": "parameter", "value": 1.0, "unit": "mm"}}
        });
        if position {
            detail["position"] = json!([1.0, 1.0, 1.0]);
        }
        serde_json::from_value(json!({"index": 7, "objectType": "HoleFeature", "detail": detail}))
            .unwrap()
    };
    for (oracle, position) in [(false, true), (true, false)] {
        importer.oracle.enabled = oracle;
        importer.guesses = Some(crate::Guesses::default());
        let _ = importer.translate_op("HoleFeature", &hole(position));
        let guesses = importer.guesses.take().unwrap();
        assert!(guesses.done, "oracle {oracle}, position {position}");
    }
    importer.oracle.enabled = false;
}

#[test]
fn a_rectangular_pattern_uses_named_bodies_and_decoded_directions_without_history() {
    for sketch_axes in [false, true] {
        let mut detail = json!({
            "inputEntities": [{"kind": "body", "objectType": "BRepBody",
                               "_f3d": {"producer": 1, "body_index": 0}}],
            "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
            "quantityTwo": {"kind": "parameter", "value": 3.0, "unit": ""},
            "distanceOne": {"kind": "parameter", "value": -5.0},
            "distanceTwo": {"kind": "parameter", "value": 4.0},
            "directionOne": [2.0, 0.0, 0.0],
            "directionTwo": [0.0, 7.0, 0.0]
        });
        if sketch_axes {
            detail.as_object_mut().unwrap().remove("directionOne");
            detail.as_object_mut().unwrap().remove("directionTwo");
            for (key, curve) in [("directionOneEntity", "c0"), ("directionTwoEntity", "c1")] {
                detail[key] = json!({"kind": "sketch_entity", "objectType": "SketchLine",
                    "sketch_timeline_index": 0, "id": curve});
            }
        }
        let dump = timeline_dump(vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
            json!({"index": 2, "name": "Pattern", "objectType": "RectangularPatternFeature",
                   "detail": detail}),
        ]);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        let item = &report.items[2];
        assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
        assert_eq!(doc.bodies().len(), 6);
        let value: Value = serde_json::from_str(
            &doc.query(&format!(
                r#"{{"query": "feature", "uid": "{}"}}"#,
                item.features[0]
            ))
            .unwrap(),
        )
        .unwrap();
        let def = &value["def"];
        assert_eq!(
            def["objects"],
            json!({"type": "bodies", "bodies": ["F2.b0"]})
        );
        let parameters = doc.parameters();
        let distance = |direction: &str| {
            parameters.value(
                parameters
                    .find(def[direction]["distance"].as_str().unwrap())
                    .unwrap(),
            )
        };
        assert_eq!(distance("direction1"), Some(-50.0));
        assert_eq!(distance("direction2"), Some(40.0));
        if sketch_axes {
            assert_eq!(def["direction1"]["axis"]["sketch"], "F1");
            assert_eq!(def["direction2"]["axis"]["sketch"], "F1");
            assert_ne!(
                def["direction1"]["axis"]["curve"],
                def["direction2"]["axis"]["curve"]
            );
        } else {
            assert_eq!(
                def["direction1"]["axis"]["direction"],
                json!([1.0, 0.0, 0.0])
            );
            assert_eq!(
                def["direction2"]["axis"]["direction"],
                json!([0.0, 1.0, 0.0])
            );
        }
    }
}

#[test]
fn a_circular_pattern_uses_its_decoded_sketch_axis_without_history() {
    let dump = timeline_dump(vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "Pattern", "objectType": "CircularPatternFeature",
        "detail": {
            "inputEntities": [{"kind": "body", "objectType": "BRepBody",
                               "_f3d": {"producer": 1, "body_index": 0}}],
            "axis": {"kind": "sketch_entity", "objectType": "SketchLine",
                     "sketch_timeline_index": 0, "id": "c0"},
            "quantity": {"kind": "parameter", "value": 3.0, "unit": ""},
            "totalAngle": {"kind": "parameter", "value": std::f64::consts::PI, "unit": "rad"}
        }}),
    ]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(doc.bodies().len(), 3);
    let value: Value = serde_json::from_str(
        &doc.query(&format!(
            r#"{{"query": "feature", "uid": "{}"}}"#,
            item.features[0]
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(value["def"]["axis"]["sketch"], "F1");
    assert_eq!(
        value["def"]["objects"],
        json!({"type": "bodies", "bodies": ["F2.b0"]})
    );
}

/// A circular pattern turns about the file's stored axis line: where the
/// axis the replay resolves points the other way (the origin Y axis, whose
/// Mitcad direction is +Y, stored as (0, -1, 0)), the angle is negated, so
/// that a partial pattern goes the file's way round (mitcad#719).
#[test]
fn a_circular_pattern_turns_the_way_of_its_stored_axis() {
    let mut angles = Vec::new();
    for stored in [[0.0, -1.0, 0.0], [0.0, 1.0, 0.0]] {
        let dump = timeline_dump(vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
            json!({"index": 2, "name": "Pattern", "objectType": "CircularPatternFeature",
            "detail": {
                "inputEntities": [{"kind": "body", "objectType": "BRepBody",
                                   "_f3d": {"producer": 1, "body_index": 0}}],
                "axis": {"kind": "construction_axis", "name": "Y", "origin": "Y"},
                "quantity": {"kind": "parameter", "value": 3.0, "unit": ""},
                "totalAngle": {"kind": "parameter", "value": 0.5, "unit": "rad"},
                "_f3d_axis": {"origin": [0.0, 0.0, 0.0], "direction": stored}
            }}),
        ]);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        let item = &report.items[2];
        assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
        let value: Value = serde_json::from_str(
            &doc.query(&format!(
                r#"{{"query": "feature", "uid": "{}"}}"#,
                item.features[0]
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["def"]["axis"], "y");
        let name = value["def"]["angle"].as_str().unwrap().to_owned();
        let parameters: Value =
            serde_json::from_str(&doc.query(r#"{"query": "parameters"}"#).unwrap()).unwrap();
        let found = parameters
            .as_array()
            .or_else(|| parameters["parameters"].as_array())
            .unwrap()
            .iter()
            .find(|p| p["name"] == name.as_str())
            .unwrap()
            .clone();
        angles.push(found["value"].as_f64());
    }
    let [Some(against), Some(along)] = angles[..] else {
        panic!("{angles:?}");
    };
    assert!(
        (against + 0.5).abs() < 1e-9 && (along - 0.5).abs() < 1e-9,
        "{angles:?}"
    );
}

/// Faces of a pattern of faces whose recipes name each other as neighbours
/// but which the replay has apart take the face between them, the one face
/// that meets both and no other selected face (mitcad#719: a corner rounded
/// after the selection was made). The plate's top and bottom meet every
/// wall, and walls that still meet need nothing.
#[test]
fn a_face_selection_takes_the_face_between_neighbours_it_names() {
    use mitcad_dump::EntityName;
    let tag = |t: &str| {
        vec![EntityName {
            tag: t.to_owned(),
            kind: 0,
            ops: vec![7],
        }]
    };
    let recipe = |own: &str, neighbours: &[&str]| {
        std::iter::once(tag(own))
            .chain(neighbours.iter().map(|n| tag(n)))
            .collect::<Vec<_>>()
    };
    // A triangle of walls a, b, c between top t and bottom u: a and b meet
    // in the recipes, the replay has the rounding r between them.
    let named = vec![
        recipe("a", &["t", "b", "c", "u"]),
        recipe("b", &["t", "a", "c", "u"]),
        recipe("c", &["t", "a", "b", "u"]),
    ];
    let body: BodyUid = "F2.b0".parse().unwrap();
    let faces: Vec<(BodyUid, String)> = ["A", "B", "C"]
        .iter()
        .map(|f| (body, (*f).to_owned()))
        .collect();
    let pair = |x: &str, y: &str| [x.to_owned(), y.to_owned()];
    let mut walls = vec![
        pair("A", "R"),
        pair("R", "B"),
        pair("B", "C"),
        pair("C", "A"),
    ];
    for w in ["A", "B", "C", "R"] {
        walls.push(pair(w, "T"));
        walls.push(pair(w, "U"));
    }
    let edges = walls.clone();
    let found =
        mitcad_import_replay::internal::ops::bridging_faces(&named, &faces, &mut |_| edges.clone());
    assert_eq!(found, [(body, "R".to_owned())]);
    // A face of that name on another body selected too is another face.
    let other: BodyUid = "F5.b0".parse().unwrap();
    let mut named_other = named.clone();
    named_other.push(recipe("r", &["t"]));
    let mut with_other = faces.clone();
    with_other.push((other, "R".to_owned()));
    let found =
        mitcad_import_replay::internal::ops::bridging_faces(&named_other, &with_other, &mut |b| {
            if b == body { edges.clone() } else { Vec::new() }
        });
    assert_eq!(found, [(body, "R".to_owned())]);
    // Where a and b meet, nothing is added.
    let mut closed: Vec<[String; 2]> = walls
        .into_iter()
        .filter(|[x, y]| x != "R" && y != "R")
        .collect();
    closed.push(pair("A", "B"));
    assert!(
        mitcad_import_replay::internal::ops::bridging_faces(&named, &faces, &mut |_| closed
            .clone())
        .is_empty()
    );
}

/// A rectangular pattern along an edge the replay runs the other way from
/// the file's stored vector negates its distance, so that the copies go
/// the file's way (mitcad#719).
#[test]
fn a_rectangular_pattern_along_an_edge_goes_the_stored_way() {
    let kernel = || TestKernel {
        face_listing: true,
        edge_points: true,
        ..TestKernel::default()
    };
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let faces = reference.bodies()[0].shape.faces.clone();
    let top = faces.iter().find(|f| f.role == "end").cloned().unwrap();
    let side = faces.iter().find(|f| f.role == "side").cloned().unwrap();
    let mut values = Vec::new();
    for vector in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]] {
        let mut items = items.clone();
        items.push(
            json!({"index": 2, "name": "Pattern", "objectType": "RectangularPatternFeature",
            "detail": {
                "inputEntities": [{"kind": "feature", "objectType": "ExtrudeFeature",
                                   "timeline_index": 1}],
                "directionOneEntity": {"kind": "edge", "objectType": "BRepEdge", "length": 0.1,
                    "mid_point": edge_point(&top, &side).map(|x| x / 10.0)},
                "directionOne": vector,
                "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
                "distanceOne": {"kind": "parameter", "value": 3.0, "unit": "cm"}
            }}),
        );
        let mut doc = Document::new(kernel());
        // The edge's line runs along +x in the replay.
        doc.kernel().mock.edge_curves.borrow_mut().insert(
            mitcad_model::EdgeName::new(top.clone(), side.clone()).to_string(),
            mitcad_model::datum::CurveGeometry::Line {
                start: [0.0; 3],
                end: [1.0, 0.0, 0.0],
            },
        );
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
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
        assert!(def["direction1"]["axis"]["edge"].is_string(), "{def}");
        let params = doc.parameters();
        values.push(
            def["direction1"]["distance"]
                .as_str()
                .and_then(|n| params.find(n))
                .and_then(|id| params.get(id))
                .map(|p| p.value()),
        );
    }
    assert_eq!(values, [Some(30.0), Some(-30.0)]);
}

/// A path pattern of a body along a sketch line (mitcad#468), as the stream
/// decoder gives it (`settings` false: without the start point, flip,
/// orientation and symmetry) or as a dump does.
fn path_pattern_item(path: Value, settings: bool) -> Value {
    let mut detail = json!({
        "inputEntities": [{"kind": "body", "objectType": "BRepBody",
                           "_f3d": {"producer": 1, "body_index": 0}}],
        "patternEntityType": "BodiesPatternType",
        "path": path,
        "quantity": {"kind": "parameter", "value": 3.0, "unit": ""},
        "distance": {"kind": "parameter", "value": 1.5, "unit": "mm"},
        "patternDistanceType": "SpacingPatternDistanceType",
        "patternComputeOption": "AdjustPatternCompute",
        "suppressedElementsIds": []
    });
    if settings {
        for (k, v) in [
            ("startPoint", json!(0.0)),
            ("isFlipDirection", json!(true)),
            ("isOrientationAlongPath", json!(false)),
            ("isSymmetric", json!(false)),
        ] {
            detail[k] = v;
        }
    }
    json!({"index": 2, "name": "PathPattern", "objectType": "PathPatternFeature",
           "detail": detail})
}

#[test]
fn a_path_pattern_follows_its_sketch_line() {
    let line = json!([{"_type": "PathEntity", "entity": {"kind": "sketch_entity",
        "objectType": "SketchLine", "sketch_timeline_index": 0, "id": "c0"}}]);
    for settings in [false, true] {
        let dump = timeline_dump(vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
            path_pattern_item(line.clone(), settings),
        ]);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        let item = &report.items[2];
        assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
        assert_eq!(doc.bodies().len(), 3);
        let value: Value = serde_json::from_str(
            &doc.query(&format!(
                r#"{{"query": "feature", "uid": "{}"}}"#,
                item.features[0]
            ))
            .unwrap(),
        )
        .unwrap();
        let def = &value["def"];
        assert_eq!(def["type"], "path_pattern");
        // One curve of the sketch (its id renumbered, points first).
        assert_eq!(def["path"]["sketch"], "F1");
        assert!(def["path"]["curve"].is_string(), "{def}");
        assert_eq!(
            def["objects"],
            json!({"type": "bodies", "bodies": ["F2.b0"]})
        );
        assert_eq!(def["distance_type"], "spacing");
        // Given, the settings are read; else the defaults, inferred.
        assert_eq!(def["flip"].as_bool().unwrap_or(false), settings);
        let provenance = if settings {
            InputProvenance::Read
        } else {
            InputProvenance::Inferred
        };
        assert_eq!(item.input_provenance, Some(provenance), "{}", report.text());
    }
}

#[test]
fn a_stream_decoded_path_pattern_reads_its_flags_centre_and_joined_copies() {
    // mitcad#673: the stream decoder's flag bytes, the objects' centre
    // (cm) that places the original on the path, and a pattern of a new
    // body's feature that made no body: its copies join that body.
    let line = json!([{"_type": "PathEntity", "entity": {"kind": "sketch_entity",
        "objectType": "SketchLine", "sketch_timeline_index": 0, "id": "c0"}}]);
    let mut item = path_pattern_item(line, false);
    let detail = &mut item["detail"];
    detail["inputEntities"] = json!([{"kind": "feature", "objectType": "ExtrudeFeature",
                                      "name": "Extrude1", "timeline_index": 1}]);
    detail["patternEntityType"] = json!("FeaturesPatternType");
    detail["isFlipDirection"] = json!(false);
    detail["isOrientationAlongPath"] = json!(true);
    detail["isSymmetric"] = json!(false);
    detail["_f3d_objects_center"] = json!([0.25, 0.5, 1.0]);
    detail["_f3d_new_bodies"] = json!(0);
    let dump = timeline_dump(vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        item,
    ]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(
        item.input_provenance,
        Some(InputProvenance::Read),
        "{}",
        report.text()
    );
    let value: Value = serde_json::from_str(
        &doc.query(&format!(
            r#"{{"query": "feature", "uid": "{}"}}"#,
            item.features[0]
        ))
        .unwrap(),
    )
    .unwrap();
    let def = &value["def"];
    assert_eq!(def["start_point"], json!([2.5, 5.0, 10.0]), "{def}");
    assert_eq!(def["along_path"], json!(true), "{def}");
    assert_eq!(def["combine"], json!(true), "{def}");
    // The copies joined the extrusion's body.
    assert_eq!(doc.bodies().len(), 1);
}

/// The stored geometry of the tests, with each state asked for logged
/// ("state N") in `log`, where the test kernel's hooks log too.
struct Logged {
    inner: TestGeometry,
    log: Arc<std::sync::Mutex<Vec<String>>>,
}

impl StoredGeometry<MockShape> for Logged {
    fn state_count(&mut self) -> usize {
        self.inner.state_count()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.inner.item_state(index)
    }

    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.inner.item_components(index)
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
        self.log.lock().unwrap().push(format!("state {index}"));
        self.inner.state(index)
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
        self.inner.final_bodies()
    }
}

/// A path pattern of the block's body with a history (nothing, the block,
/// the block and its copies; `broken`: the states that cannot be built),
/// imported with the states asked for and the copies made logged in order
/// ("state N", "copy"): the report, the document and the log.
fn path_pattern_with_history(
    broken: Vec<usize>,
) -> (DesignReport, Document<TestKernel>, Vec<String>) {
    let line = json!([{"_type": "PathEntity", "entity": {"kind": "sketch_entity",
        "objectType": "SketchLine", "sketch_timeline_index": 0, "id": "c0"}}]);
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        path_pattern_item(line, true),
    ];
    let state = |n: usize| {
        let mut reference = Document::new(TestKernel::default());
        import_design(
            &mut reference,
            &timeline_dump(items[..n].to_vec()),
            &mut NoGeometry,
            &Options::default(),
        );
        shapes(&reference)
    };
    let mut geometry = Logged {
        inner: TestGeometry {
            states: vec![Vec::new(), state(2), state(3)],
            items: HashMap::from([(1, 1), (2, 2)]),
            broken,
            ..TestGeometry::default()
        },
        log: Arc::default(),
    };
    let kernel = TestKernel::default();
    let log = Arc::clone(&geometry.log);
    *kernel.on_call.borrow_mut() = Some((
        "transform_shape",
        Box::new(move |_: &MockKernel| log.lock().unwrap().push("copy".to_owned())),
    ));
    let mut doc = Document::new(kernel);
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let log = geometry.log.lock().unwrap().clone();
    (report, doc, log)
}

#[test]
fn a_path_pattern_asks_for_its_state_after_evaluating_its_first_definition() {
    // mitcad#701: an item asks for its own state only when it compares a
    // result against it, so that a large state heals while the item's
    // first definition is evaluated. The result is the same.
    let (report, doc, log) = path_pattern_with_history(Vec::new());
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(doc.bodies().len(), 3, "{}", report.text());
    let first = |entry: &str| log.iter().position(|e| e == entry);
    let state = first("state 2").expect("its state is asked for");
    let copy = first("copy").expect("the pattern made copies");
    assert!(copy < state, "{log:?}");
    // Asked for once.
    assert_eq!(log.iter().filter(|e| *e == "state 2").count(), 1, "{log:?}");
}

#[test]
fn a_path_pattern_whose_state_does_not_rebuild_keeps_nothing_of_its_try() {
    // mitcad#701: the state turns out not to rebuild only after the first
    // definition was evaluated; nothing of that try is kept, and the item
    // is taken unchecked as when its state was asked for first.
    let (report, doc, _) = path_pattern_with_history(vec![2]);
    let item = &report.items[2];
    let note = item.note.clone().unwrap_or_default();
    assert!(
        note.contains("not checked: its state in the file's history could not be rebuilt"),
        "{}",
        report.text()
    );
    assert_eq!(item.features.len(), 1, "{}", report.text());
    let patterns = doc
        .features()
        .filter(|f| f.def.type_name() == "path_pattern")
        .count();
    assert_eq!(patterns, 1, "{}", report.text());
    assert_eq!(doc.bodies().len(), 3, "{}", report.text());
}

#[test]
fn a_path_pattern_along_edges_is_not_translated() {
    let edge = json!([{"_type": "PathEntity", "entity": {"kind": "edge",
        "objectType": "BRepEdge"}}]);
    let dump = timeline_dump(vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        path_pattern_item(edge, true),
    ]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let item = &report.items[2];
    assert_ne!(item.outcome, Outcome::Parametric, "{}", report.text());
}

#[test]
fn route_b_patterns_and_holes_fall_back() {
    // The stream decoder gives a pattern's angle only: no inputs, no axis.
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "C-Pattern1", "objectType": "CircularPatternFeature",
               "detail": {"totalAngle": param("d7", "360 deg", std::f64::consts::TAU, "deg")}}),
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Skipped,
        "{}",
        report.text()
    );
    assert!(
        report.items[2]
            .note
            .as_deref()
            .unwrap()
            .contains("no inputs"),
        "{}",
        report.text()
    );
}

#[test]
fn a_pattern_gets_the_extrusion_profiles_it_copies() {
    for learning in [false, true] {
        check_pattern_reselects_decoded_profiles(learning);
    }
}

fn check_pattern_reselects_decoded_profiles(learning: bool) {
    // Both rectangles extruded give the same body as far as the history
    // tells (the test kernel measures every body alike), but only the
    // file's region gives the pattern's copies: the extrusion takes the
    // first, and the pattern after it changes it to the file's.
    let mut items = decoded_small_rectangle();
    // The decoded loops select the large rectangle first. Without a
    // learning dump, deferring every other set used to discard the small
    // rectangle needed by the later pattern (mitcad#181).
    for (k, curve) in items[1]["detail"]["_f3d_profile_loops"][0]["loops"][0]["curves"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        curve["id"] = json!(format!("c{k}"));
        curve["primary"] = json!(k);
    }
    items.push(
        json!({"index": 2, "name": "C-Pattern1", "objectType": "CircularPatternFeature",
                      "detail": {"quantity": param("d6", "2", 2.0, "")}}),
    );
    let file = reference(vec![
        extrusion_of(SMALL),
        json!({"type": "circular_pattern", "objects": {"type": "features", "features": ["F2"]},
               "axis": "z", "quantity": 2, "angle": std::f64::consts::TAU, "symmetric": false,
               "compute": "adjust"}),
    ]);
    let extruded = file.bodies()[..1].iter().map(|b| b.shape.clone()).collect();
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), extruded, shapes(&file)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel {
        alike: true,
        ..TestKernel::default()
    });
    let dir = mitcad_model::testing::unique_temp_path("learn-revised");
    let options = Options {
        learn: learning.then(|| crate::learn::LearnOptions {
            dir: dir.clone(),
            path: None,
        }),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    let settled = learning.then(|| {
        let settled = std::fs::read_dir(dir.join("settled"))
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect::<String>();
        let _ = std::fs::remove_dir_all(&dir);
        settled
    });
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Parametric,
            Outcome::Parametric,
            Outcome::Parametric
        ],
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
    assert_eq!(
        report.items[1].input_provenance,
        Some(InputProvenance::Inferred)
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["profiles"][0]["region"], SMALL);
    // (The large rectangle was the first guess.)
    assert!(
        report.items[1]
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("chosen for C-Pattern1"),
        "{}",
        report.text()
    );
    // The learning answer follows the final extrusion, not the initially
    // accepted region that the pattern made it replace.
    let Some(settled) = settled else { return };
    let lines: Vec<Value> = settled
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let line = lines.iter().find(|line| line["item"] == 1).unwrap();
    assert_eq!(line["answer"]["input_provenance"], "inferred");
    assert_eq!(line["status"], "settled");
    let profile = &line["answer"]["defs"][0]["profiles"][0];
    assert_eq!(profile["sketch"], "T0");
    let region = profile["region"].as_u64().unwrap() as usize;
    let mut outer: Vec<&str> = line["context"]["sketches"]["T0"]["regions"][region]["outer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|curve| curve.as_str().unwrap())
        .collect();
    outer.sort_unstable();
    assert_eq!(outer, ["c4", "c5", "c6", "c7"]);
}

#[test]
fn a_rectangular_pattern_without_directions_finds_a_hexagonal_lattice() {
    // The oldest item version stores no directions (mitcad#74): the file's
    // pattern repeats the extrusion along x and at 60° to it, on both sides.
    let lattice = json!({"type": "rectangular_pattern",
        "objects": {"type": "features", "features": ["F2"]},
        "direction1": {"axis": "x", "quantity": 2, "distance": 30, "symmetric": true},
        "direction2": {"axis": {"origin": [0.0, 0.0, 0.0],
                                "direction": [0.5, 3f64.sqrt() / 2.0, 0.0]},
                       "quantity": 2, "distance": 30, "symmetric": true},
        "distance_type": "spacing", "original_bodies": true});
    let file = reference(vec![extrusion_of(SMALL), lattice]);
    let extruded = file.bodies()[..1].iter().map(|b| b.shape.clone()).collect();
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), extruded, shapes(&file)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    let distance = param("d9", "30 mm", 3.0, "mm");
    let items = vec![
        two_rectangles(0),
        json!({"index": 1, "name": "Extrude1", "objectType": "ExtrudeFeature",
               "detail": {"operation": "NewBodyFeatureOperation",
                          "profile": [{"kind": "profile", "sketch": "Sketch1",
                                       "sketch_timeline_index": 0}],
                          "extentType": "OneSideFeatureExtentType",
                          "extentOne": {"_type": "DistanceExtentDefinition",
                                        "distance": param("d2", "10 mm", 1.0, "mm")}}}),
        json!({"index": 2, "name": "R-Pattern1", "objectType": "RectangularPatternFeature",
               "detail": {"inputEntities": [{"kind": "feature", "objectType": "ExtrudeFeature",
                                             "name": "Extrude1", "timeline_index": 1}],
                          "patternEntityType": "FeaturesPatternType",
                          "quantityOne": param("d7", "2", 2.0, ""),
                          "quantityTwo": param("d8", "2", 2.0, ""),
                          "distanceOne": distance, "distanceTwo": distance}}),
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap()).unwrap();
    let def = &def["def"];
    assert_eq!(def["direction1"]["symmetric"], json!(true), "{def}");
    let across = def["direction2"]["axis"]["direction"][1]
        .as_f64()
        .unwrap_or_default();
    assert!((across - 3f64.sqrt() / 2.0).abs() < 1e-9, "{def}");
    assert_eq!(def["original_bodies"], json!(true), "{def}");
}

#[test]
fn patterns_of_features_with_a_chamfer_repeat_it_on_the_copies() {
    // The block with a chamfer on its first edge, and a circular pattern
    // of the extrusion and the chamfer (mitcad#105).
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
    let mut items = block_items;
    items.push(
        json!({"index": 2, "name": "Chamfer1", "objectType": "ChamferFeature",
        "detail": {"edgeSets": [{"edges": [edge], "distance": param("d5", "2 mm", 0.2, "mm")}]}}),
    );
    items.push(
        json!({"index": 3, "name": "C-Pattern1", "objectType": "CircularPatternFeature",
        "detail": {"inputEntities": [
                       {"kind": "feature", "objectType": "ExtrudeFeature", "name": "Extrude1",
                        "timeline_index": 1},
                       {"kind": "feature", "objectType": "ChamferFeature", "name": "Chamfer1",
                        "timeline_index": 2}],
                   "patternEntityType": "FeaturesPatternType",
                   "axis": {"kind": "construction_axis", "name": "Z", "origin": "Z"},
                   "quantity": param("d6", "2", 2.0, ""),
                   "totalAngle": param("d7", "360 deg", std::f64::consts::TAU, "deg"),
                   "isSymmetric": false}}),
    );
    let mut dump = timeline_dump(items);
    for (name, expression, unit) in [("d6", "2", ""), ("d7", "360 deg", "deg")] {
        dump.parameters
            .as_mut()
            .unwrap()
            .model
            .as_mut()
            .unwrap()
            .push(
                serde_json::from_value(
                    json!({"name": name, "expression": expression, "unit": unit}),
                )
                .unwrap(),
            );
    }
    let mut doc = Document::new(TestKernel {
        edge_points: true,
        ..TestKernel::default()
    });
    let report = import_design(&mut doc, &dump, &mut NoGeometry, &Options::default());
    let pattern = &report.items[3];
    assert_eq!(pattern.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(
        pattern.note.as_deref(),
        Some("its fillets and chamfers repeated on the copies")
    );
    let chamfer = &report.items[2].features[0];
    let def: Value = serde_json::from_str::<Value>(
        &doc.query(&format!(
            r#"{{"query": "feature", "uid": "{}"}}"#,
            pattern.features[0]
        ))
        .unwrap(),
    )
    .unwrap()["def"]
        .clone();
    // The chamfer among the objects (the first definition tried).
    assert_eq!(
        def["objects"],
        json!({"type": "features", "features": ["F2", chamfer]})
    );
    // The copy of the block got the chamfer's copy, named after the pattern.
    let prefix = format!("{}:chamfer(", pattern.features[0]);
    assert!(
        shapes(&doc)
            .iter()
            .any(|s| s.faces.iter().any(|f| f.to_string().starts_with(&prefix))),
        "{:?}",
        shapes(&doc)
    );
}

#[test]
fn patterns_with_read_inputs_or_a_missing_feature_are_not_searched() {
    let read = |provenance, guess| {
        let mut c = Candidate::new(json!({"type": "rectangular_pattern"}));
        c.input_provenance = provenance;
        c.guess = guess;
        c
    };
    let not_searched = ops::pattern_not_searched;
    // The decoded definitions have read inputs: no history suggestions.
    for kind in ["CircularPatternFeature", "RectangularPatternFeature"] {
        let decoded = Ok(vec![
            read(InputProvenance::Read, false),
            read(InputProvenance::Unknown, false),
        ]);
        assert!(not_searched(kind, &decoded), "{kind}");
        // Inputs not read, or only guessed ones: the suggestions come too.
        let unknown = Ok(vec![read(InputProvenance::Unknown, false)]);
        assert!(!not_searched(kind, &unknown), "{kind}");
        let guessed = Ok(vec![read(InputProvenance::Read, true)]);
        assert!(!not_searched(kind, &guessed), "{kind}");
        // The copied feature did not come in: the copies of another one
        // are not the file's.
        let missing = Err(ops::PATTERNED_NOT_IMPORTED.to_owned());
        assert!(not_searched(kind, &missing), "{kind}");
        let other = Err("direction one was not decoded".to_owned());
        assert!(!not_searched(kind, &other), "{kind}");
    }
    // Mirrors and other items keep their suggestions.
    let decoded = Ok(vec![read(InputProvenance::Read, false)]);
    assert!(!not_searched("MirrorFeature", &decoded));
}

#[test]
fn an_align_moves_its_bodies_and_a_pattern_of_a_paste_copies_the_pasted_bodies() {
    // (mitcad#641) A paste of an extruded body, an align that moves the
    // copy by the motion it stores (cm), and a pattern of the paste: the
    // pattern copies the body the paste brought.
    let body = |producer: i64| {
        json!({"kind": "body", "objectType": "BRepBody",
               "_f3d": {"producer": producer, "body_index": 0}})
    };
    let dump = timeline_dump(vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "CopyPasteBodies1", "objectType": "CopyPasteBodies",
               "detail": {"sourceBody": [body(1)]}}),
        json!({"index": 3, "name": "Align1", "objectType": "AlignFeature",
               "detail": {"inputEntities": [body(2)],
                          "transform": [[1.0, 0.0, 0.0, 1.4], [0.0, 0.0, -1.0, 0.27],
                                        [0.0, 1.0, 0.0, 1.4], [0.0, 0.0, 0.0, 1.0]]}}),
        json!({"index": 4, "name": "Pattern", "objectType": "RectangularPatternFeature",
               "detail": {
                   "inputEntities": [{"kind": "feature", "objectType": "CopyPasteBodies",
                                      "name": "CopyPasteBodies1", "timeline_index": 2}],
                   "patternEntityType": "FeaturesPatternType",
                   "directionOneEntity": {"kind": "construction_axis", "origin": "X"},
                   "directionOne": [1.0, 0.0, 0.0],
                   "quantityOne": {"kind": "parameter", "value": 2.0, "unit": ""},
                   "distanceOne": {"kind": "parameter", "value": 8.4, "unit": "mm"}}}),
    ]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let def = |i: usize| -> Value {
        let item = &report.items[i];
        assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
        let value: Value = serde_json::from_str(
            &doc.query(&format!(
                r#"{{"query": "feature", "uid": "{}"}}"#,
                item.features[0]
            ))
            .unwrap(),
        )
        .unwrap();
        value["def"].clone()
    };
    let paste = def(2);
    assert_eq!(paste["copy"], true);
    let align = def(3);
    assert_eq!(align["type"], "move");
    assert_eq!(align["bodies"], json!(["F3.b0"]));
    assert_eq!(
        align["transform"]["matrix"],
        json!([
            [1.0, 0.0, 0.0, 14.0],
            [0.0, 0.0, -1.0, 2.7],
            [0.0, 1.0, 0.0, 14.0]
        ])
    );
    assert_eq!(
        report.items[3].input_provenance,
        Some(InputProvenance::Inferred)
    );
    let pattern = def(4);
    assert_eq!(
        pattern["objects"],
        json!({"type": "bodies", "bodies": ["F3.b0"]})
    );
}
