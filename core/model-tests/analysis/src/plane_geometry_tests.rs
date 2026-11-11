// SPDX-License-Identifier: MIT
//! The `plane_geometry` query (mitcad#256): the visible bodies' edges and
//! vertices that lie in a sketch's plane, with the mock kernel.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, VertexName};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn plane_geometry(doc: &Document<MockKernel>, sketch: &str) -> Vec<Value> {
    query(doc, json!({"query": "plane_geometry", "sketch": sketch}))
        .as_array()
        .unwrap()
        .clone()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn near(p: &Value, q: [f64; 2]) -> bool {
    close(p[0].as_f64().unwrap(), q[0]) && close(p[1].as_f64().unwrap(), q[1])
}

/// A point of the model in the coordinates of a sketch (its `sketch` query).
fn in_sketch(doc: &Document<MockKernel>, sketch: &str, p: [f64; 3]) -> [f64; 2] {
    let frame = query(doc, json!({"query": "sketch", "uid": sketch}))["frame"].clone();
    let v = |key: &str| -> [f64; 3] { std::array::from_fn(|i| frame[key][i].as_f64().unwrap()) };
    let (origin, x, y) = (v("origin"), v("x_axis"), v("y_axis"));
    let d: [f64; 3] = std::array::from_fn(|i| p[i] - origin[i]);
    use mitcad_vec3::dot;
    [dot(d, x), dot(d, y)]
}

fn edge(a: &FaceName, b: &FaceName) -> String {
    EdgeName::new(a.clone(), b.clone()).to_string()
}

fn vertex(faces: &[&FaceName]) -> String {
    VertexName::new(faces.iter().map(|f| (*f).clone()))
        .unwrap()
        .to_string()
}

/// The curve id a side face sweeps (`F2:side(c1[c4,c2])` gives `c1`).
fn side_curve(face: &FaceName) -> String {
    let text = face.to_string();
    let start = text.find("side(").expect("a side face") + 5;
    let end = text[start..].find('[').map_or(text.len(), |k| start + k);
    text[start..end].to_owned()
}

/// A 60 x 40 block 20 high (Sketch1 F1, Extrude1 F2, Body1 F2.b0) whose
/// edges and corners the mock kernel knows where they are; its faces.
struct Block {
    doc: Document<MockKernel>,
    top: FaceName,
    bottom: FaceName,
    sides: Vec<FaceName>,
}

