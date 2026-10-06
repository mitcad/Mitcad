// SPDX-License-Identifier: MIT
//! Project files: version 2 round trips and errors, version 1 conversion.

use serde_json::{Value, json};

use super::FileError;
use crate::document_tests::{Block, block, corner, def, extrude, num, side, top_edge};
use crate::expr::LengthUnit;
use crate::features::SketchPlane;
use crate::ids::{BodyUid, FeatureUid};
use crate::testing::MockKernel;
use crate::topo::EdgeName;
use crate::{Document, FeatureStatus};

fn load(json: &str) -> Result<Document<MockKernel>, FileError> {
    Document::from_json(json, MockKernel::default())
}

fn rejected(json: &str) -> FileError {
    match load(json) {
        Ok(_) => panic!("the file must be rejected:\n{json}"),
        Err(error) => error,
    }
}

fn edited(source: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(source).unwrap();
    edit(&mut value);
    value.to_string()
}

fn assert_message(error: FileError, expected: &[&str]) {
    let message = error.to_string();
    for text in expected {
        assert!(message.contains(text), "'{text}' not in: {message}");
    }
}

fn assert_invalid(error: FileError, expected: &[&str]) {
    assert!(matches!(error, FileError::Invalid(_)), "{error:?}");
    assert_message(error, expected);
}

fn histories(doc: &Document<MockKernel>) -> Vec<(String, String, String)> {
    doc.bodies()
        .into_iter()
        .map(|b| (b.uid.to_string(), b.name, b.shape.history.clone()))
        .collect()
}

/// A block with a boss joined on, a hole cut through, a fillet on edges of
/// all three extrudes, a suppressed chamfer, a renamed body, the marker
/// before the last feature and a value without a short decimal form.
fn part() -> Block {
    let mut b = block(); // F1, F2
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid; // F3
    let boss = b
        .doc
        .add_rectangle(sketch, [10.0, 10.0], &num(20.0), &num(10.0))
        .unwrap()
        .region;
    let circle = b.doc.add_circle(sketch, [45.0, 20.0], &num(10.0)).unwrap();
    let hole = circle.region;
    let hole_curve = crate::topo::SegmentKey::closed(circle.curves[0]);
    let join = extrude(sketch, &[&boss], 30.0, "join", &[b.body]);
    let join = b.doc.add_feature(&join, None).unwrap().uid; // F4
    let cut = extrude(sketch, &[&hole], 30.0, "cut", &[b.body]);
    let cut = b.doc.add_feature(&cut, None).unwrap().uid; // F5
    let edges = [
        corner(b.extrude, 1, 1),
        corner(join, 1, 1),
        EdgeName::new(
            crate::topo::FaceName::side(cut, hole_curve),
            crate::topo::FaceName::start(cut, hole.clone()),
        ),
    ];
    let fillet = def(json!({"type": "fillet", "body": b.body, "edges": edges, "radius": 2.0}));
    b.doc.add_feature(&fillet, None).unwrap(); // F6
    let chamfer = def(json!({"type": "chamfer", "body": b.body,
                             "edges": [top_edge(b.extrude, 1, &b.region, 0)],
                             "size": {"type": "equal_distance", "distance": 1.0}}));
    let chamfer = b.doc.add_feature(&chamfer, None).unwrap().uid; // F7
    b.doc.set_suppressed(chamfer, true).unwrap();
    b.doc.add_sketch(SketchPlane::Xy).unwrap(); // F8
    b.doc.set_marker(7).unwrap();
    b.doc.rename_body(b.body, "Base").unwrap();
    b.doc.set_parameter("d1", 65.5).unwrap();
    b.doc.set_parameter("d3", 1.0 / 3.0).unwrap();
    b
}

#[test]
fn round_trip_keeps_the_definition() {
    let original = part();
    let json = original.doc.to_json();
    let mut loaded = load(&json).unwrap();
    assert_eq!(loaded.state(), original.doc.state());
    assert_eq!(loaded.to_json(), json, "saving again gives the same file");
    let stats = loaded.recompute();
    assert_eq!(
        stats.evaluated.len(),
        6,
        "the features before the marker but the suppressed one"
    );
    assert!(stats.error.is_none(), "{:?}", stats.error);
    assert_eq!(histories(&loaded), histories(&original.doc));
    assert_eq!(
        loaded.status(FeatureUid(7)),
        Some(&FeatureStatus::Suppressed)
    );
    assert_eq!(
        loaded.status(FeatureUid(8)),
        Some(&FeatureStatus::RolledBack)
    );
    // New items continue after the loaded ones.
    let sketch = loaded.add_sketch(SketchPlane::Xy).unwrap();
    assert_eq!(
        (sketch.uid, sketch.name.as_str()),
        (FeatureUid(9), "Sketch4")
    );
}

