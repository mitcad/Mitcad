// SPDX-License-Identifier: MIT
//! Geometry for agents (mitcad#714) on the mock kernel: the `faces` and
//! `edges` queries with each item's geometry and box, their filters and
//! pages; the plane of one planar face from `measure`; the warning of an
//! angle of more than one turn where one turn is the most; and the boxes
//! of the bodies a split leaves in the `bodies` query that MCP's command
//! results read.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::kernel::BoundingBox;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn refused(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

/// A box F1 of 30 x 20 x 10 from the origin (body F1.b0), its faces on
/// its bounds: bottom, top, side0 (y 0), side1 (x 30), side2 (y 20) and
/// side3 (x 0), in that order. side1 is a cylinder of radius 4 here; the
/// areas are the box's.
fn box_document() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    doc.kernel().lists_geometry.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 30, "width": 20, "height": 10, "operation": "new_body"}}),
    );
    let kernel = doc.kernel();
    for (face, area) in [
        ("bottom", 600.0),
        ("top", 600.0),
        ("side0", 300.0),
        ("side1", 200.0),
        ("side2", 300.0),
        ("side3", 200.0),
    ] {
        kernel
            .face_areas
            .borrow_mut()
            .insert(format!("F1:{face}"), area);
    }
    kernel.surfaces.borrow_mut().insert(
        "F1:side1".to_owned(),
        SurfaceGeometry::Cylinder {
            origin: [30.0, 10.0, 5.0],
            axis: [0.0, 0.0, 1.0],
            radius: 4.0,
        },
    );
    doc
}

fn names(items: &Value) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["names"][0].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn faces_give_their_geometry_and_boxes_on_request() {
    let doc = box_document();
    // As before: names, surface and area only, as an array.
    let plain = query(&doc, json!({"query": "faces", "body": "F1.b0"}));
    assert_eq!(plain.as_array().unwrap().len(), 6);
    assert_eq!(
        plain[1],
        json!({"names": ["F1:top"], "surface": "plane", "area": 600.0})
    );
    assert_eq!(plain[3]["surface"], "cylinder");

    let faces = query(
        &doc,
        json!({"query": "faces", "body": "F1.b0", "geometry": true}),
    );
    let faces = faces.as_array().unwrap();
    assert_eq!(faces.len(), 6);
    let top = &faces[1];
    assert_eq!(top["names"], json!(["F1:top"]));
    assert_eq!(top["origin"], json!([15.0, 10.0, 10.0]));
    assert_eq!(top["normal"], json!([0.0, 0.0, 1.0]));
    assert_eq!(
        top["bbox"],
        json!({"min": [0.0, 0.0, 10.0], "max": [30.0, 20.0, 10.0]})
    );
    assert!(top.get("index").is_none(), "{top}");
    let side0 = &faces[2];
    assert_eq!(side0["normal"], json!([0.0, -1.0, 0.0]));
    assert_eq!(
        side0["bbox"],
        json!({"min": [0.0, 0.0, 0.0], "max": [30.0, 0.0, 10.0]})
    );
    let cylinder = &faces[3];
    assert_eq!(cylinder["surface"], "cylinder");
    assert_eq!(cylinder["origin"], json!([30.0, 10.0, 5.0]));
    assert_eq!(cylinder["axis"], json!([0.0, 0.0, 1.0]));
    assert_eq!(cylinder["radius"], 4.0);
    assert!(cylinder.get("normal").is_none(), "{cylinder}");
    assert_eq!(
        cylinder["bbox"],
        json!({"min": [30.0, 0.0, 0.0], "max": [30.0, 20.0, 10.0]})
    );
}

