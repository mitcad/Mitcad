// SPDX-License-Identifier: MIT
//! Extrusion inputs: how the profile and the extent are chosen from the history, the decoded stream and measured regions, and approximate extrusions.

use super::*;
use mitcad_model_api::DocumentApi;

#[test]
fn the_history_chooses_the_profile() {
    // The file's design extruded the small rectangle; the decoder only
    // says "one profile of Sketch1", and the larger one is the first guess.
    let items = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let full = timeline_dump(items.clone());
    // What the file has: the same sketch with the small rectangle extruded.
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..1].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let small = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": small}],
                               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        ..TestGeometry::default()
    };

    // Without the history the first guess is used, the larger rectangle.
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &full, &mut NoGeometry, &Options::default());
    assert_eq!(report.items[1].outcome, Outcome::Parametric);
    assert_eq!(report.items[1].verified, None);
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_ne!(def["def"]["profiles"][0]["region"], small);

    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &full, &mut geometry, &Options::default());
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].verified, Some(true));
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["profiles"][0]["region"], small);
    assert_eq!(def["def"]["extent"]["distance"], "d2");
    assert_eq!(report.history.matched, 1);
    assert!(report.history.reached_end);
    assert_eq!(report.bodies.len(), 1);
    assert_eq!(report.bodies[0].volume_difference, Some(0.0));
}

#[test]
fn the_decoded_profile_loops_choose_the_profile() {
    // mitcad#96: the decoder lists the small rectangle's loop (the file's
    // curve ids with their geometry), so that even without a history the
    // import extrudes it, not the first guess (the larger one).
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(decoded_small_rectangle()),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(report.items[1].outcome, Outcome::Parametric);
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    let small = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";
    assert_eq!(def["def"]["profiles"][0]["region"], small);
    assert_eq!(def["def"]["profiles"].as_array().unwrap().len(), 1);
}

/// Imports the decoded small rectangle's extrusion with the given stream
/// fields (and thin wall `(location, thickness)` of the detail): the
/// extrusion's definition and the provenance of its inputs (mitcad#186).
fn decoded_extrusion(raw: Value, thin: Option<(&str, &str)>) -> (Value, Option<InputProvenance>) {
    let mut items = decoded_small_rectangle();
    items[1]["_f3d"]["extrude"] = raw;
    if let Some((location, thickness)) = thin {
        let detail = &mut items[1]["detail"];
        detail["isThinExtrude"] = json!(true);
        detail["thinExtrudeWallLocationOne"] = json!(location);
        detail["thinExtrudeWallThicknessOne"] = param(thickness, "2 mm", 0.2, "mm");
    }
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    (def["def"].clone(), report.items[1].input_provenance)
}

#[test]
fn extrusions_with_every_input_in_the_stream_are_read() {
    // A one-sided distance whose stream carries every field: operation,
    // codes, start, a solid's thin-wall block and an exact profile
    // selection. The stored vector against the sketch's normal flips
    // nothing: the distance's sign gives the direction.
    let raw = |extra: Value| {
        let mut raw = json!({"operation_code": 4, "extent_a": 1, "extent_b": 2,
                             "direction": 1.0, "direction_vector": [0.0, 0.0, -1.0],
                             "flag": 0, "solid": true, "start": 0, "sides": [1, 0],
                             "thin": false, "thin_location": 0});
        for (key, value) in extra.as_object().unwrap() {
            raw[key] = value.clone();
        }
        raw
    };
    let (def, provenance) = decoded_extrusion(raw(json!({})), None);
    assert_ne!(def["flip"], true);
    assert_eq!(def["extent"]["type"], "distance");
    assert!(def.get("thin").is_none());
    assert_eq!(provenance, Some(InputProvenance::Read));
    // The same stream without the side kinds, the start or the thin-wall
    // block leaves an input open: not read.
    for missing in ["sides", "start", "thin"] {
        let mut value = raw(json!({}));
        value.as_object_mut().unwrap().remove(missing);
        let (_, provenance) = decoded_extrusion(value, None);
        assert_ne!(provenance, Some(InputProvenance::Read), "{missing}");
    }
    // A thin wall: the file's side one lies inside the profile, the
    // model's side one outside, so the sides swap; the centre stays.
    for (file, model) in [("Side1", "side2"), ("Center", "center"), ("Side2", "side1")] {
        let location = match file {
            "Side1" => 0,
            "Center" => 1,
            _ => 2,
        };
        let (def, provenance) = decoded_extrusion(
            raw(json!({"thin": true, "thin_location": location})),
            Some((file, "d4")),
        );
        assert_eq!(def["thin"]["location"], model, "{file}");
        assert_eq!(provenance, Some(InputProvenance::Read), "{file}");
    }
    // A thin extrusion the stream does not mark as one is no read input.
    let (_, provenance) = decoded_extrusion(raw(json!({"thin": null})), Some(("Side1", "d4")));
    assert_ne!(provenance, Some(InputProvenance::Read));
}

