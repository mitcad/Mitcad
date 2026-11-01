// SPDX-License-Identifier: MIT
//! Sketch constraints and dimensions, and construction datums.

use super::*;
use mitcad_model_api::DocumentApi;

// Construction points, planes and axes from what they are made from
// (mitcad#186).

#[test]
fn construction_datums_follow_their_definitions() {
    let origin_plane = |name: &str| json!({"kind": "construction_plane", "name": name, "origin": name, "timeline_index": null});
    let point_ref = |index: i64| {
        json!({"kind": "construction_point", "name": format!("WorkPoint{}", index + 1),
               "origin": null, "timeline_index": index})
    };
    let fixed_point = |index: i64, at: [f64; 3]| {
        json!({"index": index, "name": format!("WorkPoint{}", index + 1),
               "objectType": "ConstructionPoint", "detail": {"geometry": at}})
    };
    let items = vec![
        // Where the three origin planes meet.
        json!({"index": 0, "name": "Center", "objectType": "ConstructionPoint",
               "detail": {"geometry": [0.0, 0.0, 0.0], "definition": {
                   "_type": "ConstructionPointThreePlanesDefinition",
                   "planeOne": origin_plane("XY"), "planeTwo": origin_plane("YZ"),
                   "planeThree": origin_plane("XZ")}}}),
        // Points the file stores only by their positions (cm).
        fixed_point(1, [0.3, 0.0, 0.0]),
        fixed_point(2, [0.0, 0.4, 0.0]),
        fixed_point(3, [0.0, 0.0, 0.5]),
        // The plane through three points: x/30 + y/40 + z/50 = 1.
        json!({"index": 4, "name": "Through", "objectType": "ConstructionPlane",
               "detail": {"definition": {"_type": "ConstructionPlaneThreePointsDefinition",
                                         "pointEntityOne": point_ref(1),
                                         "pointEntityTwo": point_ref(2),
                                         "pointEntityThree": point_ref(3)},
                          "geometry": {"type": "Plane", "origin": [0.3, 0.0, 0.0],
                                       "normal": [0.4, 0.3, 0.24]}}}),
        // Parallel to the XY plane through the third point.
        json!({"index": 5, "name": "Level", "objectType": "ConstructionPlane",
               "detail": {"definition": {"_type": "ConstructionPlaneOffsetThroughPointDefinition",
                                         "planarEntity": origin_plane("XY"),
                                         "point": point_ref(3)},
                          "geometry": {"type": "Plane", "origin": [0.0, 0.0, 0.5],
                                       "normal": [0.0, 0.0, 1.0]}}}),
        // Where the XY and XZ planes meet: the X axis, in the file's
        // direction whichever way Mitcad's rule runs.
        json!({"index": 6, "name": "Edge", "objectType": "ConstructionAxis",
               "detail": {"definition": {"_type": "ConstructionAxisTwoPlaneDefinition",
                                         "planarEntityOne": origin_plane("XY"),
                                         "planarEntityTwo": origin_plane("XZ")},
                          "geometry": {"type": "InfiniteLine3D", "origin": [0.0, 0.0, 0.0],
                                       "direction": [1.0, 0.0, 0.0]}}}),
    ];
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &timeline_dump(items));
    let read = |i: usize| report.items[i].input_provenance;
    for (i, outcome) in [
        (0, Outcome::Parametric),
        (1, Outcome::Partial),
        (2, Outcome::Partial),
        (3, Outcome::Partial),
        (4, Outcome::Parametric),
        (5, Outcome::Parametric),
        (6, Outcome::Parametric),
    ] {
        assert_eq!(report.items[i].outcome, outcome, "{}", report.text());
    }
    let def = |name: &str| {
        let uid = doc.features().find(|f| f.name == name).unwrap().uid;
        let v: Value = serde_json::from_str(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .unwrap(),
        )
        .unwrap();
        v["def"].clone()
    };
    // Each is the definition the file has, read from its inputs.
    assert_eq!(def("Center")["definition"]["type"], "three_planes");
    assert_eq!(read(0), Some(InputProvenance::Read));
    assert_eq!(def("Through")["definition"]["type"], "three_points");
    assert_eq!(read(4), Some(InputProvenance::Read));
    assert_eq!(def("Level")["definition"]["type"], "offset_through_point");
    assert_eq!(read(5), Some(InputProvenance::Read));
    assert_eq!(def("Edge")["definition"]["type"], "two_planes");
    // The axis lies on the file's line in the file's direction.
    let axis = doc.datum(doc.features().find(|f| f.name == "Edge").unwrap().uid);
    match axis {
        Some(mitcad_model::datum::Datum::Axis(a)) => {
            assert!((a.direction[0] - 1.0).abs() < 1e-9, "{a:?}");
        }
        other => panic!("{other:?}"),
    }
    let level = doc.datum(doc.features().find(|f| f.name == "Level").unwrap().uid);
    match level {
        Some(mitcad_model::datum::Datum::Plane(p)) => {
            assert!((p.origin[2] - 5.0).abs() < 1e-9, "{p:?}");
        }
        other => panic!("{other:?}"),
    }
}

