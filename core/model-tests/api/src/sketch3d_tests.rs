// SPDX-License-Identifier: MIT
//! 3D sketches through the JSON api (mitcad#1522), with the mock kernel:
//! the route of W1-W3 made from the ids the commands return, its
//! dimensions edited, bound to a named parameter and removed; refused
//! compound actions and invalid input leaving the definition, the
//! parameters and the history as they were (W5); the bent route of tangent
//! arcs with Equal radii (W11) saved, read again, edited and undone (W8);
//! fit-point and control-point splines (W17), a driven spline length,
//! drags and safe deletion; Fix, Unfix, Attach, Detach, On Sketch Plane
//! and Release Plane; planar commands and queries refusing a 3D sketch;
//! a planar sketch read again unchanged beside a 3D one, and an unknown
//! feature type refused rather than dropped.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const TOL: f64 = 1e-6;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

/// The `sketch3d` query of `uid`.
fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch3d", "uid": uid}))
}

fn xyz(value: &Value) -> [f64; 3] {
    let a = value
        .as_array()
        .unwrap_or_else(|| panic!("{value} is not a point"));
    [0, 1, 2].map(|k| a[k].as_f64().unwrap())
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

/// The same JSON, numbers within the tolerance.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= TOL
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// Where a point of the sketch is (local coordinates).
fn at(sketch: &Value, id: &Value) -> [f64; 3] {
    let point = sketch["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == *id)
        .unwrap_or_else(|| panic!("{id} in {sketch}"));
    xyz(&point["at"])
}

fn item<'a>(sketch: &'a Value, list: &str, id: &Value) -> &'a Value {
    sketch[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == *id)
        .unwrap_or_else(|| panic!("{id} in {list} of {sketch}"))
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// W1: A at the origin, then 40 along +X, 30 along +Y and 20 along +Z, each
/// segment from the end the one before returned. Returns its results.
fn route(doc: &mut Document<MockKernel>) -> [Value; 3] {
    let ab = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [37, 2, 1], "axis": "x", "length": 40, "start_source": "origin"}),
    );
    let bc = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": ab["end"],
               "end": [42, 27, 0], "axis": "y", "length": 30}),
    );
    let cd = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": bc["end"],
               "end": [40, 33, 21], "axis": "z", "sense": "positive", "length": 20}),
    );
    [ab, bc, cd]
}

/// W1-W3 and the parameters: the route from returned ids, fully
/// constrained; a dimension edit (one undo step), a named parameter it
/// is bound to, and BC's length removed leaving one shared freedom.
#[test]
fn the_route_from_returned_ids_and_its_dimensions() {
    let mut doc = Document::new(MockKernel::default());
    let created = command(&mut doc, json!({"cmd": "sketch3d.create"}));
    assert_eq!(
        (&created["uid"], &created["name"]),
        (&json!("F1"), &json!("3DSketch1"))
    );
    let [ab, bc, cd] = route(&mut doc);
    // Ids as the commands gave them out: the start, its attachment, the
    // end, the line, its axis and its length.
    assert_eq!(ab["entities"], json!(["p1", "p2", "c3"]), "{ab}");
    assert_eq!(ab["made"], json!(["c3"]));
    assert_eq!(ab["constraints"], json!(["k1", "k2"]));
    assert_eq!(ab["dimensions"], json!(["k3"]));
    assert_eq!(ab["parameters"].as_array().unwrap().len(), 1);
    assert_eq!(bc["start"], ab["end"], "a continued route shares its point");
    assert_eq!(cd["start"], bc["end"]);
    assert_eq!(cd["status"]["dof"], 0, "{cd}");
    assert_eq!(cd["status"]["fully_constrained"], true);

    let s = sketch(&doc, "F1");
    assert_eq!(s["type"], "spatial_sketch");
    assert_eq!(s["name"], "3DSketch1");
    assert_eq!(s["support"], "xy");
    assert!(near(xyz(&s["frame"]["normal"]), [0.0, 0.0, 1.0]), "{s}");
    let points = [&ab["start"], &ab["end"], &bc["end"], &cd["end"]];
    let expected = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 30.0, 0.0],
        [40.0, 30.0, 20.0],
    ];
    for (p, e) in points.iter().zip(expected) {
        assert!(near(at(&s, p), e), "{p}: {s}");
        assert_eq!(item(&s, "points", p)["determined"], true);
    }
    assert_eq!(
        s["status"]["determined"]["points"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let line = item(&s, "curves", &bc["made"][0]);
    assert_eq!(line["type"], "line");
    assert!((line["geometry"]["length"].as_f64().unwrap() - 30.0).abs() <= TOL);
    let align = item(&s, "constraints", &bc["constraints"][0]);
    assert_eq!(align["type"], "align");
    assert_eq!(align["axis"], "y");
    let attach = item(&s, "constraints", &ab["constraints"][0]);
    assert_eq!(attach["source"], "origin");
    assert!(near(xyz(&attach["target"]), [0.0; 3]));
    let length = item(&s, "dimensions", &bc["dimensions"][0]);
    assert_eq!(length["type"], "length");
    assert_eq!(length["driven"], false);
    assert_eq!(length["value"], bc["parameters"][0]);
    assert!((length["measured"].as_f64().unwrap() - 30.0).abs() <= TOL);

    // W2: BC to 50 by its dimension, one undo step that keeps every id.
    let edited = command(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1",
               "dimension": bc["dimensions"][0], "value": 50}),
    );
    assert_eq!(edited["entities"], json!([]));
    let s2 = sketch(&doc, "F1");
    assert!(near(at(&s2, &bc["end"]), [40.0, 50.0, 0.0]), "{s2}");
    assert!(near(at(&s2, &cd["end"]), [40.0, 50.0, 20.0]));
    assert_eq!(s2["curves"].as_array().unwrap().len(), 3);
    assert_eq!(doc.undo().as_deref(), Some("Edit Dimension in 3DSketch1"));
    assert!(near(at(&sketch(&doc, "F1"), &cd["end"]), expected[3]));
    doc.redo();
    assert!(near(
        at(&sketch(&doc, "F1"), &cd["end"]),
        [40.0, 50.0, 20.0]
    ));

    // Bound to a named parameter, the route follows it.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Leg", "expression": "25 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1",
               "dimension": bc["dimensions"][0], "value": "Leg"}),
    );
    assert!(near(at(&sketch(&doc, "F1"), &bc["end"]), [40.0, 25.0, 0.0]));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Leg", "expression": "35 mm"}),
    );
    let s3 = sketch(&doc, "F1");
    assert!(near(at(&s3, &cd["end"]), [40.0, 35.0, 20.0]), "{s3}");
    let bound = item(&s3, "dimensions", &bc["dimensions"][0]);
    assert!(
        bound["value"] == "Leg" || bound["expression"] == "Leg",
        "{bound}"
    );

    // W3: without BC's length, C and D share one free motion along Y.
    let removed = command(
        &mut doc,
        json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": [bc["dimensions"][0]]}),
    );
    assert_eq!(removed["removed"], json!([bc["dimensions"][0]]));
    assert_eq!(removed["status"]["dof"], 1, "{removed}");
    assert_eq!(removed["status"]["fully_constrained"], false);
    let s4 = sketch(&doc, "F1");
    let c = at(&s4, &bc["end"]);
    assert!(near(at(&s4, &cd["end"]), [40.0, c[1], 20.0]), "{s4}");
    assert_eq!(item(&s4, "points", &cd["end"])["determined"], false);
    assert!(doc.parameters().find("Leg").is_some(), "a named one stays");
}

/// W5: a compound action whose relations contradict, a collapsed segment,
/// points with two coordinates, unknown ids, a collinear Three-Point arc
/// and a closed control-point spline are refused with nothing changed and
/// no history; a duplicate length is consistent, kept and said.
#[test]
fn refused_actions_leave_no_trace() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let [ab, _, cd] = route(&mut doc);
    let before = sketch(&doc, "F1");
    let parameters = query(&doc, json!({"query": "parameters"}));
    let label = undo_label(&doc);
    assert_eq!(label, "Add Line in 3DSketch1");

    // Along +Z 10 long, but 5 up: the segment, its point, its axis and both
    // dimensions go together.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": cd["end"],
               "end": [40, 30, 30], "axis": "z", "length": 10, "offsets": [null, null, 5]}),
    );
    assert!(error.contains("3DSketch1"), "{error}");
    for (command, part) in [
        (
            json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": cd["end"],
                   "end": [40, 30, 20]}),
            "no length",
        ),
        (
            json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0],
                   "end": [1, 1, 1]}),
            "three coordinates",
        ),
        (
            json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": "p99",
                   "end": [1, 1, 1]}),
            "p99",
        ),
        (
            json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": ab["made"][0],
                   "end": [1, 1, 1]}),
            "p<number>",
        ),
        (
            json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
                   "end": [1, 1, 1], "sense": "negative"}),
            "\"axis\"",
        ),
        (
            json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "three_point",
                   "start": [0, 50, 0], "end": [20, 50, 0], "through": [10, 50, 0]}),
            "one line",
        ),
        (
            json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "center",
                   "start": [0, 50, 0], "end": [20, 50, 0], "through": [10, 50, 0]}),
            "takes no \"through\"",
        ),
        (
            json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "spiral",
                   "start": [0, 50, 0], "end": [20, 50, 0]}),
            "unknown arc mode",
        ),
        (
            json!({"cmd": "sketch3d.add_spline", "sketch": "F1", "degree": 3, "closed": true,
                   "points": [[0, 60, 0], [10, 70, 0], [20, 60, 0], [30, 70, 0]]}),
            "open",
        ),
        (
            json!({"cmd": "sketch3d.add_spline", "sketch": "F1",
                   "points": [[0, 60, 0], [0, 60, 0], [20, 60, 0]]}),
            "apart",
        ),
        (
            json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
                   "dimension": {"type": "length", "line": ab["made"][0]}, "value": 0}),
            "greater than",
        ),
        (
            json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": ["c42"]}),
            "c42",
        ),
        (
            json!({"cmd": "sketch3d.add_constraint", "sketch": "F1",
                   "constraint": {"type": "in_plane_fixed", "entity": "p1"}}),
            "in-plane",
        ),
    ] {
        let error = rejected(&mut doc, command.clone());
        assert!(error.contains(part), "{command}: {error}");
    }
    assert!(same(&sketch(&doc, "F1"), &before));
    assert_eq!(query(&doc, json!({"query": "parameters"})), parameters);
    assert_eq!(undo_label(&doc), label);

    // The same length again: consistent, kept, marked and warned.
    let twin = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": ab["made"][0]}, "value": 40}),
    );
    let k = &twin["dimensions"][0];
    assert_eq!(twin["status"]["redundant"], json!([k]), "{twin}");
    assert_eq!(twin["status"]["dof"], 0);
    let s = sketch(&doc, "F1");
    assert_eq!(item(&s, "dimensions", k)["redundant"], true);
    assert!(
        s["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("redundant")),
        "{s}"
    );
}