#[test]
fn faces_filter_by_surface_area_and_normal_in_pages() {
    let doc = box_document();
    let faces = |extra: Value| {
        let mut request = json!({"query": "faces", "body": "F1.b0"});
        for (key, value) in extra.as_object().unwrap() {
            request[key] = value.clone();
        }
        query(&doc, request)
    };
    // A filter or a page answers an object with the total and each face's
    // index in the body.
    let planes = faces(json!({"surface": "plane"}));
    assert_eq!(planes["total"], 5);
    assert_eq!(planes["offset"], 0);
    assert_eq!(
        names(&planes["faces"]),
        ["F1:bottom", "F1:top", "F1:side0", "F1:side2", "F1:side3"]
    );
    let page = faces(json!({"surface": "plane", "offset": 1, "limit": 2}));
    assert_eq!(page["total"], 5);
    assert_eq!(page["offset"], 1);
    assert_eq!(names(&page["faces"]), ["F1:top", "F1:side0"]);
    assert_eq!(page["faces"][1]["index"], 2);
    let past = faces(json!({"offset": 10}));
    assert_eq!(past["total"], 6);
    assert_eq!(past["faces"], json!([]));

    let large = faces(json!({"min_area": 250}));
    assert_eq!(large["total"], 4);
    let small = faces(json!({"max_area": 250}));
    assert_eq!(names(&small["faces"]), ["F1:side1", "F1:side3"]);
    let between = faces(json!({"min_area": 250, "max_area": 400}));
    assert_eq!(names(&between["faces"]), ["F1:side0", "F1:side2"]);

    // Planar faces facing a direction (any length), with the geometry.
    let up = faces(json!({"normal": [0, 0, 2], "geometry": true}));
    assert_eq!(up["total"], 1);
    assert_eq!(names(&up["faces"]), ["F1:top"]);
    assert_eq!(up["faces"][0]["index"], 1);
    assert_eq!(up["faces"][0]["origin"], json!([15.0, 10.0, 10.0]));
    // Within the tolerance (radians) only.
    let tilted = [0.0, 0.01_f64.sin(), 0.01_f64.cos()];
    assert_eq!(faces(json!({"normal": tilted}))["total"], 0);
    assert_eq!(
        faces(json!({"normal": tilted, "normal_tolerance": 0.02}))["total"],
        1
    );
    // A cylinder has no normal of its own: not matched.
    assert_eq!(faces(json!({"normal": [1, 0, 0]}))["total"], 0);

    let zero = refused(
        &doc,
        json!({"query": "faces", "body": "F1.b0", "normal": [0, 0, 0]}),
    );
    assert!(zero.contains("nonzero direction"), "{zero}");
    let negative = refused(
        &doc,
        json!({"query": "faces", "body": "F1.b0", "normal": [0, 0, 1], "normal_tolerance": -1}),
    );
    assert!(negative.contains("normal_tolerance"), "{negative}");
    let unknown = refused(&doc, json!({"query": "faces", "body": "F1.b0", "area": 5}));
    assert!(unknown.contains("unknown field"), "{unknown}");
}

