// SPDX-License-Identifier: MIT
//! Flex items (mitcad#1713) with the mock kernel, through the JSON api: an
//! item made with `add_feature`; a node added at an end and between two,
//! moved, attached to another face or set free, deleted and twisted; the
//! item and its own bend radius set; an item that begins at an end port;
//! a route with problems and a node whose face is gone, which warn with
//! their places and still make the item; the definition kept by the file.
//!
//! The mock kernel routes straight through the nodes, so the lengths here
//! are the distances between them; the routing itself is the geometry's
//! (`geometry.flex*`).

use serde_json::{Value, json};

use mitcad_model::flex::{EndPort, RouteProblem, RouteProblemKind};
use mitcad_model::notice::{NoticePlace, user_text_leaks};
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_path};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    doc.command(&value.to_string()).unwrap_err().to_string()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn flex(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "flex", "uid": uid}))
}

fn status<'a>(doc: &'a Document<MockKernel>, uid: &str) -> Option<&'a FeatureStatus> {
    doc.status(uid.parse().unwrap())
}

/// The last request of `op` the kernel got.
fn spec(doc: &Document<MockKernel>, op: &str) -> String {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .rev()
        .find(|line| line.starts_with(op))
        .cloned()
        .unwrap_or_default()
}

fn close(a: &Value, b: f64) -> bool {
    a.as_f64().is_some_and(|a| (a - b).abs() < 1e-9)
}

fn hose() -> Value {
    json!({"name": "Hose 10", "section": {"type": "round", "diameter": 10},
           "wall": 1.5, "bend_radius": 30})
}

fn flat_cable() -> Value {
    json!({"name": "Flat cable 12", "section": {"type": "rectangular", "width": 12, "height": 2},
           "bend_radius": 6, "max_twist": 0.05})
}

fn top() -> Value {
    json!({"body": "F1.b0", "face": "F1:top"})
}

/// A box 30 x 20 x 10 on XY at the origin (F1, body F1.b0), whose faces
/// the mock kernel finds points on.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 30, "width": 20, "height": 10, "operation": "new_body"}}),
    );
    doc
}

/// The block with a hose (F2) from a point above it to its top face.
fn hose_on_block() -> Document<MockKernel> {
    let mut doc = block();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [5, 5, 40]},
            {"id": "n2", "at": [15, 10, 12], "on": top()}]}}),
    );
    assert_eq!(added["uid"], "F2");
    doc
}

fn node_ids(answer: &Value) -> Vec<&str> {
    answer["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node.as_str().or(node["id"].as_str()).unwrap())
        .collect()
}

#[test]
fn an_item_is_routed_through_its_nodes_and_built() {
    let doc = hose_on_block();
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    assert!(doc.warnings("F2".parse().unwrap()).is_empty());
    // The node on the face is the face's point nearest to where it was
    // put; the block is the obstacle; a round section rests its radius
    // off the surface and bends one way as the other.
    let route = spec(&doc, "flex_route");
    assert_eq!(
        route,
        "flex_route F2 nodes [5,5,40; 15,10,10 on F1:top] obstacles 1 resting 5 \
         bend Some(30.0) Some(30.0)"
    );
    // A round section is a hollow round solid along the route.
    let solid = spec(&doc, "flex_solid");
    assert!(
        solid.starts_with("flex_solid F2 path F2:route1 round 10 wall Some(1.5) "),
        "{solid}"
    );
    assert_eq!(doc.kernel().count("pipe"), 0);

    let answer = flex(&doc, "F2");
    assert_eq!(answer["status"], "ok");
    assert_eq!(answer["name"], "Flex1");
    assert_eq!(answer["item"]["name"], "Hose 10");
    assert!(close(&answer["length"], 1025.0_f64.sqrt()), "{answer}");
    assert_eq!(
        answer["polyline"],
        json!([[5.0, 5.0, 40.0], [15.0, 10.0, 10.0]])
    );
    assert_eq!(answer["problems"], json!([]));
    assert_eq!(answer["slack"], Value::Null);
    let nodes = answer["nodes"].as_array().unwrap();
    assert_eq!(nodes[0]["support"], "free");
    assert_eq!(nodes[0]["along"], 0.0);
    assert_eq!(nodes[0]["ref"], Value::Null);
    assert_eq!(nodes[1]["support"], "face");
    assert_eq!(nodes[1]["point"], json!([15.0, 10.0, 10.0]));
    assert_eq!(nodes[1]["normal"], json!([0.0, 0.0, 1.0]));
    assert_eq!(nodes[1]["ref"], top());
    assert_eq!(nodes[1]["lost"], false);
    assert!(close(&nodes[1]["along"], 1025.0_f64.sqrt()));
    // The samples only when asked for.
    assert!(answer.get("frames").is_none());
    let framed = query(&doc, json!({"query": "flex", "uid": "F2", "frames": true}));
    assert_eq!(framed["frames"].as_array().unwrap().len(), 2);

    // The cut list's row, with the item's body.
    let items = query(&doc, json!({"query": "flex_items"}));
    assert_eq!(items.as_array().unwrap().len(), 1);
    assert_eq!(items[0]["uid"], "F2");
    assert_eq!(items[0]["item"], "Hose 10");
    assert_eq!(items[0]["body"], "F2.b0");
    assert!(close(&items[0]["length"], 1025.0_f64.sqrt()));
}

#[test]
fn nodes_are_added_at_the_end_and_between() {
    let mut doc = hose_on_block();
    let added = command(
        &mut doc,
        json!({"cmd": "flex.add_node", "flex": "F2", "at": [40, 10, 10]}),
    );
    assert_eq!(added["node"], "n3");
    assert_eq!(node_ids(&added), ["n1", "n2", "n3"]);
    // The item grew by the new stretch.
    assert!(close(&added["length"], 1025.0_f64.sqrt() + 25.0), "{added}");
    let between = command(
        &mut doc,
        json!({"cmd": "flex.add_node", "flex": "F2", "at": [5, 5, 20],
               "place": {"after": "n1"}}),
    );
    assert_eq!(between["node"], "n4");
    assert_eq!(node_ids(&between), ["n1", "n4", "n2", "n3"]);
    let first = command(
        &mut doc,
        json!({"cmd": "flex.add_node", "flex": "F2", "at": [5, 5, 60], "place": "start",
               "twist": 0.5}),
    );
    assert_eq!(node_ids(&first), ["n5", "n1", "n4", "n2", "n3"]);
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    // Each is one undo step.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(node_ids(&flex(&doc, "F2")), ["n1", "n2", "n3"]);
    // A node that is not there, an item that is not one: refused in words.
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.add_node", "flex": "F2", "at": [0, 0, 0], "place": {"after": "n9"}}),
    );
    assert!(error.starts_with("the item has no such node"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.add_node", "flex": "F1", "at": [0, 0, 0]}),
    );
    assert!(error.starts_with("Box1 is not a flex item"), "{error}");
    assert_eq!(node_ids(&flex(&doc, "F2")), ["n1", "n2", "n3"]);
}

#[test]
fn a_node_moves_and_slides_on_its_face() {
    let mut doc = hose_on_block();
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F2", "node": "n1", "at": [0, 0, 20]}),
    );
    let answer = flex(&doc, "F2");
    assert_eq!(answer["nodes"][0]["point"], json!([0.0, 0.0, 20.0]));
    // A node on a face stays on it: pulled past the face's edge it is the
    // face's nearest point.
    let moved = command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F2", "node": "n2", "at": [100, 10, 50]}),
    );
    assert_eq!(moved["node"], "n2");
    let answer = flex(&doc, "F2");
    assert_eq!(answer["nodes"][1]["point"], json!([30.0, 10.0, 10.0]));
    assert_eq!(answer["nodes"][1]["support"], "face");
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
}