fn block() -> Block {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    // Where Sketch1's lines are (the sketch is on XY).
    let sketch1 = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    let line = |id: &str| -> ([f64; 2], [f64; 2]) {
        let e = sketch1["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("no {id} in {sketch1}"));
        let p = |key: &str| {
            [
                e["geometry"][key][0].as_f64().unwrap(),
                e["geometry"][key][1].as_f64().unwrap(),
            ]
        };
        (p("start"), p("end"))
    };
    let shape = doc
        .body_shape(BodyUid::new(FeatureUid(2), 0))
        .expect("Body1")
        .clone();
    let role = |f: &FaceName, r: &str| f.role == r;
    let top = shape.faces.iter().find(|f| role(f, "end")).unwrap().clone();
    let bottom = shape
        .faces
        .iter()
        .find(|f| role(f, "start"))
        .unwrap()
        .clone();
    let sides: Vec<FaceName> = shape
        .faces
        .iter()
        .filter(|f| role(f, "side"))
        .cloned()
        .collect();
    assert_eq!(sides.len(), 4, "{shape:?}");
    let at = |p: [f64; 2], z: f64| [p[0], p[1], z];
    let mut curves = doc.kernel().named_curves.borrow_mut();
    let mut vertices = doc.kernel().vertices.borrow_mut();
    for (a, b) in &shape.edges {
        let (side, other) = if role(a, "side") { (a, b) } else { (b, a) };
        let (s, e) = line(&side_curve(side));
        let curve = if *other == top {
            Curve3::Line {
                start: at(s, 20.0),
                end: at(e, 20.0),
            }
        } else if *other == bottom {
            Curve3::Line {
                start: at(s, 0.0),
                end: at(e, 0.0),
            }
        } else {
            // Two sides meet at the corner their lines share.
            let (s2, e2) = line(&side_curve(other));
            let corner = [s, e]
                .into_iter()
                .find(|p| *p == s2 || *p == e2)
                .expect("neighboring sides share a corner");
            for (z, cap) in [(20.0, &top), (0.0, &bottom)] {
                vertices.push((vertex(&[cap, side, other]), at(corner, z)));
            }
            Curve3::Line {
                start: at(corner, 0.0),
                end: at(corner, 20.0),
            }
        };
        curves.insert(edge(a, b), curve);
    }
    drop(curves);
    drop(vertices);
    Block {
        doc,
        top,
        bottom,
        sides,
    }
}

fn kinds(items: &[Value]) -> Vec<String> {
    let mut kinds: Vec<String> = items
        .iter()
        .map(|i| i["kind"].as_str().unwrap().to_owned())
        .collect();
    kinds.sort();
    kinds
}

fn sources(items: &[Value], kind: &str) -> BTreeSet<String> {
    items
        .iter()
        .filter(|i| i["kind"] == kind)
        .map(|i| i["source"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_sketch_on_the_top_face_has_its_four_edges_and_corners() {
    let Block {
        mut doc,
        top,
        bottom,
        sides,
    } = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": top.to_string(), "body": "F2.b0"}}),
    );
    let items = plane_geometry(&doc, "F3");
    assert_eq!(
        kinds(&items),
        [
            "line", "line", "line", "line", "vertex", "vertex", "vertex", "vertex"
        ],
        "{items:?}"
    );
    // The top face's edges, none of the sides' or the bottom's.
    let top_edges: BTreeSet<String> = sides.iter().map(|s| edge(&top, s)).collect();
    assert_eq!(sources(&items, "line"), top_edges);
    for item in &items {
        let source = item["source"].as_str().unwrap();
        assert!(!source.contains(&bottom.to_string()), "{source}");
        assert_eq!(item["body"], "F2.b0");
        assert!(item.get("occurrence").is_none(), "{item}");
    }
    // In sketch coordinates: the corners, and each line from one to another.
    let corners: Vec<[f64; 2]> = [[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]]
        .iter()
        .map(|c| in_sketch(&doc, "F3", [c[0], c[1], 20.0]))
        .collect();
    for item in items.iter().filter(|i| i["kind"] == "vertex") {
        let at = &item["geometry"]["at"];
        assert!(corners.iter().any(|c| near(at, *c)), "{item}");
        assert!(
            item["source"].as_str().unwrap().contains(&top.to_string()),
            "{item}"
        );
    }
    for item in items.iter().filter(|i| i["kind"] == "line") {
        let g = &item["geometry"];
        assert!(corners.iter().any(|c| near(&g["start"], *c)), "{item}");
        assert!(corners.iter().any(|c| near(&g["end"], *c)), "{item}");
    }
}

#[test]
fn a_sketch_one_millimetre_above_the_top_face_has_nothing() {
    let Block { mut doc, top, .. } = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": {"body": "F2.b0", "face": top.to_string()},
                              "distance": 1}}}),
    );
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "F3"}));
    let origin = query(&doc, json!({"query": "sketch", "uid": "F4"}))["frame"]["origin"].clone();
    assert!(close(origin[2].as_f64().unwrap(), 21.0), "{origin}");
    assert_eq!(plane_geometry(&doc, "F4"), Vec::<Value>::new());
}

#[test]
fn a_hidden_body_has_nothing_to_pick() {
    let Block { mut doc, top, .. } = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": top.to_string(), "body": "F2.b0"}}),
    );
    assert_eq!(plane_geometry(&doc, "F3").len(), 8);
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}),
    );
    assert_eq!(plane_geometry(&doc, "F3"), Vec::<Value>::new());
}