/// The id of a command result's first made item.
fn made(result: &Value) -> Value {
    result["made"][0].clone()
}

/// W11 and W8: the bent route of lines and tangent arcs with Equal radii,
/// fully constrained; AB at 60 and the radius at 12 move it with its
/// tangencies; saved and read again it keeps its ids, senses, dimensions
/// and expressions, and its edits undo.
#[test]
fn the_bent_route_is_edited_saved_and_read_again() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let ab = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [38, 1, 0], "axis": "x", "length": 40, "start_source": "origin"}),
    );
    let bc = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "tangent",
               "from": made(&ab), "start": ab["end"], "end": [50, 10, 0], "radius": 10}),
    );
    assert_eq!(bc["constraints"].as_array().unwrap().len(), 1, "{bc}");
    let cd = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": bc["end"],
               "end": [50, 38, 0], "axis": "y", "length": 30}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint":
               {"type": "tangent", "a": made(&bc), "b": made(&cd), "at": bc["end"]}}),
    );
    let de = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "tangent",
               "from": made(&cd), "start": cd["end"], "end": [50, 50, 10]}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint":
               {"type": "equal", "a": made(&bc), "b": made(&de)}}),
    );
    let ef = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": de["end"],
               "end": [50, 50, 28], "axis": "z", "length": 20}),
    );
    let last = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint":
               {"type": "tangent", "a": made(&de), "b": made(&ef), "at": de["end"]}}),
    );
    assert_eq!(last["status"]["dof"], 0, "{last}");
    assert_eq!(last["status"]["fully_constrained"], true);
    let points = [
        ab["start"].clone(),
        ab["end"].clone(),
        bc["center"].clone(),
        bc["end"].clone(),
        cd["end"].clone(),
        de["center"].clone(),
        de["end"].clone(),
        ef["end"].clone(),
    ];
    let check = |doc: &Document<MockKernel>, expected: [[f64; 3]; 8]| {
        let s = sketch(doc, "F1");
        for (p, e) in points.iter().zip(expected) {
            assert!(near(at(&s, p), e), "{p}: {:?}, not {e:?}", at(&s, p));
        }
        s
    };
    let s = check(
        &doc,
        [
            [0.0, 0.0, 0.0],
            [40.0, 0.0, 0.0],
            [40.0, 10.0, 0.0],
            [50.0, 10.0, 0.0],
            [50.0, 40.0, 0.0],
            [50.0, 40.0, 10.0],
            [50.0, 50.0, 10.0],
            [50.0, 50.0, 30.0],
        ],
    );
    let arcs = |s: &Value, r: f64| {
        for (arc, normal) in [(made(&bc), [0.0, 0.0, 1.0]), (made(&de), [1.0, 0.0, 0.0])] {
            let curve = item(s, "curves", &arc);
            assert_eq!(curve["type"], "arc");
            let g = &curve["geometry"];
            assert!(near(xyz(&g["normal"]), normal), "{curve}");
            assert!((g["radius"].as_f64().unwrap() - r).abs() <= TOL, "{curve}");
            let sweep = g["sweep"].as_f64().unwrap();
            assert!(
                (sweep - std::f64::consts::FRAC_PI_2).abs() <= TOL,
                "{curve}"
            );
        }
    };
    arcs(&s, 10.0);
    let tangents: Vec<&Value> = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "tangent")
        .collect();
    assert_eq!(tangents.len(), 4);
    assert!(tangents.iter().all(|t| t["sense"] == "positive"), "{s}");

    // AB to 60 and the radius to 12: everything follows.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1",
               "dimension": ab["dimensions"][0], "value": 60}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1",
               "dimension": bc["dimensions"][0], "value": "2 * 6 mm"}),
    );
    let moved = [
        [0.0, 0.0, 0.0],
        [60.0, 0.0, 0.0],
        [60.0, 12.0, 0.0],
        [72.0, 12.0, 0.0],
        [72.0, 42.0, 0.0],
        [72.0, 42.0, 12.0],
        [72.0, 54.0, 12.0],
        [72.0, 54.0, 32.0],
    ];
    let s = check(&doc, moved);
    arcs(&s, 12.0);
    let radius = item(&s, "dimensions", &bc["dimensions"][0]);
    assert!(
        radius["expression"].as_str().unwrap().contains('*'),
        "{radius}"
    );

    // Saved and read again: the same definition, ids, expressions and
    // geometry; a planar reader's file keeps it as its own feature type.
    let file = doc.to_json();
    let saved: Value = serde_json::from_str(&file).unwrap();
    assert_eq!(saved["features"][0]["type"], "spatial_sketch", "{saved}");
    let mut back = Document::from_json(&file, MockKernel::default()).unwrap();
    back.recompute();
    let read = check(&back, moved);
    for list in ["points", "curves", "constraints", "dimensions"] {
        assert!(same(&read[list], &s[list]), "{list}");
    }
    let saved_again: Value = serde_json::from_str(&back.to_json()).unwrap();
    assert_eq!(saved_again["features"], saved["features"]);
    assert_eq!(saved_again["parameters"], saved["parameters"]);

    // An edit after reading it again, and its undo and redo.
    command(
        &mut back,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1",
               "dimension": bc["dimensions"][0], "value": 10}),
    );
    arcs(&sketch(&back, "F1"), 10.0);
    assert_eq!(back.undo().as_deref(), Some("Edit Dimension in 3DSketch1"));
    check(&back, moved);
    back.redo();
    arcs(&sketch(&back, "F1"), 10.0);
}

/// W17 through the api: a fit-point spline tangent to a fixed line, a
/// control-point spline that continues an arc's end, a spline's length
/// measured only; a drag previewed without a trace and taken as one step;
/// deleting the fit-point spline keeps the line's end, and a point that a
/// line uses is deleted only with it.
#[test]
fn splines_drags_and_safe_deletion() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let line = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [10, 0, 0]}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.fix", "sketch": "F1", "points": [line["start"], line["end"]]}),
    );
    let fit = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_spline", "sketch": "F1",
               "points": [line["end"], [20, 5, 0], [30, 0, 5], [40, 5, 5]]}),
    );
    assert_eq!(fit["spline_points"][0], line["end"], "{fit}");
    let tangent = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint":
               {"type": "tangent", "a": made(&line), "b": made(&fit), "at": line["end"]}}),
    );
    assert_eq!(tangent["status"]["dof"], 10, "{tangent}");
    let s = sketch(&doc, "F1");
    let spline = item(&s, "curves", &made(&fit));
    assert_eq!(spline["type"], "fitted_spline");
    assert_eq!(spline["geometry"]["degree"], 3);
    assert_eq!(
        spline["geometry"]["control_points"]
            .as_array()
            .unwrap()
            .len(),
        6
    );

    // A control-point spline from an arc's end.
    let arc = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "three_point",
               "start": [0, 20, 0], "end": [20, 20, 0], "through": [10, 30, 0]}),
    );
    let control = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_spline", "sketch": "F1", "degree": 3,
               "points": [arc["end"], [30, 25, 0], [40, 20, 5], [50, 25, 5]]}),
    );
    let s = sketch(&doc, "F1");
    let spline = item(&s, "curves", &made(&control));
    assert_eq!(spline["type"], "spline");
    assert_eq!(spline["control"][0], arc["end"]);
    assert_eq!(spline["geometry"]["knots"].as_array().unwrap().len(), 8);
    // Its length can drive (mitcad#1661); here it only measures.
    let length = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "spline_length", "curve": made(&control)}, "driven": true}),
    );
    let measured = item(&sketch(&doc, "F1"), "dimensions", &length["dimensions"][0])["measured"]
        .as_f64()
        .unwrap();
    // Between its ends' distance and its control polygon's length.
    assert!(measured > 30.8 && measured < 34.7, "{measured}");

    // A drag previewed changes nothing; taken, it is one step.
    let pulled = fit["spline_points"][1].clone();
    let before = sketch(&doc, "F1");
    let label = undo_label(&doc);
    let preview = command(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1", "preview": true,
               "points": [{"point": pulled, "to": [20, 8, 2]}]}),
    );
    let key = pulled.as_str().unwrap();
    assert!(
        near(xyz(&preview["points"][key]), [20.0, 8.0, 2.0]),
        "{preview}"
    );
    assert!(same(&sketch(&doc, "F1"), &before));
    assert_eq!(undo_label(&doc), label);
    command(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1",
               "points": [{"point": pulled, "to": [20, 8, 2]}]}),
    );
    assert!(near(at(&sketch(&doc, "F1"), &pulled), [20.0, 8.0, 2.0]));
    assert_eq!(undo_label(&doc), "Drag in 3DSketch1");

    // Deleting the fit-point spline keeps the line's end.
    let removed = command(
        &mut doc,
        json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": [made(&fit)]}),
    );
    assert!(
        removed["removed"].as_array().unwrap().contains(&made(&fit)),
        "{removed}"
    );
    let s = sketch(&doc, "F1");
    at(&s, &line["end"]);
    for p in &fit["spline_points"].as_array().unwrap()[1..] {
        assert!(
            !s["points"]
                .as_array()
                .unwrap()
                .iter()
                .any(|q| q["id"] == *p),
            "{p}"
        );
    }
    // A point a line uses goes only with it.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": [line["start"]]}),
    );
    assert!(error.contains(made(&line).as_str().unwrap()), "{error}");
    let removed = command(
        &mut doc,
        json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": [line["start"]],
               "dependents": true}),
    );
    assert!(
        removed["removed"]
            .as_array()
            .unwrap()
            .contains(&made(&line)),
        "{removed}"
    );
}

