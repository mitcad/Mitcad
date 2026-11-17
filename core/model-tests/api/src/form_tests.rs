// SPDX-License-Identifier: MIT
//! Forms through the JSON api on the mock kernel (mitcad#1757): a Form
//! started from a box cage, its cage queried and edited (moves, an
//! extrusion, an edge loop in and out, creases), each edit one undo step;
//! a symmetric cage whose halves stay equal; previews that change nothing;
//! refusals that leave no step; saved and read back; openings filled and
//! bridged (mitcad#1941).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was taken: {result}"),
        Err(error) => error.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn cage(doc: &Document<MockKernel>, form: &str) -> Value {
    query(doc, json!({"query": "form_cage", "uid": form}))
}

/// The faces of the Form's body.
fn faces(doc: &Document<MockKernel>, body: &str) -> Vec<String> {
    let uid: BodyUid = body.parse().unwrap();
    doc.body_shape(uid)
        .unwrap_or_else(|| panic!("no body {body}"))
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn position(cage: &Value, id: i64) -> [f64; 3] {
    let vertex = cage["vertices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap_or_else(|| panic!("vertex {id} in {cage}"));
    serde_json::from_value(vertex["at"].clone()).unwrap()
}

/// The cage face whose normal is nearest to `direction`.
fn face_towards(cage: &Value, direction: [f64; 3]) -> i64 {
    cage["faces"]
        .as_array()
        .unwrap()
        .iter()
        .max_by(|a, b| {
            let along = |f: &Value| {
                let n: [f64; 3] = serde_json::from_value(f["normal"].clone()).unwrap();
                n[0] * direction[0] + n[1] * direction[1] + n[2] * direction[2]
            };
            along(a).total_cmp(&along(b))
        })
        .unwrap()["id"]
        .as_i64()
        .unwrap()
}

/// Every vertex of the whole cage has its mirror image across x = 0.
fn assert_symmetric(cage: &Value) {
    let points: Vec<[f64; 3]> = cage["vertices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| serde_json::from_value(v["at"].clone()).unwrap())
        .collect();
    for p in &points {
        assert!(
            points.iter().any(|q| (q[0] + p[0]).abs() < 1e-9
                && (q[1] - p[1]).abs() < 1e-9
                && (q[2] - p[2]).abs() < 1e-9),
            "{p:?} has no mirror image in {cage}"
        );
    }
}

#[test]
fn a_form_starts_from_a_box_and_is_edited_step_by_step() {
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "form.create", "primitive": {"type": "box", "size": [40, 40, 40]}}),
    );
    assert_eq!(made["uid"], "F1");
    assert_eq!(made["name"], "Form1");
    assert_eq!(made["vertices"], 8);
    assert_eq!(made["faces"], 6);
    assert_eq!(made["closed"], true);
    assert!(made["error"].is_null(), "{made}");
    assert_eq!(undo_label(&doc), "Add Form1");
    // Four patches a cage face, named by the face and its corner.
    let body = faces(&doc, "F1.b0");
    assert_eq!(body.len(), 24);
    assert!(body.contains(&"F1:cage(1)#0".to_owned()), "{body:?}");

    // The cage as an editor draws and picks it.
    let before = cage(&doc, "F1");
    assert_eq!(before["vertices"].as_array().unwrap().len(), 8);
    assert_eq!(before["edges"].as_array().unwrap().len(), 12);
    assert_eq!(before["faces"].as_array().unwrap().len(), 6);
    assert_eq!(before["closed"], true);
    assert_eq!(before["name"], "Form1");
    let top = face_towards(&before, [0.0, 0.0, 1.0]);
    let top_face = before["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == top)
        .unwrap()
        .clone();
    assert_eq!(top_face["name"], format!("F1:cage({top})"));

    // The top moved up: one step, undone and done again.
    let moved = command(
        &mut doc,
        json!({"cmd": "form.move", "form": "F1", "faces": [top], "by": [0, 0, 10]}),
    );
    assert_eq!(moved["moved"].as_array().unwrap().len(), 4);
    assert_eq!(undo_label(&doc), "Move in Form1");
    let corner = top_face["vertices"][0].as_i64().unwrap();
    assert_eq!(position(&cage(&doc, "F1"), corner)[2], 30.0);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(position(&cage(&doc, "F1"), corner)[2], 20.0);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(position(&cage(&doc, "F1"), corner)[2], 30.0);

    // Extruded: four side faces, the top keeps its id; the body follows.
    let extruded = command(
        &mut doc,
        json!({"cmd": "form.extrude", "form": "F1", "faces": [top], "distance": 15}),
    );
    assert_eq!(extruded["faces"].as_array().unwrap().len(), 4);
    assert_eq!(extruded["vertices"].as_array().unwrap().len(), 4);
    assert_eq!(undo_label(&doc), "Extrude in Form1");
    assert_eq!(faces(&doc, "F1.b0").len(), 40);
    let after = cage(&doc, "F1");
    assert_eq!(face_towards(&after, [0.0, 0.0, 1.0]), top);

    // An edge loop round the extruded part, then out again.
    let wall = extruded["faces"][0].as_i64().unwrap();
    let wall_face = after["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == wall)
        .unwrap()
        .clone();
    let vertical = [
        wall_face["vertices"][1].clone(),
        wall_face["vertices"][2].clone(),
    ];
    let inserted = command(
        &mut doc,
        json!({"cmd": "form.insert_loop", "form": "F1", "edge": vertical, "at": 0.5}),
    );
    assert_eq!(inserted["vertices"].as_array().unwrap().len(), 4);
    assert_eq!(undo_label(&doc), "Insert Edge Loop in Form1");
    assert_eq!(cage(&doc, "F1")["faces"].as_array().unwrap().len(), 14);
    let across = inserted["edges"][0].clone();
    let deleted = command(
        &mut doc,
        json!({"cmd": "form.delete_loop", "form": "F1", "edge": across}),
    );
    assert_eq!(deleted["removed_vertices"].as_array().unwrap().len(), 4);
    assert_eq!(cage(&doc, "F1")["faces"].as_array().unwrap().len(), 10);
}

#[test]
fn a_crease_is_kept_through_edits_saving_and_reading() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "form.create", "primitive": {"type": "box", "size": [40, 40, 40], "segments": [2, 2, 2]}}),
    );
    let before = cage(&doc, "F1");
    let edge = before["edges"][0]["vertices"].clone();
    command(
        &mut doc,
        json!({"cmd": "form.crease", "form": "F1", "edges": [edge]}),
    );
    assert_eq!(undo_label(&doc), "Crease in Form1");
    let creased = |cage: &Value| -> Vec<Value> {
        cage["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["creased"] == true)
            .cloned()
            .collect()
    };
    let now = creased(&cage(&doc, "F1"));
    assert_eq!(now.len(), 1);
    assert!(now[0].get("sharpness").is_none(), "{now:?}");
    // A semi-sharp crease gives its sharpness.
    command(
        &mut doc,
        json!({"cmd": "form.crease", "form": "F1", "edges": [edge], "sharpness": 2.5}),
    );
    assert_eq!(creased(&cage(&doc, "F1"))[0]["sharpness"], 2.5);
    // Another edit keeps it.
    let top = face_towards(&cage(&doc, "F1"), [0.0, 0.0, 1.0]);
    command(
        &mut doc,
        json!({"cmd": "form.move", "form": "F1", "faces": [top], "by": [0, 0, 5]}),
    );
    assert_eq!(creased(&cage(&doc, "F1")).len(), 1);
    // Saved and read again: the same definition, crease and all.
    let file = doc.to_json();
    let saved: Value = serde_json::from_str(&file).unwrap();
    assert_eq!(saved["features"][0]["type"], "form", "{saved}");
    let mut back = Document::from_json(&file, MockKernel::default()).unwrap();
    back.recompute();
    assert_eq!(cage(&back, "F1")["stored"], cage(&doc, "F1")["stored"]);
    assert_eq!(creased(&cage(&back, "F1")).len(), 1);
    let saved_again: Value = serde_json::from_str(&back.to_json()).unwrap();
    assert_eq!(saved_again["features"], saved["features"]);
    // Smooth again.
    command(
        &mut back,
        json!({"cmd": "form.crease", "form": "F1", "edges": [edge], "sharpness": 0}),
    );
    assert!(creased(&cage(&back, "F1")).is_empty());
}