/// mitcad#186: what ties a text box's lines to their text, or a control
/// point spline's polygon to its spline, constrains no geometry and
/// leaves the sketch whole; a horizontal line on construction lines is a
/// constraint as any other.
#[test]
fn text_box_and_control_polygon_constraints_leave_the_sketch_whole() {
    let line = |id: &str, a: &str, b: &str| {
        let mut c = gline(id, a, b);
        c["isConstruction"] = json!(true);
        c
    };
    let points = vec![
        gpoint("p0", 0.0, 0.0),
        gpoint("p1", 4.0, 0.0),
        gpoint("p2", 4.0, 1.0),
        gpoint("p3", 0.0, 1.0),
    ];
    let curves = vec![
        line("c0", "p0", "p1"),
        line("c1", "p1", "p2"),
        line("c2", "p2", "p3"),
        line("c3", "p3", "p0"),
    ];
    let constraints = json!([
        {"id": "k0", "type": "TextBoxConstraint",
         "refs": {"entities": ["c0", "c1", "c2", "c3"]}},
        {"id": "k1", "type": "ControlPolygonConstraint",
         "refs": {"entities": ["c0", "c1"]}},
        {"id": "k2", "type": "HorizontalConstraint", "refs": {"entities": ["c0"]}},
    ]);
    let dump = group_sketch_dump(Vec::new(), points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let kinds: Vec<&str> = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|k| k["type"].as_str())
        .collect();
    assert_eq!(kinds, ["horizontal"], "{s}");
}