#[test]
fn a_circular_edge_in_the_plane_comes_with_its_centre() {
    // A cylinder of radius 10 at (30, 20), 10 high: its top circle is in the
    // plane of a sketch on its top face, the bottom circle and the seam
    // are not.
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let added = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [30, 20], "diameter": 20}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": added["region"]}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    let shape = doc
        .body_shape(BodyUid::new(FeatureUid(2), 0))
        .expect("Body1")
        .clone();
    let find = |role: &str| shape.faces.iter().find(|f| f.role == role).unwrap().clone();
    let (top, bottom, side) = (find("end"), find("start"), find("side"));
    let circle = |z: f64| Curve3::Conic {
        center: [30.0, 20.0, z],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: 10.0,
        minor: 10.0,
        start: 0.0,
        end: std::f64::consts::TAU,
        closed: true,
    };
    {
        let mut curves = doc.kernel().named_curves.borrow_mut();
        curves.insert(edge(&top, &side), circle(10.0));
        curves.insert(edge(&bottom, &side), circle(0.0));
        curves.insert(
            edge(&side, &side),
            Curve3::Line {
                start: [40.0, 20.0, 0.0],
                end: [40.0, 20.0, 10.0],
            },
        );
        let mut vertices = doc.kernel().vertices.borrow_mut();
        vertices.push((vertex(&[&top, &side]), [40.0, 20.0, 10.0]));
        vertices.push((vertex(&[&bottom, &side]), [40.0, 20.0, 0.0]));
    }
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": top.to_string(), "body": "F2.b0"}}),
    );
    let items = plane_geometry(&doc, "F3");
    assert_eq!(kinds(&items), ["circle", "vertex"], "{items:?}");
    let circle = items.iter().find(|i| i["kind"] == "circle").unwrap();
    assert_eq!(circle["source"], edge(&top, &side));
    assert!(
        near(
            &circle["geometry"]["center"],
            in_sketch(&doc, "F3", [30.0, 20.0, 10.0])
        ),
        "{circle}"
    );
    assert!(
        close(circle["geometry"]["radius"].as_f64().unwrap(), 10.0),
        "{circle}"
    );
    let seam = items.iter().find(|i| i["kind"] == "vertex").unwrap();
    assert_eq!(seam["source"], vertex(&[&top, &side]));
    assert!(near(
        &seam["geometry"]["at"],
        in_sketch(&doc, "F3", [40.0, 20.0, 10.0])
    ));
}

/// The block as a plate with a cylinder of diameter 30 on its top
/// (Sketch2 F3 on the top face, Extrude2 F4 10 up, Body2 F4.b0) and
/// Sketch3 (F5) on the cylinder's top, 10 above the plate (mitcad#866):
/// the mock kernel knows the plate's edges and corners and the cylinder's
/// two circles and its seam.
struct Plate {
    doc: Document<MockKernel>,
    top: FaceName,
    sides: Vec<FaceName>,
    /// The cylinder's faces: its top, its bottom and its side.
    cylinder: [FaceName; 3],
}