#[test]
fn a_node_is_attached_to_another_face_or_set_free() {
    let mut doc = hose_on_block();
    // Onto the side at x = 30.
    command(
        &mut doc,
        json!({"cmd": "flex.attach_node", "flex": "F2", "node": "n2",
               "on": {"body": "F1.b0", "face": "F1:side1"}, "at": [33, 10, 4]}),
    );
    let answer = flex(&doc, "F2");
    assert_eq!(answer["nodes"][1]["point"], json!([30.0, 10.0, 4.0]));
    assert_eq!(answer["nodes"][1]["normal"], json!([1.0, 0.0, 0.0]));
    assert_eq!(answer["nodes"][1]["ref"]["face"], "F1:side1");
    // Free: it stays where it is.
    command(
        &mut doc,
        json!({"cmd": "flex.attach_node", "flex": "F2", "node": "n2", "free": true}),
    );
    let answer = flex(&doc, "F2");
    assert_eq!(answer["nodes"][1]["support"], "free");
    assert_eq!(answer["nodes"][1]["point"], json!([30.0, 10.0, 4.0]));
    assert_eq!(answer["nodes"][1]["normal"], Value::Null);
    // One of on, port and free.
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.attach_node", "flex": "F2", "node": "n2"}),
    );
    assert!(error.contains("one of on, port and free"), "{error}");
}

#[test]
fn a_deleted_node_leaves_the_route_to_its_neighbours() {
    let mut doc = hose_on_block();
    command(
        &mut doc,
        json!({"cmd": "flex.add_node", "flex": "F2", "at": [5, 5, 20],
               "place": {"after": "n1"}}),
    );
    let longer = flex(&doc, "F2")["length"].as_f64().unwrap();
    let deleted = command(
        &mut doc,
        json!({"cmd": "flex.delete_node", "flex": "F2", "node": "n3"}),
    );
    assert_eq!(node_ids(&deleted), ["n1", "n2"]);
    assert!(close(&deleted["length"], 1025.0_f64.sqrt()), "{deleted}");
    assert!(longer > 1025.0_f64.sqrt());
    // Two nodes are the least.
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.delete_node", "flex": "F2", "node": "n1"}),
    );
    assert!(error.contains("at least two nodes"), "{error}");
    assert_eq!(node_ids(&flex(&doc, "F2")), ["n1", "n2"]);
}

#[test]
fn a_route_with_problems_warns_and_still_makes_the_item() {
    let mut doc = block();
    *doc.kernel().flex_problems.borrow_mut() = vec![
        RouteProblem {
            kind: RouteProblemKind::PassesThrough,
            from: 20.0,
            to: 26.0,
            obstacle: Some(0),
            places: vec![NoticePlace::point([12.0, 8.0, 10.0])],
        },
        RouteProblem {
            kind: RouteProblemKind::BendTooTight,
            from: 30.0,
            to: 31.0,
            obstacle: None,
            places: vec![NoticePlace::Box {
                min: [14.0, 9.0, 9.0],
                max: [16.0, 11.0, 11.0],
            }],
        },
        RouteProblem {
            kind: RouteProblemKind::NoCleanRoute,
            from: 0.0,
            to: 32.0,
            obstacle: None,
            places: Vec::new(),
        },
    ];
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [5, 5, 40]},
            {"id": "n2", "at": [15, 10, 12], "on": top()}]}}),
    );
    // Not a failure: the item is there, with its warnings.
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    assert_eq!(doc.kernel().count("flex_solid"), 1);
    assert_eq!(
        query(&doc, json!({"query": "flex_items"}))[0]["body"],
        "F2.b0"
    );
    let uid = "F2".parse().unwrap();
    let warnings = doc.warnings(uid).to_vec();
    assert_eq!(
        warnings,
        [
            "the item passes through a body",
            "the item bends more tightly than its bend radius allows",
            "no route was found that passes through nothing within the bend radius; \
             the item is drawn on the nearest one",
        ]
    );
    // Plain words: no names, ids or coordinates; where it is, as places.
    for warning in &warnings {
        assert_eq!(user_text_leaks(warning), None, "{warning}");
    }
    let places = doc.warning_places(uid);
    assert_eq!(places.len(), 2, "{places:?}");
    assert_eq!(places[0].0, "the item passes through a body");
    assert_eq!(places[0].1, [NoticePlace::point([12.0, 8.0, 10.0])]);
    // Agents get the detail the messages leave out.
    let answer = flex(&doc, "F2");
    assert_eq!(answer["status"], "ok");
    let problems = answer["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 3);
    assert_eq!(problems[0]["kind"], "passes_through");
    assert_eq!(problems[0]["body"], "F1.b0");
    assert_eq!(problems[0]["from"], 20.0);
    assert_eq!(problems[0]["to"], 26.0);
    assert_eq!(
        problems[0]["places"],
        json!([{"kind": "point", "at": [12.0, 8.0, 10.0]}])
    );
    assert_eq!(problems[1]["kind"], "bend_too_tight");
    assert_eq!(problems[1]["places"][0]["kind"], "box");
    assert_eq!(problems[2]["kind"], "no_clean_route");
    assert_eq!(
        problems[2]["message"],
        "no route was found that passes through nothing within the bend radius; \
         the item is drawn on the nearest one"
    );
    // A command's answer carries the warnings too.
    let moved = command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F2", "node": "n1", "at": [5, 5, 41]}),
    );
    assert_eq!(moved["warnings"].as_array().unwrap().len(), 3);
}

#[test]
fn a_node_whose_face_is_gone_stays_where_it_was_and_warns() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [5, 5, 40]},
            {"id": "n2", "at": [15, 10, 12], "on": {"body": "F1.b0", "face": "F1:lid"}}]}}),
    );
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    let uid = "F2".parse().unwrap();
    assert_eq!(
        doc.warnings(uid),
        ["a face a node was on no longer exists; the node stays where it was"]
    );
    assert_eq!(
        doc.warning_places(uid)[0].1,
        [NoticePlace::point([15.0, 10.0, 12.0])]
    );
    let answer = flex(&doc, "F2");
    let node = &answer["nodes"][1];
    assert_eq!(node["point"], json!([15.0, 10.0, 12.0]));
    assert_eq!(node["lost"], true);
    assert_eq!(node["support"], "face");
    assert_eq!(node["normal"], Value::Null);
    let problem = &answer["problems"][0];
    assert_eq!(problem["kind"], "face_lost");
    assert_eq!(problem["node"], "n2");
    assert_eq!(problem["was"]["face"], "F1:lid");
    // The route goes through the node's last place.
    assert!(spec(&doc, "flex_route").contains("[5,5,40; 15,10,12]"));
    // Attached again, the warning goes.
    command(
        &mut doc,
        json!({"cmd": "flex.attach_node", "flex": "F2", "node": "n2", "on": top()}),
    );
    assert!(doc.warnings(uid).is_empty());
    let answer = flex(&doc, "F2");
    assert_eq!(answer["nodes"][1]["lost"], false);
    assert_eq!(answer["nodes"][1]["point"], json!([15.0, 10.0, 10.0]));
}

