// SPDX-License-Identifier: MIT
//! Sweeps, pipes and lofts, and the faces their definitions refer to.

use super::*;
use mitcad_model_api::DocumentApi;

// Sweeps, pipes and lofts (F3).

#[test]
fn sweeps_pipes_and_lofts_of_the_add_in_replay() {
    // Sketch1: the 40 x 20 rectangle; Sketch2: a line, the path.
    let path_sketch = json!({
        "index": 1, "name": "Sketch2", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [{"id": "p0", "xyz": [0.0, 0.0, 0.0]}, {"id": "p1", "xyz": [0.0, 5.0, 0.0]}],
            "curves": [{"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1"}]
        }
    });
    let line = json!({"kind": "sketch_entity", "objectType": "SketchLine", "sketch": "Sketch2",
                      "sketch_timeline_index": 1, "id": "c0"});
    let profile = json!({"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0,
                         "area": 8.0, "centroid": [2.0, 1.0, 0.0]});
    let sweep = json!({"index": 2, "name": "Sweep1", "objectType": "SweepFeature", "detail": {
        "profile": [profile], "operation": "NewBodyFeatureOperation",
        "path": [{"_type": "PathEntity", "entity": line, "isOpposedToEntity": false}],
        "orientation": "ParallelOrientationType", "isSolid": true,
        "distanceOne": param("d7", "0.5", 0.5, ""), "distanceTwo": param("d8", "1", 1.0, ""),
        "twistAngle": param("d9", "30 deg", 0.5235987755982988, "deg"),
        "taperAngle": param("d10", "0 deg", 0.0, "deg"), "isDirectionFlipped": true}});
    let pipe = json!({"index": 3, "name": "Pipe1", "objectType": "PipeFeature", "detail": {
        "path": [{"_type": "PathEntity", "entity": line}], "sectionType": "SquarePipeSectionType",
        "sectionSize": param("d11", "8 mm", 0.8, "mm"), "isHollow": true,
        "sectionThickness": param("d12", "1 mm", 0.1, "mm"),
        "distanceOne": param("d13", "1", 1.0, ""), "distanceTwo": param("d14", "0", 0.0, ""),
        "operation": "NewBodyFeatureOperation"}});
    let loft = json!({"index": 4, "name": "Loft1", "objectType": "LoftFeature", "detail": {
        "loftSections": [
            {"_type": "LoftSection", "index": 1, "entity": {"kind": "construction_point", "name": "Origin",
             "origin": "Origin"}, "endCondition": {"_type": "PointSharpEndCondition"}},
            {"_type": "LoftSection", "index": 0, "entity": profile,
             "endCondition": {"_type": "FreeEndCondition"}}],
        "isClosed": false, "isSolid": true, "operation": "NewBodyFeatureOperation"}});
    let dump = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "sweeps.f3d"},
        "timeline": {"items": [rectangle_sketch(0), path_sketch, sweep, pipe, loft]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes[1..], [Outcome::Parametric; 4], "{}", report.text());
    let def = |uid: u64| {
        let query = json!({"query": "feature", "uid": format!("F{uid}")}).to_string();
        let answer: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
        answer["def"].clone()
    };
    let sweep = def(3);
    assert_eq!(sweep["type"], "sweep");
    assert_eq!(sweep["path"]["sketch"], "F2");
    assert_eq!(sweep["orientation"], "parallel");
    assert_eq!(sweep["flip"], true);
    assert_eq!(sweep["extent"]["type"], "partial");
    assert!(sweep.get("twist_angle").is_some() && sweep.get("taper_angle").is_none());
    let fraction = doc.parameters().get(
        doc.parameters()
            .find(sweep["extent"]["fraction"].as_str().unwrap())
            .unwrap(),
    );
    assert_eq!(fraction.map(|p| p.value()), Some(0.5));
    let pipe = def(4);
    assert_eq!(pipe["section"], "square");
    assert!(pipe.get("thickness").is_some() && pipe.get("extent").is_none());
    let loft = def(5);
    assert_eq!(loft["sections"][0]["type"], "profile");
    assert_eq!(
        loft["sections"][1],
        json!({"type": "point", "point": "origin"})
    );
    assert_eq!(loft["end_condition"], json!({"type": "point_sharp"}));
    assert!(loft.get("start_condition").is_none());
}

