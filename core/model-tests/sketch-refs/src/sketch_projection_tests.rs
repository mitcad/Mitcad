// SPDX-License-Identifier: MIT
//! Project of another sketch's curves and points (mitcad#904), with the
//! mock kernel (sketches need no kernel): a line, an arc, a circle and a
//! point of Sketch1 projected into a later sketch on the same plane (exact
//! copies), on a parallel offset plane and on an oblique plane (along that
//! sketch's normal: the circle an ellipse, the arc an elliptical arc); a
//! dimension change in Sketch1 moves the linked projection, an unlinked
//! one stays; a removed source curve is a lost projection with a warning;
//! a later sketch, the sketch itself and another component's sketch picked
//! outside its occurrence are refused; a whole sketch gives all its
//! curves; as a section, the points where the sketch's plane crosses them
//! (mitcad#998); the design file keeps the links, and a projection of
//! model geometry is stored as before.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::sketch::Projection;
use mitcad_model::test_support::allow_attachments;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

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
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": uid}))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn xy(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    close(a[0], b[0]) && close(a[1], b[1])
}

/// The one curve of a type in the sketch.
fn curve(s: &Value, kind: &str) -> Value {
    let found: Vec<&Value> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .collect();
    assert_eq!(found.len(), 1, "one {kind} in {s}");
    found[0].clone()
}

fn count(s: &Value, kind: &str) -> usize {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .count()
}

fn has_point_at(s: &Value, at: [f64; 2]) -> bool {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "point" && near(xy(&e["at"]), at))
}

/// The new curve (or point) of a sketch edit.
fn made(result: &Value, prefix: &str) -> String {
    result["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        .find(|e| e.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix} in {result}"))
}

/// Sketch1's geometry, on XY.
struct Source {
    line: String,
    arc: String,
    circle: String,
    point: String,
    /// The circle's diameter parameter.
    diameter: String,
}

/// Sketch1 (F1) on XY: a line from (0, 0) to (30, 0), an arc round (50, 0)
/// of radius 10 from (60, 0) to (50, 10), a circle round (0, 40) of
/// diameter 20 (a dimension) and a loose point at (20, 20).
fn source(doc: &mut Document<MockKernel>) -> Source {
    command(doc, json!({"cmd": "sketch.create"}));
    let line = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [30, 0]}),
    );
    let arc = command(
        doc,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": [50, 0],
               "start": [60, 0], "end": [50, 10]}),
    );
    let circle = command(
        doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 40], "diameter": 20}),
    );
    let point = command(
        doc,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [20, 20]}),
    );
    Source {
        line: made(&line, "c"),
        arc: made(&arc, "c"),
        circle: circle["curves"][0].as_str().unwrap().to_owned(),
        point: made(&point, "p"),
        diameter: circle["parameters"][0].as_str().unwrap().to_owned(),
    }
}

/// Projects Sketch1's line, arc, circle and point into `target`.
fn project_all(doc: &mut Document<MockKernel>, target: &str, s: &Source, linked: bool) {
    for (key, id) in [
        ("curve", &s.line),
        ("curve", &s.arc),
        ("curve", &s.circle),
        ("point", &s.point),
    ] {
        let mut source = json!({"sketch": "F1"});
        source[key] = json!(id);
        command(
            doc,
            json!({"cmd": "sketch.project", "sketch": target, "source": source, "linked": linked}),
        );
    }
}

/// Where a point of XY lands in a sketch, projected along its normal: its
/// coordinates in the sketch's frame (the query's `frame`).
fn on(s: &Value, p: [f64; 3]) -> [f64; 2] {
    let frame = &s["frame"];
    let v = |key: &str| -> [f64; 3] { std::array::from_fn(|i| frame[key][i].as_f64().unwrap()) };
    let (origin, x, y) = (v("origin"), v("x_axis"), v("y_axis"));
    let d: [f64; 3] = std::array::from_fn(|i| p[i] - origin[i]);
    let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
    [dot(x), dot(y)]
}

/// The line and the loose point where Sketch1 has them, seen in `s`.
fn assert_line_and_point(s: &Value) {
    let line = curve(s, "line");
    let ends = [xy(&line["geometry"]["start"]), xy(&line["geometry"]["end"])];
    let want = [on(s, [0.0, 0.0, 0.0]), on(s, [30.0, 0.0, 0.0])];
    assert!(
        (near(ends[0], want[0]) && near(ends[1], want[1]))
            || (near(ends[0], want[1]) && near(ends[1], want[0])),
        "line {ends:?}, not {want:?}"
    );
    assert!(has_point_at(s, on(s, [20.0, 20.0, 0.0])), "no point in {s}");
}

