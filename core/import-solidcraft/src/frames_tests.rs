// SPDX-License-Identifier: MIT
//! 3D curves as 3D sketches and frame members as pipes and sweeps
//! (mitcad#2157), on the mock kernel: the definitions the import makes,
//! the 3D sketches solved, the planes and section frames the members are
//! placed on. Volumes and boxes on the geometry kernel:
//! `core/solidcraft/tests/designs/frame-members.*` (ctest
//! `solidcraft.designs`).

use mitcad_model::testing::MockKernel;
use mitcad_model::{Datum, Document, EntityUid, FeatureDef, FeatureUid, ParamId};
use serde_json::{Value, json};

use super::*;

fn import_value(v: Value, set: &[(&str, &str)]) -> (Document<MockKernel>, SolidcraftReport) {
    let file = mitcad_solidcraft::SolidcraftFile::from_value(v).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let options = SolidcraftOptions {
        set_parameters: set
            .iter()
            .map(|(n, e)| ((*n).to_owned(), (*e).to_owned()))
            .collect(),
        ..SolidcraftOptions::default()
    };
    let report = import_solidcraft_file(&mut doc, file, "t.solidcraft", &options).unwrap();
    (doc, report)
}

fn feature(report: &SolidcraftReport, name: &str) -> FeatureReport {
    report
        .features
        .iter()
        .find(|f| f.name == name)
        .cloned()
        .unwrap_or_else(|| panic!("no {name} in {:?}", report.features))
}

fn uid(doc: &Document<MockKernel>, name: &str) -> FeatureUid {
    doc.features()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no feature {name}"))
        .uid
}

/// A definition as JSON, its values by their parameters' names.
fn json_of(doc: &Document<MockKernel>, def: &FeatureDef) -> Value {
    let named: FeatureDef<String> = def
        .map_params(&mut |_, id: &ParamId| Ok::<_, ()>(doc.parameters().name(*id)))
        .unwrap();
    serde_json::to_value(&named).unwrap()
}

/// A feature's definition as JSON.
fn def(doc: &Document<MockKernel>, name: &str) -> Value {
    let f = doc.features().find(|f| f.name == name).unwrap();
    json_of(doc, &f.def)
}

/// The definitions of every feature of a type, in timeline order.
fn defs_of(doc: &Document<MockKernel>, kind: &str) -> Vec<(String, Value)> {
    doc.features()
        .filter(|f| f.def.type_name() == kind)
        .map(|f| (f.name.clone(), json_of(doc, &f.def)))
        .collect()
}

fn item<'a>(list: &'a Value, id: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {list}"))
}