/// The editing of mitcad#1526 through the commands the editing context
/// sends: a tangent handle's drag (`tangents`, W17) previewed with the
/// curves' geometry and nothing changed, taken as one step whose tangent
/// the definition and the query keep, refused at a natural end and without
/// goals; a curve's drag as its points' goals; a dimension made driven and
/// driving again, its value an expression; a relation removed by its id.
#[test]
fn tangent_handles_curves_and_dimensions_are_edited() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let fit = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_spline", "sketch": "F1", "ends": ["free", "natural"],
               "points": [[0, 0, 0], [20, 10, 0], [40, 0, 10]]}),
    );
    let spline = made(&fit);
    command(
        &mut doc,
        json!({"cmd": "sketch3d.fix", "sketch": "F1", "points": fit["spline_points"]}),
    );
    let s = sketch(&doc, "F1");
    let tangents = item(&s, "curves", &spline)["geometry"]["end_tangents"].clone();
    assert!(tangents[0].is_array(), "{tangents}");
    let before = xyz(&tangents[0]);
    // The definition keeps the free start's, none of the natural end.
    let stored = item(&s, "curves", &spline)["tangents"].clone();
    assert!(near(xyz(&stored[0]), before), "{stored}");
    assert!(stored[1].is_null(), "{stored}");

    // The handle pulled: previewed, nothing changes.
    let label = undo_label(&doc);
    let preview = command(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1", "preview": true,
               "tangents": [{"curve": spline, "end": "start", "to": [10, 0, 90]}]}),
    );
    let key = spline.as_str().unwrap();
    assert!(
        near(
            xyz(&preview["curves"][key]["end_tangents"][0]),
            [10.0, 0.0, 90.0]
        ),
        "{preview}"
    );
    assert!(same(&sketch(&doc, "F1"), &s));
    assert_eq!(undo_label(&doc), label);
    // Taken: one step; the query and the definition keep it.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1",
               "tangents": [{"curve": spline, "end": "start", "to": [10, 0, 90]}]}),
    );
    assert_eq!(undo_label(&doc), "Drag in 3DSketch1");
    let s = sketch(&doc, "F1");
    let curve = item(&s, "curves", &spline);
    assert!(near(
        xyz(&curve["geometry"]["end_tangents"][0]),
        [10.0, 0.0, 90.0]
    ));
    assert!(near(xyz(&curve["tangents"][0]), [10.0, 0.0, 90.0]));
    // A natural end has no handle; a drag needs goals.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1",
               "tangents": [{"curve": spline, "end": "end", "to": [10, 0, 90]}]}),
    );
    assert!(error.contains("natural"), "{error}");
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1", "points": []}),
    );
    assert!(error.contains("\"tangents\""), "{error}");
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1",
               "tangents": [{"curve": spline, "end": "middle", "to": [1, 0, 0]}]}),
    );
    assert!(error.contains("start or end"), "{error}");
    assert_eq!(undo_label(&doc), "Drag in 3DSketch1");
    command(&mut doc, json!({"cmd": "undo"}));
    let s = sketch(&doc, "F1");
    assert!(near(
        xyz(&item(&s, "curves", &spline)["geometry"]["end_tangents"][0]),
        before
    ));

    // A line dragged whole: both its points pulled by the same offset.
    let line = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, -20, 0],
               "end": [30, -20, 0], "length": 30}),
    );
    let (a, b) = (line["start"].clone(), line["end"].clone());
    command(
        &mut doc,
        json!({"cmd": "sketch3d.drag", "sketch": "F1",
               "points": [{"point": a, "to": [0, -20, 15]}, {"point": b, "to": [30, -20, 15]}]}),
    );
    let s = sketch(&doc, "F1");
    assert!(near(at(&s, &a), [0.0, -20.0, 15.0]));
    assert!(near(at(&s, &b), [30.0, -20.0, 15.0]));
    assert_eq!(undo_label(&doc), "Drag in 3DSketch1");

    // The line's length: driven, then driving again; an expression.
    let k = line["dimensions"][0].clone();
    let length = item(&s, "dimensions", &k).clone();
    assert_eq!(length["type"], "length", "{length}");
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_driven", "sketch": "F1", "dimension": k}),
    );
    assert_eq!(item(&sketch(&doc, "F1"), "dimensions", &k)["driven"], true);
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_driven", "sketch": "F1", "dimension": k,
               "driven": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1", "dimension": k,
               "value": "15 * 3"}),
    );
    let s = sketch(&doc, "F1");
    let dimension = item(&s, "dimensions", &k);
    assert_eq!(dimension["driven"], false, "{dimension}");
    assert!((dimension["measured"].as_f64().unwrap() - 45.0).abs() <= TOL);
    assert_eq!(undo_label(&doc), "Edit Dimension in 3DSketch1");
    // A relation removed by its id: the spline's first Fix.
    let fix = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["type"] == "fix")
        .unwrap()["id"]
        .clone();
    let removed = command(
        &mut doc,
        json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": [fix]}),
    );
    assert!(
        removed["removed"].as_array().unwrap().contains(&fix),
        "{removed}"
    );
}

/// Fix and Unfix, Attach and Detach, On Sketch Plane and Release from
/// Plane each change their own relation; Release Plane frees a plane-bound
/// arc's normal.
#[test]
fn relations_are_added_and_released_one_by_one() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let fixed = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [5, 5, 5], "fixed": true}),
    );
    let p = made(&fixed);
    assert_eq!(fixed["constraints"].as_array().unwrap().len(), 1);
    assert_eq!(fixed["status"]["dof"], 0);
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.fix", "sketch": "F1", "points": [p]}),
    );
    assert!(error.contains("fixed already"), "{error}");
    let unfixed = command(
        &mut doc,
        json!({"cmd": "sketch3d.fix", "sketch": "F1", "points": [p], "fixed": false}),
    );
    assert_eq!(unfixed["removed"], fixed["constraints"]);
    assert_eq!(unfixed["status"]["dof"], 3);

    // Attached to the origin, then detached where it is.
    let attached = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [1, 2, 3], "source": "origin"}),
    );
    let q = made(&attached);
    assert!(near(at(&sketch(&doc, "F1"), &q), [0.0; 3]));
    let detached = command(
        &mut doc,
        json!({"cmd": "sketch3d.detach", "sketch": "F1", "points": [q]}),
    );
    assert_eq!(detached["removed"], attached["constraints"]);
    assert!(near(at(&sketch(&doc, "F1"), &q), [0.0; 3]));
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.detach", "sketch": "F1", "points": [q]}),
    );
    assert!(error.contains("not attached"), "{error}");

    // On the sketch plane, then released from it.
    let lifted = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [3, 4, 7]}),
    );
    let r = made(&lifted);
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1",
               "constraint": {"type": "on_sketch_plane", "points": [r]}}),
    );
    let s = sketch(&doc, "F1");
    assert!(near(at(&s, &r), [3.0, 4.0, 0.0]), "{s}");
    assert_eq!(item(&s, "points", &r)["on_sketch_plane"], true);
    let released = command(
        &mut doc,
        json!({"cmd": "sketch3d.release_from_plane", "sketch": "F1", "points": [r]}),
    );
    assert_eq!(released["released"], json!([r]));
    let s = sketch(&doc, "F1");
    assert!(
        !s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "on_sketch_plane"),
        "{s}"
    );

    // A plane-bound arc (as a converted sketch has it) freed by Release
    // Plane: its normal gains its two freedoms.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "spatial_sketch",
               "points": [{"id": "p1", "at": [0, 0, 0]}, {"id": "p2", "at": [10, 0, 0]},
                          {"id": "p3", "at": [0, 10, 0]}],
               "curves": [{"id": "c4", "type": "arc", "center": "p1", "start": "p2",
                           "end": "p3", "normal": [0, 0, 1], "plane": {"axis": "z"}}]}}),
    );
    let bound = sketch(&doc, "F2");
    assert_eq!(item(&bound, "curves", &json!("c4"))["plane_bound"], true);
    let dof = bound["status"]["dof"].as_u64().unwrap();
    let freed = command(
        &mut doc,
        json!({"cmd": "sketch3d.release_plane", "sketch": "F2", "curves": ["c4"]}),
    );
    assert!(freed["status"]["dof"].as_u64().unwrap() > dof, "{freed}");
    let arc = item(&sketch(&doc, "F2"), "curves", &json!("c4")).clone();
    assert!(
        arc.get("plane").is_none() && arc.get("plane_bound").is_none(),
        "{arc}"
    );
    assert!(near(xyz(&arc["normal"]), [0.0, 0.0, 1.0]));
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.release_plane", "sketch": "F2", "curves": ["c4"]}),
    );
    assert!(
        error.contains("not an arc or circle bound to a plane"),
        "{error}"
    );
    assert_eq!(undo_label(&doc), "Release Plane in 3DSketch2");
}