/// The stream decoder's sweeps, pipes and lofts (mitcad#34): profiles by
/// their sketch only, a second fraction 0, no direction, orientation,
/// section type or wall. Without a history the first reading is taken: a
/// region of the sketch, the whole path, a solid circular pipe.
#[test]
fn sweeps_pipes_and_lofts_of_the_stream_decoder() {
    let path_sketch = json!({
        "index": 1, "name": "Sketch2", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [{"id": "p0", "xyz": [0.0, 0.0, 0.0]}, {"id": "p1", "xyz": [0.0, 5.0, 0.0]}],
            "curves": [{"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1"}]
        }
    });
    let line = json!([{"_type": "PathEntity", "entity": {"kind": "sketch_entity",
        "objectType": "SketchLine", "sketch": "Sketch2", "sketch_timeline_index": 1, "id": "c0"}}]);
    let profile = json!({"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0});
    let sweep = json!({"index": 2, "name": "Sweep1", "objectType": "SweepFeature", "detail": {
        "profile": [profile], "operation": "NewBodyFeatureOperation", "path": line,
        "distanceOne": param("d7", "1", 1.0, ""), "distanceTwo": param("d8", "0", 0.0, ""),
        "twistAngle": param("d9", "0 deg", 0.0, "deg"),
        "taperAngle": param("d10", "0 deg", 0.0, "deg")}});
    let pipe = json!({"index": 3, "name": "Pipe1", "objectType": "PipeFeature", "detail": {
        "path": line, "sectionSize": param("d11", "8 mm", 0.8, "mm"),
        "sectionThickness": param("d12", "1 mm", 0.1, "mm"),
        "distanceOne": param("d13", "1", 1.0, ""), "distanceTwo": param("d14", "0", 0.0, ""),
        "operation": "NewBodyFeatureOperation"}});
    let loft = json!({"index": 4, "name": "Loft1", "objectType": "LoftFeature", "detail": {
        "operation": "NewBodyFeatureOperation",
        "loftSections": [
            {"_type": "LoftSection", "index": 0, "entity": profile,
             "endCondition": {"_type": "LoftFreeEndCondition"}},
            {"_type": "LoftSection", "index": 1, "entity": {"kind": "sketch_entity",
             "objectType": "SketchPoint", "sketch": "Sketch2", "sketch_timeline_index": 1, "id": "p1"},
             "endCondition": {"_type": "LoftPointTangentEndCondition",
                              "weight": param("d15", "1.5", 1.5, "")}}],
        "centerLineOrRails": [], "centerLineOrRails.isCenterLine": false}});
    let dump = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "sweeps.f3d"},
        "timeline": {"items": [rectangle_sketch(0), path_sketch, sweep, pipe, loft]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes[1..], [Outcome::Parametric; 4], "{}", report.text());
    let def = |uid: u64| {
        let query = json!({"query": "feature", "uid": format!("F{uid}")}).to_string();
        let answer: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
        answer["def"].clone()
    };
    let sweep = def(3);
    assert_eq!(sweep["profiles"][0]["sketch"], "F1");
    assert_eq!(sweep["path"]["sketch"], "F2");
    for key in ["extent", "flip", "orientation"] {
        assert!(sweep.get(key).is_none(), "{key}: {sweep}");
    }
    let pipe = def(4);
    for key in ["section", "thickness", "extent"] {
        assert!(pipe.get(key).is_none(), "{key}: {pipe}");
    }
    let loft = def(5);
    assert_eq!(loft["sections"][0]["type"], "profile");
    assert_eq!(loft["sections"][1]["type"], "point");
    assert_eq!(loft["sections"][1]["point"]["sketch"], "F2");
    assert_eq!(loft["end_condition"]["type"], "point_tangent");
}