/// The arc's ends where Sketch1 has them, seen in `s` (either way round).
fn assert_arc_ends(arc: &Value, s: &Value) {
    let ends = [xy(&arc["geometry"]["start"]), xy(&arc["geometry"]["end"])];
    let want = [on(s, [60.0, 0.0, 0.0]), on(s, [50.0, 10.0, 0.0])];
    assert!(
        (near(ends[0], want[0]) && near(ends[1], want[1]))
            || (near(ends[0], want[1]) && near(ends[1], want[0])),
        "arc ends {ends:?}, not {want:?}"
    );
}

#[test]
fn another_sketchs_curves_and_points_on_the_same_plane_are_exact_copies() {
    let mut doc = Document::new(MockKernel::default());
    let s = source(&mut doc);
    let target = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    project_all(&mut doc, &target, &s, true);
    let projected = sketch(&doc, &target);
    assert_line_and_point(&projected);
    let arc = curve(&projected, "arc");
    assert!(near(xy(&arc["geometry"]["center"]), [50.0, 0.0]), "{arc}");
    assert!(close(arc["geometry"]["radius"].as_f64().unwrap(), 10.0));
    assert_arc_ends(&arc, &projected);
    let circle = curve(&projected, "circle");
    assert!(
        near(xy(&circle["geometry"]["center"]), [0.0, 40.0]),
        "{circle}"
    );
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 10.0));
    // Fixed reference geometry, one linked projection per entity, each
    // naming the sketch and the entity.
    assert!(
        circle["reference"] == true && circle["fixed"] == true,
        "{circle}"
    );
    let projections = projected["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 4, "{projected}");
    assert_eq!(
        projections[2]["source"],
        json!({"sketch": "F1", "curve": s.circle})
    );
    assert_eq!(
        projections[3]["source"],
        json!({"sketch": "F1", "point": s.point})
    );
    let uid = target.parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.warnings(uid), Vec::<String>::new());

    // The circle's diameter changes in Sketch1: the linked circle follows.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": s.diameter, "value": 30}),
    );
    let followed = sketch(&doc, &target);
    let circle = curve(&followed, "circle");
    assert!(
        close(circle["geometry"]["radius"].as_f64().unwrap(), 15.0),
        "{circle}"
    );
    assert!(near(xy(&circle["geometry"]["center"]), [0.0, 40.0]));
    assert_eq!(doc.warnings(uid), Vec::<String>::new());
}

#[test]
fn on_a_parallel_plane_the_shapes_stay() {
    let mut doc = Document::new(MockKernel::default());
    let s = source(&mut doc);
    let plane = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 15}}}),
    )["uid"]
        .clone();
    let target = command(&mut doc, json!({"cmd": "sketch.create", "plane": plane}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    project_all(&mut doc, &target, &s, true);
    let projected = sketch(&doc, &target);
    assert_line_and_point(&projected);
    let arc = curve(&projected, "arc");
    assert!(close(arc["geometry"]["radius"].as_f64().unwrap(), 10.0));
    assert!(near(
        xy(&arc["geometry"]["center"]),
        on(&projected, [50.0, 0.0, 0.0])
    ));
    assert_arc_ends(&arc, &projected);
    let circle = curve(&projected, "circle");
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 10.0));
    assert!(near(
        xy(&circle["geometry"]["center"]),
        on(&projected, [0.0, 40.0, 0.0])
    ));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": s.diameter, "value": 30}),
    );
    let circle = curve(&sketch(&doc, &target), "circle");
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 15.0));
}

#[test]
fn on_an_oblique_plane_they_are_projected_along_its_normal() {
    let mut doc = Document::new(MockKernel::default());
    let s = source(&mut doc);
    // Through the X axis, 45 degrees from XY.
    let angle = std::f64::consts::FRAC_PI_4;
    let plane = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "angle", "line": "x", "angle": angle, "plane": "xy"}}}),
    )["uid"]
        .clone();
    let target = command(&mut doc, json!({"cmd": "sketch.create", "plane": plane}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    project_all(&mut doc, &target, &s, true);
    let projected = sketch(&doc, &target);
    // The line along X keeps its length; the point comes closer.
    assert_line_and_point(&projected);
    // The circle is an ellipse: its diameter along X stays, the one across
    // shrinks by the cosine of the angle.
    assert_eq!(count(&projected, "circle"), 0, "{projected}");
    let ellipse = curve(&projected, "ellipse");
    let g = &ellipse["geometry"];
    assert!(
        close(g["major_radius"].as_f64().unwrap(), 10.0),
        "{ellipse}"
    );
    assert!(
        close(g["minor_radius"].as_f64().unwrap(), 10.0 * angle.cos()),
        "{ellipse}"
    );
    assert!(near(xy(&g["center"]), on(&projected, [0.0, 40.0, 0.0])));
    // The arc an elliptical arc between the projected ends.
    let arc = curve(&projected, "elliptical_arc");
    assert_arc_ends(&arc, &projected);
    // It follows the dimension there too.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": s.diameter, "value": 30}),
    );
    let followed = sketch(&doc, &target);
    let g = curve(&followed, "ellipse")["geometry"].clone();
    assert!(close(g["major_radius"].as_f64().unwrap(), 15.0), "{g}");
    assert!(close(
        g["minor_radius"].as_f64().unwrap(),
        15.0 * angle.cos()
    ));
}