/// The lines the mock kernel gives the edges of a 40 x 30 x 20 box (F1)
/// of `height`: its top front and back edges and its upright front right
/// one.
fn box_edges(doc: &Document<MockKernel>, height: f64) {
    let h = height;
    for (name, start, end) in [
        ("E{F1:side0|F1:top}", [0.0, 0.0, h], [40.0, 0.0, h]),
        ("E{F1:side2|F1:top}", [40.0, 30.0, h], [0.0, 30.0, h]),
        ("E{F1:side0|F1:side1}", [40.0, 0.0, 0.0], [40.0, 0.0, h]),
    ] {
        let key = name
            .parse::<mitcad_model::topo::TopoName>()
            .unwrap()
            .to_string();
        doc.kernel()
            .named_curves
            .borrow_mut()
            .insert(key, mitcad_model::kernel::Curve3::Line { start, end });
    }
}

/// Linked reference geometry through the api (mitcad#1543, W12's records):
/// a point at an edge's middle and one put on an edge by
/// `sketch3d.attach` (its reference curve made, then reused by
/// `sketch3d.include`); the query's records; Replace Reference of the same
/// kind only, one undo step; a source given with the occurrence of the
/// sketch's own component keeps no link; Detach removes the record with
/// its last user; saved and read again, the records are the same; the box
/// suppressed, the record that lost its source is failed.
#[test]
fn linked_references_are_made_listed_replaced_and_detached() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 40, "width": 30, "height": 20, "operation": "new_body"}}),
    );
    box_edges(&doc, 20.0);
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let front = json!({"body": "F1.b0", "edge": "E{F1:side0|F1:top}", "at": "middle"});
    let upright = json!({"body": "F1.b0", "edge": "E{F1:side0|F1:side1}"});
    let middle = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [19, 1, 19],
               "source": front}),
    );
    let p = made(&middle);
    let free = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [40, 0, 14]}),
    );
    let q = made(&free);
    let on = command(
        &mut doc,
        json!({"cmd": "sketch3d.attach", "sketch": "F2", "point": q, "source": upright}),
    );
    let reference = on["reference"].clone();
    assert_eq!(on["curves"], json!([reference]), "{on}");
    let again = command(
        &mut doc,
        json!({"cmd": "sketch3d.include", "sketch": "F2", "source": upright}),
    );
    assert_eq!(made(&again), reference);
    assert_eq!(again["reused"], true);
    let s = sketch(&doc, "F2");
    assert!(near(at(&s, &p), [20.0, 0.0, 20.0]), "{s}");
    let on_edge = at(&s, &q);
    assert!(near(on_edge, [40.0, 0.0, 14.0]), "{s}");
    let records = s["references"].as_array().unwrap();
    assert_eq!(records.len(), 2, "{s}");
    assert_eq!(records[0]["id"], middle["constraints"][0]);
    assert_eq!(records[0]["kind"], "point");
    assert_eq!(records[0]["mode"], "include");
    assert_eq!(records[0]["body"], "F1.b0");
    assert_eq!(records[0]["status"], "ok");
    assert_eq!(records[0]["users"], json!([p]));
    assert_eq!(records[1]["id"], reference);
    assert_eq!(records[1]["kind"], "curve");
    assert_eq!(records[1]["users"], on["made"]);
    assert_eq!(records[1]["status"], "ok");

    // Replace Reference: of the same kind only, then the back edge's middle.
    let attachment = records[0]["id"].clone();
    for (item, source, part) in [
        (attachment.clone(), upright.clone(), "is a curve"),
        (reference.clone(), front.clone(), "is a point"),
    ] {
        let error = rejected(
            &mut doc,
            json!({"cmd": "sketch3d.replace_reference", "sketch": "F2", "item": item,
                   "source": source}),
        );
        assert!(error.contains(part), "{error}");
    }
    command(
        &mut doc,
        json!({"cmd": "sketch3d.replace_reference", "sketch": "F2", "item": attachment,
               "source": {"body": "F1.b0", "edge": "E{F1:side2|F1:top}", "at": "middle"}}),
    );
    assert!(near(at(&sketch(&doc, "F2"), &p), [20.0, 30.0, 20.0]));
    assert_eq!(undo_label(&doc), "Replace Reference in 3DSketch1");
    doc.undo();
    assert!(near(at(&sketch(&doc, "F2"), &p), [20.0, 0.0, 20.0]));

    // The occurrence of the sketch's own component: no link is kept; a
    // context needs its occurrence.
    let own = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [1, 1, 1],
               "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}", "at": "start",
                          "occurrence": ""}}),
    );
    let s = sketch(&doc, "F2");
    let kept = item(&s, "constraints", &own["constraints"][0]);
    assert!(kept["source"].get("link").is_none(), "{kept}");
    assert!(near(at(&s, &made(&own)), [40.0, 0.0, 0.0]), "{s}");
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [1, 1, 1],
               "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}", "at": "end",
                          "context": "O1"}}),
    );
    assert!(error.contains("needs its \"occurrence\""), "{error}");

    // Saved and read again: the same records and geometry.
    let file = doc.to_json();
    let mut back = Document::from_json(&file, MockKernel::default()).unwrap();
    box_edges(&back, 20.0);
    back.recompute();
    let read = sketch(&back, "F2");
    let s = sketch(&doc, "F2");
    for list in ["points", "constraints", "references"] {
        assert!(same(&read[list], &s[list]), "{list}: {read}");
    }

    // Detach: the Point on Curve relation and the record it was the last
    // user of go; undone, both are back.
    let detached = command(
        &mut doc,
        json!({"cmd": "sketch3d.detach", "sketch": "F2", "points": [q]}),
    );
    let removed = detached["removed"].as_array().unwrap();
    assert!(removed.contains(&reference), "{detached}");
    assert!(removed.contains(&on["made"][0]), "{detached}");
    assert!(near(at(&sketch(&doc, "F2"), &q), on_edge));
    doc.undo();
    let s = sketch(&doc, "F2");
    assert_eq!(s["references"].as_array().unwrap().len(), 3, "{s}");

    // The box suppressed: the sketch fails on a lost source; its record
    // says so.
    command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F1"}));
    let s = sketch(&doc, "F2");
    assert_eq!(s["failure"]["kind"], "reference", "{s}");
    let failed: Vec<&Value> = s["references"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "failed")
        .collect();
    assert_eq!(failed.len(), 1, "{s}");
    assert!(failed[0]["message"].as_str().is_some(), "{s}");
}

/// The planar sketch's commands and queries refuse a 3D sketch saying so
/// (none can drop its z or references), and the 3D query a planar sketch;
/// a planar sketch beside a 3D one is read again as it was; a feature type
/// this build does not know is refused rather than dropped.
#[test]
fn planar_and_spatial_sketches_stay_apart() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 20, "height": 10}),
    );
    command(&mut doc, json!({"cmd": "sketch3d.create", "support": "xz"}));
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_circle", "sketch": "F2", "center": [0, 0, 5],
               "radius": 4, "normal_axis": "z"}),
    );
    for (command, part) in [
        (
            json!({"cmd": "sketch.add_line", "sketch": "F2", "start": [0, 0], "end": [1, 0]}),
            "3D sketch: the sketch3d.* commands edit it",
        ),
        (
            json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [0, 0, 0]}),
            "is not a 3D sketch",
        ),
    ] {
        let error = rejected(&mut doc, command.clone());
        assert!(error.contains(part), "{command}: {error}");
    }
    // A 3D sketch's geometry is copied with its z and relations
    // (mitcad#1661, sketch3d_clipboard_tests.rs).
    for (q, part) in [
        (
            json!({"query": "sketch", "uid": "F2"}),
            "the sketch3d query reads it",
        ),
        (json!({"query": "sketch3d", "uid": "F1"}), "2D sketch"),
    ] {
        let error = doc.query(&q.to_string()).unwrap_err().to_string();
        assert!(error.contains(part), "{q}: {error}");
    }
    let s = sketch(&doc, "F2");
    assert_eq!(s["support"], "xz");
    let circle = &s["curves"][0];
    assert_eq!(circle["type"], "circle");
    assert!(
        (circle["geometry"]["radius"].as_f64().unwrap() - 4.0).abs() <= TOL,
        "{s}"
    );
    assert_eq!(s["dimensions"][0]["type"], "radius");
    assert_eq!(s["constraints"][0]["type"], "normal_align");
    // On XZ the local frame's normal is the component's -Y or +Y.
    assert!(xyz(&s["frame"]["normal"])[1].abs() > 1.0 - TOL, "{s}");

    let file = doc.to_json();
    let saved: Value = serde_json::from_str(&file).unwrap();
    let mut back = Document::from_json(&file, MockKernel::default()).unwrap();
    back.recompute();
    let saved_again: Value = serde_json::from_str(&back.to_json()).unwrap();
    assert_eq!(saved_again["features"], saved["features"]);
    assert_eq!(
        query(&back, json!({"query": "sketch", "uid": "F1"}))["dimensions"],
        query(&doc, json!({"query": "sketch", "uid": "F1"}))["dimensions"]
    );
    assert!(same(&sketch(&back, "F2")["curves"], &s["curves"]));

    // A reader that lacks a feature type refuses the design.
    let mut newer = saved.clone();
    newer["features"][1]["type"] = json!("spatial_sketch_v9");
    let error = Document::from_json(&newer.to_string(), MockKernel::default())
        .err()
        .expect("an unknown feature type is refused");
    let error = format!("{error:?}");
    assert!(
        error.contains("unknown variant `spatial_sketch_v9`"),
        "{error}"
    );
}