#[test]
fn saved_file_is_readable_json_with_named_references() {
    let json = part().doc.to_json();
    assert!(json.ends_with("}\n"));
    assert!(
        json.contains("\n  \"version\": 2,\n"),
        "pretty-printed:\n{json}"
    );
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["format"], "mitcad");
    assert_eq!(value["units"], json!({"length": "mm", "angle": "deg"}));
    assert_eq!(
        value["parameters"][0],
        json!({"name": "d1", "expression": "65.5 mm", "unit": "mm", "value": 65.5,
               "comment": "Sketch1 width", "owner": "F1"})
    );
    assert_eq!(value["parameters"][2]["value"], json!(1.0 / 3.0));
    assert_eq!(
        value["features"][0],
        json!({"uid": "F1", "name": "Sketch1", "type": "sketch", "plane": "xy",
            "entities": [
                {"id": "c1", "type": "line", "start": "p5", "end": "p6"},
                {"id": "c2", "type": "line", "start": "p6", "end": "p7"},
                {"id": "c3", "type": "line", "start": "p7", "end": "p8"},
                {"id": "c4", "type": "line", "start": "p8", "end": "p5"},
                {"id": "p5", "type": "point", "at": [0.0, 0.0], "fixed": true},
                {"id": "p6", "type": "point", "at": [60.0, 0.0]},
                {"id": "p7", "type": "point", "at": [60.0, 40.0]},
                {"id": "p8", "type": "point", "at": [0.0, 40.0]}],
            "constraints": [
                {"id": "k1", "type": "horizontal", "line": "c1"},
                {"id": "k2", "type": "vertical", "line": "c2"},
                {"id": "k3", "type": "horizontal", "line": "c3"},
                {"id": "k4", "type": "vertical", "line": "c4"}],
            "dimensions": [
                {"id": "k5", "type": "length", "line": "c1", "value": "d1"},
                {"id": "k6", "type": "length", "line": "c2", "value": "d2"}]})
    );
    assert_eq!(
        value["features"][3],
        json!({"uid": "F4", "name": "Extrude2", "type": "extrude",
               "profiles": [{"sketch": "F3", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": "d7"}, "operation": "join",
               "participants": ["F2.b0"]})
    );
    assert_eq!(
        value["features"][5]["edges"][0],
        "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"
    );
    assert_eq!(value["features"][6]["suppressed"], true);
    assert!(value["features"][5].get("suppressed").is_none());
    assert_eq!(value["bodies"], json!([{"uid": "F2.b0", "name": "Base"}]));
    assert_eq!(value["marker"], 7);
    // At the end, the marker is left out.
    let mut b = block();
    b.doc.set_marker(2).unwrap();
    assert!(
        serde_json::from_str::<Value>(&b.doc.to_json())
            .unwrap()
            .get("marker")
            .is_none()
    );
}

#[test]
fn a_file_can_be_written_with_another_marker() {
    let mut b = part(); // eight features, the marker at 7
    let revision = b.doc.revision();
    let marker = |json: &str| serde_json::from_str::<Value>(json).unwrap()["marker"].clone();
    assert_eq!(b.doc.to_json_with_marker(7), b.doc.to_json());
    assert_eq!(marker(&b.doc.to_json_with_marker(3)), 3);
    // At or past the end, the marker is left out as in a file saved there.
    assert_eq!(marker(&b.doc.to_json_with_marker(8)), Value::Null);
    assert_eq!(marker(&b.doc.to_json_with_marker(100)), Value::Null);
    // Only the marker differs; the document is as it was.
    let mut file: Value = serde_json::from_str(&b.doc.to_json_with_marker(3)).unwrap();
    file["marker"] = json!(7);
    assert_eq!(
        file,
        serde_json::from_str::<Value>(&b.doc.to_json()).unwrap()
    );
    assert_eq!((b.doc.marker(), b.doc.revision()), (7, revision));
    // An edit rolled back to the third feature writes the marker it
    // restores.
    b.doc.set_marker(3).unwrap();
    let restored = load(&b.doc.to_json_with_marker(7)).unwrap();
    assert_eq!(restored.marker(), 7);
    assert_eq!(restored.to_json(), part().doc.to_json());
}

/// The example of the module documentation.
const EXAMPLE: &str = r#"{
  "format": "mitcad",
  "version": 2,
  "units": { "length": "mm", "angle": "deg" },
  "parameters": [
    { "name": "d1", "expression": "60 mm", "unit": "mm", "value": 60.0,
      "comment": "Sketch1 width", "owner": "F1" },
    { "name": "d2", "expression": "40 mm", "unit": "mm", "value": 40.0,
      "comment": "Sketch1 height", "owner": "F1" },
    { "name": "d3", "expression": "20 mm", "unit": "mm", "value": 20.0,
      "comment": "Extrude1 distance", "owner": "F2" },
    { "name": "d4", "expression": "2 mm", "unit": "mm", "value": 2.0,
      "comment": "Fillet1 radius", "owner": "F3" }
  ],
  "features": [
    { "uid": "F1", "name": "Sketch1", "type": "sketch", "plane": "xy",
      "entities": [
        { "id": "c1", "type": "line", "start": "p5", "end": "p6" },
        { "id": "c2", "type": "line", "start": "p6", "end": "p7" },
        { "id": "c3", "type": "line", "start": "p7", "end": "p8" },
        { "id": "c4", "type": "line", "start": "p8", "end": "p5" },
        { "id": "p5", "type": "point", "at": [0.0, 0.0], "fixed": true },
        { "id": "p6", "type": "point", "at": [60.0, 0.0] },
        { "id": "p7", "type": "point", "at": [60.0, 40.0] },
        { "id": "p8", "type": "point", "at": [0.0, 40.0] } ],
      "constraints": [
        { "id": "k1", "type": "horizontal", "line": "c1" },
        { "id": "k2", "type": "vertical", "line": "c2" },
        { "id": "k3", "type": "horizontal", "line": "c3" },
        { "id": "k4", "type": "vertical", "line": "c4" } ],
      "dimensions": [
        { "id": "k5", "type": "length", "line": "c1", "value": "d1" },
        { "id": "k6", "type": "length", "line": "c2", "value": "d2" } ] },
    { "uid": "F2", "name": "Extrude1", "type": "extrude",
      "profiles": [ { "sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}" } ],
      "extent": { "type": "distance", "distance": "d3" }, "operation": "new_body" },
    { "uid": "F3", "name": "Fillet1", "type": "fillet", "body": "F2.b0",
      "edges": [ "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}" ], "radius": "d4" }
  ],
  "bodies": [ { "uid": "F2.b0", "name": "Body1" } ]
}"#;