/// A surface extrusion (`isSolid` false, mitcad#455) comes in as the
/// extrusion with `surface` set: a new surface body of the region's walls.
#[test]
fn a_surface_extrusion_comes_in_as_a_surface() {
    let mut items = decoded_small_rectangle();
    items[1]["detail"]["isSolid"] = json!(false);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["surface"], true, "{def}");
    assert_eq!(def["def"]["operation"], "new_body");
    let bodies = doc.bodies();
    assert_eq!(bodies.len(), 1);
    let shape = doc.body_shape(bodies[0].uid).unwrap();
    assert_eq!(
        doc.kernel().body_kind(shape).unwrap(),
        mitcad_model::BodyKind::Sheet
    );
}

/// A one-sided extrusion through all of the decoded small rectangle, after a
/// first one that makes a body: its direction follows the flag byte of the
/// stream (the API reference dumps' `isPositiveDirection`: 0 along the
/// sketch's normal, 1 against it), also where the stored vector says the
/// other way, and the extent is read.
#[test]
fn through_all_direction_follows_the_flag_byte() {
    let import_through_all = |flag: Option<u32>, positive: Option<bool>, two_sided: bool| {
        let mut items = decoded_small_rectangle();
        // The body lies on the side the through-all extrusion goes to.
        let down = if flag == Some(1) { -1.0 } else { 1.0 };
        items[1]["_f3d"]["extrude"] = json!({"operation_code": 4, "direction": down,
                                             "direction_vector": [0.0, 0.0, down]});
        let mut second = items[1].clone();
        second["index"] = json!(2);
        second["name"] = json!("Extrude2");
        // A cut of the first body (the stored vector says the other way).
        let mut raw = json!({"operation_code": 2, "extent_a": 1, "extent_b": 0,
                             "direction": 1.0, "direction_vector": [0.0, 0.0, -down],
                             "solid": true, "start": 0, "sides": [4, 0],
                             "thin": false, "thin_location": 0});
        if let Some(flag) = flag {
            raw["flag"] = json!(flag);
        }
        second["_f3d"]["extrude"] = raw;
        second["detail"]["operation"] = json!("CutFeatureOperation");
        second["detail"]["participantBodies"] = json!([{"kind": "body", "objectType": "BRepBody",
                    "_f3d": {"producer": 1, "body_index": 0}}]);
        let mut extent = json!({"_type": "ThroughAllExtentDefinition"});
        if let Some(positive) = positive {
            extent["isPositiveDirection"] = json!(positive);
        }
        second["detail"]["extentOne"] = extent;
        if two_sided {
            // Side one through all, side two a distance: the codes 4 and 1.
            second["_f3d"]["extrude"]["sides"] = json!([4, 1]);
            second["detail"]["extentType"] = json!("TwoSidesFeatureExtentType");
            second["detail"]["hasTwoExtents"] = json!(true);
            second["detail"]["extentTwo"] = json!({"_type": "DistanceExtentDefinition",
                "distance": param("d9", "1 mm", 0.1, "mm"), "isSymmetric": false});
        }
        items.push(second);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &timeline_dump(items));
        (
            report.items[2].outcome,
            doc,
            report.items[2].input_provenance,
        )
    };
    let def_of = |doc: &Document<MockKernel>| -> Value {
        let def: Value =
            serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap())
                .unwrap();
        def["def"].clone()
    };
    // Flag 0: along the normal; flag 1: against it.
    let (outcome, doc, provenance) = import_through_all(Some(0), Some(true), false);
    assert_eq!(outcome, Outcome::Parametric);
    let def = def_of(&doc);
    assert_eq!(def["extent"]["type"], "through_all");
    assert_ne!(def["extent"]["both_sides"], true);
    assert_ne!(def["flip"], true);
    assert_eq!(provenance, Some(InputProvenance::Read));
    let (outcome, doc, provenance) = import_through_all(Some(1), Some(false), false);
    assert_eq!(outcome, Outcome::Parametric);
    assert_eq!(def_of(&doc)["flip"], true);
    assert_eq!(provenance, Some(InputProvenance::Read));
    // Without the flag byte the side is a guess: with no history to check
    // it the item is left out.
    let (outcome, _, _) = import_through_all(None, None, false);
    assert_eq!(outcome, Outcome::Skipped);
    // Side one through all and side two a distance: side one goes the
    // way the flag says, and the extent is read.
    for (flag, positive, against) in [(0, true, false), (1, false, true)] {
        let (outcome, doc, provenance) = import_through_all(Some(flag), Some(positive), true);
        assert_eq!(outcome, Outcome::Parametric, "{flag}");
        let def = def_of(&doc);
        assert_eq!(def["extent"]["type"], "two_sides", "{def}");
        assert_eq!(def["flip"] == json!(true), against, "{def}");
        assert_eq!(provenance, Some(InputProvenance::Read), "{flag}");
    }
}