/// Pipe and Sweep along 3D sketch curves (mitcad#1523): the query's
/// chains give the route whole as a typed path, which a Pipe takes and
/// keeps as written; a construction line on from D is in no chain, but a
/// Pipe takes it when it is listed; a closed spline is a closed path
/// (mitcad#1550); the planar path form naming the 3D sketch is refused.
#[test]
fn chains_are_listed_and_a_pipe_takes_one() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let [ab, bc, cd] = route(&mut doc);
    let lines = json!([ab["made"][0], bc["made"][0], cd["made"][0]]);
    let extra = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": cd["end"],
               "end": [60, 30, 20], "construction": true}),
    );
    let s = sketch(&doc, "F1");
    let chains = s["chains"].as_array().unwrap();
    assert_eq!(chains.len(), 1, "{s}");
    assert_eq!(chains[0]["curves"], lines);
    assert_eq!(chains[0]["closed"], false);
    assert!(chains[0].get("branches").is_none());
    let path = chains[0]["path"].clone();
    assert_eq!(path, json!({"spatial_sketch": "F1", "curves": lines}));

    let pipe = |path: &Value| {
        json!({"cmd": "add_feature",
               "def": {"type": "pipe", "path": path, "size": 4, "operation": "new_body"}})
    };
    let added = command(&mut doc, pipe(&path));
    let feature = query(&doc, json!({"query": "feature", "uid": added["uid"]}));
    assert!(feature["error"].is_null(), "{feature}");
    assert_eq!(feature["def"]["path"], path);
    let explicit = json!({"spatial_sketch": "F1", "curves": [cd["made"][0], extra["made"][0]],
                          "reversed": true});
    let added = command(&mut doc, pipe(&explicit));
    let feature = query(&doc, json!({"query": "feature", "uid": added["uid"]}));
    assert!(feature["error"].is_null(), "{feature}");
    assert_eq!(feature["def"]["path"], explicit);

    let ring = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_spline", "sketch": "F1", "closed": true,
               "points": [[100, 0, 0], [110, 0, 0], [105, 5, 3]]}),
    );
    // A closed spline: a closed path of its own (mitcad#1550), its chain
    // listed with it.
    let closed = json!({"spatial_sketch": "F1", "curves": [ring["made"][0]]});
    let added = command(&mut doc, pipe(&closed));
    let feature = query(&doc, json!({"query": "feature", "uid": added["uid"]}));
    assert!(feature["error"].is_null(), "{feature}");
    let r = query(
        &doc,
        json!({"query": "sketch3d", "uid": "F1", "path": {"curves": [ring["made"][0]]}}),
    );
    assert_eq!(r["route"]["closed"], true, "{r}");
    let planar = json!({"sketch": "F1", "curves": [ab["made"][0]]});
    let error = rejected(&mut doc, pipe(&planar));
    assert!(error.contains("\"spatial_sketch\""), "{error}");
    let chains = sketch(&doc, "F1")["chains"].as_array().unwrap().clone();
    assert_eq!(chains.len(), 2);
    assert_eq!(chains[1]["closed"], true);
    assert_eq!(chains[1]["path"], closed);
}

/// What the editing context of the application reads (mitcad#1524): a
/// reference curve's geometry in local coordinates, which it draws; the
/// timeline's degrees of freedom and light bulb of a 3D sketch, hidden
/// and shown by `set_feature_visible` in one undo step each.
#[test]
fn the_editing_context_reads_references_freedoms_and_light_bulbs() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 40, "width": 30, "height": 20, "operation": "new_body"}}),
    );
    box_edges(&doc, 20.0);
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [1, 2, 3]}),
    );
    let upright = json!({"body": "F1.b0", "edge": "E{F1:side0|F1:side1}"});
    let included = command(
        &mut doc,
        json!({"cmd": "sketch3d.include", "sketch": "F2", "source": upright}),
    );
    let s = sketch(&doc, "F2");
    let curve = item(&s, "curves", &made(&included));
    let geometry = &curve["geometry"];
    assert_eq!(geometry["kind"], "line", "{curve}");
    assert!(near(xyz(&geometry["start"]), [40.0, 0.0, 0.0]), "{curve}");
    assert!(near(xyz(&geometry["end"]), [40.0, 0.0, 20.0]), "{curve}");

    let row = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "timeline"}))["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["uid"] == "F2")
            .cloned()
            .unwrap()
    };
    let shown = row(&doc);
    assert_eq!(shown["dof"], s["status"]["dof"], "{shown}");
    assert!(shown["dof"].as_u64().unwrap() > 0, "{shown}");
    assert_eq!(shown["visible"], true, "{shown}");
    command(
        &mut doc,
        json!({"cmd": "set_feature_visible", "uid": "F2", "visible": false}),
    );
    assert_eq!(row(&doc)["visible"], false);
    assert_eq!(undo_label(&doc), "Hide 3DSketch1");
    doc.undo();
    assert_eq!(row(&doc)["visible"], true);
}

/// mitcad#1525: the commands the 3D sketch's drawing tools send, as they
/// send them. A Line from the origin with typed Offsets (Delta X, and
/// Delta Y and Z typed as an explicit zero, each a driving dimension), on
/// along +Y by Axis and Length, then a segment with only Delta Z typed,
/// which leaves its end free in X and Y (no hidden plane relation); a
/// Point snapped onto this sketch's line, its Point on Curve relation
/// merged into the same undo step; a Fit Point Spline whose second point
/// takes the Delta X typed for it in the same step; a Center Point Arc on
/// the XY plane with Keep Plane and its Radius and Arc Length typed.
#[test]
fn the_drawing_tools_commands_make_one_step_each() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let ab = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0], "end": [37, 2, 1],
               "offsets": ["40 mm", "0 mm", "0 mm"], "start_source": "origin"}),
    );
    assert_eq!(ab["dimensions"].as_array().unwrap().len(), 3, "{ab}");
    assert_eq!(ab["status"]["dof"], 0, "{ab}");
    let bc = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": ab["end"], "end": [41, 27, 0],
               "axis": "y", "sense": "positive", "length": "30 mm"}),
    );
    assert_eq!(bc["status"]["dof"], 0, "{bc}");
    let cd = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": bc["end"], "end": [41, 31, 18],
               "offsets": [null, null, "20 mm"]}),
    );
    assert_eq!(cd["dimensions"].as_array().unwrap().len(), 1, "{cd}");
    assert_eq!(cd["status"]["dof"], 2, "the end is free in X and Y: {cd}");
    let s = sketch(&doc, "F1");
    assert!(near(at(&s, &ab["end"]), [40.0, 0.0, 0.0]), "{s}");
    assert!(near(at(&s, &bc["end"]), [40.0, 30.0, 0.0]), "{s}");
    assert!((at(&s, &cd["end"])[2] - 20.0).abs() <= TOL, "{s}");
    assert!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["type"] != "on_sketch_plane"),
        "no hidden plane relation: {s}"
    );

    // A point on this sketch's line, made with its relation: one step.
    let depth = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    let point = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [12, 0.5, 0]}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1",
               "constraint": {"type": "point_on", "point": made(&point), "curve": made(&ab)}}),
    );
    command(
        &mut doc,
        json!({"cmd": "merge_undo", "depth": depth, "label": "Add Point in 3DSketch1"}),
    );
    let s = sketch(&doc, "F1");
    let on = at(&s, &made(&point));
    assert!(on[1].abs() <= TOL && on[2].abs() <= TOL, "on the line: {s}");
    assert_eq!(undo_label(&doc), "Add Point in 3DSketch1");
    assert_eq!(
        command(&mut doc, json!({"cmd": "undo"}))["label"],
        "Add Point in 3DSketch1"
    );
    let s = sketch(&doc, "F1");
    assert!(
        s["points"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["id"] != made(&point)),
        "{s}"
    );
    assert!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["type"] != "point_on"),
        "{s}"
    );

    // A spline from D, its second point's typed Delta X in the same step.
    let depth = query(&doc, json!({"query": "document"}))["undo_depth"].clone();
    let spline = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_spline", "sketch": "F1",
               "points": [cd["end"], [58, 40, 25], [70, 50, 30]]}),
    );
    let second = spline["spline_points"][1].clone();
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "offset", "a": cd["end"], "b": second, "axis": "x"},
               "value": "15 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "merge_undo", "depth": depth, "label": "Add Spline in 3DSketch1"}),
    );
    let s = sketch(&doc, "F1");
    assert!(
        (at(&s, &second)[0] - at(&s, &cd["end"])[0] - 15.0).abs() <= TOL,
        "{s}"
    );
    assert_eq!(undo_label(&doc), "Add Spline in 3DSketch1");

    // A Center Point Arc on XY, Keep Plane, its radius and arc length.
    let arc = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "center", "center": [0, 50, 0],
               "start": [9, 50, 0], "end": [0, 61, 0], "normal_axis": "z", "normal_sense": "positive",
               "radius": "10 mm", "arc_length": "15 mm"}),
    );
    let s = sketch(&doc, "F1");
    let curve = item(&s, "curves", &made(&arc));
    assert!(
        (curve["geometry"]["radius"].as_f64().unwrap() - 10.0).abs() <= TOL,
        "{curve}"
    );
    assert!(
        (curve["geometry"]["length"].as_f64().unwrap() - 15.0).abs() <= TOL,
        "{curve}"
    );
    assert!(
        near(xyz(&curve["geometry"]["normal"]), [0.0, 0.0, 1.0]),
        "{curve}"
    );
    assert!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["type"] == "normal_align" && k["axis"] == "z"),
        "{s}"
    );
}