#[test]
fn an_item_begins_at_an_end_port() {
    let mut doc = hose_on_block();
    // The hose's end on the block's top is a port, pointing into the block.
    doc.kernel().end_ports.borrow_mut().insert(
        "F2:end".to_owned(),
        EndPort {
            position: [15.0, 10.0, 10.0],
            direction: [0.0, 0.0, -1.0],
            x_axis: [1.0, 0.0, 0.0],
            size: [10.0, 10.0],
        },
    );
    let ports = query(&doc, json!({"query": "end_ports"}));
    assert_eq!(ports.as_array().unwrap().len(), 1, "{ports}");
    assert_eq!(ports[0]["feature"], "F2");
    assert_eq!(ports[0]["kind"], "flex");
    assert_eq!(ports[0]["end"], "end");
    assert_eq!(ports[0]["ref"], json!({"body": "F2.b0", "face": "F2:end"}));
    assert_eq!(ports[0]["position"], json!([15.0, 10.0, 10.0]));
    assert_eq!(ports[0]["direction"], json!([0.0, 0.0, -1.0]));
    // Another item begins there: at the port's middle, leaving along it.
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 0], "port": ports[0]["ref"]},
            {"id": "n2", "at": [15, 10, -30]}]}}),
    );
    let uid = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(status(&doc, &uid), Some(&FeatureStatus::Ok));
    let route = spec(&doc, "flex_route");
    assert!(
        route.contains("nodes [15,10,10 along 0,0,-1; 15,10,-30]"),
        "{route}"
    );
    // The first item is an obstacle of the second.
    assert!(route.contains("obstacles 2"), "{route}");
    let answer = flex(&doc, &uid);
    assert_eq!(answer["nodes"][0]["support"], "port");
    assert_eq!(answer["nodes"][0]["point"], json!([15.0, 10.0, 10.0]));
    assert_eq!(answer["length"], 40.0);
    // A face that is no end of a pipe or of an item is no port, and a
    // port is only at an end.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 0], "port": top()},
            {"id": "n2", "at": [15, 10, -30]}]}}),
    );
    assert!(
        error.contains("is not an end of a pipe or of a flex item"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 0]},
            {"id": "n2", "at": [0, 0, 5], "port": {"body": "F2.b0", "face": "F2:end"}},
            {"id": "n3", "at": [15, 10, -30]}]}}),
    );
    assert!(error.contains("first or last node"), "{error}");
}

#[test]
fn a_flat_section_is_built_with_its_frames_and_twist() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": flat_cable(), "nodes": [
            {"id": "n1", "at": [2, 10, 12], "on": top()},
            {"id": "n2", "at": [28, 10, 12], "on": top()}]}}),
    );
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    // Half the height off the face; no bending over the edge.
    let route = spec(&doc, "flex_route");
    assert!(route.contains("resting 1 bend Some(6.0) None"), "{route}");
    let solid = spec(&doc, "flex_solid");
    assert_eq!(
        solid,
        "flex_solid F2 path F2:route1 rectangular wall None samples 2"
    );
    assert_eq!(doc.kernel().count("pipe"), 0);
    // Both nodes on one face: the stretch between them runs on it.
    let answer = flex(&doc, "F2");
    assert_eq!(
        answer["runs"],
        json!([{"from": 0.0, "to": 26.0, "body": "F1.b0", "face": "F1:top"}])
    );
    // The section turned at the second node.
    let twisted = command(
        &mut doc,
        json!({"cmd": "flex.set_twist", "flex": "F2", "node": "n2",
               "twist": 1.5}),
    );
    assert_eq!(twisted["node"], "n2");
    let route = spec(&doc, "flex_route");
    assert!(route.contains("28,10,10 on F1:top twist 1.5]"), "{route}");
    assert_eq!(flex(&doc, "F2")["nodes"][1]["twist"], 1.5);
}

#[test]
fn the_item_and_its_own_bend_radius_are_set() {
    let mut doc = hose_on_block();
    // Its own bend radius: a value of the feature.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "bend_radius": 12}),
    );
    assert!(spec(&doc, "flex_route").ends_with("bend Some(12.0) Some(12.0)"));
    let answer = flex(&doc, "F2");
    assert_eq!(answer["bend_radius"], 12.0);
    assert_eq!(answer["item"]["bend_radius"], 30.0);
    // A fixed length is reported against the routed one.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "fixed_length": 50}),
    );
    let answer = flex(&doc, "F2");
    assert_eq!(answer["fixed_length"], 50.0);
    assert!(
        close(&answer["slack"], 50.0 - 1025.0_f64.sqrt()),
        "{answer}"
    );
    // Null takes a value away: the type's radius again.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "bend_radius": null}),
    );
    assert!(spec(&doc, "flex_route").ends_with("bend Some(30.0) Some(30.0)"));
    assert_eq!(flex(&doc, "F2")["fixed_length"], 50.0);
    // Another item whole.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "item": flat_cable()}),
    );
    assert_eq!(flex(&doc, "F2")["item"]["name"], "Flat cable 12");
    assert!(spec(&doc, "flex_solid").contains(" rectangular "));
    // An item that cannot be built is refused, with nothing changed.
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2",
               "item": {"name": "Wire", "section": {"type": "round", "diameter": 2}}}),
    );
    assert!(
        error.contains("a round section needs a bend radius"),
        "{error}"
    );
    assert_eq!(flex(&doc, "F2")["item"]["name"], "Flat cable 12");
}

#[test]
fn a_previewed_item_gives_its_route() {
    let mut doc = block();
    let report: Value = serde_json::from_str(
        &doc.preview(
            &json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
                {"id": "n1", "at": [5, 5, 40]},
                {"id": "n2", "at": [15, 10, 12], "on": top()}]}})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    let answer = query(&doc, json!({"query": "flex", "preview": true}));
    assert_eq!(answer["status"], "preview");
    assert_eq!(
        answer["polyline"],
        json!([[5.0, 5.0, 40.0], [15.0, 10.0, 10.0]])
    );
    assert_eq!(node_ids(&answer), ["n1", "n2"]);
    // Nothing was added.
    assert_eq!(query(&doc, json!({"query": "flex_items"})), json!([]));
}

#[test]
fn the_file_keeps_the_item_and_its_nodes() {
    let mut doc = hose_on_block();
    command(
        &mut doc,
        json!({"cmd": "flex.set_twist", "flex": "F2", "node": "n2", "twist": 0.25}),
    );
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let stored = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "flex")
        .cloned()
        .expect("a flex item in the file");
    assert_eq!(stored["item"]["name"], "Hose 10");
    assert_eq!(stored["item"]["section"]["diameter"], 10.0);
    assert_eq!(stored["item"]["wall"], 1.5);
    assert_eq!(
        stored["nodes"],
        json!([{"id": "n1", "at": [5.0, 5.0, 40.0]},
               {"id": "n2", "at": [15.0, 10.0, 10.0], "on": top(), "twist": 0.25}])
    );
    // What is not set is not written.
    for key in ["profile", "bend_radius", "fixed_length", "links"] {
        assert!(stored.get(key).is_none(), "{key}: {stored}");
    }
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    reopened.kernel().box_faces.set(true);
    reopened.recompute();
    assert_eq!(status(&reopened, "F2"), Some(&FeatureStatus::Ok));
    assert!(close(&flex(&reopened, "F2")["length"], 1025.0_f64.sqrt()));
}