fn plate_with_cylinder() -> Plate {
    let Block {
        mut doc,
        top,
        sides,
        ..
    } = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": top.to_string(), "body": "F2.b0"}}),
    );
    let centre = in_sketch(&doc, "F3", [30.0, 20.0, 20.0]);
    let added = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": centre, "diameter": 30}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": added["region"]}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    let shape = doc
        .body_shape(BodyUid::new(FeatureUid(4), 0))
        .expect("Body2")
        .clone();
    let find = |role: &str| shape.faces.iter().find(|f| f.role == role).unwrap().clone();
    let (end, start, side) = (find("end"), find("start"), find("side"));
    let circle = |z: f64| Curve3::Conic {
        center: [30.0, 20.0, z],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: 15.0,
        minor: 15.0,
        start: 0.0,
        end: std::f64::consts::TAU,
        closed: true,
    };
    {
        let mut curves = doc.kernel().named_curves.borrow_mut();
        curves.insert(edge(&end, &side), circle(30.0));
        curves.insert(edge(&start, &side), circle(20.0));
        curves.insert(
            edge(&side, &side),
            Curve3::Line {
                start: [45.0, 20.0, 20.0],
                end: [45.0, 20.0, 30.0],
            },
        );
    }
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": end.to_string(), "body": "F4.b0"}}),
    );
    Plate {
        doc,
        top,
        sides,
        cylinder: [end, start, side],
    }
}

fn plane_geometry_all(doc: &Document<MockKernel>, sketch: &str) -> Vec<Value> {
    query(
        doc,
        json!({"query": "plane_geometry", "sketch": sketch, "out_of_plane": true}),
    )
    .as_array()
    .unwrap()
    .clone()
}

/// The top edge of the plate over the side whose line runs along x = `x`.
fn plate_edge_at_x(plate: &Plate, x: f64) -> String {
    let curves = plate.doc.kernel().named_curves.borrow();
    plate
        .sides
        .iter()
        .map(|s| edge(&plate.top, s))
        .find(|name| {
            matches!(curves.get(name), Some(Curve3::Line { start, end })
                if close(start[0], x) && close(end[0], x))
        })
        .unwrap_or_else(|| panic!("no top edge at x = {x}"))
}

#[test]
fn edges_and_vertices_out_of_the_plane_come_projected_on_request() {
    let plate = plate_with_cylinder();
    let doc = &plate.doc;
    let [end, start, side] = &plate.cylinder;
    // Without `out_of_plane`: the cylinder's top circle only, as before.
    let in_plane = plane_geometry(doc, "F5");
    assert_eq!(kinds(&in_plane), ["circle"], "{in_plane:?}");
    assert_eq!(in_plane[0]["source"], edge(end, side));
    assert!(in_plane[0].get("in_plane").is_none(), "{in_plane:?}");
    // With it: the same first, then the others, projected.
    let items = plane_geometry_all(doc, "F5");
    assert_eq!(items[0], in_plane[0], "{items:?}");
    assert!(
        items[1..].iter().all(|i| i["in_plane"] == false),
        "{items:?}"
    );
    let find = |source: &str| {
        items
            .iter()
            .find(|i| i["source"] == source)
            .unwrap_or_else(|| panic!("no {source} in {items:?}"))
    };
    // A plate edge 10 below: its line where it is seen from above.
    let right = find(&plate_edge_at_x(&plate, 60.0));
    assert_eq!(right["kind"], "line", "{right}");
    assert_eq!(right["body"], "F2.b0");
    let ends = [
        in_sketch(doc, "F5", [60.0, 0.0, 30.0]),
        in_sketch(doc, "F5", [60.0, 40.0, 30.0]),
    ];
    for key in ["start", "end"] {
        assert!(
            ends.iter().any(|e| near(&right["geometry"][key], *e)),
            "{right}"
        );
    }
    assert!(close(right["height"].as_f64().unwrap(), -10.0), "{right}");
    let path = right["path"].as_array().unwrap();
    assert_eq!(path.len(), 2, "{right}");
    for p in path {
        assert!(close(p[2].as_f64().unwrap(), -10.0), "{right}");
        assert!(ends.iter().any(|e| near(p, *e)), "{right}");
    }
    // The cylinder's bottom circle: the same circle, 10 below.
    let bottom = find(&edge(start, side));
    assert_eq!(bottom["kind"], "circle", "{bottom}");
    assert!(
        near(
            &bottom["geometry"]["center"],
            in_sketch(doc, "F5", [30.0, 20.0, 30.0])
        ),
        "{bottom}"
    );
    assert!(
        close(bottom["geometry"]["radius"].as_f64().unwrap(), 15.0),
        "{bottom}"
    );
    assert!(close(bottom["height"].as_f64().unwrap(), -10.0), "{bottom}");
    assert_eq!(bottom["path"].as_array().unwrap().len(), 25, "{bottom}");
    // The seam runs along the normal: the point it is seen as.
    let seam = find(&edge(side, side));
    assert_eq!(seam["kind"], "point", "{seam}");
    assert!(
        near(
            &seam["geometry"]["at"],
            in_sketch(doc, "F5", [45.0, 20.0, 30.0])
        ),
        "{seam}"
    );
    // A plate corner on the bottom, 30 below.
    let corner = items
        .iter()
        .find(|i| {
            i["kind"] == "vertex"
                && i["body"] == "F2.b0"
                && close(i["height"].as_f64().unwrap(), -30.0)
                && near(&i["geometry"]["at"], in_sketch(doc, "F5", [0.0, 0.0, 30.0]))
        })
        .unwrap_or_else(|| panic!("no bottom corner at the origin in {items:?}"));
    assert_eq!(corner["path"].as_array().unwrap().len(), 1, "{corner}");
}