/// Imports the extrusion of the decoded small rectangle against a history
/// whose state is the extrusion of `region` (mitcad#181): its outcome, the
/// region it took and how many times regions were probed.
fn decoded_against_history(region: &str) -> (Outcome, Value, usize) {
    let items = decoded_small_rectangle();
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..1].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        items: HashMap::from([(1, 1)]),
        ..TestGeometry::default()
    };
    let kernel = TestKernel::default();
    let probes = Arc::clone(&kernel.probes);
    let mut doc = Document::new(kernel);
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    (
        report.items[1].outcome,
        def["def"]["profiles"].clone(),
        probes.load(std::sync::atomic::Ordering::SeqCst),
    )
}

#[test]
fn decoded_profiles_that_give_the_state_are_taken_without_probing_the_others() {
    // mitcad#181: the decoded profiles' definitions are tried on their own
    // first; the regions the history suggests (found by probing the next
    // states, seconds on large bodies) only when those give no state.
    let small = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";
    let (outcome, profiles, probes) = decoded_against_history(small);
    assert_eq!(outcome, Outcome::Parametric);
    assert_eq!(profiles, json!([{"sketch": "F1", "region": small}]));
    assert_eq!(probes, 0);
}

#[test]
fn decoded_profiles_that_give_no_state_bring_in_the_other_regions() {
    // The file extruded the large rectangle where the decoder read the
    // small one: the other sets of regions come in and give the state.
    let large = "r{c9[c12,c10],c10[c9,c11],c11[c10,c12],c12[c11,c9]}";
    let (outcome, profiles, probes) = decoded_against_history(large);
    assert_eq!(outcome, Outcome::Parametric);
    assert_eq!(profiles, json!([{"sketch": "F1", "region": large}]));
    assert!(probes > 0);
}

#[test]
fn an_external_dump_replays_with_profiles_matched_by_area_and_centroid() {
    // An external dump (SCHEMA.md, made by a test producer):
    // a 20 x 10 mm rectangle with a construction circle, extruded
    // 10 mm, and a construction plane.
    let text = include_str!("../../../f3d/tests/data/external_dump_box.json");
    let dump = Dump::from_json(text).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(&mut doc, &dump, &mut NoGeometry, &Options::default());
    let sketch = &report.items[0];
    assert_eq!(sketch.outcome, Outcome::Parametric, "{}", report.text());
    let extrude = &report.items[1];
    assert_eq!(extrude.object_type, "ExtrudeFeature");
    assert_eq!(extrude.outcome, Outcome::Parametric, "{}", report.text());
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    // The rectangle's region, not the circle's (construction).
    assert_eq!(def["def"]["profiles"].as_array().unwrap().len(), 1);
    assert_eq!(def["def"]["extent"]["distance"], "d2");
    let FeatureDef::Sketch(s) = &doc.feature(FeatureUid(1)).unwrap().def else {
        panic!("a sketch");
    };
    assert!(s.entities.iter().any(|e| e.construction));
    assert_eq!(doc.bodies().len(), 1);
}

