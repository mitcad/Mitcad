// SPDX-License-Identifier: MIT
//! Sketch contents: texts, polygons, sketch patterns, offsets and curve flags.

use super::*;
use mitcad_model_api::DocumentApi;

#[test]
fn sketch_texts_keep_style_frame_and_path() {
    // A text in a frame of four construction lines turned 90 degrees, and a
    // text fitted on an arc below it (cm in the IR).
    let p = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let line = |id: &str, a: &str, b: &str| {
        json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b,
               "isConstruction": true})
    };
    let sketch = json!({
        "index": 0, "name": "Sketch1", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [p("p0", 0.0, 0.0), p("p1", 0.0, 4.0), p("p2", -2.0, 4.0), p("p3", -2.0, 0.0),
                       p("p4", 5.0, 0.0), p("p5", 7.0, 0.0), p("p6", 3.0, 0.0)],
            "curves": [line("c0", "p0", "p1"), line("c1", "p1", "p2"),
                       line("c2", "p2", "p3"), line("c3", "p3", "p0"),
                       {"id": "c4", "type": "SketchArc", "centerSketchPoint": "p4",
                        "startSketchPoint": "p5", "endSketchPoint": "p6"}],
            "texts": [
                {"id": "t0", "text": "AB", "height": 0.5, "position": [0.0, 0.0, 0.0],
                 "angle": std::f64::consts::FRAC_PI_2, "fontName": "Arial", "textStyle": 3,
                 "isHorizontalFlip": true,
                 "definition": {"_type": "MultiLineTextDefinition",
                                "rectangleLines": [{"kind": "sketch_entity", "id": "c0"},
                                                   {"kind": "sketch_entity", "id": "c1"},
                                                   {"kind": "sketch_entity", "id": "c2"},
                                                   {"kind": "sketch_entity", "id": "c3"}],
                                "horizontalAlignment": 1, "verticalAlignment": "MiddleVerticalAlignment",
                                "characterSpacing": 10.0}},
                {"id": "t1", "text": "C", "height": 0.4, "position": [5.0, 2.0, 0.0],
                 "definition": {"_type": "FitOnPathTextDefinition",
                                "path": {"kind": "sketch_entity", "id": "c4"}, "isAbovePath": false}}
            ]
        }
    });
    let mut doc = Document::new(MockKernel::default());
    import(&mut doc, &timeline_dump(vec![sketch]));
    let s: Value =
        serde_json::from_str(&doc.query(r#"{"query": "sketch", "uid": "F1"}"#).unwrap()).unwrap();
    let t = &s["texts"][0];
    assert_eq!(
        (t["bold"].clone(), t["italic"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(t["flip_x"], true);
    assert_eq!(t["height"], 5.0);
    assert_eq!(
        (t["align"].clone(), t["valign"].clone()),
        (json!("center"), json!("middle"))
    );
    assert_eq!(t["spacing"], 10.0);
    // Turned a quarter: the baseline runs up from (0, 0) along x = 0, and
    // the text's up is -x.
    let id = |stored: &str| {
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| {
                let at = &e["at"];
                e["type"] == "point"
                    && match stored {
                        "p0" => at == &json!([0.0, 0.0]),
                        "p1" => at == &json!([0.0, 40.0]),
                        _ => at == &json!([-20.0, 0.0]),
                    }
            })
            .unwrap()["id"]
            .clone()
    };
    assert_eq!(t["frame"], json!([id("p0"), id("p1"), id("p3")]));
    let path = &s["texts"][1]["path"];
    assert_eq!(
        (path["above"].clone(), path["fit"].clone()),
        (json!(false), json!(true))
    );
    assert!(path["curve"].as_str().unwrap().starts_with('c'));
    assert!(
        s["regions"].as_array().unwrap().len() > 2,
        "the letters bound regions"
    );
}

#[test]
fn a_text_below_a_circle_starts_where_the_files_circle_starts() {
    // mitcad#1177: a left-aligned text below a construction circle (the
    // file's text above the circle turned round) ends where the file's
    // circle starts as a path: Mitcad's text hangs below the circle, right
    // aligned, the circle starting there.
    let p = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let start = -84f64.to_radians();
    let sketch = json!({
        "index": 0, "name": "Sketch1", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [p("p0", 0.0, 0.0)],
            "curves": [{"id": "c0", "type": "SketchCircle", "centerSketchPoint": "p0",
                        "radius": 4.25, "isConstruction": true,
                        "geometry": {"type": "Circle3D", "center": [0.0, 0.0, 0.0],
                                     "normal": [0.0, 0.0, 1.0], "radius": 4.25}}],
            "texts": [
                {"id": "t0", "text": "1/16", "height": 0.5, "fontName": "Arial",
                 "definition": {"_type": "AlongPathTextDefinition",
                                "path": {"kind": "sketch_entity", "id": "c0"},
                                "isAbovePath": false,
                                "horizontalAlignment": "LeftHorizontalAlignment"},
                 "_f3d": {"kind": 1, "pathStart": start}}
            ]
        }
    });
    let mut doc = Document::new(MockKernel::default());
    import(&mut doc, &timeline_dump(vec![sketch]));
    let s: Value =
        serde_json::from_str(&doc.query(r#"{"query": "sketch", "uid": "F1"}"#).unwrap()).unwrap();
    let t = &s["texts"][0];
    assert_eq!(t["align"], "right", "{t}");
    assert_eq!(t["path"]["above"], false, "{t}");
    assert_eq!(t["no_kerning"], true, "{t}");
    let circle = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "circle")
        .unwrap();
    assert_eq!(t["path"]["curve"], circle["id"]);
    let at = circle["start_angle"].as_f64().unwrap();
    assert!((at - start).abs() < 1e-12, "{circle}");
}

#[test]
fn a_text_in_a_box_needs_no_position_of_its_own() {
    // The file's box: its first line runs along the text's baseline (here
    // towards -x, a half turn) from the text's corner; the API gives the
    // text no position and angle. A text without a box has no place.
    let p = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let line = |id: &str, a: &str, b: &str| {
        json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b,
               "isConstruction": true})
    };
    let sketch = json!({
        "index": 0, "name": "Sketch1", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [p("p0", 4.0, 2.0), p("p1", 3.0, 2.0), p("p2", 3.0, 1.0), p("p3", 4.0, 1.0)],
            "curves": [line("c0", "p0", "p1"), line("c1", "p1", "p2"),
                       line("c2", "p2", "p3"), line("c3", "p3", "p0")],
            "texts": [
                {"id": "t0", "text": "A B", "height": 0.3, "fontName": "Arial",
                 "lineSpacing": 0.25, "penAligned": true, "inkTop": true,
                 "glyphs": [
                     {"advance": 0.2, "contours": [[[[0.0, 0.0], [0.15, 0.0]],
                                                    [[0.15, 0.0], [0.075, 0.21]],
                                                    [[0.075, 0.21], [0.0, 0.0]]]]},
                     {"advance": 0.0},
                     {"advance": 0.2, "contours": [[[[0.0, 0.0], [0.15, 0.0]],
                                                    [[0.15, 0.0], [0.15, 0.21]],
                                                    [[0.15, 0.21], [0.0, 0.0]]]]}
                 ],
                 "definition": {"_type": "MultiLineTextDefinition",
                                "rectangleLines": [{"kind": "sketch_entity", "id": "c0"},
                                                   {"kind": "sketch_entity", "id": "c1"},
                                                   {"kind": "sketch_entity", "id": "c2"},
                                                   {"kind": "sketch_entity", "id": "c3"}],
                                "horizontalAlignment": "LeftHorizontalAlignment",
                                "verticalAlignment": "TopVerticalAlignment"}},
                {"id": "t1", "text": "X", "height": 0.3,
                 "definition": {"_type": "MultiLineTextDefinition"}}
            ]
        }
    });
    let mut doc = Document::new(MockKernel::default());
    import(&mut doc, &timeline_dump(vec![sketch]));
    let s: Value =
        serde_json::from_str(&doc.query(r#"{"query": "sketch", "uid": "F1"}"#).unwrap()).unwrap();
    let texts = s["texts"].as_array().unwrap();
    assert_eq!(texts.len(), 1, "{s}");
    let t = &texts[0];
    assert_eq!(
        (t["cap_height"].clone(), t["wrap"].clone()),
        (json!(true), json!(true))
    );
    // The `.ipt` decoder's measures (mitcad#414).
    assert_eq!(
        (
            t["line_spacing"].clone(),
            t["pen_align"].clone(),
            t["ink_top"].clone()
        ),
        (json!(2.5), json!(true), json!(true))
    );
    // The glyphs the file stores (mitcad#416) are the shapes while the font
    // is missing (the mock kernel has no Arial): the A a triangle 1.5 mm
    // wide and 2.1 mm high.
    assert_eq!(
        (
            t["has_glyphs"].clone(),
            t["stored_outlines"].clone(),
            t["family"].clone()
        ),
        (json!(true), json!(true), json!("Arial"))
    );
    let id = t["id"].as_str().unwrap();
    let area = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == format!("r{{{id}.g0.c0}}"))
        .unwrap_or_else(|| panic!("{}", s["regions"]))["area"]
        .as_f64()
        .unwrap();
    assert!((area - 1.575).abs() < 1e-9, "{area}");
    assert!((t["angle"].as_f64().unwrap() - std::f64::consts::PI).abs() < 1e-9);
    let at = |x: f64, y: f64| {
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "point" && e["at"] == json!([x, y]))
            .unwrap()["id"]
            .clone()
    };
    // The corner, the corner along the baseline and the one up (-y).
    assert_eq!(
        t["frame"],
        json!([at(40.0, 20.0), at(30.0, 20.0), at(40.0, 10.0)])
    );
}