#[test]
fn definitions_that_cannot_be_are_refused() {
    let mut doc = block();
    let mut add = |nodes: Value, item: Value| {
        refused(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "flex", "item": item, "nodes": nodes}}),
        )
    };
    let error = add(json!([{"id": "n1", "at": [0, 0, 0]}]), hose());
    assert!(error.contains("at least two nodes"), "{error}");
    let error = add(
        json!([{"id": "n1", "at": [0, 0, 0]}, {"id": "n1", "at": [5, 0, 0]}]),
        hose(),
    );
    assert!(error.contains("two nodes have the same id"), "{error}");
    let error = add(
        json!([{"id": "n1", "at": [0, 0, 0]}, {"id": "n2", "at": [5, 0, 0]}]),
        json!({"name": "Hose", "section": {"type": "round", "diameter": 10},
               "wall": 6, "bend_radius": 30}),
    );
    assert!(error.contains("fills the section"), "{error}");
}

// --- The library of Flex types ----------------------------------------------

fn type_row<'a>(types: &'a Value, library: &str, id: &str) -> Option<&'a Value> {
    types["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["library"] == library && row["id"] == id)
}

#[test]
fn the_built_in_types_are_listed_and_taken_by_their_ids() {
    let mut doc = hose_on_block();
    let types = query(&doc, json!({"query": "flex_types"}));
    let first = &types["types"][0];
    assert_eq!(first["library"], "built_in");
    assert_eq!(first["built_in"], true);
    let hose_12 = type_row(&types, "built_in", "hose-12").expect("a 12 mm hose");
    assert_eq!(
        hose_12["section"],
        json!({"type": "round", "diameter": 12.0})
    );
    assert_eq!(hose_12["wall"], 2.0);
    assert!(hose_12["bend_radius"].as_f64().unwrap() > 12.0);
    assert!(type_row(&types, "built_in", "flat-cable-12").is_some());
    assert!(type_row(&types, "built_in", "strip-10").is_some());
    assert!(type_row(&types, "built_in", "wire-2").is_some());
    // Only the types whose name or id holds every word.
    let flat = query(&doc, json!({"query": "flex_types", "text": "Flat cable"}));
    let built_in: Vec<&str> = flat["types"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["built_in"] == true)
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(built_in, ["flat-cable-12", "flat-cable-25"]);

    // A type by its id is copied into the item, with where it came from.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "type": "hose-12"}),
    );
    let answer = flex(&doc, "F2");
    assert_eq!(answer["item"]["name"], "Hose 12 mm");
    assert_eq!(
        answer["item"]["origin"],
        json!({"library": "built_in", "id": "hose-12", "version": 1})
    );
    assert!(spec(&doc, "flex_solid").contains("round 12 wall Some(2.0)"));
    assert_eq!(
        query(&doc, json!({"query": "flex_items"}))[0]["type"],
        "hose-12"
    );
    // A type that is not there, and the item with a type: refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "type": "hose-13"}),
    );
    assert!(error.starts_with("there is no such flex type"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "type": "hose-10", "item": hose()}),
    );
    assert!(error.contains("not both"), "{error}");
    assert_eq!(flex(&doc, "F2")["item"]["name"], "Hose 12 mm");
}

#[test]
fn a_type_is_saved_edited_and_deleted_in_the_projects_library() {
    let mut doc = hose_on_block();
    let project = unique_temp_path("flex-types-project");
    std::fs::create_dir_all(&project).unwrap();
    let folder = project.to_string_lossy().into_owned();
    let file = project
        .join("print-library")
        .join("flex-types")
        .join("shop-hose.json");

    // The design's item saved as a type, its own bend radius as the type's.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "bend_radius": 42}),
    );
    let saved = command(
        &mut doc,
        json!({"cmd": "flex.save_type", "flex": "F2", "library": "project",
               "project": folder, "name": "Shop hose"}),
    );
    assert_eq!(saved["id"], "shop-hose");
    assert_eq!(saved["library"], "project");
    assert_eq!(saved["version"], 1);
    assert!(file.is_file(), "{}", file.display());
    let types = query(&doc, json!({"query": "flex_types", "project": folder}));
    let own = type_row(&types, "project", "shop-hose").expect("the saved type");
    assert_eq!(own["name"], "Shop hose");
    assert_eq!(own["built_in"], false);
    assert_eq!(own["bend_radius"], 42.0);
    assert_eq!(own["wall"], 1.5);
    assert!(own.get("origin").is_none(), "{own}");
    // Without the project's folder the project's types are not there.
    let types = query(&doc, json!({"query": "flex_types"}));
    assert!(type_row(&types, "project", "shop-hose").is_none());

    // A second one of the name gets the next free id; an id in use is
    // refused.
    let second = command(
        &mut doc,
        json!({"cmd": "flex.save_type", "library": "project", "project": folder,
               "name": "Shop hose", "section": {"type": "round", "diameter": 8},
               "bend_radius": 30}),
    );
    assert_eq!(second["id"], "shop-hose-2");
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.save_type", "library": "project", "project": folder,
               "id": "shop-hose", "name": "Other", "section": {"type": "round", "diameter": 8},
               "bend_radius": 30}),
    );
    assert!(error.contains("of that id already"), "{error}");
    // A type that cannot be built is refused, and so is one with no place.
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.save_type", "library": "project", "project": folder,
               "name": "Bare", "section": {"type": "round", "diameter": 8}}),
    );
    assert!(error.contains("needs a bend radius"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.save_type", "library": "project",
               "name": "Nowhere", "section": {"type": "round", "diameter": 8},
               "bend_radius": 30}),
    );
    assert!(error.contains("the design is in no project"), "{error}");

    // Edited: the fields given change, null takes one away, the version
    // grows.
    let edited = command(
        &mut doc,
        json!({"cmd": "flex.edit_type", "library": "project", "project": folder,
               "id": "shop-hose", "bend_radius": 80, "wall": null, "mass_per_length": 60}),
    );
    assert_eq!(edited["version"], 2);
    let types = query(&doc, json!({"query": "flex_types", "project": folder}));
    let own = type_row(&types, "project", "shop-hose").unwrap();
    assert_eq!(own["version"], 2);
    assert_eq!(own["bend_radius"], 80.0);
    assert!(own.get("wall").is_none(), "{own}");
    assert_eq!(own["mass_per_length"], 60.0);
    // The same again changes nothing.
    let again = command(
        &mut doc,
        json!({"cmd": "flex.edit_type", "library": "project", "project": folder,
               "id": "shop-hose", "bend_radius": 80}),
    );
    assert_eq!(again["version"], 2);

    // Taken into the item: a copy with its origin.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "type": "shop-hose",
               "library": "project", "project": folder, "bend_radius": null}),
    );
    let answer = flex(&doc, "F2");
    assert_eq!(
        answer["item"]["origin"],
        json!({"library": "project", "id": "shop-hose", "version": 2})
    );
    assert!(spec(&doc, "flex_route").ends_with("bend Some(80.0) Some(80.0)"));
    let items = query(&doc, json!({"query": "flex_items"}));
    assert_eq!(items[0]["type"], "shop-hose");
    assert!(close(&items[0]["mass"], 1025.0_f64.sqrt() / 1000.0 * 60.0));

    // A built-in type does not change.
    for cmd in ["flex.edit_type", "flex.delete_type"] {
        let mut request = json!({"cmd": cmd, "library": "built_in", "id": "hose-10"});
        if cmd == "flex.edit_type" {
            request["bend_radius"] = json!(5);
        }
        let error = refused(&mut doc, request);
        assert!(error.contains("a built-in flex type cannot"), "{error}");
    }

    // Deleted: the file goes, the design keeps its copy; undo brings the
    // file back.
    command(
        &mut doc,
        json!({"cmd": "flex.delete_type", "library": "project", "project": folder,
               "id": "shop-hose"}),
    );
    assert!(!file.exists());
    assert_eq!(flex(&doc, "F2")["item"]["name"], "Shop hose");
    let error = refused(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "type": "shop-hose",
               "library": "project", "project": folder}),
    );
    assert!(error.starts_with("there is no such flex type"), "{error}");
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(file.is_file());
    remove_temp(&project);
}