#[test]
fn a_measured_profile_may_be_a_union_of_regions() {
    // One profile of both rectangles' area (10 cm²) and centroid: no
    // region of the sketch has them, the two together do.
    let mut feature = extrude(1, 0, "NewBodyFeatureOperation", "d2");
    feature["detail"]["profile"][0]["area"] = json!(10.0);
    feature["detail"]["profile"][0]["centroid"] = json!([2.7, 1.0, 0.0]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &timeline_dump(vec![two_rectangles(0), feature]));
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(
        def["def"]["profiles"].as_array().map(Vec::len),
        Some(2),
        "{def}"
    );
}

#[test]
fn prisms_bound_the_change_of_extrusions() {
    let extrude = |extent: Value| Candidate {
        core: CandidateCore::new(json!({"type": "extrude", "extent": extent, "operation": "cut"})),
        note: None,
        guess: true,
        predicted: Some(-100.0),
        input_provenance: crate::InputProvenance::Unknown,
        first: false,
    };
    assert!(bounded(&extrude(
        json!({"type": "distance", "distance": "d1"})
    )));
    let tapered = json!({"type": "two_sides", "side1": {"type": "distance", "taper": "d2"},
                         "side2": {"type": "distance"}});
    assert!(!bounded(&extrude(tapered)));
    assert!(!bounded(&extrude(
        json!({"type": "to_object", "object": {}})
    )));
    // A cut of at most 100 mm³ can remove 90 but not 120.
    assert!(can_change(-100.0, -90.0) && !can_change(-100.0, -120.0));
    assert!(can_change(-100.0, 50.0) && can_change(-100.0, 0.0));
}

#[test]
fn joins_that_remove_material_are_not_taken() {
    let sig = |volume: f64| Sig {
        volume,
        area: 10.0,
        center: [0.0; 3],
        faces: 6,
    };
    let feature = |operation: &str| {
        Candidate::new(json!({"type": "extrude", "operation": operation, "extent": {}}))
    };
    let (join, cut) = (feature("join"), feature("cut"));
    assert!(implausible(&join, &[sig(100.0)], &[sig(99.0)]));
    assert!(!implausible(&join, &[sig(100.0)], &[sig(101.0)]));
    assert!(implausible(&cut, &[sig(100.0)], &[sig(101.0)]));
    assert!(!implausible(&cut, &[sig(100.0)], &[sig(99.0)]));
    // Bodies joined into one hold their overlap once.
    assert!(!implausible(&join, &[sig(100.0), sig(50.0)], &[sig(140.0)]));
}

/// Extrude1 of [`SMALL`] imported on a kernel whose extrusions come out
/// 3e-4 larger than the file's (as regions next to the file's would), which
/// lists new faces with `lists_new_faces` (so it can tell that the larger
/// body's faces lie on the file's within the geometric tolerance).
fn approximate_extrusion(lists_new_faces: bool) -> (DesignReport, Document<TestKernel>) {
    let items = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let file = reference(vec![extrusion_of(SMALL)]);
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&file)],
        items: HashMap::from([(1, 1)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel {
        skew: true,
        lists_new_faces,
        ..TestKernel::default()
    });
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    (report, doc)
}

#[test]
fn an_approximate_extrusion_keeps_its_body_within_the_tolerance() {
    // The closest extrusion is kept. Its body's faces lie on the file's
    // within the geometric tolerance: it was imported correctly and stays
    // (mitcad#285), no base feature replaces it.
    let (report, doc) = approximate_extrusion(true);
    let text = report.text();
    assert_eq!(report.items[1].outcome, Outcome::Parametric, "{text}");
    assert_eq!(report.items[1].verified, Some(true));
    assert!(
        !doc.features().any(|f| matches!(f.def, FeatureDef::Base(_))),
        "{text}"
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.contains("brings the bodies")),
        "{text}"
    );
    assert_eq!(report.bodies.len(), 1);
    assert!(
        report.bodies[0]
            .mitcad
            .as_deref()
            .is_some_and(|m| m.starts_with("F2.b0")),
        "{text}"
    );
}