#[test]
fn symmetry_keeps_both_halves_equal() {
    let mut doc = Document::new(MockKernel::default());
    // One segment along x is made two, so that vertices lie on the plane.
    let made = command(
        &mut doc,
        json!({"cmd": "form.create", "symmetry": "x",
               "primitive": {"type": "box", "size": [40, 20, 20]}}),
    );
    assert_eq!(made["faces"], 5);
    let whole = cage(&doc, "F1");
    assert_eq!(whole["faces"].as_array().unwrap().len(), 10);
    assert_eq!(whole["symmetry"]["normal"], json!([1.0, 0.0, 0.0]));
    assert_symmetric(&whole);
    // A vertex of the mirrored half dragged: its stored one follows as the
    // mirror image.
    let mirrored = whole["vertices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["mirror"] == true)
        .unwrap()
        .clone();
    let id = mirrored["id"].as_i64().unwrap();
    let at: [f64; 3] = serde_json::from_value(mirrored["at"].clone()).unwrap();
    command(
        &mut doc,
        json!({"cmd": "form.set_positions", "form": "F1",
               "positions": [{"vertex": id, "at": [at[0] - 5.0, at[1] + 2.0, at[2]]}]}),
    );
    assert_eq!(undo_label(&doc), "Drag in Form1");
    let dragged = cage(&doc, "F1");
    assert_eq!(
        position(&dragged, -id),
        [-(at[0] - 5.0), at[1] + 2.0, at[2]]
    );
    assert_symmetric(&dragged);
    // An extrusion of a face on the mirrored half extrudes both.
    let left = face_towards(&dragged, [-1.0, 0.0, 0.0]);
    assert!(left < 0, "{left}");
    command(
        &mut doc,
        json!({"cmd": "form.extrude", "form": "F1", "faces": [left], "distance": 10}),
    );
    let extruded = cage(&doc, "F1");
    assert_eq!(extruded["faces"].as_array().unwrap().len(), 18);
    assert_symmetric(&extruded);
    // The body has the mirrored half's faces.
    assert!(
        faces(&doc, "F1.b0")
            .iter()
            .any(|f| f.starts_with("F1:mirror("))
    );
    // The plane taken away: the cage is whole and stays as it is.
    command(
        &mut doc,
        json!({"cmd": "form.symmetry", "form": "F1", "plane": null}),
    );
    let whole = cage(&doc, "F1");
    assert!(whole["symmetry"].is_null());
    assert_eq!(whole["faces"].as_array().unwrap().len(), 18);
    assert!(
        whole["faces"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["mirror"] == false)
    );
    assert_symmetric(&whole);
}

#[test]
fn previews_change_nothing_and_refusals_leave_no_step() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "form.create", "primitive": {"type": "box", "size": [40, 40, 40]}}),
    );
    let before = cage(&doc, "F1");
    let top = face_towards(&before, [0.0, 0.0, 1.0]);
    let preview = command(
        &mut doc,
        json!({"cmd": "form.move", "form": "F1", "faces": [top], "by": [0, 0, 10], "preview": true}),
    );
    assert_eq!(preview["preview"], true);
    assert_eq!(preview["cage"]["faces"].as_array().unwrap().len(), 6);
    let limit = &preview["limit"];
    let positions = limit["positions"].as_array().unwrap().len();
    let triangles = limit["triangles"].as_array().unwrap().len();
    assert_eq!(triangles, 3 * 2 * 6 * 64);
    assert!(positions > 0 && positions.is_multiple_of(3));
    assert_eq!(limit["faces"].as_array().unwrap().len(), triangles / 3);
    assert_eq!(cage(&doc, "F1"), before);
    assert_eq!(undo_label(&doc), "Add Form1");

    // Refused: a mirrored face without a plane, a face that is not there,
    // a zero extrusion, an unknown command; nothing changes.
    let error = refused(
        &mut doc,
        json!({"cmd": "form.extrude", "form": "F1", "faces": [-top], "distance": 5}),
    );
    assert!(error.contains("no symmetry plane"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "form.move", "form": "F1", "faces": [99], "by": [1, 0, 0]}),
    );
    assert!(error.contains("not in the cage"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "form.extrude", "form": "F1", "faces": [top], "distance": 0}),
    );
    assert!(error.contains("must not be zero"), "{error}");
    let error = refused(&mut doc, json!({"cmd": "form.fly", "form": "F1"}));
    assert!(error.contains("form.create"), "{error}");
    // A Form command on another feature.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
               "length": 10, "width": 10, "height": 10, "operation": "new_body"}}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "form.move", "form": "F2", "faces": [1], "by": [1, 0, 0]}),
    );
    assert!(error.contains("is not a Form"), "{error}");
    assert_eq!(cage(&doc, "F1"), before);
    assert_eq!(undo_label(&doc), "Add Box1");
}