// --- The length as a parameter and in the parts list ------------------------

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn the_length_is_a_driven_parameter_that_expressions_use() {
    let mut doc = hose_on_block();
    let name = flex(&doc, "F2")["length_parameter"]
        .as_str()
        .expect("the item's length parameter")
        .to_owned();
    let length = parameter(&doc, &name);
    assert_eq!(length["kind"], "reference");
    assert_eq!(length["owner"], "F2");
    assert_eq!(length["comment"], "Flex1 length (driven)");
    assert!(close(&length["value"], 1025.0_f64.sqrt()), "{length}");
    // An expression uses it, and follows when the route changes.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "cut", "expression": format!("{name} + 20")}),
    );
    assert!(close(
        &parameter(&doc, "cut")["value"],
        1025.0_f64.sqrt() + 20.0
    ));
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F2", "node": "n1", "at": [5, 5, 50]}),
    );
    assert!(close(&parameter(&doc, &name)["value"], 1725.0_f64.sqrt()));
    assert!(close(
        &parameter(&doc, "cut")["value"],
        1725.0_f64.sqrt() + 20.0
    ));
    // The item keeps the one parameter through its edits.
    assert_eq!(flex(&doc, "F2")["length_parameter"], name.as_str());
    let own = query(&doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["owner"] == "F2" && p["kind"] == "reference")
        .count();
    assert_eq!(own, 1);
    // A parameter of another is not the item's length.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "length": "cut",
               "nodes": [{"id": "n1", "at": [0, 0, 50]}, {"id": "n2", "at": [0, 0, 90]}]}}),
    );
    assert!(error.contains("a parameter of its own"), "{error}");
    // The file keeps it, and the length is measured again when it opens.
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    reopened.kernel().box_faces.set(true);
    reopened.recompute();
    assert_eq!(flex(&reopened, "F2")["length_parameter"], name.as_str());
    assert!(close(
        &parameter(&reopened, &name)["value"],
        1725.0_f64.sqrt()
    ));
}

#[test]
fn an_item_is_a_row_of_the_parts_list_with_its_type_and_length() {
    let mut doc = hose_on_block();
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "type": "hose-12"}),
    );
    let list = query(&doc, json!({"query": "parts_list"}));
    let rows = list["rows"].as_array().unwrap();
    let row = rows
        .iter()
        .find(|row| row["body"] == "F2.b0")
        .expect("the item's row");
    // Designated by its item, with how much of it there is.
    assert_eq!(row["designation"], "Hose 12 mm");
    assert_eq!(row["flex"], "F2");
    assert_eq!(row["flex_type"], "hose-12");
    assert!(close(&row["length"], 1025.0_f64.sqrt()), "{row}");
    assert_eq!(row["quantity"], 1);
    // Another body's row has no length.
    let block = rows
        .iter()
        .find(|row| row["body"] == "F1.b0")
        .expect("the block's row");
    assert!(block.get("length").is_none(), "{block}");
    assert!(block.get("flex").is_none(), "{block}");
    // The cut list's row names the length's parameter too.
    let items = query(&doc, json!({"query": "flex_items"}));
    assert_eq!(
        items[0]["length_parameter"],
        flex(&doc, "F2")["length_parameter"]
    );
    assert!(items[0]["mass"].as_f64().unwrap() > 0.0);
}

// --- The item's material and appearance -------------------------------------

/// The material and the appearance of the body of the item F2.
fn look(doc: &Document<MockKernel>) -> (Option<String>, Option<String>) {
    let body = mitcad_model::ids::BodyUid::new("F2".parse().unwrap(), 0);
    let attributes = doc.body_attributes(body);
    (attributes.material, attributes.appearance)
}

fn looks(material: Option<&str>, appearance: Option<&str>) -> (Option<String>, Option<String>) {
    (material.map(str::to_owned), appearance.map(str::to_owned))
}

#[test]
fn the_items_material_and_appearance_go_to_its_body() {
    let mut doc = block();
    let mut item = hose();
    item["material"] = json!("abs");
    item["appearance"] = json!("plastic_red");
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": item, "nodes": [
            {"id": "n1", "at": [5, 5, 40]},
            {"id": "n2", "at": [15, 10, 12], "on": top()}]}}),
    );
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    assert_eq!(look(&doc), looks(Some("abs"), Some("plastic_red")));
    // A look set on the body by hand stays through the item's other edits.
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "steel_satin"}),
    );
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F2", "node": "n1", "at": [5, 5, 45]}),
    );
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "bend_radius": 12}),
    );
    assert_eq!(look(&doc), looks(Some("abs"), Some("steel_satin")));
    // An item that names another appearance gives the body that one.
    item["appearance"] = json!("paint_blue");
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "item": item}),
    );
    assert_eq!(look(&doc), looks(Some("abs"), Some("paint_blue")));
    // One undo step with the item.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(look(&doc), looks(Some("abs"), Some("steel_satin")));
    command(&mut doc, json!({"cmd": "redo"}));
    // An item that names none leaves the body its component's.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "item": hose()}),
    );
    assert_eq!(look(&doc), looks(None, None));
    // An id the design does not have is left out, and the item is made.
    let mut item = hose();
    item["material"] = json!("no_such_material");
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "item": item}),
    );
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    assert_eq!(look(&doc), looks(None, None));
}

// --- Obstacles ---------------------------------------------------------------

#[test]
fn the_bodies_of_other_components_are_obstacles_where_they_are_placed() {
    let mut doc = block();
    // A component of its own beside the block, with a box in it.
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket",
               "transform": {"translation": [100, 0, 0]}}),
    );
    let bracket = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 10, "width": 10, "height": 10, "operation": "new_body"}}),
    );
    let bracket_body = format!("{}.b0", bracket["uid"].as_str().unwrap());
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    // The route's problem is in the bracket: the second obstacle.
    *doc.kernel().flex_problems.borrow_mut() = vec![RouteProblem {
        kind: RouteProblemKind::PassesThrough,
        from: 1.0,
        to: 2.0,
        obstacle: Some(1),
        places: vec![NoticePlace::point([105.0, 5.0, 5.0])],
    }];
    let transforms = doc.kernel().count("transform");
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [5, 5, 40]},
            {"id": "n2", "at": [15, 10, 12], "on": top()}]}}),
    );
    let uid = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(status(&doc, &uid), Some(&FeatureStatus::Ok));
    // The block of the item's own component and the bracket, moved to
    // where its occurrence places it.
    let route = spec(&doc, "flex_route");
    assert!(route.contains("obstacles 2"), "{route}");
    assert!(doc.kernel().count("transform") > transforms);
    let answer = flex(&doc, &uid);
    assert_eq!(answer["problems"][0]["body"], bracket_body.as_str());
    doc.kernel().flex_problems.borrow_mut().clear();

    // An item of the other component meets the root's bodies: the block
    // and the first item.
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": made["component"]}),
    );
    let inner = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 20]},
            {"id": "n2", "at": [0, 0, 60]}]}}),
    );
    let inner = inner["uid"].as_str().unwrap().to_owned();
    assert_eq!(status(&doc, &inner), Some(&FeatureStatus::Ok));
    let route = spec(&doc, "flex_route");
    assert!(route.contains("obstacles 3"), "{route}");
}