/// mitcad#6: the file's light bulbs are kept where they differ from Mitcad's
/// default (a sketch hidden while a feature uses it, construction geometry
/// shown), and only there; unknown ones follow the default.
#[test]
fn light_bulbs_are_kept_where_they_differ_from_the_default() {
    let sketch = |index: i64, bulb: Option<bool>| {
        let mut s = rectangle_sketch(index);
        s["detail"]["dimensions"] = json!([]);
        if let Some(on) = bulb {
            s["props"] = json!({"isLightBulbOn": on});
        }
        s
    };
    let plane = |index: i64, on: bool| {
        json!({"index": index, "name": format!("Plane{index}"), "objectType": "ConstructionPlane",
               "detail": {"geometry": {"type": "Plane", "origin": [0.0, 0.0, 1.0],
                                       "normal": [0.0, 0.0, 1.0]}},
               "props": {"isLightBulbOn": on}})
    };
    let items = vec![
        sketch(0, Some(true)), // Sketch1: used and shown: kept
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        sketch(2, Some(false)), // Sketch3: unused and hidden: kept
        sketch(3, Some(false)), // Sketch4: used and hidden: the default
        extrude(4, 3, "NewBodyFeatureOperation", "d2"),
        sketch(5, Some(true)), // Sketch6: unused and shown: the default
        sketch(6, None),       // Sketch7: not known: the default
        plane(7, false),       // hidden: kept
        plane(8, true),        // shown: the default
    ];
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &timeline_dump(items));
    let kept: Vec<(String, bool)> = doc
        .features()
        .filter_map(|f| Some((f.name.clone(), doc.feature_visible(f.uid)?)))
        .collect();
    assert_eq!(
        kept,
        [
            ("Sketch1".to_owned(), true),
            ("Sketch3".to_owned(), false),
            ("Plane7".to_owned(), false)
        ],
        "{}",
        report.text()
    );
    assert_eq!(
        report.light_bulbs,
        LightBulbReport {
            sketch_bulbs: 5,
            sketch_bulbs_unknown: 1,
            construction_bulbs: 2,
            construction_bulbs_unknown: 0,
            bulbs_set: 3,
        }
    );
    assert!(
        report
            .text()
            .contains("light bulbs: 5 sketches (1 unknown)")
    );
    let uid = |name: &str| doc.features().find(|f| f.name == name).unwrap().uid;
    let shown = |doc: &Document<MockKernel>, name: &str| {
        doc.feature_shown(doc.features().find(|f| f.name == name).unwrap().uid)
    };
    assert_eq!(shown(&doc, "Sketch4"), Some(false));
    assert_eq!(shown(&doc, "Sketch6"), Some(true));
    assert_eq!(shown(&doc, "Sketch7"), Some(true));
    assert_eq!(shown(&doc, "Plane8"), Some(true));
    // A sketch that follows the default shows when its consumer goes.
    let extrude4 = uid("Extrude4");
    doc.delete_feature(extrude4, false).unwrap();
    assert_eq!(shown(&doc, "Sketch4"), Some(true));
}