#[test]
fn the_limit_mesh_of_a_cage_given_and_other_primitives() {
    let mut doc = Document::new(MockKernel::default());
    for (primitive, closed) in [
        (
            json!({"type": "cylinder", "diameter": 20, "height": 30}),
            true,
        ),
        (json!({"type": "sphere", "diameter": 30}), true),
        (
            json!({"type": "plane", "size": [30, 20], "segments": [3, 2]}),
            false,
        ),
    ] {
        let made = command(
            &mut doc,
            json!({"cmd": "form.create", "primitive": primitive, "center": [100, 0, 0]}),
        );
        assert_eq!(made["closed"], closed, "{made}");
        assert!(made["error"].is_null(), "{made}");
    }
    // A plane is a surface body of its patches.
    assert_eq!(faces(&doc, "F3.b0").len(), 24);
    // A cage given whole, previewed without a Form.
    let stored = cage(&doc, "F1")["stored"].clone();
    let mesh = query(
        &doc,
        json!({"query": "form_limit", "cage": stored, "levels": 1}),
    );
    assert_eq!(mesh["closed"], true);
    assert_eq!(
        mesh["triangles"].as_array().unwrap().len(),
        query(
            &doc,
            json!({"query": "form_limit", "uid": "F1", "levels": 1})
        )["triangles"]
            .as_array()
            .unwrap()
            .len()
    );
    // A cage given whole makes a Form too.
    let made = command(
        &mut doc,
        json!({"cmd": "form.create", "primitive": {"type": "cage", "cage": stored}}),
    );
    assert_eq!(made["name"], "Form4");
}

