// SPDX-License-Identifier: MIT
//! New content in sight (mitcad#1995, GitHub mitcad/Mitcad#430) with the
//! mock kernel: a command that adds to a component only hidden
//! occurrences place shows them again in its own undo step and names them
//! in its reply (`shown_parents`); content in sight, a hidden occurrence
//! that follows a Show When condition and a command that adds nothing
//! leave the light bulbs alone.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{ComponentUid, OccurrenceUid};
use mitcad_model::test_support::num;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

/// Fairlead (O1) in the root and Post (O2) in it, Post active, Fairlead
/// hidden: Post's light bulb on, out of sight all the same.
fn hidden_parent() -> (Document<MockKernel>, OccurrenceUid, OccurrenceUid) {
    let mut doc = Document::new(MockKernel::default());
    let (_, fairlead) = doc
        .create_component(Some("Fairlead"), Transform::IDENTITY, true)
        .unwrap();
    let (post, o2) = doc
        .create_component(Some("Post"), Transform::IDENTITY, true)
        .unwrap();
    doc.set_occurrence_visible(fairlead, false).unwrap();
    assert_eq!(doc.active_component(), post);
    assert!(!doc.component_in_sight(post));
    (doc, fairlead, o2)
}

/// A new body from a 10 x 10 square extruded 5 in the active component,
/// through the JSON command: its reply, and the undo depth before that
/// command (after the sketch's and the rectangle's own steps).
fn new_body(doc: &mut Document<MockKernel>) -> (Value, usize) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let depth = doc.undo_depth();
    let reply = command(
        doc,
        json!({"cmd": "add_feature", "name": "Block",
               "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                       "extent": {"type": "distance", "distance": 5},
                       "operation": "new_body"}}),
    );
    (reply, depth)
}

fn visible(doc: &Document<MockKernel>, o: OccurrenceUid) -> bool {
    doc.assembly().occurrence(o).unwrap().visible
}

#[test]
fn a_new_body_under_a_hidden_parent_shows_the_parent_in_its_undo_step() {
    let (mut doc, fairlead, o2) = hidden_parent();
    let (reply, depth) = new_body(&mut doc);
    assert_eq!(
        reply["shown_parents"],
        json!([{"occurrence": fairlead, "name": "Fairlead:1", "path": "Fairlead:1"}]),
        "{reply}"
    );
    assert!(visible(&doc, fairlead));
    assert!(visible(&doc, o2));
    // Its placed body is shown now.
    let shown = query(&doc, json!({"query": "instances"}));
    assert_eq!(shown.as_array().unwrap().len(), 1, "{shown}");
    assert_eq!(shown[0]["occurrence"], "Fairlead:1/Post:1");
    // The body and the light bulb are one undo step: the extrusion's,
    // with its label.
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.undo_label(), Some("Add Block"));
    doc.undo();
    assert!(!visible(&doc, fairlead), "undo hides it again");
    assert_eq!(
        query(&doc, json!({"query": "instances", "hidden": true})),
        json!([])
    );
}

#[test]
fn a_new_component_under_hidden_parents_shows_each_of_them() {
    let (mut doc, fairlead, o2) = hidden_parent();
    // Post's own light bulb off too: both are shown, the outermost first.
    doc.set_occurrence_visible(o2, false).unwrap();
    let depth = doc.undo_depth();
    let reply = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    );
    assert_eq!(reply["shown_parents"][0]["name"], "Fairlead:1", "{reply}");
    assert_eq!(reply["shown_parents"][1]["name"], "Post:1", "{reply}");
    assert_eq!(
        reply["shown_parents"][1]["path"], "Fairlead:1/Post:1",
        "{reply}"
    );
    assert!(visible(&doc, fairlead) && visible(&doc, o2));
    let bracket: ComponentUid = reply["component"].as_str().unwrap().parse().unwrap();
    assert!(doc.component_in_sight(bracket));
    assert_eq!(doc.undo_depth(), depth + 1, "one undo step");
    doc.undo();
    assert!(!visible(&doc, fairlead) && !visible(&doc, o2));
    assert!(doc.assembly().component(bracket).is_none());
}

#[test]
fn content_in_sight_leaves_the_light_bulbs_alone() {
    let mut doc = Document::new(MockKernel::default());
    let (_, o1) = doc
        .create_component(Some("Fairlead"), Transform::IDENTITY, true)
        .unwrap();
    // Another component hidden beside it: not on the way.
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, other) = doc
        .create_component(Some("Spare"), Transform::IDENTITY, false)
        .unwrap();
    doc.set_occurrence_visible(other, false).unwrap();
    let fairlead = doc.assembly().occurrence(o1).unwrap().component;
    doc.activate_component(fairlead).unwrap();
    let (reply, _) = new_body(&mut doc);
    assert!(reply.get("shown_parents").is_none(), "{reply}");
    assert!(!visible(&doc, other));
    assert_eq!(doc.undo_label(), Some("Add Block"));
}

#[test]
fn a_component_placed_in_sight_elsewhere_is_in_sight() {
    let (mut doc, fairlead, o2) = hidden_parent();
    // Post placed a second time in the root, shown: its content is seen
    // there.
    let post = doc.assembly().occurrence(o2).unwrap().component;
    doc.add_occurrence(post, ComponentUid::ROOT, Transform::IDENTITY)
        .unwrap();
    assert!(doc.component_in_sight(post));
    let (reply, _) = new_body(&mut doc);
    assert!(reply.get("shown_parents").is_none(), "{reply}");
    assert!(!visible(&doc, fairlead));
}

#[test]
fn a_hidden_parent_that_follows_a_condition_keeps_it() {
    let (mut doc, fairlead, _) = hidden_parent();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "Fairlead:1",
               "expression": "1 > 2"}),
    );
    assert!(!visible(&doc, fairlead));
    let (reply, _) = new_body(&mut doc);
    assert!(reply.get("shown_parents").is_none(), "{reply}");
    assert!(!visible(&doc, fairlead), "the condition still decides");
}

#[test]
fn hiding_occurrences_lists_the_path_of_the_first_placement() {
    let (mut doc, fairlead, o2) = hidden_parent();
    let post = doc.assembly().occurrence(o2).unwrap().component;
    assert_eq!(doc.hiding_occurrences(post), [fairlead]);
    doc.set_occurrence_visible(o2, false).unwrap();
    assert_eq!(doc.hiding_occurrences(post), [fairlead, o2]);
    assert!(doc.hiding_occurrences(ComponentUid::ROOT).is_empty());
    // A command that adds nothing to a component shows nothing: a
    // rename.
    let reply = command(
        &mut doc,
        json!({"cmd": "rename_component", "component": "Post", "name": "Mast"}),
    );
    assert!(reply.get("shown_parents").is_none(), "{reply}");
    assert!(!visible(&doc, fairlead));
}