#[test]
fn unlinked_copies_stay_and_a_removed_source_is_a_broken_reference() {
    let mut doc = Document::new(MockKernel::default());
    let s = source(&mut doc);
    let linked = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    project_all(&mut doc, &linked, &s, true);
    let fixed = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    project_all(&mut doc, &fixed, &s, false);
    assert_eq!(sketch(&doc, &fixed)["projections"], json!([]));
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": s.diameter, "value": 30}),
    );
    // Unlinked: a fixed copy keeps its size.
    let circle = curve(&sketch(&doc, &fixed), "circle");
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 10.0));

    // The line removed from Sketch1: its linked projection is lost, keeps
    // its last geometry and the sketch warns, as for a body edge gone.
    command(
        &mut doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [s.line]}),
    );
    let uid = linked.parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let warnings = doc.warnings(uid);
    let line_name = format!("F1/{}", s.line);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains(&line_name) && w.contains("it is gone")),
        "{warnings:?}"
    );
    let after = sketch(&doc, &linked);
    let lost: Vec<&Value> = after["projections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["lost"] == true)
        .collect();
    assert_eq!(lost.len(), 1, "{after}");
    assert_eq!(lost[0]["source"], json!({"sketch": "F1", "curve": s.line}));
    assert_line_and_point(&after);
    // The others still follow.
    let circle = curve(&after, "circle");
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 15.0));
}

#[test]
fn only_earlier_sketches_and_not_the_sketch_itself() {
    let mut doc = Document::new(MockKernel::default());
    source(&mut doc);
    let target = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let later = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [0, 0], "end": [5, 5]}),
    );
    let later_line = made(&later, "c");
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": target,
               "source": {"sketch": "F3", "curve": later_line}, "linked": true}),
    );
    assert!(
        error.contains("comes after") && error.contains("before it"),
        "{error}"
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": "F3",
               "source": {"sketch": "F3", "curve": later_line}, "linked": true}),
    );
    assert!(error.contains("its own geometry"), "{error}");
    // A curve the sketch does not have is refused too.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": target,
               "source": {"sketch": "F1", "curve": "c99"}, "linked": true}),
    );
    assert!(error.contains("has no c99"), "{error}");
    // Nothing was added to the target.
    assert_eq!(sketch(&doc, &target)["entities"], json!([]));
}

#[test]
fn a_whole_sketch_gives_all_its_curves() {
    let mut doc = Document::new(MockKernel::default());
    let s = source(&mut doc);
    let target = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": target, "source": {"sketch": "F1"},
               "linked": true}),
    );
    let projected = sketch(&doc, &target);
    assert_eq!(count(&projected, "line"), 1, "{projected}");
    assert_eq!(count(&projected, "arc"), 1);
    assert_eq!(count(&projected, "circle"), 1);
    let sources: Vec<Value> = projected["projections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["source"].clone())
        .collect();
    for id in [&s.line, &s.arc, &s.circle] {
        assert!(
            sources.contains(&json!({"sketch": "F1", "curve": id})),
            "{sources:?}"
        );
    }
    // The loose point is no curve.
    assert!(!sources.contains(&json!({"sketch": "F1", "point": s.point})));
}