/// A Form from a face of the model: the outline of a round extrusion's top,
/// a few points of its circle, as a face (a surface body) or a slab on it.
#[test]
fn a_form_starts_from_a_face_of_the_model() {
    use mitcad_model::kernel::Curve3;
    use mitcad_model::topo::EdgeName;

    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 60}),
    );
    let region = query(&doc, json!({"query": "sketch", "uid": "F1"}))["regions"][0]["key"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let top = format!("F2:end({region})");
    let edge: EdgeName = format!("E{{{top}|F2:side(c1)}}").parse().unwrap();
    doc.kernel().face_boundaries.borrow_mut().insert(
        top.clone(),
        vec![(
            edge,
            Curve3::Conic {
                center: [0.0, 0.0, 20.0],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: 30.0,
                minor: 30.0,
                start: 0.0,
                end: 2.0 * std::f64::consts::PI,
                closed: true,
            },
        )],
    );
    let slab = command(
        &mut doc,
        json!({"cmd": "form.create",
               "primitive": {"type": "face", "body": "F2.b0", "face": top, "thickness": 5}}),
    );
    assert_eq!(slab["closed"], true, "{slab}");
    assert_eq!(slab["vertices"], 16);
    assert_eq!(slab["faces"], 10);
    // On the circle, the top 5 above the face.
    let whole = cage(&doc, "F3");
    for vertex in whole["vertices"].as_array().unwrap() {
        let at: [f64; 3] = serde_json::from_value(vertex["at"].clone()).unwrap();
        assert!((at[0].hypot(at[1]) - 30.0).abs() < 1e-9, "{at:?}");
        assert!(at[2] == 20.0 || (at[2] - 25.0).abs() < 1e-12, "{at:?}");
    }
    let sheet = command(
        &mut doc,
        json!({"cmd": "form.create", "primitive": {"type": "face", "body": "F2.b0", "face": top}}),
    );
    assert_eq!(sheet["closed"], false, "{sheet}");
    assert_eq!(sheet["faces"], 1);
    // An eight-sided face: eight patches.
    assert_eq!(faces(&doc, "F4.b0").len(), 8);
}