#[test]
fn edges_give_their_geometry_and_boxes_with_filters_and_pages() {
    let doc = box_document();
    let plain = query(&doc, json!({"query": "edges", "body": "F1.b0"}));
    let plain = plain.as_array().unwrap();
    let first = plain[0]["name"].as_str().unwrap().to_owned();
    let second = plain[1]["name"].as_str().unwrap().to_owned();
    assert!(plain[0].get("start").is_none());
    let kernel = doc.kernel();
    kernel.edge_curves.borrow_mut().insert(
        second.clone(),
        CurveGeometry::Circle {
            center: [15.0, 10.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            radius: 5.0,
            start: [20.0, 10.0, 0.0],
            end: [20.0, 10.0, 0.0],
        },
    );
    kernel.item_boxes.borrow_mut().insert(
        second.clone(),
        BoundingBox {
            min: [10.0, 5.0, 0.0],
            max: [20.0, 15.0, 0.0],
        },
    );

    let edges = query(
        &doc,
        json!({"query": "edges", "body": "F1.b0", "geometry": true}),
    );
    // The mock's edges are lines along x from the low corner.
    assert_eq!(edges[0]["name"], first.as_str());
    assert_eq!(edges[0]["start"], json!([0.0, 0.0, 0.0]));
    assert_eq!(edges[0]["end"], json!([1.0, 0.0, 0.0]));
    assert_eq!(
        edges[0]["bbox"],
        json!({"min": [0.0, 0.0, 0.0], "max": [1.0, 0.0, 0.0]})
    );
    let circle = &edges[1];
    assert_eq!(circle["center"], json!([15.0, 10.0, 0.0]));
    assert_eq!(circle["radius"], 5.0);
    assert_eq!(circle["normal"], json!([0.0, 0.0, -1.0]));
    assert_eq!(
        circle["bbox"],
        json!({"min": [10.0, 5.0, 0.0], "max": [20.0, 15.0, 0.0]})
    );

    let page = query(
        &doc,
        json!({"query": "edges", "body": "F1.b0", "curve": "line", "offset": 1, "limit": 1,
               "geometry": true}),
    );
    assert_eq!(page["total"], plain.len());
    assert_eq!(page["edges"].as_array().unwrap().len(), 1);
    assert_eq!(page["edges"][0]["name"], second.as_str());
    assert_eq!(page["edges"][0]["index"], 1);
    assert_eq!(page["edges"][0]["radius"], 5.0);
    // The mock's edges have no length.
    let long = query(
        &doc,
        json!({"query": "edges", "body": "F1.b0", "min_length": 1}),
    );
    assert_eq!(long, json!({"total": 0, "offset": 0, "edges": []}));
}

#[test]
fn measure_gives_the_plane_of_one_planar_face() {
    let doc = box_document();
    doc.kernel().measures_selections.set(true);
    let top = query(
        &doc,
        json!({"query": "measure", "a": {"body": "F1.b0", "face": "F1:top"}}),
    );
    assert_eq!(top["a"]["kind"], "face");
    assert_eq!(
        top["a"]["plane"],
        json!({"origin": [15.0, 10.0, 10.0], "normal": [0.0, 0.0, 1.0]})
    );
    assert!(top["a"].get("axis").is_none(), "{top}");
    // A cylinder gives its axis, not a plane.
    let side = query(
        &doc,
        json!({"query": "measure", "a": {"body": "F1.b0", "face": "F1:side1"}}),
    );
    assert!(side["a"].get("plane").is_none(), "{side}");
    assert_eq!(side["a"]["axis"]["direction"], json!([0.0, 0.0, 1.0]));
    // Both of two faces.
    let two = query(
        &doc,
        json!({"query": "measure", "a": {"body": "F1.b0", "face": "F1:top"},
               "b": {"body": "F1.b0", "face": "F1:side0"}}),
    );
    assert_eq!(two["b"]["plane"]["normal"], json!([0.0, -1.0, 0.0]));
}

#[test]
fn a_bare_number_of_more_than_one_turn_in_a_rotation_warns() {
    let mut doc = box_document();
    let rotate = |angle: Value| {
        json!({"type": "move", "bodies": ["F1.b0"],
               "transform": {"type": "rotate", "axis": "x", "angle": angle}})
    };
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": rotate(json!(90))}),
    );
    let name = added["name"].as_str().unwrap().to_owned();
    let uid = added["uid"].as_str().unwrap().to_owned();
    let expected =
        format!("{name} angle: 90 is read as radians (5156.6°); write \"90 deg\" for degrees");
    assert_eq!(added["warnings"], json!([expected]), "{added}");
    let feature = query(&doc, json!({"query": "feature", "uid": uid}));
    assert_eq!(feature["status"], "warning");
    assert_eq!(feature["warnings"], json!([expected]));

    // Degrees as the warning says: none.
    let edited = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid, "def": rotate(json!("90 deg"))}),
    );
    assert!(edited.get("warnings").is_none(), "{edited}");
    let feature = query(&doc, json!({"query": "feature", "uid": uid}));
    assert_eq!(feature["status"], "ok");
    assert!(feature.get("warnings").is_none(), "{feature}");
    // Radians within a turn: none.
    let edited = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid, "def": rotate(json!(1.5))}),
    );
    assert!(edited.get("warnings").is_none(), "{edited}");
    // Degrees beyond a turn, written so: more than one turn.
    let edited = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid, "def": rotate(json!("720 deg"))}),
    );
    assert_eq!(
        edited["warnings"],
        json!([format!("{name} angle: 720° is more than one turn")])
    );
    let negative = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid, "def": rotate(json!(-7))}),
    );
    assert_eq!(
        negative["warnings"],
        json!([format!(
            "{name} angle: -7 is read as radians (-401.1°); write \"-7 deg\" for degrees"
        )])
    );
}