// --- Where a node last was ---------------------------------------------------

/// The stored place (`at`) of the node `id` of the item F2.
fn stored_at(doc: &Document<MockKernel>, id: &str) -> Value {
    let def = doc.flex_def("F2".parse().unwrap()).unwrap();
    let node = def.nodes.iter().find(|node| node.id == id).unwrap();
    json!(node.at)
}

#[test]
fn a_node_is_remembered_where_it_last_was_on_its_face() {
    let mut doc = hose_on_block();
    // Put above the face, the node is on it, and that is what is kept.
    assert_eq!(stored_at(&doc, "n2"), json!([15.0, 10.0, 10.0]));
    // A free node is where it was put.
    assert_eq!(stored_at(&doc, "n1"), json!([5.0, 5.0, 40.0]));
    // The block grows: the node follows its face, and is remembered there.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F1", "def": {"type": "box", "plane": "xy",
               "corner": [0, 0], "length": 30, "width": 20, "height": 25,
               "operation": "new_body"}}),
    );
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    assert_eq!(
        flex(&doc, "F2")["nodes"][1]["point"],
        json!([15.0, 10.0, 25.0])
    );
    assert_eq!(stored_at(&doc, "n2"), json!([15.0, 10.0, 25.0]));
    // The face is gone: the node stays where it last was, not where it
    // was once put, and the item warns in plain words with the place.
    doc.kernel().box_faces.set(false);
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F2", "node": "n1", "at": [5, 5, 60]}),
    );
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    let answer = flex(&doc, "F2");
    assert_eq!(answer["nodes"][1]["lost"], true);
    assert_eq!(answer["nodes"][1]["point"], json!([15.0, 10.0, 25.0]));
    let uid = "F2".parse().unwrap();
    let warnings = doc.warnings(uid).to_vec();
    assert_eq!(
        warnings,
        ["a face a node was on no longer exists; the node stays where it was"]
    );
    assert_eq!(user_text_leaks(&warnings[0]), None);
    assert_eq!(
        doc.warning_places(uid)[0].1,
        [NoticePlace::point([15.0, 10.0, 25.0])]
    );
    assert_eq!(answer["problems"][0]["kind"], "face_lost");
    assert_eq!(answer["problems"][0]["node"], "n2");
    // A lost node keeps what it has, and is found again on its face.
    assert_eq!(stored_at(&doc, "n2"), json!([15.0, 10.0, 25.0]));
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F2", "node": "n1", "at": [5, 5, 61]}),
    );
    assert!(doc.warnings(uid).is_empty());
    assert_eq!(flex(&doc, "F2")["nodes"][1]["lost"], false);
}

#[test]
fn a_node_whose_port_is_gone_stays_where_the_port_was_and_warns() {
    let mut doc = hose_on_block();
    let port = EndPort {
        position: [15.0, 10.0, 10.0],
        direction: [0.0, 0.0, -1.0],
        x_axis: [1.0, 0.0, 0.0],
        size: [10.0, 10.0],
    };
    doc.kernel()
        .end_ports
        .borrow_mut()
        .insert("F2:end".to_owned(), port);
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 0], "port": {"body": "F2.b0", "face": "F2:end"}},
            {"id": "n2", "at": [15, 10, -30]}]}}),
    );
    let name = added["uid"].as_str().unwrap().to_owned();
    let uid = name.parse().unwrap();
    assert!(doc.warnings(uid).is_empty());
    // The node is remembered at the port's middle.
    let def = doc.flex_def(uid).unwrap();
    assert_eq!(def.nodes[0].at, [15.0, 10.0, 10.0]);
    // The port is gone: the kernel no longer finds it.
    doc.kernel().end_ports.borrow_mut().clear();
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": name, "node": "n2", "at": [15, 10, -40]}),
    );
    assert_eq!(status(&doc, &name), Some(&FeatureStatus::Ok));
    let warnings = doc.warnings(uid).to_vec();
    assert_eq!(
        warnings,
        ["the end the item was attached to no longer exists; the node stays where it was"]
    );
    assert_eq!(user_text_leaks(&warnings[0]), None);
    assert_eq!(
        doc.warning_places(uid)[0].1,
        [NoticePlace::point([15.0, 10.0, 10.0])]
    );
    let answer = flex(&doc, &name);
    assert_eq!(answer["nodes"][0]["lost"], true);
    assert_eq!(answer["nodes"][0]["support"], "port");
    assert_eq!(answer["nodes"][0]["point"], json!([15.0, 10.0, 10.0]));
    assert_eq!(answer["problems"][0]["kind"], "port_lost");
    assert_eq!(answer["problems"][0]["was"]["face"], "F2:end");
    assert_eq!(answer["length"], 50.0);
}

// --- Nodes through the design's changes --------------------------------------

/// A component Bracket placed 100 mm along x with a block 30 x 20 x 10 in
/// it, and the root active again: the component, its occurrence and the
/// block's body.
fn bracket(doc: &mut Document<MockKernel>) -> (String, String, String) {
    doc.kernel().box_faces.set(true);
    let made = command(
        doc,
        json!({"cmd": "create_component", "name": "Bracket",
               "transform": {"translation": [100, 0, 0]}}),
    );
    let block = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 30, "width": 20, "height": 10, "operation": "new_body"}}),
    );
    command(doc, json!({"cmd": "activate_component", "component": "C0"}));
    (
        made["component"].as_str().unwrap().to_owned(),
        made["occurrence"].as_str().unwrap().to_owned(),
        format!("{}.b0", block["uid"].as_str().unwrap()),
    )
}