/// The same part as written before expressions and the general sketch:
/// parameter values and the sketch's rectangle.
const EXAMPLE_SHAPES: &str = r#"{
  "format": "mitcad",
  "version": 2,
  "parameters": [
    { "name": "d1", "value": 60.0, "comment": "Sketch1 width", "owner": "F1" },
    { "name": "d2", "value": 40.0, "comment": "Sketch1 height", "owner": "F1" },
    { "name": "d3", "value": 20.0, "comment": "Extrude1 distance", "owner": "F2" },
    { "name": "d4", "value": 2.0, "comment": "Fillet1 radius", "owner": "F3" }
  ],
  "features": [
    { "uid": "F1", "name": "Sketch1", "type": "sketch", "plane": "xy", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2",
        "curves": ["c1", "c2", "c3", "c4"] } ] },
    { "uid": "F2", "name": "Extrude1", "type": "extrude",
      "profiles": [ { "sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}" } ],
      "extent": { "type": "distance", "distance": "d3" }, "operation": "new_body" },
    { "uid": "F3", "name": "Fillet1", "type": "fillet", "body": "F2.b0",
      "edges": [ "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}" ], "radius": "d4" }
  ],
  "bodies": [ { "uid": "F2.b0", "name": "Body1" } ]
}"#;

#[test]
fn documented_example_loads_and_recomputes() {
    let mut doc = load(EXAMPLE).unwrap();
    assert!(
        doc.bodies().is_empty(),
        "nothing is evaluated before recompute"
    );
    let stats = doc.recompute();
    assert!(stats.error.is_none());
    assert_eq!(
        histories(&doc),
        vec![(
            "F2.b0".to_owned(),
            "Body1".to_owned(),
            "fillet(prism(F2:0..20),2)".to_owned()
        )]
    );
    // The same as the commands build.
    let mut b = block();
    b.doc
        .add_feature(
            &def(json!({"type": "fillet", "body": "F2.b0", "edges": [corner(b.extrude, 1, 1)], "radius": 2.0})),
            None,
        )
        .unwrap();
    assert_eq!(doc.to_json(), b.doc.to_json());
    // Files before expressions and entities load to the same part.
    let older = load(EXAMPLE_SHAPES).unwrap();
    assert_eq!(older.to_json(), doc.to_json());
}

#[test]
fn byte_order_marks_and_optional_fields_are_accepted() {
    let json = edited(EXAMPLE, |v| {
        v["parameters"][0]
            .as_object_mut()
            .unwrap()
            .remove("comment");
        v["parameters"][0].as_object_mut().unwrap().remove("owner");
        v.as_object_mut().unwrap().remove("bodies");
        v["features"][0].as_object_mut().unwrap().remove("plane");
    });
    let mut doc = load(&format!("\u{feff}{json}")).unwrap();
    assert_eq!(doc.parameters().iter().next().unwrap().comment(), "");
    doc.recompute();
    assert_eq!(
        doc.bodies()[0].name,
        "Body1",
        "unnamed bodies get default names"
    );
}