/// A planar sketch turned into a 3D sketch in place (mitcad#1544):
/// `sketch3d.convert` keeps the feature, its ids and parameters, and
/// returns the in-plane relations, the in-plane fix of the fixed corner,
/// the plane group and the status (fully constrained); the query flags the
/// in-plane items and the points on the sketch plane; one undo step takes
/// it back. A 3D sketch is not converted again; a sketch whose profile an
/// Extrude uses converts, keeping it (mitcad#1550).
#[test]
fn a_planar_sketch_is_converted_in_place() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 20, "height": 10}),
    );
    let converted = command(&mut doc, json!({"cmd": "sketch3d.convert", "sketch": "F1"}));
    assert_eq!(converted["uid"], "F1", "{converted}");
    assert_eq!(converted["name"], "Sketch1");
    assert_eq!(converted["type"], "spatial_sketch");
    let sorted = |value: &Value| {
        let mut list: Vec<String> = value
            .as_array()
            .unwrap_or_else(|| panic!("{value} is not a list"))
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        list.sort();
        list
    };
    assert_eq!(
        sorted(&converted["kept"]["points"]),
        ["p5", "p6", "p7", "p8"]
    );
    assert_eq!(
        sorted(&converted["kept"]["curves"]),
        ["c1", "c2", "c3", "c4"]
    );
    assert_eq!(
        sorted(&converted["kept"]["constraints"]),
        ["k1", "k2", "k3", "k4"]
    );
    assert_eq!(sorted(&converted["kept"]["dimensions"]), ["k5", "k6"]);
    assert_eq!(
        sorted(&converted["in_plane"]),
        ["k1", "k2", "k3", "k4", "k5", "k6"]
    );
    assert_eq!(converted["fixes"], json!(["k7"]));
    assert_eq!(converted["on_sketch_plane"]["relation"], "k8");
    assert_eq!(
        sorted(&converted["on_sketch_plane"]["points"]),
        ["p5", "p6", "p7", "p8"]
    );
    assert_eq!(converted["status"]["dof"], 0);
    assert_eq!(converted["status"]["fully_constrained"], true);
    assert_eq!(converted["status"]["redundant"], json!([]));
    assert_eq!(converted["rewritten"], json!([]));

    let s = sketch(&doc, "F1");
    for point in s["points"].as_array().unwrap() {
        assert_eq!(point["on_sketch_plane"], true, "{point}");
    }
    for id in ["k1", "k2", "k3", "k4", "k7"] {
        assert_eq!(item(&s, "constraints", &json!(id))["in_plane"], true, "{s}");
    }
    let width = item(&s, "dimensions", &json!("k5"));
    assert_eq!(width["in_plane"], true);
    assert_eq!(width["value"], "d1");
    assert!(
        (width["measured"].as_f64().unwrap() - 20.0).abs() <= TOL,
        "{s}"
    );
    // Its own dimensions still drive it.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 25}),
    );
    let s = sketch(&doc, "F1");
    assert!(
        (item(&s, "dimensions", &json!("k5"))["measured"]
            .as_f64()
            .unwrap()
            - 25.0)
            .abs()
            <= TOL,
        "{s}"
    );
    command(&mut doc, json!({"cmd": "undo"}));

    let error = rejected(&mut doc, json!({"cmd": "sketch3d.convert", "sketch": "F1"}));
    assert!(error.contains("is a 3D sketch already"), "{error}");
    assert_eq!(undo_label(&doc), "Convert Sketch1 to 3D Sketch");
    command(&mut doc, json!({"cmd": "undo"}));
    query(&doc, json!({"query": "sketch", "uid": "F1"}));

    // A sketch whose profile an Extrude uses converts (mitcad#1550): the
    // Extrude keeps its region, which the 3D sketch's query lists.
    let region = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    command(&mut doc, json!({"cmd": "sketch3d.convert", "sketch": "F1"}));
    let s = sketch(&doc, "F1");
    assert_eq!(s["regions"], json!([region]), "{s}");
    assert_eq!(s["flat"], true);
}

/// A 3D sketch turned back into 2D (mitcad#1550): `sketch3d.convert_to_2d`
/// gives the planar sketch's ids, the 3D relations it left out (the
/// in-plane fix, On Sketch Plane) and its freedom; the Extrude keeps its
/// profile, one undo step takes it back. A 3D sketch with a point off its
/// plane is refused naming the point (its query says it is not flat).
#[test]
fn a_3d_sketch_is_turned_back_into_2d() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 20, "height": 10}),
    );
    let region = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    command(&mut doc, json!({"cmd": "sketch3d.convert", "sketch": "F1"}));
    let back = command(
        &mut doc,
        json!({"cmd": "sketch3d.convert_to_2d", "sketch": "F1"}),
    );
    assert_eq!(back["type"], "sketch", "{back}");
    assert_eq!(back["dropped"], json!(["k7", "k8"]), "{back}");
    assert_eq!(back["dof"], 0);
    assert_eq!(back["kept"]["constraints"], json!(["k1", "k2", "k3", "k4"]));
    assert_eq!(undo_label(&doc), "Convert Sketch1 to 2D Sketch");
    let planar = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    assert!(planar.to_string().contains(region), "{planar}");
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.convert_to_2d", "sketch": "F1"}),
    );
    assert!(error.contains("is a 2D sketch already"), "{error}");

    // A point off the plane.
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F3", "start": [0, 0, 0],
               "end": [10, 0, 5]}),
    );
    let s = sketch(&doc, "F3");
    assert_eq!(s["flat"], false, "{s}");
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.convert_to_2d", "sketch": "F3"}),
    );
    assert!(
        error.contains("3DSketch1 cannot become a 2D sketch: it is not all in its sketch plane"),
        "{error}"
    );
    assert!(error.contains("mm off the sketch plane"), "{error}");
}

/// The 3D sketch's Fillet (mitcad#1548) through `sketch3d.fillet`: the
/// route's corner B by its point, radius 10: AB and BC keep their ids and
/// end at the tangent points, the arc is tangent to both with a radius
/// dimension, B stays where they meet; one undo step `Fillet in 3DSketch1`.
/// Its radius edited through `set_dimension`. The refusals: one line
/// named twice, lines that share no end, a radius that does not fit, a
/// point that is no corner, input of the wrong form. The corner C by its
/// two lines with a parameter's name as the radius; undone, the route is
/// as it was.
#[test]
fn corners_are_filleted_and_their_radius_edited() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let [ab, bc, cd] = route(&mut doc);
    let before = sketch(&doc, "F1");
    let b = ab["end"].clone();
    let result = command(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "points": [b], "radius": 10}),
    );
    assert_eq!(undo_label(&doc), "Fillet in 3DSketch1");
    let f = &result["fillets"][0];
    assert_eq!(f["corner"], b);
    assert_eq!(f["lines"], json!([made(&ab), made(&bc)]));
    assert_eq!(result["made"], json!([f["arc"]]));
    assert_eq!(result["status"]["dof"], 0, "{result}");
    assert_eq!(result["status"]["redundant"], json!([]), "{result}");
    let radius = result["dimension"].clone();
    assert!(radius.is_string(), "{result}");
    let s = sketch(&doc, "F1");
    let [ta, tb] = [
        f["tangent_points"][0].clone(),
        f["tangent_points"][1].clone(),
    ];
    assert!(near(at(&s, &ta), [30.0, 0.0, 0.0]), "{s}");
    assert!(near(at(&s, &tb), [40.0, 10.0, 0.0]), "{s}");
    assert!(near(at(&s, &f["center"]), [30.0, 10.0, 0.0]), "{s}");
    assert!(near(at(&s, &b), [40.0, 0.0, 0.0]), "{s}");
    let line_ab = item(&s, "curves", &made(&ab));
    assert_eq!((&line_ab["start"], &line_ab["end"]), (&ab["start"], &ta));
    let line_bc = item(&s, "curves", &made(&bc));
    assert_eq!((&line_bc["start"], &line_bc["end"]), (&tb, &bc["end"]));
    let arc = item(&s, "curves", &f["arc"]);
    assert_eq!(arc["type"], "arc");
    assert!(
        (arc["geometry"]["radius"].as_f64().unwrap() - 10.0).abs() <= TOL,
        "{arc}"
    );
    for corner_line in f["corner_lines"].as_array().unwrap() {
        assert_eq!(item(&s, "curves", corner_line)["construction"], true);
    }
    let tangents = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k["type"] == "tangent" && (k["a"] == f["arc"] || k["b"] == f["arc"]))
        .count();
    assert_eq!(tangents, 2, "{s}");
    assert_eq!(item(&s, "dimensions", &radius)["type"], "radius");
    // AB's length still measures A to B: on a construction line A-B.
    let length = ab["dimensions"][0].clone();
    assert_eq!(
        item(&s, "dimensions", &length)["line"],
        f["full_lines"][0],
        "{s}"
    );
    let measured = s["status"]["measured"][length.as_str().unwrap()]
        .as_f64()
        .unwrap();
    assert!((measured - 40.0).abs() <= TOL, "{s}");

    // The radius through its dimension, one undo step of its own.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1", "dimension": radius, "value": 5}),
    );
    assert_eq!(undo_label(&doc), "Edit Dimension in 3DSketch1");
    let s = sketch(&doc, "F1");
    assert!(near(at(&s, &ta), [35.0, 0.0, 0.0]), "{s}");
    assert!(near(at(&s, &tb), [40.0, 5.0, 0.0]), "{s}");
    command(&mut doc, json!({"cmd": "undo"}));

    // The refusals leave everything as it was.
    let kept = sketch(&doc, "F1");
    let refused = |doc: &mut Document<MockKernel>, command: Value, wanted: &str| {
        let error = rejected(doc, command);
        assert!(error.contains(wanted), "{error}");
        assert!(same(&sketch(doc, "F1"), &kept));
        assert_eq!(undo_label(doc), "Fillet in 3DSketch1");
    };
    refused(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "a": made(&cd), "b": made(&cd),
               "radius": 2}),
        "name two",
    );
    refused(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "a": made(&ab), "b": made(&cd),
               "radius": 2}),
        "do not share an end",
    );
    refused(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "a": made(&bc), "b": made(&cd),
               "radius": 25}),
        "does not fit",
    );
    refused(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "points": [ab["start"]], "radius": 2}),
        "is no corner of two lines",
    );
    refused(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "a": made(&bc), "radius": 2}),
        "names both",
    );
    refused(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "radius": 2}),
        "\"points\"",
    );

    // C by its two lines, the radius a named parameter's.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "bend", "expression": "6 mm"}),
    );
    let result = command(
        &mut doc,
        json!({"cmd": "sketch3d.fillet", "sketch": "F1", "a": made(&bc), "b": made(&cd),
               "radius": "bend"}),
    );
    let g = &result["fillets"][0];
    assert_eq!(g["corner"], bc["end"]);
    let s = sketch(&doc, "F1");
    assert_eq!(
        item(&s, "dimensions", &result["dimension"])["value"],
        "bend",
        "{s}"
    );
    assert!(
        near(at(&s, &g["tangent_points"][0]), [40.0, 24.0, 0.0]),
        "{s}"
    );
    assert!(
        near(at(&s, &g["tangent_points"][1]), [40.0, 30.0, 6.0]),
        "{s}"
    );
    assert_eq!(s["status"]["dof"], 0, "{s}");

    // Undone, both fillets and the parameter: the route is as it was.
    for _ in 0..3 {
        command(&mut doc, json!({"cmd": "undo"}));
    }
    assert!(same(&sketch(&doc, "F1"), &before));
}