/// The circle whose centre is the point at `center` and whose radius is
/// `radius` (sketch mm).
fn circle_id(s: &Value, center: [f64; 2], radius: f64) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "circle"
                && e["center"]
                    .as_str()
                    .is_some_and(|c| (at(s, c)[0] - center[0]).abs() < 1e-6)
                && (e["geometry"]["radius"].as_f64().unwrap() - radius).abs() < 1e-6
        })
        .unwrap_or_else(|| panic!("a circle of {radius} at {center:?}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_polygon_stays_regular() {
    // A hexagon of radius 10 mm about the fixed origin, dimensioned centre
    // to corner.
    let corner = |i: usize| {
        let a = std::f64::consts::FRAC_PI_3 * i as f64;
        gpoint(&format!("p{i}"), a.cos(), a.sin())
    };
    let mut points: Vec<Value> = (0..6).map(corner).collect();
    points.push(gfixed("p6", 0.0, 0.0));
    let curves = (0..6)
        .map(|i| {
            gline(
                &format!("c{i}"),
                &format!("p{i}"),
                &format!("p{}", (i + 1) % 6),
            )
        })
        .collect();
    let dump = group_sketch_dump(
        vec![json!({"name": "d1", "expression": "10 mm", "value": 1.0, "unit": "mm"})],
        points,
        curves,
        json!([{"id": "k0", "type": "PolygonConstraint",
                "refs": {"entities": ["p0", "p1", "p2", "p3", "p4", "p5", "p6"]}}]),
        json!([{"id": "d0", "type": "SketchLinearDimension",
                "parameter": param("d1", "10 mm", 1.0, "mm"),
                "refs": {"entities": ["p6", "p0"]}}]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let corners: Vec<String> = (0..6)
        .map(|i| {
            let a = std::f64::consts::FRAC_PI_3 * i as f64;
            point_at(&s, [10.0 * a.cos(), 10.0 * a.sin()])
        })
        .collect();
    let center = point_at(&s, [0.0, 0.0]);
    // A construction circle through the corners and equal sides.
    let circles = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "circle" && e["construction"] == true)
        .count();
    assert_eq!(circles, 1);
    let kinds = |t: &str| {
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["type"] == t)
            .count()
    };
    assert_eq!((kinds("coincident"), kinds("equal")), (6, 5));
    doc.set_parameter_expression("d1", "15 mm", None).unwrap();
    let s = sketch_query(&doc);
    let c = at(&s, &center);
    for i in 0..6 {
        let (a, b) = (at(&s, &corners[i]), at(&s, &corners[(i + 1) % 6]));
        assert!(((a[0] - c[0]).hypot(a[1] - c[1]) - 15.0).abs() < 1e-6);
        assert!(((a[0] - b[0]).hypot(a[1] - b[1]) - 15.0).abs() < 1e-6);
    }
}

#[test]
fn circular_pattern_copies_follow_their_original() {
    // A circle 20 mm from the fixed centre, and its copies at 120 and 240
    // degrees.
    let s3 = 3f64.sqrt();
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", 2.0, 0.0),
        gpoint("p2", -1.0, s3),
        gpoint("p3", -1.0, -s3),
    ];
    let curves = vec![
        gcircle("c0", "p1", 0.3),
        gcircle("c1", "p2", 0.3),
        gcircle("c2", "p3", 0.3),
    ];
    let tau = std::f64::consts::TAU;
    let dump = group_sketch_dump(
        vec![
            json!({"name": "d1", "expression": "20 mm", "value": 2.0, "unit": "mm"}),
            json!({"name": "d6", "expression": "3", "value": 3.0, "unit": ""}),
            json!({"name": "d7", "expression": "360 deg", "value": tau, "unit": "deg"}),
        ],
        points,
        curves,
        json!([{"id": "k0", "type": "CircularPatternConstraint",
                "refs": {"entities": ["c0", "c1", "c2", "p0"]},
                "props": {"centerPoint": "p0", "createdEntities": ["c1", "c2"],
                          "quantity": param("d6", "3", 3.0, ""),
                          "totalAngle": param("d7", "360 deg", tau, "deg")}}]),
        json!([{"id": "d0", "type": "SketchLinearDimension",
                "parameter": param("d1", "20 mm", 2.0, "mm"),
                "refs": {"entities": ["p0", "p1"]}}]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let pattern = &s["patterns"][0];
    assert_eq!(
        (pattern["type"].clone(), pattern["count"].clone()),
        (json!("circular"), json!(3))
    );
    assert_eq!(pattern["copies"].as_array().unwrap().len(), 2);
    let copy = point_at(&s, [-10.0, 10.0 * s3]);
    doc.set_parameter_expression("d1", "30 mm", None).unwrap();
    let s = sketch_query(&doc);
    let p = at(&s, &copy);
    assert!(
        (p[0] + 15.0).abs() < 1e-6 && (p[1] - 15.0 * s3).abs() < 1e-6,
        "{p:?}"
    );
}

#[test]
fn rectangular_pattern_with_a_negative_distance() {
    // A circle at the fixed origin and its copies 20 and 40 mm to the left
    // (direction +x, distance -20 mm).
    let curves = vec![
        gcircle("c0", "p0", 0.5),
        gcircle("c1", "p1", 0.5),
        gcircle("c2", "p2", 0.5),
    ];
    let params = vec![
        json!({"name": "d6", "expression": "3", "value": 3.0, "unit": ""}),
        json!({"name": "d7", "expression": "1", "value": 1.0, "unit": ""}),
        json!({"name": "d8", "expression": "-20 mm", "value": -2.0, "unit": "mm"}),
        json!({"name": "d9", "expression": "0 mm", "value": 0.0, "unit": "mm"}),
    ];
    let constraints = json!([{"id": "k0", "type": "RectangularPatternConstraint",
        "refs": {"entities": ["c0", "c1", "c2"]},
        "props": {"createdEntities": ["c1", "c2"],
                  "quantityOne": param("d6", "3", 3.0, ""),
                  "quantityTwo": param("d7", "1", 1.0, ""),
                  "distanceOne": param("d8", "-20 mm", -2.0, "mm"),
                  "distanceTwo": param("d9", "0 mm", 0.0, "mm"),
                  "directionOne": [1.0, 0.0, 0.0], "directionTwo": [0.0, 1.0, 0.0],
                  "flags": [0, 0, 0]}}]);
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", -2.0, 0.0),
        gpoint("p2", -4.0, 0.0),
    ];
    let dump = group_sketch_dump(
        params.clone(),
        points,
        curves.clone(),
        constraints.clone(),
        json!([]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let pattern = &s["patterns"][0];
    assert_eq!(pattern["direction"], json!([-1.0, 0.0]));
    assert_eq!(pattern["count"], json!([3, 1]));
    let far = point_at(&s, [-40.0, 0.0]);
    doc.set_parameter_expression("d8", "-30 mm", None).unwrap();
    let s = sketch_query(&doc);
    let p = at(&s, &far);
    assert!((p[0] + 60.0).abs() < 1e-6 && p[1].abs() < 1e-6, "{p:?}");

    // Copies on both sides of the original: not what Mitcad's pattern
    // makes, so it is left out.
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", -2.0, 0.0),
        gpoint("p2", 2.0, 0.0),
    ];
    let dump = group_sketch_dump(params, points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.items[0].outcome, Outcome::Partial);
    assert!(
        report.items[0]
            .note
            .as_deref()
            .unwrap()
            .contains("not where the pattern puts them")
    );
}

#[test]
fn a_symmetric_rectangular_pattern_keeps_the_originals_in_the_middle() {
    // A circle at the fixed origin and its copies 20 mm on both sides
    // (the stream's flags say symmetric in direction one).
    let curves = vec![
        gcircle("c0", "p0", 0.5),
        gcircle("c1", "p1", 0.5),
        gcircle("c2", "p2", 0.5),
    ];
    let params = vec![
        json!({"name": "d6", "expression": "3", "value": 3.0, "unit": ""}),
        json!({"name": "d7", "expression": "1", "value": 1.0, "unit": ""}),
        json!({"name": "d8", "expression": "20 mm", "value": 2.0, "unit": "mm"}),
        json!({"name": "d9", "expression": "0 mm", "value": 0.0, "unit": "mm"}),
    ];
    let constraints = json!([{"id": "k0", "type": "RectangularPatternConstraint",
        "refs": {"entities": ["c0", "c1", "c2"]},
        "props": {"createdEntities": ["c1", "c2"],
                  "quantityOne": param("d6", "3", 3.0, ""),
                  "quantityTwo": param("d7", "1", 1.0, ""),
                  "distanceOne": param("d8", "20 mm", 2.0, "mm"),
                  "distanceTwo": param("d9", "0 mm", 0.0, "mm"),
                  "directionOne": [1.0, 0.0, 0.0], "directionTwo": [0.0, 1.0, 0.0],
                  "flags": [0, 1, 0]}}]);
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", -2.0, 0.0),
        gpoint("p2", 2.0, 0.0),
    ];
    let dump = group_sketch_dump(params, points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let pattern = &s["patterns"][0];
    assert_eq!(pattern["count"], json!([3, 1]), "{s}");
    assert_eq!(pattern["symmetric"], json!([true, false]), "{s}");
    let right = point_at(&s, [20.0, 0.0]);
    let left = point_at(&s, [-20.0, 0.0]);
    // A new spacing moves both copies, the originals stay.
    doc.set_parameter_expression("d8", "30 mm", None).unwrap();
    let s = sketch_query(&doc);
    let (r, l) = (at(&s, &right), at(&s, &left));
    assert!((r[0] - 30.0).abs() < 1e-6 && r[1].abs() < 1e-6, "{r:?}");
    assert!((l[0] + 30.0).abs() < 1e-6 && l[1].abs() < 1e-6, "{l:?}");
    assert_eq!(at(&s, "p1"), [0.0, 0.0]);
}

#[test]
fn offsets_keep_their_distance() {
    // A fixed 40 x 20 mm rectangle and its offset 5 mm inside (distance
    // parameter -5 mm); a circle of radius 10 mm and its offset 5 mm out,
    // and a third circle 5 mm outside that (a concentric circle dimension).
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gfixed("p1", 4.0, 0.0),
        gfixed("p2", 4.0, 2.0),
        gfixed("p3", 0.0, 2.0),
        gpoint("p4", 0.5, 0.5),
        gpoint("p5", 3.5, 0.5),
        gpoint("p6", 3.5, 1.5),
        gpoint("p7", 0.5, 1.5),
        gfixed("p8", 10.0, 0.0),
        gpoint("p9", 10.0, 0.0),
    ];
    let curves = vec![
        gline("c0", "p0", "p1"),
        gline("c1", "p1", "p2"),
        gline("c2", "p2", "p3"),
        gline("c3", "p3", "p0"),
        gline("c4", "p4", "p5"),
        gline("c5", "p5", "p6"),
        gline("c6", "p6", "p7"),
        gline("c7", "p7", "p4"),
        gcircle("c8", "p8", 1.0),
        gcircle("c9", "p9", 1.5),
        gcircle("c10", "p8", 2.0),
    ];
    let dump = group_sketch_dump(
        vec![
            json!({"name": "d1", "expression": "-5 mm", "value": -0.5, "unit": "mm"}),
            json!({"name": "d2", "expression": "5 mm", "value": 0.5, "unit": "mm"}),
            json!({"name": "d3", "expression": "5 mm", "value": 0.5, "unit": "mm"}),
            json!({"name": "d4", "expression": "20 mm", "value": 2.0, "unit": "mm"}),
        ],
        points,
        curves,
        json!([
            {"id": "k0", "type": "OffsetConstraint",
             "refs": {"entities": ["c0", "c4", "c1", "c5", "c2", "c6", "c3", "c7"]},
             "props": {"distance": -0.5, "dimension": "d0",
                       "parentCurves": ["c0", "c1", "c2", "c3"],
                       "childCurves": ["c4", "c5", "c6", "c7"]}},
            {"id": "k1", "type": "OffsetConstraint", "refs": {"entities": ["c8", "c9"]},
             "props": {"distance": 0.5, "dimension": "d1",
                       "parentCurves": ["c8"], "childCurves": ["c9"]}}
        ]),
        json!([
            {"id": "d0", "type": "SketchOffsetCurvesDimension",
             "parameter": param("d1", "-5 mm", -0.5, "mm"), "refs": {"entities": ["c0"]}},
            {"id": "d1", "type": "SketchOffsetCurvesDimension",
             "parameter": param("d2", "5 mm", 0.5, "mm"), "refs": {"entities": ["c8"]}},
            {"id": "d2", "type": "SketchConcentricCircleDimension",
             "parameter": param("d3", "5 mm", 0.5, "mm"), "refs": {"entities": ["c9", "c10"]},
             "textPosition": [10.0, 2.2, 0.0]},
            {"id": "d3", "type": "SketchDiameterDimension",
             "parameter": param("d4", "20 mm", 2.0, "mm"), "refs": {"entities": ["c8"]}}
        ]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let offsets = s["offsets"].as_array().unwrap();
    assert_eq!(offsets.len(), 2);
    // Inside the rectangle, which runs counter-clockwise: its left; the
    // distance is the magnitude of d1.
    assert_eq!(offsets[0]["left"], true);
    assert_eq!(offsets[0]["distance"], "d1_offset");
    assert_eq!(offsets[1].get("left"), None);
    assert_eq!(offsets[1]["distance"], "d2");
    let p = doc.parameters();
    let magnitude = p.get(p.find("d1_offset").unwrap()).unwrap();
    assert_eq!((magnitude.expression(), magnitude.value()), ("-(d1)", 5.0));
    let corner = point_at(&s, [5.0, 5.0]);
    let (c9, c10) = (
        circle_id(&s, [100.0, 0.0], 15.0),
        circle_id(&s, [100.0, 0.0], 20.0),
    );
    doc.set_parameter_expression("d1", "-8 mm", None).unwrap();
    doc.set_parameter_expression("d2", "7 mm", None).unwrap();
    let s = sketch_query(&doc);
    let p = at(&s, &corner);
    assert!(
        (p[0] - 8.0).abs() < 1e-6 && (p[1] - 8.0).abs() < 1e-6,
        "{p:?}"
    );
    assert!((radius(&s, &c9) - 17.0).abs() < 1e-6);
    assert!((radius(&s, &c10) - 22.0).abs() < 1e-6);
}

/// The flags the decoder reads from the sketch curves (mitcad#186): a
/// linked curve is a fixed reference entity, a fixed one is fixed, and
/// the three spline kinds come in with their stored NURBS data.
#[test]
fn curve_flags_and_spline_kinds_come_into_the_sketch() {
    let points = vec![
        gpoint("p0", 0.0, 0.0),
        gpoint("p1", 4.0, 0.0),
        gpoint("p2", 4.0, 2.0),
        gpoint("p3", 0.0, 5.0),
        gpoint("p4", 1.0, 5.0),
        gpoint("p5", 2.0, 5.0),
    ];
    let spline = |id: &str, kind: &str, y: f64, flags: Value| {
        let mut c = json!({
            "id": id, "type": kind,
            "geometry": {"type": "NurbsCurve3D", "nurbs": {
                "degree": 2, "isRational": false, "isPeriodic": false, "controlPointCount": 3,
                "knots": [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                "controlPoints": [[0.0, y, 0.0], [1.0, y + 1.0, 0.0], [2.0, y, 0.0]],
                "weights": []}}});
        for (k, v) in flags.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    let mut linked = gline("c0", "p0", "p1");
    linked["isLinked"] = json!(true);
    linked["isReference"] = json!(true);
    let mut fixed = gline("c1", "p1", "p2");
    fixed["isFixed"] = json!(true);
    let curves = vec![
        linked,
        fixed,
        spline("c2", "SketchFixedSpline", 10.0, json!({})),
        spline(
            "c3",
            "SketchFittedSpline",
            20.0,
            json!({"fitPoints": ["p3", "p4", "p5"]}),
        ),
        spline(
            "c4",
            "SketchControlPointSpline",
            30.0,
            json!({"isFixed": true}),
        ),
    ];
    let dump = group_sketch_dump(Vec::new(), points, curves, json!([]), json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let of = |kind: &str| -> Vec<&Value> {
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["type"] == kind)
            .collect()
    };
    let lines = of("line");
    assert_eq!(lines.len(), 2);
    let flag = |e: &Value, k: &str| e.get(k) == Some(&json!(true));
    // The line from (0, 0) is the linked one.
    let linked = lines
        .iter()
        .find(|l| flag(l, "reference"))
        .expect("a reference line");
    assert!(flag(linked, "fixed"));
    assert_eq!(lines.iter().filter(|l| flag(l, "fixed")).count(), 2);
    assert_eq!(lines.iter().filter(|l| flag(l, "reference")).count(), 1);
    // Three splines, the control-point one fixed as its flag says.
    let splines = of("spline");
    assert_eq!(splines.len(), 3);
    assert_eq!(splines.iter().filter(|e| flag(e, "fixed")).count(), 3);
}

#[test]
fn a_sketch_whose_points_lie_off_its_frame_is_placed_where_they_are() {
    // mitcad#169: a sketch on a face keeps its points 1.6 mm along its
    // normal from its stored frame; the sketch goes where they are.
    let mut sketch = rectangle_sketch(0);
    sketch["detail"]["referencePlane"] = json!({"kind": "face"});
    for p in sketch["detail"]["points"].as_array_mut().unwrap() {
        p["xyz"][2] = json!(0.16);
    }
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &timeline_dump(vec![sketch]));
    assert_ne!(
        report.items[0].outcome,
        Outcome::Skipped,
        "{}",
        report.text()
    );
    let uid = report.items[0]
        .features
        .iter()
        .filter_map(|f| f.parse::<mitcad_model::FeatureUid>().ok())
        .find(|uid| doc.sketch_output(*uid).is_some())
        .expect("the sketch");
    let origin = doc.sketch_output(uid).unwrap().frame.origin;
    assert!((origin[2] - 1.6).abs() < 1e-9, "{origin:?}");
}

#[test]
fn an_offset_without_curves_binds_nothing() {
    // The offset's curves are gone: its stored list is empty. The circle
    // keeps its other constraints and the sketch is whole.
    let points = vec![gfixed("p0", 0.0, 0.0)];
    let curves = vec![gcircle("c0", "p0", 2.0)];
    let constraints = json!([
        {"id": "k0", "type": "OffsetConstraint", "refs": {"entities": ["c0"]},
         "props": {"parentCurves": ["c0"], "childCurves": [], "distance": 0.5}},
    ]);
    let dump = group_sketch_dump(Vec::new(), points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    assert!(report.items[0].note.is_none(), "{}", report.text());
    let s = sketch_query(&doc);
    assert!(s["offsets"].as_array().unwrap().is_empty(), "{s}");
}

/// An offset with a round corner: the chain of two lines turns left, its
/// offset to the right goes round the corner by an arc of the distance
/// about it. The offset is Mitcad's derived one, computed from the chain,
/// and follows its distance.
#[test]
fn an_offset_with_a_round_corner_is_a_derived_offset() {
    let points = vec![
        gpoint("p0", 0.0, 0.0),
        gpoint("p1", 1.0, 0.0),
        gpoint("p2", 1.0, 1.0),
        gpoint("p3", 0.0, -0.1),
        gpoint("p4", 1.0, -0.1),
        gpoint("p5", 1.0, 0.0),
        gpoint("p6", 1.1, 0.0),
        gpoint("p7", 1.1, 1.0),
    ];
    let curves = vec![
        gline("c0", "p0", "p1"),
        gline("c1", "p1", "p2"),
        gline("c2", "p3", "p4"),
        json!({"id": "c3", "type": "SketchArc", "centerSketchPoint": "p5",
               "startSketchPoint": "p4", "endSketchPoint": "p6"}),
        gline("c4", "p6", "p7"),
    ];
    let dump = group_sketch_dump(
        vec![json!({"name": "d1", "expression": "1 mm", "value": 0.1, "unit": "mm"})],
        points,
        curves,
        json!([{"id": "k0", "type": "OffsetConstraint",
                "refs": {"entities": ["c0", "c2", "c1", "c4", "c3"]},
                "props": {"distance": 0.1, "dimension": "d0",
                          "parentCurves": ["c0", "c1"],
                          "childCurves": ["c2", "c3", "c4"]}}]),
        json!([{"id": "d0", "type": "SketchOffsetCurvesDimension",
                "parameter": param("d1", "1 mm", 0.1, "mm"), "refs": {"entities": ["c0"]}}]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let offsets = s["offsets"].as_array().unwrap();
    assert_eq!(offsets.len(), 1, "{s}");
    assert_eq!(offsets[0]["derived"], true);
    assert_eq!(offsets[0]["corners"].as_array().map(Vec::len), Some(1));
    assert_eq!(offsets[0]["distance"], "d1");
    // The distance drives the offset: the corner arc grows with it.
    let arc = offsets[0]["corners"][0]["arc"].as_str().unwrap().to_owned();
    doc.set_parameter_expression("d1", "2 mm", None).unwrap();
    let s = sketch_query(&doc);
    assert!((radius(&s, &arc) - 2.0).abs() < 1e-6, "{s}");
}

#[test]
fn text_parameters_are_imported_and_sketch_texts_follow_them() {
    // mitcad#664, a synthetic design: a user text parameter, one kept as
    // older files keep a text (without quotes), and the text parameters the
    // sketch made for its two texts (roles `Text1-Text`, `Text2-Text`), the
    // second's expression naming the user one.
    let sketch = json!({
        "index": 0, "name": "Sketch1", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [], "curves": [],
            "texts": [
                {"id": "t0", "text": "Other", "height": 0.5, "position": [0.0, 2.0, 0.0]},
                {"id": "t1", "text": "6qt lid", "height": 0.5, "position": [0.0, 0.0, 0.0]}
            ]
        }
    });
    let sketch_ref = json!({"kind": "feature", "objectType": "Sketch", "name": "Sketch1",
                            "timeline_index": 0});
    let source = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "texts.f3d"},
        "parameters": {
            "user": [
                {"name": "Label", "expression": "'6qt'", "unit": "Text", "comment": "Lid label"},
                {"name": "Note", "expression": "plain words", "unit": "Text"}
            ],
            "model": [
                {"name": "d1", "expression": "'Other'", "unit": "Text", "role": "Text1-Text",
                 "createdBy": sketch_ref},
                {"name": "d2", "expression": "Label & ' lid'", "unit": "Text",
                 "role": "Text2-Text", "createdBy": sketch_ref}
            ]
        },
        "timeline": {"items": [sketch]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &source);
    let params = doc.parameters();
    let text = |name: &str| {
        params
            .text(params.find(name).unwrap_or_else(|| panic!("no {name}")))
            .map(str::to_owned)
    };
    assert_eq!(text("Label").as_deref(), Some("6qt"));
    assert_eq!(text("d1").as_deref(), Some("Other"));
    assert_eq!(text("d2").as_deref(), Some("6qt lid"));
    // A text without quotes is taken as it is.
    assert_eq!(text("Note").as_deref(), Some("plain words"));
    assert_eq!(
        report.parameters.literal,
        ["Note"],
        "{:?}",
        report.parameters
    );
    assert!(
        report.parameters.skipped.is_empty() && report.parameters.mismatched.is_empty(),
        "{:?}",
        report.parameters
    );
    let label = params.get(params.find("Label").unwrap()).unwrap();
    assert_eq!(label.unit().to_string(), "Text");
    assert_eq!(label.comment(), "Lid label");
    // The texts follow their parameters, which the sketch owns.
    assert_eq!(
        params.owner(params.find("d2").unwrap()),
        Some(FeatureUid(1))
    );
    let s = sketch_query(&doc);
    let texts = s["texts"].as_array().unwrap();
    assert_eq!(texts.len(), 2, "{s}");
    let lid = texts.iter().find(|t| t["text"] == "6qt lid").unwrap();
    assert_eq!(lid["value"]["parameter"], "d2", "{s}");
    assert_eq!(lid["value"]["own"], true, "{s}");
    let other = texts.iter().find(|t| t["text"] == "Other").unwrap();
    assert_eq!(other["value"]["parameter"], "d1", "{s}");
    // A change of the user parameter changes the text.
    doc.command(r#"{"cmd": "set_parameter", "name": "Label", "expression": "'8qt'"}"#)
        .unwrap();
    let s = sketch_query(&doc);
    assert!(
        s["texts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["text"] == "8qt lid"),
        "{s}"
    );
}