#[test]
fn expressions_and_units_round_trip() {
    let mut b = block();
    b.doc.set_units(LengthUnit::Inch).unwrap();
    b.doc
        .add_parameter_expression("wall", "d1 / 20", None, "user")
        .unwrap();
    b.doc
        .set_parameter_expression("d3", "wall * 4", None)
        .unwrap();
    let json = b.doc.to_json();
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["units"], json!({"length": "in", "angle": "deg"}));
    assert_eq!(
        value["parameters"][2],
        json!({"name": "d3", "expression": "wall * 4", "unit": "mm", "value": 12.0,
               "comment": "Extrude1 distance", "owner": "F2"})
    );
    assert_eq!(
        value["parameters"][3],
        json!({"name": "wall", "expression": "d1 / 20", "unit": "in", "value": 3.0,
               "comment": "user"})
    );
    let mut loaded = load(&json).unwrap();
    assert_eq!(loaded.state(), b.doc.state());
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert_eq!(histories(&loaded), histories(&b.doc));

    // Errors name the parameter.
    let edit = |p: Value| {
        rejected(&edited(&json, |v| {
            v["parameters"][3] = p;
        }))
    };
    assert_invalid(
        edit(json!({"name": "wall", "expression": "zz * 2", "unit": "mm"})),
        &["parameters[3]: parameter 'wall': unknown parameter 'zz' at byte 0"],
    );
    assert_invalid(
        edit(json!({"name": "wall", "expression": "d3", "unit": "mm"})),
        &["parameters: circular reference: d3 -> wall -> d3"],
    );
    assert_invalid(
        edit(json!({"name": "wall", "expression": "2", "unit": "furlong"})),
        &["parameters[3]: unit: "],
    );
    assert_invalid(
        edit(json!({"name": "mm", "expression": "2", "unit": "mm"})),
        &["parameters[3]: 'mm' is a unit, function or constant"],
    );
    let error = rejected(&edited(&json, |v| v["units"]["length"] = json!("deg")));
    assert_invalid(error, &["units: 'deg' is not a length unit"]);
}

#[test]
fn expressions_are_saved_with_decimal_points() {
    let mut b = block();
    b.doc
        .add_parameter_expression("wall", "d1 * 0,25 + max(d2, 1,5)", None, "")
        .unwrap();
    b.doc.set_parameter_expression("d3", "12,5", None).unwrap();
    let json = b.doc.to_json();
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["parameters"][2]["expression"], json!("12.5"));
    assert_eq!(
        value["parameters"][3]["expression"],
        json!("d1 * 0.25 + max(d2, 1.5)")
    );
    // A file written with decimal commas loads with points.
    let commas = edited(&json, |v| {
        v["parameters"][3]["expression"] = json!("d1 * 0,25 + max(d2, 1,5)");
    });
    assert_eq!(load(&commas).unwrap().to_json(), json);
}