/// Sweeps along guide surfaces (mitcad#157): faces found by their
/// fingerprints become `guide_surfaces` (a guide rail the stream decoder
/// also gives is not one); without the faces' geometry the item is not
/// replayed.
#[test]
fn sweeps_along_guide_surfaces_take_the_faces_found() {
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
    let path_sketch = json!({
        "index": 2, "name": "Sketch2", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [{"id": "p0", "xyz": [0.0, 0.0, 0.0]}, {"id": "p1", "xyz": [0.0, 5.0, 0.0]}],
            "curves": [{"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1"}]
        }
    });
    let line = json!([{"_type": "PathEntity", "entity": {"kind": "sketch_entity",
        "objectType": "SketchLine", "sketch": "Sketch2", "sketch_timeline_index": 2, "id": "c0"}}]);
    let sweep = |face: Value| {
        json!({"index": 3, "name": "Sweep1", "objectType": "SweepFeature", "detail": {
            "profile": [{"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0}],
            "operation": "NewBodyFeatureOperation", "path": line, "guideRail": line,
            "guideSurfaces": [face],
            "distanceOne": param("d7", "1", 1.0, ""), "distanceTwo": param("d8", "1", 1.0, ""),
            "twistAngle": param("d9", "30 deg", 0.5235987755982988, "deg")}})
    };
    let run = |face: Value| {
        let mut items = base.clone();
        items.extend([path_sketch.clone(), sweep(face)]);
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
            .filter(|d| d["type"] == "sweep");
        (report, def)
    };

    let (report, def) = run(json!({"kind": "face", "objectType": "BRepFace",
        "geometry": {"type": "Plane"}, "point_on_face": point,
        "_f3d": {"recipe": "bounded_face"}}));
    let item = report.items.iter().find(|i| i.name == "Sweep1").unwrap();
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    let def = def.expect("a sweep");
    assert_eq!(
        def["guide_surfaces"],
        json!([{"body": "F2.b0", "face": top.to_string()}])
    );
    for key in ["guide_rail", "twist_angle", "orientation"] {
        assert!(def.get(key).is_none(), "{key}: {def}");
    }

    // The stream decoder's faces by their names only: not replayed (no
    // stored bodies here to fall back on).
    let (report, def) = run(json!({"kind": "face", "objectType": "BRepFace",
        "_f3d": {"recipe": "bounded_face"}}));
    let item = report.items.iter().find(|i| i.name == "Sweep1").unwrap();
    assert_ne!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert!(
        item.note
            .as_deref()
            .is_some_and(|n| n.contains("guide surfaces' geometry was not decoded")),
        "{}",
        report.text()
    );
    assert!(def.is_none());
}

/// A face the decoder found by the file's own names is resolved uniquely
/// when it is the only face of the replay through its point that faces
/// the way the file's does; a face without the file's names, or one that
/// faces the other way, is not.
#[test]
fn a_decoded_face_is_resolved_uniquely_only_with_its_names_and_side() {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut doc = Document::new(TestKernel {
        face_listing: true,
        ..TestKernel::default()
    });
    import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    let top = doc.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "end")
        .cloned()
        .expect("a top face");
    let body = doc.bodies()[0].uid;
    let point = face_point(&top).map(|x| x / 10.0);
    let face = |extra: Value| {
        let mut map = json!({"kind": "face", "objectType": "BRepFace",
            "geometry": {"type": "Plane"}, "point_on_face": point,
            "normal_at_point": [1.0, 0.0, 0.0]});
        for (key, value) in extra.as_object().unwrap() {
            map[key] = value.clone();
        }
        match mitcad_dump::Reference::from_map(map.as_object().unwrap().clone()) {
            mitcad_dump::Reference::Face(fp) => *fp,
            other => panic!("{other:?}"),
        }
    };
    let named = json!({"_f3d": {"recipe": "bounded_face", "entities": [[{"tag": "9", "kind": 0, "ops": [5]}]]}});
    let fp = face(named.clone());
    let (found_body, found) =
        mitcad_import_replay::internal::refs::resolve_face(&doc, &fp).expect("the face");
    assert_eq!((found_body, &found), (body, &top));
    assert!(mitcad_import_replay::internal::refs::face_resolved_uniquely(&doc, &fp, (body, &top)));
    // Without the file's names nothing says the face is the file's.
    let unnamed = face(json!({}));
    assert!(
        !mitcad_import_replay::internal::refs::face_resolved_uniquely(&doc, &unnamed, (body, &top))
    );
    // Facing the other way than the replay's face.
    let opposed = face(json!({"normal_at_point": [-1.0, 0.0, 0.0],
                              "_f3d": named["_f3d"].clone()}));
    assert!(
        !mitcad_import_replay::internal::refs::face_resolved_uniquely(&doc, &opposed, (body, &top))
    );
}