/// The cage face of the top (facing up) whose centre is furthest along +x.
fn top_face_at_right(cage: &Value) -> Value {
    cage["faces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["normal"][2].as_f64().unwrap() > 0.99)
        .max_by(|a, b| {
            let x = |f: &Value| f["center"][0].as_f64().unwrap();
            x(a).total_cmp(&x(b))
        })
        .unwrap()
        .clone()
}

/// The first edge of the cage on its boundary.
fn boundary_edge(cage: &Value) -> Value {
    cage["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["boundary"] == true)
        .unwrap_or_else(|| panic!("no boundary edge in {cage}"))["vertices"]
        .clone()
}

/// mitcad#1941: a box's top deleted and the opening filled again, with one
/// face or with quads, each one undo step; a preview changes nothing.
#[test]
fn an_opening_is_filled_with_one_face_or_with_quads() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "form.create", "primitive": {"type": "box", "size": [40, 40, 40], "segments": [2, 2, 2]}}),
    );
    let before = cage(&doc, "F1");
    let tops: Vec<i64> = before["faces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["normal"][2].as_f64().unwrap() > 0.99)
        .map(|f| f["id"].as_i64().unwrap())
        .collect();
    assert_eq!(tops.len(), 4);
    command(
        &mut doc,
        json!({"cmd": "form.delete_faces", "form": "F1", "faces": tops}),
    );
    let open = cage(&doc, "F1");
    assert_eq!(open["closed"], false);
    assert_eq!(open["faces"].as_array().unwrap().len(), 20);
    let edge = boundary_edge(&open);

    // An edge with a face on both sides is refused in words.
    let inner = open["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["boundary"] == false)
        .unwrap()["vertices"]
        .clone();
    let error = refused(
        &mut doc,
        json!({"cmd": "form.fill_hole", "form": "F1", "edge": inner}),
    );
    assert!(error.contains("not on an opening"), "{error}");
    assert!(!error.contains('['), "{error}");

    // A preview changes nothing.
    let preview = command(
        &mut doc,
        json!({"cmd": "form.fill_hole", "form": "F1", "edge": edge, "preview": true}),
    );
    assert_eq!(preview["preview"], true);
    assert_eq!(preview["cage"]["closed"], true);
    assert_eq!(cage(&doc, "F1")["closed"], false);

    // One face of eight corners.
    let filled = command(
        &mut doc,
        json!({"cmd": "form.fill_hole", "form": "F1", "edge": edge}),
    );
    assert_eq!(undo_label(&doc), "Fill Hole in Form1");
    assert_eq!(filled["closed"], true);
    assert_eq!(filled["faces"].as_array().unwrap().len(), 1);
    let one = cage(&doc, "F1");
    assert_eq!(one["closed"], true);
    let face = one["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == filled["faces"][0])
        .unwrap()
        .clone();
    assert_eq!(face["vertices"].as_array().unwrap().len(), 8);
    assert!(face["normal"][2].as_f64().unwrap() > 0.99, "{face}");
    assert!(
        faces(&doc, "F1.b0")
            .iter()
            .any(|f| f.starts_with(&format!("F1:cage({})", face["id"])))
    );

    // Undone, then filled with quads: a grid of 2 x 2 round a new vertex.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(cage(&doc, "F1")["closed"], false);
    let quads = command(
        &mut doc,
        json!({"cmd": "form.fill_hole", "form": "F1", "edge": edge, "quads": true}),
    );
    assert_eq!(quads["closed"], true);
    assert_eq!(quads["faces"].as_array().unwrap().len(), 4);
    assert_eq!(quads["vertices"].as_array().unwrap().len(), 1);
    let middle = position(&cage(&doc, "F1"), quads["vertices"][0].as_i64().unwrap());
    assert_eq!(middle, [0.0, 0.0, 20.0]);
    let after = cage(&doc, "F1");
    assert_eq!(after["faces"].as_array().unwrap().len(), 24);
    assert_eq!(after["closed"], true);
}

/// mitcad#1941: two tubes (cylinders with one cap taken out each) in one
/// cage, their open ends bridged with a strip of quads.
#[test]
fn two_open_ends_are_bridged() {
    use mitcad_model::subdiv::primitive::cylinder_cage;
    use mitcad_model::subdiv::{CageFace, CageVertex};
    let mut tubes = cylinder_cage([0.0; 3], 20.0, 10.0, 8, 1).unwrap();
    let upper = cylinder_cage([0.0, 0.0, 30.0], 20.0, 10.0, 8, 1).unwrap();
    // The lower one's top cap (face 2) and the upper one's bottom cap
    // (face 1) go; the upper one's ids are 100 on.
    tubes.faces.retain(|f| f.id != 2);
    tubes
        .vertices
        .extend(upper.vertices.iter().map(|v| CageVertex {
            id: v.id + 100,
            at: v.at,
        }));
    tubes
        .faces
        .extend(upper.faces.iter().filter(|f| f.id != 1).map(|f| CageFace {
            id: f.id + 100,
            vertices: f.vertices.iter().map(|v| v + 100).collect(),
        }));
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "form.create", "primitive": {"type": "cage", "cage": tubes}}),
    );
    assert_eq!(made["closed"], false);
    // The two ends of other lengths: refused in words.
    let error = refused(
        &mut doc,
        json!({"cmd": "form.bridge", "form": "F1", "edges": [[9, 10], [10, 11], [101, 102]]}),
    );
    assert!(error.contains("as many edges"), "{error}");
    // One edge of each end, whole openings, two rows across.
    let bridged = command(
        &mut doc,
        json!({"cmd": "form.bridge", "form": "F1", "edges": [[9, 10], [101, 102]],
               "loops": true, "segments": 2}),
    );
    assert_eq!(undo_label(&doc), "Bridge in Form1");
    assert_eq!(bridged["closed"], true);
    assert_eq!(bridged["faces"].as_array().unwrap().len(), 16);
    assert_eq!(bridged["vertices"].as_array().unwrap().len(), 8);
    let after = cage(&doc, "F1");
    assert_eq!(after["closed"], true);
    for v in bridged["vertices"].as_array().unwrap() {
        let p = position(&after, v.as_i64().unwrap());
        assert!((p[2] - 15.0).abs() < 1e-9, "{p:?}");
    }
    // The body is one piece of the tubes and the strip.
    // Two caps of eight corners, eight patches each; four a quad.
    assert_eq!(faces(&doc, "F1.b0").len(), 2 * 8 + 4 * (8 + 8 + 16));
}