/// Every part of the general sketch survives saving and loading: entities
/// of each type with their flags, constraints, driving dimensions with
/// expressions and driven ones, a frame, a face plane, a linked projection
/// and text.
#[test]
fn general_sketch_round_trips() {
    use crate::kernel::Curve3;
    let run = |doc: &mut Document<MockKernel>, command: Value| {
        doc.command(&command.to_string())
            .unwrap_or_else(|e| panic!("{command}: {e}"));
    };
    let face = "F3:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    let model = |doc: &Document<MockKernel>| {
        *doc.kernel().curves.borrow_mut() = vec![
            Curve3::Line {
                start: [0.0, 0.0, 20.0],
                end: [10.0, 0.0, 20.0],
            },
            Curve3::Point([5.0, 5.0, 20.0]),
        ];
    };
    let mut doc = Document::new(MockKernel::default());
    model(&doc);
    // F1: on XZ with its own frame.
    run(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": "xz",
               "frame": {"origin": [1, 2, 3], "x_axis": [0, 1, 0], "y_axis": [-1, 0, 0]}}),
    );
    let s = |command: Value| {
        let mut command = command;
        command["sketch"] = json!("F1");
        command
    };
    for command in [
        json!({"cmd": "sketch.add_rectangle", "corner": [0, 0], "width": 60, "height": 40}),
        json!({"cmd": "sketch.fillet", "a": "c1", "b": "c2", "radius": "d1 / 10"}),
        json!({"cmd": "sketch.circle", "mode": "center", "center": [20, 20], "radius": 5}),
        json!({"cmd": "sketch.add_dimension", "dimension": {"type": "diameter", "curve": "c13"},
               "driven": true, "text": [30, 30]}),
        json!({"cmd": "sketch.ellipse", "center": [45, 15], "major": [50, 15], "minor_radius": 2}),
        json!({"cmd": "sketch.spline", "points": [[5, 30], [10, 35], [15, 30]]}),
        json!({"cmd": "sketch.spline", "points": [[25, 30], [30, 35], [35, 30]], "degree": 2}),
        json!({"cmd": "sketch.add_line", "start": [70, 0], "end": [70, 40], "centerline": true}),
        json!({"cmd": "sketch.add_line", "start": [80, 0], "end": [90, 40], "construction": true}),
        json!({"cmd": "sketch.arc", "mode": "center", "center": [100, 0], "start": [110, 0],
               "end": [100, 10]}),
        json!({"cmd": "sketch.add_text", "text": "Mitcad", "at": [0, -10], "height": 5,
               "angle": 0.5, "font": "Arial", "bold": true}),
    ] {
        run(&mut doc, s(command));
    }
    // F2, F3: a block; F4 on its top face with a linked projection.
    run(&mut doc, json!({"cmd": "sketch.create"}));
    run(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [0, 0], "width": 10, "height": 10}),
    );
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F2", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    run(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": "F3.b0"}}),
    );
    run(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": "F4", "source": face, "linked": true}),
    );
    let sketches = |doc: &Document<MockKernel>| -> Vec<Value> {
        ["F1", "F4"]
            .iter()
            .map(|uid| {
                let query = json!({"query": "sketch", "uid": uid}).to_string();
                serde_json::from_str(&doc.query(&query).unwrap()).unwrap()
            })
            .collect()
    };
    let before = sketches(&doc);
    assert!(before.iter().all(|s| s["solved"] == true), "{before:#?}");
    let json = doc.to_json();
    let value: Value = serde_json::from_str(&json).unwrap();
    let f1 = &value["features"][0];
    assert_eq!(f1["frame"]["origin"], json!([1.0, 2.0, 3.0]));
    assert_eq!(f1["texts"][0]["font"], "Arial");
    let types: Vec<&str> = f1["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap())
        .collect();
    for t in "line point arc circle ellipse fitted_spline spline".split(' ') {
        assert!(types.contains(&t), "{t} in {types:?}");
    }
    assert!(
        f1["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["driven"] == true)
    );
    assert_eq!(value["features"][3]["plane"]["body"], "F3.b0");
    assert_eq!(value["features"][3]["projections"][0]["source"], face);

    let mut loaded = load(&json).unwrap();
    assert_eq!(loaded.state(), doc.state());
    assert_eq!(loaded.to_json(), json, "saving again gives the same file");
    model(&loaded);
    loaded.recompute();
    assert_eq!(sketches(&loaded), before);
}

#[test]
fn values_of_older_files_become_expressions() {
    // Before expressions, files stored values; names that are now units,
    // functions or constants are renamed.
    let json = edited(EXAMPLE_SHAPES, |v| {
        v["parameters"][2]["name"] = json!("m");
        v["features"][1]["extent"]["distance"] = json!("m");
    });
    let mut doc = load(&json).unwrap();
    assert_eq!(
        doc.load_warnings(),
        ["parameter 'm' is renamed to 'm_1': 'm' is a unit, function or constant in expressions"]
    );
    let params = doc.parameters();
    let m = params.get(params.find("m_1").unwrap()).unwrap();
    assert_eq!((m.expression(), m.unit()), ("20 mm", crate::expr::Unit::MM));
    assert!(
        doc.report(false)
            .unwrap()
            .starts_with("Warning: parameter 'm'")
    );
    assert!(doc.recompute().error.is_none());
    assert_eq!(histories(&doc)[0].2, "fillet(prism(F2:0..20),2)");
    // Angles of chamfers were radians; they become degrees.
    let json = edited(EXAMPLE_SHAPES, |v| {
        v["parameters"][3]["value"] = json!(30.0_f64.to_radians());
        v["features"][2] = json!({"uid": "F3", "name": "Chamfer1", "type": "chamfer",
            "body": "F2.b0", "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"],
            "size": {"type": "distance_angle", "distance": "d1", "angle": "d4"}});
    });
    let doc = load(&json).unwrap();
    let params = doc.parameters();
    let angle = params.get(params.find("d4").unwrap()).unwrap();
    assert_eq!(angle.expression(), "30 deg");
    // The same in version 1 files.
    let json = edited(EXAMPLE_V1, |v| {
        v["parameters"][8]["name"] = json!("E");
        v["features"][5]["radius"] = json!("E");
    });
    let doc = load(&json).unwrap();
    assert_eq!(doc.load_warnings().len(), 1);
    assert!(doc.parameters().find("E_1").is_some());
}

#[test]
fn invalid_values_load_and_fail_at_recompute() {
    let mut doc = load(&edited(EXAMPLE, |v| {
        v["parameters"][0]["expression"] = json!("0 mm")
    }))
    .unwrap();
    let error = doc.recompute().error.unwrap();
    assert_eq!(error.name, "Sketch1");
    assert_eq!(error.message, "d1 must be greater than zero, got 0");
    // A profile the sketch has nothing like loads and fails at recompute.
    let mut doc = load(&edited(EXAMPLE, |v| {
        v["features"][1]["profiles"][0]["region"] = json!("r{c7}")
    }))
    .unwrap();
    let error = doc.recompute().error.unwrap();
    assert_eq!(error.to_string(), "Extrude1: Sketch1 has no profile r{c7}");
}

#[test]
fn malformed_files_are_rejected() {
    for text in [
        "",
        "{",
        "{\"format\": \"mitcad\", \"version\": 2,}",
        "[".repeat(100_000).as_str(),
    ] {
        let error = rejected(text);
        assert!(
            matches!(error, FileError::Syntax(_)),
            "{text:.20}: {error:?}"
        );
        assert!(error.to_string().starts_with("not valid JSON: "));
    }
    for text in ["[]", "42", "{}", r#"{"format": "other", "version": 2}"#] {
        assert_eq!(rejected(text), FileError::NotAProject, "{text}");
    }
    let version = |v: Value| rejected(&edited(EXAMPLE, |value| value["version"] = v));
    let FileError::Version(message) = version(json!(4)) else {
        panic!("version 4 must be a version error");
    };
    assert!(
        message.contains("version 4, newer than this Mitcad can read (version 3)"),
        "{message}"
    );
    for v in [json!(0), json!("2"), json!(2.5), json!(-1), Value::Null] {
        assert_eq!(
            version(v.clone()),
            FileError::Version(format!("unsupported project file version {v}"))
        );
    }
}

