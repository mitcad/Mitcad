// SPDX-License-Identifier: MIT
//! Threads and tapped holes: the faces the decoder found, handedness, diameters and groove middles.

use super::*;
use mitcad_model_api::DocumentApi;

/// Threads (mitcad#35) on the faces the decoder found in the history: the
/// size as Mitcad lists it, a partial thread from the end the file's axis
/// points to (here against the mock cylinder's), a class the face's side
/// does not take left out, pipe threads by Mitcad's names.
#[test]
fn threads_come_in_on_the_faces_the_decoder_found() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut base = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(base.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let point = face_point(&side).map(|x| x / 10.0);
    let iso = |class: &str| {
        json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
               "threadDesignation": "M10x1.5", "threadClass": class, "isInternal": false})
    };
    let pipe = json!({"_type": "ThreadInfo", "threadType": "BSP Pipe Threads",
                      "threadDesignation": "G 1/4-19", "threadClass": "A"});
    base.push(thread_item(2, iso("6g"), false, -1.0, point));
    base.push(thread_item(3, iso("6H"), true, 1.0, point));
    base.push(thread_item(4, pipe, true, 1.0, point));
    // The forming screw threads, with and without the `ST` of the size.
    let forming = |designation: &str| {
        json!({"_type": "ThreadInfo", "threadType": "Metric Forming Screw Threads",
               "threadDesignation": designation, "threadClass": "6g", "isInternal": false})
    };
    base.push(thread_item(5, forming("ST3.5x1.3"), true, 1.0, point));
    base.push(thread_item(6, forming("2.9x1.4"), true, 1.0, point));
    let mut doc = Document::new(kernel());
    let report = import_design(
        &mut doc,
        &timeline_dump(base),
        &mut NoGeometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes[2..], [Outcome::Parametric; 5], "{}", report.text());
    let def = |uid: u64| -> Value {
        serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "F{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone()
    };
    let partial = def(3);
    assert_eq!(partial["faces"][0]["face"], side.to_string());
    assert_eq!(
        partial["thread"],
        json!({"designation": "M10x1.5", "class": "6g"})
    );
    // The length a parameter of the thread's own, the offset the file's d5.
    assert!(partial["length"].is_string(), "{partial}");
    assert_eq!(partial["offset"], "d5");
    // The file's axis runs down, the mock cylinder's up: the high end in
    // the file is Mitcad's low end.
    assert_eq!(partial["location"], "low_end");
    let full = def(4);
    assert_eq!(full["thread"], json!({"designation": "M10x1.5"}));
    assert!(full.get("length").is_none());
    assert!(
        report.items[3]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("class")),
        "{}",
        report.text()
    );
    assert_eq!(
        def(5)["thread"],
        json!({"standard": "whitworth", "designation": "G 1/4", "class": "A"})
    );
    assert_eq!(
        def(6)["thread"],
        json!({"standard": "forming_screw", "designation": "ST3.5x1.3", "class": "6g"})
    );
    assert_eq!(
        def(7)["thread"],
        json!({"standard": "forming_screw", "designation": "2.9x1.4", "class": "6g"})
    );
}

/// A thread's handedness read from the file comes in as the
/// thread's, and a thread of which every input is read says so: here a
/// left-handed one; one whose handedness the file does not give, or whose
/// class Mitcad does not list for the face's side, does not.
#[test]
fn a_threads_handedness_comes_in_and_its_inputs_are_read() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut base = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(base.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let point = face_point(&side).map(|x| x / 10.0);
    let iso = |class: &str, right: Option<bool>| {
        let mut info = json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
                              "threadDesignation": "M10x1.5", "threadClass": class,
                              "isInternal": false});
        if let Some(right) = right {
            info["isRightHanded"] = json!(right);
        }
        info
    };
    base.push(thread_item(2, iso("6g", Some(false)), true, 1.0, point));
    base.push(thread_item(3, iso("6g", None), true, 1.0, point));
    base.push(thread_item(4, iso("6H", Some(true)), true, 1.0, point));
    let mut doc = Document::new(kernel());
    let report = import_design(
        &mut doc,
        &timeline_dump(base),
        &mut NoGeometry,
        &Options::default(),
    );
    let text = report.text();
    assert_eq!(
        report.items[2..]
            .iter()
            .map(|i| i.outcome)
            .collect::<Vec<_>>(),
        [Outcome::Parametric; 3],
        "{text}"
    );
    let def = |uid: u64| -> Value {
        serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "F{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone()
    };
    assert_eq!(def(3)["thread"]["right_handed"], json!(false), "{}", def(3));
    assert_eq!(def(4)["thread"].get("right_handed"), None, "{}", def(4));
    let provenance: Vec<_> = report.items[2..]
        .iter()
        .map(|i| i.input_provenance)
        .collect();
    assert_eq!(
        provenance,
        [
            Some(InputProvenance::Read),
            Some(InputProvenance::Unknown),
            // (6H is no class of an external thread: left out.)
            Some(InputProvenance::Unknown)
        ],
        "{text}"
    );
}