/// mitcad#1941: a symmetric Form fills the opening and its mirror image;
/// an opening across the plane is refused in words.
#[test]
fn a_symmetric_form_fills_both_sides() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "form.create", "symmetry": "x",
               "primitive": {"type": "box", "size": [40, 20, 20], "segments": [4, 1, 1]}}),
    );
    let whole = cage(&doc, "F1");
    assert_eq!(whole["faces"].as_array().unwrap().len(), 18);
    // The stored top face away from the plane.
    let outer = top_face_at_right(&whole);
    assert!(outer["id"].as_i64().unwrap() > 0, "{outer}");
    command(
        &mut doc,
        json!({"cmd": "form.delete_faces", "form": "F1", "faces": [outer["id"]]}),
    );
    let open = cage(&doc, "F1");
    assert_eq!(open["closed"], false);
    assert_eq!(open["faces"].as_array().unwrap().len(), 16);
    // Picked on the mirrored half: both openings are filled.
    let edge = [
        -outer["vertices"][0].as_i64().unwrap(),
        -outer["vertices"][1].as_i64().unwrap(),
    ];
    let filled = command(
        &mut doc,
        json!({"cmd": "form.fill_hole", "form": "F1", "edge": edge}),
    );
    assert_eq!(filled["closed"], true);
    let after = cage(&doc, "F1");
    assert_eq!(after["faces"].as_array().unwrap().len(), 18);
    assert_eq!(after["closed"], true);
    assert_symmetric(&after);

    // The top face beside the plane taken out: its opening runs across it.
    let inner = after["faces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["normal"][2].as_f64().unwrap() > 0.99)
        .find(|f| {
            let x = f["center"][0].as_f64().unwrap();
            x > 0.0 && x < 10.0
        })
        .unwrap()
        .clone();
    command(
        &mut doc,
        json!({"cmd": "form.delete_faces", "form": "F1", "faces": [inner["id"]]}),
    );
    let edge = boundary_edge(&cage(&doc, "F1"));
    let error = refused(
        &mut doc,
        json!({"cmd": "form.fill_hole", "form": "F1", "edge": edge}),
    );
    assert!(error.contains("crosses the symmetry plane"), "{error}");
    assert_eq!(undo_label(&doc), "Delete Faces in Form1");
}