#[test]
fn schema_errors_say_where() {
    type Edit = fn(&mut Value);
    let cases: [(Edit, &str); 10] = [
        (|v| v["extra"] = json!(1), "unknown field `extra`"),
        (
            |v| {
                v.as_object_mut().unwrap().remove("features");
            },
            "missing field `features`",
        ),
        (
            |v| v["parameters"][1]["value"] = json!("40"),
            "parameters[1]: invalid type: string \"40\", expected f64",
        ),
        (
            |v| v["features"][2]["radus"] = json!("d4"),
            "features[2] (Fillet1): unknown field `radus`",
        ),
        (
            |v| v["features"][1]["type"] = json!("warp"),
            "features[1] (Extrude1): unknown variant `warp`",
        ),
        (
            |v| v["features"][1]["uid"] = json!("2"),
            "features[1] (Extrude1): invalid feature id '2'",
        ),
        (
            |v| {
                v["features"][0].as_object_mut().unwrap().remove("uid");
            },
            "features[0]: missing field `uid`",
        ),
        (
            |v| v["features"][2]["edges"][0] = json!("E{F2:side(c1)}"),
            "features[2] (Fillet1): invalid name 'E{F2:side(c1)}'",
        ),
        (
            |v| v["features"][2]["suppressed"] = json!("yes"),
            "\"suppressed\" must be true or false",
        ),
        (
            |v| v["features"][1]["extent"] = json!({"type": "distance"}),
            "features[1] (Extrude1): missing field `distance`",
        ),
    ];
    for (edit, expected) in cases {
        let error = rejected(&edited(EXAMPLE, edit));
        assert!(
            matches!(error, FileError::Schema(_)),
            "{expected}: {error:?}"
        );
        assert_message(error, &["invalid project file: ", expected]);
    }
}

#[test]
fn references_and_names_are_checked() {
    type Edit = fn(&mut Value);
    let cases: [(Edit, &str); 13] = [
        (
            |v| v["features"][2]["radius"] = json!("d99"),
            "features[2] (Fillet1): radius: parameter 'd99' does not exist",
        ),
        (
            |v| v["features"][0]["dimensions"][1]["value"] = json!("h"),
            "features[0] (Sketch1): dimensions[k6].length: parameter 'h' does not exist",
        ),
        (
            |v| v["features"][0]["entities"][0]["end"] = json!("p9"),
            "features[0] (Sketch1): c1: point p9 does not exist",
        ),
        (
            |v| v["features"][0]["constraints"][0]["line"] = json!("c9"),
            "features[0] (Sketch1): k1: c9 does not exist",
        ),
        (
            |v| v["features"][0]["dimensions"][0]["id"] = json!("k1"),
            "features[0] (Sketch1): dimension k1 is used more than once",
        ),
        (
            |v| v["features"][1]["profiles"][0]["sketch"] = json!("F9"),
            "features[1] (Extrude1): feature F9 does not exist",
        ),
        (
            |v| v["features"][1]["profiles"][0]["sketch"] = json!("F3"),
            "features[1] (Extrude1): Fillet1 (F3) does not come before this feature",
        ),
        (
            |v| v["features"][2]["body"] = json!("F1.b0"),
            "features[2] (Fillet1): Sketch1 (F1) does not create bodies",
        ),
        (
            |v| v["features"][1]["uid"] = json!("F1"),
            "features[1] (Extrude1): uid F1 is used more than once",
        ),
        (
            |v| v["features"][1]["name"] = json!("Sketch1"),
            "features[1]: feature name 'Sketch1' is used more than once",
        ),
        (
            |v| v["parameters"][1]["name"] = json!("d1"),
            "parameters[1]: parameter 'd1' already exists",
        ),
        (
            |v| v["parameters"][1]["owner"] = json!("F9"),
            "parameters[1]: the owner F9 is not a feature of the file",
        ),
        (
            |v| v["marker"] = json!(4),
            "the marker 4 is past the end of the timeline (3 features)",
        ),
    ];
    for (edit, expected) in cases {
        assert_invalid(rejected(&edited(EXAMPLE, edit)), &[expected]);
    }
    let error = rejected(&edited(EXAMPLE, |v| v["bodies"][0]["uid"] = json!("F7.b0")));
    assert_invalid(error, &["bodies[0]: feature F7 does not exist"]);
}

// Version 1.