/// Path selection in the application (mitcad#1527): the `sketch3d`
/// query's `route` orders picked curves as a Pipe runs along them, either
/// way, a whole chain from one pick and a sub-chain, and says why other
/// picks are no path (a branch, a curve that only starts where another
/// ends, an unknown curve) in the words a Pipe along them fails with; a
/// Pipe's 3D sketch is listed among its sketches and stays shown; a rib
/// takes the planar form of a 3D sketch's curves in its sketch plane and
/// fails on one out of it, naming the sketch (mitcad#1550), and the query
/// lists the curves in the plane.
#[test]
fn routes_are_ordered_and_refused_as_paths() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let [ab, bc, cd] = route(&mut doc);
    let [ab_c, bc_c, cd_c] = [made(&ab), made(&bc), made(&cd)];
    let route_of = |doc: &Document<MockKernel>, path: Value| {
        query(doc, json!({"query": "sketch3d", "uid": "F1", "path": path}))["route"].clone()
    };

    // Picked in any order: along AB's own direction, A to D.
    let r = route_of(&doc, json!({"curves": [ab_c, cd_c, bc_c]}));
    assert_eq!(r["curves"], json!([ab_c, bc_c, cd_c]), "{r}");
    assert_eq!(r["forward"], json!([true, true, true]), "{r}");
    assert_eq!((&r["start"], &r["end"]), (&ab["start"], &cd["end"]), "{r}");
    // CD first, reversed: D to A, every curve against its direction.
    let r = route_of(
        &doc,
        json!({"curves": [cd_c, ab_c, bc_c], "reversed": true}),
    );
    assert_eq!(r["curves"], json!([cd_c, bc_c, ab_c]), "{r}");
    assert_eq!(r["forward"], json!([false, false, false]), "{r}");
    assert_eq!((&r["start"], &r["end"]), (&cd["end"], &ab["start"]), "{r}");
    // One pick of BC with chain selection: the whole route, through BC
    // along its direction.
    let r = route_of(&doc, json!({"curves": [bc_c], "chain": true}));
    assert_eq!(r["curves"], json!([ab_c, bc_c, cd_c]), "{r}");
    assert_eq!((&r["start"], &r["end"]), (&ab["start"], &cd["end"]), "{r}");
    // A sub-chain: BC alone.
    let r = route_of(&doc, json!({"curves": [bc_c]}));
    assert_eq!(r["curves"], json!([bc_c]), "{r}");
    assert_eq!((&r["start"], &r["end"]), (&bc["start"], &bc["end"]), "{r}");

    // A Pipe along AB, BC and CD as picked: listed under the sketch, which
    // stays shown.
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "pipe",
               "path": {"spatial_sketch": "F1", "curves": [ab_c, bc_c, cd_c]},
               "size": 4, "operation": "new_body"}}),
    );
    let timeline = query(&doc, json!({"query": "timeline"}));
    let row = |uid: &Value| {
        timeline["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["uid"] == *uid)
            .cloned()
            .unwrap()
    };
    assert_eq!(row(&added["uid"])["sketches"], json!(["F1"]), "{timeline}");
    assert_eq!(row(&json!("F1"))["visible"], true, "{timeline}");

    // A line on from C branches the chain there; a line drawn from where
    // A is, but not from A, joins nothing; neither changes the Pipe's own
    // path, which lists its curves.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": bc["end"],
               "end": [60, 30, 0]}),
    );
    let apart = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [-10, 0, 0]}),
    );
    let r = route_of(&doc, json!({"curves": [ab_c], "chain": true}));
    let error = r["error"].as_str().unwrap_or_else(|| panic!("{r}"));
    assert!(error.contains("the path branches at"), "{error}");
    let r = route_of(&doc, json!({"curves": [ab_c, made(&apart)]}));
    let error = r["error"].as_str().unwrap_or_else(|| panic!("{r}"));
    assert!(error.contains("does not join the others"), "{error}");
    let r = route_of(&doc, json!({"curves": ["c99"]}));
    let error = r["error"].as_str().unwrap_or_else(|| panic!("{r}"));
    assert!(error.contains("has no curve c99"), "{error}");
    let feature = query(&doc, json!({"query": "feature", "uid": added["uid"]}));
    assert!(feature["error"].is_null(), "{feature}");

    // A rib along a 3D sketch's curve in its sketch plane, in the planar
    // form (mitcad#1550); along CD, out of the plane, it fails naming the
    // sketch; the query lists the curves in the plane.
    let rib = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "rib",
               "curves": {"sketch": "F1", "curves": [ab_c]}, "thickness": 2,
               "extent": {"type": "depth", "depth": 10}}}),
    );
    let feature = query(&doc, json!({"query": "feature", "uid": rib["uid"]}));
    assert!(feature["error"].is_null(), "{feature}");
    let off = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "rib",
               "curves": {"sketch": "F1", "curves": [cd_c]}, "thickness": 2,
               "extent": {"type": "depth", "depth": 10}}}),
    );
    let feature = query(&doc, json!({"query": "feature", "uid": off["uid"]}));
    let error = feature["error"]
        .as_str()
        .unwrap_or_else(|| panic!("{feature}"));
    assert!(
        error.contains(&format!(
            "3DSketch1's {} does not lie in its sketch plane",
            cd_c.as_str().unwrap()
        )),
        "{error}"
    );
    let plane = sketch(&doc, "F1")["plane_curves"].clone();
    let plane = plane.as_array().unwrap_or_else(|| panic!("{plane}"));
    assert!(plane.contains(&ab_c) && plane.contains(&bc_c), "{plane:?}");
    assert!(!plane.contains(&cd_c), "{plane:?}");
}