#[test]
fn an_approximate_extrusion_does_not_carry_its_difference() {
    // Where the kernel cannot tell the extrusion's faces to lie on the
    // file's, its body differs: a base feature brings that body to the
    // file's state after it, so that the final bodies are the stored ones
    // exactly. (Before mitcad#285 this happened to a body within the
    // tolerance too, as above.)
    let (report, doc) = approximate_extrusion(false);
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].verified, Some(true));
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["profiles"][0]["region"], SMALL);
    // The base feature after it, and the stored body exactly.
    assert!(
        matches!(
            doc.feature(FeatureUid(3)).map(|f| &f.def),
            Some(FeatureDef::Base(_))
        ),
        "{}",
        report.text()
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("brings the bodies that differ to the file's state after Extrude1")),
        "{}",
        report.text()
    );
    assert_eq!(report.bodies.len(), 1);
    assert_eq!(report.bodies[0].volume_difference, Some(0.0));
    // A difference the bodies carry from an approximate item before is not
    // the item's own.
    assert!(introduced(3e-4, 0.0) && !introduced(3e-4, 3e-4) && introduced(6e-4, 3e-4));
    assert!(!introduced(5e-6, 0.0));
    // A closer extrusion takes an approximate one's place; a fillet only
    // when it comes closer by a quarter.
    let fillet = Candidate::new(json!({"type": "fillet", "body": "F2.b0", "sets": []}));
    let extrude = Candidate::new(json!({"type": "extrude"}));
    assert!(closer(&extrude, 4.0e-4, 4.4e-4) && !closer(&fillet, 4.0e-4, 4.4e-4));
    assert!(closer(&fillet, 5.2e-6, 1.0e-5) && !closer(&extrude, 5e-4, 4.4e-4));
}

/// An extrusion of the stream decoder with its raw extent codes and
/// direction vector, without an extent of its own unless `distance`.
fn stream_extrude(index: i64, operation: &str, raw: Value, distance: Option<&str>) -> Value {
    let mut item = extrude(index, 0, operation, "d2");
    item["_f3d"]["extrude"] = raw;
    let detail = item["detail"].as_object_mut().unwrap();
    detail.remove("extentType");
    match distance {
        Some(d) => detail["extentOne"]["distance"] = param(d, "10 mm", 1.0, "mm"),
        None => {
            detail.remove("extentOne");
        }
    }
    item
}

#[test]
fn stream_extent_codes_and_direction_vector_choose_the_extent() {
    // Without a history only the first definition that is not a guess (of
    // the last item).
    let last = |items: Vec<Value>| {
        let n = items.len();
        let dump = timeline_dump(items);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        // (The sketch is partial: its polygon constraint is left out.)
        assert!(
            report.items[1..]
                .iter()
                .all(|i| i.outcome == Outcome::Parametric),
            "{}",
            report.text()
        );
        let def: Value = serde_json::from_str(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "F{n}"}}"#))
                .unwrap(),
        )
        .unwrap();
        def["def"].clone()
    };
    let new_body = "NewBodyFeatureOperation";
    let first = |raw: Value, distance: Option<&str>| {
        last(vec![
            rectangle_sketch(0),
            stream_extrude(1, new_body, raw, distance),
        ])
    };
    // The vector against the sketch's normal flips it, whatever the first
    // ±1 of the stream says.
    let def = first(
        json!({"operation_code": 4, "extent_a": 1, "extent_b": 2, "direction": 1.0,
               "direction_vector": [0.0, 0.0, -1.0]}),
        Some("d2"),
    );
    assert_eq!(def["extent"]["type"], "distance");
    assert_eq!(def["flip"], true);
    let def = first(
        json!({"operation_code": 4, "extent_a": 1, "extent_b": 2, "direction": -1.0,
               "direction_vector": [0.0, 0.0, 1.0]}),
        Some("d2"),
    );
    assert_ne!(def["flip"], true);
    // A vector oblique to the sketch's normal tells no side: along the
    // normal (mitcad#104).
    let def = first(
        json!({"operation_code": 4, "extent_a": 1, "extent_b": 2,
               "direction_vector": [0.0, 0.6, -0.8]}),
        Some("d2"),
    );
    assert_ne!(def["flip"], true);
    // Code 3: symmetric, a half each way.
    let def = first(
        json!({"operation_code": 4, "extent_a": 3, "extent_b": 2,
               "direction_vector": [0.0, 0.0, 1.0]}),
        Some("d2"),
    );
    assert_eq!(def["extent"]["type"], "symmetric");
    assert_ne!(def["extent"]["full_length"], true);
    // One side through all (code 0) of a block below the sketch: the
    // stored vector points away from the extrusion.
    let def = last(vec![
        rectangle_sketch(0),
        stream_extrude(
            1,
            new_body,
            json!({"operation_code": 4, "extent_a": 1, "extent_b": 2,
                   "direction_vector": [0.0, 0.0, -1.0]}),
            Some("d2"),
        ),
        stream_extrude(
            2,
            "CutFeatureOperation",
            json!({"operation_code": 2, "extent_a": 1, "extent_b": 0,
                   "direction_vector": [0.0, 0.0, 1.0]}),
            None,
        ),
    ]);
    assert_eq!(def["extent"]["type"], "through_all");
    assert_eq!(def["flip"], true);
}