/// The version 1 example of V0's documentation: a block with a boss joined
/// onto it, a hole cut through it and two rounded edges.
const EXAMPLE_V1: &str = r#"{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0, "comment": "Sketch1 width" },
    { "name": "d2", "value": 40.0, "comment": "Sketch1 height" },
    { "name": "d3", "value": 20.0, "comment": "Extrude1 distance" },
    { "name": "d4", "value": 20.0, "comment": "Sketch2 width" },
    { "name": "d5", "value": 10.0, "comment": "Sketch2 height" },
    { "name": "d6", "value": 10.0, "comment": "Sketch2 diameter" },
    { "name": "d7", "value": 30.0, "comment": "Extrude2 distance" },
    { "name": "d8", "value": 30.0, "comment": "Extrude3 distance" },
    { "name": "d9", "value": 2.0, "comment": "Fillet1 radius" }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" }
    ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" },
    { "type": "sketch", "name": "Sketch2", "shapes": [
      { "type": "rectangle", "corner": [10.0, 10.0], "width": "d4", "height": "d5" },
      { "type": "circle", "center": [45.0, 20.0], "diameter": "d6" }
    ] },
    { "type": "extrude", "name": "Extrude2", "sketch": "Sketch2", "profile": 0,
      "distance": "d7", "operation": "join", "body": "Extrude1" },
    { "type": "extrude", "name": "Extrude3", "sketch": "Sketch2", "profile": 1,
      "distance": "d8", "operation": "cut", "body": "Extrude1" },
    { "type": "fillet", "name": "Fillet1", "body": "Extrude1", "edges": [
      { "extrude": "Extrude1", "role": "side", "index": 1 },
      { "extrude": "Extrude2", "role": "side", "index": 1 }
    ], "radius": "d9" }
  ]
}"#;