#[test]
fn a_node_on_another_components_face_is_linked_and_follows_its_occurrence() {
    let mut doc = Document::new(MockKernel::default());
    let (_, occurrence, body) = bracket(&mut doc);
    let face = format!("{}:top", body.trim_end_matches(".b0"));
    // The definition says where the face was picked; it is stored as a
    // link of the body.
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [50, 5, 40]},
            {"id": "n2", "at": [115, 10, 12],
             "on": {"body": body, "face": face, "occurrence": occurrence}}]}}),
    );
    let uid = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(status(&doc, &uid), Some(&FeatureStatus::Ok));
    let answer = flex(&doc, &uid);
    // On the face where the occurrence places it, in the item's component.
    assert_eq!(answer["nodes"][1]["support"], "face");
    assert_eq!(answer["nodes"][1]["lost"], false);
    assert_eq!(answer["nodes"][1]["point"], json!([115.0, 10.0, 10.0]));
    assert_eq!(
        answer["nodes"][1]["ref"],
        json!({"body": body, "face": face})
    );
    let def = doc.flex_def(uid.parse().unwrap()).unwrap();
    assert_eq!(def.links.len(), 1);
    let link = def.links.values().next().unwrap();
    assert_eq!(link.source.0.len(), 1);
    // The linked block is one obstacle, not two.
    assert!(spec(&doc, "flex_route").contains("obstacles 1"));

    // The occurrence moves: the node goes with its face.
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": occurrence,
               "transform": {"translation": [100, 0, 50]}}),
    );
    assert_eq!(status(&doc, &uid), Some(&FeatureStatus::Ok));
    assert_eq!(
        flex(&doc, &uid)["nodes"][1]["point"],
        json!([115.0, 10.0, 60.0])
    );

    // A node added by its command the same way; set free, its link goes.
    let more = command(
        &mut doc,
        json!({"cmd": "flex.add_node", "flex": uid, "at": [125, 10, 62],
               "on": {"body": body, "face": face, "occurrence": occurrence}}),
    );
    assert_eq!(more["node"], "n3");
    assert_eq!(
        flex(&doc, &uid)["nodes"][2]["point"],
        json!([125.0, 10.0, 60.0])
    );
    for node in ["n2", "n3"] {
        command(
            &mut doc,
            json!({"cmd": "flex.attach_node", "flex": uid, "node": node, "free": true}),
        );
    }
    let def = doc.flex_def(uid.parse().unwrap()).unwrap();
    assert!(def.links.is_empty());
    assert_eq!(flex(&doc, &uid)["nodes"][2]["support"], "free");
    // Attached again through the occurrence, the link is back.
    command(
        &mut doc,
        json!({"cmd": "flex.attach_node", "flex": uid, "node": "n3",
               "on": {"body": body, "face": face, "occurrence": occurrence}}),
    );
    let def = doc.flex_def(uid.parse().unwrap()).unwrap();
    assert_eq!(def.links.len(), 1);
    assert_eq!(flex(&doc, &uid)["nodes"][2]["lost"], false);
}

#[test]
fn an_item_rolled_back_keeps_its_nodes_and_comes_back() {
    let mut doc = hose_on_block();
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    let answer = flex(&doc, "F2");
    assert_eq!(answer["status"], "rolled_back");
    assert_eq!(answer["item"]["name"], "Hose 10");
    assert_eq!(query(&doc, json!({"query": "flex_items"})), json!([]));
    assert_eq!(query(&doc, json!({"query": "end_ports"})), json!([]));
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    let answer = flex(&doc, "F2");
    assert_eq!(answer["status"], "ok");
    assert_eq!(node_ids(&answer), ["n1", "n2"]);
    assert_eq!(answer["nodes"][1]["point"], json!([15.0, 10.0, 10.0]));
}

#[test]
fn a_turned_design_turns_the_nodes_places() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [5, 0, 0]},
            {"id": "n2", "at": [5, 7, 40]}]}}),
    );
    let before = flex(&doc, "F1")["length"].as_f64().unwrap();
    command(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    );
    assert_eq!(status(&doc, "F1"), Some(&FeatureStatus::Ok));
    let def = doc.flex_def("F1".parse().unwrap()).unwrap();
    assert_eq!(def.nodes[0].at, [5.0, 0.0, 0.0]);
    // A quarter turn about x: y and z change places, one changes its sign.
    let at = def.nodes[1].at;
    assert_eq!(at[0], 5.0);
    assert!((at[1].abs() - 40.0).abs() < 1e-9, "{at:?}");
    assert!((at[2].abs() - 7.0).abs() < 1e-9, "{at:?}");
    assert!(close(&flex(&doc, "F1")["length"], before));
}

// --- End ports of pipes --------------------------------------------------------

/// A pipe 40 mm along x in the active component, with the ports the mock
/// kernel gives its ends; its uid.
fn pipe(doc: &mut Document<MockKernel>) -> String {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "pipe",
               "path": {"fixed_curves": [
                   {"type": "line", "start": [0, 0, 0], "end": [40, 0, 0]}]},
               "size": 8, "operation": "new_body"}}),
    );
    let uid = added["uid"].as_str().unwrap().to_owned();
    let mut ports = doc.kernel().end_ports.borrow_mut();
    ports.insert(
        format!("{uid}:start"),
        EndPort {
            position: [0.0, 0.0, 0.0],
            direction: [-1.0, 0.0, 0.0],
            x_axis: [0.0, 1.0, 0.0],
            size: [8.0, 8.0],
        },
    );
    ports.insert(
        format!("{uid}:end"),
        EndPort {
            position: [40.0, 0.0, 0.0],
            direction: [1.0, 0.0, 0.0],
            x_axis: [0.0, 1.0, 0.0],
            size: [8.0, 8.0],
        },
    );
    drop(ports);
    uid
}

#[test]
fn a_pipes_ends_are_the_ports_of_the_body_under_the_pointer() {
    let mut doc = block();
    let uid = pipe(&mut doc);
    assert_eq!(status(&doc, &uid), Some(&FeatureStatus::Ok));
    let body = format!("{uid}.b0");
    let ports = query(&doc, json!({"query": "end_ports"}));
    let ports = ports.as_array().unwrap();
    assert_eq!(ports.len(), 2, "{ports:?}");
    assert_eq!(ports[0]["kind"], "pipe");
    assert_eq!(ports[0]["feature"], uid.as_str());
    assert_eq!(ports[0]["end"], "start");
    assert_eq!(ports[1]["end"], "end");
    assert_eq!(
        ports[1]["ref"],
        json!({"body": body, "face": format!("{uid}:end")})
    );
    assert_eq!(ports[1]["position"], json!([40.0, 0.0, 0.0]));
    assert_eq!(ports[1]["direction"], json!([1.0, 0.0, 0.0]));
    assert_eq!(ports[1]["size"], json!([8.0, 8.0]));
    // Of the body under the pointer only: the pipe's two, the block's none,
    // and the block is not measured for them.
    let own = query(&doc, json!({"query": "end_ports", "body": body}));
    assert_eq!(own.as_array().unwrap().len(), 2);
    let measured = doc.kernel().count("end_port");
    let none = query(&doc, json!({"query": "end_ports", "body": "F1.b0"}));
    assert_eq!(none, json!([]));
    assert_eq!(doc.kernel().count("end_port"), measured);
    // An item begins at the pipe's end, leaving along it.
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 0], "port": ports[1]["ref"]},
            {"id": "n2", "at": [90, 0, 30]}]}}),
    );
    let item = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(status(&doc, &item), Some(&FeatureStatus::Ok));
    let route = spec(&doc, "flex_route");
    assert!(
        route.contains("nodes [40,0,0 along 1,0,0; 90,0,30]"),
        "{route}"
    );
    assert_eq!(flex(&doc, &item)["nodes"][0]["support"], "port");
}