#[test]
fn a_section_of_another_sketchs_curves_gives_where_its_plane_crosses_them() {
    // mitcad#998: Sketch1's circle round (0, 40) of radius 10 crosses the
    // YZ plane at (0, 30) and (0, 50), its line from (0, 0) touches it at
    // its start; the arc round (50, 0) misses it.
    let mut doc = Document::new(MockKernel::default());
    let s = source(&mut doc);
    let target = command(&mut doc, json!({"cmd": "sketch.create", "plane": "yz"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    for curve in [&s.circle, &s.line] {
        command(
            &mut doc,
            json!({"cmd": "sketch.project", "sketch": target,
                   "source": {"sketch": "F1", "curve": curve}, "linked": true,
                   "section": true}),
        );
    }
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": target,
               "source": {"sketch": "F1", "curve": s.arc}, "section": true}),
    );
    assert!(error.contains("does not cross"), "{error}");
    let cut = sketch(&doc, &target);
    // Points only, each where the plane crosses its curve.
    assert_eq!(count(&cut, "point"), 3, "{cut}");
    assert_eq!(cut["entities"].as_array().unwrap().len(), 3, "{cut}");
    for at in [[0.0, 30.0, 0.0], [0.0, 50.0, 0.0], [0.0, 0.0, 0.0]] {
        assert!(
            has_point_at(&cut, on(&cut, at)),
            "no point at {at:?} in {cut}"
        );
    }
    let projections = cut["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 2, "{cut}");
    assert_eq!(projections[0]["section"], true, "{cut}");
    assert_eq!(
        projections[0]["source"],
        json!({"sketch": "F1", "curve": s.circle})
    );
    let uid = target.parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.warnings(uid), Vec::<String>::new());

    // The circle grows to radius 15: its crossings follow.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": s.diameter, "value": 30}),
    );
    let followed = sketch(&doc, &target);
    for at in [[0.0, 25.0, 0.0], [0.0, 55.0, 0.0], [0.0, 0.0, 0.0]] {
        assert!(
            has_point_at(&followed, on(&followed, at)),
            "no point at {at:?} in {followed}"
        );
    }
    assert_eq!(doc.warnings(uid), Vec::<String>::new());
}

#[test]
fn the_design_file_keeps_the_links() {
    let mut doc = Document::new(MockKernel::default());
    let s = source(&mut doc);
    let target = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    project_all(&mut doc, &target, &s, true);
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    loaded.recompute();
    let reopened = sketch(&loaded, &target);
    assert_eq!(
        reopened["projections"],
        sketch(&doc, &target)["projections"]
    );
    assert_eq!(
        reopened["projections"][0]["source"],
        json!({"sketch": "F1", "curve": s.line})
    );
    // Still linked after the reopen.
    command(
        &mut loaded,
        json!({"cmd": "set_parameter", "name": s.diameter, "value": 30}),
    );
    let circle = curve(&sketch(&loaded, &target), "circle");
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 15.0));
}

#[test]
fn a_projection_of_model_geometry_is_stored_as_before() {
    // Files of earlier versions: the source is the name's text, read and
    // written unchanged.
    let stored = json!({"source": "F2:end(r{c1})", "entities": ["c3"], "by_geometry": true});
    let projection: Projection = serde_json::from_value(stored.clone()).unwrap();
    assert!(projection.source.topo().is_some());
    assert_eq!(serde_json::to_value(&projection).unwrap(), stored);
    // A sketch's entity in its object form.
    let stored = json!({"source": {"sketch": "F1", "point": "p4"}, "entities": ["p7"]});
    let projection: Projection = serde_json::from_value(stored.clone()).unwrap();
    assert_eq!(projection.source.to_string(), "F1/p4");
    assert_eq!(serde_json::to_value(&projection).unwrap(), stored);
    let wrong =
        json!({"source": {"sketch": "F1", "curve": "c1", "point": "p2"}, "entities": ["c3"]});
    assert!(serde_json::from_value::<Projection>(wrong).is_err());
}

#[test]
fn another_components_sketch_comes_through_its_occurrence() {
    // A:1 (O1) at (100, 50, 10) holds a sketch with a line from (0, 0) to
    // (10, 0); a sketch on the root's XY projects it where the occurrence
    // places it.
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("A"), Transform::translation([100.0, 50.0, 10.0]), true)
        .unwrap();
    let first = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"].clone();
    let line = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": first, "start": [0, 0], "end": [10, 0]}),
    );
    let line = made(&line, "c");
    doc.activate_component(ComponentUid::ROOT).unwrap();
    allow_attachments(&mut doc);
    let target = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    // Not without the occurrence that shows it.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": target,
               "source": {"sketch": first, "curve": line}, "linked": true}),
    );
    assert!(error.contains("pick it where an occurrence"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": target,
               "source": {"sketch": first, "curve": line}, "linked": true, "occurrence": "O1"}),
    );
    let projected = sketch(&doc, &target);
    let g = curve(&projected, "line")["geometry"].clone();
    let ends = [xy(&g["start"]), xy(&g["end"])];
    assert!(
        (near(ends[0], [100.0, 50.0]) && near(ends[1], [110.0, 50.0]))
            || (near(ends[1], [100.0, 50.0]) && near(ends[0], [110.0, 50.0])),
        "{g}"
    );
    assert!(
        projected["projections"][0]["link"].is_object(),
        "{projected}"
    );
    let uid = target.parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
}