/// The signed distance of `p` from the line through `a` and `b`.
fn from_line(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0].hypot(d[1]);
    (d[0] * (p[1] - a[1]) - d[1] * (p[0] - a[0])) / length
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

#[test]
fn a_distance_to_a_plate_edge_below_follows_the_plate() {
    // GitHub #50: a sketch on the cylinder's top, a circle on it, and a
    // distance from the circle's centre to a plate edge below.
    let plate = plate_with_cylinder();
    let source = plate_edge_at_x(&plate, 60.0);
    let mut doc = plate.doc;
    let at = in_sketch(&doc, "F5", [20.0, 10.0, 30.0]);
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch.circle", "sketch": "F5", "mode": "center", "center": at,
               "radius": 3}),
    );
    let circle = circle["made"][0].as_str().unwrap().to_owned();
    let entity = |doc: &Document<MockKernel>, id: &str| -> Value {
        query(doc, json!({"query": "sketch", "uid": "F5"}))["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("no {id} in Sketch3"))
            .clone()
    };
    let centre = entity(&doc, &circle)["center"].as_str().unwrap().to_owned();
    // What the sketch Dimension tool picks and projects: the edge out of
    // the plane, linked, as `sketch.project` makes it.
    let item = plane_geometry_all(&doc, "F5")
        .into_iter()
        .find(|i| i["source"] == source.as_str())
        .expect("the plate edge is offered");
    assert_eq!(item["in_plane"], false);
    let projected = command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": "F5", "source": source, "body": "F2.b0",
               "linked": true}),
    );
    let line = projected["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| entity(&doc, id.as_str().unwrap()))
        .find(|e| e["type"] == "line")
        .unwrap_or_else(|| panic!("no projected line in {projected}"));
    let line_id = line["id"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F5",
               "dimension": {"type": "point_line_distance", "point": centre, "line": line_id},
               "value": 15}),
    );
    let s = query(&doc, json!({"query": "sketch", "uid": "F5"}));
    assert_eq!(s["projections"].as_array().unwrap().len(), 1, "{s}");
    assert_eq!(s["projections"][0]["source"], source.as_str(), "{s}");
    assert_eq!(s["projections"][0]["by_geometry"], true, "{s}");
    let measure = |doc: &Document<MockKernel>| -> ([f64; 2], [f64; 2], f64) {
        let line = entity(doc, &line_id);
        let (a, b) = (xy(&line["geometry"]["start"]), xy(&line["geometry"]["end"]));
        let p = xy(&entity(doc, &centre)["at"]);
        (a, b, from_line(p, a, b))
    };
    let (a, b, before) = measure(&doc);
    assert!(close(before.abs(), 15.0), "{before}");
    let ends = |doc: &Document<MockKernel>, x: f64| {
        [
            in_sketch(doc, "F5", [x, 0.0, 30.0]),
            in_sketch(doc, "F5", [x, 40.0, 30.0]),
        ]
    };
    let on = |p: [f64; 2], q: [f64; 2]| close(p[0], q[0]) && close(p[1], q[1]);
    let at_x = |doc: &Document<MockKernel>, a: [f64; 2], b: [f64; 2], x: f64| {
        let [e0, e1] = ends(doc, x);
        (on(a, e0) && on(b, e1)) || (on(a, e1) && on(b, e0))
    };
    assert!(at_x(&doc, a, b, 60.0), "{a:?} {b:?}");
    // The plate made 80 wide: its right edge moves, and with it the
    // projection and the circle 15 from it, on the same side.
    {
        let mut curves = doc.kernel().named_curves.borrow_mut();
        curves.insert(
            source.clone(),
            Curve3::Line {
                start: [80.0, 0.0, 20.0],
                end: [80.0, 40.0, 20.0],
            },
        );
    }
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
    let (a, b, after) = measure(&doc);
    assert!(at_x(&doc, a, b, 80.0), "{a:?} {b:?}");
    assert!(close(after, before), "{after} vs {before}");
    let s = query(&doc, json!({"query": "sketch", "uid": "F5"}));
    assert_eq!(s["warnings"], json!([]), "{s}");
}