/// mitcad#1550: the plane-bound drawing tools' commands, each one undo
/// step: a circle on the YZ plane through two points, an ellipse on the ZX
/// plane with its radii dimensioned (the minor radius solved), a centre
/// rectangle with its width, a polygon with its radius and a text, all on
/// their planes; the query lists the ellipse's solved radii, the texts and
/// the in-plane and Plane-Bound relations; a text is deleted; refused input
/// changes nothing.
#[test]
fn plane_bound_tools_make_one_step_each() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let depth = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "document"}))["undo_depth"]
            .as_u64()
            .unwrap()
    };
    let before = depth(&doc);
    let a = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [5, -10, 0]}),
    );
    let b = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [5, 10, 0]}),
    );
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_circle", "sketch": "F1", "center": [5, 0, 0],
               "radius": 20, "plane": "yz", "diameter": true,
               "through": [made(&a), made(&b)]}),
    );
    assert_eq!(depth(&doc), before + 3);
    let s = sketch(&doc, "F1");
    let c = item(&s, "curves", &made(&circle));
    assert_eq!(c["plane"]["axis"], "x", "{c}");
    assert_eq!(c["plane_bound"], true);
    assert!(
        (c["geometry"]["radius"].as_f64().unwrap() - 10.0).abs() <= TOL,
        "{c}"
    );

    let ellipse = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_ellipse", "sketch": "F1", "center": [0, 50, 0],
               "major": [0, 51, 30], "minor_radius": 4, "plane": "zx",
               "major_value": 30, "minor_value": "12 mm"}),
    );
    assert_eq!(
        ellipse["dimensions"].as_array().unwrap().len(),
        2,
        "{ellipse}"
    );
    let s = sketch(&doc, "F1");
    let e = item(&s, "curves", &made(&ellipse));
    assert_eq!(e["type"], "ellipse");
    assert_eq!(e["normal"], "y", "{e}");
    assert!(
        (e["geometry"]["major"].as_f64().unwrap() - 30.0).abs() <= TOL,
        "{e}"
    );
    assert!(
        (e["geometry"]["minor"].as_f64().unwrap() - 12.0).abs() <= TOL,
        "{e}"
    );
    assert!(
        (e["minor_radius"].as_f64().unwrap() - 12.0).abs() <= TOL,
        "{e}"
    );
    assert!(near(xyz(&e["geometry"]["normal"]), [0.0, 1.0, 0.0]));
    assert!((at(&s, &e["major"])[1] - 50.0).abs() <= TOL, "{s}");

    let rectangle = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_rectangle", "sketch": "F1", "mode": "center",
               "center": [0, 0, 40], "corner": [15, 8, 40], "plane": "xy", "width": 40}),
    );
    assert_eq!(
        rectangle["made"].as_array().unwrap().len(),
        4,
        "{rectangle}"
    );
    assert_eq!(rectangle["corners"].as_array().unwrap().len(), 4);
    assert_eq!(rectangle["construction"].as_array().unwrap().len(), 1);
    let s = sketch(&doc, "F1");
    let kinds: Vec<&str> = rectangle["relations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| item(&s, "constraints", k)["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "in_plane").count(),
        5,
        "{kinds:?}"
    );
    assert_eq!(kinds.iter().filter(|k| **k == "plane_bound").count(), 1);
    let corners: Vec<[f64; 3]> = rectangle["corners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| at(&s, p))
        .collect();
    assert!(
        (corners[1][0] - corners[0][0] - 40.0).abs() <= TOL,
        "{corners:?}"
    );
    assert!(corners.iter().all(|p| (p[2] - 40.0).abs() <= TOL));

    let polygon = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_polygon", "sketch": "F1", "center": [0, 0, -30],
               "vertex": [0, 0, -20], "sides": 5, "plane": "zx", "radius": 10}),
    );
    assert_eq!(polygon["made"].as_array().unwrap().len(), 5, "{polygon}");
    let text = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_text", "sketch": "F1", "text": "MITCAD",
               "at": [0, -40, 0], "plane": "yz", "height": 6}),
    );
    let id = made(&text);
    assert!(id.as_str().unwrap().starts_with('t'), "{text}");
    let s = sketch(&doc, "F1");
    let texts = s["texts"].as_array().unwrap();
    assert_eq!(texts.len(), 1, "{s}");
    assert_eq!(texts[0]["id"], id);
    assert_eq!(texts[0]["plane"], "YZ");
    assert_eq!(texts[0]["normal"], "x");
    assert!(s["failure"].is_null(), "{s}");
    assert_eq!(depth(&doc), before + 7);

    // Refused: an unknown plane, a degenerate rectangle, an empty text.
    let s0 = sketch(&doc, "F1");
    for bad in [
        json!({"cmd": "sketch3d.add_ellipse", "sketch": "F1", "center": [0, 0, 0],
               "major": [10, 0, 0], "minor_radius": 3, "plane": "xz"}),
        json!({"cmd": "sketch3d.add_rectangle", "sketch": "F1", "mode": "two_point",
               "a": [0, 0, 0], "b": [0, 10, 0], "plane": "xy"}),
        json!({"cmd": "sketch3d.add_rectangle", "sketch": "F1", "mode": "two_point",
               "a": [0, 0, 0], "corner": [0, 10, 0], "plane": "xy"}),
        json!({"cmd": "sketch3d.add_text", "sketch": "F1", "text": "", "at": [0, 0, 0],
               "plane": "xy", "height": 5}),
    ] {
        rejected(&mut doc, bad);
    }
    assert!(same(&sketch(&doc, "F1"), &s0));

    // The text deleted on its own, as one step.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.remove", "sketch": "F1", "items": [id]}),
    );
    assert!(sketch(&doc, "F1")["texts"].is_null());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(sketch(&doc, "F1")["texts"].as_array().unwrap().len(), 1);
}

/// mitcad#1661: shapes on a rotated placement plane (the XY plane turned
/// 30° about X: x along X, normal (0, -sin 30°, cos 30°)). A circle keeps
/// its normal along the plane's (Normal Along); a rectangle's sides run
/// along the plane's x and y with its width and height, its corners in one
/// plane square to the normal (In Frame relations and one Frame Bound); an
/// ellipse, a polygon and a text lie on it; a line along the plane's y with
/// a length, and one with offsets along its x and y (Align Along, Offset
/// Along). Everything solves; saved and read again, the definition and the
/// geometry are the same.
#[test]
fn shapes_on_a_rotated_plane_are_solved_saved_and_read_again() {
    let (s30, c30) = (0.5, 3.0_f64.sqrt() / 2.0);
    let frame = json!({"x": [1, 0, 0], "normal": [0, -s30, c30]});
    let (x, y, n) = ([1.0, 0.0, 0.0], [0.0, c30, s30], [0.0, -s30, c30]);
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let on =
        |uv: [f64; 2], from: [f64; 3]| [0, 1, 2].map(|k| from[k] + uv[0] * x[k] + uv[1] * y[k]);
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let origin = [0.0, 0.0, 10.0];

    // A circle: its normal along the plane's, either way.
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_circle", "sketch": "F1", "center": origin,
               "radius": 8, "plane": frame}),
    );
    let s = sketch(&doc, "F1");
    assert!(s["failure"].is_null(), "{s}");
    let c = item(&s, "curves", &made(&circle));
    let normal = xyz(&c["geometry"]["normal"]);
    assert!((dot(normal, n).abs() - 1.0).abs() <= TOL, "{c}");
    assert!((c["geometry"]["radius"].as_f64().unwrap() - 8.0).abs() <= TOL);
    assert!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["type"] == "normal_along"),
        "{s}"
    );

    // A rectangle 30 by 20 along the plane's x and y.
    let rectangle = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_rectangle", "sketch": "F1", "mode": "two_point",
               "a": on([40.0, 0.0], origin), "b": on([68.0, 21.0], origin),
               "plane": frame, "width": 30, "height": 20}),
    );
    let s = sketch(&doc, "F1");
    assert!(s["failure"].is_null(), "{s}");
    let kinds: Vec<&str> = rectangle["relations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| item(&s, "constraints", k)["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "in_frame").count(),
        4,
        "{kinds:?}"
    );
    assert_eq!(kinds.iter().filter(|k| **k == "frame_bound").count(), 1);
    let corners: Vec<[f64; 3]> = rectangle["corners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| at(&s, p))
        .collect();
    let level = dot(corners[0], n);
    assert!(
        corners.iter().all(|p| (dot(*p, n) - level).abs() <= TOL),
        "{corners:?}"
    );
    let first = sub(corners[1], corners[0]);
    let second = sub(corners[2], corners[1]);
    assert!((dot(first, x) - 30.0).abs() <= TOL, "{corners:?}");
    assert!(dot(first, y).abs() <= TOL && dot(first, n).abs() <= TOL);
    assert!((dot(second, y) - 20.0).abs() <= TOL, "{corners:?}");
    assert!(dot(second, x).abs() <= TOL && dot(second, n).abs() <= TOL);

    // An ellipse, a polygon and a text on it.
    let ellipse = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_ellipse", "sketch": "F1", "center": on([0.0, 40.0], origin),
               "major": on([20.0, 40.0], origin), "minor_radius": 5, "plane": frame,
               "major_value": 20}),
    );
    let polygon = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_polygon", "sketch": "F1", "center": on([0.0, -40.0], origin),
               "vertex": on([0.0, -30.0], origin), "sides": 6, "plane": frame, "radius": 10}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_text", "sketch": "F1", "text": "MITCAD",
               "at": on([-40.0, 0.0], origin), "plane": frame, "height": 6}),
    );
    let s = sketch(&doc, "F1");
    assert!(s["failure"].is_null(), "{s}");
    let e = item(&s, "curves", &made(&ellipse));
    assert!(near(xyz(&e["geometry"]["normal"]), n), "{e}");
    assert!((e["geometry"]["major"].as_f64().unwrap() - 20.0).abs() <= TOL);
    let hexagon: Vec<[f64; 3]> = polygon["corners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| at(&s, p))
        .collect();
    let level = dot(hexagon[0], n);
    assert!(hexagon.iter().all(|p| (dot(*p, n) - level).abs() <= TOL));
    assert_eq!(s["texts"][0]["plane"], "rotated", "{s}");
    assert!(s["texts"][0]["frame"].is_object(), "{s}");

    // Lines along the plane's axes: Keep Axis with a length, offsets.
    let along = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": origin,
               "end": on([1.0, 9.0], origin), "frame": frame, "axis": "y", "length": 12}),
    );
    let offsets = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": along["end"],
               "end": [3, 4, 5], "frame": frame, "offsets": [10, 4, null]}),
    );
    let s = sketch(&doc, "F1");
    assert!(s["failure"].is_null(), "{s}");
    let (b, c) = (at(&s, &along["end"]), at(&s, &offsets["end"]));
    assert!(near(b, on([0.0, 12.0], origin)), "{b:?}");
    assert!((dot(sub(c, b), x) - 10.0).abs() <= TOL, "{c:?}");
    assert!((dot(sub(c, b), y) - 4.0).abs() <= TOL, "{c:?}");
    for kind in ["align_along", "offset_along"] {
        let listed = s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .chain(s["dimensions"].as_array().unwrap())
            .any(|k| k["type"] == kind);
        assert!(listed, "{kind} in {s}");
    }

    // Saved and read again: the same definition and geometry.
    let file = doc.to_json();
    let saved: Value = serde_json::from_str(&file).unwrap();
    let mut back = Document::from_json(&file, MockKernel::default()).unwrap();
    back.recompute();
    let read = sketch(&back, "F1");
    assert!(read["failure"].is_null(), "{read}");
    for list in ["points", "curves", "constraints", "dimensions", "texts"] {
        assert!(same(&read[list], &s[list]), "{list}: {read}");
    }
    let saved_again: Value = serde_json::from_str(&back.to_json()).unwrap();
    assert_eq!(saved_again["features"], saved["features"]);

    // A rotated plane that is no plane is refused, nothing changed.
    let s0 = sketch(&doc, "F1");
    rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_circle", "sketch": "F1", "center": origin, "radius": 3,
               "plane": {"x": [0, 0, 1], "normal": [0, 0, 2]}}),
    );
    assert!(same(&sketch(&doc, "F1"), &s0));
}