/// A modelled thread (mitcad#57) comes in with the file's diameters: the
/// thread alone first (it sizes its part of the face itself). Mitcad
/// models only the 60-degree threads: a modelled pipe thread falls back.
#[test]
fn modelled_threads_take_the_files_diameters() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let point = face_point(&side).map(|x| x / 10.0);
    let info = json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
        "threadDesignation": "M10x1.5", "threadClass": "6g", "isInternal": false,
        "majorDiameter": 0.985, "minorDiameter": 0.8141, "pitchDiameter": 0.8928});
    // A modelled pipe thread on the same face, in a design of its own: the
    // modelled metric thread's crest lies inside the face and leaves no
    // piece of it (mitcad#433).
    let pipe = json!({"_type": "ThreadInfo", "threadType": "BSP Pipe Threads",
                      "threadDesignation": "G 1/4-19", "threadClass": "A"});
    let mut modelled_pipe = thread_item(2, pipe, true, 1.0, point);
    modelled_pipe["detail"]["isModeled"] = json!(true);
    let mut pipe_items = items.clone();
    pipe_items.push(modelled_pipe);
    let mut modelled = thread_item(2, info, true, 1.0, point);
    modelled["detail"]["isModeled"] = json!(true);
    items.push(modelled);
    let mut doc = Document::new(kernel());
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
    let def: Value =
        serde_json::from_str::<Value>(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap())
            .unwrap()["def"]
            .clone();
    assert_eq!(def["type"], "thread", "{def}");
    assert_eq!(def["modeled"], true, "{def}");
    // The file's thread ends at its face's ends, leaving the lip of a cone
    // beyond them (mitcad#1302): its ends are cut flat.
    assert_eq!(def["ends"], "flat", "{def}");
    let spec = doc.kernel().mock.specs.borrow().join("\n");
    assert!(
        spec.contains(&format!(
            "thread {side} pitch 1.5 diameters 9.8500/8.1410/8.9280"
        )),
        "{spec}"
    );
    assert!(spec.contains(" flat ends"), "{spec}");
    // Without a history the angle is not measured.
    assert!(def.get("angle").is_none(), "{def}");
    let mut doc = Document::new(kernel());
    let report = import_design(
        &mut doc,
        &timeline_dump(pipe_items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
    assert!(
        report.items[2]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("Whitworth")),
        "{}",
        report.text()
    );
}

/// A thread sizes its face to the thread's major diameter first, as the
/// file does (mitcad#35); the thread keeps the item's name.
#[test]
fn a_thread_sizes_its_face_first() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let info = json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
        "threadDesignation": "M10x1.5", "threadClass": "6g", "isInternal": false,
        "majorDiameter": 0.985, "minorDiameter": 0.8141});
    items.push(thread_item(
        2,
        info,
        true,
        1.0,
        face_point(&side).map(|x| x / 10.0),
    ));
    let mut doc = Document::new(kernel());
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
    assert!(
        report.items[2]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("sized")),
        "{}",
        report.text()
    );
    let offset = doc.feature(FeatureUid(3)).unwrap();
    assert!(
        matches!(offset.def, FeatureDef::OffsetFace(_)),
        "{offset:?}"
    );
    let thread = doc.feature(FeatureUid(4)).unwrap();
    assert!(matches!(thread.def, FeatureDef::Thread(_)));
    assert_eq!(thread.name, "Thread2");
}