fn plane_geometry_key(doc: &Document<MockKernel>, sketch: &str) -> String {
    let answer = query(
        doc,
        json!({"query": "plane_geometry_key", "sketch": sketch}),
    );
    let key = answer["key"].as_str().unwrap_or_else(|| panic!("{answer}"));
    assert_eq!(key.len(), 32, "{answer}");
    key.to_owned()
}

#[test]
fn plane_geometry_key_stays_while_the_sketch_itself_is_edited() {
    // mitcad#1646: the Dimension tool read every edge of a large assembly
    // again after each dimension added; the key tells it nothing changed.
    let Block { mut doc, top, .. } = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": top.to_string(), "body": "F2.b0"}}),
    );
    let key = plane_geometry_key(&doc, "F3");
    let items = plane_geometry_all(&doc, "F3");
    let line = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [10, 10], "end": [30, 20]}),
    );
    let line = line["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .find(|id| id.starts_with('c'))
        .unwrap_or_else(|| panic!("no line in {line}"));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3",
               "dimension": {"type": "length", "line": line}, "value": 25}),
    );
    assert_eq!(plane_geometry_key(&doc, "F3"), key);
    assert_eq!(plane_geometry_all(&doc, "F3"), items);
}

#[test]
fn plane_geometry_key_changes_with_the_bodies_before_the_sketch() {
    let Block { mut doc, top, .. } = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": top.to_string(), "body": "F2.b0"}}),
    );
    let key = plane_geometry_key(&doc, "F3");
    // Hidden: nothing to pick, another key; shown again: the same answer
    // and key as before.
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}),
    );
    let hidden = plane_geometry_key(&doc, "F3");
    assert_ne!(hidden, key);
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": true}),
    );
    assert_eq!(plane_geometry_key(&doc, "F3"), key);
    // The block made 70 wide by Sketch1's width, before the sketch: Body1
    // is built again, another key.
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 70}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
    assert_ne!(plane_geometry_key(&doc, "F3"), key);
}

#[test]
fn plane_geometry_key_is_refused_as_plane_geometry_is() {
    let Block { doc, .. } = block();
    for sketch in ["F9", "F2"] {
        let key = doc.query(&json!({"query": "plane_geometry_key", "sketch": sketch}).to_string());
        let all = doc.query(&json!({"query": "plane_geometry", "sketch": sketch}).to_string());
        assert!(key.is_err() && all.is_err(), "{sketch}: {key:?} {all:?}");
    }
}