/// A 3D sketch's solved point.
fn solved(doc: &Document<MockKernel>, sketch: FeatureUid, point: &Value) -> [f64; 3] {
    let n: u32 = point.as_str().unwrap()[1..].parse().unwrap();
    let out = doc.spatial_sketch(sketch).expect("a 3D sketch");
    assert!(out.is_solved(), "{:?}", out.failure);
    out.solved.as_ref().unwrap().points[&EntityUid(n)]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

fn wire(id: &str, fit: &[[f64; 3]]) -> Value {
    json!({"id": id, "space": "component", "fit": fit, "pts": fit})
}

fn sketch_3d(id: u64, name: &str, wires: Vec<Value>, extra: Value) -> Value {
    let mut sketch = json!({"points": [{"id": "origin", "pos": [0.0, 0.0], "fixed": true}],
        "curves": [], "constraints": [], "wires": wires});
    if let Some(o) = extra.as_object() {
        for (k, v) in o {
            sketch[k] = v.clone();
        }
    }
    json!({"id": id, "name": name, "type": "sketch",
        "plane": {"type": "origin", "name": "XY"}, "sketch": sketch})
}

/// A sketch of 3D lines, an exact arc, a fit spline and a point (format
/// 57, before 3D relations) is a 3D sketch of its own name: ends that meet
/// share one point, every point is fixed where the file has it; a sketch
/// with planar curves too keeps them and gets `<name> 3D` after it.
#[test]
fn three_d_curves_come_in_as_a_3d_sketch_with_joined_ends() {
    let s = std::f64::consts::FRAC_1_SQRT_2 * 10.0;
    let path = sketch_3d(
        1,
        "Path",
        vec![
            wire("w1", &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]),
            json!({"id": "w2", "space": "component",
                "fit": [[10.0, 0.0, 0.0], [10.0 + s, 0.0, 10.0 - s], [20.0, 0.0, 10.0]],
                "pts": [[10.0, 0.0, 0.0], [20.0, 0.0, 10.0]],
                "arc": {"center": [10.0, 0.0, 10.0], "normal": [0.0, -1.0, 0.0], "radius": 10.0,
                        "start": [10.0, 0.0, 0.0], "end": [20.0, 0.0, 10.0]}}),
            wire(
                "w3",
                &[
                    [20.0, 0.0, 10.0],
                    [25.0, 5.0, 15.0],
                    [30.0, 0.0, 20.0],
                    [35.0, -5.0, 25.0],
                ],
            ),
            wire("q4", &[[50.0, 50.0, 50.0]]),
        ],
        json!({}),
    );
    let mixed = json!({"id": 2, "name": "Mixed", "type": "sketch",
        "plane": {"type": "origin", "name": "XY"},
        "sketch": {"points": [{"id": "origin", "pos": [0.0, 0.0], "fixed": true},
                              {"id": "a", "pos": [0.0, 0.0]}, {"id": "b", "pos": [5.0, 0.0]}],
                   "curves": [{"id": "l1", "type": "line", "a": 1, "b": 2}], "constraints": [],
                   "wires": [{"id": "w1", "fit": [[0.0, 0.0, 0.0], [0.0, 0.0, 5.0]],
                              "pts": [[0.0, 0.0, 0.0], [0.0, 0.0, 5.0]]}]}});
    let (doc, report) = import_value(
        json!({"format": "solidcraft/57", "name": "W", "params": [], "features": [path, mixed]}),
        &[],
    );
    // The fit spline's note: MitCad fits its own cubic through the points.
    assert_eq!(feature(&report, "Path").outcome, Outcome::Partial);
    let d = def(&doc, "Path");
    assert_eq!(d["type"], "spatial_sketch");
    let curves = d["curves"].as_array().unwrap();
    let kinds: Vec<&str> = curves.iter().map(|c| c["type"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["line", "arc", "fitted_spline"]);
    let (line, arc, spline) = (&curves[0], &curves[1], &curves[2]);
    // Joined: the line's end is the arc's start, the arc's end the
    // spline's first point.
    assert_eq!(line["end"], arc["start"]);
    assert_eq!(arc["end"], spline["points"][0]);
    assert_eq!(spline["points"].as_array().unwrap().len(), 4);
    // Line ends, the arc's centre and end, the spline's three more points
    // and the point: eight, each fixed.
    assert_eq!(d["points"].as_array().unwrap().len(), 8, "{d}");
    let fixes = d["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "fix")
        .count();
    assert_eq!(fixes, 8);
    let path = uid(&doc, "Path");
    assert!(near(solved(&doc, path, &arc["center"]), [10.0, 0.0, 10.0]));
    assert!(near(solved(&doc, path, &arc["end"]), [20.0, 0.0, 10.0]));
    let arc_def = item(&d["curves"], arc["id"].as_str().unwrap());
    assert!(near(
        std::array::from_fn(|i| arc_def["normal"][i].as_f64().unwrap()),
        [0.0, -1.0, 0.0]
    ));
    // The planar sketch stays one, its 3D curve after it.
    assert_eq!(def(&doc, "Mixed")["type"], "sketch");
    assert_eq!(def(&doc, "Mixed 3D")["type"], "spatial_sketch");
    let names: Vec<String> = doc.features().map(|f| f.name.clone()).collect();
    assert_eq!(names, ["Path", "Mixed", "Mixed 3D"]);
    let sk = report.sketches.iter().find(|s| s.name == "Path").unwrap();
    assert_eq!(sk.curves, 4);
    assert!(sk.dropped.is_empty(), "{:?}", sk.dropped);
}

/// The relations of format 58 (fix, coincident, along an axis, a
/// parameter's distance, perpendicular, on a plane, along an axis's
/// distance, tangent, radius, parallel, equal, a plane's distance, an
/// angle, collinear, point on a line, a driven distance) come in as the 3D
/// sketch's; the two without a counterpart are left out with their kinds.
/// The length follows its parameter: changing it moves the curves.
#[test]
fn three_d_relations_and_a_parameter_drive_the_3d_sketch() {
    let s = std::f64::consts::FRAC_1_SQRT_2 * 10.0;
    let (q, h) = (5.0, -10.0 + 10.0 * (std::f64::consts::PI / 3.0).sin());
    let p = |w: &str, i: usize| json!({"wire": w, "index": i});
    let sketch = sketch_3d(
        1,
        "Frame",
        vec![
            wire("w1", &[[0.0, 0.0, 0.0], [40.0, 0.0, 0.0]]),
            wire("w2", &[[40.0, 0.0, 0.0], [40.0, 30.0, 0.0]]),
            wire("w3", &[[40.0, 30.0, 0.0], [40.0, 30.0, 20.0]]),
            json!({"id": "w4", "space": "component",
                "fit": [[40.0, 30.0, 20.0], [30.0 + s, 30.0, 20.0 + s], [30.0, 30.0, 30.0]],
                "pts": [[40.0, 30.0, 20.0], [30.0, 30.0, 30.0]],
                "arc": {"center": [30.0, 30.0, 20.0], "normal": [0.0, -1.0, 0.0], "radius": 10.0,
                        "start": [40.0, 30.0, 20.0], "end": [30.0, 30.0, 30.0]}}),
            wire("w5", &[[0.0, 30.0, 0.0], [0.0, 30.0, 20.0]]),
            wire("w6", &[[0.0, -10.0, 0.0], [q, h, 0.0]]),
            wire("w7", &[[50.0, 0.0, 0.0], [60.0, 0.0, 0.0]]),
            wire("q8", &[[20.0, 0.0, 0.0]]),
        ],
        json!({"wires_joined": true, "wire_constraints": [
            {"id": "k1", "type": "fix", "p": p("w1", 0), "at": [0, 0, 0]},
            {"id": "k2", "type": "coincident", "a": p("w1", 1), "b": p("w2", 0)},
            {"id": "k3", "type": "coincident", "a": p("w2", 1), "b": p("w3", 0)},
            {"id": "k4", "type": "coincident", "a": p("w3", 1), "b": p("w4", 0)},
            {"id": "k5", "type": "along_axis", "line": "w1", "axis": "x"},
            {"id": "k6", "type": "distance", "a": p("w1", 0), "b": p("w1", 1), "value": 40, "param": "d1"},
            {"id": "k7", "type": "perpendicular", "a": "w1", "b": "w2"},
            {"id": "k8", "type": "on_plane", "p": p("w2", 1), "plane": "xy"},
            {"id": "k9", "type": "axis_distance", "a": p("w2", 0), "b": p("w2", 1), "axis": "y", "value": 30},
            {"id": "k10", "type": "along_axis", "line": "w3", "axis": "z"},
            {"id": "k11", "type": "distance", "a": p("w3", 0), "b": p("w3", 1), "value": 20},
            {"id": "k12", "type": "tangent", "line": "w3", "arc": "w4", "line_end": 1, "arc_end": 0},
            {"id": "k13", "type": "radius", "arc": "w4", "value": 10},
            {"id": "k14", "type": "parallel", "a": "w5", "b": "w3"},
            {"id": "k15", "type": "equal", "a": "w5", "b": "w3"},
            {"id": "k16", "type": "on_plane", "p": p("w5", 0), "plane": "yz"},
            {"id": "k17", "type": "on_plane", "p": p("w5", 0), "plane": "xy"},
            {"id": "k18", "type": "plane_distance", "p": p("w5", 0), "plane": "xz", "value": 30},
            {"id": "k19", "type": "fix", "p": p("w6", 0), "at": [0, -10, 0]},
            {"id": "k20", "type": "distance", "a": p("w6", 0), "b": p("w6", 1), "value": 10},
            {"id": "k21", "type": "angle", "a": "w1", "b": "w6", "value": std::f64::consts::PI / 3.0},
            {"id": "k22", "type": "on_plane", "p": p("w6", 1), "plane": "xy"},
            {"id": "k23", "type": "collinear", "a": "w1", "b": "w7"},
            {"id": "k24", "type": "axis_distance", "a": p("w1", 1), "b": p("w7", 0), "axis": "x", "value": 10},
            {"id": "k30", "type": "distance", "a": p("w7", 0), "b": p("w7", 1), "value": 10},
            {"id": "k25", "type": "point_on_line", "p": p("q8", 0), "line": "w1"},
            {"id": "k26", "type": "axis_distance", "a": p("w1", 0), "b": p("q8", 0), "axis": "x", "value": 20},
            {"id": "k27", "type": "distance", "a": p("w5", 0), "b": p("w2", 1), "value": 40, "driven": true},
            {"id": "k28", "type": "parallel_to_plane", "line": "w1", "plane": "xy"},
            {"id": "k29", "type": "plane_angle", "line": "w6", "plane": "xy", "value": 0}
        ]}),
    );
    let design = |len: &str| {
        json!({"format": "solidcraft/58", "name": "R",
            "params": [{"name": "len", "expr": len, "unit": "mm"},
                       {"name": "d1", "expr": "len", "unit": "mm", "model": true}],
            "features": [sketch.clone()]})
    };
    let (doc, report) = import_value(design("40 mm"), &[]);
    let sk = report.sketches.iter().find(|s| s.name == "Frame").unwrap();
    // Every relation and dimension but the two to a plane's direction.
    assert_eq!(sk.constraints + sk.dimensions, 30);
    assert_eq!(
        sk.constraints_mapped + sk.dimensions_mapped,
        28,
        "{:?}",
        sk.dropped
    );
    let dropped = sk.dropped.join("; ");
    assert!(dropped.contains("k28 (parallel_to_plane)"), "{dropped}");
    assert!(dropped.contains("k29 (plane_angle)"), "{dropped}");
    let d = def(&doc, "Frame");
    let kinds = |key: &str| -> Vec<String> {
        d[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["type"].as_str().unwrap().to_owned())
            .collect()
    };
    let relations = kinds("constraints");
    for k in [
        "fix",
        "align",
        "perpendicular",
        "on_plane",
        "tangent",
        "parallel",
        "equal",
        "point_on",
        "attach",
    ] {
        assert!(relations.iter().any(|r| r == k), "{k} in {relations:?}");
    }
    let dims = kinds("dimensions");
    for k in ["length", "offset", "angle", "radius"] {
        assert!(dims.iter().any(|r| r == k), "{k} in {dims:?}");
    }
    // The driven distance: a construction line's length without a value.
    assert!(
        d["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["type"] == "length" && x.get("value").is_none()),
        "{d}"
    );
    // The length's parameter is the file's, bound to its expression.
    let p = doc.parameters();
    let d1 = p.find("d1").expect("d1");
    assert_eq!(p.get(d1).unwrap().expression(), "len");
    assert_eq!(p.owner(d1), Some(uid(&doc, "Frame")));
    // Coincident points are one: w1's end is w2's start.
    let c = d["curves"].as_array().unwrap();
    assert_eq!(c[0]["end"], c[1]["start"]);
    let frame = uid(&doc, "Frame");
    let b = c[0]["end"].clone();
    let corner = c[2]["end"].clone();
    assert!(near(solved(&doc, frame, &b), [40.0, 0.0, 0.0]));
    assert!(near(solved(&doc, frame, &corner), [40.0, 30.0, 20.0]));
    assert!(
        feature(&report, "Frame")
            .notes
            .join(" ")
            .contains("plane_angle")
    );

    // len 50: the line is longer, the curves after it moved.
    let (doc, _) = import_value(design("40 mm"), &[("len", "50 mm")]);
    let frame = uid(&doc, "Frame");
    assert!(near(solved(&doc, frame, &b), [50.0, 0.0, 0.0]));
    assert!(near(solved(&doc, frame, &corner), [50.0, 30.0, 20.0]));
}

/// A round tube over two 3D lines that meet: a pipe per line, each square
/// at the corner, of the file's diameter and wall (one parameter each,
/// shared), its body named as the file names it.
#[test]
fn a_round_tube_member_is_a_pipe_per_line() {
    let (doc, report) = import_value(
        json!({"format": "solidcraft/57", "name": "T", "params": [], "features": [
            sketch_3d(1, "Route", vec![
                wire("w1", &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]),
                wire("w2", &[[100.0, 0.0, 0.0], [100.0, 0.0, 80.0]])], json!({})),
            {"id": 2, "name": "Tubes", "type": "frame_member", "path_sketch": 1,
             "profile": {"source": "library", "profile": {"kind": "round_tube", "diameter": "20", "wall": "2"}},
             "body_name": "Tube"}]}),
        &[],
    );
    assert_eq!(feature(&report, "Tubes").outcome, Outcome::Parametric);
    let pipes = defs_of(&doc, "pipe");
    assert_eq!(pipes.len(), 2, "{pipes:?}");
    assert_eq!(pipes[0].0, "Tubes");
    assert_eq!(pipes[1].0, "Tubes (2)");
    let route = uid(&doc, "Route").to_string();
    let lines = def(&doc, "Route")["curves"].clone();
    for (k, (_, pipe)) in pipes.iter().enumerate() {
        assert_eq!(pipe["path"]["spatial_sketch"], route);
        assert_eq!(pipe["path"]["curves"], json!([lines[k]["id"]]));
        // Circular, the default (left out of the written definition).
        assert!(
            pipe["section"].is_null() || pipe["section"] == "circular",
            "{pipe}"
        );
        assert!(pipe.get("corners").is_none());
    }
    // One parameter each for the diameter and the wall.
    assert_eq!(pipes[0].1["size"], pipes[1].1["size"]);
    assert_eq!(pipes[0].1["thickness"], pipes[1].1["thickness"]);
    let p = doc.parameters();
    let values: Vec<f64> = p
        .owned_by(uid(&doc, "Tubes"))
        .into_iter()
        .filter_map(|q| p.value(q))
        .collect();
    assert!(
        values.contains(&20.0) && values.contains(&2.0),
        "{values:?}"
    );
    let mut names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Tube", "Tube (2)"]);
}

/// A rectangular tube turned by its angle and a sketch's profile: per line
/// a plane along the line at its start, the section in the member's frame
/// on it (its y up as far as the line allows, turned about the line; the
/// profile point on the line) and a sweep; the line's end moved by a
/// parameter turns the plane with it.
#[test]
fn other_members_are_sweeps_that_follow_their_lines() {
    let p = |w: &str, i: usize| json!({"wire": w, "index": i});
    let design = json!({"format": "solidcraft/58", "name": "S",
        "params": [{"name": "lift", "expr": "0 mm", "unit": "mm"}],
        "features": [
            sketch_3d(1, "Rails", vec![wire("w1", &[[0.0, 100.0, 0.0], [200.0, 100.0, 0.0]])],
                json!({"wires_joined": true, "wire_constraints": [
                    {"id": "k1", "type": "fix", "p": p("w1", 0), "at": [0, 100, 0]},
                    {"id": "k2", "type": "fix", "p": p("w1", 1), "at": [200, 100, 0], "exprs": {"z": "lift"}}]})),
            {"id": 2, "name": "Rail", "type": "frame_member", "path_sketch": 1, "lines": ["w1"],
             "profile": {"source": "library", "profile": {"kind": "rect_tube", "width": "40", "height": "20",
                         "wall": "2"}},
             "angle": "30", "names": {"w1": "Top rail"}},
            {"id": 3, "name": "Profile", "type": "sketch", "plane": {"type": "origin", "name": "XY"},
             "sketch": {"points": [{"id": "origin", "pos": [0.0, 0.0], "fixed": true},
                                   {"id": "a", "pos": [10.0, 0.0], "fixed": true},
                                   {"id": "b", "pos": [10.0, 10.0], "fixed": true},
                                   {"id": "c", "pos": [0.0, 10.0], "fixed": true}],
                        "curves": [{"id": "l1", "type": "line", "a": 0, "b": 1},
                                   {"id": "l2", "type": "line", "a": 1, "b": 2},
                                   {"id": "l3", "type": "line", "a": 2, "b": 3},
                                   {"id": "l4", "type": "line", "a": 3, "b": 0}],
                        "constraints": []}},
            sketch_3d(4, "Posts", vec![wire("w1", &[[300.0, 0.0, 0.0], [300.0, 0.0, 50.0]])], json!({})),
            {"id": 5, "name": "Post", "type": "frame_member", "path_sketch": 4,
             "profile": {"source": "sketch", "sketch": 3}, "mirror": true}]});
    let (doc, report) = import_value(design.clone(), &[]);
    assert_ne!(feature(&report, "Rail").outcome, Outcome::Failed);
    let sweeps = defs_of(&doc, "sweep");
    assert_eq!(sweeps.len(), 2, "{:?}", report.features);
    let rails = uid(&doc, "Rails").to_string();
    assert_eq!(sweeps[0].1["path"]["spatial_sketch"], rails);
    // Perpendicular, the default (left out of the written definition).
    assert!(
        sweeps[0].1["orientation"].is_null() || sweeps[0].1["orientation"] == "perpendicular",
        "{}",
        sweeps[0].1
    );
    // The section's frame: x along (0, cos 30, sin 30), y along
    // (0, -sin 30, cos 30), its origin the line's start (the profile's
    // centre on the line).
    let section = uid(&doc, "Rail Section");
    let f = doc.sketch_output(section).unwrap().frame;
    let (c, s) = (
        (std::f64::consts::PI / 6.0).cos(),
        (std::f64::consts::PI / 6.0).sin(),
    );
    assert!(near(f.x_axis, [0.0, c, s]), "{f:?}");
    assert!(near(f.y_axis, [0.0, -s, c]), "{f:?}");
    assert!(near(f.origin, [0.0, 100.0, 0.0]), "{f:?}");
    // The sketch profile, mirrored, up the post: its x is -X.
    let post = doc.sketch_output(uid(&doc, "Post Section")).unwrap().frame;
    assert!(near(post.x_axis, [1.0, 0.0, 0.0]), "{post:?}");
    assert!(near(post.origin, [300.0, 0.0, 0.0]), "{post:?}");
    let entities = def(&doc, "Post Section")["entities"].clone();
    assert!(
        entities
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "point" && e["at"][0].as_f64() == Some(-10.0)),
        "{entities}"
    );
    let mut names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Profile", "Top rail"]);
    // The plane along the rail, before and after its end is lifted.
    let plane_normal = |doc: &Document<MockKernel>| {
        let planes = defs_of(doc, "construction_plane");
        let uid = doc
            .features()
            .find(|f| f.def.type_name() == "construction_plane")
            .unwrap()
            .uid;
        assert_eq!(planes[0].1["definition"]["type"], "along_path");
        let Some(Datum::Plane(p)) = doc.datum(uid) else {
            panic!("no plane")
        };
        p.normal()
    };
    assert!(near(plane_normal(&doc), [1.0, 0.0, 0.0]));
    let (doc, _) = import_value(design, &[("lift", "50 mm")]);
    let n = plane_normal(&doc);
    let l = (200.0f64 * 200.0 + 50.0 * 50.0).sqrt();
    assert!(near(n, [200.0 / l, 0.0, 50.0 / l]), "{n:?}");
}

/// A linked 3D curve's exact pieces (a line, a quarter circle, a spline)
/// are fixed curves; one known only by its points is left out with a
/// note.
#[test]
fn linked_3d_curves_are_fixed_curves() {
    let sketch = sketch_3d(
        1,
        "Linked",
        vec![
            json!({"id": "w1", "link": "j1", "space": "component",
                "pts": [[0, 0, 0], [10, 0, 0], [10, 10, 0]],
                "exact": [
                    {"curve": {"kind": "line", "start": [0, 0, 0], "end": [10, 0, 0]}, "interval": [0, 1]},
                    {"curve": {"kind": "circle", "frame": {"origin": [10, 5, 0], "x_axis": [0, -1, 0], "y_axis": [1, 0, 0]},
                               "radius": 5}, "interval": [0, std::f64::consts::PI]},
                    {"curve": {"kind": "spline", "knots": [0, 0, 0, 1, 1, 1],
                               "points": [[10, 10, 0], [5, 15, 5], [0, 10, 10]]}, "interval": [0, 1]}]}),
            json!({"id": "w2", "link": "j2", "pts": [[0, 0, 0], [1, 1, 1], [2, 0, 2]]}),
        ],
        json!({"links": [{"id": "j1", "type": "include"}, {"id": "j2", "type": "project"}]}),
    );
    let (doc, report) = import_value(
        json!({"format": "solidcraft/57", "name": "L", "params": [], "features": [sketch]}),
        &[],
    );
    let d = def(&doc, "Linked");
    let kinds: Vec<&str> = d["curves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["line", "arc", "spline"]);
    let c = d["curves"].as_array().unwrap();
    assert_eq!(c[0]["end"], c[1]["start"]);
    assert_eq!(c[1]["end"], c[2]["control"][0]);
    let linked = uid(&doc, "Linked");
    assert!(near(solved(&doc, linked, &c[1]["end"]), [10.0, 10.0, 0.0]));
    let sk = report.sketches.iter().find(|s| s.name == "Linked").unwrap();
    assert!(sk.dropped.join(" ").contains("w2"), "{:?}", sk.dropped);
}

/// A pipe network saved in format 63 (mitcad#2175, the shape of GitHub
/// #526's reduced file): three sketches of nothing but 3D lines whose ends
/// stop short of each other (`wires_joined` without relations), each with
/// a round tube member that names its parameters (`param_names`) and its
/// bodies by line (`names`). No newer-design warning; each sketch is a 3D
/// sketch of its own name with its points fixed; one pipe per line, each
/// member's diameter and wall its own parameters of the file's names and
/// values, shared by its pipes; every feature parametric, without a note.
#[test]
fn a_pipe_network_of_format_63_comes_in_whole() {
    let view = json!({"view": {"three_d": true}, "wires_joined": true, "counters": {"w": 3}});
    let tube = |id: u64,
                name: &str,
                sketch: u64,
                d: &str,
                wall: &str,
                names: Value,
                params: [&str; 2]| {
        let body = names["w1"].clone();
        json!({"id": id, "name": name, "type": "frame_member", "path_sketch": sketch,
            "profile": {"source": "library", "profile": {"kind": "round_tube", "diameter": d, "wall": wall}},
            "body_name": body, "names": names, "param_names": params, "suppressed": false})
    };
    let design = json!({"format": "solidcraft/63", "name": "N", "units": "mm", "params": [],
        "next_id": 7, "next_component_color_index": 0,
        "appearances": {"bodies": {"Main": {"color": [85, 93, 97], "name": "Custom", "opacity": 1.0}}},
        "features": [
            sketch_3d(1, "Main route", vec![
                wire("w1", &[[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0]]),
                wire("w2", &[[1020.0, 20.0, 0.0], [1020.0, 800.0, 0.0]]),
                wire("w3", &[[1020.0, 820.0, 20.0], [1020.0, 820.0, 500.0]])], view.clone()),
            tube(2, "Main members", 1, "48.3", "2.6",
                json!({"w1": "Main", "w2": "Main (2)", "w3": "Main (3)"}), ["d1", "d2"]),
            sketch_3d(3, "Branch route", vec![
                wire("w1", &[[0.0, 300.0, 0.0], [600.0, 300.0, 0.0]])], view.clone()),
            tube(4, "Branch member", 3, "48.3", "2.6", json!({"w1": "Branch"}), ["d3", "d4"]),
            sketch_3d(5, "Small route", vec![
                wire("w1", &[[0.0, 900.0, 0.0], [0.0, 1400.0, 0.0]]),
                wire("w2", &[[15.0, 1415.0, 0.0], [500.0, 1415.0, 0.0]])], view),
            tube(6, "Small members", 5, "26.9", "2.3",
                json!({"w1": "Small", "w2": "Small (2)"}), ["d5", "d6"])]});
    let (doc, report) = import_value(design, &[]);
    assert!(report.newer.is_none(), "{:?}", report.newer);
    assert_eq!(report.features.len(), 6, "{:?}", report.features);
    for f in &report.features {
        assert_eq!(f.outcome, Outcome::Parametric, "{}: {:?}", f.name, f.notes);
        assert!(f.notes.is_empty(), "{}: {:?}", f.name, f.notes);
    }
    // Each sketch is a 3D sketch of its own name, its lines apart, every
    // point fixed where the file has it; no planar sketch, no placeholder.
    let spatial = defs_of(&doc, "spatial_sketch");
    let names: Vec<&str> = spatial.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Main route", "Branch route", "Small route"]);
    assert!(defs_of(&doc, "sketch").is_empty());
    assert_eq!(
        doc.features().count(),
        9,
        "{:?}",
        doc.features()
            .map(|f| f.def.type_name())
            .collect::<Vec<_>>()
    );
    for ((name, d), lines) in spatial.iter().zip([3, 1, 2]) {
        let curves = d["curves"].as_array().unwrap();
        assert_eq!(curves.len(), lines, "{name}: {d}");
        assert!(curves.iter().all(|c| c["type"] == "line"), "{name}: {d}");
        // Ends that stop short of each other are points of their own.
        assert_eq!(
            d["points"].as_array().unwrap().len(),
            2 * lines,
            "{name}: {d}"
        );
        let sk = report.sketches.iter().find(|s| s.name == *name).unwrap();
        assert_eq!(sk.curves, lines);
        assert!(sk.dropped.is_empty(), "{name}: {:?}", sk.dropped);
    }
    let main = uid(&doc, "Main route");
    let w3 = def(&doc, "Main route")["curves"][2].clone();
    assert!(near(
        solved(&doc, main, &w3["start"]),
        [1020.0, 820.0, 20.0]
    ));
    assert!(near(solved(&doc, main, &w3["end"]), [1020.0, 820.0, 500.0]));
    // One pipe per line along its 3D line, each member's diameter and wall
    // the parameters the file names, shared by its pipes.
    let pipes = defs_of(&doc, "pipe");
    let names: Vec<&str> = pipes.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "Main members",
            "Main members (2)",
            "Main members (3)",
            "Branch member",
            "Small members",
            "Small members (2)"
        ]
    );
    let routes = [main, main, main, uid(&doc, "Branch route")]
        .into_iter()
        .chain([uid(&doc, "Small route"); 2]);
    for ((name, pipe), route) in pipes.iter().zip(routes) {
        assert_eq!(pipe["path"]["spatial_sketch"], route.to_string(), "{name}");
        assert_eq!(
            pipe["path"]["curves"].as_array().unwrap().len(),
            1,
            "{name}"
        );
    }
    let sizes = [
        ("d1", "d2", 0..3, 48.3, 2.6),
        ("d3", "d4", 3..4, 48.3, 2.6),
        ("d5", "d6", 4..6, 26.9, 2.3),
    ];
    let p = doc.parameters();
    for (size, wall, range, dia, thickness) in sizes {
        for (name, pipe) in &pipes[range.clone()] {
            assert_eq!(pipe["size"], size, "{name}: {pipe}");
            assert_eq!(pipe["thickness"], wall, "{name}: {pipe}");
        }
        let owner = uid(&doc, &pipes[range.start].0);
        for (param, value) in [(size, dia), (wall, thickness)] {
            let id = p.find(param).unwrap_or_else(|| panic!("no {param}"));
            assert!((p.value(id).unwrap() - value).abs() < 1e-9, "{param}");
            assert_eq!(p.owner(id), Some(owner), "{param}");
        }
    }
    let mut bodies: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    bodies.sort();
    assert_eq!(
        bodies,
        [
            "Branch",
            "Main",
            "Main (2)",
            "Main (3)",
            "Small",
            "Small (2)"
        ]
    );
}