/// A tapped hole (mitcad#35): the hole, and a thread on its wall from the
/// hole's start.
#[test]
fn a_tapped_hole_comes_in_with_its_thread_on_the_wall() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
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
    let top = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "end")
        .cloned()
        .expect("a top face");
    items.push(json!({
        "index": 2, "name": "Hole1", "objectType": "HoleFeature",
        "detail": {
            "holeType": "SimpleHoleType", "holeTapType": "TappedHoleTapType",
            "holeDiameter": param("d6", "5 mm", 0.5, "mm"),
            "tipAngle": param("d7", "118 deg", 2.0594885173533086, "deg"),
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": param("d8", "15 mm", 1.5, "mm")},
            "position": [2.0, 1.0, 1.0],
            "holePositionDefinition": {"planarEntity": {
                "kind": "face", "geometry": {"type": "Plane"},
                "point_on_face": face_point(&top).map(|x| x / 10.0)}},
            "tappedHoleInfo": {"_type": "ThreadInfo", "threadType": "ISO Metric profile",
                               "threadDesignation": "M6x1", "threadClass": "6H",
                               "isInternal": true, "minorDiameter": 0.5035},
            "thread": {"_type": "ThreadFeature", "isModeled": false, "isFullLength": false,
                       "threadLength": param("d9", "10 mm", 1.0, "mm"),
                       "threadOffset": param("d10", "0 mm", 0.0, "mm")}
        }
    }));
    let mut doc = Document::new(kernel());
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
    let def = |uid: &str| -> Value {
        serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone()
    };
    let hole = def("F3");
    assert_eq!(hole["type"], "hole");
    assert!(hole.get("thread").is_none(), "{hole}");
    let thread = def("F4");
    assert_eq!(thread["faces"][0]["face"], "F3:hole0.wall", "{thread}");
    assert_eq!(
        thread["thread"],
        json!({"designation": "M6x1", "class": "6H"})
    );
    assert_eq!(thread["location"], "low_end");
    // A cosmetic thread has no ends to cut (mitcad#1302).
    assert!(thread.get("ends").is_none(), "{thread}");
    // The bore is the hole's diameter (its own state's); the minor
    // diameter is the next guess.
    let params = doc.parameters();
    let bore = hole["diameter"].as_str().and_then(|n| params.find(n));
    assert_eq!(
        bore.and_then(|id| params.get(id)).map(|p| p.value()),
        Some(5.0),
        "{hole}"
    );
}