// Lofts with end conditions and rails (P2).

#[test]
fn lofts_with_end_conditions_and_rails_come_in_as_definitions() {
    // Sketch1: the rectangle; Sketch2: a line, the rail.
    let path_sketch = json!({
        "index": 1, "name": "Sketch2", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [{"id": "p0", "xyz": [0.0, 0.0, 0.0]}, {"id": "p1", "xyz": [0.0, 5.0, 0.0]}],
            "curves": [{"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1"}]
        }
    });
    let line = json!({"kind": "sketch_entity", "objectType": "SketchLine", "sketch": "Sketch2",
                      "sketch_timeline_index": 1, "id": "c0"});
    let profile = json!({"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0,
                         "area": 8.0, "centroid": [2.0, 1.0, 0.0]});
    let loft = json!({"index": 2, "name": "Loft1", "objectType": "LoftFeature", "detail": {
        "loftSections": [
            {"_type": "LoftSection", "index": 0, "entity": profile,
             "endCondition": {"_type": "DirectionEndCondition",
                              "angle": param("d20", "10 deg", 0.17453292519943295, "deg"),
                              "weight": param("d21", "2", 2.0, "")}},
            {"_type": "LoftSection", "index": 1, "entity": {"kind": "construction_point", "name": "Origin",
             "origin": "Origin"},
             "endCondition": {"_type": "PointTangentEndCondition", "weight": param("d22", "0.5", 0.5, "")}}],
        "centerLineOrRails": [[{"_type": "PathEntity", "entity": line}]],
        "isClosed": false, "isSolid": true, "operation": "NewBodyFeatureOperation"}});
    let dump = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "lofts.f3d"},
        "timeline": {"items": [rectangle_sketch(0), path_sketch, loft]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let query = json!({"query": "feature", "uid": "F3"}).to_string();
    let answer: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
    let def = &answer["def"];
    assert_eq!(def["start_condition"]["type"], "direction");
    assert_eq!(def["end_condition"]["type"], "point_tangent");
    assert_eq!(def["rails"][0]["sketch"], "F2");
    let value = |slot: &Value| {
        let parameters = doc.parameters();
        parameters
            .get(parameters.find(slot.as_str().unwrap()).unwrap())
            .map(|p| p.value())
    };
    assert_eq!(value(&def["start_condition"]["weight"]), Some(2.0));
    assert_eq!(value(&def["end_condition"]["weight"]), Some(0.5));
    // Mitcad's own rules for end conditions: matched against
    // the history more loosely.
    assert_eq!(
        Candidate::new(def.clone()).tolerance(),
        history::CONVENTIONS
    );
    let mut free = def.clone();
    for key in ["start_condition", "end_condition", "rails"] {
        free.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(Candidate::new(free).tolerance(), history::RELATIVE);
}

#[test]
fn closed_and_surface_lofts_come_in_as_definitions() {
    let section = |i: i64| {
        json!({"_type": "LoftSection", "index": i,
               "entity": {"kind": "profile", "sketch": format!("Sketch{}", i + 1),
                          "sketch_timeline_index": i, "area": 8.0, "centroid": [2.0, 1.0, 0.0]}})
    };
    for (closed, solid) in [(true, true), (false, false)] {
        let loft = json!({"index": 3, "name": "Loft1", "objectType": "LoftFeature", "detail": {
            "loftSections": [section(0), section(1), section(2)],
            "isClosed": closed, "isSolid": solid, "operation": "NewBodyFeatureOperation"}});
        let dump = dump(json!({
            "schema": "mitcad-f3d-dump", "schema_version": 2,
            "source": {"mode": "f3d_stream", "file": "lofts.f3d"},
            "timeline": {"items": [rectangle_sketch(0), rectangle_sketch(1), rectangle_sketch(2), loft]}
        }));
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        assert_eq!(
            report.items[3].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        let query = json!({"query": "feature", "uid": "F4"}).to_string();
        let answer: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
        let def = &answer["def"];
        assert_eq!(def["closed"] == json!(true), closed, "{def}");
        assert_eq!(def["surface"] == json!(true), !solid, "{def}");
    }
}

#[test]
fn a_loft_merges_its_tangent_edges_where_the_dump_says_so() {
    // Tangent Edges (mitcad#2162): the dump's isTangentEdgesMerged true is
    // `merge`; false or left out keeps them (the field left out).
    let section = |i: i64| {
        json!({"_type": "LoftSection", "index": i,
               "entity": {"kind": "profile", "sketch": format!("Sketch{}", i + 1),
                          "sketch_timeline_index": i, "area": 8.0, "centroid": [2.0, 1.0, 0.0]}})
    };
    for merged in [Some(true), Some(false), None] {
        let mut detail = json!({"loftSections": [section(0), section(1)], "isClosed": false,
                                "isSolid": true, "operation": "NewBodyFeatureOperation"});
        if let Some(merged) = merged {
            detail["isTangentEdgesMerged"] = json!(merged);
        }
        let loft =
            json!({"index": 2, "name": "Loft1", "objectType": "LoftFeature", "detail": detail});
        let dump = dump(json!({
            "schema": "mitcad-f3d-dump", "schema_version": 2,
            "source": {"mode": "f3d_stream", "file": "lofts.f3d"},
            "timeline": {"items": [rectangle_sketch(0), rectangle_sketch(1), loft]}
        }));
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        assert_eq!(
            report.items[2].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        let query = json!({"query": "feature", "uid": "F3"}).to_string();
        let answer: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
        let def = &answer["def"];
        let expected = if merged == Some(true) {
            json!("merge")
        } else {
            Value::Null
        };
        assert_eq!(def["tangent_edges"], expected, "{def}");
    }
}

#[test]
fn a_loft_section_of_edges_is_the_face_they_go_round() {
    // A tangent condition is taken only at a section of edges: here the
    // loop round the block's top, as a Path of its BRepEdges (cm).
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
    let block = shapes(&reference)[0].clone();
    let top = block
        .faces
        .iter()
        .find(|f| f.to_string().starts_with("F2:end("))
        .unwrap()
        .clone();
    let edge = |a: &mitcad_model::FaceName, b: &mitcad_model::FaceName| {
        let p = edge_point(a, b);
        json!({"_type": "PathEntity", "entity": {"kind": "edge", "objectType": "BRepEdge",
               "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0], "length": 0.1}})
    };
    let round_top: Vec<Value> = block
        .edges
        .iter()
        .filter(|(a, b)| *a == top || *b == top)
        .map(|(a, b)| edge(a, b))
        .collect();
    assert_eq!(round_top.len(), 4);
    let loft = |entity: Value| {
        json!({"index": 2, "name": "Loft1", "objectType": "LoftFeature", "detail": {
            "loftSections": [
                {"_type": "LoftSection", "index": 0, "entity": entity,
                 "endCondition": {"_type": "LoftTangentEndCondition", "weight": param("d9", "1", 1.0, "")}},
                {"_type": "LoftSection", "index": 1, "entity": {"kind": "construction_point",
                 "name": "Origin", "origin": "Origin"},
                 "endCondition": {"_type": "LoftPointSharpEndCondition"}}],
            "isClosed": false, "isSolid": true, "operation": "NewBodyFeatureOperation"}})
    };
    let replay = |entity: Value| {
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        let mut items = block_items.clone();
        items.push(loft(entity));
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        let def = report.items[2].features.first().map(|uid| {
            serde_json::from_str::<Value>(
                &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                    .unwrap(),
            )
            .unwrap()["def"]
                .clone()
        });
        (report, def)
    };
    let (report, def) = replay(json!(round_top));
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def = def.unwrap();
    assert_eq!(
        def["sections"][0],
        json!({"type": "face", "body": "F2.b0", "face": top.to_string()})
    );
    // The dump's type names (`LoftTangentEndCondition`).
    assert_eq!(def["start_condition"]["type"], "tangent");
    assert_eq!(def["end_condition"], json!({"type": "point_sharp"}));
    // One edge of the loop (a BRepEdge as the section) goes round no face.
    let (report, _) = replay(round_top[0]["entity"].clone());
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
    assert!(
        report.text().contains("its edges do not go round one face"),
        "{}",
        report.text()
    );
}