#[test]
fn version_1_converts_to_the_current_model() {
    let mut doc = load(EXAMPLE_V1).unwrap();
    let value: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(value["version"], 2);
    let uids: Vec<_> = value["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["uid"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(uids, ["F1", "F2", "F3", "F4", "F5", "F6"]);
    // Shapes get curve ids in order, points after them; profiles are their
    // regions.
    let sketch2 = &value["features"][2];
    assert_eq!(
        sketch2["entities"][8],
        json!({"id": "c5", "type": "circle", "center": "p10", "radius": 5.0})
    );
    assert_eq!(
        sketch2["entities"][9],
        json!({"id": "p10", "type": "point", "at": [45.0, 20.0], "fixed": true})
    );
    assert_eq!(
        sketch2["dimensions"][2],
        json!({"id": "k7", "type": "diameter", "curve": "c5", "value": "d6"})
    );
    assert_eq!(
        sketch2["entities"][6],
        json!({"id": "p8", "type": "point", "at": [30.0, 20.0]})
    );
    assert_eq!(value["features"][4]["profiles"][0]["region"], "r{c5}");
    assert_eq!(value["features"][3]["operation"], "join");
    assert_eq!(value["features"][3]["participants"], json!(["F2.b0"]));
    // Corner 1 is where lines 0 and 1 of the extruded rectangle meet.
    assert_eq!(
        value["features"][5]["edges"],
        json!([
            "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}",
            "E{F4:side(c1[c4,c2])|F4:side(c2[c1,c3])}"
        ])
    );
    assert_eq!(value["features"][5]["body"], "F2.b0");
    // Each dimension belongs to the feature that uses it.
    assert_eq!(value["parameters"][3]["owner"], "F3");
    assert_eq!(value["parameters"][8]["owner"], "F6");
    assert_eq!(value["bodies"], json!([{"uid": "F2.b0", "name": "Body1"}]));

    let stats = doc.recompute();
    assert!(stats.error.is_none(), "{:?}", stats.error);
    assert_eq!(stats.evaluated.len(), 6);
    let shape = doc.body_shape(BodyUid::new(FeatureUid(2), 0)).unwrap();
    assert_eq!(
        shape.history,
        "fillet(cut(join(prism(F2:0..20),prism(F4:0..30)),prism(F5:0..30)),2)"
    );
    // Saved as version 2, it loads back to the same model.
    let again = load(&doc.to_json()).unwrap();
    assert_eq!(again.state(), doc.state());
}

#[test]
fn version_1_edges_of_every_role_convert() {
    let edge = |role: &str, index: usize| {
        let json = edited(EXAMPLE_V1, |v| {
            v["features"][5]["edges"] =
                json!([{"extrude": "Extrude2", "role": role, "index": index}]);
        });
        let value: Value = serde_json::from_str(&load(&json).unwrap().to_json()).unwrap();
        value["features"][5]["edges"][0]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let boss = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
    assert_eq!(edge("side", 0), "E{F4:side(c1[c4,c2])|F4:side(c4[c3,c1])}");
    assert_eq!(
        edge("start_cap", 2),
        format!("E{{F4:side(c3[c2,c4])|F4:start({boss})}}")
    );
    assert_eq!(
        edge("end_cap", 3),
        format!("E{{F4:end({boss})|F4:side(c4[c3,c1])}}")
    );
    let hole = |role: &str| {
        let json = edited(EXAMPLE_V1, |v| {
            v["features"][5]["edges"] = json!([{"extrude": "Extrude3", "role": role, "index": 0}]);
        });
        let value: Value = serde_json::from_str(&load(&json).unwrap().to_json()).unwrap();
        value["features"][5]["edges"][0]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(hole("start_cap"), "E{F5:side(c5)|F5:start(r{c5})}");
    assert_eq!(hole("end_cap"), "E{F5:end(r{c5})|F5:side(c5)}");
    // The same names as the commands make.
    assert_eq!(
        corner(FeatureUid(4), 1, 0),
        "E{F4:side(c1[c4,c2])|F4:side(c4[c3,c1])}".parse().unwrap()
    );
    assert_eq!(side(FeatureUid(4), 1, 2).to_string(), "F4:side(c3[c2,c4])");
}

#[test]
fn version_1_errors_keep_their_messages() {
    let rejected_edit = |edit: &dyn Fn(&mut Value)| {
        let mut value: Value = serde_json::from_str(EXAMPLE_V1).unwrap();
        edit(&mut value);
        rejected(&value.to_string())
    };
    let schema = rejected_edit(&|v| v["features"][5]["radus"] = json!("d9"));
    assert!(
        matches!(&schema, FileError::Schema(m) if m.contains("unknown field `radus`") && m.contains(" at line "))
    );
    type Case = (Box<dyn Fn(&mut Value)>, &'static str);
    let cases: Vec<Case> = vec![
        (
            Box::new(|v| v["features"][0]["shapes"][0]["height"] = json!("h")),
            "features[0] (Sketch1): shapes[0]: parameter 'h' does not exist",
        ),
        (
            Box::new(|v| v["features"][5]["radius"] = json!("r1")),
            "features[5] (Fillet1): parameter 'r1' does not exist",
        ),
        (
            Box::new(|v| v["features"][1]["sketch"] = json!("Sketch9")),
            "features[1] (Extrude1): feature 'Sketch9' does not exist",
        ),
        (
            Box::new(|v| v["features"][1]["sketch"] = json!("Sketch2")),
            "features[1] (Extrude1): feature 'Sketch2' does not come before this feature",
        ),
        (
            Box::new(|v| v["features"][3]["sketch"] = json!("Extrude1")),
            "features[3] (Extrude2): 'Extrude1' is not a sketch",
        ),
        (
            Box::new(|v| v["features"][4]["body"] = json!("Extrude2")),
            "features[4] (Extrude3): 'Extrude2' is not a new_body extrude",
        ),
        (
            Box::new(|v| v["features"][5]["body"] = json!("Sketch1")),
            "features[5] (Fillet1): 'Sketch1' is not a new_body extrude",
        ),
        (
            Box::new(|v| v["features"][1]["body"] = json!("Extrude1")),
            "a new_body extrude has no \"body\"",
        ),
        (
            Box::new(|v| {
                v["features"][4].as_object_mut().unwrap().remove("body");
            }),
            "features[4] (Extrude3): a join or cut extrude needs the \"body\"",
        ),
        (
            Box::new(|v| v["features"][1]["profile"] = json!(1)),
            "features[1] (Extrude1): Sketch1 has no profile 1",
        ),
        (
            Box::new(|v| v["features"][5]["edges"] = json!([])),
            "features[5] (Fillet1): no edges",
        ),
        (
            Box::new(|v| {
                v["features"][5]["edges"] =
                    json!([{"extrude": "Extrude1", "role": "side", "index": 4}]);
            }),
            "Extrude1 has no side edge 4",
        ),
        (
            Box::new(|v| {
                v["features"][5]["edges"] =
                    json!([{"extrude": "Extrude1", "role": "start_cap", "index": u64::MAX}]);
            }),
            "Extrude1 has no start cap edge 18446744073709551615",
        ),
        (
            Box::new(|v| {
                v["features"][5]["edges"] =
                    json!([{"extrude": "Extrude3", "role": "side", "index": 0}]);
            }),
            "Extrude3 has no side edge 0",
        ),
        (
            Box::new(|v| {
                let twice = json!({"extrude": "Extrude2", "role": "end_cap", "index": 2});
                v["features"][5]["edges"] = json!([twice.clone(), twice]);
            }),
            "end cap edge 2 of Extrude2 is listed more than once",
        ),
        (
            Box::new(|v| {
                v["features"][5]["edges"][0] =
                    json!({"extrude": "Sketch2", "role": "side", "index": 0});
            }),
            "side edge 0 of Sketch2 is not on the body of Extrude1",
        ),
        (
            Box::new(|v| v["parameters"][3]["name"] = json!("")),
            "parameters[3]: the name is empty",
        ),
        (
            Box::new(|v| v["features"][2]["name"] = json!("Sketch1")),
            "features[2]: feature name 'Sketch1' is used more than once",
        ),
    ];
    for (edit, expected) in cases {
        assert_invalid(rejected_edit(&*edit), &[expected]);
    }
    // V0 allowed one fillet per body; version 1 files with more now load.
    let mut value: Value = serde_json::from_str(EXAMPLE_V1).unwrap();
    let mut second = value["features"][5].clone();
    second["name"] = json!("Fillet2");
    value["features"].as_array_mut().unwrap().push(second);
    assert!(load(&value.to_string()).is_ok());
}