/// A symmetric extent's length from the stream decoder (mitcad#96): tried
/// first, without a history the only one.
#[test]
fn decoded_symmetric_length_comes_first() {
    for full in [true, false] {
        let dump = timeline_dump(vec![
            rectangle_sketch(0),
            stream_extrude(
                1,
                "NewBodyFeatureOperation",
                json!({"operation_code": 4, "extent_a": 3, "extent_b": 2,
                       "direction_vector": [0.0, 0.0, 1.0], "full_length": full}),
                Some("d2"),
            ),
        ]);
        let mut doc = Document::new(MockKernel::default());
        import(&mut doc, &dump);
        let def: Value =
            serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap())
                .unwrap();
        assert_eq!(def["def"]["extent"]["type"], "symmetric");
        let length = def["def"]["extent"]["full_length"].as_bool();
        assert_eq!(length.unwrap_or(false), full);
    }
}

/// A two-sided extrusion with both sides up to faces the replay does not
/// have (mitcad#104): each side goes up to its face's plane, side one
/// towards the side of the sketch its face lies on.
#[test]
fn two_sided_extrusion_goes_up_to_both_decoded_planes() {
    let face = |z: f64| {
        json!({"kind": "face", "objectType": "BRepFace",
               "point_on_face": [1.0, 1.0, z],
               "geometry": {"type": "Plane", "origin": [0.0, 0.0, z], "normal": [0.0, 0.0, 1.0]},
               "_f3d": {"recipe": "bounded_face", "entities": [[{"tag": "9", "ops": [5]}]]}})
    };
    let mut item = stream_extrude(
        1,
        "NewBodyFeatureOperation",
        json!({"operation_code": 4, "extent_a": 2, "extent_b": 1, "flag": 0,
               "direction_vector": [0.0, 0.0, 1.0], "slot_roles": [17, 17, 65]}),
        None,
    );
    item["detail"]["extentType"] = json!("TwoSidesFeatureExtentType");
    item["detail"]["extentOne"] =
        json!({"_type": "ToEntityExtentDefinition", "entity": face(-0.3)});
    item["detail"]["extentTwo"] = json!({"_type": "ToEntityExtentDefinition", "entity": face(0.5)});
    let dump = timeline_dump(vec![rectangle_sketch(0), item]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    let extent = &def["def"]["extent"];
    assert_eq!(extent["type"], "two_sides", "{def}");
    let origin = |side: &str| extent[side]["object"]["plane"]["origin"][2].as_f64();
    assert_eq!((origin("side1"), origin("side2")), (Some(-3.0), Some(5.0)));
    assert_eq!(def["def"]["flip"], true);
}

/// A two-sided extrusion up to two objects is read when the stream gives
/// its side codes and start, side one's code and side two's block of the
/// object (its nearest-solution byte), and the direction follows the side
/// of the sketch side one's object lies on.
#[test]
fn two_sides_up_to_objects_are_read_with_the_second_objects_block() {
    let face = |z: f64| {
        json!({"kind": "face", "objectType": "BRepFace",
               "point_on_face": [1.0, 1.0, z],
               "geometry": {"type": "Plane", "origin": [0.0, 0.0, z], "normal": [0.0, 0.0, 1.0]},
               "_f3d": {"recipe": "bounded_face", "entities": [[{"tag": "9", "kind": 0, "ops": [5]}]]}})
    };
    let provenance = |second_minimum: Option<bool>| {
        let mut items = decoded_small_rectangle();
        let mut item = stream_extrude(
            1,
            "NewBodyFeatureOperation",
            json!({"operation_code": 4, "extent_a": 2, "extent_b": 1, "flag": 0,
                   "direction_vector": [0.0, 0.0, 1.0], "slot_roles": [17, 17, 65],
                   "solid": true, "start": 0, "sides": [2, 0],
                   "thin": false, "thin_location": 0}),
            None,
        );
        item["detail"]["extentType"] = json!("TwoSidesFeatureExtentType");
        let one = json!({"_type": "ToEntityExtentDefinition", "entity": face(-0.3),
                             "isMinimumSolution": false});
        let mut two = json!({"_type": "ToEntityExtentDefinition", "entity": face(0.5)});
        if let Some(m) = second_minimum {
            two["isMinimumSolution"] = json!(m);
        }
        item["detail"]["extentOne"] = one;
        item["detail"]["extentTwo"] = two;
        let mut doc = Document::new(TestKernel::default());
        item["detail"]["_f3d_profile_loops"] = items[1]["detail"]["_f3d_profile_loops"].clone();
        items[1] = item;
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        assert_eq!(
            report.items[1].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        report.items[1].input_provenance
    };
    assert_eq!(provenance(Some(false)), Some(InputProvenance::Read));
    assert_ne!(provenance(None), Some(InputProvenance::Read));
}

/// An extrusion up to a vertex (mitcad#1329): the vertex comes in by the
/// faces that meet there, and the extrusion goes up to the point, the
/// plane through it parallel to its sketch, and builds.
#[test]
fn an_extrusion_up_to_a_vertex_comes_in_up_to_the_point() {
    let block = || {
        vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        ]
    };
    // The block's top face as the replay names it: the only face of the
    // mock's block through its top corner whose plane it knows.
    let mut first = Document::new(MockKernel::default());
    import(&mut first, &timeline_dump(block()));
    let top = first
        .body_shape(BodyUid::new(FeatureUid(2), 0))
        .expect("the block")
        .faces
        .iter()
        .map(ToString::to_string)
        .find(|f| f.contains(":end("))
        .expect("a top face");
    let vertex = format!("V{{{top}}}")
        .parse::<mitcad_model::VertexName>()
        .unwrap()
        .to_string();
    // A second extrusion of the rectangle up to the block's corner at
    // (4, 0, 1) cm.
    let mut items = block();
    let mut item = extrude(2, 0, "NewBodyFeatureOperation", "d2");
    let detail = item["detail"].as_object_mut().unwrap();
    detail.remove("taperAngleOne");
    detail.insert(
        "extentOne".to_owned(),
        json!({"_type": "ToEntityExtentDefinition",
               "entity": {"kind": "vertex", "objectType": "BRepVertex", "point": [4.0, 0.0, 1.0]}}),
    );
    items.push(item);
    let mut doc = Document::new(MockKernel::default());
    doc.kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.clone(), [40.0, 0.0, 10.0]);
    let report = import(&mut doc, &timeline_dump(items));
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["extent"]["type"], "to_object", "{def}");
    assert_eq!(
        def["def"]["extent"]["object"],
        json!({"type": "point", "point": {"body": "F2.b0", "vertex": vertex}}),
        "{def}"
    );
    // The corner lies along the sketch's normal.
    assert!(!def["def"]["flip"].as_bool().unwrap_or(false), "{def}");
    assert_eq!(doc.status(FeatureUid(3)).and_then(|s| s.error()), None);
    assert!(doc.body_shape(BodyUid::new(FeatureUid(3), 0)).is_some());
}