/// A tapped hole with a modelled thread (mitcad#1302): the thread on its
/// wall is cut with flat ends, as the file's thread ends at the wall's
/// ends and leaves a countersink above unthreaded.
#[test]
fn a_tapped_holes_modelled_thread_keeps_flat_ends() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
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
    let top = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "end")
        .cloned()
        .expect("a top face");
    items.push(json!({
        "index": 2, "name": "Hole1", "objectType": "HoleFeature",
        "detail": {
            "holeType": "SimpleHoleType", "holeTapType": "TappedHoleTapType",
            "holeDiameter": param("d6", "5 mm", 0.5, "mm"),
            "tipAngle": param("d7", "118 deg", 2.0594885173533086, "deg"),
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": param("d8", "15 mm", 1.5, "mm")},
            "position": [2.0, 1.0, 1.0],
            "holePositionDefinition": {"planarEntity": {
                "kind": "face", "geometry": {"type": "Plane"},
                "point_on_face": face_point(&top).map(|x| x / 10.0)}},
            "tappedHoleInfo": {"_type": "ThreadInfo", "threadType": "ISO Metric profile",
                               "threadDesignation": "M6x1", "threadClass": "6H",
                               "isInternal": true, "minorDiameter": 0.5035},
            "thread": {"_type": "ThreadFeature", "isModeled": true, "isFullLength": true}
        }
    }));
    let mut doc = Document::new(kernel());
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
    let thread: Value =
        serde_json::from_str::<Value>(&doc.query(r#"{"query": "feature", "uid": "F4"}"#).unwrap())
            .unwrap()["def"]
            .clone();
    assert_eq!(thread["type"], "thread", "{thread}");
    assert_eq!(thread["faces"][0]["face"], "F3:hole0.wall", "{thread}");
    assert_eq!(thread["modeled"], true, "{thread}");
    assert_eq!(thread["ends"], "flat", "{thread}");
    let spec = doc.kernel().mock.specs.borrow().join("\n");
    assert!(spec.contains(" flat ends"), "{spec}");
}

/// A hole placed at distances from two edges of its face comes in at those
/// distances (`face_offsets`), before the point the file keeps with it
/// (mitcad#719: the point need not be where the distances put the hole).
#[test]
fn a_hole_at_distances_from_two_edges_keeps_them() {
    let kernel = || TestKernel {
        face_listing: true,
        edge_points: true,
        ..TestKernel::default()
    };
    let mut items = vec![
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
    let sides: Vec<_> = faces.iter().filter(|f| f.role == "side").cloned().collect();
    let edge = |side: &mitcad_model::FaceName| {
        json!({"kind": "edge", "objectType": "BRepEdge", "length": 0.1,
               "mid_point": edge_point(&top, side).map(|x| x / 10.0)})
    };
    items.push(json!({
        "index": 2, "name": "Hole1", "objectType": "HoleFeature",
        "detail": {
            "holeType": "SimpleHoleType",
            "holeDiameter": param("d6", "5 mm", 0.5, "mm"),
            "tipAngle": param("d7", "118 deg", 2.0594885173533086, "deg"),
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": param("d8", "15 mm", 1.5, "mm")},
            "position": [2.0, 1.0, 1.0],
            "holePositionDefinition": {
                "_type": "PlaneAndOffsetsHolePositionDefinition",
                "planarEntity": {"kind": "face", "geometry": {"type": "Plane"},
                                 "point_on_face": face_point(&top).map(|x| x / 10.0)},
                "edgeOne": edge(&sides[0]),
                "edgeTwo": edge(&sides[1]),
                "offsetOne": param("d9", "10 mm", 1.0, "mm"),
                "offsetTwo": param("d10", "12 mm", 1.2, "mm")},
            "holeTapType": "TappedHoleTapType",
            "tappedHoleInfo": {"_type": "ThreadInfo", "threadType": "ISO Metric profile",
                               "threadDesignation": "M6x1", "threadClass": "6H",
                               "isInternal": true, "minorDiameter": 0.5035},
            "thread": {"_type": "ThreadFeature", "isModeled": false, "isFullLength": true}
        }
    }));
    let mut doc = Document::new(kernel());
    // The two edges' lines: along x and along y.
    for (side, end) in [(&sides[0], [1.0, 0.0, 0.0]), (&sides[1], [0.0, 1.0, 0.0])] {
        doc.kernel().mock.edge_curves.borrow_mut().insert(
            mitcad_model::EdgeName::new(top.clone(), side.clone()).to_string(),
            mitcad_model::datum::CurveGeometry::Line {
                start: [0.0; 3],
                end,
            },
        );
    }
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
    let hole =
        serde_json::from_str::<Value>(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap())
            .unwrap()["def"]
            .clone();
    let placement = &hole["placement"];
    assert_eq!(placement["type"], "face_offsets", "{hole}");
    assert_eq!(placement["face"], top.to_string(), "{hole}");
    assert_eq!(
        placement["edge1"],
        mitcad_model::EdgeName::new(top.clone(), sides[0].clone()).to_string(),
        "{hole}"
    );
    // Its thread on its wall.
    let thread =
        serde_json::from_str::<Value>(&doc.query(r#"{"query": "feature", "uid": "F4"}"#).unwrap())
            .unwrap()["def"]
            .clone();
    assert_eq!(thread["faces"][0]["face"], "F3:hole0.wall", "{thread}");
    let params = doc.parameters();
    let value = |slot: &str| {
        placement[slot]
            .as_str()
            .and_then(|n| params.find(n))
            .and_then(|id| params.get(id))
            .map(|p| p.value())
    };
    assert_eq!(
        (value("offset1"), value("offset2")),
        (Some(10.0), Some(12.0)),
        "{hole}"
    );
}

/// A hole on a shaft's side measured along the axis from the shaft's end
/// face comes in at that distance (`cylinder_offset`), on the side of the
/// point the file keeps with it (mitcad#719).
#[test]
fn a_hole_on_a_shaft_at_a_distance_from_its_end_keeps_it() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        circle_sketch(0),
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
    let side = faces.iter().find(|f| f.role == "side").cloned().unwrap();
    let start = faces.iter().find(|f| f.role == "start").cloned().unwrap();
    let face = |f: &mitcad_model::FaceName, kind: &str| {
        json!({"kind": "face", "geometry": {"type": kind},
               "point_on_face": face_point(f).map(|x| x / 10.0)})
    };
    items.push(json!({
        "index": 2, "name": "Hole1", "objectType": "HoleFeature",
        "detail": {
            "holeType": "SimpleHoleType",
            "holeDiameter": param("d6", "2 mm", 0.2, "mm"),
            "tipAngle": param("d7", "118 deg", 2.0594885173533086, "deg"),
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": param("d8", "3 mm", 0.3, "mm")},
            "position": [2.0, 1.5, 0.5],
            "holePositionDefinition": {
                "_type": "PlaneAndOffsetsHolePositionDefinition",
                "planarEntity": face(&side, "Cylinder"),
                "edgeOne": face(&start, "Plane"),
                "offsetOne": param("d9", "4 mm", 0.4, "mm")}
        }
    }));
    let mut doc = Document::new(kernel());
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
    let hole =
        serde_json::from_str::<Value>(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap())
            .unwrap()["def"]
            .clone();
    let placement = &hole["placement"];
    assert_eq!(placement["type"], "cylinder_offset", "{hole}");
    assert_eq!(placement["face"], side.to_string(), "{hole}");
    assert_eq!(
        placement["plane"],
        json!({"body": "F2.b0", "face": start.to_string()}),
        "{hole}"
    );
    assert_eq!(placement["toward"], json!([20.0, 15.0, 5.0]), "{hole}");
    let params = doc.parameters();
    let offset = placement["offset"]
        .as_str()
        .and_then(|n| params.find(n))
        .and_then(|id| params.get(id))
        .map(|p| p.value());
    assert_eq!(offset, Some(4.0), "{hole}");
}

/// A tapped hole that starts on a plane (no face of a body) takes its
/// thread on the wall it makes in the one body of its component
/// (mitcad#719).
#[test]
fn a_tapped_hole_on_a_plane_threads_its_wall_in_its_components_body() {
    let mut items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    items.push(json!({
        "index": 2, "name": "Hole1", "objectType": "HoleFeature",
        "detail": {
            "holeType": "SimpleHoleType", "holeTapType": "TappedHoleTapType",
            "holeDiameter": param("d6", "5 mm", 0.5, "mm"),
            "tipAngle": param("d7", "118 deg", 2.0594885173533086, "deg"),
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": param("d8", "15 mm", 1.5, "mm")},
            "position": [2.0, 1.0, 0.0],
            "holePositionDefinition": {
                "_type": "PlaneAndOffsetsHolePositionDefinition",
                "planarEntity": {"kind": "construction_plane", "origin": "XY"}},
            "tappedHoleInfo": {"_type": "ThreadInfo", "threadType": "ISO Metric profile",
                               "threadDesignation": "M6x1", "threadClass": "6H",
                               "isInternal": true, "minorDiameter": 0.5035},
            "thread": {"_type": "ThreadFeature", "isModeled": false, "isFullLength": true}
        }
    }));
    let mut doc = Document::new(TestKernel {
        face_listing: true,
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
    let def = |uid: &str| -> Value {
        serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone()
    };
    assert_eq!(def("F3")["placement"]["type"], "plane");
    let thread = def("F4");
    assert_eq!(
        thread["faces"][0],
        json!({"body": "F2.b0", "face": "F3:hole0.wall"}),
        "{thread}"
    );
}

/// A modelled thread's groove found where the faces cross a segment one
/// pitch long at the pitch radius (mitcad#161): its middle, also where the
/// groove runs across the segment's ends; none for two grooves.
#[test]
fn a_thread_groove_middle_comes_from_the_crossings() {
    use mitcad_import_replay::internal::ops::groove_middle;
    // Out at 0.2, in at 0.7: the groove's middle is at 0.45.
    let middle = groove_middle(&[(0.2, false), (0.7, true)]).unwrap();
    assert!((middle - 0.45).abs() < 1e-12, "{middle}");
    // In at 0.3, out at 0.8: the groove runs from 0.8 round to 0.3, its
    // middle at 0.05.
    let middle = groove_middle(&[(0.3, true), (0.8, false)]).unwrap();
    assert!((middle - 0.05).abs() < 1e-12, "{middle}");
    assert_eq!(
        groove_middle(&[(0.1, false), (0.2, true), (0.5, false), (0.6, true)]),
        None
    );
}