#[test]
fn three_collinear_points_lie_on_the_line_between_two_of_them() {
    // A centre line from p1 to p2 and the point p0 that it passes: the
    // stored constraint names the three points, not the line.
    let points = vec![
        gpoint("p0", 0.0, 0.0),
        gpoint("p1", 0.0, 2.0),
        gpoint("p2", 0.0, -0.3),
        gpoint("p3", 1.0, 0.5),
    ];
    let curves = vec![gline("c0", "p1", "p2")];
    let constraints = json!([
        {"id": "k0", "type": "CollinearConstraint",
         "refs": {"entities": ["p1", "p0", "p2"]}},
    ]);
    let dump = group_sketch_dump(
        Vec::new(),
        points.clone(),
        curves.clone(),
        constraints,
        json!([]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let kept: Vec<&Value> = s["constraints"].as_array().unwrap().iter().collect();
    assert_eq!(kept.len(), 1, "{s}");
    assert_eq!(kept[0]["type"], "coincident", "{s}");
    let line = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "line")
        .unwrap();
    assert_eq!(kept[0]["entity"], line["id"], "{s}");
    assert_eq!(kept[0]["point"], point_at(&s, [0.0, 0.0]), "{s}");

    // No line between two of the points: the constraint is left out, and
    // the sketch says so.
    let constraints = json!([
        {"id": "k0", "type": "CollinearConstraint",
         "refs": {"entities": ["p1", "p0", "p3"]}},
    ]);
    let dump = group_sketch_dump(Vec::new(), points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert!(
        report.text().contains("collinear points without a line"),
        "{}",
        report.text()
    );
}

/// A sketch whose stored positions break one of its constraints (not
/// solved again after the geometry it follows changed) comes in with the
/// others active, that one inactive (mitcad#721), not without any.
#[test]
fn constraints_the_stored_positions_break_are_left_out_alone() {
    let points = vec![
        gpoint("p0", 0.0, 0.0),
        gpoint("p1", 4.0, 0.0),
        gpoint("p2", 4.0, 2.0),
        gpoint("p3", 0.0, 2.5),
    ];
    let curves = vec![
        gline("c0", "p0", "p1"),
        gline("c1", "p1", "p2"),
        gline("c2", "p2", "p3"),
    ];
    let constraints = json!([
        {"id": "k0", "type": "HorizontalConstraint", "refs": {"entities": ["c0"]}},
        {"id": "k1", "type": "VerticalConstraint", "refs": {"entities": ["c1"]}},
        // The file's p2 and p3 are 5 mm apart in height.
        {"id": "k2", "type": "HorizontalConstraint", "refs": {"entities": ["c2"]}},
    ]);
    let dump = group_sketch_dump(Vec::new(), points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(
        report.items[0].outcome,
        Outcome::Partial,
        "{}",
        report.text()
    );
    let note = report.items[0].note.as_deref().unwrap_or_default();
    assert!(
        note.contains("1 constraint inactive, not met in the file: horizontal"),
        "{note}"
    );
    let s = sketch_query(&doc);
    let kinds = |active: bool| -> Vec<&str> {
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|k| k["active"].as_bool().unwrap_or(true) == active)
            .filter_map(|k| k["type"].as_str())
            .collect()
    };
    assert_eq!(kinds(true), ["horizontal", "vertical"], "{s}");
    assert_eq!(kinds(false), ["horizontal"], "{s}");
    // The positions are the file's.
    assert_eq!(at(&s, &point_at(&s, [40.0, 20.0])), [40.0, 20.0]);
}

/// A profile's corner the file holds on a linked spline lies 1.3 µm off
/// the spline's stored control points (mitcad#396): solved with the
/// coincidence, the corner moves that far, within the import's geometric
/// tolerance (mitcad#224) and with the sketch's regions the file's, so the
/// coincidence stays and the sketch is parametric. 50 µm off, beyond the
/// tolerance, it comes in inactive (mitcad#721).
#[test]
fn a_corner_micrometres_off_a_linked_spline_keeps_its_coincidence() {
    for (off, kept) in [(1.3e-4, true), (5e-3, false)] {
        // The spline (4t, 4t(1 - t)) cm; its point at t = 1/4 is (1, 0.75).
        let mut spline = json!({
            "id": "c0", "type": "SketchFixedSpline", "isLinked": true, "isReference": true,
            "geometry": {"type": "NurbsCurve3D", "nurbs": {
                "degree": 2, "isRational": false, "isPeriodic": false, "controlPointCount": 3,
                "knots": [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                "controlPoints": [[0.0, 0.0, 0.0], [2.0, 2.0, 0.0], [4.0, 0.0, 0.0]],
                "weights": []}}});
        spline["startSketchPoint"] = json!("p0");
        spline["endSketchPoint"] = json!("p1");
        let points = vec![
            gpoint("p0", 0.0, 0.0),
            gpoint("p1", 4.0, 0.0),
            gpoint("p3", 1.0, 0.75 + off),
            gpoint("p4", 1.0, 3.0),
            gpoint("p5", 3.0, 3.0),
        ];
        let curves = vec![
            spline,
            gline("c1", "p3", "p4"),
            gline("c2", "p4", "p5"),
            gline("c3", "p5", "p3"),
        ];
        let constraints = json!([
            {"id": "k0", "type": "CoincidentConstraint", "refs": {"entities": ["p3", "c0"]}},
            {"id": "k1", "type": "VerticalConstraint", "refs": {"entities": ["c1"]}},
        ]);
        let dump = group_sketch_dump(Vec::new(), points, curves, constraints, json!([]));
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        let note = report.items[0].note.clone().unwrap_or_default();
        let s = sketch_query(&doc);
        let kinds: Vec<&str> = s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|k| k["type"].as_str())
            .collect();
        if kept {
            assert_parametric(&report);
            assert!(
                note.contains("solved within the geometric tolerance"),
                "{note}"
            );
            assert!(kinds.contains(&"coincident"), "{s}");
            // The corner on the spline, 1.3 µm from the file's.
            let uid = report.items[0]
                .features
                .iter()
                .filter_map(|f| f.parse::<mitcad_model::FeatureUid>().ok())
                .find(|uid| doc.sketch_output(*uid).is_some())
                .expect("the sketch");
            let corner = doc
                .sketch_output(uid)
                .unwrap()
                .solved
                .points
                .values()
                .copied()
                .find(|p| (p[0] - 10.0).hypot(p[1] - 7.5) < 0.01)
                .unwrap_or_else(|| panic!("{s}"));
            let moved = (corner[0] - 10.0).hypot(corner[1] - 7.5 - off * 10.0);
            assert!(moved > 1e-4 && moved < 2e-3, "{corner:?}");
            // On the spline: y = x (1 - x / 40) mm.
            let on = corner[1] - corner[0] * (1.0 - corner[0] / 40.0);
            assert!(on.abs() < 1e-7, "{corner:?}");
        } else {
            assert_eq!(report.items[0].outcome, Outcome::Partial, "{note}");
            // It comes in inactive (mitcad#721).
            assert!(
                note.contains("1 constraint inactive, not met in the file: coincident"),
                "{note}"
            );
            let coincident = s["constraints"]
                .as_array()
                .unwrap()
                .iter()
                .find(|k| k["type"] == "coincident")
                .unwrap_or_else(|| panic!("{s}"));
            assert_eq!(coincident["active"], false, "{s}");
        }
    }
}

/// A solve within the geometric tolerance that changes the sketch's
/// regions is not the file's sketch (mitcad#396): a line 2 µm short of a
/// rectangle's top edge, its length dimensioned 4 µm longer than the
/// file's positions make it, would cross the edge and split the
/// rectangle; the dimension comes in inactive (mitcad#721).
#[test]
fn a_solve_within_the_tolerance_that_changes_the_regions_leaves_the_dimension_inactive() {
    for (end, kept) in [(1.5, true), (1.9998, false)] {
        let points = vec![
            gpoint("p0", 0.0, 0.0),
            gpoint("p1", 4.0, 0.0),
            gpoint("p2", 4.0, 2.0),
            gpoint("p3", 0.0, 2.0),
            gfixed("p4", 2.0, -0.5),
            gpoint("p5", 2.0, end),
        ];
        let curves = vec![
            gline("c0", "p0", "p1"),
            gline("c1", "p1", "p2"),
            gline("c2", "p2", "p3"),
            gline("c3", "p3", "p0"),
            gline("c4", "p4", "p5"),
        ];
        let constraints = json!([
            {"id": "k0", "type": "HorizontalConstraint", "refs": {"entities": ["c0"]}},
            {"id": "k1", "type": "VerticalConstraint", "refs": {"entities": ["c1"]}},
            {"id": "k2", "type": "HorizontalConstraint", "refs": {"entities": ["c2"]}},
            {"id": "k3", "type": "VerticalConstraint", "refs": {"entities": ["c4"]}},
        ]);
        // The file's length plus 4 µm.
        let length = (end + 0.5) * 10.0 + 4e-3;
        let dimensions = json!([
            {"id": "d0", "type": "SketchLinearDimension",
             "parameter": param("d1", &format!("{length} mm"), length / 10.0, "mm"),
             "props": {"orientation": "VerticalDimensionOrientation"},
             "refs": {"entities": ["p4", "p5"]}, "textPosition": [2.5, 0.5, 0.0]},
        ]);
        let dump = group_sketch_dump(
            vec![json!({"name": "d1", "expression": format!("{length} mm"),
                        "value": length / 10.0, "unit": "mm"})],
            points,
            curves,
            constraints,
            dimensions,
        );
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        let note = report.items[0].note.clone().unwrap_or_default();
        if kept {
            assert_parametric(&report);
            assert!(
                note.contains("solved within the geometric tolerance"),
                "{note}"
            );
        } else {
            assert_eq!(report.items[0].outcome, Outcome::Partial, "{note}");
            assert!(
                note.contains("1 dimension inactive, not met in the file: vertical distance"),
                "{note}"
            );
            let s = sketch_query(&doc);
            assert_eq!(s["dimensions"][0]["active"], false, "{s}");
            assert_eq!(s["constraints"].as_array().unwrap().len(), 4, "{s}");
        }
    }
}

#[test]
fn a_tangent_distance_dimension_is_the_gap_between_a_circle_and_a_line() {
    // A circle of 12 mm about the fixed origin and a line 37 mm below its
    // centre: the gap is 25 mm, with the entities in either order.
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", -3.0, -3.7),
        gpoint("p2", 3.0, -3.7),
    ];
    let curves = vec![gcircle("c0", "p0", 1.2), gline("c1", "p1", "p2")];
    for order in [["c0", "c1"], ["c1", "c0"]] {
        let dump = group_sketch_dump(
            vec![json!({"name": "d1", "expression": "25 mm", "value": 2.5, "unit": "mm"})],
            points.clone(),
            curves.clone(),
            json!([]),
            json!([{"id": "d0", "type": "SketchTangentDistanceDimension",
                    "parameter": param("d1", "25 mm", 2.5, "mm"),
                    "refs": {"entities": order}}]),
        );
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        assert_parametric(&report);
        let s = sketch_query(&doc);
        let dimension = &s["dimensions"][0];
        assert_eq!(dimension["type"], "tangent_distance", "{s}");
        assert!((dimension["measured"].as_f64().unwrap() - 25.0).abs() < 1e-6);
        // The parameter drives the gap: the line moves away from the
        // circle, whose size and place the sketch keeps.
        doc.set_parameter_expression("d1", "30 mm", None).unwrap();
        let s = sketch_query(&doc);
        assert!((s["dimensions"][0]["measured"].as_f64().unwrap() - 30.0).abs() < 1e-6);
    }
}

/// Reference curves the file locks that do not meet exactly: two elliptical
/// arcs share their ends, which lie on one of them and a tenth of a micrometre off the other.
/// No solution keeps both in place, so they are imported free.
#[test]
fn linked_arcs_that_do_not_meet_exactly_are_not_locked() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let on_first = |angle: f64| {
        let t = angle.to_radians();
        [a * t.cos(), b * t.sin(), 0.0]
    };
    let points = vec![
        json!({"id": "p0", "xyz": [0.0, 0.0, 0.0]}),
        json!({"id": "p1", "xyz": on_first(20.0)}),
        json!({"id": "p2", "xyz": on_first(80.0)}),
    ];
    let arc = |id: &str, minor: f64| {
        json!({"id": id, "type": "SketchEllipticalArc", "isReference": true, "isLinked": true,
               "startSketchPoint": "p1", "endSketchPoint": "p2", "centerSketchPoint": "p0",
               "majorAxisRadius": a, "minorAxisRadius": minor, "majorAxis": [a, 0.0, 0.0],
               "geometry": {"type": "EllipticalArc3D", "center": [0.0, 0.0, 0.0],
                            "normal": [0.0, 0.0, 1.0], "majorAxis": [1.0, 0.0, 0.0],
                            "majorRadius": a, "minorRadius": minor,
                            "startAngle": 0.3, "endAngle": 1.4}})
    };
    let dump = group_sketch_dump(
        vec![],
        points,
        vec![arc("c0", b), arc("c1", b * 1.00003)],
        json!([]),
        json!([]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let note = report.items[0].note.as_deref().unwrap_or_default();
    assert!(note.contains("not locked"), "{note}");
    let s = sketch_query(&doc);
    let arcs = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "elliptical_arc")
        .count();
    assert_eq!(arcs, 2, "{s}");
}

/// Two linked arcs that share an end, one of them with its ends 30 µm
/// apart in radius about its centre, so that they are imported free, and
/// an own circle through their common end (round 11 of mitcad#343): that
/// end stays where the file has it, the arc's inconsistency taken up by its
/// other points, so that the circle keeps the file's radius and centre.
#[test]
fn an_own_circle_through_a_free_linked_arc_s_end_keeps_the_file_s_radius() {
    let end: [f64; 2] = [1.00003 * 0.6, 1.00003 * 0.8];
    let center = [3.0, 0.5];
    let radius = (end[0] - center[0]).hypot(end[1] - center[1]);
    // The second arc about (0.6, 2.8), from the common end to its top.
    let second = [0.6, 2.8];
    let second_r = (end[0] - second[0]).hypot(end[1] - second[1]);
    let points = vec![
        gpoint("p0", 0.0, 0.0),
        gpoint("p1", 1.0, 0.0),
        gpoint("p2", end[0], end[1]),
        gpoint("p3", center[0], center[1]),
        gpoint("p4", second[0], second[1]),
        gpoint("p5", second[0] + second_r, second[1]),
    ];
    let curves = vec![
        json!({"id": "c0", "type": "SketchArc", "isReference": true, "isLinked": true,
               "centerSketchPoint": "p0", "startSketchPoint": "p1", "endSketchPoint": "p2"}),
        gcircle("c1", "p3", radius),
        json!({"id": "c2", "type": "SketchArc", "isReference": true, "isLinked": true,
               "centerSketchPoint": "p4", "startSketchPoint": "p5", "endSketchPoint": "p2"}),
    ];
    let constraints = json!([
        {"id": "k0", "type": "CoincidentConstraint", "refs": {"entities": ["p2", "c1"]}},
    ]);
    let dump = group_sketch_dump(vec![], points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let note = report.items[0].note.as_deref().unwrap_or_default();
    assert!(note.contains("not locked"), "{note}");
    let s = sketch_query(&doc);
    let entities = s["entities"].as_array().unwrap();
    let circle = entities
        .iter()
        .find(|e| e["type"] == "circle")
        .expect("the circle");
    // (The file's lengths are centimetres.)
    let r = circle["geometry"]["radius"].as_f64().unwrap() / 10.0;
    assert!(
        (r - radius).abs() < 1e-9,
        "radius {r} against the file's {radius}: {s}"
    );
    let c = circle["geometry"]["center"].as_array().unwrap();
    let (cx, cy) = (c[0].as_f64().unwrap() / 10.0, c[1].as_f64().unwrap() / 10.0);
    assert!(
        (cx - center[0]).abs() < 1e-9 && (cy - center[1]).abs() < 1e-9,
        "center {cx}, {cy}: {s}"
    );
}

/// The midpoint constraints of a fitted spline's fit points with the
/// tangent handles (lines the sketch does not list) bind nothing in a
/// sketch that keeps the spline's curve: the sketch is imported whole.
#[test]
fn tangent_handles_of_a_fitted_spline_are_no_constraints_left_out() {
    let handle = json!({"kind": "sketch_entity", "objectType": "SketchLine", "id": null,
                        "geometry": {"type": "Line3D", "startPoint": [-0.3, 0.0, 0.0],
                                     "endPoint": [0.3, 0.0, 0.0]}});
    let spline = json!({"id": "c0", "type": "SketchFittedSpline", "fitPoints": ["p1", "p2"],
        "startSketchPoint": "p1", "endSketchPoint": "p2",
        "geometry": {"type": "NurbsCurve3D", "nurbs": {
            "degree": 3, "isRational": false, "isPeriodic": false, "controlPointCount": 4,
            "knots": [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            "controlPoints": [[0.0, 0.0, 0.0], [1.0, 2.0, 0.0], [3.0, 2.0, 0.0],
                              [4.0, 0.0, 0.0]]}}});
    let dump = group_sketch_dump(
        vec![],
        vec![gpoint("p1", 0.0, 0.0), gpoint("p2", 4.0, 0.0)],
        vec![spline],
        json!([{"id": "k0", "type": "MidPointConstraint",
                "refs": {"entities": [handle, "p1"]}}]),
        json!([]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    assert_eq!(report.items[0].note, None, "{}", report.text());
}

#[test]
fn datums_follow_several_sketch_curves_and_the_plane_of_a_profile() {
    let curve = |id: &str| {
        json!({"kind": "sketch_entity", "objectType": "SketchLine", "sketch": "Sketch1",
               "sketch_timeline_index": 0, "id": id})
    };
    let items = vec![
        rectangle_sketch(0),
        // 50 mm along the bottom and the right side of the rectangle: on
        // the right side, 10 mm up, normal to it.
        json!({"index": 1, "name": "Along", "objectType": "ConstructionPlane",
               "detail": {"definition": {"_type": "ConstructionPlaneDistanceOnPathDefinition",
                                         "distance": param("d9", "50 mm", 5.0, "mm"),
                                         "pathEntities": [curve("c0"), curve("c1")]},
                          "geometry": {"type": "Plane", "origin": [4.0, 1.0, 0.0],
                                       "normal": [0.0, 1.0, 0.0]}}}),
        // Normal to the rectangle's plane at its second corner.
        json!({"index": 2, "name": "Upright", "objectType": "ConstructionAxis",
               "detail": {"definition": {"_type": "ConstructionAxisNormalToFaceAtPointDefinition",
                                         "face": {"kind": "profile", "sketch": "Sketch1",
                                                  "sketch_timeline_index": 0},
                                         "pointEntity": {"kind": "sketch_entity",
                                                         "objectType": "SketchPoint",
                                                         "sketch": "Sketch1",
                                                         "sketch_timeline_index": 0, "id": "p1"}},
                          "geometry": {"type": "InfiniteLine3D", "origin": [4.0, 0.0, 0.0],
                                       "direction": [0.0, 0.0, 1.0]}}}),
    ];
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &timeline_dump(items));
    for i in [1, 2] {
        assert_eq!(
            report.items[i].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
    }
    let def = |name: &str| {
        let uid = doc.features().find(|f| f.name == name).unwrap().uid;
        let v: Value = serde_json::from_str(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .unwrap(),
        )
        .unwrap();
        v["def"].clone()
    };
    let along = def("Along");
    assert_eq!(along["definition"]["type"], "along_path");
    assert_eq!(
        along["definition"]["path"]["curves"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        def("Upright")["definition"]["type"],
        "normal_to_face_at_point"
    );
    assert_eq!(
        report.items[2].input_provenance,
        Some(InputProvenance::Read)
    );
}

/// A midpoint constraint of two lines (the file's API names only the first
/// as `midPointCurve`): the first line's midpoint lies on the second.
#[test]
fn a_midpoint_of_two_lines_puts_the_midpoint_of_the_first_on_the_second() {
    let dump = group_sketch_dump(
        vec![],
        vec![
            gpoint("p0", 0.0, 0.0),
            gpoint("p1", 0.0, 1.0),
            gpoint("p2", -0.3, 0.5),
            gpoint("p3", 0.3, 0.5),
        ],
        vec![gline("c0", "p0", "p1"), gline("c1", "p2", "p3")],
        json!([{"id": "k0", "type": "MidPointConstraint",
                "refs": {"entities": ["c0", "c1"]}}]),
        json!([]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let kinds: Vec<&str> = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|k| k["type"].as_str())
        .collect();
    assert_eq!(kinds, ["midpoint", "coincident"], "{s}");
    // The helper point is at the middle of the first line.
    let helper = point_at(&s, [0.0, 5.0]);
    assert_eq!(at(&s, &helper), [0.0, 5.0]);
}

/// A dimension the file keeps driven, whose parameter the file has (`d4`):
/// that parameter becomes the dimension's reference (mitcad#250), owned by
/// the sketch with the value the sketch measures, instead of a user
/// parameter beside a reference of the dimension's own; `half` = `d4 / 2`
/// follows it.
#[test]
fn a_driven_dimensions_parameter_of_the_file_is_its_reference() {
    use mitcad_model::expr::ParamKind;
    let mut sketch = rectangle_sketch(0);
    sketch["detail"]["dimensions"].as_array_mut().unwrap().push(
        json!({"id": "d1", "type": "SketchLinearDimension", "isDriving": false,
                     "parameter": param("d4", "20 mm", 2.0, "mm"),
                     "props": {"orientation": "VerticalDimensionOrientation"},
                     "refs": {"entities": ["p1", "p2"]}, "textPosition": [5.0, 1.0, 0.0]}),
    );
    let source = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "driven.f3d"},
        "parameters": {
            "user": [{"name": "half", "expression": "d4 / 2", "value": 1.0, "unit": "mm"}],
            "model": [
                {"name": "d1", "expression": "40 mm", "value": 4.0, "unit": "mm",
                 "createdBy": {"kind": "sketch_dimension", "sketch": "Sketch1",
                               "sketch_timeline_index": 0, "id": "d0"}},
                {"name": "d4", "expression": "20 mm", "value": 2.0, "unit": "mm",
                 "createdBy": {"kind": "sketch_dimension", "sketch": "Sketch1",
                               "sketch_timeline_index": 0, "id": "d1"}}
            ]
        },
        "timeline": {"items": [sketch]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &source);
    assert!(
        report.parameters.mismatched.is_empty(),
        "{:?}",
        report.parameters
    );
    let def = doc.query(r#"{"query": "feature", "uid": "F1"}"#).unwrap();
    let def: Value = serde_json::from_str(&def).unwrap();
    let driven: Vec<&Value> = def["def"]["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["driven"] == true)
        .collect();
    assert_eq!(driven.len(), 1, "{def}\n{}", report.text());
    assert_eq!(driven[0]["reference"], "d4", "{def}");
    let params = doc.parameters();
    let d4 = params.find("d4").unwrap();
    assert_eq!(params.get(d4).unwrap().kind(), ParamKind::Reference);
    assert_eq!(params.owner(d4), Some(FeatureUid(1)));
    assert!((params.value(d4).unwrap() - 20.0).abs() < 1e-9);
    let half = params.find("half").unwrap();
    assert!((params.value(half).unwrap() - 10.0).abs() < 1e-9);
    let references = params
        .iter()
        .filter(|p| p.kind() == ParamKind::Reference)
        .count();
    assert_eq!(references, 1);
    // The driving width's parameter is the sketch's model parameter.
    assert_eq!(
        params.get(params.find("d1").unwrap()).unwrap().kind(),
        ParamKind::Model
    );
}

/// A sketch definition without its driven dimensions' references, as an
/// import falls back on when the model refuses one (mitcad#250).
#[test]
fn sketch_definitions_lose_their_references_for_the_fallback() {
    let def = json!({"type": "sketch", "dimensions": [
        {"id": "k1", "type": "length", "line": "c1", "value": "d1"},
        {"id": "k2", "type": "length", "line": "c2", "driven": true, "reference": "d4"}]});
    assert!(crate::sketch::has_references(&def));
    let plain = crate::sketch::without_references(&def);
    assert!(!crate::sketch::has_references(&plain));
    assert_eq!(plain["dimensions"][1]["driven"], true);
    assert_eq!(plain["dimensions"][0]["value"], "d1");
}

/// Angle dimensions take their side from the file (mitcad#296): the sector
/// of the lines' extensions their value is shown in, when its angle is the
/// file's value, else the neighbouring sector nearer to the text.
#[test]
fn an_angle_dimensions_side_comes_from_the_file() {
    // A trapezoid's corner: the bottom c0 from the corner to the right, the
    // side c1 from up right down to the corner; inside, 63.43 degrees.
    let p = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let line = |id: &str, a: &str, b: &str| json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b});
    let inner = 2.0f64.atan();
    let angle = |id: &str, value: f64, text: [f64; 2]| {
        json!({"id": id, "type": "SketchAngularDimension", "isDriving": false,
               "parameter": param(id, "", value, "deg"),
               "refs": {"lineOne": "c0", "lineTwo": "c1"}, "textPosition": [text[0], text[1], 0.0]})
    };
    let sketch = json!({
        "index": 0, "name": "Sketch1", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [p("p0", 0.0, 0.0), p("p1", 4.0, 0.0), p("p2", 1.0, 2.0)],
            "curves": [line("c0", "p0", "p1"), line("c1", "p2", "p0")],
            "dimensions": [
                // Inside the corner.
                angle("d0", inner, [0.6, 0.6]),
                // Between the bottom's extension and the side.
                angle("d1", std::f64::consts::PI - inner, [-0.2, 0.6]),
                // The supplement shown inside, near the bottom: the sector
                // below the bottom.
                angle("d2", std::f64::consts::PI - inner, [0.6, 0.3]),
                // A value neither angle has: no side.
                angle("d3", 0.5, [0.6, 0.6])
            ]
        }
    });
    let source = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "angles.f3d"},
        "parameters": {"user": [], "model": []},
        "timeline": {"items": [sketch]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &source);
    let def = doc.query(r#"{"query": "feature", "uid": "F1"}"#).unwrap();
    let def: Value = serde_json::from_str(&def).unwrap();
    let dimensions = def["def"]["dimensions"].as_array().unwrap();
    assert_eq!(dimensions.len(), 4, "{def}\n{}", report.text());
    let side = |d: &Value| (d.get("reverse_a").cloned(), d.get("reverse_b").cloned());
    let given = |a: bool, b: bool| (Some(json!(a)), Some(json!(b)));
    assert_eq!(side(&dimensions[0]), given(false, true), "{def}");
    assert_eq!(side(&dimensions[1]), given(true, true), "{def}");
    assert_eq!(side(&dimensions[2]), given(false, false), "{def}");
    assert_eq!(side(&dimensions[3]), (None, None), "{def}");
}

// Inactive constraints and dimensions (mitcad#721).

/// One constraint or dimension the file's own sketch does not meet among
/// good ones: a 40 x 20 mm rectangle from the fixed origin, held by
/// horizontal and vertical constraints and its width and height, with
/// either its bottom parallel to its side or a second width of 50 mm. The
/// bad one comes in inactive and the report names it; the other
/// constraints stay, the dimensions keep driving the rectangle.
#[test]
fn a_constraint_the_file_does_not_meet_comes_in_inactive_and_the_others_stay() {
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", 4.0, 0.0),
        gpoint("p2", 4.0, 2.0),
        gpoint("p3", 0.0, 2.0),
    ];
    let curves = vec![
        gline("c0", "p0", "p1"),
        gline("c1", "p1", "p2"),
        gline("c2", "p2", "p3"),
        gline("c3", "p3", "p0"),
    ];
    let line = |id: &str, kind: &str, refs: &[&str]| json!({"id": id, "type": kind, "refs": {"entities": refs}});
    let distance = |id: &str, name: &str, mm: f64, orientation: &str, refs: &[&str]| {
        json!({"id": id, "type": "SketchLinearDimension",
               "parameter": param(name, &format!("{mm} mm"), mm / 10.0, "mm"),
               "props": {"orientation": orientation},
               "refs": {"entities": refs}})
    };
    let model = |name: &str, mm: f64| json!({"name": name, "expression": format!("{mm} mm"), "value": mm / 10.0, "unit": "mm"});
    let horizontal = "HorizontalDimensionOrientation";
    let vertical = "VerticalDimensionOrientation";
    for bad_constraint in [true, false] {
        let mut constraints = vec![
            line("k0", "HorizontalConstraint", &["c0"]),
            line("k1", "VerticalConstraint", &["c1"]),
            line("k2", "HorizontalConstraint", &["c2"]),
            line("k3", "VerticalConstraint", &["c3"]),
        ];
        let mut dimensions = vec![
            distance("d0", "d1", 40.0, horizontal, &["p0", "p1"]),
            distance("d1", "d2", 20.0, vertical, &["p1", "p2"]),
        ];
        let mut params = vec![model("d1", 40.0), model("d2", 20.0)];
        if bad_constraint {
            constraints.push(line("k4", "ParallelConstraint", &["c0", "c1"]));
        } else {
            dimensions.push(distance("d2", "d3", 50.0, horizontal, &["p0", "p1"]));
            params.push(model("d3", 50.0));
        }
        let dump = group_sketch_dump(
            params,
            points.clone(),
            curves.clone(),
            json!(constraints),
            json!(dimensions),
        );
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        let note = report.items[0].note.clone().unwrap_or_default();
        assert_eq!(report.items[0].outcome, Outcome::Partial, "{note}");
        let expected = if bad_constraint {
            "1 constraint inactive, not met in the file: parallel k"
        } else {
            "1 dimension inactive, not met in the file: horizontal distance k"
        };
        assert!(note.contains(expected), "{note}");
        let s = sketch_query(&doc);
        assert!(s["error"].is_null(), "{s}");
        let inactive = |list: &str| -> Vec<Value> {
            s[list]
                .as_array()
                .unwrap()
                .iter()
                .filter(|k| k["active"] == false)
                .cloned()
                .collect()
        };
        let (k, d) = (inactive("constraints"), inactive("dimensions"));
        if bad_constraint {
            assert_eq!(k.len(), 1, "{s}");
            assert_eq!(k[0]["type"], "parallel", "{s}");
            assert_eq!(k[0]["inactive_reason"], "not met in the imported file");
            assert!(d.is_empty(), "{s}");
            assert_eq!(s["constraints"].as_array().unwrap().len(), 5, "{s}");
            assert_eq!(s["unmet_inactive"], json!([k[0]["id"]]), "{s}");
        } else {
            assert!(k.is_empty(), "{s}");
            assert_eq!(d.len(), 1, "{s}");
            let measured = d[0]["measured"].as_f64().unwrap_or_default();
            assert!((measured - 50.0).abs() < 1e-6, "{s}");
            assert_eq!(s["constraints"].as_array().unwrap().len(), 4, "{s}");
        }
        // Everything else drives the rectangle: fully constrained, and the
        // width follows its parameter.
        assert_eq!(s["dof"], 0, "{s}");
        let driving = s["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["active"] != false && x["driven"] != true)
            .count();
        assert_eq!(driving, 2, "{s}");
        let corner = point_at(&s, [40.0, 20.0]);
        doc.set_parameter_expression("d1", "60 mm", None).unwrap();
        let s = sketch_query(&doc);
        assert!(s["error"].is_null(), "{s}");
        let [x, y] = at(&s, &corner);
        assert!((x - 60.0).abs() < 1e-6 && (y - 20.0).abs() < 1e-6, "{s}");
    }
}