#[test]
fn only_a_typed_number_of_more_than_one_turn_warns() {
    // mitcad#1092: the warning is for a number read in the wrong unit; an
    // angle computed from parameters may pass a turn.
    let mut doc = box_document();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "step", "expression": "450", "unit": "deg"}),
    );
    let rotate = |angle: Value| {
        json!({"type": "move", "bodies": ["F1.b0"],
               "transform": {"type": "rotate", "axis": "x", "angle": angle}})
    };
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": rotate(json!("450 deg"))}),
    );
    let name = added["name"].as_str().unwrap().to_owned();
    let uid = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(
        added["warnings"],
        json!([format!("{name} angle: 450° is more than one turn")]),
        "{added}"
    );
    let plain = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid, "def": rotate(json!(90))}),
    );
    assert_eq!(
        plain["warnings"],
        json!([format!(
            "{name} angle: 90 is read as radians (5156.6°); write \"90 deg\" for degrees"
        )]),
        "{plain}"
    );
    for computed in ["step", "step + 30 deg"] {
        let edited = command(
            &mut doc,
            json!({"cmd": "edit_feature", "uid": uid, "def": rotate(json!(computed))}),
        );
        assert!(edited.get("warnings").is_none(), "{computed}: {edited}");
        let feature = query(&doc, json!({"query": "feature", "uid": uid}));
        assert_eq!(feature["status"], "ok", "{computed}: {feature}");
        assert!(feature.get("warnings").is_none(), "{computed}: {feature}");
    }
}

#[test]
fn construction_planes_stepping_past_a_turn_do_not_warn() {
    let mut doc = box_document();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "step", "expression": "400", "unit": "deg"}),
    );
    let mut planes = Vec::new();
    for angle in ["step", "step + 45 deg"] {
        let plane = command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "construction_plane",
                   "definition": {"type": "angle", "line": "x", "angle": angle,
                                  "plane": "xy"}}}),
        );
        assert!(plane.get("warnings").is_none(), "{angle}: {plane}");
        planes.push(plane["uid"].as_str().unwrap().to_owned());
    }
    // As when the design is opened: every feature computed again.
    command(&mut doc, json!({"cmd": "recompute"}));
    for uid in planes {
        let feature = query(&doc, json!({"query": "feature", "uid": uid}));
        assert_eq!(feature["status"], "ok", "{feature}");
        assert!(feature.get("warnings").is_none(), "{feature}");
    }
}

#[test]
fn a_helix_of_several_turns_does_not_warn() {
    let mut doc = box_document();
    let sketch = command(&mut doc, json!({"cmd": "sketch.create"}));
    let sketch = sketch["uid"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": sketch, "center": [40, 0], "diameter": 2}),
    );
    let helix = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "helix",
               "profiles": [{"sketch": sketch, "region": "r{c1}"}], "axis": "z", "pitch": 5,
               "revolutions": 4, "operation": "new_body"}}),
    );
    assert!(helix.get("warnings").is_none(), "{helix}");
    let feature = query(&doc, json!({"query": "feature", "uid": helix["uid"]}));
    assert_eq!(feature["status"], "ok", "{feature}");
}

#[test]
fn bodies_give_their_boxes_and_a_split_leaves_both_pieces_boxes() {
    let mut doc = box_document();
    doc.kernel().split_bounds.set(true);
    let plain = query(&doc, json!({"query": "bodies"}));
    assert!(plain[0].get("bbox").is_none(), "{plain}");
    let split = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "split_body", "bodies": ["F1.b0"],
               "tool": {"type": "origin_plane", "plane": "yz", "offset": 10}}}),
    );
    let piece = format!("{}.b0", split["uid"].as_str().unwrap());
    // What MCP's snapshots read (`mitcad_mcp::SNAPSHOT_QUERIES`).
    let bodies = query(
        &doc,
        json!({"query": "bodies", "versions": true, "bbox": true}),
    );
    let bodies = bodies.as_array().unwrap();
    assert_eq!(bodies.len(), 2);
    // The original keeps the piece behind the plane along its normal.
    assert_eq!(bodies[0]["uid"], "F1.b0");
    assert_eq!(
        bodies[0]["bbox"],
        json!({"min": [0.0, 0.0, 0.0], "max": [10.0, 20.0, 10.0]})
    );
    assert_eq!(bodies[1]["uid"], piece.as_str());
    assert_eq!(
        bodies[1]["bbox"],
        json!({"min": [10.0, 0.0, 0.0], "max": [30.0, 20.0, 10.0]})
    );
    assert!(bodies.iter().all(|b| b["version"].is_string()));
    // Without the other properties.
    assert!(bodies[0].get("volume").is_none());
}