#[test]
fn a_pipe_of_another_component_gives_its_ports_where_its_occurrence_is() {
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Tube",
               "transform": {"translation": [100, 0, 20]}}),
    );
    let occurrence = made["occurrence"].as_str().unwrap().to_owned();
    let uid = pipe(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let body = format!("{uid}.b0");
    // As the pipe's component has them.
    let own = query(&doc, json!({"query": "end_ports", "body": body}));
    assert_eq!(own[1]["position"], json!([40.0, 0.0, 0.0]));
    // Picked at its occurrence for an item of the root: where it is there.
    let seen = query(
        &doc,
        json!({"query": "end_ports", "body": body, "occurrence": occurrence}),
    );
    assert_eq!(seen.as_array().unwrap().len(), 2);
    assert_eq!(seen[1]["position"], json!([140.0, 0.0, 20.0]));
    assert_eq!(seen[1]["direction"], json!([1.0, 0.0, 0.0]));
    // The item begins there through the occurrence.
    let mut port = seen[1]["ref"].clone();
    port["occurrence"] = json!(occurrence);
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 0], "port": port},
            {"id": "n2", "at": [190, 0, 50]}]}}),
    );
    let item = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(status(&doc, &item), Some(&FeatureStatus::Ok));
    let answer = flex(&doc, &item);
    assert_eq!(answer["nodes"][0]["support"], "port");
    assert_eq!(answer["nodes"][0]["lost"], false);
}

// --- The route alone, and its tightest bend ------------------------------------

fn preview(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.preview(&value.to_string()).unwrap()).unwrap()
}

#[test]
fn a_preview_of_the_route_alone_builds_no_solid() {
    let mut doc = block();
    let solids = doc.kernel().count("flex_solid");
    let report = preview(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "route_only": true,
               "nodes": [{"id": "n1", "at": [5, 5, 40]},
                         {"id": "n2", "at": [15, 10, 12], "on": top()}]}}),
    );
    assert_eq!(report["status"], "ok", "{report}");
    // Routed, with the nodes where they are and the length; nothing built.
    assert!(doc.kernel().count("flex_route") >= 1);
    assert_eq!(doc.kernel().count("flex_solid"), solids);
    let answer = query(
        &doc,
        json!({"query": "flex", "preview": true, "frames": true}),
    );
    assert_eq!(answer["status"], "preview");
    assert!(close(&answer["length"], 1025.0_f64.sqrt()), "{answer}");
    assert_eq!(answer["nodes"][1]["point"], json!([15.0, 10.0, 10.0]));
    assert_eq!(answer["frames"].as_array().unwrap().len(), 2);
    assert_eq!(answer["problems"], json!([]));

    // A node of an item dragged: the item's definition with the node moved.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [5, 5, 40]},
            {"id": "n2", "at": [15, 10, 12], "on": top()}]}}),
    );
    let solids = doc.kernel().count("flex_solid");
    let report = preview(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": {"type": "flex", "item": hose(),
               "route_only": true,
               "nodes": [{"id": "n1", "at": [5, 5, 60]},
                         {"id": "n2", "at": [15, 10, 10], "on": top()}]}}),
    );
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(doc.kernel().count("flex_solid"), solids);
    let answer = query(&doc, json!({"query": "flex", "preview": true}));
    assert!(close(&answer["length"], 2625.0_f64.sqrt()), "{answer}");
    // The item itself is as it was.
    assert!(close(&flex(&doc, "F2")["length"], 1025.0_f64.sqrt()));

    // Only a preview: an item is not stored without its body.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "route_only": true,
               "nodes": [{"id": "n1", "at": [5, 5, 40]}, {"id": "n2", "at": [5, 5, 80]}]}}),
    );
    assert!(error.contains("route_only is for a previewed"), "{error}");
}

#[test]
fn the_tightest_bend_is_given_beside_the_radius_it_may_not_go_below() {
    let mut doc = Document::new(MockKernel::default());
    // Straight: no bend.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(), "nodes": [
            {"id": "n1", "at": [0, 0, 0]},
            {"id": "n2", "at": [30, 0, 0]},
            {"id": "n3", "at": [60, 0, 0]}]}}),
    );
    let answer = flex(&doc, "F1");
    assert_eq!(answer["smallest_bend"], Value::Null);
    assert_eq!(answer["bend_radius"], 30.0);
    // A right angle of two 30 mm legs: the circle through the three
    // samples, at the middle one.
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F1", "node": "n3", "at": [30, 30, 0]}),
    );
    let answer = flex(&doc, "F1");
    let bend = &answer["smallest_bend"];
    assert!(close(&bend["radius"], 450.0_f64.sqrt()), "{bend}");
    assert_eq!(bend["along"], 30.0);
    assert_eq!(bend["point"], json!([30.0, 0.0, 0.0]));
    // A round section bends one way only.
    assert_eq!(answer["smallest_bend_edge"], Value::Null);
}

// A flat item's bends over its flat side and over its edge apart
// (mitcad#1713: the badge beside a warning about the edge showed the flat
// bend). The mock's samples of free nodes in the XY plane have the normal
// +Z: a turn in that plane is over the edge alone, one in XZ over the flat
// side alone.
#[test]
fn a_flat_items_tightest_bends_are_given_over_its_flat_side_and_its_edge() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": flat_cable(), "nodes": [
            {"id": "n1", "at": [0, 0, 0]},
            {"id": "n2", "at": [30, 0, 0]},
            {"id": "n3", "at": [30, 30, 0]}]}}),
    );
    let answer = flex(&doc, "F1");
    assert_eq!(answer["smallest_bend"], Value::Null, "{answer}");
    let edge = &answer["smallest_bend_edge"];
    assert!(close(&edge["radius"], 450.0_f64.sqrt()), "{edge}");
    assert_eq!(edge["along"], 30.0);
    assert_eq!(edge["point"], json!([30.0, 0.0, 0.0]));
    command(
        &mut doc,
        json!({"cmd": "flex.move_node", "flex": "F1", "node": "n3", "at": [30, 0, 30]}),
    );
    let answer = flex(&doc, "F1");
    let flat = &answer["smallest_bend"];
    assert!(close(&flat["radius"], 450.0_f64.sqrt()), "{answer}");
    assert_eq!(flat["point"], json!([30.0, 0.0, 0.0]));
    assert_eq!(answer["smallest_bend_edge"], Value::Null, "{answer}");
}

// A bend over the edge too tight is its own warning, apart from one over
// the flat side.
#[test]
fn a_bend_over_the_edge_too_tight_is_its_own_warning() {
    let mut doc = block();
    *doc.kernel().flex_problems.borrow_mut() = vec![RouteProblem {
        kind: RouteProblemKind::EdgeBendTooTight,
        from: 10.0,
        to: 12.0,
        obstacle: None,
        places: vec![NoticePlace::point([12.0, 8.0, 10.0])],
    }];
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": flat_cable(), "nodes": [
            {"id": "n1", "at": [5, 5, 40]},
            {"id": "n2", "at": [15, 10, 12], "on": top()}]}}),
    );
    assert_eq!(status(&doc, "F2"), Some(&FeatureStatus::Ok));
    let uid = "F2".parse().unwrap();
    let warnings = doc.warnings(uid).to_vec();
    assert_eq!(
        warnings,
        ["the item bends over its edge more tightly than its type allows"]
    );
    assert_eq!(user_text_leaks(&warnings[0]), None);
    assert_eq!(
        doc.warning_places(uid)[0].1,
        [NoticePlace::point([12.0, 8.0, 10.0])]
    );
    let answer = flex(&doc, "F2");
    assert_eq!(answer["problems"][0]["kind"], "edge_bend_too_tight");
    assert_eq!(
        answer["problems"][0]["message"],
        "the item bends over its edge more tightly than its type allows"
    );
}
